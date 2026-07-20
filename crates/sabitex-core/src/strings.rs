//! String handling: the string pool.
//!
//! Ports tex.web Part 4 (§38-§53). All strings live in one `str_pool`
//! array; string number `s` is `str_pool[str_start[s] .. str_start[s+1]]`.
//! String numbers 0..255 are the single-character strings, with unprintable
//! characters stored in `^^X` / `^^xx` form (§48-§49).
//!
//! Deviations from tex.web, recorded in specification/architecture.md:
//! - Pool elements are UTF-16 code units (`u16`), following XeTeX, since the
//!   engine is Unicode-native. (XeTeX's virtual strings for the supplementary
//!   planes arrive with M7.)
//! - There is no TEX.POOL file: the WEB string constants are interned by the
//!   engine at start-up as needed, via [`StringPool::intern`]. String
//!   *numbers* therefore differ from tangled tex.web; nothing in the engine
//!   depends on their absolute values (the format file is custom anyway).

use crate::error::{TexInterrupt, TexResult};
use crate::types::StrNumber;

/// The string pool (tex.web §39).
pub struct StringPool {
    /// `str_pool`: the characters, as UTF-16 code units.
    pool: Vec<u16>,
    /// `str_start`: starting pointers; `str_start[str_ptr]` is the start of
    /// the string currently being built, so `len() == str_ptr + 1`.
    str_start: Vec<usize>,
    /// `pool_size`: maximum number of pool units.
    pool_size: usize,
    /// `max_strings`: maximum number of strings.
    max_strings: usize,
    /// `init_str_ptr`: value of `str_ptr` after initialization (§43).
    pub init_str_ptr: usize,
    /// `init_pool_ptr`: value of `pool_ptr` after initialization (§43).
    pub init_pool_ptr: usize,
}

impl StringPool {
    /// Initializes the pool with the 256 single-character strings
    /// (tex.web §47-§49, `get_strings_started`).
    pub fn new(pool_size: usize, max_strings: usize) -> StringPool {
        let mut sp = StringPool {
            pool: Vec::with_capacity(1024),
            str_start: vec![0],
            pool_size,
            max_strings,
            init_str_ptr: 0,
            init_pool_ptr: 0,
        };
        // §48: make the first 256 strings.
        for k in 0u16..=255 {
            if Self::cannot_be_printed(k) {
                sp.pool.push(u16::from(b'^'));
                sp.pool.push(u16::from(b'^'));
                if k < 0o100 {
                    sp.pool.push(k + 0o100);
                } else if k < 0o200 {
                    sp.pool.push(k - 0o100);
                } else {
                    // §49: codes 128..255 are rendered ^^80..^^ff.
                    sp.pool.push(lc_hex(k / 16));
                    sp.pool.push(lc_hex(k % 16));
                }
            } else {
                sp.pool.push(k);
            }
            sp.str_start.push(sp.pool.len());
        }
        sp.init_str_ptr = sp.str_ptr();
        sp.init_pool_ptr = sp.pool.len();
        sp
    }

    /// §49: character `k` cannot be printed (`k < " " or k > "~"`).
    fn cannot_be_printed(k: u16) -> bool {
        k < u16::from(b' ') || k > u16::from(b'~')
    }

    /// §1309: dump the string pool.
    pub fn dump(&self, w: &mut crate::fmt::FmtWriter) {
        w.u16s(&self.pool);
        w.len_of(self.str_start.len());
        for &s in &self.str_start {
            w.u64(s as u64);
        }
        w.u64(self.init_str_ptr as u64);
        w.u64(self.init_pool_ptr as u64);
    }

    /// §1310: undump the string pool.
    pub fn undump(&mut self, r: &mut crate::fmt::FmtReader) -> crate::fmt::FmtResult<()> {
        let pool = r.u16s()?;
        if pool.len() > self.pool_size {
            return Err("string pool overflow");
        }
        let n = r.seq_len()?;
        if n > self.max_strings + 1 {
            return Err("max strings overflow");
        }
        let mut str_start = Vec::with_capacity(n);
        for _ in 0..n {
            str_start.push(r.u64()? as usize);
        }
        self.pool = pool;
        self.str_start = str_start;
        // §1310: the dumped values are discarded — after loading a format,
        // "initialization" means the state just undumped.
        let (_, _) = (r.u64()?, r.u64()?);
        self.init_str_ptr = self.str_ptr();
        self.init_pool_ptr = self.pool.len();
        Ok(())
    }

    /// `str_ptr`: the number of the current string being created.
    pub fn str_ptr(&self) -> usize {
        self.str_start.len() - 1
    }

    /// `pool_ptr`: first unused position in `str_pool`.
    pub fn pool_ptr(&self) -> usize {
        self.pool.len()
    }

    /// `length(s)` (tex.web §40).
    pub fn length(&self, s: StrNumber) -> usize {
        self.str_start[s as usize + 1] - self.str_start[s as usize]
    }

    /// `cur_length` (tex.web §41): length of the string being built.
    pub fn cur_length(&self) -> usize {
        self.pool.len() - self.str_start[self.str_ptr()]
    }

    /// The UTF-16 units of string `s`.
    pub fn str(&self, s: StrNumber) -> &[u16] {
        &self.pool[self.str_start[s as usize]..self.str_start[s as usize + 1]]
    }

    /// The units of the yet-unmade current string.
    pub fn cur_str(&self) -> &[u16] {
        &self.pool[self.str_start[self.str_ptr()]..]
    }

    /// Decodes string `s` for host-side use (tests, diagnostics).
    pub fn text(&self, s: StrNumber) -> String {
        String::from_utf16_lossy(self.str(s))
    }

    /// `str_room(l)` (tex.web §42): ensure room for `l` more units.
    pub fn str_room(&self, l: usize) -> TexResult<()> {
        if self.pool.len() + l > self.pool_size {
            return Err(TexInterrupt::Overflow {
                what: "pool size",
                size: (self.pool_size - self.init_pool_ptr) as i32,
            });
        }
        Ok(())
    }

    /// Whether `append_char` may be called for one more scalar value.
    pub fn has_room(&self, l: usize) -> bool {
        self.pool.len() + l <= self.pool_size
    }

    /// Takes the current (unfinished) string accumulated via `append_char`,
    /// resetting the pool: `special_out`'s `pool_ptr:=str_start[str_ptr]`
    /// (§1368).
    pub fn take_cur_string(&mut self) -> Vec<u16> {
        let start = self.str_start[self.str_ptr()];
        self.pool.split_off(start)
    }

    /// `append_char(c)` (tex.web §42): put a character at the end of
    /// `str_pool`. The room test is the caller's responsibility, as in
    /// tex.web. Supplementary-plane values are stored as surrogate pairs.
    pub fn append_char(&mut self, c: i32) {
        debug_assert!((0..=0x10FFFF).contains(&c));
        if c <= 0xFFFF {
            self.pool.push(c as u16);
        } else {
            let v = (c as u32) - 0x10000;
            self.pool.push(0xD800 + (v >> 10) as u16);
            self.pool.push(0xDC00 + (v & 0x3FF) as u16);
        }
    }

    /// `flush_char` (tex.web §42): forget the last unit in the pool.
    pub fn flush_char(&mut self) {
        self.pool.pop();
    }

    /// `make_string` (tex.web §43): the current string officially enters
    /// the pool; returns its number.
    pub fn make_string(&mut self) -> TexResult<StrNumber> {
        if self.str_ptr() == self.max_strings {
            return Err(TexInterrupt::Overflow {
                what: "number of strings",
                size: (self.max_strings - self.init_str_ptr) as i32,
            });
        }
        self.str_start.push(self.pool.len());
        Ok((self.str_ptr() - 1) as StrNumber)
    }

    /// `flush_string` (tex.web §44): destroy the most recently made string.
    pub fn flush_string(&mut self) {
        self.str_start.pop();
        self.pool.truncate(*self.str_start.last().unwrap());
    }

    /// `str_eq_buf(s, k)` (tex.web §45): compare string `s` with a slice of
    /// a buffer of the same length starting at `k`.
    pub fn str_eq_buf(&self, s: StrNumber, buffer: &[u16], k: usize) -> bool {
        let range = self.str_start[s as usize]..self.str_start[s as usize + 1];
        for (i, j) in range.enumerate() {
            if self.pool[j] != buffer[k + i] {
                return false;
            }
        }
        true
    }

    /// `str_eq_str(s, t)` (tex.web §46): compare two pool strings.
    pub fn str_eq_str(&self, s: StrNumber, t: StrNumber) -> bool {
        if self.length(s) != self.length(t) {
            return false;
        }
        self.str(s) == self.str(t)
    }

    /// Interns a host string as a pool string (replaces TEX.POOL loading;
    /// see module docs). Appends its UTF-16 units and calls `make_string`.
    pub fn intern(&mut self, s: &str) -> TexResult<StrNumber> {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.str_room(units.len())?;
        self.pool.extend_from_slice(&units);
        self.make_string()
    }

    /// Makes a string from the given units even while another string is
    /// being built (equivalent to the §260 "move current string up" dance).
    pub fn make_string_from(&mut self, units: &[u16]) -> TexResult<StrNumber> {
        self.str_room(units.len())?;
        let cur_start = self.str_start[self.str_ptr()];
        let pending: Vec<u16> = self.pool.split_off(cur_start);
        self.pool.extend_from_slice(units);
        let s = self.make_string()?;
        self.pool.extend_from_slice(&pending);
        Ok(s)
    }

    /// The pool contents from position `b` to the end (for `str_toks`).
    pub fn pool_suffix(&self, b: usize) -> &[u16] {
        &self.pool[b..]
    }

    /// Truncates the pool back to position `b` (`pool_ptr := b`, §464).
    pub fn pool_truncate(&mut self, b: usize) {
        self.pool.truncate(b);
    }
}

/// `app_lc_hex` digit (tex.web §48): `0..9 -> '0'.., 10..15 -> 'a'..`.
fn lc_hex(l: u16) -> u16 {
    if l < 10 {
        l + u16::from(b'0')
    } else {
        l - 10 + u16::from(b'a')
    }
}
