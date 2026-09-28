//! Format files as a producer/consumer pair (C-JOB, C-RESOURCE; T1/T2).
//!
//! A format dumped by one engine is loaded by a fresh engine, and the
//! macros and registers it carried are observed through the job that
//! follows — not by comparing bytes. Corrupt, truncated and mismatched
//! formats are normal errors, never panics, and an engine whose load
//! failed refuses to run a job (its arenas hold an unspecified mixture of
//! old and new state).

use std::panic::{catch_unwind, AssertUnwindSafe};

use sabitex_core::io::{CaptureTerminal, MemFs};
use sabitex_core::{Engine, Sizes};

const SOURCE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \
                      \\def\\hello#1{HELLO(#1)}\\count1=42 \\dimen2=3pt \\dump\n";

/// INITEX pass: defines a macro and two registers, then `\dump`s.
fn dump() -> Vec<u8> {
    let mut fs = MemFs::default();
    fs.files
        .insert("job.tex".to_string(), SOURCE.as_bytes().to_vec());
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e.run_file("job")
        .unwrap_or_else(|err| panic!("INITEX pass failed: {err}\n{}", out.borrow()));
    e.take_output("job.fmt").expect("job.fmt dumped")
}

fn fresh(script: &[&str]) -> (Engine, std::rc::Rc<std::cell::RefCell<String>>) {
    let (term, out) = CaptureTerminal::new(script.iter().map(|s| s.to_string()));
    let e = Engine::new(Sizes::default(), Box::new(MemFs::default()), Box::new(term));
    (e, out)
}

#[test]
fn a_dumped_format_restores_macros_and_registers_in_a_fresh_engine() {
    let fmt = dump();
    let (mut e, out) = fresh(&[
        "&job \\showthe\\count1 \\showthe\\dimen2 \\show\\hello \\message{\\hello{x}}\\end",
    ]);
    e.load_fmt(&fmt).expect("format loads");
    // The format carries the interaction mode it was dumped in (§1327), so
    // set nonstopmode after loading: \show must not wait for the terminal.
    e.set_interaction(1);
    e.run_terminal_job()
        .unwrap_or_else(|err| panic!("VIRTEX job failed: {err}\n{}", out.borrow()));
    // No file was \input, so no transcript is opened: \show and \message
    // go to the terminal. Look at both channels.
    let log = format!("{}\n{}", out.borrow(), String::from_utf8_lossy(&e.log));
    for needle in [
        "> 42.",
        "> 3.0pt.",
        "> \\hello=macro:",
        "->HELLO(#1).",
        "HELLO(x)",
    ] {
        assert!(log.contains(needle), "missing {needle:?} in\n{log}");
    }
}

/// T1: truncation anywhere is a normal error, and no panic.
#[test]
fn truncated_formats_are_errors_not_panics() {
    let fmt = dump();
    let cuts = [
        0,
        5,
        11,
        12,
        51,
        60,
        fmt.len() / 3,
        fmt.len() / 2,
        fmt.len() - 1,
    ];
    for cut in cuts {
        let (mut e, _) = fresh(&[]);
        let r = catch_unwind(AssertUnwindSafe(|| e.load_fmt(&fmt[..cut]).is_err()));
        assert!(matches!(r, Ok(true)), "cut at {cut}: {r:?}");
    }
}

/// T1: hostile lengths inside the file are normal errors (the codec checks
/// them against the bytes that remain before allocating).
#[test]
fn hostile_lengths_are_errors_not_panics() {
    let fmt = dump();
    // After the 11-byte magic, 10 constants (40 bytes) and the eTeX byte,
    // the string pool length is the first sequence length.
    let pool_len_at = 11 + 40 + 1;
    for (name, patch) in [
        ("u64::MAX", u64::MAX),
        ("i64::MAX", i64::MAX as u64),
        ("2^32", 1u64 << 32),
        ("file length", fmt.len() as u64),
    ] {
        let mut bad = fmt.clone();
        bad[pool_len_at..pool_len_at + 8].copy_from_slice(&patch.to_le_bytes());
        let (mut e, _) = fresh(&[]);
        let r = catch_unwind(AssertUnwindSafe(|| e.load_fmt(&bad).is_err()));
        assert!(matches!(r, Ok(true)), "{name}: {r:?}");
    }
    // Every length field in the file set to u64::MAX one at a time (a
    // coarse sweep: every 8-byte aligned position after the header).
    let mut checked = 0;
    let mut pos = pool_len_at;
    while pos + 8 <= fmt.len() {
        let mut bad = fmt.clone();
        bad[pos..pos + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        let (mut e, _) = fresh(&[]);
        let r = catch_unwind(AssertUnwindSafe(|| {
            let _ = e.load_fmt(&bad);
        }));
        assert!(r.is_ok(), "panic with u64::MAX at byte {pos}");
        checked += 1;
        pos += 8 * 97; // sample positions across the file
    }
    assert!(checked > 3);
}

/// A format made with different constants, or with garbage, is refused.
#[test]
fn mismatched_and_garbage_formats_are_refused() {
    let fmt = dump();
    let mut sizes = Sizes::default();
    sizes.mem_top += 1000;
    let (term, _) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(sizes, Box::new(MemFs::default()), Box::new(term));
    assert!(e.load_fmt(&fmt).is_err(), "different constants");
    let (mut e, _) = fresh(&[]);
    assert!(e.load_fmt(b"not a format").is_err());
    let (mut e, _) = fresh(&[]);
    let mut wrong_check = fmt.clone();
    let n = wrong_check.len();
    wrong_check[n - 4..].copy_from_slice(&0i32.to_le_bytes());
    assert!(e.load_fmt(&wrong_check).is_err(), "check word");
    let (mut e, _) = fresh(&[]);
    let mut trailing = fmt.clone();
    trailing.push(0);
    assert!(e.load_fmt(&trailing).is_err(), "trailing garbage");
}

/// T2 (core side): after a failed load the engine refuses to run a job and
/// to load again; a fresh engine is the only way back.
#[test]
fn an_engine_whose_format_load_failed_refuses_to_run() {
    let fmt = dump();
    let (mut e, out) = fresh(&["\\end"]);
    assert!(e.load_fmt(&fmt[..fmt.len() / 2]).is_err());
    assert!(e.fmt_poisoned);
    let err = e.run_terminal_job().expect_err("poisoned engine ran a job");
    assert!(
        err.to_string()
            .contains("unusable after a failed format load"),
        "{err}
{}",
        out.borrow()
    );
    assert!(
        e.load_fmt(&fmt).is_err(),
        "no second chance on the same engine"
    );
    // A fresh engine with the same bytes works.
    let (mut e, _) = fresh(&["\\end"]);
    e.load_fmt(&fmt).expect("format loads");
    e.run_terminal_job().expect("job runs");
}
