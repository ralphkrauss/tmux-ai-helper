use std::io::{self, Read};

use crate::activity::{Activity, ActivitySource};
use crate::notify::{self, Notification};
use crate::osc::OscParser;
use crate::signals::{osc_title, parse_osc94, ProgressSignal};
use crate::title::{display_title, first_non_empty, parse_title, FALLBACK_TITLE};
use crate::tmux;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    base_title: String,
    activity: Activity,
    attention: bool,
    percent: Option<u8>,
    source: Option<ActivitySource>,
}

pub struct PaneState {
    pane: String,
    base_title: String,
    activity: Activity,
    attention: bool,
    source: Option<ActivitySource>,
    saw_osc94: bool,
    percent: Option<u8>,
    displayed_title: Option<String>,
    persisted: Option<Snapshot>,
}

impl PaneState {
    pub fn new(pane: &str) -> Self {
        let raw_title = tmux::read_format(pane, "#{pane_title}").unwrap_or_default();
        let parsed = parse_title(&raw_title);
        let window_name = tmux::read_format(pane, "#{window_name}").unwrap_or_default();
        let stored_display_title = tmux::get_pane_option(pane, tmux::OPT_DISPLAY_TITLE);
        let parsed_display = stored_display_title.as_deref().map(parse_title);
        let parsed_display_activity = parsed_display.as_ref().and_then(|parsed| parsed.activity);

        let stored_base = tmux::get_pane_option(pane, tmux::OPT_BASE_TITLE);
        let stored_activity = tmux::get_pane_option(pane, tmux::OPT_ACTIVITY)
            .as_deref()
            .and_then(Activity::from_str);
        let stored_attention = tmux::get_pane_option(pane, tmux::OPT_ATTENTION)
            .as_deref()
            .map(tmux::is_truthy);
        let stored_percent = tmux::get_pane_option(pane, tmux::OPT_PERCENT)
            .as_deref()
            .and_then(parse_stored_percent);
        let stored_source = tmux::get_pane_option(pane, tmux::OPT_SOURCE)
            .as_deref()
            .and_then(ActivitySource::from_str);
        let raw_activity = parsed.activity;
        let raw_has_activity = raw_activity.is_some();

        let base_title = if raw_has_activity {
            first_non_empty([
                parsed.base.as_str(),
                stored_base.as_deref().unwrap_or_default(),
                parsed_display
                    .as_ref()
                    .map(|parsed| parsed.base.as_str())
                    .unwrap_or_default(),
                window_name.as_str(),
            ])
        } else {
            first_non_empty([
                stored_base.as_deref().unwrap_or_default(),
                parsed_display
                    .as_ref()
                    .map(|parsed| parsed.base.as_str())
                    .unwrap_or_default(),
                parsed.base.as_str(),
                window_name.as_str(),
            ])
        }
        .unwrap_or(FALLBACK_TITLE)
        .to_owned();

        let activity = raw_activity
            .or(stored_activity)
            .or(parsed_display_activity)
            .unwrap_or(Activity::Idle);
        let attention = if raw_activity == Some(Activity::Active) {
            false
        } else {
            stored_attention
                .or_else(|| parsed_display.as_ref().map(|parsed| parsed.attention))
                .unwrap_or(parsed.attention)
        };
        let source = if raw_activity == Some(Activity::Active) {
            Some(ActivitySource::TitleSpinner)
        } else if raw_has_activity {
            None
        } else {
            stored_source.or_else(|| {
                (parsed_display_activity == Some(Activity::Active))
                    .then_some(ActivitySource::TitleSpinner)
            })
        };

        Self {
            pane: pane.to_owned(),
            base_title,
            activity,
            attention,
            source,
            saw_osc94: false,
            percent: if raw_has_activity {
                parsed.percent
            } else {
                stored_percent
                    .or_else(|| parsed_display.as_ref().and_then(|parsed| parsed.percent))
                    .or(parsed.percent)
            },
            displayed_title: stored_display_title,
            persisted: None,
        }
    }

    fn handle_osc(&mut self, payload: &[u8]) -> io::Result<()> {
        if let Some(rest) = payload.strip_prefix(b"9;4;") {
            self.handle_osc94(rest)?;
        } else if let Some(title) = osc_title(payload) {
            self.handle_title(&title)?;
        }

        Ok(())
    }

    fn handle_osc94(&mut self, payload: &[u8]) -> io::Result<()> {
        self.saw_osc94 = true;
        let previous_activity = self.activity;

        match parse_osc94(payload) {
            ProgressSignal::Clear => {
                self.percent = None;
                if matches!(self.activity, Activity::Active | Activity::Paused) {
                    self.activity = Activity::Done;
                } else if self.activity != Activity::Error {
                    self.activity = Activity::Idle;
                }
                self.source = None;
            }
            ProgressSignal::Active(percent) => {
                self.activity = Activity::Active;
                self.source = Some(ActivitySource::Osc94);
                self.percent = percent;
            }
            ProgressSignal::Error => {
                self.activity = Activity::Error;
                self.source = Some(ActivitySource::Osc94);
                self.percent = None;
            }
            ProgressSignal::Paused => {
                self.activity = Activity::Paused;
                self.source = Some(ActivitySource::Osc94);
                self.percent = None;
            }
        }

        self.commit(previous_activity)
    }

    fn handle_title(&mut self, title: &str) -> io::Result<()> {
        let parsed = parse_title(title);
        if parsed.base.is_empty() {
            return Ok(());
        }

        let previous_activity = self.activity;
        self.base_title = parsed.base;

        match parsed.activity {
            Some(Activity::Active) => {
                if !self.saw_osc94 && self.source != Some(ActivitySource::Osc94) {
                    self.activity = Activity::Active;
                    self.source = Some(ActivitySource::TitleSpinner);
                    self.percent = parsed.percent;
                } else if self.source != Some(ActivitySource::Osc94) {
                    self.percent = None;
                }
            }
            Some(activity @ (Activity::Done | Activity::Error | Activity::Paused)) => {
                self.activity = activity;
                self.source = None;
                self.percent = parsed.percent;
            }
            Some(Activity::Idle) => {
                self.activity = Activity::Idle;
                self.source = None;
                self.percent = None;
            }
            None => {
                self.percent = None;
                if self.source == Some(ActivitySource::TitleSpinner)
                    && matches!(self.activity, Activity::Active | Activity::Paused)
                {
                    self.activity = Activity::Done;
                    self.source = None;
                } else if self.source.is_none()
                    && matches!(
                        self.activity,
                        Activity::Done | Activity::Error | Activity::Paused
                    )
                {
                    self.activity = Activity::Idle;
                }
            }
        }

        self.commit(previous_activity)
    }

    fn commit(&mut self, previous_activity: Activity) -> io::Result<()> {
        let pane_visible = tmux::is_pane_visible(&self.pane) || window_is_held_for_pane(&self.pane);
        let transition = attention_transition(
            previous_activity,
            self.activity,
            pane_visible,
            self.attention,
        );

        self.attention = transition.attention;

        self.apply_title()?;

        if transition.notify {
            if let Some(notification) = self.notification() {
                notify::send(&notification);
            }
        }

        Ok(())
    }

    fn apply_title(&mut self) -> io::Result<()> {
        let snapshot = self.snapshot();
        let snapshot_changed = self.persisted.as_ref() != Some(&snapshot);
        let title = display_title(
            snapshot.activity,
            snapshot.attention,
            snapshot.percent,
            &snapshot.base_title,
        );

        if snapshot_changed {
            persist_snapshot(&self.pane, &snapshot)?;
            self.persisted = Some(snapshot);
        }

        let title_changed = self.displayed_title.as_deref() != Some(title.as_str());
        if title_changed {
            tmux::set_pane_option(&self.pane, tmux::OPT_DISPLAY_TITLE, &title)?;
            self.displayed_title = Some(title);
        }

        if snapshot_changed || title_changed {
            tmux::sync_window_state_for_pane(&self.pane)?;
        }

        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            base_title: self.base_title.clone(),
            activity: self.activity,
            attention: self.attention,
            percent: self.percent,
            source: self.source,
        }
    }

    fn notification(&self) -> Option<Notification> {
        let window = tmux::window_id_for_pane(&self.pane)?;
        let session = tmux::session_id_for_target(&window)?;

        Some(Notification {
            pane: self.pane.clone(),
            window,
            session,
            activity: self.activity,
            title: self.base_title.clone(),
        })
    }
}

pub fn listen(pane: &str) -> io::Result<()> {
    let mut state = PaneState::new(pane);
    state.apply_title()?;

    let mut parser = OscParser::default();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        for osc in parser.feed(&buffer[..read]) {
            state.handle_osc(&osc)?;
        }
    }

    if matches!(state.activity, Activity::Active | Activity::Paused) {
        let previous_activity = state.activity;
        state.activity = Activity::Done;
        state.source = None;
        state.percent = None;
        state.commit(previous_activity)?;
    }

    Ok(())
}

pub fn clear_pane(pane: &str) -> io::Result<()> {
    let stored_attention = tmux::get_pane_option(pane, tmux::OPT_ATTENTION)
        .as_deref()
        .is_some_and(tmux::is_truthy);
    let display_attention = tmux::get_pane_option(pane, tmux::OPT_DISPLAY_TITLE)
        .map(|title| parse_title(&title).attention)
        .unwrap_or(false);
    let title_attention = tmux::read_format(pane, "#{pane_title}")
        .map(|title| parse_title(&title).attention)
        .unwrap_or(false);

    if !stored_attention && !display_attention && !title_attention {
        return Ok(());
    }

    let mut state = PaneState::new(pane);
    state.attention = false;
    state.apply_title()
}

pub fn mark_pane(pane: &str) -> io::Result<()> {
    let mut state = PaneState::new(pane);
    state.attention = true;
    state.apply_title()
}

pub fn clear_window(window: &str) -> io::Result<()> {
    for pane in tmux::list_panes(window) {
        clear_pane(&pane)?;
    }

    tmux::sync_window_attention(window)?;
    tmux::sync_window_summary(window)?;
    if let Some(session) = tmux::session_id_for_target(window) {
        tmux::sync_session_attention_count(&session)?;
    }

    Ok(())
}

pub fn clear_session(session: &str) -> io::Result<()> {
    for window in tmux::list_windows(session) {
        clear_window(&window)?;
    }
    tmux::sync_session_attention_count(session)?;
    Ok(())
}

fn persist_snapshot(pane: &str, snapshot: &Snapshot) -> io::Result<()> {
    tmux::set_pane_option(pane, tmux::OPT_ACTIVITY, snapshot.activity.as_str())?;
    tmux::set_pane_option(
        pane,
        tmux::OPT_ATTENTION,
        tmux::bool_value(snapshot.attention),
    )?;
    tmux::set_pane_option(pane, tmux::OPT_BASE_TITLE, &snapshot.base_title)?;
    tmux::set_pane_option(
        pane,
        tmux::OPT_PERCENT,
        &snapshot
            .percent
            .map(|value| value.to_string())
            .unwrap_or_default(),
    )?;
    tmux::set_pane_option(
        pane,
        tmux::OPT_SOURCE,
        snapshot
            .source
            .map(ActivitySource::as_str)
            .unwrap_or_default(),
    )
}

fn parse_stored_percent(value: &str) -> Option<u8> {
    let value = value.parse::<u8>().ok()?;
    (value <= 100).then_some(value)
}

fn window_is_held_for_pane(pane: &str) -> bool {
    tmux::window_id_for_pane(pane)
        .as_deref()
        .is_some_and(tmux::window_has_hold)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AttentionTransition {
    attention: bool,
    notify: bool,
}

fn attention_transition(
    previous_activity: Activity,
    current_activity: Activity,
    pane_visible: bool,
    current_attention: bool,
) -> AttentionTransition {
    let creates_attention = matches!(previous_activity, Activity::Active | Activity::Paused)
        && matches!(current_activity, Activity::Done | Activity::Error)
        && !pane_visible
        && !current_attention;

    let attention = if current_activity == Activity::Active || pane_visible {
        false
    } else if creates_attention {
        true
    } else {
        current_attention
    };

    AttentionTransition {
        attention,
        notify: creates_attention,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_only_starts_on_hidden_completion() {
        assert_eq!(
            attention_transition(Activity::Active, Activity::Done, false, false),
            AttentionTransition {
                attention: true,
                notify: true,
            }
        );
        assert_eq!(
            attention_transition(Activity::Active, Activity::Error, false, false),
            AttentionTransition {
                attention: true,
                notify: true,
            }
        );
    }

    #[test]
    fn attention_clears_for_active_or_visible_panes() {
        assert_eq!(
            attention_transition(Activity::Done, Activity::Active, false, true),
            AttentionTransition {
                attention: false,
                notify: false,
            }
        );
        assert_eq!(
            attention_transition(Activity::Done, Activity::Done, true, true),
            AttentionTransition {
                attention: false,
                notify: false,
            }
        );
    }

    #[test]
    fn attention_is_not_created_for_visible_or_stale_state() {
        assert_eq!(
            attention_transition(Activity::Active, Activity::Done, true, false),
            AttentionTransition {
                attention: false,
                notify: false,
            }
        );
        assert_eq!(
            attention_transition(Activity::Done, Activity::Done, false, false),
            AttentionTransition {
                attention: false,
                notify: false,
            }
        );
        assert_eq!(
            attention_transition(Activity::Active, Activity::Done, false, true),
            AttentionTransition {
                attention: true,
                notify: false,
            }
        );
    }
}
