use std::env;
use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

use crate::activity::Activity;

pub const OPT_ACTIVITY: &str = "@tmux_ai_helper_v1_activity";
pub const OPT_ATTENTION: &str = "@tmux_ai_helper_v1_attention";
pub const OPT_BASE_TITLE: &str = "@tmux_ai_helper_v1_base_title";
pub const OPT_DISPLAY_TITLE: &str = "@tmux_ai_helper_v1_display_title";
pub const OPT_PERCENT: &str = "@tmux_ai_helper_v1_percent";
pub const OPT_SOURCE: &str = "@tmux_ai_helper_v1_source";
pub const OPT_ATTENTION_COUNT: &str = "@tmux_ai_helper_v1_attention_count";
pub const OPT_WINDOW_SUMMARY: &str = "@tmux_ai_helper_v1_window_summary";
pub const OPT_HOLD_KEY: &str = "@tmux_ai_helper_v1_hold_key";
pub const OPT_HOLD_LABEL: &str = "@tmux_ai_helper_v1_hold_label";
pub const OPT_HOLD_SINCE: &str = "@tmux_ai_helper_v1_hold_since";
pub const OPT_HOLD_STATE_ORDER: &str = "@tmux_ai_helper_hold_state_order";
pub const OPT_HOLD_STATE_PREFIX: &str = "@tmux_ai_helper_hold_state_";
pub const OPT_NOTIFY_BACKENDS: &str = "@tmux_ai_helper_notify_backends";
pub const OPT_NOTIFY_COMMAND: &str = "@tmux_ai_helper_notify_command";

const ATTACH_ALL_ATTEMPTS: usize = 4;
const ATTACH_ALL_RETRY_DELAY: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Eq, PartialEq)]
struct PanePipe {
    pane: String,
    pipe: bool,
}

pub fn attach(pane: &str) -> io::Result<()> {
    if read_format(pane, "#{pane_pipe}").as_deref() == Some("1") {
        return Ok(());
    }

    let exe = env::current_exe().unwrap_or_else(|_| "tmux-ai-helper".into());
    let pane_for_shell = pane.replace('%', "%%");
    let command = format!(
        "{} listen {}",
        shell_quote_path(&exe),
        shell_quote(&pane_for_shell)
    );

    run_status(["pipe-pane", "-t", pane, "-o", "-O", &command])
}

pub fn attach_all(session: Option<&str>) -> io::Result<()> {
    let panes = match list_panes_for_attach_with_retry(session) {
        Ok(panes) => panes,
        Err(err) => {
            let message = format!("tmux-ai-helper attach-all failed: {err}");
            display_message(&message);
            return Err(io::Error::other(message));
        }
    };
    let mut failures = Vec::new();

    for pane in panes {
        if pane.pipe {
            continue;
        }

        if let Err(err) = attach_with_retry(&pane.pane) {
            failures.push(format!("{}: {err}", pane.pane));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        let message = format!("tmux-ai-helper attach-all failed: {}", failures.join("; "));
        display_message(&message);
        Err(io::Error::other(message))
    }
}

pub fn read_format(target: &str, format: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "-t", target, format])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned(),
    )
}

pub fn get_pane_option(pane: &str, option: &str) -> Option<String> {
    show_option(["show-options", "-p", "-qv", "-t", pane, option])
}

pub fn set_pane_option(pane: &str, option: &str, value: &str) -> io::Result<()> {
    run_status(["set-option", "-p", "-q", "-t", pane, option, value])
}

pub fn get_window_option(window: &str, option: &str) -> Option<String> {
    show_option(["show-options", "-w", "-qv", "-t", window, option])
}

pub fn set_window_option(window: &str, option: &str, value: &str) -> io::Result<()> {
    run_status(["set-option", "-w", "-q", "-t", window, option, value])
}

pub fn unset_window_option(window: &str, option: &str) -> io::Result<()> {
    run_status(["set-option", "-w", "-q", "-u", "-t", window, option])
}

pub fn get_global_option(option: &str) -> Option<String> {
    show_option(["show-options", "-g", "-qv", option])
}

pub fn get_session_option(session: &str, option: &str) -> Option<String> {
    show_option(["show-options", "-qv", "-t", session, option])
}

pub fn set_session_option(session: &str, option: &str, value: &str) -> io::Result<()> {
    run_status(["set-option", "-q", "-t", session, option, value])
}

pub fn list_panes(target: &str) -> Vec<String> {
    list_items(["list-panes", "-t", target, "-F", "#{pane_id}"])
}

pub fn list_windows(session: &str) -> Vec<String> {
    list_items(["list-windows", "-t", session, "-F", "#{window_id}"])
}

pub fn list_client_ttys(session: &str) -> Vec<String> {
    let mut ttys = list_items(["list-clients", "-t", session, "-F", "#{client_tty}"]);
    ttys.sort();
    ttys.dedup();
    ttys
}

pub fn window_id_for_pane(pane: &str) -> Option<String> {
    read_format(pane, "#{window_id}")
}

pub fn current_window_id() -> Option<String> {
    read_current_format("#{window_id}")
}

pub fn current_pane_id() -> Option<String> {
    read_current_format("#{pane_id}")
}

pub fn session_id_for_target(target: &str) -> Option<String> {
    read_format(target, "#{session_id}")
}

pub fn is_pane_visible(pane: &str) -> bool {
    let Some(output) = read_format(
        pane,
        "#{window_active_clients_list}\t#{window_zoomed_flag}\t#{pane_active}\t#{session_id}",
    ) else {
        return false;
    };

    let mut parts = output.split('\t');
    let active_client_ttys = parts.next().unwrap_or_default();
    let window_zoomed = parts.next() == Some("1");
    let pane_active = parts.next() == Some("1");
    let session = parts.next().unwrap_or_default();

    if window_zoomed && !pane_active {
        return false;
    }

    active_clients_are_visible(active_client_ttys, &client_focus_states(session))
}

pub fn sync_window_state_for_pane(pane: &str) -> io::Result<()> {
    let Some(window) = window_id_for_pane(pane) else {
        return Ok(());
    };
    let Some(session) = session_id_for_target(&window) else {
        return Ok(());
    };

    sync_window_attention(&window)?;
    sync_window_summary(&window)?;
    sync_session_attention_count(&session)?;
    Ok(())
}

pub fn sync_window_attention(window: &str) -> io::Result<bool> {
    if window_has_hold(window) {
        set_window_option(window, OPT_ATTENTION, bool_value(false))?;
        return Ok(false);
    }

    let has_attention = list_panes(window).into_iter().any(|pane| {
        get_pane_option(&pane, OPT_ATTENTION)
            .as_deref()
            .is_some_and(is_truthy)
    });

    set_window_option(window, OPT_ATTENTION, bool_value(has_attention))?;
    Ok(has_attention)
}

pub fn sync_window_summary(window: &str) -> io::Result<String> {
    let summary = window_summary(&pane_summaries(window));
    set_window_option(window, OPT_WINDOW_SUMMARY, &summary)?;
    Ok(summary)
}

pub fn sync_session_attention_count(session: &str) -> io::Result<usize> {
    let count = list_windows(session)
        .into_iter()
        .filter(|window| {
            !window_has_hold(window)
                && get_window_option(window, OPT_ATTENTION)
                    .as_deref()
                    .is_some_and(is_truthy)
        })
        .count();

    set_session_option(session, OPT_ATTENTION_COUNT, &count.to_string())?;
    Ok(count)
}

pub fn sync_window_state(window: &str) -> io::Result<()> {
    sync_window_attention(window)?;
    sync_window_summary(window)?;
    if let Some(session) = session_id_for_target(window) {
        sync_session_attention_count(&session)?;
    }
    Ok(())
}

pub fn window_has_hold(window: &str) -> bool {
    get_window_option(window, OPT_HOLD_LABEL)
        .as_deref()
        .is_some_and(|label| !label.trim().is_empty())
}

pub fn bool_value(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

pub fn is_truthy(value: &str) -> bool {
    !matches!(value.trim(), "" | "0" | "false" | "off" | "no")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PaneSummary {
    activity: Activity,
    attention: bool,
}

fn pane_summaries(window: &str) -> Vec<PaneSummary> {
    list_items([
        "list-panes",
        "-t",
        window,
        "-F",
        "#{?#{@tmux_ai_helper_v1_activity},#{@tmux_ai_helper_v1_activity},idle}\t#{@tmux_ai_helper_v1_attention}",
    ])
    .into_iter()
    .map(|line| {
        let mut parts = line.split('\t');
        let activity = parts
            .next()
            .and_then(Activity::from_str)
            .unwrap_or(Activity::Idle);
        let attention = parts.next().is_some_and(is_truthy);

        PaneSummary {
            activity,
            attention,
        }
    })
    .collect()
}

fn window_summary(panes: &[PaneSummary]) -> String {
    panes
        .iter()
        .filter_map(pane_badge)
        .collect::<Vec<_>>()
        .join(" ")
}

fn pane_badge(pane: &PaneSummary) -> Option<String> {
    let activity = match pane.activity {
        Activity::Idle => None,
        Activity::Active => Some("⏳"),
        Activity::Done => Some("✅"),
        Activity::Error => Some("❌"),
        Activity::Paused => Some("⏸"),
    };

    match (pane.attention, activity) {
        (false, None) => None,
        (false, Some(activity)) => Some(activity.to_owned()),
        (true, None) => Some("🔔".to_owned()),
        (true, Some(activity)) => Some(format!("🔔{activity}")),
    }
}

fn show_option<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("tmux").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned();
    (!value.is_empty()).then_some(value)
}

fn read_current_format(format: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", format])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned(),
    )
}

fn list_items<const N: usize>(args: [&str; N]) -> Vec<String> {
    let Ok(output) = Command::new("tmux").args(args).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClientFocusState {
    tty: String,
    supports_focus: bool,
    focused: bool,
}

fn client_focus_states(session: &str) -> Vec<ClientFocusState> {
    let Ok(output) = Command::new("tmux")
        .args([
            "list-clients",
            "-t",
            session,
            "-F",
            "#{client_tty}\t#{client_flags}\t#{client_termfeatures}",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_client_focus_state)
        .collect()
}

fn active_clients_are_visible(
    active_client_ttys: &str,
    client_states: &[ClientFocusState],
) -> bool {
    let active_states = active_client_ttys
        .split([',', ' '])
        .filter(|tty| !tty.is_empty())
        .filter_map(|active_tty| client_states.iter().find(|state| state.tty == active_tty))
        .collect::<Vec<_>>();

    if active_states.is_empty() {
        return false;
    }

    let has_focus_reporting = active_states.iter().any(|state| state.supports_focus);
    if has_focus_reporting {
        active_states
            .iter()
            .any(|state| state.supports_focus && state.focused)
    } else {
        true
    }
}

fn parse_client_focus_state(line: &str) -> Option<ClientFocusState> {
    let mut parts = line.split('\t');
    let tty = parts.next()?;
    let flags = parts.next()?;
    let termfeatures = parts.next().unwrap_or_default();

    Some(ClientFocusState {
        tty: tty.to_owned(),
        supports_focus: termfeatures_contains_focus(termfeatures),
        focused: client_flags_contain_focus(flags),
    })
}

fn client_flags_contain_focus(flags: &str) -> bool {
    flags.split(',').any(|flag| flag == "focused")
}

fn termfeatures_contains_focus(termfeatures: &str) -> bool {
    termfeatures.split(',').any(|feature| feature == "focus")
}

fn attach_with_retry(pane: &str) -> io::Result<()> {
    let mut last_error = None;

    for attempt in 1..=ATTACH_ALL_ATTEMPTS {
        match attach(pane) {
            Ok(()) if read_format(pane, "#{pane_pipe}").as_deref() == Some("1") => return Ok(()),
            Ok(()) => {
                last_error = Some("pane still reports pipe=0 after attach".to_owned());
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }

        if attempt < ATTACH_ALL_ATTEMPTS {
            thread::sleep(ATTACH_ALL_RETRY_DELAY);
        }
    }

    Err(io::Error::other(
        last_error.unwrap_or_else(|| "attach did not complete".to_owned()),
    ))
}

fn list_panes_for_attach(session: Option<&str>) -> io::Result<Vec<PanePipe>> {
    let mut args = vec!["list-panes".to_owned()];
    match session {
        Some(session) if !session.is_empty() => {
            args.push("-s".to_owned());
            args.push("-t".to_owned());
            args.push(session.to_owned());
        }
        _ => {
            args.push("-a".to_owned());
        }
    }
    args.push("-F".to_owned());
    args.push("#{pane_id}\t#{pane_pipe}".to_owned());

    let output = Command::new("tmux").args(&args).output()?;
    if !output.status.success() {
        return Err(tmux_output_error(&args, &output));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_pane_pipe_line)
        .collect())
}

fn list_panes_for_attach_with_retry(session: Option<&str>) -> io::Result<Vec<PanePipe>> {
    let mut last_error = None;

    for attempt in 1..=ATTACH_ALL_ATTEMPTS {
        match list_panes_for_attach(session) {
            Ok(panes) if !panes.is_empty() => return Ok(panes),
            Ok(_) => {
                last_error = Some("no panes found".to_owned());
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }

        if attempt < ATTACH_ALL_ATTEMPTS {
            thread::sleep(ATTACH_ALL_RETRY_DELAY);
        }
    }

    Err(io::Error::other(
        last_error.unwrap_or_else(|| "could not list panes".to_owned()),
    ))
}

fn parse_pane_pipe_line(line: &str) -> Option<PanePipe> {
    let mut parts = line.split('\t');
    let pane = parts.next()?.trim();
    let pipe = parts.next()?.trim() == "1";

    (!pane.is_empty()).then(|| PanePipe {
        pane: pane.to_owned(),
        pipe,
    })
}

fn display_message(message: &str) {
    let _ = Command::new("tmux")
        .args(["display-message", message])
        .status();
}

fn run_status<const N: usize>(args: [&str; N]) -> io::Result<()> {
    let output = Command::new("tmux").args(args).output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(tmux_output_error(&args, &output))
    }
}

fn tmux_output_error<S: AsRef<str>>(args: &[S], output: &Output) -> io::Error {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let detail = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    let command = args.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(" ");

    if detail.is_empty() {
        io::Error::other(format!(
            "tmux {command} failed with status {}",
            output.status
        ))
    } else {
        io::Error::other(format!(
            "tmux {command} failed with status {}: {detail}",
            output.status
        ))
    }
}

fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.to_string_lossy())
}

fn shell_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_focused_client_flags() {
        assert!(client_flags_contain_focus("attached,focused,UTF-8"));
        assert!(!client_flags_contain_focus("attached,UTF-8"));
        assert!(termfeatures_contains_focus("clipboard,focus,RGB,title"));
        assert!(!termfeatures_contains_focus("clipboard,RGB,title"));
    }

    #[test]
    fn parses_client_focus_states() {
        assert_eq!(
            parse_client_focus_state(
                "/dev/ttys010\tattached,focused,UTF-8\tclipboard,focus,RGB,title"
            ),
            Some(ClientFocusState {
                tty: "/dev/ttys010".to_owned(),
                supports_focus: true,
                focused: true,
            })
        );
        assert_eq!(
            parse_client_focus_state("/dev/ttys011\tattached,UTF-8\tclipboard,RGB,title"),
            Some(ClientFocusState {
                tty: "/dev/ttys011".to_owned(),
                supports_focus: false,
                focused: false,
            })
        );
    }

    #[test]
    fn parses_pane_pipe_lines() {
        assert_eq!(
            parse_pane_pipe_line("%0\t0"),
            Some(PanePipe {
                pane: "%0".to_owned(),
                pipe: false,
            })
        );
        assert_eq!(
            parse_pane_pipe_line("%1\t1"),
            Some(PanePipe {
                pane: "%1".to_owned(),
                pipe: true,
            })
        );
        assert_eq!(parse_pane_pipe_line("\t0"), None);
    }

    #[test]
    fn focus_aware_clients_must_be_focused_to_be_visible() {
        let states = vec![ClientFocusState {
            tty: "/dev/ttys010".to_owned(),
            supports_focus: true,
            focused: true,
        }];

        assert!(active_clients_are_visible("/dev/ttys010", &states));

        let states = vec![ClientFocusState {
            tty: "/dev/ttys010".to_owned(),
            supports_focus: true,
            focused: false,
        }];

        assert!(!active_clients_are_visible("/dev/ttys010", &states));
    }

    #[test]
    fn non_focus_clients_fall_back_to_active_window_visibility() {
        let states = vec![ClientFocusState {
            tty: "/dev/ttys010".to_owned(),
            supports_focus: false,
            focused: false,
        }];

        assert!(active_clients_are_visible("/dev/ttys010", &states));
        assert!(!active_clients_are_visible("/dev/ttys011", &states));
    }

    #[test]
    fn window_summary_summarizes_single_pane_state() {
        let panes = vec![PaneSummary {
            activity: Activity::Active,
            attention: false,
        }];

        assert_eq!(window_summary(&panes), "⏳");
    }

    #[test]
    fn window_summary_is_empty_when_panes_are_idle() {
        let panes = vec![
            PaneSummary {
                activity: Activity::Idle,
                attention: false,
            },
            PaneSummary {
                activity: Activity::Idle,
                attention: false,
            },
        ];

        assert_eq!(window_summary(&panes), "");
    }

    #[test]
    fn window_summary_follows_pane_order() {
        let panes = vec![
            PaneSummary {
                activity: Activity::Active,
                attention: false,
            },
            PaneSummary {
                activity: Activity::Active,
                attention: false,
            },
            PaneSummary {
                activity: Activity::Done,
                attention: true,
            },
        ];

        assert_eq!(window_summary(&panes), "⏳ ⏳ 🔔✅");
    }

    #[test]
    fn window_summary_keeps_errors_in_pane_order() {
        let panes = vec![
            PaneSummary {
                activity: Activity::Active,
                attention: false,
            },
            PaneSummary {
                activity: Activity::Error,
                attention: true,
            },
            PaneSummary {
                activity: Activity::Paused,
                attention: false,
            },
        ];

        assert_eq!(window_summary(&panes), "⏳ 🔔❌ ⏸");
    }

    #[test]
    fn window_summary_shows_attention_for_idle_panes() {
        let panes = vec![
            PaneSummary {
                activity: Activity::Idle,
                attention: true,
            },
            PaneSummary {
                activity: Activity::Done,
                attention: false,
            },
        ];

        assert_eq!(window_summary(&panes), "🔔 ✅");
    }
}
