use std::fs::OpenOptions;
use std::io::{self, Write};
use std::process::Command;

use crate::activity::Activity;
use crate::tmux;

pub struct Notification {
    pub pane: String,
    pub window: String,
    pub session: String,
    pub activity: Activity,
    pub title: String,
}

pub fn send(notification: &Notification) {
    let backends = tmux::get_session_option(&notification.session, tmux::OPT_NOTIFY_BACKENDS)
        .unwrap_or_else(|| "bell".to_owned());

    for backend in backends
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(str::trim)
        .filter(|backend| !backend.is_empty())
    {
        let result = match backend {
            "bell" => send_bell(notification),
            "command" => run_command(notification),
            "none" | "off" => Ok(()),
            _ => Ok(()),
        };

        if let Err(err) = result {
            eprintln!("tmux-ai-helper: notify backend {backend:?} failed: {err}");
        }
    }
}

fn send_bell(notification: &Notification) -> io::Result<()> {
    for tty in tmux::list_client_ttys(&notification.session) {
        let mut tty = OpenOptions::new().write(true).open(tty)?;
        tty.write_all(b"\x07")?;
    }

    Ok(())
}

fn run_command(notification: &Notification) -> io::Result<()> {
    let Some(command) = tmux::get_session_option(&notification.session, tmux::OPT_NOTIFY_COMMAND)
    else {
        return Ok(());
    };

    if command.trim().is_empty() {
        return Ok(());
    }

    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("TMUX_AI_HELPER_PANE", &notification.pane)
        .env("TMUX_AI_HELPER_WINDOW", &notification.window)
        .env("TMUX_AI_HELPER_SESSION", &notification.session)
        .env("TMUX_AI_HELPER_ACTIVITY", notification.activity.as_str())
        .env("TMUX_AI_HELPER_TITLE", &notification.title)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "notify command failed with status {status}"
        )))
    }
}
