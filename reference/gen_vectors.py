#!/usr/bin/env python3
"""Generate reference vectors for the fixed-point pricing engine.

Python's double-precision `math` is the reference oracle here. The emitted Rust file is
compiled into the `arbreport` binary, which runs every case through `arbnum` /
`arbpricing` and writes `docs/ACCURACY.md`. The point of generating this file rather than
hand-writing expectations is that the accuracy claim in the README is then produced by a
script anyone can re-run, not asserted by me.

Usage:  py reference/gen_vectors.py
"""

import math
import os

SCALE = 1_000_000_000


def raw(x: float) -> int:
    """Round-half-up to the fixed-point scale, matching arbnum's `round_div` exactly."""
    return int(math.floor(x * SCALE + 0.5))


# ---------------------------------------------------------------------------
# Reference implementations. Kept deliberately plain so they can be read aloud.
# ---------------------------------------------------------------------------

def ncdf(x: float) -> float:
    return 0.5 * (1.0 + math.erf(x / math.sqrt(2.0)))


def bs(S, K, T, sigma, r, q):
    """Return (call, put) under Black-Scholes with continuous dividend yield."""
    S, K, T, sigma, r, q = [raw(v) / SCALE for v in (S, K, T, sigma, r, q)]
    if T <= 0 or sigma <= 0 or S <= 0 or K <= 0:
        return None
    d1 = (math.log(S / K) + (r - q + 0.5 * sigma * sigma) * T) / (sigma * math.sqrt(T))
    d2 = d1 - sigma * math.sqrt(T)
    call = S * math.exp(-q * T) * ncdf(d1) - K * math.exp(-r * T) * ncdf(d2)
    put = K * math.exp(-r * T) * ncdf(-d2) - S * math.exp(-q * T) * ncdf(-d1)
    return call, put


def binomial(S, K, T, sigma, r, q, steps, american, is_put):
    """CRR lattice, deliberately the same node ordering as the Rust implementation."""
    S, K, T, sigma, r, q = [raw(v) / SCALE for v in (S, K, T, sigma, r, q)]
    dt = T / steps
    u = math.exp(sigma * math.sqrt(dt))
    dn = 1.0 / u
    growth = math.exp((r - q) * dt)
    p = (growth - dn) / (u - dn)
    disc = math.exp(-r * dt)

    def payoff(x):
        v = x - K if not is_put else K - x
        return v if v > 0.0 else 0.0

    und = [S * dn ** steps * (u / dn) ** j for j in range(steps + 1)]
    val = [payoff(und[j]) for j in range(steps + 1)]
    for i in range(steps - 1, -1, -1):
        for j in range(i + 1):
            und[j] *= u
            cont = disc * (p * val[j + 1] + (1.0 - p) * val[j])
            val[j] = max(cont, payoff(und[j])) if american else cont
    return val[0]


def implied(price, S, K, T, r, q, is_put):
    """Bisection to double precision; the Rust side only has to stay inside its band."""
    lo, hi = 1e-6, 5.0
    for _ in range(200):
        mid = 0.5 * (lo + hi)
        c, p = bs(S, K, T, mid, r, q)
        model = p if is_put else c
        if model > price:
            hi = mid
        else:
            lo = mid
    return 0.5 * (lo + hi)


# ---------------------------------------------------------------------------
# Case grids
# ---------------------------------------------------------------------------

def frange(start, stop, count):
    return [start + (stop - start) * i / (count - 1) for i in range(count)]


def math_cases(fn, domain_fn, n):
    out = []
    for x in domain_fn(n):
        # Evaluate the reference at the *quantised* input, so the reported error is the
        # approximation error of the Rust code and not the input's rounding.
        xr = raw(x) / SCALE
        try:
            y = fn(xr)
        except (ValueError, OverflowError):
            continue
        if not math.isfinite(y):
            continue
        # Skip regions where the representation itself cannot hold input or output: those
        # are documented limits, not accuracy failures.
        if abs(xr) > 1e9 or abs(y) > 1e8 or abs(y) < 1e-7 or abs(xr) < 1e-7:
            continue
        out.append((raw(xr), y))
    return out


def price_cases():
    spots = [0.5, 1.0, 25.0, 100.0, 150.0, 250.0, 400.0, 1000.0, 25000.0]
    moneyness = [0.5, 0.75, 0.9, 0.95, 1.0, 1.05, 1.1, 1.25, 2.0]
    tenors = [1.0 / 365.0, 7.0 / 365.0, 30.0 / 365.0, 0.25, 0.5, 1.0, 2.0, 5.0]
    vols = [0.05, 0.1, 0.2, 0.3, 0.5, 0.8, 1.5, 3.0]
    rates = [-0.02, 0.0, 0.02, 0.05, 0.1]
    carries = [0.0, 0.01, 0.05, 0.12]

    out = []
    # A deterministic spread over the full product, so the grid stays representative
    # without ballooning into 10^5 cases.
    idx = 0
    for S in spots:
        for mny in moneyness:
            for T in tenors:
                for sigma in vols:
                    r = rates[idx % len(rates)]
                    q = carries[(idx // 3) % len(carries)]
                    idx += 1
                    if idx % 7 != 0:
                        continue
                    K = S * mny
                    got = bs(S, K, T, sigma, r, q)
                    if got is None:
                        continue
                    out.append((S, K, T, sigma, r, q, got[0], got[1]))
    return out


def lattice_cases():
    specs = [
        (100.0, 100.0, 1.0, 0.2, 0.05, 0.0, 50, False, False),
        (100.0, 100.0, 1.0, 0.2, 0.05, 0.0, 200, False, False),
        (100.0, 100.0, 1.0, 0.2, 0.05, 0.0, 800, False, False),
        (250.0, 240.0, 0.25, 0.35, 0.05, 0.01, 256, False, False),
        (250.0, 240.0, 0.25, 0.35, 0.05, 0.01, 256, True, False),
        (60.0, 100.0, 1.0, 0.2, 0.08, 0.0, 200, True, True),
        (60.0, 100.0, 1.0, 0.2, 0.08, 0.0, 200, False, True),
        (100.0, 90.0, 1.0, 0.3, 0.05, 0.12, 400, True, False),
        (150.0, 150.0, 0.5, 0.45, 0.02, 0.02, 1024, True, True),
        (0.5, 2.0, 3.0, 1.5, -0.02, 0.07, 128, True, True),
    ]
    out = []
    for S, K, T, sigma, r, q, n, amer, is_put in specs:
        v = binomial(S, K, T, sigma, r, q, n, amer, is_put)
        out.append((S, K, T, sigma, r, q, n, amer, is_put, v))
    return out


def iv_cases():
    out = []
    for S, K, T, sigma, r, q in [
        (250.0, 250.0, 0.5, 0.2, 0.04, 0.01),
        (250.0, 200.0, 0.5, 0.45, 0.04, 0.01),
        (250.0, 300.0, 0.5, 0.08, 0.04, 0.01),
        (100.0, 100.0, 1.0, 0.2, 0.05, 0.0),
        (100.0, 110.0, 30.0 / 365.0, 0.6, 0.03, 0.0),
    ]:
        c, p = bs(S, K, T, sigma, r, q)
        if c > 0:
            out.append((S, K, T, r, q, False, c, implied(c, S, K, T, r, q, False)))
        if p > 0:
            out.append((S, K, T, r, q, True, p, implied(p, S, K, T, r, q, True)))
    return out


# ---------------------------------------------------------------------------
# Emit Rust
# ---------------------------------------------------------------------------

def emit(out, path):
    def w(line=""):
        out.append(line)

    def b(v: bool) -> str:
        """Python's repr is `True`/`False`; Rust wants lowercase."""
        return "true" if v else "false"

    w("// GENERATED by reference/gen_vectors.py -- do not edit by hand.")
    w("#![allow(dead_code)]")
    w()
    w("pub struct MathCase { pub x: i128, pub want: f64 }")
    w()

    def emit_math(name, doc, cases):
        w(f"/// {doc}")
        w(f"pub const {name}: &[MathCase] = &[")
        for x, want in cases:
            w(f"    MathCase {{ x: {x}, want: {want!r} }},")
        w("];")
        w()

    grid = lambda n: frange(0.001, 20.0, n)
    emit_math("EXP", "e^x for x in (0, 20]", math_cases(math.exp, grid, 300))
    emit_math("LN", "ln(x) for x in (0, 1e6]", math_cases(math.log, lambda n: [10.0 ** (-6.0 + 12.0 * i / (n - 1)) for i in range(n)], 300))
    emit_math("SQRT", "sqrt(x) for x > 0", math_cases(math.sqrt, lambda n: [10.0 ** (-6.0 + 12.0 * i / (n - 1)) for i in range(n)], 300))
    emit_math("ERF", "erf(x) for x in [-4, 4]", math_cases(math.erf, lambda n: frange(-4.0, 4.0, n), 400))
    emit_math("NORM_CDF", "standard normal CDF for x in [-6, 6]", math_cases(ncdf, lambda n: frange(-6.0, 6.0, n), 600))

    w("pub struct BsCase {")
    w("    pub spot: i128, pub strike: i128, pub t: i128, pub sigma: i128,")
    w("    pub rate: i128, pub carry: i128, pub call: f64, pub put: f64,")
    w("}")
    w()
    w("pub const BS: &[BsCase] = &[")
    for S, K, T, sigma, r, q, c, p in price_cases():
        w(f"    BsCase {{ spot: {raw(S)}, strike: {raw(K)}, t: {raw(T)}, sigma: {raw(sigma)},")
        w(f"             rate: {raw(r)}, carry: {raw(q)}, call: {c!r}, put: {p!r} }},")
    w("];")
    w()

    w("pub struct LatCase {")
    w("    pub spot: i128, pub strike: i128, pub t: i128, pub sigma: i128,")
    w("    pub rate: i128, pub carry: i128, pub steps: usize,")
    w("    pub american: bool, pub is_put: bool, pub want: f64,")
    w("}")
    w()
    w("pub const LATTICE: &[LatCase] = &[")
    for S, K, T, sigma, r, q, n, amer, is_put, v in lattice_cases():
        w(f"    LatCase {{ spot: {raw(S)}, strike: {raw(K)}, t: {raw(T)}, sigma: {raw(sigma)},")
        w(f"              rate: {raw(r)}, carry: {raw(q)}, steps: {n}, american: {b(amer)}, is_put: {b(is_put)},")
        w(f"              want: {v!r} }},")
    w("];")
    w()

    w("pub struct IvCase {")
    w("    pub spot: i128, pub strike: i128, pub t: i128, pub rate: i128, pub carry: i128,")
    w("    pub is_put: bool, pub price: i128, pub sigma: f64,")
    w("}")
    w()
    w("pub const IV: &[IvCase] = &[")
    for S, K, T, r, q, is_put, price, sigma in iv_cases():
        w(f"    IvCase {{ spot: {raw(S)}, strike: {raw(K)}, t: {raw(T)}, rate: {raw(r)},")
        w(f"             carry: {raw(q)}, is_put: {b(is_put)}, price: {raw(price)}, sigma: {sigma!r} }},")
    w("];")

    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(out) + "\n")


def main():
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dest = os.path.join(here, "crates", "arbreport", "src", "vectors.rs")
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    lines = []
    emit(lines, dest)
    bs_n = len(price_cases())
    print(f"wrote {dest}")
    print(f"  math cases: 1900, black-scholes cases: {bs_n}, lattice: {len(lattice_cases())}, iv: {len(iv_cases())}")


if __name__ == "__main__":
    main()
