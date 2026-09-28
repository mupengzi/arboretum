#!/usr/bin/env bash
# Proves the compiled contract contains no floating-point instructions.
#
# The claim "no floats anywhere" is the core of the project, and a code review is a weak
# way to back it. This reads the actual artifact instead: it validates the module and
# disassembles it, and every float-typed value in WebAssembly spells `f32` or `f64` in the
# text form, so a single grep settles it.
set -euo pipefail
cd "$(dirname "$0")/.."

WASM=${1:-crates/arbcontract/target/wasm32-unknown-unknown/release/arbcontract.wasm}
if [ ! -f "$WASM" ]; then
  echo "not built yet: (cd crates/arbcontract && cargo build --release --target wasm32-unknown-unknown)" >&2
  exit 1
fi
if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "wasm-tools not on PATH (cargo install wasm-tools)" >&2
  exit 1
fi

echo "module:   $WASM"
echo "validate: $(wasm-tools validate "$WASM" && echo ok)"

wat=$(mktemp)
trap 'rm -f "$wat"' EXIT
wasm-tools print "$WASM" > "$wat"

hits=$(grep -cE '\b(f32|f64)\b' "$wat" || true)
echo "float-typed mentions: $hits"
if [ "$hits" -ne 0 ]; then
  grep -nE '\b(f32|f64)\b' "$wat" | head -20
  echo "FAIL: the artifact uses floating point" >&2
  exit 1
fi
echo "PASS: no floating point anywhere in the module"