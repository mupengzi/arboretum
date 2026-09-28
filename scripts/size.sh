#!/usr/bin/env bash
# Measures the size that matters for Stylus: the raw WASM, and the brotli compression the
# program size is actually limited by. The contract is the real number; the kernel-only
# probe is built too so the cost of the numerics can be told apart from the cost of the
# SDK glue.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "building the kernel probe ..."
cargo build --release --target wasm32-unknown-unknown -p arbwasm
probe=target/wasm32-unknown-unknown/release/arbwasm.wasm

echo "building the contract ..."
(cd crates/arbcontract && cargo build --release --target wasm32-unknown-unknown)
contract=crates/arbcontract/target/wasm32-unknown-unknown/release/arbcontract.wasm

kb() { awk -v b="$1" 'BEGIN { printf "%.1f KB", b / 1024 }'; }

report() {
  local name=$1 file=$2
  if [ ! -f "$file" ]; then
    printf '%-24s not built\n' "$name"
    return
  fi
  printf '%-24s raw %s' "$name" "$(kb "$(wc -c < "$file")")"
  if command -v brotli >/dev/null 2>&1; then
    printf '   brotli -q11 %s' "$(kb "$(brotli -q 11 -c "$file" | wc -c)")"
    printf '   brotli -q9 %s' "$(kb "$(brotli -q 9 -c "$file" | wc -c)")"
  fi
  printf '\n'
}

echo
report "arbcontract (contract)" "$contract"
report "arbwasm (kernel only)" "$probe"
echo "limits                   96 KB (ArbOS Elara, 2026-08-20); 24 KB before it"