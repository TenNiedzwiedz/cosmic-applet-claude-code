name := 'cosmic-applet-claude-code'
appid := 'io.github.tenniedzwiedz.CosmicAppletClaudeCode'

# Install locations. Override for packaging:
#   just prefix=/usr destdir=pkgroot install
prefix := env_var_or_default('PREFIX', env_var('HOME') / '.local')
destdir := env_var_or_default('DESTDIR', '')

# Set native=true to use the host toolchain instead of the build container
# (distribution packagers want this).
native := 'false'

image := name + '-build'
cargo-volume := name + '-cargo'

bin-src := 'target' / 'release' / name
metainfo-src := 'data' / (appid + '.metainfo.xml')
# DESTDIR semantics: plain concatenation, so 'pkgroot' + '/usr' = 'pkgroot/usr'.
base-dir := destdir + prefix
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / (appid + '.desktop')
metainfo-dst := base-dir / 'share' / 'metainfo' / (appid + '.metainfo.xml')
icon-dst := base-dir / 'share' / 'icons' / 'hicolor' / 'scalable' / 'apps' / (appid + '-symbolic.svg')

default: build

# Build the container image used for compilation (no-op with native=true).
image:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ '{{native}}' != 'true' ]; then
        docker build -t '{{image}}' .
    fi

# Run a command, in the container unless native=true.
[private]
run command:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ '{{native}}' = 'true' ]; then
        {{command}}
    else
        just native='{{native}}' image
        docker run --rm -t \
            -u "$(id -u):$(id -g)" \
            -v "$PWD:/src" -w /src \
            -v '{{cargo-volume}}:/cargo' \
            '{{image}}' {{command}}
    fi

[private]
cargo *args: (run ('cargo ' + args))

build: (cargo 'build --release')

build-debug: (cargo 'build')

test: (cargo 'test')

check: (cargo 'clippy --all-targets -- -D warnings') (cargo 'fmt --check') validate-metainfo

# --no-net keeps this offline and deterministic: the <url> tags only resolve
# once the repository is public, and a flaky network must not fail the check.

# Validate the AppStream metadata that software centres read.
validate-metainfo: (run ('appstreamcli validate --no-net --explain ' + metainfo-src))

# Print the applet's data model as JSON without starting the GUI.
dump: build-debug
    ./target/debug/{{name}} --dump

# Run the applet as a normal window, without touching the panel.
run-dev: build-debug
    ./target/debug/{{name}}

install: build
    install -Dm0755 '{{bin-src}}' '{{bin-dst}}'
    install -Dm0644 '{{metainfo-src}}' '{{metainfo-dst}}'
    install -Dm0644 data/icons/{{appid}}-symbolic.svg '{{icon-dst}}'
    mkdir -p '{{base-dir}}/share/applications'
    sed 's|@BINDIR@|{{prefix}}/bin|g' data/{{appid}}.desktop.in > '{{desktop-dst}}'
    chmod 0644 '{{desktop-dst}}'
    @echo 'Installed. Add the applet in Settings -> Desktop -> Panel -> Applets,'
    @echo 'then run: just install-bridge'

uninstall:
    rm -f '{{bin-dst}}' '{{desktop-dst}}' '{{metainfo-dst}}' '{{icon-dst}}'

# Wire the statusline bridge into ~/.claude/settings.json (chains, never replaces).
install-bridge:
    '{{bin-dst}}' bridge install

uninstall-bridge:
    '{{bin-dst}}' bridge uninstall

# Restart the panel so it picks up a rebuilt applet.
restart-panel:
    -pkill cosmic-panel

# Vendored dependency tarball for offline/distro builds.
vendor:
    #!/usr/bin/env bash
    set -euo pipefail
    just native='{{native}}' cargo 'vendor --versioned-dirs vendor'
    tar pcf vendor.tar vendor
    rm -rf vendor

clean:
    rm -rf target vendor vendor.tar
