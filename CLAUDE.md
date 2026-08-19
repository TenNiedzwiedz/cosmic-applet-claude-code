# CLAUDE.md

Context for Claude Code sessions working on this repository.

## What this is

A COSMIC panel applet (Rust, libcosmic) that shows how many Claude Code sessions are
running, which one is working, and how much of the Claude subscription's 5-hour and
weekly limits is consumed. Unofficial, MIT, intended for public release under
`github.com/TenNiedzwiedz/cosmic-applet-claude-code`. App ID:
`io.github.tenniedzwiedz.CosmicAppletClaudeCode`.

Target platform: Pop!_OS 24.04 / COSMIC 1.0 (`cosmic-applets 1.0.15`), libcosmic
pinned to rev `511384f6`.

## The one thing to understand first

The usage percentages are **official numbers, not estimates**. Claude Code passes a
`rate_limits` object (`five_hour`/`seven_day`, each with `used_percentage` and
`resets_at`) to whatever status line command is configured. `--status-line` is that
command: it stores a small snapshot per session in
`$XDG_RUNTIME_DIR/cosmic-applet-claude-code/` and prints a status line. The applet
reads those snapshots.

Never replace this with estimation from `~/.claude/projects/**/*.jsonl`, never read
`~/.claude/.credentials.json`, never make network requests. Anthropic restricts OAuth
tokens to official clients, and the status line already provides the real numbers.

## Data sources

| Data | Source |
| --- | --- |
| Sessions, state, cwd, name | `~/.claude/sessions/<pid>.json` (honours `CLAUDE_CONFIG_DIR`) |
| Liveness | `/proc/<pid>/stat` field 22 vs the file's `procStart` - PIDs get recycled |
| Limits, context %, model, cost | status line snapshots written by `--status-line` |

Both are Claude Code internals. Parse defensively (`Option` everywhere): a renamed
field must degrade the display, never panic.

## Traps that cost time already

* **An idle session re-emits stale limits with a fresh timestamp.** Observed live: one
  session reported a 5-hour window that had already reset (14%) while the active one
  reported 46%. `snapshots::best_limits` therefore drops expired windows, prefers the
  later `resets_at`, and within one window prefers the higher usage (usage only grows
  until the window rolls over). Test:
  `an_idle_session_cannot_report_a_window_that_already_reset`.
* **A session's `status` is a closed set**: `busy`, `shell`, `idle`, `waiting`
  (verified in the Claude Code 2.1.235 bundle). `waiting` means a permission prompt or
  another dialog is blocking on the user, and it is the state the applet exists to
  surface - do not fold it into `idle`. Anything else must land on `Unknown` and be
  shown as `?`. The session file also carries `waitingFor` with the reason; it is
  deliberately not parsed and not displayed.
* **`/proc/<pid>/stat` field 22 is index 19 after the last `)`** - the command name may
  contain spaces and brackets, so never split the whole line.
* Snapshots are written by rename, so file watches break; the applet polls instead
  (`POLL_ACTIVE` 2 s while the popup is open or a session is working or waiting,
  `POLL_IDLE` 10 s otherwise - the slow rate only delays noticing that an idle session
  started working). Do not "fix" this with inotify without handling replaces.
* `bridge install` edits the user's `~/.claude/settings.json`. It backs the file up
  (`settings.json.backup-<epoch-ms>`, the five newest kept), keeps any existing
  `statusLine` by chaining to it, and `bridge uninstall` restores it. Keep that
  contract.
* The popup offers an install button when the bridge is missing, so `bridge::install`
  reports what it did (`Installed`) instead of returning prose - the command line
  words it in English, the applet through `fl!`.
* `bridge status` prints English, never `fl!`, and its output starts with `installed`
  exactly when our bridge is wired up - `just uninstall` greps for that prefix and
  refuses to delete the binary while the bridge would be left dangling
  (`just force=true uninstall` overrides).

## Layout

| Path | Role |
| --- | --- |
| `src/data/sessions.rs` | session files + `/proc` liveness |
| `src/data/snapshots.rs` | snapshot reading, limit selection, pruning |
| `src/data/model.rs` | `AppData`, `Session`, `Limits`, `RateWindow` |
| `src/statusline.rs` | `--status-line` mode |
| `src/bridge.rs` | `bridge install/uninstall/status` |
| `src/window.rs` | the applet UI, `panel_label` (shared with `--dump`) |
| `i18n/{en,pl}/…ftl` | all user-visible strings, via `fl!` |

## Commands

Builds run in a container by default (`ubuntu:24.04`, same glibc as the host), so no
Rust toolchain or `-dev` packages are needed on the machine. `just` itself is **not**
installed here - either `sudo apt install just` or call docker directly:

```sh
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/src" -w /src \
    -v cosmic-applet-claude-code-cargo:/cargo cosmic-applet-claude-code-build \
    sh -c "cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release"
```

With `just`: `just build | test | check | dump | run-dev | install | install-bridge |
validate-metainfo | vendor`, plus `native=true` for the host toolchain (that is what
packagers use). The build image carries `appstreamcli`, so `just check` validates the
metainfo too - no host tooling needed.

`./target/debug/cosmic-applet-claude-code --dump` prints the whole data model as JSON -
the fastest way to check behaviour without the panel.

## Verifying a change in the panel

```sh
install -Dm0755 target/release/cosmic-applet-claude-code ~/.local/bin/cosmic-applet-claude-code
pkill cosmic-panel     # cosmic-session respawns it, one applet process per output
cosmic-screenshot --interactive=false --notify=false --modal=false -s /tmp
```

The applet is registered in `~/.local/share/applications/` and listed in
`~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings` (backups of both the
panel config and `settings.json` sit next to the originals).

## Conventions

* English in code, comments, docs and commit messages; user-visible strings go through
  `fl!` with `en` + `pl` translations.
* `cargo fmt` and `cargo clippy -- -D warnings` must be clean, and
  `appstreamcli validate --no-net` must pass on the metainfo; CI runs all three.
* Every behavioural fix gets a test, preferably built from data actually observed on a
  real machine.
* Nothing outside the repo is modified without an explicit ask, and anything that
  touches the user's config must back it up and be reversible.
