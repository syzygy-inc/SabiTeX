//! Building token lists.
//!
//! Ports tex.web Part 27 (§463-§486): `str_toks`, `the_toks`,
//! `ins_the_toks`, `conv_toks`, `scan_toks` and `read_toks`.

use crate::cmds::*;
use crate::engine::Engine;
use crate::error::TexResult;
use crate::input::{ABSORBING, DEFINING, NORMAL_STATUS};
use crate::print::NEW_STRING;
use crate::scan::{DIMEN_VAL, GLUE_VAL, IDENT_VAL, INT_VAL, TOK_VAL};
use crate::tokens::*;
use crate::types::{Pointer, NULL};

// §468: convert codes.
pub const NUMBER_CODE: i32 = 0;
pub const ROMAN_NUMERAL_CODE: i32 = 1;
pub const STRING_CODE: i32 = 2;
pub const MEANING_CODE: i32 = 3;
pub const FONT_NAME_CODE: i32 = 4;
pub const JOB_NAME_CODE: i32 = 5;
// etex.ch: `etex_convert_base` — \eTeXrevision.
pub const ETEX_REVISION_CODE: i32 = JOB_NAME_CODE + 1;
/// pdfTeX `\expanded` (required by expl3/LaTeX).
pub const EXPANDED_CODE: i32 = ETEX_REVISION_CODE + 1;
/// pdfTeX `\pdfstrcmp` / XeTeX `\strcmp`.
pub const PDF_STRCMP_CODE: i32 = EXPANDED_CODE + 1;
/// XeTeX `\XeTeXrevision`.
pub const XETEX_REVISION_CODE: i32 = PDF_STRCMP_CODE + 10;
// xetex.web: \Uchar / \Ucharcat.
pub const XETEX_UCHAR_CODE: i32 = PDF_STRCMP_CODE + 11;
pub const XETEX_UCHARCAT_CODE: i32 = PDF_STRCMP_CODE + 12;
/// pTeX `\kansuji`.
pub const KANSUJI_CODE: i32 = PDF_STRCMP_CODE + 20;
/// pdfTeX `\pdfcreationdate` ("D:YYYYMMDDhhmmss" from \year etc.).
pub const PDF_CREATION_DATE_CODE: i32 = PDF_STRCMP_CODE + 2;
/// pdfTeX `\pdffilesize` (empty when the file does not exist).
pub const PDF_FILE_SIZE_CODE: i32 = PDF_STRCMP_CODE + 1;
/// `show_tokens` (etex.ch): chr for `\showtokens` / `\detokenize` (odd!).
pub const SHOW_TOKENS: i32 = 5;

impl Engine {
    /// `str_toks(b)` (§464): converts `str_pool[b..pool_ptr]` to a token
    /// list beginning at `link(temp_head)`; returns the tail.
    pub fn str_toks(&mut self, b: usize) -> TexResult<Pointer> {
        self.strings.str_room(1)?;
        let mut p = self.mem.temp_head();
        self.mem.set_link(p, NULL);
        // Decode the UTF-16 units back to scalar values for tokens.
        let units: Vec<u16> = self.strings.pool_suffix(b).to_vec();
        let scalars: Vec<i32> = char::decode_utf16(units.iter().copied())
            .map(|r| r.map(|c| c as i32).unwrap_or(0xFFFD))
            .collect();
        for t in scalars {
            let tok = if t == ' ' as i32 {
                SPACE_TOKEN
            } else {
                OTHER_TOKEN + t
            };
            let q = self.mem.get_avail()?;
            self.mem.set_link(p, q);
            self.mem.set_info(q, tok);
            p = q;
        }
        self.strings.pool_truncate(b);
        Ok(p)
    }

    /// `the_toks` (§465-§466): scans what can follow `\the` and constructs
    /// the corresponding token list; returns the tail.
    pub fn the_toks(&mut self) -> TexResult<Pointer> {
        // etex.ch: handle \unexpanded (chr 1) or \detokenize (chr 5); the
        // command modifiers are odd whereas \the and \showthe are even.
        if self.cur_chr % 2 == 1 {
            let c = self.cur_chr;
            self.scan_general_text()?;
            return if c == 1 {
                Ok(self.cur_val)
            } else {
                let old_setting = self.prn.selector;
                self.prn.selector = NEW_STRING;
                let b = self.strings.pool_ptr();
                let p = self.mem.get_avail()?;
                let th = self.mem.temp_head();
                let l = self.mem.link(th);
                self.mem.set_link(p, l);
                self.token_show(p);
                self.mem.flush_list(p);
                self.prn.selector = old_setting;
                self.str_toks(b)
            };
        }
        self.get_x_token()?;
        self.scan_something_internal(TOK_VAL, false)?;
        if self.cur_val_level >= IDENT_VAL {
            // §466: copy the token list.
            let mut p = self.mem.temp_head();
            self.mem.set_link(p, NULL);
            if self.cur_val_level == IDENT_VAL {
                let q = self.mem.get_avail()?;
                self.mem.set_link(p, q);
                let t = CS_TOKEN_FLAG + self.cur_val;
                self.mem.set_info(q, t);
                p = q;
            } else if self.cur_val != NULL {
                let mut r = self.mem.link(self.cur_val); // skip the ref count
                while r != NULL {
                    let q = self.mem.get_avail()?;
                    self.mem.set_link(p, q);
                    let t = self.mem.info(r);
                    self.mem.set_info(q, t);
                    p = q;
                    r = self.mem.link(r);
                }
            }
            Ok(p)
        } else {
            let old_setting = self.prn.selector;
            self.prn.selector = NEW_STRING;
            let b = self.strings.pool_ptr();
            match self.cur_val_level {
                INT_VAL => {
                    let v = self.cur_val;
                    self.print_int(v);
                }
                DIMEN_VAL => {
                    let v = self.cur_val;
                    self.print_scaled(v);
                    self.print_chars("pt");
                }
                GLUE_VAL => {
                    let v = self.cur_val;
                    self.print_spec(v, "pt");
                    self.mem.delete_glue_ref(v);
                }
                _ => {
                    // mu_val
                    let v = self.cur_val;
                    self.print_spec(v, "mu");
                    self.mem.delete_glue_ref(v);
                }
            }
            self.prn.selector = old_setting;
            self.str_toks(b)
        }
    }

    /// `scan_general_text` (etex.ch): scans a `{...}` balanced text and
    /// hangs it from `temp_head`; `cur_val` gets the tail (or `temp_head`
    /// itself when the text is empty).
    pub fn scan_general_text(&mut self) -> TexResult<()> {
        let s = self.inp.scanner_status;
        let w = self.inp.warning_index;
        let d = self.inp.def_ref;
        self.inp.scanner_status = ABSORBING;
        self.inp.warning_index = self.cur_cs;
        self.inp.def_ref = self.mem.get_avail()?;
        let def_ref = self.inp.def_ref;
        self.mem.set_info(def_ref, NULL); // token_ref_count
        let mut p = def_ref;
        self.scan_left_brace()?; // remove the compulsory left brace
        let mut unbalance = 1;
        loop {
            self.get_token()?;
            if self.cur_tok < RIGHT_BRACE_LIMIT {
                if self.cur_cmd < RIGHT_BRACE {
                    unbalance += 1;
                } else {
                    unbalance -= 1;
                    if unbalance == 0 {
                        break;
                    }
                }
            }
            let q = self.mem.get_avail()?;
            self.mem.set_link(p, q);
            let t = self.cur_tok;
            self.mem.set_info(q, t);
            p = q;
        }
        let q = self.mem.link(def_ref);
        self.mem.free_avail(def_ref); // discard reference count
        self.cur_val = if q == NULL { self.mem.temp_head() } else { p };
        let th = self.mem.temp_head();
        self.mem.set_link(th, q);
        self.inp.scanner_status = s;
        self.inp.warning_index = w;
        self.inp.def_ref = d;
        Ok(())
    }

    /// `ins_the_toks` (§467).
    pub fn ins_the_toks(&mut self) -> TexResult<()> {
        let _tail = self.the_toks()?;
        let th = self.mem.temp_head();
        let l = self.mem.link(th);
        self.ins_list(l)
    }

    /// `conv_toks` (§470-§472): expands `\number`, `\romannumeral`,
    /// `\string`, `\meaning`, `\fontname`, `\jobname`.
    pub fn conv_toks(&mut self) -> TexResult<()> {
        let c = self.cur_chr;
        // pdftex.web: \expanded fully expands a general text (like
        // \edef) and inserts the result directly — no string step.
        if c == EXPANDED_CODE {
            // pdftex.web: save def_ref (an outer scan_toks may be in
            // progress) and the scanner status around the inner scan.
            let save_scanner_status = self.inp.scanner_status;
            let save_def_ref = self.inp.def_ref;
            let save_warning_index = self.inp.warning_index;
            self.inp.scanner_status = NORMAL_STATUS;
            self.scan_toks(false, true)?; // returns the TAIL; head is inp.def_ref
            let def_ref = self.inp.def_ref;
            self.inp.scanner_status = save_scanner_status;
            self.inp.warning_index = save_warning_index;
            self.inp.def_ref = save_def_ref;
            let list = self.mem.link(def_ref);
            if list == NULL {
                self.mem.free_avail(def_ref);
            } else {
                self.ins_list(list)?;
                self.mem.free_avail(def_ref);
            }
            return Ok(());
        }
        if c == XETEX_UCHAR_CODE || c == XETEX_UCHARCAT_CODE {
            // xetex.web: expands to ONE character token; \Ucharcat
            // takes the category explicitly, \Uchar uses 12 (10 for
            // a space), like \string.
            self.scan_char_num()?;
            let ch = self.cur_val;
            let cat: i32 = if c == XETEX_UCHARCAT_CODE {
                self.scan_int()?;
                let v = self.cur_val;
                if matches!(v, 1..=4 | 6..=8 | 10..=13) {
                    v
                } else {
                    self.print_err("Invalid code (");
                    self.print_int(v);
                    self.print_chars("), should be in the ranges 1..4, 6..8, 10..13");
                    self.help(&["I'm going to use 12 instead of that illegal code value."]);
                    self.error()?;
                    12
                }
            } else if ch == ' ' as i32 {
                i32::from(crate::cmds::SPACER)
            } else {
                i32::from(crate::cmds::OTHER_CHAR)
            };
            let q = self.mem.get_avail()?;
            self.mem.set_info(q, cat * crate::tokens::MAX_CHAR_VAL + ch);
            self.mem.set_link(q, crate::types::NULL);
            self.ins_list(q)?;
            return Ok(());
        }
        if c == PDF_STRCMP_CODE {
            // pdftex.web compare_strings: fully expand two general
            // texts, stringify them (show_token_list rules) and compare.
            let s1 = self.scan_toks_to_chars()?;
            let s2 = self.scan_toks_to_chars()?;
            let r = match s1.cmp(&s2) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            let old_setting = self.prn.selector;
            self.prn.selector = NEW_STRING;
            let b = self.strings.pool_ptr();
            self.print_int(r);
            self.prn.selector = old_setting;
            let tail = self.str_toks(b)?;
            let th = self.mem.temp_head();
            let _ = tail;
            let l = self.mem.link(th);
            self.ins_list(l)?;
            return Ok(());
        }
        if c == PDF_CREATION_DATE_CODE {
            let old_setting = self.prn.selector;
            self.prn.selector = NEW_STRING;
            let b = self.strings.pool_ptr();
            let lay = self.eqtb.lay.clone();
            let year = self.eqtb.int_par(crate::eqtb::YEAR_CODE);
            let month = self.eqtb.int_par(crate::eqtb::MONTH_CODE);
            let day = self.eqtb.int_par(crate::eqtb::DAY_CODE);
            let time = self.eqtb.int_par(crate::eqtb::TIME_CODE);
            let _ = lay;
            self.print_chars(&format!(
                "D:{year:04}{month:02}{day:02}{:02}{:02}00",
                time / 60,
                time % 60
            ));
            self.prn.selector = old_setting;
            self.str_toks(b)?;
            let th = self.mem.temp_head();
            let l = self.mem.link(th);
            self.ins_list(l)?;
            return Ok(());
        }
        if c == PDF_FILE_SIZE_CODE {
            let name_chars = self.scan_toks_to_chars()?;
            let name: String = name_chars
                .iter()
                .map(|&c| char::from_u32(u32::from(c)).unwrap_or('?'))
                .collect();
            let size = self
                .fs
                .read_file(&name, crate::io::FileKind::OpenIn)
                .map(|d| d.len());
            let old_setting = self.prn.selector;
            self.prn.selector = NEW_STRING;
            let b = self.strings.pool_ptr();
            if let Some(n) = size {
                self.print_int(n as i32);
            }
            self.prn.selector = old_setting;
            self.str_toks(b)?;
            let th = self.mem.temp_head();
            let l = self.mem.link(th);
            self.ins_list(l)?;
            return Ok(());
        }
        // §471: scan the argument for command c.
        match c {
            NUMBER_CODE | ROMAN_NUMERAL_CODE | KANSUJI_CODE => self.scan_int()?,
            STRING_CODE | MEANING_CODE => {
                let save_scanner_status = self.inp.scanner_status;
                self.inp.scanner_status = NORMAL_STATUS;
                self.get_token()?;
                self.inp.scanner_status = save_scanner_status;
            }
            FONT_NAME_CODE => self.scan_font_ident()?,
            ETEX_REVISION_CODE | XETEX_REVISION_CODE => {} // do_nothing
            _ => {
                // job_name_code
                if self.job_name.is_none() {
                    self.open_log_file()?;
                }
            }
        }
        let old_setting = self.prn.selector;
        self.prn.selector = NEW_STRING;
        let b = self.strings.pool_ptr();
        // §472: print the result of command c.
        match c {
            NUMBER_CODE => {
                let v = self.cur_val;
                self.print_int(v);
            }
            KANSUJI_CODE => {
                let v = self.cur_val;
                self.print_kansuji(v);
            }
            ROMAN_NUMERAL_CODE => {
                let v = self.cur_val;
                self.print_roman_int(v);
            }
            STRING_CODE => {
                if self.cur_cs != 0 {
                    let cs = self.cur_cs;
                    self.sprint_cs(cs);
                } else {
                    let ch = self.cur_chr;
                    self.print_char_code(ch);
                }
            }
            MEANING_CODE => self.print_meaning(),
            ETEX_REVISION_CODE => self.print_chars(crate::engine::ETEX_REVISION),
            XETEX_REVISION_CODE => self.print_chars(".999994"),
            FONT_NAME_CODE => {
                // §472: the font name, with the at size if it differs.
                let f = self.cur_val as usize;
                let name = self.fonts.name[f].clone();
                self.print_chars(&name);
                if self.fonts.size[f] != self.fonts.dsize[f] {
                    self.print_chars(" at ");
                    let s = self.fonts.size[f];
                    self.print_scaled(s);
                    self.print_chars("pt");
                }
            }
            _ => {
                let j = self.job_name.clone().unwrap_or_else(|| "texput".into());
                self.print_chars(&j);
            }
        }
        self.prn.selector = old_setting;
        let _tail = self.str_toks(b)?;
        let th = self.mem.temp_head();
        let l = self.mem.link(th);
        self.ins_list(l)
    }

    /// `scan_toks(macro_def, xpand)` (§473-§482): builds the token list for
    /// a macro definition or a balanced text; makes `def_ref` point to the
    /// reference count and returns the tail.
    pub fn scan_toks(&mut self, macro_def: bool, xpand: bool) -> TexResult<Pointer> {
        self.inp.scanner_status = if macro_def { DEFINING } else { ABSORBING };
        self.inp.warning_index = self.cur_cs;
        self.inp.def_ref = self.mem.get_avail()?;
        let def_ref = self.inp.def_ref;
        self.mem.set_info(def_ref, NULL); // token_ref_count
        let mut p = def_ref;
        let mut hash_brace = 0;
        let mut t = ZERO_TOKEN;
        macro_rules! store {
            ($tok:expr) => {{
                let q = self.mem.get_avail()?;
                self.mem.set_link(p, q);
                let tk = $tok;
                self.mem.set_info(q, tk);
                p = q;
            }};
        }
        'found: {
            if macro_def {
                // §474: scan and build the parameter part.
                'done: {
                    loop {
                        // continue:
                        self.get_token()?;
                        if self.cur_tok < RIGHT_BRACE_LIMIT {
                            break; // done1
                        }
                        if self.cur_cmd == MAC_PARAM {
                            // §476: parameter number or #{.
                            let s = MATCH_TOKEN + self.cur_chr;
                            self.get_token()?;
                            if self.cur_tok < LEFT_BRACE_LIMIT {
                                hash_brace = self.cur_tok;
                                store!(self.cur_tok);
                                store!(END_MATCH_TOKEN);
                                break 'done;
                            }
                            if t == ZERO_TOKEN + 9 {
                                self.print_err("You already have nine parameters");
                                self.help(&[
                                    "I'm going to ignore the # sign you just used,",
                                    "as well as the token that followed it.",
                                ]);
                                self.error()?;
                                continue;
                            } else {
                                t += 1;
                                if self.cur_tok != t {
                                    self.print_err("Parameters must be numbered consecutively");
                                    self.help(&[
                                        "I've inserted the digit you should have used after the #.",
                                        "Type `1' to delete what you did use.",
                                    ]);
                                    self.back_error()?;
                                }
                                self.cur_tok = s;
                            }
                        }
                        store!(self.cur_tok);
                    }
                    // done1:
                    store!(END_MATCH_TOKEN);
                    if self.cur_cmd == RIGHT_BRACE {
                        // §475: express shock at the missing left brace.
                        self.print_err("Missing { inserted");
                        self.help(&[
                            "Where was the left brace? You said something like `\\def\\a}',",
                            "which I'm going to interpret as `\\def\\a{}'.",
                        ]);
                        self.inp.align_state += 1;
                        self.error()?;
                        break 'found;
                    }
                }
            } else {
                self.scan_left_brace()?; // remove the compulsory left brace
            }
            // §477: scan and build the body.
            let mut unbalance = 1;
            loop {
                if xpand {
                    // §478: expand the next part of the input.
                    loop {
                        self.get_next()?;
                        // etex.ch §478: \protected macros are not expanded
                        // here — they behave like \relax for this scan.
                        if self.cur_cmd >= CALL
                            && self.mem.info(self.mem.link(self.cur_chr))
                                == crate::tokens::PROTECTED_TOKEN
                        {
                            self.cur_cmd = RELAX;
                            self.cur_chr = crate::getnext::NO_EXPAND_FLAG;
                        }
                        if self.cur_cmd <= MAX_COMMAND {
                            break;
                        }
                        if self.cur_cmd != THE {
                            self.expand()?;
                        } else {
                            let q = self.the_toks()?;
                            let th = self.mem.temp_head();
                            if self.mem.link(th) != NULL {
                                let l = self.mem.link(th);
                                self.mem.set_link(p, l);
                                p = q;
                            }
                        }
                    }
                    self.x_token()?;
                } else {
                    self.get_token()?;
                }
                if self.cur_tok < RIGHT_BRACE_LIMIT {
                    if self.cur_cmd < RIGHT_BRACE {
                        unbalance += 1;
                    } else {
                        unbalance -= 1;
                        if unbalance == 0 {
                            break 'found;
                        }
                    }
                } else if self.cur_cmd == MAC_PARAM && macro_def {
                    // §479: look for parameter number or ##.
                    let s = self.cur_tok;
                    if xpand {
                        self.get_x_token()?;
                    } else {
                        self.get_token()?;
                    }
                    if self.cur_cmd != MAC_PARAM {
                        if self.cur_tok <= ZERO_TOKEN || self.cur_tok > t {
                            self.print_err("Illegal parameter number in definition of ");
                            let w = self.inp.warning_index;
                            self.sprint_cs(w);
                            self.help(&[
                                "You meant to type ## instead of #, right?",
                                "Or maybe a } was forgotten somewhere earlier, and things",
                                "are all screwed up? I'm going to assume that you meant ##.",
                            ]);
                            self.back_error()?;
                            self.cur_tok = s;
                        } else {
                            self.cur_tok = OUT_PARAM_TOKEN - '0' as i32 + self.cur_chr;
                        }
                    }
                }
                store!(self.cur_tok);
            }
        }
        // found:
        self.inp.scanner_status = NORMAL_STATUS;
        if hash_brace != 0 {
            store!(hash_brace);
        }
        Ok(p)
    }

    /// Scans a fully-expanded `{...}` text and returns its stringified
    /// character codes (pdfTeX's tokens_to_string, for \pdfstrcmp).
    fn scan_toks_to_chars(&mut self) -> TexResult<Vec<u16>> {
        let save_scanner_status = self.inp.scanner_status;
        let save_def_ref = self.inp.def_ref;
        let save_warning_index = self.inp.warning_index;
        self.inp.scanner_status = NORMAL_STATUS;
        self.scan_toks(false, true)?;
        let def_ref = self.inp.def_ref;
        self.inp.scanner_status = save_scanner_status;
        self.inp.warning_index = save_warning_index;
        self.inp.def_ref = save_def_ref;
        let old_setting = self.prn.selector;
        self.prn.selector = NEW_STRING;
        let b = self.strings.pool_ptr();
        let list = self.mem.link(def_ref);
        self.show_token_list(list, NULL, i32::MAX / 2);
        self.prn.selector = old_setting;
        self.mem.flush_list(def_ref);
        let cur = self.strings.cur_str();
        let start_of_cur = self.strings.pool_ptr() - cur.len();
        let chars = cur[b - start_of_cur..].to_vec();
        // roll the pool back to b
        while self.strings.pool_ptr() > b {
            self.strings.flush_char();
        }
        Ok(chars)
    }

    /// `read_toks(n, r)` (§482-§486): reads a line (or balanced lines) from
    /// stream `n` into a token list; `cur_val` receives the list.
    pub fn read_toks(&mut self, n: i32, r: crate::types::Pointer, j: i32) -> TexResult<()> {
        use crate::cond::{CLOSED, JUST_OPEN};
        self.inp.scanner_status = DEFINING;
        self.inp.warning_index = r;
        self.inp.def_ref = self.mem.get_avail()?;
        let def_ref = self.inp.def_ref;
        self.mem.set_info(def_ref, NULL); // token_ref_count
        let mut p = def_ref;
        macro_rules! store {
            ($tok:expr) => {{
                let q = self.mem.get_avail()?;
                self.mem.set_link(p, q);
                self.mem.set_info(q, $tok);
                p = q;
            }};
        }
        store!(END_MATCH_TOKEN);
        let m = if !(0..=15).contains(&n) {
            16
        } else {
            n as usize
        };
        let s = self.inp.align_state;
        self.inp.align_state = 1_000_000; // disable tab marks, etc.
        let mut n = n;
        loop {
            // §484: input and store tokens from the next line of the file.
            self.begin_file_reading()?;
            self.inp.cur.name = m as i32 + 1;
            if m == 16 || self.read_open[m] == CLOSED {
                // §485: \read from the terminal.
                if self.interaction > crate::input::NONSTOP_MODE {
                    if n < 0 {
                        self.prompt_input("")?;
                    } else {
                        self.print_ln();
                        self.sprint_cs(r);
                        self.prompt_input("=")?;
                        n = -1;
                    }
                } else {
                    return Err(
                        self.fatal_error("*** (cannot \\read from terminal in nonstop modes)")
                    );
                }
            } else if self.read_open[m] == JUST_OPEN {
                // §486: the first line of read_file[m].
                if self.input_ln_read(m)? {
                    self.read_open[m] = crate::input::NORMAL_STATUS;
                } else {
                    self.inp.read_file[m] = None;
                    self.read_open[m] = CLOSED;
                }
            } else {
                // §487: the next line of read_file[m].
                if !self.input_ln_read(m)? {
                    self.inp.read_file[m] = None;
                    self.read_open[m] = CLOSED;
                    if self.inp.align_state != 1_000_000 {
                        self.runaway();
                        self.print_err("File ended within ");
                        self.print_esc_str("read");
                        self.help(&["This \\read has unbalanced braces."]);
                        self.inp.align_state = 1_000_000;
                        self.inp.cur.limit = 0;
                        self.error()?;
                    }
                }
            }
            self.inp.cur.limit = self.inp.last;
            if self.end_line_char_inactive() {
                self.inp.cur.limit -= 1;
            } else {
                let e = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
                self.inp.buffer[self.inp.cur.limit as usize] = e;
            }
            self.inp.first = self.inp.cur.limit + 1;
            self.inp.cur.loc = self.inp.cur.start;
            self.inp.cur.state = crate::input::NEW_LINE;
            'done: {
                // etex.ch: \readline turns the raw characters into space
                // and "other" tokens, ignoring category codes.
                if j == 1 {
                    while self.inp.cur.loc <= self.inp.cur.limit {
                        let c = self.inp.buffer[self.inp.cur.loc as usize];
                        self.inp.cur.loc += 1;
                        let tok = if c == ' ' as i32 {
                            SPACE_TOKEN
                        } else {
                            OTHER_TOKEN + c
                        };
                        store!(tok);
                    }
                    break 'done;
                }
                loop {
                    self.get_token()?;
                    if self.cur_tok == 0 {
                        break 'done; // cur_cmd=cur_chr=0 at the end of the line
                    }
                    if self.inp.align_state < 1_000_000 {
                        // unmatched `}` aborts the line
                        loop {
                            self.get_token()?;
                            if self.cur_tok == 0 {
                                break;
                            }
                        }
                        self.inp.align_state = 1_000_000;
                        break 'done;
                    }
                    store!(self.cur_tok);
                }
            }
            self.end_file_reading();
            if self.inp.align_state == 1_000_000 {
                break;
            }
        }
        self.cur_val = def_ref;
        self.inp.scanner_status = NORMAL_STATUS;
        self.inp.align_state = s;
        Ok(())
    }

    /// `input_ln` for `read_file[m]` (§486-§487): an empty line is
    /// appended at the end of a read file.
    fn input_ln_read(&mut self, m: usize) -> crate::error::TexResult<bool> {
        self.inp.last = self.inp.first; // §31: cf. Matthew 19:30
        let Some(file) = self.inp.read_file[m].as_mut() else {
            return Ok(false);
        };
        if file.next >= file.lines.len() {
            return Ok(false);
        }
        let line = file.lines[file.next].clone();
        file.next += 1;
        self.copy_line_to_buffer(&line)
    }
}
