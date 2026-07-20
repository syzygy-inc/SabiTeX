//! Control sequence lookup, primitive installation, and symbolic printing.
//!
//! Ports `id_lookup` (§259-§261), `print_cs`/`sprint_cs` (§262-§263),
//! `primitive` (§264) and `print_cmd_chr` (§298 plus its scattered cases).
//! `print_cmd_chr` covers exactly the primitives installed so far; later
//! milestones extend it alongside their `primitive` calls.

use crate::cmds::*;
use crate::cond::*;
use crate::engine::Engine;
use crate::eqtb::*;
use crate::error::TexResult;
use crate::getnext::TOO_BIG_CHAR;
use crate::scan::{BADNESS_CODE, DIMEN_VAL, GLUE_VAL, INPUT_LINE_NO_CODE, INT_VAL};
use crate::toks::*;
use crate::types::{Pointer, StrNumber};

// §135-§136 (box node fields, needed for \wd, \ht, \dp).
pub const WIDTH_OFFSET: i32 = 1;
pub const DEPTH_OFFSET: i32 = 2;
pub const HEIGHT_OFFSET: i32 = 3;

// §1291: \show family codes (+ etex.ch: \showgroups, \showtokens=5 lives
// in toks.rs, \showifs).
pub const SHOW_CODE: i32 = 0;
pub const SHOW_BOX_CODE: i32 = 1;
pub const SHOW_THE_CODE: i32 = 2;
pub const SHOW_LISTS_CODE: i32 = 3;
pub const SHOW_GROUPS_CODE: i32 = 4;
pub const SHOW_IFS_CODE: i32 = 6;

impl Engine {
    /// `id_lookup(j, l)` (§259): searches the hash table for the identifier
    /// in `buffer[j..j+l]`, inserting it if allowed.
    pub fn id_lookup(&mut self, j: i32, l: i32) -> Pointer {
        // §261: compute the hash code h.
        let prime = self.eqtb.lay.hash_prime;
        let mut h = self.inp.buffer[j as usize];
        for k in (j + 1)..(j + l) {
            h = h + h + self.inp.buffer[k as usize];
            while h >= prime {
                h -= prime;
            }
        }
        let mut p = h + self.eqtb.lay.hash_base;
        loop {
            let text = self.eqtb.text(p);
            if text > 0 && self.strings.length(text) == l as usize && self.str_eq_buf(text, j) {
                return p;
            }
            if self.eqtb.next(p) == 0 {
                if self.eqtb.no_new_control_sequence {
                    return self.eqtb.lay.undefined_control_sequence;
                }
                // §260: insert a new control sequence after p.
                if self.eqtb.text(p) > 0 {
                    loop {
                        if self.eqtb.hash_is_full() {
                            // tex.web overflows here. id_lookup cannot
                            // return TexResult yet, so make the failure
                            // LOUD instead of silently mis-tokenizing
                            // (a full table once shredded an amsmath
                            // load): every miss reports once to the
                            // terminal and the transcript.
                            if !self.hash_overflow_reported {
                                self.hash_overflow_reported = true;
                                let hs = self.sizes.hash_size;
                                self.print_nl_chars("! TeX capacity exceeded, sorry [hash size=");
                                self.print_int(hs);
                                self.print_chars("]. Control sequences are being LOST.");
                                self.print_ln();
                            }
                            return self.eqtb.lay.undefined_control_sequence;
                        }
                        self.eqtb.hash_used -= 1;
                        if self.eqtb.text(self.eqtb.hash_used) == 0 {
                            break;
                        }
                    }
                    let hu = self.eqtb.hash_used;
                    self.eqtb.set_next(p, hu);
                    p = hu;
                }
                let units: Vec<u16> = (j..j + l)
                    .flat_map(|k| {
                        let c = self.inp.buffer[k as usize] as u32;
                        let mut buf = [0u16; 2];
                        char::from_u32(c)
                            .unwrap_or(char::REPLACEMENT_CHARACTER)
                            .encode_utf16(&mut buf)
                            .to_vec()
                    })
                    .collect();
                let s = self.strings.make_string_from(&units).unwrap_or(0);
                self.eqtb.set_text(p, s);
                self.eqtb.cs_count += 1;
                return p;
            }
            p = self.eqtb.next(p);
        }
    }

    /// `str_eq_buf(s, k)` (§45) against the Unicode buffer.
    fn str_eq_buf(&self, s: StrNumber, k: i32) -> bool {
        let k = k as usize;
        // Compare decoded scalars (pool is UTF-16; buffer is UTF-32).
        let units = self.strings.str(s);
        for (i, r) in char::decode_utf16(units.iter().copied()).enumerate() {
            let c = r.map(|c| c as i32).unwrap_or(0xFFFD);
            if c != self.inp.buffer[k + i] {
                return false;
            }
        }
        true
    }

    /// `print_cs(p)` (§262): prints a control sequence name, padding with a
    /// space unless it is a single nonletter or active character.
    pub fn print_cs(&mut self, p: Pointer) {
        let lay = &self.eqtb.lay;
        let (hash_base, single_base, null_cs, active_base, ucs) = (
            lay.hash_base,
            lay.single_base,
            lay.null_cs,
            lay.active_base,
            lay.undefined_control_sequence,
        );
        if p < hash_base {
            // single character
            if p >= single_base {
                if p == null_cs {
                    self.print_esc_str("csname");
                    self.print_esc_str("endcsname");
                    self.print_char(' ' as i32);
                } else {
                    let c = p - single_base;
                    self.print_esc_char(c);
                    if self.eqtb.cat_code(c) == i32::from(LETTER) {
                        self.print_char(' ' as i32);
                    }
                }
            } else if p < active_base {
                self.print_esc_str("IMPOSSIBLE.");
            } else {
                self.print_char_code(p - active_base);
            }
        } else if p >= ucs {
            self.print_esc_str("IMPOSSIBLE.");
        } else if self.eqtb.text(p) < 0 || self.eqtb.text(p) >= self.strings.str_ptr() as i32 {
            self.print_esc_str("NONEXISTENT.");
        } else {
            let t = self.eqtb.text(p);
            self.print_esc(t);
            self.print_char(' ' as i32);
        }
    }

    /// `sprint_cs(p)` (§263): like `print_cs` but never adds a trailing
    /// space and skips the error checks.
    pub fn sprint_cs(&mut self, p: Pointer) {
        let lay = &self.eqtb.lay;
        let (hash_base, single_base, null_cs, active_base) =
            (lay.hash_base, lay.single_base, lay.null_cs, lay.active_base);
        if p < hash_base {
            if p < single_base {
                self.print_char_code(p - active_base);
            } else if p < null_cs {
                self.print_esc_char(p - single_base);
            } else {
                self.print_esc_str("csname");
                self.print_esc_str("endcsname");
            }
        } else {
            let t = self.eqtb.text(p);
            self.print_esc(t);
        }
    }

    /// `primitive(s, c, o)` (§264): puts a primitive into the hash table.
    /// tex.web scribbles the name over `buffer[0..l-1]`; e-TeX generates
    /// primitives while the first input line sits there (etex.ch §1337), so
    /// this port saves and restores the clobbered prefix.
    pub fn primitive(&mut self, s: &str, c: u16, o: i32) -> TexResult<Pointer> {
        let chars: Vec<i32> = s.chars().map(|ch| ch as i32).collect();
        let val = if chars.len() == 1 {
            chars[0] + self.eqtb.lay.single_base
        } else {
            let saved: Vec<i32> = self.inp.buffer[..chars.len()].to_vec();
            for (j, &ch) in chars.iter().enumerate() {
                self.inp.buffer[j] = ch;
            }
            let was = self.eqtb.no_new_control_sequence;
            self.eqtb.no_new_control_sequence = false;
            let p = self.id_lookup(0, chars.len() as i32);
            self.eqtb.no_new_control_sequence = was;
            self.inp.buffer[..saved.len()].copy_from_slice(&saved);
            p
        };
        self.eqtb.set_eq_level(val, LEVEL_ONE);
        self.eqtb.set_eq_type(val, c);
        self.eqtb.set_equiv(val, o);
        Ok(val)
    }

    /// `restore_trace(p, s)` (§284): `eqtb[p]` has just been restored or
    /// retained.
    pub fn restore_trace(&mut self, p: Pointer, s: &str) {
        self.begin_diagnostic();
        self.print_char('{' as i32);
        self.print_chars(s);
        self.print_char(' ' as i32);
        self.show_eqtb(p);
        self.print_char('}' as i32);
        self.end_diagnostic(false);
    }

    /// `show_eqtb(n)` (§252): displays the contents of `eqtb[n]`
    /// symbolically.
    pub fn show_eqtb(&mut self, n: Pointer) {
        let lay = self.eqtb.lay.clone();
        if n < lay.active_base {
            self.print_char('?' as i32); // this can't happen
        } else if n < lay.glue_base {
            // §223: region 1 or 2.
            self.sprint_cs(n);
            self.print_char('=' as i32);
            let (t, e) = (self.eqtb.eq_type(n), self.eqtb.equiv(n));
            self.print_cmd_chr(t, e);
            if t >= CALL {
                self.print_char(':' as i32);
                let l = self.mem.link(e);
                self.show_token_list(l, crate::types::NULL, 32);
            }
        } else if n < lay.local_base {
            // §229: region 3.
            let e = self.eqtb.equiv(n);
            if n < lay.skip_base {
                self.print_cmd_chr(ASSIGN_GLUE, n); // print_skip_param
                self.print_char('=' as i32);
                if n < lay.glue_base + THIN_MU_SKIP_CODE {
                    self.print_spec(e, "pt");
                } else {
                    self.print_spec(e, "mu");
                }
            } else if n < lay.mu_skip_base {
                self.print_esc_str("skip");
                self.print_int(n - lay.skip_base);
                self.print_char('=' as i32);
                self.print_spec(e, "pt");
            } else {
                self.print_esc_str("muskip");
                self.print_int(n - lay.mu_skip_base);
                self.print_char('=' as i32);
                self.print_spec(e, "mu");
            }
        } else if n < lay.int_base {
            // §233: region 4.
            let e = self.eqtb.equiv(n);
            if n == lay.par_shape_loc || (n >= lay.etex_pen_base && n < lay.box_base) {
                // §233 (+ etex.ch): \parshape and the penalties arrays.
                self.print_cmd_chr(SET_SHAPE, n);
                self.print_char('=' as i32);
                if e == crate::types::NULL {
                    self.print_char('0' as i32);
                } else if n > lay.par_shape_loc {
                    let c = self.mem.word(e + 1).int();
                    self.print_int(c);
                    self.print_char(' ' as i32);
                    let v = self.mem.word(e + 2).int();
                    self.print_int(v);
                    if c > 1 {
                        self.print_esc_str("ETC.");
                    }
                } else {
                    let i = self.mem.info(e);
                    self.print_int(i);
                }
            } else if n < lay.toks_base {
                self.print_cmd_chr(ASSIGN_TOKS, n);
                self.print_char('=' as i32);
                if e != crate::types::NULL {
                    let l = self.mem.link(e);
                    self.show_token_list(l, crate::types::NULL, 32);
                }
            } else if n < lay.box_base {
                self.print_esc_str("toks");
                self.print_int(n - lay.toks_base);
                self.print_char('=' as i32);
                if e != crate::types::NULL {
                    let l = self.mem.link(e);
                    self.show_token_list(l, crate::types::NULL, 32);
                }
            } else if n < lay.cur_font_loc {
                self.print_esc_str("box");
                self.print_int(n - lay.box_base);
                self.print_char('=' as i32);
                if e == crate::types::NULL {
                    self.print_chars("void");
                } else {
                    self.depth_threshold = 0;
                    self.breadth_max = 1;
                    self.show_node_list(e);
                }
            } else if n < lay.cat_code_base {
                // §234: the font identifier.
                if n == lay.cur_font_loc {
                    self.print_chars("current font");
                } else if n < lay.math_font_base + 256 {
                    self.print_esc_str("textfont");
                    self.print_int(n - lay.math_font_base);
                } else if n < lay.math_font_base + 512 {
                    self.print_esc_str("scriptfont");
                    self.print_int(n - lay.math_font_base - 256);
                } else {
                    self.print_esc_str("scriptscriptfont");
                    self.print_int(n - lay.math_font_base - 512);
                }
                self.print_char('=' as i32);
                let t = self.eqtb.text(lay.font_id_base + e); // font_id_text
                self.print_esc(t);
            } else if n < lay.math_code_base {
                // §235: the halfword codes.
                if n < lay.lc_code_base {
                    self.print_esc_str("catcode");
                    self.print_int(n - lay.cat_code_base);
                } else if n < lay.uc_code_base {
                    self.print_esc_str("lccode");
                    self.print_int(n - lay.lc_code_base);
                } else if n < lay.sf_code_base {
                    self.print_esc_str("uccode");
                    self.print_int(n - lay.uc_code_base);
                } else {
                    self.print_esc_str("sfcode");
                    self.print_int(n - lay.sf_code_base);
                }
                self.print_char('=' as i32);
                self.print_int(e);
            } else {
                self.print_esc_str("mathcode");
                self.print_int(n - lay.math_code_base);
                self.print_char('=' as i32);
                self.print_int(e);
            }
        } else if n < lay.dimen_base {
            // §242: region 5.
            if n < lay.count_base {
                self.print_cmd_chr(ASSIGN_INT, n);
            } else if n < lay.del_code_base {
                self.print_esc_str("count");
                self.print_int(n - lay.count_base);
            } else {
                self.print_esc_str("delcode");
                self.print_int(n - lay.del_code_base);
            }
            self.print_char('=' as i32);
            let v = self.eqtb.word(n).int();
            self.print_int(v);
        } else if n <= lay.eqtb_size {
            // §251: region 6.
            if n < lay.scaled_base {
                self.print_cmd_chr(ASSIGN_DIMEN, n);
            } else {
                self.print_esc_str("dimen");
                self.print_int(n - lay.scaled_base);
            }
            self.print_char('=' as i32);
            let v = self.eqtb.word(n).int();
            self.print_scaled(v);
            self.print_chars("pt");
        } else {
            self.print_char('?' as i32); // this can't happen either
        }
    }

    /// `print_cmd_chr(cmd, chr_code)` (§298 and scattered cases): symbolic
    /// interpretation of a command code and its modifier.
    pub fn print_cmd_chr(&mut self, cmd: u16, chr_code: i32) {
        let lay_snapshot = self.eqtb.lay.clone();
        let lay = &lay_snapshot;
        let chr_cmd = |e: &mut Self, s: &str| {
            e.print_chars(s);
            e.print_char_code(chr_code);
        };
        match cmd {
            LEFT_BRACE => chr_cmd(self, "begin-group character "),
            RIGHT_BRACE => chr_cmd(self, "end-group character "),
            MATH_SHIFT => chr_cmd(self, "math shift character "),
            MAC_PARAM => chr_cmd(self, "macro parameter character "),
            SUP_MARK => chr_cmd(self, "superscript character "),
            SUB_MARK => chr_cmd(self, "subscript character "),
            ENDV => self.print_chars("end of alignment template"),
            SPACER => chr_cmd(self, "blank space "),
            LETTER => chr_cmd(self, "the letter "),
            OTHER_CHAR => chr_cmd(self, "the character "),
            TAB_MARK => {
                // §780: \span or alignment tab.
                if chr_code == crate::align::SPAN_CODE {
                    self.print_esc_str("span");
                } else {
                    chr_cmd(self, "alignment tab character ");
                }
            }
            CAR_RET => {
                // §780: \cr or \crcr.
                if chr_code == crate::align::CR_CODE {
                    self.print_esc_str("cr");
                } else {
                    self.print_esc_str("crcr");
                }
            }
            PAR_END if chr_code == TOO_BIG_CHAR => {
                self.print_esc_str("par");
            }
            ASSIGN_GLUE | ASSIGN_MU_GLUE => {
                // §229.
                if chr_code < lay.skip_base {
                    let n = (chr_code - lay.glue_base) as usize;
                    self.print_esc_str(SKIP_PARAM_NAMES.get(n).unwrap_or(&"?"));
                } else if chr_code < lay.mu_skip_base {
                    self.print_esc_str("skip");
                    self.print_int(chr_code - lay.skip_base);
                } else {
                    self.print_esc_str("muskip");
                    self.print_int(chr_code - lay.mu_skip_base);
                }
            }
            ASSIGN_TOKS => {
                // §233.
                if chr_code >= lay.toks_base {
                    self.print_esc_str("toks");
                    self.print_int(chr_code - lay.toks_base);
                } else {
                    let n = (chr_code - lay.output_routine_loc) as usize;
                    self.print_esc_str(TOKS_PARAM_NAMES.get(n).unwrap_or(&"errhelp"));
                }
            }
            ASSIGN_INT => {
                // §242.
                if chr_code < lay.count_base {
                    let n = (chr_code - lay.int_base) as usize;
                    self.print_esc_str(INT_PARAM_NAMES.get(n).unwrap_or(&"?"));
                } else {
                    self.print_esc_str("count");
                    self.print_int(chr_code - lay.count_base);
                }
            }
            ASSIGN_DIMEN => {
                // §249.
                if chr_code < lay.scaled_base {
                    let n = (chr_code - lay.dimen_base) as usize;
                    self.print_esc_str(DIMEN_PARAM_NAMES.get(n).unwrap_or(&"?"));
                } else {
                    self.print_esc_str("dimen");
                    self.print_int(chr_code - lay.scaled_base);
                }
            }
            REGISTER => {
                // §412 (+ etex.ch sparse arrays).
                let (t, leaf) =
                    if chr_code < self.mem.mem_bot || chr_code > self.mem.lo_mem_stat_max() {
                        (self.sa_type(chr_code), chr_code)
                    } else {
                        ((chr_code - self.mem.mem_bot) as u8, crate::types::NULL)
                    };
                let s = match t {
                    INT_VAL => "count",
                    DIMEN_VAL => "dimen",
                    GLUE_VAL => "skip",
                    _ => "muskip",
                };
                self.print_esc_str(s);
                if leaf != crate::types::NULL {
                    self.print_sa_num(leaf);
                }
            }
            TOKS_REGISTER => {
                // etex.ch.
                self.print_esc_str("toks");
                if chr_code != self.mem.mem_bot {
                    self.print_sa_num(chr_code);
                }
            }
            SET_AUX => {
                if chr_code == crate::engine::VMODE {
                    self.print_esc_str("prevdepth");
                } else {
                    self.print_esc_str("spacefactor");
                }
            }
            SET_PAGE_INT => {
                if chr_code == 0 {
                    self.print_esc_str("deadcycles");
                } else if chr_code == 2 {
                    self.print_esc_str("interactionmode"); // etex.ch
                } else {
                    self.print_esc_str("insertpenalties");
                }
            }
            SET_BOX_DIMEN => {
                if chr_code == WIDTH_OFFSET {
                    self.print_esc_str("wd");
                } else if chr_code == HEIGHT_OFFSET {
                    self.print_esc_str("ht");
                } else {
                    self.print_esc_str("dp");
                }
            }
            LAST_ITEM => {
                let s = match chr_code {
                    x if x == i32::from(INT_VAL) => "lastpenalty",
                    x if x == i32::from(DIMEN_VAL) => "lastkern",
                    x if x == i32::from(GLUE_VAL) => "lastskip",
                    crate::scan::LAST_NODE_TYPE_CODE => "lastnodetype",
                    INPUT_LINE_NO_CODE => "inputlineno",
                    BADNESS_CODE => "badness",
                    crate::scan::ETEX_VERSION_CODE => "eTeXversion",
                    crate::scan::CURRENT_GROUP_LEVEL_CODE => "currentgrouplevel",
                    crate::scan::CURRENT_GROUP_TYPE_CODE => "currentgrouptype",
                    crate::scan::CURRENT_IF_LEVEL_CODE => "currentiflevel",
                    crate::scan::CURRENT_IF_TYPE_CODE => "currentiftype",
                    crate::scan::CURRENT_IF_BRANCH_CODE => "currentifbranch",
                    crate::scan::GLUE_STRETCH_ORDER_CODE => "gluestretchorder",
                    crate::scan::GLUE_SHRINK_ORDER_CODE => "glueshrinkorder",
                    crate::scan::FONT_CHAR_WD_CODE => "fontcharwd",
                    crate::scan::FONT_CHAR_HT_CODE => "fontcharht",
                    crate::scan::FONT_CHAR_DP_CODE => "fontchardp",
                    crate::scan::FONT_CHAR_IC_CODE => "fontcharic",
                    crate::scan::PAR_SHAPE_LENGTH_CODE => "parshapelength",
                    crate::scan::PAR_SHAPE_INDENT_CODE => "parshapeindent",
                    crate::scan::PAR_SHAPE_DIMEN_CODE => "parshapedimen",
                    crate::scan::GLUE_STRETCH_CODE => "gluestretch",
                    crate::scan::GLUE_SHRINK_CODE => "glueshrink",
                    crate::scan::MU_TO_GLUE_CODE => "mutoglue",
                    crate::scan::GLUE_TO_MU_CODE => "gluetomu",
                    crate::scan::NUMEXPR_CODE => "numexpr",
                    crate::scan::DIMEXPR_CODE => "dimexpr",
                    crate::scan::GLUEEXPR_CODE => "glueexpr",
                    crate::scan::MUEXPR_CODE => "muexpr",
                    _ => "badness",
                };
                self.print_esc_str(s);
            }
            CONVERT => {
                let s = match chr_code {
                    NUMBER_CODE => "number",
                    ROMAN_NUMERAL_CODE => "romannumeral",
                    STRING_CODE => "string",
                    MEANING_CODE => "meaning",
                    FONT_NAME_CODE => "fontname",
                    ETEX_REVISION_CODE => "eTeXrevision",
                    crate::toks::EXPANDED_CODE => "expanded",
                    crate::toks::XETEX_REVISION_CODE => "XeTeXrevision",
                    crate::toks::PDF_STRCMP_CODE => "strcmp",
                    crate::toks::PDF_FILE_SIZE_CODE => "pdffilesize",
                    crate::toks::PDF_CREATION_DATE_CODE => "pdfcreationdate",
                    _ => "jobname",
                };
                self.print_esc_str(s);
            }
            IF_TEST => {
                // etex.ch: an \unless prefix is folded into the chr code.
                if chr_code >= UNLESS_CODE {
                    self.print_esc_str("unless");
                }
                let s = match chr_code % UNLESS_CODE {
                    IF_CAT_CODE => "ifcat",
                    IF_INT_CODE => "ifnum",
                    IF_DIM_CODE => "ifdim",
                    IF_ODD_CODE => "ifodd",
                    IF_VMODE_CODE => "ifvmode",
                    IF_HMODE_CODE => "ifhmode",
                    IF_MMODE_CODE => "ifmmode",
                    IF_INNER_CODE => "ifinner",
                    IF_VOID_CODE => "ifvoid",
                    IF_HBOX_CODE => "ifhbox",
                    IF_VBOX_CODE => "ifvbox",
                    IFX_CODE => "ifx",
                    IF_EOF_CODE => "ifeof",
                    IF_TRUE_CODE => "iftrue",
                    IF_FALSE_CODE => "iffalse",
                    IF_CASE_CODE => "ifcase",
                    IF_DEF_CODE => "ifdefined",
                    IF_CS_CODE => "ifcsname",
                    IF_FONT_CHAR_CODE => "iffontchar",
                    _ => "if",
                };
                self.print_esc_str(s);
            }
            FI_OR_ELSE => {
                let s = if chr_code == i32::from(FI_CODE) {
                    "fi"
                } else if chr_code == i32::from(OR_CODE) {
                    "or"
                } else {
                    "else"
                };
                self.print_esc_str(s);
            }
            INPUT => {
                if chr_code == 0 {
                    self.print_esc_str("input");
                } else if chr_code == 2 {
                    self.print_esc_str("scantokens"); // etex.ch
                } else {
                    self.print_esc_str("endinput");
                }
            }
            TOP_BOT_MARK => {
                // §286 (+ etex.ch: the \...marks variants).
                let s = match (chr_code % crate::expand::MARKS_CODE) as usize {
                    crate::expand::FIRST_MARK_CODE => "firstmark",
                    crate::expand::BOT_MARK_CODE => "botmark",
                    crate::expand::SPLIT_FIRST_MARK_CODE => "splitfirstmark",
                    crate::expand::SPLIT_BOT_MARK_CODE => "splitbotmark",
                    _ => "topmark",
                };
                self.print_esc_str(s);
                if chr_code >= crate::expand::MARKS_CODE {
                    self.print_char('s' as i32);
                }
            }
            PREFIX => {
                let s = if chr_code == 1 {
                    "long"
                } else if chr_code == 2 {
                    "outer"
                } else if chr_code == 8 {
                    "protected" // etex.ch
                } else {
                    "global"
                };
                self.print_esc_str(s);
            }
            DEF => {
                let s = match chr_code {
                    0 => "def",
                    1 => "gdef",
                    2 => "edef",
                    _ => "xdef",
                };
                self.print_esc_str(s);
            }
            LET => {
                if chr_code != 0 {
                    self.print_esc_str("futurelet");
                } else {
                    self.print_esc_str("let");
                }
            }
            SHORTHAND_DEF => {
                let s = match chr_code {
                    0 => "chardef",
                    1 => "mathchardef",
                    2 => "countdef",
                    3 => "dimendef",
                    4 => "skipdef",
                    5 => "muskipdef",
                    8 => "Umathcharnumdef",
                    9 => "Umathchardef",
                    _ => "toksdef",
                };
                self.print_esc_str(s);
            }
            CHAR_GIVEN => {
                self.print_esc_str("char");
                self.print_hex(chr_code);
            }
            MATH_GIVEN => {
                self.print_esc_str("mathchar");
                self.print_hex(chr_code);
            }
            XETEX_MATH_GIVEN => {
                // xetex.web: printed as \Umathchar with separate fields.
                self.print_esc_str("Umathchar");
                self.print_hex(crate::xemath::math_class_field(chr_code));
                self.print_hex(crate::xemath::math_fam_field(chr_code));
                self.print_hex(crate::xemath::math_char_field(chr_code));
            }
            XETEX_DEF_CODE => {
                let s = if chr_code == lay.math_code_base {
                    "Umathcodenum"
                } else if chr_code == lay.math_code_base + 1 {
                    "Umathcode"
                } else if chr_code == lay.sf_code_base {
                    "XeTeXcharclass"
                } else if chr_code == lay.del_code_base {
                    "Udelcodenum"
                } else {
                    "Udelcode"
                };
                self.print_esc_str(s);
            }
            DEF_CODE => {
                let s = if chr_code == lay.cat_code_base {
                    "catcode"
                } else if chr_code == lay.math_code_base {
                    "mathcode"
                } else if chr_code == lay.lc_code_base {
                    "lccode"
                } else if chr_code == lay.uc_code_base {
                    "uccode"
                } else if chr_code == lay.sf_code_base {
                    "sfcode"
                } else {
                    "delcode"
                };
                self.print_esc_str(s);
            }
            DEF_FAMILY => {
                // §1231 print_size.
                let s = if chr_code == lay.math_font_base {
                    "textfont"
                } else if chr_code == lay.math_font_base + 256 {
                    "scriptfont"
                } else {
                    "scriptscriptfont"
                };
                self.print_esc_str(s);
            }
            MESSAGE => {
                if chr_code == 0 {
                    self.print_esc_str("message");
                } else {
                    self.print_esc_str("errmessage");
                }
            }
            CASE_SHIFT => {
                if chr_code == lay.lc_code_base {
                    self.print_esc_str("lowercase");
                } else {
                    self.print_esc_str("uppercase");
                }
            }
            XRAY => {
                let s = match chr_code {
                    SHOW_BOX_CODE => "showbox",
                    SHOW_THE_CODE => "showthe",
                    SHOW_LISTS_CODE => "showlists",
                    SHOW_GROUPS_CODE => "showgroups", // etex.ch
                    x if x == SHOW_TOKENS => "showtokens", // etex.ch
                    SHOW_IFS_CODE => "showifs",       // etex.ch
                    _ => "show",
                };
                self.print_esc_str(s);
            }
            STOP => {
                if chr_code == 1 {
                    self.print_esc_str("dump");
                } else {
                    self.print_esc_str("end");
                }
            }
            SET_INTERACTION => {
                let s = match chr_code {
                    0 => "batchmode",
                    1 => "nonstopmode",
                    2 => "scrollmode",
                    _ => "errorstopmode",
                };
                self.print_esc_str(s);
            }
            // §296/§1295 + etex.ch: macro and undefined cases used by
            // print_meaning (a \protected marker adds the prefix).
            UNDEFINED_CS => self.print_chars("undefined"),
            CALL | LONG_CALL | OUTER_CALL | LONG_OUTER_CALL => {
                let mut n = i32::from(cmd - CALL);
                if self.mem.info(self.mem.link(chr_code)) == crate::tokens::PROTECTED_TOKEN {
                    n += 4;
                }
                if (n / 4) % 2 == 1 {
                    self.print_esc_str("protected");
                }
                if n % 2 == 1 {
                    self.print_esc_str("long");
                }
                if (n / 2) % 2 == 1 {
                    self.print_esc_str("outer");
                }
                if n > 0 {
                    self.print_char(' ' as i32);
                }
                self.print_chars("macro");
            }
            END_TEMPLATE => self.print_esc_str("outer endtemplate"),
            RELAX => {
                self.print_esc_str("relax");
            }
            // Simple primitives, identified by eq_type alone (§266 etc.).
            _ => {
                let s = match (cmd, chr_code) {
                    (EX_SPACE, _) => " ",
                    (ITAL_CORR, _) => "/",
                    (ACCENT, _) => "accent",
                    (ADVANCE, _) => "advance",
                    (AFTER_ASSIGNMENT, _) => "afterassignment",
                    (AFTER_GROUP, _) => "aftergroup",
                    (ASSIGN_FONT_DIMEN, _) => "fontdimen",
                    (BEGIN_GROUP, _) => "begingroup",
                    (BREAK_PENALTY, _) => "penalty",
                    (CHAR_NUM, _) => "char",
                    (CS_NAME, _) => "csname",
                    (DEF_FONT, _) => "font",
                    (DELIM_NUM, _) => "delimiter",
                    (DIVIDE, _) => "divide",
                    (END_CS_NAME, _) => "endcsname",
                    (END_GROUP, _) => "endgroup",
                    (EXPAND_AFTER, 1) => "unless", // etex.ch
                    (EXPAND_AFTER, _) => "expandafter",
                    (HALIGN, _) => "halign",
                    (HRULE, _) => "hrule",
                    (IGNORE_SPACES, _) => "ignorespaces",
                    (INSERT, _) => "insert",
                    (MARK, c) if c > 0 => "marks", // etex.ch
                    (MARK, _) => "mark",
                    (MATH_ACCENT, _) => "mathaccent",
                    (MATH_CHAR_NUM, _) => "mathchar",
                    (MATH_CHOICE, _) => "mathchoice",
                    (MULTIPLY, _) => "multiply",
                    (NO_ALIGN, _) => "noalign",
                    (NO_BOUNDARY, _) => "noboundary",
                    (NO_EXPAND, _) => "noexpand",
                    (NON_SCRIPT, _) => "nonscript",
                    (OMIT, _) => "omit",
                    // etex.ch: the penalties arrays share set_shape.
                    (SET_SHAPE, c) if c == self.eqtb.lay.etex_pen_base => "interlinepenalties",
                    (SET_SHAPE, c) if c == self.eqtb.lay.etex_pen_base + 1 => "clubpenalties",
                    (SET_SHAPE, c) if c == self.eqtb.lay.etex_pen_base + 2 => "widowpenalties",
                    (SET_SHAPE, c) if c == self.eqtb.lay.etex_pen_base + 3 => {
                        "displaywidowpenalties"
                    }
                    (SET_SHAPE, _) => "parshape",
                    (SET_PREV_GRAF, _) => "prevgraf",
                    (RADICAL, _) => "radical",
                    (READ_TO_CS, 1) => "readline", // etex.ch
                    (READ_TO_CS, _) => "read",
                    (IN_STREAM, 0) => "closein",
                    (IN_STREAM, _) => "openin",
                    (SET_BOX, _) => "setbox",
                    (THE, 1) => "unexpanded", // etex.ch
                    (THE, 5) => "detokenize", // etex.ch (show_tokens)
                    (THE, _) => "the",

                    (VADJUST, _) => "vadjust",
                    (VALIGN, c) if c == i32::from(crate::nodes::BEGIN_L_CODE) => "beginL",
                    (VALIGN, c) if c == i32::from(crate::nodes::END_L_CODE) => "endL",
                    (VALIGN, c) if c == i32::from(crate::nodes::BEGIN_R_CODE) => "beginR",
                    (VALIGN, c) if c == i32::from(crate::nodes::END_R_CODE) => "endR",
                    (VALIGN, _) => "valign",
                    (VCENTER, _) => "vcenter",
                    (VRULE, _) => "vrule",
                    (ASSIGN_FONT_INT, 0) => "hyphenchar",
                    (ASSIGN_FONT_INT, _) => "skewchar",
                    (HYPH_DATA, 1) => "patterns",
                    (HYPH_DATA, _) => "hyphenation",
                    // §1088, §1107-§1108: paragraphs and unboxing.
                    (START_PAR, 0) => "noindent",
                    (START_PAR, _) => "indent",
                    (REMOVE_ITEM, c) if c == i32::from(crate::nodes::GLUE_NODE) => "unskip",
                    (REMOVE_ITEM, c) if c == i32::from(crate::nodes::KERN_NODE) => "unkern",
                    (REMOVE_ITEM, _) => "unpenalty",
                    (UN_HBOX, 1) => "unhcopy",
                    (UN_HBOX, _) => "unhbox",
                    (UN_VBOX, 1) => "unvcopy",
                    (UN_VBOX, 2) => "pagediscards",  // etex.ch
                    (UN_VBOX, 3) => "splitdiscards", // etex.ch
                    (UN_VBOX, _) => "unvbox",
                    (DISCRETIONARY, 1) => "-",
                    (DISCRETIONARY, _) => "discretionary",
                    // §1058: glue and kern commands.
                    (HSKIP, c) if c == crate::control::SKIP_CODE => "hskip",
                    (HSKIP, c) if c == crate::control::FIL_CODE => "hfil",
                    (HSKIP, c) if c == crate::control::FILL_CODE => "hfill",
                    (HSKIP, c) if c == crate::control::SS_CODE => "hss",
                    (HSKIP, _) => "hfilneg",
                    (VSKIP, c) if c == crate::control::SKIP_CODE => "vskip",
                    (VSKIP, c) if c == crate::control::FIL_CODE => "vfil",
                    (VSKIP, c) if c == crate::control::FILL_CODE => "vfill",
                    (VSKIP, c) if c == crate::control::SS_CODE => "vss",
                    (VSKIP, _) => "vfilneg",
                    (MSKIP, _) => "mskip",
                    (KERN, _) => "kern",
                    (MKERN, _) => "mkern",
                    // §1049, §1072: box-moving and box-making commands.
                    (HMOVE, 1) => "moveleft",
                    (HMOVE, _) => "moveright",
                    (VMOVE, 1) => "raise",
                    (VMOVE, _) => "lower",
                    (MAKE_BOX, c) if c == crate::control::BOX_CODE => "box",
                    (MAKE_BOX, c) if c == crate::control::COPY_CODE => "copy",
                    (MAKE_BOX, c) if c == crate::control::LAST_BOX_CODE => "lastbox",
                    (MAKE_BOX, c) if c == crate::control::VSPLIT_CODE => "vsplit",
                    (MAKE_BOX, c) if c == crate::control::VTOP_CODE => "vtop",
                    (MAKE_BOX, c) if c == crate::control::VTOP_CODE + crate::engine::VMODE => {
                        "vbox"
                    }
                    (MAKE_BOX, _) => "hbox",
                    (LEADER_SHIP, c) if c == i32::from(crate::nodes::A_LEADERS) - 1 => "shipout",
                    (LEADER_SHIP, c) if c == i32::from(crate::nodes::A_LEADERS) => "leaders",
                    (LEADER_SHIP, c) if c == i32::from(crate::nodes::C_LEADERS) => "cleaders",
                    (LEADER_SHIP, _) => "xleaders",
                    // §1156-§1188: math primitives.
                    (MATH_COMP, c) if c == i32::from(crate::math::ORD_NOAD) => "mathord",
                    (MATH_COMP, c) if c == i32::from(crate::math::OP_NOAD) => "mathop",
                    (MATH_COMP, c) if c == i32::from(crate::math::BIN_NOAD) => "mathbin",
                    (MATH_COMP, c) if c == i32::from(crate::math::REL_NOAD) => "mathrel",
                    (MATH_COMP, c) if c == i32::from(crate::math::OPEN_NOAD) => "mathopen",
                    (MATH_COMP, c) if c == i32::from(crate::math::CLOSE_NOAD) => "mathclose",
                    (MATH_COMP, c) if c == i32::from(crate::math::PUNCT_NOAD) => "mathpunct",
                    (MATH_COMP, c) if c == i32::from(crate::math::INNER_NOAD) => "mathinner",
                    (MATH_COMP, c) if c == i32::from(crate::math::UNDER_NOAD) => "underline",
                    (MATH_COMP, _) => "overline",
                    (LIMIT_SWITCH, c) if c == i32::from(crate::math::LIMITS) => "limits",
                    (LIMIT_SWITCH, c) if c == i32::from(crate::math::NO_LIMITS) => "nolimits",
                    (LIMIT_SWITCH, _) => "displaylimits",
                    (ABOVE, 1) => "over",
                    (ABOVE, 2) => "atop",
                    (ABOVE, 3) => "abovewithdelims",
                    (ABOVE, 4) => "overwithdelims",
                    (ABOVE, 5) => "atopwithdelims",
                    (ABOVE, _) => "above",
                    (LEFT_RIGHT, c) if c == i32::from(crate::math::LEFT_NOAD) => "left",
                    (LEFT_RIGHT, c) if c == i32::from(crate::math::MIDDLE_NOAD) => "middle",
                    (LEFT_RIGHT, _) => "right",
                    (EQ_NO, 1) => "leqno",
                    (EQ_NO, _) => "eqno",
                    (MATH_STYLE, _) => {
                        self.print_style(chr_code);
                        return;
                    }
                    // §1346: the extensions.
                    (EXTENSION, 0) => "openout",
                    (EXTENSION, 1) => "write",
                    (EXTENSION, 2) => "closeout",
                    (EXTENSION, 3) => "special",
                    (EXTENSION, 4) => "immediate",
                    (EXTENSION, 5) => "setlanguage",
                    // §983: \pagegoal .. \pagedepth.
                    (SET_PAGE_DIMEN, 0) => "pagegoal",
                    (SET_PAGE_DIMEN, 1) => "pagetotal",
                    (SET_PAGE_DIMEN, 2) => "pagestretch",
                    (SET_PAGE_DIMEN, 3) => "pagefilstretch",
                    (SET_PAGE_DIMEN, 4) => "pagefillstretch",
                    (SET_PAGE_DIMEN, 5) => "pagefilllstretch",
                    (SET_PAGE_DIMEN, 6) => "pageshrink",
                    (SET_PAGE_DIMEN, _) => "pagedepth",
                    (SET_FONT, f) => {
                        // §1257.
                        self.print_chars("select font ");
                        let name = self.fonts.name[f as usize].clone();
                        self.print_chars(&name);
                        if self.fonts.size[f as usize] != self.fonts.dsize[f as usize] {
                            self.print_chars(" at ");
                            let s = self.fonts.size[f as usize];
                            self.print_scaled(s);
                            self.print_chars("pt");
                        }
                        return;
                    }
                    _ => {
                        self.print_chars("[unknown command code!]");
                        return;
                    }
                };
                self.print_esc_str(s);
            }
        }
    }
}
