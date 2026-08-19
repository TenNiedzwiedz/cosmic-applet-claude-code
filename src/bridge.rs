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
/// Infix of the copies `backup` leaves next to settings.json.
const BACKUP_INFIX: &str = ".backup-";
/// How many of those copies survive a change.
const KEEP_BACKUPS: usize = 5;

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

    backup(&path, KEEP_BACKUPS)?;
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

    backup(&path, KEEP_BACKUPS)?;
    write_settings(&path, &settings)?;
    let _ = std::fs::remove_file(config_path());

    Ok(match restored {
        Some(command) => format!("Bridge removed; restored your previous status line: {command}"),
        None => "Bridge removed from settings.json.".to_string(),
    })
}

/// Reports what is currently configured.
///
/// The wording is a contract, not just prose: the output starts with
/// `installed` exactly when our bridge is the configured status line, and
/// `just uninstall` branches on that. None of these strings go through `fl!` -
/// the command line stays English so scripts can read it.
pub fn status() -> Result<String> {
    let path = settings_path();
    let settings = read_settings(&path)?;
    let command = settings
        .get("statusLine")
        .and_then(|value| value.get("command"))
        .and_then(Value::as_str);

    Ok(match command {
        Some(command) if is_ours(command) => match binary_from(command) {
            // The settings still point at us, but the binary was moved or
            // removed: Claude Code runs a command that cannot work.
            Some(binary) if !Path::new(&binary).exists() => {
                format!("installed, but the binary is missing: {binary}")
            }
            _ => format!("installed: {command}"),
        },
        Some(command) => format!("not installed; another status line is configured: {command}"),
        None => "not installed".to_string(),
    })
}

fn is_ours(command: &str) -> bool {
    command.contains(BINARY_NAME) && command.contains(MARKER)
}

/// The binary path out of a command this module wrote: everything before the
/// marker, unquoted the way `quote` quoted it.
fn binary_from(command: &str) -> Option<String> {
    let (head, _) = command.split_once(MARKER)?;
    let head = head.trim();

    let unquoted = match head
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        Some(inner) => inner.replace(r"'\''", "'"),
        None => head.to_string(),
    };

    (!unquoted.is_empty()).then_some(unquoted)
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
/// we would rather leave a trail than surprise them. Only the newest `keep`
/// copies are kept, so the trail cannot grow without bound.
fn backup(path: &Path, keep: usize) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // One slot is about to be taken by the copy written below.
    prune_backups(path, keep.saturating_sub(1));

    // Milliseconds, not seconds: two changes within the same second would
    // otherwise land on the same name and the older copy would be lost.
    let backup = path.with_extension(format!(
        "json{BACKUP_INFIX}{}",
        crate::data::snapshots::now_millis()
    ));
    std::fs::copy(path, backup)?;
    Ok(())
}

/// Delete all but the `keep` newest copies of `path`. Ordering comes from the
/// timestamp in the name rather than from the mtime, which a restore or a
/// `cp -p` would happily rewrite. Files that only look like ours (anything
/// whose suffix is not a plain number) are left alone.
fn prune_backups(path: &Path, keep: usize) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return;
    };
    let prefix = format!("{name}{BACKUP_INFIX}");

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut backups: Vec<(u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let stamp = file_name.to_str()?.strip_prefix(&prefix)?.parse().ok()?;
            Some((stamp, entry.path()))
        })
        .collect();

    if backups.len() <= keep {
        return;
    }

    backups.sort_unstable_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    for (_, old) in backups.into_iter().skip(keep) {
        // A copy we cannot delete is untidy, never fatal.
        let _ = std::fs::remove_file(old);
    }
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

    /// `install` writes the command with `quote`, so `binary_from` has to
    /// survive exactly what `quote` produces.
    #[test]
    fn the_binary_path_survives_the_round_trip_through_quote() {
        for path in [
            "/usr/bin/cosmic-applet-claude-code",
            "/home/my apps/bin/cosmic-applet-claude-code",
            "/it's/here/cosmic-applet-claude-code",
        ] {
            let command = format!("{} {MARKER}", quote(path));
            assert_eq!(binary_from(&command).as_deref(), Some(path));
        }

        assert_eq!(binary_from("~/.claude/statusline.sh"), None);
    }

    #[test]
    fn only_the_newest_backups_survive() {
        let dir = tempdir("backups");
        let settings = dir.join("settings.json");
        std::fs::write(&settings, "{}\n").unwrap();

        for stamp in 1..=8u64 {
            std::fs::write(dir.join(format!("settings.json.backup-{stamp}")), "old").unwrap();
        }
        // Not ours: a suffix that is not a plain number, a different file's
        // copies, and something that merely looks similar.
        for name in [
            "settings.json.backup-yesterday",
            "settings.json.bak",
            "other.json.backup-3",
        ] {
            std::fs::write(dir.join(name), "keep me").unwrap();
        }

        backup(&settings, 5).unwrap();

        let mut surviving: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        surviving.sort();

        let ours: Vec<&String> = surviving
            .iter()
            .filter(|name| {
                name.strip_prefix("settings.json.backup-")
                    .is_some_and(|stamp| stamp.parse::<u64>().is_ok())
            })
            .collect();
        assert_eq!(ours.len(), 5, "four newest plus one fresh copy: {ours:?}");

        // The four newest of the pre-existing copies, and no older ones.
        for stamp in 5..=8u64 {
            assert!(dir.join(format!("settings.json.backup-{stamp}")).exists());
        }
        for stamp in 1..=4u64 {
            assert!(!dir.join(format!("settings.json.backup-{stamp}")).exists());
        }

        for name in [
            "settings.json.backup-yesterday",
            "settings.json.bak",
            "other.json.backup-3",
        ] {
            assert!(dir.join(name).exists(), "{name} is not ours to delete");
        }

        // The fresh copy is the file as it was, not an empty placeholder.
        let fresh = ours
            .iter()
            .max_by_key(|name| {
                name.strip_prefix("settings.json.backup-")
                    .and_then(|stamp| stamp.parse::<u64>().ok())
                    .unwrap_or_default()
            })
            .unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(fresh)).unwrap(), "{}\n");

        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A missing settings.json has nothing to copy, and no directory to prune.
    #[test]
    fn backing_up_a_file_that_is_not_there_is_a_no_op() {
        let dir = tempdir("nobackup");
        backup(&dir.join("settings.json"), 5).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn tempdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cosmic-applet-cc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
