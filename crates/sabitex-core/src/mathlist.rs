//! Building math lists.
//!
//! Ports tex.web Part 48 (§1136-§1206): entering and leaving math mode,
//! `scan_math`, subscripts and superscripts, fractions, `\left`/`\right`,
//! and the finishing of inline and displayed formulas.

use crate::cmds::*;
use crate::engine::{Engine, HMODE, MMODE, VMODE};
use crate::eqtb::*;
use crate::error::TexResult;
use crate::math::*;
use crate::memword::MemoryWord;
use crate::nest::IGNORE_DEPTH;
use crate::nodes::*;
use crate::pack::{ADDITIONAL, EXACTLY};
use crate::scan::MAX_DIMEN;
use crate::types::{Pointer, Scaled, NULL};

// §1178: generalized-fraction codes.
pub const ABOVE_CODE: i32 = 0;
pub const OVER_CODE: i32 = 1;
pub const ATOP_CODE: i32 = 2;
pub const DELIMITED_CODE: i32 = 3;

// xetex.web: math accent subtypes.
pub const FIXED_ACC: u16 = 1;
pub const BOTTOM_ACC: u16 = 2;

impl Engine {
    /// `push_math(c)` (§1136).
    pub fn push_math(&mut self, c: u16) -> TexResult<()> {
        self.push_nest()?;
        self.nest.cur.mode = -MMODE;
        self.nest.cur.aux.set_int(NULL); // incompleat_noad
        self.new_save_level(c)
    }

    /// etex.ch: the text direction before the display, from LR_save
    /// (the eTeX_aux of the enclosing list, written back by line_break).
    fn pre_display_dir_of_lr_save(&self) -> i32 {
        let lr_save = self.nest.cur.etex_aux;
        if lr_save == NULL {
            0
        } else if self.mem.info(lr_save) >= i32::from(crate::nodes::R_CODE) {
            -1
        } else {
            1
        }
    }

    /// `just_copy(p, h, t)` (etex.ch): copies the parts of the hlist `p`
    /// relevant for pre_display_size onto `h`, ending with `t`.
    fn just_copy(&mut self, p: Pointer, h: Pointer, t: Pointer) -> TexResult<()> {
        use crate::nodes::*;
        let mut p = p;
        let mut h = h;
        while p != NULL {
            let mut words = 1;
            let mut copy_words = true;
            let r;
            if self.mem.is_char_node(p) {
                r = self.mem.get_avail()?;
            } else {
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE => {
                        r = self.mem.get_node(BOX_NODE_SIZE)?;
                        *self.mem.word_mut(r + 6) = self.mem.word(p + 6);
                        *self.mem.word_mut(r + 5) = self.mem.word(p + 5);
                        words = 5;
                        self.mem.set_list_ptr(r, NULL); // this affects mem[r+5]
                    }
                    RULE_NODE => {
                        r = self.mem.get_node(crate::nodes::RULE_NODE_SIZE)?;
                        words = crate::nodes::RULE_NODE_SIZE as usize;
                    }
                    LIGATURE_NODE => {
                        r = self.mem.get_avail()?;
                        *self.mem.word_mut(r) = self.mem.word(p + 1); // lig_char
                        copy_words = false;
                    }
                    KERN_NODE | MATH_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        words = SMALL_NODE_SIZE as usize;
                    }
                    GLUE_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        let g = self.mem.glue_ptr(p);
                        self.mem.add_glue_ref(g);
                        self.mem.set_glue_ptr(r, g);
                        self.mem.set_leader_ptr(r, NULL);
                    }
                    WHATSIT_NODE => {
                        // §partial copy of the whatsit node (tex.web §1357).
                        use crate::ext::{OPEN_NODE_SIZE, WRITE_NODE_SIZE};
                        use crate::par::{
                            CLOSE_NODE, LANGUAGE_NODE, OPEN_NODE, SPECIAL_NODE, WRITE_NODE,
                        };
                        match self.mem.subtype(p) {
                            OPEN_NODE => {
                                r = self.mem.get_node(OPEN_NODE_SIZE)?;
                                words = OPEN_NODE_SIZE as usize;
                            }
                            WRITE_NODE | SPECIAL_NODE => {
                                r = self.mem.get_node(WRITE_NODE_SIZE)?;
                                let wt = self.mem.link(p + 1); // write_tokens
                                self.add_token_ref(wt);
                                words = WRITE_NODE_SIZE as usize;
                            }
                            CLOSE_NODE | LANGUAGE_NODE => {
                                r = self.mem.get_node(SMALL_NODE_SIZE)?;
                                words = SMALL_NODE_SIZE as usize;
                            }
                            _ => return self.confusion("ext2"),
                        }
                    }
                    _ => {
                        // not_found: irrelevant node, skip it.
                        p = self.mem.link(p);
                        continue;
                    }
                }
            }
            if copy_words {
                for k in (0..words as i32).rev() {
                    *self.mem.word_mut(r + k) = self.mem.word(p + k);
                }
            }
            // found:
            self.mem.set_link(h, r);
            h = r;
            // not_found:
            p = self.mem.link(p);
        }
        self.mem.set_link(h, t);
        Ok(())
    }

    /// `just_reverse(p)` (etex.ch): reverses an hlist segment for the
    /// natural-width computation; the reversed copy hangs off temp_head.
    fn just_reverse(&mut self, p: Pointer) -> TexResult<()> {
        use crate::nodes::*;
        let mut m: i32 = 0;
        let mut n: i32 = 0;
        let th = self.mem.temp_head();
        let mut q;
        if self.mem.link(th) == NULL {
            let lp = self.mem.link(p);
            self.just_copy(lp, th, NULL)?;
            q = self.mem.link(th);
        } else {
            q = self.mem.link(p);
            self.mem.set_link(p, NULL);
            let lth = self.mem.link(th);
            self.flush_node_list(lth);
        }
        let t = {
            let d = u16::from(self.cur_dir);
            let e = self.mem.get_node(crate::math::STYLE_NODE_SIZE)?;
            self.mem.set_node_type(e, crate::math::STYLE_NODE); // edge_node
            self.mem.set_subtype(e, d);
            self.mem.set_width(e, 0);
            self.mem.set_depth(e, 0);
            e
        };
        let mut l = t;
        self.cur_dir = 1 - self.cur_dir; // reflected
        let mut found = false;
        while q != NULL {
            if self.mem.is_char_node(q) {
                while self.mem.is_char_node(q) {
                    let pp = q;
                    q = self.mem.link(pp);
                    self.mem.set_link(pp, l);
                    l = pp;
                }
            } else {
                let pp = q;
                q = self.mem.link(pp);
                if self.mem.node_type(pp) == MATH_NODE {
                    // Adjust the LR stack for the just_reverse routine.
                    if end_lr(self.mem.subtype(pp)) {
                        if self.mem.info(self.lr_ptr)
                            != i32::from(end_lr_type(self.mem.subtype(pp)))
                        {
                            self.mem.set_node_type(pp, KERN_NODE);
                            self.lr_problems += 1;
                        } else {
                            self.pop_lr();
                            if n > 0 {
                                n -= 1;
                                let st = self.mem.subtype(pp);
                                self.mem.set_subtype(pp, st - 1);
                            } else if m > 0 {
                                m -= 1;
                                self.mem.set_node_type(pp, KERN_NODE);
                            } else {
                                // found: end of the segment to reverse.
                                let w = self.mem.width(pp);
                                self.mem.set_width(t, w);
                                self.mem.set_link(t, q);
                                self.mem.free_node(pp, SMALL_NODE_SIZE);
                                found = true;
                                break;
                            }
                        }
                    } else {
                        self.push_lr(pp)?;
                        if n > 0 || lr_dir(self.mem.subtype(pp)) != self.cur_dir {
                            n += 1;
                            let st = self.mem.subtype(pp);
                            self.mem.set_subtype(pp, st + 1);
                        } else {
                            self.mem.set_node_type(pp, KERN_NODE);
                            m += 1;
                        }
                    }
                }
                self.mem.set_link(pp, l);
                l = pp;
            }
        }
        let _ = found;
        self.mem.set_link(th, l);
        Ok(())
    }

    /// etex.ch: the prototype box for the display — an hlist node with
    /// the width, glue set, and shift amount of just_box, whose hlist
    /// reflects the current \leftskip and \rightskip.
    fn prototype_box(&mut self) -> TexResult<Pointer> {
        let zg = self.mem.zero_glue();
        let mut j;
        if self.eqtb.glue_par(RIGHT_SKIP_CODE) == zg {
            j = self.new_kern(0)?;
        } else {
            j = self.new_param_glue(RIGHT_SKIP_CODE)?;
        }
        let p = if self.eqtb.glue_par(LEFT_SKIP_CODE) == zg {
            self.new_kern(0)?
        } else {
            self.new_param_glue(LEFT_SKIP_CODE)?
        };
        self.mem.set_link(p, j);
        let jb = self.lb.just_box;
        j = self.new_null_box()?;
        let w = self.mem.width(jb);
        self.mem.set_width(j, w);
        let sa = self.mem.shift_amount(jb);
        self.mem.set_shift_amount(j, sa);
        self.mem.set_list_ptr(j, p);
        let go = self.mem.glue_order(jb);
        self.mem.set_glue_order(j, go);
        let gs = self.mem.glue_sign(jb);
        self.mem.set_glue_sign(j, gs);
        let g = self.mem.glue_set(jb);
        self.mem.set_glue_set(j, g);
        Ok(j)
    }

    /// `init_math` (§1138-§1146).
    pub fn init_math(&mut self) -> TexResult<()> {
        self.get_token()?; // get_x_token would fail on \ifmmode!
        if self.cur_cmd == MATH_SHIFT && self.mode() > 0 {
            // §1145 (+ etex.ch): go into display math mode.
            let mut j: Pointer = NULL;
            let mut w: Scaled = -MAX_DIMEN;
            let x: i32;
            if self.nest.cur.head == self.nest.cur.tail {
                // `\noindent$$` or `$${ }$$`
                self.pop_nest();
                x = self.pre_display_dir_of_lr_save();
            } else {
                self.line_break(true)?;
                // etex.ch: prepare for display after a non-empty paragraph.
                if self.etex_ex() {
                    j = self.prototype_box()?;
                }
                let (w2, x2) = self.natural_width_of_final_line(j)?;
                w = w2;
                x = x2;
            }
            // now we are in vertical mode, working on the list with the display
            // §1149: calculate the length l and shift amount s.
            let (l, s) = self.display_line_dimensions();
            self.push_math(MATH_SHIFT_GROUP)?;
            self.nest.cur.mode = MMODE;
            let lay = self.eqtb.lay.clone();
            self.eq_word_define(lay.int_base + CUR_FAM_CODE, -1)?;
            self.eq_word_define(lay.dimen_base + PRE_DISPLAY_SIZE_CODE, w)?;
            self.nest.cur.etex_aux = j; // LR_box
            if self.etex_ex() {
                self.eq_word_define(lay.int_base + crate::eqtb::PRE_DISPLAY_DIRECTION_CODE, x)?;
            }
            self.eq_word_define(lay.dimen_base + DISPLAY_WIDTH_CODE, l)?;
            self.eq_word_define(lay.dimen_base + DISPLAY_INDENT_CODE, s)?;
            let ed = self.eqtb.equiv(lay.local_base + EVERY_DISPLAY_OFFSET);
            if ed != NULL {
                self.begin_token_list(ed, 9)?; // every_display_text
            }
            if self.nest.ptr == 1 {
                self.build_page()?;
            }
        } else {
            self.back_input()?;
            self.go_into_ordinary_math()?;
        }
        Ok(())
    }

    /// §1139: go into ordinary math mode.
    fn go_into_ordinary_math(&mut self) -> TexResult<()> {
        self.push_math(MATH_SHIFT_GROUP)?;
        let loc = self.eqtb.lay.int_base + CUR_FAM_CODE;
        self.eq_word_define(loc, -1)?;
        let em = self
            .eqtb
            .equiv(self.eqtb.lay.local_base + EVERY_MATH_OFFSET);
        if em != NULL {
            self.begin_token_list(em, 8)?; // every_math_text
        }
        Ok(())
    }

    /// `start_eq_no` (§1142).
    pub fn start_eq_no(&mut self) -> TexResult<()> {
        let c = self.cur_chr;
        self.save.set_saved(0, c);
        self.save.save_ptr += 1;
        self.go_into_ordinary_math()
    }

    /// §1146-§1147 (+ etex.ch): the natural width `w` by which the final
    /// line of the interrupted paragraph extends right of the reference
    /// point, plus two ems (or `max_dimen` if affected by stretching or
    /// shrinking), and the pre-display direction `x`. When the final line
    /// ends with R-text, `w` refers to the line reflected with respect to
    /// the left edge of the enclosing vertical list.
    fn natural_width_of_final_line(&mut self, _j: Pointer) -> TexResult<(Scaled, i32)> {
        let jb = self.lb.just_box;
        let mut v = self.mem.shift_amount(jb);
        let x = self.pre_display_dir_of_lr_save();
        let th = self.mem.temp_head();
        let mut p;
        if x >= 0 {
            p = self.mem.list_ptr(jb);
            self.mem.set_link(th, NULL);
        } else {
            v = -v - self.mem.width(jb);
            p = self.new_math(0, crate::nodes::BEGIN_L_CODE)?;
            self.mem.set_link(th, p);
            let e = self.new_math(0, crate::nodes::END_L_CODE)?;
            let lp = self.mem.list_ptr(jb);
            self.just_copy(lp, p, e)?;
            self.cur_dir = crate::nodes::RIGHT_TO_LEFT;
        }
        v += 2 * self.fonts.quad(self.eqtb.cur_font());
        if self.texxet_en() {
            self.put_lr(i32::from(crate::nodes::BEFORE))?;
        }
        let mut w = -MAX_DIMEN;
        'done: while p != NULL {
            // §1147: let d be the natural width of node p.
            let found;
            let d: Scaled;
            let mut pp = p;
            'reswitch: loop {
                if self.mem.is_char_node(pp) {
                    let f = i32::from(self.mem.font(pp));
                    let ci = self.fonts.char_info(f, i32::from(self.mem.character(pp)));
                    d = self.fonts.char_width(f, ci);
                    found = true;
                    break 'reswitch;
                }
                match self.mem.node_type(pp) {
                    HLIST_NODE | VLIST_NODE | RULE_NODE => {
                        d = self.mem.width(pp);
                        found = true;
                    }
                    LIGATURE_NODE => {
                        // §652: make node pp look like a char_node.
                        let lt = self.mem.lig_trick();
                        *self.mem.word_mut(lt) = self.mem.word(pp + 1);
                        self.mem.set_link(lt, self.mem.link(pp));
                        pp = lt;
                        continue 'reswitch;
                    }
                    KERN_NODE => {
                        d = self.mem.width(pp);
                        found = false;
                    }
                    MATH_NODE => {
                        d = self.mem.width(pp);
                        if self.texxet_en() {
                            // etex.ch: adjust the LR stack for init_math.
                            use crate::nodes::{end_lr, end_lr_type, lr_dir};
                            if end_lr(self.mem.subtype(pp)) {
                                if self.mem.info(self.lr_ptr)
                                    == i32::from(end_lr_type(self.mem.subtype(pp)))
                                {
                                    self.pop_lr();
                                } else if self.mem.subtype(pp) > crate::nodes::L_CODE {
                                    w = MAX_DIMEN;
                                    break 'done;
                                }
                            } else {
                                self.push_lr(pp)?;
                                if lr_dir(self.mem.subtype(pp)) != self.cur_dir {
                                    self.just_reverse(pp)?;
                                    p = self.mem.temp_head();
                                }
                            }
                        } else if self.mem.subtype(pp) >= crate::nodes::L_CODE {
                            w = MAX_DIMEN;
                            break 'done;
                        }
                        found = false;
                    }
                    tp if tp == crate::math::STYLE_NODE => {
                        // etex.ch: an edge node changes the direction.
                        d = self.mem.width(pp);
                        self.cur_dir = self.mem.subtype(pp) as u8;
                        found = false;
                    }
                    GLUE_NODE => {
                        // §1148.
                        let q = self.mem.glue_ptr(pp);
                        d = self.mem.width(q);
                        if self.mem.glue_sign(jb) == STRETCHING {
                            if self.mem.glue_order(jb) == self.mem.stretch_order(q)
                                && self.mem.stretch(q) != 0
                            {
                                v = MAX_DIMEN;
                            }
                        } else if self.mem.glue_sign(jb) == SHRINKING
                            && self.mem.glue_order(jb) == self.mem.shrink_order(q)
                            && self.mem.shrink(q) != 0
                        {
                            v = MAX_DIMEN;
                        }
                        found = self.mem.subtype(pp) >= A_LEADERS;
                    }
                    WHATSIT_NODE => {
                        d = 0; // no whatsit has width in TeX82
                        found = false;
                    }
                    _ => {
                        d = 0;
                        found = false;
                    }
                }
                break;
            }
            if found {
                if v < MAX_DIMEN {
                    v += d;
                    w = v;
                } else {
                    w = MAX_DIMEN;
                    break 'done;
                }
            } else if v < MAX_DIMEN {
                v += d;
            }
            p = self.mem.link(p);
        }
        // etex.ch: finish the natural width computation.
        if self.texxet_en() {
            while self.lr_ptr != NULL {
                self.pop_lr();
            }
            if self.lr_problems != 0 {
                w = MAX_DIMEN;
                self.lr_problems = 0;
            }
        }
        self.cur_dir = crate::nodes::LEFT_TO_RIGHT;
        let lth = self.mem.link(th);
        self.flush_node_list(lth);
        self.mem.set_link(th, NULL);
        Ok((w, x))
    }

    /// §1149: the length and shift of display lines (line `prev_graf+2`).
    fn display_line_dimensions(&self) -> (Scaled, Scaled) {
        let ps = self.eqtb.equiv(self.eqtb.lay.par_shape_loc);
        let pg = self.nest.cur.pg;
        if ps == NULL {
            let hang_indent = self.eqtb.dimen_par(HANG_INDENT_CODE);
            let hang_after = self.eqtb.int_par(HANG_AFTER_CODE);
            if hang_indent != 0
                && ((hang_after >= 0 && pg + 2 > hang_after) || pg + 1 < -hang_after)
            {
                let l = self.eqtb.dimen_par(HSIZE_CODE) - hang_indent.abs();
                let s = if hang_indent > 0 { hang_indent } else { 0 };
                (l, s)
            } else {
                (self.eqtb.dimen_par(HSIZE_CODE), 0)
            }
        } else {
            let n = self.mem.info(ps);
            let p = if pg + 2 >= n {
                ps + 2 * n
            } else {
                ps + 2 * (pg + 2)
            };
            (self.mem.word(p).sc(), self.mem.word(p - 1).sc())
        }
    }

    /// `scan_math(p)` (§1151-§1153).
    pub fn scan_math(&mut self, p: Pointer) -> TexResult<()> {
        'restart: loop {
            self.get_next_nonblank_nonrelax_noncall()?;
            let c: i32;
            'reswitch: loop {
                match self.cur_cmd {
                    LETTER | OTHER_CHAR | CHAR_GIVEN => {
                        let mc = self.eqtb.math_code(self.cur_chr);
                        if crate::xemath::is_active_math_char(mc) {
                            // §1152: treat cur_chr as an active character.
                            self.treat_as_active()?;
                            continue 'restart;
                        }
                        c = mc;
                    }
                    CHAR_NUM => {
                        self.scan_char_num()?;
                        self.cur_chr = self.cur_val;
                        self.cur_cmd = CHAR_GIVEN;
                        continue 'reswitch;
                    }
                    MATH_CHAR_NUM => {
                        // xetex.web scan_math: chr 2 = \Umathchar,
                        // chr 1 = \Umathcharnum, chr 0 = classic.
                        if self.cur_chr == 2 {
                            self.scan_math_class_int()?;
                            let mut v = crate::xemath::set_class_field(self.cur_val);
                            self.scan_math_fam_int()?;
                            v += crate::xemath::set_family_field(self.cur_val);
                            self.scan_char_num()?;
                            c = v + self.cur_val;
                        } else if self.cur_chr == 1 {
                            self.scan_xetex_math_char_int()?;
                            c = self.cur_val;
                        } else {
                            self.scan_fifteen_bit_int()?;
                            c = crate::xemath::from_classic(self.cur_val);
                        }
                    }
                    MATH_GIVEN => {
                        c = crate::xemath::from_classic(self.cur_chr);
                    }
                    XETEX_MATH_GIVEN => {
                        c = self.cur_chr;
                    }
                    DELIM_NUM => {
                        if self.cur_chr == 1 {
                            // \Udelimiter <class> <fam> <usv>.
                            self.scan_math_class_int()?;
                            let mut v = crate::xemath::set_class_field(self.cur_val);
                            self.scan_math_fam_int()?;
                            v += crate::xemath::set_family_field(self.cur_val);
                            self.scan_char_num()?;
                            c = v + self.cur_val;
                        } else {
                            self.scan_twenty_seven_bit_int()?;
                            c = crate::xemath::from_classic(self.cur_val / 0o10000);
                        }
                    }
                    _ => {
                        // §1153: scan a subformula enclosed in braces.
                        self.back_input()?;
                        self.scan_left_brace()?;
                        self.save.set_saved(0, p);
                        self.save.save_ptr += 1;
                        self.push_math(MATH_GROUP)?;
                        return Ok(());
                    }
                }
                break;
            }
            self.mem.set_math_type(p, MATH_CHAR);
            let ch = crate::xemath::math_char_field(c);
            self.mem.set_character(p, (ch % 0x10000) as u16);
            let cur_fam = self.eqtb.int_par(CUR_FAM_CODE);
            let fam = if crate::xemath::is_var_family(c) && (0..256).contains(&cur_fam) {
                cur_fam
            } else {
                crate::xemath::math_fam_field(c)
            };
            self.mem.set_font(p, (fam + (ch / 0x10000) * 0x100) as u16);
            return Ok(());
        }
    }

    /// §1152: treat `cur_chr` as an active character.
    fn treat_as_active(&mut self) -> TexResult<()> {
        self.cur_cs = self.cur_chr + self.eqtb.lay.active_base;
        self.cur_cmd = self.eqtb.eq_type(self.cur_cs);
        self.cur_chr = self.eqtb.equiv(self.cur_cs);
        self.x_token()?;
        self.back_input()
    }

    /// `set_math_char(c)` (xetex.web set_math_char): `c` is an EXTENDED
    /// math code (char in bits 0..20, class 21..23, family 24..31).
    pub fn set_math_char(&mut self, c: i32) -> TexResult<()> {
        use crate::xemath::*;
        if is_active_math_char(c) {
            self.treat_as_active()
        } else {
            let p = self.new_noad()?;
            self.mem.set_math_type(p + 1, MATH_CHAR);
            let ch = math_char_field(c);
            self.mem.set_character(p + 1, (ch % 0x10000) as u16);
            let mut fam = math_fam_field(c);
            if is_var_family(c) {
                let cur_fam = self.eqtb.int_par(CUR_FAM_CODE);
                if (0..256).contains(&cur_fam) {
                    fam = cur_fam;
                }
                self.mem.set_node_type(p, ORD_NOAD);
            } else {
                self.mem
                    .set_node_type(p, ORD_NOAD + math_class_field(c) as u16);
            }
            // plane_and_fam: the character plane rides in the high byte
            // of the family field (xetex.web set_math_char).
            self.mem
                .set_font(p + 1, (fam + (ch / 0x10000) * 0x100) as u16);
            self.tail_append(p);
            Ok(())
        }
    }

    /// `math_limit_switch` (§1159).
    pub fn math_limit_switch(&mut self) -> TexResult<()> {
        let tail = self.nest.cur.tail;
        if self.nest.cur.head != tail && self.mem.node_type(tail) == OP_NOAD {
            let c = self.cur_chr as u16;
            self.mem.set_subtype(tail, c);
            return Ok(());
        }
        self.print_err("Limit controls must follow a math operator");
        self.help(&["I'm ignoring this misplaced \\limits or \\nolimits command."]);
        self.error()
    }

    /// `scan_delimiter(p, r)` (xetex.web scan_delimiter): fills the
    /// delimiter fields, understanding extended delcodes (bit 30 set:
    /// family at bit 21, USV in the low 21 bits, one size only).
    pub fn scan_delimiter(&mut self, p: Pointer, r: bool) -> TexResult<()> {
        if r {
            if self.cur_chr == 1 {
                // \Uradical <fam> <usv>.
                let mut v: i64 = 0x4000_0000;
                self.scan_math_fam_int()?;
                v += i64::from(self.cur_val) * 0x20_0000;
                self.scan_char_num()?;
                v += i64::from(self.cur_val);
                self.cur_val = v as i32;
            } else {
                self.scan_twenty_seven_bit_int()?;
            }
        } else {
            self.get_next_nonblank_nonrelax_noncall()?;
            match self.cur_cmd {
                LETTER | OTHER_CHAR => {
                    self.cur_val = self.eqtb.del_code(self.cur_chr);
                }
                DELIM_NUM => {
                    if self.cur_chr == 1 {
                        // \Udelimiter <class> <fam> <usv>.
                        let mut v: i64 = 0x4000_0000;
                        self.scan_math_class_int()?; // class is discarded
                        self.scan_math_fam_int()?;
                        v += i64::from(self.cur_val) * 0x20_0000;
                        self.scan_char_num()?;
                        v += i64::from(self.cur_val);
                        self.cur_val = v as i32;
                    } else {
                        self.scan_twenty_seven_bit_int()?;
                    }
                }
                _ => self.cur_val = -1,
            }
        }
        if self.cur_val < 0 {
            // §1161.
            self.print_err("Missing delimiter (. inserted)");
            self.help(&[
                "I was expecting to see something like `(' or `\\{' or",
                "`\\}' here. If you typed, e.g., `{' instead of `\\{', you",
                "should probably delete the `{' by typing `1' now, so that",
                "braces don't get unbalanced. Otherwise just proceed.",
                "Acceptable delimiters are characters whose \\delcode is",
                "nonnegative, or you can use `\\delimiter <delimiter code>'.",
            ]);
            self.back_error()?;
            self.cur_val = 0;
        }
        let v = self.cur_val;
        let w = self.mem.word_mut(p);
        if v >= 0x4000_0000 {
            // Extended delimiter code: one size; the plane rides in
            // the high byte of the fam field (xetex.web scan_delimiter).
            let usv = v % 0x20_0000;
            let fam = (v / 0x20_0000) % 0x100;
            w.set_qqqq(0, ((usv / 0x10000) * 0x100 + fam) as u16); // plane+fam
            w.set_qqqq(1, (usv % 0x10000) as u16); // small_char
            w.set_qqqq(2, 0);
            w.set_qqqq(3, 0);
        } else {
            w.set_qqqq(0, ((v / 0o4000000) % 16) as u16); // small_fam
            w.set_qqqq(1, ((v / 0o10000) % 256) as u16); // small_char
            w.set_qqqq(2, ((v / 256) % 16) as u16); // large_fam
            w.set_qqqq(3, (v % 256) as u16); // large_char
        }
        Ok(())
    }

    /// `math_radical` (§1163).
    pub fn math_radical(&mut self) -> TexResult<()> {
        let p = self.mem.get_node(RADICAL_NOAD_SIZE)?;
        self.tail_append(p);
        self.mem.set_node_type(p, RADICAL_NOAD);
        self.mem.set_subtype(p, NORMAL);
        *self.mem.word_mut(p + 1) = MemoryWord::ZERO;
        *self.mem.word_mut(p + 2) = MemoryWord::ZERO;
        *self.mem.word_mut(p + 3) = MemoryWord::ZERO;
        self.scan_delimiter(p + 4, true)?;
        self.scan_math(p + 1)
    }

    /// `math_ac` (§1165).
    pub fn math_ac(&mut self) -> TexResult<()> {
        if self.cur_cmd == ACCENT {
            // §1166.
            self.print_err("Please use ");
            self.print_esc_str("mathaccent");
            self.print_chars(" for accents in math mode");
            self.help(&[
                "I'm changing \\accent to \\mathaccent here; wish me luck.",
                "(Accents are not the same in formulas as they are in text.)",
            ]);
            self.error()?;
        }
        let p = self.mem.get_node(ACCENT_NOAD_SIZE)?;
        self.tail_append(p);
        self.mem.set_node_type(p, ACCENT_NOAD);
        self.mem.set_subtype(p, NORMAL);
        *self.mem.word_mut(p + 1) = MemoryWord::ZERO;
        *self.mem.word_mut(p + 2) = MemoryWord::ZERO;
        *self.mem.word_mut(p + 3) = MemoryWord::ZERO;
        self.mem.set_math_type(p + 4, MATH_CHAR);
        // xetex.web math_ac: chr 1 = \Umathaccent with optional
        // `fixed'/`bottom' keywords and <class> <fam> <usv> operands.
        let v = if self.cur_chr == 1 {
            if self.scan_keyword("fixed")? {
                self.mem.set_subtype(p, FIXED_ACC);
            } else if self.scan_keyword("bottom")? {
                if self.scan_keyword("fixed")? {
                    self.mem.set_subtype(p, BOTTOM_ACC + FIXED_ACC);
                } else {
                    self.mem.set_subtype(p, BOTTOM_ACC);
                }
            }
            self.scan_math_class_int()?;
            let mut c = crate::xemath::set_class_field(self.cur_val);
            self.scan_math_fam_int()?;
            c += crate::xemath::set_family_field(self.cur_val);
            self.scan_char_num()?;
            c + self.cur_val
        } else {
            self.scan_fifteen_bit_int()?;
            crate::xemath::from_classic(self.cur_val)
        };
        let ch = crate::xemath::math_char_field(v);
        self.mem.set_character(p + 4, (ch % 0x10000) as u16);
        let cur_fam = self.eqtb.int_par(CUR_FAM_CODE);
        let fam = if crate::xemath::is_var_family(v) && (0..256).contains(&cur_fam) {
            cur_fam
        } else {
            crate::xemath::math_fam_field(v)
        };
        self.mem
            .set_font(p + 4, (fam + (ch / 0x10000) * 0x100) as u16);
        self.scan_math(p + 1)
    }

    /// `append_choices` (§1172).
    pub fn append_choices(&mut self) -> TexResult<()> {
        let c = self.new_choice()?;
        self.tail_append(c);
        self.save.save_ptr += 1;
        self.save.set_saved(-1, 0);
        self.push_math(MATH_CHOICE_GROUP)?;
        self.scan_left_brace()
    }

    /// `build_choices` (§1174): called when a `\mathchoice` group ends.
    pub fn build_choices(&mut self) -> TexResult<()> {
        self.unsave()?;
        let p = self.fin_mlist(NULL)?;
        let tail = self.nest.cur.tail;
        match self.save.saved(-1) {
            0 => self.mem.set_info(tail + 1, p),
            1 => self.mem.set_link(tail + 1, p),
            2 => self.mem.set_info(tail + 2, p),
            _ => {
                self.mem.set_link(tail + 2, p);
                self.save.save_ptr -= 1;
                return Ok(());
            }
        }
        let s = self.save.saved(-1);
        self.save.set_saved(-1, s + 1);
        self.push_math(MATH_CHOICE_GROUP)?;
        self.scan_left_brace()
    }

    /// `sub_sup` (§1175-§1177).
    pub fn sub_sup(&mut self) -> TexResult<()> {
        let mut t = EMPTY;
        let mut p: Pointer = NULL;
        let tail = self.nest.cur.tail;
        if tail != self.nest.cur.head {
            let ty = self.mem.node_type(tail);
            if (ORD_NOAD..LEFT_NOAD).contains(&ty) {
                // scripts_allowed
                p = tail + 2 + i32::from(self.cur_cmd - SUP_MARK); // supscr or subscr
                t = self.mem.math_type(p);
            }
        }
        if p == NULL || t != EMPTY {
            // §1177: insert a dummy noad.
            let n = self.new_noad()?;
            self.tail_append(n);
            p = n + 2 + i32::from(self.cur_cmd - SUP_MARK);
            if t != EMPTY {
                if self.cur_cmd == SUP_MARK {
                    self.print_err("Double superscript");
                    self.help(&["I treat `x^1^2' essentially like `x^1{}^2'."]);
                } else {
                    self.print_err("Double subscript");
                    self.help(&["I treat `x_1_2' essentially like `x_1{}_2'."]);
                }
                self.error()?;
            }
        }
        self.scan_math(p)
    }

    /// `math_fraction` (§1181-§1183).
    pub fn math_fraction(&mut self) -> TexResult<()> {
        let c = self.cur_chr;
        if self.nest.cur.aux.int() != NULL {
            // §1183: ambiguous fraction.
            if c >= DELIMITED_CODE {
                let g = self.mem.lig_trick(); // garbage
                self.scan_delimiter(g, false)?;
                self.scan_delimiter(g, false)?;
            }
            if c % DELIMITED_CODE == ABOVE_CODE {
                self.scan_normal_dimen()?;
            }
            self.print_err("Ambiguous; you need another { and }");
            self.help(&[
                "I'm ignoring this fraction specification, since I don't",
                "know whether a construction like `x \\over y \\over z'",
                "means `{x \\over y} \\over z' or `x \\over {y \\over z}'.",
            ]);
            self.error()
        } else {
            let inc = self.mem.get_node(FRACTION_NOAD_SIZE)?;
            self.nest.cur.aux.set_int(inc);
            self.mem.set_node_type(inc, FRACTION_NOAD);
            self.mem.set_subtype(inc, NORMAL);
            self.mem.set_math_type(inc + 2, SUB_MLIST); // numerator
            let h = self.nest.cur.head;
            let l = self.mem.link(h);
            self.mem.set_info(inc + 2, l);
            *self.mem.word_mut(inc + 3) = MemoryWord::ZERO; // denominator
            *self.mem.word_mut(inc + 4) = MemoryWord::ZERO; // null_delimiter
            *self.mem.word_mut(inc + 5) = MemoryWord::ZERO;
            self.mem.set_link(h, NULL);
            self.nest.cur.tail = h;
            // §1182: distinguish the generalized fractions.
            if c >= DELIMITED_CODE {
                self.scan_delimiter(inc + 4, false)?;
                self.scan_delimiter(inc + 5, false)?;
            }
            match c % DELIMITED_CODE {
                ABOVE_CODE => {
                    self.scan_normal_dimen()?;
                    let v = self.cur_val;
                    self.mem.set_width(inc, v); // thickness
                }
                OVER_CODE => self.mem.set_width(inc, DEFAULT_CODE),
                _ => self.mem.set_width(inc, 0), // atop
            }
            Ok(())
        }
    }

    /// `fin_mlist(p)` (§1184-§1185).
    pub fn fin_mlist(&mut self, p: Pointer) -> TexResult<Pointer> {
        let q;
        let inc = self.nest.cur.aux.int();
        if inc != NULL {
            // §1185: compleat the incompleat noad.
            self.mem.set_math_type(inc + 3, SUB_MLIST);
            let h = self.nest.cur.head;
            let l = self.mem.link(h);
            self.mem.set_info(inc + 3, l);
            if p == NULL {
                q = inc;
            } else {
                // etex.ch §1185: splice at the most recent \left or
                // \middle noad, not necessarily the group-opening \left.
                q = self.mem.info(inc + 2); // numerator
                let dp = self.nest.cur.etex_aux; // delim_ptr
                if self.mem.node_type(q) != LEFT_NOAD || dp == NULL {
                    return self.confusion("right").map(|_| NULL);
                }
                let ld = self.mem.link(dp);
                self.mem.set_info(inc + 2, ld);
                self.mem.set_link(dp, inc);
                self.mem.set_link(inc, p);
            }
        } else {
            let t = self.nest.cur.tail;
            self.mem.set_link(t, p);
            q = self.mem.link(self.nest.cur.head);
        }
        self.pop_nest();
        Ok(q)
    }

    /// `math_left_right` (§1191-§1192).
    pub fn math_left_right(&mut self) -> TexResult<()> {
        use crate::math::MIDDLE_NOAD;
        let t = self.cur_chr as u16;
        if t != LEFT_NOAD && self.save.cur_group != MATH_LEFT_GROUP {
            // §1192: recover from mismatched \right or \middle.
            if self.save.cur_group == MATH_SHIFT_GROUP {
                let g = self.mem.lig_trick();
                self.scan_delimiter(g, false)?;
                self.print_err("Extra ");
                if t == MIDDLE_NOAD {
                    self.print_esc_str("middle");
                    self.help(&["I'm ignoring a \\middle that had no matching \\left."]);
                } else {
                    self.print_esc_str("right");
                    self.help(&["I'm ignoring a \\right that had no matching \\left."]);
                }
                self.error()
            } else {
                self.off_save()
            }
        } else {
            let p = self.new_noad()?;
            self.mem.set_node_type(p, t);
            self.scan_delimiter(p + 1, false)?;
            if t == MIDDLE_NOAD {
                // etex.ch §1191: \middle is a right noad with a subtype.
                self.mem.set_node_type(p, RIGHT_NOAD);
                self.mem.set_subtype(p, MIDDLE_NOAD);
            }
            let q = if t == LEFT_NOAD {
                p
            } else {
                let q = self.fin_mlist(p)?;
                self.unsave()?; // end of math_left_group
                q
            };
            if t != RIGHT_NOAD {
                self.push_math(MATH_LEFT_GROUP)?;
                let h = self.nest.cur.head;
                self.mem.set_link(h, q);
                self.nest.cur.tail = p;
                self.nest.cur.etex_aux = p; // delim_ptr
            } else {
                let n = self.new_noad()?;
                self.tail_append(n);
                self.mem.set_node_type(n, INNER_NOAD);
                self.mem.set_math_type(n + 1, SUB_MLIST);
                self.mem.set_info(n + 1, q);
            }
            Ok(())
        }
    }

    /// §1195: check that the necessary symbol/extension fonts are present;
    /// flush the math lists if not (`danger`).
    fn check_math_fonts(&mut self) -> TexResult<bool> {
        let params = |e: &Engine, fam: i32| -> i32 {
            let mut min = i32::MAX;
            for size in [TEXT_SIZE, SCRIPT_SIZE, SCRIPT_SCRIPT_SIZE] {
                let f = e.fam_fnt(fam + size);
                min = min.min(e.fonts.params[f as usize]);
            }
            min
        };
        if params(self, 2) < TOTAL_MATHSY_PARAMS {
            self.print_err("Math formula deleted: Insufficient symbol fonts");
            self.help(&[
                "Sorry, but I can't typeset math unless \\textfont 2",
                "and \\scriptfont 2 and \\scriptscriptfont 2 have all",
                "the \\fontdimen values needed in math symbol fonts.",
            ]);
            self.error()?;
            self.flush_math();
            Ok(true)
        } else if params(self, 3) < TOTAL_MATHEX_PARAMS {
            self.print_err("Math formula deleted: Insufficient extension fonts");
            self.help(&[
                "Sorry, but I can't typeset math unless \\textfont 3",
                "and \\scriptfont 3 and \\scriptscriptfont 3 have all",
                "the \\fontdimen values needed in math extension fonts.",
            ]);
            self.error()?;
            self.flush_math();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// §1197: check that another `$` follows.
    fn check_dollar(&mut self) -> TexResult<()> {
        self.get_x_token()?;
        if self.cur_cmd != MATH_SHIFT {
            self.print_err("Display math should end with $$");
            self.help(&[
                "The `$' that I just saw supposedly matches a previous `$$'.",
                "So I shall assume that you typed `$$' both times.",
            ]);
            self.back_error()?;
        }
        Ok(())
    }

    /// `after_math` (§1194-§1206).
    pub fn after_math(&mut self) -> TexResult<()> {
        let mut danger = self.check_math_fonts()?;
        // etex.ch: retrieve the prototype box (LR_box of the mmode list).
        let mut j = if self.mode() == MMODE {
            self.nest.cur.etex_aux
        } else {
            NULL
        };
        let mut m = self.mode();
        let mut l = false;
        let mut p = self.fin_mlist(NULL)?; // this pops the nest
        let a: Pointer;
        if self.mode() == -m {
            // end of equation number
            self.check_dollar()?;
            self.cur_mlist = p;
            self.cur_style = TEXT_STYLE;
            self.mlist_penalties = false;
            self.mlist_to_hlist()?;
            let th = self.mem.temp_head();
            let lk = self.mem.link(th);
            a = self.hpack(lk, 0, ADDITIONAL)?;
            self.mem.set_subtype(a, crate::nodes::DLIST); // etex.ch
            self.unsave()?;
            self.save.save_ptr -= 1; // now cur_group = math_shift_group
            if self.save.saved(0) == 1 {
                l = true;
            }
            danger = self.check_math_fonts()?;
            m = self.mode();
            j = if self.mode() == MMODE {
                self.nest.cur.etex_aux
            } else {
                NULL
            };
            p = self.fin_mlist(NULL)?;
        } else {
            a = NULL;
        }
        if m < 0 {
            // §1196: finish math in text.
            let ms = self.eqtb.dimen_par(MATH_SURROUND_CODE);
            let mn = self.new_math(ms, BEFORE)?;
            self.tail_append(mn);
            self.cur_mlist = p;
            self.cur_style = TEXT_STYLE;
            self.mlist_penalties = self.mode() > 0;
            self.mlist_to_hlist()?;
            let th = self.mem.temp_head();
            let lk = self.mem.link(th);
            let t = self.nest.cur.tail;
            self.mem.set_link(t, lk);
            while self.mem.link(self.nest.cur.tail) != NULL {
                let nx = self.mem.link(self.nest.cur.tail);
                self.nest.cur.tail = nx;
            }
            let mn = self.new_math(ms, AFTER)?;
            self.tail_append(mn);
            self.set_space_factor(1000);
            self.unsave()?;
        } else {
            if a == NULL {
                self.check_dollar()?;
            }
            self.finish_displayed_math(l, danger, p, a, j)?;
        }
        Ok(())
    }

    /// §1199-§1205: finish displayed math.
    fn finish_displayed_math(
        &mut self,
        l: bool,
        danger: bool,
        p: Pointer,
        a: Pointer,
        j: Pointer,
    ) -> TexResult<()> {
        self.cur_mlist = p;
        self.cur_style = DISPLAY_STYLE;
        self.mlist_penalties = false;
        self.mlist_to_hlist()?;
        let mut p = self.mem.link(self.mem.temp_head());
        self.adjust_tail = self.mem.adjust_head();
        let mut b = self.hpack(p, 0, ADDITIONAL)?;
        p = self.mem.list_ptr(b);
        let t = self.adjust_tail;
        self.adjust_tail = NULL;
        let mut w = self.mem.width(b);
        let z = self.eqtb.dimen_par(DISPLAY_WIDTH_CODE);
        let mut s = self.eqtb.dimen_par(DISPLAY_INDENT_CODE);
        // etex.ch §1199: in R-text the display hangs off the right edge.
        if self.eqtb.int_par(crate::eqtb::PRE_DISPLAY_DIRECTION_CODE) < 0 {
            s = -s - z;
        }
        let mut e;
        let q: Scaled;
        if a == NULL || danger {
            e = 0;
            q = 0;
        } else {
            e = self.mem.width(a);
            q = e + self.math_quad(TEXT_SIZE);
        }
        if w + q > z {
            // §1201: squeeze the equation as much as possible.
            if e != 0
                && (w - self.total_shrink[0] + q <= z
                    || self.total_shrink[1] != 0
                    || self.total_shrink[2] != 0
                    || self.total_shrink[3] != 0)
            {
                self.mem.free_node(b, BOX_NODE_SIZE);
                b = self.hpack(p, z - q, EXACTLY)?;
            } else {
                e = 0;
                if w > z {
                    self.mem.free_node(b, BOX_NODE_SIZE);
                    b = self.hpack(p, z, EXACTLY)?;
                }
            }
            w = self.mem.width(b);
        }
        // §1202 (+ etex.ch): the displacement d.
        self.mem.set_subtype(b, crate::nodes::DLIST);
        let mut d: Scaled = crate::arith::half(z - w);
        if e > 0 && d < 2 * e {
            // too close
            d = crate::arith::half(z - w - e);
            if p != NULL && !self.mem.is_char_node(p) && self.mem.node_type(p) == GLUE_NODE {
                d = 0;
            }
        }
        // §1203: append the glue or equation number preceding the display.
        let pdp = self.eqtb.int_par(PRE_DISPLAY_PENALTY_CODE);
        let pn = self.new_penalty(pdp)?;
        self.tail_append(pn);
        let g1;
        let mut g2;
        if d + s <= self.eqtb.dimen_par(PRE_DISPLAY_SIZE_CODE) || l {
            // not enough clearance
            g1 = ABOVE_DISPLAY_SKIP_CODE;
            g2 = BELOW_DISPLAY_SKIP_CODE;
        } else {
            g1 = ABOVE_DISPLAY_SHORT_SKIP_CODE;
            g2 = BELOW_DISPLAY_SHORT_SKIP_CODE;
        }
        if l && e == 0 {
            // it follows that type(a) = hlist_node
            self.app_display(j, a, 0)?;
            let pn = self.new_penalty(INF_PENALTY)?;
            self.tail_append(pn);
        } else {
            let g = self.new_param_glue(g1)?;
            self.tail_append(g);
        }
        // §1204: append the display and perhaps also the equation number.
        if e != 0 {
            let r = self.new_kern(z - w - e - d)?;
            if l {
                self.mem.set_link(a, r);
                self.mem.set_link(r, b);
                b = a;
                d = 0;
            } else {
                self.mem.set_link(b, r);
                self.mem.set_link(r, a);
            }
            b = self.hpack(b, 0, ADDITIONAL)?;
        }
        self.app_display(j, b, d)?;
        // §1205: append the glue or equation number following the display.
        if a != NULL && e == 0 && !l {
            let pn = self.new_penalty(INF_PENALTY)?;
            self.tail_append(pn);
            let wa = self.mem.width(a);
            self.app_display(j, a, z - wa)?;
            g2 = 0;
        }
        if t != self.mem.adjust_head() {
            // migrating material comes after equation number
            let ah = self.mem.adjust_head();
            let tl = self.nest.cur.tail;
            let la = self.mem.link(ah);
            self.mem.set_link(tl, la);
            self.nest.cur.tail = t;
        }
        let pdp = self.eqtb.int_par(POST_DISPLAY_PENALTY_CODE);
        let pn = self.new_penalty(pdp)?;
        self.tail_append(pn);
        if g2 > 0 {
            let g = self.new_param_glue(g2)?;
            self.tail_append(g);
        }
        // etex.ch: flush the prototype box.
        self.flush_node_list(j);
        self.resume_after_display()
    }

    /// `app_display(j, b, d)` (etex.ch): appends the display line box `b`
    /// with displacement `d`, rebuilding it around the prototype box `j`
    /// when the paragraph has mixed direction text.
    fn app_display(&mut self, j: Pointer, b: Pointer, d: Scaled) -> TexResult<()> {
        use crate::nodes::*;
        let mut b = b;
        let mut d = d;
        let mut s = self.eqtb.dimen_par(DISPLAY_INDENT_CODE);
        let x = self.eqtb.int_par(crate::eqtb::PRE_DISPLAY_DIRECTION_CODE);
        if x == 0 {
            self.mem.set_shift_amount(b, s + d);
        } else {
            let z = self.eqtb.dimen_par(DISPLAY_WIDTH_CODE);
            let mut p = b;
            // Set up the hlist for the display line.
            let mut e;
            if x > 0 {
                e = z - d - self.mem.width(p);
            } else {
                e = d;
                d = z - e - self.mem.width(p);
            }
            if j != NULL {
                b = self.copy_node_list(j)?;
                let h = self.mem.height(p);
                self.mem.set_height(b, h);
                let dp = self.mem.depth(p);
                self.mem.set_depth(b, dp);
                s -= self.mem.shift_amount(b);
                d += s;
                e = e + self.mem.width(b) - z - s;
            }
            let q;
            if self.mem.subtype(p) == DLIST {
                q = p; // display or equation number
            } else {
                // display and equation number
                let mut r = self.mem.list_ptr(p);
                self.mem.free_node(p, crate::nodes::BOX_NODE_SIZE);
                if r == NULL {
                    return self.confusion("LR4");
                }
                if x > 0 {
                    p = r;
                    let mut qq;
                    loop {
                        qq = r;
                        r = self.mem.link(r);
                        if r == NULL {
                            break;
                        }
                    }
                    q = qq;
                } else {
                    p = NULL;
                    q = r; // the old head becomes the tail
                    loop {
                        let t = self.mem.link(r);
                        self.mem.set_link(r, p);
                        p = r;
                        r = t;
                        if r == NULL {
                            break;
                        }
                    }
                }
            }
            // Package the display line.
            let (r, t);
            if j == NULL {
                r = self.new_kern(0)?;
                t = self.new_kern(0)?;
            } else {
                r = self.mem.list_ptr(b);
                t = self.mem.link(r);
            }
            let u = self.new_math(0, END_M_CODE)?;
            if self.mem.node_type(t) == GLUE_NODE {
                // t is the \rightskip glue: cancel_glue(right_skip)(q)(u)(t)(e)
                let (g, tp) = self.new_skip_param(RIGHT_SKIP_CODE)?;
                self.mem.set_link(q, g);
                self.mem.set_link(g, u);
                let gp = self.mem.glue_ptr(t);
                let so = self.mem.stretch_order(gp);
                self.mem.set_stretch_order(tp, so);
                let sh = self.mem.shrink_order(gp);
                self.mem.set_shrink_order(tp, sh);
                let wd = e - self.mem.width(gp);
                self.mem.set_width(tp, wd);
                let st = -self.mem.stretch(gp);
                self.mem.set_stretch(tp, st);
                let sk = -self.mem.shrink(gp);
                self.mem.set_shrink(tp, sk);
                self.mem.set_link(u, t);
            } else {
                self.mem.set_width(t, e);
                self.mem.set_link(t, u);
                self.mem.set_link(q, t);
            }
            let u = self.new_math(0, BEGIN_M_CODE)?;
            if self.mem.node_type(r) == GLUE_NODE {
                // r is the \leftskip glue: cancel_glue(left_skip)(u)(p)(r)(d)
                let (g, tp) = self.new_skip_param(LEFT_SKIP_CODE)?;
                self.mem.set_link(u, g);
                self.mem.set_link(g, p);
                let gp = self.mem.glue_ptr(r);
                let so = self.mem.stretch_order(gp);
                self.mem.set_stretch_order(tp, so);
                let sh = self.mem.shrink_order(gp);
                self.mem.set_shrink_order(tp, sh);
                let wd = d - self.mem.width(gp);
                self.mem.set_width(tp, wd);
                let st = -self.mem.stretch(gp);
                self.mem.set_stretch(tp, st);
                let sk = -self.mem.shrink(gp);
                self.mem.set_shrink(tp, sk);
                self.mem.set_link(r, u);
            } else {
                self.mem.set_width(r, d);
                self.mem.set_link(r, p);
                self.mem.set_link(u, r);
                if j == NULL {
                    b = self.hpack(u, 0, ADDITIONAL)?;
                    self.mem.set_shift_amount(b, s);
                } else {
                    self.mem.set_list_ptr(b, u);
                }
            }
        }
        self.append_to_vlist(b)
    }

    /// `resume_after_display` (§1200).
    pub fn resume_after_display(&mut self) -> TexResult<()> {
        if self.save.cur_group != MATH_SHIFT_GROUP {
            return self.confusion("display");
        }
        self.unsave()?;
        self.nest.cur.pg += 3;
        self.push_nest()?;
        self.nest.cur.mode = HMODE;
        self.set_space_factor(1000);
        // set_cur_lang (§1034 / §934).
        let language = self.eqtb.int_par(LANGUAGE_CODE);
        let cur_lang = if !(1..=255).contains(&language) {
            0
        } else {
            language
        };
        self.hy.cur_lang = cur_lang;
        self.set_clang(cur_lang);
        self.nest.cur.pg = (crate::hyph::norm_min(self.eqtb.int_par(LEFT_HYPHEN_MIN_CODE)) * 0o100
            + crate::hyph::norm_min(self.eqtb.int_par(RIGHT_HYPHEN_MIN_CODE)))
            * 0o200000
            + cur_lang;
        // §443: scan an optional space.
        self.get_x_token()?;
        if self.cur_cmd != SPACER {
            self.back_input()?;
        }
        if self.nest.ptr == 1 {
            self.build_page()?;
        }
        Ok(())
    }

    /// §1167: `\vcenter`.
    pub fn begin_vcenter(&mut self) -> TexResult<()> {
        self.scan_spec(VCENTER_GROUP, false)?;
        self.normal_paragraph()?;
        self.push_nest()?;
        self.nest.cur.mode = -VMODE;
        self.set_prev_depth(IGNORE_DEPTH);
        let ev = self
            .eqtb
            .equiv(self.eqtb.lay.local_base + EVERY_VBOX_OFFSET);
        if ev != NULL {
            self.begin_token_list(ev, 11)?; // every_vbox_text
        }
        Ok(())
    }

    /// §1168: end of a `\vcenter` group.
    pub fn finish_vcenter(&mut self) -> TexResult<()> {
        self.end_graf()?;
        self.unsave()?;
        self.save.save_ptr -= 2;
        let h = self.nest.cur.head;
        let lk = self.mem.link(h);
        let (hh, m) = (self.save.saved(1), self.save.saved(0));
        let p = self.vpack(lk, hh, m)?;
        self.pop_nest();
        let n = self.new_noad()?;
        self.tail_append(n);
        self.mem.set_node_type(n, VCENTER_NOAD);
        self.mem.set_math_type(n + 1, SUB_BOX);
        self.mem.set_info(n + 1, p);
        Ok(())
    }

    /// §1186: end of a math group (`}` in `math_group`).
    pub fn finish_math_group(&mut self) -> TexResult<()> {
        self.unsave()?;
        self.save.save_ptr -= 1;
        let field = self.save.saved(0);
        self.mem.set_math_type(field, SUB_MLIST);
        let p = self.fin_mlist(NULL)?;
        self.mem.set_info(field, p);
        if p != NULL && self.mem.link(p) == NULL {
            if self.mem.node_type(p) == ORD_NOAD {
                if self.mem.math_type(p + 3) == EMPTY && self.mem.math_type(p + 2) == EMPTY {
                    let w = self.mem.word(p + 1);
                    let f = self.mem.word_mut(field);
                    f.set_lh(w.lh());
                    f.set_rh(w.rh());
                    self.mem.free_node(p, NOAD_SIZE);
                }
            } else if self.mem.node_type(p) == ACCENT_NOAD
                && field == self.nest.cur.tail + 1
                && self.mem.node_type(self.nest.cur.tail) == ORD_NOAD
            {
                // §1187: replace the tail of the list by p.
                let mut q = self.nest.cur.head;
                let tail = self.nest.cur.tail;
                while self.mem.link(q) != tail {
                    q = self.mem.link(q);
                }
                self.mem.set_link(q, p);
                self.mem.free_node(tail, NOAD_SIZE);
                self.nest.cur.tail = p;
            }
        }
        Ok(())
    }
}
