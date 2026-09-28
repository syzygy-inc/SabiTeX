//! Contract case ledger (SabiSeries quality assurance `qa-v0`; evidence
//! states PASS / FAIL / BLOCKED / NOT-RUN). The same protocol as the
//! `*-qa` crates of SabiFace, SabiDVI and SabiRender.
//!
//! An oracle test creates one `Case` and
//!
//! - calls `blocked(reason)` when the reference environment (a tool, a
//!   TeX Live file) is missing (BLOCKED). A required case then fails under
//!   `SABI_STRICT_TESTS`; an optional one only when `SABI_STRICT_OPTIONAL`
//!   is also set. Whether a case is optional is a property of the case,
//!   never inferred from the reason text;
//! - calls `tool_failed(reason)` when the reference tool ran but failed
//!   (exit status, missing output): FAIL, always;
//! - calls `compared()` after each comparison and `done()` at the end
//!   (PASS).
//!
//! A case that ends without `done()` or `blocked()` is recorded as
//! NOT-RUN and fails under strict mode (this catches `return` and
//! `if let Some` paths that silently skip a comparison).
//!
//! The ledger lives in the directory named by `SABI_QA_LEDGER`, or
//! `target/qa-ledger`, one line per case execution:
//! `<case-id>\t<STATUS>\tcomparisons=<n>\t<contracts>\t<optional|required>\t<reason>`.
//! `scripts/qa-ledger.sh` reconciles it with `specification/cases.md`.

use std::cell::Cell;
use std::io::Write;
use std::path::PathBuf;

pub struct Case {
    id: &'static str,
    contracts: &'static [&'static str],
    optional: bool,
    comparisons: Cell<usize>,
    finished: Cell<bool>,
}

fn strict() -> bool {
    std::env::var_os("SABI_STRICT_TESTS").is_some()
}

fn strict_optional() -> bool {
    std::env::var_os("SABI_STRICT_OPTIONAL").is_some()
}

fn ledger_dir() -> PathBuf {
    match std::env::var_os("SABI_QA_LEDGER") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("qa-ledger"),
    }
}

fn record(line: &str) {
    let dir = ledger_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join(format!("{}.tsv", env!("CARGO_PKG_NAME")));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        // Cases run on parallel test threads; one write call per line keeps
        // the appends whole (a formatted write would issue several).
        let _ = f.write_all(format!("{line}\n").as_bytes());
    }
}

impl Case {
    pub fn required(id: &'static str, contracts: &'static [&'static str]) -> Case {
        Case::new(id, contracts, false)
    }

    /// A case that depends on an optional part of the reference
    /// environment; the ledger marks it `optional`.
    pub fn optional(id: &'static str, contracts: &'static [&'static str]) -> Case {
        Case::new(id, contracts, true)
    }

    fn new(id: &'static str, contracts: &'static [&'static str], optional: bool) -> Case {
        Case {
            id,
            contracts,
            optional,
            comparisons: Cell::new(0),
            finished: Cell::new(false),
        }
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    fn line(&self, status: &str, reason: &str) -> String {
        format!(
            "{}\t{}\tcomparisons={}\t{}\t{}\t{}",
            self.id,
            status,
            self.comparisons.get(),
            self.contracts.join(","),
            if self.optional {
                "optional"
            } else {
                "required"
            },
            reason.replace(['\t', '\n'], " ")
        )
    }

    /// The reference environment is missing. A required case fails under
    /// strict mode; an optional one under strict + optional.
    pub fn blocked(&self, reason: &str) {
        self.finished.set(true);
        record(&self.line("BLOCKED", reason));
        let fail = strict() && (!self.optional || strict_optional());
        if fail {
            panic!(
                "{}: required reference environment is missing: {reason}",
                self.id
            );
        }
        eprintln!(
            "{}: BLOCKED{}: {reason}",
            self.id,
            if self.optional { " (optional)" } else { "" }
        );
    }

    /// The reference tool is present but failed (exit status, missing
    /// output): a test failure, not a missing environment.
    pub fn tool_failed(&self, reason: &str) -> ! {
        self.finished.set(true);
        record(&self.line("FAIL", reason));
        panic!("{}: reference tool failed: {reason}", self.id)
    }

    /// One comparison completed.
    pub fn compared(&self) {
        self.comparisons.set(self.comparisons.get() + 1);
    }

    pub fn compared_n(&self, n: usize) {
        self.comparisons.set(self.comparisons.get() + n);
    }

    /// All planned comparisons completed. Zero comparisons is NOT-RUN.
    pub fn done(&self) {
        self.finished.set(true);
        if self.comparisons.get() == 0 {
            record(&self.line("NOT-RUN", "done() without any comparison"));
            panic!("{}: finished without any comparison", self.id);
        }
        record(&self.line("PASS", ""));
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        if self.finished.get() {
            return;
        }
        if std::thread::panicking() {
            record(&self.line("FAIL", "assertion failed"));
            return;
        }
        record(&self.line("NOT-RUN", "returned before done() or blocked()"));
        if strict() {
            panic!(
                "{}: test returned without completing its comparisons",
                self.id
            );
        }
        eprintln!("{}: NOT-RUN (returned early)", self.id);
    }
}

/// Runs a reference tool. `None` when it cannot be started (the caller
/// decides BLOCKED); `tool_failed` when it started and failed.
pub fn run_tool(
    case: &Case,
    program: &str,
    args: &[&str],
    cwd: Option<&std::path::Path>,
) -> Option<std::process::Output> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    match cmd.output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => case.tool_failed(&format!("{program}: {e}")),
        Ok(out) => {
            if !out.status.success() {
                case.tool_failed(&format!(
                    "{program} {} exited with {}: {}",
                    args.join(" "),
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Some(out)
        }
    }
}

/// Locates a file through `kpsewhich`. `None` when kpsewhich itself is
/// missing or does not know the file.
pub fn kpsewhich(name: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("kpsewhich")
        .arg(name)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// Compares a DVI file's `dvitype` listing with a reference listing, line
/// by line. The first line (the DVItype banner, which carries the TeX Live
/// version) is never compared; with `mask_comment` the DVI preamble
/// comment line (`' TeX output <date>'`) is skipped too. Returns the
/// number of lines compared, or `None` when `dvitype` is not installed
/// (the caller records BLOCKED). Differences are reported and counted as
/// a failure of the case.
pub fn dvitype_matches(
    case: &Case,
    dvi_path: &std::path::Path,
    reference: &str,
    mask_comment: bool,
) -> Option<usize> {
    let dir = dvi_path.parent()?;
    let file = dvi_path.file_name()?.to_string_lossy().into_owned();
    let out = run_tool(
        case,
        "dvitype",
        &[
            "-output-level=2",
            "-dpi=72.27",
            "-page-start=*.*.*.*.*.*.*.*.*.*",
            &file,
        ],
        Some(dir),
    )?;
    let ours = String::from_utf8_lossy(&out.stdout).into_owned();
    let keep = |l: &&str| !(mask_comment && l.starts_with('\''));
    let a: Vec<&str> = ours.lines().skip(1).filter(keep).collect();
    let b: Vec<&str> = reference.lines().skip(1).filter(keep).collect();
    let mut diffs = 0;
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if x.trim_end() != y.trim_end() {
            diffs += 1;
            if diffs <= 10 {
                eprintln!("dvitype line {}:\n  ours: {x}\n  ref:  {y}", i + 2);
            }
        }
    }
    if a.len() != b.len() {
        eprintln!("dvitype line counts: ours {} vs ref {}", a.len(), b.len());
        diffs += 1;
    }
    assert_eq!(
        diffs,
        0,
        "{}: dvitype listing differs from the reference",
        case.id()
    );
    Some(a.len().min(b.len()))
}
