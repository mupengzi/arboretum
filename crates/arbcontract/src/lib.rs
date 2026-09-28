//! Arboretum — deterministic derivatives pricing as an on-chain primitive.
//!
//! Every method is a pure function of its arguments: no storage is written, no oracle is
//! consulted, no float is executed anywhere. Two callers who pass the same integers get
//! the same integer back, on any node, forever. That is the whole point: a settlement or
//! fallback price that anyone can recompute rather than an assertion they have to trust.
//!
//! All quantities are fixed point at 1e9 (see [`arbnum::SCALE`]): `250.0` is
//! `250_000_000_000`. `t` is in years, `sigma` is annualised volatility as a decimal
//! (`0.35` = 35%), and `rate`/`carry` are continuously compounded and may be negative.
//! The ABI is `int128` throughout, which is a first-class Solidity type, so a Solidity
//! caller needs no library to encode or decode it.

#![cfg_attr(not(any(feature = "export-abi", test)), no_main)]

extern crate alloc;

use alloc::vec::Vec;
use arbnum::D;
use arbpricing::{
    binomial, bounds, european, greeks, implied_vol, price_noise_band, Kind, Market,
    MAX_LATTICE_STEPS,
};
use stylus_sdk::prelude::*;

sol_storage! {
    #[entrypoint]
    pub struct Arboretum {
        /// Never written. The entrypoint needs a storage layout, and occupying one slot
        /// with a value that is never set keeps the contract stateless in practice: a
        /// read of it costs nothing and no code path can change it.
        uint256 reserved;
    }
}

/// Every revert carries a short ASCII reason rather than an opaque selector, because the
/// difference between "domain" and "overflow" is the difference between a bug in the
/// caller and a limit of the representation.
fn reason(e: arbnum::NumError) -> Vec<u8> {
    match e {
        arbnum::NumError::Domain => b"domain".to_vec(),
        arbnum::NumError::Overflow => b"overflow".to_vec(),
        arbnum::NumError::DivByZero => b"div_by_zero".to_vec(),
        arbnum::NumError::NotConverged => b"not_converged".to_vec(),
        arbnum::NumError::Inconsistent => b"inconsistent".to_vec(),
    }
}

#[inline]
fn market(spot: i128, strike: i128, t: i128, sigma: i128, rate: i128, carry: i128) -> Market {
    Market {
        spot: D::from_raw(spot),
        strike: D::from_raw(strike),
        t: D::from_raw(t),
        sigma: D::from_raw(sigma),
        rate: D::from_raw(rate),
        carry: D::from_raw(carry),
    }
}

#[inline]
fn kind(is_put: bool) -> Kind {
    if is_put {
        Kind::Put
    } else {
        Kind::Call
    }
}

#[public]
impl Arboretum {
    /// The scale every argument and return value is expressed in. Integrators should read
    /// this rather than hard-code it.
    pub fn scale(&self) -> i128 {
        arbnum::SCALE
    }

    /// The lattice depth ceiling, so a caller can clamp its own inputs.
    pub fn max_lattice_steps(&self) -> u32 {
        MAX_LATTICE_STEPS as u32
    }

    /// Black-Scholes price with continuous carry, call or put.
    pub fn price_european(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        european(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|v| v.raw())
            .map_err(reason)
    }

    /// Cox-Ross-Rubinstein lattice price. `american` allows early exercise at every node;
    /// `steps` is capped at [`Self::max_lattice_steps`] so that a call cannot be turned
    /// into an unbounded allocation.
    pub fn price_lattice(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
        american: bool,
        steps: u32,
    ) -> Result<i128, Vec<u8>> {
        binomial(
            &market(spot, strike, t, sigma, rate, carry),
            kind(is_put),
            steps as usize,
            american,
        )
        .map(|v| v.raw())
        .map_err(reason)
    }

    /// Invert a quoted price for volatility. Reverts with `domain` when the quote sits
    /// outside the no-arbitrage band, because no volatility reproduces it.
    pub fn implied_vol(
        &self,
        price: i128,
        spot: i128,
        strike: i128,
        t: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        // The market's sigma field is only the solver's starting guess here.
        let m = market(spot, strike, t, 500_000_000, rate, carry);
        implied_vol(D::from_raw(price), m, kind(is_put))
            .map(|v| v.raw())
            .map_err(reason)
    }

    /// `dPrice/dSpot`, in units of the underlying per unit of spot.
    pub fn delta(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        greeks(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|g| g.delta.raw())
            .map_err(reason)
    }

    /// `d2Price/dSpot2`.
    pub fn gamma(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        greeks(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|g| g.gamma.raw())
            .map_err(reason)
    }

    /// Sensitivity to one whole unit of volatility; divide by 100 for one vol point.
    pub fn vega(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        greeks(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|g| g.vega.raw())
            .map_err(reason)
    }

    /// Per year; divide by 365 for a day. Negative for a long option.
    pub fn theta(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        greeks(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|g| g.theta.raw())
            .map_err(reason)
    }

    /// Sensitivity to one whole unit of the risk-free rate.
    pub fn rho(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        greeks(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|g| g.rho.raw())
            .map_err(reason)
    }

    /// The no-arbitrage floor a European price must sit above: the discounted forward
    /// difference, which is *not* intrinsic value, because a European option cannot be
    /// exercised early.
    pub fn lower_bound(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        bounds(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|(lo, _hi)| lo.raw())
            .map_err(reason)
    }

    /// The no-arbitrage ceiling: the discounted spot for a call, the discounted strike
    /// for a put.
    pub fn upper_bound(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
        is_put: bool,
    ) -> Result<i128, Vec<u8>> {
        bounds(&market(spot, strike, t, sigma, rate, carry), kind(is_put))
            .map(|(_lo, hi)| hi.raw())
            .map_err(reason)
    }

    /// How wide the representation's uncertainty is at these parameters. A quote closer
    /// to the model than this band is asking for precision the engine does not claim.
    pub fn noise_band(
        &self,
        spot: i128,
        strike: i128,
        t: i128,
        sigma: i128,
        rate: i128,
        carry: i128,
    ) -> Result<i128, Vec<u8>> {
        price_noise_band(&market(spot, strike, t, sigma, rate, carry))
            .map(|v| v.raw())
            .map_err(reason)
    }
}