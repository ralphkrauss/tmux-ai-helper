mod activity;
mod notify;
mod osc;
mod signals;
mod state;
mod title;
mod tmux;

use std::env;
use std::ffi::OsString;
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("tmux-ai-helper: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut args = env::args_os();
    let _program = args.next();

    match args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .as_deref()
    {
        Some("attach") => {
            let pane = required_arg(args.next(), "missing pane id for attach")?;
            tmux::attach(&pane)
        }
        Some("listen") => {
            let pane = required_arg(args.next(), "missing pane id for listen")?;
            state::listen(&pane)
        }
        Some("clear-pane") => {
            let pane = required_arg(args.next(), "missing pane id for clear-pane")?;
            state::clear_pane(&pane)
        }
        Some("clear-window") => {
            let window = required_arg(args.next(), "missing window id for clear-window")?;
            state::clear_window(&window)
        }
        Some("clear-session") => {
            let session = required_arg(args.next(), "missing session id for clear-session")?;
            state::clear_session(&session)
        }
        Some("-h") | Some("--help") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown command: {other}"),
        )),
    }
}

fn required_arg(arg: Option<OsString>, message: &'static str) -> io::Result<String> {
    arg.and_then(|value| value.into_string().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, message))
}

fn print_help() {
    eprintln!(
        "usage:\n  tmux-ai-helper attach <pane-id>\n  tmux-ai-helper listen <pane-id>\n  tmux-ai-helper clear-pane <pane-id>\n  tmux-ai-helper clear-window <window-id>\n  tmux-ai-helper clear-session <session-id>"
    );
}
