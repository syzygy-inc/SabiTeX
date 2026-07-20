//! I/O abstraction.
//!
//! tex.web Part 3 binds TeX to Pascal files; this port routes *all* file
//! and terminal traffic through the [`TexFs`] / [`Terminal`] traits so the
//! engine is byte-identical on native and wasm32 targets. File-name
//! *resolution* (the kpathsea role) happens outside the engine: by the time
//! a name reaches `TexFs` it is a plain key.
//!
//! Files are read whole (TeX inputs are small; this removes Seek from the
//! interface). Output files are written whole when closed; incremental
//! streaming can be added behind the same trait if profiling demands it.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

/// What kind of input file is being requested (drives default extensions
/// and search paths in the resolver layer).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A TeX source file (`\input`, command line).
    Tex,
    /// A TeX font metric file.
    Tfm,
    /// A format file (M5+).
    Fmt,
    /// `\openin` data file.
    OpenIn,
    /// A native (OpenType/TrueType) font file (M7, XeTeX).
    Font,
}

/// What kind of output file is being produced.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OutKind {
    /// The transcript (log) file.
    Log,
    /// The DVI/XDV output file.
    Dvi,
    /// `\openout` data file.
    OpenOut,
    /// A format file produced by `\dump` (M5).
    Fmt,
}

/// The engine's view of a file system.
pub trait TexFs {
    /// Returns the entire contents of `name`, or `None` if it does not
    /// exist (the engine then follows tex.web's missing-file paths).
    fn read_file(&mut self, name: &str, kind: FileKind) -> Option<Vec<u8>>;

    /// Stores an output file. Returns `false` if the host refused it.
    fn write_file(&mut self, name: &str, kind: OutKind, data: &[u8]) -> bool;

    /// Appends to an output file. Backends that support it (the CLI)
    /// return true, enabling streaming transcripts; the default keeps
    /// everything buffered in the engine until the final write.
    fn append_file(&mut self, _name: &str, _kind: OutKind, _data: &[u8]) -> bool {
        false
    }

    /// Retrieves (and removes) a previously written output, if the backing
    /// store keeps them (memory file systems do; the native one doesn't).
    fn take_output(&mut self, _name: &str) -> Option<Vec<u8>> {
        None
    }
}

/// The engine's view of the user's terminal (tex.web §71 `term_input` etc.).
pub trait Terminal {
    /// Writes a chunk of text (no newline added).
    fn write_str(&mut self, s: &str);

    /// Reads one line of input; `None` means end of file on the terminal.
    fn read_line(&mut self) -> Option<String>;
}

/// An in-memory file system, used by tests and as the wasm backing store.
#[derive(Default)]
pub struct MemFs {
    /// Input files available to the engine.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Output files produced by the engine.
    pub outputs: BTreeMap<String, Vec<u8>>,
}

impl TexFs for MemFs {
    fn read_file(&mut self, name: &str, _kind: FileKind) -> Option<Vec<u8>> {
        // Files the job has written (e.g. via \openout/\write) can be read
        // back with \input or \openin, exactly as on a real file system.
        self.files
            .get(name)
            .or_else(|| self.outputs.get(name))
            .cloned()
    }

    fn write_file(&mut self, name: &str, _kind: OutKind, data: &[u8]) -> bool {
        self.outputs.insert(name.to_string(), data.to_vec());
        true
    }

    fn take_output(&mut self, name: &str) -> Option<Vec<u8>> {
        self.outputs.remove(name)
    }
}

/// A scriptable terminal that captures output, for tests and batch runs.
/// The output buffer is shared so callers can inspect it while the engine
/// owns the `Terminal`.
pub struct CaptureTerminal {
    buf: Rc<RefCell<String>>,
    script: VecDeque<String>,
}

impl CaptureTerminal {
    /// Creates a terminal with the given scripted input lines; returns the
    /// terminal and a shared handle to everything it prints.
    pub fn new<I: IntoIterator<Item = String>>(
        script: I,
    ) -> (CaptureTerminal, Rc<RefCell<String>>) {
        let buf = Rc::new(RefCell::new(String::new()));
        (
            CaptureTerminal {
                buf: Rc::clone(&buf),
                script: script.into_iter().collect(),
            },
            buf,
        )
    }
}

impl Terminal for CaptureTerminal {
    fn write_str(&mut self, s: &str) {
        self.buf.borrow_mut().push_str(s);
    }

    fn read_line(&mut self) -> Option<String> {
        self.script.pop_front()
    }
}
