//! Conditional processing.
//!
//! Ports tex.web Part 28 (§487-§500): the condition stack, `pass_text`,
//! `change_if_limit` and `conditional`.

use crate::cmds::*;
use crate::engine::Engine;
use crate::error::TexResult;
use crate::getnext::{NO_EXPAND_FLAG, TOO_BIG_CHAR};
use crate::input::{NORMAL_STATUS, SKIPPING};
use crate::mem::{HLIST_NODE, VLIST_NODE};
use crate::tokens::*;
use crate::types::{Pointer, NULL};

// §487: the \if codes.
pub const IF_CHAR_CODE: i32 = 0;
pub const IF_CAT_CODE: i32 = 1;
pub const IF_INT_CODE: i32 = 2;
pub const IF_DIM_CODE: i32 = 3;
pub const IF_ODD_CODE: i32 = 4;
pub const IF_VMODE_CODE: i32 = 5;
pub const IF_HMODE_CODE: i32 = 6;
pub const IF_MMODE_CODE: i32 = 7;
pub const IF_INNER_CODE: i32 = 8;
pub const IF_VOID_CODE: i32 = 9;
pub const IF_HBOX_CODE: i32 = 10;
pub const IF_VBOX_CODE: i32 = 11;
pub const IFX_CODE: i32 = 12;
pub const IF_EOF_CODE: i32 = 13;
pub const IF_TRUE_CODE: i32 = 14;
pub const IF_FALSE_CODE: i32 = 15;
pub const IF_CASE_CODE: i32 = 16;
// etex.ch: the additional conditionals and the \unless prefix.
pub const IF_DEF_CODE: i32 = 17;
pub const IF_CS_CODE: i32 = 18;
pub const IF_FONT_CHAR_CODE: i32 = 19;
/// pdfTeX/XeTeX `\ifincsname` (also in xetex).
pub const IF_IN_CSNAME_CODE: i32 = 20;
/// pTeX direction conditionals. Until tate (vertical) typesetting
/// exists the list direction is always yoko: \ifydir is true,
/// \iftdir/\ifmdir false. graphics' xetex.def emits \iftdir when it
/// detects a pTeX-family engine (\ifx\kanjiskip test).
pub const IF_TDIR_CODE: i32 = 21;
pub const IF_YDIR_CODE: i32 = 22;
pub const IF_MDIR_CODE: i32 = 23;
/// `unless_code` (etex.ch): amount added for the `\unless` prefix.
pub const UNLESS_CODE: i32 = 32;

// §489: condition stack codes.
pub const IF_NODE_SIZE: i32 = 2;
pub const IF_CODE: u8 = 1;
pub const FI_CODE: u8 = 2;
pub const ELSE_CODE: u8 = 3;
pub const OR_CODE: u8 = 4;

/// `read_open` states (§480).
pub const CLOSED: u8 = 2;
pub const JUST_OPEN: u8 = 1;

impl Engine {
    /// §495: push the condition stack.
    fn push_cond_stack(&mut self) -> TexResult<()> {
        let p = self.mem.get_node(IF_NODE_SIZE)?;
        let c = self.cond_ptr;
        self.mem.set_link(p, c);
        let (lim, ci, il) = (self.if_limit, self.cur_if, self.if_line);
        self.mem.word_mut(p).set_b0(u16::from(lim)); // type = if_limit
        self.mem.word_mut(p).set_b1(ci); // subtype = cur_if
        self.mem.word_mut(p + 1).set_int(il); // if_line_field
        self.cond_ptr = p;
        self.cur_if = self.cur_chr as u16;
        self.if_limit = IF_CODE;
        self.if_line = self.inp.line;
        Ok(())
    }

    /// §496 (+ etex.ch): pop the condition stack.
    pub fn pop_cond_stack(&mut self) {
        if self.if_stack[self.inp.in_open] == self.cond_ptr {
            self.if_warning(); // conditionals not properly nested with files
        }
        let p = self.cond_ptr;
        self.if_line = self.mem.word(p + 1).int();
        self.cur_if = self.mem.word(p).b1();
        self.if_limit = self.mem.word(p).b0() as u8;
        self.cond_ptr = self.mem.link(p);
        self.mem.free_node(p, IF_NODE_SIZE);
    }

    /// `change_if_limit(l, p)` (§497).
    fn change_if_limit(&mut self, l: u8, p: Pointer) -> TexResult<()> {
        if p == self.cond_ptr {
            self.if_limit = l; // that's the easy case
            Ok(())
        } else {
            let mut q = self.cond_ptr;
            loop {
                if q == NULL {
                    return self.confusion("if");
                }
                if self.mem.link(q) == p {
                    self.mem.word_mut(q).set_b0(u16::from(l));
                    return Ok(());
                }
                q = self.mem.link(q);
            }
        }
    }

    /// `pass_text` (§494): ignores text up to `\or`/`\else`/`\fi` at the
    /// current nesting level; afterwards `cur_chr` indicates what was found.
    pub fn pass_text(&mut self) -> TexResult<()> {
        let save_scanner_status = self.inp.scanner_status;
        self.inp.scanner_status = SKIPPING;
        let mut l = 0i32;
        self.skip_line = self.inp.line;
        loop {
            self.get_next()?;
            if self.cur_cmd == FI_OR_ELSE {
                if l == 0 {
                    break;
                }
                if self.cur_chr == i32::from(FI_CODE) {
                    l -= 1;
                }
            } else if self.cur_cmd == IF_TEST {
                l += 1;
            }
        }
        self.inp.scanner_status = save_scanner_status;
        // etex.ch §494: 	racingifs shows the \or/\else/i that ended
        // the skipped text.
        if self.eqtb.int_par(crate::eqtb::TRACING_IFS_CODE) > 0 {
            self.show_cur_cmd_chr();
        }
        Ok(())
    }

    /// §380-variant: `get_x_token_or_active_char` (§506).
    fn get_x_token_or_active_char(&mut self) -> TexResult<()> {
        self.get_x_token()?;
        if self.cur_cmd == RELAX && self.cur_chr == NO_EXPAND_FLAG {
            self.cur_cmd = ACTIVE_CHAR;
            self.cur_chr = self.cur_tok - CS_TOKEN_FLAG - self.eqtb.lay.active_base;
        }
        Ok(())
    }

    /// `conditional` (§498-§509 + etex.ch): evaluates an `\if...` test.
    pub fn conditional(&mut self) -> TexResult<()> {
        // etex.ch: \tracingifs shows the conditional even when
        // \tracingcommands didn't already.
        if self.eqtb.int_par(crate::eqtb::TRACING_IFS_CODE) > 0
            && self.eqtb.int_par(crate::eqtb::TRACING_COMMANDS_CODE) <= 1
        {
            self.show_cur_cmd_chr();
        }
        self.push_cond_stack()?;
        let save_cond_ptr = self.cond_ptr;
        // etex.ch: was this if preceded by `\unless`?
        let is_unless = self.cur_chr >= UNLESS_CODE;
        let this_if = self.cur_chr % UNLESS_CODE;
        let mut b = false;
        // §501: either process \ifcase or set b.
        let mut is_case_pending = false;
        match this_if {
            IF_CHAR_CODE | IF_CAT_CODE => {
                // §506: test if two characters match.
                self.get_x_token_or_active_char()?;
                let (m, n) = if self.cur_cmd > ACTIVE_CHAR || self.cur_chr > 0x10FFFF {
                    (RELAX, TOO_BIG_CHAR)
                } else {
                    (self.cur_cmd, self.cur_chr)
                };
                self.get_x_token_or_active_char()?;
                if self.cur_cmd > ACTIVE_CHAR || self.cur_chr > 0x10FFFF {
                    self.cur_cmd = RELAX;
                    self.cur_chr = TOO_BIG_CHAR;
                }
                b = if this_if == IF_CHAR_CODE {
                    n == self.cur_chr
                } else {
                    m == self.cur_cmd
                };
            }
            IF_INT_CODE | IF_DIM_CODE => {
                // §503: test relation between integers or dimensions.
                if this_if == IF_INT_CODE {
                    self.scan_int()?;
                } else {
                    self.scan_normal_dimen()?;
                }
                let n = self.cur_val;
                self.get_next_nonblank_noncall()?;
                let r = if self.cur_tok >= OTHER_TOKEN + '<' as i32
                    && self.cur_tok <= OTHER_TOKEN + '>' as i32
                {
                    self.cur_tok - OTHER_TOKEN
                } else {
                    self.print_err("Missing = inserted for ");
                    self.print_cmd_chr(IF_TEST, this_if);
                    self.help(&["I was expecting to see `<', `=', or `>'. Didn't."]);
                    self.back_error()?;
                    '=' as i32
                };
                if this_if == IF_INT_CODE {
                    self.scan_int()?;
                } else {
                    self.scan_normal_dimen()?;
                }
                b = match r {
                    x if x == '<' as i32 => n < self.cur_val,
                    x if x == '=' as i32 => n == self.cur_val,
                    _ => n > self.cur_val,
                };
            }
            IF_ODD_CODE => {
                // §504.
                self.scan_int()?;
                b = self.cur_val % 2 != 0;
            }
            IF_VMODE_CODE => b = self.mode().abs() == crate::engine::VMODE,
            IF_HMODE_CODE => b = self.mode().abs() == crate::engine::HMODE,
            IF_MMODE_CODE => b = self.mode().abs() == crate::engine::MMODE,
            IF_INNER_CODE => b = self.mode() < 0,
            IF_VOID_CODE | IF_HBOX_CODE | IF_VBOX_CODE => {
                // §505 (+ etex.ch): test box register status.
                self.scan_register_num()?;
                let v = self.cur_val;
                let p = self.fetch_box(v)?;
                b = if this_if == IF_VOID_CODE {
                    p == NULL
                } else if p == NULL {
                    false
                } else if this_if == IF_HBOX_CODE {
                    self.mem.word(p).b0() == HLIST_NODE
                } else {
                    self.mem.word(p).b0() == VLIST_NODE
                };
            }
            IFX_CODE => {
                // §507: test if two tokens match.
                let save_scanner_status = self.inp.scanner_status;
                self.inp.scanner_status = NORMAL_STATUS;
                self.get_next()?;
                let n = self.cur_cs;
                let p = self.cur_cmd;
                let q = self.cur_chr;
                self.get_next()?;
                if self.cur_cmd != p {
                    b = false;
                } else if self.cur_cmd < CALL {
                    b = self.cur_chr == q;
                } else {
                    // §508: test if two macro texts match.
                    let mut pp = self.mem.link(self.cur_chr);
                    let mut qq = self.mem.link(self.eqtb.equiv(n)); // omit ref counts
                    if pp == qq {
                        b = true;
                    } else {
                        while pp != NULL && qq != NULL {
                            if self.mem.info(pp) != self.mem.info(qq) {
                                pp = NULL;
                            } else {
                                pp = self.mem.link(pp);
                                qq = self.mem.link(qq);
                            }
                        }
                        b = pp == NULL && qq == NULL;
                    }
                }
                self.inp.scanner_status = save_scanner_status;
            }
            IF_EOF_CODE => {
                self.scan_four_bit_int()?;
                b = self.read_open[self.cur_val as usize] == CLOSED;
            }
            IF_TRUE_CODE => b = true,
            IF_FALSE_CODE => b = false,
            IF_DEF_CODE => {
                // etex.ch: test if a control sequence is defined. \outer
                // macros are allowed, so scanner_status is reset.
                let save_scanner_status = self.inp.scanner_status;
                self.inp.scanner_status = NORMAL_STATUS;
                self.get_next()?;
                b = self.cur_cmd != UNDEFINED_CS;
                self.inp.scanner_status = save_scanner_status;
            }
            IF_CS_CODE => {
                // etex.ch: like \expandafter\ifdefined\csname, but without
                // entering a new control sequence into the hash table.
                let n = self.mem.get_avail()?;
                let mut p = n;
                loop {
                    self.get_x_token()?;
                    if self.cur_cs != 0 {
                        break;
                    }
                    let q = self.mem.get_avail()?;
                    self.mem.set_link(p, q);
                    let t = self.cur_tok;
                    self.mem.set_info(q, t);
                    p = q;
                }
                if self.cur_cmd != END_CS_NAME {
                    self.complain_missing_endcsname()?;
                }
                // Look up the characters of list n in the hash table.
                let first = self.inp.first;
                let mut m = first;
                let mut q = self.mem.link(n);
                while q != NULL {
                    if m >= self.inp.max_buf_stack {
                        self.inp.max_buf_stack = m + 1;
                    }
                    self.inp.buffer[m as usize] = self.mem.info(q) % crate::tokens::MAX_CHAR_VAL;
                    m += 1;
                    q = self.mem.link(q);
                }
                self.cur_cs = if m > first + 1 {
                    self.id_lookup(first, m - first) // no_new_control_sequence is true
                } else if m == first {
                    self.eqtb.lay.null_cs // the list is empty
                } else {
                    self.eqtb.lay.single_base + self.inp.buffer[first as usize]
                };
                self.mem.flush_list(n);
                b = self.eqtb.eq_type(self.cur_cs) != UNDEFINED_CS;
            }
            IF_TDIR_CODE | IF_MDIR_CODE => {
                b = false; // horizontal-only engine (A7 pending)
            }
            IF_YDIR_CODE => {
                b = true;
            }
            IF_IN_CSNAME_CODE => {
                // pdftex.web: true while scanning a \csname body.
                b = self.in_csname;
            }
            IF_FONT_CHAR_CODE => {
                // etex.ch: test the existence of a character in a font.
                self.scan_font_ident()?;
                let f = self.cur_val;
                self.scan_char_num()?;
                b = self.fonts.bc[f as usize] <= self.cur_val
                    && self.fonts.ec[f as usize] >= self.cur_val
                    && crate::fonts::FontMem::char_exists(self.fonts.char_info(f, self.cur_val));
            }
            _ => {
                // §509: \ifcase — select the appropriate case.
                self.scan_int()?;
                let mut n = self.cur_val; // number of cases to pass
                if self.eqtb.int_par(crate::eqtb::TRACING_COMMANDS_CODE) > 1 {
                    self.begin_diagnostic();
                    self.print_chars("{case ");
                    self.print_int(n);
                    self.print_char('}' as i32);
                    self.end_diagnostic(false);
                }
                let mut found_case = true;
                while n != 0 {
                    self.pass_text()?;
                    if self.cond_ptr == save_cond_ptr {
                        if self.cur_chr == i32::from(OR_CODE) {
                            n -= 1;
                        } else {
                            // goto common_ending
                            found_case = false;
                            break;
                        }
                    } else if self.cur_chr == i32::from(FI_CODE) {
                        self.pop_cond_stack();
                    }
                }
                if found_case {
                    self.change_if_limit(OR_CODE, save_cond_ptr)?;
                    return Ok(()); // wait for \or, \else, or \fi
                }
                is_case_pending = true; // fall through to common_ending
            }
        }
        if !is_case_pending {
            if is_unless {
                b = !b; // etex.ch: \unless reverses the result
            }
            if self.eqtb.int_par(crate::eqtb::TRACING_COMMANDS_CODE) > 1 {
                // §502: display the value of b.
                self.begin_diagnostic();
                if b {
                    self.print_chars("{true}");
                } else {
                    self.print_chars("{false}");
                }
                self.end_diagnostic(false);
            }
            if b {
                self.change_if_limit(ELSE_CODE, save_cond_ptr)?;
                return Ok(()); // wait for \else or \fi
            }
            // §500: skip to \else or \fi.
            loop {
                self.pass_text()?;
                if self.cond_ptr == save_cond_ptr {
                    if self.cur_chr != i32::from(OR_CODE) {
                        break; // goto common_ending
                    }
                    self.print_err("Extra ");
                    self.print_esc_str("or");
                    self.help(&["I'm ignoring this; it doesn't match any \\if."]);
                    self.error()?;
                } else if self.cur_chr == i32::from(FI_CODE) {
                    self.pop_cond_stack();
                }
            }
        }
        // common_ending:
        if self.cur_chr == i32::from(FI_CODE) {
            self.pop_cond_stack();
        } else {
            self.if_limit = FI_CODE; // wait for \fi
        }
        Ok(())
    }
}
