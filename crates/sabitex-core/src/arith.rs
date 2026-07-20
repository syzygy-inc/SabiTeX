//! Arithmetic with scaled dimensions.
//!
//! A faithful port of tex.web Part 7 (§99-§108). These routines must be
//! followed *exactly* — TeX's output is only reproducible across machines
//! because every implementation rounds the same way (§99). Pascal's `div` /
//! `mod` truncate toward zero for the operand ranges used here, which
//! matches Rust's `/` and `%` on `i32`, so the bodies translate directly.
//!
//! No other module may multiply or divide `Scaled` values with raw
//! operators; addition and subtraction are unchecked, exactly as in tex.web
//! §104 ("the chance of overflow is so remote").

use crate::types::{Scaled, INF_BAD, TWO};

/// `arith_error` and `remainder` (tex.web §104): out-of-band results of the
/// routines below. Lives inside [`crate::engine::Engine`].
#[derive(Default, Debug)]
pub struct ArithState {
    /// Has arithmetic overflow occurred recently?
    pub arith_error: bool,
    /// Amount subtracted to get an exact division.
    pub remainder: Scaled,
}

/// `half(x)` (tex.web §100): half of an integer, unambiguous for odd values.
pub fn half(x: i32) -> i32 {
    if x % 2 != 0 {
        (x + 1) / 2
    } else {
        x / 2
    }
}

/// `round_decimals(k)` (tex.web §102): converts the decimal fraction
/// `.d[0] d[1] ... d[k-1]` (each digit in `0..=9`, `k <= 17`) to a correctly
/// rounded `Scaled`.
pub fn round_decimals(dig: &[u8]) -> Scaled {
    debug_assert!(dig.len() <= 17);
    let mut a: i32 = 0;
    for &d in dig.iter().rev() {
        a = (a + i32::from(d) * TWO) / 10;
    }
    (a + 1) / 2
}

/// `mult_and_add(n, x, y, max_answer)` (tex.web §105): computes `n*x + y`,
/// setting `arith_error` and returning 0 if the magnitude would exceed
/// `max_answer`.
pub fn mult_and_add(
    st: &mut ArithState,
    n: i32,
    x: Scaled,
    y: Scaled,
    max_answer: Scaled,
) -> Scaled {
    let (n, x) = if n < 0 { (-n, -x) } else { (n, x) };
    if n == 0 {
        y
    } else if x <= (max_answer - y) / n && -x <= (max_answer + y) / n {
        n * x + y
    } else {
        st.arith_error = true;
        0
    }
}

/// `nx_plus_y(n, x, y)` (tex.web §105): `n*x + y` with the dimension limit
/// `@'7777777777` (2^30 - 1).
pub fn nx_plus_y(st: &mut ArithState, n: i32, x: Scaled, y: Scaled) -> Scaled {
    mult_and_add(st, n, x, y, 0x3FFF_FFFF)
}

/// `mult_integers(n, x)` (tex.web §105): `n*x` with the integer limit
/// `@'17777777777` (2^31 - 1).
pub fn mult_integers(st: &mut ArithState, n: i32, x: i32) -> i32 {
    mult_and_add(st, n, x, 0, 0x7FFF_FFFF)
}

/// `x_over_n(x, n)` (tex.web §106): truncating division of a scaled value by
/// an integer; the (sign-matched) remainder is left in `st.remainder`.
pub fn x_over_n(st: &mut ArithState, x: Scaled, n: i32) -> Scaled {
    let mut negative = false;
    let (mut x, mut n) = (x, n);
    let result;
    if n == 0 {
        st.arith_error = true;
        st.remainder = x;
        return 0;
    }
    if n < 0 {
        x = -x;
        n = -n;
        negative = true;
    }
    if x >= 0 {
        result = x / n;
        st.remainder = x % n;
    } else {
        result = -((-x) / n);
        st.remainder = -((-x) % n);
    }
    if negative {
        st.remainder = -st.remainder;
    }
    result
}

/// `xn_over_d(x, n, d)` (tex.web §107): `x * n / d` in simulated
/// 1.5-precision arithmetic, where `0 <= n, d <= 2^16` and `d > 0`. The
/// remainder of the division is left in `st.remainder`.
pub fn xn_over_d(st: &mut ArithState, x: Scaled, n: i32, d: i32) -> Scaled {
    let positive = x >= 0;
    let x = if positive { x } else { -x };
    let t = (x % 0o100000) * n;
    let mut u = (x / 0o100000) * n + t / 0o100000;
    let v = (u % d) * 0o100000 + t % 0o100000;
    if u / d >= 0o100000 {
        st.arith_error = true;
    } else {
        u = 0o100000 * (u / d) + v / d;
    }
    if positive {
        st.remainder = v % d;
        u
    } else {
        st.remainder = -(v % d);
        -u
    }
}

/// `badness(t, s)` (tex.web §108): approximates `100 * (t/s)^3` for `t >= 0`,
/// the badness when a total `t` must be made from amounts summing to `s`.
/// Any badness of 2^13 or more is `INF_BAD` (10000).
pub fn badness(t: Scaled, s: Scaled) -> i32 {
    if t == 0 {
        return 0;
    }
    if s <= 0 {
        return INF_BAD;
    }
    // r approximates alpha*t/s, where alpha^3 ~= 100 * 2^18.
    let r = if t <= 7_230_584 {
        (t * 297) / s // 297^3 = 99.94 * 2^18
    } else if s >= 1_663_497 {
        t / (s / 297)
    } else {
        t
    };
    if r > 1290 {
        INF_BAD // 1290^3 < 2^31 < 1291^3
    } else {
        (r * r * r + 0o400000) / 0o1000000 // r^3 / 2^18, rounded
    }
}
