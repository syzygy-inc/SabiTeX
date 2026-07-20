//! Input stacks and states.
//!
//! Ports tex.web Part 22 (§300-§322) and Part 23 (§323-§331): the input
//! stack of `in_state_record`s, the `buffer`, token-list input levels, the
//! parameter stack, and `align_state`.
//!
//! Files are provided whole by [`crate::io::TexFs`] and split into lines at
//! load time; `input_ln` (§31) becomes "copy the next stored line into
//! `buffer`", with trailing spaces removed exactly as §31 specifies.

use crate::error::{TexInterrupt, TexResult};
use crate::tokens::{LEFT_BRACE_LIMIT, RIGHT_BRACE_LIMIT};
use crate::types::{Pointer, UnicodeChar, NULL};

// §73: interaction modes.
pub const BATCH_MODE: u8 = 0;
pub const NONSTOP_MODE: u8 = 1;
pub const SCROLL_MODE: u8 = 2;
pub const ERROR_STOP_MODE: u8 = 3;

// §303: scanner states.
pub const MID_LINE: u16 = 1;
pub const SKIP_BLANKS: u16 = 2 + crate::cmds::MAX_CHAR_CODE;
pub const NEW_LINE: u16 = 3 + 2 * crate::cmds::MAX_CHAR_CODE;

// §305: scanner_status values.
pub const NORMAL_STATUS: u8 = 0;
pub const SKIPPING: u8 = 1;
pub const DEFINING: u8 = 2;
pub const MATCHING: u8 = 3;
pub const ALIGNING: u8 = 4;
pub const ABSORBING: u8 = 5;

// §307: token list types.
pub const TOKEN_LIST: u16 = 0; // state code when scanning a token list
pub const PARAMETER: u16 = 0;
pub const U_TEMPLATE: u16 = 1;
pub const V_TEMPLATE: u16 = 2;
pub const BACKED_UP: u16 = 3;
pub const INSERTED: u16 = 4;
pub const MACRO: u16 = 5;
pub const OUTPUT_TEXT: u16 = 6;
pub const EVERY_PAR_TEXT: u16 = 7;
pub const MARK_TEXT: u16 = 14;
/// etex.ch §307: `\everyeof` precedes the `\toks` registers, so its token
/// type slots in before `write_text`.
pub const EVERY_EOF_TEXT: u16 = 15;
pub const WRITE_TEXT: u16 = 16;

/// `in_state_record` (§300).
#[derive(Copy, Clone, Default, Debug)]
pub struct InStateRecord {
    /// `state_field`: scanner state, or `TOKEN_LIST`.
    pub state: u16,
    /// `index_field`: buffer reference / `token_type`.
    pub index: u16,
    /// `start_field`: start of line in `buffer` / first node of token list.
    pub start: Pointer,
    /// `loc_field`: next character / next token node.
    pub loc: Pointer,
    /// `limit_field`: end of line / `param_start` for macros.
    pub limit: Pointer,
    /// `name_field`: 0 = terminal, 1..17 = `\read` stream, else a string
    /// number (file name) or eqtb address (macro).
    pub name: Pointer,
}

/// A fully loaded input file, replacing `alpha_file` + `input_ln`.
pub struct FileSource {
    /// Lines, already trimmed of trailing spaces (§31).
    pub lines: Vec<Vec<UnicodeChar>>,
    /// Next line to deliver.
    pub next: usize,
}

/// Decodes file bytes into trimmed lines (§31 semantics; input is UTF-8,
/// with XeTeX-style BOM sniffing for UTF-8 and UTF-16).
/// A final newline terminates the last line — it does not start an empty
/// extra line (a file ending `"x\n"` has exactly one line).
pub fn decode_lines(bytes: &[u8]) -> Vec<Vec<UnicodeChar>> {
    // BOM sniffing (XeTeX defaults to UTF-8; a UTF-16 BOM is honored).
    let text: std::borrow::Cow<str> = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..])
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units).into()
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units).into()
    } else {
        String::from_utf8_lossy(bytes)
    };
    let text: &str = &text;
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.split('\n')
        .map(|l| {
            // Trailing blanks survive here; `copy_line_to_buffer` strips
            // them per §31 (`last_nonblank`), so `max_buf_stack` sees the
            // raw line length as in tex.web.
            let l = l.strip_suffix('\r').unwrap_or(l);
            l.chars().map(|c| c as UnicodeChar).collect()
        })
        .collect()
}

/// All input-side state (Parts 22-23).
pub struct Input {
    /// `input_stack` / `input_ptr` / `max_in_stack` (§301).
    pub stack: Vec<InStateRecord>,
    pub input_ptr: usize,
    pub max_in_stack: usize,
    /// `cur_input`: the "top" input state (§301).
    pub cur: InStateRecord,
    /// `in_open`, `open_parens`, `input_file`, `line`, `line_stack` (§304).
    pub in_open: usize,
    pub open_parens: i32,
    pub input_file: Vec<Option<FileSource>>, // index 1..=max_in_open
    pub line: i32,
    pub line_stack: Vec<i32>,
    /// `scanner_status`, `warning_index`, `def_ref` (§305).
    pub scanner_status: u8,
    pub warning_index: Pointer,
    pub def_ref: Pointer,
    /// `param_stack` / `param_ptr` / `max_param_stack` (§308).
    pub param_stack: Vec<Pointer>,
    pub param_ptr: usize,
    pub max_param_stack: usize,
    /// `align_state` (§309).
    pub align_state: i32,
    /// `buffer`, `first`, `last`, `max_buf_stack` (§30).
    pub buffer: Vec<UnicodeChar>,
    pub first: i32,
    pub last: i32,
    pub max_buf_stack: i32,
    /// `force_eof` (§361).
    pub force_eof: bool,
    /// `read_file[0..15]` (§480): line sources for `\read`.
    pub read_file: Vec<Option<FileSource>>,
    /// Sizes.
    pub stack_size: usize,
    pub max_in_open: usize,
    pub param_size: usize,
    pub buf_size: i32,
}

impl Input {
    pub fn new(stack_size: usize, max_in_open: usize, param_size: usize, buf_size: i32) -> Input {
        // §331: initialize the input routines (minus init_terminal, which
        // the driver replaces).
        Input {
            stack: vec![InStateRecord::default(); stack_size + 1],
            input_ptr: 0,
            max_in_stack: 0,
            cur: InStateRecord {
                state: NEW_LINE,
                index: 0,
                start: 1,
                loc: 1,
                limit: 0,
                name: 0,
            },
            in_open: 0,
            open_parens: 0,
            input_file: (0..=max_in_open).map(|_| None).collect(),
            line: 0,
            line_stack: vec![0; max_in_open + 1],
            scanner_status: NORMAL_STATUS,
            warning_index: NULL,
            def_ref: NULL,
            param_stack: vec![NULL; param_size + 1],
            param_ptr: 0,
            max_param_stack: 0,
            align_state: 1_000_000,
            buffer: vec![0; buf_size as usize + 1],
            first: 1,
            last: 0,
            max_buf_stack: 0,
            force_eof: false,
            read_file: (0..16).map(|_| None).collect(),
            stack_size,
            max_in_open,
            param_size,
            buf_size,
        }
    }

    /// `terminal_input` (§304).
    pub fn terminal_input(&self) -> bool {
        self.cur.name == 0
    }
}

impl crate::engine::Engine {
    /// `push_input` (§321).
    pub fn push_input(&mut self) -> TexResult<()> {
        if self.inp.input_ptr > self.inp.max_in_stack {
            self.inp.max_in_stack = self.inp.input_ptr;
            if self.inp.input_ptr == self.inp.stack_size {
                return Err(TexInterrupt::Overflow {
                    what: "input stack size",
                    size: self.inp.stack_size as i32,
                });
            }
        }
        self.inp.stack[self.inp.input_ptr] = self.inp.cur;
        self.inp.input_ptr += 1;
        Ok(())
    }

    /// `pop_input` (§322).
    pub fn pop_input(&mut self) {
        self.inp.input_ptr -= 1;
        self.inp.cur = self.inp.stack[self.inp.input_ptr];
    }

    /// `begin_token_list(p, t)` (§323).
    pub fn begin_token_list(&mut self, p: Pointer, t: u16) -> TexResult<()> {
        self.push_input()?;
        self.inp.cur.state = TOKEN_LIST;
        self.inp.cur.start = p;
        self.inp.cur.index = t; // token_type
        if t >= MACRO {
            // the token list starts with a reference count
            self.add_token_ref(p);
            if t == MACRO {
                self.inp.cur.limit = self.inp.param_ptr as i32; // param_start
            } else {
                self.inp.cur.loc = self.mem.link(p);
                if self.eqtb.int_par(crate::eqtb::TRACING_MACROS_CODE) > 1 {
                    self.begin_diagnostic();
                    self.print_nl_chars("");
                    match t {
                        MARK_TEXT => self.print_esc_str("mark"),
                        WRITE_TEXT => self.print_esc_str("write"),
                        _ => {
                            let chr = i32::from(t) - i32::from(OUTPUT_TEXT)
                                + self.eqtb.lay.output_routine_loc;
                            self.print_cmd_chr(crate::cmds::ASSIGN_TOKS, chr);
                        }
                    }
                    self.print_chars("->");
                    self.token_show(p);
                    self.end_diagnostic(false);
                }
            }
        } else {
            self.inp.cur.loc = p;
        }
        Ok(())
    }

    /// `back_list(p)` (§323).
    pub fn back_list(&mut self, p: Pointer) -> TexResult<()> {
        self.begin_token_list(p, BACKED_UP)
    }

    /// `ins_list(p)` (§323).
    pub fn ins_list(&mut self, p: Pointer) -> TexResult<()> {
        self.begin_token_list(p, INSERTED)
    }

    /// `end_token_list` (§324).
    pub fn end_token_list(&mut self) -> TexResult<()> {
        let t = self.inp.cur.index;
        if t >= BACKED_UP {
            if t <= INSERTED {
                let s = self.inp.cur.start;
                self.mem.flush_list(s);
            } else {
                let s = self.inp.cur.start;
                self.delete_token_ref(s); // update reference count
                if t == MACRO {
                    // parameters must be flushed
                    while self.inp.param_ptr > self.inp.cur.limit as usize {
                        self.inp.param_ptr -= 1;
                        let p = self.inp.param_stack[self.inp.param_ptr];
                        self.mem.flush_list(p);
                    }
                }
            }
        } else if t == U_TEMPLATE {
            if self.inp.align_state > 500_000 {
                self.inp.align_state = 0;
            } else {
                return Err(TexInterrupt::FatalError(
                    "(interwoven alignment preambles are not allowed)",
                ));
            }
        }
        self.pop_input();
        Ok(())
    }

    /// `back_input` (§325): puts `cur_tok` back into the input stream.
    pub fn back_input(&mut self) -> TexResult<()> {
        while self.inp.cur.state == TOKEN_LIST
            && self.inp.cur.loc == NULL
            && self.inp.cur.index != V_TEMPLATE
        {
            self.end_token_list()?; // conserve stack space
        }
        let p = self.mem.get_avail()?;
        let tok = self.cur_tok;
        self.mem.set_info(p, tok);
        if tok < RIGHT_BRACE_LIMIT {
            if tok < LEFT_BRACE_LIMIT {
                self.inp.align_state -= 1;
            } else {
                self.inp.align_state += 1;
            }
        }
        self.push_input()?;
        self.inp.cur.state = TOKEN_LIST;
        self.inp.cur.start = p;
        self.inp.cur.index = BACKED_UP;
        self.inp.cur.loc = p;
        Ok(())
    }

    /// `back_error` (§327): back up one token and call `error`.
    pub fn back_error(&mut self) -> TexResult<()> {
        self.back_input()?;
        self.error()
    }

    /// `ins_error` (§327): back up one *inserted* token and call `error`.
    pub fn ins_error(&mut self) -> TexResult<()> {
        self.back_input()?;
        self.inp.cur.index = INSERTED;
        self.error()
    }

    /// `begin_file_reading` (§328).
    pub fn begin_file_reading(&mut self) -> TexResult<()> {
        if self.inp.in_open == self.inp.max_in_open {
            return Err(TexInterrupt::Overflow {
                what: "text input levels",
                size: self.inp.max_in_open as i32,
            });
        }
        if self.inp.first == self.inp.buf_size {
            return Err(TexInterrupt::Overflow {
                what: "buffer size",
                size: self.inp.buf_size,
            });
        }
        self.inp.in_open += 1;
        self.push_input()?;
        self.inp.cur.index = self.inp.in_open as u16;
        // etex.ch §328: nesting bookkeeping for \everyeof and
        // \tracingnesting.
        self.eof_seen[self.inp.cur.index as usize] = false;
        self.grp_stack[self.inp.cur.index as usize] = self.save.cur_boundary;
        self.if_stack[self.inp.cur.index as usize] = self.cond_ptr;
        self.inp.line_stack[self.inp.cur.index as usize] = self.inp.line;
        self.inp.cur.start = self.inp.first;
        self.inp.cur.state = MID_LINE;
        self.inp.cur.name = 0; // terminal_input is now true
        Ok(())
    }

    /// `end_file_reading` (§329).
    pub fn end_file_reading(&mut self) {
        self.inp.first = self.inp.cur.start;
        self.inp.line = self.inp.line_stack[self.inp.cur.index as usize];
        if self.inp.cur.name == 18 || self.inp.cur.name == 19 {
            self.pseudo_close(); // etex.ch §329
        } else if self.inp.cur.name > 17 {
            // a_close(cur_file): forget it.
            self.inp.input_file[self.inp.cur.index as usize] = None;
        }
        self.pop_input();
        self.inp.in_open -= 1;
    }

    /// `runaway` (§306): prints a warning when a subfile has ended
    /// unexpectedly inside a definition/use/preamble/text.
    pub fn runaway(&mut self) {
        if self.inp.scanner_status > SKIPPING {
            self.print_nl_chars("Runaway ");
            let p = match self.inp.scanner_status {
                DEFINING => {
                    self.print_chars("definition");
                    self.inp.def_ref
                }
                MATCHING => {
                    self.print_chars("argument");
                    self.mem.temp_head()
                }
                ALIGNING => {
                    self.print_chars("preamble");
                    self.mem.hold_head()
                }
                _ => {
                    self.print_chars("text"); // absorbing
                    self.inp.def_ref
                }
            };
            self.print_char('?' as i32);
            self.print_ln();
            let l = self.mem.link(p);
            let limit = self.sizes.error_line as i32 - 10;
            self.show_token_list(l, NULL, limit);
        }
    }

    /// etex.ch: should this nesting anomaly be reported? Scans the input
    /// stack to see if level `i` corresponds to a real file.
    fn nesting_warning_wanted(&mut self, i: usize) -> bool {
        if self.eqtb.int_par(crate::eqtb::TRACING_NESTING_CODE) <= 0 {
            return false;
        }
        self.inp.stack[self.inp.input_ptr] = self.inp.cur;
        let mut base_ptr = self.inp.input_ptr;
        while self.inp.stack[base_ptr].state == TOKEN_LIST
            || self.inp.stack[base_ptr].index as usize > i
        {
            base_ptr -= 1;
        }
        self.inp.stack[base_ptr].name > 17
    }

    /// `group_warning` (etex.ch): a group ends that apparently began in a
    /// different file.
    pub fn group_warning(&mut self) {
        let mut i = self.inp.in_open;
        let mut w = false;
        while self.grp_stack[i] == self.save.cur_boundary && i > 0 {
            if self.nesting_warning_wanted(i) {
                w = true;
            }
            self.grp_stack[i] = self.save.save_index(self.save.save_ptr);
            i -= 1;
        }
        if w {
            self.print_nl_chars("Warning: end of ");
            self.print_group(true);
            self.print_chars(" of a different file");
            self.print_ln();
            if self.eqtb.int_par(crate::eqtb::TRACING_NESTING_CODE) > 1 {
                self.show_context();
            }
            if self.history == crate::error::History::Spotless {
                self.history = crate::error::History::WarningIssued;
            }
        }
    }

    /// `if_warning` (etex.ch): a conditional ends that apparently began in
    /// a different file.
    pub fn if_warning(&mut self) {
        let mut i = self.inp.in_open;
        let mut w = false;
        while self.if_stack[i] == self.cond_ptr {
            if self.nesting_warning_wanted(i) {
                w = true;
            }
            self.if_stack[i] = self.mem.link(self.cond_ptr);
            i -= 1;
        }
        if w {
            self.print_nl_chars("Warning: end of ");
            let ci = self.cur_if;
            self.print_cmd_chr(crate::cmds::IF_TEST, i32::from(ci));
            if self.if_line != 0 {
                self.print_chars(" entered on line ");
                let l = self.if_line;
                self.print_int(l);
            }
            self.print_chars(" of a different file");
            self.print_ln();
            if self.eqtb.int_par(crate::eqtb::TRACING_NESTING_CODE) > 1 {
                self.show_context();
            }
            if self.history == crate::error::History::Spotless {
                self.history = crate::error::History::WarningIssued;
            }
        }
    }

    /// `file_warning` (etex.ch): a file ends while groups or conditionals
    /// begun there are still incomplete.
    pub fn file_warning(&mut self) {
        let (p, l, c) = (self.save.save_ptr, self.save.cur_level, self.save.cur_group);
        self.save.save_ptr = self.save.cur_boundary as usize;
        while self.grp_stack[self.inp.in_open] != self.save.save_ptr as i32 {
            self.save.cur_level -= 1;
            self.print_nl_chars("Warning: end of file when ");
            self.print_group(true);
            self.print_chars(" is incomplete");
            let sp = self.save.save_ptr;
            self.save.cur_group = self.save.save_level(sp);
            self.save.save_ptr = self.save.save_index(sp) as usize;
        }
        self.save.save_ptr = p;
        self.save.cur_level = l;
        self.save.cur_group = c;
        let (p, l, c, i) = (self.cond_ptr, self.if_limit, self.cur_if, self.if_line);
        while self.if_stack[self.inp.in_open] != self.cond_ptr {
            self.print_nl_chars("Warning: end of file when ");
            let ci = self.cur_if;
            self.print_cmd_chr(crate::cmds::IF_TEST, i32::from(ci));
            if self.if_limit == crate::cond::FI_CODE {
                self.print_esc_str("else");
            }
            if self.if_line != 0 {
                self.print_chars(" entered on line ");
                let ln = self.if_line;
                self.print_int(ln);
            }
            self.print_chars(" is incomplete");
            self.if_line = self.mem.word(self.cond_ptr + 1).int();
            self.cur_if = self.mem.subtype(self.cond_ptr);
            self.if_limit = self.mem.node_type(self.cond_ptr) as u8;
            self.cond_ptr = self.mem.link(self.cond_ptr);
        }
        self.cond_ptr = p;
        self.if_limit = l;
        self.cur_if = c;
        self.if_line = i;
        self.print_ln();
        if self.eqtb.int_par(crate::eqtb::TRACING_NESTING_CODE) > 1 {
            self.show_context();
        }
        if self.history == crate::error::History::Spotless {
            self.history = crate::error::History::WarningIssued;
        }
    }

    /// `pseudo_start` (etex.ch): `\scantokens` — print a balanced text
    /// through the string pool and convert it into a pseudo file (lines of
    /// UTF-16 units packed four per `mem` word), then start reading it.
    pub fn pseudo_start(&mut self) -> TexResult<()> {
        self.scan_general_text()?;
        let old_setting = self.prn.selector;
        self.prn.selector = crate::print::NEW_STRING;
        let th = self.mem.temp_head();
        self.token_show(th);
        self.prn.selector = old_setting;
        let l = self.mem.link(th);
        self.mem.flush_list(l);
        // make_string + flush_string, without entering the string table.
        let units = self.strings.take_cur_string();
        // Convert the string into a new pseudo file.
        let nl = self.eqtb.int_par(crate::eqtb::NEW_LINE_CHAR_CODE);
        let nl_unit: Option<u16> = u16::try_from(nl).ok();
        let p = self.mem.get_avail()?;
        let mut q = p;
        let mut l = 0usize;
        while l < units.len() {
            let m = l;
            while l < units.len() && Some(units[l]) != nl_unit {
                l += 1;
            }
            let line = &units[m..l];
            let mut sz = (line.len() as i32 + 7) / 4;
            if sz == 1 {
                sz = 2;
            }
            let r = self.mem.get_node(sz)?;
            self.mem.set_link(q, r);
            q = r;
            self.mem.set_info(q, sz);
            for (k, chunk) in line.chunks(4).enumerate() {
                let w = self.mem.word_mut(r + 1 + k as i32);
                for i in 0..4 {
                    w.set_qqqq(i, chunk.get(i).copied().unwrap_or(b' ' as u16));
                }
            }
            if l < units.len() {
                l += 1; // skip the newline character
            }
        }
        let first_line = self.mem.link(p);
        self.mem.set_info(p, first_line);
        self.mem.set_link(p, self.pseudo_files);
        self.pseudo_files = p;
        // Initiate input from the new pseudo file.
        self.begin_file_reading()?;
        self.inp.line = 0;
        self.inp.cur.limit = self.inp.cur.start;
        self.inp.cur.loc = self.inp.cur.limit + 1; // force line read
        if self.eqtb.int_par(crate::eqtb::TRACING_SCAN_TOKENS_CODE) > 0 {
            // etex.ch: show the pseudo file like a real file open.
            if self.prn.term_offset > self.sizes.max_print_line - 3 {
                self.print_ln();
            } else if self.prn.term_offset > 0 || self.prn.file_offset > 0 {
                self.print_char(' ' as i32);
            }
            self.inp.cur.name = 19;
            self.print_chars("( ");
            self.inp.open_parens += 1;
        } else {
            self.inp.cur.name = 18;
        }
        Ok(())
    }

    /// `pseudo_input` (etex.ch): reads the next pseudo-file line into
    /// `buffer[first..]`; returns false at end of file.
    pub fn pseudo_input(&mut self) -> crate::error::TexResult<bool> {
        self.inp.last = self.inp.first; // cf. Matthew 19:30
        let p = self.mem.info(self.pseudo_files);
        if p == NULL {
            return Ok(false);
        }
        let nxt = self.mem.link(p);
        self.mem.set_info(self.pseudo_files, nxt);
        let sz = self.mem.info(p);
        let mut units: Vec<u16> = Vec::with_capacity(4 * (sz as usize - 1));
        for r in (p + 1)..(p + sz) {
            let w = self.mem.word(r);
            for i in 0..4 {
                units.push(w.qqqq(i));
            }
        }
        self.mem.free_node(p, sz);
        let line: Vec<UnicodeChar> = char::decode_utf16(units)
            .map(|c| c.map(|c| c as UnicodeChar).unwrap_or(0xFFFD))
            .collect();
        let raw_last = self.inp.first + line.len() as i32;
        self.copy_line_to_buffer(&line)?;
        // etex.ch (pseudo_input): unlike §31, max_buf_stack accounts for
        // the padded line PLUS one, measured after the copy loop.
        if raw_last >= self.inp.max_buf_stack {
            self.inp.max_buf_stack = raw_last + 1;
        }
        Ok(true)
    }

    /// `pseudo_close` (etex.ch): closes the top pseudo file.
    pub fn pseudo_close(&mut self) {
        let p = self.mem.link(self.pseudo_files);
        let mut q = self.mem.info(self.pseudo_files);
        let pf = self.pseudo_files;
        self.mem.free_avail(pf);
        self.pseudo_files = p;
        while q != NULL {
            let r = q;
            q = self.mem.link(r);
            let sz = self.mem.info(r);
            self.mem.free_node(r, sz);
        }
    }

    /// `input_ln` (§31) for the current input file: copies the next stored
    /// line into `buffer[first..]`, setting `last`. Returns false at EOF.
    pub fn input_ln_file(&mut self) -> crate::error::TexResult<bool> {
        let idx = self.inp.cur.index as usize;
        let Some(file) = self.inp.input_file[idx].as_mut() else {
            return Ok(false);
        };
        if file.next >= file.lines.len() {
            return Ok(false);
        }
        let line = file.lines[file.next].clone();
        file.next += 1;
        self.copy_line_to_buffer(&line)
    }

    /// Shared tail of `input_ln` (§31): place a raw line at
    /// `buffer[first..]`, growing `max_buf_stack` over the raw length, then
    /// drop trailing blanks via `last_nonblank`.
    pub fn copy_line_to_buffer(&mut self, line: &[UnicodeChar]) -> crate::error::TexResult<bool> {
        let first = self.inp.first;
        self.inp.last = first;
        let mut last_nonblank = first;
        for &c in line {
            if self.inp.last >= self.inp.max_buf_stack {
                self.inp.max_buf_stack = self.inp.last + 1;
            }
            if self.inp.last >= self.inp.buf_size {
                // §31 (via §35 overflow): a silent truncation here would
                // desync the tokenizer (and once crashed ^^-reduction).
                return Err(crate::error::TexInterrupt::Overflow {
                    what: "buffer size",
                    size: self.inp.buf_size,
                });
            }
            self.inp.buffer[self.inp.last as usize] = c;
            self.inp.last += 1;
            if c != ' ' as UnicodeChar {
                last_nonblank = self.inp.last;
            }
        }
        self.inp.last = last_nonblank;
        Ok(true)
    }

    /// `firm_up_the_line` (§363): sets `limit`; `\pausing` is not supported
    /// yet (TODO(M5)).
    pub fn firm_up_the_line(&mut self) {
        self.inp.cur.limit = self.inp.last;
    }

    /// `term_input` (§71), reduced: reads one scripted/interactive line into
    /// `buffer[first..]`. Returns false on end of terminal input.
    pub fn term_input_line(&mut self) -> crate::error::TexResult<bool> {
        match self.term.read_line() {
            None => Ok(false),
            Some(s) => {
                let mut line: Vec<UnicodeChar> = s.chars().map(|c| c as UnicodeChar).collect();
                while line.last() == Some(&(' ' as UnicodeChar)) {
                    line.pop();
                }
                // Echo to the log as §71 does (selector dance omitted).
                self.copy_line_to_buffer(&line)
            }
        }
    }

    /// `prompt_input(s)` (§71): prints `s` and reads a terminal line into
    /// `buffer[first..last]`, echoing it to the transcript.
    pub fn prompt_input(&mut self, s: &str) -> TexResult<()> {
        self.print_chars(s);
        if !self.term_input_line()? {
            return Err(self.fatal_error("End of file on the terminal!"));
        }
        self.prn.term_offset = 0; // the user's line ended with <return>
        self.prn.selector -= 1; // prepare to echo the input
        if self.inp.last != self.inp.first {
            for k in self.inp.first..self.inp.last {
                let c = self.inp.buffer[k as usize];
                self.print_char(c);
            }
        }
        self.print_ln();
        self.prn.selector += 1;
        Ok(())
    }

    /// `prompt_file_name("output file name", ".tex")` (§530), reached from
    /// §1374 when `\openout` cannot write its file. In batch/nonstop mode
    /// this is a fatal error ("job aborted, file error"); interactively it
    /// reads a replacement name from the terminal (§531 buffer scan).
    pub fn prompt_output_file_name(&mut self, name: &str) -> TexResult<String> {
        self.print_err("I can't write on file `");
        self.print_chars(name);
        self.print_chars("'.");
        self.show_context(); // §530: e = ".tex"
        self.print_nl_chars("Please type another output file name");
        if self.interaction < SCROLL_MODE {
            return Err(self.fatal_error("*** (job aborted, file error in nonstop mode)"));
        }
        self.prompt_input(": ")?;
        // §531: scan the file name in the buffer.
        let mut k = self.inp.first;
        while k < self.inp.last && self.inp.buffer[k as usize] == ' ' as i32 {
            k += 1;
        }
        let mut s = String::new();
        while k < self.inp.last {
            let c = self.inp.buffer[k as usize];
            if c == ' ' as i32 {
                break;
            }
            if let Some(ch) = char::from_u32(c as u32) {
                s.push(ch);
            }
            k += 1;
        }
        if !s.contains('.') {
            s.push_str(".tex");
        }
        Ok(s)
    }

    /// `open_or_close_in` (§1275): `\openin`, `\closein`.
    pub fn open_or_close_in(&mut self) -> TexResult<()> {
        use crate::cond::{CLOSED, JUST_OPEN};
        let c = self.cur_chr;
        self.scan_four_bit_int()?;
        let n = self.cur_val as usize;
        if self.read_open[n] != CLOSED {
            self.inp.read_file[n] = None;
            self.read_open[n] = CLOSED;
        }
        if c != 0 {
            self.scan_optional_equals()?;
            self.scan_file_name()?;
            let mut name = std::mem::take(&mut self.cur_name);
            if !name.contains('.') {
                name.push_str(".tex");
            }
            if let Some(bytes) = self.fs.read_file(&name, crate::io::FileKind::OpenIn) {
                let lines = decode_lines(&bytes);
                self.inp.read_file[n] = Some(FileSource { lines, next: 0 });
                self.read_open[n] = JUST_OPEN;
            }
        }
        Ok(())
    }

    /// `end_line_char_inactive` (§360), with the XeTeX-sized char range.
    pub fn end_line_char_inactive(&self) -> bool {
        let e = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
        !(0..=0x10FFFF).contains(&e)
    }

    /// `start_input` (§537-§538, simplified): opens the file named by
    /// `scan_file_name` and starts reading it. kpathsea-style searching is
    /// reduced to "ask TexFs for `name` then `name.tex`".
    pub fn start_input(&mut self) -> TexResult<()> {
        self.scan_file_name()?;
        self.start_input_resolved()
    }

    /// The §537 body, once `cur_name` holds the file name.
    pub fn start_input_resolved(&mut self) -> TexResult<()> {
        let mut name = std::mem::take(&mut self.cur_name);
        let bytes = match self.fs.read_file(&name, crate::io::FileKind::Tex) {
            Some(b) => Some(b),
            None => {
                if !name.contains('.') {
                    name.push_str(".tex");
                    self.fs.read_file(&name, crate::io::FileKind::Tex)
                } else {
                    None
                }
            }
        };
        let Some(bytes) = bytes else {
            // §530 prompts for another name; in this port a missing file is
            // an error the host must resolve (missing-file protocol).
            self.print_err("I can't find file `");
            self.print_chars(&name);
            self.print_chars("'.");
            return self.error();
        };
        let lines = decode_lines(&bytes);
        self.begin_file_reading()?; // set up cur_file and new level of input
        let idx = self.inp.cur.index as usize;
        self.inp.input_file[idx] = Some(FileSource { lines, next: 0 });
        let name_str = self.strings.intern(&name)?;
        self.inp.cur.name = name_str;
        if self.job_name.is_none() {
            self.job_name = Some(
                name.trim_end_matches(".tex")
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("texput")
                    .to_string(),
            );
            self.open_log_file()?; // §537
        }
        // §537: print the file-open paren, with the usual spacing rule.
        if self.prn.term_offset + name.chars().count() > self.sizes.max_print_line - 2 {
            self.print_ln();
        } else if self.prn.term_offset > 0 || self.prn.file_offset > 0 {
            self.print_char(' ' as i32);
        }
        self.print_char('(' as i32);
        self.inp.open_parens += 1;
        self.print_chars(&name);
        self.inp.line = 1;
        // §538: read the first line of the new file.
        if self.input_ln_file()? {
            self.firm_up_the_line();
            if self.end_line_char_inactive() {
                self.inp.cur.limit -= 1;
            } else {
                let e = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
                self.inp.buffer[self.inp.cur.limit as usize] = e;
            }
            self.inp.first = self.inp.cur.limit + 1;
            self.inp.cur.loc = self.inp.cur.start;
        }
        self.inp.cur.state = NEW_LINE;
        Ok(())
    }

    /// `scan_file_name` (§526-§537, simplified to the §526 grammar): skips
    /// spaces, then collects characters until a space or non-character
    /// token, leaving the result in `cur_name`.
    pub fn scan_file_name(&mut self) -> TexResult<()> {
        self.name_in_progress = true;
        // <Get the next non-blank non-call token> (§406)
        loop {
            self.get_x_token()?;
            if !(self.cur_cmd == crate::cmds::SPACER && self.cur_chr == ' ' as i32) {
                break;
            }
        }
        let mut name = String::new();
        // XeTeX: a quoted name may contain spaces and marks the request
        // as a candidate for a native font.
        self.quoted_filename = false;
        let mut quoted = false;
        loop {
            if self.cur_cmd > crate::cmds::OTHER_CHAR || self.cur_chr > 0x10FFFF {
                // not a character
                self.back_input()?;
                break;
            }
            if self.cur_chr == '"' as i32 {
                quoted = !quoted;
                self.quoted_filename = true;
                self.get_x_token()?;
                continue;
            }
            if self.cur_cmd == crate::cmds::SPACER && !quoted {
                break;
            }
            if let Some(c) = char::from_u32(self.cur_chr as u32) {
                name.push(c);
            }
            self.get_x_token()?;
        }
        self.cur_name = name;
        self.name_in_progress = false;
        Ok(())
    }
}
