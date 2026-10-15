#!/usr/bin/env bash
# The site's build on Cloudflare Pages ("Build command": bash tools/cloudflare-build.sh,
# "Build output directory": build/dist). Installs what the build image lacks (Rust as
# rust-toolchain.toml asks, with the wasm target, and Trunk), then builds the site from
# content/ only. Anything already installed is used as is, so it runs locally too.
set -euo pipefail
TRUNK_VERSION="0.21.14"

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi
# the toolchain, target and components rust-toolchain.toml names (older rustups install
# them on the first cargo call instead)
rustup toolchain install 2>/dev/null || true
cargo --version

if ! command -v trunk >/dev/null 2>&1; then
  mkdir -p "$HOME/.cargo/bin"
  curl --proto '=https' --tlsv1.2 -sSfL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" | tar -xz -C "$HOME/.cargo/bin"
  export PATH="$HOME/.cargo/bin:$PATH"
fi
trunk --version

trunk build --release
