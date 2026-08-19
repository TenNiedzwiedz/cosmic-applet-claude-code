// SPDX-License-Identifier: MIT
//!
//! Reads `~/.claude/sessions/<pid>.json`. These files outlive the process that
//! wrote them (a killed session leaves its file behind), so every entry is
//! checked against `/proc` before it is reported as live.

use serde::Deserialize;
use std::path::Path;

use super::model::{Session, SessionStatus};
use super::paths;

#[derive(Debug, Deserialize)]
struct RawSession {
    pid: Option<u32>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    cwd: Option<String>,
    name: Option<String>,
    status: Option<String>,
    #[serde(rename = "startedAt")]
    started_at: Option<i64>,
    #[serde(rename = "procStart")]
    proc_start: Option<String>,
    version: Option<String>,
}

pub fn live_sessions() -> Vec<Session> {
    let mut sessions = read_dir(&paths::sessions_dir());
    sessions.sort_by_key(|session| session.started_at.unwrap_or(i64::MAX));
    sessions
}

pub fn read_dir(dir: &Path) -> Vec<Session> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| parse_file(&entry.path()))
        .filter(|parsed| is_alive(parsed.pid, parsed.proc_start.as_deref()))
        .map(|parsed| parsed.session)
        .collect()
}

struct Parsed {
    session: Session,
    pid: u32,
    proc_start: Option<String>,
}

fn parse_file(path: &Path) -> Option<Parsed> {
    let contents = std::fs::read_to_string(path).ok()?;
    let raw: RawSession = serde_json::from_str(&contents).ok()?;

    let pid = raw.pid?;
    let session_id = raw.session_id?;
    let cwd = raw.cwd.unwrap_or_default();
    let name = raw.name.filter(|name| !name.is_empty()).unwrap_or_else(|| {
        cwd.rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or("claude")
            .to_string()
    });

    Some(Parsed {
        session: Session {
            pid,
            session_id,
            name,
            cwd,
            status: SessionStatus::from(raw.status.as_deref()),
            started_at: raw.started_at,
            version: raw.version,
            context_percent: None,
            model: None,
            cost_usd: None,
        },
        pid,
        proc_start: raw.proc_start,
    })
}

/// A PID alone is not enough: PIDs are recycled. Claude Code records the
/// kernel's start time for the process, which is field 22 of `/proc/<pid>/stat`
/// (counted after the parenthesised command name, which may itself contain
/// spaces or brackets).
fn is_alive(pid: u32, proc_start: Option<&str>) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };

    let Some(expected) = proc_start else {
        // No recorded start time: trust the PID, an existing /proc entry is all
        // we can check.
        return true;
    };

    match start_time_field(&stat) {
        Some(actual) => actual == expected,
        None => true,
    }
}

fn start_time_field(stat: &str) -> Option<&str> {
    let after_comm = &stat[stat.rfind(')')? + 1..];
    // Field 3 (state) is the first one after the command name, so field 22 sits
    // at index 19.
    after_comm.split_whitespace().nth(19)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_time_is_field_22() {
        let stat = "76640 (claude) S 76639 76640 76639 0 -1 4194304 12 0 0 0 5 1 0 0 20 0 21 0 2403737 123 456";
        assert_eq!(start_time_field(stat), Some("2403737"));
    }

    #[test]
    fn command_names_with_spaces_and_brackets_do_not_shift_fields() {
        let stat = "42 (weird ) name) S 1 42 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 999 0 0";
        assert_eq!(start_time_field(stat), Some("999"));
    }

    #[test]
    fn dead_pid_is_not_alive() {
        // PID 0 never has a /proc entry.
        assert!(!is_alive(0, Some("1")));
    }

    #[test]
    fn parses_a_real_session_file() {
        let dir = tempdir();
        let file = dir.join("4242.json");
        std::fs::write(
            &file,
            r#"{"pid":4242,"sessionId":"abc","cwd":"/home/u/projects/api-server","startedAt":1787034128903,"procStart":"10456","version":"2.1.233","kind":"interactive","name":"api-server-7b","status":"idle"}"#,
        )
        .unwrap();

        let parsed = parse_file(&file).expect("file should parse");
        assert_eq!(parsed.session.name, "api-server-7b");
        assert_eq!(parsed.session.dir_label(), "api-server");
        assert_eq!(parsed.session.status, SessionStatus::Idle);
        assert_eq!(parsed.proc_start.as_deref(), Some("10456"));

        // Unknown future fields must not break parsing.
        std::fs::write(
            &file,
            r#"{"pid":4242,"sessionId":"abc","cwd":"/tmp","somethingNew":{"a":1}}"#,
        )
        .unwrap();
        let parsed = parse_file(&file).expect("unknown fields are tolerated");
        assert_eq!(parsed.session.status, SessionStatus::Unknown);

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cosmic-applet-claude-code-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
