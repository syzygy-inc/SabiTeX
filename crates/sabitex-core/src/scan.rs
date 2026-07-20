//! Basic scanning subroutines.
//!
//! Ports tex.web Part 26 (§402-§465): `scan_left_brace`,
//! `scan_optional_equals`, `scan_keyword`, the restricted integer scanners,
//! `scan_something_internal`, `scan_int`, `scan_dimen` and `scan_glue`.
//!
//! Unicode adaptations (XeTeX conventions): `scan_char_num` allows
//! 0..0x10FFFF; alphabetic constants likewise. Registers stay 8-bit until
//! e-TeX (M6). `em`/`ex` use the null font (zero) until fonts arrive (M2).

use crate::arith;
use crate::cmds::*;
use crate::engine::Engine;
use crate::error::TexResult;
use crate::mem::NORMAL;
use crate::tokens::*;
use crate::types::{Scaled, NULL, UNITY};

// §410: levels of scan_something_internal (TeX82 numbering; etex.ch's
// last_node_type_code=3 lives in the *last_item chr* code space only and
// coexists with mu_val=3).
pub const INT_VAL: u8 = 0;
pub const DIMEN_VAL: u8 = 1;
pub const GLUE_VAL: u8 = 2;
pub const MU_VAL: u8 = 3;
pub const IDENT_VAL: u8 = 4;
pub const TOK_VAL: u8 = 5;

// §424 + etex.ch: last_item modifiers beyond glue_val.
pub const LAST_NODE_TYPE_CODE: i32 = GLUE_VAL as i32 + 1;
pub const INPUT_LINE_NO_CODE: i32 = GLUE_VAL as i32 + 2;
pub const BADNESS_CODE: i32 = INPUT_LINE_NO_CODE + 1;
/// First of e-TeX's codes for integers (`eTeX_int`).
pub const ETEX_INT: i32 = BADNESS_CODE + 1;
/// First of e-TeX's codes for dimensions (`eTeX_dim`).
pub const ETEX_DIM: i32 = ETEX_INT + 8;
/// First of e-TeX's codes for glue (`eTeX_glue`).
pub const ETEX_GLUE: i32 = ETEX_DIM + 9;
/// First of e-TeX's codes for muglue (`eTeX_mu`).
pub const ETEX_MU: i32 = ETEX_GLUE + 1;
/// First of e-TeX's codes for expressions (`eTeX_expr`).
pub const ETEX_EXPR: i32 = ETEX_MU + 1;

// etex.ch: the e-TeX integer codes (eTeX_int + 0..7).
pub const ETEX_VERSION_CODE: i32 = ETEX_INT;
pub const CURRENT_GROUP_LEVEL_CODE: i32 = ETEX_INT + 1;
pub const CURRENT_GROUP_TYPE_CODE: i32 = ETEX_INT + 2;
pub const CURRENT_IF_LEVEL_CODE: i32 = ETEX_INT + 3;
pub const CURRENT_IF_TYPE_CODE: i32 = ETEX_INT + 4;
pub const CURRENT_IF_BRANCH_CODE: i32 = ETEX_INT + 5;
pub const GLUE_STRETCH_ORDER_CODE: i32 = ETEX_INT + 6;
pub const GLUE_SHRINK_ORDER_CODE: i32 = ETEX_INT + 7;
/// pdfTeX `\shellescape` (sabitex: always 0 — no shell access).
pub const SHELL_ESCAPE_CODE: i32 = ETEX_INT + 90;
/// XeTeX `\XeTeXversion` (sabitex asserts the XeTeX identity so that
/// LaTeX picks the xdvipdfmx backend).
pub const XETEX_VERSION_CODE: i32 = ETEX_INT + 91;
pub const PDF_LAST_X_POS_CODE: i32 = ETEX_INT + 92;
pub const PDF_LAST_Y_POS_CODE: i32 = ETEX_INT + 93;

// etex.ch: the e-TeX dimension codes (eTeX_dim + 0..8).
pub const FONT_CHAR_WD_CODE: i32 = ETEX_DIM;
pub const FONT_CHAR_HT_CODE: i32 = ETEX_DIM + 1;
pub const FONT_CHAR_DP_CODE: i32 = ETEX_DIM + 2;
pub const FONT_CHAR_IC_CODE: i32 = ETEX_DIM + 3;
pub const PAR_SHAPE_LENGTH_CODE: i32 = ETEX_DIM + 4;
pub const PAR_SHAPE_INDENT_CODE: i32 = ETEX_DIM + 5;
pub const PAR_SHAPE_DIMEN_CODE: i32 = ETEX_DIM + 6;
pub const GLUE_STRETCH_CODE: i32 = ETEX_DIM + 7;
pub const GLUE_SHRINK_CODE: i32 = ETEX_DIM + 8;

// etex.ch: glue/mu conversions and the expression codes.
pub const MU_TO_GLUE_CODE: i32 = ETEX_GLUE;
pub const GLUE_TO_MU_CODE: i32 = ETEX_MU;
pub const NUMEXPR_CODE: i32 = ETEX_EXPR + INT_VAL as i32;
pub const DIMEXPR_CODE: i32 = ETEX_EXPR + DIMEN_VAL as i32;
pub const GLUEEXPR_CODE: i32 = ETEX_EXPR + GLUE_VAL as i32;
pub const MUEXPR_CODE: i32 = ETEX_EXPR + MU_VAL as i32;

/// `max_dimen` (§421): 2^30 - 1.
pub const MAX_DIMEN: Scaled = 0o7777777777;

/// `infinity` (§445): the largest positive value TeX knows.
pub const INFINITY: i32 = 0o17777777777;

impl Engine {
    /// §406: get the next non-blank non-call token.
    pub fn get_next_nonblank_noncall(&mut self) -> TexResult<()> {
        loop {
            self.get_x_token()?;
            if self.cur_cmd != SPACER {
                return Ok(());
            }
        }
    }

    /// `get_x_or_protected` (etex.ch): like get_x_token, but protected
    /// macros are not expanded. Used by align_peek and fin_col so that
    /// alignment material can be produced by protected macros.
    pub fn get_x_or_protected(&mut self) -> TexResult<()> {
        use crate::cmds::{CALL, END_TEMPLATE, MAX_COMMAND};
        use crate::tokens::PROTECTED_TOKEN;
        loop {
            self.get_token()?;
            if self.cur_cmd <= MAX_COMMAND {
                return Ok(());
            }
            if (CALL..END_TEMPLATE).contains(&self.cur_cmd) {
                let r = self.mem.link(self.cur_chr);
                if self.mem.info(r) == PROTECTED_TOKEN {
                    return Ok(());
                }
            }
            self.expand()?;
        }
    }

    /// §404: get the next non-blank non-relax non-call token.
    pub fn get_next_nonblank_nonrelax_noncall(&mut self) -> TexResult<()> {
        loop {
            self.get_x_token()?;
            if self.cur_cmd != SPACER && self.cur_cmd != RELAX {
                return Ok(());
            }
        }
    }

    /// `scan_left_brace` (§403): reads a mandatory left brace.
    pub fn scan_left_brace(&mut self) -> TexResult<()> {
        self.get_next_nonblank_nonrelax_noncall()?;
        if self.cur_cmd != LEFT_BRACE {
            self.print_err("Missing { inserted");
            self.help(&[
                "A left brace was mandatory here, so I've put one in.",
                "You might want to delete and/or insert some corrections",
                "so that I will find a matching right brace soon.",
                "(If you're confused by all this, try typing `I}' now.)",
            ]);
            self.back_error()?;
            self.cur_tok = LEFT_BRACE_TOKEN + '{' as i32;
            self.cur_cmd = LEFT_BRACE;
            self.cur_chr = '{' as i32;
            self.inp.align_state += 1;
        }
        Ok(())
    }

    /// `scan_optional_equals` (§405).
    pub fn scan_optional_equals(&mut self) -> TexResult<()> {
        self.get_next_nonblank_noncall()?;
        if self.cur_tok != OTHER_TOKEN + '=' as i32 {
            self.back_input()?;
        }
        Ok(())
    }

    /// `scan_keyword(s)` (§407): looks for the given (lowercase) keyword,
    /// case-insensitively for ASCII letters.
    pub fn scan_keyword(&mut self, s: &str) -> TexResult<bool> {
        let bh = self.mem.backup_head();
        let mut p = bh; // tail of the backup list
        self.mem.set_link(p, NULL);
        let chars: Vec<i32> = s.chars().map(|c| c as i32).collect();
        let mut k = 0usize; // §407: k advances only on a match
        while k < chars.len() {
            self.get_x_token()?; // recursion is possible here
            if self.cur_cs == 0 && (self.cur_chr == chars[k] || self.cur_chr == chars[k] - 0x20) {
                let q = self.mem.get_avail()?;
                self.mem.set_link(p, q);
                let t = self.cur_tok;
                self.mem.set_info(q, t);
                p = q;
                k += 1;
            } else if self.cur_cmd != SPACER || p != bh {
                self.back_input()?;
                if p != bh {
                    let l = self.mem.link(bh);
                    self.back_list(l)?;
                }
                return Ok(false);
            }
        }
        let l = self.mem.link(bh);
        self.mem.flush_list(l);
        Ok(true)
    }

    /// `mu_error` (§408).
    pub fn mu_error(&mut self) -> TexResult<()> {
        self.print_err("Incompatible glue units");
        self.help(&["I'm going to assume that 1mu=1pt when they're mixed."]);
        self.error()
    }

    /// `scan_eight_bit_int` (§433).
    pub fn scan_eight_bit_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=255).contains(&self.cur_val) {
            self.print_err("Bad register code");
            self.help(&[
                "A register number must be between 0 and 255.",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_register_num` (etex.ch): a register number, up to 255 in
    /// compatibility mode and 32767 in extended mode.
    pub fn scan_register_num(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if self.cur_val < 0 || self.cur_val > self.max_reg_num {
            self.print_err("Bad register code");
            if self.max_reg_num > 255 {
                self.help(&[
                    "A register number must be between 0 and 32767.",
                    "I changed this one to zero.",
                ]);
            } else {
                self.help(&[
                    "A register number must be between 0 and 255.",
                    "I changed this one to zero.",
                ]);
            }
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_char_num` (§434), Unicode-wide.
    pub fn scan_char_num(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=0x10FFFF).contains(&self.cur_val) {
            self.print_err("Bad character code");
            self.help(&[
                "A character number must be between 0 and 255.",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_four_bit_int` (§435).
    pub fn scan_four_bit_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=15).contains(&self.cur_val) {
            self.print_err("Bad number");
            self.help(&[
                "Since I expected to read a number between 0 and 15,",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_fifteen_bit_int` (§436).
    pub fn scan_fifteen_bit_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=0o77777).contains(&self.cur_val) {
            self.print_err("Bad mathchar");
            self.help(&[
                "A mathchar number must be between 0 and 32767.",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_twenty_seven_bit_int` (§437).
    pub fn scan_twenty_seven_bit_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=0o777777777).contains(&self.cur_val) {
            self.print_err("Bad delimiter code");
            self.help(&[
                "A numeric delimiter code must be between 0 and 2^{27}-1.",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_xetex_math_char_int` (xetex.web §10322): a packed 32-bit
    /// math code; the character field must be a scalar value unless the
    /// whole code is the active marker.
    pub fn scan_xetex_math_char_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if crate::xemath::is_active_math_char(self.cur_val) {
            if self.cur_val != crate::xemath::ACTIVE_MATH_CHAR {
                self.print_err("Bad active XeTeX math code");
                self.help(&[
                    "Since I ignore class and family for active math chars,",
                    "I changed this one to \"1FFFFF.",
                ]);
                let v = self.cur_val;
                self.int_error(v)?;
                self.cur_val = crate::xemath::ACTIVE_MATH_CHAR;
            }
        } else if crate::xemath::math_char_field(self.cur_val) > 0x10FFFF {
            self.print_err("Bad XeTeX math character code");
            self.help(&[
                "Since I expected a character number between 0 and \"10FFFF,",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_math_class_int` (xetex.web §10338).
    pub fn scan_math_class_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=7).contains(&self.cur_val) {
            self.print_err("Bad math class");
            self.help(&[
                "Since I expected to read a number between 0 and 7,",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_math_fam_int` (xetex.web §10348).
    pub fn scan_math_fam_int(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=255).contains(&self.cur_val) {
            self.print_err("Bad math family");
            self.help(&[
                "Since I expected to read a number between 0 and 255,",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_char_class` (xetex.web): 0..4096 for `\XeTeXcharclass`.
    pub fn scan_char_class(&mut self) -> TexResult<()> {
        self.scan_int()?;
        if !(0..=crate::xemath::CHAR_CLASS_LIMIT).contains(&self.cur_val) {
            self.print_err("Bad character class");
            self.help(&[
                "A character class must be between 0 and 4096.",
                "I changed this one to zero.",
            ]);
            let v = self.cur_val;
            self.int_error(v)?;
            self.cur_val = 0;
        }
        Ok(())
    }

    /// `scan_something_internal(level, negative)` (§413-§424): fetches an
    /// internal numeric quantity into `cur_val`/`cur_val_level`.
    pub fn scan_something_internal(&mut self, level: u8, negative: bool) -> TexResult<()> {
        let m = self.cur_chr;
        match self.cur_cmd {
            DEF_CODE => {
                // §414 / xetex.web §9833: fetch a character code.
                self.scan_char_num()?;
                if m == self.eqtb.lay.math_code_base {
                    // \the\mathcode must fit the classic 15-bit form
                    // (xetex.web §9838).
                    let x = self.eqtb.math_code(self.cur_val);
                    match crate::xemath::to_classic(x) {
                        Some(v) => self.cur_val = v,
                        None => {
                            self.print_err("Extended mathchar used as mathchar");
                            self.help(&[
                                "A mathchar number must be between 0 and \"7FFF.",
                                "I changed this one to zero.",
                            ]);
                            self.int_error(x)?;
                            self.cur_val = 0;
                        }
                    }
                } else if m == self.eqtb.lay.sf_code_base {
                    // xetex: \sfcode reads the low half (the XeTeX char
                    // class lives in the high half).
                    let v = self.eqtb.equiv(m + self.cur_val);
                    self.cur_val = v % 0x10000;
                } else if m < self.eqtb.lay.math_code_base {
                    self.cur_val = self.eqtb.equiv(m + self.cur_val);
                } else {
                    self.cur_val = self.eqtb.int(m + self.cur_val);
                }
                self.cur_val_level = INT_VAL;
            }
            XETEX_DEF_CODE => {
                // xetex.web §9784: the extended code tables.
                self.scan_char_num()?;
                let lay = self.eqtb.lay.clone();
                if m == lay.sf_code_base {
                    let v = self.eqtb.equiv(m + self.cur_val);
                    self.cur_val = v / 0x10000; // \XeTeXcharclass
                } else if m == lay.math_code_base {
                    self.cur_val = self.eqtb.math_code(self.cur_val);
                } else if m == lay.math_code_base + 1 {
                    self.print_err("Can't use \\Umathcode as a number (try \\Umathcodenum)");
                    self.help(&[
                        "\\Umathcode is for setting a mathcode from separate values;",
                        "use \\Umathcodenum to access them as single values.",
                    ]);
                    self.error()?;
                    self.cur_val = 0;
                } else if m == lay.del_code_base {
                    self.cur_val = self.eqtb.del_code(self.cur_val);
                } else {
                    self.print_err("Can't use \\Udelcode as a number (try \\Udelcodenum)");
                    self.help(&[
                        "\\Udelcode is for setting a delcode from separate values;",
                        "use \\Udelcodenum to access them as single values.",
                    ]);
                    self.error()?;
                    self.cur_val = 0;
                }
                self.cur_val_level = INT_VAL;
            }
            TOKS_REGISTER | ASSIGN_TOKS | DEF_FAMILY | SET_FONT | DEF_FONT => {
                // §415: fetch a token list or font identifier.
                if level != TOK_VAL {
                    self.print_err("Missing number, treated as zero");
                    self.help(&[
                        "A number should have been here; I inserted `0'.",
                        "(If you can't figure out why I needed to see a number,",
                        "look up `weird error' in the index to The TeXbook.)",
                    ]);
                    self.back_error()?;
                    self.cur_val = 0;
                    self.cur_val_level = DIMEN_VAL;
                } else if self.cur_cmd <= ASSIGN_TOKS {
                    // §415 (+ etex.ch sparse arrays).
                    if self.cur_cmd < ASSIGN_TOKS {
                        // cur_cmd = toks_register
                        if m == self.mem.mem_bot {
                            self.scan_register_num()?;
                            if self.cur_val < 256 {
                                let loc = self.eqtb.lay.toks_base + self.cur_val;
                                self.cur_val = self.eqtb.equiv(loc);
                            } else {
                                self.find_sa_element(TOK_VAL, self.cur_val, false)?;
                                self.cur_val = if self.cur_ptr == NULL {
                                    NULL
                                } else {
                                    self.sa_ptr(self.cur_ptr)
                                };
                            }
                        } else {
                            self.cur_val = self.sa_ptr(m);
                        }
                    } else {
                        self.cur_val = self.eqtb.equiv(m);
                    }
                    self.cur_val_level = TOK_VAL;
                } else {
                    // font identifiers: TODO(M2) scan_font_ident; only the
                    // null font exists so far.
                    self.back_input()?;
                    self.scan_font_ident()?;
                    self.cur_val += self.eqtb.lay.font_id_base;
                    self.cur_val_level = IDENT_VAL;
                }
            }
            ASSIGN_KINSOKU => {
                // ptex-base.ch: fetch the kinsoku penalty of a character
                // (0 when unset or set with the other pre/post type).
                let ty = self.cur_chr as u16;
                self.scan_char_num()?;
                let c = self.cur_val;
                let kp = self.get_kinsoku_pos(c, false);
                self.cur_val = if kp != crate::kanji::NO_ENTRY
                    && self.eqtb.eq_type(self.eqtb.lay.kinsoku_base + kp) == ty
                {
                    self.kinsoku_penalty(kp)
                } else {
                    0
                };
                self.cur_val_level = INT_VAL;
            }
            ASSIGN_INHIBIT_XSP => {
                // ptex-base.ch: fetch \inhibitxspcode of a character.
                self.scan_char_num()?;
                let c = self.cur_val;
                self.cur_val = self.inhibit_xsp_code_of(c);
                self.cur_val_level = INT_VAL;
            }
            ASSIGN_INT => {
                self.cur_val = self.eqtb.int(m);
                self.cur_val_level = INT_VAL;
            }
            ASSIGN_DIMEN => {
                self.cur_val = self.eqtb.int(m);
                self.cur_val_level = DIMEN_VAL;
            }
            ASSIGN_GLUE => {
                self.cur_val = self.eqtb.equiv(m);
                self.cur_val_level = GLUE_VAL;
            }
            ASSIGN_MU_GLUE => {
                self.cur_val = self.eqtb.equiv(m);
                self.cur_val_level = MU_VAL;
            }
            SET_AUX => {
                // §418: \spacefactor / \prevdepth.
                if self.mode().abs() != m {
                    self.print_err("Improper ");
                    self.print_cmd_chr(SET_AUX, m);
                    self.help(&[
                        "You can refer to \\spacefactor only in horizontal mode;",
                        "you can refer to \\prevdepth only in vertical mode; and",
                        "neither of these is meaningful inside \\write. So",
                        "I'm forgetting what you said and using zero instead.",
                    ]);
                    self.error()?;
                    if level != TOK_VAL {
                        self.cur_val = 0;
                        self.cur_val_level = DIMEN_VAL;
                    } else {
                        self.cur_val = 0;
                        self.cur_val_level = INT_VAL;
                    }
                } else if m == crate::engine::VMODE {
                    self.cur_val = self.prev_depth();
                    self.cur_val_level = DIMEN_VAL;
                } else {
                    self.cur_val = self.space_factor();
                    self.cur_val_level = INT_VAL;
                }
            }
            SET_PREV_GRAF => {
                // §422: prev_graf of the enclosing vmode level.
                if self.mode() == 0 {
                    self.cur_val = 0; // prev_graf = 0 within \write
                } else {
                    self.nest.stack[self.nest.ptr] = self.nest.cur;
                    let mut p = self.nest.ptr;
                    while self.nest.stack[p].mode.abs() != crate::engine::VMODE {
                        p -= 1;
                    }
                    self.cur_val = self.nest.stack[p].pg;
                }
                self.cur_val_level = INT_VAL;
            }
            SET_PAGE_INT => {
                // §419 (+ etex.ch: \interactionmode).
                self.cur_val = if m == 0 {
                    self.dead_cycles
                } else if m == 2 {
                    i32::from(self.interaction)
                } else {
                    self.insert_penalties
                };
                self.cur_val_level = INT_VAL;
            }
            SET_PAGE_DIMEN => {
                // §421: page totals.
                if self.page_contents == crate::page::EMPTY && !self.output_active {
                    self.cur_val = if m == 0 { MAX_DIMEN } else { 0 };
                } else {
                    self.cur_val = self.page_so_far[m as usize];
                }
                self.cur_val_level = DIMEN_VAL;
            }
            SET_SHAPE => {
                // §423 (+ etex.ch: fetch a penalties array element).
                if m > self.eqtb.lay.par_shape_loc {
                    self.scan_int()?;
                    let p = self.eqtb.equiv(m);
                    if p == NULL || self.cur_val < 0 {
                        self.cur_val = 0;
                    } else {
                        // penalty(p) == mem[p+1].int holds the count.
                        if self.cur_val > self.mem.word(p + 1).int() {
                            self.cur_val = self.mem.word(p + 1).int();
                        }
                        let v = self.cur_val;
                        self.cur_val = self.mem.word(p + 1 + v).int();
                    }
                } else {
                    let ps = self.eqtb.equiv(self.eqtb.lay.par_shape_loc);
                    self.cur_val = if ps == NULL { 0 } else { self.mem.info(ps) };
                }
                self.cur_val_level = INT_VAL;
            }
            SET_BOX_DIMEN => {
                // §420 (+ etex.ch).
                self.scan_register_num()?;
                let n = self.cur_val;
                let b = self.fetch_box(n)?;
                self.cur_val = if b == NULL {
                    0
                } else {
                    self.mem.word(b + m).sc()
                };
                self.cur_val_level = DIMEN_VAL;
            }
            CHAR_GIVEN | MATH_GIVEN | XETEX_MATH_GIVEN => {
                self.cur_val = self.cur_chr;
                self.cur_val_level = INT_VAL;
            }
            ASSIGN_FONT_DIMEN => {
                // §425: \fontdimen.
                self.find_font_dimen(false)?;
                let fp = self.fonts.fmem_ptr as usize;
                self.fonts.info[fp].set_sc(0);
                self.cur_val = self.fonts.info[self.cur_val as usize].sc();
                self.cur_val_level = DIMEN_VAL;
            }
            ASSIGN_FONT_INT => {
                // §426: \hyphenchar / \skewchar.
                self.scan_font_ident()?;
                let f = self.cur_val as usize;
                self.cur_val = if m == 0 {
                    self.fonts.hyphen_char[f]
                } else {
                    self.fonts.skew_char[f]
                };
                self.cur_val_level = INT_VAL;
            }
            REGISTER => {
                // §427 (+ etex.ch): fetch a register, possibly sparse.
                if m < self.mem.mem_bot || m > self.mem.lo_mem_stat_max() {
                    // A shorthand reference to a sparse array element.
                    self.cur_val_level = self.sa_type(m);
                    self.cur_val = if self.cur_val_level < GLUE_VAL {
                        self.sa_int(m)
                    } else {
                        self.sa_ptr(m)
                    };
                } else {
                    let t = (m - self.mem.mem_bot) as u8;
                    self.scan_register_num()?;
                    self.cur_val_level = t;
                    let n = self.cur_val;
                    if n > 255 {
                        self.find_sa_element(t, n, false)?;
                        self.cur_val = if self.cur_ptr == NULL {
                            if t < GLUE_VAL {
                                0
                            } else {
                                self.mem.zero_glue()
                            }
                        } else if t < GLUE_VAL {
                            self.sa_int(self.cur_ptr)
                        } else {
                            self.sa_ptr(self.cur_ptr)
                        };
                    } else {
                        self.cur_val = match t {
                            INT_VAL => self.eqtb.count(n),
                            DIMEN_VAL => self.eqtb.dimen(n),
                            GLUE_VAL => self.eqtb.equiv(self.eqtb.lay.skip_base + n),
                            _ => self.eqtb.equiv(self.eqtb.lay.mu_skip_base + n),
                        };
                    }
                }
            }
            LAST_ITEM => {
                // §424 + etex.ch: \lastpenalty, \lastkern, \lastskip,
                // \lastnodetype, \inputlineno, \badness, and the e-TeX
                // integer/dimension/expression fetches.
                let m = self.cur_chr;
                if m == PDF_LAST_X_POS_CODE || m == PDF_LAST_Y_POS_CODE {
                    self.cur_val = if m == PDF_LAST_X_POS_CODE {
                        self.last_x_pos
                    } else {
                        self.last_y_pos
                    };
                    self.cur_val_level = INT_VAL;
                    return Ok(());
                }
                if m == SHELL_ESCAPE_CODE || m == XETEX_VERSION_CODE {
                    // \shellescape: no shell — 0. \XeTeXversion: 0.
                    self.cur_val = 0;
                    self.cur_val_level = INT_VAL;
                } else if m >= INPUT_LINE_NO_CODE {
                    if m >= ETEX_GLUE {
                        // etex.ch: process an expression and return —
                        // this path manages glue reference counts itself.
                        if m < ETEX_MU {
                            // mu_to_glue_code
                            self.scan_mu_glue()?;
                            self.cur_val_level = GLUE_VAL;
                        } else if m < ETEX_EXPR {
                            // glue_to_mu_code
                            self.scan_normal_glue()?;
                            self.cur_val_level = MU_VAL;
                        } else {
                            self.cur_val_level = (m - ETEX_EXPR) as u8;
                            self.scan_expr()?;
                        }
                        while self.cur_val_level > level {
                            if self.cur_val_level == GLUE_VAL {
                                let q = self.cur_val;
                                self.cur_val = self.mem.width(q);
                                self.mem.delete_glue_ref(q);
                            } else if self.cur_val_level == MU_VAL {
                                self.mu_error()?;
                            }
                            self.cur_val_level -= 1;
                        }
                        if negative {
                            if self.cur_val_level >= GLUE_VAL {
                                let q = self.cur_val;
                                self.cur_val = self.new_spec(q)?;
                                self.mem.delete_glue_ref(q);
                                let v = self.cur_val;
                                let (w, st, sh) =
                                    (self.mem.width(v), self.mem.stretch(v), self.mem.shrink(v));
                                self.mem.set_width(v, -w);
                                self.mem.set_stretch(v, -st);
                                self.mem.set_shrink(v, -sh);
                            } else {
                                self.cur_val = -self.cur_val;
                            }
                        }
                        return Ok(());
                    } else if m >= ETEX_DIM {
                        match m {
                            FONT_CHAR_WD_CODE | FONT_CHAR_HT_CODE | FONT_CHAR_DP_CODE
                            | FONT_CHAR_IC_CODE => {
                                // etex.ch: character metrics queries.
                                self.scan_font_ident()?;
                                let f = self.cur_val;
                                self.scan_char_num()?;
                                if self.fonts.bc[f as usize] <= self.cur_val
                                    && self.fonts.ec[f as usize] >= self.cur_val
                                {
                                    let i = self.fonts.char_info(f, self.cur_val);
                                    let hd = crate::fonts::FontMem::height_depth(i);
                                    self.cur_val = match m {
                                        FONT_CHAR_WD_CODE => self.fonts.char_width(f, i),
                                        FONT_CHAR_HT_CODE => self.fonts.char_height(f, hd),
                                        FONT_CHAR_DP_CODE => self.fonts.char_depth(f, hd),
                                        _ => self.fonts.char_italic(f, i),
                                    };
                                } else {
                                    self.cur_val = 0;
                                }
                            }
                            PAR_SHAPE_LENGTH_CODE
                            | PAR_SHAPE_INDENT_CODE
                            | PAR_SHAPE_DIMEN_CODE => {
                                // etex.ch: \parshape queries.
                                let mut q = m - PAR_SHAPE_LENGTH_CODE;
                                self.scan_int()?;
                                let ps = self.eqtb.equiv(self.eqtb.lay.par_shape_loc);
                                if ps == NULL || self.cur_val <= 0 {
                                    self.cur_val = 0;
                                } else {
                                    if q == 2 {
                                        q = self.cur_val % 2;
                                        self.cur_val = (self.cur_val + q) / 2;
                                    }
                                    if self.cur_val > self.mem.info(ps) {
                                        self.cur_val = self.mem.info(ps);
                                    }
                                    self.cur_val = self.mem.word(ps + 2 * self.cur_val - q).sc();
                                }
                            }
                            _ => {
                                // glue_stretch_code / glue_shrink_code
                                self.scan_normal_glue()?;
                                let q = self.cur_val;
                                self.cur_val = if m == GLUE_STRETCH_CODE {
                                    self.mem.stretch(q)
                                } else {
                                    self.mem.shrink(q)
                                };
                                self.mem.delete_glue_ref(q);
                            }
                        }
                        self.cur_val_level = DIMEN_VAL;
                    } else if m == GLUE_STRETCH_ORDER_CODE || m == GLUE_SHRINK_ORDER_CODE {
                        // etex.ch: order-of-infinity queries.
                        self.scan_normal_glue()?;
                        let q = self.cur_val;
                        self.cur_val = if m == GLUE_STRETCH_ORDER_CODE {
                            i32::from(self.mem.stretch_order(q))
                        } else {
                            i32::from(self.mem.shrink_order(q))
                        };
                        self.mem.delete_glue_ref(q);
                        self.cur_val_level = INT_VAL;
                    } else {
                        self.cur_val = match m {
                            INPUT_LINE_NO_CODE => self.inp.line,
                            BADNESS_CODE => self.last_badness,
                            ETEX_VERSION_CODE => crate::engine::ETEX_VERSION,
                            CURRENT_GROUP_LEVEL_CODE => {
                                i32::from(self.save.cur_level) - i32::from(crate::eqtb::LEVEL_ONE)
                            }
                            CURRENT_GROUP_TYPE_CODE => i32::from(self.save.cur_group),
                            CURRENT_IF_LEVEL_CODE => {
                                let mut q = self.cond_ptr;
                                let mut n = 0;
                                while q != NULL {
                                    n += 1;
                                    q = self.mem.link(q);
                                }
                                n
                            }
                            CURRENT_IF_TYPE_CODE => {
                                if self.cond_ptr == NULL {
                                    0
                                } else if i32::from(self.cur_if) < crate::cond::UNLESS_CODE {
                                    i32::from(self.cur_if) + 1
                                } else {
                                    -(i32::from(self.cur_if) - crate::cond::UNLESS_CODE + 1)
                                }
                            }
                            CURRENT_IF_BRANCH_CODE => {
                                if self.if_limit == crate::cond::OR_CODE
                                    || self.if_limit == crate::cond::ELSE_CODE
                                {
                                    1
                                } else if self.if_limit == crate::cond::FI_CODE {
                                    -1
                                } else {
                                    0
                                }
                            }
                            _ => 0,
                        };
                        self.cur_val_level = INT_VAL;
                    }
                } else {
                    self.cur_val = if m == i32::from(GLUE_VAL) {
                        self.mem.zero_glue()
                    } else {
                        0
                    };
                    // find_effective_tail (etex.ch): a final \endM math
                    // node is transparent to \lastpenalty and friends.
                    let head = self.nest.cur.head;
                    let mut tx = self.nest.cur.tail;
                    if !self.mem.is_char_node(tx)
                        && self.mem.node_type(tx) == crate::nodes::MATH_NODE
                        && self.mem.subtype(tx) == crate::nodes::END_M_CODE
                    {
                        let mut r = head;
                        let mut q = r;
                        while r != tx {
                            q = r;
                            r = self.mem.link(q);
                        }
                        tx = q;
                    }
                    if m == LAST_NODE_TYPE_CODE {
                        self.cur_val_level = INT_VAL;
                        if tx == head || self.mode() == 0 {
                            self.cur_val = -1;
                        }
                    } else {
                        self.cur_val_level = m as u8;
                    }
                    if !self.mem.is_char_node(tx) && self.mode() != 0 {
                        match m as u8 {
                            INT_VAL => {
                                if self.mem.node_type(tx) == crate::nodes::PENALTY_NODE {
                                    self.cur_val = self.mem.penalty(tx);
                                }
                            }
                            DIMEN_VAL => {
                                if self.mem.node_type(tx) == crate::nodes::KERN_NODE {
                                    self.cur_val = self.mem.width(tx);
                                }
                            }
                            GLUE_VAL => {
                                if self.mem.node_type(tx) == crate::nodes::GLUE_NODE {
                                    self.cur_val = self.mem.glue_ptr(tx);
                                    if self.mem.subtype(tx) == crate::nodes::MU_GLUE {
                                        self.cur_val_level = MU_VAL;
                                    }
                                }
                            }
                            _ => {
                                // last_node_type_code
                                let t = self.mem.node_type(tx);
                                self.cur_val = if t <= crate::nodes::UNSET_NODE {
                                    i32::from(t) + 1
                                } else {
                                    i32::from(crate::nodes::UNSET_NODE) + 2
                                };
                            }
                        }
                    } else if self.mode() == crate::engine::VMODE && tx == head {
                        match m as u8 {
                            INT_VAL => self.cur_val = self.last_penalty,
                            DIMEN_VAL => self.cur_val = self.last_kern,
                            GLUE_VAL => {
                                if self.last_glue != crate::types::MAX_HALFWORD {
                                    self.cur_val = self.last_glue;
                                }
                            }
                            _ => self.cur_val = self.last_node_type, // \lastnodetype
                        }
                    }
                }
            }
            _ => {
                // §428: complain that \the can't do this.
                self.print_err("You can't use `");
                let (c, ch) = (self.cur_cmd, self.cur_chr);
                self.print_cmd_chr(c, ch);
                self.print_chars("' after ");
                self.print_esc_str("the");
                self.help(&["I'm forgetting what you said and using zero instead."]);
                self.error()?;
                if level != TOK_VAL {
                    self.cur_val = 0;
                    self.cur_val_level = DIMEN_VAL;
                } else {
                    self.cur_val = 0;
                    self.cur_val_level = INT_VAL;
                }
            }
        }
        while self.cur_val_level > level {
            // §429: convert cur_val to a lower level.
            if self.cur_val_level == GLUE_VAL {
                self.cur_val = self.mem.width(self.cur_val);
            } else if self.cur_val_level == MU_VAL {
                self.mu_error()?;
            }
            self.cur_val_level -= 1;
        }
        // §430: fix the reference count and negate if needed.
        if negative {
            if self.cur_val_level >= GLUE_VAL {
                let v = self.cur_val;
                self.cur_val = self.new_spec(v)?;
                let q = self.cur_val;
                let (w, st, sh) = (self.mem.width(q), self.mem.stretch(q), self.mem.shrink(q));
                self.mem.set_width(q, -w);
                self.mem.set_stretch(q, -st);
                self.mem.set_shrink(q, -sh);
            } else {
                self.cur_val = -self.cur_val;
            }
        } else if self.cur_val_level >= GLUE_VAL && self.cur_val_level <= MU_VAL {
            let v = self.cur_val;
            self.mem.add_glue_ref(v);
        }
        Ok(())
    }

    /// `scan_font_ident` (§577).
    pub fn scan_font_ident(&mut self) -> TexResult<()> {
        self.get_next_nonblank_noncall()?;
        if self.cur_cmd == DEF_FONT {
            self.cur_val = self.eqtb.cur_font();
        } else if self.cur_cmd == SET_FONT {
            self.cur_val = self.cur_chr;
        } else if self.cur_cmd == DEF_FAMILY {
            let m = self.cur_chr;
            self.scan_four_bit_int()?;
            self.cur_val = self.eqtb.equiv(m + self.cur_val);
        } else {
            self.print_err("Missing font identifier");
            self.help(&[
                "I was looking for a control sequence whose",
                "current meaning has been defined by \\font.",
            ]);
            self.back_error()?;
            self.cur_val = 0; // null_font
        }
        Ok(())
    }

    /// §406 + §441: get the next non-blank non-sign token, setting
    /// `negative` appropriately.
    fn scan_signs(&mut self) -> TexResult<bool> {
        let mut negative = false;
        loop {
            self.get_next_nonblank_noncall()?;
            if self.cur_tok == OTHER_TOKEN + '-' as i32 {
                negative = !negative;
                self.cur_tok = OTHER_TOKEN + '+' as i32;
            }
            if self.cur_tok != OTHER_TOKEN + '+' as i32 {
                return Ok(negative);
            }
        }
    }

    /// `scan_int` (§440-§445): sets `cur_val` to an integer.
    pub fn scan_int(&mut self) -> TexResult<()> {
        self.radix = 0;
        let mut ok_so_far = true;
        let negative = self.scan_signs()?;
        if self.cur_tok == ALPHA_TOKEN {
            // §442: scan an alphabetic character code.
            self.get_token()?; // suppress macro expansion
            if self.cur_tok < CS_TOKEN_FLAG {
                self.cur_val = self.cur_chr;
                if self.cur_cmd <= RIGHT_BRACE {
                    if self.cur_cmd == RIGHT_BRACE {
                        self.inp.align_state += 1;
                    } else {
                        self.inp.align_state -= 1;
                    }
                }
            } else if self.cur_tok < CS_TOKEN_FLAG + self.eqtb.lay.single_base {
                self.cur_val = self.cur_tok - CS_TOKEN_FLAG - self.eqtb.lay.active_base;
            } else {
                self.cur_val = self.cur_tok - CS_TOKEN_FLAG - self.eqtb.lay.single_base;
            }
            if self.cur_val > 0x10FFFF {
                self.print_err("Improper alphabetic constant");
                self.help(&[
                    "A one-character control sequence belongs after a ` mark.",
                    "So I'm essentially inserting \\0 here.",
                ]);
                self.cur_val = '0' as i32;
                self.back_error()?;
            } else {
                self.scan_optional_space()?;
            }
        } else if self.cur_cmd >= MIN_INTERNAL && self.cur_cmd <= MAX_INTERNAL {
            self.scan_something_internal(INT_VAL, false)?;
        } else {
            // §444: scan a numeric constant.
            self.radix = 10;
            let mut m = 214_748_364;
            if self.cur_tok == OCTAL_TOKEN {
                self.radix = 8;
                m = 0o2000000000;
                self.get_x_token()?;
            } else if self.cur_tok == HEX_TOKEN {
                self.radix = 16;
                m = 0o1000000000;
                self.get_x_token()?;
            }
            let mut vacuous = true;
            self.cur_val = 0;
            // §445: accumulate the constant.
            loop {
                let d;
                if self.cur_tok < ZERO_TOKEN + self.radix
                    && self.cur_tok >= ZERO_TOKEN
                    && self.cur_tok <= ZERO_TOKEN + 9
                {
                    d = self.cur_tok - ZERO_TOKEN;
                } else if self.radix == 16 {
                    if self.cur_tok <= A_TOKEN + 5 && self.cur_tok >= A_TOKEN {
                        d = self.cur_tok - A_TOKEN + 10;
                    } else if self.cur_tok <= OTHER_A_TOKEN + 5 && self.cur_tok >= OTHER_A_TOKEN {
                        d = self.cur_tok - OTHER_A_TOKEN + 10;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
                vacuous = false;
                if self.cur_val >= m && (self.cur_val > m || d > 7 || self.radix != 10) {
                    if ok_so_far {
                        self.print_err("Number too big");
                        self.help(&[
                            "I can only go up to 2147483647='17777777777=\"7FFFFFFF,",
                            "so I'm using that number instead of yours.",
                        ]);
                        self.error()?;
                        self.cur_val = INFINITY;
                        ok_so_far = false;
                    }
                } else {
                    self.cur_val = self.cur_val * self.radix + d;
                }
                self.get_x_token()?;
            }
            if vacuous {
                // §446: express astonishment.
                if std::env::var("SABITEX_DEBUG_SCAN").is_ok() {
                    eprintln!(
                        "DBG vacuous: cur_cmd={} cur_chr={} cur_cs={} tok={}",
                        self.cur_cmd, self.cur_chr, self.cur_cs, self.cur_tok
                    );
                    let cs = self.cur_cs;
                    if cs > 0 {
                        let t = self.eqtb.text(cs);
                        eprintln!("DBG vacuous cs name: {:?}", self.strings.text(t));
                    }
                    let save_sel = self.prn.selector;
                    self.prn.selector = crate::print::TERM_ONLY;
                    let ecl = self.eqtb.lay.int_base + crate::eqtb::ERROR_CONTEXT_LINES_CODE;
                    let old_e = self.eqtb.int(ecl);
                    self.eqtb.set_int(ecl, 100);
                    self.show_context();
                    self.eqtb.set_int(ecl, old_e);
                    self.prn.selector = save_sel;
                }
                self.print_err("Missing number, treated as zero");
                self.help(&[
                    "A number should have been here; I inserted `0'.",
                    "(If you can't figure out why I needed to see a number,",
                    "look up `weird error' in the index to The TeXbook.)",
                ]);
                self.back_error()?;
            } else if self.cur_cmd != SPACER {
                self.back_input()?;
            }
        }
        if negative {
            self.cur_val = -self.cur_val;
        }
        Ok(())
    }

    /// §443: scan an optional space.
    pub fn scan_optional_space(&mut self) -> TexResult<()> {
        self.get_x_token()?;
        if self.cur_cmd != SPACER {
            self.back_input()?;
        }
        Ok(())
    }

    /// `scan_dimen(mu, inf, shortcut)` (§448-§460): sets `cur_val` to a
    /// dimension.
    pub fn scan_dimen(&mut self, mu: bool, inf: bool, shortcut: bool) -> TexResult<()> {
        let mut f: i32 = 0;
        self.arith.arith_error = false;
        self.cur_order = NORMAL;
        let mut negative = false;
        'attach_sign: {
            if !shortcut {
                negative = self.scan_signs()?;
                if self.cur_cmd >= MIN_INTERNAL && self.cur_cmd <= MAX_INTERNAL {
                    // §449: fetch an internal dimension or integer.
                    if mu {
                        self.scan_something_internal(MU_VAL, false)?;
                        // §451: coerce glue to a dimension.
                        if self.cur_val_level >= GLUE_VAL {
                            let v = self.mem.width(self.cur_val);
                            let cv = self.cur_val;
                            self.mem.delete_glue_ref(cv);
                            self.cur_val = v;
                        }
                        if self.cur_val_level == MU_VAL {
                            break 'attach_sign;
                        }
                        if self.cur_val_level != INT_VAL {
                            self.mu_error()?;
                        }
                    } else {
                        self.scan_something_internal(DIMEN_VAL, false)?;
                        if self.cur_val_level == DIMEN_VAL {
                            break 'attach_sign;
                        }
                    }
                } else {
                    self.back_input()?;
                    if self.cur_tok == CONTINENTAL_POINT_TOKEN {
                        self.cur_tok = POINT_TOKEN;
                    }
                    if self.cur_tok != POINT_TOKEN {
                        self.scan_int()?;
                    } else {
                        self.radix = 10;
                        self.cur_val = 0;
                    }
                    if self.cur_tok == CONTINENTAL_POINT_TOKEN {
                        self.cur_tok = POINT_TOKEN;
                    }
                    if self.radix == 10 && self.cur_tok == POINT_TOKEN {
                        // §452: scan a decimal fraction.
                        let mut dig = [0u8; 17];
                        let mut k: usize = 0;
                        self.get_token()?; // point_token is being re-scanned
                        loop {
                            self.get_x_token()?;
                            if self.cur_tok > ZERO_TOKEN + 9 || self.cur_tok < ZERO_TOKEN {
                                break;
                            }
                            if k < 17 {
                                dig[k] = (self.cur_tok - ZERO_TOKEN) as u8;
                                k += 1;
                            }
                        }
                        f = arith::round_decimals(&dig[..k]);
                        if self.cur_cmd != SPACER {
                            self.back_input()?;
                        }
                    }
                }
            }
            if self.cur_val < 0 {
                // in this case f = 0
                negative = !negative;
                self.cur_val = -self.cur_val;
            }
            // §453: scan units and set cur_val to x*(cur_val + f/2^16).
            'attach_fraction: {
                'not_found: {
                    if inf {
                        // §454: scan for fil units.
                        if self.scan_keyword("fil")? {
                            self.cur_order = crate::mem::FIL;
                            while self.scan_keyword("l")? {
                                if self.cur_order == crate::mem::FILLL {
                                    self.print_err("Illegal unit of measure (");
                                    self.print_chars("replaced by filll)");
                                    self.help(&["I dddon't go any higher than filll."]);
                                    self.error()?;
                                } else {
                                    self.cur_order += 1;
                                }
                            }
                            break 'attach_fraction;
                        }
                    }
                    // §455: scan for units that are internal dimensions.
                    let save_cur_val = self.cur_val;
                    self.get_next_nonblank_noncall()?;
                    let v: Scaled;
                    if self.cur_cmd < MIN_INTERNAL || self.cur_cmd > MAX_INTERNAL {
                        self.back_input()?;
                        if mu {
                            break 'not_found;
                        }
                        if self.scan_keyword("em")? {
                            v = self.fonts.quad(self.eqtb.cur_font()); // §558
                        } else if self.scan_keyword("ex")? {
                            v = self.fonts.x_height(self.eqtb.cur_font()); // §559
                        } else {
                            break 'not_found;
                        }
                        self.scan_optional_space()?;
                    } else {
                        if mu {
                            self.scan_something_internal(MU_VAL, false)?;
                            if self.cur_val_level >= GLUE_VAL {
                                let w = self.mem.width(self.cur_val);
                                let cv = self.cur_val;
                                self.mem.delete_glue_ref(cv);
                                self.cur_val = w;
                            }
                            if self.cur_val_level != MU_VAL {
                                self.mu_error()?;
                            }
                        } else {
                            self.scan_something_internal(DIMEN_VAL, false)?;
                        }
                        v = self.cur_val;
                    }
                    // found:
                    let xd = arith::xn_over_d(&mut self.arith, v, f, 0o200000);
                    self.cur_val = arith::nx_plus_y(&mut self.arith, save_cur_val, v, xd);
                    break 'attach_sign;
                }
                // not_found:
                if mu {
                    // §456: scan for mu units.
                    if !self.scan_keyword("mu")? {
                        self.print_err("Illegal unit of measure (");
                        self.print_chars("mu inserted)");
                        self.help(&[
                            "The unit of measurement in math glue must be mu.",
                            "To recover gracefully from this error, it's best to",
                            "delete the erroneous units; e.g., type `2' to delete",
                            "two letters. (See Chapter 27 of The TeXbook.)",
                        ]);
                        self.error()?;
                    }
                    break 'attach_fraction;
                }
                if self.scan_keyword("true")? {
                    // §457: adjust for the magnification ratio.
                    self.prepare_mag()?;
                    let mag = self.eqtb.int_par(crate::eqtb::MAG_CODE);
                    if mag != 1000 {
                        self.cur_val = arith::xn_over_d(&mut self.arith, self.cur_val, 1000, mag);
                        f = (1000 * f + 0o200000 * self.arith.remainder) / mag;
                        self.cur_val += f / 0o200000;
                        f %= 0o200000;
                    }
                }
                if self.scan_keyword("pt")? {
                    break 'attach_fraction; // the easy case
                }
                // §458: scan for all other units.
                'done2: {
                    let (num, denom): (i32, i32) = if self.scan_keyword("in")? {
                        (7227, 100)
                    } else if self.scan_keyword("pc")? {
                        (12, 1)
                    } else if self.scan_keyword("cm")? {
                        (7227, 254)
                    } else if self.scan_keyword("mm")? {
                        (7227, 2540)
                    } else if self.scan_keyword("bp")? {
                        (7227, 7200)
                    } else if self.scan_keyword("dd")? {
                        (1238, 1157)
                    } else if self.scan_keyword("cc")? {
                        (14856, 1157)
                    } else if self.scan_keyword("sp")? {
                        // goto done (sp: cur_val is already in sp)
                        self.scan_optional_space()?;
                        break 'attach_sign;
                    } else {
                        // §459: complain about unknown unit.
                        self.print_err("Illegal unit of measure (");
                        self.print_chars("pt inserted)");
                        self.help(&[
                            "Dimensions can be in units of em, ex, in, pt, pc,",
                            "cm, mm, dd, cc, bp, or sp; but yours is a new one!",
                            "I'll assume that you meant to say pt, for printer's points.",
                            "To recover gracefully from this error, it's best to",
                            "delete the erroneous units; e.g., type `2' to delete",
                            "two letters. (See Chapter 27 of The TeXbook.)",
                        ]);
                        self.error()?;
                        break 'done2;
                    };
                    self.cur_val = arith::xn_over_d(&mut self.arith, self.cur_val, num, denom);
                    f = (num * f + 0o200000 * self.arith.remainder) / denom;
                    self.cur_val += f / 0o200000;
                    f %= 0o200000;
                }
            }
            // attach_fraction:
            if self.cur_val >= 0o40000 {
                self.arith.arith_error = true;
            } else {
                self.cur_val = self.cur_val * UNITY + f;
            }
            // done:
            self.scan_optional_space()?;
        }
        // attach_sign:
        if self.arith.arith_error || self.cur_val.abs() >= 0o10000000000 {
            // §460: report that this dimension is out of range.
            self.print_err("Dimension too large");
            self.help(&[
                "I can't work with sizes bigger than about 19 feet.",
                "Continue and I'll use the largest value I can.",
            ]);
            self.error()?;
            self.cur_val = MAX_DIMEN;
            self.arith.arith_error = false;
        }
        if negative {
            self.cur_val = -self.cur_val;
        }
        Ok(())
    }

    /// `scan_normal_dimen` (§448).
    pub fn scan_normal_dimen(&mut self) -> TexResult<()> {
        self.scan_dimen(false, false, false)
    }

    /// `scan_glue(level)` (§461-§462): sets `cur_val` to a glue spec.
    pub fn scan_glue(&mut self, level: u8) -> TexResult<()> {
        let mu = level == MU_VAL;
        let negative = self.scan_signs()?;
        if self.cur_cmd >= MIN_INTERNAL && self.cur_cmd <= MAX_INTERNAL {
            self.scan_something_internal(level, negative)?;
            if self.cur_val_level >= GLUE_VAL {
                if self.cur_val_level != level {
                    self.mu_error()?;
                }
                return Ok(());
            }
            if self.cur_val_level == INT_VAL {
                self.scan_dimen(mu, false, true)?;
            } else if level == MU_VAL {
                self.mu_error()?;
            }
        } else {
            self.back_input()?;
            self.scan_dimen(mu, false, false)?;
            if negative {
                self.cur_val = -self.cur_val;
            }
        }
        // §462: create a new glue spec with width cur_val; scan stretch/shrink.
        let zg = self.mem.zero_glue();
        let q = self.new_spec(zg)?;
        let w = self.cur_val;
        self.mem.set_width(q, w);
        if self.scan_keyword("plus")? {
            self.scan_dimen(mu, true, false)?;
            let (v, o) = (self.cur_val, self.cur_order);
            self.mem.set_stretch(q, v);
            self.mem.set_stretch_order(q, o);
        }
        if self.scan_keyword("minus")? {
            self.scan_dimen(mu, true, false)?;
            let (v, o) = (self.cur_val, self.cur_order);
            self.mem.set_shrink(q, v);
            self.mem.set_shrink_order(q, o);
        }
        self.cur_val = q;
        Ok(())
    }

    /// `prepare_mag` (§288), reduced: \mag freezing.
    pub fn prepare_mag(&mut self) -> TexResult<()> {
        let mag = self.eqtb.int_par(crate::eqtb::MAG_CODE);
        if self.mag_set > 0 && mag != self.mag_set {
            self.print_err("Incompatible magnification (");
            self.print_int(mag);
            self.print_chars(");");
            self.print_nl_chars(" the previous value will be retained");
            self.help(&[
                "I can handle only one magnification ratio per job. So I've",
                "reverted to the magnification you used earlier on this run.",
            ]);
            let ms = self.mag_set;
            self.int_error(ms)?;
            let p = self.eqtb.lay.int_base + crate::eqtb::MAG_CODE;
            let v = self.mag_set;
            self.geq_word_define(p, v);
        }
        let mag = self.eqtb.int_par(crate::eqtb::MAG_CODE);
        if !(1..=32768).contains(&mag) {
            self.print_err("Illegal magnification has been changed to 1000");
            self.help(&["The magnification ratio must be between 1 and 32768."]);
            self.int_error(mag)?;
            let p = self.eqtb.lay.int_base + crate::eqtb::MAG_CODE;
            self.geq_word_define(p, 1000);
        }
        self.mag_set = self.eqtb.int_par(crate::eqtb::MAG_CODE);
        Ok(())
    }
}
