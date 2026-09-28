# Thesis, and the answers to the four objections

Written before the code, and kept honest afterwards. This is the file to read if you are
about to argue with the project.

## The claim

A price that a contract acts on is normally an **assertion**: an off-chain model computes
a number, an oracle delivers it, and the chain executes whatever it was told. The number
that decides whether you get liquidated, and what you settle at, comes from something you
can neither inspect nor reproduce.

Moving the computation itself on-chain changes its nature. Given the same inputs —
spot from a named feed, strike and expiry in the transaction, volatility as a committed
input — anyone can re-execute the same program and arrive at the same integer. The price
becomes a **computation** rather than an assertion: it has a proof you can check by
running it.

That is the entire intellectual content of the project, and everything below follows
from taking it seriously.

## Why it was not done before, and why that changed

1. **Gas.** A 2000-step binomial tree is not affordable in Solidity: no usable fixed-point
   math library, loops priced per iteration, and a stack discipline that fights recursion.
   Rust compiled to WASM through Stylus is where loop-heavy integer arithmetic becomes
   cheap — Arbitrum's own gas-optimisation documentation puts loop-heavy computation at
   roughly 50–100× cheaper than the EVM equivalent
   (<https://docs.arbitrum.io/stylus/best-practices/gas-optimization>).
2. **Toolchain.** Before Stylus there was no credible path for this class of numerics on
   an EVM chain.
3. **Demand.** There was no meaningful pool of tokenised equities to price derivatives on.
   Stablecoins and perps did not need an equity-volatility engine.

All three have moved. The third one moved most recently, and it is the one that decides
whether this is a product or a curiosity.

## Who actually needs it

- Tokenised equities trade on Robinhood Chain (an Arbitrum Orbit chain) with real volume,
  and the derivatives available there are perpetuals and lending — **option supply is
  zero**.
- Tokenised-equity market cap moved from roughly $1.7B to $2.3B during 2026, and the
  instruments that come next are precisely the ones that need a price: covered calls,
  structured notes, option-adjusted collateral values.
- Lending markets that accept tokenised equity as collateral need a defensible mark, and
  "the oracle said so" is a weaker answer to a risk committee than "recompute it yourself".

## Where it should stop being on-chain

Verifiable computation costs one to two orders of magnitude more than trusting a feed. It
belongs where the stakes justify it:

- **settlement and expiry prices** — once per market, adversarial, high value;
- **fallback and dispute marks** — when an oracle is stale or contested;
- **structured-product construction** — when a product must be assembled from parts;
- **audit** — when someone needs to reproduce last quarter's number.

Streaming quotes are not that, and no part of the design tries to be a market maker.

## The four objections, with answers

**1. "Pricing is competitive information; the market moved off-chain for a reason."**
True, and it is a reason about *quoting*, not about settlement. A market maker's edge is
the freshness and secrecy of its quotes; nobody's edge is the expiry settlement price,
which must be public, contestable and identical for all holders. The design targets the
second thing.

**2. "Keep only settlement and slow oracles on-chain; do less computation."**
Also largely true, and it is the reason for the scope above rather than a contradiction of
it. The disagreement is only about *which* computations: a once-per-market price on a
large notional is exactly the case where verifiability beats cheapness.

**3. "Deep lattices and Monte Carlo cost too much gas, and precision is a minefield."**
Both are measured rather than waved away. 3336 reference cases against CPython, with
derived budgets, live in `docs/ACCURACY.md`. The lattice is capped at 4096 steps, refuses
to allocate unboundedly, and reverts rather than wrapping. The compressed program is
14.8 KB against a 96 KB limit.

**4. "Who uses a primitive with no order book? No market maker, no revenue."**
This is the strongest objection, and the honest answer is that the primitive is not the
product. The product is the settlement and collateral path that a lending market, a
structured-note issuer, or a tokenised-equity protocol integrates. A primitive earns
nothing on its own; it earns by being the thing three other teams depend on.

## What is deliberately not claimed

- **Not first.** Lyra's first version ran Black-Scholes in-contract. The claim is that a
  complete, neutral, reusable pricing layer in Rust/Stylus on Arbitrum did not exist.
- **Not a model suite.** Black-Scholes and CRR, priced carefully. No jumps, no stochastic
  vol, no barriers.
- **Not an oracle replacement.** The volatility surface is an input; the project is the
  transform, not the data.