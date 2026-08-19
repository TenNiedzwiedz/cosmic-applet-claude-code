# Packaging notes

The applet installs like any other COSMIC applet: a binary, a `.desktop` entry
with `X-CosmicApplet=true`, an AppStream metainfo file and a symbolic icon.

```sh
just native=true prefix=/usr destdir="$pkgdir" install
```

* Build natively (`native=true`); the container image in the repository root is a
  convenience for contributors, not part of packaging.
* Offline builds: `just native=true vendor` produces `vendor.tar` plus the usual
  `.cargo/config.toml` snippet printed by `cargo vendor`.
* Runtime dependencies are the ones COSMIC itself already pulls in: wayland,
  libxkbcommon, fontconfig, freetype.
* The status line bridge is **not** enabled at install time - it edits the user's
  `~/.claude/settings.json`, so it stays an explicit, per-user opt-in
  (`cosmic-applet-claude-code bridge install`). Do not run it from a package
  post-install hook.
