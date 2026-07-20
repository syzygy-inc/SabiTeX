//! Tests for the printing routines of tex.web Part 5 and `print_scaled`
//! (§103). Output is captured TeX-natively by switching `selector` to
//! `new_string` and reading back the pool string, plus a terminal-capture
//! check for line breaking.

use sabitex_core::arith::round_decimals;
use sabitex_core::io::{CaptureTerminal, MemFs};
use sabitex_core::print::{NEW_STRING, TERM_ONLY};
use sabitex_core::types::{Scaled, UNITY};
use sabitex_core::{Engine, Sizes};

fn engine() -> Engine {
    let (term, _) = CaptureTerminal::new(Vec::new());
    Engine::new(Sizes::default(), Box::<MemFs>::default(), Box::new(term))
}

/// Runs `f` with printing deflected to the string pool and returns the text.
fn captured(engine: &mut Engine, f: impl FnOnce(&mut Engine)) -> String {
    let saved = engine.prn.selector;
    engine.prn.selector = NEW_STRING;
    f(engine);
    engine.prn.selector = saved;
    let s = engine.make_string().unwrap();
    let text = engine.strings.text(s);
    engine.strings.flush_string();
    text
}

#[test]
fn print_scaled_golden() {
    let e = &mut engine();
    let cases: &[(Scaled, &str)] = &[
        (0, "0.0"),
        (UNITY, "1.0"),
        (-UNITY, "-1.0"),
        (UNITY / 2, "0.5"),
        (UNITY + UNITY / 2, "1.5"),
        (6554, "0.1"),  // round_decimals(.1) = 6554 prints back as 0.1
        (1, "0.00002"), // one scaled point
        (2, "0.00003"),
        (65535, "0.99998"),           // unity - 1
        (0x3FFF_FFFF, "16383.99998"), // \maxdimen
        (-0x3FFF_FFFF, "-16383.99998"),
        (4_736_286, "72.26999"), // 72.27pt - 1sp territory (4736286.72 sp/in)
        (4_736_287, "72.27"),
    ];
    for &(s, want) in cases {
        assert_eq!(
            captured(e, |e| e.print_scaled(s)),
            want,
            "print_scaled({s})"
        );
    }
}

#[test]
fn print_scaled_round_trips_through_round_decimals() {
    // §103: if the printed digits are fed back to round_decimals, the
    // original scaled value is reproduced exactly. Sweep interesting values.
    let e = &mut engine();
    let mut samples: Vec<Scaled> = vec![0, 1, 2, 3, 65535, 65536, 65537, 0x3FFF_FFFF];
    for k in 0..16 {
        samples.push((1 << k) + k);
        samples.push(6554 * k + 1);
    }
    for &s in &samples {
        let text = captured(e, |e| e.print_scaled(s));
        let (int_part, frac_part) = text.split_once('.').unwrap();
        let digits: Vec<u8> = frac_part.bytes().map(|b| b - b'0').collect();
        assert!(!digits.is_empty(), "at least one digit after the point");
        assert!(digits.len() <= 5, "rounded to five digits: {text}");
        let rebuilt = int_part.parse::<Scaled>().unwrap() * UNITY + round_decimals(&digits);
        assert_eq!(rebuilt, s, "round trip of {s} via {text}");
    }
}

#[test]
fn print_int_handles_extremes() {
    let e = &mut engine();
    assert_eq!(captured(e, |e| e.print_int(0)), "0");
    assert_eq!(captured(e, |e| e.print_int(42)), "42");
    assert_eq!(captured(e, |e| e.print_int(-42)), "-42");
    assert_eq!(captured(e, |e| e.print_int(i32::MAX)), "2147483647");
    // §65 is written so that -2^31 (whose negation overflows) works.
    assert_eq!(captured(e, |e| e.print_int(i32::MIN)), "-2147483648");
    assert_eq!(captured(e, |e| e.print_int(-100_000_000)), "-100000000");
    assert_eq!(captured(e, |e| e.print_int(-99_999_999)), "-99999999");
}

#[test]
fn print_hex_two_roman() {
    let e = &mut engine();
    assert_eq!(captured(e, |e| e.print_hex(0)), "\"0");
    assert_eq!(captured(e, |e| e.print_hex(255)), "\"FF");
    assert_eq!(captured(e, |e| e.print_hex(0xABCDEF)), "\"ABCDEF");

    assert_eq!(captured(e, |e| e.print_two(7)), "07");
    assert_eq!(captured(e, |e| e.print_two(1990)), "90");
    assert_eq!(captured(e, |e| e.print_two(-3)), "03");

    // §69: 1990 yields mcmxc, not mxm.
    assert_eq!(captured(e, |e| e.print_roman_int(1990)), "mcmxc");
    assert_eq!(captured(e, |e| e.print_roman_int(2026)), "mmxxvi");
    assert_eq!(captured(e, |e| e.print_roman_int(49)), "xlix");
    assert_eq!(captured(e, |e| e.print_roman_int(4)), "iv");
    assert_eq!(captured(e, |e| e.print_roman_int(0)), "");
}

#[test]
fn single_character_strings_use_hat_hat_notation() {
    // §48-§49: unprintable characters print as ^^X / ^^xx. (Only when
    // selector <= pseudo: §59 prints raw characters above pseudo, so this
    // test captures the terminal instead of the string pool.)
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::<MemFs>::default(), Box::new(term));
    e.prn.selector = TERM_ONLY;
    // INITEX's \newlinechar is 0 (§240), which would turn char 0 into a
    // newline (§244); disable it to observe the ^^ forms themselves.
    let p = e.eqtb.lay.int_base + sabitex_core::eqtb::NEW_LINE_CHAR_CODE;
    e.eqtb.set_int(p, -1);
    for c in [b'a' as i32, 13, 127, 0, 128, 255] {
        e.print(c);
        e.print_char(b' ' as i32);
    }
    assert_eq!(*out.borrow(), "a ^^M ^^? ^^@ ^^80 ^^ff ");
}

#[test]
fn term_lines_wrap_at_max_print_line() {
    let (term, out) = CaptureTerminal::new(Vec::new());
    let sizes = Sizes {
        max_print_line: 10,
        ..Sizes::default()
    };
    let mut e = Engine::new(sizes, Box::<MemFs>::default(), Box::new(term));
    e.prn.selector = TERM_ONLY;
    e.print_chars("abcdefghijklmnop"); // 16 chars: wraps after 10
    e.print_ln();
    assert_eq!(*out.borrow(), "abcdefghij\nklmnop\n");
    assert_eq!(e.prn.tally, 16);
}

#[test]
fn print_nl_only_breaks_when_mid_line() {
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::<MemFs>::default(), Box::new(term));
    e.prn.selector = TERM_ONLY;
    e.print_nl_chars("first"); // at line start: no leading newline
    e.print_nl_chars("second"); // mid-line: breaks first
    assert_eq!(*out.borrow(), "first\nsecond");
}
