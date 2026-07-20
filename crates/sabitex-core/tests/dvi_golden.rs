//! M2 acceptance tests: DVI output must be byte-identical to Knuth's tex.
//!
//! Each test runs the same source through (a) this engine with cmr10.tfm
//! provided in a memory file system, and (b) the installed TeX Live `tex`
//! in INITEX mode, then compares the DVI files byte for byte. The date
//! parameters are pinned in the source so the DVI preamble comment matches.
//!
//! The tests skip (with a note) when no `tex`/`kpsewhich` is on PATH.

use std::process::Command;

use sabitex_core::io::{CaptureTerminal, MemFs};
use sabitex_core::{Engine, Sizes};

const PREAMBLE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \
                        \\year=1776 \\month=7 \\day=4 \\time=720 ";

/// Locates a TFM file via kpsewhich; None if TeX Live is unavailable.
fn kpsewhich(name: &str) -> Option<Vec<u8>> {
    let out = Command::new("kpsewhich").arg(name).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    std::fs::read(path).ok()
}

/// Runs `src` through Knuth's tex (INITEX); returns the DVI bytes.
fn reference_dvi(src: &str, tag: &str) -> Option<Vec<u8>> {
    let dir = std::env::temp_dir().join(format!("sabitex-golden-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("test.tex"), src).ok()?;
    let out = Command::new("tex")
        .args(["-ini", "-interaction=batchmode", "test.tex"])
        .current_dir(&dir)
        .output()
        .ok()?;
    let _ = out;
    let dvi = std::fs::read(dir.join("test.dvi")).ok();
    let _ = std::fs::remove_dir_all(&dir);
    dvi
}

/// Runs `src` through SabiTeX; returns the DVI bytes.
fn sabitex_dvi(src: &str, tfms: &[&str]) -> Vec<u8> {
    let mut fs = MemFs::default();
    fs.files
        .insert("test.tex".to_string(), src.as_bytes().to_vec());
    for tfm in tfms {
        let bytes = kpsewhich(&format!("{tfm}.tfm")).expect("tfm available");
        fs.files.insert(format!("{tfm}.tfm"), bytes);
    }
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let r = e.run_file("test.tex");
    if let Err(err) = r {
        panic!("engine failed: {err}\nterminal: {}", out.borrow());
    }
    // Retrieve the written DVI from the engine's file system.
    // (MemFs is owned by the engine; downcast via the outputs map carried
    // through write_file is not exposed, so go through a second channel.)
    let term_out = out.borrow().clone();
    match e.take_output("test.dvi") {
        Some(d) => d,
        None => panic!("no DVI produced; terminal: {term_out}"),
    }
}

fn compare(src_body: &str, tfms: &[&str], tag: &str) {
    let src = format!("{PREAMBLE}{src_body}");
    let Some(reference) = reference_dvi(&src, tag) else {
        eprintln!("SKIPPED ({tag}): TeX Live not available");
        return;
    };
    let ours = sabitex_dvi(&src, tfms);
    if ours != reference {
        // Find the first difference for the report.
        let n = ours
            .iter()
            .zip(&reference)
            .take_while(|(a, b)| a == b)
            .count();
        panic!(
            "DVI mismatch at byte {n} (ours len {}, ref len {}): \
             ours[{n}..{}]={:?} ref[{n}..{}]={:?}",
            ours.len(),
            reference.len(),
            (n + 16).min(ours.len()),
            &ours[n..(n + 16).min(ours.len())],
            (n + 16).min(reference.len()),
            &reference[n..(n + 16).min(reference.len())],
        );
    }
}

/// A `TexFs` that serves `test.tex` from memory and everything else from
/// the installed TeX Live via kpsewhich (plain.tex, hyphen.tex, TFMs, ...).
#[derive(Default)]
struct KpseFs {
    files: std::collections::BTreeMap<String, Vec<u8>>,
    outputs: std::collections::BTreeMap<String, Vec<u8>>,
}

impl sabitex_core::io::TexFs for KpseFs {
    fn read_file(&mut self, name: &str, kind: sabitex_core::io::FileKind) -> Option<Vec<u8>> {
        if let Some(data) = self.files.get(name) {
            return Some(data.clone());
        }
        let _ = kind;
        kpsewhich(name)
    }

    fn write_file(&mut self, name: &str, _kind: sabitex_core::io::OutKind, data: &[u8]) -> bool {
        self.outputs.insert(name.to_string(), data.to_vec());
        true
    }

    fn take_output(&mut self, name: &str) -> Option<Vec<u8>> {
        self.outputs.remove(name)
    }
}

/// Runs `src` through SabiTeX with kpsewhich-backed file lookups.
fn sabitex_dvi_kpse(src: &str) -> Vec<u8> {
    let mut fs = KpseFs::default();
    fs.files
        .insert("test.tex".to_string(), src.as_bytes().to_vec());
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let r = e.run_file("test.tex");
    if let Err(err) = r {
        panic!("engine failed: {err}\nterminal: {}", out.borrow());
    }
    let term_out = out.borrow().clone();
    match e.take_output("test.dvi") {
        Some(d) => d,
        None => panic!("no DVI produced; terminal: {term_out}"),
    }
}

/// The M3 acceptance comparison: kpsewhich-backed inputs on both sides.
fn compare_kpse(src_body: &str, tag: &str) {
    let src = format!("{PREAMBLE}{src_body}");
    if kpsewhich("plain.tex").is_none() {
        eprintln!("SKIPPED ({tag}): TeX Live not available");
        return;
    }
    let Some(reference) = reference_dvi(&src, tag) else {
        eprintln!("SKIPPED ({tag}): TeX Live not available");
        return;
    };
    let ours = sabitex_dvi_kpse(&src);
    if ours != reference {
        let n = ours
            .iter()
            .zip(&reference)
            .take_while(|(a, b)| a == b)
            .count();
        panic!(
            "DVI mismatch at byte {n} (ours len {}, ref len {}): \
             ours[{n}..{}]={:?} ref[{n}..{}]={:?}",
            ours.len(),
            reference.len(),
            (n + 16).min(ours.len()),
            &ours[n..(n + 16).min(ours.len())],
            (n + 16).min(reference.len()),
            &reference[n..(n + 16).min(reference.len())],
        );
    }
}

#[test]
fn empty_hbox_page() {
    compare("\\shipout\\hbox{}\\end", &[], "empty");
}

#[test]
fn paragraph_line_breaking_and_page_builder() {
    compare(
        "\\font\\tf=cmr10 \\tf \\hsize=200pt \\vsize=400pt \\parindent=20pt \
         \\baselineskip=12pt \\parfillskip=0pt plus 1fil \
         Once upon a time, in a distant galaxy, there lived a computer named \
         R. J. Drofnats. He was happiest when he was at work typesetting \
         beautiful documents, day and night, for everyone in the galaxy.\\par \
         \\end",
        &["cmr10"],
        "paragraph",
    );
}

/// The M3 acceptance test: story.tex through plain.tex, byte-identical DVI.
#[test]
fn story_dvi_matches_knuth_tex() {
    compare_kpse("\\input plain \\input story \\end", "story");
}

/// M4: inline and display math — fractions, radicals, delimiters, limits.
#[test]
fn math_dvi_matches_knuth_tex() {
    compare_kpse(
        "\\input plain \\hsize=200pt \\vsize=400pt \
         Test $a+b = c^2 - {x \\over y}$ and \
         $$\\sqrt{x+1} \\left( {a \\over b} \\right) \\sum_{k=1}^n k$$ \
         then $f(x) \\mathrel{\\mathop=^{\\rm def}} x_0$, \
         $\\underline{u}\\overline{v}$, $\\vec\\imath$, \\hbox{$y\\!z$}, \
         and $$\\displaylines{a=b\\cr c=d\\cr}$$ done. \\end",
        "math",
    );
}

/// M4: \halign with spans, \noalign, \omit, and a display alignment.
#[test]
fn alignment_dvi_matches_knuth_tex() {
    compare_kpse(
        "\\input plain \\hsize=250pt \\vsize=400pt \\tabskip=10pt \
         \\halign{\\hfil#&\\bf #\\hfil&$#$\\hfil\\cr one&two&x+y\\cr \
         a longer entry&b&z^2\\cr \\noalign{\\hrule} \
         span test\\span ned&c\\cr \\omit wide&d&e\\cr} \
         After the alignment. $$\\eqalign{a&=b+c\\cr d&=e\\cr}$$ \
         \\settabs 3 \\columns \\+ tab&one&two \\cr \\end",
        "align",
    );
}

/// M5: `\dump` a plain format, reload it in a fresh engine, and verify
/// that story.tex still comes out byte-identical to Knuth's tex.
#[test]
fn format_dump_and_load_roundtrip() {
    if kpsewhich("plain.tex").is_none() {
        eprintln!("SKIPPED (fmt): TeX Live not available");
        return;
    }
    let src = format!("{PREAMBLE}\\input plain \\input story \\end");
    let Some(reference) = reference_dvi(&src, "fmtstory") else {
        eprintln!("SKIPPED (fmt): TeX Live not available");
        return;
    };
    // 1) INITEX pass: load plain.tex and \dump.
    let mut fs = KpseFs::default();
    fs.files.insert(
        "plain-dump.tex".to_string(),
        b"\\input plain \\dump".to_vec(),
    );
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e1 = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    if let Err(err) = e1.run_file("plain-dump.tex") {
        panic!("dump pass failed: {err}\nterminal: {}", out.borrow());
    }
    let fmt = e1
        .take_output("plain-dump.fmt")
        .expect("a format file is produced by \\dump");
    // 2) production pass: load the format, then typeset story.tex.
    let mut fs = KpseFs::default();
    fs.files.insert(
        "test.tex".to_string(),
        format!("{PREAMBLE}\\input story \\end").into_bytes(),
    );
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e2 = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e2.load_fmt(&fmt).expect("format loads");
    if let Err(err) = e2.run_file("test.tex") {
        panic!("fmt pass failed: {err}\nterminal: {}", out.borrow());
    }
    let ours = e2.take_output("test.dvi").expect("DVI produced");
    if ours != reference {
        let n = ours
            .iter()
            .zip(&reference)
            .take_while(|(a, b)| a == b)
            .count();
        panic!(
            "fmt-path DVI mismatch at byte {n} (ours len {}, ref len {})",
            ours.len(),
            reference.len()
        );
    }
}

/// M5: `\openin`/`\read`/`\ifeof` against a data file.
#[test]
fn read_from_file_matches_knuth_tex() {
    let dir = std::env::temp_dir().join(format!("sabitex-golden-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("data.tex"), "alpha beta\n{gamma} delta\n").ok();
    let body = "\\font\\tf=cmr10 \\tf \\openin3=data \
                \\ifeof3 \\shipout\\hbox{closed}\\else\\shipout\\hbox{open}\\fi \
                \\read3 to \\lineone \\read3 to \\linetwo \
                \\shipout\\hbox{\\lineone-\\linetwo} \\closein3 \\end";
    // Reference run needs data.tex next to test.tex.
    let src = format!("{PREAMBLE}{body}");
    let Some(reference) = ({
        std::fs::write(dir.join("test.tex"), &src).ok();
        let out = Command::new("tex")
            .args(["-ini", "-interaction=batchmode", "test.tex"])
            .current_dir(&dir)
            .output()
            .ok();
        out.and_then(|_| std::fs::read(dir.join("test.dvi")).ok())
    }) else {
        eprintln!("SKIPPED (read): TeX Live not available");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    };
    let _ = std::fs::remove_dir_all(&dir);
    let mut fs = KpseFs::default();
    fs.files
        .insert("test.tex".to_string(), src.as_bytes().to_vec());
    fs.files.insert(
        "data.tex".to_string(),
        b"alpha beta\n{gamma} delta\n".to_vec(),
    );
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    if let Err(err) = e.run_file("test.tex") {
        panic!("engine failed: {err}\nterminal: {}", out.borrow());
    }
    let ours = e.take_output("test.dvi").expect("DVI produced");
    if ours != reference {
        let n = ours
            .iter()
            .zip(&reference)
            .take_while(|(a, b)| a == b)
            .count();
        panic!(
            "\\read DVI mismatch at byte {n} (ours len {}, ref len {})",
            ours.len(),
            reference.len()
        );
    }
}

/// M4: \discretionary and explicit hyphen control.
#[test]
fn discretionary_dvi_matches_knuth_tex() {
    compare_kpse(
        "\\input plain \\hsize=60pt \\vsize=400pt \
         supercalifragilistic\\-expialidocious and a man\\discretionary{-}{u}{n}script \
         here.\\par \\end",
        "disc",
    );
}

#[test]
fn characters_ligatures_and_kerns() {
    compare(
        "\\font\\tf=cmr10 \\tf \\shipout\\hbox{Effiziente Wave fi ffl AVATAR To.}\\end",
        &["cmr10"],
        "ligkern",
    );
}

#[test]
fn glue_setting_and_rules() {
    compare(
        "\\font\\tf=cmr10 \\tf \
         \\shipout\\hbox to 100pt{A\\hfil B\\hskip 3pt plus 1fil minus 1pt C}\
         \\shipout\\hbox to 50pt{A\\hss B\\vrule width 2pt height 4pt depth 1pt}\\end",
        &["cmr10"],
        "glue",
    );
}

#[test]
fn vbox_baselines_and_kerns() {
    compare(
        "\\font\\tf=cmr10 \\tf \\baselineskip=12pt \\lineskip=1pt \\lineskiplimit=2pt \
         \\shipout\\vbox{\\hbox{Ag}\\hbox{pq}\\kern 3pt\\hbox{x}\\hrule height 0.4pt}\\end",
        &["cmr10"],
        "vbox",
    );
}

#[test]
fn boxes_registers_and_leaders() {
    compare(
        "\\font\\tf=cmr10 \\tf \
         \\setbox0=\\hbox{ab}\\setbox1=\\vbox{\\copy0\\box0}\
         \\shipout\\hbox to 80pt{\\raise 2pt\\box1\\leaders\\hbox{.}\\hfil x}\\end",
        &["cmr10"],
        "boxes",
    );
}

#[test]
fn at_sized_font_and_spaces() {
    compare(
        "\\font\\big=cmr10 at 14.4pt \\font\\sc=cmr10 scaled 800 \
         \\shipout\\hbox{\\big Big A. \\sc small spaced words}\\end",
        &["cmr10"],
        "atsize",
    );
}
