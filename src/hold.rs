use std::env;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{state, tmux};

const DEFAULT_STATE_ORDER: &str = "test review blocked parked";

#[derive(Clone, Debug, Eq, PartialEq)]
struct HoldState {
    key: String,
    label: String,
}

pub fn set(key: &str, target: Option<&str>) -> io::Result<()> {
    let window = resolve_window(target)?;
    let states = hold_states();
    let key_candidates = key_candidates(key);
    let state = states
        .iter()
        .find(|state| {
            key_candidates
                .iter()
                .any(|candidate| state.key == *candidate)
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "unknown hold state: {key}. Available states: {}",
                    states
                        .iter()
                        .map(|state| state.key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;

    tmux::set_window_option(&window, tmux::OPT_HOLD_KEY, &state.key)?;
    tmux::set_window_option(&window, tmux::OPT_HOLD_LABEL, &state.label)?;
    tmux::set_window_option(&window, tmux::OPT_HOLD_SINCE, &unix_timestamp().to_string())?;

    state::clear_window(&window)
}

pub fn clear(target: Option<&str>) -> io::Result<()> {
    let window = resolve_window(target)?;

    tmux::unset_window_option(&window, tmux::OPT_HOLD_KEY)?;
    tmux::unset_window_option(&window, tmux::OPT_HOLD_LABEL)?;
    tmux::unset_window_option(&window, tmux::OPT_HOLD_SINCE)?;
    tmux::sync_window_state(&window)
}

pub fn menu(target: Option<&str>) -> io::Result<()> {
    let menu_target = target
        .filter(|target| target.starts_with('%'))
        .unwrap_or_default();
    let window = resolve_window(target)?;
    let states = hold_states();
    if states.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no hold states configured",
        ));
    }

    let exe = env::current_exe().unwrap_or_else(|_| "tmux-ai-helper".into());
    let mut args = vec![
        "display-menu".to_owned(),
        "-T".to_owned(),
        "Hold state".to_owned(),
    ];

    if !menu_target.is_empty() {
        args.push("-t".to_owned());
        args.push(menu_target.to_owned());
    }

    let mut used_shortcuts = Vec::new();
    for state in states {
        let shortcut = shortcut_for(&state.key, &mut used_shortcuts);
        args.push(state.label);
        args.push(shortcut);
        args.push(tmux_command(&exe, &["hold", &state.key, &window]));
    }

    args.push("Clear hold".to_owned());
    args.push("C".to_owned());
    args.push(tmux_command(&exe, &["hold-clear", &window]));

    run_tmux(args)
}

fn resolve_window(target: Option<&str>) -> io::Result<String> {
    match target {
        Some(target) if target.starts_with('%') => {
            tmux::window_id_for_pane(target).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("could not resolve window for pane target: {target}"),
                )
            })
        }
        Some(target) if !target.is_empty() => Ok(target.to_owned()),
        _ => tmux::current_window_id().ok_or_else(|| {
            io::Error::other("could not resolve current tmux window; pass a window id explicitly")
        }),
    }
}

fn hold_states() -> Vec<HoldState> {
    let order = tmux::get_global_option(tmux::OPT_HOLD_STATE_ORDER)
        .unwrap_or_else(|| DEFAULT_STATE_ORDER.to_owned());
    let mut seen = Vec::new();

    order
        .split_whitespace()
        .filter_map(|raw_key| {
            let key = canonical_state_key(raw_key);
            if !is_valid_state_key(key) || seen.iter().any(|seen_key| seen_key == key) {
                return None;
            }
            seen.push(key.to_owned());

            let label = hold_state_label(key, raw_key)
                .or_else(|| default_label(key).map(str::to_owned))
                .unwrap_or_else(|| key.to_owned());

            Some(HoldState {
                key: key.to_owned(),
                label,
            })
        })
        .collect()
}

fn hold_state_label(key: &str, raw_key: &str) -> Option<String> {
    let canonical_option = format!("{}{}", tmux::OPT_HOLD_STATE_PREFIX, key);
    let raw_option = format!("{}{}", tmux::OPT_HOLD_STATE_PREFIX, raw_key);

    tmux::get_global_option(&canonical_option).or_else(|| {
        (raw_option != canonical_option)
            .then(|| tmux::get_global_option(&raw_option))
            .flatten()
    })
}

fn default_label(key: &str) -> Option<&'static str> {
    match key {
        "test" => Some("🧪 Test"),
        "review" => Some("👀 Review"),
        "blocked" => Some("⛔ Blocked"),
        "parked" => Some("📌 Parked"),
        _ => None,
    }
}

fn canonical_state_key(key: &str) -> &str {
    match key {
        "pr" => "review",
        _ => key,
    }
}

fn key_candidates(key: &str) -> Vec<&str> {
    match key {
        "pr" => vec!["review"],
        _ => vec![key],
    }
}

fn is_valid_state_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn shortcut_for(key: &str, used: &mut Vec<char>) -> String {
    let shortcut = key
        .chars()
        .map(|ch| ch.to_ascii_lowercase())
        .find(|ch| ch.is_ascii_alphanumeric() && !used.contains(ch));

    if let Some(shortcut) = shortcut {
        used.push(shortcut);
        shortcut.to_string()
    } else {
        String::new()
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn tmux_command(exe: &Path, args: &[&str]) -> String {
    let mut words = vec![exe.to_string_lossy().into_owned()];
    words.extend(args.iter().map(|arg| (*arg).to_owned()));

    let shell_command = words
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ");

    format!("run-shell -b {}", shell_quote(&shell_command))
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

fn run_tmux(args: Vec<String>) -> io::Result<()> {
    let status = Command::new("tmux").args(args).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "tmux command failed with status {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_labels_cover_builtin_states() {
        assert_eq!(default_label("test"), Some("🧪 Test"));
        assert_eq!(default_label("review"), Some("👀 Review"));
        assert_eq!(default_label("blocked"), Some("⛔ Blocked"));
        assert_eq!(default_label("parked"), Some("📌 Parked"));
        assert_eq!(default_label("other"), None);
    }

    #[test]
    fn pr_is_an_alias_for_review() {
        assert_eq!(canonical_state_key("pr"), "review");
        assert_eq!(key_candidates("pr"), vec!["review"]);
        assert_eq!(key_candidates("review"), vec!["review"]);
    }

    #[test]
    fn validates_state_keys_for_tmux_option_suffixes() {
        assert!(is_valid_state_key("review"));
        assert!(is_valid_state_key("qa_hold"));
        assert!(is_valid_state_key("pre-merge"));
        assert!(!is_valid_state_key(""));
        assert!(!is_valid_state_key("needs review"));
        assert!(!is_valid_state_key("review,blocked"));
    }

    #[test]
    fn shortcuts_are_unique_when_possible() {
        let mut used = Vec::new();
        assert_eq!(shortcut_for("test", &mut used), "t");
        assert_eq!(shortcut_for("triage", &mut used), "r");
        assert_eq!(shortcut_for("123", &mut used), "1");
    }

    #[test]
    fn shell_quote_handles_single_quotes() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("it'll wait"), "'it'\\''ll wait'");
    }
}
