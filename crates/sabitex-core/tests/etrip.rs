//! The e-TRIP test harness (etex.ch Appendix / etripman.tex).
//!
//! Vendored inputs live in `reference/etex/etrip/`. The INITEX pass feeds `*etrip`
//! at the `**` prompt (etrip2.in) with standard input redirected (no
//! terminal echo) and compares the transcript with the reference
//! `etripin.log`. The VIRTEX pass reloads etrip.fmt via `&etrip etrip`
//! (etrip3.in) and compares etrip.log/etrip.fot/etrip.out.
//!
//! Artifacts are written to `target/etrip/` for side-by-side comparison.

use sabitex_core::io::{CaptureTerminal, MemFs};
use sabitex_core::{Engine, Sizes};

fn repo_path(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// The e-TRIP engine parameters (reference/etex/etrip/texmf.cnf + web2c fixed
/// values: mem_bot stays 0, hash_size 15000/8501 as in tex.ch).
fn etrip_sizes() -> Sizes {
    Sizes {
        mem_top: 3999,
        mem_bot: 0,
        pool_size: 32000,
        max_strings: 3300,
        max_print_line: 72,
        error_line: 64,
        half_error_line: 32,
        hash_size: 15000,
        hash_prime: 8501,
        font_max: 75,
        save_size: 600,
        stack_size: 200,
        max_in_open: 20,
        param_size: 60,
        nest_size: 40,
        font_mem_size: 20000,
        buf_size: 500,
        trie_size: 32000,
        trie_op_size: 35111,
        hyph_size: 659,
    }
}

/// etripman.tex: lines that legitimately differ between implementations.
fn masked(line: &str) -> bool {
    line.starts_with("This is ") // banner (engine name/version differ)
        || line.starts_with("**") // the ** prompt echo line in the log
        || line.contains("memory locations dumped")
        || line.contains("strings of total length")
        || line.contains("words of font info")
        || line.contains("strings out of")
        || line.contains("string characters out of")
        || line.contains("Memory usage")
        || line.contains("words of memory out of")
        || line.starts_with("Beginning to dump")
        || line.starts_with(" (preloaded format=")
        // Known divergence: TeX Live e-TeX binaries report one more
        // multiletter control sequence than tex.web + etex.ch produce
        // (the same +1 appears in the vendored etrip.diffs, where trip
        // on e-TeX gives 342 vs Knuth's 341). This port matches the
        // sources: 407. See specification/etrip.md.
        || line.contains("multiletter control sequences")
        // Known divergence: web2c replaces a re-declared \hyphenation
        // exception in place (hyph_link chains); tex.web §940 inserts a
        // second entry. etrip re-declares qq-B-pp for language 3, so the
        // final count is 11 here vs 10 in the reference. The looked-up
        // hyphen positions are identical. See specification/etrip.md.
        || line.contains("hyphenation exceptions out of")
}

/// Blocks in the reference that legitimately do not occur in this
/// engine. Each entry is the first line of the block and the number of
/// lines to drop; see specification/etrip.md.
const LEGIT_REF_BLOCKS: &[(&str, usize)] = &[
    // Unicode-wide character codes (XeTeX semantics): 256 is a valid
    // character number here, so the §434-equivalent error does not fire.
    ("! Bad character code (256).", 6),
];

/// Drops masked lines. The reference (TeX Live e-TeX) and this engine
/// print `\u{0100}` handling identically in extended mode, so no
/// escapechar masking is needed here yet; entries land as the grind
/// uncovers them.
fn normalize(log: &str, is_reference: bool) -> Vec<String> {
    let mut lines: Vec<&str> = log.lines().collect();
    if is_reference {
        for (first, n) in LEGIT_REF_BLOCKS {
            while let Some(i) = lines.iter().position(|l| l == first) {
                lines.drain(i..(i + n).min(lines.len()));
            }
        }
    }
    lines
        .into_iter()
        .filter(|l| !masked(l))
        // kpathsea opens ./etrip.tex; this engine's TexFs resolves plain
        // names, so the reference's "./" prefixes are stripped.
        .map(|l| l.replace("(./", "(").replace("`./", "`"))
        .collect()
}

/// Compares a transcript against the reference, returning the number of
/// unmasked differences (and reporting the first few).
fn compare(what: &str, ours_raw: &str, reference: &str) -> usize {
    let ours = normalize(ours_raw, false);
    let knuth = normalize(reference, true);
    let mut diffs = 0;
    for (i, (a, b)) in ours.iter().zip(&knuth).enumerate() {
        if a != b {
            diffs += 1;
            if diffs <= 10 {
                eprintln!("{what} line {}:\n  ours: {a}\n  ref:  {b}", i + 1);
            }
        }
    }
    if ours.len() != knuth.len() {
        eprintln!(
            "{what} line counts: ours {} vs ref {}",
            ours.len(),
            knuth.len()
        );
        diffs += 1;
    }
    diffs
}

#[test]
fn etrip_virtex_pass() {
    let etrip_tex = std::fs::read(repo_path("reference/etex/etrip/etrip.tex"))
        .expect("reference/etex/etrip vendored");
    let etrip_tfm = std::fs::read(repo_path("reference/etex/etrip/etrip.tfm")).expect("etrip.tfm");
    let reference = std::fs::read_to_string(repo_path("reference/etex/etrip/etrip.log")).unwrap();

    // Pass 1 (INITEX): produce etrip.fmt.
    let mut fs = MemFs::default();
    fs.files.insert("etrip.tex".to_string(), etrip_tex.clone());
    fs.files.insert("etrip.tfm".to_string(), etrip_tfm.clone());
    let (term, _) = CaptureTerminal::new(vec!["*etrip".to_string()]);
    let mut e1 = Engine::new(etrip_sizes(), Box::new(fs), Box::new(term));
    e1.terminal_echo = false;
    e1.mem.glue_ratio_wide = true;
    e1.run_terminal_job().expect("INITEX pass completes");
    let fmt = e1.take_output("etrip.fmt").expect("etrip.fmt dumped");

    // Pass 2 (VIRTEX): `&etrip etrip` with redirected stdin (etrip3.in).
    let mut fs = MemFs::default();
    fs.files.insert("etrip.tex".to_string(), etrip_tex);
    fs.files.insert("etrip.tfm".to_string(), etrip_tfm);
    let (term, out) = CaptureTerminal::new(vec!["&etrip etrip".to_string()]);
    let mut e2 = Engine::new(etrip_sizes(), Box::new(fs), Box::new(term));
    e2.terminal_echo = false;
    e2.mem.glue_ratio_wide = true;
    e2.load_fmt(&fmt).expect("format loads");
    let r = e2.run_terminal_job();
    let log = String::from_utf8_lossy(&e2.log).to_string();
    let fot = out.borrow().clone();

    let dir = repo_path("target/etrip");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("ours-etrip.log"), &log).ok();
    std::fs::write(dir.join("ours-etrip.fot"), &fot).ok();
    if let Err(err) = r {
        panic!("engine aborted during VIRTEX etrip.tex: {err}");
    }
    // dvitype output of this file matches reference/etex/etrip/etrip.typ exactly
    // (banner and DVI-comment date aside); guard the byte count.
    let dvi = e2.take_output("etrip.dvi").expect("etrip.dvi produced");
    std::fs::write(dir.join("ours-etrip.dvi"), &dvi).ok();
    assert_eq!(dvi.len(), 220, "etrip.dvi byte count");
    let outf = e2.take_output("etrip.out").expect("etrip.out produced");
    std::fs::write(dir.join("ours-etrip.out"), &outf).ok();
    let out_ref = std::fs::read(repo_path("reference/etex/etrip/etrip.out")).unwrap();
    assert_eq!(outf, out_ref, "etrip.out differs");

    let diffs = compare("etrip.log", &log, &reference);
    let fot_ref = std::fs::read_to_string(repo_path("reference/etex/etrip/etrip.fot")).unwrap();
    let fot_diffs = compare("etrip.fot", &fot, &fot_ref);
    assert_eq!(diffs + fot_diffs, 0, "unmasked differences remain");
}

#[test]
fn etrip_initex_pass() {
    let etrip_tex = std::fs::read(repo_path("reference/etex/etrip/etrip.tex"))
        .expect("reference/etex/etrip vendored");
    let etrip_tfm = std::fs::read(repo_path("reference/etex/etrip/etrip.tfm"))
        .expect("reference/etex/etrip/etrip.tfm generated from etrip.pl via pltotf");
    let reference = std::fs::read_to_string(repo_path("reference/etex/etrip/etripin.log")).unwrap();

    let mut fs = MemFs::default();
    fs.files.insert("etrip.tex".to_string(), etrip_tex);
    fs.files.insert("etrip.tfm".to_string(), etrip_tfm);
    // etrip2.in: the single line `*etrip` at the ** prompt, stdin redirected.
    let (term, out) = CaptureTerminal::new(vec!["*etrip".to_string()]);
    let mut e = Engine::new(etrip_sizes(), Box::new(fs), Box::new(term));
    e.terminal_echo = false;
    e.mem.glue_ratio_wide = true; // TeX Live glue_ratio is double
    let r = e.run_terminal_job();
    let log = String::from_utf8_lossy(&e.log).to_string();
    let fot = out.borrow().clone();

    let dir = repo_path("target/etrip");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("ours-etripin.log"), &log).ok();
    std::fs::write(dir.join("ours-etripin.fot"), &fot).ok();
    if let Err(err) = r {
        panic!("engine aborted during etrip.tex: {err}");
    }
    assert!(e.take_output("etrip.fmt").is_some(), "etrip.fmt was dumped");

    let diffs = compare("etripin.log", &log, &reference);
    assert_eq!(diffs, 0, "{diffs} unmasked differences against etripin.log");
}
