//! Math mode: noads and the conversion of mlists to hlists.
//!
//! Ports tex.web Part 34 (§680-§698: noad data structures and their
//! display/destruction), Part 35 (§699-§718: subroutines such as
//! `var_delimiter`, `rebox` and `math_glue`), and Part 36 (§719-§767:
//! `mlist_to_hlist` and the `make_*` construction procedures).

use crate::arith::half;
use crate::engine::Engine;
use crate::eqtb::*;
use crate::error::TexResult;
use crate::fonts::{FontMem, EXT_TAG, KERN_FLAG, LIG_TAG, LIST_TAG, NULL_FONT, STOP_FLAG};
use crate::mem::Mem;
use crate::memword::MemoryWord;
use crate::nodes::*;
use crate::pack::{ADDITIONAL, EXACTLY};
use crate::types::{Pointer, Scaled, NULL};

// §681: noad fields. nucleus(p)=p+1, supscr(p)=p+2, subscr(p)=p+3;
// math_type==link, fam==font.
pub const NOAD_SIZE: i32 = 4;
pub const MATH_CHAR: i32 = 1;
pub const SUB_BOX: i32 = 2;
pub const SUB_MLIST: i32 = 3;
pub const MATH_TEXT_CHAR: i32 = 4;
/// `empty == 0` (§16): the `math_type` of an absent field.
pub const EMPTY: i32 = 0;

// §682: noad types.
pub const ORD_NOAD: u16 = UNSET_NODE + 3;
pub const OP_NOAD: u16 = ORD_NOAD + 1;
pub const BIN_NOAD: u16 = ORD_NOAD + 2;
pub const REL_NOAD: u16 = ORD_NOAD + 3;
pub const OPEN_NOAD: u16 = ORD_NOAD + 4;
pub const CLOSE_NOAD: u16 = ORD_NOAD + 5;
pub const PUNCT_NOAD: u16 = ORD_NOAD + 6;
pub const INNER_NOAD: u16 = ORD_NOAD + 7;
pub const LIMITS: u16 = 1;
pub const NO_LIMITS: u16 = 2;

// §683: radical and fraction noads.
pub const RADICAL_NOAD: u16 = INNER_NOAD + 1;
pub const RADICAL_NOAD_SIZE: i32 = 5;
pub const FRACTION_NOAD: u16 = RADICAL_NOAD + 1;
pub const FRACTION_NOAD_SIZE: i32 = 6;
/// `default_code` (§683): stands for `default_rule_thickness`.
pub const DEFAULT_CODE: Scaled = 0o10000000000;

// §687: the remaining noads.
pub const UNDER_NOAD: u16 = FRACTION_NOAD + 1;
pub const OVER_NOAD: u16 = UNDER_NOAD + 1;
pub const ACCENT_NOAD: u16 = OVER_NOAD + 1;
pub const ACCENT_NOAD_SIZE: i32 = 5;
pub const VCENTER_NOAD: u16 = ACCENT_NOAD + 1;
pub const LEFT_NOAD: u16 = VCENTER_NOAD + 1;
pub const RIGHT_NOAD: u16 = LEFT_NOAD + 1;
/// etex.ch: subtype of a right noad representing \middle.
pub const MIDDLE_NOAD: u16 = 1;

// §688: style and choice nodes.
pub const STYLE_NODE: u16 = UNSET_NODE + 1;
pub const STYLE_NODE_SIZE: i32 = 3;
pub const DISPLAY_STYLE: u16 = 0;
pub const TEXT_STYLE: u16 = 2;
pub const SCRIPT_STYLE: u16 = 4;
pub const SCRIPT_SCRIPT_STYLE: u16 = 6;
pub const CRAMPED: u16 = 1;
pub const CHOICE_NODE: u16 = UNSET_NODE + 2;

// §699: size codes (xetex.web: 256 math families per size).
pub const TEXT_SIZE: i32 = 0;
pub const SCRIPT_SIZE: i32 = 256;
pub const SCRIPT_SCRIPT_SIZE: i32 = 512;

// §700-§701: math font parameter numbers (in family 2 / family 3).
pub const TOTAL_MATHSY_PARAMS: i32 = 22;
pub const TOTAL_MATHEX_PARAMS: i32 = 13;

// §702: subsidiary styles.
pub fn cramped_style(s: u16) -> u16 {
    2 * (s / 2) + CRAMPED
}
pub fn sub_style(s: u16) -> u16 {
    2 * (s / 4) + SCRIPT_STYLE + CRAMPED
}
pub fn sup_style(s: u16) -> u16 {
    2 * (s / 4) + SCRIPT_STYLE + (s % 2)
}
pub fn num_style(s: u16) -> u16 {
    s + 2 - 2 * (s / 6)
}
pub fn denom_style(s: u16) -> u16 {
    2 * (s / 2) + CRAMPED + 2 - 2 * (s / 6)
}

/// §764: the inter-element spacing table (`math_spacing`).
const MATH_SPACING: &[u8; 64] = b"0234000122*4000133**3**344*0400400*000000234000111*1111112341011";

impl Mem {
    /// `math_type(p) == link(p)` for a noad field address `p` (§681).
    pub fn math_type(&self, p: Pointer) -> i32 {
        self.link(p)
    }

    pub fn set_math_type(&mut self, p: Pointer, t: i32) {
        self.set_link(p, t);
    }

    /// `new_hlist(p) == mem[nucleus(p)].int` (§725).
    pub fn new_hlist(&self, p: Pointer) -> Pointer {
        self.word(p + 1).int()
    }

    pub fn set_new_hlist(&mut self, p: Pointer, v: Pointer) {
        self.word_mut(p + 1).set_int(v);
    }

    /// §684: small/large fam/char of a delimiter word at `p`.
    pub fn small_fam(&self, p: Pointer) -> i32 {
        i32::from(self.word(p).qqqq(0))
    }
    pub fn small_char(&self, p: Pointer) -> i32 {
        i32::from(self.word(p).qqqq(1))
    }
    pub fn large_fam(&self, p: Pointer) -> i32 {
        i32::from(self.word(p).qqqq(2))
    }
    pub fn large_char(&self, p: Pointer) -> i32 {
        i32::from(self.word(p).qqqq(3))
    }
}

impl Engine {
    /// `fam_fnt(n)`: the font for family-plus-size code `n` (§230).
    pub fn fam_fnt(&self, n: i32) -> i32 {
        self.eqtb.equiv(self.eqtb.lay.math_font_base + n)
    }

    /// `mathsy(n, size)` (§700): symbol-font parameters from family 2.
    fn mathsy(&self, n: i32, size: i32) -> Scaled {
        self.fonts.param(n, self.fam_fnt(2 + size))
    }

    pub fn math_x_height(&self, size: i32) -> Scaled {
        self.mathsy(5, size)
    }
    pub fn math_quad(&self, size: i32) -> Scaled {
        self.mathsy(6, size)
    }
    fn num1(&self, size: i32) -> Scaled {
        self.mathsy(8, size)
    }
    fn num2(&self, size: i32) -> Scaled {
        self.mathsy(9, size)
    }
    fn num3(&self, size: i32) -> Scaled {
        self.mathsy(10, size)
    }
    fn denom1(&self, size: i32) -> Scaled {
        self.mathsy(11, size)
    }
    fn denom2(&self, size: i32) -> Scaled {
        self.mathsy(12, size)
    }
    fn sup1(&self, size: i32) -> Scaled {
        self.mathsy(13, size)
    }
    fn sup2(&self, size: i32) -> Scaled {
        self.mathsy(14, size)
    }
    fn sup3(&self, size: i32) -> Scaled {
        self.mathsy(15, size)
    }
    fn sub1(&self, size: i32) -> Scaled {
        self.mathsy(16, size)
    }
    fn sub2(&self, size: i32) -> Scaled {
        self.mathsy(17, size)
    }
    fn sup_drop(&self, size: i32) -> Scaled {
        self.mathsy(18, size)
    }
    fn sub_drop(&self, size: i32) -> Scaled {
        self.mathsy(19, size)
    }
    fn delim1(&self, size: i32) -> Scaled {
        self.mathsy(20, size)
    }
    fn delim2(&self, size: i32) -> Scaled {
        self.mathsy(21, size)
    }
    pub fn axis_height(&self, size: i32) -> Scaled {
        self.mathsy(22, size)
    }

    /// `mathex(n)` (§701): extension-font parameters from family 3 at
    /// `cur_size`.
    fn mathex(&self, n: i32) -> Scaled {
        self.fonts.param(n, self.fam_fnt(3 + self.cur_size))
    }

    pub fn default_rule_thickness(&self) -> Scaled {
        self.mathex(8)
    }
    fn big_op_spacing1(&self) -> Scaled {
        self.mathex(9)
    }
    fn big_op_spacing2(&self) -> Scaled {
        self.mathex(10)
    }
    fn big_op_spacing3(&self) -> Scaled {
        self.mathex(11)
    }
    fn big_op_spacing4(&self) -> Scaled {
        self.mathex(12)
    }
    fn big_op_spacing5(&self) -> Scaled {
        self.mathex(13)
    }

    /// §703: set up `cur_size` and `cur_mu` based on `cur_style`.
    pub fn set_cur_size_and_mu(&mut self) {
        if self.cur_style < SCRIPT_STYLE {
            self.cur_size = TEXT_SIZE;
        } else {
            self.cur_size = SCRIPT_SIZE * (i32::from(self.cur_style - TEXT_STYLE) / 2);
        }
        let mq = self.math_quad(self.cur_size);
        self.cur_mu = crate::arith::x_over_n(&mut self.arith, mq, 18);
    }

    /// `new_noad` (§686).
    pub fn new_noad(&mut self) -> TexResult<Pointer> {
        let p = self.mem.get_node(NOAD_SIZE)?;
        self.mem.set_node_type(p, ORD_NOAD);
        self.mem.set_subtype(p, NORMAL);
        *self.mem.word_mut(p + 1) = MemoryWord::ZERO; // empty_field
        *self.mem.word_mut(p + 2) = MemoryWord::ZERO;
        *self.mem.word_mut(p + 3) = MemoryWord::ZERO;
        Ok(p)
    }

    /// `new_style(s)` (§688).
    pub fn new_style(&mut self, s: u16) -> TexResult<Pointer> {
        let p = self.mem.get_node(STYLE_NODE_SIZE)?;
        self.mem.set_node_type(p, STYLE_NODE);
        self.mem.set_subtype(p, s);
        self.mem.set_width(p, 0);
        self.mem.set_depth(p, 0);
        Ok(p)
    }

    /// `new_choice` (§689).
    pub fn new_choice(&mut self) -> TexResult<Pointer> {
        let p = self.mem.get_node(STYLE_NODE_SIZE)?;
        self.mem.set_node_type(p, CHOICE_NODE);
        self.mem.set_subtype(p, 0);
        self.mem.set_info(p + 1, NULL); // display_mlist
        self.mem.set_link(p + 1, NULL); // text_mlist
        self.mem.set_info(p + 2, NULL); // script_mlist
        self.mem.set_link(p + 2, NULL); // script_script_mlist
        Ok(p)
    }

    // ----- Display of noads (§690-§698). -----

    /// `print_fam_and_char(p)` (§691).
    pub fn print_fam_and_char(&mut self, p: Pointer) {
        self.print_esc_str("fam");
        let f = i32::from(self.mem.font(p));
        self.print_int(f);
        self.print_char(' ' as i32);
        let c = i32::from(self.mem.character(p));
        self.print_ascii(c);
    }

    /// `print_delimiter(p)` (§691).
    pub fn print_delimiter(&mut self, p: Pointer) {
        let mut a = self.mem.small_fam(p) * 256 + self.mem.small_char(p);
        a = a * 0x1000 + self.mem.large_fam(p) * 256 + self.mem.large_char(p);
        if a < 0 {
            self.print_int(a);
        } else {
            self.print_hex(a);
        }
    }

    /// `print_subsidiary_data(p, c)` (§692).
    pub fn print_subsidiary_data(&mut self, p: Pointer, c: i32) {
        if self.strings.cur_length() >= self.depth_threshold as usize {
            if self.mem.math_type(p) != EMPTY {
                self.print_chars(" []");
            }
            return;
        }
        self.strings.append_char(c); // include c in the recursion history
        match self.mem.math_type(p) {
            MATH_CHAR => {
                self.print_ln();
                self.print_current_string();
                self.print_fam_and_char(p);
            }
            SUB_BOX => {
                let q = self.mem.info(p);
                self.show_node_list(q);
            }
            SUB_MLIST => {
                if self.mem.info(p) == NULL {
                    self.print_ln();
                    self.print_current_string();
                    self.print_chars("{}");
                } else {
                    let q = self.mem.info(p);
                    self.show_node_list(q);
                }
            }
            _ => {} // empty
        }
        self.strings.flush_char();
    }

    /// `print_style(c)` (§694).
    pub fn print_style(&mut self, c: i32) {
        match c / 2 {
            0 => self.print_esc_str("displaystyle"),
            1 => self.print_esc_str("textstyle"),
            2 => self.print_esc_str("scriptstyle"),
            3 => self.print_esc_str("scriptscriptstyle"),
            _ => self.print_chars("Unknown style!"),
        }
    }

    /// §690, §695-§698: the mlist cases of `show_node_list`.
    pub fn show_math_node(&mut self, p: Pointer) {
        match self.mem.node_type(p) {
            STYLE_NODE => {
                let s = i32::from(self.mem.subtype(p));
                self.print_style(s);
            }
            CHOICE_NODE => {
                // §695.
                self.print_esc_str("mathchoice");
                for (c, q) in [
                    ('D', self.mem.info(p + 1)),
                    ('T', self.mem.link(p + 1)),
                    ('S', self.mem.info(p + 2)),
                    ('s', self.mem.link(p + 2)),
                ] {
                    self.strings.append_char(c as i32);
                    self.show_node_list(q);
                    self.strings.flush_char();
                }
            }
            FRACTION_NOAD => {
                // §697.
                self.print_esc_str("fraction, thickness ");
                if self.mem.width(p) == DEFAULT_CODE {
                    self.print_chars("= default");
                } else {
                    let t = self.mem.width(p);
                    self.print_scaled(t);
                }
                if self.mem.small_fam(p + 4) != 0
                    || self.mem.small_char(p + 4) != 0
                    || self.mem.large_fam(p + 4) != 0
                    || self.mem.large_char(p + 4) != 0
                {
                    self.print_chars(", left-delimiter ");
                    self.print_delimiter(p + 4);
                }
                if self.mem.small_fam(p + 5) != 0
                    || self.mem.small_char(p + 5) != 0
                    || self.mem.large_fam(p + 5) != 0
                    || self.mem.large_char(p + 5) != 0
                {
                    self.print_chars(", right-delimiter ");
                    self.print_delimiter(p + 5);
                }
                self.print_subsidiary_data(p + 2, '\\' as i32); // numerator
                self.print_subsidiary_data(p + 3, '/' as i32); // denominator
            }
            _ => {
                // §696: a normal noad.
                match self.mem.node_type(p) {
                    ORD_NOAD => self.print_esc_str("mathord"),
                    OP_NOAD => self.print_esc_str("mathop"),
                    BIN_NOAD => self.print_esc_str("mathbin"),
                    REL_NOAD => self.print_esc_str("mathrel"),
                    OPEN_NOAD => self.print_esc_str("mathopen"),
                    CLOSE_NOAD => self.print_esc_str("mathclose"),
                    PUNCT_NOAD => self.print_esc_str("mathpunct"),
                    INNER_NOAD => self.print_esc_str("mathinner"),
                    OVER_NOAD => self.print_esc_str("overline"),
                    UNDER_NOAD => self.print_esc_str("underline"),
                    VCENTER_NOAD => self.print_esc_str("vcenter"),
                    RADICAL_NOAD => {
                        self.print_esc_str("radical");
                        self.print_delimiter(p + 4);
                    }
                    ACCENT_NOAD => {
                        self.print_esc_str("accent");
                        self.print_fam_and_char(p + 4);
                    }
                    LEFT_NOAD => {
                        self.print_esc_str("left");
                        self.print_delimiter(p + 1);
                    }
                    RIGHT_NOAD => {
                        // etex.ch §696: subtype middle_noad displays \middle.
                        if self.mem.subtype(p) == NORMAL {
                            self.print_esc_str("right");
                        } else {
                            self.print_esc_str("middle");
                        }
                        self.print_delimiter(p + 1);
                    }
                    _ => {}
                }
                if self.mem.node_type(p) < LEFT_NOAD {
                    if self.mem.subtype(p) != NORMAL {
                        if self.mem.subtype(p) == LIMITS {
                            self.print_esc_str("limits");
                        } else {
                            self.print_esc_str("nolimits");
                        }
                    }
                    self.print_subsidiary_data(p + 1, '.' as i32);
                }
                self.print_subsidiary_data(p + 2, '^' as i32);
                self.print_subsidiary_data(p + 3, '_' as i32);
            }
        }
    }

    /// §698: the mlist cases of `flush_node_list`. Returns the node size
    /// freed, after flushing subsidiaries (the caller must not free again).
    pub fn flush_math_node(&mut self, p: Pointer) {
        match self.mem.node_type(p) {
            STYLE_NODE => self.mem.free_node(p, STYLE_NODE_SIZE),
            CHOICE_NODE => {
                let d = self.mem.info(p + 1);
                self.flush_node_list(d);
                let t = self.mem.link(p + 1);
                self.flush_node_list(t);
                let s = self.mem.info(p + 2);
                self.flush_node_list(s);
                let ss = self.mem.link(p + 2);
                self.flush_node_list(ss);
                self.mem.free_node(p, STYLE_NODE_SIZE);
            }
            LEFT_NOAD | RIGHT_NOAD => self.mem.free_node(p, NOAD_SIZE),
            FRACTION_NOAD => {
                let n = self.mem.info(p + 2);
                self.flush_node_list(n);
                let d = self.mem.info(p + 3);
                self.flush_node_list(d);
                self.mem.free_node(p, FRACTION_NOAD_SIZE);
            }
            _ => {
                for field in 1..=3 {
                    if self.mem.math_type(p + field) >= SUB_BOX {
                        let q = self.mem.info(p + field);
                        self.flush_node_list(q);
                    }
                }
                let s = match self.mem.node_type(p) {
                    RADICAL_NOAD => RADICAL_NOAD_SIZE,
                    ACCENT_NOAD => ACCENT_NOAD_SIZE,
                    _ => NOAD_SIZE,
                };
                self.mem.free_node(p, s);
            }
        }
    }

    // ----- Part 35: subroutines for math mode. -----

    /// `fraction_rule(t)` (§704).
    fn fraction_rule(&mut self, t: Scaled) -> TexResult<Pointer> {
        let p = self.new_rule()?;
        self.mem.set_height(p, t);
        self.mem.set_depth(p, 0);
        Ok(p)
    }

    /// `overbar(b, k, t)` (§705).
    fn overbar(&mut self, b: Pointer, k: Scaled, t: Scaled) -> TexResult<Pointer> {
        let p = self.new_kern(k)?;
        self.mem.set_link(p, b);
        let q = self.fraction_rule(t)?;
        self.mem.set_link(q, p);
        let p = self.new_kern(t)?;
        self.mem.set_link(p, q);
        self.vpack(p, 0, ADDITIONAL)
    }

    /// `char_box(f, c)` (§709).
    fn char_box(&mut self, f: i32, c: i32) -> TexResult<Pointer> {
        let q = self.fonts.char_info(f, c);
        let hd = FontMem::height_depth(q);
        let b = self.new_null_box()?;
        let w = self.fonts.char_width(f, q) + self.fonts.char_italic(f, q);
        self.mem.set_width(b, w);
        let h = self.fonts.char_height(f, hd);
        self.mem.set_height(b, h);
        let d = self.fonts.char_depth(f, hd);
        self.mem.set_depth(b, d);
        let p = self.mem.get_avail()?;
        self.mem.set_character(p, c as u16);
        self.mem.set_font(p, f as u16);
        self.mem.set_list_ptr(b, p);
        Ok(b)
    }

    /// `stack_into_box(b, f, c)` (§711).
    fn stack_into_box(&mut self, b: Pointer, f: i32, c: i32) -> TexResult<()> {
        let p = self.char_box(f, c)?;
        let l = self.mem.list_ptr(b);
        self.mem.set_link(p, l);
        self.mem.set_list_ptr(b, p);
        let h = self.mem.height(p);
        self.mem.set_height(b, h);
        Ok(())
    }

    /// `height_plus_depth(f, c)` (§712).
    fn height_plus_depth(&self, f: i32, c: i32) -> Scaled {
        let q = self.fonts.char_info(f, c);
        let hd = FontMem::height_depth(q);
        self.fonts.char_height(f, hd) + self.fonts.char_depth(f, hd)
    }

    /// `var_delimiter(d, s, v)` (§706-§713).
    pub fn var_delimiter(&mut self, d: Pointer, s: i32, v: Scaled) -> TexResult<Pointer> {
        let mut f = NULL_FONT;
        let mut c: i32 = 0;
        let mut w: Scaled = 0;
        let mut large_attempt = false;
        let mut z = self.mem.small_fam(d);
        let mut x = self.mem.small_char(d);
        'found: loop {
            // §707: look at the variants of (z, x).
            if z != 0 || x != 0 {
                let mut zz = z + s + 16;
                loop {
                    zz -= 16;
                    let g = self.fam_fnt(zz);
                    if g != NULL_FONT {
                        // §708: the list of characters starting with x.
                        let mut y = x;
                        if y >= self.fonts.bc[g as usize] && y <= self.fonts.ec[g as usize] {
                            loop {
                                // continue:
                                let q = self.fonts.char_info(g, y);
                                if !FontMem::char_exists(q) {
                                    break;
                                }
                                if FontMem::char_tag(q) == EXT_TAG {
                                    f = g;
                                    c = y;
                                    break 'found;
                                }
                                let hd = FontMem::height_depth(q);
                                let u =
                                    self.fonts.char_height(g, hd) + self.fonts.char_depth(g, hd);
                                if u > w {
                                    f = g;
                                    c = y;
                                    w = u;
                                    if u >= v {
                                        break 'found;
                                    }
                                }
                                if FontMem::char_tag(q) == LIST_TAG {
                                    y = i32::from(FontMem::rem_byte(q));
                                    continue;
                                }
                                break;
                            }
                        }
                    }
                    if zz < 16 {
                        break;
                    }
                }
            }
            if large_attempt {
                break 'found; // there were none large enough
            }
            large_attempt = true;
            z = self.mem.large_fam(d);
            x = self.mem.large_char(d);
        }
        let b = if f != NULL_FONT {
            // §710: make b point to a box for (f, c).
            let q = self.fonts.char_info(f, c);
            if FontMem::char_tag(q) == EXT_TAG {
                // §713: construct an extensible character.
                let b = self.new_null_box()?;
                self.mem.set_node_type(b, VLIST_NODE);
                let r = self.fonts.info[(self.fonts.exten_base[f as usize]
                    + i32::from(FontMem::rem_byte(q)))
                    as usize];
                // §714: compute the minimum suitable height w.
                let rep = i32::from(r.qqqq(3)); // ext_rep
                let u = self.height_plus_depth(f, rep);
                let mut w: Scaled = 0;
                let qi = self.fonts.char_info(f, rep);
                let bw = self.fonts.char_width(f, qi) + self.fonts.char_italic(f, qi);
                self.mem.set_width(b, bw);
                let bot = i32::from(r.qqqq(2)); // ext_bot
                if bot != 0 {
                    w += self.height_plus_depth(f, bot);
                }
                let mid = i32::from(r.qqqq(1)); // ext_mid
                if mid != 0 {
                    w += self.height_plus_depth(f, mid);
                }
                let top = i32::from(r.qqqq(0)); // ext_top
                if top != 0 {
                    w += self.height_plus_depth(f, top);
                }
                let mut n = 0;
                if u > 0 {
                    while w < v {
                        w += u;
                        n += 1;
                        if mid != 0 {
                            w += u;
                        }
                    }
                }
                if bot != 0 {
                    self.stack_into_box(b, f, bot)?;
                }
                for _ in 1..=n {
                    self.stack_into_box(b, f, rep)?;
                }
                if mid != 0 {
                    self.stack_into_box(b, f, mid)?;
                    for _ in 1..=n {
                        self.stack_into_box(b, f, rep)?;
                    }
                }
                if top != 0 {
                    self.stack_into_box(b, f, top)?;
                }
                let d2 = w - self.mem.height(b);
                self.mem.set_depth(b, d2);
                b
            } else {
                self.char_box(f, c)?
            }
        } else {
            let b = self.new_null_box()?;
            let nds = self.eqtb.dimen_par(NULL_DELIMITER_SPACE_CODE);
            self.mem.set_width(b, nds); // use this width if no delimiter was found
            b
        };
        let sa = half(self.mem.height(b) - self.mem.depth(b)) - self.axis_height(s);
        self.mem.set_shift_amount(b, sa);
        Ok(b)
    }

    /// `rebox(b, w)` (§715).
    pub fn rebox(&mut self, b: Pointer, w: Scaled) -> TexResult<Pointer> {
        if self.mem.width(b) != w && self.mem.list_ptr(b) != NULL {
            let mut b = b;
            if self.mem.node_type(b) == VLIST_NODE {
                b = self.hpack(b, 0, ADDITIONAL)?;
            }
            let p = self.mem.list_ptr(b);
            if self.mem.is_char_node(p) && self.mem.link(p) == NULL {
                let f = i32::from(self.mem.font(p));
                let ci = self.fonts.char_info(f, i32::from(self.mem.character(p)));
                let v = self.fonts.char_width(f, ci);
                if v != self.mem.width(b) {
                    let k = self.new_kern(self.mem.width(b) - v)?;
                    self.mem.set_link(p, k);
                }
            }
            self.mem.free_node(b, BOX_NODE_SIZE);
            let ss = self.mem.ss_glue();
            let b = self.new_glue(ss)?;
            self.mem.set_link(b, p);
            let mut p = p;
            while self.mem.link(p) != NULL {
                p = self.mem.link(p);
            }
            let g = self.new_glue(ss)?;
            self.mem.set_link(p, g);
            self.hpack(b, w, EXACTLY)
        } else {
            self.mem.set_width(b, w);
            Ok(b)
        }
    }

    /// `mu_mult(x)` (§716): `nx_plus_y(n, x, xn_over_d(x, f, 2^16))`.
    fn mu_mult(&mut self, x: Scaled, n: i32, f: Scaled) -> TexResult<Scaled> {
        let xn = crate::arith::xn_over_d(&mut self.arith, x, f, 0o200000);
        Ok(crate::arith::nx_plus_y(&mut self.arith, n, x, xn))
    }

    /// `math_glue(g, m)` (§716).
    pub fn math_glue(&mut self, g: Pointer, m: Scaled) -> TexResult<Pointer> {
        let mut n = crate::arith::x_over_n(&mut self.arith, m, 0o200000);
        let mut f = self.arith.remainder;
        if f < 0 {
            n -= 1;
            f += 0o200000;
        }
        let p = self.mem.get_node(crate::mem::GLUE_SPEC_SIZE)?;
        let w = self.mu_mult(self.mem.width(g), n, f)?; // convert mu to pt
        self.mem.set_width(p, w);
        let so = self.mem.stretch_order(g);
        self.mem.set_stretch_order(p, so);
        if so == crate::mem::NORMAL {
            let s = self.mu_mult(self.mem.stretch(g), n, f)?;
            self.mem.set_stretch(p, s);
        } else {
            let s = self.mem.stretch(g);
            self.mem.set_stretch(p, s);
        }
        let sho = self.mem.shrink_order(g);
        self.mem.set_shrink_order(p, sho);
        if sho == crate::mem::NORMAL {
            let s = self.mu_mult(self.mem.shrink(g), n, f)?;
            self.mem.set_shrink(p, s);
        } else {
            let s = self.mem.shrink(g);
            self.mem.set_shrink(p, s);
        }
        Ok(p)
    }

    /// `math_kern(p, m)` (§717).
    pub fn math_kern(&mut self, p: Pointer, m: Scaled) -> TexResult<()> {
        if self.mem.subtype(p) == MU_GLUE {
            let mut n = crate::arith::x_over_n(&mut self.arith, m, 0o200000);
            let mut f = self.arith.remainder;
            if f < 0 {
                n -= 1;
                f += 0o200000;
            }
            let w = self.mu_mult(self.mem.width(p), n, f)?;
            self.mem.set_width(p, w);
            self.mem.set_subtype(p, EXPLICIT);
        }
        Ok(())
    }

    /// `flush_math` (§718).
    pub fn flush_math(&mut self) {
        let h = self.nest.cur.head;
        let l = self.mem.link(h);
        self.flush_node_list(l);
        let inc = self.nest.cur.aux.int();
        self.flush_node_list(inc);
        self.mem.set_link(h, NULL);
        self.nest.cur.tail = h;
        self.nest.cur.aux.set_int(NULL); // incompleat_noad
    }

    // ----- Part 36: mlist_to_hlist and friends. -----

    /// `clean_box(p, s)` (§720-§721): box a noad field in style `s`.
    fn clean_box(&mut self, p: Pointer, s: u16) -> TexResult<Pointer> {
        let q: Pointer;
        'found: {
            match self.mem.math_type(p) {
                MATH_CHAR => {
                    let n = self.new_noad()?;
                    *self.mem.word_mut(n + 1) = self.mem.word(p);
                    self.cur_mlist = n;
                }
                SUB_BOX => {
                    q = self.mem.info(p);
                    break 'found;
                }
                SUB_MLIST => {
                    self.cur_mlist = self.mem.info(p);
                }
                _ => {
                    q = self.new_null_box()?;
                    break 'found;
                }
            }
            let save_style = self.cur_style;
            self.cur_style = s;
            self.mlist_penalties = false;
            self.mlist_to_hlist()?; // recursive call
            q = self.mem.link(self.mem.temp_head());
            self.cur_style = save_style; // restore the style
            self.set_cur_size_and_mu();
        }
        let x = if self.mem.is_char_node(q) || q == NULL {
            self.hpack(q, 0, ADDITIONAL)?
        } else if self.mem.link(q) == NULL
            && self.mem.node_type(q) <= VLIST_NODE
            && self.mem.shift_amount(q) == 0
        {
            q // it's already clean
        } else {
            self.hpack(q, 0, ADDITIONAL)?
        };
        // §721: simplify a trivial box.
        let q = self.mem.list_ptr(x);
        if self.mem.is_char_node(q) {
            let r = self.mem.link(q);
            if r != NULL
                && self.mem.link(r) == NULL
                && !self.mem.is_char_node(r)
                && self.mem.node_type(r) == KERN_NODE
            {
                // unneeded italic correction
                self.mem.free_node(r, SMALL_NODE_SIZE);
                self.mem.set_link(q, NULL);
            }
        }
        Ok(x)
    }

    /// `fetch(a)` (§722): unpack a `math_char` field into `cur_f`,
    /// `cur_c`, `cur_i`.
    fn fetch(&mut self, a: Pointer) -> TexResult<()> {
        self.cur_c = i32::from(self.mem.character(a));
        self.cur_f = self.fam_fnt(i32::from(self.mem.font(a)) + self.cur_size);
        if self.cur_f == NULL_FONT {
            // §723: complain about an undefined family.
            self.print_err("");
            let s = self.cur_size;
            self.print_size(s);
            self.print_char(' ' as i32);
            let fam = i32::from(self.mem.font(a));
            self.print_int(fam);
            self.print_chars(" is undefined (character ");
            let c = self.cur_c;
            self.print_ascii(c);
            self.print_char(')' as i32);
            self.help(&[
                "Somewhere in the math formula just ended, you used the",
                "stated character from an undefined font family. For example,",
                "plain TeX doesn't allow \\it or \\sl in subscripts. Proceed,",
                "and I'll try to forget that I needed that character.",
            ]);
            self.error()?;
            self.cur_i = MemoryWord::ZERO; // null_character
            self.mem.set_math_type(a, EMPTY);
        } else {
            if self.cur_c >= self.fonts.bc[self.cur_f as usize]
                && self.cur_c <= self.fonts.ec[self.cur_f as usize]
            {
                self.cur_i = self.fonts.char_info(self.cur_f, self.cur_c);
            } else {
                self.cur_i = MemoryWord::ZERO;
            }
            if !FontMem::char_exists(self.cur_i) {
                let (f, c) = (self.cur_f, self.cur_c);
                self.char_warning(f, c);
                self.mem.set_math_type(a, EMPTY);
                self.cur_i = MemoryWord::ZERO;
            }
        }
        Ok(())
    }

    /// `print_size(s)` (§699).
    pub fn print_size(&mut self, s: i32) {
        if s == TEXT_SIZE {
            self.print_esc_str("textfont");
        } else if s == SCRIPT_SIZE {
            self.print_esc_str("scriptfont");
        } else {
            self.print_esc_str("scriptscriptfont");
        }
    }

    /// `mlist_to_hlist` (§726-§767). Implicit parameters: `cur_mlist`,
    /// `cur_style`, `mlist_penalties`; the result is `link(temp_head)`.
    pub fn mlist_to_hlist(&mut self) -> TexResult<()> {
        let mlist = self.cur_mlist;
        let penalties = self.mlist_penalties;
        let style = self.cur_style; // tuck global parameters away
        let mut q = mlist;
        let mut r: Pointer = NULL;
        let mut r_type: u16 = OP_NOAD;
        let mut max_h: Scaled = 0;
        let mut max_d: Scaled = 0;
        self.set_cur_size_and_mu();
        while q != NULL {
            // §727: process node-or-noad q.
            'done_with_node: {
                'done_with_noad: {
                    'check_dimensions: {
                        // §728: first-pass processing based on type(q).
                        let mut delta: Scaled;
                        'reswitch: loop {
                            delta = 0;
                            match self.mem.node_type(q) {
                                BIN_NOAD => {
                                    if matches!(
                                        r_type,
                                        BIN_NOAD
                                            | OP_NOAD
                                            | REL_NOAD
                                            | OPEN_NOAD
                                            | PUNCT_NOAD
                                            | LEFT_NOAD
                                    ) {
                                        self.mem.set_node_type(q, ORD_NOAD);
                                        continue 'reswitch;
                                    }
                                }
                                REL_NOAD | CLOSE_NOAD | PUNCT_NOAD | RIGHT_NOAD => {
                                    // §729: convert a final bin_noad.
                                    if r_type == BIN_NOAD {
                                        self.mem.set_node_type(r, ORD_NOAD);
                                    }
                                    if self.mem.node_type(q) == RIGHT_NOAD {
                                        break 'done_with_noad;
                                    }
                                }
                                // §733: cases for noads following a bin_noad.
                                LEFT_NOAD => break 'done_with_noad,
                                FRACTION_NOAD => {
                                    self.make_fraction(q)?;
                                    break 'check_dimensions;
                                }
                                OP_NOAD => {
                                    delta = self.make_op(q)?;
                                    if self.mem.subtype(q) == LIMITS {
                                        break 'check_dimensions;
                                    }
                                }
                                ORD_NOAD => self.make_ord(q)?,
                                OPEN_NOAD | INNER_NOAD => {}
                                RADICAL_NOAD => self.make_radical(q)?,
                                OVER_NOAD => self.make_over(q)?,
                                UNDER_NOAD => self.make_under(q)?,
                                ACCENT_NOAD => self.make_math_accent(q)?,
                                VCENTER_NOAD => self.make_vcenter(q)?,
                                // §730: cases for nodes in an mlist.
                                STYLE_NODE => {
                                    let s = self.mem.subtype(q);
                                    self.cur_style = s;
                                    self.set_cur_size_and_mu();
                                    break 'done_with_node;
                                }
                                CHOICE_NODE => {
                                    // §731.
                                    let p = match self.cur_style / 2 {
                                        0 => {
                                            let p = self.mem.info(q + 1);
                                            self.mem.set_info(q + 1, NULL);
                                            p
                                        }
                                        1 => {
                                            let p = self.mem.link(q + 1);
                                            self.mem.set_link(q + 1, NULL);
                                            p
                                        }
                                        2 => {
                                            let p = self.mem.info(q + 2);
                                            self.mem.set_info(q + 2, NULL);
                                            p
                                        }
                                        _ => {
                                            let p = self.mem.link(q + 2);
                                            self.mem.set_link(q + 2, NULL);
                                            p
                                        }
                                    };
                                    let d = self.mem.info(q + 1);
                                    self.flush_node_list(d);
                                    let t = self.mem.link(q + 1);
                                    self.flush_node_list(t);
                                    let s = self.mem.info(q + 2);
                                    self.flush_node_list(s);
                                    let ss = self.mem.link(q + 2);
                                    self.flush_node_list(ss);
                                    self.mem.set_node_type(q, STYLE_NODE);
                                    let cs = self.cur_style;
                                    self.mem.set_subtype(q, cs);
                                    self.mem.set_width(q, 0);
                                    self.mem.set_depth(q, 0);
                                    if p != NULL {
                                        let z = self.mem.link(q);
                                        self.mem.set_link(q, p);
                                        let mut p = p;
                                        while self.mem.link(p) != NULL {
                                            p = self.mem.link(p);
                                        }
                                        self.mem.set_link(p, z);
                                    }
                                    break 'done_with_node;
                                }
                                INS_NODE | MARK_NODE | ADJUST_NODE | WHATSIT_NODE
                                | PENALTY_NODE | DISC_NODE => break 'done_with_node,
                                RULE_NODE => {
                                    if self.mem.height(q) > max_h {
                                        max_h = self.mem.height(q);
                                    }
                                    if self.mem.depth(q) > max_d {
                                        max_d = self.mem.depth(q);
                                    }
                                    break 'done_with_node;
                                }
                                GLUE_NODE => {
                                    // §732: convert math glue to ordinary glue.
                                    if self.mem.subtype(q) == MU_GLUE {
                                        let x = self.mem.glue_ptr(q);
                                        let m = self.cur_mu;
                                        let y = self.math_glue(x, m)?;
                                        self.mem.delete_glue_ref(x);
                                        self.mem.set_glue_ptr(q, y);
                                        self.mem.set_subtype(q, NORMAL);
                                    } else if self.cur_size != TEXT_SIZE
                                        && self.mem.subtype(q) == COND_MATH_GLUE
                                    {
                                        let p = self.mem.link(q);
                                        if p != NULL
                                            && (self.mem.node_type(p) == GLUE_NODE
                                                || self.mem.node_type(p) == KERN_NODE)
                                        {
                                            let lp = self.mem.link(p);
                                            self.mem.set_link(q, lp);
                                            self.mem.set_link(p, NULL);
                                            self.flush_node_list(p);
                                        }
                                    }
                                    break 'done_with_node;
                                }
                                KERN_NODE => {
                                    let m = self.cur_mu;
                                    self.math_kern(q, m)?;
                                    break 'done_with_node;
                                }
                                _ => return self.confusion("mlist1"),
                            }
                            break;
                        }
                        // §754: convert nucleus(q) to an hlist and attach
                        // the sub/superscripts.
                        let p = match self.mem.math_type(q + 1) {
                            MATH_CHAR | MATH_TEXT_CHAR => {
                                // §755.
                                self.fetch(q + 1)?;
                                if FontMem::char_exists(self.cur_i) {
                                    delta = self.fonts.char_italic(self.cur_f, self.cur_i);
                                    let (f, c) = (self.cur_f, self.cur_c);
                                    let p = self.new_character(f, c)?;
                                    if self.mem.math_type(q + 1) == MATH_TEXT_CHAR
                                        && self.fonts.space(f) != 0
                                    {
                                        delta = 0; // no italic correction mid-word
                                    }
                                    if self.mem.math_type(q + 3) == EMPTY && delta != 0 {
                                        let k = self.new_kern(delta)?;
                                        self.mem.set_link(p, k);
                                        delta = 0;
                                    }
                                    p
                                } else {
                                    NULL
                                }
                            }
                            EMPTY => NULL,
                            SUB_BOX => self.mem.info(q + 1),
                            SUB_MLIST => {
                                self.cur_mlist = self.mem.info(q + 1);
                                let save_style = self.cur_style;
                                self.mlist_penalties = false;
                                self.mlist_to_hlist()?; // recursive call
                                self.cur_style = save_style;
                                self.set_cur_size_and_mu();
                                let th = self.mem.temp_head();
                                let l = self.mem.link(th);
                                self.hpack(l, 0, ADDITIONAL)?
                            }
                            _ => return self.confusion("mlist2"),
                        };
                        self.mem.set_new_hlist(q, p);
                        if self.mem.math_type(q + 3) == EMPTY && self.mem.math_type(q + 2) == EMPTY
                        {
                            break 'check_dimensions;
                        }
                        self.make_scripts(q, delta)?;
                    }
                    // check_dimensions:
                    let nh = self.mem.new_hlist(q);
                    let z = self.hpack(nh, 0, ADDITIONAL)?;
                    if self.mem.height(z) > max_h {
                        max_h = self.mem.height(z);
                    }
                    if self.mem.depth(z) > max_d {
                        max_d = self.mem.depth(z);
                    }
                    self.mem.free_node(z, BOX_NODE_SIZE);
                }
                // done_with_noad:
                r = q;
                r_type = self.mem.node_type(r);
                // etex.ch §727: a \middle noad (a mid-list right noad)
                // restores the outer style for what follows.
                if r_type == RIGHT_NOAD {
                    r_type = LEFT_NOAD;
                    self.cur_style = style;
                    self.set_cur_size_and_mu();
                }
            }
            // done_with_node:
            q = self.mem.link(q);
        }
        // §729: convert a final bin_noad to an ord_noad.
        if r_type == BIN_NOAD {
            self.mem.set_node_type(r, ORD_NOAD);
        }
        // §760: second pass — remove noads, insert spacing and penalties.
        let mut p = self.mem.temp_head();
        self.mem.set_link(p, NULL);
        let mut q = mlist;
        let mut r_type: u16 = 0;
        self.cur_style = style;
        self.set_cur_size_and_mu();
        while q != NULL {
            // §761: defaults.
            let mut t = ORD_NOAD;
            let mut s = NOAD_SIZE;
            let mut pen = INF_PENALTY;
            let mut to_delete_q = false;
            match self.mem.node_type(q) {
                OP_NOAD | OPEN_NOAD | CLOSE_NOAD | PUNCT_NOAD | INNER_NOAD => {
                    t = self.mem.node_type(q);
                }
                BIN_NOAD => {
                    t = BIN_NOAD;
                    pen = self.eqtb.int_par(BIN_OP_PENALTY_CODE);
                }
                REL_NOAD => {
                    t = REL_NOAD;
                    pen = self.eqtb.int_par(REL_PENALTY_CODE);
                }
                ORD_NOAD | VCENTER_NOAD | OVER_NOAD | UNDER_NOAD => {}
                RADICAL_NOAD => s = RADICAL_NOAD_SIZE,
                ACCENT_NOAD => s = ACCENT_NOAD_SIZE,
                FRACTION_NOAD => s = FRACTION_NOAD_SIZE,
                LEFT_NOAD | RIGHT_NOAD => {
                    t = self.make_left_right(q, style, max_d, max_h)?;
                }
                STYLE_NODE => {
                    // §763: change the current style.
                    let st = self.mem.subtype(q);
                    self.cur_style = st;
                    s = STYLE_NODE_SIZE;
                    self.set_cur_size_and_mu();
                    to_delete_q = true;
                }
                WHATSIT_NODE | PENALTY_NODE | RULE_NODE | DISC_NODE | ADJUST_NODE | INS_NODE
                | MARK_NODE | GLUE_NODE | KERN_NODE => {
                    self.mem.set_link(p, q);
                    p = q;
                    q = self.mem.link(q);
                    self.mem.set_link(p, NULL);
                    continue; // goto done
                }
                _ => return self.confusion("mlist3"),
            }
            if !to_delete_q {
                // §766: append inter-element spacing based on r_type and t.
                if r_type > 0 {
                    let idx = (usize::from(r_type - ORD_NOAD)) * 8 + usize::from(t - ORD_NOAD);
                    let x = match MATH_SPACING[idx] {
                        b'0' => 0,
                        b'1' => {
                            if self.cur_style < SCRIPT_STYLE {
                                THIN_MU_SKIP_CODE
                            } else {
                                0
                            }
                        }
                        b'2' => THIN_MU_SKIP_CODE,
                        b'3' => {
                            if self.cur_style < SCRIPT_STYLE {
                                MED_MU_SKIP_CODE
                            } else {
                                0
                            }
                        }
                        b'4' => {
                            if self.cur_style < SCRIPT_STYLE {
                                THICK_MU_SKIP_CODE
                            } else {
                                0
                            }
                        }
                        _ => return self.confusion("mlist4"),
                    };
                    if x != 0 {
                        let g = self.eqtb.glue_par(x);
                        let m = self.cur_mu;
                        let y = self.math_glue(g, m)?;
                        let z = self.new_glue(y)?;
                        self.mem.set_glue_ref_count(y, NULL);
                        self.mem.set_link(p, z);
                        p = z;
                        self.mem.set_subtype(z, (x + 1) as u16); // symbolic subtype
                    }
                }
                // §767: append new_hlist entries and penalties.
                if self.mem.new_hlist(q) != NULL {
                    let nh = self.mem.new_hlist(q);
                    self.mem.set_link(p, nh);
                    loop {
                        p = self.mem.link(p);
                        if self.mem.link(p) == NULL {
                            break;
                        }
                    }
                }
                if penalties && self.mem.link(q) != NULL && pen < INF_PENALTY {
                    let nt = self.mem.node_type(self.mem.link(q));
                    if nt != PENALTY_NODE && nt != REL_NOAD {
                        let z = self.new_penalty(pen)?;
                        self.mem.set_link(p, z);
                        p = z;
                    }
                }
                // etex.ch §760: a right noad here can only be \middle
                // (plain ight ends its mlist); it acts as an Open atom
                // for the following spacing.
                if self.mem.node_type(q) == RIGHT_NOAD {
                    t = OPEN_NOAD;
                }
                r_type = t;
            }
            // delete_q:
            let r = q;
            q = self.mem.link(q);
            self.mem.free_node(r, s);
        }
        Ok(())
    }

    /// `make_over(q)` (§734).
    fn make_over(&mut self, q: Pointer) -> TexResult<()> {
        let cb = self.clean_box(q + 1, cramped_style(self.cur_style))?;
        let drt = self.default_rule_thickness();
        let b = self.overbar(cb, 3 * drt, drt)?;
        self.mem.set_info(q + 1, b);
        self.mem.set_math_type(q + 1, SUB_BOX);
        Ok(())
    }

    /// `make_under(q)` (§735).
    fn make_under(&mut self, q: Pointer) -> TexResult<()> {
        let x = self.clean_box(q + 1, self.cur_style)?;
        let drt = self.default_rule_thickness();
        let p = self.new_kern(3 * drt)?;
        self.mem.set_link(x, p);
        let fr = self.fraction_rule(drt)?;
        self.mem.set_link(p, fr);
        let y = self.vpack(x, 0, ADDITIONAL)?;
        let delta = self.mem.height(y) + self.mem.depth(y) + drt;
        let hx = self.mem.height(x);
        self.mem.set_height(y, hx);
        let d = delta - self.mem.height(y);
        self.mem.set_depth(y, d);
        self.mem.set_info(q + 1, y);
        self.mem.set_math_type(q + 1, SUB_BOX);
        Ok(())
    }

    /// `make_vcenter(q)` (§736).
    fn make_vcenter(&mut self, q: Pointer) -> TexResult<()> {
        let v = self.mem.info(q + 1);
        if self.mem.node_type(v) != VLIST_NODE {
            return self.confusion("vcenter");
        }
        let delta = self.mem.height(v) + self.mem.depth(v);
        let h = self.axis_height(self.cur_size) + half(delta);
        self.mem.set_height(v, h);
        let d = delta - h;
        self.mem.set_depth(v, d);
        Ok(())
    }

    /// `make_radical(q)` (§737).
    fn make_radical(&mut self, q: Pointer) -> TexResult<()> {
        let x = self.clean_box(q + 1, cramped_style(self.cur_style))?;
        let drt = self.default_rule_thickness();
        let mut clr = if self.cur_style < TEXT_STYLE {
            drt + self.math_x_height(self.cur_size).abs() / 4
        } else {
            drt + drt.abs() / 4
        };
        let v = self.mem.height(x) + self.mem.depth(x) + clr + drt;
        let cs = self.cur_size;
        let y = self.var_delimiter(q + 4, cs, v)?;
        let delta = self.mem.depth(y) - (self.mem.height(x) + self.mem.depth(x) + clr);
        if delta > 0 {
            clr += half(delta); // increase the actual clearance
        }
        let sa = -(self.mem.height(x) + clr);
        self.mem.set_shift_amount(y, sa);
        let hy = self.mem.height(y);
        let ob = self.overbar(x, clr, hy)?;
        self.mem.set_link(y, ob);
        let b = self.hpack(y, 0, ADDITIONAL)?;
        self.mem.set_info(q + 1, b);
        self.mem.set_math_type(q + 1, SUB_BOX);
        Ok(())
    }

    /// `make_math_accent(q)` (§738-§743).
    fn make_math_accent(&mut self, q: Pointer) -> TexResult<()> {
        self.fetch(q + 4)?; // accent_chr
        if !FontMem::char_exists(self.cur_i) {
            return Ok(());
        }
        let mut i = self.cur_i;
        let mut c = self.cur_c;
        let f = self.cur_f;
        // §741: compute the amount of skew.
        let mut s: Scaled = 0;
        if self.mem.math_type(q + 1) == MATH_CHAR {
            self.fetch(q + 1)?;
            if FontMem::char_tag(self.cur_i) == LIG_TAG {
                let mut a = self.fonts.lig_kern_start(self.cur_f, self.cur_i);
                self.cur_i = self.fonts.info[a as usize];
                if FontMem::skip_byte(self.cur_i) > STOP_FLAG {
                    a = self.fonts.lig_kern_restart(self.cur_f, self.cur_i);
                    self.cur_i = self.fonts.info[a as usize];
                }
                loop {
                    if i32::from(FontMem::next_char(self.cur_i))
                        == self.fonts.skew_char[self.cur_f as usize]
                    {
                        if FontMem::op_byte(self.cur_i) >= KERN_FLAG
                            && FontMem::skip_byte(self.cur_i) <= STOP_FLAG
                        {
                            s = self.fonts.char_kern(self.cur_f, self.cur_i);
                        }
                        break;
                    }
                    if FontMem::skip_byte(self.cur_i) >= STOP_FLAG {
                        break;
                    }
                    a += i32::from(FontMem::skip_byte(self.cur_i)) + 1;
                    self.cur_i = self.fonts.info[a as usize];
                }
            }
        }
        let mut x = self.clean_box(q + 1, cramped_style(self.cur_style))?;
        let w = self.mem.width(x);
        let mut h = self.mem.height(x);
        // §740: switch to a larger accent if available and appropriate.
        loop {
            if FontMem::char_tag(i) != LIST_TAG {
                break;
            }
            let y = i32::from(FontMem::rem_byte(i));
            i = self.fonts.char_info(f, y);
            if !FontMem::char_exists(i) {
                break;
            }
            if self.fonts.char_width(f, i) > w {
                break;
            }
            c = y;
        }
        let mut delta = if h < self.fonts.x_height(f) {
            h
        } else {
            self.fonts.x_height(f)
        };
        if (self.mem.math_type(q + 2) != EMPTY || self.mem.math_type(q + 3) != EMPTY)
            && self.mem.math_type(q + 1) == MATH_CHAR
        {
            // §742: swap the subscript and superscript into box x.
            self.flush_node_list(x);
            let nx = self.new_noad()?;
            *self.mem.word_mut(nx + 1) = self.mem.word(q + 1);
            *self.mem.word_mut(nx + 2) = self.mem.word(q + 2);
            *self.mem.word_mut(nx + 3) = self.mem.word(q + 3);
            *self.mem.word_mut(q + 2) = MemoryWord::ZERO;
            *self.mem.word_mut(q + 3) = MemoryWord::ZERO;
            self.mem.set_math_type(q + 1, SUB_MLIST);
            self.mem.set_info(q + 1, nx);
            x = self.clean_box(q + 1, self.cur_style)?;
            delta += self.mem.height(x) - h;
            h = self.mem.height(x);
        }
        let y = self.char_box(f, c)?;
        let sa = s + half(w - self.mem.width(y));
        self.mem.set_shift_amount(y, sa);
        self.mem.set_width(y, 0);
        let p = self.new_kern(-delta)?;
        self.mem.set_link(p, x);
        self.mem.set_link(y, p);
        let mut y = self.vpack(y, 0, ADDITIONAL)?;
        self.mem.set_width(y, self.mem.width(x));
        if self.mem.height(y) < h {
            // §739: make the height of box y equal to h.
            let p = self.new_kern(h - self.mem.height(y))?;
            let l = self.mem.list_ptr(y);
            self.mem.set_link(p, l);
            self.mem.set_list_ptr(y, p);
            self.mem.set_height(y, h);
        }
        let _ = &mut y;
        self.mem.set_info(q + 1, y);
        self.mem.set_math_type(q + 1, SUB_BOX);
        Ok(())
    }

    /// `make_fraction(q)` (§743-§748).
    fn make_fraction(&mut self, q: Pointer) -> TexResult<()> {
        if self.mem.width(q) == DEFAULT_CODE {
            let drt = self.default_rule_thickness();
            self.mem.set_width(q, drt); // thickness
        }
        // §744: create equal-width boxes x and z.
        let mut x = self.clean_box(q + 2, num_style(self.cur_style))?;
        let mut z = self.clean_box(q + 3, denom_style(self.cur_style))?;
        if self.mem.width(x) < self.mem.width(z) {
            let w = self.mem.width(z);
            x = self.rebox(x, w)?;
        } else {
            let w = self.mem.width(x);
            z = self.rebox(z, w)?;
        }
        let mut shift_up;
        let mut shift_down;
        if self.cur_style < TEXT_STYLE {
            // display style
            shift_up = self.num1(self.cur_size);
            shift_down = self.denom1(self.cur_size);
        } else {
            shift_down = self.denom2(self.cur_size);
            if self.mem.width(q) != 0 {
                shift_up = self.num2(self.cur_size);
            } else {
                shift_up = self.num3(self.cur_size);
            }
        }
        let delta;
        if self.mem.width(q) == 0 {
            // §745: no fraction line.
            let clr = if self.cur_style < TEXT_STYLE {
                7 * self.default_rule_thickness()
            } else {
                3 * self.default_rule_thickness()
            };
            let d =
                half(clr - ((shift_up - self.mem.depth(x)) - (self.mem.height(z) - shift_down)));
            if d > 0 {
                shift_up += d;
                shift_down += d;
            }
            delta = 0; // (not used without a fraction line)
        } else {
            // §746: a fraction line.
            let clr = if self.cur_style < TEXT_STYLE {
                3 * self.mem.width(q)
            } else {
                self.mem.width(q)
            };
            delta = half(self.mem.width(q));
            let ax = self.axis_height(self.cur_size);
            let delta1 = clr - ((shift_up - self.mem.depth(x)) - (ax + delta));
            let delta2 = clr - ((ax - delta) - (self.mem.height(z) - shift_down));
            if delta1 > 0 {
                shift_up += delta1;
            }
            if delta2 > 0 {
                shift_down += delta2;
            }
        }
        // §747: construct a vlist box for the fraction.
        let v = self.new_null_box()?;
        self.mem.set_node_type(v, VLIST_NODE);
        let hv = shift_up + self.mem.height(x);
        self.mem.set_height(v, hv);
        let dv = self.mem.depth(z) + shift_down;
        self.mem.set_depth(v, dv);
        let wv = self.mem.width(x);
        self.mem.set_width(v, wv);
        let p;
        if self.mem.width(q) == 0 {
            p =
                self.new_kern((shift_up - self.mem.depth(x)) - (self.mem.height(z) - shift_down))?;
            self.mem.set_link(p, z);
        } else {
            let t = self.mem.width(q);
            let y = self.fraction_rule(t)?;
            let ax = self.axis_height(self.cur_size);
            let p2 = self.new_kern((ax - delta) - (self.mem.height(z) - shift_down))?;
            self.mem.set_link(y, p2);
            self.mem.set_link(p2, z);
            p = self.new_kern((shift_up - self.mem.depth(x)) - (ax + delta))?;
            self.mem.set_link(p, y);
        }
        self.mem.set_link(x, p);
        self.mem.set_list_ptr(v, x);
        // §748: put the fraction into a box with its delimiters.
        let delta = if self.cur_style < TEXT_STYLE {
            self.delim1(self.cur_size)
        } else {
            self.delim2(self.cur_size)
        };
        let cs = self.cur_size;
        let x = self.var_delimiter(q + 4, cs, delta)?;
        self.mem.set_link(x, v);
        let z = self.var_delimiter(q + 5, cs, delta)?;
        self.mem.set_link(v, z);
        let h = self.hpack(x, 0, ADDITIONAL)?;
        self.mem.set_new_hlist(q, h);
        Ok(())
    }

    /// `make_op(q)` (§749-§751): returns the sub/superscript offset.
    fn make_op(&mut self, q: Pointer) -> TexResult<Scaled> {
        if self.mem.subtype(q) == NORMAL && self.cur_style < TEXT_STYLE {
            self.mem.set_subtype(q, LIMITS);
        }
        let mut delta: Scaled = 0;
        if self.mem.math_type(q + 1) == MATH_CHAR {
            self.fetch(q + 1)?;
            if self.cur_style < TEXT_STYLE && FontMem::char_tag(self.cur_i) == LIST_TAG {
                // make it larger
                let c = i32::from(FontMem::rem_byte(self.cur_i));
                let i = self.fonts.char_info(self.cur_f, c);
                if FontMem::char_exists(i) {
                    self.cur_c = c;
                    self.cur_i = i;
                    self.mem.set_character(q + 1, c as u16);
                }
            }
            delta = self.fonts.char_italic(self.cur_f, self.cur_i);
            let x = self.clean_box(q + 1, self.cur_style)?;
            if self.mem.math_type(q + 3) != EMPTY && self.mem.subtype(q) != LIMITS {
                let w = self.mem.width(x) - delta; // remove italic correction
                self.mem.set_width(x, w);
            }
            let sa = half(self.mem.height(x) - self.mem.depth(x)) - self.axis_height(self.cur_size);
            self.mem.set_shift_amount(x, sa); // center vertically
            self.mem.set_math_type(q + 1, SUB_BOX);
            self.mem.set_info(q + 1, x);
        }
        if self.mem.subtype(q) == LIMITS {
            // §750-§751: construct a box with limits above and below.
            let x = self.clean_box(q + 2, sup_style(self.cur_style))?;
            let y = self.clean_box(q + 1, self.cur_style)?;
            let z = self.clean_box(q + 3, sub_style(self.cur_style))?;
            let v = self.new_null_box()?;
            self.mem.set_node_type(v, VLIST_NODE);
            let mut wv = self.mem.width(y);
            if self.mem.width(x) > wv {
                wv = self.mem.width(x);
            }
            if self.mem.width(z) > wv {
                wv = self.mem.width(z);
            }
            self.mem.set_width(v, wv);
            let x = self.rebox(x, wv)?;
            let y = self.rebox(y, wv)?;
            let z = self.rebox(z, wv)?;
            let sx = half(delta);
            self.mem.set_shift_amount(x, sx);
            self.mem.set_shift_amount(z, -sx);
            let hy = self.mem.height(y);
            self.mem.set_height(v, hy);
            let dy = self.mem.depth(y);
            self.mem.set_depth(v, dy);
            // §751: attach the limits.
            if self.mem.math_type(q + 2) == EMPTY {
                self.mem.free_node(x, BOX_NODE_SIZE);
                self.mem.set_list_ptr(v, y);
            } else {
                let mut shift_up = self.big_op_spacing3() - self.mem.depth(x);
                if shift_up < self.big_op_spacing1() {
                    shift_up = self.big_op_spacing1();
                }
                let p = self.new_kern(shift_up)?;
                self.mem.set_link(p, y);
                self.mem.set_link(x, p);
                let s5 = self.big_op_spacing5();
                let p = self.new_kern(s5)?;
                self.mem.set_link(p, x);
                self.mem.set_list_ptr(v, p);
                let h = self.mem.height(v) + s5 + self.mem.height(x) + self.mem.depth(x) + shift_up;
                self.mem.set_height(v, h);
            }
            if self.mem.math_type(q + 3) == EMPTY {
                self.mem.free_node(z, BOX_NODE_SIZE);
            } else {
                let mut shift_down = self.big_op_spacing4() - self.mem.height(z);
                if shift_down < self.big_op_spacing2() {
                    shift_down = self.big_op_spacing2();
                }
                let p = self.new_kern(shift_down)?;
                self.mem.set_link(y, p);
                self.mem.set_link(p, z);
                let s5 = self.big_op_spacing5();
                let p = self.new_kern(s5)?;
                self.mem.set_link(z, p);
                let d =
                    self.mem.depth(v) + s5 + self.mem.height(z) + self.mem.depth(z) + shift_down;
                self.mem.set_depth(v, d);
            }
            self.mem.set_new_hlist(q, v);
        }
        Ok(delta)
    }

    /// `make_ord(q)` (§752-§753): kerns and ligatures between noads.
    fn make_ord(&mut self, q: Pointer) -> TexResult<()> {
        'restart: loop {
            if self.mem.math_type(q + 3) != EMPTY
                || self.mem.math_type(q + 2) != EMPTY
                || self.mem.math_type(q + 1) != MATH_CHAR
            {
                return Ok(());
            }
            let p = self.mem.link(q);
            if p == NULL
                || self.mem.node_type(p) < ORD_NOAD
                || self.mem.node_type(p) > PUNCT_NOAD
                || self.mem.math_type(p + 1) != MATH_CHAR
                || self.mem.font(p + 1) != self.mem.font(q + 1)
            {
                return Ok(());
            }
            self.mem.set_math_type(q + 1, MATH_TEXT_CHAR);
            self.fetch(q + 1)?;
            if FontMem::char_tag(self.cur_i) != LIG_TAG {
                return Ok(());
            }
            let mut a = self.fonts.lig_kern_start(self.cur_f, self.cur_i);
            self.cur_c = i32::from(self.mem.character(p + 1));
            self.cur_i = self.fonts.info[a as usize];
            if FontMem::skip_byte(self.cur_i) > STOP_FLAG {
                a = self.fonts.lig_kern_restart(self.cur_f, self.cur_i);
                self.cur_i = self.fonts.info[a as usize];
            }
            loop {
                // §753: process instruction cur_i.
                if i32::from(FontMem::next_char(self.cur_i)) == self.cur_c
                    && FontMem::skip_byte(self.cur_i) <= STOP_FLAG
                {
                    if FontMem::op_byte(self.cur_i) >= KERN_FLAG {
                        let k = self.fonts.char_kern(self.cur_f, self.cur_i);
                        let kn = self.new_kern(k)?;
                        let lq = self.mem.link(q);
                        self.mem.set_link(kn, lq);
                        self.mem.set_link(q, kn);
                        return Ok(());
                    }
                    // a ligature.
                    match FontMem::op_byte(self.cur_i) {
                        1 | 5 => {
                            let c = FontMem::rem_byte(self.cur_i);
                            self.mem.set_character(q + 1, c); // =:| , =:|>
                        }
                        2 | 6 => {
                            let c = FontMem::rem_byte(self.cur_i);
                            self.mem.set_character(p + 1, c); // |=: , |=:>
                        }
                        3 | 7 | 11 => {
                            let r = self.new_noad()?; // |=:| , |=:|> , |=:|>>
                            let c = FontMem::rem_byte(self.cur_i);
                            self.mem.set_character(r + 1, c);
                            let fam = self.mem.font(q + 1);
                            self.mem.set_font(r + 1, fam);
                            self.mem.set_link(q, r);
                            self.mem.set_link(r, p);
                            if FontMem::op_byte(self.cur_i) < 11 {
                                self.mem.set_math_type(r + 1, MATH_CHAR);
                            } else {
                                self.mem.set_math_type(r + 1, MATH_TEXT_CHAR);
                            }
                        }
                        _ => {
                            let lp = self.mem.link(p);
                            self.mem.set_link(q, lp); // =:
                            let c = FontMem::rem_byte(self.cur_i);
                            self.mem.set_character(q + 1, c);
                            *self.mem.word_mut(q + 3) = self.mem.word(p + 3);
                            *self.mem.word_mut(q + 2) = self.mem.word(p + 2);
                            self.mem.free_node(p, NOAD_SIZE);
                        }
                    }
                    if FontMem::op_byte(self.cur_i) > 3 {
                        return Ok(());
                    }
                    self.mem.set_math_type(q + 1, MATH_CHAR);
                    continue 'restart;
                }
                if FontMem::skip_byte(self.cur_i) >= STOP_FLAG {
                    return Ok(());
                }
                a += i32::from(FontMem::skip_byte(self.cur_i)) + 1;
                self.cur_i = self.fonts.info[a as usize];
            }
        }
    }

    /// `make_scripts(q, delta)` (§756-§759).
    fn make_scripts(&mut self, q: Pointer, delta: Scaled) -> TexResult<()> {
        let p = self.mem.new_hlist(q);
        let mut shift_up: Scaled = 0;
        let mut shift_down: Scaled = 0;
        if !self.mem.is_char_node(p) {
            // (p may be NULL here; hpack of NULL gives an empty box.)
            let z = self.hpack(p, 0, ADDITIONAL)?;
            let t = if self.cur_style < SCRIPT_STYLE {
                SCRIPT_SIZE
            } else {
                SCRIPT_SCRIPT_SIZE
            };
            shift_up = self.mem.height(z) - self.sup_drop(t);
            shift_down = self.mem.depth(z) + self.sub_drop(t);
            self.mem.free_node(z, BOX_NODE_SIZE);
        }
        let x;
        if self.mem.math_type(q + 2) == EMPTY {
            // §757: subscript without superscript.
            x = self.clean_box(q + 3, sub_style(self.cur_style))?;
            let w = self.mem.width(x) + self.eqtb.dimen_par(SCRIPT_SPACE_CODE);
            self.mem.set_width(x, w);
            if shift_down < self.sub1(self.cur_size) {
                shift_down = self.sub1(self.cur_size);
            }
            let clr = self.mem.height(x) - (self.math_x_height(self.cur_size) * 4).abs() / 5;
            if shift_down < clr {
                shift_down = clr;
            }
            self.mem.set_shift_amount(x, shift_down);
        } else {
            // §758: construct a superscript box x.
            let mut xx = self.clean_box(q + 2, sup_style(self.cur_style))?;
            let w = self.mem.width(xx) + self.eqtb.dimen_par(SCRIPT_SPACE_CODE);
            self.mem.set_width(xx, w);
            let mut clr = if self.cur_style % 2 == 1 {
                self.sup3(self.cur_size)
            } else if self.cur_style < TEXT_STYLE {
                self.sup1(self.cur_size)
            } else {
                self.sup2(self.cur_size)
            };
            if shift_up < clr {
                shift_up = clr;
            }
            clr = self.mem.depth(xx) + self.math_x_height(self.cur_size).abs() / 4;
            if shift_up < clr {
                shift_up = clr;
            }
            if self.mem.math_type(q + 3) == EMPTY {
                self.mem.set_shift_amount(xx, -shift_up);
                x = xx;
            } else {
                // §759: sub/superscript combination box.
                let y = self.clean_box(q + 3, sub_style(self.cur_style))?;
                let w = self.mem.width(y) + self.eqtb.dimen_par(SCRIPT_SPACE_CODE);
                self.mem.set_width(y, w);
                if shift_down < self.sub2(self.cur_size) {
                    shift_down = self.sub2(self.cur_size);
                }
                let mut clr = 4 * self.default_rule_thickness()
                    - ((shift_up - self.mem.depth(xx)) - (self.mem.height(y) - shift_down));
                if clr > 0 {
                    shift_down += clr;
                    clr = (self.math_x_height(self.cur_size) * 4).abs() / 5
                        - (shift_up - self.mem.depth(xx));
                    if clr > 0 {
                        shift_up += clr;
                        shift_down -= clr;
                    }
                }
                self.mem.set_shift_amount(xx, delta); // superscript over subscript
                let k = self.new_kern(
                    (shift_up - self.mem.depth(xx)) - (self.mem.height(y) - shift_down),
                )?;
                self.mem.set_link(xx, k);
                self.mem.set_link(k, y);
                xx = self.vpack(xx, 0, ADDITIONAL)?;
                self.mem.set_shift_amount(xx, shift_down);
                x = xx;
            }
        }
        if self.mem.new_hlist(q) == NULL {
            self.mem.set_new_hlist(q, x);
        } else {
            let mut p = self.mem.new_hlist(q);
            while self.mem.link(p) != NULL {
                p = self.mem.link(p);
            }
            self.mem.set_link(p, x);
        }
        Ok(())
    }

    /// `make_left_right(q, style, max_d, max_h)` (§762).
    fn make_left_right(
        &mut self,
        q: Pointer,
        style: u16,
        max_d: Scaled,
        max_h: Scaled,
    ) -> TexResult<u16> {
        // etex.ch §762: set cur_size (and cur_mu) from the outer style.
        self.cur_style = style;
        self.set_cur_size_and_mu();
        let delta2 = max_d + self.axis_height(self.cur_size);
        let mut delta1 = max_h + max_d - delta2;
        if delta2 > delta1 {
            delta1 = delta2; // delta1 is max distance from axis
        }
        let mut delta = (delta1 / 500) * self.eqtb.int_par(DELIMITER_FACTOR_CODE);
        let delta2 = delta1 + delta1 - self.eqtb.dimen_par(DELIMITER_SHORTFALL_CODE);
        if delta < delta2 {
            delta = delta2;
        }
        let cs = self.cur_size;
        let b = self.var_delimiter(q + 1, cs, delta)?;
        self.mem.set_new_hlist(q, b);
        Ok(self.mem.node_type(q) - (LEFT_NOAD - OPEN_NOAD)) // open or close
    }
}
