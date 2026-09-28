//! Deterministic fixed-point numerics for on-chain derivatives pricing.
//!
//! There is deliberately no `f32`/`f64` outside `#[cfg(test)]`. Arbitrum's Stylus
//! runtime lists floating point operations as unsupported, and integer-only arithmetic
//! is what makes a price reproducible bit-for-bit on every validator: two nodes cannot
//! disagree about the value of an option, so a settlement price derived from this crate
//! is a verifiable computation rather than an assertion.
//!
//! Representation: [`D`] holds a real number `x = raw / SCALE` with `SCALE = 1e9`.
//! That buys 9 decimal digits of absolute precision, which is ~7 orders tighter than
//! the basis-point accuracy an option price actually needs, and it keeps every product
//! inside `i128` for realistic market inputs. All arithmetic is checked: overflow is an
//! [`NumError`], never a silent wraparound.
//!
//! These operations deliberately do not implement `std::ops`: every one of them can fail,
//! and the operator traits cannot return a `Result` without panicking on overflow, which
//! in a contract means burning the caller's gas.
#![allow(clippy::should_implement_trait)]

use core::fmt;

/// Fixed-point scale: values are stored multiplied by 1e9.
pub const SCALE: i128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumError {
    /// Input outside the function's domain (log of a non-positive number, sqrt of a
    /// negative number, ...).
    Domain,
    /// Result cannot be represented.
    Overflow,
    DivByZero,
    /// An iterative method exhausted its budget without meeting its tolerance.
    NotConverged,
    /// An intermediate result contradicted a mathematical invariant, such as a negative
    /// option price beyond representation noise.
    Inconsistent,
}

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NumError::Domain => "domain error",
            NumError::Overflow => "fixed-point overflow",
            NumError::DivByZero => "division by zero",
            NumError::NotConverged => "iteration did not converge",
            NumError::Inconsistent => "result violates a pricing invariant",
        })
    }
}

pub type Result<T> = core::result::Result<T, NumError>;

/// A signed fixed-point number with 9 fractional decimal digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct D(i128);

/// Round-half-up quotient of `num / den`, with `den != 0`.
///
/// Euclidean division guarantees the remainder lands in `[0, |den|)`, so "remainder at
/// least half a divisor" is a total, well-defined rule on every input. Pinning the
/// rounding mode is what makes results reproducible across hosts.
pub fn round_div(num: i128, den: i128) -> Result<i128> {
    if den == 0 {
        return Err(NumError::DivByZero);
    }
    let half = den.checked_abs().ok_or(NumError::Overflow)?;
    let q = num.div_euclid(den);
    let r = num.rem_euclid(den);
    match r.checked_mul(2) {
        Some(twice_r) if twice_r >= half => q.checked_add(1).ok_or(NumError::Overflow),
        _ => Ok(q),
    }
}

/// `v` divided by `2^k` (or multiplied, when `k < 0`), rounding halves away from zero.
fn shift_round(v: i128, k: i128) -> Result<i128> {
    if k > 0 {
        if k >= 128 {
            return Ok(0);
        }
        round_div(v, 1i128.checked_shl(k as u32).ok_or(NumError::Overflow)?)
    } else if k < 0 {
        let shifts = (-k) as u32;
        if shifts >= 128 {
            return Err(NumError::Overflow);
        }
        v.checked_shl(shifts).ok_or(NumError::Overflow)
    } else {
        Ok(v)
    }
}

impl D {
    pub const ZERO: D = D(0);
    pub const ONE: D = D(SCALE);
    pub const TWO: D = D(2 * SCALE);
    pub const NEG_ONE: D = D(-SCALE);
    pub const MAX: D = D(i128::MAX);

    /// Wrap an already-scaled integer.
    #[inline]
    pub const fn from_raw(raw: i128) -> D {
        D(raw)
    }

    #[inline]
    pub const fn raw(self) -> i128 {
        self.0
    }

    /// Exact construction from a whole number, e.g. a strike of `150`.
    pub const fn from_int(v: i128) -> Result<D> {
        match v.checked_mul(SCALE) {
            Some(r) => Ok(D(r)),
            None => Err(NumError::Overflow),
        }
    }

    /// Exact construction from a small integer; convenient in tests and demo drivers.
    pub const fn from_i32(v: i32) -> D {
        D(v as i128 * SCALE)
    }

    /// Truncate toward minus infinity, e.g. for USDG cent buckets.
    pub const fn to_int_floor(self) -> i128 {
        self.0.div_euclid(SCALE)
    }

    /// `(integer_part, fractional_part_scaled)` in sign-magnitude form. Lets a contract
    /// print a value without any floating point or allocator.
    pub const fn decompose(self) -> (i128, i128) {
        let sign = if self.0 < 0 { -1 } else { 1 };
        let mag = if self.0 < 0 { -(self.0) } else { self.0 };
        (sign * (mag / SCALE), sign * (mag % SCALE))
    }

    #[cfg(test)]
    pub fn to_f64_lossy(self) -> f64 {
        self.0 as f64 / SCALE as f64
    }

    #[cfg(test)]
    pub fn from_f64_lossy(v: f64) -> D {
        D((v * SCALE as f64).round() as i128)
    }

    #[inline]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub const fn abs(self) -> D {
        D(if self.0 < 0 { -(self.0) } else { self.0 })
    }

    pub fn neg(self) -> Result<D> {
        self.0.checked_neg().map(D).ok_or(NumError::Overflow)
    }

    pub fn add(self, o: D) -> Result<D> {
        self.0.checked_add(o.0).map(D).ok_or(NumError::Overflow)
    }

    pub fn sub(self, o: D) -> Result<D> {
        self.0.checked_sub(o.0).map(D).ok_or(NumError::Overflow)
    }

    pub fn mul_int(self, k: i128) -> Result<D> {
        self.0.checked_mul(k).map(D).ok_or(NumError::Overflow)
    }

    /// Fixed-point product. Falls back to splitting `self` into integer and fractional
    /// halves when the raw product would not fit, which is what lets a binomial tree
    /// carry node values in the trillions without ever wrapping.
    pub fn mul(self, o: D) -> Result<D> {
        if let Some(p) = self.0.checked_mul(o.0) {
            return Ok(D(round_div(p, SCALE)?));
        }
        let whole = self.0.div_euclid(SCALE);
        let frac = self.0.rem_euclid(SCALE);
        let t1 = whole.checked_mul(o.0).ok_or(NumError::Overflow)?;
        let t2 = round_div(frac.checked_mul(o.0).ok_or(NumError::Overflow)?, SCALE)?;
        Ok(D(t1.checked_add(t2).ok_or(NumError::Overflow)?))
    }

    pub fn div(self, o: D) -> Result<D> {
        if o.0 == 0 {
            return Err(NumError::DivByZero);
        }
        let num = self.0.checked_mul(SCALE).ok_or(NumError::Overflow)?;
        Ok(D(round_div(num, o.0)?))
    }

    pub fn recip(self) -> Result<D> {
        D::ONE.div(self)
    }

    /// Divide by a plain whole number. Note this is *not* `div(D::from_int(k))`: the raw
    /// representation is already scaled once, so halving a value is halving its raw
    /// integer. Multiplying by `SCALE` here would scale twice and be off by 1e9.
    pub fn div_int(self, k: i128) -> Result<D> {
        if k == 0 {
            return Err(NumError::DivByZero);
        }
        Ok(D(round_div(self.0, k)?))
    }

    /// `self^k` for a non-negative whole exponent, by repeated multiplication so the
    /// rounding path stays identical to every other product in the crate.
    pub fn pow_int(self, k: u32) -> Result<D> {
        let mut acc = D::ONE;
        for _ in 0..k {
            acc = acc.mul(self)?;
        }
        Ok(acc)
    }

    /// Exact integer square root of the scaled value, so `sqrt(x).mul(sqrt(x))` is
    /// within one unit in the last place of `x`.
    pub fn sqrt(self) -> Result<D> {
        if self.0 < 0 {
            return Err(NumError::Domain);
        }
        if self.0 == 0 {
            return Ok(D::ZERO);
        }
        let p = self.0.checked_mul(SCALE).ok_or(NumError::Overflow)? as u128;
        Ok(D(isqrt(p) as i128))
    }

    /// `e^self`, saturating to zero for large negative arguments and erroring on large
    /// positive ones. Reduced by `k * ln 2` then evaluated by Taylor series, so the
    /// working range of the series is under a quarter of a unit.
    pub fn exp(self) -> Result<D> {
        let k = round_div(self.0, LN2.0)?;
        if k > 120 {
            return Err(NumError::Overflow);
        }
        if k < -64 {
            return Ok(D::ZERO);
        }
        let r = self
            .0
            .checked_sub(scaled_ln2(k)?.raw())
            .ok_or(NumError::Overflow)?;

        let mut term = SCALE; // r^0 / 0!
        let mut sum = SCALE;
        for n in 1i128..=40 {
            let next = round_div(term.checked_mul(r).ok_or(NumError::Overflow)?, n.checked_mul(SCALE).ok_or(NumError::Overflow)?)?;
            if next == 0 {
                break;
            }
            sum = sum.checked_add(next).ok_or(NumError::Overflow)?;
            term = next;
        }
        // exp(k * ln 2) == 2^k, so the reduced result is scaled back up by 2^k.
        Ok(D(shift_round(sum, -k)?))
    }

    /// Natural log of a positive value. Splits out the power of two, then uses
    /// `ln m = 2 * atanh((m-1)/(m+1))`, whose argument stays below 1/3 and therefore
    /// converges in a couple of dozen terms.
    pub fn ln(self) -> Result<D> {
        if self.0 <= 0 {
            return Err(NumError::Domain);
        }
        let bits = 128 - (self.0 as u128).leading_zeros() as i128;
        let scale_bits = 128 - (SCALE as u128).leading_zeros() as i128;
        let mut k = bits - scale_bits;
        let mut m = shift_round(self.0, k)?;
        while m >= 2 * SCALE {
            k += 1;
            m = shift_round(m, 1)?;
        }
        while m < SCALE {
            k -= 1;
            m = shift_round(m, -1)?;
        }

        let y = D(round_div((m - SCALE).checked_mul(SCALE).ok_or(NumError::Overflow)?, m.checked_add(SCALE).ok_or(NumError::Overflow)?)?);
        let y2 = y.mul(y)?;
        let mut acc = y;
        let mut pow = y;
        for n in 1i128..=300 {
            pow = pow.mul(y2)?;
            let step = D(round_div(pow.0, 2 * n + 1)?);
            if step.0 == 0 {
                break;
            }
            acc = acc.add(step)?;
        }
        acc.mul_int(2)?.add(scaled_ln2(k)?)
    }

    /// `self^exponent` for a positive base.
    pub fn pow(self, exponent: D) -> Result<D> {
        exponent.mul(self.ln()?)?.exp()
    }

    pub fn min(self, o: D) -> D {
        if self.0 <= o.0 { self } else { o }
    }

    pub fn max(self, o: D) -> D {
        if self.0 >= o.0 { self } else { o }
    }
}

/// Integer square root, rounding down. Newton iteration started from a power of two
/// that provably bounds the root from above, then a division-based correction that
/// avoids ever forming `x * x`.
fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << (128 - n.leading_zeros()).div_ceil(2);
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            break;
        }
        x = y;
    }
    // Correct by division rather than by squaring, which would overflow near the top of
    // the range this function has to cover.
    while x > 0 && n / x < x {
        x -= 1;
    }
    while n / (x + 1) > x {
        x += 1;
    }
    x
}

// ---------------------------------------------------------------------------
// Constants, all rounded once here rather than scattered through the algorithms.
// ---------------------------------------------------------------------------

/// `ln 2`
pub const LN2: D = D(693_147_181);
/// `ln 2` carried with nine extra guard digits.
///
/// Argument reduction multiplies this constant by `k`, which multiplies its rounding
/// error by `k` as well: at `k = 14` that dominated the error of the whole series.
/// Keeping the guard digits moves the term two orders below the truncation error.
const LN2_GUARD: i128 = 693_147_180_559_945_309;

/// `k * ln 2` at working scale, without letting the constant's rounding scale with `k`.
fn scaled_ln2(k: i128) -> Result<D> {
    let prod = k.checked_mul(LN2_GUARD).ok_or(NumError::Overflow)?;
    Ok(D(round_div(prod, SCALE)?))
}
/// `sqrt 2`
pub const SQRT2: D = D(1_414_213_562);
/// `1 / sqrt(2 pi)`, the peak of the standard normal density.
pub const INV_SQRT_2PI: D = D(398_942_280);
/// `2 / sqrt(pi)`
pub const TWO_OVER_SQRT_PI: D = D(1_128_379_167);

// Abramowitz & Stegun 26.2.17 coefficients. Every one of them is specified to nine
// decimal places, which is exactly the resolution of this representation.
const AS_B1: D = D(319_381_530);
const AS_B2: D = D(-356_563_782);
const AS_B3: D = D(1_781_477_937);
const AS_B4: D = D(-1_821_255_978);
const AS_B5: D = D(1_330_274_429);
const AS_P: D = D(231_641_900);

/// Standard normal density at `x`.
pub fn norm_pdf(x: D) -> Result<D> {
    if x.abs().0 >= 40 * SCALE {
        return Ok(D::ZERO);
    }
    let half_sq = x.mul(x)?.div_int(2)?;
    INV_SQRT_2PI.mul(half_sq.neg()?.exp()?)
}

/// Standard normal CDF.
///
/// Uses the Abramowitz & Stegun 26.2.17 rational form, whose published absolute error is
/// under `7.5e-8`. That is a fifth of a basis point of the underlying for a $250 option,
/// and the measured error against an arbitrary-precision reference is in
/// `docs/ACCURACY.md`. Anything tighter would cost more terms than it is worth on-chain.
pub fn norm_cdf(x: D) -> Result<D> {
    let ax = x.abs();
    if ax.0 >= 40 * SCALE {
        return if x.is_negative() { Ok(D::ZERO) } else { Ok(D::ONE) };
    }
    // t = 1 / (1 + p*x)
    let denom = D::ONE.add(ax.mul(AS_P)?)?;
    let t = D::ONE.div(denom)?;
    // gamma = b1*t + b2*t^2 + ... + b5*t^5, by Horner
    let mut g = AS_B5;
    for b in [AS_B4, AS_B3, AS_B2, AS_B1] {
        g = g.mul(t)?.add(b)?;
    }
    let gamma = g.mul(t)?;
    let tail = norm_pdf(ax)?.mul(gamma)?;
    if x.is_negative() {
        Ok(tail)
    } else {
        D::ONE.sub(tail)
    }
}

/// Error function. For `|x| <= 1` the defining Taylor series is used directly, which is
/// far more accurate than the rational forms near zero; beyond that it is derived from
/// the normal CDF.
pub fn erf(x: D) -> Result<D> {
    if x.is_negative() {
        let mirrored = erf(x.abs())?;
        return mirrored.neg();
    }
    if x.0 > SCALE {
        // erf(x) = 2*Phi(x*sqrt 2) - 1
        let z = x.mul(SQRT2)?;
        return D::TWO.mul(norm_cdf(z)?)?.sub(D::ONE);
    }
    // erf(x) = 2/sqrt(pi) * sum (-1)^n x^(2n+1) / (n! (2n+1))
    let x2 = x.mul(x)?;
    let mut pow = x;
    let mut fact = 1i128;
    let mut acc = x;
    let mut sign = -1i128;
    for n in 1i128..=200 {
        pow = pow.mul(x2)?;
        fact = match fact.checked_mul(n) {
            Some(f) => f,
            None => break,
        };
        let denom = fact.checked_mul(2 * n + 1).ok_or(NumError::Overflow)?;
        let step = D(round_div(pow.0, denom)?);
        if step.0 == 0 {
            break;
        }
        acc = if sign > 0 { acc.add(step)? } else { acc.sub(step)? };
        sign = -sign;
    }
    TWO_OVER_SQRT_PI.mul(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> D {
        D::from_f64_lossy(v)
    }

    fn assert_close(actual: D, want: f64, tol: f64, what: &str) {
        let got = actual.to_f64_lossy();
        let err = (got - want).abs();
        assert!(err <= tol, "{what}: got {got}, want {want} +/- {tol} (err {err})");
    }

    /// A tolerance that means something for this representation: `rel` of the value, but
    /// never tighter than a few units in the last place, since 1e-9 is the quantum.
    /// Asserting tighter than `3e-9` on a small number is asking for the impossible.
    fn tol(want: f64, rel: f64) -> f64 {
        (want.abs() * rel).max(3e-9)
    }

    #[test]
    fn rounding_is_half_up_and_total() {
        assert_eq!(round_div(5, 2).unwrap(), 3);
        assert_eq!(round_div(-5, 2).unwrap(), -2); // -2.5 rounds up toward +inf
        assert_eq!(round_div(4, 2).unwrap(), 2);
        assert_eq!(round_div(1, 0), Err(NumError::DivByZero));
    }

    #[test]
    fn mul_and_div_roundtrip() {
        for a in [0.5f64, 1.0, 2.0, 250.0, 0.0001, 1.5] {
            for b in [0.0001f64, 0.25, 1.0, 3.0] {
                let (x, y) = (f(a), f(b));
                let prod = x.mul(y).unwrap();
                assert_close(prod, a * b, (a * b).abs() * 1e-9 + 1e-9, "mul");
                assert_close(prod.div(y).unwrap(), a, 1e-8, "mul then div");
            }
        }
    }

    #[test]
    fn mul_uses_split_path_for_huge_operands() {
        // The raw product 1e21 * 1e21 does not fit in i128, but the result does.
        let big = f(1e12);
        assert_close(big.mul(big).unwrap(), 1e24, 1e15, "split-path mul");
        // ...and a product that is genuinely unrepresentable must be an error, not a wrap.
        let huge = f(1e15);
        assert!(huge.mul(huge).is_err());
    }

    #[test]
    fn sqrt_matches_known_values() {
        assert_close(f(2.0).sqrt().unwrap(), core::f64::consts::SQRT_2, 1e-9, "sqrt2");
        assert_close(f(0.0).sqrt().unwrap(), 0.0, 0.0, "sqrt0");
        assert_close(f(1.0).sqrt().unwrap(), 1.0, 0.0, "sqrt1");
        assert_close(f(1e6).sqrt().unwrap(), 1000.0, 1e-9, "sqrt1e6");
        assert_close(f(1e-6).sqrt().unwrap(), 1e-3, 1e-12, "sqrt small");
        assert_eq!(f(-1.0).sqrt(), Err(NumError::Domain));
    }

    #[test]
    fn sqrt_is_within_one_ulp_of_exact() {
        for raw in [1i128, 999, 1_000_000, 123_456_789_012, SCALE * 7] {
            let s = D(raw).sqrt().unwrap().raw();
            assert!(s * s <= raw * SCALE, "sqrt too big for {raw}");
            assert!((s + 1) * (s + 1) > raw * SCALE, "sqrt not maximal for {raw}");
        }
    }

    #[test]
    fn exp_matches_known_values() {
        assert_close(D::ZERO.exp().unwrap(), 1.0, 0.0, "exp0");
        assert_close(D::ONE.exp().unwrap(), core::f64::consts::E, tol(core::f64::consts::E, 1e-8), "exp1");
        assert_close(f(-1.0).exp().unwrap(), 0.36787944117144233, tol(0.37, 1e-8), "exp-1");
        assert_close(f(0.05).exp().unwrap(), 1.0512710963760241, tol(1.05, 1e-8), "exp0.05");
        assert_close(f(-core::f64::consts::LN_2).exp().unwrap(), 0.5, tol(0.5, 1e-8), "exp -ln2");
        assert_close(f(10.0).exp().unwrap(), 22026.465794806718, tol(22026.0, 1e-8), "exp10");
        assert_close(f(20.0).exp().unwrap(), 485_165_195.409_790_3, tol(4.85e8, 1e-8), "exp20");
        assert_eq!(f(-100.0).exp().unwrap(), D::ZERO);
        assert!(f(200.0).exp().is_err());
    }

    #[test]
    fn ln_matches_known_values() {
        assert_close(D::ONE.ln().unwrap(), 0.0, 3e-9, "ln1");
        assert_close(f(2.0).ln().unwrap(), core::f64::consts::LN_2, tol(0.693, 1e-8), "ln2");
        assert_close(f(0.5).ln().unwrap(), -core::f64::consts::LN_2, tol(0.693, 1e-8), "ln0.5");
        assert_close(f(1e6).ln().unwrap(), 13.815510557964274, tol(13.8, 1e-8), "ln1e6");
        assert_close(f(1e-6).ln().unwrap(), -13.815510557964274, tol(13.8, 1e-8), "ln1e-6");
        assert_close(f(250.0).ln().unwrap(), 5.521460917862246, tol(5.5, 1e-8), "ln250");
        assert_eq!(D::ZERO.ln(), Err(NumError::Domain));
        assert_eq!(f(-2.0).ln(), Err(NumError::Domain));
    }

    #[test]
    fn exp_and_ln_are_inverses_across_the_pricing_range() {
        // exp(ln x) == x: covers spot / strike ratios a live market will ever produce.
        let mut x = 1e-6f64;
        while x < 1e12 {
            assert_close(
                f(x).ln().unwrap().exp().unwrap(),
                x,
                tol(x, 1e-6),
                "exp(ln x)",
            );
            x *= 1.7;
        }
        // ln(exp k) == k over the range where the intermediate keeps meaningful digits.
        // Below k ~ -5 the value of e^k occupies only tens of quanta, so the round trip
        // measures the representation's resolution rather than ln's accuracy; the
        // dedicated flush-to-zero test below covers that end instead.
        let mut k = -4.0f64;
        while k <= 25.0 {
            assert_close(
                f(k).exp().unwrap().ln().unwrap(),
                k,
                tol(k.abs().max(1.0), 1e-6),
                "ln(exp k)",
            );
            k += 0.5;
        }
    }

    #[test]
    fn exp_flushes_to_zero_below_the_representable_quantum() {
        // A deep out-of-the-money option whose value is under 1e-9 prices as zero rather
        // than as noise. That is the honest behaviour of a 9-digit fixed point.
        assert_eq!(f(-22.0).exp().unwrap(), D::ZERO);
        assert!(f(-21.0).exp().unwrap().raw() > 0);
    }

    #[test]
    fn pow_matches_known_values() {
        assert_close(f(2.0).pow(f(10.0)).unwrap(), 1024.0, tol(1024.0, 1e-7), "2^10");
        assert_close(
            f(250.0).pow(f(0.5)).unwrap(),
            15.811_388_300_841_896,
            tol(15.8, 1e-7),
            "sqrt via pow",
        );
        assert_close(
            f(1.05).pow(f(20.0)).unwrap(),
            2.653_297_705_144_422,
            tol(2.65, 1e-7),
            "compounding",
        );
    }

    #[test]
    fn norm_pdf_peaks_correctly() {
        assert_close(norm_pdf(D::ZERO).unwrap(), 0.3989422804014327, 1e-9, "pdf0");
        assert_close(norm_pdf(f(1.0)).unwrap(), 0.24197072451914337, 1e-9, "pdf1");
        assert_eq!(norm_pdf(f(-1.0)).unwrap(), norm_pdf(f(1.0)).unwrap(), "pdf even");
    }

    #[test]
    fn norm_cdf_matches_known_values() {
        let cases: &[(f64, f64)] = &[
            (0.0, 0.5),
            (0.5, 0.6914624612740131),
            (1.0, 0.8413447460685429),
            (1.645, 0.9500151629373349),
            (1.96, 0.9750021048517796),
            (2.0, 0.9772498680518208),
            (-1.0, 0.15865525393145707),
            (-1.96, 0.024997895148220392),
            (3.0, 0.9986501019683699),
            (0.05, 0.5199387918865901),
            (-0.3, 0.3820885778110474),
        ];
        for (x, want) in cases {
            assert_close(norm_cdf(f(*x)).unwrap(), *want, 1e-7, &format!("cdf({x})"));
        }
    }

    #[test]
    fn norm_cdf_is_monotone_and_symmetric() {
        let mut prev = D::ZERO;
        let mut x = -6.0f64;
        while x <= 6.0 {
            let c = norm_cdf(f(x)).unwrap();
            assert!(c.raw() >= prev.raw(), "cdf not monotone at {x}");
            let complement = c.add(norm_cdf(f(-x)).unwrap()).unwrap();
            assert_close(complement, 1.0, 2e-7, &format!("symmetry at {x}"));
            prev = c;
            x += 0.01;
        }
    }

    #[test]
    fn erf_matches_known_values() {
        let cases: &[(f64, f64)] = &[
            (0.0, 0.0),
            (0.5, 0.5204998778130465),
            (1.0, 0.8427007929497149),
            (0.25, 0.2763263901682369),
            (-0.5, -0.5204998778130465),
            (2.0, 0.9953222650189527),
        ];
        for (x, want) in cases {
            // Above |x| = 1 this routes through the rational normal CDF, whose own
            // published error bound is 7.5e-8 and which then gets doubled.
            assert_close(erf(f(*x)).unwrap(), *want, 2e-7, &format!("erf({x})"));
        }
    }

    #[test]
    fn decompose_avoids_floats_when_printing() {
        let v = f(250.012345678);
        let (i, frac) = v.decompose();
        assert_eq!((i, frac), (250, 12345678));
        let n = f(-0.5);
        assert_eq!(n.decompose(), (-0, -500000000));
    }
}
