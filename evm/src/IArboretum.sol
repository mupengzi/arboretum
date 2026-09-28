// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title IArboretum
/// @notice The pricing primitive as a Solidity caller sees it.
///
/// Every quantity is fixed point at 1e9: `250.0` is `250_000_000_000`. `t` is in years,
/// `sigma` is annualised volatility as a decimal, `rate` and `carry` are continuously
/// compounded and may be negative. The ABI is `int128` throughout, so no library is
/// needed to encode or decode it.
///
/// Nothing here is trusted for *correctness*: because the engine is deterministic integer
/// arithmetic, a caller can re-derive any number it returns from the same inputs. What the
/// interface offers is the two things an integrator actually needs to reason about a mark:
/// the price, and the width of the representation's own uncertainty.
interface IArboretum {
    /// @notice The fixed-point scale every argument and return value uses.
    function scale() external view returns (int128);

    /// @notice Lattice depth ceiling, so a caller can clamp its own inputs.
    function maxLatticeSteps() external view returns (uint32);

    /// @notice Black-Scholes price with continuous carry.
    function priceEuropean(
        int128 spot,
        int128 strike,
        int128 t,
        int128 sigma,
        int128 rate,
        int128 carry,
        bool isPut
    ) external view returns (int128);

    /// @notice Cox-Ross-Rubinstein price; `american` allows early exercise at every node.
    function priceLattice(
        int128 spot,
        int128 strike,
        int128 t,
        int128 sigma,
        int128 rate,
        int128 carry,
        bool isPut,
        bool american,
        uint32 steps
    ) external view returns (int128);

    /// @notice Invert a quoted price for volatility. Reverts if the quote is outside the
    /// no-arbitrage band, because no volatility reproduces it.
    function impliedVol(
        int128 price,
        int128 spot,
        int128 strike,
        int128 t,
        int128 rate,
        int128 carry,
        bool isPut
    ) external view returns (int128);

    /// @notice One Greek at a time. `which`: 0 delta, 1 gamma, 2 vega, 3 theta, 4 rho.
    function delta(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);
    function gamma(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);
    function vega(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);
    function theta(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);
    function rho(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);

    /// @notice No-arbitrage floor: the discounted forward difference. Not intrinsic value,
    /// because a European option cannot be exercised early.
    function lowerBound(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);

    /// @notice No-arbitrage ceiling: discounted spot for a call, discounted strike for a put.
    function upperBound(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry, bool isPut) external view returns (int128);

    /// @notice How wide the representation's uncertainty is at these parameters. A quote
    /// inside this band is asking for precision the engine does not claim.
    function noiseBand(int128 spot, int128 strike, int128 t, int128 sigma, int128 rate, int128 carry) external view returns (int128);
}