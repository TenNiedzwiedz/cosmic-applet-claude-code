// SPDX-License-Identifier: MIT

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Busy,
    /// Claude Code is blocked on the user: a permission prompt, a plan review
    /// or another dialog is waiting for an answer. The reason travels in the
    /// session file too, but it is not ours to display.
    Waiting,
    /// The session dropped to a shell.
    Shell,
    Idle,
    Unknown,
}

impl From<Option<&str>> for SessionStatus {
    /// Claude Code writes one of `busy`, `shell`, `idle` or `waiting`.
    /// Anything else is a value from a future release, and guessing what it
    /// means would be worse than admitting we do not know.
    fn from(value: Option<&str>) -> Self {
        match value {
            Some("busy") => Self::Busy,
            Some("waiting") => Self::Waiting,
            Some("shell") => Self::Shell,
            Some("idle") => Self::Idle,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub pid: u32,
    pub session_id: String,
    /// Human readable name Claude Code derives from the directory, e.g. `api-server-7b`.
    pub name: String,
    pub cwd: String,
    pub status: SessionStatus,
    /// Unix milliseconds, when Claude Code last changed `status`.
    pub status_updated_at: Option<i64>,
    /// Unix milliseconds.
    pub started_at: Option<i64>,
    pub version: Option<String>,
    /// Filled in from the status line snapshot, when the bridge is installed.
    pub context_percent: Option<f64>,
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
}

impl Session {
    /// Last path component of the working directory, for compact display.
    pub fn dir_label(&self) -> &str {
        self.cwd
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(&self.cwd)
    }

    pub fn is_busy(&self) -> bool {
        self.status == SessionStatus::Busy
    }

    /// Blocked on the user - the one state worth interrupting them for.
    pub fn is_waiting(&self) -> bool {
        self.status == SessionStatus::Waiting
    }
}

/// One rate limit window as reported by Claude Code itself.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RateWindow {
    pub used_percentage: f64,
    /// Unix seconds.
    pub resets_at: i64,
}

impl RateWindow {
    /// Seconds until the window resets; `None` once it is in the past.
    pub fn seconds_until_reset(&self, now: i64) -> Option<i64> {
        (self.resets_at > now).then(|| self.resets_at - now)
    }
}

/// Account-wide usage limits, taken from the freshest snapshot that had them.
#[derive(Debug, Clone, Serialize)]
pub struct Limits {
    pub five_hour: Option<RateWindow>,
    pub seven_day: Option<RateWindow>,
    /// Unix seconds when the status line produced this data.
    pub captured_at: i64,
    pub source_session: String,
}

impl Limits {
    pub fn age_seconds(&self, now: i64) -> i64 {
        (now - self.captured_at).max(0)
    }

    /// Limits are stale once every window they describe has rolled over.
    pub fn is_expired(&self, now: i64) -> bool {
        let windows = [self.five_hour, self.seven_day];
        windows
            .iter()
            .flatten()
            .all(|window| window.resets_at <= now)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AppData {
    pub sessions: Vec<Session>,
    pub limits: Option<Limits>,
}

impl AppData {
    pub fn busy_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.is_busy()).count()
    }

    pub fn any_busy(&self) -> bool {
        self.busy_count() > 0
    }

    pub fn waiting_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.is_waiting()).count()
    }

    /// The percentage shown in the panel, or `None` once the window it
    /// describes has rolled over.
    pub fn five_hour_percent(&self, now: i64) -> Option<f64> {
        self.limits
            .as_ref()
            .and_then(|limits| limits.five_hour)
            .filter(|window| window.resets_at > now)
            .map(|window| window.used_percentage)
    }
}
