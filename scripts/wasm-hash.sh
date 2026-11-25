#!/usr/bin/env bash
# Builds the optimized Aidline contract and prints its SHA-256 and size.
#
# Usage:
#   ./scripts/wasm-hash.sh                 build once and print the report
#   ./scripts/wasm-hash.sh --twice         build twice from clean target dirs and fail if the hashes differ
#   ./scripts/wasm-hash.sh --verify <hash> build once and fail unless the SHA-256 equals <hash>
#
# The build uses --locked, a fresh target directory, no incremental compilation,
# a fixed SOURCE_DATE_EPOCH and remapped source and cargo paths, so the result
# does not depend on where the repository is checked out. A hash only matches
# when the Rust and Stellar CLI versions match too, which is why the report
# prints them.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE=aidline
TARGET=wasm32v1-none

mode=once
expected=""
case "${1:-}" in
  "") ;;
  --twice) mode=twice ;;
  --verify)
    mode=verify
    expected="${2:?usage: wasm-hash.sh --verify <sha256>}"
    ;;
  *)
    echo "usage: wasm-hash.sh [--twice | --verify <sha256>]" >&2
    exit 2
    ;;
esac

for tool in cargo rustc stellar sha256sum; do
  command -v "$tool" >/dev/null || { echo "error: $tool not found in PATH" >&2; exit 1; }
done

# Builds into a throwaway target directory and prints the SHA-256 of the
# optimized wasm. Progress goes to stderr so stdout is only the hash.
build_hash() {
  local target_dir out
  target_dir="$(mktemp -d)"
  out="$target_dir/$PACKAGE.optimized.wasm"

  (
    cd "$ROOT"
    export CARGO_TARGET_DIR="$target_dir"
    export CARGO_INCREMENTAL=0
    export SOURCE_DATE_EPOCH=0
    export RUSTFLAGS="--remap-path-prefix=$ROOT=/src --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
    cargo build --locked --release --target "$TARGET" --package "$PACKAGE" >&2
    stellar contract optimize \
      --wasm "$target_dir/$TARGET/release/$PACKAGE.wasm" \
      --wasm-out "$out" >&2
  )

  sha256sum "$out" | cut -d' ' -f1
  LAST_WASM="$out"
}

report() {
  local hash="$1" size
  size="$(wc -c < "$LAST_WASM" | tr -d ' ')"
  echo "wasm:    $PACKAGE.optimized.wasm"
  echo "sha256:  $hash"
  echo "size:    $size bytes"
  echo "rustc:   $(rustc --version)"
  echo "cargo:   $(cargo --version)"
  echo "stellar: $(stellar --version | head -n 1)"
}

case "$mode" in
  once)
    hash="$(build_hash)"
    report "$hash"
    ;;
  twice)
    first="$(build_hash)"
    second="$(build_hash)"
    report "$second"
    if [ "$first" != "$second" ]; then
      echo "error: two clean builds produced different hashes" >&2
      echo "  build 1: $first" >&2
      echo "  build 2: $second" >&2
      exit 1
    fi
    echo "check:   two clean builds match"
    ;;
  verify)
    hash="$(build_hash)"
    report "$hash"
    if [ "$hash" != "$expected" ]; then
      echo "error: hash mismatch" >&2
      echo "  expected: $expected" >&2
      echo "  built:    $hash" >&2
      exit 1
    fi
    echo "check:   matches the expected hash"
    ;;
esac
