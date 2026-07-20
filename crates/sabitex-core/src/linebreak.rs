//! Breaking paragraphs into lines.
//!
//! Ports tex.web Parts 38-39 (§813-§890): `line_break`, `try_break`,
//! `post_line_break`, the delta-node machinery, and the second-pass
//! hyphenation hook (§894-§899). The `\tracingparagraphs` displays are
//! included since TRIP exercises them.

use crate::engine::Engine;
use crate::eqtb::*;
use crate::error::TexResult;
use crate::fonts::NON_CHAR;
use crate::hyph::norm_lang;
use crate::nodes::*;
use crate::pack::EXACTLY;
use crate::types::{Pointer, Scaled, MAX_HALFWORD, NULL};

// §817: fitness classes.
pub const VERY_LOOSE_FIT: u16 = 0;
pub const LOOSE_FIT: u16 = 1;
pub const DECENT_FIT: u16 = 2;
pub const TIGHT_FIT: u16 = 3;

// §819-§822: break node layout.
pub const ACTIVE_NODE_SIZE: i32 = 3;
/// `active_node_size_extended` (etex.ch): active nodes with
/// `active_short` (word 3) and `active_glue` (word 4) for \lastlinefit.
pub const ACTIVE_NODE_SIZE_EXTENDED: i32 = 5;
pub const PASSIVE_NODE_SIZE: i32 = 2;
pub const DELTA_NODE_SIZE: i32 = 7;
pub const DELTA_NODE: u16 = 2;
pub const UNHYPHENATED: u16 = 0;
pub const HYPHENATED: u16 = 1;

/// `awful_bad` (§833).
pub const AWFUL_BAD: i32 = 0o7777777777;

/// Line-breaking state (§821-§823, §828-§833, §839, §847, §872-§875).
pub struct LineBreak {
    pub just_box: Pointer,
    pub passive: Pointer,
    pub printed_node: Pointer,
    pub pass_number: i32,
    pub active_width: [Scaled; 7],
    pub cur_active_width: [Scaled; 7],
    pub background: [Scaled; 7],
    pub break_width: [Scaled; 7],
    pub no_shrink_error_yet: bool,
    pub cur_p: Pointer,
    pub second_pass: bool,
    pub final_pass: bool,
    pub threshold: i32,
    pub minimal_demerits: [i32; 4],
    pub minimum_demerits: i32,
    pub best_place: [Pointer; 4],
    pub best_pl_line: [i32; 4],
    pub easy_line: i32,
    pub last_special_line: i32,
    pub first_width: Scaled,
    pub second_width: Scaled,
    pub first_indent: Scaled,
    pub second_indent: Scaled,
    pub disc_width: Scaled,
    pub best_bet: Pointer,
    pub fewest_demerits: i32,
    pub best_line: i32,
    pub actual_looseness: i32,
    // etex.ch \lastlinefit machinery.
    /// `last_line_fill`: the \parfillskip glue node of the paragraph.
    pub last_line_fill: Pointer,
    /// `do_last_line_fit`: special algorithm for the last line?
    pub do_last_line_fit: bool,
    /// `active_node_size`: 3 normally, 5 when extended.
    pub active_node_size: i32,
    /// `fill_width[0..2]`: infinite stretch components of \parfillskip.
    pub fill_width: [Scaled; 3],
    /// `best_pl_short` / `best_pl_glue`, indexed by fitness class.
    pub best_pl_short: [Scaled; 4],
    pub best_pl_glue: [Scaled; 4],
}

impl Default for LineBreak {
    fn default() -> Self {
        LineBreak {
            just_box: NULL,
            passive: NULL,
            printed_node: NULL,
            pass_number: 0,
            active_width: [0; 7],
            cur_active_width: [0; 7],
            background: [0; 7],
            break_width: [0; 7],
            no_shrink_error_yet: true,
            cur_p: NULL,
            second_pass: false,
            final_pass: false,
            threshold: 0,
            minimal_demerits: [AWFUL_BAD; 4],
            minimum_demerits: AWFUL_BAD,
            best_place: [NULL; 4],
            best_pl_line: [0; 4],
            easy_line: 0,
            last_special_line: 0,
            first_width: 0,
            second_width: 0,
            first_indent: 0,
            second_indent: 0,
            disc_width: 0,
            best_bet: NULL,
            fewest_demerits: 0,
            best_line: 0,
            actual_looseness: 0,
            last_line_fill: NULL,
            do_last_line_fit: false,
            active_node_size: ACTIVE_NODE_SIZE,
            fill_width: [0; 3],
            best_pl_short: [0; 4],
            best_pl_glue: [0; 4],
        }
    }
}

impl Engine {
    // Active/passive field accessors (§819, §821).
    fn total_demerits(&self, p: Pointer) -> i32 {
        self.mem.word(p + 2).int()
    }

    fn set_total_demerits(&mut self, p: Pointer, v: i32) {
        self.mem.word_mut(p + 2).set_int(v);
    }

    /// `finite_shrink(p)` (§826): recovers from infinite shrinkage.
    fn finite_shrink(&mut self, p: Pointer) -> TexResult<Pointer> {
        if self.lb.no_shrink_error_yet {
            self.lb.no_shrink_error_yet = false;
            if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                self.end_diagnostic(true);
            }
            self.print_err("Infinite glue shrinkage found in a paragraph");
            self.help(&[
                "The paragraph just ended includes some glue that has",
                "infinite shrinkability, e.g., `\\hskip 0pt minus 1fil'.",
                "Such glue doesn't belong there---it allows a paragraph",
                "of any length to fit on one line. But it's safe to proceed,",
                "since the offensive shrinkability has been made finite.",
            ]);
            self.error()?;
            if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                self.begin_diagnostic();
            }
        }
        let q = self.new_spec(p)?;
        self.mem.set_shrink_order(q, crate::mem::NORMAL);
        self.mem.delete_glue_ref(p);
        Ok(q)
    }

    fn check_shrinkage_glue_par(&mut self, code: i32) -> TexResult<()> {
        let g = self.eqtb.glue_par(code);
        if self.mem.shrink_order(g) != crate::mem::NORMAL && self.mem.shrink(g) != 0 {
            let q = self.finite_shrink(g)?;
            self.eqtb.set_equiv(self.eqtb.lay.glue_base + code, q);
        }
        Ok(())
    }

    /// `try_break(pi, break_type)` (§829-§861).
    pub fn try_break(&mut self, pi: i32, break_type: u16) -> TexResult<()> {
        let mut pi = pi;
        // §831: make sure pi is in the proper range.
        if pi.abs() >= INF_PENALTY {
            if pi > 0 {
                return Ok(());
            }
            pi = EJECT_PENALTY;
        }
        let last_active = self.mem.active();
        let mut no_break_yet = true;
        let mut prev_r = last_active;
        let mut prev_prev_r: Pointer = NULL;
        let mut old_l: i32 = 0;
        let mut line_width: Scaled = 0;
        for k in 1..=6 {
            self.lb.cur_active_width[k] = self.lb.active_width[k];
        }
        'body: loop {
            // continue:
            let mut r = self.mem.link(prev_r);
            // §832: delta nodes update cur_active_width.
            if self.mem.node_type(r) == DELTA_NODE && r != last_active {
                for k in 1..=6i32 {
                    self.lb.cur_active_width[k as usize] += self.mem.word(r + k).sc();
                }
                prev_prev_r = prev_r;
                prev_r = r;
                continue 'body;
            }
            // §835: if a line number class has ended, create new actives.
            let l = self.mem.llink(r); // line_number
            if l > old_l {
                if self.lb.minimum_demerits < AWFUL_BAD
                    && (old_l != self.lb.easy_line || r == last_active)
                {
                    // §836: create new active nodes for the best breaks.
                    if no_break_yet {
                        self.compute_break_width(break_type)?;
                        no_break_yet = false;
                    }
                    // §843: insert a delta node before the breaks.
                    if self.mem.node_type(prev_r) == DELTA_NODE && prev_r != last_active {
                        for k in 1..=6i32 {
                            let v = self.mem.word(prev_r + k).sc()
                                - self.lb.cur_active_width[k as usize]
                                + self.lb.break_width[k as usize];
                            self.mem.word_mut(prev_r + k).set_sc(v);
                        }
                    } else if prev_r == last_active {
                        for k in 1..=6 {
                            self.lb.active_width[k] = self.lb.break_width[k];
                        }
                    } else {
                        let q = self.mem.get_node(DELTA_NODE_SIZE)?;
                        self.mem.set_link(q, r);
                        self.mem.set_node_type(q, DELTA_NODE);
                        self.mem.set_subtype(q, 0);
                        for k in 1..=6i32 {
                            let v = self.lb.break_width[k as usize]
                                - self.lb.cur_active_width[k as usize];
                            self.mem.word_mut(q + k).set_sc(v);
                        }
                        self.mem.set_link(prev_r, q);
                        prev_prev_r = prev_r;
                        prev_r = q;
                    }
                    if self.eqtb.int_par(ADJ_DEMERITS_CODE).abs()
                        >= AWFUL_BAD - self.lb.minimum_demerits
                    {
                        self.lb.minimum_demerits = AWFUL_BAD - 1;
                    } else {
                        self.lb.minimum_demerits += self.eqtb.int_par(ADJ_DEMERITS_CODE).abs();
                    }
                    for fit_class in VERY_LOOSE_FIT..=TIGHT_FIT {
                        if self.lb.minimal_demerits[fit_class as usize] <= self.lb.minimum_demerits
                        {
                            // §845: insert a new active node.
                            let q = self.mem.get_node(PASSIVE_NODE_SIZE)?;
                            let pv = self.lb.passive;
                            self.mem.set_link(q, pv);
                            self.lb.passive = q;
                            let cp = self.lb.cur_p;
                            self.mem.set_rlink(q, cp); // cur_break
                            self.lb.pass_number += 1;
                            let pn = self.lb.pass_number;
                            self.mem.set_info(q, pn); // serial
                            let bp = self.lb.best_place[fit_class as usize];
                            self.mem.set_llink(q, bp); // prev_break
                            let ans = self.lb.active_node_size;
                            let q2 = self.mem.get_node(ans)?;
                            let pv = self.lb.passive;
                            self.mem.set_rlink(q2, pv); // break_node
                            let bl = self.lb.best_pl_line[fit_class as usize] + 1;
                            self.mem.set_llink(q2, bl); // line_number
                            self.mem.set_subtype(q2, fit_class); // fitness
                            self.mem.set_node_type(q2, break_type);
                            let md = self.lb.minimal_demerits[fit_class as usize];
                            self.set_total_demerits(q2, md);
                            if self.lb.do_last_line_fit {
                                // etex.ch §845: store the additional data.
                                let s = self.lb.best_pl_short[fit_class as usize];
                                self.mem.word_mut(q2 + 3).set_sc(s); // active_short
                                let gl = self.lb.best_pl_glue[fit_class as usize];
                                self.mem.word_mut(q2 + 4).set_sc(gl); // active_glue
                            }
                            self.mem.set_link(q2, r);
                            self.mem.set_link(prev_r, q2);
                            prev_r = q2;
                            if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                                // §846.
                                self.print_nl_chars("@@");
                                let s = self.mem.info(self.lb.passive);
                                self.print_int(s);
                                self.print_chars(": line ");
                                let ln = self.mem.llink(q2) - 1;
                                self.print_int(ln);
                                self.print_char('.' as i32);
                                self.print_int(i32::from(fit_class));
                                if break_type == HYPHENATED {
                                    self.print_char('-' as i32);
                                }
                                self.print_chars(" t=");
                                let td = self.total_demerits(q2);
                                self.print_int(td);
                                if self.lb.do_last_line_fit {
                                    // etex.ch §846.
                                    self.print_chars(" s=");
                                    let s = self.mem.word(q2 + 3).sc();
                                    self.print_scaled(s);
                                    if self.lb.cur_p == NULL {
                                        self.print_chars(" a=");
                                    } else {
                                        self.print_chars(" g=");
                                    }
                                    let gl = self.mem.word(q2 + 4).sc();
                                    self.print_scaled(gl);
                                }
                                self.print_chars(" -> @@");
                                let pb = self.mem.llink(self.lb.passive);
                                if pb == NULL {
                                    self.print_char('0' as i32);
                                } else {
                                    let s = self.mem.info(pb);
                                    self.print_int(s);
                                }
                            }
                        }
                        self.lb.minimal_demerits[fit_class as usize] = AWFUL_BAD;
                    }
                    self.lb.minimum_demerits = AWFUL_BAD;
                    // §844: insert a delta node for the next active node.
                    if r != last_active {
                        let q = self.mem.get_node(DELTA_NODE_SIZE)?;
                        self.mem.set_link(q, r);
                        self.mem.set_node_type(q, DELTA_NODE);
                        self.mem.set_subtype(q, 0);
                        for k in 1..=6i32 {
                            let v = self.lb.cur_active_width[k as usize]
                                - self.lb.break_width[k as usize];
                            self.mem.word_mut(q + k).set_sc(v);
                        }
                        self.mem.set_link(prev_r, q);
                        prev_prev_r = prev_r;
                        prev_r = q;
                    }
                }
                if r == last_active {
                    break 'body; // return
                }
                // §850: compute the new line width.
                if l > self.lb.easy_line {
                    line_width = self.lb.second_width;
                    old_l = MAX_HALFWORD - 1;
                } else {
                    old_l = l;
                    line_width = if l > self.lb.last_special_line {
                        self.lb.second_width
                    } else if self.eqtb.equiv(self.eqtb.lay.par_shape_loc) == NULL {
                        self.lb.first_width
                    } else {
                        let ps = self.eqtb.equiv(self.eqtb.lay.par_shape_loc);
                        self.mem.word(ps + 2 * l).sc()
                    };
                }
            }
            // §851: consider the demerits for a line from r to cur_p.
            let mut artificial_demerits = false;
            let mut shortfall = line_width - self.lb.cur_active_width[1];
            // etex.ch: glue stretch/shrink (or adjustment) of this line.
            let mut g: Scaled = 0;
            #[allow(unused_assignments)]
            let mut b: i32 = 0;
            #[allow(unused_assignments)]
            let mut fit_class: u16 = DECENT_FIT;
            'found: {
                if shortfall > 0 {
                    // §852.
                    if self.lb.cur_active_width[3] != 0
                        || self.lb.cur_active_width[4] != 0
                        || self.lb.cur_active_width[5] != 0
                    {
                        // etex.ch: perform computations for the last line.
                        if self.lb.do_last_line_fit {
                            'not_found: {
                                if self.lb.cur_p != NULL {
                                    break 'not_found;
                                }
                                if self.mem.word(r + 3).sc() == 0 || self.mem.word(r + 4).sc() <= 0
                                {
                                    break 'not_found;
                                }
                                if self.lb.cur_active_width[3] != self.lb.fill_width[0]
                                    || self.lb.cur_active_width[4] != self.lb.fill_width[1]
                                    || self.lb.cur_active_width[5] != self.lb.fill_width[2]
                                {
                                    break 'not_found;
                                }
                                let short = self.mem.word(r + 3).sc();
                                g = if short > 0 {
                                    self.lb.cur_active_width[2]
                                } else {
                                    self.lb.cur_active_width[6]
                                };
                                if g <= 0 {
                                    break 'not_found;
                                }
                                self.arith.arith_error = false;
                                let glue = self.mem.word(r + 4).sc();
                                g = self.expr_fract(g, short, glue, crate::scan::MAX_DIMEN);
                                let llf = self.eqtb.int_par(crate::eqtb::LAST_LINE_FIT_CODE);
                                if llf < 1000 {
                                    g = self.expr_fract(g, llf, 1000, crate::scan::MAX_DIMEN);
                                }
                                if self.arith.arith_error {
                                    g = if short > 0 {
                                        crate::scan::MAX_DIMEN
                                    } else {
                                        -crate::scan::MAX_DIMEN
                                    };
                                }
                                if g > 0 {
                                    // badness of the last line for stretching
                                    if g > shortfall {
                                        g = shortfall;
                                    }
                                    if g > 7_230_584 && self.lb.cur_active_width[2] < 1_663_497 {
                                        b = crate::types::INF_BAD;
                                        fit_class = VERY_LOOSE_FIT;
                                        break 'found;
                                    }
                                    b = crate::arith::badness(g, self.lb.cur_active_width[2]);
                                    fit_class = if b > 12 {
                                        if b > 99 {
                                            VERY_LOOSE_FIT
                                        } else {
                                            LOOSE_FIT
                                        }
                                    } else {
                                        DECENT_FIT
                                    };
                                    break 'found;
                                } else if g < 0 {
                                    // badness of the last line for shrinking
                                    if -g > self.lb.cur_active_width[6] {
                                        g = -self.lb.cur_active_width[6];
                                    }
                                    b = crate::arith::badness(-g, self.lb.cur_active_width[6]);
                                    fit_class = if b > 12 { TIGHT_FIT } else { DECENT_FIT };
                                    break 'found;
                                }
                            }
                            shortfall = 0;
                        }
                        b = 0;
                        fit_class = DECENT_FIT; // infinite stretch
                    } else if shortfall > 7_230_584 && self.lb.cur_active_width[2] < 1_663_497 {
                        b = crate::types::INF_BAD;
                        fit_class = VERY_LOOSE_FIT;
                    } else {
                        b = crate::arith::badness(shortfall, self.lb.cur_active_width[2]);
                        fit_class = if b > 12 {
                            if b > 99 {
                                VERY_LOOSE_FIT
                            } else {
                                LOOSE_FIT
                            }
                        } else {
                            DECENT_FIT
                        };
                    }
                } else {
                    // §853.
                    if -shortfall > self.lb.cur_active_width[6] {
                        b = crate::types::INF_BAD + 1;
                    } else {
                        b = crate::arith::badness(-shortfall, self.lb.cur_active_width[6]);
                    }
                    fit_class = if b > 12 { TIGHT_FIT } else { DECENT_FIT };
                }
                // etex.ch §851: adjust the additional data for the last
                // line (skipped when the last-line computation above
                // jumped to "found").
                if self.lb.do_last_line_fit {
                    if self.lb.cur_p == NULL {
                        shortfall = 0;
                    }
                    g = if shortfall > 0 {
                        self.lb.cur_active_width[2]
                    } else if shortfall < 0 {
                        self.lb.cur_active_width[6]
                    } else {
                        0
                    };
                }
            }
            let node_r_stays_active: bool;
            let mut goto_deactivate = false;
            if b > crate::types::INF_BAD || pi == EJECT_PENALTY {
                // §854.
                if self.lb.final_pass
                    && self.lb.minimum_demerits == AWFUL_BAD
                    && self.mem.link(r) == last_active
                    && prev_r == last_active
                {
                    artificial_demerits = true; // forced break
                } else if b > self.lb.threshold {
                    goto_deactivate = true;
                }
                node_r_stays_active = false;
            } else {
                prev_r = r;
                if b > self.lb.threshold {
                    continue 'body;
                }
                node_r_stays_active = true;
            }
            if !goto_deactivate {
                // §855: record a new feasible break.
                let mut d: i32;
                if artificial_demerits {
                    d = 0;
                } else {
                    // §859.
                    d = self.eqtb.int_par(LINE_PENALTY_CODE) + b;
                    if d.abs() >= 10000 {
                        d = 100_000_000;
                    } else {
                        d = d * d;
                    }
                    if pi != 0 {
                        if pi > 0 {
                            d += pi * pi;
                        } else if pi > EJECT_PENALTY {
                            d -= pi * pi;
                        }
                    }
                    if break_type == HYPHENATED && self.mem.node_type(r) == HYPHENATED {
                        if self.lb.cur_p != NULL {
                            d += self.eqtb.int_par(DOUBLE_HYPHEN_DEMERITS_CODE);
                        } else {
                            d += self.eqtb.int_par(FINAL_HYPHEN_DEMERITS_CODE);
                        }
                    }
                    if (i32::from(fit_class) - i32::from(self.mem.subtype(r))).abs() > 1 {
                        d += self.eqtb.int_par(ADJ_DEMERITS_CODE);
                    }
                }
                if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                    // §856.
                    if self.lb.printed_node != self.lb.cur_p {
                        // §857: print the list between printed_node and cur_p.
                        self.print_nl_chars("");
                        if self.lb.cur_p == NULL {
                            let pn = self.mem.link(self.lb.printed_node);
                            self.short_display(pn);
                        } else {
                            let save_link = self.mem.link(self.lb.cur_p);
                            let cp = self.lb.cur_p;
                            self.mem.set_link(cp, NULL);
                            self.print_nl_chars("");
                            let pn = self.mem.link(self.lb.printed_node);
                            self.short_display(pn);
                            self.mem.set_link(cp, save_link);
                        }
                        self.lb.printed_node = self.lb.cur_p;
                    }
                    self.print_nl_chars("@");
                    if self.lb.cur_p == NULL {
                        self.print_esc_str("par");
                    } else if self.mem.node_type(self.lb.cur_p) != GLUE_NODE {
                        let t = self.mem.node_type(self.lb.cur_p);
                        if t == PENALTY_NODE {
                            self.print_esc_str("penalty");
                        } else if t == DISC_NODE {
                            self.print_esc_str("discretionary");
                        } else if t == KERN_NODE {
                            self.print_esc_str("kern");
                        } else {
                            self.print_esc_str("math");
                        }
                    }
                    self.print_chars(" via @@");
                    let bn = self.mem.rlink(r); // break_node
                    if bn == NULL {
                        self.print_char('0' as i32);
                    } else {
                        let s = self.mem.info(bn);
                        self.print_int(s);
                    }
                    self.print_chars(" b=");
                    if b > crate::types::INF_BAD {
                        self.print_char('*' as i32);
                    } else {
                        self.print_int(b);
                    }
                    self.print_chars(" p=");
                    self.print_int(pi);
                    self.print_chars(" d=");
                    if artificial_demerits {
                        self.print_char('*' as i32);
                    } else {
                        self.print_int(d);
                    }
                }
                d += self.total_demerits(r);
                if d <= self.lb.minimal_demerits[fit_class as usize] {
                    self.lb.minimal_demerits[fit_class as usize] = d;
                    let bn = self.mem.rlink(r);
                    self.lb.best_place[fit_class as usize] = bn;
                    self.lb.best_pl_line[fit_class as usize] = l;
                    if self.lb.do_last_line_fit {
                        // etex.ch §855: record shortfall and glue.
                        self.lb.best_pl_short[fit_class as usize] = shortfall;
                        self.lb.best_pl_glue[fit_class as usize] = g;
                    }
                    if d < self.lb.minimum_demerits {
                        self.lb.minimum_demerits = d;
                    }
                }
                if node_r_stays_active {
                    continue 'body;
                }
            }
            // §860: deactivate node r.
            let lr = self.mem.link(r);
            self.mem.set_link(prev_r, lr);
            let ans = self.lb.active_node_size;
            self.mem.free_node(r, ans);
            if prev_r == last_active {
                // §861.
                r = self.mem.link(last_active);
                if self.mem.node_type(r) == DELTA_NODE && r != last_active {
                    for k in 1..=6i32 {
                        self.lb.active_width[k as usize] += self.mem.word(r + k).sc();
                        self.lb.cur_active_width[k as usize] = self.lb.active_width[k as usize];
                    }
                    let lr = self.mem.link(r);
                    self.mem.set_link(last_active, lr);
                    self.mem.free_node(r, DELTA_NODE_SIZE);
                }
            } else if self.mem.node_type(prev_r) == DELTA_NODE {
                r = self.mem.link(prev_r);
                if r == last_active {
                    for k in 1..=6i32 {
                        self.lb.cur_active_width[k as usize] -= self.mem.word(prev_r + k).sc();
                    }
                    self.mem.set_link(prev_prev_r, last_active);
                    self.mem.free_node(prev_r, DELTA_NODE_SIZE);
                    prev_r = prev_prev_r;
                } else if self.mem.node_type(r) == DELTA_NODE {
                    for k in 1..=6i32 {
                        self.lb.cur_active_width[k as usize] += self.mem.word(r + k).sc();
                        let v = self.mem.word(prev_r + k).sc() + self.mem.word(r + k).sc();
                        self.mem.word_mut(prev_r + k).set_sc(v);
                    }
                    let lr = self.mem.link(r);
                    self.mem.set_link(prev_r, lr);
                    self.mem.free_node(r, DELTA_NODE_SIZE);
                }
            }
        }
        // §858: update printed_node (tracing only).
        if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0
            && self.lb.cur_p == self.lb.printed_node
            && self.lb.cur_p != NULL
            && self.mem.node_type(self.lb.cur_p) == DISC_NODE
        {
            let mut t = self.mem.replace_count(self.lb.cur_p);
            while t > 0 {
                t -= 1;
                let pn = self.mem.link(self.lb.printed_node);
                self.lb.printed_node = pn;
            }
        }
        Ok(())
    }

    /// §837-§842: compute the `break_width` values at `cur_p`.
    fn compute_break_width(&mut self, break_type: u16) -> TexResult<()> {
        for k in 1..=6 {
            self.lb.break_width[k] = self.lb.background[k];
        }
        let mut s = self.lb.cur_p;
        if break_type > UNHYPHENATED && self.lb.cur_p != NULL {
            // §840: discretionary break widths.
            let cur_p = self.lb.cur_p;
            let mut t = self.mem.replace_count(cur_p);
            let mut v = cur_p;
            s = self.mem.post_break(cur_p);
            while t > 0 {
                t -= 1;
                v = self.mem.link(v);
                // §841: subtract the width of node v.
                self.lb.break_width[1] -= self.node_width_disc(v)?;
            }
            while s != NULL {
                // §842: add the width of node s.
                self.lb.break_width[1] += self.node_width_disc(s)?;
                s = self.mem.link(s);
            }
            self.lb.break_width[1] += self.lb.disc_width;
            if self.mem.post_break(cur_p) == NULL {
                s = self.mem.link(v); // nodes may be discardable after the break
            }
        }
        while s != NULL {
            if self.mem.is_char_node(s) {
                break;
            }
            match self.mem.node_type(s) {
                GLUE_NODE => {
                    // §838.
                    let v = self.mem.glue_ptr(s);
                    self.lb.break_width[1] -= self.mem.width(v);
                    self.lb.break_width[2 + self.mem.stretch_order(v) as usize] -=
                        self.mem.stretch(v);
                    self.lb.break_width[6] -= self.mem.shrink(v);
                }
                PENALTY_NODE => {}
                MATH_NODE => {
                    self.lb.break_width[1] -= self.mem.width(s);
                }
                KERN_NODE => {
                    if self.mem.subtype(s) != EXPLICIT {
                        break;
                    }
                    self.lb.break_width[1] -= self.mem.width(s);
                }
                _ => break,
            }
            s = self.mem.link(s);
        }
        Ok(())
    }

    /// §841/§842/§866/§869: the width of a node in discretionary texts.
    fn node_width_disc(&mut self, v: Pointer) -> TexResult<Scaled> {
        if self.mem.is_char_node(v) {
            let f = i32::from(self.mem.font(v));
            let i = self.fonts.char_info(f, i32::from(self.mem.character(v)));
            Ok(self.fonts.char_width(f, i))
        } else {
            match self.mem.node_type(v) {
                LIGATURE_NODE => {
                    let f = i32::from(self.mem.font(v + 1));
                    let i = self
                        .fonts
                        .char_info(f, i32::from(self.mem.character(v + 1)));
                    Ok(self.fonts.char_width(f, i))
                }
                HLIST_NODE | VLIST_NODE | RULE_NODE | KERN_NODE => Ok(self.mem.width(v)),
                _ => {
                    self.confusion("disc")?;
                    Ok(0)
                }
            }
        }
    }

    /// `line_break(d)` (§815-§890 + etex.ch): `d` is true when breaking a
    /// partial paragraph preceding display math mode.
    pub fn line_break(&mut self, d: bool) -> TexResult<()> {
        self.latch_kanji_skips(); // pTeX: lines measure with current skips
        self.adjust_hlist(self.nest.cur.head, true)?; // pTeX [39.863]
        self.pack_begin_line = self.nest.cur.ml;
        // §816: get ready to start.
        let th = self.mem.temp_head();
        let head = self.nest.cur.head;
        let lk = self.mem.link(head);
        self.mem.set_link(th, lk);
        let tail = self.nest.cur.tail;
        if self.mem.is_char_node(tail) || self.mem.node_type(tail) != GLUE_NODE {
            let p = self.new_penalty(INF_PENALTY)?;
            self.tail_append(p);
        } else {
            self.mem.set_node_type(tail, PENALTY_NODE);
            let g = self.mem.glue_ptr(tail);
            self.mem.delete_glue_ref(g);
            let l = self.mem.leader_ptr(tail);
            self.flush_node_list(l);
            self.mem.set_penalty(tail, INF_PENALTY);
        }
        let pg = self.new_param_glue(PAR_FILL_SKIP_CODE)?;
        let tail = self.nest.cur.tail;
        self.mem.set_link(tail, pg);
        self.lb.last_line_fill = pg; // etex.ch §816
        self.hy.init_cur_lang = self.nest.cur.pg % 0o200000;
        self.hy.init_l_hyf = self.nest.cur.pg / 0o20000000;
        self.hy.init_r_hyf = (self.nest.cur.pg / 0o200000) % 0o100;
        self.pop_nest();
        // §827: compute the background.
        self.lb.no_shrink_error_yet = true;
        self.check_shrinkage_glue_par(LEFT_SKIP_CODE)?;
        self.check_shrinkage_glue_par(RIGHT_SKIP_CODE)?;
        let q = self.eqtb.glue_par(LEFT_SKIP_CODE);
        let r = self.eqtb.glue_par(RIGHT_SKIP_CODE);
        self.lb.background[1] = self.mem.width(q) + self.mem.width(r);
        for k in 2..=5 {
            self.lb.background[k] = 0;
        }
        self.lb.background[2 + self.mem.stretch_order(q) as usize] = self.mem.stretch(q);
        self.lb.background[2 + self.mem.stretch_order(r) as usize] += self.mem.stretch(r);
        self.lb.background[6] = self.mem.shrink(q) + self.mem.shrink(r);
        // etex.ch §827: check for special treatment of the last line.
        self.lb.do_last_line_fit = false;
        self.lb.active_node_size = ACTIVE_NODE_SIZE;
        if self.eqtb.int_par(crate::eqtb::LAST_LINE_FIT_CODE) > 0 {
            let q = self.mem.glue_ptr(self.lb.last_line_fill);
            if self.mem.stretch(q) > 0
                && self.mem.stretch_order(q) > crate::mem::NORMAL
                && self.lb.background[3] == 0
                && self.lb.background[4] == 0
                && self.lb.background[5] == 0
            {
                self.lb.do_last_line_fit = true;
                self.lb.active_node_size = ACTIVE_NODE_SIZE_EXTENDED;
                self.lb.fill_width = [0; 3];
                self.lb.fill_width[self.mem.stretch_order(q) as usize - 1] = self.mem.stretch(q);
            }
        }
        // §834.
        self.lb.minimum_demerits = AWFUL_BAD;
        for fc in 0..4 {
            self.lb.minimal_demerits[fc] = AWFUL_BAD;
        }
        // §848-§849: line-length parameters.
        let par_shape = self.eqtb.equiv(self.eqtb.lay.par_shape_loc);
        let hsize = self.eqtb.dimen_par(HSIZE_CODE);
        let hang_indent = self.eqtb.dimen_par(HANG_INDENT_CODE);
        let hang_after = self.eqtb.int_par(HANG_AFTER_CODE);
        if par_shape == NULL {
            if hang_indent == 0 {
                self.lb.last_special_line = 0;
                self.lb.second_width = hsize;
                self.lb.second_indent = 0;
            } else {
                // §849.
                self.lb.last_special_line = hang_after.abs();
                if hang_after < 0 {
                    self.lb.first_width = hsize - hang_indent.abs();
                    self.lb.first_indent = if hang_indent >= 0 { hang_indent } else { 0 };
                    self.lb.second_width = hsize;
                    self.lb.second_indent = 0;
                } else {
                    self.lb.first_width = hsize;
                    self.lb.first_indent = 0;
                    self.lb.second_width = hsize - hang_indent.abs();
                    self.lb.second_indent = if hang_indent >= 0 { hang_indent } else { 0 };
                }
            }
        } else {
            self.lb.last_special_line = self.mem.info(par_shape) - 1;
            self.lb.second_width = self
                .mem
                .word(par_shape + 2 * (self.lb.last_special_line + 1))
                .sc();
            self.lb.second_indent = self
                .mem
                .word(par_shape + 2 * self.lb.last_special_line + 1)
                .sc();
        }
        let looseness = self.eqtb.int_par(LOOSENESS_CODE);
        self.lb.easy_line = if looseness == 0 {
            self.lb.last_special_line
        } else {
            MAX_HALFWORD
        };
        // §863: find optimal breakpoints.
        self.lb.threshold = self.eqtb.int_par(PRETOLERANCE_CODE);
        if self.lb.threshold >= 0 {
            if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                self.begin_diagnostic();
                self.print_nl_chars("@firstpass");
            }
            self.lb.second_pass = false;
            self.lb.final_pass = false;
        } else {
            self.lb.threshold = self.eqtb.int_par(TOLERANCE_CODE);
            self.lb.second_pass = true;
            self.lb.final_pass = self.eqtb.dimen_par(EMERGENCY_STRETCH_CODE) <= 0;
            if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                self.begin_diagnostic();
            }
        }
        let last_active = self.mem.active();
        loop {
            if self.lb.threshold > crate::types::INF_BAD {
                self.lb.threshold = crate::types::INF_BAD;
            }
            if self.lb.second_pass {
                // §891: initialize for hyphenation.
                if self.hy.trie_not_ready {
                    self.init_trie()?;
                }
                self.hy.cur_lang = self.hy.init_cur_lang;
                self.hy.l_hyf = self.hy.init_l_hyf;
                self.hy.r_hyf = self.hy.init_r_hyf;
                self.set_hyph_index(); // etex.ch §891
            }
            // §864: create the beginning-of-paragraph active node.
            let ans = self.lb.active_node_size;
            let q = self.mem.get_node(ans)?;
            self.mem.set_node_type(q, UNHYPHENATED);
            self.mem.set_subtype(q, DECENT_FIT);
            self.mem.set_link(q, last_active);
            self.mem.set_rlink(q, NULL); // break_node
            let pg1 = self.nest.cur.pg + 1;
            self.mem.set_llink(q, pg1); // line_number
            self.set_total_demerits(q, 0);
            self.mem.set_link(last_active, q);
            if self.lb.do_last_line_fit {
                // etex.ch §864: initialize the additional fields.
                self.mem.word_mut(q + 3).set_sc(0); // active_short
                self.mem.word_mut(q + 4).set_sc(0); // active_glue
            }
            for k in 1..=6 {
                self.lb.active_width[k] = self.lb.background[k];
            }
            self.lb.passive = NULL;
            self.lb.printed_node = self.mem.temp_head();
            self.lb.pass_number = 0;
            self.font_in_short_display = crate::fonts::NULL_FONT;
            self.lb.cur_p = self.mem.link(self.mem.temp_head());
            let mut auto_breaking = true;
            let mut prev_p = self.lb.cur_p;
            while self.lb.cur_p != NULL && self.mem.link(last_active) != last_active {
                // §866: the main legal-breakpoint switch.
                self.line_break_step(&mut prev_p, &mut auto_breaking)?;
            }
            if self.lb.cur_p == NULL {
                // §873: try the final line break.
                self.try_break(EJECT_PENALTY, HYPHENATED)?;
                if self.mem.link(last_active) != last_active {
                    // §874: find an active node with fewest demerits.
                    let mut r = self.mem.link(last_active);
                    self.lb.fewest_demerits = AWFUL_BAD;
                    loop {
                        if self.mem.node_type(r) != DELTA_NODE
                            && self.total_demerits(r) < self.lb.fewest_demerits
                        {
                            self.lb.fewest_demerits = self.total_demerits(r);
                            self.lb.best_bet = r;
                        }
                        r = self.mem.link(r);
                        if r == last_active {
                            break;
                        }
                    }
                    self.lb.best_line = self.mem.llink(self.lb.best_bet);
                    let looseness = self.eqtb.int_par(LOOSENESS_CODE);
                    if looseness == 0 {
                        break; // done
                    }
                    // §875: match the desired looseness.
                    let mut r = self.mem.link(last_active);
                    self.lb.actual_looseness = 0;
                    loop {
                        if self.mem.node_type(r) != DELTA_NODE {
                            let line_diff = self.mem.llink(r) - self.lb.best_line;
                            if (line_diff < self.lb.actual_looseness && looseness <= line_diff)
                                || (line_diff > self.lb.actual_looseness && looseness >= line_diff)
                            {
                                self.lb.best_bet = r;
                                self.lb.actual_looseness = line_diff;
                                self.lb.fewest_demerits = self.total_demerits(r);
                            } else if line_diff == self.lb.actual_looseness
                                && self.total_demerits(r) < self.lb.fewest_demerits
                            {
                                self.lb.best_bet = r;
                                self.lb.fewest_demerits = self.total_demerits(r);
                            }
                        }
                        r = self.mem.link(r);
                        if r == last_active {
                            break;
                        }
                    }
                    self.lb.best_line = self.mem.llink(self.lb.best_bet);
                    if self.lb.actual_looseness == looseness || self.lb.final_pass {
                        break; // done
                    }
                }
            }
            // §865: clean up the memory.
            self.clean_up_break_nodes();
            if !self.lb.second_pass {
                if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                    self.print_nl_chars("@secondpass");
                }
                self.lb.threshold = self.eqtb.int_par(TOLERANCE_CODE);
                self.lb.second_pass = true;
                self.lb.final_pass = self.eqtb.dimen_par(EMERGENCY_STRETCH_CODE) <= 0;
            } else {
                if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
                    self.print_nl_chars("@emergencypass");
                }
                self.lb.background[2] += self.eqtb.dimen_par(EMERGENCY_STRETCH_CODE);
                self.lb.final_pass = true;
            }
        }
        // done:
        if self.eqtb.int_par(TRACING_PARAGRAPHS_CODE) > 0 {
            self.end_diagnostic(true);
        }
        if self.lb.do_last_line_fit {
            // etex.ch §863: adjust the final line of the paragraph.
            let bb = self.lb.best_bet;
            if self.mem.word(bb + 3).sc() == 0 {
                self.lb.do_last_line_fit = false;
            } else {
                let old = self.mem.glue_ptr(self.lb.last_line_fill);
                let q = self.new_spec(old)?;
                self.mem.delete_glue_ref(old);
                let w = self.mem.width(q) + self.mem.word(bb + 3).sc() - self.mem.word(bb + 4).sc();
                self.mem.set_width(q, w);
                self.mem.set_stretch(q, 0);
                let llf = self.lb.last_line_fill;
                self.mem.set_glue_ptr(llf, q);
            }
        }
        // §876-§890.
        self.post_line_break(d)?;
        self.clean_up_break_nodes();
        self.pack_begin_line = 0;
        Ok(())
    }

    /// §865: remove the break nodes.
    fn clean_up_break_nodes(&mut self) {
        let last_active = self.mem.active();
        let mut q = self.mem.link(last_active);
        while q != last_active {
            let next = self.mem.link(q);
            if self.mem.node_type(q) == DELTA_NODE {
                self.mem.free_node(q, DELTA_NODE_SIZE);
            } else {
                let ans = self.lb.active_node_size;
                self.mem.free_node(q, ans);
            }
            q = next;
        }
        let mut q = self.lb.passive;
        while q != NULL {
            let next = self.mem.link(q);
            self.mem.free_node(q, PASSIVE_NODE_SIZE);
            q = next;
        }
        self.lb.passive = NULL;
        self.mem.set_link(last_active, last_active);
    }

    /// §866-§869: process node `cur_p` looking for legal breakpoints.
    fn line_break_step(&mut self, prev_p: &mut Pointer, auto_breaking: &mut bool) -> TexResult<()> {
        let mut cur_p = self.lb.cur_p;
        if self.mem.is_char_node(cur_p) {
            // §867 (+ ptex-base.ch [39.867]): advance past characters.
            // A break is legal before a Japanese character that follows
            // a box/rule/lig/disc/math, between Japanese characters
            // (where the implicit \kanjiskip stretches), and at a
            // Japanese-to-anything transition.
            if self.is_kanji_head(cur_p) {
                match self.mem.node_type(*prev_p) {
                    crate::nodes::HLIST_NODE
                    | crate::nodes::VLIST_NODE
                    | crate::nodes::RULE_NODE
                    | crate::nodes::LIGATURE_NODE
                    | crate::nodes::DISC_NODE
                    | crate::nodes::MATH_NODE
                        if !self.mem.is_char_node(*prev_p) =>
                    {
                        self.lb.cur_p = *prev_p;
                        self.try_break(0, UNHYPHENATED)?;
                        self.lb.cur_p = self.mem.link(self.lb.cur_p);
                    }
                    _ => {}
                }
                cur_p = self.lb.cur_p;
            }
            *prev_p = cur_p;
            loop {
                let f = i32::from(self.mem.font(cur_p));
                let i = self
                    .fonts
                    .char_info(f, i32::from(self.mem.character(cur_p)));
                self.lb.active_width[1] += self.fonts.char_width(f, i);
                let p_is_kanji = self.fonts.dir[f as usize] != 0;
                if p_is_kanji {
                    cur_p = self.mem.link(cur_p); // skip the KANJI code node
                }
                let post_p = self.mem.link(cur_p);
                if p_is_kanji {
                    // A break is legal after this Japanese character.
                    // pTeX keeps cur_p ON the pair's code node while
                    // trying the break, so the whole pair stays on the
                    // line (registering the NEXT node instead splits
                    // the pair: head ends one line, code starts the
                    // next, and hpack then reads the code node as a
                    // char node — the "font 12354" crash).
                    *prev_p = cur_p;
                    self.lb.cur_p = cur_p;
                    if self.mem.is_char_node(post_p) {
                        let chain = self.is_kanji_head(post_p);
                        self.try_break(0, UNHYPHENATED)?;
                        if chain {
                            let g = self.cur_kanji_skip;
                            if g != NULL {
                                self.lb.active_width[1] += self.mem.width(g);
                                self.lb.active_width[2 + self.mem.stretch_order(g) as usize] +=
                                    self.mem.stretch(g);
                                self.lb.active_width[6] += self.mem.shrink(g);
                            }
                        }
                    } else {
                        match self.mem.node_type(post_p) {
                            crate::nodes::HLIST_NODE
                            | crate::nodes::VLIST_NODE
                            | crate::nodes::RULE_NODE
                            | crate::nodes::LIGATURE_NODE
                            | crate::nodes::DISC_NODE
                            | crate::nodes::MATH_NODE => {
                                self.try_break(0, UNHYPHENATED)?;
                            }
                            _ => {}
                        }
                    }
                    cur_p = post_p;
                    self.lb.cur_p = post_p;
                } else {
                    if self.mem.is_char_node(post_p) && self.is_kanji_head(post_p) {
                        // alphabetic -> Japanese boundary: break with
                        // cur_p on the alphabetic char (it ends the line).
                        *prev_p = cur_p;
                        self.lb.cur_p = cur_p;
                        self.try_break(0, UNHYPHENATED)?;
                        self.lb.cur_p = post_p;
                    }
                    cur_p = post_p;
                }
                if !self.mem.is_char_node(cur_p) {
                    break;
                }
            }
            self.lb.cur_p = cur_p;
        }
        match self.mem.node_type(cur_p) {
            HLIST_NODE | VLIST_NODE | RULE_NODE => {
                self.lb.active_width[1] += self.mem.width(cur_p);
            }
            WHATSIT_NODE => {
                // §1362: advance past a whatsit (language nodes).
                if self.mem.subtype(cur_p) == crate::par::LANGUAGE_NODE {
                    self.hy.cur_lang = self.mem.link(cur_p + 1); // what_lang
                    self.hy.l_hyf = i32::from(self.mem.node_type(cur_p + 1)); // what_lhm
                    self.hy.r_hyf = i32::from(self.mem.subtype(cur_p + 1)); // what_rhm
                    self.set_hyph_index(); // etex.ch §1362
                } else if self.mem.is_native_word_node(cur_p) || self.mem.is_glyph_node(cur_p) {
                    // xetex.web: native material has width.
                    self.lb.active_width[1] += self.mem.width(cur_p);
                }
            }
            GLUE_NODE => {
                // §868: a legal breakpoint if prev_p is suitable.
                if *auto_breaking {
                    let pp = *prev_p;
                    if self.mem.is_char_node(pp)
                        || self.mem.precedes_break(pp)
                        || (self.mem.node_type(pp) == KERN_NODE && self.mem.subtype(pp) != EXPLICIT)
                    {
                        self.try_break(0, UNHYPHENATED)?;
                    }
                }
                let g = self.mem.glue_ptr(cur_p);
                if self.mem.shrink_order(g) != crate::mem::NORMAL && self.mem.shrink(g) != 0 {
                    let q = self.finite_shrink(g)?;
                    self.mem.set_glue_ptr(cur_p, q);
                }
                let q = self.mem.glue_ptr(cur_p);
                self.lb.active_width[1] += self.mem.width(q);
                self.lb.active_width[2 + self.mem.stretch_order(q) as usize] += self.mem.stretch(q);
                self.lb.active_width[6] += self.mem.shrink(q);
                if self.lb.second_pass && *auto_breaking {
                    // §894: try to hyphenate the following word.
                    self.try_hyphenate_following_word(cur_p)?;
                }
            }
            KERN_NODE => {
                if self.mem.subtype(cur_p) == EXPLICIT {
                    // kern_break (§866).
                    let nx = self.mem.link(cur_p);
                    if !self.mem.is_char_node(nx)
                        && *auto_breaking
                        && self.mem.node_type(nx) == GLUE_NODE
                    {
                        self.try_break(0, UNHYPHENATED)?;
                    }
                    self.lb.active_width[1] += self.mem.width(cur_p);
                } else {
                    self.lb.active_width[1] += self.mem.width(cur_p);
                }
            }
            LIGATURE_NODE => {
                let f = i32::from(self.mem.font(cur_p + 1));
                let i = self
                    .fonts
                    .char_info(f, i32::from(self.mem.character(cur_p + 1)));
                self.lb.active_width[1] += self.fonts.char_width(f, i);
            }
            DISC_NODE => {
                // §869: try to break after a discretionary fragment.
                let mut s = self.mem.pre_break(cur_p);
                self.lb.disc_width = 0;
                if s == NULL {
                    let php = self.eqtb.int_par(EX_HYPHEN_PENALTY_CODE);
                    self.try_break(php, HYPHENATED)?;
                } else {
                    loop {
                        let w = self.node_width_disc(s)?;
                        self.lb.disc_width += w;
                        s = self.mem.link(s);
                        if s == NULL {
                            break;
                        }
                    }
                    self.lb.active_width[1] += self.lb.disc_width;
                    let hp = self.eqtb.int_par(HYPHEN_PENALTY_CODE);
                    self.try_break(hp, HYPHENATED)?;
                    self.lb.active_width[1] -= self.lb.disc_width;
                }
                let mut r = self.mem.replace_count(cur_p);
                let mut s = self.mem.link(cur_p);
                while r > 0 {
                    let w = self.node_width_disc(s)?;
                    self.lb.active_width[1] += w;
                    r -= 1;
                    s = self.mem.link(s);
                }
                *prev_p = cur_p;
                self.lb.cur_p = s;
                return Ok(()); // goto done5
            }
            MATH_NODE => {
                // etex.ch §866: direction nodes never toggle auto_breaking.
                if self.mem.subtype(cur_p) < crate::nodes::L_CODE {
                    *auto_breaking = self.mem.subtype(cur_p) % 2 == 1;
                }
                let nx = self.mem.link(cur_p);
                if !self.mem.is_char_node(nx)
                    && *auto_breaking
                    && self.mem.node_type(nx) == GLUE_NODE
                {
                    self.try_break(0, UNHYPHENATED)?;
                }
                self.lb.active_width[1] += self.mem.width(cur_p);
            }
            PENALTY_NODE => {
                let p = self.mem.penalty(cur_p);
                self.try_break(p, UNHYPHENATED)?;
            }
            MARK_NODE | INS_NODE | ADJUST_NODE => {}
            crate::nodes::DISP_NODE => {
                // ptex-base.ch [39.866]: no width, no break opportunity.
            }
            _ => {
                return self.confusion("paragraph");
            }
        }
        *prev_p = cur_p;
        self.lb.cur_p = self.mem.link(cur_p);
        Ok(())
    }

    /// §894-§899: scan ahead from the glue node `cur_p` for a hyphenatable
    /// word, then call `hyphenate`.
    fn try_hyphenate_following_word(&mut self, cur_p: Pointer) -> TexResult<()> {
        let mut prev_s = cur_p;
        let mut s = self.mem.link(prev_s);
        if s == NULL {
            return Ok(());
        }
        // §896: skip to node ha.
        let mut c: i32;
        loop {
            if self.mem.is_char_node(s) {
                c = i32::from(self.mem.character(s));
                self.hy.hf = i32::from(self.mem.font(s));
            } else if self.mem.node_type(s) == LIGATURE_NODE {
                if self.mem.lig_ptr(s) == NULL {
                    prev_s = s;
                    s = self.mem.link(prev_s);
                    continue;
                }
                let q = self.mem.lig_ptr(s);
                c = i32::from(self.mem.character(q));
                self.hy.hf = i32::from(self.mem.font(q));
            } else if self.mem.node_type(s) == KERN_NODE
                && self.mem.subtype(s) == crate::mem::NORMAL
            {
                prev_s = s;
                s = self.mem.link(prev_s);
                continue;
            } else if self.mem.node_type(s) == MATH_NODE
                && self.mem.subtype(s) >= crate::nodes::L_CODE
            {
                // etex.ch §896: skip direction nodes.
                prev_s = s;
                s = self.mem.link(prev_s);
                continue;
            } else if self.mem.node_type(s) == WHATSIT_NODE {
                // §1363: advance past a whatsit in the pre-hyphenation loop.
                if self.mem.subtype(s) == crate::par::LANGUAGE_NODE {
                    self.hy.cur_lang = self.mem.link(s + 1);
                    self.hy.l_hyf = i32::from(self.mem.node_type(s + 1));
                    self.hy.r_hyf = i32::from(self.mem.subtype(s + 1));
                    self.set_hyph_index(); // etex.ch §1362 (adv_past)
                }
                prev_s = s;
                s = self.mem.link(prev_s);
                continue;
            } else {
                return Ok(()); // done1
            }
            let lc = self.set_lc_code(c);
            if lc != 0 {
                if lc == c || self.eqtb.int_par(UC_HYPH_CODE) > 0 {
                    break; // done2
                }
                return Ok(()); // done1
            }
            prev_s = s;
            s = self.mem.link(prev_s);
        }
        // done2:
        self.hy.hyf_char = self.fonts.hyphen_char[self.hy.hf as usize];
        if !(0..=255).contains(&self.hy.hyf_char) {
            return Ok(());
        }
        self.hy.ha = prev_s;
        if self.hy.l_hyf + self.hy.r_hyf > 63 {
            return Ok(());
        }
        // §897: skip to node hb, putting letters into hu and hc.
        self.hy.hn = 0;
        'done3: loop {
            if self.mem.is_char_node(s) {
                if i32::from(self.mem.font(s)) != self.hy.hf {
                    break 'done3;
                }
                self.hy.hyf_bchar = i32::from(self.mem.character(s));
                c = self.hy.hyf_bchar;
                if self.set_lc_code(c) == 0 || self.hy.hn == 63 {
                    break 'done3;
                }
                self.hy.hb = s;
                self.hy.hn += 1;
                let hn = self.hy.hn as usize;
                self.hy.hu[hn] = c;
                self.hy.hc[hn] = self.set_lc_code(c);
                self.hy.hyf_bchar = NON_CHAR;
            } else if self.mem.node_type(s) == LIGATURE_NODE {
                // §898: move a ligature's characters to hu and hc.
                if i32::from(self.mem.font(s + 1)) != self.hy.hf {
                    break 'done3;
                }
                let mut j = self.hy.hn;
                let mut q = self.mem.lig_ptr(s);
                if q > NULL {
                    self.hy.hyf_bchar = i32::from(self.mem.character(q));
                }
                while q > NULL {
                    c = i32::from(self.mem.character(q));
                    if self.set_lc_code(c) == 0 || j == 63 {
                        break 'done3;
                    }
                    j += 1;
                    self.hy.hu[j as usize] = c;
                    self.hy.hc[j as usize] = self.set_lc_code(c);
                    q = self.mem.link(q);
                }
                self.hy.hb = s;
                self.hy.hn = j;
                if self.mem.subtype(s) % 2 == 1 {
                    self.hy.hyf_bchar = self.fonts.bchar[self.hy.hf as usize];
                } else {
                    self.hy.hyf_bchar = NON_CHAR;
                }
            } else if self.mem.node_type(s) == KERN_NODE
                && self.mem.subtype(s) == crate::mem::NORMAL
            {
                self.hy.hb = s;
                self.hy.hyf_bchar = self.fonts.bchar[self.hy.hf as usize];
            } else {
                break 'done3;
            }
            s = self.mem.link(s);
        }
        // done3 → §899: check the nodes following hb.
        if self.hy.hn < self.hy.l_hyf + self.hy.r_hyf {
            return Ok(()); // l_hyf and r_hyf are >= 1
        }
        loop {
            if !self.mem.is_char_node(s) {
                match self.mem.node_type(s) {
                    LIGATURE_NODE => {}
                    KERN_NODE => {
                        if self.mem.subtype(s) != crate::mem::NORMAL {
                            break;
                        }
                    }
                    WHATSIT_NODE | GLUE_NODE | PENALTY_NODE | INS_NODE | ADJUST_NODE
                    | MARK_NODE => {
                        break;
                    }
                    MATH_NODE => {
                        // etex.ch §899: direction nodes end the word.
                        if self.mem.subtype(s) >= crate::nodes::L_CODE {
                            break;
                        }
                        return Ok(()); // done1
                    }
                    _ => {
                        return Ok(()); // done1
                    }
                }
            }
            s = self.mem.link(s);
        }
        // done4:
        self.hyphenate(cur_p)
    }

    /// `post_line_break(d)` (§877-§890 + etex.ch).
    /// etex.ch: adjust a local LR stack for `post_line_break`.
    fn plb_adjust_lr(&mut self, q: Pointer, lr: &mut Pointer) {
        if crate::nodes::end_lr(self.mem.subtype(q)) {
            if *lr != NULL
                && self.mem.info(*lr) == i32::from(crate::nodes::end_lr_type(self.mem.subtype(q)))
            {
                let t = *lr;
                *lr = self.mem.link(t);
                self.mem.free_avail(t);
            }
        } else {
            // push_LR
            if let Ok(t) = self.mem.get_avail() {
                let v = i32::from(crate::nodes::end_lr_type(self.mem.subtype(q)));
                self.mem.set_info(t, v);
                self.mem.set_link(t, *lr);
                *lr = t;
            }
        }
    }

    fn post_line_break(&mut self, d: bool) -> TexResult<()> {
        // etex.ch: the LR stack survives display-math interruptions.
        let mut lr: Pointer = self.nest.cur.etex_aux;
        let texxet = self.texxet_en();
        // §878: reverse the links of the relevant passive nodes.
        let mut q = self.mem.rlink(self.lb.best_bet); // break_node
        let mut cur_p: Pointer = NULL;
        loop {
            let r = q;
            q = self.mem.llink(q); // prev_break
            self.mem.set_llink(r, cur_p); // next_break
            cur_p = r;
            if q == NULL {
                break;
            }
        }
        let mut cur_line = self.nest.cur.pg + 1;
        let mut post_disc_break;
        loop {
            if texxet {
                // etex.ch: insert LR nodes at the beginning of the line
                // and adjust the stack from the nodes within it.
                let mut q = self.mem.link(self.mem.temp_head());
                if lr != NULL {
                    let mut temp_ptr = lr;
                    let mut r = q;
                    loop {
                        let t = crate::nodes::begin_lr_type(self.mem.info(temp_ptr) as u16);
                        let sm = self.new_math(0, t)?;
                        self.mem.set_link(sm, r);
                        r = sm;
                        temp_ptr = self.mem.link(temp_ptr);
                        if temp_ptr == NULL {
                            break;
                        }
                    }
                    let th = self.mem.temp_head();
                    self.mem.set_link(th, r);
                }
                while q != self.mem.rlink(cur_p) {
                    if !self.mem.is_char_node(q) && self.mem.node_type(q) == MATH_NODE {
                        self.plb_adjust_lr(q, &mut lr);
                    }
                    q = self.mem.link(q);
                }
            }
            // §880: modify the end of the line.
            let mut disc_break = false;
            post_disc_break = false;
            let mut q = self.mem.rlink(cur_p); // cur_break
            if q != NULL {
                // ptex-base.ch [39.881]: q may be a char node (a break
                // after a Japanese pair); it simply ends the line and
                // takes the \rightskip after it.
                if self.mem.is_char_node(q) {
                    let r = self.new_param_glue(RIGHT_SKIP_CODE)?;
                    let lq = self.mem.link(q);
                    self.mem.set_link(r, lq);
                    self.mem.set_link(q, r);
                    q = r;
                } else if self.mem.node_type(q) == GLUE_NODE {
                    let g = self.mem.glue_ptr(q);
                    self.mem.delete_glue_ref(g);
                    let rs = self.eqtb.glue_par(RIGHT_SKIP_CODE);
                    self.mem.set_glue_ptr(q, rs);
                    self.mem.set_subtype(q, (RIGHT_SKIP_CODE + 1) as u16);
                    self.mem.add_glue_ref(rs);
                } else {
                    if self.mem.node_type(q) == DISC_NODE {
                        // §882: change discretionary to compulsory.
                        let t = self.mem.replace_count(q);
                        // §883: destroy the t nodes following q.
                        let mut r;
                        if t == 0 {
                            r = self.mem.link(q);
                        } else {
                            r = q;
                            let mut t = t;
                            while t > 1 {
                                r = self.mem.link(r);
                                t -= 1;
                            }
                            let s = self.mem.link(r);
                            r = self.mem.link(s);
                            self.mem.set_link(s, NULL);
                            let lq = self.mem.link(q);
                            self.flush_node_list(lq);
                            self.mem.set_replace_count(q, 0);
                        }
                        if self.mem.post_break(q) != NULL {
                            // §884: transplant the post-break list.
                            let mut s = self.mem.post_break(q);
                            while self.mem.link(s) != NULL {
                                s = self.mem.link(s);
                            }
                            self.mem.set_link(s, r);
                            r = self.mem.post_break(q);
                            self.mem.set_post_break(q, NULL);
                            post_disc_break = true;
                        }
                        if self.mem.pre_break(q) != NULL {
                            // §885: transplant the pre-break list.
                            let s = self.mem.pre_break(q);
                            self.mem.set_link(q, s);
                            let mut s = s;
                            while self.mem.link(s) != NULL {
                                s = self.mem.link(s);
                            }
                            self.mem.set_pre_break(q, NULL);
                            q = s;
                        }
                        self.mem.set_link(q, r);
                        disc_break = true;
                    } else if self.mem.node_type(q) == KERN_NODE {
                        self.mem.set_width(q, 0);
                    } else if self.mem.node_type(q) == MATH_NODE {
                        self.mem.set_width(q, 0);
                        if texxet {
                            self.plb_adjust_lr(q, &mut lr);
                        }
                    }
                    // §886: put the \rightskip glue after node q.
                    let r = self.new_param_glue(RIGHT_SKIP_CODE)?;
                    let lq = self.mem.link(q);
                    self.mem.set_link(r, lq);
                    self.mem.set_link(q, r);
                    q = r;
                }
            } else {
                q = self.mem.temp_head();
                while self.mem.link(q) != NULL {
                    q = self.mem.link(q);
                }
                let r = self.new_param_glue(RIGHT_SKIP_CODE)?;
                let lq = self.mem.link(q);
                self.mem.set_link(r, lq);
                self.mem.set_link(q, r);
                q = r;
            }
            if texxet && lr != NULL {
                // etex.ch: insert LR end nodes before the \rightskip glue.
                let mut sp = self.mem.temp_head();
                let mut rp = self.mem.link(sp);
                while rp != q {
                    sp = rp;
                    rp = self.mem.link(sp);
                }
                let mut rp = lr;
                while rp != NULL {
                    let t = self.mem.info(rp) as u16;
                    let tm = self.new_math(0, t)?;
                    self.mem.set_link(sp, tm);
                    sp = tm;
                    rp = self.mem.link(rp);
                }
                self.mem.set_link(sp, q);
            }
            // §887: put the \leftskip glue at the left and detach the line.
            let r = self.mem.link(q);
            self.mem.set_link(q, NULL);
            let th = self.mem.temp_head();
            let mut q = self.mem.link(th);
            self.mem.set_link(th, r);
            let ls = self.eqtb.glue_par(LEFT_SKIP_CODE);
            if ls != self.mem.zero_glue() {
                let r = self.new_param_glue(LEFT_SKIP_CODE)?;
                self.mem.set_link(r, q);
                q = r;
            }
            // §889: call the packaging subroutine.
            let (cur_width, cur_indent) = if cur_line > self.lb.last_special_line {
                (self.lb.second_width, self.lb.second_indent)
            } else {
                let ps = self.eqtb.equiv(self.eqtb.lay.par_shape_loc);
                if ps == NULL {
                    (self.lb.first_width, self.lb.first_indent)
                } else {
                    (
                        self.mem.word(ps + 2 * cur_line).sc(),
                        self.mem.word(ps + 2 * cur_line - 1).sc(),
                    )
                }
            };
            self.adjust_tail = self.mem.adjust_head();
            let jb = self.hpack(q, cur_width, EXACTLY)?;
            self.lb.just_box = jb;
            self.mem.set_shift_amount(jb, cur_indent);
            // §888: append the new box to the current vertical list.
            self.append_to_vlist(jb)?;
            let ah = self.mem.adjust_head();
            if ah != self.adjust_tail {
                let t = self.nest.cur.tail;
                let l = self.mem.link(ah);
                self.mem.set_link(t, l);
                self.nest.cur.tail = self.adjust_tail;
            }
            self.adjust_tail = NULL;
            // §890 (+ etex.ch): append a penalty node, if appropriate. The
            // penalties arrays (\interlinepenalties etc.) override the
            // corresponding single parameters when set; entry min(r, count)
            // applies, where penalty(q) == mem[q+1].int holds the count.
            if cur_line + 1 != self.lb.best_line {
                let pen_base = self.eqtb.lay.etex_pen_base;
                let array_pen = |mem: &crate::mem::Mem, q: Pointer, r: i32| {
                    let r = r.min(mem.word(q + 1).int());
                    mem.word(q + r + 1).int()
                };
                let q = self.eqtb.equiv(pen_base); // \interlinepenalties
                let mut pen = if q != NULL {
                    array_pen(&self.mem, q, cur_line)
                } else {
                    self.eqtb.int_par(INTER_LINE_PENALTY_CODE)
                };
                let q = self.eqtb.equiv(pen_base + 1); // \clubpenalties
                if q != NULL {
                    pen += array_pen(&self.mem, q, cur_line - self.nest.cur.pg);
                } else if cur_line == self.nest.cur.pg + 1 {
                    pen += self.eqtb.int_par(CLUB_PENALTY_CODE);
                }
                // \displaywidowpenalties or \widowpenalties, by d.
                let q = self.eqtb.equiv(pen_base + if d { 3 } else { 2 });
                if q != NULL {
                    pen += array_pen(&self.mem, q, self.lb.best_line - cur_line - 1);
                } else if cur_line + 2 == self.lb.best_line {
                    pen += self.eqtb.int_par(if d {
                        crate::eqtb::DISPLAY_WIDOW_PENALTY_CODE
                    } else {
                        crate::eqtb::WIDOW_PENALTY_CODE
                    });
                }
                if disc_break {
                    pen += self.eqtb.int_par(BROKEN_PENALTY_CODE);
                }
                if pen != 0 {
                    let r = self.new_penalty(pen)?;
                    self.tail_append(r);
                }
            }
            cur_line += 1;
            cur_p = self.mem.llink(cur_p); // next_break
            if cur_p != NULL && !post_disc_break {
                // §879: prune unwanted nodes at the start of the next line.
                let th = self.mem.temp_head();
                let mut r = th;
                let q;
                loop {
                    let q2 = self.mem.link(r);
                    if q2 == self.mem.rlink(cur_p) {
                        q = q2;
                        break;
                    }
                    if self.mem.is_char_node(q2) || self.mem.non_discardable(q2) {
                        q = q2;
                        break;
                    }
                    if self.mem.node_type(q2) == KERN_NODE && self.mem.subtype(q2) != EXPLICIT {
                        q = q2;
                        break;
                    }
                    if self.mem.node_type(q2) == MATH_NODE && texxet {
                        // etex.ch §879: track direction nodes being pruned.
                        self.plb_adjust_lr(q2, &mut lr);
                    }
                    r = q2;
                }
                if r != th {
                    self.mem.set_link(r, NULL);
                    let l = self.mem.link(th);
                    self.flush_node_list(l);
                    self.mem.set_link(th, q);
                }
            }
            if cur_p == NULL {
                break;
            }
        }
        if cur_line != self.lb.best_line || self.mem.link(self.mem.temp_head()) != NULL {
            return self.confusion("line breaking");
        }
        self.nest.cur.pg = self.lb.best_line - 1;
        self.nest.cur.etex_aux = lr; // etex.ch: LR_save
        Ok(())
    }
}

// Re-exported for `new_graf` (§1091).
pub use crate::hyph::norm_min;
const _: fn(i32) -> i32 = norm_lang;
