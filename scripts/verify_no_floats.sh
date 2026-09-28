#!/usr/bin/env bash
# Proves the compiled contract contains no floating-point instructions.
#
# The claim "no floats anywhere" is the core of the project, and a code review is a weak
# way to back it. This reads the actual artifact instead: it validates the module and
# disassembles it, and every float-typed value in WebAssembly spells `f32` or `f64` in the
# text form, so a single grep settles it.
set -euo pipefail
cd "$(dirname "$0")/.."

WASM=${1:-}
if [ -z "$WASM" ]; then
  # Ask for the lib target by name: the bin in this package is a 368-byte constructor
  # probe that shares its output filename, and whichever build finished last owns the path.
  (cd crates/arbcontract && cargo build --release --target wasm32-unknown-unknown --lib)
  WASM=crates/arbcontract/target/wasm32-unknown-unknown/release/arbcontract.wasm
fi
if [ ! -f "$WASM" ]; then
  echo "not built yet: (cd crates/arbcontract && cargo build --release --target wasm32-unknown-unknown --lib)" >&2
  exit 1
fi
if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "wasm-tools not on PATH (cargo install wasm-tools)" >&2
  exit 1
fi

echo "module:   $WASM"
echo "size:     $(wc -c < "$WASM") bytes"
echo "validate: $(wasm-tools validate "$WASM" && echo ok)"

wat=$(mktemp)
trap 'rm -f "$wat"' EXIT
wasm-tools print "$WASM" > "$wat"

# A module with no user_entrypoint export is not a deployed Stylus contract, and checking
# floats in it would prove nothing about the one that is.
if ! grep -q '(export "user_entrypoint"' "$wat"; then
  echo "FAIL: $WASM does not export user_entrypoint, so it is not the contract program" >&2
  exit 1
fi

hits=$(grep -cE '\b(f32|f64)\b' "$wat" || true)
echo "float-typed mentions: $hits"
if [ "$hits" -ne 0 ]; then
  grep -nE '\b(f32|f64)\b' "$wat" | head -20
  echo "FAIL: the artifact uses floating point" >&2
  exit 1
fi
echo "PASS: no floating point anywhere in the module"