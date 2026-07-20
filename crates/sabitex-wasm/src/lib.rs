//! SabiTeX WebAssembly bindings.
//!
//! A hand-rolled C ABI (no wasm-bindgen, keeping the dependency-zero
//! policy): the host allocates buffers with `sabitex_alloc`, registers
//! virtual files with `sabitex_vfs_add` (the format file is a virtual
//! file named in `sabitex_load_fmt`), runs `sabitex_compile`, and reads
//! the outputs back with `sabitex_output`.
//!
//! Missing-file protocol (SwiftLaTeX style): file names the engine
//! asked for but the VFS did not have are collected; the host fetches
//! them, adds them to the VFS, and re-runs.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use sabitex_core::io::{FileKind, OutKind, Terminal, TexFs};
use sabitex_core::{Engine, Sizes};

thread_local! {
    static PREPARED: RefCell<Option<Prepared>> = const { RefCell::new(None) };
    static PANIC_MSG: RefCell<String> = const { RefCell::new(String::new()) };
    static STATS: RefCell<String> = const { RefCell::new(String::new()) };
    static CLOCK: RefCell<Option<(i32, i32, i32, i32)>> = const { RefCell::new(None) };
    static VFS: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    static MISSING: Rc<RefCell<BTreeSet<String>>> = Rc::new(RefCell::new(BTreeSet::new()));
    static OUTPUTS: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    static TERM_OUT: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
}

/// The engine's file system: the wasm VFS, with misses recorded.
struct WasmFs {
    missing: Rc<RefCell<BTreeSet<String>>>,
}

impl TexFs for WasmFs {
    fn read_file(&mut self, name: &str, _kind: FileKind) -> Option<Vec<u8>> {
        let hit = VFS.with(|v| v.borrow().get(name).cloned());
        if hit.is_none() {
            // Outputs written earlier in the job (e.g. the .aux) can be
            // read back.
            let out = OUTPUTS.with(|o| o.borrow().get(name).cloned());
            if out.is_some() {
                return out;
            }
            self.missing.borrow_mut().insert(name.to_string());
        }
        hit
    }

    fn write_file(&mut self, name: &str, _kind: OutKind, data: &[u8]) -> bool {
        OUTPUTS.with(|o| {
            o.borrow_mut().insert(name.to_string(), data.to_vec());
        });
        true
    }
}

/// Batch terminal: one injected first line, then a bounded number of
/// empty lines, then EOF; output captured.
///
/// The empty lines matter: LaTeX's missing-file prompt ("Enter file
/// name:") accepts an empty line as "proceed without the file", so one
/// compile pass can discover ALL missing files instead of dying at the
/// first one (which would cost one full pass per file).
struct WasmTerminal {
    /// Shared slot: `sabitex_run` deposits the first line here after the
    /// engine (and this terminal) were built by `sabitex_prepare`.
    first: Rc<RefCell<Option<String>>>,
    grace: u32,
    out: Rc<RefCell<Vec<u8>>>,
}

impl Terminal for WasmTerminal {
    fn write_str(&mut self, s: &str) {
        self.out.borrow_mut().extend_from_slice(s.as_bytes());
    }

    fn read_line(&mut self) -> Option<String> {
        if let Some(f) = self.first.borrow_mut().take() {
            return Some(f);
        }
        if self.grace > 0 {
            self.grace -= 1;
            return Some(String::new());
        }
        None
    }
}

/// An engine prepared by `sabitex_prepare`, waiting for its first line.
struct Prepared {
    engine: Engine,
    first_slot: Rc<RefCell<Option<String>>>,
}

/// Installs a panic hook that records the message so the host can read
/// it after a trap (output name "<panic>").
#[no_mangle]
pub extern "C" fn sabitex_init_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        PANIC_MSG.with(|p| *p.borrow_mut() = msg);
    }));
}

/// Sets the clock used by subsequent compiles (\year etc.). Without a
/// call the engine keeps its deterministic fixed date.
#[no_mangle]
pub extern "C" fn sabitex_set_time(year: i32, month: i32, day: i32, minutes: i32) {
    CLOCK.with(|c| *c.borrow_mut() = Some((year, month, day, minutes)));
}

/// Allocates a buffer the host can write into.
#[no_mangle]
pub extern "C" fn sabitex_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Frees a buffer from `sabitex_alloc` (same length!).
#[no_mangle]
pub unsafe extern "C" fn sabitex_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

unsafe fn slice_from(ptr: *const u8, len: usize) -> &'static [u8] {
    std::slice::from_raw_parts(ptr, len)
}

/// Registers (or replaces) a virtual file.
#[no_mangle]
pub unsafe extern "C" fn sabitex_vfs_add(
    name_ptr: *const u8,
    name_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) {
    let name = String::from_utf8_lossy(slice_from(name_ptr, name_len)).into_owned();
    let data = slice_from(data_ptr, data_len).to_vec();
    VFS.with(|v| {
        v.borrow_mut().insert(name, data);
    });
}

/// Runs one job. `first_line` is the ** line (e.g. "\\input doc").
/// `fmt_name` is the VFS name of a format to preload ("" for INITEX).
/// Returns 0 on success, 1 on error, 2 when the format failed to load.
/// Builds an engine and loads the named format (empty = INITEX). The
/// prepared engine waits for `sabitex_run`; hosts can call this ahead of
/// time (and can time undump separately from the job).
///
/// # Safety
/// `fmt_ptr..fmt_ptr+fmt_len` must be a live allocation.
#[no_mangle]
pub unsafe extern "C" fn sabitex_prepare(fmt_ptr: *const u8, fmt_len: usize) -> u32 {
    let fmt_name = String::from_utf8_lossy(slice_from(fmt_ptr, fmt_len)).into_owned();
    let missing = MISSING.with(|m| {
        m.borrow_mut().clear();
        Rc::clone(m)
    });
    let term_out = TERM_OUT.with(|t| {
        t.borrow_mut().clear();
        Rc::clone(t)
    });
    let first_slot = Rc::new(RefCell::new(None));
    let fs = WasmFs { missing };
    let term = WasmTerminal {
        first: Rc::clone(&first_slot),
        grace: 1000,
        out: term_out,
    };
    let mut engine = Engine::new(Sizes::production(), Box::new(fs), Box::new(term));
    if let Some((y, mo, d, min)) = CLOCK.with(|c| *c.borrow()) {
        engine.set_date_and_time(y, mo, d, min);
    }
    if !fmt_name.is_empty() {
        let Some(fmt) = VFS.with(|v| v.borrow().get(&fmt_name).cloned()) else {
            return 2;
        };
        if engine.load_fmt(&fmt).is_err() {
            return 2;
        }
    }
    PREPARED.with(|p| *p.borrow_mut() = Some(Prepared { engine, first_slot }));
    0
}

/// Runs the prepared engine on the given first line. Consumes the
/// prepared engine (each job needs a fresh `sabitex_prepare`).
///
/// # Safety
/// `first_ptr..first_ptr+first_len` must be a live allocation.
#[no_mangle]
pub unsafe extern "C" fn sabitex_run(first_ptr: *const u8, first_len: usize) -> u32 {
    let first = String::from_utf8_lossy(slice_from(first_ptr, first_len)).into_owned();
    let Some(prepared) = PREPARED.with(|p| p.borrow_mut().take()) else {
        return 2;
    };
    let Prepared {
        mut engine,
        first_slot,
    } = prepared;
    *first_slot.borrow_mut() = Some(first);
    OUTPUTS.with(|o| o.borrow_mut().clear());
    let result = engine.run_terminal_job();
    STATS.with(|t| {
        *t.borrow_mut() = format!(
            "{{\"errorCount\":{},\"ok\":{}}}",
            engine.error_count,
            result.is_ok(),
        );
    });
    let log_name = format!(
        "{}.log",
        engine.job_name.clone().unwrap_or_else(|| "texput".into())
    );
    let log = std::mem::take(&mut engine.log);
    let term_bytes = TERM_OUT.with(|t| t.borrow().clone());
    OUTPUTS.with(|o| {
        o.borrow_mut().insert(log_name, log);
        o.borrow_mut().insert("<terminal>".into(), term_bytes);
    });
    u32::from(result.is_err())
}

/// One-shot compile: prepare + run.
///
/// # Safety
/// Both buffers must be live allocations.
#[no_mangle]
pub unsafe extern "C" fn sabitex_compile(
    first_ptr: *const u8,
    first_len: usize,
    fmt_ptr: *const u8,
    fmt_len: usize,
) -> u32 {
    let rc = sabitex_prepare(fmt_ptr, fmt_len);
    if rc != 0 {
        return rc;
    }
    sabitex_run(first_ptr, first_len)
}

fn take_output(name: &str) -> Vec<u8> {
    if name == "<panic>" {
        return PANIC_MSG.with(|p| p.borrow().clone().into_bytes());
    }
    if name == "<outputs>" {
        // B3: newline-separated names of every output the last compile
        // produced (plus the virtual <terminal>/<missing> channels).
        return OUTPUTS.with(|o| {
            o.borrow()
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes()
        });
    }
    if name == "<stats>" {
        return STATS.with(|t| t.borrow().clone().into_bytes());
    }
    if name == "<missing>" {
        let list = MISSING.with(|m| {
            m.borrow()
                .iter()
                .filter(|n| {
                    // A miss on "x" satisfied later as "x.tex" (the
                    // \input probing order) is not a real miss.
                    let probed = format!("{n}.tex");
                    !VFS.with(|v| v.borrow().contains_key(&probed))
                        && !n.ends_with(".aux")
                        && !n.ends_with(".log")
                })
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        });
        return list.into_bytes();
    }
    OUTPUTS.with(|o| o.borrow().get(name).cloned().unwrap_or_default())
}

thread_local! {
    static LAST_OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Fetches a named output ("doc.dvi", "doc.log", "<terminal>",
/// "<missing>"). Returns the byte length; the data pointer is obtained
/// with `sabitex_output_ptr` (valid until the next call).
#[no_mangle]
pub unsafe extern "C" fn sabitex_output_len(name_ptr: *const u8, name_len: usize) -> usize {
    let name = String::from_utf8_lossy(slice_from(name_ptr, name_len)).into_owned();
    let data = take_output(&name);
    let len = data.len();
    LAST_OUTPUT.with(|l| *l.borrow_mut() = data);
    len
}

#[no_mangle]
pub extern "C" fn sabitex_output_ptr() -> *const u8 {
    LAST_OUTPUT.with(|l| l.borrow().as_ptr())
}
