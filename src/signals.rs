#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressSignal {
    Clear,
    Active(Option<u8>),
    Error,
    Paused,
}

pub fn parse_osc94(payload: &[u8]) -> ProgressSignal {
    let mut parts = payload.split(|byte| *byte == b';');
    let state = parts.next().unwrap_or_default();
    let percent = parse_percent(parts.next());

    if ascii_eq(state, "0")
        || ascii_eq(state, "clear")
        || ascii_eq(state, "completed")
        || ascii_eq(state, "done")
        || ascii_eq(state, "inactive")
    {
        ProgressSignal::Clear
    } else if ascii_eq(state, "2") || ascii_eq(state, "error") || ascii_eq(state, "failed") {
        ProgressSignal::Error
    } else if ascii_eq(state, "4") || ascii_eq(state, "pause") || ascii_eq(state, "paused") {
        ProgressSignal::Paused
    } else {
        ProgressSignal::Active(percent)
    }
}

pub fn parse_percent(value: Option<&[u8]>) -> Option<u8> {
    let value = std::str::from_utf8(value?).ok()?;
    let percent = value.parse::<u8>().ok()?;
    (percent <= 100).then_some(percent)
}

pub fn osc_title(payload: &[u8]) -> Option<String> {
    payload
        .strip_prefix(b"0;")
        .or_else(|| payload.strip_prefix(b"2;"))
        .map(|title| String::from_utf8_lossy(title).into_owned())
}

fn ascii_eq(bytes: &[u8], value: &str) -> bool {
    bytes.eq_ignore_ascii_case(value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_sequences() {
        assert_eq!(osc_title(b"0;hello").as_deref(), Some("hello"));
        assert_eq!(osc_title(b"2;world").as_deref(), Some("world"));
        assert_eq!(osc_title(b"9;4;3"), None);
    }

    #[test]
    fn parses_progress_sequences() {
        assert_eq!(parse_osc94(b"0"), ProgressSignal::Clear);
        assert_eq!(parse_osc94(b"1;42"), ProgressSignal::Active(Some(42)));
        assert_eq!(parse_osc94(b"3"), ProgressSignal::Active(None));
        assert_eq!(parse_osc94(b"2"), ProgressSignal::Error);
        assert_eq!(parse_osc94(b"4"), ProgressSignal::Paused);
    }
}
