//! The `&format` specification on the ** line (document/cli.md): the CLI
//! resolves and loads `name.fmt` itself, in the documented order, and a
//! format it cannot find is a clean failure rather than a silent INITEX
//! run with an undefined `\quad` (C-JOB).

use std::path::{Path, PathBuf};
use std::process::Command;

fn sabitex() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sabitex"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sabitex-fmtspec-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// INITEX: defines `\hello` and dumps `texput.fmt` into `dir`.
fn dump_format(dir: &Path) {
    let out = sabitex()
        .current_dir(dir)
        .args([
            "--interaction=batchmode",
            "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\hello{HELLO}\\dump",
        ])
        .output()
        .expect("sabitex runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("texput.fmt").is_file(), "texput.fmt dumped");
}

/// A job file that shows the macro carried by the format.
fn write_job(dir: &Path) {
    std::fs::write(dir.join("run.tex"), "\\show\\hello \\end\n").unwrap();
}

#[test]
fn ampersand_loads_the_format_from_the_working_directory() {
    let dir = temp_dir("cwd");
    dump_format(&dir);
    write_job(&dir);
    let out = sabitex()
        .current_dir(&dir)
        .args(["--interaction=nonstopmode", "&texput run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("->HELLO."),
        "macro from the format:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ampersand_finds_the_format_through_sabitex_formats() {
    let fmt_dir = temp_dir("formats");
    dump_format(&fmt_dir);
    let work = temp_dir("work");
    write_job(&work);
    let out = sabitex()
        .current_dir(&work)
        .env("SABITEX_FORMATS", &fmt_dir)
        .args(["--interaction=nonstopmode", "&texput run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("->HELLO."), "{stdout}");
    let _ = std::fs::remove_dir_all(&fmt_dir);
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn a_missing_format_is_a_clean_failure_not_an_initex_run() {
    let dir = temp_dir("missing");
    write_job(&dir);
    let out = sabitex()
        .current_dir(&dir)
        .env_remove("SABITEX_FORMATS")
        .args(["--interaction=batchmode", "&no-such-format run"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("can't find the format"), "{stderr}");
    assert!(!dir.join("texput.dvi").exists(), "nothing was typeset");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_explicit_fmt_option_wins_over_the_spec() {
    let dir = temp_dir("explicit");
    dump_format(&dir);
    std::fs::rename(dir.join("texput.fmt"), dir.join("other.fmt")).unwrap();
    write_job(&dir);
    let out = sabitex()
        .current_dir(&dir)
        .args([
            "--fmt",
            "other.fmt",
            "--interaction=nonstopmode",
            "&no-such-format run",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("->HELLO."), "{stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}
