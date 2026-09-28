# Deploying

The contract builds, validates and measures on this machine, and has been checked against
Arbitrum Sepolia with the official tooling. Two things needed fixing along the way, and
both of them will be waiting for anyone else who tries this on Windows.

## Verified state

```
cargo stylus check --endpoint https://sepolia-rollup.arbitrum.io/rpc

contract size: 14.8 KB (14825 bytes)
wasm data fee: 0.000108 ETH (originally 0.000090 ETH with 20% bump)
```

No errors. Compressed size against a 96 KB limit (24 KB before ArbOS Elara), so the
contract is deployable on Sepolia as it stands. The fee is the data cost of the
deployment itself, not including activation gas.

Note the official number is *smaller* than `scripts/size.sh` reports (18.6 KB): the CLI
runs `wasm-opt` before compressing, and reports the optimized size. Treat 14,825 bytes as
the authoritative figure and the script's number as the conservative one.

## Three things the CLI requires, in the order it asks for them

**1. `cargo install cargo-stylus` does not build on Windows.**

```
error[E0433]: cannot find `unix` in `os`
  --> cargo-stylus-0.10.9/src/commands/debug_hook.rs:12:9
   |
12 |     os::unix::net::{UnixListener, UnixStream},
```

The published source imports `std::os::unix::net` unconditionally, so the whole CLI fails
to compile — even though the only thing that touches Unix sockets is
`StylusDebuggerHook`, reachable solely from `cargo stylus replay --debugger stylusdb`, and
the file carries a TODO saying Windows is not a target for stylusdb debugging. There is no
prebuilt Windows binary either (the newest GitHub release is v0.6.3 with no assets).

`scripts/patches/cargo-stylus-windows.patch` gates that one struct and its three impl
blocks with `#[cfg(unix)]` and compiles out the single call site in `replay.rs`. Nothing
else changes.

```bash
mkdir -p tools && cp -r ~/.cargo/registry/src/index.crates.io-*/cargo-stylus-0.10.9 \
  tools/cargo-stylus-src
(cd tools/cargo-stylus-src && patch -p1 < ../../scripts/patches/cargo-stylus-windows.patch)
cargo install --path tools/cargo-stylus-src --root tools/cargo-stylus
export PATH="$PWD/tools/cargo-stylus/bin:$PATH"
cargo stylus --version        # stylus 0.10.9
```

`tools/` is gitignored: the patch is committed, the vendored source is not. The root
`Cargo.toml` also excludes `tools/cargo-stylus-src`, or cargo will try to treat it as a
workspace member.

**2. `Stylus.toml` must exist, and it has two different schemas.**

The error is just `missing Stylus.toml`, with no hint about shape, and the two shapes are
not interchangeable:

```toml
# workspace-level, loaded from the workspace root
[workspace.networks.arbitrum-sepolia]
endpoint = "https://sepolia-rollup.arbitrum.io/rpc"
```

```toml
# contract-level, written BY the CLI after a deploy (not by hand)
[contract.deployments.<name>]
network = "arbitrum-sepolia"
no_activate = false
deployer_address = "0x..."
```

`arbcontract` is its own workspace root, so it needs the **workspace** form —
`crates/arbcontract/Stylus.toml`. The named networks are what make `--network` work
instead of passing an endpoint every time. A `[contract]` table in that file makes the
parse fail with `missing field 'networks'`, because it is being read as the other schema.

**3. `rust-toolchain.toml` with a pinned channel.**

```
expected to find a rust-toolchain.toml file ... The channel ... must be a specific version
e.g., '1.80.0' ... it cannot be a generic channel like 'stable'
```

Verification has to be reproducible, so `crates/arbcontract/rust-toolchain.toml` pins
`1.97.1`. A generic `stable` is rejected.

**4. `check` needs an endpoint to complete.** Without one it tries `http://localhost:8547`
(the local devnode) and dies with a connection refused. `--endpoint` against a public RPC
is enough and needs no key:

```bash
cargo stylus check --endpoint https://sepolia-rollup.arbitrum.io/rpc
```

## Deploying

```bash
export PATH="$PWD/tools/cargo-stylus/bin:$PATH"
cd crates/arbcontract
cargo stylus deploy --network arbitrum-sepolia --private-key "$ARB_DEPLOYER_KEY"
```

`deploy` builds, checks, deploys and activates in one go, then writes the resulting address
into `Stylus.toml`.

**Keys.** Pass the key through an environment variable as above and never commit it. Use a
throwaway deployer account holding only testnet ETH. Faucets for Arbitrum Sepolia are in
the Buildathon resources tab: `arbitrum.faucet.dev` for ETH, `faucet.circle.com` for
testnet USDC.

## After it is deployed

```bash
cast call <ADDRESS> "scale()(int128)" --rpc-url https://sepolia-rollup.arbitrum.io/rpc

cast call <ADDRESS> \
  "priceEuropean(int128,int128,int128,int128,int128,int128,bool)(int128)" \
  250000000000 240000000000 250000000 350000000 50000000 10000000 false \
  --rpc-url https://sepolia-rollup.arbitrum.io/rpc
```

`scale()` returns 1000000000, and the price call returns the fixed-point value of
Black-Scholes for 250 spot, 240 strike, a quarter year, 35% vol, 5% rate, 1% carry.

**Two maintenance facts that bite people:**

1. A Stylus program must be **re-activated** every 365 days or after a Stylus upgrade, or
   calls to it start failing. `cargo stylus activate` does it; `stylus-tools` also has a
   `codehash_keepalive` operation for doing it unattended.
2. **Robinhood Chain testnet has not been verified to run Stylus.** It is an Arbitrum Orbit
   chain, and Orbit chains only have Stylus if they have taken the upgrade. Confirm before
   promising a deployment there. Arbitrum Sepolia is confirmed working above.

## What is still missing

The frontend and the demo video, both of which want a deployed address. The honest order
is: deploy, then build the page that reads a real feed and quotes on-chain, then record it.
`evm/` already holds the Solidity side of that story — a consumer contract and its tests —
so the page has something real to call.