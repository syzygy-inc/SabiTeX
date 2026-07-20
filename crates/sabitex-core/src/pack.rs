//! Packaging: `hpack` and `vpack`.
//!
//! Ports tex.web Part 33 (§644-§679): `scan_spec`, `hpack`, `vpackage` and
//! `append_to_vlist`. The glue-set ratio is computed in `f64` (web2c's
//! `glue_ratio`), and badness goes through [`crate::arith::badness`].

use crate::engine::Engine;
use crate::error::TexResult;
use crate::mem::{FIL, FILL, FILLL, NORMAL as G_NORMAL};
use crate::nodes::*;
use crate::types::{Pointer, Scaled, NULL};

// §644: scan_spec codes.
pub const EXACTLY: i32 = 0;
pub const ADDITIONAL: i32 = 1;

impl Engine {
    /// `put_LR(t)` (etex.ch): pushes an end code onto the LR stack.
    pub fn put_lr(&mut self, t: i32) -> TexResult<()> {
        let p = self.mem.get_avail()?;
        self.mem.set_info(p, t);
        let l = self.lr_ptr;
        self.mem.set_link(p, l);
        self.lr_ptr = p;
        Ok(())
    }

    /// `push_LR(p)` (etex.ch).
    pub fn push_lr(&mut self, p: Pointer) -> TexResult<()> {
        let t = i32::from(crate::nodes::end_lr_type(self.mem.subtype(p)));
        self.put_lr(t)
    }

    /// `pop_LR` (etex.ch).
    pub fn pop_lr(&mut self) {
        let p = self.lr_ptr;
        self.lr_ptr = self.mem.link(p);
        self.mem.free_avail(p);
    }

    /// etex.ch: check for LR anomalies at the end of `hpack`.
    fn hpack_check_lr(&mut self, r: Pointer) -> TexResult<()> {
        use crate::nodes::BEFORE;
        if self.mem.info(self.lr_ptr) != i32::from(BEFORE) {
            let mut q = r + 5; // list_offset
            while self.mem.link(q) != NULL {
                q = self.mem.link(q);
            }
            loop {
                let t = self.mem.info(self.lr_ptr) as u16;
                let n = self.new_math(0, t)?;
                self.mem.set_link(q, n);
                q = n;
                self.lr_problems += 10000;
                self.pop_lr();
                if self.mem.info(self.lr_ptr) == i32::from(BEFORE) {
                    break;
                }
            }
        }
        if self.lr_problems > 0 {
            // Report LR problems, then run the common diagnostic ending.
            self.print_ln();
            self.print_nl_chars("\\endL or \\endR problem (");
            let missing = self.lr_problems / 10000;
            self.print_int(missing);
            self.print_chars(" missing, ");
            let extra = self.lr_problems % 10000;
            self.print_int(extra);
            self.print_chars(" extra");
            self.lr_problems = 0;
            self.hpack_common_ending(r)?;
            // In etex.ch the goto common_ending falls through to exit:,
            // which RE-ENTERS this check block; with the stack already
            // completed and LR_problems reset, that second pass just pops
            // the sentinel. Do that directly.
        }
        self.pop_lr();
        if self.lr_ptr != NULL {
            return self.confusion("LR1");
        }
        Ok(())
    }
    /// `scan_spec(c, three_codes)` (§645): scans a box specification and
    /// the left brace.
    pub fn scan_spec(&mut self, c: u16, three_codes: bool) -> TexResult<()> {
        let s = if three_codes { self.save.saved(0) } else { 0 };
        let spec_code;
        if self.scan_keyword("to")? {
            spec_code = EXACTLY;
            self.scan_normal_dimen()?;
        } else if self.scan_keyword("spread")? {
            spec_code = ADDITIONAL;
            self.scan_normal_dimen()?;
        } else {
            spec_code = ADDITIONAL;
            self.cur_val = 0;
        }
        if three_codes {
            self.save.set_saved(0, s);
            self.save.save_ptr += 1;
        }
        self.save.set_saved(0, spec_code);
        let v = self.cur_val;
        self.save.set_saved(1, v);
        self.save.save_ptr += 2;
        self.new_save_level(c)?;
        self.scan_left_brace()
    }

    /// `hpack(p, w, m)` (§649-§663 + etex.ch LR checking).
    pub fn hpack(&mut self, p: Pointer, w: Scaled, m: i32) -> TexResult<Pointer> {
        let texxet = self.texxet_en();
        if texxet {
            // Initialize the LR stack (a sentinel that never matches).
            self.put_lr(i32::from(crate::nodes::BEFORE))?;
        }
        let r = self.hpack_inner(p, w, m)?;
        // pTeX: remember which implicit skips this box was measured
        // with (side table; see specification/japanese.md).
        if self.cur_kanji_skip != crate::types::NULL
            && (self.cur_kanji_skip != self.mem.zero_glue()
                || self.cur_xkanji_skip != self.mem.zero_glue())
        {
            self.box_spacing
                .insert(r, (self.cur_kanji_skip, self.cur_xkanji_skip));
        }
        if texxet {
            self.hpack_check_lr(r)?;
        }
        Ok(r)
    }

    fn hpack_inner(&mut self, p: Pointer, w: Scaled, m: i32) -> TexResult<Pointer> {
        self.last_badness = 0;
        let r = self.mem.get_node(BOX_NODE_SIZE)?;
        self.mem.set_node_type(r, HLIST_NODE);
        self.mem.set_subtype(r, 0);
        self.mem.set_shift_amount(r, 0);
        let mut q = r + 5; // list_offset
        self.mem.set_link(q, p);
        let mut h: Scaled = 0;
        // §650: clear dimensions to zero.
        let mut d: Scaled = 0;
        let mut x: Scaled = 0;
        self.total_stretch = [0; 4];
        self.total_shrink = [0; 4];
        let mut p = p;
        while p != NULL {
            // §651: examine node p.
            'reswitch: loop {
                while self.mem.is_char_node(p) {
                    // §654 (+ pTeX): incorporate character dimensions.
                    // A Japanese pair's first node holds the JFM class in
                    // its character field, so char_info works unchanged;
                    // the code node that follows is skipped.
                    let f = i32::from(self.mem.font(p));
                    let i = self.fonts.char_info(f, i32::from(self.mem.character(p)));
                    let hd = crate::fonts::FontMem::height_depth(i);
                    x += self.fonts.char_width(f, i);
                    let s = self.fonts.char_height(f, hd);
                    if s > h {
                        h = s;
                    }
                    let s = self.fonts.char_depth(f, hd);
                    if s > d {
                        d = s;
                    }
                    let p_is_kanji = self.fonts.dir[f as usize] != 0;
                    if p_is_kanji {
                        p = self.mem.link(p); // skip the KANJI code node
                    }
                    p = self.mem.link(p);
                    // pTeX: implicit \kanjiskip / \xkanjiskip between
                    // character nodes — width and stretch/shrink count,
                    // but no node appears in the list.
                    if p != NULL && self.mem.is_char_node(p) {
                        // (kanji-kanji only: the alphabetic boundary gets
                        // a real \xkanjiskip node from adjust_hlist.)
                        let g = if p_is_kanji && self.is_kanji_head(p) {
                            self.cur_kanji_skip
                        } else {
                            NULL
                        };
                        if g != NULL && g != self.mem.zero_glue() {
                            x += self.mem.width(g);
                            let o = self.mem.stretch_order(g) as usize;
                            self.total_stretch[o] += self.mem.stretch(g);
                            let o = self.mem.shrink_order(g) as usize;
                            self.total_shrink[o] += self.mem.shrink(g);
                        }
                    }
                }
                if p == NULL {
                    break 'reswitch;
                }
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | RULE_NODE | UNSET_NODE => {
                        // §653: incorporate box dimensions.
                        x += self.mem.width(p);
                        let s = if self.mem.node_type(p) >= RULE_NODE {
                            0
                        } else {
                            self.mem.shift_amount(p)
                        };
                        if self.mem.height(p) - s > h {
                            h = self.mem.height(p) - s;
                        }
                        if self.mem.depth(p) + s > d {
                            d = self.mem.depth(p) + s;
                        }
                    }
                    INS_NODE | MARK_NODE | ADJUST_NODE => {
                        if self.adjust_tail != NULL {
                            // §655: transfer node p to the adjustment list.
                            while self.mem.link(q) != p {
                                q = self.mem.link(q);
                            }
                            if self.mem.node_type(p) == ADJUST_NODE {
                                let a = self.mem.adjust_ptr(p);
                                let at = self.adjust_tail;
                                self.mem.set_link(at, a);
                                while self.mem.link(self.adjust_tail) != NULL {
                                    self.adjust_tail = self.mem.link(self.adjust_tail);
                                }
                                p = self.mem.link(p);
                                let lq = self.mem.link(q);
                                self.mem.free_node(lq, SMALL_NODE_SIZE);
                            } else {
                                let at = self.adjust_tail;
                                self.mem.set_link(at, p);
                                self.adjust_tail = p;
                                p = self.mem.link(p);
                            }
                            self.mem.set_link(q, p);
                            p = q;
                        }
                    }
                    WHATSIT_NODE => {
                        // xetex.web: native words and glyphs have box
                        // metrics.
                        if self.mem.is_native_word_node(p) || self.mem.is_glyph_node(p) {
                            x += self.mem.width(p);
                            let ht = self.mem.height(p);
                            let dp = self.mem.depth(p);
                            if ht > h {
                                h = ht;
                            }
                            if dp > d {
                                d = dp;
                            }
                        }
                    }
                    GLUE_NODE => {
                        // §656: incorporate glue into the horizontal totals.
                        let g = self.mem.glue_ptr(p);
                        x += self.mem.width(g);
                        let o = self.mem.stretch_order(g) as usize;
                        self.total_stretch[o] += self.mem.stretch(g);
                        let o = self.mem.shrink_order(g) as usize;
                        self.total_shrink[o] += self.mem.shrink(g);
                        if self.mem.subtype(p) >= A_LEADERS {
                            let g = self.mem.leader_ptr(p);
                            if self.mem.height(g) > h {
                                h = self.mem.height(g);
                            }
                            if self.mem.depth(g) > d {
                                d = self.mem.depth(g);
                            }
                        }
                    }
                    KERN_NODE => {
                        x += self.mem.width(p);
                    }
                    MATH_NODE => {
                        x += self.mem.width(p);
                        if self.texxet_en() {
                            // etex.ch: adjust the LR stack.
                            if crate::nodes::end_lr(self.mem.subtype(p)) {
                                if self.mem.info(self.lr_ptr)
                                    == i32::from(crate::nodes::end_lr_type(self.mem.subtype(p)))
                                {
                                    self.pop_lr();
                                } else {
                                    self.lr_problems += 1;
                                    self.mem.set_node_type(p, KERN_NODE);
                                    self.mem.set_subtype(p, EXPLICIT);
                                }
                            } else {
                                self.push_lr(p)?;
                            }
                        }
                    }
                    LIGATURE_NODE => {
                        // §652: make node p look like a char_node.
                        let lt = self.mem.lig_trick();
                        *self.mem.word_mut(lt) = self.mem.word(p + 1);
                        let l = self.mem.link(p);
                        self.mem.set_link(lt, l);
                        p = lt;
                        continue 'reswitch;
                    }
                    _ => {}
                }
                p = self.mem.link(p);
                break 'reswitch;
            }
        }
        if self.adjust_tail != NULL {
            let at = self.adjust_tail;
            self.mem.set_link(at, NULL);
        }
        self.mem.set_height(r, h);
        self.mem.set_depth(r, d);
        // §657: determine the width and the glue setting.
        let mut w = w;
        if m == ADDITIONAL {
            w += x;
        }
        self.mem.set_width(r, w);
        let x = w - x; // now x is the excess to be made up
        if x == 0 {
            self.mem.set_glue_sign(r, G_NORMAL);
            self.mem.set_glue_order(r, G_NORMAL);
            self.mem.set_glue_set(r, 0.0);
            return Ok(r);
        }
        if x > 0 {
            // §658: determine horizontal glue stretch setting.
            let o = if self.total_stretch[FILLL as usize] != 0 {
                FILLL
            } else if self.total_stretch[FILL as usize] != 0 {
                FILL
            } else if self.total_stretch[FIL as usize] != 0 {
                FIL
            } else {
                G_NORMAL
            };
            self.mem.set_glue_order(r, o);
            self.mem.set_glue_sign(r, STRETCHING);
            if self.total_stretch[o as usize] != 0 {
                let gs = f64::from(x) / f64::from(self.total_stretch[o as usize]);
                self.mem.set_glue_set(r, gs);
            } else {
                self.mem.set_glue_sign(r, G_NORMAL);
                self.mem.set_glue_set(r, 0.0);
            }
            if o == G_NORMAL && self.mem.list_ptr(r) != NULL {
                // §660: report an underfull hbox.
                self.last_badness = crate::arith::badness(x, self.total_stretch[0]);
                if self.last_badness > self.eqtb.int_par(crate::eqtb::HBADNESS_CODE) {
                    self.print_ln();
                    if self.last_badness > 100 {
                        self.print_nl_chars("Underfull");
                    } else {
                        self.print_nl_chars("Loose");
                    }
                    self.print_chars(" \\hbox (badness ");
                    let b = self.last_badness;
                    self.print_int(b);
                    self.hpack_common_ending(r)?;
                    return Ok(r);
                }
            }
            Ok(r)
        } else {
            // §664: determine horizontal glue shrink setting.
            let o = if self.total_shrink[FILLL as usize] != 0 {
                FILLL
            } else if self.total_shrink[FILL as usize] != 0 {
                FILL
            } else if self.total_shrink[FIL as usize] != 0 {
                FIL
            } else {
                G_NORMAL
            };
            self.mem.set_glue_order(r, o);
            self.mem.set_glue_sign(r, SHRINKING);
            if self.total_shrink[o as usize] != 0 {
                let gs = f64::from(-x) / f64::from(self.total_shrink[o as usize]);
                self.mem.set_glue_set(r, gs);
            } else {
                self.mem.set_glue_sign(r, G_NORMAL);
                self.mem.set_glue_set(r, 0.0);
            }
            if self.total_shrink[o as usize] < -x && o == G_NORMAL && self.mem.list_ptr(r) != NULL {
                self.last_badness = 1_000_000;
                self.mem.set_glue_set(r, 1.0); // use the maximum shrinkage
                                               // §666: report an overfull hbox.
                let hfuzz = self.eqtb.dimen_par(crate::eqtb::HFUZZ_CODE);
                let hbadness = self.eqtb.int_par(crate::eqtb::HBADNESS_CODE);
                if -x - self.total_shrink[0] > hfuzz || hbadness < 100 {
                    let overfull_rule = self.eqtb.dimen_par(crate::eqtb::OVERFULL_RULE_CODE);
                    if overfull_rule > 0 && -x - self.total_shrink[0] > hfuzz {
                        while self.mem.link(q) != NULL {
                            q = self.mem.link(q);
                        }
                        let rule = self.new_rule()?;
                        self.mem.set_link(q, rule);
                        self.mem.set_width(rule, overfull_rule);
                    }
                    self.print_ln();
                    self.print_nl_chars("Overfull \\hbox (");
                    let amount = -x - self.total_shrink[0];
                    self.print_scaled(amount);
                    self.print_chars("pt too wide");
                    self.hpack_common_ending(r)?;
                    return Ok(r);
                }
            } else if o == G_NORMAL && self.mem.list_ptr(r) != NULL {
                // §667: report a tight hbox.
                self.last_badness = crate::arith::badness(-x, self.total_shrink[0]);
                if self.last_badness > self.eqtb.int_par(crate::eqtb::HBADNESS_CODE) {
                    self.print_ln();
                    self.print_nl_chars("Tight \\hbox (badness ");
                    let b = self.last_badness;
                    self.print_int(b);
                    self.hpack_common_ending(r)?;
                    return Ok(r);
                }
            }
            Ok(r)
        }
    }

    /// §663: finish issuing a diagnostic message for an over/underfull hbox.
    fn hpack_common_ending(&mut self, r: Pointer) -> TexResult<()> {
        if self.output_active {
            self.print_chars(") has occurred while \\output is active");
        } else {
            if self.pack_begin_line != 0 {
                if self.pack_begin_line > 0 {
                    self.print_chars(") in paragraph at lines ");
                } else {
                    self.print_chars(") in alignment at lines ");
                }
                let l = self.pack_begin_line.abs();
                self.print_int(l);
                self.print_chars("--");
            } else {
                self.print_chars(") detected at line ");
            }
            let l = self.inp.line;
            self.print_int(l);
        }
        self.print_ln();
        self.font_in_short_display = crate::fonts::NULL_FONT;
        let lp = self.mem.list_ptr(r);
        self.short_display(lp);
        self.print_ln();
        self.begin_diagnostic();
        self.in_pack_diagnostic = true;
        self.show_box(r);
        self.in_pack_diagnostic = false;
        self.end_diagnostic(true);
        Ok(())
    }

    /// `vpackage(p, h, m, l)` (§668-§675).
    pub fn vpackage(&mut self, p: Pointer, h: Scaled, m: i32, l: Scaled) -> TexResult<Pointer> {
        self.last_badness = 0;
        let r = self.mem.get_node(BOX_NODE_SIZE)?;
        self.mem.set_node_type(r, VLIST_NODE);
        self.mem.set_subtype(r, 0);
        self.mem.set_shift_amount(r, 0);
        self.mem.set_list_ptr(r, p);
        let mut w: Scaled = 0;
        let mut d: Scaled = 0;
        let mut x: Scaled = 0;
        self.total_stretch = [0; 4];
        self.total_shrink = [0; 4];
        let mut p = p;
        while p != NULL {
            // §669: examine node p.
            if self.mem.is_char_node(p) {
                self.confusion("vpack")?;
            } else {
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | RULE_NODE | UNSET_NODE => {
                        // §670.
                        x += d + self.mem.height(p);
                        d = self.mem.depth(p);
                        let s = if self.mem.node_type(p) >= RULE_NODE {
                            0
                        } else {
                            self.mem.shift_amount(p)
                        };
                        if self.mem.width(p) + s > w {
                            w = self.mem.width(p) + s;
                        }
                    }
                    WHATSIT_NODE => {
                        if self.mem.is_native_word_node(p) || self.mem.is_glyph_node(p) {
                            x += d + self.mem.height(p);
                            d = self.mem.depth(p);
                            let wd = self.mem.width(p);
                            if wd > w {
                                w = wd;
                            }
                        }
                    }
                    GLUE_NODE => {
                        // §671.
                        x += d;
                        d = 0;
                        let g = self.mem.glue_ptr(p);
                        x += self.mem.width(g);
                        let o = self.mem.stretch_order(g) as usize;
                        self.total_stretch[o] += self.mem.stretch(g);
                        let o = self.mem.shrink_order(g) as usize;
                        self.total_shrink[o] += self.mem.shrink(g);
                        if self.mem.subtype(p) >= A_LEADERS {
                            let g = self.mem.leader_ptr(p);
                            if self.mem.width(g) > w {
                                w = self.mem.width(g);
                            }
                        }
                    }
                    KERN_NODE => {
                        x += d + self.mem.width(p);
                        d = 0;
                    }
                    _ => {}
                }
            }
            p = self.mem.link(p);
        }
        self.mem.set_width(r, w);
        if d > l {
            x += d - l;
            self.mem.set_depth(r, l);
        } else {
            self.mem.set_depth(r, d);
        }
        // §672: determine the height and the glue setting.
        let mut h = h;
        if m == ADDITIONAL {
            h += x;
        }
        self.mem.set_height(r, h);
        let x = h - x;
        if x == 0 {
            self.mem.set_glue_sign(r, G_NORMAL);
            self.mem.set_glue_order(r, G_NORMAL);
            self.mem.set_glue_set(r, 0.0);
            return Ok(r);
        }
        if x > 0 {
            // §673.
            let o = if self.total_stretch[FILLL as usize] != 0 {
                FILLL
            } else if self.total_stretch[FILL as usize] != 0 {
                FILL
            } else if self.total_stretch[FIL as usize] != 0 {
                FIL
            } else {
                G_NORMAL
            };
            self.mem.set_glue_order(r, o);
            self.mem.set_glue_sign(r, STRETCHING);
            if self.total_stretch[o as usize] != 0 {
                let gs = f64::from(x) / f64::from(self.total_stretch[o as usize]);
                self.mem.set_glue_set(r, gs);
            } else {
                self.mem.set_glue_sign(r, G_NORMAL);
                self.mem.set_glue_set(r, 0.0);
            }
            if o == G_NORMAL && self.mem.list_ptr(r) != NULL {
                // §674.
                self.last_badness = crate::arith::badness(x, self.total_stretch[0]);
                if self.last_badness > self.eqtb.int_par(crate::eqtb::VBADNESS_CODE) {
                    self.print_ln();
                    if self.last_badness > 100 {
                        self.print_nl_chars("Underfull");
                    } else {
                        self.print_nl_chars("Loose");
                    }
                    self.print_chars(" \\vbox (badness ");
                    let b = self.last_badness;
                    self.print_int(b);
                    self.vpack_common_ending(r)?;
                    return Ok(r);
                }
            }
            Ok(r)
        } else {
            // §676.
            let o = if self.total_shrink[FILLL as usize] != 0 {
                FILLL
            } else if self.total_shrink[FILL as usize] != 0 {
                FILL
            } else if self.total_shrink[FIL as usize] != 0 {
                FIL
            } else {
                G_NORMAL
            };
            self.mem.set_glue_order(r, o);
            self.mem.set_glue_sign(r, SHRINKING);
            if self.total_shrink[o as usize] != 0 {
                let gs = f64::from(-x) / f64::from(self.total_shrink[o as usize]);
                self.mem.set_glue_set(r, gs);
            } else {
                self.mem.set_glue_sign(r, G_NORMAL);
                self.mem.set_glue_set(r, 0.0);
            }
            if self.total_shrink[o as usize] < -x && o == G_NORMAL && self.mem.list_ptr(r) != NULL {
                self.last_badness = 1_000_000;
                self.mem.set_glue_set(r, 1.0);
                // §677.
                let vfuzz = self.eqtb.dimen_par(crate::eqtb::VFUZZ_CODE);
                let vbadness = self.eqtb.int_par(crate::eqtb::VBADNESS_CODE);
                if -x - self.total_shrink[0] > vfuzz || vbadness < 100 {
                    self.print_ln();
                    self.print_nl_chars("Overfull \\vbox (");
                    let amount = -x - self.total_shrink[0];
                    self.print_scaled(amount);
                    self.print_chars("pt too high");
                    self.vpack_common_ending(r)?;
                    return Ok(r);
                }
            } else if o == G_NORMAL && self.mem.list_ptr(r) != NULL {
                // §678.
                self.last_badness = crate::arith::badness(-x, self.total_shrink[0]);
                if self.last_badness > self.eqtb.int_par(crate::eqtb::VBADNESS_CODE) {
                    self.print_ln();
                    self.print_nl_chars("Tight \\vbox (badness ");
                    let b = self.last_badness;
                    self.print_int(b);
                    self.vpack_common_ending(r)?;
                    return Ok(r);
                }
            }
            Ok(r)
        }
    }

    /// §675: the vbox diagnostic tail.
    fn vpack_common_ending(&mut self, r: Pointer) -> TexResult<()> {
        if self.output_active {
            self.print_chars(") has occurred while \\output is active");
        } else {
            if self.pack_begin_line != 0 {
                self.print_chars(") in alignment at lines ");
                let l = self.pack_begin_line.abs();
                self.print_int(l);
                self.print_chars("--");
            } else {
                self.print_chars(") detected at line ");
            }
            let l = self.inp.line;
            self.print_int(l);
            self.print_ln(); // §675: only in the non-\output branch
        }
        self.begin_diagnostic();
        self.in_pack_diagnostic = true;
        self.show_box(r);
        self.in_pack_diagnostic = false;
        self.end_diagnostic(true);
        Ok(())
    }

    /// `vpack(p, h, m)` (§668).
    pub fn vpack(&mut self, p: Pointer, h: Scaled, m: i32) -> TexResult<Pointer> {
        self.vpackage(p, h, m, crate::scan::MAX_DIMEN)
    }

    /// `append_to_vlist(b)` (§679): baselineskip calculation.
    pub fn append_to_vlist(&mut self, b: Pointer) -> TexResult<()> {
        if self.prev_depth() > crate::nest::IGNORE_DEPTH {
            let bs = self.eqtb.glue_par(crate::eqtb::BASELINE_SKIP_CODE);
            let d = self.mem.width(bs) - self.prev_depth() - self.mem.height(b);
            let p = if d < self.eqtb.dimen_par(crate::eqtb::LINE_SKIP_LIMIT_CODE) {
                self.new_param_glue(crate::eqtb::LINE_SKIP_CODE)?
            } else {
                let (p, spec) = self.new_skip_param(crate::eqtb::BASELINE_SKIP_CODE)?;
                self.mem.set_width(spec, d);
                p
            };
            self.tail_append(p);
        }
        self.tail_append(b);
        let d = self.mem.depth(b);
        self.set_prev_depth(d);
        Ok(())
    }
}
