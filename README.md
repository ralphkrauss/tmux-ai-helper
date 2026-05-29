# tmux-ai-helper

Tiny tmux helper that listens for terminal progress/title signals from tools such as Claude Code and Codex and maps them into tmux status titles and attention markers.

It does not wrap or replace any AI binary. tmux starts it with `pipe-pane`; the helper reads terminal output, keeps durable state in tmux user options, and regenerates helper-managed display titles from that state.

Supported signals:

- `OSC 9;4` progress reports, including active, percent, clear, error, and paused states.
- `OSC 0` / `OSC 2` title updates with native progress prefixes such as Codex's braille frames and Claude Code's tmux title frames.
- Claude Code's visible output text is not parsed.

Display mapping:

- active: `⏳ <title>`
- percent: `⏳ 42% <title>`
- done: `✅ <title>`
- error: `❌ <title>`
- paused: `⏸ <title>`
- needs attention: `🔔 ✅ <title>` or `🔔 ❌ <title>`

The tmux window list keeps your manual window name and appends compact helper state in pane order, such as `[⏳]`, `[⏳ ⏳ 🔔✅]`, or `[⏳ 🔔❌ ⏸]`. Idle panes are omitted unless they need attention, and fully idle windows do not get a helper suffix.

Attention is separate from completion. `✅` means a tool finished; `🔔` means it finished while the pane/window was hidden. Selecting the pane/window clears `🔔` but leaves `✅`.

The helper persists state in versioned tmux user options:

- pane options: `@tmux_ai_helper_v1_activity`, `@tmux_ai_helper_v1_attention`, `@tmux_ai_helper_v1_base_title`, `@tmux_ai_helper_v1_display_title`, `@tmux_ai_helper_v1_percent`, `@tmux_ai_helper_v1_source`
- window options: `@tmux_ai_helper_v1_attention`, `@tmux_ai_helper_v1_window_summary`
- session option: `@tmux_ai_helper_v1_attention_count`

## Install

### Requirements

- tmux 3.x recommended.
- Rust stable toolchain.
- A terminal that can display Unicode symbols for the pane/window status UI.

The terminal bell backend is best-effort. Different terminals may show a tab marker, flash, play a sound, bounce a dock icon, or do nothing depending on user settings.

### Linux / EC2 over SSH

On Ubuntu or Debian:

```sh
sudo apt update
sudo apt install -y git build-essential tmux curl
```

On Amazon Linux 2023, Fedora, or RHEL-like systems:

```sh
sudo dnf install -y git gcc make tmux curl
```

On Amazon Linux 2:

```sh
sudo yum install -y git gcc make tmux curl
```

Install Rust if it is not already installed:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Build and install the helper:

```sh
git clone https://github.com/ralphkrauss/tmux-ai-helper.git
cd tmux-ai-helper
cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/tmux-ai-helper "$HOME/.local/bin/tmux-ai-helper"
```

### macOS

If Rust and tmux are already installed:

```sh
git clone https://github.com/ralphkrauss/tmux-ai-helper.git
cd tmux-ai-helper
cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/tmux-ai-helper "$HOME/.local/bin/tmux-ai-helper"
```

## Recommended tmux Settings

Add this to `~/.tmux.conf`:

```tmux
set -g focus-events on
set -g @tmux_ai_helper_path "$HOME/.local/bin/tmux-ai-helper"

# tmux-ai-helper writes its normalized display title to a pane option, so
# #{pane_title} can remain app-owned on tmux builds without allow-set-title.

# Let tmux, not applications, own the outer terminal title. The terminal owns
# native bell behavior; tmux keeps only the durable unread count by default.
set -g set-titles on
set -g @tmux_ai_helper_title_mode "count"
set -g set-titles-string '#{?#{>:#{@tmux_ai_helper_v1_attention_count},0},#{?#{==:#{@tmux_ai_helper_title_mode},off},,#{?#{==:#{@tmux_ai_helper_title_mode},emoji},🔔#{@tmux_ai_helper_v1_attention_count} ,[#{@tmux_ai_helper_v1_attention_count}] }},}#S:#I:#W#{?#{@tmux_ai_helper_v1_window_summary}, [#{@tmux_ai_helper_v1_window_summary}],}'

# Ring the attached terminal when hidden AI work completes. Add "command" here
# later to run @tmux_ai_helper_notify_command as well.
set -g @tmux_ai_helper_notify_backends "bell"
set -g @tmux_ai_helper_notify_command ""

# Show helper-managed display titles in tmux's window list. The window-level marker
# covers split-pane cases where a hidden pane in the window needs attention.
setw -g window-status-format '#I:#W#{?#{@tmux_ai_helper_v1_window_summary}, [#{@tmux_ai_helper_v1_window_summary}],}#{?window_flags,#{window_flags}, }'
setw -g window-status-current-format '#I:#W#{?#{@tmux_ai_helper_v1_window_summary}, [#{@tmux_ai_helper_v1_window_summary}],}#{?window_flags,#{window_flags}, }'
setw -g pane-border-format '#{?pane_active,#[reverse],}#{pane_index}#[default] "#{?#{@tmux_ai_helper_v1_display_title},#{@tmux_ai_helper_v1_display_title},#{pane_title}}"'
set -g status-right '#{?window_bigger,[#{window_offset_x}#,#{window_offset_y}] ,}"#{=21:#{?#{@tmux_ai_helper_v1_display_title},#{@tmux_ai_helper_v1_display_title},#{pane_title}}}" %H:%M %d-%b-%y'

# Attach the helper automatically to new panes.
set-hook -g after-new-session 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; pane=\"#{pane_id}\"; test -z \"\$pane\" || test ! -x \"\$helper\" || \"\$helper\" attach \"\$pane\""'
set-hook -g after-new-window 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; pane=\"#{pane_id}\"; test -z \"\$pane\" || test ! -x \"\$helper\" || \"\$helper\" attach \"\$pane\""'
set-hook -g after-split-window 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; pane=\"#{pane_id}\"; test -z \"\$pane\" || test ! -x \"\$helper\" || \"\$helper\" attach \"\$pane\""'

# Clear attention when you visit a marked window/pane.
set-hook -g after-select-window 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; test ! -x \"\$helper\" || \"\$helper\" clear-window \"#{window_id}\""'
set-hook -g session-window-changed 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; test ! -x \"\$helper\" || \"\$helper\" clear-window \"#{window_id}\""'
set-hook -g after-select-pane 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; test ! -x \"\$helper\" || \"\$helper\" clear-pane \"#{pane_id}\""'
set-hook -g client-attached 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; test ! -x \"\$helper\" || \"\$helper\" clear-window \"#{window_id}\""'
set-hook -g client-focus-in 'run-shell -b "helper=\"#{@tmux_ai_helper_path}\"; test ! -x \"\$helper\" || \"\$helper\" clear-window \"#{window_id}\""'

# Attach the helper to panes that already exist when the config is sourced.
run-shell -b 'helper="#{@tmux_ai_helper_path}"; test ! -x "$helper" || tmux list-panes -a -F "##{pane_id}" | xargs -n 1 "$helper" attach'
```

After editing `~/.tmux.conf`, apply it with:

```sh
tmux source-file ~/.tmux.conf
```

If you are enabling `focus-events` for an already attached tmux client, detach and attach once after sourcing the config. tmux requests focus reporting when the client attaches.

For SSH, run these commands inside the remote tmux server on the EC2 instance:

```sh
tmux source-file ~/.tmux.conf
tmux detach-client
```

Then reconnect or reattach:

```sh
ssh ec2-user@your-host
tmux attach
```

Over SSH, the durable tmux state still works on the remote server:

- remote tmux window list keeps your manual window name and appends helper state such as `[⏳ 🔔✅]`
- remote tmux outer title shows `[N]` by default and uses the helper-managed window summary when available
- BEL travels through SSH to your local terminal as a best-effort notification

Focus detection over SSH depends on the local terminal, SSH connection, and remote tmux terminal features. If focus reporting is unavailable, the helper falls back to tmux active-window visibility, so the durable tmux indicators still work.

### tmux compatibility

The helper is designed for tmux 3.x. It does not require `allow-set-title`, so the same configuration works on builds that have that option and on builds, such as tmux 3.4, that do not. Applications may still update tmux's built-in `#{pane_title}` with OSC title sequences; tmux-ai-helper stores its normalized pane display in `@tmux_ai_helper_v1_display_title` and its ordered window state suffix in `@tmux_ai_helper_v1_window_summary`.

### Why these settings

- `@tmux_ai_helper_v1_display_title`: keeps the helper-owned display title separate from app-owned `#{pane_title}`. This avoids title races on tmux builds without `allow-set-title`.
- `@tmux_ai_helper_path`: points tmux hooks at the installed binary. Change this if you install somewhere else.
- `focus-events on`: lets tmux distinguish a tmux window that is selected from a terminal tab/window that is actually focused. Without this, hidden Ghostty tabs can look "visible" to tmux.
- `set-titles on` with the provided `set-titles-string`: lets tmux show a persistent aggregate attention count, your manual window name, and the helper-managed window state in the outer terminal title. The default `@tmux_ai_helper_title_mode "count"` shows `[2] work:3:#123-fix-auth [⏳ ⏳ 🔔✅]`.
- `window-status-format` / `window-status-current-format`: keeps your manual window name visible and adds the helper-managed window summary next to it.
- `@tmux_ai_helper_notify_backends "bell"`: sends a terminal bell when hidden AI work finishes. The terminal may turn this into a flash, sound, title marker, dock badge, or nothing depending on user settings.
- `pipe-pane -o`: attaches the helper only when a pane does not already have a pipe.

### Terminal title modes

The default title mode is terminal-neutral:

```tmux
set -g @tmux_ai_helper_title_mode "count"
```

Supported modes:

- `count`: show `[2] session:window` when there are unread AI completions.
- `emoji`: show `🔔2 session:window`; use this only if you want tmux to own the bell glyph.
- `off`: show only `session:window`.

Native terminal bells are transient. A terminal may add and clear its own bell marker independently of tmux. For robustness, keep tmux's durable unread indicator separate from the terminal's native bell behavior.

### Notification hooks

The default notification backend is:

```tmux
set -g @tmux_ai_helper_notify_backends "bell"
```

The helper does not require terminal-specific configuration. If a terminal has native bell/title settings, those remain user preferences outside the helper.

You can also run a command when hidden AI work completes:

```tmux
set -g @tmux_ai_helper_notify_backends "bell command"
set -g @tmux_ai_helper_notify_command 'notify-send "AI finished" "$TMUX_AI_HELPER_TITLE"'
```

The command receives these environment variables:

- `TMUX_AI_HELPER_PANE`
- `TMUX_AI_HELPER_WINDOW`
- `TMUX_AI_HELPER_SESSION`
- `TMUX_AI_HELPER_ACTIVITY`
- `TMUX_AI_HELPER_TITLE`

### Maintenance notes

- The helper uses one idle process per attached pane. It reads with blocking I/O and only calls tmux when parsed state changes or attention is created/cleared.
- tmux supports only one `pipe-pane` command per pane. If you use `pipe-pane` for logging, it will conflict with this helper in that pane.
- The helper-managed pane display title is stored in `@tmux_ai_helper_v1_display_title`; the compact per-window summary is stored in `@tmux_ai_helper_v1_window_summary`. On attach, the helper strips old helper-owned prefixes such as `🔔`, `⏳`, `✅`, `❌`, and `⏸`, then regenerates those options from tmux state. This prevents emoji stacking after detach/reattach or helper restarts while leaving `#{pane_title}` app-owned.
- If the install path changes, update `@tmux_ai_helper_path` in `~/.tmux.conf`.
- After rebuilding the helper, reinstall it and restart existing pane listeners:

```sh
cargo build --release
install -m 0755 target/release/tmux-ai-helper ~/.local/bin/tmux-ai-helper
tmux source-file ~/.tmux.conf
```

To confirm the helper is attached to panes:

```sh
tmux list-panes -a -F '#{session_name}:#{window_index}.#{pane_index} pipe=#{pane_pipe} window=#{@tmux_ai_helper_v1_window_summary} title=#{@tmux_ai_helper_v1_display_title} raw=#{pane_title}'
```

`pipe=1` means a pane has a `pipe-pane` listener attached.
