# Build environment for cosmic-applet-claude-code.
#
# Ubuntu 24.04 matches Pop!_OS 24.04 / COSMIC 1.0 (glibc 2.39), so a binary
# built here runs on the host without installing any -dev package there.
FROM ubuntu:24.04

ARG RUST_VERSION=stable

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
        appstream \
        build-essential \
        ca-certificates \
        clang \
        curl \
        git \
        libfontconfig-dev \
        libfreetype-dev \
        libinput-dev \
        libssl-dev \
        libudev-dev \
        libwayland-dev \
        libxkbcommon-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Toolchain lives in /usr/local/cargo; the registry cache is mounted at /cargo
# at run time so it survives between builds.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain ${RUST_VERSION} \
    && rustup component add clippy rustfmt \
    && chmod -R a+rX /usr/local/rustup /usr/local/cargo

# Ownership is inherited by the named volume mounted here, so an unprivileged
# UID can write the registry cache.
RUN mkdir -p /cargo && chown 1000:1000 /cargo
ENV CARGO_HOME=/cargo

WORKDIR /src
