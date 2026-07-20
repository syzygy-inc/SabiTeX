//! Breaking vertical lists into pages, and the page builder.
//!
//! Ports tex.web Part 44 (§967-§979: `prune_page_top`, `vert_break`,
//! `vsplit`) and Part 45 (§980-§1028: `build_page`, `fire_up`, and the
//! output-routine plumbing).

use crate::engine::{Engine, VMODE};
use crate::eqtb::*;
use crate::error::TexResult;
use crate::expand::{
    BOT_MARK_CODE, FIRST_MARK_CODE, SPLIT_BOT_MARK_CODE, SPLIT_FIRST_MARK_CODE, TOP_MARK_CODE,
};
use crate::linebreak::AWFUL_BAD;
use crate::nest::IGNORE_DEPTH;
use crate::nodes::*;
use crate::pack::{ADDITIONAL, EXACTLY};
use crate::scan::MAX_DIMEN;
use crate::types::{Pointer, Scaled, MAX_HALFWORD, NULL};

// §980: page_contents values.
pub const EMPTY: u8 = 0;
pub const INSERTS_ONLY: u8 = 1;
pub const BOX_THERE: u8 = 2;

// §981: page insertion nodes.
pub const PAGE_INS_NODE_SIZE: i32 = 4;
pub const INSERTING: u16 = 0;
pub const SPLIT_UP: u16 = 1;

/// `deplorable` (§974).
pub const DEPLORABLE: i32 = 100_000;

impl Engine {
    /// `prune_page_top(p, s)` (§968-§969 + etex.ch): when `s` is true the
    /// deleted nodes are collected on `split_disc` instead of destroyed.
    pub fn prune_page_top(&mut self, p: Pointer, s: bool) -> TexResult<Pointer> {
        let th = self.mem.temp_head();
        let mut prev_p = th;
        self.mem.set_link(th, p);
        let mut r: Pointer = NULL;
        let mut p = p;
        while p != NULL {
            match self.mem.node_type(p) {
                HLIST_NODE | VLIST_NODE | RULE_NODE => {
                    // §969: insert glue for split_top_skip.
                    let (q, spec) = self.new_skip_param(SPLIT_TOP_SKIP_CODE)?;
                    self.mem.set_link(prev_p, q);
                    self.mem.set_link(q, p);
                    if self.mem.width(spec) > self.mem.height(p) {
                        let w = self.mem.width(spec) - self.mem.height(p);
                        self.mem.set_width(spec, w);
                    } else {
                        self.mem.set_width(spec, 0);
                    }
                    p = NULL;
                }
                WHATSIT_NODE | MARK_NODE | INS_NODE => {
                    prev_p = p;
                    p = self.mem.link(prev_p);
                }
                GLUE_NODE | KERN_NODE | PENALTY_NODE => {
                    let q = p;
                    p = self.mem.link(q);
                    self.mem.set_link(q, NULL);
                    self.mem.set_link(prev_p, p);
                    if s {
                        // etex.ch: collect on split_disc.
                        if self.disc_ptr[crate::control::VSPLIT_CODE as usize] == NULL {
                            self.disc_ptr[crate::control::VSPLIT_CODE as usize] = q;
                        } else {
                            self.mem.set_link(r, q);
                        }
                        r = q;
                    } else {
                        self.flush_node_list(q);
                    }
                }
                _ => {
                    return self.confusion("pruning").map(|_| NULL);
                }
            }
        }
        Ok(self.mem.link(th))
    }

    /// `vert_break(p, h, d)` (§970-§977): finds the optimum page break.
    pub fn vert_break(&mut self, p: Pointer, h: Scaled, d: Scaled) -> TexResult<Pointer> {
        let mut prev_p = p;
        let mut p = p;
        let mut least_cost = AWFUL_BAD;
        let mut best_place: Pointer = NULL;
        let mut prev_dp: Scaled = 0;
        let mut active_height: [Scaled; 7] = [0; 7];
        loop {
            // §972: check if p is a legal breakpoint.
            let pi: i32;
            let mut update_heights = false;
            let mut not_found = false;
            if p == NULL {
                pi = EJECT_PENALTY;
            } else {
                // §973.
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE | RULE_NODE => {
                        active_height[1] += prev_dp + self.mem.height(p);
                        prev_dp = self.mem.depth(p);
                        not_found = true;
                        pi = 0;
                    }
                    WHATSIT_NODE => {
                        not_found = true;
                        pi = 0;
                    }
                    GLUE_NODE => {
                        if self.mem.precedes_break(prev_p) {
                            pi = 0;
                        } else {
                            update_heights = true;
                            pi = 0;
                        }
                    }
                    KERN_NODE => {
                        let t = if self.mem.link(p) == NULL {
                            PENALTY_NODE
                        } else {
                            self.mem.node_type(self.mem.link(p))
                        };
                        if t == GLUE_NODE {
                            pi = 0;
                        } else {
                            update_heights = true;
                            pi = 0;
                        }
                    }
                    PENALTY_NODE => {
                        pi = self.mem.penalty(p);
                    }
                    MARK_NODE | INS_NODE => {
                        not_found = true;
                        pi = 0;
                    }
                    _ => {
                        return self.confusion("vertbreak").map(|_| NULL);
                    }
                }
            }
            // §974: check if p is a new champion breakpoint.
            if !update_heights && !not_found && pi < INF_PENALTY {
                let b = if active_height[1] < h {
                    if active_height[3] != 0 || active_height[4] != 0 || active_height[5] != 0 {
                        0
                    } else {
                        crate::arith::badness(h - active_height[1], active_height[2])
                    }
                } else if active_height[1] - h > active_height[6] {
                    AWFUL_BAD
                } else {
                    crate::arith::badness(active_height[1] - h, active_height[6])
                };
                let mut b2 = b;
                if b < AWFUL_BAD {
                    if pi <= EJECT_PENALTY {
                        b2 = pi;
                    } else if b < crate::types::INF_BAD {
                        b2 = b + pi;
                    } else {
                        b2 = DEPLORABLE;
                    }
                }
                if b2 <= least_cost {
                    best_place = p;
                    least_cost = b2;
                    self.best_height_plus_depth = active_height[1] + prev_dp;
                }
                if b2 == AWFUL_BAD || pi <= EJECT_PENALTY {
                    return Ok(best_place);
                }
            }
            if !not_found
                && p != NULL
                && (self.mem.node_type(p) == GLUE_NODE || self.mem.node_type(p) == KERN_NODE)
            {
                // §976: update heights for a glue or kern node.
                let q = if self.mem.node_type(p) == KERN_NODE {
                    p
                } else {
                    let mut q = self.mem.glue_ptr(p);
                    active_height[2 + self.mem.stretch_order(q) as usize] += self.mem.stretch(q);
                    active_height[6] += self.mem.shrink(q);
                    if self.mem.shrink_order(q) != crate::mem::NORMAL && self.mem.shrink(q) != 0 {
                        self.print_err("Infinite glue shrinkage found in box being split");
                        self.help(&[
                            "The box you are \\vsplitting contains some infinitely",
                            "shrinkable glue, e.g., `\\vss' or `\\vskip 0pt minus 1fil'.",
                            "Such glue doesn't belong there; but you can safely proceed,",
                            "since the offensive shrinkability has been made finite.",
                        ]);
                        self.error()?;
                        let r = self.new_spec(q)?;
                        self.mem.set_shrink_order(r, crate::mem::NORMAL);
                        self.mem.delete_glue_ref(q);
                        self.mem.set_glue_ptr(p, r);
                        q = r;
                    }
                    q
                };
                active_height[1] += prev_dp + self.mem.width(q);
                prev_dp = 0;
            }
            // not_found:
            if prev_dp > d {
                active_height[1] += prev_dp - d;
                prev_dp = d;
            }
            prev_p = p;
            p = self.mem.link(prev_p);
        }
    }

    /// `vsplit(n, h)` (§977-§979).
    pub fn vsplit(&mut self, n: i32, h: Scaled) -> TexResult<Pointer> {
        let v = self.fetch_box(n)?;
        // etex.ch: recycle the previous \splitdiscards list.
        let sd = self.disc_ptr[crate::control::VSPLIT_CODE as usize];
        self.flush_node_list(sd);
        self.disc_ptr[crate::control::VSPLIT_CODE as usize] = NULL;
        // etex.ch: discard the split marks of every mark class.
        if self.sa_root[crate::sa::MARK_VAL as usize] != NULL {
            let m = self.sa_root[crate::sa::MARK_VAL as usize];
            if self.do_marks(crate::sa::VSPLIT_INIT, 0, m) {
                self.sa_root[crate::sa::MARK_VAL as usize] = NULL;
            }
        }
        if self.cur_mark[SPLIT_FIRST_MARK_CODE] != NULL {
            let m = self.cur_mark[SPLIT_FIRST_MARK_CODE];
            self.delete_token_ref(m);
            self.cur_mark[SPLIT_FIRST_MARK_CODE] = NULL;
            let m = self.cur_mark[SPLIT_BOT_MARK_CODE];
            self.delete_token_ref(m);
            self.cur_mark[SPLIT_BOT_MARK_CODE] = NULL;
        }
        // §978: trivial cases.
        if v == NULL {
            return Ok(NULL);
        }
        if self.mem.node_type(v) != VLIST_NODE {
            self.print_err("");
            self.print_esc_str("vsplit");
            self.print_chars(" needs a ");
            self.print_esc_str("vbox");
            self.help(&[
                "The box you are trying to split is an \\hbox.",
                "I can't split such a box, so I'll leave it alone.",
            ]);
            self.error()?;
            return Ok(NULL);
        }
        let smd = self.eqtb.dimen_par(SPLIT_MAX_DEPTH_CODE);
        let q = self.vert_break(self.mem.list_ptr(v), h, smd)?;
        // §979: process the marks and cut the list.
        let mut p = self.mem.list_ptr(v);
        if p == q {
            self.mem.set_list_ptr(v, NULL);
        } else {
            loop {
                if self.mem.node_type(p) == MARK_NODE {
                    if self.mem.mark_class(p) != 0 {
                        // etex.ch: update the current marks for vsplit.
                        let cls = self.mem.mark_class(p);
                        self.find_sa_element(crate::sa::MARK_VAL, cls, true)?;
                        let q = self.cur_ptr;
                        let m = self.mem.mark_ptr(p);
                        if self.mem.link(q + 2) == NULL {
                            self.mem.set_link(q + 2, m); // sa_split_first_mark
                            self.add_token_ref(m);
                        } else {
                            let old = self.mem.info(q + 3);
                            self.delete_token_ref(old);
                        }
                        self.mem.set_info(q + 3, m); // sa_split_bot_mark
                        self.add_token_ref(m);
                    } else if self.cur_mark[SPLIT_FIRST_MARK_CODE] == NULL {
                        let m = self.mem.mark_ptr(p);
                        self.cur_mark[SPLIT_FIRST_MARK_CODE] = m;
                        self.cur_mark[SPLIT_BOT_MARK_CODE] = m;
                        let c = self.mem.info(m);
                        self.mem.set_info(m, c + 2);
                    } else {
                        let m = self.cur_mark[SPLIT_BOT_MARK_CODE];
                        self.delete_token_ref(m);
                        let m = self.mem.mark_ptr(p);
                        self.cur_mark[SPLIT_BOT_MARK_CODE] = m;
                        self.add_token_ref(m);
                    }
                }
                if self.mem.link(p) == q {
                    self.mem.set_link(p, NULL);
                    break;
                }
                p = self.mem.link(p);
            }
        }
        let saving = self.eqtb.int_par(crate::eqtb::SAVING_VDISCARDS_CODE) > 0;
        let q = self.prune_page_top(q, saving)?;
        let p = self.mem.list_ptr(v);
        self.mem.free_node(v, BOX_NODE_SIZE);
        let boxed = if q == NULL {
            NULL
        } else {
            self.vpack(q, 0, ADDITIONAL)? // natural
        };
        self.change_box(n, boxed)?;
        self.vpackage(p, h, EXACTLY, smd)
    }

    /// `freeze_page_specs(s)` (§987).
    fn freeze_page_specs(&mut self, s: u8) {
        self.page_contents = s;
        self.page_so_far[0] = self.eqtb.dimen_par(VSIZE_CODE); // page_goal
        self.page_max_depth = self.eqtb.dimen_par(MAX_DEPTH_CODE);
        self.page_so_far[7] = 0; // page_depth
        for k in 1..=6 {
            self.page_so_far[k] = 0;
        }
        self.least_page_cost = AWFUL_BAD;
        if self.eqtb.int_par(TRACING_PAGES_CODE) > 0 {
            self.begin_diagnostic();
            self.print_nl_chars("%% goal height=");
            let g = self.page_so_far[0];
            self.print_scaled(g);
            self.print_chars(", max depth=");
            let d = self.page_max_depth;
            self.print_scaled(d);
            self.end_diagnostic(false);
        }
    }

    /// `print_totals` (§985).
    pub fn print_totals(&mut self) {
        let t = self.page_so_far[1];
        self.print_scaled(t);
        for (k, unit) in [(2, ""), (3, "fil"), (4, "fill"), (5, "filll")] {
            if self.page_so_far[k] != 0 {
                self.print_chars(" plus ");
                let v = self.page_so_far[k];
                self.print_scaled(v);
                self.print_chars(unit);
            }
        }
        if self.page_so_far[6] != 0 {
            self.print_chars(" minus ");
            let v = self.page_so_far[6];
            self.print_scaled(v);
        }
    }

    /// `box_error(n)` (§992).
    pub fn box_error(&mut self, n: i32) -> TexResult<()> {
        self.error()?;
        self.begin_diagnostic();
        self.print_nl_chars("The following box has been deleted:");
        let b = self.fetch_box(n)?;
        self.show_box(b);
        self.end_diagnostic(true);
        let b = self.fetch_box(n)?;
        self.flush_node_list(b);
        self.change_box(n, NULL)?;
        Ok(())
    }

    /// `ensure_vbox(n)` (§993).
    pub fn ensure_vbox(&mut self, n: i32) -> TexResult<()> {
        let p = self.eqtb.box_reg(n);
        if p != NULL && self.mem.node_type(p) == HLIST_NODE {
            self.print_err("Insertions can only be added to a vbox");
            self.help(&[
                "Tut tut: You're trying to \\insert into a",
                "\\box register that now contains an \\hbox.",
                "Proceed, and I'll discard its present contents.",
            ]);
            self.box_error(n)?;
        }
        Ok(())
    }

    /// `build_page` (§994-§1012): move contributions to the current page.
    pub fn build_page(&mut self) -> TexResult<()> {
        let ch = self.mem.contrib_head();
        let ph = self.mem.page_head();
        if self.mem.link(ch) == NULL || self.output_active {
            return Ok(());
        }
        'outer: loop {
            // continue:
            let p = self.mem.link(ch);
            // §996: update last_glue/last_penalty/last_kern.
            if self.last_glue != MAX_HALFWORD {
                let lg = self.last_glue;
                self.mem.delete_glue_ref(lg);
            }
            self.last_penalty = 0;
            self.last_kern = 0;
            self.last_node_type = i32::from(self.mem.node_type(p)) + 1;
            if self.mem.node_type(p) == GLUE_NODE {
                self.last_glue = self.mem.glue_ptr(p);
                let lg = self.last_glue;
                self.mem.add_glue_ref(lg);
            } else {
                self.last_glue = MAX_HALFWORD;
                if self.mem.node_type(p) == PENALTY_NODE {
                    self.last_penalty = self.mem.penalty(p);
                } else if self.mem.node_type(p) == KERN_NODE {
                    self.last_kern = self.mem.width(p);
                }
            }
            // §997: move node p to the current page.
            let mut pi = INF_PENALTY;
            let mut recycle = false;
            let mut contribute = false;
            let mut update_heights = false;
            match self.mem.node_type(p) {
                HLIST_NODE | VLIST_NODE | RULE_NODE => {
                    if self.page_contents < BOX_THERE {
                        // §1000: insert the \topskip glue.
                        if self.page_contents == EMPTY {
                            self.freeze_page_specs(BOX_THERE);
                        } else {
                            self.page_contents = BOX_THERE;
                        }
                        let (q, spec) = self.new_skip_param(TOP_SKIP_CODE)?;
                        if self.mem.width(spec) > self.mem.height(p) {
                            let w = self.mem.width(spec) - self.mem.height(p);
                            self.mem.set_width(spec, w);
                        } else {
                            self.mem.set_width(spec, 0);
                        }
                        self.mem.set_link(q, p);
                        self.mem.set_link(ch, q);
                        continue 'outer;
                    }
                    // §1001.
                    self.page_so_far[1] += self.page_so_far[7] + self.mem.height(p);
                    self.page_so_far[7] = self.mem.depth(p);
                    contribute = true;
                }
                WHATSIT_NODE => {
                    contribute = true;
                }
                GLUE_NODE => {
                    if self.page_contents < BOX_THERE {
                        recycle = true;
                    } else if self.mem.precedes_break(self.page_tail) {
                        pi = 0;
                    } else {
                        update_heights = true;
                    }
                }
                KERN_NODE => {
                    if self.page_contents < BOX_THERE {
                        recycle = true;
                    } else if self.mem.link(p) == NULL {
                        return Ok(());
                    } else if self.mem.node_type(self.mem.link(p)) == GLUE_NODE {
                        pi = 0;
                    } else {
                        update_heights = true;
                    }
                }
                PENALTY_NODE => {
                    if self.page_contents < BOX_THERE {
                        recycle = true;
                    } else {
                        pi = self.mem.penalty(p);
                    }
                }
                MARK_NODE => {
                    contribute = true;
                }
                INS_NODE => {
                    self.append_insertion_to_page(p)?;
                    contribute = true;
                }
                _ => {
                    return self.confusion("page");
                }
            }
            if !recycle && !contribute && !update_heights && pi < INF_PENALTY {
                // §1005: check for a new champion breakpoint.
                let b = if self.page_so_far[1] < self.page_so_far[0] {
                    if self.page_so_far[3] != 0
                        || self.page_so_far[4] != 0
                        || self.page_so_far[5] != 0
                    {
                        0
                    } else {
                        crate::arith::badness(
                            self.page_so_far[0] - self.page_so_far[1],
                            self.page_so_far[2],
                        )
                    }
                } else if self.page_so_far[1] - self.page_so_far[0] > self.page_so_far[6] {
                    AWFUL_BAD
                } else {
                    crate::arith::badness(
                        self.page_so_far[1] - self.page_so_far[0],
                        self.page_so_far[6],
                    )
                };
                let mut c = if b < AWFUL_BAD {
                    if pi <= EJECT_PENALTY {
                        pi
                    } else if b < crate::types::INF_BAD {
                        b + pi + self.insert_penalties
                    } else {
                        DEPLORABLE
                    }
                } else {
                    b
                };
                if self.insert_penalties >= 10000 {
                    c = AWFUL_BAD;
                }
                if self.eqtb.int_par(TRACING_PAGES_CODE) > 0 {
                    // §1006.
                    self.begin_diagnostic();
                    self.print_nl_chars("%");
                    self.print_chars(" t=");
                    self.print_totals();
                    self.print_chars(" g=");
                    let g = self.page_so_far[0];
                    self.print_scaled(g);
                    self.print_chars(" b=");
                    if b == AWFUL_BAD {
                        self.print_char('*' as i32);
                    } else {
                        self.print_int(b);
                    }
                    self.print_chars(" p=");
                    self.print_int(pi);
                    self.print_chars(" c=");
                    if c == AWFUL_BAD {
                        self.print_char('*' as i32);
                    } else {
                        self.print_int(c);
                    }
                    if c <= self.least_page_cost {
                        self.print_char('#' as i32);
                    }
                    self.end_diagnostic(false);
                }
                if c <= self.least_page_cost {
                    self.best_page_break = p;
                    self.best_size = self.page_so_far[0];
                    self.least_page_cost = c;
                    let pih = self.mem.page_ins_head();
                    let mut r = self.mem.link(pih);
                    while r != pih {
                        let li = self.mem.link(r + 2); // last_ins_ptr
                        self.mem.set_info(r + 2, li); // best_ins_ptr
                        r = self.mem.link(r);
                    }
                }
                if c == AWFUL_BAD || pi <= EJECT_PENALTY {
                    self.fire_up(p)?;
                    if self.output_active {
                        return Ok(()); // the output routine will act
                    }
                    // the page was shipped out by the default routine
                    if self.mem.link(ch) == NULL {
                        break 'outer;
                    }
                    continue 'outer;
                }
            }
            if !recycle && !contribute {
                if self.mem.node_type(p) == GLUE_NODE || self.mem.node_type(p) == KERN_NODE {
                    // §1004: update the page measurements.
                    let q = if self.mem.node_type(p) == KERN_NODE {
                        p
                    } else {
                        let mut q = self.mem.glue_ptr(p);
                        self.page_so_far[2 + self.mem.stretch_order(q) as usize] +=
                            self.mem.stretch(q);
                        self.page_so_far[6] += self.mem.shrink(q);
                        if self.mem.shrink_order(q) != crate::mem::NORMAL && self.mem.shrink(q) != 0
                        {
                            self.print_err("Infinite glue shrinkage found on current page");
                            self.help(&[
                                "The page about to be output contains some infinitely",
                                "shrinkable glue, e.g., `\\vss' or `\\vskip 0pt minus 1fil'.",
                                "Such glue doesn't belong there; but you can safely proceed,",
                                "since the offensive shrinkability has been made finite.",
                            ]);
                            self.error()?;
                            let r = self.new_spec(q)?;
                            self.mem.set_shrink_order(r, crate::mem::NORMAL);
                            self.mem.delete_glue_ref(q);
                            self.mem.set_glue_ptr(p, r);
                            q = r;
                        }
                        q
                    };
                    self.page_so_far[1] += self.page_so_far[7] + self.mem.width(q);
                    self.page_so_far[7] = 0;
                }
                contribute = true;
            }
            if recycle {
                // §999.
                let lp = self.mem.link(p);
                self.mem.set_link(ch, lp);
                self.mem.set_link(p, NULL);
                if self.eqtb.int_par(crate::eqtb::SAVING_VDISCARDS_CODE) > 0 {
                    // etex.ch §999: collect on \pagediscards.
                    if self.disc_ptr[crate::control::LAST_BOX_CODE as usize] == NULL {
                        self.disc_ptr[crate::control::LAST_BOX_CODE as usize] = p;
                    } else {
                        let t = self.disc_ptr[crate::control::COPY_CODE as usize];
                        self.mem.set_link(t, p);
                    }
                    self.disc_ptr[crate::control::COPY_CODE as usize] = p; // tail
                } else {
                    self.flush_node_list(p);
                }
            } else if contribute {
                // §1003: ensure page_max_depth, then link p in (§998).
                if self.page_so_far[7] > self.page_max_depth {
                    self.page_so_far[1] += self.page_so_far[7] - self.page_max_depth;
                    self.page_so_far[7] = self.page_max_depth;
                }
                let pt = self.page_tail;
                self.mem.set_link(pt, p);
                self.page_tail = p;
                let lp = self.mem.link(p);
                self.mem.set_link(ch, lp);
                self.mem.set_link(p, NULL);
            }
            if self.mem.link(ch) == NULL {
                break;
            }
        }
        // §995: make the contribution list empty.
        if self.nest.ptr == 0 {
            self.nest.cur.tail = ch; // vertical mode
        } else {
            self.nest.stack[0].tail = ch; // other modes
        }
        let _ = ph;
        Ok(())
    }

    /// §1008-§1010: append an insertion to the current page.
    fn append_insertion_to_page(&mut self, p: Pointer) -> TexResult<()> {
        if self.page_contents == EMPTY {
            self.freeze_page_specs(INSERTS_ONLY);
        }
        let n = i32::from(self.mem.subtype(p));
        let pih = self.mem.page_ins_head();
        let mut r = pih;
        while n >= i32::from(self.mem.subtype(self.mem.link(r))) {
            r = self.mem.link(r);
        }
        if i32::from(self.mem.subtype(r)) != n {
            // §1009: create a page insertion node.
            let q = self.mem.get_node(PAGE_INS_NODE_SIZE)?;
            let lr = self.mem.link(r);
            self.mem.set_link(q, lr);
            self.mem.set_link(r, q);
            r = q;
            self.mem.set_subtype(r, n as u16);
            self.mem.set_node_type(r, INSERTING);
            self.ensure_vbox(n)?;
            let b = self.eqtb.box_reg(n);
            let h = if b == NULL {
                0
            } else {
                self.mem.height(b) + self.mem.depth(b)
            };
            self.mem.set_height(r, h);
            self.mem.set_info(r + 2, NULL); // best_ins_ptr
            let q = self.eqtb.equiv(self.eqtb.lay.skip_base + n);
            let h2 = if self.eqtb.count(n) == 1000 {
                self.mem.height(r)
            } else {
                crate::arith::x_over_n(&mut self.arith, self.mem.height(r), 1000)
                    * self.eqtb.count(n)
            };
            self.page_so_far[0] -= h2 + self.mem.width(q);
            self.page_so_far[2 + self.mem.stretch_order(q) as usize] += self.mem.stretch(q);
            self.page_so_far[6] += self.mem.shrink(q);
            if self.mem.shrink_order(q) != crate::mem::NORMAL && self.mem.shrink(q) != 0 {
                self.print_err("Infinite glue shrinkage inserted from ");
                self.print_esc_str("skip");
                self.print_int(n);
                self.help(&[
                    "The correction glue for page breaking with insertions",
                    "must have finite shrinkability. But you may proceed,",
                    "since the offensive shrinkability has been made finite.",
                ]);
                self.error()?;
            }
        }
        if self.mem.node_type(r) == SPLIT_UP {
            self.insert_penalties += self.mem.float_cost(p);
        } else {
            self.mem.set_link(r + 2, p); // last_ins_ptr
            let delta = self.page_so_far[0] - self.page_so_far[1] - self.page_so_far[7]
                + self.page_so_far[6];
            let h = if self.eqtb.count(n) == 1000 {
                self.mem.height(p)
            } else {
                crate::arith::x_over_n(&mut self.arith, self.mem.height(p), 1000)
                    * self.eqtb.count(n)
            };
            if (h <= 0 || h <= delta)
                && self.mem.height(p) + self.mem.height(r) <= self.eqtb.dimen(n)
            {
                self.page_so_far[0] -= h;
                let hh = self.mem.height(r) + self.mem.height(p);
                self.mem.set_height(r, hh);
            } else {
                // §1010: split the insertion.
                let w = if self.eqtb.count(n) <= 0 {
                    MAX_DIMEN
                } else {
                    let mut w = self.page_so_far[0] - self.page_so_far[1] - self.page_so_far[7];
                    if self.eqtb.count(n) != 1000 {
                        w = crate::arith::x_over_n(&mut self.arith, w, self.eqtb.count(n)) * 1000;
                    }
                    w
                };
                let w = w.min(self.eqtb.dimen(n) - self.mem.height(r));
                let q = self.vert_break(self.mem.ins_ptr(p), w, self.mem.depth(p))?;
                let hh = self.mem.height(r) + self.best_height_plus_depth;
                self.mem.set_height(r, hh);
                if self.eqtb.int_par(TRACING_PAGES_CODE) > 0 {
                    // §1011.
                    self.begin_diagnostic();
                    self.print_nl_chars("% split");
                    self.print_int(n);
                    self.print_chars(" to ");
                    self.print_scaled(w);
                    self.print_char(',' as i32);
                    let bh = self.best_height_plus_depth;
                    self.print_scaled(bh);
                    self.print_chars(" p=");
                    if q == NULL {
                        self.print_int(EJECT_PENALTY);
                    } else if self.mem.node_type(q) == PENALTY_NODE {
                        let pq = self.mem.penalty(q);
                        self.print_int(pq);
                    } else {
                        self.print_char('0' as i32);
                    }
                    self.end_diagnostic(false);
                }
                if self.eqtb.count(n) != 1000 {
                    self.best_height_plus_depth =
                        crate::arith::x_over_n(&mut self.arith, self.best_height_plus_depth, 1000)
                            * self.eqtb.count(n);
                }
                self.page_so_far[0] -= self.best_height_plus_depth;
                self.mem.set_node_type(r, SPLIT_UP);
                self.mem.set_link(r + 1, q); // broken_ptr
                self.mem.set_info(r + 1, p); // broken_ins
                if q == NULL {
                    self.insert_penalties += EJECT_PENALTY;
                } else if self.mem.node_type(q) == PENALTY_NODE {
                    self.insert_penalties += self.mem.penalty(q);
                }
            }
        }
        Ok(())
    }

    /// `fire_up(c)` (§1012-§1028).
    pub fn fire_up(&mut self, c: Pointer) -> TexResult<()> {
        // §1013: set \outputpenalty.
        let bpb = self.best_page_break;
        if self.mem.node_type(bpb) == PENALTY_NODE {
            let pen = self.mem.penalty(bpb);
            let loc = self.eqtb.lay.int_base + OUTPUT_PENALTY_CODE;
            self.geq_word_define(loc, pen);
            self.mem.set_penalty(bpb, INF_PENALTY);
        } else {
            let loc = self.eqtb.lay.int_base + OUTPUT_PENALTY_CODE;
            self.geq_word_define(loc, INF_PENALTY);
        }
        // etex.ch §1012: initialize the sparse mark classes.
        if self.sa_root[crate::sa::MARK_VAL as usize] != NULL {
            let m = self.sa_root[crate::sa::MARK_VAL as usize];
            if self.do_marks(crate::sa::FIRE_UP_INIT, 0, m) {
                self.sa_root[crate::sa::MARK_VAL as usize] = NULL;
            }
        }
        if self.cur_mark[BOT_MARK_CODE] != NULL {
            if self.cur_mark[TOP_MARK_CODE] != NULL {
                let m = self.cur_mark[TOP_MARK_CODE];
                self.delete_token_ref(m);
            }
            self.cur_mark[TOP_MARK_CODE] = self.cur_mark[BOT_MARK_CODE];
            let m = self.cur_mark[TOP_MARK_CODE];
            self.add_token_ref(m);
            let m = self.cur_mark[FIRST_MARK_CODE];
            self.delete_token_ref(m);
            self.cur_mark[FIRST_MARK_CODE] = NULL;
        }
        // §1014: put the optimal page into box 255.
        if c == self.best_page_break {
            self.best_page_break = NULL; // c not yet linked in
        }
        // §1015: ensure box 255 is empty.
        if self.eqtb.box_reg(255) != NULL {
            self.print_err("");
            self.print_esc_str("box");
            self.print_chars("255 is not void");
            self.help(&[
                "You shouldn't use \\box255 except in \\output routines.",
                "Proceed, and I'll discard its present contents.",
            ]);
            self.box_error(255)?;
        }
        self.insert_penalties = 0; // counts insertions held over
        let save_split_top_skip = self.eqtb.glue_par(SPLIT_TOP_SKIP_CODE);
        let holding = self.eqtb.int_par(HOLDING_INSERTS_CODE);
        let pih = self.mem.page_ins_head();
        if holding <= 0 {
            // §1018: prepare insertion boxes to act as queues.
            let mut r = self.mem.link(pih);
            while r != pih {
                if self.mem.info(r + 2) != NULL {
                    // best_ins_ptr
                    let n = i32::from(self.mem.subtype(r));
                    self.ensure_vbox(n)?;
                    if self.eqtb.box_reg(n) == NULL {
                        let b = self.new_null_box()?;
                        let loc = self.eqtb.lay.box_base + n;
                        self.eqtb.set_equiv(loc, b);
                    }
                    let mut p = self.eqtb.box_reg(n) + 5; // list_offset
                    while self.mem.link(p) != NULL {
                        p = self.mem.link(p);
                    }
                    self.mem.set_link(r + 2, p); // last_ins_ptr
                }
                r = self.mem.link(r);
            }
        }
        let hh = self.mem.hold_head();
        let mut q = hh;
        self.mem.set_link(q, NULL);
        let ph = self.mem.page_head();
        let mut prev_p = ph;
        let mut p = self.mem.link(prev_p);
        while p != self.best_page_break {
            if self.mem.node_type(p) == INS_NODE {
                if holding <= 0 {
                    // §1020-§1022: insert or hold over.
                    let mut r = self.mem.link(pih);
                    while self.mem.subtype(r) != self.mem.subtype(p) {
                        r = self.mem.link(r);
                    }
                    let mut wait = self.mem.info(r + 2) == NULL; // best_ins_ptr
                    if !wait {
                        let mut s = self.mem.link(r + 2); // last_ins_ptr
                        let ip = self.mem.ins_ptr(p);
                        self.mem.set_link(s, ip);
                        if self.mem.info(r + 2) == p {
                            // §1021: wrap up the box for r.
                            if self.mem.node_type(r) == SPLIT_UP
                                && self.mem.info(r + 1) == p
                                && self.mem.link(r + 1) != NULL
                            {
                                while self.mem.link(s) != self.mem.link(r + 1) {
                                    s = self.mem.link(s);
                                }
                                self.mem.set_link(s, NULL);
                                let stp = self.mem.split_top_ptr(p);
                                let loc = self.eqtb.lay.glue_base + SPLIT_TOP_SKIP_CODE;
                                self.eqtb.set_equiv(loc, stp);
                                let bp = self.mem.link(r + 1);
                                let pruned = self.prune_page_top(bp, false)?;
                                self.mem.set_ins_ptr(p, pruned);
                                if pruned != NULL {
                                    let tb = self.vpack(pruned, 0, ADDITIONAL)?;
                                    let h = self.mem.height(tb) + self.mem.depth(tb);
                                    self.mem.set_height(p, h);
                                    self.mem.free_node(tb, BOX_NODE_SIZE);
                                    wait = true;
                                }
                            }
                            self.mem.set_info(r + 2, NULL); // best_ins_ptr
                            let n = i32::from(self.mem.subtype(r));
                            let b = self.eqtb.box_reg(n);
                            let tp = self.mem.list_ptr(b);
                            self.mem.free_node(b, BOX_NODE_SIZE);
                            let nb = self.vpack(tp, 0, ADDITIONAL)?;
                            let loc = self.eqtb.lay.box_base + n;
                            self.eqtb.set_equiv(loc, nb);
                        } else {
                            while self.mem.link(s) != NULL {
                                s = self.mem.link(s);
                            }
                            self.mem.set_link(r + 2, s); // last_ins_ptr
                        }
                    }
                    // §1022: remove p from the page.
                    let lp = self.mem.link(p);
                    self.mem.set_link(prev_p, lp);
                    self.mem.set_link(p, NULL);
                    if wait {
                        self.mem.set_link(q, p);
                        q = p;
                        self.insert_penalties += 1;
                    } else {
                        let stp = self.mem.split_top_ptr(p);
                        self.mem.delete_glue_ref(stp);
                        self.mem.free_node(p, INS_NODE_SIZE);
                    }
                    p = prev_p;
                }
            } else if self.mem.node_type(p) == MARK_NODE {
                if self.mem.mark_class(p) != 0 {
                    // etex.ch: update the current marks for fire_up.
                    let cls = self.mem.mark_class(p);
                    self.find_sa_element(crate::sa::MARK_VAL, cls, true)?;
                    let q = self.cur_ptr;
                    let m = self.mem.mark_ptr(p);
                    if self.mem.link(q + 1) == NULL {
                        self.mem.set_link(q + 1, m); // sa_first_mark
                        self.add_token_ref(m);
                    }
                    if self.mem.info(q + 2) != NULL {
                        let old = self.mem.info(q + 2);
                        self.delete_token_ref(old);
                    }
                    self.mem.set_info(q + 2, m); // sa_bot_mark
                    self.add_token_ref(m);
                } else {
                    // §1016: update first_mark and bot_mark.
                    if self.cur_mark[FIRST_MARK_CODE] == NULL {
                        let m = self.mem.mark_ptr(p);
                        self.cur_mark[FIRST_MARK_CODE] = m;
                        self.add_token_ref(m);
                    }
                    if self.cur_mark[BOT_MARK_CODE] != NULL {
                        let m = self.cur_mark[BOT_MARK_CODE];
                        self.delete_token_ref(m);
                    }
                    let m = self.mem.mark_ptr(p);
                    self.cur_mark[BOT_MARK_CODE] = m;
                    self.add_token_ref(m);
                }
            }
            prev_p = p;
            p = self.mem.link(prev_p);
        }
        let loc = self.eqtb.lay.glue_base + SPLIT_TOP_SKIP_CODE;
        self.eqtb.set_equiv(loc, save_split_top_skip);
        // §1017: break the page at p, put it in box 255.
        let ch = self.mem.contrib_head();
        if p != NULL {
            if self.mem.link(ch) == NULL {
                if self.nest.ptr == 0 {
                    self.nest.cur.tail = self.page_tail;
                } else {
                    self.nest.stack[0].tail = self.page_tail;
                }
            }
            let pt = self.page_tail;
            let lc = self.mem.link(ch);
            self.mem.set_link(pt, lc);
            self.mem.set_link(ch, p);
            self.mem.set_link(prev_p, NULL);
        }
        let save_vbadness = self.eqtb.int_par(VBADNESS_CODE);
        let loc_vb = self.eqtb.lay.int_base + VBADNESS_CODE;
        self.eqtb.set_int(loc_vb, crate::types::INF_BAD);
        let save_vfuzz = self.eqtb.dimen_par(VFUZZ_CODE);
        let loc_vf = self.eqtb.lay.dimen_base + VFUZZ_CODE;
        self.eqtb.set_int(loc_vf, MAX_DIMEN);
        let bs = self.best_size;
        let pmd = self.page_max_depth;
        let lph = self.mem.link(ph);
        let b255 = self.vpackage(lph, bs, EXACTLY, pmd)?;
        let loc = self.eqtb.lay.box_base + 255;
        self.eqtb.set_equiv(loc, b255);
        self.eqtb.set_int(loc_vb, save_vbadness);
        self.eqtb.set_int(loc_vf, save_vfuzz);
        if self.last_glue != MAX_HALFWORD {
            let lg = self.last_glue;
            self.mem.delete_glue_ref(lg);
        }
        // §991: start a new current page.
        self.start_new_page();
        if q != hh {
            let lh = self.mem.link(hh);
            self.mem.set_link(ph, lh);
            self.page_tail = q;
        }
        // §1019: delete the page-insertion nodes.
        let mut r = self.mem.link(pih);
        while r != pih {
            let nq = self.mem.link(r);
            self.mem.free_node(r, PAGE_INS_NODE_SIZE);
            r = nq;
        }
        self.mem.set_link(pih, pih);
        // marks (§1012 tail + etex.ch fire_up_done).
        if self.sa_root[crate::sa::MARK_VAL as usize] != NULL {
            let m = self.sa_root[crate::sa::MARK_VAL as usize];
            if self.do_marks(crate::sa::FIRE_UP_DONE, 0, m) {
                self.sa_root[crate::sa::MARK_VAL as usize] = NULL;
            }
        }
        if self.cur_mark[TOP_MARK_CODE] != NULL && self.cur_mark[FIRST_MARK_CODE] == NULL {
            self.cur_mark[FIRST_MARK_CODE] = self.cur_mark[TOP_MARK_CODE];
            let m = self.cur_mark[TOP_MARK_CODE];
            self.add_token_ref(m);
        }
        let or = self.eqtb.equiv(self.eqtb.lay.output_routine_loc);
        if or != NULL {
            if self.dead_cycles >= self.eqtb.int_par(MAX_DEAD_CYCLES_CODE) {
                // §1024.
                self.print_err("Output loop---");
                let dc = self.dead_cycles;
                self.print_int(dc);
                self.print_chars(" consecutive dead cycles");
                self.help(&[
                    "I've concluded that your \\output is awry; it never does a",
                    "\\shipout, so I'm shipping \\box255 out myself. Next time",
                    "increase \\maxdeadcycles if you want me to be more patient!",
                ]);
                self.error()?;
            } else {
                // §1025: fire up the user's output routine.
                self.output_active = true;
                self.dead_cycles += 1;
                self.push_nest()?;
                self.nest.cur.mode = -VMODE;
                self.set_prev_depth(IGNORE_DEPTH);
                self.nest.cur.ml = -self.inp.line;
                self.begin_token_list(or, crate::input::OUTPUT_TEXT)?;
                self.new_save_level(crate::eqtb::OUTPUT_GROUP)?;
                self.normal_paragraph()?;
                self.scan_left_brace()?;
                return Ok(());
            }
        }
        // §1023: perform the default output routine.
        if self.mem.link(ph) != NULL {
            if self.mem.link(ch) == NULL {
                if self.nest.ptr == 0 {
                    self.nest.cur.tail = self.page_tail;
                } else {
                    self.nest.stack[0].tail = self.page_tail;
                }
            } else {
                let pt = self.page_tail;
                let lc = self.mem.link(ch);
                self.mem.set_link(pt, lc);
            }
            let lh = self.mem.link(ph);
            self.mem.set_link(ch, lh);
            self.mem.set_link(ph, NULL);
            self.page_tail = ph;
        }
        // etex.ch: recycle the page discards before shipping out.
        let pd = self.disc_ptr[crate::control::LAST_BOX_CODE as usize];
        self.flush_node_list(pd);
        self.disc_ptr[crate::control::LAST_BOX_CODE as usize] = NULL;
        let b = self.eqtb.box_reg(255);
        let loc = self.eqtb.lay.box_base + 255;
        self.eqtb.set_equiv(loc, NULL);
        self.ship_out(b)?;
        Ok(())
    }

    /// §991: start a new current page.
    pub fn start_new_page(&mut self) {
        self.page_contents = EMPTY;
        let ph = self.mem.page_head();
        self.page_tail = ph;
        self.mem.set_link(ph, NULL);
        self.last_glue = MAX_HALFWORD;
        self.last_penalty = 0;
        self.last_kern = 0;
        self.last_node_type = -1;
        self.page_so_far[7] = 0; // page_depth
        self.page_max_depth = 0;
    }

    /// §1026: resume the page builder after `\output` has ended (called
    /// from `handle_right_brace` for `output_group`).
    pub fn resume_after_output(&mut self) -> TexResult<()> {
        if self.inp.cur.loc != NULL
            || (self.inp.cur.index != crate::input::OUTPUT_TEXT
                && self.inp.cur.index != crate::input::BACKED_UP)
        {
            // §1027: recover from an unbalanced output routine.
            self.print_err("Unbalanced output routine");
            self.help(&[
                "Your sneaky output routine has problematic {'s and/or }'s.",
                "I can't handle that very well; good luck.",
            ]);
            self.error()?;
            loop {
                self.get_token()?;
                if self.inp.cur.loc == NULL {
                    break;
                }
            }
        }
        self.end_token_list()?;
        self.end_graf()?;
        self.unsave()?;
        self.output_active = false;
        self.insert_penalties = 0;
        // §1028: ensure box 255 is empty.
        if self.eqtb.box_reg(255) != NULL {
            self.print_err("Output routine didn't use all of ");
            self.print_esc_str("box");
            self.print_int(255);
            self.help(&[
                "Your \\output commands should empty \\box255,",
                "e.g., by saying `\\shipout\\box255'.",
                "Proceed; I'll discard its present contents.",
            ]);
            self.box_error(255)?;
        }
        let ph = self.mem.page_head();
        let ch = self.mem.contrib_head();
        if self.nest.cur.tail != self.nest.cur.head {
            // current list goes after heldover insertions
            let pt = self.page_tail;
            let h = self.nest.cur.head;
            let lh = self.mem.link(h);
            self.mem.set_link(pt, lh);
            self.page_tail = self.nest.cur.tail;
        }
        if self.mem.link(ph) != NULL {
            // both go before heldover contributions
            if self.mem.link(ch) == NULL {
                self.nest.stack[0].tail = self.page_tail;
            }
            let pt = self.page_tail;
            let lc = self.mem.link(ch);
            self.mem.set_link(pt, lc);
            let lh = self.mem.link(ph);
            self.mem.set_link(ch, lh);
            self.mem.set_link(ph, NULL);
            self.page_tail = ph;
        }
        // etex.ch §1026: recycle the page discards.
        let pd = self.disc_ptr[crate::control::LAST_BOX_CODE as usize];
        self.flush_node_list(pd);
        self.disc_ptr[crate::control::LAST_BOX_CODE as usize] = NULL;
        self.pop_nest();
        self.build_page()
    }
}
