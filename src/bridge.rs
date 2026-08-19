// SPDX-License-Identifier: MIT
//!
//! Installs and removes the status line hook in Claude Code's settings.
//!
//! The applet's numbers come from the status line payload, which means editing
//! the user's `settings.json`. That file may already carry a status line the
//! user cares about, so an existing command is never thrown away: it is
//! remembered here and executed by `--status-line` on every update, with its
//! output passed straight through.

use serde_json::{Map, Value};
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

use crate::data::paths;

const MARKER: &str = "--status-line";
const BINARY_NAME: &str = "cosmic-applet-claude-code";

pub fn settings_path() -> PathBuf {
    paths::claude_dir().join("settings.json")
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::home().join(".config"));

    base.join(BINARY_NAME).join("bridge.json")
}

/// The status line command that was configured before the bridge took over.
pub fn chained_command() -> Option<String> {
    let contents = std::fs::read_to_string(config_path()).ok()?;
    let value: Value = serde_json::from_str(&contents).ok()?;
    value
        .get("chained_command")
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .map(str::to_string)
}

pub fn install() -> Result<String> {
    let binary = current_binary()?;
    let command = format!("{} {MARKER}", quote(&binary.to_string_lossy()));

    let path = settings_path();
    let mut settings = read_settings(&path)?;

    let previous = settings
        .get("statusLine")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let previous_command = previous
        .get("command")
        .and_then(Value::as_str)
        .filter(|existing| !is_ours(existing))
        .map(str::to_string);

    let mut status_line = previous;
    status_line.insert("type".into(), Value::String("command".into()));
    status_line.insert("command".into(), Value::String(command.clone()));
    // Rate limits only refresh when the status line runs; a timer keeps the
    // applet current while a session sits idle.
    status_line
        .entry("refreshInterval")
        .or_insert_with(|| Value::from(10));

    settings.insert("statusLine".into(), Value::Object(status_line));

    backup(&path)?;
    write_settings(&path, &settings)?;
    write_bridge_config(previous_command.as_deref())?;

    Ok(match previous_command {
        Some(existing) => format!(
            "Status line bridge installed in {}.\nYour previous status line is kept and still runs: {existing}",
            path.display()
        ),
        None => format!("Status line bridge installed in {}.", path.display()),
    })
}

pub fn uninstall() -> Result<String> {
    let path = settings_path();
    let mut settings = read_settings(&path)?;

    let Some(status_line) = settings
        .get("statusLine")
        .and_then(Value::as_object)
        .cloned()
    else {
        return Ok("No status line configured; nothing to remove.".to_string());
    };

    let is_bridge = status_line
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(is_ours);

    if !is_bridge {
        return Ok(format!(
            "The status line in {} is not the bridge; leaving it alone.",
            path.display()
        ));
    }

    let restored = chained_command();
    match restored.as_deref() {
        Some(command) => {
            let mut status_line = status_line;
            status_line.insert("command".into(), Value::String(command.to_string()));
            settings.insert("statusLine".into(), Value::Object(status_line));
        }
        None => {
            settings.remove("statusLine");
        }
    }

    backup(&path)?;
    write_settings(&path, &settings)?;
    let _ = std::fs::remove_file(config_path());

    Ok(match restored {
        Some(command) => format!("Bridge removed; restored your previous status line: {command}"),
        None => "Bridge removed from settings.json.".to_string(),
    })
}

pub fn status() -> Result<String> {
    let path = settings_path();
    let settings = read_settings(&path)?;
    let command = settings
        .get("statusLine")
        .and_then(|value| value.get("command"))
        .and_then(Value::as_str);

    Ok(match command {
        Some(command) if is_ours(command) => format!("installed: {command}"),
        Some(command) => format!("not installed; another status line is configured: {command}"),
        None => "not installed".to_string(),
    })
}

fn is_ours(command: &str) -> bool {
    command.contains(BINARY_NAME) && command.contains(MARKER)
}

fn current_binary() -> Result<PathBuf> {
    let path = std::env::current_exe()?;
    path.canonicalize().or(Ok(path))
}

fn read_settings(path: &Path) -> Result<Map<String, Value>> {
    match std::fs::read_to_string(path) {
        Ok(contents) if contents.trim().is_empty() => Ok(Map::new()),
        Ok(contents) => match serde_json::from_str::<Value>(&contents) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err(Error::new(
                ErrorKind::InvalidData,
                format!("{} does not contain a JSON object", path.display()),
            )),
            Err(error) => Err(Error::new(
                ErrorKind::InvalidData,
                format!("{} is not valid JSON: {error}", path.display()),
            )),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error),
    }
}

fn write_settings(path: &Path, settings: &Map<String, Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut json = serde_json::to_string_pretty(&Value::Object(settings.clone()))?;
    json.push('\n');

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Keep one timestamped copy per change; settings.json is the user's file and
/// we would rather leave a trail than surprise them.
fn backup(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let backup = path.with_extension(format!("json.backup-{}", crate::data::snapshots::now()));
    std::fs::copy(path, backup)?;
    Ok(())
}

fn write_bridge_config(chained: Option<&str>) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut config = Map::new();
    config.insert(
        "installed_at".into(),
        Value::from(crate::data::snapshots::now()),
    );
    if let Some(command) = chained {
        config.insert("chained_command".into(), Value::String(command.to_string()));
    }

    let mut json = serde_json::to_string_pretty(&Value::Object(config))?;
    json.push('\n');
    std::fs::write(path, json)
}

fn quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_its_own_command() {
        assert!(is_ours(
            "/home/u/.local/bin/cosmic-applet-claude-code --status-line"
        ));
        assert!(!is_ours("~/.claude/statusline.sh"));
        assert!(!is_ours("cosmic-applet-claude-code --dump"));
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(quote("/usr/bin/app"), "/usr/bin/app");
        assert_eq!(quote("/home/my apps/bin"), "'/home/my apps/bin'");
        assert_eq!(quote("/it's/here"), r"'/it'\''s/here'");
    }
}
