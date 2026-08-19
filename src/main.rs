// SPDX-License-Identifier: MIT
//!
//! A COSMIC panel applet that shows how many Claude Code sessions are running
//! and how much of the subscription's usage limits they have consumed.
//!
//! Besides the applet itself the binary provides the pieces that feed it:
//! `--status-line` is what Claude Code executes on every status line update,
//! and `bridge install` wires that up in the user's settings.

mod bridge;
mod data;
mod localize;
mod statusline;
mod window;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
cosmic-applet-claude-code - Claude Code sessions and usage limits in the COSMIC panel

Usage:
  cosmic-applet-claude-code              Run the applet (started by the panel)
  cosmic-applet-claude-code --dump       Print the applet's data as JSON and exit
  cosmic-applet-claude-code --status-line
                                         Read a Claude Code status line payload on
                                         stdin, store a snapshot, print a status line
  cosmic-applet-claude-code bridge install|uninstall|status
                                         Manage the status line hook in Claude Code's
                                         settings.json
  cosmic-applet-claude-code --help | --version
";

fn main() -> cosmic::iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        None => run_applet(),
        Some("--status-line") => exit_with(statusline::run()),
        Some("--dump") => exit_with(dump()),
        Some("bridge") => exit_with(run_bridge(args.get(1).map(String::as_str))),
        Some("--help" | "-h") => {
            print!("{HELP}");
            Ok(())
        }
        Some("--version" | "-V") => {
            println!("cosmic-applet-claude-code {VERSION}");
            Ok(())
        }
        Some(unknown) => {
            eprintln!("unknown argument: {unknown}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

fn run_applet() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    localize::localize();
    tracing::info!("starting cosmic-applet-claude-code {VERSION}");

    cosmic::applet::run::<window::Window>(())
}

fn dump() -> std::io::Result<()> {
    let data = data::collect();
    let now = data::snapshots::now();

    let report = serde_json::json!({
        "panel_label": window::panel_label(&data, now),
        "now": now,
        "paths": {
            "claude_dir": data::paths::claude_dir(),
            "sessions_dir": data::paths::sessions_dir(),
            "snapshot_dir": data::paths::snapshot_dir(),
            "settings": bridge::settings_path(),
        },
        "bridge": bridge::status().unwrap_or_else(|error| format!("unknown: {error}")),
        "data": data,
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_bridge(subcommand: Option<&str>) -> std::io::Result<()> {
    let message = match subcommand {
        Some("install") => bridge::install()?,
        Some("uninstall") => bridge::uninstall()?,
        Some("status") | None => bridge::status()?,
        Some(other) => {
            eprintln!("unknown bridge subcommand: {other}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    println!("{message}");
    Ok(())
}

fn exit_with(result: std::io::Result<()>) -> cosmic::iced::Result {
    if let Err(error) = result {
        eprintln!("cosmic-applet-claude-code: {error}");
        std::process::exit(1);
    }

    Ok(())
}
