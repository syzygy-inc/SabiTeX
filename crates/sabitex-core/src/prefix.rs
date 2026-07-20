//! Mode-independent assignments: `prefixed_command` and friends.
//!
//! Ports the M1 subset of tex.web Part 49 (§1208-§1298): prefixes, `\def`
//! family, `\let`/`\futurelet`, `\chardef` family, code assignments
//! (`\catcode` etc.), register assignment/arithmetic, `\message`,
//! `\lowercase`/`\uppercase` and `\show`/`\showthe`. Box/font/hyphenation
//! assignments arrive with their subsystems (M2+).

use crate::cmdchr::{
    SHOW_BOX_CODE, SHOW_GROUPS_CODE, SHOW_IFS_CODE, SHOW_LISTS_CODE, SHOW_THE_CODE,
};
use crate::cmds::*;
use crate::engine::Engine;
use crate::eqtb::NUMBER_USVS;
use crate::error::TexResult;
use crate::getnext::TOO_BIG_CHAR;
use crate::mem::NORMAL;
use crate::print::NEW_STRING;
use crate::scan::{DIMEN_VAL, GLUE_VAL, INT_VAL, MU_VAL};
use crate::tokens::*;
use crate::types::{Pointer, NULL};

// §1222: shorthand_def codes.
pub const CHAR_DEF_CODE: i32 = 0;
pub const MATH_CHAR_DEF_CODE: i32 = 1;
pub const COUNT_DEF_CODE: i32 = 2;
pub const DIMEN_DEF_CODE: i32 = 3;
pub const SKIP_DEF_CODE: i32 = 4;
pub const MU_SKIP_DEF_CODE: i32 = 5;
pub const TOKS_DEF_CODE: i32 = 6;
// xetex.web §27671-27672.
pub const XETEX_MATH_CHAR_NUM_DEF_CODE: i32 = 8;
pub const XETEX_MATH_CHAR_DEF_CODE: i32 = 9;

impl Engine {
    /// `get_r_token` (§1215): gets the control sequence to be defined.
    pub fn get_r_token(&mut self) -> TexResult<()> {
        loop {
            loop {
                self.get_token()?;
                if self.cur_tok != SPACE_TOKEN {
                    break;
                }
            }
            if self.cur_cs == 0 || self.cur_cs > self.eqtb.lay.frozen_control_sequence {
                self.print_err("Missing control sequence inserted");
                self.help(&[
                    "Please don't say `\\def cs{...}', say `\\def\\cs{...}'.",
                    "I've inserted an inaccessible control sequence so that your",
                    "definition will be completed without mixing me up too badly.",
                    "You can recover graciously from this error, if you're",
                    "careful; see exercise 27.2 in The TeXbook.",
                ]);
                if self.cur_cs == 0 {
                    self.back_input()?;
                }
                self.cur_tok = CS_TOKEN_FLAG + self.eqtb.lay.frozen_protection;
                self.ins_error()?;
                continue; // restart
            }
            return Ok(());
        }
    }

    /// `define`/`word_define` (§1214).
    fn define(&mut self, global: bool, p: Pointer, t: u16, e: i32) -> TexResult<()> {
        if global {
            self.geq_define(p, t, e);
            Ok(())
        } else {
            self.eq_define(p, t, e)
        }
    }

    fn word_define(&mut self, global: bool, p: Pointer, w: i32) -> TexResult<()> {
        if global {
            self.geq_word_define(p, w);
            Ok(())
        } else {
            self.eq_word_define(p, w)
        }
    }

    /// `prefixed_command` (§1211-§1236, subset).
    pub fn prefixed_command(&mut self) -> TexResult<()> {
        let mut a: i32 = 0; // accumulated prefix codes
        while self.cur_cmd == PREFIX {
            if (a / self.cur_chr) % 2 == 0 {
                a += self.cur_chr;
            }
            self.get_next_nonblank_nonrelax_noncall()?;
            if self.cur_cmd <= MAX_NON_PREFIXED_COMMAND {
                // §1212: discard erroneous prefixes.
                self.print_err("You can't use a prefix with `");
                let (c, ch) = (self.cur_cmd, self.cur_chr);
                self.print_cmd_chr(c, ch);
                self.print_char('\'' as i32);
                self.help(&["I'll pretend you didn't say \\long or \\outer or \\global."]);
                self.back_error()?;
                return Ok(());
            }
        }
        // §1213 + etex.ch: split off \protected; discard \long and \outer
        // if irrelevant.
        let protected = a >= 8;
        if protected {
            a -= 8;
        }
        if self.cur_cmd != DEF && (a % 4 != 0 || protected) {
            self.print_err("You can't use `");
            self.print_esc_str("long");
            self.print_chars("' or `");
            self.print_esc_str("outer");
            if self.etex_ex() {
                self.print_chars("' or `");
                self.print_esc_str("protected");
            }
            self.print_chars("' with `");
            let (c, ch) = (self.cur_cmd, self.cur_chr);
            self.print_cmd_chr(c, ch);
            self.print_char('\'' as i32);
            if self.etex_ex() {
                self.help(&["I'll pretend you didn't say \\long or \\outer or \\protected here."]);
            } else {
                self.help(&["I'll pretend you didn't say \\long or \\outer here."]);
            }
            self.error()?;
        }
        // §1214: adjust for \globaldefs.
        let gd = self.eqtb.int_par(crate::eqtb::GLOBAL_DEFS_CODE);
        if gd != 0 {
            if gd < 0 {
                if a >= 4 {
                    a -= 4;
                }
            } else if a < 4 {
                a += 4;
            }
        }
        let mut global = a >= 4;
        match self.cur_cmd {
            XETEX_DEF_CODE => {
                // xetex.web XeTeX_def_code assignment: five extended code
                // tables share one command, told apart by chr.
                let chr = self.cur_chr;
                let lay = self.eqtb.lay.clone();
                if chr == lay.sf_code_base {
                    // \XeTeXcharclass c = <class>: high half of sf_code.
                    self.scan_char_num()?;
                    let p = chr + self.cur_val;
                    let n = self.eqtb.equiv(p) % 0x10000;
                    self.scan_optional_equals()?;
                    self.scan_char_class()?;
                    let v = self.cur_val;
                    self.define(global, p, DATA, v * 0x10000 + n)?;
                } else if chr == lay.math_code_base {
                    // \Umathcodenum c = <packed 32-bit code>.
                    self.scan_char_num()?;
                    let p = chr + self.cur_val;
                    self.scan_optional_equals()?;
                    self.scan_xetex_math_char_int()?;
                    let v = self.cur_val;
                    self.define(global, p, DATA, v)?;
                } else if chr == lay.math_code_base + 1 {
                    // \Umathcode c = <class> <fam> <usv>.
                    self.scan_char_num()?;
                    let p = chr - 1 + self.cur_val;
                    self.scan_optional_equals()?;
                    self.scan_math_class_int()?;
                    let mut n = crate::xemath::set_class_field(self.cur_val);
                    self.scan_math_fam_int()?;
                    n += crate::xemath::set_family_field(self.cur_val);
                    self.scan_char_num()?;
                    n += self.cur_val;
                    self.define(global, p, DATA, n)?;
                } else if chr == lay.del_code_base {
                    // \Udelcodenum c = <packed delcode>.
                    self.scan_char_num()?;
                    let p = chr + self.cur_val;
                    self.scan_optional_equals()?;
                    self.scan_int()?;
                    let v = self.cur_val;
                    self.word_define(global, p, v)?;
                } else {
                    // \Udelcode c = <fam> <usv>  (0x40000000 flags the
                    // extended form; the family sits at bit 21 here).
                    self.scan_char_num()?;
                    let p = chr - 1 + self.cur_val;
                    self.scan_optional_equals()?;
                    let mut n = 0x4000_0000;
                    self.scan_math_fam_int()?;
                    n += self.cur_val * 0x20_0000;
                    self.scan_char_num()?;
                    n += self.cur_val;
                    self.word_define(global, p, n)?;
                }
            }
            ASSIGN_INHIBIT_XSP => {
                // ptex-base.ch: \inhibitxspcode`c=v (0..3; empty slot has
                // equiv 0, so the code itself marks occupancy).
                self.scan_int()?;
                let c = self.cur_val;
                self.scan_optional_equals()?;
                self.scan_int()?;
                let v = self.cur_val;
                let j = self.get_inhibit_pos(c, true);
                let base = self.eqtb.lay.inhibit_xsp_code_base;
                if j == crate::kanji::NO_ENTRY {
                    self.print_err("INHIBIT table is full!!");
                    self.help(&["I'm skipping this control sequences."]);
                    self.error()?;
                } else if !(0..=3).contains(&v) {
                    self.print_err("Invalid code (");
                    self.print_int(v);
                    self.print_chars("), should be in the range 0..3");
                    self.help(&["I'm going to use 0 instead of that illegal code value."]);
                    self.error()?;
                    self.define(global, base + j, 0, c)?;
                } else {
                    self.define(global, base + j, v as u16, c)?;
                }
            }
            ASSIGN_KINSOKU => {
                // ptex-base.ch: prebreakpenalty`c=n / postbreakpenalty.
                let ty = self.cur_chr as u16;
                self.scan_int()?;
                let c = self.cur_val;
                self.scan_optional_equals()?;
                self.scan_int()?;
                let pen = self.cur_val;
                let j = self.get_kinsoku_pos(c, true);
                let base = self.eqtb.lay.kinsoku_base;
                let pbase = self.eqtb.lay.kinsoku_penalty_base;
                if j != crate::kanji::NO_ENTRY
                    && pen == 0
                    && (global || self.save.cur_level == crate::eqtb::LEVEL_ONE)
                {
                    // remove the entry from the KINSOKU table
                    self.define(global, base + j, crate::kanji::KINSOKU_UNUSED_CODE, 0)?;
                } else if j == crate::kanji::NO_ENTRY {
                    self.print_err("KINSOKU table is full!!");
                    self.help(&["I'm skipping this control sequences."]);
                    self.error()?;
                } else {
                    self.define(global, base + j, ty, c)?;
                    if global {
                        self.geq_word_define(pbase + j, pen);
                    } else {
                        self.eq_word_define(pbase + j, pen)?;
                    }
                }
            }
            SET_AUTO_SPACING => {
                // pTeX: chr 0/1 = noautospacing/autospacing,
                //       chr 2/3 = noautoxspacing/autoxspacing.
                let chr = self.cur_chr;
                let loc = if chr < 2 {
                    self.eqtb.lay.auto_spacing_loc
                } else {
                    self.eqtb.lay.auto_xspacing_loc
                };
                self.define(global, loc, DATA, chr % 2)?;
            }
            SET_FONT => {
                // §1217 (+ pTeX): a Japanese font selects the Japanese
                // current font, leaving the alphabetic one untouched.
                let chr = self.cur_chr;
                let loc = if self.fonts.dir[chr as usize] != 0 {
                    self.eqtb.lay.cur_jfont_loc
                } else {
                    self.eqtb.lay.cur_font_loc
                };
                self.define(global, loc, DATA, chr)?;
            }
            DEF => {
                // §1218.
                if self.cur_chr % 2 == 1 && !global && gd >= 0 {
                    a += 4;
                    global = true;
                }
                let e = self.cur_chr >= 2;
                self.get_r_token()?;
                let p = self.cur_cs;
                let _q = self.scan_toks(true, e)?;
                let dr = self.inp.def_ref;
                if protected {
                    // etex.ch §1218: mark the macro with protected_token.
                    let q = self.mem.get_avail()?;
                    self.mem.set_info(q, crate::tokens::PROTECTED_TOKEN);
                    let l = self.mem.link(dr);
                    self.mem.set_link(q, l);
                    self.mem.set_link(dr, q);
                }
                self.define(global, p, CALL + (a % 4) as u16, dr)?;
            }
            LET => {
                // §1221.
                let n = self.cur_chr;
                self.get_r_token()?;
                let p = self.cur_cs;
                if n == 0 {
                    loop {
                        self.get_token()?;
                        if self.cur_cmd != SPACER {
                            break;
                        }
                    }
                    if self.cur_tok == OTHER_TOKEN + '=' as i32 {
                        self.get_token()?;
                        if self.cur_cmd == SPACER {
                            self.get_token()?;
                        }
                    }
                } else {
                    // \futurelet: look ahead, then back up.
                    self.get_token()?;
                    let q = self.cur_tok;
                    self.get_token()?;
                    self.back_input()?;
                    self.cur_tok = q;
                    self.back_input()?;
                    // back_input doesn't affect cur_cmd, cur_chr
                }
                if self.cur_cmd >= CALL {
                    let ch = self.cur_chr;
                    self.add_token_ref(ch);
                } else if (self.cur_cmd == REGISTER || self.cur_cmd == TOKS_REGISTER)
                    && (self.cur_chr < self.mem.mem_bot
                        || self.cur_chr > self.mem.lo_mem_stat_max())
                {
                    // etex.ch §1221: the copy references a sparse element.
                    let ch = self.cur_chr;
                    self.add_sa_ref(ch);
                }
                let (c, ch) = (self.cur_cmd, self.cur_chr);
                self.define(global, p, c, ch)?;
            }
            SHORTHAND_DEF => {
                // §1224.
                let n = self.cur_chr;
                self.get_r_token()?;
                let p = self.cur_cs;
                self.define(global, p, RELAX, TOO_BIG_CHAR)?;
                self.scan_optional_equals()?;
                match n {
                    CHAR_DEF_CODE => {
                        self.scan_char_num()?;
                        let v = self.cur_val;
                        self.define(global, p, CHAR_GIVEN, v)?;
                    }
                    MATH_CHAR_DEF_CODE => {
                        self.scan_fifteen_bit_int()?;
                        let v = self.cur_val;
                        self.define(global, p, MATH_GIVEN, v)?;
                    }
                    XETEX_MATH_CHAR_NUM_DEF_CODE => {
                        // \Umathcharnumdef: the 32-bit code as-is.
                        self.scan_xetex_math_char_int()?;
                        let v = self.cur_val;
                        self.define(global, p, XETEX_MATH_GIVEN, v)?;
                    }
                    XETEX_MATH_CHAR_DEF_CODE => {
                        // \Umathchardef <cs> = <class> <fam> <usv>.
                        self.scan_math_class_int()?;
                        let mut v = crate::xemath::set_class_field(self.cur_val);
                        self.scan_math_fam_int()?;
                        v += crate::xemath::set_family_field(self.cur_val);
                        self.scan_char_num()?;
                        v += self.cur_val;
                        self.define(global, p, XETEX_MATH_GIVEN, v)?;
                    }
                    _ => {
                        self.scan_register_num()?;
                        let v = self.cur_val;
                        if v > 255 {
                            // etex.ch §1224: a sparse array element.
                            let mut j = (n - COUNT_DEF_CODE) as u8; // int..mu(..box)
                            if j > MU_VAL {
                                j = crate::scan::TOK_VAL;
                            }
                            self.find_sa_element(j, v, true)?;
                            let leaf = self.cur_ptr;
                            self.add_sa_ref(leaf);
                            let cmd = if j == crate::scan::TOK_VAL {
                                TOKS_REGISTER
                            } else {
                                REGISTER
                            };
                            self.define(global, p, cmd, leaf)?;
                        } else {
                            let lay = &self.eqtb.lay;
                            let (cmd, loc) = match n {
                                COUNT_DEF_CODE => (ASSIGN_INT, lay.count_base + v),
                                DIMEN_DEF_CODE => (ASSIGN_DIMEN, lay.scaled_base + v),
                                SKIP_DEF_CODE => (ASSIGN_GLUE, lay.skip_base + v),
                                MU_SKIP_DEF_CODE => (ASSIGN_MU_GLUE, lay.mu_skip_base + v),
                                _ => (ASSIGN_TOKS, lay.toks_base + v),
                            };
                            self.define(global, p, cmd, loc)?;
                        }
                    }
                }
            }
            TOKS_REGISTER | ASSIGN_TOKS => {
                // §1226-§1227 (+ etex.ch sparse arrays).
                let q = self.cur_cs;
                let mut e = false; // sparse array element?
                if self.cur_cmd == TOKS_REGISTER {
                    if self.cur_chr == self.mem.mem_bot {
                        self.scan_register_num()?;
                        if self.cur_val > 255 {
                            self.find_sa_element(crate::scan::TOK_VAL, self.cur_val, true)?;
                            self.cur_chr = self.cur_ptr;
                            e = true;
                        } else {
                            self.cur_chr = self.eqtb.lay.toks_base + self.cur_val;
                        }
                    } else {
                        e = true;
                    }
                }
                let p = self.cur_chr;
                self.scan_optional_equals()?;
                self.get_next_nonblank_nonrelax_noncall()?;
                if self.cur_cmd != LEFT_BRACE
                    && (self.cur_cmd == TOKS_REGISTER || self.cur_cmd == ASSIGN_TOKS)
                {
                    // §1227: rhs is a token parameter or register.
                    let q2 = if self.cur_cmd == TOKS_REGISTER {
                        if self.cur_chr == self.mem.mem_bot {
                            self.scan_register_num()?;
                            if self.cur_val < 256 {
                                self.eqtb.equiv(self.eqtb.lay.toks_base + self.cur_val)
                            } else {
                                self.find_sa_element(crate::scan::TOK_VAL, self.cur_val, false)?;
                                if self.cur_ptr == NULL {
                                    NULL
                                } else {
                                    self.sa_ptr(self.cur_ptr)
                                }
                            }
                        } else {
                            self.sa_ptr(self.cur_chr)
                        }
                    } else {
                        self.eqtb.equiv(self.cur_chr)
                    };
                    if q2 == NULL {
                        if e {
                            if global {
                                self.gsa_def(p, NULL)?;
                            } else {
                                self.sa_def(p, NULL)?;
                            }
                        } else {
                            self.define(global, p, UNDEFINED_CS, NULL)?;
                        }
                    } else {
                        self.add_token_ref(q2);
                        if e {
                            if global {
                                self.gsa_def(p, q2)?;
                            } else {
                                self.sa_def(p, q2)?;
                            }
                        } else {
                            self.define(global, p, CALL, q2)?;
                        }
                    }
                    return self.finish_assignment();
                }
                self.back_input()?;
                self.cur_cs = q;
                let q = self.scan_toks(false, false)?;
                let def_ref = self.inp.def_ref;
                if self.mem.link(def_ref) == NULL {
                    // empty list: revert to the default
                    if e {
                        if global {
                            self.gsa_def(p, NULL)?;
                        } else {
                            self.sa_def(p, NULL)?;
                        }
                    } else {
                        self.define(global, p, UNDEFINED_CS, NULL)?;
                    }
                    self.mem.free_avail(def_ref);
                } else {
                    if p == self.eqtb.lay.output_routine_loc && !e {
                        // enclose in curlies
                        let r = self.mem.get_avail()?;
                        self.mem.set_link(q, r);
                        self.mem.set_info(r, RIGHT_BRACE_TOKEN + '}' as i32);
                        let l = self.mem.get_avail()?;
                        self.mem.set_info(l, LEFT_BRACE_TOKEN + '{' as i32);
                        let body = self.mem.link(def_ref);
                        self.mem.set_link(l, body);
                        self.mem.set_link(def_ref, l);
                    }
                    if e {
                        if global {
                            self.gsa_def(p, def_ref)?;
                        } else {
                            self.sa_def(p, def_ref)?;
                        }
                    } else {
                        self.define(global, p, CALL, def_ref)?;
                    }
                }
            }
            ASSIGN_INT => {
                // §1228.
                let p = self.cur_chr;
                self.scan_optional_equals()?;
                self.scan_int()?;
                let v = self.cur_val;
                self.word_define(global, p, v)?;
            }
            ASSIGN_DIMEN => {
                let p = self.cur_chr;
                self.scan_optional_equals()?;
                self.scan_normal_dimen()?;
                let v = self.cur_val;
                self.word_define(global, p, v)?;
            }
            ASSIGN_GLUE | ASSIGN_MU_GLUE => {
                let p = self.cur_chr;
                let n = self.cur_cmd;
                self.scan_optional_equals()?;
                if n == ASSIGN_MU_GLUE {
                    self.scan_glue(MU_VAL)?;
                } else {
                    self.scan_glue(GLUE_VAL)?;
                }
                self.trap_zero_glue();
                let v = self.cur_val;
                self.define(global, p, GLUE_REF, v)?;
            }
            DEF_CODE => {
                // §1232-§1233.
                let chr = self.cur_chr;
                let lay = self.eqtb.lay.clone();
                let n: i32 = if chr == lay.cat_code_base {
                    i32::from(MAX_CHAR_CODE)
                } else if chr == lay.kcat_code_base {
                    crate::kanji::MODIFIER // latin_ucs..modifier
                } else if chr == lay.math_code_base {
                    0o100000
                } else if chr == lay.sf_code_base {
                    0o77777
                } else if chr == lay.del_code_base {
                    0o77777777
                } else {
                    0x10FFFF // lc/uc codes are character codes (XeTeX-wide)
                };
                let mut p = chr;
                self.scan_char_num()?;
                if chr == lay.kansuji_base && !(0..=9).contains(&self.cur_val) {
                    // ptex-base.ch: \kansujichar index must be a digit.
                    self.print_err("Invalid KANSUJI number (");
                    let v = self.cur_val;
                    self.print_int(v);
                    self.print_chars(")");
                    self.help(&["I'm skipping this control sequences."]);
                    self.error()?;
                    self.cur_val = 0;
                }
                if chr == lay.kcat_code_base {
                    // uptex-m.ch: \kcatcode indexes by Unicode block.
                    p += crate::kanji::kcatcodekey(self.cur_val);
                } else {
                    p += self.cur_val;
                }
                self.scan_optional_equals()?;
                self.scan_int()?;
                if (self.cur_val < 0 && p < self.eqtb.lay.del_code_base) || self.cur_val > n {
                    self.print_err("Invalid code (");
                    let v = self.cur_val;
                    self.print_int(v);
                    if p < self.eqtb.lay.del_code_base {
                        self.print_chars("), should be in the range 0..");
                    } else {
                        self.print_chars("), should be at most ");
                    }
                    self.print_int(n);
                    self.help(&["I'm going to use 0 instead of that illegal code value."]);
                    self.error()?;
                    self.cur_val = 0;
                }
                let v = self.cur_val;
                let lay = self.eqtb.lay.clone();
                if p >= lay.sf_code_base && p < lay.math_code_base {
                    // xetex.web §1232: \sfcode keeps the char class in
                    // the high half of the entry.
                    let cls = self.eqtb.equiv(p) / 0x10000;
                    self.define(global, p, DATA, cls * 0x10000 + v)?;
                } else if p >= lay.math_code_base && p < lay.math_code_base + NUMBER_USVS {
                    // Classic \mathcode values are stored in the
                    // extended representation ("8000 becomes active).
                    let x = crate::xemath::from_classic(v);
                    self.define(global, p, DATA, x)?;
                } else if p < lay.del_code_base {
                    self.define(global, p, DATA, v)?;
                } else {
                    self.word_define(global, p, v)?;
                }
            }
            DEF_FAMILY => {
                // §1234 (xetex: 256 families).
                let mut p = self.cur_chr;
                self.scan_math_fam_int()?;
                p += self.cur_val;
                self.scan_optional_equals()?;
                self.scan_font_ident()?;
                let v = self.cur_val;
                self.define(global, p, DATA, v)?;
            }
            REGISTER | ADVANCE | MULTIPLY | DIVIDE => {
                self.do_register_command(a)?;
            }
            SET_AUX => {
                // §1243 alter_aux.
                let c = self.cur_chr;
                if c != self.mode().abs() {
                    self.report_illegal_case()?;
                } else if c == crate::engine::VMODE {
                    self.scan_optional_equals()?;
                    self.scan_normal_dimen()?;
                    let v = self.cur_val;
                    self.set_prev_depth(v);
                } else {
                    self.scan_optional_equals()?;
                    self.scan_int()?;
                    if self.cur_val <= 0 || self.cur_val > 32767 {
                        self.print_err("Bad space factor");
                        self.help(&["I allow only values in the range 1..32767 here."]);
                        let v = self.cur_val;
                        self.int_error(v)?;
                    } else {
                        let v = self.cur_val;
                        self.set_space_factor(v);
                    }
                }
            }
            SET_PREV_GRAF => {
                // §1244 alter_prev_graf: find the enclosing vmode level.
                self.scan_optional_equals()?;
                self.scan_int()?;
                if self.cur_val < 0 {
                    self.print_err("Bad ");
                    self.print_esc_str("prevgraf");
                    self.help(&["I allow only nonnegative values here."]);
                    let v = self.cur_val;
                    self.int_error(v)?;
                } else {
                    self.nest.stack[self.nest.ptr] = self.nest.cur;
                    let mut p = self.nest.ptr;
                    while self.nest.stack[p].mode.abs() != crate::engine::VMODE {
                        p -= 1;
                    }
                    self.nest.stack[p].pg = self.cur_val;
                    self.nest.cur = self.nest.stack[self.nest.ptr];
                }
            }
            SET_PAGE_INT => {
                // §1246 (+ etex.ch: \interactionmode).
                let c = self.cur_chr;
                self.scan_optional_equals()?;
                self.scan_int()?;
                if c == 0 {
                    self.dead_cycles = self.cur_val;
                } else if c == 2 {
                    if !(0..=3).contains(&self.cur_val) {
                        self.print_err("Bad interaction mode");
                        self.help(&[
                            "Modes are 0=batch, 1=nonstop, 2=scroll, and",
                            "3=errorstop. Proceed, and I'll ignore this case.",
                        ]);
                        self.int_error(self.cur_val)?;
                    } else {
                        self.cur_chr = self.cur_val;
                        self.new_interaction();
                    }
                } else {
                    self.insert_penalties = self.cur_val;
                }
            }
            SET_BOX => {
                // §1241-§1242 (+ etex.ch): \setbox.
                self.scan_register_num()?;
                let ctx = if global {
                    crate::control::GLOBAL_BOX_FLAG + self.cur_val
                } else {
                    crate::control::BOX_FLAG + self.cur_val
                };
                self.scan_optional_equals()?;
                if self.set_box_allowed {
                    self.scan_box(ctx)?;
                } else {
                    self.print_err("Improper ");
                    self.print_esc_str("setbox");
                    self.help(&[
                        "Sorry, \\setbox is not allowed after \\halign in a display,",
                        "or between \\accent and an accented character.",
                    ]);
                    self.error()?;
                }
            }
            SET_BOX_DIMEN => {
                // §1247 alter_box_dimen.
                let c = self.cur_chr;
                self.scan_register_num()?;
                let n = self.cur_val;
                let b = self.fetch_box(n)?;
                self.scan_optional_equals()?;
                self.scan_normal_dimen()?;
                if b != NULL {
                    let v = self.cur_val;
                    self.mem.word_mut(b + c).set_sc(v);
                }
            }
            ASSIGN_FONT_DIMEN => {
                // §1253.
                self.find_font_dimen(true)?;
                let k = self.cur_val;
                self.scan_optional_equals()?;
                self.scan_normal_dimen()?;
                let v = self.cur_val;
                self.fonts.info[k as usize].set_sc(v);
            }
            ASSIGN_FONT_INT => {
                // §1254.
                let n = self.cur_chr;
                self.scan_font_ident()?;
                let f = self.cur_val;
                self.scan_optional_equals()?;
                self.scan_int()?;
                if n == 0 {
                    self.fonts.hyphen_char[f as usize] = self.cur_val;
                } else {
                    self.fonts.skew_char[f as usize] = self.cur_val;
                }
            }
            DEF_FONT => {
                self.new_font(a)?;
            }
            SET_PAGE_DIMEN => {
                // §1245 alter_page_so_far.
                let c = self.cur_chr;
                self.scan_optional_equals()?;
                self.scan_normal_dimen()?;
                self.page_so_far[c as usize] = self.cur_val;
            }
            SET_SHAPE => {
                // §1248 (+ etex.ch): \parshape and the penalties arrays.
                let q = self.cur_chr;
                self.scan_optional_equals()?;
                self.scan_int()?;
                let n = self.cur_val;
                let p = if n <= 0 {
                    NULL
                } else if q > self.eqtb.lay.par_shape_loc {
                    // etex.ch: a list of n penalty values.
                    let words = (self.cur_val / 2) + 1;
                    let p = self.mem.get_node(2 * words + 1)?;
                    self.mem.set_info(p, words);
                    self.mem.word_mut(p + 1).set_int(n); // number of penalties
                    for j in (p + 2)..=(p + n + 1) {
                        self.scan_int()?;
                        let v = self.cur_val;
                        self.mem.word_mut(j).set_int(v);
                    }
                    if n % 2 == 0 {
                        self.mem.word_mut(p + n + 2).set_int(0); // unused
                    }
                    p
                } else {
                    let p = self.mem.get_node(2 * n + 1)?;
                    self.mem.set_info(p, n);
                    for j in 1..=n {
                        self.scan_normal_dimen()?;
                        let v = self.cur_val;
                        self.mem.word_mut(p + 2 * j - 1).set_sc(v); // indentation
                        self.scan_normal_dimen()?;
                        let v = self.cur_val;
                        self.mem.word_mut(p + 2 * j).set_sc(v); // width
                    }
                    p
                };
                self.define(global, q, SHAPE_REF, p)?;
            }
            HYPH_DATA => {
                // §1252: store hyphenation data in the trie / exceptions.
                if self.cur_chr == 1 {
                    self.new_patterns()?;
                } else {
                    self.new_hyph_exceptions()?;
                }
            }
            READ_TO_CS => {
                // §1225: \read<number> to <control sequence>; etex.ch adds
                // \readline (chr 1).
                let j = self.cur_chr;
                self.scan_int()?;
                let n = self.cur_val;
                if !self.scan_keyword("to")? {
                    self.print_err("Missing `to' inserted");
                    self.help(&[
                        "You should have said `\\read<number> to \\cs'.",
                        "I'm going to look for the \\cs now.",
                    ]);
                    self.error()?;
                }
                self.get_r_token()?;
                let p = self.cur_cs;
                self.read_toks(n, p, j)?;
                let v = self.cur_val;
                self.define(global, p, CALL, v)?;
            }
            SET_INTERACTION => {
                self.new_interaction();
            }
            _ => {
                return self.confusion("prefix");
            }
        }
        self.finish_assignment()
    }

    /// `do_assignments` (§1270): used by `\accent` and `\aftergroup`-free
    /// constructions that allow assignments between two tokens.
    pub fn do_assignments(&mut self) -> TexResult<()> {
        loop {
            self.get_next_nonblank_nonrelax_noncall()?;
            if self.cur_cmd <= MAX_NON_PREFIXED_COMMAND {
                return Ok(());
            }
            self.set_box_allowed = false;
            self.prefixed_command()?;
            self.set_box_allowed = true;
        }
    }

    /// §1269: insert a token saved by `\afterassignment`, if any.
    fn finish_assignment(&mut self) -> TexResult<()> {
        if self.after_token != 0 {
            self.cur_tok = self.after_token;
            self.after_token = 0;
            self.back_input()?;
        }
        Ok(())
    }

    /// `trap_zero_glue` (§1229).
    fn trap_zero_glue(&mut self) {
        let q = self.cur_val;
        if self.mem.width(q) == 0 && self.mem.stretch(q) == 0 && self.mem.shrink(q) == 0 {
            let zg = self.mem.zero_glue();
            self.mem.add_glue_ref(zg);
            self.mem.delete_glue_ref(q);
            self.cur_val = zg;
        }
    }

    /// `do_register_command(a)` (§1236-§1241 + etex.ch).
    fn do_register_command(&mut self, a: i32) -> TexResult<()> {
        let q = self.cur_cmd;
        let global = a >= 4;
        // etex.ch: does l refer to a sparse array element?
        let mut e = false;
        // §1237: compute the register location l and its type p.
        let (l, p): (Pointer, u8);
        'found: {
            if q != REGISTER {
                self.get_x_token()?;
                if self.cur_cmd >= ASSIGN_INT && self.cur_cmd <= ASSIGN_MU_GLUE {
                    l = self.cur_chr;
                    p = (self.cur_cmd - ASSIGN_INT) as u8;
                    break 'found;
                }
                if self.cur_cmd != REGISTER {
                    self.print_err("You can't use `");
                    let (c, ch) = (self.cur_cmd, self.cur_chr);
                    self.print_cmd_chr(c, ch);
                    self.print_chars("' after ");
                    self.print_cmd_chr(q, 0);
                    self.help(&["I'm forgetting what you said and not changing anything."]);
                    self.error()?;
                    return Ok(());
                }
            }
            let chr = self.cur_chr;
            if chr < self.mem.mem_bot || chr > self.mem.lo_mem_stat_max() {
                // A shorthand reference to a sparse array element.
                l = chr;
                p = self.sa_type(l);
                e = true;
                break 'found;
            }
            let pp = (chr - self.mem.mem_bot) as u8;
            self.scan_register_num()?;
            if self.cur_val > 255 {
                self.find_sa_element(pp, self.cur_val, true)?;
                l = self.cur_ptr;
                e = true;
                p = pp;
                break 'found;
            }
            let lay = &self.eqtb.lay;
            l = match pp {
                INT_VAL => self.cur_val + lay.count_base,
                DIMEN_VAL => self.cur_val + lay.scaled_base,
                GLUE_VAL => self.cur_val + lay.skip_base,
                _ => self.cur_val + lay.mu_skip_base,
            };
            p = pp;
        }
        if q == REGISTER {
            self.scan_optional_equals()?;
        } else {
            let _ = self.scan_keyword("by")?; // optional `by`
        }
        self.arith.arith_error = false;
        // etex.ch: the old value, from eqtb or from the sparse element.
        let w: i32 = if p < GLUE_VAL {
            if e {
                self.sa_int(l)
            } else {
                self.eqtb.int(l)
            }
        } else {
            0
        };
        let s_old: Pointer = if p >= GLUE_VAL {
            if e {
                self.sa_ptr(l)
            } else {
                self.eqtb.equiv(l)
            }
        } else {
            NULL
        };
        if q < MULTIPLY {
            // §1238: result of register or advance.
            if p < GLUE_VAL {
                if p == INT_VAL {
                    self.scan_int()?;
                } else {
                    self.scan_normal_dimen()?;
                }
                if q == ADVANCE {
                    self.cur_val += w;
                }
            } else {
                self.scan_glue(p)?;
                if q == ADVANCE {
                    // §1239: compute the sum of two glue specs.
                    let cv = self.cur_val;
                    let qq = self.new_spec(cv)?;
                    let r = s_old;
                    self.mem.delete_glue_ref(cv);
                    let w = self.mem.width(qq) + self.mem.width(r);
                    self.mem.set_width(qq, w);
                    if self.mem.stretch(qq) == 0 {
                        self.mem.set_stretch_order(qq, NORMAL);
                    }
                    if self.mem.stretch_order(qq) == self.mem.stretch_order(r) {
                        let st = self.mem.stretch(qq) + self.mem.stretch(r);
                        self.mem.set_stretch(qq, st);
                    } else if self.mem.stretch_order(qq) < self.mem.stretch_order(r)
                        && self.mem.stretch(r) != 0
                    {
                        let (st, so) = (self.mem.stretch(r), self.mem.stretch_order(r));
                        self.mem.set_stretch(qq, st);
                        self.mem.set_stretch_order(qq, so);
                    }
                    if self.mem.shrink(qq) == 0 {
                        self.mem.set_shrink_order(qq, NORMAL);
                    }
                    if self.mem.shrink_order(qq) == self.mem.shrink_order(r) {
                        let sh = self.mem.shrink(qq) + self.mem.shrink(r);
                        self.mem.set_shrink(qq, sh);
                    } else if self.mem.shrink_order(qq) < self.mem.shrink_order(r)
                        && self.mem.shrink(r) != 0
                    {
                        let (sh, so) = (self.mem.shrink(r), self.mem.shrink_order(r));
                        self.mem.set_shrink(qq, sh);
                        self.mem.set_shrink_order(qq, so);
                    }
                    self.cur_val = qq;
                }
            }
        } else {
            // §1240: result of multiply or divide.
            self.scan_int()?;
            if p < GLUE_VAL {
                self.cur_val = if q == MULTIPLY {
                    if p == INT_VAL {
                        crate::arith::mult_integers(&mut self.arith, w, self.cur_val)
                    } else {
                        crate::arith::nx_plus_y(&mut self.arith, w, self.cur_val, 0)
                    }
                } else {
                    crate::arith::x_over_n(&mut self.arith, w, self.cur_val)
                };
            } else {
                let s = s_old;
                let r = self.new_spec(s)?;
                let n = self.cur_val;
                if q == MULTIPLY {
                    let w = crate::arith::nx_plus_y(&mut self.arith, self.mem.width(s), n, 0);
                    let st = crate::arith::nx_plus_y(&mut self.arith, self.mem.stretch(s), n, 0);
                    let sh = crate::arith::nx_plus_y(&mut self.arith, self.mem.shrink(s), n, 0);
                    self.mem.set_width(r, w);
                    self.mem.set_stretch(r, st);
                    self.mem.set_shrink(r, sh);
                } else {
                    let w = crate::arith::x_over_n(&mut self.arith, self.mem.width(s), n);
                    let st = crate::arith::x_over_n(&mut self.arith, self.mem.stretch(s), n);
                    let sh = crate::arith::x_over_n(&mut self.arith, self.mem.shrink(s), n);
                    self.mem.set_width(r, w);
                    self.mem.set_stretch(r, st);
                    self.mem.set_shrink(r, sh);
                }
                self.cur_val = r;
            }
        }
        if self.arith.arith_error {
            self.print_err("Arithmetic overflow");
            self.help(&[
                "I can't carry out that multiplication or division,",
                "since the result is out of range.",
            ]);
            if p >= GLUE_VAL {
                let cv = self.cur_val;
                self.mem.delete_glue_ref(cv);
            }
            self.error()?;
            return Ok(());
        }
        if p < GLUE_VAL {
            let v = self.cur_val;
            if e {
                if global {
                    self.gsa_w_def(l, v)?;
                } else {
                    self.sa_w_def(l, v)?;
                }
            } else {
                self.word_define(global, l, v)?;
            }
        } else {
            self.trap_zero_glue();
            let v = self.cur_val;
            if e {
                if global {
                    self.gsa_def(l, v)?;
                } else {
                    self.sa_def(l, v)?;
                }
            } else {
                self.define(global, l, GLUE_REF, v)?;
            }
        }
        Ok(())
    }

    /// `report_illegal_case` (§1050).
    pub fn report_illegal_case(&mut self) -> TexResult<()> {
        self.you_cant();
        self.help(&[
            "Sorry, but I'm not programmed to handle this case;",
            "I'll just pretend that you didn't ask for it.",
            "If you're in the wrong mode, you might be able to",
            "return to the right one by typing `I}' or `I$' or `I\\par'.",
        ]);
        self.error()
    }

    /// `new_font(a)` (§1257-§1260): the `\font` command.
    pub fn new_font(&mut self, a: i32) -> TexResult<()> {
        if self.job_name.is_none() {
            self.open_log_file()?;
        }
        self.get_r_token()?;
        let u = self.cur_cs;
        // §1257: the name for diagnostics.
        let t: crate::types::StrNumber = if u >= self.eqtb.lay.hash_base {
            self.eqtb.text(u)
        } else if u >= self.eqtb.lay.single_base {
            if u == self.eqtb.lay.null_cs {
                self.strings.intern("FONT")?
            } else {
                // single-character name: use that character's string if ASCII.
                let c = u - self.eqtb.lay.single_base;
                if (0..256).contains(&c) {
                    c
                } else {
                    self.strings.intern("FONT")?
                }
            }
        } else {
            // §1257: an active character — the name is "FONT" plus the char.
            let c = u - self.eqtb.lay.active_base;
            let old_setting = self.prn.selector;
            self.prn.selector = crate::print::NEW_STRING;
            self.print_chars("FONT");
            self.print(c);
            self.prn.selector = old_setting;
            self.strings.str_room(1)?;
            self.make_string()?
        };
        self.define(a >= 4, u, SET_FONT, crate::fonts::NULL_FONT)?;
        self.scan_optional_equals()?;
        self.scan_file_name()?;
        let nom = std::mem::take(&mut self.cur_name);
        // §1258: scan the font size specification.
        self.name_in_progress = true; // no \input during the size scan
        let s: i32 = if self.scan_keyword("at")? {
            self.scan_normal_dimen()?;
            let s = self.cur_val;
            if s <= 0 || s >= 0o1000000000 {
                self.print_err("Improper `at' size (");
                self.print_scaled(s);
                self.print_chars("pt), replaced by 10pt");
                self.help(&[
                    "I can only handle fonts at positive sizes that are",
                    "less than 2048pt, so I've changed what you said to 10pt.",
                ]);
                self.error()?;
                10 * crate::types::UNITY
            } else {
                s
            }
        } else if self.scan_keyword("scaled")? {
            self.scan_int()?;
            let n = self.cur_val;
            if n <= 0 || n > 32768 {
                self.print_err("Illegal magnification has been changed to 1000");
                self.help(&["The magnification ratio must be between 1 and 32768."]);
                self.int_error(n)?;
                -1000
            } else {
                -n
            }
        } else {
            -1000
        };
        self.name_in_progress = false;
        // §1260: if this font has already been loaded, reuse it.
        let mut f = crate::fonts::NULL_FONT;
        for g in 1..=self.fonts.font_ptr {
            if self.fonts.name[g as usize] == nom && self.fonts.area[g as usize].is_empty() {
                let matches = if s > 0 {
                    s == self.fonts.size[g as usize]
                } else {
                    self.fonts.size[g as usize]
                        == crate::arith::xn_over_d(
                            &mut self.arith,
                            self.fonts.dsize[g as usize],
                            -s,
                            1000,
                        )
                };
                if matches {
                    f = g;
                    break;
                }
            }
        }
        if f == crate::fonts::NULL_FONT {
            // XeTeX: a quoted name tries a native font first; an unquoted
            // name tries the TFM first and falls back to a native font
            // before reporting the error.
            let try_native_first = self.quoted_filename
                || self
                    .fs
                    .read_file(&format!("{nom}.tfm"), crate::io::FileKind::Tfm)
                    .is_none();
            if try_native_first {
                if let Some(g) = self.load_native_font(u, &nom, s)? {
                    f = g;
                }
            }
            if f == crate::fonts::NULL_FONT {
                f = self.read_font_info(u, &nom, "", s)?;
                if self.fonts.dir[f as usize] != 0 {
                    self.jfont_seen = true; // gates pTeX box annotations
                }
            }
        }
        // common_ending (§1257 + etex.ch: traced via eq_define).
        self.define(a >= 4, u, SET_FONT, f)?;
        let idb = self.eqtb.lay.font_id_base;
        *self.eqtb.word_mut(idb + f) = self.eqtb.word(u);
        self.eqtb.set_text(idb + f, t);
        Ok(())
    }

    /// `issue_message` (§1279-§1281): `\message` and `\errmessage`.
    pub fn issue_message(&mut self) -> TexResult<()> {
        let c = self.cur_chr;
        let _ = self.scan_toks(false, true)?;
        let old_setting = self.prn.selector;
        self.prn.selector = NEW_STRING;
        let dr = self.inp.def_ref;
        self.token_show(dr);
        self.prn.selector = old_setting;
        self.mem.flush_list(dr);
        self.strings.str_room(1)?;
        let s = self.strings.make_string()?;
        if c == 0 {
            // §1280: print string s on the terminal.
            if self.prn.term_offset + self.strings.length(s) > self.sizes.max_print_line - 2 {
                self.print_ln();
            } else if self.prn.term_offset > 0 || self.prn.file_offset > 0 {
                self.print_char(' ' as i32);
            }
            self.slow_print(s);
        } else {
            // §1283: print string s as an error message.
            self.print_err("");
            self.slow_print(s);
            let eh = self
                .eqtb
                .equiv(self.eqtb.lay.local_base + crate::eqtb::ERR_HELP_OFFSET);
            if eh != NULL {
                self.use_err_help = true;
            } else if self.long_help_seen {
                self.help(&["(That was another \\errmessage.)"]);
            } else {
                if self.interaction < crate::input::ERROR_STOP_MODE {
                    self.long_help_seen = true;
                }
                self.help(&[
                    "This error message was generated by an \\errmessage",
                    "command, so I can't give any explicit help.",
                    "Pretend that you're Hercule Poirot: Examine all clues,",
                    "and deduce the truth by order and method.",
                ]);
            }
            self.error()?;
            self.use_err_help = false;
        }
        self.strings.flush_string();
        Ok(())
    }

    /// `shift_case` (§1285-§1288): `\lowercase` and `\uppercase`.
    pub fn shift_case(&mut self) -> TexResult<()> {
        let b = self.cur_chr; // lc_code_base or uc_code_base
        let _ = self.scan_toks(false, false)?;
        let def_ref = self.inp.def_ref;
        let mut p = self.mem.link(def_ref);
        while p != NULL {
            // §1289: change the case of token p if appropriate. Character
            // tokens and active-character cs tokens both shift; the cmd is
            // unchanged.
            let t = self.mem.info(p);
            if t < CS_TOKEN_FLAG + self.eqtb.lay.single_base {
                let c = t % MAX_CHAR_VAL;
                let shifted = self.eqtb.equiv(b + c);
                if shifted != 0 {
                    self.mem.set_info(p, t - c + shifted);
                }
            }
            p = self.mem.link(p);
        }
        let body = self.mem.link(def_ref);
        self.back_list(body)?;
        self.mem.free_avail(def_ref); // omit reference count
        Ok(())
    }

    /// `show_whatever` (§1293-§1298, subset): `\show`, `\showthe`,
    /// `\showbox` (always void in M1), `\showlists` (stub).
    pub fn show_whatever(&mut self) -> TexResult<()> {
        let mut long_show = false; // did we go through §1298's completion?
        match self.cur_chr {
            SHOW_LISTS_CODE => {
                self.begin_diagnostic();
                self.show_activities();
                long_show = true;
            }
            SHOW_BOX_CODE => {
                // §1296 (+ etex.ch): show the current contents of a box.
                self.scan_register_num()?;
                self.begin_diagnostic();
                self.print_nl_chars("> \\box");
                let v = self.cur_val;
                self.print_int(v);
                self.print_char('=' as i32);
                let b = self.fetch_box(v)?;
                if b == NULL {
                    self.print_chars("void");
                } else {
                    self.show_box(b);
                }
                long_show = true;
            }
            SHOW_THE_CODE => {
                // §1297: show the current value of some parameter/register.
                let _ = self.the_toks()?;
                self.print_nl_chars("> ");
                let th = self.mem.temp_head();
                let l = self.mem.link(th);
                self.show_token_list(l, NULL, 10_000_000);
                self.mem.flush_list(l);
            }
            x if x == crate::toks::SHOW_TOKENS => {
                // etex.ch: \showtokens displays a balanced text.
                let p = self.the_toks()?;
                self.print_nl_chars("> ");
                let _ = p;
                let th = self.mem.temp_head();
                let l = self.mem.link(th);
                self.show_token_list(l, NULL, 10_000_000);
                self.mem.flush_list(l);
            }
            SHOW_GROUPS_CODE => {
                // etex.ch: \showgroups displays the grouping hierarchy.
                self.begin_diagnostic();
                self.show_save_groups();
                long_show = true;
            }
            SHOW_IFS_CODE => {
                // etex.ch: \showifs displays the active conditionals.
                self.begin_diagnostic();
                self.print_nl_chars("");
                self.print_ln();
                if self.cond_ptr == NULL {
                    self.print_nl_chars("### ");
                    self.print_chars("no active conditionals");
                } else {
                    let mut p = self.cond_ptr;
                    let mut n = 0;
                    while p != NULL {
                        n += 1;
                        p = self.mem.link(p);
                    }
                    let mut p = self.cond_ptr;
                    let mut t = self.cur_if;
                    let mut l = self.if_line;
                    let mut m = self.if_limit;
                    loop {
                        self.print_nl_chars("### level ");
                        self.print_int(n);
                        self.print_chars(": ");
                        self.print_cmd_chr(IF_TEST, i32::from(t));
                        if m == crate::cond::FI_CODE {
                            self.print_esc_str("else");
                        }
                        self.print_if_line(l);
                        n -= 1;
                        t = self.mem.subtype(p);
                        l = self.mem.word(p + 1).int();
                        m = self.mem.node_type(p) as u8;
                        p = self.mem.link(p);
                        if p == NULL {
                            break;
                        }
                    }
                }
                long_show = true;
            }
            _ => {
                // §1294: show the current meaning of a token.
                self.get_token()?;
                self.print_nl_chars("> ");
                if self.cur_cs != 0 {
                    let cs = self.cur_cs;
                    self.sprint_cs(cs);
                    self.print_char('=' as i32);
                }
                self.print_meaning();
            }
        }
        if long_show {
            // §1298: complete a potentially long \show command.
            self.end_diagnostic(true);
            self.print_err("OK");
            if self.prn.selector == crate::print::TERM_AND_LOG
                && self.eqtb.int_par(crate::eqtb::TRACING_ONLINE_CODE) <= 0
            {
                self.prn.selector = crate::print::TERM_ONLY;
                self.print_chars(" (see the transcript file)");
                self.prn.selector = crate::print::TERM_AND_LOG;
            }
        }
        // common_ending (§1293).
        if self.interaction < crate::input::ERROR_STOP_MODE {
            self.help(&[]);
            self.error_count -= 1;
        } else if self.eqtb.int_par(crate::eqtb::TRACING_ONLINE_CODE) > 0 {
            self.help(&[
                "This isn't an error message; I'm just \\showing something.",
                "Type `I\\show...' to show more (e.g., \\show\\cs,",
                "\\showthe\\count10, \\showbox255, \\showlists).",
            ]);
        } else {
            self.help(&[
                "This isn't an error message; I'm just \\showing something.",
                "Type `I\\show...' to show more (e.g., \\show\\cs,",
                "\\showthe\\count10, \\showbox255, \\showlists).",
                "And type `I\\tracingonline=1\\show...' to show boxes and",
                "lists on your terminal as well as in the transcript file.",
            ]);
        }
        self.error()
    }
}
