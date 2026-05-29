use crate::activity::Activity;

pub const FALLBACK_TITLE: &str = "shell";

const ATTENTION_PREFIX: &str = "🔔";

const CODEX_SPINNER_PREFIXES: &[&str] =
    &["⠋ ", "⠙ ", "⠹ ", "⠸ ", "⠼ ", "⠴ ", "⠦ ", "⠧ ", "⠇ ", "⠏ "];

const CLAUDE_ACTIVE_TITLE_PREFIXES: &[&str] = &[
    "⠐ ", "⠂ ", "✢ ", "✣ ", "✤ ", "✥ ", "✦ ", "✧ ", "✩ ", "✪ ", "✫ ", "✬ ", "✭ ", "✮ ", "✯ ", "✰ ",
    "✱ ", "✲ ", "✴ ", "✵ ", "✶ ", "✷ ", "✸ ", "✹ ", "✺ ", "✼ ", "✽ ", "✾ ", "✿ ", "❀ ", "❁ ", "❂ ",
    "❃ ", "❇ ", "❈ ", "❉ ", "❊ ", "❋ ",
];

const CLAUDE_DONE_TITLE_PREFIXES: &[&str] = &["✳ "];

const COMMON_SPINNER_PREFIXES: &[&str] = &[
    "⣾ ", "⣽ ", "⣻ ", "⢿ ", "⡿ ", "⣟ ", "⣯ ", "⣷ ", "◐ ", "◓ ", "◑ ", "◒ ", "◴ ", "◷ ", "◶ ", "◵ ",
    "◜ ", "◝ ", "◞ ", "◟ ",
];

#[derive(Debug, Eq, PartialEq)]
pub struct ParsedTitle {
    pub base: String,
    pub activity: Option<Activity>,
    pub attention: bool,
    pub percent: Option<u8>,
}

pub fn display_title(
    activity: Activity,
    attention: bool,
    percent: Option<u8>,
    base_title: &str,
) -> String {
    let base_title = clean_base(base_title);
    let activity_title = match activity {
        Activity::Idle => base_title.to_owned(),
        Activity::Active => match percent {
            Some(percent) => format!("⏳ {percent}% {base_title}"),
            None => format!("⏳ {base_title}"),
        },
        Activity::Done => format!("✅ {base_title}"),
        Activity::Error => format!("❌ {base_title}"),
        Activity::Paused => format!("⏸ {base_title}"),
    };

    if attention {
        format!("{ATTENTION_PREFIX} {activity_title}")
    } else {
        activity_title
    }
}

pub fn parse_title(title: &str) -> ParsedTitle {
    let mut title = title.trim().to_owned();
    let mut activity = None;
    let mut attention = false;
    let mut percent = None;

    loop {
        let previous = title.clone();
        title = title.trim_start().to_owned();

        if let Some(rest) = strip_marker(&title, ATTENTION_PREFIX) {
            attention = true;
            title = rest.to_owned();
        }

        if let Some(rest) = strip_marker(&title, "⏳") {
            activity = Some(Activity::Active);
            let rest = rest.trim_start();
            if let Some((parsed_percent, after_percent)) = strip_percent_prefix(rest) {
                percent = Some(parsed_percent);
                title = after_percent.trim_start().to_owned();
            } else {
                title = rest.to_owned();
            }
        }

        for (prefix, parsed_activity) in [
            ("✅", Activity::Done),
            ("❌", Activity::Error),
            ("⏸", Activity::Paused),
        ] {
            if let Some(rest) = strip_marker(&title, prefix) {
                activity = Some(parsed_activity);
                title = rest.to_owned();
            }
        }

        if let Some(rest) = strip_active_title_prefix(&title) {
            activity = Some(Activity::Active);
            percent = None;
            title = rest.trim_start().to_owned();
        }

        if let Some(rest) = strip_done_title_prefix(&title) {
            activity = Some(Activity::Done);
            percent = None;
            title = rest.trim_start().to_owned();
        }

        if title == previous {
            break;
        }
    }

    ParsedTitle {
        base: title,
        activity,
        attention,
        percent,
    }
}

pub fn first_non_empty<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn clean_base(base_title: &str) -> &str {
    let base_title = base_title.trim();
    if base_title.is_empty() {
        FALLBACK_TITLE
    } else {
        base_title
    }
}

fn strip_marker<'a>(value: &'a str, marker: &str) -> Option<&'a str> {
    value.strip_prefix(marker).map(str::trim_start)
}

fn strip_percent_prefix(value: &str) -> Option<(u8, &str)> {
    let bytes = value.as_bytes();
    let mut end = 0;

    while end < bytes.len() && end < 3 && bytes[end].is_ascii_digit() {
        end += 1;
    }

    if end == 0 || bytes.get(end) != Some(&b'%') {
        return None;
    }

    let percent = value[..end].parse::<u8>().ok()?;
    if percent > 100 {
        return None;
    }

    Some((percent, &value[end + 1..]))
}

fn strip_active_title_prefix(value: &str) -> Option<&str> {
    CODEX_SPINNER_PREFIXES
        .iter()
        .chain(CLAUDE_ACTIVE_TITLE_PREFIXES)
        .chain(COMMON_SPINNER_PREFIXES)
        .find_map(|prefix| value.strip_prefix(prefix))
}

fn strip_done_title_prefix(value: &str) -> Option<&str> {
    CLAUDE_DONE_TITLE_PREFIXES
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native_spinner_titles() {
        assert_eq!(
            parse_title("⠋ codex"),
            ParsedTitle {
                base: "codex".to_owned(),
                activity: Some(Activity::Active),
                attention: false,
                percent: None,
            }
        );
    }

    #[test]
    fn parses_claude_title_state() {
        assert_eq!(
            parse_title("✳ Claude Code"),
            ParsedTitle {
                base: "Claude Code".to_owned(),
                activity: Some(Activity::Done),
                attention: false,
                percent: None,
            }
        );
        for title in ["⠐ Claude Code", "⠂ Claude Code", "✢ Claude Code"] {
            assert_eq!(
                parse_title(title),
                ParsedTitle {
                    base: "Claude Code".to_owned(),
                    activity: Some(Activity::Active),
                    attention: false,
                    percent: None,
                }
            );
        }
    }

    #[test]
    fn parses_helper_titles_without_stacking() {
        assert_eq!(
            parse_title("🔔 🔔 ✅ ✅ Claude Code"),
            ParsedTitle {
                base: "Claude Code".to_owned(),
                activity: Some(Activity::Done),
                attention: true,
                percent: None,
            }
        );
        assert_eq!(
            parse_title("🔔 ⏳ 42% Claude Code"),
            ParsedTitle {
                base: "Claude Code".to_owned(),
                activity: Some(Activity::Active),
                attention: true,
                percent: Some(42),
            }
        );
    }

    #[test]
    fn displays_activity_titles() {
        assert_eq!(display_title(Activity::Idle, false, None, "codex"), "codex");
        assert_eq!(
            display_title(Activity::Active, false, None, "codex"),
            "⏳ codex"
        );
        assert_eq!(
            display_title(Activity::Active, false, Some(42), "Claude Code"),
            "⏳ 42% Claude Code"
        );
        assert_eq!(
            display_title(Activity::Done, false, None, "codex"),
            "✅ codex"
        );
        assert_eq!(
            display_title(Activity::Error, false, None, "codex"),
            "❌ codex"
        );
        assert_eq!(
            display_title(Activity::Paused, false, None, "codex"),
            "⏸ codex"
        );
        assert_eq!(
            display_title(Activity::Done, true, None, "codex"),
            "🔔 ✅ codex"
        );
    }
}
