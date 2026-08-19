// SPDX-License-Identifier: MIT
//!
//! Everything the applet knows about Claude Code, read from two directories of
//! small JSON files. No network access, no credentials, no transcript parsing.

pub mod model;
pub mod paths;
pub mod sessions;
pub mod snapshots;

pub use model::AppData;

/// Collect the full applet state: live sessions enriched with whatever the
/// status line bridge captured, plus account-wide usage limits.
pub fn collect() -> AppData {
    let mut sessions = sessions::live_sessions();
    let snapshots = snapshots::load_all();

    for session in &mut sessions {
        if let Some(snap) = snapshots
            .iter()
            .find(|s| s.session_id == session.session_id)
        {
            session.context_percent = snap.context_percent;
            session.model = snap.model.clone();
            session.cost_usd = snap.cost_usd;
        }
    }

    let limits = snapshots::best_limits(&snapshots, snapshots::now());

    AppData { sessions, limits }
}
