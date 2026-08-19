// SPDX-License-Identifier: MIT

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Directory the snapshots live in: written by `--status-line`
/// (`crate::statusline`), read by the applet.
pub const SNAPSHOT_SUBDIR: &str = "cosmic-applet-claude-code";

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Claude Code's config directory. Honours `CLAUDE_CONFIG_DIR`, which Claude
/// Code itself supports, so users who moved it keep working.
pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"))
}

pub fn sessions_dir() -> PathBuf {
    claude_dir().join("sessions")
}

/// `$XDG_RUNTIME_DIR/cosmic-applet-claude-code`, with the same fallbacks the
/// bridge script uses.
pub fn snapshot_dir() -> PathBuf {
    runtime_base().join(SNAPSHOT_SUBDIR)
}

fn runtime_base() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }

    if let Ok(meta) = std::fs::metadata(home()) {
        let candidate = PathBuf::from(format!("/run/user/{}", meta.uid()));
        if candidate.is_dir() {
            return candidate;
        }
    }

    home().join(".cache")
}
