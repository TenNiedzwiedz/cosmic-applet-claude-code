// SPDX-License-Identifier: MIT
//!
//! Desktop notifications for the two moments someone running several sessions
//! cares about: one has finished, or one is blocked on them.
//!
//! Two things make this less obvious than it looks. The panel starts one
//! applet process per output, so a lock file decides which of them speaks -
//! otherwise a three monitor desk gets every notification three times. And the
//! first poll after start only records where things stand, so restarting the
//! panel does not replay the state of every session as news.

use std::collections::HashMap;
use std::fs::File;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use crate::data::model::{Session, SessionStatus};
use crate::data::paths;

const APP_NAME: &str = "Claude Code";
const ICON: &str = "io.github.tenniedzwiedz.CosmicAppletClaudeCode-symbolic";
/// Let the notification server decide how long to show it.
const EXPIRE_DEFAULT: i32 = -1;
/// Passing 0 as `replaces_id` asks for a new notification rather than an
/// update of an existing one.
const NEW_NOTIFICATION: u32 = 0;
const LOCK_FILE: &str = "notifier.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Finished,
    NeedsInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: Kind,
    pub session_id: String,
    pub name: String,
    pub dir: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct State {
    status: SessionStatus,
    /// Unix milliseconds. Claude Code rewrites this whenever it sets the
    /// status, even to the value it already had.
    updated_at: Option<i64>,
}

#[derive(Default)]
pub struct Tracker {
    seen: HashMap<String, State>,
    primed: bool,
}

impl Tracker {
    /// Compare this poll against the last one and report what happened.
    ///
    /// A session that was working and is now idle has finished. A session that
    /// is waiting when it was not - or that is waiting again, which Claude
    /// Code marks by bumping the timestamp - wants an answer. Sessions that
    /// appear or disappear say nothing: neither starting a session nor closing
    /// its terminal is news.
    pub fn observe(
        &mut self,
        sessions: &[Session],
        notify_finished: bool,
        notify_waiting: bool,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let mut current = HashMap::with_capacity(sessions.len());

        for session in sessions {
            let state = State {
                status: session.status,
                updated_at: session.status_updated_at,
            };
            let previous = self.seen.get(&session.session_id).copied();
            current.insert(session.session_id.clone(), state);

            let Some(previous) = previous.filter(|_| self.primed) else {
                continue;
            };

            let kind = if notify_finished
                && previous.status == SessionStatus::Busy
                && state.status == SessionStatus::Idle
            {
                Kind::Finished
            } else if notify_waiting && state.status == SessionStatus::Waiting && previous != state
            {
                Kind::NeedsInput
            } else {
                continue;
            };

            events.push(Event {
                kind,
                session_id: session.session_id.clone(),
                name: session.name.clone(),
                dir: session.dir_label().to_string(),
            });
        }

        self.seen = current;
        self.primed = true;
        events
    }
}

/// Decides whether this process is the one that speaks.
#[derive(Default)]
pub struct Notifier {
    lock: Option<File>,
}

impl Notifier {
    /// Try to take the lock, or confirm we already hold it. One `flock` call
    /// while somebody else holds it, nothing at all once we do.
    ///
    /// The kernel releases the lock when the process exits, so the applet on
    /// another output takes over by itself - no cleanup, no stale lock after a
    /// crash.
    pub fn is_speaker(&mut self) -> bool {
        self.claim(&paths::snapshot_dir())
    }

    fn claim(&mut self, dir: &Path) -> bool {
        if self.lock.is_some() {
            return true;
        }

        if std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .is_err()
        {
            return false;
        }

        let Ok(file) = File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dir.join(LOCK_FILE))
        else {
            return false;
        };

        if rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_ok() {
            self.lock = Some(file);
            return true;
        }

        false
    }
}

/// Show one notification, replacing the one this session last produced.
/// Returns the server's id for the next replacement.
pub async fn send(summary: String, body: String, replaces: Option<u32>) -> Option<u32> {
    match notify(summary, body, replaces.unwrap_or(NEW_NOTIFICATION)).await {
        Ok(id) => Some(id),
        Err(error) => {
            // No session bus, no notification daemon, a refused call: none of
            // it is worth breaking the applet over.
            tracing::warn!(%error, "could not deliver a notification");
            None
        }
    }
}

/// A fresh connection per notification. This runs a handful of times an hour,
/// and keeping one around would mean carrying an async lock through the
/// applet's state for no measurable gain.
async fn notify(summary: String, body: String, replaces: u32) -> zbus::Result<u32> {
    let connection = zbus::Connection::session().await?;

    let reply = connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                APP_NAME,
                replaces,
                ICON,
                summary,
                body,
                Vec::<&str>::new(),
                HashMap::<&str, zbus::zvariant::Value<'_>>::new(),
                EXPIRE_DEFAULT,
            ),
        )
        .await?;

    reply.body().deserialize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, status: SessionStatus, updated_at: Option<i64>) -> Session {
        Session {
            pid: 1,
            session_id: id.to_string(),
            name: format!("{id}-7b"),
            cwd: format!("/home/u/projects/{id}"),
            status,
            status_updated_at: updated_at,
            started_at: None,
            version: None,
            context_percent: None,
            model: None,
            cost_usd: None,
        }
    }

    fn kinds(events: &[Event]) -> Vec<Kind> {
        events.iter().map(|event| event.kind).collect()
    }

    /// Restarting the panel must not announce everything that is already on
    /// screen.
    #[test]
    fn the_first_poll_is_only_a_baseline() {
        let mut tracker = Tracker::default();
        let sessions = [
            session("a", SessionStatus::Busy, Some(1)),
            session("b", SessionStatus::Waiting, Some(1)),
        ];

        assert!(tracker.observe(&sessions, true, true).is_empty());
        // ... and the state it recorded is still the basis for the next poll.
        assert!(tracker.observe(&sessions, true, true).is_empty());
    }

    #[test]
    fn work_ending_and_a_dialog_opening_are_the_two_events() {
        let mut tracker = Tracker::default();
        tracker.observe(
            &[
                session("a", SessionStatus::Busy, Some(1)),
                session("b", SessionStatus::Busy, Some(1)),
            ],
            true,
            true,
        );

        let events = tracker.observe(
            &[
                session("a", SessionStatus::Idle, Some(2)),
                session("b", SessionStatus::Waiting, Some(2)),
            ],
            true,
            true,
        );

        assert_eq!(kinds(&events), vec![Kind::Finished, Kind::NeedsInput]);
        assert_eq!(events[0].name, "a-7b");
        assert_eq!(events[0].dir, "a");
    }

    /// The applet polls; the same state read twice is not a second event.
    #[test]
    fn an_unchanged_session_is_silent() {
        let mut tracker = Tracker::default();
        let waiting = [session("a", SessionStatus::Waiting, Some(2))];

        tracker.observe(&[session("a", SessionStatus::Busy, Some(1))], true, true);
        assert_eq!(
            kinds(&tracker.observe(&waiting, true, true)),
            vec![Kind::NeedsInput]
        );
        assert!(tracker.observe(&waiting, true, true).is_empty());

        // Answered and asked again inside one poll interval: the status did
        // not change, but Claude Code bumped the timestamp.
        let again = [session("a", SessionStatus::Waiting, Some(3))];
        assert_eq!(
            kinds(&tracker.observe(&again, true, true)),
            vec![Kind::NeedsInput]
        );
    }

    #[test]
    fn starting_and_closing_a_session_is_not_news() {
        let mut tracker = Tracker::default();
        tracker.observe(&[session("a", SessionStatus::Idle, Some(1))], true, true);

        // A session that appears already waiting has not changed under us.
        let events = tracker.observe(
            &[
                session("a", SessionStatus::Idle, Some(1)),
                session("b", SessionStatus::Waiting, Some(1)),
            ],
            true,
            true,
        );
        assert!(events.is_empty());

        // And one that disappears while working never "finished".
        let events = tracker.observe(&[session("b", SessionStatus::Waiting, Some(1))], true, true);
        assert!(events.is_empty());
    }

    #[test]
    fn each_kind_can_be_turned_off_on_its_own() {
        let before = [
            session("a", SessionStatus::Busy, Some(1)),
            session("b", SessionStatus::Busy, Some(1)),
        ];
        let after = [
            session("a", SessionStatus::Idle, Some(2)),
            session("b", SessionStatus::Waiting, Some(2)),
        ];

        let mut tracker = Tracker::default();
        tracker.observe(&before, true, false);
        assert_eq!(
            kinds(&tracker.observe(&after, true, false)),
            vec![Kind::Finished]
        );

        let mut tracker = Tracker::default();
        tracker.observe(&before, false, true);
        assert_eq!(
            kinds(&tracker.observe(&after, false, true)),
            vec![Kind::NeedsInput]
        );

        let mut tracker = Tracker::default();
        tracker.observe(&before, false, false);
        assert!(tracker.observe(&after, false, false).is_empty());
    }

    /// Whoever holds the lock keeps it; a second Notifier in the same process
    /// sees it is taken, exactly as an applet on another output would.
    #[test]
    fn only_one_notifier_speaks() {
        let dir =
            std::env::temp_dir().join(format!("cosmic-applet-cc-notifier-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut first = Notifier::default();
        let mut second = Notifier::default();

        assert!(first.claim(&dir));
        assert!(first.claim(&dir), "holding the lock is not re-acquired");
        // flock treats two open descriptions independently, even inside one
        // process, so this is what an applet on another output sees.
        assert!(!second.claim(&dir));

        drop(first);
        assert!(
            second.claim(&dir),
            "the lock is free once its holder is gone"
        );

        drop(second);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
