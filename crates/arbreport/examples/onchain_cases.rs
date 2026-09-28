//! Prints the host build's answer for the exact cases `scripts/verify_onchain.sh` calls on
//! the deployed contract, one `name value` pair per line.
//!
//! This is the whole thesis in executable form: if the numbers here and the numbers the
//! chain returns are identical, then the price is a computation anyone can reproduce
//! rather than an assertion anyone has to trust.
//!
//! `cargo run --release -p arbreport --example onchain_cases`

use arbnum::D;
use arbpricing::{binomial, bounds, european, greeks, implied_vol, price_noise_band, Kind, Market};

const SCALE: i128 = 1_000_000_000;

fn d(v: i128) -> D {
    D::from_raw(v * SCALE)
}

/// 250 spot, 240 strike, a quarter year, 35% vol, 5% rate, 1% carry.
fn market() -> Market {
    Market { spot: d(250), strike: d(240), t: D::from_raw(250_000_000), sigma: D::from_raw(350_000_000), rate: D::from_raw(50_000_000), carry: D::from_raw(10_000_000) }
}

type CaseResult = arbnum::Result<D>;

fn show(name: &str, value: CaseResult) {
    match value {
        Ok(v) => println!("{name} {}", v.raw()),
        Err(e) => println!("{name} ERR {e}"),
    }
}

fn main() {
    let m = market();
    show("priceEuropean_call", european(&m, Kind::Call));
    show("priceEuropean_put", european(&m, Kind::Put));
    show("priceLattice_euro_call_512", binomial(&m, Kind::Call, 512, false));
    show("priceLattice_amer_put_512", binomial(&m, Kind::Put, 512, true));
    show("delta_call", greeks(&m, Kind::Call).map(|g| g.delta));
    show("vega_call", greeks(&m, Kind::Call).map(|g| g.vega));
    show("noiseBand", price_noise_band(&m));
    show("lowerBound_call", bounds(&m, Kind::Call).map(|(lo, _)| lo));
    show("upperBound_call", bounds(&m, Kind::Call).map(|(_, hi)| hi));

    // Round-trip the price the contract returned earlier through implied volatility.
    let quoted = D::from_raw(23_800_000_000);
    show("impliedVol_from_23.8", implied_vol(quoted, m, Kind::Call));
}