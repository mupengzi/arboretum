//! Deterministic derivatives pricing: Black-Scholes with continuous carry, Greeks,
//! CRR binomial lattices (European and American), and implied volatility inversion.
//!
//! Every operation is integer arithmetic via [`arbnum`], so the same six inputs produce
//! the same price on every validator. That is the property an oracle-fed quote does not
//! have, and it is what this crate exists to provide: see `docs/THESIS.md`.
//!
//! Conventions: `spot` and `strike` share one unit; `t` is years; `sigma` is annualised
//! volatility as a decimal (`0.2` == 20%); `rate` is a continuously compounded rate;
//! `carry` is the continuous dividend / funding yield.

use arbnum::{norm_cdf, norm_pdf, round_div, D, NumError, Result, SCALE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Call,
    Put,
}

impl Kind {
    pub const fn opposite(self) -> Kind {
        match self {
            Kind::Call => Kind::Put,
            Kind::Put => Kind::Call,
        }
    }
}

/// A European option description, all fields fixed-point.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Market {
    pub spot: D,
    pub strike: D,
    /// Time to expiry in years.
    pub t: D,
    /// Annualised volatility, e.g. `0.2` for 20%.
    pub sigma: D,
    /// Continuously compounded risk-free rate.
    pub rate: D,
    /// Continuous dividend / carry yield.
    pub carry: D,
}

impl Market {
    /// Reject inputs that have no price, rather than returning a plausible-looking zero.
    pub fn validate(&self) -> Result<()> {
        if self.spot.is_negative() || self.spot == D::ZERO {
            return Err(NumError::Domain);
        }
        if self.strike.is_negative() || self.strike == D::ZERO {
            return Err(NumError::Domain);
        }
        if self.t.is_negative() || self.t == D::ZERO {
            return Err(NumError::Domain);
        }
        if self.sigma.is_negative() || self.sigma == D::ZERO {
            return Err(NumError::Domain);
        }
        // exp() and a lattice both blow up well before the type does, so bound the
        // inputs explicitly: the failure mode becomes a revert with a reason.
        if self.sigma.raw() > 10 * SCALE || self.t.raw() > 100 * SCALE {
            return Err(NumError::Domain);
        }
        if self.rate.abs().raw() > SCALE || self.carry.abs().raw() > SCALE {
            return Err(NumError::Domain);
        }
        Ok(())
    }

    /// `exp(-x * t)`, the discount factor used throughout.
    pub fn discount(rate: D, t: D) -> Result<D> {
        rate.mul(t)?.neg()?.exp()
    }
}

#[inline]
fn payoff(spot: D, strike: D, kind: Kind) -> Result<D> {
    let intrinsic = match kind {
        Kind::Call => spot.sub(strike)?,
        Kind::Put => strike.sub(spot)?,
    };
    Ok(intrinsic.max(D::ZERO))
}

/// The no-arbitrage band a European price must fall inside.
///
/// The lower bound is the *discounted* forward difference, not plain intrinsic: a
/// European put legitimately trades below `K - S` because it cannot be exercised early.
/// Anything outside this band has no volatility that reproduces it.
pub fn bounds(m: &Market, kind: Kind) -> Result<(D, D)> {
    let spot_df = m.spot.mul(Market::discount(m.carry, m.t)?)?;
    let strike_df = m.strike.mul(Market::discount(m.rate, m.t)?)?;
    match kind {
        Kind::Call => Ok((spot_df.sub(strike_df)?.max(D::ZERO), spot_df)),
        Kind::Put => Ok((strike_df.sub(spot_df)?.max(D::ZERO), strike_df)),
    }
}

/// Absolute error of the normal CDF approximation, as a reciprocal: A&S 26.2.17
/// publishes 7.5e-8, and `1 / 12_500_000` is 8e-8 rounded up with room to spare.
const NOISE_DIVISOR: i128 = 12_500_000;

/// How far a computed price may legitimately miss, derived rather than guessed.
///
/// A European price is the difference of two positive terms and each term carries the
/// CDF's *absolute* error, so the residual uncertainty scales with the terms, not with
/// the answer. Far out of the money the two nearly cancel and this band dwarfs the
/// premium itself.
pub fn price_noise_band(m: &Market) -> Result<D> {
    let spot_df = m.spot.mul(Market::discount(m.carry, m.t)?)?;
    let strike_df = m.strike.mul(Market::discount(m.rate, m.t)?)?;
    noise_from(spot_df, strike_df)
}

fn noise_from(spot_df: D, strike_df: D) -> Result<D> {
    let sum = spot_df.raw().checked_add(strike_df.raw()).ok_or(NumError::Overflow)?;
    Ok(D::from_raw(sum / NOISE_DIVISOR))
}

/// A negative European price is mathematically impossible, so it is always an artifact of
/// cancellation. Inside the derived band that is arithmetic, and zero is the correct
/// answer; outside it an invariant has genuinely been broken and the caller is told.
fn scrub(price: D, band: D) -> Result<D> {
    if price.is_negative() {
        if price.raw() >= -band.raw() {
            return Ok(D::ZERO);
        }
        return Err(NumError::Inconsistent);
    }
    Ok(price)
}

/// `wa * hi + wb * lo` rounded exactly once, where both weights already carry the
/// discount factor. Falls back to the two-step form when the fused intermediate would
/// not fit, which happens only at lattice extremes.
fn blend(wa: i128, wb: i128, hi: D, lo: D) -> Result<D> {
    if let (Some(x), Some(y)) = (wa.checked_mul(hi.raw()), wb.checked_mul(lo.raw())) {
        if let Some(sum) = x.checked_add(y) {
            // Each product carries two 1e9 scalings, so the sum is at 1e18 and one
            // division by SCALE brings it back to the representation's own scale.
            return Ok(D::from_raw(round_div(sum, SCALE)?));
        }
    }
    D::from_raw(wa).mul(hi)?.add(D::from_raw(wb).mul(lo)?)
}

/// The `d1` term of Black-Scholes.
fn d1(m: &Market) -> Result<D> {
    let vol_sqrt_t = m.sigma.mul(m.t.sqrt()?)?;
    if vol_sqrt_t == D::ZERO {
        return Err(NumError::Domain);
    }
    let drift = m.rate.sub(m.carry)?.add(m.sigma.mul(m.sigma)?.div_int(2)?)?.mul(m.t)?;
    m.spot.div(m.strike)?.ln()?.add(drift)?.div(vol_sqrt_t)
}

/// Black-Scholes price with continuous dividends.
pub fn european(m: &Market, kind: Kind) -> Result<D> {
    m.validate()?;
    let u1 = d1(m)?;
    let u2 = u1.sub(m.sigma.mul(m.t.sqrt()?)?)?;
    let spot_df = m.spot.mul(Market::discount(m.carry, m.t)?)?;
    let strike_df = m.strike.mul(Market::discount(m.rate, m.t)?)?;
    let band = noise_from(spot_df, strike_df)?;
    scrub(
        match kind {
            Kind::Call => spot_df.mul(norm_cdf(u1)?)?.sub(strike_df.mul(norm_cdf(u2)?)?)?,
            Kind::Put => strike_df.mul(norm_cdf(u2.neg()?)?)?.sub(spot_df.mul(norm_cdf(u1.neg()?)?)?)?,
        },
        band,
    )
}

/// The five standard Greeks.
///
/// `vega` is per unit of volatility (divide by 100 for one vol point) and `theta` is per
/// year (divide by 365 for per-day), so that both stay exact fixed-point numbers rather
/// than inheriting a rounding step from an arbitrary quoting convention.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Greeks {
    pub delta: D,
    pub gamma: D,
    pub vega: D,
    pub theta: D,
    pub rho: D,
}

pub fn greeks(m: &Market, kind: Kind) -> Result<Greeks> {
    m.validate()?;
    let sqrt_t = m.t.sqrt()?;
    let vol_sqrt_t = m.sigma.mul(sqrt_t)?;
    let u1 = d1(m)?;
    let u2 = u1.sub(vol_sqrt_t)?;
    let spot_df = m.spot.mul(Market::discount(m.carry, m.t)?)?;
    let strike_df = m.strike.mul(Market::discount(m.rate, m.t)?)?;
    let pdf = norm_pdf(u1)?;

    let delta = match kind {
        Kind::Call => Market::discount(m.carry, m.t)?.mul(norm_cdf(u1)?)?,
        Kind::Put => Market::discount(m.carry, m.t)?.mul(norm_cdf(u1.neg()?)?)?.neg()?,
    };
    // Both shared second-order Greeks are identical for calls and puts.
    let gamma = pdf.div(spot_df.mul(vol_sqrt_t)?)?;
    let vega = spot_df.mul(pdf)?.mul(sqrt_t)?;
    let rho = match kind {
        Kind::Call => m.t.mul(strike_df)?.mul(norm_cdf(u2)?)?,
        Kind::Put => m.t.mul(strike_df)?.neg()?.mul(norm_cdf(u2.neg()?)?)?,
    };
    let decay = spot_df.mul(pdf)?.mul(m.sigma)?.div_int(2)?.div(sqrt_t)?;
    let theta = match kind {
        Kind::Call => decay
            .neg()?
            .sub(m.rate.mul(strike_df)?.mul(norm_cdf(u2)?)?)?
            .add(m.carry.mul(spot_df)?.mul(norm_cdf(u1)?)?)?,
        Kind::Put => decay
            .neg()?
            .add(m.rate.mul(strike_df)?.mul(norm_cdf(u2.neg()?)?)?)?
            .sub(m.carry.mul(spot_df)?.mul(norm_cdf(u1.neg()?)?)?)?,
    };
    Ok(Greeks { delta, gamma, vega, theta, rho })
}

/// Refusing to allocate unboundedly is the difference between a pricing call and a DoS
/// vector, so the lattice depth is capped.
pub const MAX_LATTICE_STEPS: usize = 4096;

/// Cox-Ross-Rubinstein lattice; `american == true` allows early exercise at every node.
///
/// One row holds the option values and is folded backwards. The underlying lattice is
/// folded with it by multiplying the live nodes by `u` once per level, which follows from
/// `S u^j d^(i-j) == (S u^j d^(i+1-j)) * u` and avoids recomputing node positions.
pub fn binomial(m: &Market, kind: Kind, steps: usize, american: bool) -> Result<D> {
    m.validate()?;
    if steps == 0 || steps > MAX_LATTICE_STEPS {
        return Err(NumError::Domain);
    }
    let dt = m.t.div(D::from_int(steps as i128)?)?;
    let u = m.sigma.mul(dt.sqrt()?)?.exp()?;
    let d = D::ONE.div(u)?;
    let growth = m.rate.sub(m.carry)?.mul(dt)?.exp()?;
    let span = u.sub(d)?;
    if span == D::ZERO {
        return Err(NumError::Domain);
    }
    let p = growth.sub(d)?.div(span)?;
    if p.is_negative() || p.raw() > SCALE {
        // No risk-neutral measure at these parameters; blending with a negative or
        // greater-than-one probability would silently produce a wrong number.
        return Err(NumError::Domain);
    }
    let one_minus_p = D::ONE.sub(p)?;
    let disc = Market::discount(m.rate, dt)?;
    // Fold `disc * (p*V[j+1] + (1-p)*V[j])` into a single rounding per node. The naive
    // form rounds five times at every node, and over the ~N^2/2 nodes of a deep lattice
    // that half-quantum bias accumulates into a price error you can see in basis points.
    let wa = disc.mul(p)?.raw();
    let wb = disc.mul(one_minus_p)?.raw();

    // Terminal payoffs, built from the top of the tree downward: the top node is S*u^N,
    // and each step to the left multiplies by d^2, because u*d = 1.
    //
    // Walking the other way (from j = 0, where the value is S*d^N and at depth 1024 is
    // 1e-11) is what produced a three-percent error on a deep lattice, and worse: an
    // entry that underflows the representation gets pinned at exactly zero, which is
    // harmless for a payoff but not for the early-exercise test below.
    let down2 = d.mul(d)?;
    let mut value = vec![D::ZERO; steps + 1];
    let mut cur = m.spot.mul(u.pow_int(steps as u32)?)?;
    for j in (0..=steps).rev() {
        value[j] = payoff(cur, m.strike, kind)?;
        cur = cur.mul(down2)?;
    }

    // The American branch needs each node's own underlying, and it is rebuilt every level
    // from the diagonal instead of being carried forward by multiplication.
    //
    // Carrying it forward means an entry that underflowed at the terminal row stays at
    // zero for the whole fold, while its true value grows by u per level and at the root
    // is the spot itself. The early-exercise test then compares `K - 0 = K` against the
    // continuation value and exercises when it should not. Nothing in the European path
    // reads the lattice, which is why only the American one was wrong.
    //
    // The diagonal node (level, level) is S*u^level, always representable, and nodes to
    // its left are that times d^2 per step, so underflow now only happens where the true
    // underlying really is below one quantum and `K - S` is `K` to within 1e-9 anyway.
    let mut row = vec![D::ZERO; steps + 1];
    let mut diag = m.spot.mul(u.pow_int((steps - 1) as u32)?)?;
    for level in (0..steps).rev() {
        if american {
            let mut cur = diag;
            for j in (0..=level).rev() {
                row[j] = cur;
                cur = cur.mul(down2)?;
            }
        }
        for j in 0..=level {
            let continuation = blend(wa, wb, value[j + 1], value[j])?;
            value[j] = if american {
                continuation.max(payoff(row[j], m.strike, kind)?)
            } else {
                continuation
            };
        }
        if american {
            diag = diag.mul(d)?;
        }
    }
    Ok(value[0])
}

/// Maximum iterations before the solver gives up.
pub const MAX_IV_STEPS: usize = 80;

/// Stop when the model price matches to within four quanta of the quote currency.
pub const IV_PRICE_TOLERANCE_RAW: i128 = 4;
/// ...or when the volatility bracket is this tight, whichever comes first.
///
/// The second test is not a convenience: `sigma` itself only resolves to 1e-9, so the
/// achievable price granularity near the solution is about `vega * 1e-9`, which for a
/// typical equity option is ten times coarser than the price tolerance alone. Requiring
/// the price test by itself would report a perfectly good quote as unconverged.
pub const IV_SIGMA_TOLERANCE_RAW: i128 = 100;

/// Invert the Black-Scholes price for volatility.
///
/// Safeguarded Newton: the analytic correction uses `vega`, but the bracket is tightened
/// on every iteration, so a poor initial guess or a near-flat vega close to expiry falls
/// back to bisection instead of diverging.
///
/// `market.sigma` is used only as the starting guess.
pub fn implied_vol(price: D, market: Market, kind: Kind) -> Result<D> {
    if price.is_negative() {
        return Err(NumError::Domain);
    }
    market.validate()?;
    let (lower, upper) = bounds(&market, kind)?;
    if price.raw() < lower.raw() || price.raw() > upper.raw() {
        // Outside the no-arbitrage band there is no volatility that reproduces the quote.
        return Err(NumError::Domain);
    }
    let mut m = market;
    let mut lo = D::from_raw(10_000); // 1e-5
    let mut hi = D::from_raw(5 * SCALE); // 500%
    let mut guess = market.sigma.max(lo).min(hi);
    for _ in 0..MAX_IV_STEPS {
        m.sigma = guess;
        // A quote is only known to within its own noise band, so demanding a closer match
        // than that would be solving a problem the price cannot actually pose.
        let tolerance = price_noise_band(&m)?.raw().max(IV_PRICE_TOLERANCE_RAW);
        let diff = european(&m, kind)?.sub(price)?;
        if diff.abs().raw() <= tolerance {
            return Ok(guess);
        }
        if diff.is_negative() {
            lo = guess;
        } else {
            hi = guess;
        }
        if hi.sub(lo)?.raw() < IV_SIGMA_TOLERANCE_RAW {
            return Ok(guess);
        }
        let vega = greeks(&m, kind)?.vega;
        let step = if vega.raw() > 0 { diff.div(vega)? } else { D::ZERO };
        let next = guess.sub(step)?;
        guess = if next.raw() > lo.raw() && next.raw() < hi.raw() {
            next
        } else {
            lo.add(hi)?.div_int(2)?
        };
    }
    Err(NumError::NotConverged)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Host-side conversion helpers, kept local on purpose: `arbnum` exposes no float API
    // to its dependents, so nothing that ships to the chain can reach a float by
    // accident. These exist only to write readable expectations in tests.
    fn f(v: f64) -> D {
        D::from_raw((v * SCALE as f64).round() as i128)
    }

    fn flt(d: D) -> f64 {
        d.raw() as f64 / SCALE as f64
    }

    fn market(spot: f64, strike: f64, t: f64, sigma: f64, rate: f64, carry: f64) -> Market {
        Market { spot: f(spot), strike: f(strike), t: f(t), sigma: f(sigma), rate: f(rate), carry: f(carry) }
    }

    fn assert_close(actual: D, want: f64, tol: f64, what: &str) {
        let got = flt(actual);
        let err = (got - want).abs();
        assert!(err <= tol, "{what}: got {got}, want {want} +/- {tol} (err {err})");
    }

    fn assert_rel(actual: D, want: f64, rel: f64, what: &str) {
        let got = flt(actual);
        let err = (got - want).abs();
        let budget = (want.abs() * rel).max(1e-9);
        assert!(err <= budget, "{what}: got {got}, want {want} +/- {budget} (err {err})");
    }

    /// Haug's reference case: S=K=100, T=1, sigma=0.2, r=0.05.
    #[test]
    fn european_matches_textbook_values() {
        let m = market(100.0, 100.0, 1.0, 0.2, 0.05, 0.0);
        assert_rel(european(&m, Kind::Call).unwrap(), 10.450583552, 1e-5, "call");
        assert_rel(european(&m, Kind::Put).unwrap(), 5.573526217, 1e-5, "put");
    }

    #[test]
    fn greeks_match_textbook_values() {
        let m = market(100.0, 100.0, 1.0, 0.2, 0.05, 0.0);
        let g = greeks(&m, Kind::Call).unwrap();
        assert_rel(g.delta, 0.6368306, 1e-5, "delta call");
        assert_rel(g.gamma, 0.0187622, 1e-4, "gamma");
        // S*sqrt(T)*phi(d1) with d1 = 0.35, so phi(d1) = 0.375240.
        assert_rel(g.vega, 37.5240346, 1e-5, "vega per unit vol");
        assert!(g.theta.is_negative(), "ATM call theta should decay, got {:?}", flt(g.theta));
        assert!(g.rho.raw() > 0, "call rho should be positive");
    }

    #[test]
    fn greeks_satisfy_their_identities() {
        // delta_call - delta_put = exp(-qT); gamma and vega are shared by both sides.
        for (spot, strike, t, sigma, rate, carry) in [
            (100.0, 100.0, 1.0, 0.2, 0.05, 0.0),
            (250.0, 240.0, 0.25, 0.35, 0.05, 0.01),
            (60.0, 100.0, 1.0, 0.4, 0.08, 0.03),
        ] {
            let m = market(spot, strike, t, sigma, rate, carry);
            let c = greeks(&m, Kind::Call).unwrap();
            let p = greeks(&m, Kind::Put).unwrap();
            assert_rel(c.delta.sub(p.delta).unwrap(), flt(Market::discount(m.carry, m.t).unwrap()), 1e-6, "delta spread");
            assert_eq!(c.gamma, p.gamma, "gamma must match");
            assert_eq!(c.vega, p.vega, "vega must match");
        }
    }

    #[test]
    fn put_call_parity_holds() {
        for (spot, strike, t, sigma, rate, carry) in [
            (250.0, 240.0, 0.25, 0.35, 0.05, 0.01),
            (250.0, 260.0, 0.5, 0.4, 0.03, 0.02),
            (100.0, 100.0, 1.0, 0.2, 0.05, 0.0),
            (0.5, 2.0, 3.0, 1.5, -0.02, 0.07),
            (1e4, 1.0, 0.01, 0.9, 0.1, 0.1),
        ] {
            let m = market(spot, strike, t, sigma, rate, carry);
            let lhs = european(&m, Kind::Call).unwrap().sub(european(&m, Kind::Put).unwrap()).unwrap();
            let rhs = m
                .spot
                .mul(Market::discount(m.carry, m.t).unwrap())
                .unwrap()
                .sub(m.strike.mul(Market::discount(m.rate, m.t).unwrap()).unwrap())
                .unwrap();
            assert_close(lhs, flt(rhs), (flt(rhs).abs() * 1e-6).max(1e-6), "put-call parity");
        }
    }

    #[test]
    fn prices_respect_no_arbitrage_bounds() {
        for sigma in [0.05f64, 0.2, 0.5, 1.0, 2.0] {
            for strike in [50.0f64, 100.0, 200.0] {
                let m = market(100.0, strike, 1.0, sigma, 0.05, 0.0);
                for kind in [Kind::Call, Kind::Put] {
                    let price = european(&m, kind).unwrap();
                    let (lower, upper) = bounds(&m, kind).unwrap();
                    assert!(!price.is_negative(), "{kind:?} negative at s={sigma} k={strike}");
                    assert!(price.raw() >= lower.raw(), "{kind:?} below band at s={sigma} k={strike}: {} < {}", flt(price), flt(lower));
                    assert!(price.raw() <= upper.raw(), "{kind:?} above band at s={sigma} k={strike}: {} > {}", flt(price), flt(upper));
                }
            }
        }
    }

    #[test]
    fn european_put_can_trade_below_intrinsic() {
        // The invariant I got wrong at first: only an American put owns the right to
        // exercise, so a European one is bounded by the discounted forward, not by K - S.
        let m = market(250.0, 300.0, 0.5, 0.08, 0.04, 0.01);
        let put = european(&m, Kind::Put).unwrap();
        let intrinsic = payoff(m.spot, m.strike, Kind::Put).unwrap();
        assert!(put.raw() < intrinsic.raw(), "expected {} below intrinsic {}", flt(put), flt(intrinsic));
        assert!(put.raw() >= bounds(&m, Kind::Put).unwrap().0.raw(), "must still respect the band");
    }

    #[test]
    fn deep_otm_cancellation_floors_at_zero() {
        // Far enough out that both terms flush away, the price is exactly zero.
        let m = market(100.0, 110.0, 0.5, 0.01, 0.02, 0.0);
        assert_eq!(european(&m, Kind::Call).unwrap(), D::ZERO);
        // Across the cancellation zone the residue must stay non-negative and must not
        // masquerade as a real premium: the true value there is a few hundred-thousandths.
        for sigma in [0.01f64, 0.015, 0.02, 0.03, 0.05] {
            let m = market(100.0, 110.0, 0.5, sigma, 0.02, 0.0);
            let call = european(&m, Kind::Call).unwrap();
            assert!(!call.is_negative(), "call went negative at sigma {sigma}: {}", call.raw());
            assert!(flt(call) < 5e-2, "at sigma {sigma} a near-cancellation looks too big to be noise: {}", flt(call));
        }
    }

    #[test]
    fn price_is_monotone_in_volatility() {
        let mut prev_call = D::ZERO;
        let mut prev_put = D::ZERO;
        for i in 1..40 {
            let sigma = i as f64 / 100.0;
            let m = market(100.0, 110.0, 0.5, sigma, 0.02, 0.0);
            let call = european(&m, Kind::Call).unwrap();
            let put = european(&m, Kind::Put).unwrap();
            // Monotonicity is asserted up to 2e-6 of the quote currency, not exactly.
            // In the far tail the true increment per volatility step is smaller than the
            // 7.5e-8 absolute error of the normal-CDF approximation, so demanding a
            // strict rise would be demanding the impossible. Where the premium is a real
            // number, this bound is far tighter than anything a market would notice.
            const MONO_TOL_RAW: i128 = 2_000;
            assert!(call.raw() >= prev_call.raw() - MONO_TOL_RAW, "call should rise with vol at {sigma}: {} after {}", flt(call), flt(prev_call));
            assert!(put.raw() >= prev_put.raw() - MONO_TOL_RAW, "put should rise with vol at {sigma}: {} after {}", flt(put), flt(prev_put));
            prev_call = call;
            prev_put = put;
        }
    }

    #[test]
    fn atm_with_no_rate_or_dividend_is_symmetric() {
        let m = market(100.0, 100.0, 1.0, 0.2, 0.0, 0.0);
        let c = european(&m, Kind::Call).unwrap();
        let p = european(&m, Kind::Put).unwrap();
        assert_eq!(c, p, "atm forward with r=q=0 must price identically both ways");
        assert_rel(c, 7.9655636, 1e-4, "atm forward price 100*(2N(0.1)-1)");
    }

    #[test]
    fn binomial_converges_to_black_scholes() {
        let m = market(100.0, 100.0, 1.0, 0.2, 0.05, 0.0);
        let bs = flt(european(&m, Kind::Call).unwrap());
        let mut last_err = f64::MAX;
        for steps in [10usize, 20, 50, 100, 200, 400, 800, 1600] {
            let v = flt(binomial(&m, Kind::Call, steps, false).unwrap());
            let err = (v - bs).abs();
            assert!(err < 0.2, "binomial {steps} too far from BS: {v} vs {bs}");
            assert!(
                err <= last_err * 1.15,
                "error should shrink with depth: steps={steps} gave {err} after {last_err}"
            );
            last_err = err;
        }
        assert!(last_err < 0.01, "final lattice error {last_err}");
    }

    #[test]
    fn american_put_pays_an_early_exercise_premium() {
        // Deep ITM put with a positive rate: waiting costs more than the option gains.
        let m = market(60.0, 100.0, 1.0, 0.2, 0.08, 0.0);
        let euro = binomial(&m, Kind::Put, 200, false).unwrap();
        let amer = binomial(&m, Kind::Put, 200, true).unwrap();
        assert!(amer.raw() > euro.raw(), "expected an early premium: {} vs {}", flt(amer), flt(euro));
        // Exercising immediately is worth exactly the 40 intrinsic, and no put can be
        // worth more than the strike it pays out.
        assert!(amer.raw() >= 40 * SCALE, "american put must be worth at least intrinsic: {}", flt(amer));
        assert!(amer.raw() <= m.strike.raw(), "a put can never exceed the strike");
    }

    #[test]
    fn american_call_without_dividends_matches_european() {
        let m = market(100.0, 90.0, 1.0, 0.3, 0.05, 0.0);
        let euro = flt(european(&m, Kind::Call).unwrap());
        let amer = flt(binomial(&m, Kind::Call, 400, true).unwrap());
        assert!((amer - euro).abs() < 0.02, "no early premium expected without dividends: {amer} vs {euro}");
    }

    #[test]
    fn american_call_with_dividends_can_exercise_early() {
        let m = market(100.0, 90.0, 1.0, 0.3, 0.05, 0.12);
        let euro = binomial(&m, Kind::Call, 400, false).unwrap();
        let amer = binomial(&m, Kind::Call, 400, true).unwrap();
        assert!(amer.raw() >= euro.raw(), "early exercise can only add value");
    }

    #[test]
    fn american_put_at_a_negative_rate_never_pays_a_spurious_early_premium() {
        // Regression, and the bug is worth remembering.
        //
        // The lattice used to carry each node's underlying forward across levels by
        // multiplying by u. A terminal entry whose value underflowed the representation
        // stayed pinned at exactly zero for the whole fold -- but its true value grows by
        // u per level and is the spot itself by the time it reaches the root. The
        // early-exercise test then compared `K - 0 = K` against the continuation value and
        // exercised, pricing the American put 5% above its European twin and above
        // anything attainable. The European path never reads the lattice, which is exactly
        // why the two agreed on everything else while the American was wrong.
        //
        // At a negative rate, waiting is strictly better than taking K - S now, so the
        // early-exercise premium here has to be negligible.
        for steps in [32usize, 64, 128, 256] {
            let m = market(0.5, 2.0, 3.0, 1.5, -0.02, 0.07);
            let euro = binomial(&m, Kind::Put, steps, false).unwrap();
            let amer = binomial(&m, Kind::Put, steps, true).unwrap();
            assert!(amer.raw() >= euro.raw(), "american can never be worth less");
            let premium = amer.sub(euro).unwrap();
            assert!(
                premium.raw() <= (euro.raw() / 100_000).max(4),
                "expected no meaningful early premium: amer {} vs euro {} at n={steps}",
                flt(amer),
                flt(euro)
            );
        }
    }

    #[test]
    fn implied_vol_recovers_the_input_vol() {
        for sigma in [0.08f64, 0.2, 0.45, 1.2] {
            for (spot, strike) in [(250.0, 250.0), (250.0, 200.0), (250.0, 300.0)] {
                for kind in [Kind::Call, Kind::Put] {
                    let m = market(spot, strike, 0.5, sigma, 0.04, 0.01);
                    let price = european(&m, kind).unwrap();
                    let iv = implied_vol(price, m, kind).unwrap();
                    assert_rel(iv, sigma, 1e-3, "iv round trip");
                }
            }
        }
    }

    #[test]
    fn implied_vol_rejects_quotes_outside_the_band() {
        let m = market(100.0, 100.0, 1.0, 0.2, 0.05, 0.0);
        let (lower, upper) = bounds(&m, Kind::Call).unwrap();
        // Above the spot-equivalent ceiling, or below the discounted-forward floor, no
        // volatility exists that reproduces the quote.
        assert_eq!(implied_vol(upper.add(D::ONE).unwrap(), m, Kind::Call).map(|_| ()), Err(NumError::Domain));
        assert_eq!(implied_vol(lower.sub(D::ONE).unwrap().max(D::ZERO), m, Kind::Call).map(|_| ()), Err(NumError::Domain));
        assert_eq!(implied_vol(D::ZERO.sub(D::ONE).unwrap(), m, Kind::Call).map(|_| ()), Err(NumError::Domain));
        // Inside the band, an expensive quote simply implies a higher volatility.
        let price = european(&m, Kind::Call).unwrap();
        let iv = implied_vol(price.mul_int(2).unwrap(), m, Kind::Call).unwrap();
        assert!(iv.raw() > m.sigma.raw(), "doubling the quote must imply more vol, got {}", flt(iv));
    }

    #[test]
    fn invalid_inputs_are_rejected_not_silently_zeroed() {
        let bad = [
            market(100.0, 100.0, 0.0, 0.2, 0.05, 0.0),
            market(100.0, 100.0, 1.0, 0.0, 0.05, 0.0),
            market(0.0, 100.0, 1.0, 0.2, 0.05, 0.0),
            market(100.0, 0.0, 1.0, 0.2, 0.05, 0.0),
            market(100.0, 100.0, 1.0, 0.2, 5.0, 0.0),
            market(100.0, 100.0, 200.0, 0.2, 0.05, 0.0),
        ];
        for m in bad {
            assert_eq!(european(&m, Kind::Call).map(|_| ()), Err(NumError::Domain), "input {:?}", m);
            assert_eq!(binomial(&m, Kind::Call, 100, false).map(|_| ()), Err(NumError::Domain));
        }
        assert_eq!(binomial(&market(100.0, 100.0, 1.0, 0.2, 0.05, 0.0), Kind::Call, 0, false).map(|_| ()), Err(NumError::Domain));
        assert_eq!(binomial(&market(100.0, 100.0, 1.0, 0.2, 0.05, 0.0), Kind::Call, MAX_LATTICE_STEPS + 1, false).map(|_| ()), Err(NumError::Domain));
    }

    #[test]
    fn pricing_is_bit_reproducible() {
        // The entire reason to do this on-chain: same inputs, same answer, every time.
        let m = market(250.0, 240.0, 0.25, 0.35, 0.05, 0.01);
        let closed = european(&m, Kind::Call).unwrap();
        let lattice = binomial(&m, Kind::Put, 300, true).unwrap();
        for _ in 0..500 {
            assert_eq!(european(&m, Kind::Call).unwrap(), closed);
            assert_eq!(binomial(&m, Kind::Put, 300, true).unwrap(), lattice);
        }
    }

    #[test]
    fn lattice_extremes_error_out_rather_than_wrap() {
        // sigma*sqrt(T) large enough that the bottom node falls under one quantum and the
        // top overflows: the contract must revert, never hand back a wrapped price.
        for sigma in [3.0f64, 6.0, 9.9] {
            let m = market(250.0, 250.0, 1.0, sigma, 0.05, 0.0);
            match binomial(&m, Kind::Call, 2000, false) {
                Ok(v) => assert!(!v.is_negative(), "negative price at sigma {sigma}"),
                Err(e) => assert!(matches!(e, NumError::Overflow | NumError::Domain), "unexpected {e:?} at sigma {sigma}"),
            }
        }
    }

    #[test]
    fn short_dated_options_stay_sane() {
        // 1-day expiry is where a naive sqrt(T) or division by zero would surface.
        let m = market(250.0, 250.0, 1.0 / 365.0, 0.3, 0.05, 0.0);
        let c = european(&m, Kind::Call).unwrap();
        assert_rel(c, 1.58, 3e-2, "one-day atm call");
        assert!(greeks(&m, Kind::Call).unwrap().vega.raw() > 0);
    }
}
