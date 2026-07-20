//! e-TeX sparse arrays (etex.ch): registers 256..32767 and mark classes.
//!
//! Seven doubly-linked trees (count, dimen, skip, muskip, box, toks, and
//! mark classes) of four index-node levels (16-way branching on the hex
//! digits of the register number) ending in element nodes. Elements carry
//! a reference count (`sa_ref`) and a grouping level (`sa_lev`); saved
//! values live on `sa_chain` heads recorded in `restore_sa` save-stack
//! entries.

use crate::engine::Engine;
use crate::error::TexResult;
use crate::scan::{DIMEN_VAL, GLUE_VAL, INT_VAL, MU_VAL, TOK_VAL};
use crate::types::{Pointer, NULL};

/// `box_val` (etex.ch): the additional box registers.
pub const BOX_VAL: u8 = 4;
/// `mark_val` (etex.ch): the additional mark classes.
pub const MARK_VAL: u8 = 6;

pub const DIMEN_VAL_LIMIT: u16 = 0x20;
pub const MU_VAL_LIMIT: u16 = 0x40;
pub const BOX_VAL_LIMIT: u16 = 0x50;
pub const TOK_VAL_LIMIT: u16 = 0x60;

pub const INDEX_NODE_SIZE: i32 = 9;
pub const POINTER_NODE_SIZE: i32 = 2;
pub const WORD_NODE_SIZE: i32 = 3;
pub const MARK_CLASS_NODE_SIZE: i32 = 4;

// do_marks action codes.
pub const VSPLIT_INIT: u8 = 0;
pub const FIRE_UP_INIT: u8 = 1;
pub const FIRE_UP_DONE: u8 = 2;
pub const DESTROY_MARKS: u8 = 3;

impl Engine {
    // sa_index == type (b0), sa_used/sa_lev == subtype (b1),
    // sa_ref == info(p+1), sa_ptr/sa_num == link(p+1),
    // sa_int/sa_dim == mem[p+2].int.

    pub fn sa_index(&self, p: Pointer) -> u16 {
        self.mem.node_type(p)
    }

    pub fn sa_type(&self, p: Pointer) -> u8 {
        (self.sa_index(p) / 16) as u8
    }

    pub fn sa_lev(&self, p: Pointer) -> u16 {
        self.mem.subtype(p)
    }

    pub fn sa_ref(&self, p: Pointer) -> i32 {
        self.mem.info(p + 1)
    }

    pub fn sa_ptr(&self, p: Pointer) -> Pointer {
        self.mem.link(p + 1)
    }

    pub fn sa_int(&self, p: Pointer) -> i32 {
        self.mem.word(p + 2).int()
    }

    fn set_sa_ptr(&mut self, p: Pointer, v: Pointer) {
        self.mem.set_link(p + 1, v);
    }

    fn set_sa_int(&mut self, p: Pointer, v: i32) {
        self.mem.word_mut(p + 2).set_int(v);
    }

    /// `add_sa_ref(p)` (etex.ch).
    pub fn add_sa_ref(&mut self, p: Pointer) {
        let r = self.sa_ref(p);
        self.mem.set_info(p + 1, r + 1);
    }

    /// Reads pointer `i` (0..15) of index node `q` (`get_sa_ptr`).
    fn get_sa_ptr(&self, q: Pointer, i: i32) -> Pointer {
        if i % 2 == 1 {
            self.mem.link(q + (i / 2) + 1)
        } else {
            self.mem.info(q + (i / 2) + 1)
        }
    }

    /// Stores pointer `i` of index node `q` (`put_sa_ptr`).
    fn put_sa_ptr(&mut self, q: Pointer, i: i32, v: Pointer) {
        if i % 2 == 1 {
            self.mem.set_link(q + (i / 2) + 1, v);
        } else {
            self.mem.set_info(q + (i / 2) + 1, v);
        }
    }

    /// `new_index(i, q)` (etex.ch): creates an index node in `cur_ptr`.
    fn new_index(&mut self, i: u16, q: Pointer) -> TexResult<()> {
        let p = self.mem.get_node(INDEX_NODE_SIZE)?;
        self.mem.set_node_type(p, i); // sa_index
        self.mem.set_subtype(p, 0); // sa_used
        self.mem.set_link(p, q);
        for k in 1..INDEX_NODE_SIZE {
            // two null pointers per word (sa_null)
            self.mem.set_info(p + k, NULL);
            self.mem.set_link(p + k, NULL);
        }
        self.cur_ptr = p;
        Ok(())
    }

    /// `find_sa_element(t, n, w)` (etex.ch): sets `cur_ptr` to the sparse
    /// array element for type `t`, number `n`, or NULL; `w` forces
    /// creation.
    pub fn find_sa_element(&mut self, t: u8, n: i32, w: bool) -> TexResult<()> {
        let digits = [n / 4096, (n / 256) % 16, (n / 16) % 16, n % 16];
        self.cur_ptr = self.sa_root[t as usize];
        let mut q;
        let mut level = 0; // how many levels exist already
        'found: {
            if self.cur_ptr == NULL {
                break 'found;
            }
            q = self.cur_ptr;
            for (k, &i) in digits.iter().enumerate() {
                self.cur_ptr = self.get_sa_ptr(q, i);
                if self.cur_ptr == NULL {
                    level = k + 1;
                    break 'found;
                }
                if k < 3 {
                    q = self.cur_ptr;
                }
            }
            return Ok(()); // found (or NULL leaf handled above)
        }
        // Some tree element is missing.
        if !w {
            self.cur_ptr = NULL;
            return Ok(());
        }
        // Create the missing nodes, top-down from `level`.
        if level == 0 {
            self.new_index(u16::from(t), NULL)?;
            self.sa_root[t as usize] = self.cur_ptr;
            level = 1;
        }
        q = if level == 1 {
            self.sa_root[t as usize]
        } else {
            // Re-walk to the deepest existing node.
            let mut q = self.sa_root[t as usize];
            for &i in digits.iter().take(level - 1) {
                q = self.get_sa_ptr(q, i);
            }
            q
        };
        for k in level..=3 {
            let i = digits[k - 1];
            self.new_index(digits[k - 1] as u16, q)?;
            let c = self.cur_ptr;
            self.put_sa_ptr(q, i, c);
            let u = self.mem.subtype(q);
            self.mem.set_subtype(q, u + 1); // incr(sa_used)
            q = self.cur_ptr;
        }
        // Create a new array element of type t with the low hex digit.
        let i = digits[3];
        let p = if t == MARK_VAL {
            let p = self.mem.get_node(MARK_CLASS_NODE_SIZE)?;
            for k in 1..MARK_CLASS_NODE_SIZE {
                self.mem.set_info(p + k, NULL);
                self.mem.set_link(p + k, NULL);
            }
            p
        } else {
            let p = if t <= DIMEN_VAL {
                let p = self.mem.get_node(WORD_NODE_SIZE)?;
                self.set_sa_int(p, 0);
                self.set_sa_ptr(p, n); // sa_num
                p
            } else {
                let p = self.mem.get_node(POINTER_NODE_SIZE)?;
                if t <= MU_VAL {
                    let zg = self.mem.zero_glue();
                    self.set_sa_ptr(p, zg);
                    self.mem.add_glue_ref(zg);
                } else {
                    self.set_sa_ptr(p, NULL);
                }
                p
            };
            self.mem.set_info(p + 1, NULL); // sa_ref
            p
        };
        self.mem.set_node_type(p, 16 * u16::from(t) + i as u16);
        self.mem.set_subtype(p, crate::eqtb::LEVEL_ONE); // sa_lev
        self.mem.set_link(p, q);
        self.put_sa_ptr(q, i, p);
        let u = self.mem.subtype(q);
        self.mem.set_subtype(q, u + 1);
        self.cur_ptr = p;
        Ok(())
    }

    /// `delete_sa_ref(q)` (etex.ch): drop one reference; release default-
    /// valued elements (and emptied index nodes) entirely.
    pub fn delete_sa_ref(&mut self, q: Pointer) {
        let r = self.sa_ref(q);
        self.mem.set_info(q + 1, r - 1);
        if self.sa_ref(q) != NULL {
            return;
        }
        let s;
        if self.sa_index(q) < DIMEN_VAL_LIMIT {
            if self.sa_int(q) == 0 {
                s = WORD_NODE_SIZE;
            } else {
                return;
            }
        } else {
            if self.sa_index(q) < MU_VAL_LIMIT {
                if self.sa_ptr(q) == self.mem.zero_glue() {
                    let zg = self.mem.zero_glue();
                    self.mem.delete_glue_ref(zg);
                } else {
                    return;
                }
            } else if self.sa_ptr(q) != NULL {
                return;
            }
            s = POINTER_NODE_SIZE;
        }
        let mut q = q;
        let mut s = s;
        loop {
            let i = i32::from(self.sa_index(q)) % 16;
            let p = q;
            q = self.mem.link(p);
            self.mem.free_node(p, s);
            if q == NULL {
                // the whole tree has been freed
                self.sa_root[i as usize] = NULL;
                return;
            }
            self.put_sa_ptr(q, i, NULL);
            let u = self.mem.subtype(q);
            self.mem.set_subtype(q, u - 1); // decr(sa_used)
            s = INDEX_NODE_SIZE;
            if self.mem.subtype(q) > 0 {
                break;
            }
        }
    }

    /// `print_sa_num(q)` (etex.ch): the register number of an element.
    pub fn print_sa_num(&mut self, q: Pointer) {
        let n;
        if self.sa_index(q) < DIMEN_VAL_LIMIT {
            n = self.sa_ptr(q); // sa_num
        } else {
            let mut m = i32::from(self.sa_index(q)) % 16;
            let mut q = self.mem.link(q);
            m += 16 * (i32::from(self.sa_index(q)) % 16);
            q = self.mem.link(q);
            m += 256
                * (i32::from(self.sa_index(q)) % 16
                    + 16 * (i32::from(self.sa_index(self.mem.link(q))) % 16));
            n = m;
        }
        self.print_int(n);
    }

    /// `show_sa(p, s)` (etex.ch): trace display of an array element.
    pub fn show_sa(&mut self, p: Pointer, s: &str) {
        self.begin_diagnostic();
        self.print_char('{' as i32);
        self.print_chars(s);
        self.print_char(' ' as i32);
        if p == NULL {
            self.print_char('?' as i32);
        } else {
            let t = self.sa_type(p);
            if t < BOX_VAL {
                self.print_cmd_chr(crate::cmds::REGISTER, p);
            } else if t == BOX_VAL {
                self.print_esc_str("box");
                self.print_sa_num(p);
            } else if t == TOK_VAL {
                self.print_cmd_chr(crate::cmds::TOKS_REGISTER, p);
            } else {
                self.print_char('?' as i32);
            }
            self.print_char('=' as i32);
            if t == INT_VAL {
                let v = self.sa_int(p);
                self.print_int(v);
            } else if t == DIMEN_VAL {
                let v = self.sa_int(p);
                self.print_scaled(v);
                self.print_chars("pt");
            } else {
                let p = self.sa_ptr(p);
                if t == GLUE_VAL {
                    self.print_spec(p, "pt");
                } else if t == MU_VAL {
                    self.print_spec(p, "mu");
                } else if t == BOX_VAL {
                    if p == NULL {
                        self.print_chars("void");
                    } else {
                        self.depth_threshold = 0;
                        self.breadth_max = 1;
                        self.show_node_list(p);
                    }
                } else if t == TOK_VAL {
                    if p != NULL {
                        let l = self.mem.link(p);
                        self.show_token_list(l, NULL, 32);
                    }
                } else {
                    self.print_char('?' as i32);
                }
            }
        }
        self.print_char('}' as i32);
        self.end_diagnostic(false);
    }

    /// `sa_save(p)` (etex.ch): saves the value of element `p`.
    fn sa_save(&mut self, p: Pointer) -> TexResult<()> {
        if self.save.cur_level != self.sa_level {
            self.sa_check_full_save_stack()?;
            let sp = self.save.save_ptr;
            self.save.set_save_type(sp, crate::eqtb::RESTORE_SA);
            let l = self.sa_level;
            self.save.set_save_level(sp, l);
            let c = self.sa_chain;
            self.save.set_save_index(sp, c);
            self.save.save_ptr += 1;
            self.sa_chain = NULL;
            self.sa_level = self.save.cur_level;
        }
        let mut i = self.sa_index(p);
        let q;
        if i < DIMEN_VAL_LIMIT {
            if self.sa_int(p) == 0 {
                q = self.mem.get_node(POINTER_NODE_SIZE)?;
                i = TOK_VAL_LIMIT;
            } else {
                q = self.mem.get_node(WORD_NODE_SIZE)?;
                let v = self.sa_int(p);
                self.set_sa_int(q, v);
            }
            self.set_sa_ptr(q, NULL);
        } else {
            q = self.mem.get_node(POINTER_NODE_SIZE)?;
            let v = self.sa_ptr(p);
            self.set_sa_ptr(q, v);
        }
        self.mem.set_info(q + 1, p); // sa_loc
        self.mem.set_node_type(q, i);
        let lv = self.sa_lev(p);
        self.mem.set_subtype(q, lv);
        let c = self.sa_chain;
        self.mem.set_link(q, c);
        self.sa_chain = q;
        self.add_sa_ref(p);
        Ok(())
    }

    fn sa_check_full_save_stack(&mut self) -> TexResult<()> {
        if self.save.save_ptr > self.save.max_save_stack {
            self.save.max_save_stack = self.save.save_ptr;
            if self.save.max_save_stack > self.save.save_size - 7 {
                return Err(crate::error::TexInterrupt::Overflow {
                    what: "save size",
                    size: self.save.save_size as i32,
                });
            }
        }
        Ok(())
    }

    /// `sa_destroy(p)` (etex.ch): destroy the value of element `p`.
    fn sa_destroy(&mut self, p: Pointer) {
        if self.sa_index(p) < MU_VAL_LIMIT {
            let v = self.sa_ptr(p);
            self.mem.delete_glue_ref(v);
        } else if self.sa_ptr(p) != NULL {
            if self.sa_index(p) < BOX_VAL_LIMIT {
                let v = self.sa_ptr(p);
                self.flush_node_list(v);
            } else {
                let v = self.sa_ptr(p);
                self.delete_token_ref(v);
            }
        }
    }

    /// `sa_def(p, e)` (etex.ch): new pointer value for element `p`.
    pub fn sa_def(&mut self, p: Pointer, e: Pointer) -> TexResult<()> {
        let tracing = self.eqtb.int_par(crate::eqtb::TRACING_ASSIGNS_CODE) > 0;
        self.add_sa_ref(p);
        if self.sa_ptr(p) == e {
            if tracing {
                self.show_sa(p, "reassigning");
            }
            self.sa_destroy(p);
        } else {
            if tracing {
                self.show_sa(p, "changing");
            }
            if self.sa_lev(p) == self.save.cur_level {
                self.sa_destroy(p);
            } else {
                self.sa_save(p)?;
            }
            let l = self.save.cur_level;
            self.mem.set_subtype(p, l);
            self.set_sa_ptr(p, e);
            if tracing {
                self.show_sa(p, "into");
            }
        }
        self.delete_sa_ref(p);
        Ok(())
    }

    /// `sa_w_def(p, w)` (etex.ch): new word value for element `p`.
    pub fn sa_w_def(&mut self, p: Pointer, w: i32) -> TexResult<()> {
        let tracing = self.eqtb.int_par(crate::eqtb::TRACING_ASSIGNS_CODE) > 0;
        self.add_sa_ref(p);
        if self.sa_int(p) == w {
            if tracing {
                self.show_sa(p, "reassigning");
            }
        } else {
            if tracing {
                self.show_sa(p, "changing");
            }
            if self.sa_lev(p) != self.save.cur_level {
                self.sa_save(p)?;
            }
            let l = self.save.cur_level;
            self.mem.set_subtype(p, l);
            self.set_sa_int(p, w);
            if tracing {
                self.show_sa(p, "into");
            }
        }
        self.delete_sa_ref(p);
        Ok(())
    }

    /// `gsa_def(p, e)` (etex.ch): global `sa_def`.
    pub fn gsa_def(&mut self, p: Pointer, e: Pointer) -> TexResult<()> {
        let tracing = self.eqtb.int_par(crate::eqtb::TRACING_ASSIGNS_CODE) > 0;
        self.add_sa_ref(p);
        if tracing {
            self.show_sa(p, "globally changing");
        }
        self.sa_destroy(p);
        self.mem.set_subtype(p, crate::eqtb::LEVEL_ONE);
        self.set_sa_ptr(p, e);
        if tracing {
            self.show_sa(p, "into");
        }
        self.delete_sa_ref(p);
        Ok(())
    }

    /// `gsa_w_def(p, w)` (etex.ch): global `sa_w_def`.
    pub fn gsa_w_def(&mut self, p: Pointer, w: i32) -> TexResult<()> {
        let tracing = self.eqtb.int_par(crate::eqtb::TRACING_ASSIGNS_CODE) > 0;
        self.add_sa_ref(p);
        if tracing {
            self.show_sa(p, "globally changing");
        }
        self.mem.set_subtype(p, crate::eqtb::LEVEL_ONE);
        self.set_sa_int(p, w);
        if tracing {
            self.show_sa(p, "into");
        }
        self.delete_sa_ref(p);
        Ok(())
    }

    /// `sa_restore` (etex.ch): restores the entries on `sa_chain`.
    pub fn sa_restore(&mut self) {
        let tracing = self.eqtb.int_par(crate::eqtb::TRACING_RESTORES_CODE) > 0;
        loop {
            let chain = self.sa_chain;
            let p = self.mem.info(chain + 1); // sa_loc
            if self.sa_lev(p) == crate::eqtb::LEVEL_ONE {
                if self.sa_index(p) >= DIMEN_VAL_LIMIT {
                    self.sa_destroy(chain);
                }
                if tracing {
                    self.show_sa(p, "retaining");
                }
            } else {
                if self.sa_index(p) < DIMEN_VAL_LIMIT {
                    if self.sa_index(chain) < DIMEN_VAL_LIMIT {
                        let v = self.sa_int(chain);
                        self.set_sa_int(p, v);
                    } else {
                        self.set_sa_int(p, 0);
                    }
                } else {
                    self.sa_destroy(p);
                    let v = self.sa_ptr(chain);
                    self.set_sa_ptr(p, v);
                }
                let l = self.sa_lev(chain);
                self.mem.set_subtype(p, l);
                if tracing {
                    self.show_sa(p, "restoring");
                }
            }
            self.delete_sa_ref(p);
            self.sa_chain = self.mem.link(chain);
            if self.sa_index(chain) < DIMEN_VAL_LIMIT {
                self.mem.free_node(chain, WORD_NODE_SIZE);
            } else {
                self.mem.free_node(chain, POINTER_NODE_SIZE);
            }
            if self.sa_chain == NULL {
                break;
            }
        }
    }

    /// `fetch_box` (etex.ch): `box(cur_val)` for any register number.
    pub fn fetch_box(&mut self, n: i32) -> TexResult<Pointer> {
        if n < 256 {
            Ok(self.eqtb.box_reg(n))
        } else {
            self.find_sa_element(BOX_VAL, n, false)?;
            if self.cur_ptr == NULL {
                Ok(NULL)
            } else {
                Ok(self.sa_ptr(self.cur_ptr))
            }
        }
    }

    /// `change_box` / `set_sa_box` (etex.ch): replace `box(cur_val)`
    /// without touching the grouping level.
    pub fn change_box(&mut self, n: i32, b: Pointer) -> TexResult<()> {
        if n < 256 {
            let loc = self.eqtb.lay.box_base + n;
            self.eqtb.set_equiv(loc, b);
        } else {
            self.find_sa_element(BOX_VAL, n, false)?;
            if self.cur_ptr != NULL {
                let p = self.cur_ptr;
                self.set_sa_ptr(p, b);
                self.add_sa_ref(p);
                self.delete_sa_ref(p);
            }
        }
        Ok(())
    }

    /// `do_marks(a, l, q)` (etex.ch): walks the mark-class tree performing
    /// action `a`; returns true when the (sub)tree was deleted.
    pub fn do_marks(&mut self, a: u8, l: u8, q: Pointer) -> bool {
        if l < 4 {
            // q is an index node
            for i in 0..16 {
                let p = self.get_sa_ptr(q, i);
                if p != NULL && self.do_marks(a, l + 1, p) {
                    self.put_sa_ptr(q, i, NULL);
                    let u = self.mem.subtype(q);
                    self.mem.set_subtype(q, u - 1);
                }
            }
            if self.mem.subtype(q) == 0 {
                self.mem.free_node(q, INDEX_NODE_SIZE);
                return true;
            }
            false
        } else {
            // q is the node for a mark class:
            // sa_top_mark=info(q+1), sa_first_mark=link(q+1),
            // sa_bot_mark=info(q+2), sa_split_first_mark=link(q+2),
            // sa_split_bot_mark=info(q+3).
            match a {
                VSPLIT_INIT => {
                    if self.mem.link(q + 2) != NULL {
                        let m = self.mem.link(q + 2);
                        self.delete_token_ref(m);
                        self.mem.set_link(q + 2, NULL);
                        let m = self.mem.info(q + 3);
                        self.delete_token_ref(m);
                        self.mem.set_info(q + 3, NULL);
                    }
                }
                FIRE_UP_INIT => {
                    if self.mem.info(q + 2) != NULL {
                        if self.mem.info(q + 1) != NULL {
                            let m = self.mem.info(q + 1);
                            self.delete_token_ref(m);
                        }
                        let m = self.mem.link(q + 1);
                        self.delete_token_ref(m);
                        self.mem.set_link(q + 1, NULL);
                        let bot = self.mem.info(q + 2);
                        if self.mem.link(bot) == NULL {
                            // an empty token list
                            self.delete_token_ref(bot);
                            self.mem.set_info(q + 2, NULL);
                        } else {
                            self.add_token_ref(bot);
                        }
                        let bot = self.mem.info(q + 2);
                        self.mem.set_info(q + 1, bot);
                    }
                }
                FIRE_UP_DONE => {
                    if self.mem.info(q + 1) != NULL && self.mem.link(q + 1) == NULL {
                        let m = self.mem.info(q + 1);
                        self.mem.set_link(q + 1, m);
                        self.add_token_ref(m);
                    }
                }
                _ => {
                    // destroy_marks
                    for i in 0..=4 {
                        let p = self.get_sa_ptr(q, i);
                        if p != NULL {
                            self.delete_token_ref(p);
                            self.put_sa_ptr(q, i, NULL);
                        }
                    }
                }
            }
            if self.mem.info(q + 2) == NULL && self.mem.info(q + 3) == NULL {
                self.mem.free_node(q, MARK_CLASS_NODE_SIZE);
                return true;
            }
            false
        }
    }
}
