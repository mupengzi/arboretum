#!/usr/bin/env bash
# Regenerates the parity dataset the web app compares the deployed contract against.
#
# The prices in it come from the same Rust code the contract was built from, so a MATCH in
# the browser means the chain and this machine agree. The web app deliberately contains no
# pricing code of its own: a JavaScript reimplementation would be a second, floating-point
# answer to a question the project claims has exactly one.
set -euo pipefail
cd "$(dirname "$0")/.."

out=web/src/data/parity.json
mkdir -p "$(dirname "$out")"
cargo run --release -q -p arbreport --example parity_grid > "$out"
printf 'wrote %s (%s bytes)\n' "$out" "$(wc -c < "$out")"