//! Golden tests for tex.web Part 7 (scaled arithmetic). Expected values are
//! derived by hand-executing the Pascal in tex.web §100-§108; these results
//! are what every TeX82 implementation must produce bit-for-bit.

use sabitex_core::arith::{
    badness, half, mult_and_add, mult_integers, nx_plus_y, round_decimals, x_over_n, xn_over_d,
    ArithState,
};
use sabitex_core::types::{Scaled, INF_BAD, UNITY};

#[test]
fn half_uses_unambiguous_rounding_for_odd_numbers() {
    // §100: if odd(x) then (x+1) div 2 else x div 2.
    assert_eq!(half(0), 0);
    assert_eq!(half(1), 1);
    assert_eq!(half(-1), 0);
    assert_eq!(half(3), 2);
    assert_eq!(half(-3), -1);
    assert_eq!(half(4), 2);
    assert_eq!(half(-4), -2);
    assert_eq!(half(7), 4);
    assert_eq!(half(-7), -3);
}

#[test]
fn round_decimals_golden() {
    // §102. ".5" -> 0.5 = 32768 sp, ".25" -> 16384, ".1" -> 6554 (rounded up).
    assert_eq!(round_decimals(&[]), 0);
    assert_eq!(round_decimals(&[5]), 32768);
    assert_eq!(round_decimals(&[2, 5]), 16384);
    assert_eq!(round_decimals(&[1]), 6554);
    assert_eq!(round_decimals(&[0]), 0);
    assert_eq!(round_decimals(&[9]), 58982); // 0.9*65536 = 58982.4
                                             // Seventeen nines must round up to exactly 1.0 (= unity).
    assert_eq!(round_decimals(&[9; 17]), UNITY);
    // "0.00001" — smallest printable increment territory.
    assert_eq!(round_decimals(&[0, 0, 0, 0, 1]), 1);
    // print_scaled/round_decimals round trip is exercised in print_scaled.rs.
}

#[test]
fn mult_and_add_family() {
    let st = &mut ArithState::default();

    // nx_plus_y within range.
    assert_eq!(nx_plus_y(st, 2, 3 * UNITY, UNITY), 7 * UNITY);
    assert!(!st.arith_error);
    // n = 0 returns y untouched.
    assert_eq!(nx_plus_y(st, 0, 12345, -678), -678);
    // Negative n negates x (§105).
    assert_eq!(nx_plus_y(st, -2, 3 * UNITY, UNITY), -5 * UNITY);
    assert!(!st.arith_error);

    // Overflow past max_dimen (2^30 - 1) sets arith_error and yields 0.
    assert_eq!(nx_plus_y(st, 2, 0x3FFF_FFFF, 0), 0);
    assert!(st.arith_error);
    st.arith_error = false;

    // Exactly at the limit is fine.
    assert_eq!(nx_plus_y(st, 1, 0x3FFF_FFFE, 1), 0x3FFF_FFFF);
    assert!(!st.arith_error);

    // mult_integers uses the 2^31 - 1 limit: 46341^2 = 2147488281 > 2^31-1.
    assert_eq!(mult_integers(st, 46341, 46341), 0);
    assert!(st.arith_error);
    st.arith_error = false;
    assert_eq!(mult_integers(st, 46340, 46340), 2_147_395_600);
    assert!(!st.arith_error);

    // max_answer boundary behavior of mult_and_add itself.
    assert_eq!(mult_and_add(st, 3, 10, 5, 35), 35);
    assert!(!st.arith_error);
    assert_eq!(mult_and_add(st, 3, 10, 6, 35), 0);
    assert!(st.arith_error);
}

#[test]
fn x_over_n_truncates_toward_zero_with_signed_remainder() {
    let st = &mut ArithState::default();

    assert_eq!(x_over_n(st, 10, 3), 3);
    assert_eq!(st.remainder, 1);
    assert_eq!(x_over_n(st, -10, 3), -3);
    assert_eq!(st.remainder, -1);
    // §106: negative n negates x and the remainder.
    assert_eq!(x_over_n(st, 10, -3), -3);
    assert_eq!(st.remainder, 1);
    assert_eq!(x_over_n(st, -10, -3), 3);
    assert_eq!(st.remainder, -1);
    assert_eq!(x_over_n(st, 0, 5), 0);
    assert_eq!(st.remainder, 0);

    // Division by zero: error, result 0, remainder = x.
    assert!(!st.arith_error);
    assert_eq!(x_over_n(st, 42, 0), 0);
    assert_eq!(st.remainder, 42);
    assert!(st.arith_error);
}

#[test]
fn xn_over_d_is_exact_1_5_precision() {
    let st = &mut ArithState::default();

    // unity * 297 / 100 = 19464192 / 100 = 194641 remainder 92.
    assert_eq!(xn_over_d(st, UNITY, 297, 100), 194_641);
    assert_eq!(st.remainder, 92);
    assert!(!st.arith_error);

    // Sign handling: result and remainder are negated together (§107).
    assert_eq!(xn_over_d(st, -UNITY, 297, 100), -194_641);
    assert_eq!(st.remainder, -92);

    // Identity: n = d leaves x unchanged, remainder 0.
    assert_eq!(xn_over_d(st, 123_456_789, 1000, 1000), 123_456_789);
    assert_eq!(st.remainder, 0);

    // Large x near max_dimen with the magnification ratio used by \mag.
    // (2^30-1) * 1000 / 1000 must not overflow intermediate quantities.
    assert_eq!(xn_over_d(st, 0x3FFF_FFFF, 1000, 1000), 0x3FFF_FFFF);
    assert!(!st.arith_error);

    // Overflow: u div d >= 2^15 sets arith_error.
    assert_eq!(xn_over_d(st, 0x3FFF_FFFF, 2, 1), {
        // x div '100000 * n = 32767*2 = 65534 >= 32768 -> error; u keeps its
        // pre-correction value per §107 (result is garbage but deterministic).
        65534 + 1 // t div '100000 contribution: t = 32767*2 = 65534, div 32768 = 1
    });
    assert!(st.arith_error);
    st.arith_error = false;

    // sp-per-inch conversion used by \dimen units: 72.27 pt/in style ratios.
    // 65536 * 7227 / 100 = 4736286.72 -> 4736286 remainder 72.
    assert_eq!(xn_over_d(st, UNITY, 7227, 100), 4_736_286);
    assert_eq!(st.remainder, 72);
}

#[test]
fn badness_matches_tex82() {
    // §108 boundary structure.
    assert_eq!(badness(0, 0), 0); // t = 0 -> 0
    assert_eq!(badness(0, -5), 0);
    assert_eq!(badness(1, 0), INF_BAD); // s <= 0 -> inf_bad
    assert_eq!(badness(1, -1), INF_BAD);

    // t = s -> r = 297 -> badness 100 (the canonical "100 (t/s)^3").
    assert_eq!(badness(100, 100), 100);
    assert_eq!(badness(UNITY, UNITY), 100);
    // t = 2s -> 800; t = s/2 -> 12 ((297/2)^3 + 2^17) div 2^18 = 12.
    assert_eq!(badness(2 * UNITY, UNITY), 800);
    assert_eq!(badness(UNITY, 2 * UNITY), 12);

    // Tiny ratios.
    assert_eq!(badness(1, 100_000), 0);

    // r > 1290 -> inf_bad.
    assert_eq!(badness(1291, 297), INF_BAD); // r = 1291*297/297... = 1291
    assert_eq!(
        badness(1290, 297),
        (1290i64.pow(3) as Scaled + 0o400000) / 0o1000000
    );

    // The three branches of r:
    // (a) t <= 7230584 uses (t*297) div s.
    assert_eq!(badness(7_230_584, 7_230_584), 100);
    // (b) t > 7230584, s >= 1663497 uses t div (s div 297):
    //     r = 7230585 div (7230585 div 297) = 7230585 div 24345 = 297 -> 100.
    assert_eq!(badness(7_230_585, 7_230_585), 100);
    // (c) t > 7230584, s < 1663497: r = t > 1290 -> inf_bad.
    assert_eq!(badness(7_230_585, 1_663_496), INF_BAD);

    // Monotonicity spot check: badness(t+1,s) >= badness(t,s) >= badness(t,s+1).
    for &(t, s) in &[(100, 99), (5000, 4999), (65536, 32768), (7_230_584, 65536)] {
        assert!(badness(t + 1, s) >= badness(t, s));
        assert!(badness(t, s) >= badness(t, s + 1));
    }
}
