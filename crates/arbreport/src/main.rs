//! Host-only accuracy harness.
//!
//! This crate is **never compiled to WASM**. It is the one place floating point is
//! allowed, and only because the reference numbers it compares against arrive from
//! Python as `f64`. Nothing it contains can reach a validator.
//!
//! Run `py reference/gen_vectors.py` to refresh `src/vectors.rs`, then
//! `cargo run -p arbreport > docs/ACCURACY.md` to regenerate the report. The budget
//! constants below are asserted by the test in this file, so the report cannot drift
//! away from what the code actually enforces.

mod vectors;

use arbnum::{erf, norm_cdf, norm_pdf, D};
use arbpricing::{binomial, european, greeks, implied_vol, price_noise_band, Kind, Market};

const SCALE_F: f64 = 1_000_000_000.0;

fn d(raw: i128) -> D {
    D::from_raw(raw)
}

fn as_f64(v: D) -> f64 {
    v.raw() as f64 / SCALE_F
}

/// Relative budget for an option price, in units of the quote currency.
pub const PRICE_REL_BUDGET: f64 = 1e-6;
/// Absolute floor, because a relative budget against a near-zero price is meaningless.
pub const PRICE_ABS_BUDGET: f64 = 1e-6;
/// Budget for the elementary functions, which feed every price downstream.
pub const MATH_REL_BUDGET: f64 = 1e-7;
/// The Abramowitz & Stegun form used for the normal CDF publishes an absolute error
/// bound of 7.5e-8; anything looser than that would mean a transcription mistake.
pub const CDF_ABS_BUDGET: f64 = 1e-7;
/// Above |x| = 1, `erf` is composed as `2*Phi(x*sqrt2) - 1`, which doubles the CDF's
/// absolute error before it is reported.
pub const ERF_ABS_BUDGET: f64 = 1.6e-7;
/// Lattices accumulate rounding over O(steps^2) nodes, so they get a wider band.
pub const LATTICE_REL_BUDGET: f64 = 1e-5;
/// Implied volatility is a solved quantity, quoted on the sigma axis.
pub const IV_REL_BUDGET: f64 = 1e-5;

#[derive(Default, Debug)]
pub struct Report {
    pub cases: usize,
    pub violations: usize,
    pub rejected: usize,
    pub max_abs: f64,
    pub max_rel: f64,
    pub worst: String,
    pub worst_rejection: String,
}

impl Report {
    fn check(&mut self, got: f64, want: f64, rel: f64, abs: f64, label: String) {
        self.cases += 1;
        let err = (got - want).abs();
        let rel_err = if want.abs() > 1e-15 { err / want.abs() } else { err };
        if err > self.max_abs {
            self.max_abs = err;
        }
        if rel_err > self.max_rel {
            self.max_rel = rel_err;
        }
        if err > (want.abs() * rel).max(abs) {
            self.violations += 1;
            if self.worst.is_empty() {
                self.worst = format!("{label}: got {got}, reference {want}, err {err:e}");
            }
        }
    }

    fn rejected(&mut self, label: String, why: String) {
        self.rejected += 1;
        if self.worst_rejection.is_empty() {
            self.worst_rejection = format!("{label} -> {why}");
        }
    }
}

fn market_of(spot: i128, strike: i128, t: i128, sigma: i128, rate: i128, carry: i128) -> Market {
    Market { spot: d(spot), strike: d(strike), t: d(t), sigma: d(sigma), rate: d(rate), carry: d(carry) }
}

/// Re-runs every reference group and returns one [`Report`] per group.
pub fn evaluate() -> Vec<(&'static str, &'static str, Report)> {
    let mut out: Vec<(&'static str, &'static str, Report)> = Vec::new();

    // --- elementary functions -----------------------------------------------------
    let mut r = Report::default();
    for c in vectors::EXP {
        match d(c.x).exp() {
            Ok(v) => r.check(as_f64(v), c.want, MATH_REL_BUDGET, 1e-9, format!("exp({})", as_f64(d(c.x)))),
            Err(e) => r.rejected(format!("exp({})", as_f64(d(c.x))), e.to_string()),
        }
    }
    out.push(("exp(x)", "Taylor series after reduction by k*ln2", r));

    let mut r = Report::default();
    for c in vectors::LN {
        match d(c.x).ln() {
            Ok(v) => r.check(as_f64(v), c.want, MATH_REL_BUDGET, 1e-9, format!("ln({})", as_f64(d(c.x)))),
            Err(e) => r.rejected(format!("ln({})", as_f64(d(c.x))), e.to_string()),
        }
    }
    out.push(("ln(x)", "power-of-two split plus atanh series", r));

    let mut r = Report::default();
    for c in vectors::SQRT {
        match d(c.x).sqrt() {
            Ok(v) => r.check(as_f64(v), c.want, MATH_REL_BUDGET, 1e-9, format!("sqrt({})", as_f64(d(c.x)))),
            Err(e) => r.rejected(format!("sqrt({})", as_f64(d(c.x))), e.to_string()),
        }
    }
    out.push(("sqrt(x)", "exact integer square root of the scaled value", r));

    let mut r = Report::default();
    for c in vectors::ERF {
        match erf(d(c.x)) {
            Ok(v) => r.check(as_f64(v), c.want, MATH_REL_BUDGET, ERF_ABS_BUDGET, format!("erf({})", as_f64(d(c.x)))),
            Err(e) => r.rejected(format!("erf({})", as_f64(d(c.x))), e.to_string()),
        }
    }
    out.push(("erf(x)", "Taylor below 1, normal CDF above", r));

    let mut r = Report::default();
    for c in vectors::NORM_CDF {
        match norm_cdf(d(c.x)) {
            Ok(v) => r.check(as_f64(v), c.want, 0.0, CDF_ABS_BUDGET, format!("Phi({})", as_f64(d(c.x)))),
            Err(e) => r.rejected(format!("Phi({})", as_f64(d(c.x))), e.to_string()),
        }
    }
    out.push(("Phi(x)", "Abramowitz & Stegun 26.2.17 rational form", r));

    // --- prices -------------------------------------------------------------------
    let mut r = Report::default();
    for c in vectors::BS {
        let m = market_of(c.spot, c.strike, c.t, c.sigma, c.rate, c.carry);
        for (kind, want) in [(Kind::Call, c.call), (Kind::Put, c.put)] {
            match european(&m, kind) {
                Ok(v) => {
                    let label = format!("{} S={} K={} T={} sig={} r={} q={}", if kind == Kind::Call { "call" } else { "put" }, as_f64(m.spot), as_f64(m.strike), as_f64(m.t), as_f64(m.sigma), as_f64(m.rate), as_f64(m.carry));
                    // The floor is the derived noise band, not a constant: near total
                    // cancellation the honest error bar is larger than the premium.
                    let floor = PRICE_ABS_BUDGET.max(as_f64(price_noise_band(&m).unwrap()));
                    r.check(as_f64(v), want, PRICE_REL_BUDGET, floor, label);
                }
                Err(e) => r.rejected(format!("{kind:?} S={} K={} T={} sig={}", as_f64(m.spot), as_f64(m.strike), as_f64(m.t), as_f64(m.sigma)), e.to_string()),
            }
        }
    }
    out.push(("Black-Scholes", "closed form with continuous dividends", r));

    let mut r = Report::default();
    for c in vectors::LATTICE {
        let m = market_of(c.spot, c.strike, c.t, c.sigma, c.rate, c.carry);
        let kind = if c.is_put { Kind::Put } else { Kind::Call };
        match binomial(&m, kind, c.steps, c.american) {
            Ok(v) => r.check(as_f64(v), c.want, LATTICE_REL_BUDGET, PRICE_ABS_BUDGET, format!("crr n={} {} S={} K={} sig={}", c.steps, if c.american { "american" } else { "european" }, as_f64(m.spot), as_f64(m.strike), as_f64(m.sigma))),
            Err(e) => r.rejected(format!("crr n={} sig={}", c.steps, as_f64(m.sigma)), e.to_string()),
        }
    }
    out.push(("CRR lattice", "binomial, European and American", r));

    let mut r = Report::default();
    for c in vectors::IV {
        let m = market_of(c.spot, c.strike, c.t, 500_000_000, c.rate, c.carry);
        let kind = if c.is_put { Kind::Put } else { Kind::Call };
        match implied_vol(d(c.price), m, kind) {
            Ok(v) => {
                // Sigma is only recoverable to (price noise) / vega. Below that the quote
                // carries no more information, so a tighter budget would be fiction.
                let solved = Market { sigma: v, ..m };
                let band = as_f64(price_noise_band(&solved).unwrap());
                let vega = as_f64(greeks(&solved, kind).unwrap().vega).max(1e-9);
                r.check(
                    as_f64(v),
                    c.sigma,
                    IV_REL_BUDGET,
                    (band / vega).max(1e-6),
                    format!("iv S={} K={} T={} price={}", as_f64(m.spot), as_f64(m.strike), as_f64(m.t), as_f64(d(c.price))),
                )
            }
            Err(e) => r.rejected(format!("iv S={} K={} price={}", as_f64(m.spot), as_f64(m.strike), as_f64(d(c.price))), e.to_string()),
        }
    }
    out.push(("implied volatility", "safeguarded Newton on the price residual", r));

    out
}

/// A handful of sanity numbers that belong in the report but have no reference group.
fn extras() -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    v.push(("norm_pdf(0)", format!("{:.9}", as_f64(norm_pdf(D::ZERO).unwrap()))));
    v.push(("quantum", format!("{:.1}e-9 (the representation's smallest step)", 1.0)));
    v
}

fn main() {
    let reports = evaluate();
    let total: usize = reports.iter().map(|(_, _, r)| r.cases).sum();
    let violations: usize = reports.iter().map(|(_, _, r)| r.violations).sum();
    let rejected: usize = reports.iter().map(|(_, _, r)| r.rejected).sum();

    println!("# Accuracy report");
    println!();
    println!("Generated by `cargo run -p arbreport`. Reference numbers come from CPython's");
    println!("`math` module via `reference/gen_vectors.py`; every reference is evaluated at");
    println!("the *quantised* input, so what is measured here is approximation error, not the");
    println!("rounding of the inputs.");
    println!();
    println!("Engine: signed fixed point at 1e9 (`i128`), no floating point, no third-party");
    println!("math crates. Comparisons use doubles only on this host side.");
    println!();
    println!("| group | method | cases | max abs err | max rel err | over budget | engine rejected |");
    println!("|---|---|---:|---:|---:|---:|---:|");
    for (name, method, r) in &reports {
        println!(
            "| {name} | {method} | {} | {:.3e} | {:.3e} | {} | {} |",
            r.cases, r.max_abs, r.max_rel, r.violations, r.rejected
        );
    }
    println!();
    println!("**Totals:** {total} cases, {violations} over budget, {rejected} rejected by the engine.");
    println!();
    println!("## Budgets (asserted by `cargo test -p arbreport`)");
    println!();
    println!("| quantity | budget |");
    println!("|---|---|");
    println!("| elementary functions | rel {MATH_REL_BUDGET:e} |");
    println!("| normal CDF | abs {CDF_ABS_BUDGET:e} (published bound 7.5e-8) |");
    println!("| option price | rel {PRICE_REL_BUDGET:e}, floor {PRICE_ABS_BUDGET:e} |");
    println!("| lattice | rel {LATTICE_REL_BUDGET:e} |");
    println!("| implied volatility | rel {IV_REL_BUDGET:e} |");
    println!();
    for (name, _, r) in &reports {
        if r.violations > 0 {
            println!("- `{name}` worst case: {}", r.worst);
        }
        if r.rejected > 0 {
            println!("- `{name}` first rejection: {}", r.worst_rejection);
        }
    }
    println!();
    println!("## Interpretation");
    println!();
    println!("A 1e-6 relative budget on an option price is a hundredth of a basis point on a");
    println!("$250 premium. The binding constraint is the normal CDF's 7.5e-8 absolute error,");
    println!("which propagates to roughly `spot * 7.5e-8`, or 1.9e-5 on a $250 underlying:");
    println!("two thousandths of a basis point.");
    println!();
    for (label, value) in extras() {
        println!("- {label} = {value}");
    }
    println!();
    println!("## Known limits this report does not cover");
    println!();
    println!("- Deep out-of-the-money premiums below ~1e-9 flush to zero; the two terms of");
    println!("  the closed form cancel into representation noise there. Monotonicity in");
    println!("  volatility is asserted only up to that noise band (see the lattice tests).");
    println!("- No jump-diffusion, stochastic-volatility or barrier models: this is the");
    println!("  Black-Scholes and CRR family, priced exactly, not a general model suite.");
    println!("- Volatility is an input. There is no live implied-volatility surface for");
    println!("  tokenised equities to read, and pretending otherwise would be the honest");
    println!("  thing to call out in a review.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reference_group_is_within_budget() {
        let mut problems = Vec::new();
        let mut total = 0;
        for (name, _, r) in evaluate() {
            total += r.cases;
            if r.violations > 0 {
                problems.push(format!("{name}: {} of {} over budget; {}", r.violations, r.cases, r.worst));
            }
            if r.rejected > 0 {
                problems.push(format!("{name}: {} cases the engine rejected; {}", r.rejected, r.worst_rejection));
            }
        }
        assert!(total > 1000, "expected a large reference sweep, only saw {total} cases");
        assert!(problems.is_empty(), "accuracy regressions:\n{}", problems.join("\n"));
    }
}
