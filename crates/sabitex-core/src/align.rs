//! Alignment: `\halign` and `\valign`.
//!
//! Ports tex.web Part 37 (§768-§812): the preamble scanner, the row and
//! column routines (`init_row` .. `fin_row`), and `fin_align`, plus the
//! `do_endv` dispatcher of §1131.

use crate::cmds::*;
use crate::engine::{Engine, HMODE, MMODE, VMODE};
use crate::eqtb::*;
use crate::error::TexResult;
use crate::input::{ALIGNING, NORMAL_STATUS, TOKEN_LIST, U_TEMPLATE, V_TEMPLATE};
use crate::mem::Mem;
use crate::nest::IGNORE_DEPTH;
use crate::nodes::*;
use crate::nodes::{SHRINKING, STRETCHING};
use crate::pack::ADDITIONAL;
use crate::tokens::CS_TOKEN_FLAG;
use crate::types::{Pointer, Scaled, NULL};

// §780: command modifiers for \span, \cr, \crcr. tex.web uses 256/257
// ("distinct from any character"); with USV-wide characters they must sit
// above the Unicode range, as in XeTeX (special_char = biggest_usv+2).
pub const SPAN_CODE: i32 = 0x11_0001;
pub const CR_CODE: i32 = SPAN_CODE + 1;
pub const CR_CR_CODE: i32 = CR_CODE + 1;

/// `align_stack_node_size` (§770).
pub const ALIGN_STACK_NODE_SIZE: i32 = 5;
/// `span_node_size` (§797).
pub const SPAN_NODE_SIZE: i32 = 2;

impl Mem {
    /// `align_head == mem_top - 8` (§162).
    pub fn align_head(&self) -> Pointer {
        self.mem_top - 8
    }

    /// `end_span == mem_top - 9` (§162).
    pub fn end_span(&self) -> Pointer {
        self.mem_top - 9
    }

    /// `omit_template == mem_top - 10` (§162).
    pub fn omit_template(&self) -> Pointer {
        self.mem_top - 10
    }

    /// `null_list == mem_top - 11` (§162).
    pub fn null_list(&self) -> Pointer {
        self.mem_top - 11
    }

    /// `u_part(p) == mem[p+height_offset].int` (§769).
    pub fn u_part(&self, p: Pointer) -> Pointer {
        self.word(p + 3).int()
    }

    pub fn set_u_part(&mut self, p: Pointer, v: Pointer) {
        self.word_mut(p + 3).set_int(v);
    }

    /// `v_part(p) == mem[p+depth_offset].int` (§769).
    pub fn v_part(&self, p: Pointer) -> Pointer {
        self.word(p + 2).int()
    }

    pub fn set_v_part(&mut self, p: Pointer, v: Pointer) {
        self.word_mut(p + 2).set_int(v);
    }

    /// `extra_info(p) == info(p+list_offset)` (§769).
    pub fn extra_info(&self, p: Pointer) -> i32 {
        self.info(p + 5)
    }

    pub fn set_extra_info(&mut self, p: Pointer, v: i32) {
        self.set_info(p + 5, v);
    }
}

impl Engine {
    /// `preamble == link(align_head)` (§770).
    fn preamble(&self) -> Pointer {
        self.mem.link(self.mem.align_head())
    }

    fn set_preamble(&mut self, p: Pointer) {
        let ah = self.mem.align_head();
        self.mem.set_link(ah, p);
    }

    /// `push_alignment` / `pop_alignment` (§772).
    fn push_alignment(&mut self) -> TexResult<()> {
        let p = self.mem.get_node(ALIGN_STACK_NODE_SIZE)?;
        let ap = self.align_ptr;
        self.mem.set_link(p, ap);
        let ca = self.cur_align;
        self.mem.set_info(p, ca);
        let pre = self.preamble();
        self.mem.set_llink(p, pre);
        let cs = self.cur_span;
        self.mem.set_rlink(p, cs);
        let cl = self.cur_loop;
        self.mem.word_mut(p + 2).set_int(cl);
        let als = self.inp.align_state;
        self.mem.word_mut(p + 3).set_int(als);
        let ch = self.cur_head;
        self.mem.set_info(p + 4, ch);
        let ct = self.cur_tail;
        self.mem.set_link(p + 4, ct);
        self.align_ptr = p;
        self.cur_head = self.mem.get_avail()?;
        Ok(())
    }

    fn pop_alignment(&mut self) {
        let ch = self.cur_head;
        self.mem.free_avail(ch);
        let p = self.align_ptr;
        self.cur_tail = self.mem.link(p + 4);
        self.cur_head = self.mem.info(p + 4);
        self.inp.align_state = self.mem.word(p + 3).int();
        self.cur_loop = self.mem.word(p + 2).int();
        self.cur_span = self.mem.rlink(p);
        let pre = self.mem.llink(p);
        self.set_preamble(pre);
        self.cur_align = self.mem.info(p);
        self.align_ptr = self.mem.link(p);
        self.mem.free_node(p, ALIGN_STACK_NODE_SIZE);
    }

    /// `init_align` (§774-§777).
    pub fn init_align(&mut self) -> TexResult<()> {
        let save_cs_ptr = self.cur_cs; // \halign or \valign, usually
        self.push_alignment()?;
        self.inp.align_state = -1_000_000; // enter a new alignment level

        // §776: check for improper alignment in displayed math.
        if self.mode() == MMODE
            && (self.nest.cur.head != self.nest.cur.tail || self.nest.cur.aux.int() != NULL)
        {
            self.print_err("Improper ");
            self.print_esc_str("halign");
            self.print_chars(" inside $$'s");
            self.help(&[
                "Displays can use special alignments (like \\eqalignno)",
                "only if nothing but the alignment itself is between $$'s.",
                "So I've deleted the formulas that preceded this alignment.",
            ]);
            self.error()?;
            self.flush_math();
        }
        self.push_nest()?; // enter a new semantic level
                           // §775: -vmode for \halign, -hmode for \valign.
        if self.mode() == MMODE {
            self.nest.cur.mode = -VMODE;
            let pd = self.nest.stack[self.nest.ptr - 2].aux.sc();
            self.set_prev_depth(pd);
        } else if self.mode() > 0 {
            self.nest.cur.mode = -self.nest.cur.mode;
        }
        self.scan_spec(ALIGN_GROUP, false)?;
        // §777: scan the preamble.
        self.set_preamble(NULL);
        self.cur_align = self.mem.align_head();
        self.cur_loop = NULL;
        self.inp.scanner_status = ALIGNING;
        self.inp.warning_index = save_cs_ptr;
        self.inp.align_state = -1_000_000;
        // at this point, cur_cmd = left_brace
        loop {
            // §778: append the current tabskip glue to the preamble list.
            let g = self.new_param_glue(TAB_SKIP_CODE)?;
            let ca = self.cur_align;
            self.mem.set_link(ca, g);
            self.cur_align = g;
            if self.cur_cmd == CAR_RET {
                break; // \cr ends the preamble
            }
            // §779: scan a preamble entry.
            // §783: scan the template u_j.
            let hh = self.mem.hold_head();
            let mut p = hh;
            self.mem.set_link(p, NULL);
            loop {
                self.get_preamble_token()?;
                if self.cur_cmd == MAC_PARAM {
                    break;
                }
                if self.cur_cmd <= CAR_RET
                    && self.cur_cmd >= TAB_MARK
                    && self.inp.align_state == -1_000_000
                {
                    if p == hh && self.cur_loop == NULL && self.cur_cmd == TAB_MARK {
                        self.cur_loop = self.cur_align;
                    } else {
                        self.print_err("Missing # inserted in alignment preamble");
                        self.help(&[
                            "There should be exactly one # between &'s, when an",
                            "\\halign or \\valign is being set up. In this case you had",
                            "none, so I've put one in; maybe that will work.",
                        ]);
                        self.back_error()?;
                        break;
                    }
                } else if self.cur_cmd != SPACER || p != hh {
                    let q = self.mem.get_avail()?;
                    self.mem.set_link(p, q);
                    p = q;
                    let t = self.cur_tok;
                    self.mem.set_info(p, t);
                }
            }
            let b = self.new_null_box()?;
            let ca = self.cur_align;
            self.mem.set_link(ca, b);
            self.cur_align = b; // a new alignrecord
            let es = self.mem.end_span();
            self.mem.set_info(b, es);
            self.mem.set_width(b, NULL_FLAG);
            let u = self.mem.link(hh);
            self.mem.set_u_part(b, u);
            // §784: scan the template v_j.
            let mut p = hh;
            self.mem.set_link(p, NULL);
            loop {
                self.get_preamble_token()?;
                if self.cur_cmd <= CAR_RET
                    && self.cur_cmd >= TAB_MARK
                    && self.inp.align_state == -1_000_000
                {
                    break;
                }
                if self.cur_cmd == MAC_PARAM {
                    self.print_err("Only one # is allowed per tab");
                    self.help(&[
                        "There should be exactly one # between &'s, when an",
                        "\\halign or \\valign is being set up. In this case you had",
                        "more than one, so I'm ignoring all but the first.",
                    ]);
                    self.error()?;
                    continue;
                }
                let q = self.mem.get_avail()?;
                self.mem.set_link(p, q);
                p = q;
                let t = self.cur_tok;
                self.mem.set_info(p, t);
            }
            let q = self.mem.get_avail()?;
            self.mem.set_link(p, q);
            let ett = CS_TOKEN_FLAG + self.eqtb.lay.frozen_end_template;
            self.mem.set_info(q, ett); // put \endtemplate at the end
            let v = self.mem.link(hh);
            let ca = self.cur_align;
            self.mem.set_v_part(ca, v);
        }
        self.inp.scanner_status = NORMAL_STATUS;
        self.new_save_level(ALIGN_GROUP)?;
        let ec = self.eqtb.equiv(self.eqtb.lay.local_base + EVERY_CR_OFFSET);
        if ec != NULL {
            self.begin_token_list(ec, 13)?; // every_cr_text
        }
        self.align_peek() // look for \noalign or \omit
    }

    /// `get_preamble_token` (§782).
    fn get_preamble_token(&mut self) -> TexResult<()> {
        'restart: loop {
            self.get_token()?;
            while self.cur_chr == SPAN_CODE && self.cur_cmd == TAB_MARK {
                self.get_token()?; // this token will be expanded once
                if self.cur_cmd > MAX_COMMAND {
                    self.expand()?;
                    self.get_token()?;
                }
            }
            if self.cur_cmd == ENDV {
                return Err(self.fatal_error("(interwoven alignment preambles are not allowed)"));
            }
            if self.cur_cmd == ASSIGN_GLUE
                && self.cur_chr == self.eqtb.lay.glue_base + TAB_SKIP_CODE
            {
                self.scan_optional_equals()?;
                self.scan_glue(crate::scan::GLUE_VAL)?;
                let v = self.cur_val;
                if self.eqtb.int_par(GLOBAL_DEFS_CODE) > 0 {
                    self.geq_define(self.eqtb.lay.glue_base + TAB_SKIP_CODE, GLUE_REF, v);
                } else {
                    self.eq_define(self.eqtb.lay.glue_base + TAB_SKIP_CODE, GLUE_REF, v)?;
                }
                continue 'restart;
            }
            return Ok(());
        }
    }

    /// `align_peek` (§785).
    pub fn align_peek(&mut self) -> TexResult<()> {
        loop {
            self.inp.align_state = 1_000_000;
            // etex.ch §785: expand non-protected macros only.
            loop {
                self.get_x_or_protected()?;
                if self.cur_cmd != crate::cmds::SPACER {
                    break;
                }
            }
            if self.cur_cmd == NO_ALIGN {
                self.scan_left_brace()?;
                self.new_save_level(NO_ALIGN_GROUP)?;
                if self.mode() == -VMODE {
                    self.normal_paragraph()?;
                }
                return Ok(());
            } else if self.cur_cmd == RIGHT_BRACE {
                return self.fin_align();
            } else if self.cur_cmd == CAR_RET && self.cur_chr == CR_CR_CODE {
                continue; // ignore \crcr
            } else {
                self.init_row()?; // start a new row
                return self.init_col(); // start a new column
            }
        }
    }

    /// `init_row` (§786).
    fn init_row(&mut self) -> TexResult<()> {
        self.push_nest()?;
        self.nest.cur.mode = (-HMODE - VMODE) - self.nest.cur.mode;
        if self.nest.cur.mode == -HMODE {
            self.set_space_factor(0);
        } else {
            self.set_prev_depth(0);
        }
        let pre = self.preamble();
        let gp = self.mem.glue_ptr(pre);
        let g = self.new_glue(gp)?;
        self.tail_append(g);
        let t = self.nest.cur.tail;
        self.mem.set_subtype(t, (TAB_SKIP_CODE + 1) as u16);
        self.cur_align = self.mem.link(pre);
        self.cur_tail = self.cur_head;
        let ca = self.cur_align;
        self.init_span(ca)
    }

    /// `init_span(p)` (§787).
    fn init_span(&mut self, p: Pointer) -> TexResult<()> {
        self.push_nest()?;
        if self.nest.cur.mode == -HMODE {
            self.set_space_factor(1000);
        } else {
            self.set_prev_depth(IGNORE_DEPTH);
            self.normal_paragraph()?;
        }
        self.cur_span = p;
        Ok(())
    }

    /// `init_col` (§788).
    pub fn init_col(&mut self) -> TexResult<()> {
        let ca = self.cur_align;
        let c = i32::from(self.cur_cmd);
        self.mem.set_extra_info(ca, c);
        if self.cur_cmd == OMIT {
            self.inp.align_state = 0;
        } else {
            self.back_input()?;
            let u = self.mem.u_part(ca);
            self.begin_token_list(u, U_TEMPLATE)?;
        } // now align_state = 1000000
        Ok(())
    }

    /// §791: insert the v_j template when a tab/\cr ends a column
    /// (called from `get_next` with `align_state = 0`).
    pub fn insert_vj_template(&mut self) -> TexResult<()> {
        if self.inp.scanner_status == ALIGNING || self.cur_align == NULL {
            return Err(self.fatal_error("(interwoven alignment preambles are not allowed)"));
        }
        let ca = self.cur_align;
        let cmd = self.mem.extra_info(ca);
        let chr = self.cur_chr;
        self.mem.set_extra_info(ca, chr);
        if cmd == i32::from(OMIT) {
            let ot = self.mem.omit_template();
            self.begin_token_list(ot, V_TEMPLATE)?;
        } else {
            let v = self.mem.v_part(ca);
            self.begin_token_list(v, V_TEMPLATE)?;
        }
        self.inp.align_state = 1_000_000;
        Ok(())
    }

    /// `fin_col` (§791-§796): returns true if a row has also been finished.
    pub fn fin_col(&mut self) -> TexResult<bool> {
        if self.cur_align == NULL {
            return self.confusion("endv").map(|_| false);
        }
        let q = self.mem.link(self.cur_align);
        if q == NULL {
            return self.confusion("endv").map(|_| false);
        }
        if self.inp.align_state < 500_000 {
            return Err(self.fatal_error("(interwoven alignment preambles are not allowed)"));
        }
        let mut p = self.mem.link(q);
        // §792: if the preamble list has been traversed, the row must end.
        if p == NULL && self.mem.extra_info(self.cur_align) < CR_CODE {
            if self.cur_loop != NULL {
                // §793: lengthen the preamble periodically.
                let b = self.new_null_box()?;
                self.mem.set_link(q, b);
                p = b; // a new alignrecord
                let es = self.mem.end_span();
                self.mem.set_info(p, es);
                self.mem.set_width(p, NULL_FLAG);
                self.cur_loop = self.mem.link(self.cur_loop);
                // §794: copy the templates from cur_loop into p.
                let hh = self.mem.hold_head();
                let mut qq = hh;
                let mut r = self.mem.u_part(self.cur_loop);
                while r != NULL {
                    let n = self.mem.get_avail()?;
                    self.mem.set_link(qq, n);
                    qq = n;
                    let i = self.mem.info(r);
                    self.mem.set_info(qq, i);
                    r = self.mem.link(r);
                }
                self.mem.set_link(qq, NULL);
                let u = self.mem.link(hh);
                self.mem.set_u_part(p, u);
                let mut qq = hh;
                self.mem.set_link(qq, NULL);
                let mut r = self.mem.v_part(self.cur_loop);
                while r != NULL {
                    let n = self.mem.get_avail()?;
                    self.mem.set_link(qq, n);
                    qq = n;
                    let i = self.mem.info(r);
                    self.mem.set_info(qq, i);
                    r = self.mem.link(r);
                }
                self.mem.set_link(qq, NULL);
                let v = self.mem.link(hh);
                self.mem.set_v_part(p, v);
                self.cur_loop = self.mem.link(self.cur_loop);
                let gp = self.mem.glue_ptr(self.cur_loop);
                let g = self.new_glue(gp)?;
                self.mem.set_link(p, g);
                self.mem.set_subtype(g, (TAB_SKIP_CODE + 1) as u16);
            } else {
                self.print_err("Extra alignment tab has been changed to ");
                self.print_esc_str("cr");
                self.help(&[
                    "You have given more \\span or & marks than there were",
                    "in the preamble to the \\halign or \\valign now in progress.",
                    "So I'll assume that you meant to type \\cr instead.",
                ]);
                let ca = self.cur_align;
                self.mem.set_extra_info(ca, CR_CODE);
                self.error()?;
            }
        }
        if self.mem.extra_info(self.cur_align) != SPAN_CODE {
            self.unsave()?;
            self.new_save_level(ALIGN_GROUP)?;
            // §796: package an unset box for the current column.
            let u: Pointer;
            let w: Scaled;
            if self.nest.cur.mode == -HMODE {
                self.adjust_tail = self.cur_tail;
                let h = self.nest.cur.head;
                let l = self.mem.link(h);
                u = self.hpack(l, 0, ADDITIONAL)?;
                w = self.mem.width(u);
                self.cur_tail = self.adjust_tail;
                self.adjust_tail = NULL;
            } else {
                let h = self.nest.cur.head;
                let l = self.mem.link(h);
                u = self.vpackage(l, 0, ADDITIONAL, 0)?;
                w = self.mem.height(u);
            }
            let mut n: i32 = 0; // min_quarterword: a span count of 1
            if self.cur_span != self.cur_align {
                // §798: update the width entry for spanned columns.
                let mut q = self.cur_span;
                loop {
                    n += 1;
                    q = self.mem.link(self.mem.link(q));
                    if q == self.cur_align {
                        break;
                    }
                }
                if n > 0xFFFF {
                    return self.confusion("256 spans").map(|_| false);
                }
                let mut q = self.cur_span;
                while self.mem.link(self.mem.info(q)) < n {
                    q = self.mem.info(q);
                }
                if self.mem.link(self.mem.info(q)) > n {
                    let s = self.mem.get_node(SPAN_NODE_SIZE)?;
                    let i = self.mem.info(q);
                    self.mem.set_info(s, i);
                    self.mem.set_link(s, n);
                    self.mem.set_info(q, s);
                    self.mem.set_width(s, w);
                } else if self.mem.width(self.mem.info(q)) < w {
                    let i = self.mem.info(q);
                    self.mem.set_width(i, w);
                }
            } else if w > self.mem.width(self.cur_align) {
                let ca = self.cur_align;
                self.mem.set_width(ca, w);
            }
            self.mem.set_node_type(u, UNSET_NODE);
            self.mem.set_span_count(u, n as u16);
            // §659: determine the stretch and shrink orders.
            let o = if self.total_stretch[3] != 0 {
                3
            } else if self.total_stretch[2] != 0 {
                2
            } else if self.total_stretch[1] != 0 {
                1
            } else {
                0
            };
            self.mem.set_glue_order(u, o as u16);
            let ts = self.total_stretch[o];
            self.mem.set_glue_stretch(u, ts);
            let o = if self.total_shrink[3] != 0 {
                3
            } else if self.total_shrink[2] != 0 {
                2
            } else if self.total_shrink[1] != 0 {
                1
            } else {
                0
            };
            self.mem.set_glue_sign(u, o as u16);
            let ts = self.total_shrink[o];
            self.mem.set_glue_shrink(u, ts);
            self.pop_nest();
            let t = self.nest.cur.tail;
            self.mem.set_link(t, u);
            self.nest.cur.tail = u;
            // §795: copy the tabskip glue between columns.
            let gp = self.mem.glue_ptr(self.mem.link(self.cur_align));
            let g = self.new_glue(gp)?;
            self.tail_append(g);
            let t = self.nest.cur.tail;
            self.mem.set_subtype(t, (TAB_SKIP_CODE + 1) as u16);
            if self.mem.extra_info(self.cur_align) >= CR_CODE {
                return Ok(true);
            }
            self.init_span(p)?;
        }
        self.inp.align_state = 1_000_000;
        // etex.ch §791: expand non-protected macros only.
        loop {
            self.get_x_or_protected()?;
            if self.cur_cmd != crate::cmds::SPACER {
                break;
            }
        }
        self.cur_align = p;
        self.init_col()?;
        Ok(false)
    }

    /// `fin_row` (§799).
    pub fn fin_row(&mut self) -> TexResult<()> {
        let p: Pointer;
        if self.nest.cur.mode == -HMODE {
            let h = self.nest.cur.head;
            let l = self.mem.link(h);
            p = self.hpack(l, 0, ADDITIONAL)?;
            self.pop_nest();
            self.append_to_vlist(p)?;
            if self.cur_head != self.cur_tail {
                let t = self.nest.cur.tail;
                let lh = self.mem.link(self.cur_head);
                self.mem.set_link(t, lh);
                self.nest.cur.tail = self.cur_tail;
            }
        } else {
            let h = self.nest.cur.head;
            let l = self.mem.link(h);
            p = self.vpack(l, 0, ADDITIONAL)?;
            self.pop_nest();
            let t = self.nest.cur.tail;
            self.mem.set_link(t, p);
            self.nest.cur.tail = p;
            self.set_space_factor(1000);
        }
        self.mem.set_node_type(p, UNSET_NODE);
        self.mem.set_glue_stretch(p, 0);
        let ec = self.eqtb.equiv(self.eqtb.lay.local_base + EVERY_CR_OFFSET);
        if ec != NULL {
            self.begin_token_list(ec, 13)?; // every_cr_text
        }
        self.align_peek()
    }

    /// `fin_align` (§800-§812).
    pub fn fin_align(&mut self) -> TexResult<()> {
        if self.save.cur_group != ALIGN_GROUP {
            return self.confusion("align1");
        }
        self.unsave()?; // that align_group was for individual entries
        if self.save.cur_group != ALIGN_GROUP {
            return self.confusion("align0");
        }
        self.unsave()?; // that align_group was for the whole alignment
        let o: Scaled = if self.nest.stack[self.nest.ptr - 1].mode == MMODE {
            self.eqtb.dimen_par(DISPLAY_INDENT_CODE)
        } else {
            0
        };
        // §801-§803: compute the column widths from the preamble.
        let mut q = self.mem.link(self.preamble());
        loop {
            let u = self.mem.u_part(q);
            self.mem.flush_list(u);
            let v = self.mem.v_part(q);
            self.mem.flush_list(v);
            let p = self.mem.link(self.mem.link(q));
            if self.mem.width(q) == NULL_FLAG {
                // §802: nullify width(q) and the tabskip glue after it.
                self.mem.set_width(q, 0);
                let r = self.mem.link(q);
                let s = self.mem.glue_ptr(r);
                if s != self.mem.zero_glue() {
                    let zg = self.mem.zero_glue();
                    self.mem.add_glue_ref(zg);
                    self.mem.delete_glue_ref(s);
                    self.mem.set_glue_ptr(r, zg);
                }
            }
            if self.mem.info(q) != self.mem.end_span() {
                // §803: merge the widths of the span nodes of q into p.
                let t = self.mem.width(q) + self.mem.width(self.mem.glue_ptr(self.mem.link(q)));
                let mut r = self.mem.info(q);
                let mut s = self.mem.end_span();
                self.mem.set_info(s, p);
                let mut n: i32 = 1;
                loop {
                    let w = self.mem.width(r) - t;
                    self.mem.set_width(r, w);
                    let u = self.mem.info(r);
                    while self.mem.link(r) > n {
                        s = self.mem.info(s);
                        n = self.mem.link(self.mem.info(s)) + 1;
                    }
                    if self.mem.link(r) < n {
                        let i = self.mem.info(s);
                        self.mem.set_info(r, i);
                        self.mem.set_info(s, r);
                        let lr = self.mem.link(r) - 1;
                        self.mem.set_link(r, lr);
                        s = r;
                    } else {
                        if self.mem.width(r) > self.mem.width(self.mem.info(s)) {
                            let i = self.mem.info(s);
                            let w = self.mem.width(r);
                            self.mem.set_width(i, w);
                        }
                        self.mem.free_node(r, SPAN_NODE_SIZE);
                    }
                    r = u;
                    if r == self.mem.end_span() {
                        break;
                    }
                }
            }
            self.mem.set_node_type(q, UNSET_NODE);
            self.mem.set_span_count(q, 0);
            self.mem.set_height(q, 0);
            self.mem.set_depth(q, 0);
            self.mem.set_glue_order(q, crate::mem::NORMAL);
            self.mem.set_glue_sign(q, crate::mem::NORMAL);
            self.mem.set_glue_stretch(q, 0);
            self.mem.set_glue_shrink(q, 0);
            q = p;
            if q == NULL {
                break;
            }
        }
        // §804: package the preamble to get the prototype box p.
        self.save.save_ptr -= 2;
        self.pack_begin_line = -self.nest.cur.ml;
        let p: Pointer;
        if self.nest.cur.mode == -VMODE {
            let rule_save = self.eqtb.dimen_par(OVERFULL_RULE_CODE);
            let loc = self.eqtb.lay.dimen_base + OVERFULL_RULE_CODE;
            self.eqtb.set_int(loc, 0); // prevent the rule from being packaged
            let pre = self.preamble();
            let (w, m) = (self.save.saved(1), self.save.saved(0));
            p = self.hpack(pre, w, m)?;
            self.eqtb.set_int(loc, rule_save);
        } else {
            let mut q = self.mem.link(self.preamble());
            loop {
                let w = self.mem.width(q);
                self.mem.set_height(q, w);
                self.mem.set_width(q, 0);
                q = self.mem.link(self.mem.link(q));
                if q == NULL {
                    break;
                }
            }
            let pre = self.preamble();
            let (w, m) = (self.save.saved(1), self.save.saved(0));
            p = self.vpack(pre, w, m)?;
            let mut q = self.mem.link(self.preamble());
            loop {
                let h = self.mem.height(q);
                self.mem.set_width(q, h);
                self.mem.set_height(q, 0);
                q = self.mem.link(self.mem.link(q));
                if q == NULL {
                    break;
                }
            }
        }
        self.pack_begin_line = 0;
        // §805: set the glue in all the unset boxes of the current list.
        let mut q = self.mem.link(self.nest.cur.head);
        let mut s = self.nest.cur.head;
        while q != NULL {
            if !self.mem.is_char_node(q) {
                if self.mem.node_type(q) == UNSET_NODE {
                    self.set_unset_row(q, p, o)?;
                } else if self.mem.node_type(q) == RULE_NODE {
                    // §806: running dimensions in rule q.
                    if crate::nodes::is_running(self.mem.width(q)) {
                        let w = self.mem.width(p);
                        self.mem.set_width(q, w);
                    }
                    if crate::nodes::is_running(self.mem.height(q)) {
                        let h = self.mem.height(p);
                        self.mem.set_height(q, h);
                    }
                    if crate::nodes::is_running(self.mem.depth(q)) {
                        let d = self.mem.depth(p);
                        self.mem.set_depth(q, d);
                    }
                    if o != 0 {
                        let r = self.mem.link(q);
                        self.mem.set_link(q, NULL);
                        let mut qq = self.hpack(q, 0, ADDITIONAL)?;
                        self.mem.set_shift_amount(qq, o);
                        self.mem.set_link(qq, r);
                        self.mem.set_link(s, qq);
                        q = qq;
                        let _ = &mut qq;
                    }
                }
            }
            s = q;
            q = self.mem.link(q);
        }
        self.flush_node_list(p);
        self.pop_alignment();
        // §812: insert the current list into its environment.
        let aux_save = self.nest.cur.aux;
        let p = self.mem.link(self.nest.cur.head);
        let q = self.nest.cur.tail;
        self.pop_nest();
        if self.mode() == MMODE {
            // §1206: finish an alignment in a display.
            self.do_assignments()?;
            if self.cur_cmd != MATH_SHIFT {
                // §1207.
                self.print_err("Missing $$ inserted");
                self.help(&[
                    "Displays can use special alignments (like \\eqalignno)",
                    "only if nothing but the alignment itself is between $$'s.",
                ]);
                self.back_error()?;
            } else {
                // check that another $ follows
                self.get_x_token()?;
                if self.cur_cmd != MATH_SHIFT {
                    self.print_err("Display math should end with $$");
                    self.help(&[
                        "The `$' that I just saw supposedly matches a previous `$$'.",
                        "So I shall assume that you typed `$$' both times.",
                    ]);
                    self.back_error()?;
                }
            }
            // etex.ch (§1206): drop the prototype box of the display.
            let lrb = self.nest.cur.etex_aux;
            self.flush_node_list(lrb);
            self.pop_nest();
            let pen = self.eqtb.int_par(PRE_DISPLAY_PENALTY_CODE);
            let pn = self.new_penalty(pen)?;
            self.tail_append(pn);
            let g = self.new_param_glue(ABOVE_DISPLAY_SKIP_CODE)?;
            self.tail_append(g);
            let t = self.nest.cur.tail;
            self.mem.set_link(t, p);
            if p != NULL {
                self.nest.cur.tail = q;
            }
            let pen = self.eqtb.int_par(POST_DISPLAY_PENALTY_CODE);
            let pn = self.new_penalty(pen)?;
            self.tail_append(pn);
            let g = self.new_param_glue(BELOW_DISPLAY_SKIP_CODE)?;
            self.tail_append(g);
            self.set_prev_depth(aux_save.sc());
            self.resume_after_display()?;
        } else {
            self.nest.cur.aux = aux_save;
            let t = self.nest.cur.tail;
            self.mem.set_link(t, p);
            if p != NULL {
                self.nest.cur.tail = q;
            }
            if self.mode() == VMODE {
                self.build_page()?;
            }
        }
        Ok(())
    }

    /// §807-§808: set the unset row `q` and the unset boxes in it.
    fn set_unset_row(&mut self, q: Pointer, p: Pointer, o: Scaled) -> TexResult<()> {
        if self.nest.cur.mode == -VMODE {
            self.mem.set_node_type(q, HLIST_NODE);
            let w = self.mem.width(p);
            self.mem.set_width(q, w);
            // etex.ch §807: rows of an alignment inside display math form
            // display lists for ship_out (TeXXeT).
            let outer = self.nest.stack[self.nest.ptr - 1].mode;
            if outer == crate::engine::MMODE {
                self.mem.set_subtype(q, crate::nodes::DLIST);
            }
        } else {
            self.mem.set_node_type(q, VLIST_NODE);
            let h = self.mem.height(p);
            self.mem.set_height(q, h);
        }
        let go = self.mem.glue_order(p);
        self.mem.set_glue_order(q, go);
        let gs = self.mem.glue_sign(p);
        self.mem.set_glue_sign(q, gs);
        let g = self.mem.glue_set(p);
        self.mem.set_glue_set(q, g);
        self.mem.set_shift_amount(q, o);
        let mut r = self.mem.link(self.mem.list_ptr(q));
        let mut s = self.mem.link(self.mem.list_ptr(p));
        loop {
            // §808: set the glue in node r.
            let mut n = i32::from(self.mem.span_count(r));
            let mut t = self.mem.width(s);
            let w = t;
            let hh = self.mem.hold_head();
            let mut u = hh;
            self.mem.set_link(u, NULL);
            // etex.ch (§808): clear box_lr for ship_out.
            self.mem.set_subtype(r, 0);
            while n > 0 {
                n -= 1;
                // §809: append tabskip glue and an empty box to list u.
                s = self.mem.link(s);
                let v = self.mem.glue_ptr(s);
                let g = self.new_glue(v)?;
                self.mem.set_link(u, g);
                u = g;
                self.mem.set_subtype(u, (TAB_SKIP_CODE + 1) as u16);
                t += self.mem.width(v);
                if self.mem.glue_sign(p) == STRETCHING {
                    if self.mem.stretch_order(v) == self.mem.glue_order(p) {
                        t += (self.mem.glue_set(p) * f64::from(self.mem.stretch(v))).round()
                            as Scaled;
                    }
                } else if self.mem.glue_sign(p) == SHRINKING
                    && self.mem.shrink_order(v) == self.mem.glue_order(p)
                {
                    t -= (self.mem.glue_set(p) * f64::from(self.mem.shrink(v))).round() as Scaled;
                }
                s = self.mem.link(s);
                let b = self.new_null_box()?;
                self.mem.set_link(u, b);
                u = b;
                t += self.mem.width(s);
                if self.nest.cur.mode == -VMODE {
                    let w = self.mem.width(s);
                    self.mem.set_width(u, w);
                } else {
                    self.mem.set_node_type(u, VLIST_NODE);
                    let w = self.mem.width(s);
                    self.mem.set_height(u, w);
                }
            }
            if self.nest.cur.mode == -VMODE {
                // §810: make the unset node r into an hlist_node of width w.
                let h = self.mem.height(q);
                self.mem.set_height(r, h);
                let d = self.mem.depth(q);
                self.mem.set_depth(r, d);
                if t == self.mem.width(r) {
                    self.mem.set_glue_sign(r, crate::mem::NORMAL);
                    self.mem.set_glue_order(r, crate::mem::NORMAL);
                    self.mem.set_glue_set(r, 0.0);
                } else if t > self.mem.width(r) {
                    self.mem.set_glue_sign(r, STRETCHING);
                    if self.mem.glue_stretch(r) == 0 {
                        self.mem.set_glue_set(r, 0.0);
                    } else {
                        let gs =
                            f64::from(t - self.mem.width(r)) / f64::from(self.mem.glue_stretch(r));
                        self.mem.set_glue_set(r, gs);
                    }
                } else {
                    let gs = self.mem.glue_sign(r);
                    self.mem.set_glue_order(r, gs);
                    self.mem.set_glue_sign(r, SHRINKING);
                    if self.mem.glue_shrink(r) == 0 {
                        self.mem.set_glue_set(r, 0.0);
                    } else if self.mem.glue_order(r) == crate::mem::NORMAL
                        && self.mem.width(r) - t > self.mem.glue_shrink(r)
                    {
                        self.mem.set_glue_set(r, 1.0);
                    } else {
                        let gs =
                            f64::from(self.mem.width(r) - t) / f64::from(self.mem.glue_shrink(r));
                        self.mem.set_glue_set(r, gs);
                    }
                }
                self.mem.set_width(r, w);
                self.mem.set_node_type(r, HLIST_NODE);
            } else {
                // §811: make the unset node r into a vlist_node of height w.
                let wq = self.mem.width(q);
                self.mem.set_width(r, wq);
                if t == self.mem.height(r) {
                    self.mem.set_glue_sign(r, crate::mem::NORMAL);
                    self.mem.set_glue_order(r, crate::mem::NORMAL);
                    self.mem.set_glue_set(r, 0.0);
                } else if t > self.mem.height(r) {
                    self.mem.set_glue_sign(r, STRETCHING);
                    if self.mem.glue_stretch(r) == 0 {
                        self.mem.set_glue_set(r, 0.0);
                    } else {
                        let gs =
                            f64::from(t - self.mem.height(r)) / f64::from(self.mem.glue_stretch(r));
                        self.mem.set_glue_set(r, gs);
                    }
                } else {
                    let gs = self.mem.glue_sign(r);
                    self.mem.set_glue_order(r, gs);
                    self.mem.set_glue_sign(r, SHRINKING);
                    if self.mem.glue_shrink(r) == 0 {
                        self.mem.set_glue_set(r, 0.0);
                    } else if self.mem.glue_order(r) == crate::mem::NORMAL
                        && self.mem.height(r) - t > self.mem.glue_shrink(r)
                    {
                        self.mem.set_glue_set(r, 1.0);
                    } else {
                        let gs =
                            f64::from(self.mem.height(r) - t) / f64::from(self.mem.glue_shrink(r));
                        self.mem.set_glue_set(r, gs);
                    }
                }
                self.mem.set_height(r, w);
                self.mem.set_node_type(r, VLIST_NODE);
            }
            self.mem.set_shift_amount(r, 0);
            if u != hh {
                // append blank boxes to account for spanned nodes
                let lr = self.mem.link(r);
                self.mem.set_link(u, lr);
                let lh = self.mem.link(hh);
                self.mem.set_link(r, lh);
                r = u;
            }
            r = self.mem.link(self.mem.link(r));
            s = self.mem.link(self.mem.link(s));
            if r == NULL {
                break;
            }
        }
        Ok(())
    }

    /// `do_endv` (§1131).
    pub fn do_endv(&mut self) -> TexResult<()> {
        self.inp.stack[self.inp.input_ptr] = self.inp.cur;
        let mut base = self.inp.input_ptr;
        while self.inp.stack[base].index != V_TEMPLATE
            && self.inp.stack[base].loc == NULL
            && self.inp.stack[base].state == TOKEN_LIST
        {
            base -= 1;
        }
        if self.inp.stack[base].index != V_TEMPLATE
            || self.inp.stack[base].loc != NULL
            || self.inp.stack[base].state != TOKEN_LIST
        {
            return Err(self.fatal_error("(interwoven alignment preambles are not allowed)"));
        }
        if self.save.cur_group == ALIGN_GROUP {
            self.end_graf()?;
            if self.fin_col()? {
                self.fin_row()?;
            }
            Ok(())
        } else {
            self.off_save()
        }
    }
}
