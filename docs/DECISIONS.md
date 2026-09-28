# Decision log

Why the code looks the way it does, what the alternative was, and what the choice costs.
Written for whoever reads this next, including me in three weeks.

## 1. Signed fixed point at 1e9 in `i128`

**Chosen** over `f64`, over 1e18 scaling, and over a decimal crate.

`f64` is not an option: Stylus lists floating point as unsupported. 1e18 scaling is the
DeFi house style, but a binomial lattice's node values span `S·u^N`, and at depth 1024 that
is around `1e12`; multiplying two such values needs a 512-bit intermediate and a
hand-written `muldiv`, which is a large surface to get wrong. `rust_decimal` and `fixed`
would work but add dependency weight against a code-size limit and an audit surface I would
rather control.

**Cost:** the representation's quantum is 1e-9. Premiums below that flush to zero, which is
documented behaviour rather than an accident.

## 2. Round half up, everywhere

`floor(num/den)`, plus one when the remainder is at least half the divisor. Euclidean
division makes that total and well defined for negative inputs too.

**Alternatives:** truncation toward zero biases every alternating series; banker's rounding
adds branches and fixes nothing that matters here.

**Cost:** a half-quantum upward bias per operation, which is why the lattice needed
decision 4.

## 3. Zero third-party dependencies

Nothing from crates.io, not even for the elementary functions.

**Why:** two reasons that happen to agree. `exp`, `ln`, `sqrt`, `pow`, `erf`, `norm_cdf`
and `norm_pdf` are the parts a reviewer needs to trust, and they are more auditable as a
few hundred lines in this repository than as a transitive tree. And the Stylus size limit
punishes pulling in a general-purpose math crate for six functions.

**Cost:** the numerics are my responsibility, and the accuracy report exists because of
that.

## 4. The lattice's blend is fused into one rounding

`disc·(p·V[j+1] + (1-p)·V[j])` was five roundings per node. Precomputing `wa = disc·p` and
`wb = disc·(1-p)` and evaluating `(wa·hi + wb·lo) / 1e9` in one step makes it one.

**Why:** a lattice has about `N²/2` nodes. At depth 1024 that is half a million half-quantum
biases, and they do not cancel.

**Cost:** an intermediate that can exceed `i128`, so there is a checked fallback path to the
two-step form.

## 5. Each node's underlying is rebuilt per level, not carried forward

This one shipped broken for a day. Carrying the underlying forward by multiplying by `u`
per level is correct arithmetic and wrong in fixed point: an entry that underflows at the
terminal row stays pinned at exactly zero for the whole fold, while its true value grows by
`u` per level and equals the spot by the time it reaches the root. The American
early-exercise test then compared `K - 0 = K` against the continuation value, exercised, and
priced a deep in-the-money put five percent too high — above anything attainable.

Rebuilding from the diagonal (`S·u^level`, always representable) downward by `d²` per step
keeps the property that underflow only happens where the true underlying really is below
one quantum, and where `K - S` is `K` to within 1e-9 anyway.

**Why it was hard to see:** the European path never reads the lattice, so it agreed with the
reference to 1e-8 while the American path was wrong. Pinned by a regression test.

**Cost:** roughly double the work on the American path.

## 6. Terminal payoffs are built from the top of the tree downward

The top node is `S·u^N`, and each step left multiplies by `d²`. Anchoring at `j = 0`
instead starts from `S·d^N`, which at depth 1024 is 1e-11 — thirteen quanta, carrying three
percent relative error into every node in the row.

## 7. The cancellation band is derived, not chosen

A price is the difference of two positive terms and each carries the normal CDF's absolute
error, so the uncertainty scales with the *terms*, not with the answer. The band is
`(spot_df + strike_df) / 12_500_000`, i.e. the CDF's published 7.5e-8 propagated. Inside it
a negative result floors to zero; outside it the call reports `Inconsistent`, because a
large negative price means a real invariant broke.

## 8. Implied volatility converges on two tests, not one

The price residual must be within a few quanta — **or** the volatility bracket must have
narrowed to 1e-7, whichever comes first. Sigma itself resolves only to 1e-9, so the
achievable price granularity near the solution is about `vega·1e-9`, which for a normal
equity option is ten times coarser than the price test alone. With only the price test,
reachable quotes were reported as unconverged.

## 9. Lattice depth is capped, and refuses to allocate beyond it

4096 steps. A pricing call must not be a denial-of-service vector, and the failure is a
revert with a reason rather than an allocation that grows with caller input.

## 10. `bounds()` is public

The no-arbitrage band a European price must sit inside. It exists because I got this wrong
first: a European put is *not* bounded below by intrinsic value — it cannot be exercised
early, so its floor is the discounted forward difference. The band is also what makes
implied-volatility rejection meaningful rather than a solver timeout.

## 11. Floating point exists in exactly two places

`#[cfg(test)]` code and the host-only `arbreport` binary. `arbnum` exposes no float API to
its dependents at all, so nothing that compiles into the WASM module can reach one by
accident. The `arbreport` crate says so in its own doc comment.

## 12. Accuracy budgets are derived per case and asserted in tests

The price budget is `max(relative, derived noise band)`, and the implied-volatility budget
is `max(relative, noise band / vega)`. The harness reports *and* enforces, so
`docs/ACCURACY.md` cannot drift away from what `cargo test` checks.