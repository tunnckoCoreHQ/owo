#!/usr/bin/env bash
# Sets up a CI machine (Linux or macOS): rustup when it is missing, then the
# Rust toolchain of rust-toolchain.toml. PATH changes go to $BASH_ENV, which
# every later step sources.
#
#   scripts/ci/setup.sh
set -euo pipefail

cd "$(dirname "$0")/../.."

export PATH=$HOME/.cargo/bin:$PATH
echo "export PATH=\"$HOME/.cargo/bin:\$PATH\"" >> "${BASH_ENV:-/dev/null}"

if ! command -v rustup > /dev/null; then
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
fi
rustup toolchain install
rustup show active-toolchain
