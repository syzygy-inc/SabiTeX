//! M6: e-TeX extended-mode tests. The `*` prefix on the first input line
//! (etex.ch §1337) switches a virgin INITEX into extended mode; without it
//! the engine must remain indistinguishable from TeX82.

use sabitex_core::io::{CaptureTerminal, MemFs};
use sabitex_core::{Engine, Sizes};

const PREAMBLE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

/// Runs `src` as test.tex via the `**` prompt; `extended` prepends `*`.
/// Returns the terminal output even if the job aborts (e.g. on an error
/// prompt hitting terminal EOF).
fn run_mode(src: &str, extended: bool) -> String {
    let mut fs = MemFs::default();
    let body = format!("{PREAMBLE}{src}");
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let star = if extended { "*" } else { "" };
    let (term, out) = CaptureTerminal::new(vec![format!("{star}\\input test")]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    s
}

/// Extracts \message output (between the file-open echo and the job tail).
fn messages_mode(src: &str, extended: bool) -> String {
    let out = run_mode(src, extended);
    let pos = out
        .find("(test.tex")
        .unwrap_or_else(|| panic!("missing file-open echo in {out:?}"));
    let out = &out[pos + "(test.tex".len()..];
    let out = out.trim_end();
    let out = out
        .strip_suffix("Transcript written on test.log.")
        .unwrap_or(out);
    let out = out.trim_end();
    let out = out.strip_suffix("No pages of output.").unwrap_or(out);
    let out = out.trim_end();
    let out = out.strip_suffix(" )").unwrap_or(out);
    out.replace('\n', " ").trim().to_string()
}

#[test]
fn extended_mode_is_announced() {
    let out = run_mode("\\end", true);
    assert!(
        out.contains("entering extended mode"),
        "terminal output: {out:?}"
    );
    let out = run_mode("\\end", false);
    assert!(!out.contains("entering extended mode"));
}

#[test]
fn etex_version_and_revision() {
    assert_eq!(
        messages_mode("\\message{\\the\\eTeXversion\\eTeXrevision}\\end", true),
        "2.6"
    );
}

#[test]
fn etex_primitives_are_undefined_in_compatibility_mode() {
    // TeX82 compatibility: \eTeXversion must not exist without the `*`.
    let out = run_mode("\\message{\\the\\eTeXversion}\\end", false);
    assert!(
        out.contains("Undefined control sequence"),
        "terminal output: {out:?}"
    );
}

#[test]
fn lastnodetype_reports_the_tail() {
    // Empty vertical list: -1. After a kern: kern_node + 1 = 12. (The
    // trailing "[0]" is the shipout page display for the kern-bearing page.)
    let out = messages_mode(
        "\\message{A\\the\\lastnodetype}\\kern2pt \\message{B\\the\\lastnodetype}\\end",
        true,
    );
    assert!(out.starts_with("A-1 B12"), "got {out:?}");
}

#[test]
fn ifdefined_and_ifcsname() {
    assert_eq!(
        messages_mode(
            "\\def\\x{}\\message{[\\ifdefined\\x D\\else U\\fi]\
             [\\ifdefined\\nope D\\else U\\fi]\
             [\\ifcsname x\\endcsname C\\else N\\fi]\
             [\\ifcsname nope\\endcsname C\\else N\\fi]}\\end",
            true
        ),
        "[D][U][C][N]"
    );
    // \ifcsname must not enter a new control sequence: \nope stays
    // undefined, so a later \ifdefined still says no.
    assert_eq!(
        messages_mode(
            "\\message{[\\ifcsname nope\\endcsname C\\else N\\fi\
             \\ifdefined\\nope D\\else U\\fi]}\\end",
            true
        ),
        "[NU]"
    );
}

#[test]
fn unless_negates_conditionals() {
    assert_eq!(
        messages_mode(
            "\\message{[\\unless\\iftrue T\\else F\\fi]\
             [\\unless\\iffalse T\\else F\\fi]}\\end",
            true
        ),
        "[F][T]"
    );
}

#[test]
fn protected_macros_resist_edef() {
    assert_eq!(
        messages_mode(
            "\\protected\\def\\p{X}\\def\\q{Y}\\edef\\r{\\p\\q}\
             \\message{\\meaning\\r}\\end",
            true
        ),
        "macro:->\\p Y"
    );
    assert_eq!(
        messages_mode("\\protected\\def\\p{X}\\message{\\meaning\\p}\\end", true),
        "\\protected macro:->X"
    );
}

#[test]
fn unexpanded_and_detokenize() {
    assert_eq!(
        messages_mode(
            "\\def\\q{Y}\\edef\\r{\\unexpanded{\\q Z}}\\message{\\meaning\\r}\\end",
            true
        ),
        "macro:->\\q Z"
    );
    assert_eq!(
        messages_mode("\\def\\q{Y}\\message{\\detokenize{\\q &}}\\end", true),
        "\\q &"
    );
}

#[test]
fn current_group_and_if_state() {
    assert_eq!(
        messages_mode(
            "\\message{[\\the\\currentgrouplevel:\\the\\currentgrouptype]}\
             {\\message{[\\the\\currentgrouplevel:\\the\\currentgrouptype]}}\
             \\message{[\\iftrue\\the\\currentiflevel:\\the\\currentiftype:\
             \\the\\currentifbranch\\fi]}\\end",
            true
        ),
        "[0:0] [1:1] [1:15:1]"
    );
}

#[test]
fn interactionmode_reads_and_writes() {
    assert_eq!(
        messages_mode(
            "\\message{[\\the\\interactionmode]}\\interactionmode=1 \
             \\message{[\\the\\interactionmode]}\\end",
            true
        ),
        "[3] [1]"
    );
}

#[test]
fn scantokens_retokenizes() {
    // \scantokens pushes the text back through the reader: with the
    // current catcodes, "\noexpand" written as characters becomes a real
    // control sequence again.
    assert_eq!(
        messages_mode("\\def\\x{\\message{S}}\\scantokens{\\x}\\end", true),
        "S"
    );
    // The text ends like a file: an \endinput-style end of "file" — and
    // \everyeof fires there.
    assert_eq!(
        messages_mode(
            "\\everyeof{\\message{E}}\\scantokens{\\message{S}}\\everyeof{}\\end",
            true
        ),
        "S E"
    );
}

#[test]
fn readline_yields_catcode_free_tokens() {
    let mut fs = MemFs::default();
    let body = format!(
        "{PREAMBLE}\\openin1=data \\readline1 to\\l \\closein1 \
         \\message{{\\meaning\\l}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    fs.files.insert("data.tex".to_string(), b"a{b} %c".to_vec());
    let (term, out) = CaptureTerminal::new(vec!["*\\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // Every character (including { } %) arrives as catcode-12 "other".
    assert!(s.contains("macro:->a{b} %c"), "{s:?}");
}

#[test]
fn numexpr_evaluates_with_rounding() {
    assert_eq!(
        messages_mode(
            "\\message{[\\the\\numexpr 2+3*4\\relax]\
             [\\the\\numexpr (2+3)*4\\relax]\
             [\\the\\numexpr 7/2\\relax]\
             [\\the\\numexpr -7/2\\relax]\
             [\\the\\numexpr 100*100/3\\relax]}\\end",
            true
        ),
        // \numexpr rounds division; 100*100/3 is a combined scale.
        "[14][20][4][-4][3333]"
    );
}

#[test]
fn dimexpr_and_glueexpr() {
    assert_eq!(
        messages_mode(
            "\\message{[\\the\\dimexpr 1pt+2pt*3\\relax]\
             [\\the\\dimexpr 10pt/4\\relax]}\\end",
            true
        ),
        "[7.0pt][2.5pt]"
    );
    assert_eq!(
        messages_mode(
            "\\skip0=1pt plus 2fil minus 3pt \
             \\message{[\\the\\glueexpr\\skip0*2\\relax]\
             [\\the\\gluestretch\\skip0]\
             [\\the\\gluestretchorder\\skip0]\
             [\\the\\glueshrink\\skip0]}\\end",
            true
        ),
        "[2.0pt plus 4.0fil minus 6.0pt][2.0pt][1][3.0pt]"
    );
}

#[test]
fn parshape_queries() {
    assert_eq!(
        messages_mode(
            "\\parshape 2 1pt 2pt 3pt 4pt \
             \\message{[\\the\\parshapeindent1][\\the\\parshapelength2]\
             [\\the\\parshapedimen3][\\the\\parshapedimen0]}\\end",
            true
        ),
        "[1.0pt][4.0pt][3.0pt][0.0pt]"
    );
}

#[test]
fn tracing_groups_and_assigns() {
    let out = run_mode(
        "\\tracingonline1 \\tracinggroups1 {\\relax}\
         \\tracingassigns1 \\count0=5 \\count0=5 \\end",
        true,
    );
    assert!(
        out.contains("{entering simple group (level 1) at line"),
        "{out:?}"
    );
    assert!(
        out.contains("{leaving simple group (level 1) entered at line"),
        "{out:?}"
    );
    assert!(out.contains("{changing \\count0=0}"), "{out:?}");
    assert!(out.contains("{into \\count0=5}"), "{out:?}");
    // The second \count0=5 is redundant in extended mode.
    assert!(out.contains("{reassigning \\count0=5}"), "{out:?}");
}

#[test]
fn showifs_and_showgroups() {
    let out = run_mode(
        "\\nonstopmode\\tracingonline1 \\iftrue\\showifs\\fi\\end",
        true,
    );
    assert!(
        out.contains("### level 1: \\iftrue entered on line"),
        "{out:?}"
    );
    let out = run_mode(
        "\\nonstopmode\\tracingonline1 {\\begingroup\\showgroups\\endgroup}\\end",
        true,
    );
    assert!(out.contains("### semi simple group (level 2)"), "{out:?}");
    assert!(out.contains("### simple group (level 1)"), "{out:?}");
    assert!(out.contains("### bottom level"), "{out:?}");
}

#[test]
fn interlinepenalties_assignment_and_query() {
    assert_eq!(
        messages_mode(
            "\\interlinepenalties=3 100 200 300 \
             \\message{[\\the\\interlinepenalties0]\
             [\\the\\interlinepenalties1][\\the\\interlinepenalties3]\
             [\\the\\interlinepenalties9]}\\end",
            true
        ),
        "[3][100][300][300]"
    );
}

#[test]
fn extended_mode_survives_a_format_dump() {
    // Dump an extended format, reload it, and check \eTeXversion works.
    let mut fs = MemFs::default();
    fs.files.insert("fmt.tex".to_string(), b"\\dump".to_vec());
    let (term, _) = CaptureTerminal::new(vec!["*\\input fmt".to_string()]);
    let mut e1 = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e1.run_terminal_job().expect("dump completes");
    let fmt = e1.take_output("fmt.fmt").expect("fmt.fmt dumped");

    let mut fs = MemFs::default();
    let body = format!("{PREAMBLE}\\message{{\\the\\eTeXversion\\eTeXrevision}}\\end");
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec!["&fmt test".to_string()]);
    let mut e2 = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e2.load_fmt(&fmt).expect("format loads");
    e2.run_terminal_job().expect("job should end at \\end");
    let s = out.borrow().clone();
    assert!(s.contains("entering extended mode"), "{s:?}");
    assert!(s.contains("2.6"), "{s:?}");
}

#[test]
fn sparse_registers() {
    // Registers above 255 live in the sparse trees; grouping works.
    assert_eq!(
        messages_mode(
            "\\count30000=7 \
             \\message{[\\the\\count30000]}\
             {\\count30000=9 \\message{[\\the\\count30000]}}\
             \\message{[\\the\\count30000]}\
             \\global\\count30000=11 \\message{[\\the\\count30000]}\\end",
            true
        ),
        "[7] [9] [7] [11]"
    );
}

#[test]
fn sparse_skip_toks_and_shorthand() {
    assert_eq!(
        messages_mode(
            "\\skip900=1pt plus 2pt \\toks900={T!} \
             \\countdef\\big=700 \\big=42 \
             \\message{[\\the\\skip900][\\the\\toks900][\\the\\big]\
             [\\the\\skip901]}\\end",
            true
        ),
        "[1.0pt plus 2.0pt][T!][42][0.0pt]"
    );
}

#[test]
fn texxet_direction_nodes() {
    // Disabled: \beginL is an error.
    let out = run_mode("\\beginL\\end", true);
    assert!(out.contains("Improper \\beginL"), "{out:?}");
    // Enabled: direction nodes enter the list and display symbolically.
    let out = run_mode(
        "\\TeXXeTstate=1 \\nonstopmode\\tracingonline1 \
         \\setbox0=\\hbox{\\beginR\\endR}\\showbox0 \\end",
        true,
    );
    assert!(out.contains("\\beginR"), "{out:?}");
    assert!(out.contains("\\endR"), "{out:?}");
    // An unmatched \beginR is reported by hpack.
    let out = run_mode(
        "\\TeXXeTstate=1 \\nonstopmode\\setbox0=\\hbox{\\beginR}\\end",
        true,
    );
    assert!(
        out.contains("\\endL or \\endR problem (1 missing, 0 extra"),
        "{out:?}"
    );
}

#[test]
fn middle_without_left_and_compat_absence() {
    // \middle outside a \left group reports its own error message.
    let out = run_mode("\\nonstopmode\\catcode`\\$=3 $\\middle.$\\end", true);
    assert!(out.contains("Extra \\middle"), "{out:?}");
    // In compatibility mode the primitive does not exist.
    let out = run_mode("\\nonstopmode\\middle\\end", false);
    assert!(out.contains("Undefined control sequence"), "{out:?}");
}

#[test]
fn texxet_ship_out_reverses_r_text() {
    // An R-text segment is reversed by ship_out: rules inside
    // \beginR...\endR must appear in the DVI in reversed order.
    let mut fs = MemFs::default();
    let body = format!(
        "{PREAMBLE}\\TeXXeTstate=1 \
         \\shipout\\hbox{{\\vrule width 3pt height 5pt \\kern6pt \
         \\beginR\\vrule width 9pt height 5pt \\kern2pt \
         \\vrule width 12pt height 5pt \\endR}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, _out) = CaptureTerminal::new(vec!["*\\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e.run_terminal_job().expect("job runs");
    let dvi = e.take_output("test.dvi").expect("test.dvi produced");
    let widths: Vec<i32> = dvi
        .windows(9)
        .filter(|w| w[0] == 132) // set_rule ht wd
        .map(|w| i32::from_be_bytes([w[5], w[6], w[7], w[8]]))
        .collect();
    let pt = 65536;
    assert_eq!(
        widths,
        vec![3 * pt, 12 * pt, 9 * pt],
        "R-text rules must be emitted right-to-left"
    );
}

#[test]
fn sparse_boxes() {
    let out = run_mode(
        "\\nonstopmode\\tracingonline1 \
         \\setbox5000=\\hbox{}\\message{[\\ifvoid5000 V\\else B\\fi\
         \\ifhbox5000 H\\else N\\fi]}\\showbox5000 \\end",
        true,
    );
    assert!(out.contains("[BH]"), "{out:?}");
    assert!(out.contains("> \\box5000="), "{out:?}");
}

#[test]
fn bom_sniffing_utf8_and_utf16() {
    use sabitex_core::input::decode_lines;
    // UTF-8 BOM is stripped.
    let l = decode_lines(b"\xEF\xBB\xBFabc\n");
    assert_eq!(l, vec![vec!['a' as i32, 'b' as i32, 'c' as i32]]);
    // UTF-16LE with BOM.
    let l = decode_lines(b"\xFF\xFEa\x00b\x00");
    assert_eq!(l, vec![vec!['a' as i32, 'b' as i32]]);
    // UTF-16BE with BOM, non-ASCII.
    let l = decode_lines(b"\xFE\xFF\x30\x42");
    assert_eq!(l, vec![vec![0x3042]]); // あ
                                       // Plain UTF-8, multibyte.
    let l = decode_lines("é\n".as_bytes());
    assert_eq!(l, vec![vec![0xE9]]);
}

#[cfg(feature = "shaping")]
#[test]
fn native_font_loads_and_measures() {
    let mut fs = MemFs::default();
    let font = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/latin-modern/lmroman10-regular.otf"
    ))
    .expect("fixture font");
    fs.files.insert("lmroman10-regular.otf".to_string(), font);
    let body = format!(
        "{PREAMBLE}\\font\\f=\"[lmroman10-regular.otf]\" at 10pt \\f\
         \\message{{[\\fontname\\f][\\the\\fontdimen6\\f]}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // quad (fontdimen6) equals the at-size for native fonts.
    assert!(s.contains("[10.0pt]"), "{s:?}");
    assert!(s.contains("[lmroman10-regular.otf]"), "{s:?}");
}

#[cfg(feature = "shaping")]
#[test]
fn native_word_shapes_in_hbox() {
    let mut fs = MemFs::default();
    let font = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/latin-modern/lmroman10-regular.otf"
    ))
    .expect("fixture font");
    fs.files.insert("lmroman10-regular.otf".to_string(), font);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\f=\"[lmroman10-regular.otf]\" at 10pt \\f\
         \\setbox0=\\hbox{{Hello}}\\showbox0 \\message{{WD=\\the\\wd0}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // The shaped word displays with its text and has a real width.
    assert!(s.contains("Hello"), "{s:?}");
    assert!(s.contains("\\f Hello"), "{s:?}");
    // \wd0 is the shaped width: nonzero and plausible (2-6 em at 10pt).
    let wd = s
        .rsplit("WD=")
        .next()
        .and_then(|t| t.split("pt").next())
        .and_then(|t| t.parse::<f64>().ok())
        .expect("WD marker");
    assert!(wd > 15.0 && wd < 40.0, "width {wd}");
}

#[cfg(feature = "shaping")]
#[test]
fn xdv_output_structure() {
    let mut fs = MemFs::default();
    let font = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/latin-modern/lmroman10-regular.otf"
    ))
    .expect("fixture font");
    fs.files.insert("lmroman10-regular.otf".to_string(), font);
    let body = format!(
        "{PREAMBLE}\\font\\f=\"[lmroman10-regular.otf]\" at 10pt \\f\
         \\shipout\\hbox{{Hi}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, _out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e.run_terminal_job().expect("job");
    let xdv = e.take_output("test.dvi").expect("xdv");
    // Preamble and postamble carry id_byte 7.
    assert_eq!(xdv[0], 247, "pre");
    assert_eq!(xdv[1], 7, "XDV id");
    // post_post id precedes the 223-padding (4-7 bytes of it).
    let last_non_pad = *xdv.iter().rev().find(|&&b| b != 223).unwrap();
    assert_eq!(last_non_pad, 7, "post_post id");
    // define_native_font (252) and set_glyphs (253) appear.
    assert!(xdv.contains(&252), "define_native_font missing");
    assert!(xdv.contains(&253), "set_glyphs missing");
    // The font name travels in the definition.
    let s = String::from_utf8_lossy(&xdv);
    assert!(s.contains("lmroman10-regular.otf"), "font name in def");
}

#[test]
fn jfm_loads_and_classifies() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\font\\j=umin10 \\message{{QUAD=\\the\\fontdimen6\\j}}\
         \\message{{DIR=1}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // The JFM loads as a horizontal (yoko) Japanese font...
    assert!(s.contains("DIR=1"), "{s:?}");
    // ...with min10's famous fullwidth quad of 9.62216pt (\fontdimen6).
    assert!(s.contains("QUAD=9.62216pt"), "{s:?}");
    // char_type classes: U+3042 (あ) is class 0 (default kanji);
    // U+3001 (、) is a closing-punctuation class (nonzero).
    let f = {
        // Look up through the engine directly.
        let f = e.fonts.font_ptr;
        assert!(e.fonts.dir[f as usize] == 1);
        f
    };
    assert_eq!(e.get_jfm_pos(0x3042, f), 0, "hiragana class");
    assert_ne!(e.get_jfm_pos(0x3001, f), 0, "comma class");
}

#[test]
fn kanji_pair_nodes_in_hbox() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\j=umin10 \\j\\setbox0=\\hbox{{ああ}}\\showbox0 \
         \\message{{WD=\\the\\wd0}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // The box displays the two Japanese characters with the JFM font.
    assert_eq!(s.matches(".\\j あ").count(), 2, "{s:?}");
    // Width = 2 fullwidth quads of min10: 2 x 9.62216pt = 19.24431pt.
    assert!(s.contains("WD=19.244"), "{s:?}");
}

#[test]
fn kanji_skip_and_jfm_glue_match_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\j=umin10 \\j\\setbox0=\\hbox{{ああ、あ}}\\message{{W0=\\the\\wd0}}\\showbox0 \
         \\kanjiskip=1pt plus 2pt minus 3pt \\autospacing \
         \\setbox2=\\hbox to 30pt{{ああ}}\\showbox2 \
         \\setbox3=\\hbox{{ああ}}\\message{{W3=\\the\\wd3}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // JFM glue after the comma (euptex: 4.58203 minus 2.291) and the
    // box width 38.48865 (= 3 quads + halfwidth comma + jfm glue).
    assert!(s.contains("W0=38.48865pt"), "{s:?}");
    assert!(
        s.contains("\\glue(refer from jfm) 4.58203 minus 2.291"),
        "{s:?}"
    );
    // Implicit \kanjiskip: natural width grows by 1pt (euptex 20.24432),
    // and a to-box distributes over its stretch (glue set 4.87784).
    assert!(s.contains("W3=20.24432pt"), "{s:?}");
    assert!(s.contains("glue set 4.87784"), "{s:?}");
}

#[test]
fn kinsoku_and_line_break_match_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\j=umin10 \\j\
         \\kanjiskip=1pt plus 2pt minus 3pt \\autospacing \
         \\prebreakpenalty`ん=150 \\postbreakpenalty`あ=200 \
         \\setbox0=\\hbox{{あんいあ}}\\message{{W0=\\the\\wd0}}\\showbox0 \
         \\hsize=25pt \\parfillskip=0pt plus1fil \\parindent=0pt \
         \\pretolerance=-1 \\tolerance=10000 \
         \\setbox9=\\vbox{{\\noindent ああああ\\par}}\\showbox9 \\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // hbox: merged kinsoku penalty (200 post + 150 pre) + real kanjiskip.
    assert!(s.contains("W0=41.48865pt"), "{s:?}");
    assert!(s.contains(r"\penalty 350(for kinsoku)"), "{s:?}");
    assert!(
        s.contains(r"\glue(\kanjiskip) 1.0 plus 2.0 minus 3.0"),
        "{s:?}"
    );
    // paragraph: first line tight with glue set - 0.97775 (euptex),
    // second line stretches parfillskip by 15.37784fil.
    assert!(s.contains("glue set - 0.97775"), "{s:?}");
    assert!(s.contains("glue set 15.37784fil"), "{s:?}");
    assert!(s.contains(r"\penalty 200(for kinsoku)"), "{s:?}");
}

#[test]
fn xkanjiskip_and_inhibit_match_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let cmr = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/computer-modern/cmr10.tfm"
    ))
    .expect("fixture cmr10");
    fs.files.insert("cmr10.tfm".to_string(), cmr);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\j=umin10 \\j \\font\\r=cmr10 \
         \\xkanjiskip=2pt plus 1pt minus 1pt \\autoxspacing \
         \\setbox0=\\hbox{{\\r aあa}}\\message{{W0=\\the\\wd0}}\\showbox0 \
         \\inhibitxspcode`あ=1 \
         \\setbox2=\\hbox{{\\r aあa}}\\message{{W2=\\the\\wd2}}\
         \\inhibitxspcode`あ=2 \
         \\setbox3=\\hbox{{\\r aあa}}\\message{{W3=\\the\\wd3}}\
         \\inhibitxspcode`あ=0 \
         \\setbox4=\\hbox{{\\r aあa}}\\message{{W4=\\the\\wd4}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // euptex: both boundaries get a real \xkanjiskip (23.6222), then
    // \inhibitxspcode 1/2/0 suppresses before/after/both.
    assert!(s.contains("W0=23.6222pt"), "{s:?}");
    assert!(
        s.contains(r"\glue(\xkanjiskip) 2.0 plus 1.0 minus 1.0"),
        "{s:?}"
    );
    assert!(s.contains("W2=21.6222pt"), "{s:?}");
    assert!(s.contains("W3=21.6222pt"), "{s:?}");
    assert!(s.contains("W4=19.6222pt"), "{s:?}");
}

#[test]
fn xkanjiskip_across_ligature_matches_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let cmr = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/computer-modern/cmr10.tfm"
    ))
    .expect("fixture cmr10");
    fs.files.insert("cmr10.tfm".to_string(), cmr);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\j=umin10 \\j \\font\\r=cmr10 \
         \\xkanjiskip=2pt plus 1pt minus 1pt \\autoxspacing \
         \\setbox0=\\hbox{{\\r effあeffe}}\\message{{W0=\\the\\wd0}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // euptex: the ff ligature also takes \xkanjiskip on both sides.
    assert!(s.contains("W0=38.62221pt"), "{s:?}");
}

#[test]
fn kanji_dvi_uses_set2_and_jfm_fnt_def() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\font\\j=umin10 \\j\
         \\shipout\\hbox{{あ}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, _out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let dvi = e.take_output("test.dvi").expect("dvi produced");
    // Classic id byte (no native font): pTeX-compatible DVI.
    assert_eq!(dvi[1], 2, "pre id");
    // set2 U+3042 (euptex: "set2 12354") somewhere in the page.
    let set2 = [129u8, 0x30, 0x42];
    assert!(dvi.windows(3).any(|w| w == set2), "set2 missing: {dvi:?}");
    // fnt_def carries the JFM name.
    assert!(dvi.windows(6).any(|w| w == b"umin10"), "font name missing");
}

#[test]
fn jchar_widow_penalty_matches_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\font\\j=umin10 \\j\\jcharwidowpenalty=500 \
         \\hsize=100pt \\parfillskip=0pt plus1fil \\parindent=0pt \
         \\pretolerance=-1 \\tolerance=10000 \
         \\setbox9=\\vbox{{\\noindent ああああああああああああ\\par}}\\showbox9 \\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // euptex: on the final 2-char line the widow penalty sits before
    // the last character with a materialised (zero) \kanjiskip after.
    assert!(s.contains(r"\penalty 500(for \jcharwidowpenalty)"), "{s:?}");
    assert!(s.contains(r"\glue(\kanjiskip) 0.0"), "{s:?}");
    // the split must not orphan a pair (a torn pair shows CLOBBERED or
    // crashes hpack with a bogus font index).
    assert!(!s.contains("CLOBBERED"), "{s:?}");
}

#[test]
fn ptex_primitives_a8_match_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9 \
         \\message{{k1=\\kansuji 2026.}}\n\
         \\kansujichar1=`\u{58F1} \\message{{k4=\\kansuji 11.}}\n\
         \\message{{ru=\\the\\inhibitxspcode`X.}}\n\
         \\inhibitxspcode`X=2 \\message{{rs=\\the\\inhibitxspcode`X.}}\n\
         \\prebreakpenalty`Y=99 \\message{{rp=\\the\\prebreakpenalty`Y.}}\n\
         \\postbreakpenalty`Y=55 \\message{{rq=\\the\\prebreakpenalty`Y.}}\n\
         \\font\\j=umin10 \\j\n\
         \\setbox1=\\hbox{{\\kchar`あ\\kchar\"3042}}\\showbox1\n\
         \\setbox3=\\hbox{{\\inhibitglue(あ\\inhibitglue)}}\\showbox3 \\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // \kansuji digits and \kansujichar override (euptex-verified).
    assert!(s.contains("k1=\u{4E8C}\u{3007}\u{4E8C}\u{516D}."), "{s:?}");
    assert!(s.contains("k4=\u{58F1}\u{58F1}."), "{s:?}");
    // \the readbacks: unset inhibitxspcode = 3; setting a post penalty
    // retypes the shared kinsoku slot so the pre readback answers 0.
    assert!(s.contains("ru=3."), "{s:?}");
    assert!(s.contains("rs=2."), "{s:?}");
    assert!(s.contains("rp=99."), "{s:?}");
    assert!(s.contains("rq=0."), "{s:?}");
    // \kchar builds real Japanese pairs: two chars, one kanjiskip-able
    // box of width 2 em (umin10: 9.62216pt each, euptex-identical).
    assert!(s.contains("\\hbox(7.77588+1.38855)x19.24432"), "{s:?}");
}

#[test]
fn overlong_line_reports_overflow_not_panic() {
    // A16: a source line beyond buf_size must surface as the classic
    // "TeX capacity exceeded [buffer size]" instead of silently
    // truncating (which desynced the tokenizer and panicked in
    // ^^-reduction).
    let mut fs = MemFs::default();
    let long = "\\message{x}".repeat(200); // ~2200 chars > 500
    fs.files.insert(
        "test.tex".to_string(),
        format!("{PREAMBLE}\\nonstopmode\n{long}\n\\end").into_bytes(),
    );
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let r = e.run_terminal_job();
    let s = out.borrow().clone();
    assert!(
        r.is_err() || s.contains("buffer size"),
        "expected overflow, got ok with {s:?}"
    );
}

#[test]
fn disp_node_and_direction_match_euptex() {
    let mut fs = MemFs::default();
    let jfm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/uptex/umin10.tfm"
    ))
    .expect("fixture jfm");
    fs.files.insert("umin10.tfm".to_string(), jfm);
    let body = format!(
        "{PREAMBLE}\\nonstopmode\\tracingonline1 \\showboxbreadth99 \\showboxdepth9\n\
         \\font\\j=umin10 \\j \\kanjiskip=5pt \\autospacing\n\
         \\setbox4=\\hbox{{あ}}\\showbox4\n\
         \\setbox1=\\hbox{{ああ}}\\kanjiskip=20pt\n\
         \\hsize=200pt \\parfillskip=0pt plus1fil \\parindent=0pt\n\
         \\pretolerance=-1 \\tolerance=10000\n\
         \\setbox2=\\vbox{{\\noindent\\unhbox1 ああ\\par}}\\showbox2 \\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    // euptex-verified: the first Japanese char of a list is preceded by
    // a disp_node, top-level boxes carry the direction annotation, and
    // the K-K skip across a disp boundary (from \unhbox) materialises
    // with the CURRENT \kanjiskip (20pt), disp first, glue after.
    assert!(s.contains(", yoko direction"), "{s:?}");
    assert!(s.contains("\\displace 0.0"), "{s:?}");
    assert!(
        s.contains("..\\displace 0.0\n..\\glue(\\kanjiskip) 20.0\n..\\j"),
        "{s:?}"
    );
}

#[test]
fn ifincsname_matches_pdftex() {
    // pdfTeX/XeTeX \ifincsname: true only while the \csname body is
    // being expanded (the LaTeX kernel's active ~ relies on it).
    let mut fs = MemFs::default();
    let body = format!(
        "{PREAMBLE}\\nonstopmode\
         \\def\\probe{{\\ifincsname I\\else O\\fi}}\
         \\message{{out=\\probe.}}\
         \\expandafter\\def\\csname X\\probe\\endcsname{{}}\
         \\message{{in=\\expandafter\\string\\csname X\\probe\\endcsname.}}\\end"
    );
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(vec![r"*\input test".to_string()]);
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    let _ = e.run_terminal_job();
    let s = out.borrow().clone();
    assert!(s.contains("out=O."), "{s:?}");
    assert!(s.contains("in=\\XI."), "{s:?}");
}
