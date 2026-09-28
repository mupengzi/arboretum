//! The binary target cargo-stylus needs.
//!
//! `cargo stylus deploy` probes the contract's constructor by running the crate with the
//! `export-abi` feature on, which requires a bin target to exist. That is the only reason
//! this file is here.
//!
//! When the feature is off — i.e. in the WASM that actually gets deployed — this compiles
//! down to an empty `main` that the linker discards, and `no_main` keeps the entry
//! convention the Stylus runtime expects.

#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]

#[cfg(not(any(test, feature = "export-abi")))]
#[no_mangle]
pub extern "C" fn main() {}

#[cfg(feature = "export-abi")]
fn main() {
    stylus_sdk::abi::export::print_from_args::<arbcontract::Arboretum>();
}