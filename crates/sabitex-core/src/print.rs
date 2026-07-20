//! On-line and off-line printing.
//!
//! Ports tex.web Part 5 (§54-§70) plus `print_scaled` (§103) from Part 7.
//! All printing flows through `print_char` / `print_ln`, dispatching on
//! `selector`. The `\write` streams (selectors 0..15) arrive with M1.

use crate::engine::Engine;
use crate::error::TexResult;
use crate::types::{Scaled, StrNumber, UnicodeChar, UNITY};

/// `no_print = 16`: makes data disappear (tex.web §54).
pub const NO_PRINT: u8 = 16;
/// `term_only = 17`: printing destined for the terminal only.
pub const TERM_ONLY: u8 = 17;
/// `log_only = 18`: printing destined for the transcript file only.
pub const LOG_ONLY: u8 = 18;
/// `term_and_log = 19`: the normal setting.
pub const TERM_AND_LOG: u8 = 19;
/// `pseudo = 20`: cyclic-buffer capture for `show_context`.
pub const PSEUDO: u8 = 20;
/// `new_string = 21`: deflect printing to the string pool.
pub const NEW_STRING: u8 = 21;
/// `max_selector = 21`.
pub const MAX_SELECTOR: u8 = 21;

/// Printing state (tex.web §54).
pub struct PrintState {
    /// `selector`: where to print a message.
    pub selector: u8,
    /// `dig`: digits in a number being output.
    pub dig: [u8; 23],
    /// `tally`: the number of characters recently printed.
    pub tally: i32,
    /// `term_offset`: characters on the current terminal line.
    pub term_offset: usize,
    /// `file_offset`: characters on the current transcript-file line.
    pub file_offset: usize,
    /// `trick_buf`: circular buffer for pseudoprinting (`error_line + 1` long).
    pub trick_buf: Vec<u16>,
    /// `trick_count`: threshold for pseudoprinting.
    pub trick_count: i32,
    /// `first_count`: another pseudoprinting variable.
    pub first_count: i32,
}

impl PrintState {
    /// §55: initialize the output routines.
    pub fn new(error_line: usize) -> PrintState {
        PrintState {
            selector: TERM_ONLY,
            dig: [0; 23],
            tally: 0,
            term_offset: 0,
            file_offset: 0,
            trick_buf: vec![0; error_line + 1],
            trick_count: 0,
            first_count: 0,
        }
    }
}

impl Engine {
    /// The `\newlinechar` integer parameter (§240 / §244).
    fn new_line_char(&self) -> i32 {
        self.eqtb.int_par(crate::eqtb::NEW_LINE_CHAR_CODE)
    }

    /// Sets `\newlinechar` directly (the §59 temporary-disable dance writes
    /// eqtb without going through `eq_define`).
    fn set_new_line_char_raw(&mut self, v: i32) {
        let p = self.eqtb.lay.int_base + crate::eqtb::NEW_LINE_CHAR_CODE;
        self.eqtb.set_int(p, v);
    }

    /// The `\escapechar` integer parameter (§243).
    fn escape_char(&self) -> i32 {
        self.eqtb.int_par(crate::eqtb::ESCAPE_CHAR_CODE)
    }

    /// `wterm` family (§56): one character to the terminal.
    fn wterm_char(&mut self, c: UnicodeChar) {
        let ch = char::from_u32(c as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
        let mut buf = [0u8; 4];
        self.term.write_str(ch.encode_utf8(&mut buf));
    }

    /// `wlog` family (§56): one character to the transcript file.
    fn wlog_char(&mut self, c: UnicodeChar) {
        let ch = char::from_u32(c as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
        let mut buf = [0u8; 4];
        self.log
            .extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    }

    /// `print_ln` (tex.web §57): prints an end-of-line. `tally` is not
    /// affected.
    pub fn print_ln(&mut self) {
        // A2: stream the transcript in ~16 KB line-aligned chunks.
        if self.log.len() - self.log_streamed > 16384 {
            self.flush_log_stream();
        }
        match self.prn.selector {
            TERM_AND_LOG => {
                self.term.write_str("\n");
                self.log.push(b'\n');
                self.prn.term_offset = 0;
                self.prn.file_offset = 0;
            }
            LOG_ONLY => {
                self.log.push(b'\n');
                self.prn.file_offset = 0;
            }
            TERM_ONLY => {
                self.term.write_str("\n");
                self.prn.term_offset = 0;
            }
            NO_PRINT | PSEUDO | NEW_STRING => {}
            j if j < 16 => {
                // §57: an end-of-line on \write stream j.
                self.write_buf[j as usize].push(b'\n');
            }
            _ => {}
        }
    }

    /// `print_char(s)` (tex.web §58): prints a single character. All
    /// printing comes through `print_ln` or `print_char`.
    pub fn print_char(&mut self, s: UnicodeChar) {
        if s == self.new_line_char() && self.prn.selector < PSEUDO {
            self.print_ln();
            return;
        }
        match self.prn.selector {
            TERM_AND_LOG => {
                self.wterm_char(s);
                self.wlog_char(s);
                self.prn.term_offset += 1;
                self.prn.file_offset += 1;
                if self.prn.term_offset == self.sizes.max_print_line {
                    self.term.write_str("\n");
                    self.prn.term_offset = 0;
                }
                if self.prn.file_offset == self.sizes.max_print_line {
                    self.log.push(b'\n');
                    self.prn.file_offset = 0;
                }
            }
            LOG_ONLY => {
                self.wlog_char(s);
                self.prn.file_offset += 1;
                if self.prn.file_offset == self.sizes.max_print_line {
                    self.print_ln();
                }
            }
            TERM_ONLY => {
                self.wterm_char(s);
                self.prn.term_offset += 1;
                if self.prn.term_offset == self.sizes.max_print_line {
                    self.print_ln();
                }
            }
            NO_PRINT => {}
            PSEUDO => {
                if self.prn.tally < self.prn.trick_count {
                    let i = (self.prn.tally as usize) % self.sizes.error_line;
                    self.prn.trick_buf[i] = s as u16;
                }
            }
            NEW_STRING => {
                // We drop characters if the string space is full (§58).
                if self.strings.has_room(1) {
                    self.strings.append_char(s);
                }
            }
            j if j < 16 => {
                // §58: a character on \write stream j (buffered; UTF-8).
                let ch = char::from_u32(s as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
                let mut buf = [0u8; 4];
                self.write_buf[j as usize].extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            _ => {}
        }
        self.prn.tally += 1;
    }

    /// `print(s)` (tex.web §59): prints string `s`; for `s < 256` this is
    /// the single character `s`, honoring `\newlinechar`.
    pub fn print(&mut self, s: StrNumber) {
        if s >= self.strings.str_ptr() as i32 || s < 0 {
            // "???" — this can't happen.
            self.print_chars("???");
            return;
        }
        if s < 256 {
            if self.prn.selector > PSEUDO {
                self.print_char(s); // internal strings are not expanded
                return;
            }
            if s == self.new_line_char() && self.prn.selector < PSEUDO {
                self.print_ln();
                return;
            }
            let nl = self.new_line_char();
            self.set_new_line_char_raw(-1); // temporarily disable it
            for j in 0..self.strings.length(s) {
                let c = i32::from(self.strings.str(s)[j]);
                self.print_char(c);
            }
            self.set_new_line_char_raw(nl);
            return;
        }
        for j in 0..self.strings.length(s) {
            let c = i32::from(self.strings.str(s)[j]);
            self.print_char(c);
        }
    }

    /// Prints a host string literal character by character. Equivalent to
    /// tex.web's `print("...")` on a pool string with number >= 256 (each
    /// character goes through `print_char`).
    pub fn print_chars(&mut self, s: &str) {
        for ch in s.chars() {
            self.print_char(ch as UnicodeChar);
        }
    }

    /// `slow_print(s)` (tex.web §60): like `print`, but each character of a
    /// multi-character string is printed via `print` so that unprintable
    /// codes come out in `^^` form.
    pub fn slow_print(&mut self, s: StrNumber) {
        if s >= self.strings.str_ptr() as i32 || s < 256 {
            self.print(s);
        } else {
            // xetex: characters above 255 are printed directly (they
            // would collide with string numbers in print()), and UTF-16
            // surrogate pairs recombine into one code point.
            let n = self.strings.length(s);
            let mut j = 0;
            while j < n {
                let u = u32::from(self.strings.str(s)[j]);
                let c = if (0xD800..0xDC00).contains(&u) && j + 1 < n {
                    let lo = u32::from(self.strings.str(s)[j + 1]);
                    if (0xDC00..0xE000).contains(&lo) {
                        j += 1;
                        0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00)
                    } else {
                        u
                    }
                } else {
                    u
                } as i32;
                if c < 256 {
                    self.print(c);
                } else {
                    self.print_char(c);
                }
                j += 1;
            }
        }
    }

    /// `print_nl(s)` (tex.web §62): print `s` at the beginning of a line.
    pub fn print_nl(&mut self, s: StrNumber) {
        if (self.prn.term_offset > 0 && self.prn.selector % 2 == 1)
            || (self.prn.file_offset > 0 && self.prn.selector >= LOG_ONLY)
        {
            self.print_ln();
        }
        self.print(s);
    }

    /// `print_nl` for host string literals.
    pub fn print_nl_chars(&mut self, s: &str) {
        if (self.prn.term_offset > 0 && self.prn.selector % 2 == 1)
            || (self.prn.file_offset > 0 && self.prn.selector >= LOG_ONLY)
        {
            self.print_ln();
        }
        self.print_chars(s);
    }

    /// `print_esc(s)` (tex.web §63): prints the escape character, then `s`.
    /// (The character range is Unicode-wide, following XeTeX.)
    pub fn print_esc(&mut self, s: StrNumber) {
        let c = self.escape_char(); // the current escape character
        if (0..=0x10FFFF).contains(&c) {
            self.print_char_code(c);
        }
        self.slow_print(s);
    }

    /// `print_esc` for host string literals.
    pub fn print_esc_str(&mut self, s: &str) {
        let c = self.escape_char();
        if (0..=0x10FFFF).contains(&c) {
            self.print_char_code(c);
        }
        self.print_chars(s);
    }

    /// `print_esc` for a single character code (e.g. one-character control
    /// sequence names).
    pub fn print_esc_char(&mut self, c: UnicodeChar) {
        let e = self.escape_char();
        if (0..=0x10FFFF).contains(&e) {
            self.print_char_code(e);
        }
        self.print_char_code(c);
    }

    /// Prints a *character code*: codes below 256 go through the
    /// single-character pool strings (so unprintables appear in `^^` form,
    /// §48), larger Unicode scalars print directly (XeTeX behavior).
    /// In tex.web `print(c)` serves this purpose because characters and
    /// single-character strings coincide there.
    pub fn print_char_code(&mut self, c: UnicodeChar) {
        if (0..256).contains(&c) {
            self.print(c);
        } else {
            self.print_char(c);
        }
    }

    /// `print_the_digs(k)` (tex.web §64): prints `dig[k-1] ... dig[0]`.
    pub fn print_the_digs(&mut self, k: usize) {
        let mut k = k;
        while k > 0 {
            k -= 1;
            let d = self.prn.dig[k];
            if d < 10 {
                self.print_char(i32::from(b'0' + d));
            } else {
                self.print_char(i32::from(b'A' - 10 + d));
            }
        }
    }

    /// `print_int(n)` (tex.web §65): decimal representation, careful with
    /// `n = -2^31`.
    pub fn print_int(&mut self, n: i32) {
        let mut n = n;
        let mut k: usize = 0;
        if n < 0 {
            self.print_char(i32::from(b'-'));
            if n > -100_000_000 {
                n = -n;
            } else {
                let mut m = -1 - n;
                n = m / 10;
                m = (m % 10) + 1;
                k = 1;
                if m < 10 {
                    self.prn.dig[0] = m as u8;
                } else {
                    self.prn.dig[0] = 0;
                    n += 1;
                }
            }
        }
        loop {
            self.prn.dig[k] = (n % 10) as u8;
            n /= 10;
            k += 1;
            if n == 0 {
                break;
            }
        }
        self.print_the_digs(k);
    }

    /// `print_two(n)` (tex.web §66): two least significant digits.
    pub fn print_two(&mut self, n: i32) {
        let n = n.abs() % 100;
        self.print_char(i32::from(b'0') + n / 10);
        self.print_char(i32::from(b'0') + n % 10);
    }

    /// `print_hex(n)` (tex.web §67): prints a nonnegative integer in
    /// hexadecimal, preceded by `"`.
    pub fn print_hex(&mut self, n: i32) {
        let mut n = n;
        let mut k: usize = 0;
        self.print_char(i32::from(b'"'));
        loop {
            self.prn.dig[k] = (n % 16) as u8;
            n /= 16;
            k += 1;
            if n == 0 {
                break;
            }
        }
        self.print_the_digs(k);
    }

    /// `print_roman_int(n)` (tex.web §69). The mysterious string
    /// `"m2d5c2l5x2v5i"` is used verbatim from tex.web.
    pub fn print_roman_int(&mut self, n: i32) {
        const S: &[u8] = b"m2d5c2l5x2v5i";
        let mut n = n;
        let mut j: usize = 0;
        let mut v: i32 = 1000;
        loop {
            while n >= v {
                self.print_char(i32::from(S[j]));
                n -= v;
            }
            if n <= 0 {
                return; // nonpositive input produces no output
            }
            let mut k = j + 2;
            let mut u = v / i32::from(S[k - 1] - b'0');
            if S[k - 1] == b'2' {
                k += 2;
                u /= i32::from(S[k - 1] - b'0');
            }
            if n + u >= v {
                self.print_char(i32::from(S[k]));
                n += u;
            } else {
                j += 2;
                v /= i32::from(S[j - 1] - b'0');
            }
        }
    }

    /// `print_ASCII(s)` (tex.web §68): prints a character as selected by
    /// `s`, with unprintables in `^^` form (Unicode scalars print direct).
    pub fn print_ascii(&mut self, s: i32) {
        if !(0..=0x10FFFF).contains(&s) {
            self.print_esc_str("???");
        } else if s < 256 {
            self.print(s);
        } else {
            self.print_char(s);
        }
    }

    /// `print_current_string` (tex.web §70): prints a yet-unmade string.
    pub fn print_current_string(&mut self) {
        for i in 0..self.strings.cur_length() {
            let c = i32::from(self.strings.cur_str()[i]);
            self.print_char(c);
        }
    }

    /// `print_scaled(s)` (tex.web §103): prints a scaled real, rounded to
    /// five digits, such that `round_decimals` reproduces it exactly.
    pub fn print_scaled(&mut self, s: Scaled) {
        let mut s = s;
        if s < 0 {
            self.print_char(i32::from(b'-'));
            s = -s; // print the sign, if negative
        }
        self.print_int(s / UNITY); // print the integer part
        self.print_char(i32::from(b'.'));
        s = 10 * (s % UNITY) + 5;
        let mut delta: Scaled = 10;
        loop {
            if delta > UNITY {
                s += 0o100000 - 50000; // round the last digit
            }
            self.print_char(i32::from(b'0') + s / UNITY);
            s = 10 * (s % UNITY);
            delta *= 10;
            if s <= delta {
                break;
            }
        }
    }

    /// `print_glue(d, order, s)` (§177): prints a glue component, possibly
    /// followed by an order of infinity or the given unit name.
    pub fn print_glue(&mut self, d: Scaled, order: u16, s: &str) {
        self.print_scaled(d);
        if order > crate::mem::FILLL {
            self.print_chars("foul");
        } else if order > crate::mem::NORMAL {
            self.print_chars("fil");
            let mut o = order;
            while o > crate::mem::FIL {
                self.print_char('l' as i32);
                o -= 1;
            }
        } else if !s.is_empty() {
            self.print_chars(s);
        }
    }

    /// `print_spec(p, s)` (§178): prints a glue specification.
    pub fn print_spec(&mut self, p: crate::types::Pointer, s: &str) {
        if p < 0 || p >= self.mem.lo_mem_max {
            self.print_char('*' as i32);
        } else {
            let w = self.mem.width(p);
            self.print_scaled(w);
            if !s.is_empty() {
                self.print_chars(s);
            }
            if self.mem.stretch(p) != 0 {
                self.print_chars(" plus ");
                let (st, so) = (self.mem.stretch(p), self.mem.stretch_order(p));
                self.print_glue(st, so, s);
            }
            if self.mem.shrink(p) != 0 {
                self.print_chars(" minus ");
                let (sh, so) = (self.mem.shrink(p), self.mem.shrink_order(p));
                self.print_glue(sh, so, s);
            }
        }
    }

    /// Finishes the current string built under `selector = new_string` and
    /// returns its number (a recurring tex.web idiom).
    pub fn make_string(&mut self) -> TexResult<StrNumber> {
        self.strings.make_string()
    }
}
