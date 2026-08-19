// SPDX-License-Identifier: MIT
//!
//! Reads the snapshots written by `res/statusline-bridge.sh`. Each running
//! Claude Code session hands its status line payload to that script, which
//! stores it as one JSON file per session. The payload carries the official
//! subscription limits (`rate_limits`), so nothing here is estimated.

use serde::Deserialize;
use std::path::Path;

use super::model::{Limits, RateWindow};
use super::paths;

/// Snapshots older than this are ignored and cleaned up: the session that wrote
/// them is long gone and the numbers no longer describe anything useful.
const MAX_AGE_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub session_id: String,
    pub captured_at: i64,
    pub context_percent: Option<f64>,
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
    pub five_hour: Option<RateWindow>,
    pub seven_day: Option<RateWindow>,
}

#[derive(Debug, Deserialize)]
struct RawSnapshot {
    session_id: Option<String>,
    /// Added by the bridge, Unix seconds.
    captured_at: Option<i64>,
    model: Option<RawModel>,
    context_window: Option<RawContext>,
    cost: Option<RawCost>,
    rate_limits: Option<RawRateLimits>,
}

#[derive(Debug, Deserialize)]
struct RawModel {
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawContext {
    used_percentage: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RawCost {
    total_cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RawRateLimits {
    five_hour: Option<RawWindow>,
    seven_day: Option<RawWindow>,
}

#[derive(Debug, Deserialize)]
struct RawWindow {
    used_percentage: Option<f64>,
    resets_at: Option<i64>,
}

impl RawWindow {
    fn into_window(self) -> Option<RateWindow> {
        Some(RateWindow {
            used_percentage: self.used_percentage?,
            resets_at: self.resets_at?,
        })
    }
}

pub fn load_all() -> Vec<Snapshot> {
    read_dir(&paths::snapshot_dir(), now())
}

pub fn read_dir(dir: &Path, now: i64) -> Vec<Snapshot> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut snapshots = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }

        match parse_file(&path) {
            Some(snapshot) if now - snapshot.captured_at <= MAX_AGE_SECONDS => {
                snapshots.push(snapshot)
            }
            // Expired or unparsable: the writing session is gone, drop the file
            // so the directory does not grow without bound.
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.captured_at));
    snapshots
}

/// Rate limits belong to the account, but each session reports the last values
/// *it* saw. An idle session keeps re-emitting its cached numbers with a fresh
/// timestamp, and those can be hours old - one of the sessions on the author's
/// machine reported a window that had already reset. So the freshest write does
/// not win; each window is picked on its own merits:
///
/// 1. windows that have already reset are dropped,
/// 2. the latest window boundary (`resets_at`) wins, since a session that saw a
///    newer window has newer information,
/// 3. within the same window the highest usage wins, because usage only grows
///    until the window rolls over.
pub fn best_limits(snapshots: &[Snapshot], now: i64) -> Option<Limits> {
    let five_hour = pick_window(snapshots, now, |snapshot| snapshot.five_hour);
    let seven_day = pick_window(snapshots, now, |snapshot| snapshot.seven_day);

    let chosen = five_hour.as_ref().or(seven_day.as_ref())?;

    Some(Limits {
        five_hour: five_hour.as_ref().map(|picked| picked.window),
        seven_day: seven_day.as_ref().map(|picked| picked.window),
        captured_at: chosen.captured_at,
        source_session: chosen.session_id.clone(),
    })
}

struct Picked {
    window: RateWindow,
    captured_at: i64,
    session_id: String,
}

fn pick_window(
    snapshots: &[Snapshot],
    now: i64,
    select: impl Fn(&Snapshot) -> Option<RateWindow>,
) -> Option<Picked> {
    snapshots
        .iter()
        .filter_map(|snapshot| {
            let window = select(snapshot).filter(|window| window.resets_at > now)?;
            Some(Picked {
                window,
                captured_at: snapshot.captured_at,
                session_id: snapshot.session_id.clone(),
            })
        })
        .max_by(|a, b| {
            a.window
                .resets_at
                .cmp(&b.window.resets_at)
                .then_with(|| {
                    a.window
                        .used_percentage
                        .total_cmp(&b.window.used_percentage)
                })
                .then_with(|| a.captured_at.cmp(&b.captured_at))
        })
}

fn parse_file(path: &Path) -> Option<Snapshot> {
    let contents = std::fs::read_to_string(path).ok()?;
    let raw: RawSnapshot = serde_json::from_str(&contents).ok()?;

    let session_id = raw.session_id.or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
    })?;

    let captured_at = raw.captured_at.or_else(|| file_mtime(path))?;
    let (five_hour, seven_day) = match raw.rate_limits {
        Some(limits) => (
            limits.five_hour.and_then(RawWindow::into_window),
            limits.seven_day.and_then(RawWindow::into_window),
        ),
        None => (None, None),
    };

    Some(Snapshot {
        session_id,
        captured_at,
        context_percent: raw.context_window.and_then(|ctx| ctx.used_percentage),
        model: raw.model.and_then(|model| model.display_name),
        cost_usd: raw.cost.and_then(|cost| cost.total_cost_usd),
        five_hour,
        seven_day,
    })
}

fn file_mtime(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(secs).ok()
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cosmic-applet-cc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn freshest_snapshot_with_limits_wins() {
        let dir = tempdir("limits");
        write(
            &dir,
            "old.json",
            r#"{"session_id":"old","captured_at":1000,"rate_limits":{"five_hour":{"used_percentage":10.0,"resets_at":9999}}}"#,
        );
        write(
            &dir,
            "new.json",
            r#"{"session_id":"new","captured_at":2000,"rate_limits":{"five_hour":{"used_percentage":42.5,"resets_at":9999},"seven_day":{"used_percentage":13.0,"resets_at":99999}}}"#,
        );
        // A session that never reached the API: no rate_limits at all.
        write(
            &dir,
            "newest.json",
            r#"{"session_id":"newest","captured_at":3000,"context_window":{"used_percentage":7.5}}"#,
        );

        let snapshots = read_dir(&dir, 3000);
        assert_eq!(snapshots.len(), 3);

        let limits = best_limits(&snapshots, 3000).expect("limits from the snapshot that has them");
        assert_eq!(limits.source_session, "new");
        assert_eq!(limits.five_hour.unwrap().used_percentage, 42.5);
        assert_eq!(limits.seven_day.unwrap().used_percentage, 13.0);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn api_key_sessions_without_limits_are_still_usable() {
        let dir = tempdir("nolimits");
        write(
            &dir,
            "s.json",
            r#"{"session_id":"s","captured_at":10,"context_window":{"used_percentage":31.0},"model":{"display_name":"Opus 5"}}"#,
        );

        let snapshots = read_dir(&dir, 10);
        assert_eq!(snapshots[0].context_percent, Some(31.0));
        assert_eq!(snapshots[0].model.as_deref(), Some("Opus 5"));
        assert!(best_limits(&snapshots, 10).is_none());

        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Reproduces what three live sessions actually reported: an idle session
    /// re-emitting a window that had already reset, next to two sessions in the
    /// current window.
    #[test]
    fn an_idle_session_cannot_report_a_window_that_already_reset() {
        let dir = tempdir("idle");
        let now = 1_787_061_419;
        write(
            &dir,
            "idle.json",
            r#"{"session_id":"idle","captured_at":1787061413,"rate_limits":{"five_hour":{"used_percentage":14.0,"resets_at":1787052000},"seven_day":{"used_percentage":8.0,"resets_at":1787166000}}}"#,
        );
        write(
            &dir,
            "quiet.json",
            r#"{"session_id":"quiet","captured_at":1787061413,"rate_limits":{"five_hour":{"used_percentage":7.0,"resets_at":1787070600},"seven_day":{"used_percentage":11.0,"resets_at":1787166000}}}"#,
        );
        write(
            &dir,
            "busy.json",
            r#"{"session_id":"busy","captured_at":1787061419,"rate_limits":{"five_hour":{"used_percentage":46.0,"resets_at":1787070600},"seven_day":{"used_percentage":15.0,"resets_at":1787166000}}}"#,
        );

        let snapshots = read_dir(&dir, now);
        let limits = best_limits(&snapshots, now).expect("current window");

        assert_eq!(limits.five_hour.unwrap().used_percentage, 46.0);
        assert_eq!(limits.five_hour.unwrap().resets_at, 1787070600);
        assert_eq!(limits.seven_day.unwrap().used_percentage, 15.0);
        assert_eq!(limits.source_session, "busy");

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_and_broken_files_are_removed() {
        let dir = tempdir("stale");
        write(
            &dir,
            "stale.json",
            r#"{"session_id":"stale","captured_at":1}"#,
        );
        write(&dir, "broken.json", "{not json");

        let snapshots = read_dir(&dir, MAX_AGE_SECONDS + 100);
        assert!(snapshots.is_empty());
        assert!(!dir.join("stale.json").exists());
        assert!(!dir.join("broken.json").exists());

        std::fs::remove_dir_all(dir).unwrap();
    }
}
