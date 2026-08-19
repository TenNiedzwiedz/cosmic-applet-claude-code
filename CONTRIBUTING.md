# Contributing

Thanks for taking a look.

## Building

`just` drives everything. By default it compiles inside a container
(`ubuntu:24.04`, matching COSMIC 1.0's glibc), so no development packages are
needed on the host:

```sh
just build          # release build
just test           # unit tests
just check          # clippy, rustfmt, AppStream metadata
just dump           # print the applet's data model as JSON
just run-dev        # run the applet as a normal window
```

Add `native=true` to any recipe to use the host toolchain instead.

## Layout

| Path | What it does |
| --- | --- |
| `src/data/sessions.rs` | reads `~/.claude/sessions/*.json`, verifies PIDs against `/proc` |
| `src/data/snapshots.rs` | reads the status line snapshots, picks the freshest limits |
| `src/statusline.rs` | `--status-line`: stores a snapshot, prints a status line |
| `src/bridge.rs` | installs/removes the status line hook in `settings.json` |
| `src/window.rs` | the applet itself |

## Things to keep in mind

* The session files and the status line payload are Claude Code internals. Parse
  them defensively: a missing or renamed field must degrade the display, never
  panic.
* Never read credentials and never make network requests. The official usage
  numbers arrive through the status line payload; nothing needs to be estimated.
* Snapshots may only contain what the applet displays, and only in the runtime
  directory.
