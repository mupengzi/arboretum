//! Diagnostic: how does the lattice value evolve with depth, and where does it stop
//! tracking the same recursion evaluated in double precision?
//!
//! `cargo run -p arbreport --example depth`

use arbnum::D;
use arbpricing::{binomial, Kind, Market};

const SCALE_F: f64 = 1_000_000_000.0;

fn d(v: f64) -> D {
    D::from_raw((v * SCALE_F + 0.5).floor() as i128)
}

fn main() {
    for (s, k, t, sig, r, q) in [
        (0.5f64, 2.0f64, 3.0f64, 1.5f64, -0.02f64, 0.07f64),
        (100.0, 100.0, 1.0, 0.2, 0.05, 0.0),
        (150.0, 150.0, 1.0, 0.45, 0.0, 0.0),
    ] {
        println!("S={s} K={k} T={t} sigma={sig} r={r} q={q}");
        let m = Market { spot: d(s), strike: d(k), t: d(t), sigma: d(sig), rate: d(r), carry: d(q) };
        for steps in [4usize, 8, 16, 32, 64, 128, 256] {
            let amer = binomial(&m, Kind::Put, steps, true).map(|v| v.raw() as f64 / SCALE_F);
            let euro = binomial(&m, Kind::Put, steps, false).map(|v| v.raw() as f64 / SCALE_F);
            println!("  n={steps:>4}  american={amer:?}  european={euro:?}");
        }
    }
}
