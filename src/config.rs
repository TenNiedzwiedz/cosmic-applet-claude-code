// SPDX-License-Identifier: MIT
//!
//! The applet's own settings. They live in cosmic-config rather than a file of
//! our own so the desktop's tooling can read them and a future settings page
//! has somewhere to write.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::window::APP_ID;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    /// Notify when a session stops working.
    pub notify_finished: bool,
    /// Notify when a session blocks on the user.
    pub notify_waiting: bool,
}

impl Default for Config {
    /// Both on: telling the user which session wants them is the reason this
    /// applet exists, and the popup can turn either off.
    fn default() -> Self {
        Self {
            notify_finished: true,
            notify_waiting: true,
        }
    }
}

/// Load the settings, tolerating every failure - a missing, unreadable or
/// half-broken config leaves the applet running on the defaults rather than
/// not running at all.
pub fn load() -> (Option<cosmic_config::Config>, Config) {
    let handler = match cosmic_config::Config::new(APP_ID, Config::VERSION) {
        Ok(handler) => handler,
        Err(error) => {
            tracing::warn!(%error, "no config storage; using defaults");
            return (None, Config::default());
        }
    };

    let config = match Config::get_entry(&handler) {
        Ok(config) => config,
        Err((errors, config)) => {
            for error in errors {
                tracing::warn!(%error, "config key fell back to its default");
            }
            config
        }
    };

    (Some(handler), config)
}
