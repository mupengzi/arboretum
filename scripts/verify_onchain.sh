#!/usr/bin/env bash
# Cross-checks the deployed contract against this machine's host build, call for call.
#
# The entire claim of the project is that these two agree bit for bit: an off-chain model
# feeding a price through an oracle cannot make that statement, and a deterministic
# integer program can. If this script ever prints FAIL, the claim is false.
#
#   ARBORETUM_ADDRESS=0x... scripts/verify_onchain.sh
set -euo pipefail
cd "$(dirname "$0")/.."

ADDRESS=${ARBORETUM_ADDRESS:-0x374f469725d735115b8b15dee3f8749ff929d94a}
RPC=${ARBORETUM_RPC:-https://sepolia-rollup.arbitrum.io/rpc}

# 250 spot, 240 strike, a quarter year, 35% vol, 5% rate, 1% carry.
S=250000000000; K=240000000000; T=250000000; V=350000000; R=50000000; Q=10000000

echo "contract $ADDRESS"
echo "rpc      $RPC"
echo

host=$(cargo run -q --release -p arbreport --example onchain_cases)

fails=0
check() {
  local name=$1; shift
  local want got
  want=$(printf '%s\n' "$host" | awk -v n="$name" '$1 == n { print $2 }')
  got=$(cast call "$ADDRESS" "$@" --rpc-url "$RPC" | awk '{ print $1 }')
  if [ -z "$want" ]; then
    printf 'SKIP  %-28s host build produced nothing\n' "$name"
    return
  fi
  if [ "$want" = "$got" ]; then
    printf 'PASS  %-28s %s\n' "$name" "$got"
  else
    printf 'FAIL  %-28s chain=%s host=%s\n' "$name" "$got" "$want"
    fails=$((fails + 1))
  fi
}

check priceEuropean_call "priceEuropean(int128,int128,int128,int128,int128,int128,bool)(int128)" $S $K $T $V $R $Q false
check priceEuropean_put  "priceEuropean(int128,int128,int128,int128,int128,int128,bool)(int128)" $S $K $T $V $R $Q true
check priceLattice_euro_call_512 "priceLattice(int128,int128,int128,int128,int128,int128,bool,bool,uint32)(int128)" $S $K $T $V $R $Q false false 512
check priceLattice_amer_put_512  "priceLattice(int128,int128,int128,int128,int128,int128,bool,bool,uint32)(int128)" $S $K $T $V $R $Q true true 512
check delta_call     "delta(int128,int128,int128,int128,int128,int128,bool)(int128)" $S $K $T $V $R $Q false
check vega_call      "vega(int128,int128,int128,int128,int128,int128,bool)(int128)" $S $K $T $V $R $Q false
check noiseBand      "noiseBand(int128,int128,int128,int128,int128,int128)(int128)" $S $K $T $V $R $Q
check lowerBound_call "lowerBound(int128,int128,int128,int128,int128,int128,bool)(int128)" $S $K $T $V $R $Q false
check upperBound_call "upperBound(int128,int128,int128,int128,int128,int128,bool)(int128)" $S $K $T $V $R $Q false
check impliedVol_from_23.8 "impliedVol(int128,int128,int128,int128,int128,int128,bool)(int128)" 23800000000 $S $K $T $R $Q false

echo
if [ "$fails" -eq 0 ]; then
  echo "all cases agree: the deployed contract and this host build are bit-identical"
else
  echo "$fails case(s) disagree" >&2
  exit 1
fi