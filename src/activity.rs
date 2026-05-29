#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activity {
    Idle,
    Active,
    Done,
    Error,
    Paused,
}

impl Activity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Active => "active",
            Self::Done => "done",
            Self::Error => "error",
            Self::Paused => "paused",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "active" => Some(Self::Active),
            "done" => Some(Self::Done),
            "error" => Some(Self::Error),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivitySource {
    TitleSpinner,
    Osc94,
}

impl ActivitySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TitleSpinner => "title-spinner",
            Self::Osc94 => "osc94",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "title-spinner" => Some(Self::TitleSpinner),
            "osc94" => Some(Self::Osc94),
            _ => None,
        }
    }
}
