//! Token lists.
//!
//! Ports tex.web Part 20 (§289-§296). A token is a halfword: a character
//! token packs `cmd * MAX_CHAR_VAL + chr` and a control sequence whose eqtb
//! address is `p` is `CS_TOKEN_FLAG + p`.
//!
//! Unicode adaptation (XeTeX layout): tex.web packs `2^8 m + c`; here the
//! character field is 21 bits (`MAX_CHAR_VAL = 0x200000 > 0x10FFFF`) and
//! `CS_TOKEN_FLAG = 2^25 - 1`, the XeTeX values. All derived constants
//! follow the tex.web §289 formulas.

use crate::cmds::*;
use crate::engine::Engine;
use crate::mem::GLUE_SPEC_SIZE;
use crate::types::{Halfword, Pointer, NULL};

/// Size of the character field in a packed token.
pub const MAX_CHAR_VAL: Halfword = 0x20_0000;

/// `cs_token_flag` (§289): added to an eqtb location to form a cs token.
pub const CS_TOKEN_FLAG: Halfword = 0x1FF_FFFF;

pub const LEFT_BRACE_TOKEN: Halfword = LEFT_BRACE as Halfword * MAX_CHAR_VAL;
pub const LEFT_BRACE_LIMIT: Halfword = (LEFT_BRACE as Halfword + 1) * MAX_CHAR_VAL;
pub const RIGHT_BRACE_TOKEN: Halfword = RIGHT_BRACE as Halfword * MAX_CHAR_VAL;
pub const RIGHT_BRACE_LIMIT: Halfword = (RIGHT_BRACE as Halfword + 1) * MAX_CHAR_VAL;
pub const MATH_SHIFT_TOKEN: Halfword = MATH_SHIFT as Halfword * MAX_CHAR_VAL;
pub const TAB_TOKEN: Halfword = TAB_MARK as Halfword * MAX_CHAR_VAL;
pub const OUT_PARAM_TOKEN: Halfword = OUT_PARAM as Halfword * MAX_CHAR_VAL;
pub const SPACE_TOKEN: Halfword = SPACER as Halfword * MAX_CHAR_VAL + ' ' as Halfword;
pub const LETTER_TOKEN: Halfword = LETTER as Halfword * MAX_CHAR_VAL;
pub const OTHER_TOKEN: Halfword = OTHER_CHAR as Halfword * MAX_CHAR_VAL;
pub const MATCH_TOKEN: Halfword = MATCH as Halfword * MAX_CHAR_VAL;
pub const END_MATCH_TOKEN: Halfword = END_MATCH as Halfword * MAX_CHAR_VAL;
/// `protected_token` (etex.ch §289): marks a `\protected` macro.
pub const PROTECTED_TOKEN: Halfword = END_MATCH_TOKEN + 1;

/// `zero_token`, `A_token` etc. (§445-§446), used by `scan_int`.
pub const ZERO_TOKEN: Halfword = OTHER_TOKEN + '0' as Halfword;
pub const OCTAL_TOKEN: Halfword = OTHER_TOKEN + '\'' as Halfword;
pub const HEX_TOKEN: Halfword = OTHER_TOKEN + '"' as Halfword;
pub const ALPHA_TOKEN: Halfword = OTHER_TOKEN + '`' as Halfword;
pub const POINT_TOKEN: Halfword = OTHER_TOKEN + '.' as Halfword;
pub const CONTINENTAL_POINT_TOKEN: Halfword = OTHER_TOKEN + ',' as Halfword;
pub const A_TOKEN: Halfword = LETTER_TOKEN + 'A' as Halfword;
pub const OTHER_A_TOKEN: Halfword = OTHER_TOKEN + 'A' as Halfword;

impl Engine {
    /// `token_ref_count(p) == info(p)` (§203); `add_token_ref(p)`.
    pub fn add_token_ref(&mut self, p: Pointer) {
        let c = self.mem.info(p);
        self.mem.set_info(p, c + 1);
    }

    /// `delete_token_ref(p)` (§200): `p` points to the reference count of a
    /// token list; a count of `null` means one reference.
    pub fn delete_token_ref(&mut self, p: Pointer) {
        if self.mem.info(p) == NULL {
            self.mem.flush_list(p);
        } else {
            let c = self.mem.info(p);
            self.mem.set_info(p, c - 1);
        }
    }

    /// `delete_glue_ref(p)` — forwarded so `eq_destroy` reads like §275.
    pub fn delete_glue_ref(&mut self, p: Pointer) {
        self.mem.delete_glue_ref(p);
    }

    /// `new_spec(p)` (§151): duplicates a glue specification.
    pub fn new_spec(&mut self, p: Pointer) -> crate::error::TexResult<Pointer> {
        let q = self.mem.get_node(GLUE_SPEC_SIZE)?;
        *self.mem.word_mut(q) = self.mem.word(p);
        self.mem.set_glue_ref_count(q, NULL);
        let (w, st, sh) = (self.mem.width(p), self.mem.stretch(p), self.mem.shrink(p));
        self.mem.set_width(q, w);
        self.mem.set_stretch(q, st);
        self.mem.set_shrink(q, sh);
        Ok(q)
    }

    /// `show_token_list(p, q, l)` (§292-§294): prints a symbolic form of a
    /// token list (which should not begin with a reference count). `q`
    /// marks the spot for the error-context "magic computation" (§320).
    pub fn show_token_list(&mut self, p: Pointer, q: Pointer, l: i32) {
        let mut match_chr = '#' as i32;
        let mut n = '0' as i32;
        self.prn.tally = 0;
        let mut p = p;
        while p != NULL && self.prn.tally < l {
            if p == q {
                // §320: do magic computation (for show_context).
                self.set_trick_count();
            }
            // §293: display token p.
            if p < self.mem.hi_mem_min || p > self.mem.mem_end {
                self.print_esc_str("CLOBBERED.");
                return;
            }
            let t = self.mem.info(p);
            if t >= CS_TOKEN_FLAG {
                self.print_cs(t - CS_TOKEN_FLAG);
            } else if t < 0 {
                self.print_esc_str("BAD.");
            } else {
                // §294: display the token (m, c).
                let m = (t / MAX_CHAR_VAL) as u16;
                let c = t % MAX_CHAR_VAL;
                match m {
                    LEFT_BRACE | RIGHT_BRACE | MATH_SHIFT | TAB_MARK | SUP_MARK | SUB_MARK
                    | SPACER | LETTER | OTHER_CHAR => self.print_char_code(c),
                    MAC_PARAM => {
                        self.print_char_code(c);
                        self.print_char_code(c);
                    }
                    OUT_PARAM => {
                        self.print_char_code(match_chr);
                        if c <= 9 {
                            self.print_char('0' as i32 + c);
                        } else {
                            self.print_char('!' as i32);
                            return;
                        }
                    }
                    MATCH => {
                        match_chr = c;
                        self.print_char_code(c);
                        n += 1;
                        self.print_char(n);
                        if n > '9' as i32 {
                            return;
                        }
                    }
                    END_MATCH => {
                        // etex.ch §294: the protected marker (chr 1) is
                        // invisible here.
                        if c == 0 {
                            self.print_chars("->");
                        }
                    }
                    _ => self.print_esc_str("BAD."),
                }
            }
            p = self.mem.link(p);
        }
        if p != NULL {
            self.print_esc_str("ETC.");
        }
    }

    /// `token_show(p)` (§295): shows a token list given its reference count.
    pub fn token_show(&mut self, p: Pointer) {
        if p != NULL {
            let l = self.mem.link(p);
            self.show_token_list(l, NULL, 10_000_000);
        }
    }

    /// `print_meaning` (§296): displays `cur_cmd`/`cur_chr` symbolically,
    /// including macro expansions.
    pub fn print_meaning(&mut self) {
        let (cmd, chr) = (self.cur_cmd, self.cur_chr);
        self.print_cmd_chr(cmd, chr);
        if cmd >= CALL {
            self.print_char(':' as i32);
            self.print_ln();
            self.token_show(chr);
        } else if cmd == TOP_BOT_MARK {
            self.print_char(':' as i32);
            self.print_ln();
            let m = self.cur_mark[chr as usize];
            self.token_show(m);
        }
    }
}
