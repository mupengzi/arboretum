//! Size probe.
//!
//! Each pricing path gets its own exported entry point so the linker cannot dead-strip
//! it, which makes the resulting `.wasm` a stand-in for "the contract that exposes these
//! functions". Stylus limits the *compressed* program size, so the brotli figure matters
//! more than the raw byte count; both are reported by `scripts/size.sh`.
//!
//! This is a probe, not the contract: it has no stylus-sdk glue and no storage layer.
//! Expect the real contract to land somewhat above these numbers.

use arbnum::{erf, norm_cdf, norm_pdf, D};
use arbpricing::{binomial, bounds, european, greeks, implied_vol, price_noise_band, Kind, Market};

const SENTINEL: i64 = -1;
/// Used as the starting guess where implied volatility needs one.
const IV_GUESS: i64 = 500_000_000;

fn d(raw: i64) -> D {
    D::from_raw(raw as i128)
}

fn kind(is_put: u8) -> Kind {
    if is_put == 0 {
        Kind::Call
    } else {
        Kind::Put
    }
}

macro_rules! market {
    ($spot:ident, $strike:ident, $t:ident, $sigma:ident, $rate:ident, $carry:ident) => {
        Market {
            spot: d($spot),
            strike: d($strike),
            t: d($t),
            sigma: d($sigma),
            rate: d($rate),
            carry: d($carry),
        }
    };
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn arb_price_european(spot: i64, strike: i64, t: i64, sigma: i64, rate: i64, carry: i64, is_put: u8) -> i64 {
    let m = market!(spot, strike, t, sigma, rate, carry);
    european(&m, kind(is_put)).map(|v| v.raw() as i64).unwrap_or(SENTINEL)
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn arb_price_lattice(spot: i64, strike: i64, t: i64, sigma: i64, rate: i64, carry: i64, is_put: u8, steps: u32, american: u8) -> i64 {
    let m = market!(spot, strike, t, sigma, rate, carry);
    binomial(&m, kind(is_put), steps as usize, american != 0).map(|v| v.raw() as i64).unwrap_or(SENTINEL)
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn arb_greek(spot: i64, strike: i64, t: i64, sigma: i64, rate: i64, carry: i64, is_put: u8, which: u8) -> i64 {
    let m = market!(spot, strike, t, sigma, rate, carry);
    let g = match greeks(&m, kind(is_put)) {
        Ok(g) => g,
        Err(_) => return SENTINEL,
    };
    let v = match which {
        0 => g.delta,
        1 => g.gamma,
        2 => g.vega,
        3 => g.theta,
        _ => g.rho,
    };
    v.raw() as i64
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn arb_implied_vol(price: i64, spot: i64, strike: i64, t: i64, rate: i64, carry: i64, is_put: u8) -> i64 {
    let m = market!(spot, strike, t, IV_GUESS, rate, carry);
    implied_vol(d(price), m, kind(is_put)).map(|v| v.raw() as i64).unwrap_or(SENTINEL)
}

/// Both band helpers, since a settlement-priced contract will want them: the derived
/// noise band, and the no-arbitrage lower bound.
#[inline(never)]
#[no_mangle]
pub extern "C" fn arb_band(spot: i64, strike: i64, t: i64, sigma: i64, rate: i64, carry: i64, is_put: u8, which: u8) -> i64 {
    let m = market!(spot, strike, t, sigma, rate, carry);
    let got = if which == 0 {
        price_noise_band(&m)
    } else {
        bounds(&m, kind(is_put)).map(|(lo, _hi)| lo)
    };
    got.map(|v| v.raw() as i64).unwrap_or(SENTINEL)
}

/// The elementary functions on their own, so the probe's cost can be attributed.
#[inline(never)]
#[no_mangle]
pub extern "C" fn arb_math(x: i64, which: u8) -> i64 {
    fn go(x: D, which: u8) -> arbnum::Result<D> {
        Ok(match which {
            0 => x.exp()?,
            1 => x.ln()?,
            2 => x.sqrt()?,
            3 => x.mul(x)?.exp()?,
            4 => erf(x)?,
            5 => norm_cdf(x)?,
            6 => norm_pdf(x)?,
            _ => x.pow(D::from_raw(IV_GUESS as i128))?,
        })
    }
    go(d(x), which).map(|v| v.raw() as i64).unwrap_or(SENTINEL)
}
