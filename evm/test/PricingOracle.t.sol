// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {IArboretum} from "../src/IArboretum.sol";
import {PricingOracle} from "../src/PricingOracle.sol";

/// @notice A stand-in for the engine, so the consumer contract can be tested without a
/// deployment. The canned numbers are deliberately round so the assertions read clearly.
contract MockEngine is IArboretum {
    int128 public constant PRICE = 100e9;
    int128 public constant BAND = 5e9;
    uint32 public constant MAX_STEPS = 4096;

    function scale() external pure returns (int128) { return 1e9; }
    function maxLatticeSteps() external pure returns (uint32) { return MAX_STEPS; }

    function priceEuropean(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) {
        return PRICE;
    }

    function priceLattice(int128, int128, int128, int128, int128, int128, bool, bool, uint32 steps)
        external
        pure
        returns (int128)
    {
        // Encodes the depth it was asked for, which lets a test assert the clamp.
        return int128(uint128(steps));
    }

    function impliedVol(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) {
        return 350e6;
    }

    function noiseBand(int128, int128, int128, int128, int128, int128) external pure returns (int128) {
        return BAND;
    }

    function lowerBound(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return 0; }
    function upperBound(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return PRICE; }
    function delta(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return 0; }
    function gamma(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return 0; }
    function vega(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return 0; }
    function theta(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return 0; }
    function rho(int128, int128, int128, int128, int128, int128, bool) external pure returns (int128) { return 0; }
}

contract PricingOracleTest is Test {
    MockEngine engine;
    PricingOracle oracle;

    function setUp() public {
        engine = new MockEngine();
        oracle = new PricingOracle(IArboretum(address(engine)), 0.05e9, 0.01e9);
    }

    // forge-std's assertEq is overloaded on int256 and uint256, so the int128 values the
    // engine speaks have to be widened explicitly or the call is ambiguous.
    function testQuoteReturnsPriceAndBand() public view {
        (int128 price, int128 band) = oracle.quote(250e9, 240e9, 0.25e9, 0.35e9, false);
        assertEq(int256(price), int256(engine.PRICE()));
        assertEq(int256(band), int256(engine.BAND()));
    }

    function testAssertQuoteAcceptsWithinTheBand() public view {
        // Exactly on the price, and at either edge of the band, all pass.
        assertTrue(oracle.assertQuote(engine.PRICE(), 250e9, 240e9, 0.25e9, 0.35e9, false));
        assertTrue(oracle.assertQuote(engine.PRICE() + engine.BAND(), 250e9, 240e9, 0.25e9, 0.35e9, false));
        assertTrue(oracle.assertQuote(engine.PRICE() - engine.BAND(), 250e9, 240e9, 0.25e9, 0.35e9, false));
    }

    function testAssertQuoteRejectsOutsideTheBand() public {
        int128 bad = engine.PRICE() + engine.BAND() + 1;
        vm.expectRevert(
            abi.encodeWithSelector(
                PricingOracle.QuoteOutsideBand.selector, bad, engine.PRICE(), engine.BAND()
            )
        );
        oracle.assertQuote(bad, 250e9, 240e9, 0.25e9, 0.35e9, false);
    }

    function testCollateralValueClampsStepsToTheEngineCeiling() public view {
        assertEq(int256(oracle.collateralValue(250e9, 240e9, 0.25e9, 0.35e9, 512)), int256(512));
        assertEq(
            int256(oracle.collateralValue(250e9, 240e9, 0.25e9, 0.35e9, 99_999)),
            int256(uint256(engine.MAX_STEPS()))
        );
    }

    function testImpliedVolPassesThrough() public view {
        assertEq(
            int256(oracle.impliedVolFromQuote(engine.PRICE(), 250e9, 240e9, 0.25e9, false)),
            int256(350e6)
        );
    }
}