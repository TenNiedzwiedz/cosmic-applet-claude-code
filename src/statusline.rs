// SPDX-License-Identifier: MIT
//!
//! `--status-line` mode. Claude Code runs this on every status line update and
//! feeds it the session payload on stdin; we keep the few fields the applet
//! needs and print a status line back for the terminal.
//!
//! Doing this in the applet binary rather than a shell script keeps the setup
//! free of `jq` and gives the same JSON handling as the applet itself.

use serde_json::Value;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::bridge;
use crate::data::paths;

pub fn run() -> std::io::Result<()> {
    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload)?;

    let parsed: Option<Value> = serde_json::from_str(&payload).ok();

    if let Some(value) = parsed.as_ref() {
        // A failure to store the snapshot must never break the user's status
        // line, so errors are swallowed after being reported to stderr.
        if let Err(error) = store(value) {
            eprintln!("cosmic-applet-claude-code: could not store snapshot: {error}");
        }
    }

    let line = match bridge::chained_command() {
        Some(command) => run_chained(&command, &payload),
        None => parsed.as_ref().map(own_status_line).unwrap_or_default(),
    };

    if !line.is_empty() {
        println!("{line}");
    }

    Ok(())
}

/// Only the fields the applet reads are persisted - not the whole payload.
fn store(payload: &Value) -> std::io::Result<()> {
    let Some(session_id) = payload.get("session_id").and_then(Value::as_str) else {
        return Ok(());
    };

    let mut snapshot = serde_json::Map::new();
    snapshot.insert("session_id".into(), Value::String(session_id.to_string()));
    snapshot.insert(
        "captured_at".into(),
        Value::from(crate::data::snapshots::now()),
    );

    if let Some(name) = payload.pointer("/model/display_name") {
        snapshot.insert(
            "model".into(),
            serde_json::json!({ "display_name": name.clone() }),
        );
    }
    if let Some(used) = payload.pointer("/context_window/used_percentage") {
        snapshot.insert(
            "context_window".into(),
            serde_json::json!({ "used_percentage": used.clone() }),
        );
    }
    if let Some(cost) = payload.pointer("/cost/total_cost_usd") {
        snapshot.insert(
            "cost".into(),
            serde_json::json!({ "total_cost_usd": cost.clone() }),
        );
    }
    if let Some(limits) = payload.get("rate_limits") {
        snapshot.insert("rate_limits".into(), limits.clone());
    }

    let dir = paths::snapshot_dir();
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    // An existing directory keeps its old mode, so tighten it explicitly.
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));

    write_atomically(
        &dir.join(format!("{session_id}.json")),
        &Value::Object(snapshot),
    )
}

fn write_atomically(path: &Path, value: &Value) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(value)?)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, path)
}

fn run_chained(command: &str, payload: &str) -> String {
    let child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();

    let Ok(mut child) = child else {
        return String::new();
    };

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(payload.as_bytes());
    }

    match child.wait_with_output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
        Err(_) => String::new(),
    }
}

fn own_status_line(payload: &Value) -> String {
    let mut parts = Vec::new();

    if let Some(model) = payload
        .pointer("/model/display_name")
        .and_then(Value::as_str)
    {
        parts.push(model.to_string());
    }

    if let Some(dir) = payload
        .pointer("/workspace/current_dir")
        .and_then(Value::as_str)
    {
        let name = dir.rsplit('/').find(|part| !part.is_empty()).unwrap_or(dir);
        parts.push(name.to_string());
    }

    if let Some(ctx) = payload
        .pointer("/context_window/used_percentage")
        .and_then(Value::as_f64)
    {
        parts.push(format!("{ctx:.0}% ctx"));
    }

    if let Some(five) = payload
        .pointer("/rate_limits/five_hour/used_percentage")
        .and_then(Value::as_f64)
    {
        parts.push(format!("5h {five:.0}%"));
    }

    if let Some(week) = payload
        .pointer("/rate_limits/seven_day/used_percentage")
        .and_then(Value::as_f64)
    {
        parts.push(format!("7d {week:.0}%"));
    }

    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_status_line_from_a_full_payload() {
        let payload = serde_json::json!({
            "model": { "display_name": "Opus 5" },
            "workspace": { "current_dir": "/home/u/projects/api-server" },
            "context_window": { "used_percentage": 61.7 },
            "rate_limits": {
                "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
                "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
            }
        });

        assert_eq!(
            own_status_line(&payload),
            "Opus 5 · api-server · 62% ctx · 5h 24% · 7d 41%"
        );
    }

    #[test]
    fn missing_sections_are_skipped() {
        let payload = serde_json::json!({ "model": { "display_name": "Sonnet 5" } });
        assert_eq!(own_status_line(&payload), "Sonnet 5");
        assert_eq!(own_status_line(&serde_json::json!({})), "");
    }
}
