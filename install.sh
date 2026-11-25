#!/bin/sh
# From a fresh clone, or after every `git pull`: ./install.sh
# Builds halftone, puts the `halftone` command on your PATH, and opens it.
# The first run walks you through linking your apps (halftone setup).
set -e
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
    echo "halftone is written in Rust and builds with its toolchain (https://rustup.rs)."
    printf "Install Rust now? [y/N] "
    read -r answer
    case "$answer" in
      y* | Y*) curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y ;;
      *) echo "Install Rust from https://rustup.rs, then run ./install.sh again."; exit 1 ;;
    esac
  fi
  . "$HOME/.cargo/env"
fi

echo "Building halftone (about a minute the first time)…"
cargo install --path . --locked --force --quiet
bin="${CARGO_HOME:-$HOME/.cargo}/bin/halftone"
echo "Installed $bin"
exec "$bin" "$@"
