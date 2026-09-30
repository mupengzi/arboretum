# Deploying

**Deployed and verified on Arbitrum Sepolia.**

```
address            0x374f469725d735115b8b15dee3f8749ff929d94a
deploy tx          0xeae8c5dd5bc09d8b3d866ad7eaedd86fd827d418a63c894b63245a39dd5ee037
activation tx      a562c36deb7fd9eebc64e60cfc0ed2e29c1b22c5e46a4c38d851c8080e97d7be
contract size      14.7 KB (14668 bytes) against a 96 KB limit
wasm data fee      0.000108 ETH
```

`scripts/verify_onchain.sh` calls every entry point on that address and compares each result
against a local build of the same source. All ten cases agree **bit for bit** — which is the
entire claim of the project, stated as a command anyone can run:

```
all cases agree: the deployed contract and this host build are bit-identical
```

Reproduce it with `ARBORETUM_ADDRESS=0x374f... scripts/verify_onchain.sh`.

## Six things the tooling requires, in the order it asks for them

The list is long and every item cost time to find, so it is written down in full.

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
`crates/arbcontract/Stylus.toml`. A `[contract]` table in that file makes the parse fail
with `missing field 'networks'`, because it is being read as the other schema. The named
networks have to be *present* to satisfy the schema, but 0.10.9's `check` and `deploy` do
not read them.

**3. `rust-toolchain.toml` with a pinned channel.**

```
expected to find a rust-toolchain.toml file ... The channel ... must be a specific version
e.g., '1.80.0' ... it cannot be a generic channel like 'stable'
```

Verification has to be reproducible, so `crates/arbcontract/rust-toolchain.toml` pins
`1.97.1`.

**4. The pinned toolchain needs the wasm target separately.**

rustup treats `1.97.1` as a different toolchain from `stable` even when they are the same
version, and it installs a fresh copy without any targets. The first deploy appeared to
work because the build was cached; the first source change surfaced it:

```
error[E0463]: can't find crate for `std`
  = note: the `wasm32-unknown-unknown` target may not be installed
```

```bash
rustup target add wasm32-unknown-unknown --toolchain 1.97.1
```

**5. Deployment needs a bin target for the constructor probe, and a gas ceiling.**

`deploy` runs the crate with `export-abi` to read the constructor signature, which needs a
bin target — hence `crates/arbcontract/src/main.rs`. And the default fee estimate can land
under the base fee on Sepolia:

```
max fee per gas less than block base fee: maxFeePerGas: 31828000 baseFee: 32042000
```

`--max-fee-per-gas-gwei 1` fixes it and still costs a fraction of a cent.

**6. Reproducible builds need WSL on Windows.**

```
error: Reproducible cargo stylus commands on Windows are only supported in Windows Linux
Subsystem (WSL). Please install within WSL. To instead opt out of reproducible builds, add
the --no-verify flag to your commands.
```

The deployment above used `--no-verify`, which deploys the locally built WASM and skips the
Docker-based reproducible build. The consequence is honest and worth stating: the on-chain
codehash cannot be re-derived through `cargo stylus verify` afterwards. The artifact is
still fully public — the source, `Cargo.lock` and the toolchain are all pinned — but
reproducing it requires a Linux environment. Running the deploy inside WSL removes this
caveat.

So the full command that worked:

```bash
export PATH="$PWD/tools/cargo-stylus/bin:$PATH"
cd crates/arbcontract
cargo stylus deploy \
  -e https://sepolia-rollup.arbitrum.io/rpc \
  --private-key "$ARB_DEPLOYER_KEY" \
  --no-verify --max-fee-per-gas-gwei 1
```

`deploy` builds, checks, deploys and activates in one go. `check` and `deploy` take
`-e/--endpoint`; neither accepts `--network` in 0.10.9. `--estimate-gas` prices a
deployment without broadcasting it.

**Keys.** Pass the key through an environment variable and never commit it. Use a throwaway
account holding only testnet ETH. Faucets: `arbitrum.faucet.dev` for ETH,
`faucet.circle.com` for testnet USDC.

## After it is deployed

```bash
cast call <ADDRESS> "scale()(int128)" --rpc-url https://sepolia-rollup.arbitrum.io/rpc

cast call <ADDRESS> \
  "priceEuropean(int128,int128,int128,int128,int128,int128,bool)(int128)" \
  250000000000 240000000000 250000000 350000000 50000000 10000000 false \
  --rpc-url https://sepolia-rollup.arbitrum.io/rpc
```

`scale()` returns 1000000000, and the price call returns 23843783735 — 23.843783735, which
is what a local build of the same source produces.

The CLI also recommends caching the program in ArbOS, which makes calls cheaper:

```bash
cargo stylus cache bid 0x374f469725d735115b8b15dee3f8749ff929d94a 0
```

Not done here, because it is another transaction and the cost of calls is not yet the
constraint.

**Two maintenance facts that bite people:**

1. A Stylus program must be **re-activated** every 365 days or after a Stylus upgrade, or
   calls to it start failing. `cargo stylus activate` does it; `stylus-tools` also has a
   `codehash_keepalive` operation for doing it unattended.
2. **Orbit chains only have Stylus if they have taken the upgrade**, so anything other than
   Arbitrum Sepolia or One has to be confirmed before promising a deployment there. Robinhood
   Chain testnet was measured here rather than assumed: chain id `46630`, RPC
   `https://rpc.testnet.chain.robinhood.com`, and `cargo stylus check` against it passes,
   reporting the same 14,707-byte program and the same project metadata hash as Arbitrum
   Sepolia and Arbitrum One. Its own developer docs say only "fully EVM-compatible" and never
   mention Stylus, which is exactly why this needed measuring instead of reading.

## What is still open

1. **The deployed program was not built through the reproducible path.** It went out with
   `--no-verify` because the Docker image could not be pulled here, so `cargo stylus verify`
   cannot re-derive that codehash. The source, `Cargo.lock` and the pinned toolchain are all
   public, so a rebuild is possible, just not in a container nobody else can pin to.
2. **The compiled artifact embeds build paths.** Rust writes panic locations into the binary,
   and Stylus's stripping only removes user custom sections, not those strings, so a program
   built at `/home/name/project` and one built at `C:\Users\name\Desktop\project` differ.
   That is why the same source measures a slightly different size on a different machine, and
   why a codehash is only reproducible from a fixed path. `--remap-path-prefix` removes the
   local layout from the artifact; the Docker path does it by construction.
3. **No mainnet deployment, no audit, no external consumer.**
4. **The signed or Merkle-committed volatility input is designed but not built.**