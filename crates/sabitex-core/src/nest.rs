//! The semantic nest.
//!
//! Ports tex.web Part 16 (§211-§219): the stack of suspended lists and
//! modes. The current level is cached in `cur` (tex.web's `cur_list`).

use crate::engine::Engine;
use crate::error::{TexInterrupt, TexResult};
use crate::memword::MemoryWord;
use crate::types::{Pointer, Scaled};

/// `ignore_depth` (§212): `prev_depth` value that is ignored.
pub const IGNORE_DEPTH: Scaled = -65_536_000;

/// `list_state_record` (§212).
#[derive(Copy, Clone)]
pub struct ListStateRecord {
    /// `mode_field`: ±vmode/hmode/mmode, or 0 inside `\write`.
    pub mode: i32,
    /// `head_field` / `tail_field`.
    pub head: Pointer,
    pub tail: Pointer,
    /// `pg_field`: `prev_graf`.
    pub pg: i32,
    /// `ml_field`: `mode_line`.
    pub ml: i32,
    /// `aux_field`: `prev_depth` / `space_factor`+`clang` /
    /// `incompleat_noad`.
    pub aux: MemoryWord,
    /// `eTeX_aux_field` (etex.ch): `LR_save` in vertical modes, `LR_box`
    /// in display math, `delim_ptr` in math mode.
    pub etex_aux: Pointer,
    /// pTeX `disp_called_field`: a disp_node is already present in the
    /// current list.
    pub disp_called: bool,
}

/// The nest (§213).
pub struct Nest {
    pub stack: Vec<ListStateRecord>,
    pub ptr: usize,
    pub max_nest_stack: usize,
    /// `cur_list`: the "top" semantic state.
    pub cur: ListStateRecord,
    /// `shown_mode` (§213).
    pub shown_mode: i32,
    pub nest_size: usize,
}

impl Nest {
    /// §215: the outermost level — vertical mode, building the
    /// contribution list.
    pub fn new(nest_size: usize, contrib_head: Pointer) -> Nest {
        let mut aux = MemoryWord::ZERO;
        aux.set_sc(IGNORE_DEPTH);
        Nest {
            stack: vec![
                ListStateRecord {
                    mode: 0,
                    head: 0,
                    tail: 0,
                    pg: 0,
                    ml: 0,
                    aux: MemoryWord::ZERO,
                    etex_aux: crate::types::NULL,
                    disp_called: false,
                };
                nest_size + 1
            ],
            ptr: 0,
            max_nest_stack: 0,
            cur: ListStateRecord {
                mode: crate::engine::VMODE,
                head: contrib_head,
                tail: contrib_head,
                pg: 0,
                ml: 0,
                aux,
                etex_aux: crate::types::NULL,
                disp_called: false,
            },
            shown_mode: 0,
            nest_size,
        }
    }
}

impl Engine {
    /// `mode` (§213).
    pub fn mode(&self) -> i32 {
        self.nest.cur.mode
    }

    /// `prev_depth == aux.sc` (§213).
    pub fn prev_depth(&self) -> Scaled {
        self.nest.cur.aux.sc()
    }

    pub fn set_prev_depth(&mut self, v: Scaled) {
        self.nest.cur.aux.set_sc(v);
    }

    /// `space_factor == aux.hh.lh` (§213).
    pub fn space_factor(&self) -> i32 {
        self.nest.cur.aux.lh()
    }

    pub fn set_space_factor(&mut self, v: i32) {
        self.nest.cur.aux.set_lh(v);
    }

    /// `clang == aux.hh.rh` (§213).
    pub fn clang(&self) -> i32 {
        self.nest.cur.aux.rh()
    }

    pub fn set_clang(&mut self, v: i32) {
        self.nest.cur.aux.set_rh(v);
    }

    /// `tail_append(p)` (§214).
    pub fn tail_append(&mut self, p: Pointer) {
        let t = self.nest.cur.tail;
        self.mem.set_link(t, p);
        self.nest.cur.tail = self.mem.link(t);
    }

    /// `push_nest` (§216): enter a new semantic level.
    pub fn push_nest(&mut self) -> TexResult<()> {
        if self.nest.ptr > self.nest.max_nest_stack {
            self.nest.max_nest_stack = self.nest.ptr;
            if self.nest.ptr == self.nest.nest_size {
                return Err(TexInterrupt::Overflow {
                    what: "semantic nest size",
                    size: self.nest.nest_size as i32,
                });
            }
        }
        self.nest.stack[self.nest.ptr] = self.nest.cur;
        self.nest.ptr += 1;
        let h = self.mem.get_avail()?;
        self.nest.cur.head = h;
        self.nest.cur.tail = h;
        self.nest.cur.pg = 0;
        self.nest.cur.ml = self.inp.line;
        self.nest.cur.etex_aux = crate::types::NULL; // etex.ch §216
        self.nest.cur.disp_called = false; // ptex-base.ch
        Ok(())
    }

    /// `pop_nest` (§217): leave a semantic level.
    pub fn pop_nest(&mut self) {
        let h = self.nest.cur.head;
        self.mem.free_avail(h);
        self.nest.ptr -= 1;
        self.nest.cur = self.nest.stack[self.nest.ptr];
    }

    /// `print_mode(m)` (§211).
    pub fn print_mode(&mut self, m: i32) {
        let unit = i32::from(crate::cmds::MAX_COMMAND) + 1;
        if m > 0 {
            match m / unit {
                0 => self.print_chars("vertical"),
                1 => self.print_chars("horizontal"),
                _ => self.print_chars("display math"),
            }
        } else if m == 0 {
            self.print_chars("no");
        } else {
            match (-m) / unit {
                0 => self.print_chars("internal vertical"),
                1 => self.print_chars("restricted horizontal"),
                _ => self.print_chars("math"),
            }
        }
        self.print_chars(" mode");
    }

    /// `show_activities` (§218-§219, with the §986 page status).
    pub fn show_activities(&mut self) {
        self.nest.stack[self.nest.ptr] = self.nest.cur;
        self.print_nl_chars("");
        self.print_ln();
        for p in (0..=self.nest.ptr).rev() {
            let rec = self.nest.stack[p];
            self.print_nl_chars("### ");
            self.print_mode(rec.mode);
            self.print_chars(" entered at line ");
            self.print_int(rec.ml.abs());
            if rec.mode == crate::engine::HMODE && rec.pg != 0o40600000 {
                self.print_chars(" (language");
                self.print_int(rec.pg % 0o200000);
                self.print_chars(":hyphenmin");
                self.print_int(rec.pg / 0o20000000);
                self.print_char(',' as i32);
                self.print_int((rec.pg / 0o200000) % 0o100);
                self.print_char(')' as i32);
            }
            if rec.ml < 0 {
                self.print_chars(" (\\output routine)");
            }
            if p == 0 && self.mem.page_head() != self.page_tail {
                // §986: show the status of the current page.
                self.print_nl_chars("### current page:");
                if self.output_active {
                    self.print_chars(" (held over for next output)");
                }
                let ph = self.mem.page_head();
                let l = self.mem.link(ph);
                self.show_box(l);
                if self.page_contents > crate::page::EMPTY {
                    self.print_nl_chars("total height ");
                    self.print_totals();
                    self.print_nl_chars(" goal height ");
                    let g = self.page_so_far[0];
                    self.print_scaled(g);
                    // §987: the list of insertions on the page.
                    let pih = self.mem.page_ins_head();
                    let mut r = self.mem.link(pih);
                    while r != pih {
                        self.print_ln();
                        self.print_esc_str("insert");
                        let t = i32::from(self.mem.subtype(r));
                        self.print_int(t);
                        self.print_chars(" adds ");
                        let amount = if self.eqtb.count(t) == 1000 {
                            self.mem.height(r)
                        } else {
                            crate::arith::x_over_n(&mut self.arith, self.mem.height(r), 1000)
                                * self.eqtb.count(t)
                        };
                        self.print_scaled(amount);
                        if self.mem.node_type(r) == crate::page::SPLIT_UP {
                            let mut q = self.mem.page_head();
                            let mut t = 0;
                            loop {
                                q = self.mem.link(q);
                                if self.mem.node_type(q) == crate::nodes::INS_NODE
                                    && self.mem.subtype(q) == self.mem.subtype(r)
                                {
                                    t += 1;
                                }
                                if q == self.mem.info(r + 1) {
                                    break; // broken_ins(r)
                                }
                            }
                            self.print_chars(", #");
                            self.print_int(t);
                            self.print_chars(" might split");
                        }
                        r = self.mem.link(r);
                    }
                }
            }
            if p == 0 {
                let ch = self.mem.contrib_head();
                if self.mem.link(ch) != crate::types::NULL {
                    self.print_nl_chars("### recent contributions:");
                }
            }
            let l = self.mem.link(rec.head);
            self.show_box(l);
            // §219: show the auxiliary field.
            let unit = i32::from(crate::cmds::MAX_COMMAND) + 1;
            match rec.mode.abs() / unit {
                0 => {
                    self.print_nl_chars("prevdepth ");
                    if rec.aux.sc() <= IGNORE_DEPTH {
                        self.print_chars("ignored");
                    } else {
                        let d = rec.aux.sc();
                        self.print_scaled(d);
                    }
                    if rec.pg != 0 {
                        self.print_chars(", prevgraf ");
                        self.print_int(rec.pg);
                        self.print_chars(" line");
                        if rec.pg != 1 {
                            self.print_char('s' as i32);
                        }
                    }
                }
                1 => {
                    self.print_nl_chars("spacefactor ");
                    let sf = rec.aux.lh();
                    self.print_int(sf);
                    if rec.mode > 0 && rec.aux.rh() > 0 {
                        self.print_chars(", current language ");
                        let cl = rec.aux.rh();
                        self.print_int(cl);
                    }
                }
                _ => {
                    // §219: incompleat_noad (math mode).
                    if rec.aux.int() != crate::types::NULL {
                        self.print_chars("this will begin denominator of:");
                        let b = rec.aux.int();
                        self.show_box(b);
                    }
                }
            }
        }
    }
}
