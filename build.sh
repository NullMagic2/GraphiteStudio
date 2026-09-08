#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: Cargo was not found in PATH. Install Rust from https://rustup.rs/ and try again." >&2
    exit 1
fi

echo "Checking Graphite Studio..."
cargo check

echo "Building optimized binary..."
cargo build --release

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) OUTPUT="target/release/graphite-studio.exe" ;;
    *) OUTPUT="target/release/graphite-studio" ;;
esac

echo "Done: $OUTPUT"
