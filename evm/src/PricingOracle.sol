// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IArboretum} from "./IArboretum.sol";

/// @title PricingOracle
/// @notice What an integrator actually builds: a thin consumer that turns the pricing
/// primitive into the two things a protocol needs — a mark it can quote, and the ability
/// to check somebody else's mark against it.
///
/// The interesting function is `assertQuote`. A protocol that receives a price from a
/// market maker, a keeper, or an off-chain model can hand the number to this contract and
/// have it rejected unless the on-chain engine reproduces it inside the engine's own
/// uncertainty band. That is the whole reason to compute on-chain: not to be cheaper than
/// a feed, but to make the number checkable.
///
/// Engine failures propagate. The engine reverts with a short ASCII reason (`domain`,
/// `overflow`, `not_converged`, `inconsistent`), and so does any call that reaches it, so
/// there is no sentinel value to test for here.
contract PricingOracle {
    IArboretum public immutable engine;
    int128 public immutable rate;
    int128 public immutable carry;

    error QuoteOutsideBand(int128 candidate, int128 computed, int128 band);

    constructor(IArboretum engine_, int128 rate_, int128 carry_) {
        engine = engine_;
        rate = rate_;
        carry = carry_;
    }

    /// @notice Price and the width of the engine's own uncertainty, in one call.
    /// A consumer that ignores the band is claiming precision the engine does not offer.
    function quote(int128 spot, int128 strike, int128 t, int128 sigma, bool isPut)
        external
        view
        returns (int128 price, int128 band)
    {
        band = engine.noiseBand(spot, strike, t, sigma, rate, carry);
        price = engine.priceEuropean(spot, strike, t, sigma, rate, carry, isPut);
    }

    /// @notice Collateral value, with early exercise taken into account.
    /// A lender holding tokenised equity against a written option wants this number rather
    /// than the European one, because the American value is what it costs to close now.
    function collateralValue(int128 spot, int128 strike, int128 t, int128 sigma, uint32 steps)
        external
        view
        returns (int128)
    {
        uint32 capped = steps > engine.maxLatticeSteps() ? engine.maxLatticeSteps() : steps;
        return engine.priceLattice(spot, strike, t, sigma, rate, carry, true, true, capped);
    }

    /// @notice Reverts unless `candidate` is reproducible by the engine within its band.
    /// @dev The integration point: any price, from any source, becomes verifiable.
    function assertQuote(
        int128 candidate,
        int128 spot,
        int128 strike,
        int128 t,
        int128 sigma,
        bool isPut
    ) external view returns (bool) {
        int128 computed = engine.priceEuropean(spot, strike, t, sigma, rate, carry, isPut);
        int128 band = engine.noiseBand(spot, strike, t, sigma, rate, carry);
        int128 diff = candidate > computed ? candidate - computed : computed - candidate;
        if (diff > band) {
            revert QuoteOutsideBand(candidate, computed, band);
        }
        return true;
    }

    /// @notice Volatility implied by a quoted price. Reverts if the quote sits outside the
    /// no-arbitrage band, because no volatility reproduces it.
    function impliedVolFromQuote(int128 price, int128 spot, int128 strike, int128 t, bool isPut)
        external
        view
        returns (int128)
    {
        return engine.impliedVol(price, spot, strike, t, rate, carry, isPut);
    }
}