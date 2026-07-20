//! Native CLI driver.
//!
//! Usage mirrors the classic TeX command line:
//!   sabitex [--fmt <file.fmt>] [<first line>]
//! where <first line> is what you would type at the ** prompt — a file
//! name (`story`), `&format story`, `*\input etex-file`, or raw commands
//! (`\relax ...`). With no argument the ** prompt reads from stdin.

use std::collections::HashMap;
use std::io::Write;

use sabitex_core::io::{FileKind, OutKind, Terminal, TexFs};
use sabitex_core::{Engine, Sizes, BANNER};

/// `TexFs` over the real file system: the working directory first, then
/// an installed TeX Live — resolved through its ls-R databases (built
/// once from a single `kpsewhich -var-value` call), with per-file
/// kpsewhich as the fallback for anything outside the index.
struct NativeFs {
    cache: HashMap<String, Option<String>>,
    /// name -> full path, from the ls-R databases; None until built.
    index: Option<HashMap<String, String>>,
}

impl NativeFs {
    fn new() -> Self {
        NativeFs {
            cache: HashMap::new(),
            index: None,
        }
    }

    /// kpathsea-like priority of a directory inside a tree: standard
    /// LaTeX inputs beat format-specific ones (tex/cslatex etc. sorts
    /// BEFORE tex/latex in ls-R, so plain first-wins picked the cslatex
    /// fonttext.cfg and silently broke the TS1 encoding). doc/ and
    /// source/ are not on any input path at all.
    fn dir_priority(dir: &str) -> Option<u8> {
        let d = dir.to_ascii_lowercase();
        if d.contains("/doc/") || d.contains("/source/") {
            return None;
        }
        if d.contains("/tex/latex/") {
            Some(0)
        } else if d.contains("/tex/generic/") {
            Some(1)
        } else if d.contains("/tex/") || d.contains("/fonts/") || d.contains("/web2c/") {
            Some(2)
        } else {
            Some(3)
        }
    }

    /// Parses one ls-R database ("./dir:" headers followed by entries).
    /// Within and across trees, the better `dir_priority` wins; equal
    /// priority keeps the earlier entry (tree order HOME, LOCAL, DIST).
    fn index_ls_r(root: &str, map: &mut HashMap<String, (u8, String)>) {
        let Ok(data) = std::fs::read_to_string(format!("{root}/ls-R")) else {
            return;
        };
        let mut dir = String::new();
        let mut prio: Option<u8> = Some(3);
        for line in data.lines() {
            if let Some(d) = line.strip_suffix(':') {
                dir = format!("{root}/{}", d.trim_start_matches("./"));
                prio = Self::dir_priority(&dir);
            } else if !line.is_empty() && !line.starts_with('%') {
                let Some(pr) = prio else { continue };
                let better = match map.get(line) {
                    Some(&(old, _)) => pr < old,
                    None => true,
                };
                if better {
                    map.insert(line.to_string(), (pr, format!("{dir}/{line}")));
                }
            }
        }
    }

    fn build_index(&mut self) -> &HashMap<String, String> {
        if self.index.is_none() {
            let mut map: HashMap<String, (u8, String)> = HashMap::new();
            let roots = std::process::Command::new("kpsewhich")
                .args(["-var-value", "TEXMFDBS"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            for root in roots.split([';', ',']) {
                let root = root.trim().trim_matches(['{', '}', '!']).replace('\\', "/");
                if !root.is_empty() {
                    Self::index_ls_r(&root, &mut map);
                }
            }
            self.index = Some(map.into_iter().map(|(k, (_, v))| (k, v)).collect());
        }
        self.index.as_ref().unwrap()
    }

    fn kpsewhich(&mut self, name: &str) -> Option<String> {
        if let Some(hit) = self.cache.get(name) {
            return hit.clone();
        }
        let mut found = self
            .build_index()
            .get(name)
            .filter(|p| std::path::Path::new(p).is_file())
            .cloned();
        if found.is_none() {
            // Outside the databases (generated files, aliases, fontmaps
            // resolved by kpathsea rules): ask the real kpsewhich.
            found = std::process::Command::new("kpsewhich")
                .arg(name)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|p| !p.is_empty());
        }
        self.cache.insert(name.to_string(), found.clone());
        found
    }
}

impl TexFs for NativeFs {
    fn read_file(&mut self, name: &str, kind: FileKind) -> Option<Vec<u8>> {
        if let Ok(data) = std::fs::read(name) {
            if std::env::var("SABITEX_TRACE_FILES").is_ok() {
                eprintln!("FILE {name} => (cwd)");
            }
            return Some(data);
        }
        if !matches!(kind, FileKind::Fmt) {
            let path = self.kpsewhich(name)?;
            if std::env::var("SABITEX_TRACE_FILES").is_ok() {
                eprintln!("FILE {name} => {path}");
            }
            return std::fs::read(path).ok();
        }
        None
    }

    fn write_file(&mut self, name: &str, _kind: OutKind, data: &[u8]) -> bool {
        std::fs::write(name, data).is_ok()
    }

    fn append_file(&mut self, name: &str, _kind: OutKind, data: &[u8]) -> bool {
        use std::io::Write as _;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(name)
            .and_then(|mut f| f.write_all(data))
            .is_ok()
    }
}

/// `Terminal` over stdin/stdout, with an optional injected first line
/// (the command-line remainder, exactly like real TeX's ** line).
struct NativeTerminal {
    first: Option<String>,
}

impl Terminal for NativeTerminal {
    fn write_str(&mut self, s: &str) {
        print!("{s}");
        let _ = std::io::stdout().flush();
    }

    fn read_line(&mut self) -> Option<String> {
        if let Some(line) = self.first.take() {
            return Some(line);
        }
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        }
    }
}

/// Unix seconds -> local-ish (UTC) civil date + minutes past midnight.
fn civil_from_unix(secs: i64) -> (i32, i32, i32, i32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as i32, d as i32, (rem / 60) as i32)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{BANNER}");
        return;
    }
    let mut fmt_path: Option<String> = None;
    let mut interaction: Option<u8> = None;
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--fmt" {
            fmt_path = it.next();
        } else if let Some(p) = a.strip_prefix("--fmt=") {
            fmt_path = Some(p.to_string());
        } else if let Some(m) = a
            .strip_prefix("--interaction=")
            .or_else(|| a.strip_prefix("-interaction="))
        {
            interaction = match m {
                "batchmode" => Some(0),
                "nonstopmode" => Some(1),
                "scrollmode" => Some(2),
                "errorstopmode" => Some(3),
                _ => {
                    eprintln!("sabitex: unknown interaction mode {m}");
                    std::process::exit(1);
                }
            };
        } else {
            rest.push(a);
        }
    }
    // The remainder is the ** line. A bare name becomes \input <name>
    // unless it already starts with \, &, or * (the TeX conventions).
    let first = if rest.is_empty() {
        None
    } else {
        let joined = rest.join(" ");
        if joined.starts_with('\\') || joined.starts_with('&') || joined.starts_with('*') {
            Some(joined)
        } else {
            Some(format!("\\input {joined}"))
        }
    };
    let mut engine = Engine::new(
        Sizes::production(),
        Box::new(NativeFs::new()),
        Box::new(NativeTerminal { first }),
    );
    if let Some(p) = fmt_path {
        match std::fs::read(&p) {
            Ok(bytes) => {
                if let Err(e) = engine.load_fmt(&bytes) {
                    eprintln!("sabitex: cannot load format {p}: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("sabitex: cannot read format {p}: {e}");
                std::process::exit(1);
            }
        }
    }
    if let Some(mode) = interaction {
        engine.set_interaction(mode);
    }
    // Real local time (the engine's own default stays fixed for
    // deterministic tests; the CLI opts in).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let (y, mo, d, min) = civil_from_unix(now);
    engine.set_date_and_time(y, mo, d, min);
    let result = engine.run_terminal_job();
    // Final transcript write (partial chunks were streamed during the
    // job; this appends the tail).
    engine.write_log_file();
    if let Err(e) = result {
        eprintln!("sabitex: {e}");
        std::process::exit(1);
    }
}
