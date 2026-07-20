//! M1 acceptance tests: the expansion "REPL". Each source is fed through
//! the real input/expansion machinery (Parts 17-28) and the terminal output
//! is compared against the behavior of Knuth's tex (INITEX, no format).

use sabitex_core::io::{CaptureTerminal, MemFs};
use sabitex_core::{Engine, Sizes};

/// INITEX has no category codes for braces etc. — formats set them up
/// (plain.tex §intro). The tests bootstrap the same way real INITEX jobs do.
const PREAMBLE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

/// Runs `src` as file test.tex; returns everything printed to the terminal.
fn run(src: &str) -> String {
    let mut fs = MemFs::default();
    let body = format!("{PREAMBLE}{src}");
    fs.files.insert("test.tex".to_string(), body.into_bytes());
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e.run_file("test.tex").expect("job should end at \\end");
    let s = out.borrow().clone();
    s
}

/// Like `run`, but returns only what \message printed: the output between
/// the file-open "(test.tex" and the final " )" / "No pages of output.".
fn messages(src: &str) -> String {
    let out = run(src);
    let out = out
        .strip_prefix("(test.tex")
        .unwrap_or_else(|| panic!("missing file-open echo in {out:?}"));
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
fn message_and_end() {
    assert_eq!(
        run("\\message{hi}\\end"),
        "(test.tex hi )\nNo pages of output.\nTranscript written on test.log.\n"
    );
}

#[test]
fn macro_with_undelimited_parameters() {
    assert_eq!(
        messages("\\def\\a#1#2{[#1|#2]}\\message{\\a {xy}z}\\end"),
        "[xy|z]"
    );
}

#[test]
fn macro_with_delimited_parameters() {
    assert_eq!(
        messages("\\def\\pair(#1,#2){<#1><#2>}\\message{\\pair(a,b)}\\end"),
        "<a><b>"
    );
    // Partial-match backtracking (§397): delimiter `ab` against input `aab`.
    assert_eq!(messages("\\def\\q#1ab{(#1)}\\message{\\q aab}\\end"), "(a)");
}

#[test]
fn nine_parameters_and_hash_hash() {
    assert_eq!(
        messages(
            "\\def\\nine#1#2#3#4#5#6#7#8#9{#9#8#7#6#5#4#3#2#1}\
             \\message{\\nine 123456789}\\end"
        ),
        "987654321"
    );
    // ## in a body becomes a single # parameter token (§294 shows it as ##).
    assert_eq!(
        messages("\\def\\h{\\message{x}}\\def\\g#1{#1}\\message{\\g{ok}}\\end"),
        "ok"
    );
}

#[test]
fn expandafter_and_string() {
    assert_eq!(
        messages("\\def\\a{\\b}\\def\\b{B}\\message{\\expandafter\\string\\a}\\end"),
        "\\b"
    );
}

#[test]
fn noexpand_and_edef_and_the() {
    // (\nonstopmode: \show ends with error(), which would otherwise prompt.)
    let out = run(
        "\\nonstopmode\\count5=42 \\def\\x{X}\\edef\\y{\\the\\count5 \\noexpand\\x}\\show\\y\\end",
    );
    assert!(out.contains("> \\y=macro:"), "{out}");
    assert!(out.contains("->42\\x ."), "{out}");
}

#[test]
fn csname_makes_control_sequences() {
    assert_eq!(
        messages(
            "\\expandafter\\def\\csname my cs\\endcsname{ok}\
             \\message{\\csname my cs\\endcsname}\\end"
        ),
        "ok"
    );
    // An undefined \csname...\endcsname becomes \relax (§372). (\ifx does
    // not expand, hence the \expandafter idiom.)
    assert_eq!(
        messages("\\message{\\expandafter\\ifx\\csname nope\\endcsname\\relax R\\else X\\fi}\\end"),
        "R"
    );
}

#[test]
fn conditionals() {
    assert_eq!(messages("\\message{\\ifnum3<5 lt\\else ge\\fi}\\end"), "lt");
    assert_eq!(messages("\\message{\\ifnum3>5 lt\\else ge\\fi}\\end"), "ge");
    assert_eq!(
        messages("\\message{\\ifcase2 a\\or b\\or c\\or d\\fi}\\end"),
        "c"
    );
    assert_eq!(
        messages("\\message{\\ifodd7 odd\\else even\\fi}\\end"),
        "odd"
    );
    assert_eq!(
        messages("\\message{\\ifdim 1in>72pt big\\else small\\fi}\\end"),
        "big"
    );
    assert_eq!(messages("\\message{\\ifvmode v\\else h\\fi}\\end"), "v");
    // Nested skipping: the inner \if is skipped wholesale.
    assert_eq!(
        messages("\\message{\\iffalse a\\iftrue b\\fi c\\else d\\fi}\\end"),
        "d"
    );
    // \if character comparison after full expansion.
    assert_eq!(
        messages("\\def\\aa{a}\\message{\\if a\\aa eq\\else ne\\fi}\\end"),
        "eq"
    );
    assert_eq!(
        messages("\\message{\\ifcat a1 same\\else diff\\fi}\\end"),
        "diff"
    );
}

#[test]
fn ifx_compares_macros_and_meanings() {
    assert_eq!(
        messages("\\def\\p{q}\\def\\q{q}\\message{\\ifx\\p\\q same\\else diff\\fi}\\end"),
        "same"
    );
    assert_eq!(
        messages("\\def\\p{q}\\def\\q{r}\\message{\\ifx\\p\\q same\\else diff\\fi}\\end"),
        "diff"
    );
    // \long makes macros \ifx-different (§508 intro).
    assert_eq!(
        messages("\\def\\p{q}\\long\\def\\q{q}\\message{\\ifx\\p\\q same\\else diff\\fi}\\end"),
        "diff"
    );
}

#[test]
fn grouping_saves_and_restores() {
    assert_eq!(
        messages("\\count0=1 {\\count0=2 \\message{\\the\\count0}}\\message{\\the\\count0}\\end"),
        "2 1"
    );
    assert_eq!(
        messages("{\\global\\count1=7}\\message{\\the\\count1}\\end"),
        "7"
    );
    assert_eq!(
        messages("\\def\\v{outer}{\\def\\v{inner}\\message{\\v}}\\message{\\v}\\end"),
        "inner outer"
    );
    // \begingroup/\endgroup and \aftergroup.
    assert_eq!(
        messages(
            "\\def\\later{\\message{after}}\\begingroup\\aftergroup\\later\
             \\message{in}\\endgroup\\message{out}\\end"
        ),
        "in after out"
    );
}

#[test]
fn registers_and_arithmetic() {
    assert_eq!(
        messages(
            "\\count3=10 \\advance\\count3 by 5 \\multiply\\count3 by -2 \
             \\message{\\the\\count3}\\end"
        ),
        "-30"
    );
    assert_eq!(
        messages("\\count3=-30 \\divide\\count3 by 7 \\message{\\the\\count3}\\end"),
        "-4"
    );
    assert_eq!(
        messages("\\dimen0=1in \\message{\\the\\dimen0}\\end"),
        "72.26999pt"
    );
    assert_eq!(
        messages("\\skip2=3pt plus 1fil minus 2.5pt \\message{\\the\\skip2}\\end"),
        "3.0pt plus 1.0fil minus 2.5pt"
    );
    assert_eq!(
        messages("\\countdef\\cnt=8 \\cnt=12 \\message{\\the\\cnt}\\end"),
        "12"
    );
    assert_eq!(
        messages("\\chardef\\pct=37 \\message{\\the\\pct}\\end"),
        "37"
    );
}

#[test]
fn number_romannumeral_jobname() {
    assert_eq!(messages("\\message{\\number\\time}\\end"), "720");
    assert_eq!(messages("\\message{\\romannumeral 1990}\\end"), "mcmxc");
    assert_eq!(messages("\\message{\\jobname}\\end"), "test");
    assert_eq!(messages("\\message{\\number`\\A}\\end"), "65");
    assert_eq!(messages("\\message{\\number\"FF}\\end"), "255");
    assert_eq!(messages("\\message{\\number'777}\\end"), "511");
}

#[test]
fn let_and_futurelet() {
    assert_eq!(
        messages("\\let\\m=\\message \\m{letworks}\\end"),
        "letworks"
    );
    // (Inside an \hbox: a bare letter in vertical mode would start a
    // paragraph, which arrives in M3.)
    assert_eq!(
        messages(
            "\\setbox0=\\hbox{\\def\\skipme{}\\futurelet\\next\\skipme A\
             \\message{\\meaning\\next}}\\end"
        ),
        "the letter A"
    );
    assert_eq!(messages("\\message{\\meaning\\relax}\\end"), "\\relax");
    assert_eq!(
        messages("\\message{\\meaning\\undefinedthing}\\end"),
        "undefined"
    );
}

#[test]
fn uppercase_lowercase() {
    assert_eq!(messages("\\uppercase{\\message{abc}}\\end"), "ABC");
    assert_eq!(messages("\\lowercase{\\message{AbC}}\\end"), "abc");
    // \uccode changes feed into \uppercase.
    assert_eq!(
        messages("\\uccode`\\a=`\\Z \\uppercase{\\message{aa}}\\end"),
        "ZZ"
    );
}

#[test]
fn catcode_changes_affect_scanning() {
    assert_eq!(
        messages("\\catcode`\\@=11 \\def\\my@cs{good}\\message{\\my@cs}\\end"),
        "good"
    );
    // ^^ notation: ^^41 is hex 41 = A.
    assert_eq!(messages("\\message{^^41}\\end"), "A");
    assert_eq!(messages("\\message{\\number`^^41}\\end"), "65");
}

#[test]
fn showthe_and_errors_recover() {
    let out = run("\\nonstopmode\\count7=99 \\showthe\\count7\\end");
    assert!(out.contains("> 99."), "{out}");

    let out = run("\\nonstopmode\\undefinedmacro\\message{still alive}\\end");
    assert!(out.contains("! Undefined control sequence"), "{out}");
    assert!(out.contains("still alive"), "{out}");
}

#[test]
fn input_nests_files() {
    let mut fs = MemFs::default();
    fs.files.insert(
        "test.tex".to_string(),
        format!("{PREAMBLE}\\message{{a}}\\input sub \\message{{c}}\\end").into_bytes(),
    );
    fs.files
        .insert("sub.tex".to_string(), b"\\message{b}".to_vec());
    let (term, out) = CaptureTerminal::new(Vec::new());
    let mut e = Engine::new(Sizes::default(), Box::new(fs), Box::new(term));
    e.run_file("test.tex").expect("job should end at \\end");
    let s = out.borrow().clone();
    assert_eq!(
        s,
        "(test.tex a (sub.tex b) c )\nNo pages of output.\nTranscript written on test.log.\n"
    );
}
