# Claude Code applet for COSMIC

A [COSMIC](https://system76.com/cosmic) panel applet that shows how many Claude Code
sessions are running, which one is working, which one is waiting for you, and how
much of your Claude subscription's 5-hour and weekly usage limits you have consumed.
It can also tell you - as a desktop notification - when a session has finished or
wants an answer.

```
[ ● 2 ▲ 1 · 68% ]         <- dot in the accent colour while a session works,
                             triangle in the warning colour with the number of
                             sessions blocked on you

┌──────────────────────────────┐
│ cosmic-applet-4f     working │
│ cosmic-applet · 62% context  │
│ · Opus 5                     │
│ api-server-7b      needs you │
│ api-server · 11% context     │
│ ──────────────────────────── │
│ 5-hour limit             68% │
│ ████████████████░░░░░░░░░░   │
│ resets in 1h 12m             │
│ Weekly limit             41% │
│ ██████████░░░░░░░░░░░░░░░░   │
│ resets Wed 09:00             │
│ ──────────────────────────── │
│ Notify when a session        │
│ finishes                 ◉── │
│ Notify when a session        │
│ needs me                 ◉── │
└──────────────────────────────┘
```

On a vertical panel there is no room for the percentage, so the applet shows the
icon, the session count and - when it applies - the waiting count only. Clicking it
opens the popup either way.

> Unofficial community project. Not affiliated with, endorsed by, or supported by
> Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.

## Where the numbers come from

Everything is read from local files. There are no network requests, and the applet
never touches `~/.claude/.credentials.json`.

| Shown | Source |
| --- | --- |
| Session list, session state, directory | `~/.claude/sessions/<pid>.json`, verified against `/proc` |
| 5-hour and weekly usage, reset times | Claude Code's status line payload (`rate_limits`) |
| Context usage and model per session | the same payload |

Claude Code reports one of four states per session: *working*, *needs you* (a
permission prompt or another dialog is open), *shell* and *idle*. Anything a future
release adds shows up as `?` rather than being guessed at. The reason a session is
waiting is not displayed.

Session cost is collected too, but only surfaces in `--dump`; the popup keeps to what
fits. If you moved Claude Code's config directory, the applet follows
`CLAUDE_CONFIG_DIR` just like Claude Code does.

The applet re-reads those files itself rather than watching them: every two seconds
while the popup is open or a session is working or waiting, every ten otherwise. It
is a handful of small JSON files and one `/proc` lookup per session, and nothing runs
in the background when the panel is not.

The usage percentages are the **official** ones - the same values `/usage` prints
inside Claude Code. Claude Code hands them to whatever status line command you
configure; this project ships a small bridge (`--status-line`) that stores them in
`$XDG_RUNTIME_DIR/cosmic-applet-claude-code/` for the applet to read, and prints a
status line for your terminal.

Because of that, usage numbers appear only once a Claude Code session has made at
least one request, and only for Claude.ai subscriptions (an API-key session has no
subscription limits). Without the bridge the applet still shows the session list.

Each session reports the last limits *it* saw, and an idle session keeps re-emitting
its cached numbers with a fresh timestamp - including windows that have already
reset. So the applet does not simply trust the newest snapshot: expired windows are
dropped, the latest window boundary wins, and within one window the highest reported
usage wins (usage only grows until the window rolls over).

## Notifications

Two things are worth interrupting someone for, and the applet notifies about both:

* **Claude Code finished** - a session that was working went idle.
* **Claude Code needs you** - a session opened a permission prompt or another dialog.

Both are on by default and either can be turned off in the popup. Starting a session,
closing its terminal and a session that is already waiting when the applet starts are
all silent: restarting the panel must not replay the state of the world as news.

The panel runs one applet process per monitor, so the processes agree among
themselves which one speaks, through an exclusive lock on
`$XDG_RUNTIME_DIR/cosmic-applet-claude-code/notifier.lock`. Whoever holds it
notifies; if that process goes away the kernel drops the lock and another takes over
on its next poll. Each session replaces its own previous notification instead of
stacking them up, and if there is no notification daemon at all the applet just
carries on.

The two switches are stored by cosmic-config in
`~/.config/cosmic/io.github.tenniedzwiedz.CosmicAppletClaudeCode/v1/` - one small file
per setting, written the first time you change it. Deleting that directory restores
the defaults.

## Install

Requires a COSMIC 1.0 desktop and [`just`](https://github.com/casey/just)
(`sudo apt install just` on Pop!_OS). Building needs either Docker (the default -
nothing else is installed on your system) or a local Rust toolchain (1.85+, the
crate is on edition 2024).

```sh
just install                 # builds in a container, installs into ~/.local
just install-bridge          # adds the status line hook to ~/.claude/settings.json
```

Then add the applet: **Settings → Desktop → Panel → Applets**.

The second command is optional: when the bridge is missing, the applet's popup says
so and offers a button that does the same thing.

Building with the host toolchain instead (what distribution packagers want):

```sh
just native=true install prefix=/usr destdir=pkgroot
```

Other recipes: `just build`, `just build-debug`, `just test`, `just check` (clippy,
rustfmt and AppStream metadata validation), `just dump` (prints the applet's data
as JSON), `just run-dev` (runs it as a normal window), `just restart-panel`,
`just validate-metainfo`, `just vendor`, `just clean`.

### Uninstall

```sh
just uninstall-bridge        # restores your previous settings.json state
just uninstall               # removes binary, .desktop, metainfo and icon
```

That order matters, and `just uninstall` enforces it: only the binary can
unwire the bridge, so removing it first would leave Claude Code calling a status
line command that no longer exists. Use `just force=true uninstall` if you
really want to skip the check.

Remove the applet from the panel in **Settings → Desktop → Panel → Applets** as well.

Your settings are deliberately left behind - removing a binary is not the same as
wanting your preferences gone. Delete
`~/.config/cosmic/io.github.tenniedzwiedz.CosmicAppletClaudeCode/` by hand if you
want nothing left. The timestamped `settings.json` backups stay in `~/.claude/` as
well; `just uninstall-bridge` has already removed the bridge's own
`~/.config/cosmic-applet-claude-code/bridge.json`.

## The status line bridge

`just install-bridge` (or `cosmic-applet-claude-code bridge install`) adds this to
`~/.claude/settings.json`, with the absolute path of the installed binary:

```json
"statusLine": {
  "type": "command",
  "command": "/home/you/.local/bin/cosmic-applet-claude-code --status-line",
  "refreshInterval": 10
}
```

* An existing `statusLine` command is **not** replaced. It is remembered in
  `~/.config/cosmic-applet-claude-code/bridge.json`, executed on every update with
  the same input, and its output is printed unchanged - so your status line keeps
  working exactly as before.
* `settings.json` is backed up before every change
  (`settings.json.backup-<epoch-ms>`); the five newest copies are kept.
* The applet's popup offers an **Install bridge** button whenever the bridge is not
  configured, so none of this needs a terminal.
* `just uninstall-bridge` restores the previous state.
* `cosmic-applet-claude-code bridge status` reports what is currently configured,
  including the case where the settings still point at a binary that has been
  moved or removed.
* Enabling a status line hides some of Claude Code's footer hints (`esc to
  interrupt`, `? for shortcuts`). That is Claude Code's behaviour, not the applet's.

Snapshots contain only what the applet reads: session id, model name, context
percentage, session cost and the rate limit windows. They live in your runtime
directory (mode 0700, wiped on reboot) and are deleted when they go stale.

## Command line

The applet binary is also the bridge and a debugging tool:

```
cosmic-applet-claude-code                        run the applet (the panel does this)
cosmic-applet-claude-code --dump                 print the whole data model as JSON
cosmic-applet-claude-code --status-line          read a payload on stdin, store a
                                                 snapshot, print a status line
cosmic-applet-claude-code bridge install         wire the status line hook up
cosmic-applet-claude-code bridge uninstall       restore the previous settings.json
cosmic-applet-claude-code bridge status          report what is configured
cosmic-applet-claude-code --help | --version
```

`--dump` is the quickest way to see what the applet sees: panel label, resolved
paths, bridge status, every session and the selected limits.

## Packaging

`packaging/` has notes for distributors and an Arch `PKGBUILD`. The short version:
build with `native=true`, install with `prefix=/usr destdir="$pkgdir"`, and never run
the bridge from a post-install hook - it edits the user's `~/.claude/settings.json`
and stays an explicit opt-in.

## Compatibility

| Applet | COSMIC | Claude Code |
| --- | --- | --- |
| 0.1.x - 0.2.x | 1.0 | 2.1.x |

The session files and the status line payload are Claude Code internals. If a future
release changes them, the applet degrades to showing less rather than failing - and
an issue report is welcome.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). User-visible strings live in
`i18n/<lang>/cosmic_applet_claude_code.ftl`; English and Polish ship today and further
translations are welcome - copy `i18n/en/` and translate.

## License

MIT - see [LICENSE](LICENSE).
