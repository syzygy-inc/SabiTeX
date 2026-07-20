//! Displaying, destroying, and copying boxes.
//!
//! Ports tex.web Part 12 (§173-§198: `short_display`, `show_node_list`,
//! `show_box`), Part 13 (§199-§202: `flush_node_list`) and Part 14
//! (§203-§206: `copy_node_list`). Whatsit nodes (`\write`/`\special`)
//! arrive with the extensions (M2 tail / Part 53); until they can be
//! created, the whatsit branches are `confusion`.

use crate::engine::Engine;
use crate::error::TexResult;
use crate::mem::GLUE_SPEC_SIZE;
use crate::nodes::*;
use crate::types::{Pointer, Scaled, NULL, UNITY};

impl Engine {
    /// `short_display(p)` (§174-§175): top-level highlights of a list.
    pub fn short_display(&mut self, p: Pointer) {
        let mut p = p;
        while p > self.mem.mem_bot {
            if self.mem.is_char_node(p) {
                if p <= self.mem.mem_end {
                    let f = i32::from(self.mem.font(p));
                    if f != self.font_in_short_display {
                        if !(0..=self.sizes.font_max).contains(&f) {
                            self.print_char('*' as i32);
                        } else {
                            let t = self.eqtb.font_id_text(f);
                            self.print_esc(t);
                        }
                        self.print_char(' ' as i32);
                        self.font_in_short_display = f;
                    }
                    if self.fonts.dir[f as usize] != 0 {
                        // pTeX: the code node follows; print the USV.
                        p = self.mem.link(p);
                        let c = self.mem.info(p);
                        self.print_char_code(c);
                    } else {
                        let c = i32::from(self.mem.character(p));
                        self.print_char_code(c);
                    }
                }
            } else {
                // §175: a short indication of the contents of node p.
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | INS_NODE | WHATSIT_NODE | MARK_NODE | ADJUST_NODE
                    | UNSET_NODE => self.print_chars("[]"),
                    RULE_NODE => self.print_char('|' as i32),
                    GLUE_NODE => {
                        if self.mem.glue_ptr(p) != self.mem.zero_glue() {
                            self.print_char(' ' as i32);
                        }
                    }
                    MATH_NODE => {
                        // etex.ch: direction nodes display as [].
                        if self.mem.subtype(p) >= L_CODE {
                            self.print_chars("[]");
                        } else {
                            self.print_char('$' as i32);
                        }
                    }
                    LIGATURE_NODE => {
                        let l = self.mem.lig_ptr(p);
                        self.short_display(l);
                    }
                    DISC_NODE => {
                        let pre = self.mem.pre_break(p);
                        self.short_display(pre);
                        let post = self.mem.post_break(p);
                        self.short_display(post);
                        let mut n = self.mem.replace_count(p);
                        while n > 0 {
                            if self.mem.link(p) != NULL {
                                p = self.mem.link(p);
                            }
                            n -= 1;
                        }
                    }
                    _ => {}
                }
            }
            p = self.mem.link(p);
        }
    }

    /// `print_font_and_char(p)` (§176).
    pub fn print_font_and_char(&mut self, p: Pointer) {
        if p > self.mem.mem_end {
            self.print_esc_str("CLOBBERED.");
        } else {
            let f = i32::from(self.mem.font(p));
            if !(0..=self.sizes.font_max).contains(&f) {
                self.print_char('*' as i32);
            } else {
                let t = self.eqtb.font_id_text(f);
                self.print_esc(t);
            }
            self.print_char(' ' as i32);
            if (0..=self.sizes.font_max).contains(&f) && self.fonts.dir[f as usize] != 0 {
                // pTeX [12.183]: print the character from the code node.
                let c = self.mem.info(self.mem.link(p));
                self.print_char_code(c);
            } else {
                let c = i32::from(self.mem.character(p));
                self.print_char_code(c);
            }
        }
    }

    /// `print_mark(p)` (§176).
    pub fn print_mark(&mut self, p: Pointer) {
        self.print_char('{' as i32);
        if p < self.mem.hi_mem_min || p > self.mem.mem_end {
            self.print_esc_str("CLOBBERED.");
        } else {
            let l = self.mem.link(p);
            let lim = self.sizes.max_print_line as i32 - 10;
            self.show_token_list(l, NULL, lim);
        }
        self.print_char('}' as i32);
    }

    /// `print_rule_dimen(d)` (§176).
    pub fn print_rule_dimen(&mut self, d: Scaled) {
        if is_running_dimen(d) {
            self.print_char('*' as i32);
        } else {
            self.print_scaled(d);
        }
    }

    /// `print_skip_param(n)` (§225 + ptex-base.ch).
    pub fn print_skip_param(&mut self, n: i32) {
        if n == i32::from(crate::kanji::JFM_SKIP) {
            self.print_chars("refer from jfm");
            return;
        }
        match crate::eqtb::SKIP_PARAM_NAMES.get(n as usize) {
            Some(s) => self.print_esc_str(s),
            None => self.print_chars("[unknown glue parameter!]"),
        }
    }

    /// `show_node_list(p)` (§182): prints a node list symbolically, with
    /// nesting recorded in the current string.
    pub fn show_node_list(&mut self, p: Pointer) {
        if self.strings.cur_length() as i32 > self.depth_threshold {
            if p > NULL {
                self.print_chars(" []"); // there's been some truncation
            }
            return;
        }
        let mut n = 0;
        let mut p = p;
        while p > self.mem.mem_bot {
            self.print_ln();
            self.print_current_string(); // display the nesting history
            if p > self.mem.mem_end {
                self.print_chars("Bad link, display aborted.");
                return;
            }
            n += 1;
            if n > self.breadth_max {
                self.print_chars("etc.");
                return;
            }
            // §183 (+ pTeX [12.183]): display node p.
            if self.mem.is_char_node(p) {
                self.print_font_and_char(p);
                if self.is_kanji_head(p) {
                    p = self.mem.link(p); // skip the KANJI code node
                }
            } else {
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | UNSET_NODE => self.display_box(p),
                    RULE_NODE => {
                        // §187.
                        self.print_esc_str("rule(");
                        let h = self.mem.height(p);
                        self.print_rule_dimen(h);
                        self.print_char('+' as i32);
                        let d = self.mem.depth(p);
                        self.print_rule_dimen(d);
                        self.print_chars(")x");
                        let w = self.mem.width(p);
                        self.print_rule_dimen(w);
                    }
                    INS_NODE => {
                        // §188.
                        self.print_esc_str("insert");
                        let s = i32::from(self.mem.subtype(p));
                        self.print_int(s);
                        self.print_chars(", natural size ");
                        let h = self.mem.height(p);
                        self.print_scaled(h);
                        self.print_chars("; split(");
                        let st = self.mem.split_top_ptr(p);
                        self.print_spec(st, "");
                        self.print_char(',' as i32);
                        let d = self.mem.depth(p);
                        self.print_scaled(d);
                        self.print_chars("); float cost ");
                        let fc = self.mem.float_cost(p);
                        self.print_int(fc);
                        let i = self.mem.ins_ptr(p);
                        self.node_list_display(i);
                    }
                    WHATSIT_NODE => self.show_whatsit(p), // §1356
                    GLUE_NODE => self.display_glue(p),
                    KERN_NODE => {
                        // §191.
                        if self.mem.subtype(p) != MU_GLUE {
                            self.print_esc_str("kern");
                            if self.mem.subtype(p) != NORMAL {
                                self.print_char(' ' as i32);
                            }
                            let w = self.mem.width(p);
                            self.print_scaled(w);
                            if self.mem.subtype(p) == ACC_KERN {
                                self.print_chars(" (for accent)");
                            }
                        } else {
                            self.print_esc_str("mkern");
                            let w = self.mem.width(p);
                            self.print_scaled(w);
                            self.print_chars("mu");
                        }
                    }
                    MATH_NODE => {
                        // §192 (+ etex.ch: the direction nodes).
                        if self.mem.subtype(p) > AFTER {
                            if end_lr(self.mem.subtype(p)) {
                                self.print_esc_str("end");
                            } else {
                                self.print_esc_str("begin");
                            }
                            if self.mem.subtype(p) > R_CODE {
                                self.print_char('R' as i32);
                            } else if self.mem.subtype(p) > L_CODE {
                                self.print_char('L' as i32);
                            } else {
                                self.print_char('M' as i32);
                            }
                        } else {
                            self.print_esc_str("math");
                            if self.mem.subtype(p) == BEFORE {
                                self.print_chars("on");
                            } else {
                                self.print_chars("off");
                            }
                            if self.mem.width(p) != 0 {
                                self.print_chars(", surrounded ");
                                let w = self.mem.width(p);
                                self.print_scaled(w);
                            }
                        }
                    }
                    LIGATURE_NODE => {
                        // §193.
                        let lc = self.mem.lig_char(p);
                        self.print_font_and_char(lc);
                        self.print_chars(" (ligature ");
                        if self.mem.subtype(p) > 1 {
                            self.print_char('|' as i32);
                        }
                        self.font_in_short_display = i32::from(self.mem.font(lc));
                        let l = self.mem.lig_ptr(p);
                        self.short_display(l);
                        if self.mem.subtype(p) % 2 == 1 {
                            self.print_char('|' as i32);
                        }
                        self.print_char(')' as i32);
                    }
                    crate::nodes::DISP_NODE => {
                        // ptex-base.ch [12.183].
                        self.print_esc_str("displace ");
                        let v = self.mem.width(p);
                        self.print_scaled(v);
                    }
                    PENALTY_NODE => {
                        self.print_esc_str("penalty ");
                        let v = self.mem.penalty(p);
                        self.print_int(v);
                        // ptex-base.ch: annotate Japanese penalties.
                        if self.mem.subtype(p) == crate::kanji::WIDOW_PENA {
                            self.print_chars(r"(for \jcharwidowpenalty)");
                        } else if self.mem.subtype(p) == crate::kanji::KINSOKU_PENA {
                            self.print_chars("(for kinsoku)");
                        }
                    }
                    DISC_NODE => {
                        // §195.
                        self.print_esc_str("discretionary");
                        if self.mem.replace_count(p) > 0 {
                            self.print_chars(" replacing ");
                            let rc = i32::from(self.mem.replace_count(p));
                            self.print_int(rc);
                        }
                        let pre = self.mem.pre_break(p);
                        self.node_list_display(pre);
                        self.strings.append_char('|' as i32);
                        let post = self.mem.post_break(p);
                        self.show_node_list(post);
                        self.strings.flush_char();
                    }
                    MARK_NODE => {
                        self.print_esc_str("mark");
                        if self.mem.mark_class(p) != 0 {
                            // etex.ch: a nonzero mark class.
                            self.print_char('s' as i32);
                            let c = self.mem.mark_class(p);
                            self.print_int(c);
                        }
                        let m = self.mem.mark_ptr(p);
                        self.print_mark(m);
                    }
                    ADJUST_NODE => {
                        self.print_esc_str("vadjust");
                        let a = self.mem.adjust_ptr(p);
                        self.node_list_display(a);
                    }
                    t if t >= crate::math::STYLE_NODE => self.show_math_node(p), // §690
                    _ => self.print_chars("Unknown node type!"),
                }
            }
            p = self.mem.link(p);
        }
    }

    /// `node_list_display(p)` (§180): recursive call with "." nesting.
    fn node_list_display(&mut self, p: Pointer) {
        self.strings.append_char('.' as i32);
        self.show_node_list(p);
        self.strings.flush_char();
    }

    /// §184-§186: display a box node.
    fn display_box(&mut self, p: Pointer) {
        let t = self.mem.node_type(p);
        if t == HLIST_NODE {
            self.print_esc_str("h");
        } else if t == VLIST_NODE {
            self.print_esc_str("v");
        } else {
            self.print_esc_str("unset");
        }
        self.print_chars("box(");
        let h = self.mem.height(p);
        self.print_scaled(h);
        self.print_char('+' as i32);
        let d = self.mem.depth(p);
        self.print_scaled(d);
        self.print_chars(")x");
        let w = self.mem.width(p);
        self.print_scaled(w);
        if self.jfont_seen
            && t != UNSET_NODE
            && self.strings.cur_length() == 0
            && !self.in_pack_diagnostic
        {
            // ptex-base.ch: box direction annotation, printed only when
            // the box direction differs from the display context — i.e.
            // at nesting depth 0, since everything is yoko until tate
            // exists. Gated on jfont_seen so TRIP/e-TRIP logs stay
            // tex.web-identical (euptex prints unconditionally; its trip
            // reference logs differ from Knuth's).
            self.print_chars(", yoko direction");
        }
        if t == UNSET_NODE {
            // §185.
            if self.mem.span_count(p) != 0 {
                self.print_chars(" (");
                let sc = i32::from(self.mem.span_count(p)) + 1;
                self.print_int(sc);
                self.print_chars(" columns)");
            }
            if self.mem.glue_stretch(p) != 0 {
                self.print_chars(", stretch ");
                let (g, o) = (self.mem.glue_stretch(p), self.mem.glue_order(p));
                self.print_glue(g, o, "");
            }
            if self.mem.glue_shrink(p) != 0 {
                self.print_chars(", shrink ");
                let (g, o) = (self.mem.glue_shrink(p), self.mem.glue_sign(p));
                self.print_glue(g, o, "");
            }
        } else {
            // §186: the glue set value.
            let g = self.mem.glue_set(p);
            if g != 0.0 && self.mem.glue_sign(p) != NORMAL {
                self.print_chars(", glue set ");
                if self.mem.glue_sign(p) == SHRINKING {
                    self.print_chars("- ");
                }
                if g.abs() > 20000.0 {
                    if g > 0.0 {
                        self.print_char('>' as i32);
                    } else {
                        self.print_chars("< -");
                    }
                    let o = self.mem.glue_order(p);
                    self.print_glue(20000 * UNITY, o, "");
                } else {
                    let o = self.mem.glue_order(p);
                    self.print_glue((f64::from(UNITY) * g).round() as Scaled, o, "");
                }
            }
            if self.mem.shift_amount(p) != 0 {
                self.print_chars(", shifted ");
                let s = self.mem.shift_amount(p);
                self.print_scaled(s);
            }
            // etex.ch: display lists are never reversed by ship_out.
            if self.etex_ex()
                && self.mem.node_type(p) == HLIST_NODE
                && self.mem.subtype(p) == crate::nodes::DLIST
            {
                self.print_chars(", display");
            }
        }
        let l = self.mem.list_ptr(p);
        self.node_list_display(l);
    }

    /// §189-§190: display a glue node.
    fn display_glue(&mut self, p: Pointer) {
        let s = self.mem.subtype(p);
        if s >= A_LEADERS {
            // §190.
            self.print_esc_str("");
            if s == C_LEADERS {
                self.print_char('c' as i32);
            } else if s == X_LEADERS {
                self.print_char('x' as i32);
            }
            self.print_chars("leaders ");
            let g = self.mem.glue_ptr(p);
            self.print_spec(g, "");
            let l = self.mem.leader_ptr(p);
            self.node_list_display(l);
        } else {
            self.print_esc_str("glue");
            if s != NORMAL {
                self.print_char('(' as i32);
                if s < COND_MATH_GLUE {
                    self.print_skip_param(i32::from(s) - 1);
                } else if s == COND_MATH_GLUE {
                    self.print_esc_str("nonscript");
                } else {
                    self.print_esc_str("mskip");
                }
                self.print_char(')' as i32);
            }
            if s != COND_MATH_GLUE {
                self.print_char(' ' as i32);
                let g = self.mem.glue_ptr(p);
                if s < COND_MATH_GLUE {
                    self.print_spec(g, "");
                } else {
                    self.print_spec(g, "mu");
                }
            }
        }
    }

    /// `show_box(p)` (§198).
    pub fn show_box(&mut self, p: Pointer) {
        // §236: depth_threshold/breadth_max from \showboxdepth/\showboxbreadth.
        self.depth_threshold = self.eqtb.int_par(crate::eqtb::SHOW_BOX_DEPTH_CODE);
        self.breadth_max = self.eqtb.int_par(crate::eqtb::SHOW_BOX_BREADTH_CODE);
        if self.breadth_max <= 0 {
            self.breadth_max = 5;
        }
        if self.strings.pool_ptr() as i32 + self.depth_threshold >= self.sizes.pool_size as i32 {
            self.depth_threshold = self.sizes.pool_size as i32 - self.strings.pool_ptr() as i32 - 1;
        }
        self.show_node_list(p);
        self.print_ln();
    }

    /// `flush_node_list(p)` (§202): erase a list of nodes recursively.
    pub fn flush_node_list(&mut self, p: Pointer) {
        let mut p = p;
        while p != NULL {
            let q = self.mem.link(p);
            if self.mem.is_char_node(p) {
                self.mem.free_avail(p);
            } else {
                let mut small = true;
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | UNSET_NODE => {
                        let l = self.mem.list_ptr(p);
                        self.flush_node_list(l);
                        self.mem.free_node(p, BOX_NODE_SIZE);
                        small = false;
                    }
                    RULE_NODE => {
                        self.mem.free_node(p, RULE_NODE_SIZE);
                        small = false;
                    }
                    INS_NODE => {
                        let i = self.mem.ins_ptr(p);
                        self.flush_node_list(i);
                        let st = self.mem.split_top_ptr(p);
                        self.mem.delete_glue_ref(st);
                        self.mem.free_node(p, INS_NODE_SIZE);
                        small = false;
                    }
                    WHATSIT_NODE => {
                        // §1358.
                        self.free_whatsit(p);
                        small = false;
                    }
                    GLUE_NODE => {
                        let g = self.mem.glue_ptr(p);
                        if self.mem.glue_ref_count(g) == NULL {
                            self.mem.free_node(g, GLUE_SPEC_SIZE);
                        } else {
                            let c = self.mem.glue_ref_count(g);
                            self.mem.set_glue_ref_count(g, c - 1);
                        }
                        let l = self.mem.leader_ptr(p);
                        if l != NULL {
                            self.flush_node_list(l);
                        }
                    }
                    KERN_NODE | MATH_NODE | PENALTY_NODE | crate::nodes::DISP_NODE => {}
                    LIGATURE_NODE => {
                        let l = self.mem.lig_ptr(p);
                        self.flush_node_list(l);
                    }
                    MARK_NODE => {
                        let m = self.mem.mark_ptr(p);
                        self.delete_token_ref(m);
                    }
                    DISC_NODE => {
                        let pre = self.mem.pre_break(p);
                        self.flush_node_list(pre);
                        let post = self.mem.post_break(p);
                        self.flush_node_list(post);
                    }
                    ADJUST_NODE => {
                        let a = self.mem.adjust_ptr(p);
                        self.flush_node_list(a);
                    }
                    t if t >= crate::math::STYLE_NODE => {
                        // §698: the mlist cases.
                        self.flush_math_node(p);
                        small = false;
                    }
                    _ => {
                        // confusion("flushing") — but flush runs in cleanup
                        // paths where Err would be awkward; tex aborts here.
                        debug_assert!(false, "flushing unknown node type");
                    }
                }
                if small {
                    self.mem.free_node(p, SMALL_NODE_SIZE);
                }
            }
            p = q;
        }
    }

    /// `copy_node_list(p)` (§204-§206): duplicates a node list.
    pub fn copy_node_list(&mut self, p: Pointer) -> TexResult<Pointer> {
        let h = self.mem.get_avail()?;
        let mut q = h;
        let mut p = p;
        while p != NULL {
            // §205: make a copy of node p in node r.
            let mut words: i32 = 1;
            let r: Pointer;
            if self.mem.is_char_node(p) {
                r = self.mem.get_avail()?;
            } else {
                // §206.
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | UNSET_NODE => {
                        r = self.mem.get_node(BOX_NODE_SIZE)?;
                        *self.mem.word_mut(r + 6) = self.mem.word(p + 6);
                        *self.mem.word_mut(r + 5) = self.mem.word(p + 5);
                        let l = self.mem.list_ptr(p);
                        let lc = self.copy_node_list(l)?;
                        self.mem.set_list_ptr(r, lc);
                        words = 5;
                    }
                    RULE_NODE => {
                        r = self.mem.get_node(RULE_NODE_SIZE)?;
                        words = RULE_NODE_SIZE;
                    }
                    INS_NODE => {
                        r = self.mem.get_node(INS_NODE_SIZE)?;
                        *self.mem.word_mut(r + 4) = self.mem.word(p + 4);
                        let st = self.mem.split_top_ptr(p);
                        self.mem.add_glue_ref(st);
                        let i = self.mem.ins_ptr(p);
                        let ic = self.copy_node_list(i)?;
                        self.mem.set_ins_ptr(r, ic);
                        words = INS_NODE_SIZE - 1;
                    }
                    GLUE_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        let g = self.mem.glue_ptr(p);
                        self.mem.add_glue_ref(g);
                        self.mem.set_glue_ptr(r, g);
                        let l = self.mem.leader_ptr(p);
                        let lc = self.copy_node_list(l)?;
                        self.mem.set_leader_ptr(r, lc);
                    }
                    KERN_NODE | MATH_NODE | PENALTY_NODE | crate::nodes::DISP_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        words = SMALL_NODE_SIZE;
                    }
                    LIGATURE_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        *self.mem.word_mut(r + 1) = self.mem.word(p + 1);
                        let l = self.mem.lig_ptr(p);
                        let lc = self.copy_node_list(l)?;
                        self.mem.set_lig_ptr(r, lc);
                    }
                    DISC_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        let pre = self.mem.pre_break(p);
                        let prec = self.copy_node_list(pre)?;
                        self.mem.set_pre_break(r, prec);
                        let post = self.mem.post_break(p);
                        let postc = self.copy_node_list(post)?;
                        self.mem.set_post_break(r, postc);
                    }
                    MARK_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        let m = self.mem.mark_ptr(p);
                        self.add_token_ref(m);
                        words = SMALL_NODE_SIZE;
                    }
                    ADJUST_NODE => {
                        r = self.mem.get_node(SMALL_NODE_SIZE)?;
                        let a = self.mem.adjust_ptr(p);
                        let ac = self.copy_node_list(a)?;
                        self.mem.set_adjust_ptr(r, ac);
                    }
                    WHATSIT_NODE => {
                        // §1357.
                        let (rr, w) = self.copy_whatsit(p)?;
                        r = rr;
                        words = w;
                    }
                    _ => {
                        return self.confusion("copying").map(|_| NULL);
                    }
                }
            }
            while words > 0 {
                words -= 1;
                *self.mem.word_mut(r + words) = self.mem.word(p + words);
            }
            self.mem.set_link(q, r);
            q = r;
            p = self.mem.link(p);
        }
        self.mem.set_link(q, NULL);
        let result = self.mem.link(h);
        self.mem.free_avail(h);
        Ok(result)
    }
}

/// `is_running(d)` (§138) as a free helper.
fn is_running_dimen(d: Scaled) -> bool {
    d == NULL_FLAG
}
