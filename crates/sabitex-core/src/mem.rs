//! Dynamic memory allocation: the `mem` array.
//!
//! Ports tex.web Part 9 (§115-§131) and the static layout / initialization
//! of Part 11 (§162-§164). `mem` is a flat array of [`MemoryWord`]; pointers
//! are `i32` indices. Locations `<= lo_mem_max` hold variable-size nodes
//! (TAOCP ex. 2.5-19 style ring of empties); locations `>= hi_mem_min` hold
//! one-word nodes on a conventional AVAIL stack (§116).
//!
//! Key pointer order (§116):
//! `null <= mem_min <= mem_bot < lo_mem_max < hi_mem_min < mem_top <= mem_end <= mem_max`.
//!
//! This port keeps `mem_min = mem_bot` (a single configurable knob; the
//! default is 0 as recommended by tex.web §12, while the TRIP test requires
//! `mem_min = mem_bot = 1`). Production memory extension (`mem_max >
//! mem_top`) can be revisited later.

use crate::error::{TexInterrupt, TexResult};
use crate::memword::MemoryWord;
use crate::types::{Halfword, Pointer, MAX_HALFWORD, NULL};

/// `empty_flag == max_halfword`: the `link` of an empty variable-size node
/// (tex.web §124).
pub const EMPTY_FLAG: Halfword = MAX_HALFWORD;

/// `glue_spec_size = 4`: number of words in a glue specification
/// (tex.web §150; needed here for the static node layout of §162).
pub const GLUE_SPEC_SIZE: i32 = 4;

// §135-§137: the node types needed so far (box nodes land in M2).
pub const HLIST_NODE: u16 = 0;
pub const VLIST_NODE: u16 = 1;

// §150: glue orders of infinity.
pub const NORMAL: u16 = 0;
pub const FIL: u16 = 1;
pub const FILL: u16 = 2;
pub const FILLL: u16 = 3;

/// `lo_mem_stat_max - mem_bot` (tex.web §162): the five static glue specs
/// occupy `mem_bot..=mem_bot + LO_MEM_STAT_SPAN`.
pub const LO_MEM_STAT_SPAN: Pointer = 5 * GLUE_SPEC_SIZE - 1;

/// `hi_mem_stat_usage = 14` (tex.web §162): the number of one-word nodes
/// always present (`backup_head` = `mem_top - 13` .. `page_ins_head` = `mem_top`).
pub const HI_MEM_STAT_USAGE: i32 = 14;

/// Offset of `hi_mem_stat_min` below `mem_top` (tex.web §162).
pub const HI_MEM_STAT_MIN_OFFSET: i32 = 13;

/// The big dynamic storage area and its allocation state
/// (tex.web §116, §118-§120, §124-§125).
pub struct Mem {
    words: Vec<MemoryWord>,
    /// Whether glue_set values keep full f64 precision. tex.web declares
    /// glue_ratio as `real`; Knuth-era binaries (and the TRIP reference)
    /// behave like f32, while TeX Live e-TeX uses double (the etrip
    /// reference). Default false = f32 rounding.
    pub glue_ratio_wide: bool,
    /// `mem_bot`: smallest dumpable location (fixed at 0 in this port).
    pub mem_bot: Pointer,
    /// `mem_top`: largest dumpable location.
    pub mem_top: Pointer,
    /// `mem_max`: greatest index in the `mem` array.
    pub mem_max: Pointer,
    /// `lo_mem_max`: the largest location of variable-size memory in use.
    pub lo_mem_max: Pointer,
    /// `hi_mem_min`: the smallest location of one-word memory in use.
    pub hi_mem_min: Pointer,
    /// `avail`: head of the list of available one-word nodes.
    pub avail: Pointer,
    /// `mem_end`: the last one-word node used in `mem`.
    pub mem_end: Pointer,
    /// `rover`: points to some node in the ring of empty variable-size nodes.
    pub rover: Pointer,
    /// `var_used`: statistics about variable-size memory in use (§117).
    pub var_used: i32,
    /// `dyn_used`: statistics about one-word memory in use (§117).
    pub dyn_used: i32,
}

impl Mem {
    /// Builds and initializes `mem` the slow way (tex.web §163-§164,
    /// INITEX). `mem_top == mem_max` in this port (no production extension
    /// yet). The "special list heads and constant nodes" of §790 etc. are
    /// initialized in later milestones, when those nodes exist.
    pub fn new(mem_top: Pointer, mem_bot: Pointer) -> Mem {
        // §164 lays out rover (1000 words) below hi_mem_stat_min; tex.web's
        // consistency check (§14) effectively requires mem_top >= 1100 — we
        // need room for the statics, the initial 1000-word node and slack.
        assert!(
            mem_top > mem_bot + LO_MEM_STAT_SPAN + 1 + 1000 + 1 + HI_MEM_STAT_MIN_OFFSET,
            "mem_top too small: {mem_top}"
        );
        assert!(
            mem_top < MAX_HALFWORD,
            "mem_max must be < max_halfword (tex.web §111)"
        );
        assert!((0..=1).contains(&mem_bot), "mem_bot must be 0 or 1");
        let mut mem = Mem {
            words: vec![MemoryWord::ZERO; mem_top as usize + 1],
            glue_ratio_wide: false,
            mem_bot,
            mem_top,
            mem_max: mem_top,
            lo_mem_max: 0,
            hi_mem_min: 0,
            avail: NULL,
            mem_end: 0,
            rover: 0,
            var_used: 0,
            dyn_used: 0,
        };
        // §164: glue-spec words mem_bot..lo_mem_stat_max are zeroed (already
        // true); set the first words of the glue specifications.
        let mut k = mem.mem_bot;
        while k <= mem.lo_mem_stat_max() {
            mem.set_glue_ref_count(k, NULL + 1);
            mem.set_stretch_order(k, NORMAL);
            mem.set_shrink_order(k, NORMAL);
            k += GLUE_SPEC_SIZE;
        }
        let (fil, fill, ss, fil_neg) = (
            mem.fil_glue(),
            mem.fill_glue(),
            mem.ss_glue(),
            mem.fil_neg_glue(),
        );
        mem.set_stretch(fil, crate::types::UNITY);
        mem.set_stretch_order(fil, FIL);
        mem.set_stretch(fill, crate::types::UNITY);
        mem.set_stretch_order(fill, FILL);
        mem.set_stretch(ss, crate::types::UNITY);
        mem.set_stretch_order(ss, FIL);
        mem.set_shrink(ss, crate::types::UNITY);
        mem.set_shrink_order(ss, FIL);
        mem.set_stretch(fil_neg, -crate::types::UNITY);
        mem.set_stretch_order(fil_neg, FIL);
        mem.rover = mem.lo_mem_stat_max() + 1;
        mem.set_link(mem.rover, EMPTY_FLAG); // now initialize the dynamic memory
        mem.set_node_size(mem.rover, 1000); // which is a 1000-word available node
        mem.set_llink(mem.rover, mem.rover);
        mem.set_rlink(mem.rover, mem.rover);
        mem.lo_mem_max = mem.rover + 1000;
        mem.set_link(mem.lo_mem_max, NULL);
        mem.set_info(mem.lo_mem_max, NULL);
        let cleared = mem.word(mem.lo_mem_max);
        for k in (mem_top - HI_MEM_STAT_MIN_OFFSET)..=mem_top {
            *mem.word_mut(k) = cleared; // clear list heads
        }
        // §819: the active list ends at `last_active == active`, a
        // permanently allocated node (two words: mem_top-7, mem_top-6).
        mem.set_node_type(mem_top - 7, crate::linebreak::HYPHENATED);
        mem.set_llink(mem_top - 7, MAX_HALFWORD); // line_number
        mem.set_subtype(mem_top - 7, 0);
        // §981: page_ins_head is its own successor, with the maximum
        // insertion number, so searches always terminate.
        mem.set_subtype(mem_top, 255);
        mem.set_node_type(mem_top, crate::page::SPLIT_UP);
        mem.set_link(mem_top, mem_top);
        // §988: the current page starts with glue, conceptually.
        mem.set_node_type(mem_top - 2, crate::nodes::GLUE_NODE);
        mem.set_subtype(mem_top - 2, NORMAL);
        // §797: end_span has the largest possible link (span count) field.
        // (info(omit_template) is set by Engine::new once the frozen
        // \endtemplate location is known.)
        mem.set_link(mem_top - 9, 0x10000); // max_quarterword + 1
        mem.set_info(mem_top - 9, NULL);
        mem.avail = NULL;
        mem.mem_end = mem_top;
        mem.hi_mem_min = mem_top - HI_MEM_STAT_MIN_OFFSET;
        mem.var_used = LO_MEM_STAT_SPAN + 1; // initialize statistics
        mem.dyn_used = HI_MEM_STAT_USAGE;
        mem
    }

    // §162: the statically allocated glue specifications, from mem_bot up.

    /// `zero_glue == mem_bot`.
    pub fn zero_glue(&self) -> Pointer {
        self.mem_bot
    }

    /// `fil_glue == zero_glue + glue_spec_size`.
    pub fn fil_glue(&self) -> Pointer {
        self.mem_bot + GLUE_SPEC_SIZE
    }

    /// `fill_glue == fil_glue + glue_spec_size`.
    pub fn fill_glue(&self) -> Pointer {
        self.mem_bot + 2 * GLUE_SPEC_SIZE
    }

    /// `ss_glue == fill_glue + glue_spec_size`.
    pub fn ss_glue(&self) -> Pointer {
        self.mem_bot + 3 * GLUE_SPEC_SIZE
    }

    /// `fil_neg_glue == ss_glue + glue_spec_size`.
    pub fn fil_neg_glue(&self) -> Pointer {
        self.mem_bot + 4 * GLUE_SPEC_SIZE
    }

    /// `lo_mem_stat_max` (§162): largest statically allocated word in the
    /// variable-size `mem`.
    pub fn lo_mem_stat_max(&self) -> Pointer {
        self.mem_bot + LO_MEM_STAT_SPAN
    }

    /// §1311-§1312: dump the dynamic memory.
    pub fn dump(&self, w: &mut crate::fmt::FmtWriter) {
        w.i32(self.mem_bot);
        w.i32(self.mem_top);
        w.i32(self.lo_mem_max);
        w.i32(self.hi_mem_min);
        w.i32(self.avail);
        w.i32(self.mem_end);
        w.i32(self.rover);
        w.i32(self.var_used);
        w.i32(self.dyn_used);
        // §1311: only the live regions travel.
        w.words(&self.words[..=self.lo_mem_max as usize]);
        w.words(&self.words[self.hi_mem_min as usize..=self.mem_end as usize]);
    }

    /// §1312: undump the dynamic memory.
    pub fn undump(
        &mut self,
        r: &mut crate::fmt::FmtReader,
        pristine: bool,
    ) -> crate::fmt::FmtResult<()> {
        if r.i32()? != self.mem_bot {
            return Err("mem_bot mismatch");
        }
        if r.i32()? != self.mem_top {
            return Err("mem_top mismatch");
        }
        self.lo_mem_max = r.i32()?;
        self.hi_mem_min = r.i32()?;
        self.avail = r.i32()?;
        self.mem_end = r.i32()?;
        self.rover = r.i32()?;
        self.var_used = r.i32()?;
        self.dyn_used = r.i32()?;
        // §1312: the two live regions (the gap stays zeroed).
        let lo = r.words()?;
        if lo.len() != self.lo_mem_max as usize + 1 {
            return Err("lo mem size mismatch");
        }
        let hi = r.words()?;
        if hi.len() != (self.mem_end - self.hi_mem_min + 1) as usize {
            return Err("hi mem size mismatch");
        }
        if self.mem_end as usize >= self.words.len() {
            return Err("mem_end exceeds arena");
        }
        if !pristine {
            self.words.fill(crate::memword::MemoryWord::default());
        }
        self.words[..lo.len()].copy_from_slice(&lo);
        self.words[self.hi_mem_min as usize..=self.mem_end as usize].copy_from_slice(&hi);
        Ok(())
    }

    /// `mem[p]`.
    pub fn word(&self, p: Pointer) -> MemoryWord {
        self.words[p as usize]
    }

    /// `mem[p]` for writing.
    pub fn word_mut(&mut self, p: Pointer) -> &mut MemoryWord {
        &mut self.words[p as usize]
    }

    /// `link(p) == mem[p].hh.rh` (tex.web §118).
    pub fn link(&self, p: Pointer) -> Pointer {
        self.word(p).rh()
    }

    /// Sets `link(p)`.
    pub fn set_link(&mut self, p: Pointer, v: Pointer) {
        self.word_mut(p).set_rh(v);
    }

    /// `info(p) == mem[p].hh.lh` (tex.web §118).
    pub fn info(&self, p: Pointer) -> Pointer {
        self.word(p).lh()
    }

    /// Sets `info(p)`.
    pub fn set_info(&mut self, p: Pointer, v: Pointer) {
        self.word_mut(p).set_lh(v);
    }

    /// `is_empty(p)`: tests for an empty variable-size node (tex.web §124).
    pub fn is_empty(&self, p: Pointer) -> bool {
        self.link(p) == EMPTY_FLAG
    }

    /// `node_size(p) == info(p)`: the size field in empty variable-size
    /// nodes (tex.web §124).
    pub fn node_size(&self, p: Pointer) -> i32 {
        self.info(p)
    }

    /// Sets `node_size(p)`.
    pub fn set_node_size(&mut self, p: Pointer, v: i32) {
        self.set_info(p, v);
    }

    /// `llink(p) == info(p+1)`: left link in the ring of empties (§124).
    pub fn llink(&self, p: Pointer) -> Pointer {
        self.info(p + 1)
    }

    /// Sets `llink(p)`.
    pub fn set_llink(&mut self, p: Pointer, v: Pointer) {
        self.set_info(p + 1, v);
    }

    /// `rlink(p) == link(p+1)`: right link in the ring of empties (§124).
    pub fn rlink(&self, p: Pointer) -> Pointer {
        self.link(p + 1)
    }

    /// Sets `rlink(p)`.
    pub fn set_rlink(&mut self, p: Pointer, v: Pointer) {
        self.set_link(p + 1, v);
    }

    // §150: glue specification fields.

    /// `glue_ref_count(p) == link(p)`: reference count of a glue spec.
    pub fn glue_ref_count(&self, p: Pointer) -> Pointer {
        self.link(p)
    }

    pub fn set_glue_ref_count(&mut self, p: Pointer, v: Pointer) {
        self.set_link(p, v);
    }

    /// `width(p) == mem[p+1].sc` (for glue specs; box nodes share this).
    pub fn width(&self, p: Pointer) -> i32 {
        self.word(p + 1).sc()
    }

    pub fn set_width(&mut self, p: Pointer, v: i32) {
        self.word_mut(p + 1).set_sc(v);
    }

    /// `stretch(p) == mem[p+2].sc`.
    pub fn stretch(&self, p: Pointer) -> i32 {
        self.word(p + 2).sc()
    }

    pub fn set_stretch(&mut self, p: Pointer, v: i32) {
        self.word_mut(p + 2).set_sc(v);
    }

    /// `shrink(p) == mem[p+3].sc`.
    pub fn shrink(&self, p: Pointer) -> i32 {
        self.word(p + 3).sc()
    }

    pub fn set_shrink(&mut self, p: Pointer, v: i32) {
        self.word_mut(p + 3).set_sc(v);
    }

    /// `stretch_order(p) == type(p)` (hh.b0).
    pub fn stretch_order(&self, p: Pointer) -> u16 {
        self.word(p).b0()
    }

    pub fn set_stretch_order(&mut self, p: Pointer, v: u16) {
        self.word_mut(p).set_b0(v);
    }

    /// `shrink_order(p) == subtype(p)` (hh.b1).
    pub fn shrink_order(&self, p: Pointer) -> u16 {
        self.word(p).b1()
    }

    pub fn set_shrink_order(&mut self, p: Pointer, v: u16) {
        self.word_mut(p).set_b1(v);
    }

    /// `delete_glue_ref(p)` (§203): decrements a glue spec's reference
    /// count, freeing the spec when the count was `null` (= one reference).
    pub fn delete_glue_ref(&mut self, p: Pointer) {
        if self.glue_ref_count(p) == NULL {
            self.free_node(p, GLUE_SPEC_SIZE);
        } else {
            let c = self.glue_ref_count(p);
            self.set_glue_ref_count(p, c - 1);
        }
    }

    /// `add_glue_ref(p)` (§203).
    pub fn add_glue_ref(&mut self, p: Pointer) {
        let c = self.glue_ref_count(p);
        self.set_glue_ref_count(p, c + 1);
    }

    // §162: the permanent one-word list heads near mem_top.

    /// `temp_head == mem_top - 3`: head of a temporary list.
    pub fn temp_head(&self) -> Pointer {
        self.mem_top - 3
    }

    /// `hold_head == mem_top - 4`: head of another temporary list.
    pub fn hold_head(&self) -> Pointer {
        self.mem_top - 4
    }

    /// `backup_head == mem_top - 13`: head of `scan_keyword`'s token list.
    pub fn backup_head(&self) -> Pointer {
        self.mem_top - 13
    }

    /// `adjust_head == mem_top - 5`: head of the adjustment list (§162).
    pub fn adjust_head(&self) -> Pointer {
        self.mem_top - 5
    }

    /// `page_ins_head == mem_top`: list of insertion data for `build_page`.
    pub fn page_ins_head(&self) -> Pointer {
        self.mem_top
    }

    /// `contrib_head == mem_top - 1`: vlist of items not yet on the page.
    pub fn contrib_head(&self) -> Pointer {
        self.mem_top - 1
    }

    /// `page_head == mem_top - 2`: vlist for the current page.
    pub fn page_head(&self) -> Pointer {
        self.mem_top - 2
    }

    /// `active == mem_top - 7`: head of the active list in `line_break`
    /// (a two-word node occupying `mem_top-7` and `mem_top-6`).
    pub fn active(&self) -> Pointer {
        self.mem_top - 7
    }

    /// `lig_trick == garbage == mem_top - 12` (§162).
    pub fn lig_trick(&self) -> Pointer {
        self.mem_top - 12
    }

    /// `get_avail` (tex.web §120): single-word node allocation. The new
    /// node's `link` is null.
    pub fn get_avail(&mut self) -> TexResult<Pointer> {
        let mut p = self.avail; // get top location in the avail stack
        if p != NULL {
            self.avail = self.link(self.avail); // and pop it off
        } else if self.mem_end < self.mem_max {
            self.mem_end += 1; // or go into virgin territory
            p = self.mem_end;
        } else {
            self.hi_mem_min -= 1;
            p = self.hi_mem_min;
            if self.hi_mem_min <= self.lo_mem_max {
                // TODO(M1): runaway() — display possible runaway text.
                return Err(TexInterrupt::Overflow {
                    what: "main memory size",
                    size: self.mem_max + 1,
                });
            }
        }
        self.set_link(p, NULL); // provide an oft-desired initialization
        self.dyn_used += 1; // maintain statistics
        Ok(p)
    }

    /// `free_avail(p)` (tex.web §121): single-word node liberation.
    pub fn free_avail(&mut self, p: Pointer) {
        self.set_link(p, self.avail);
        self.avail = p;
        self.dyn_used -= 1;
    }

    /// `flush_list(p)` (tex.web §123): frees an entire linked list of
    /// one-word nodes starting at `p`.
    pub fn flush_list(&mut self, p: Pointer) {
        if p != NULL {
            let mut q;
            let mut r = p;
            loop {
                q = r;
                r = self.link(r);
                self.dyn_used -= 1;
                if r == NULL {
                    break;
                }
            }
            // now q is the last node on the list
            self.set_link(q, self.avail);
            self.avail = p;
        }
    }

    /// `get_node(s)` (tex.web §125): variable-size node allocation. Returns
    /// a node of size `s >= 2` whose first word has a null `link`. Calling
    /// with `s = 2^30` merely merges adjacent free areas and returns
    /// `max_halfword`.
    pub fn get_node(&mut self, s: i32) -> TexResult<Pointer> {
        'restart: loop {
            let mut p = self.rover; // start at some free node in the ring
            loop {
                // §127: Try to allocate within node p and its physical successors.
                let mut q = p + self.node_size(p); // the physical successor
                while self.is_empty(q) {
                    // merge node p with node q
                    let t = self.rlink(q);
                    if q == self.rover {
                        self.rover = t;
                    }
                    let lq = self.llink(q);
                    self.set_llink(t, lq);
                    self.set_rlink(lq, t);
                    q += self.node_size(q);
                }
                let r = q - s;
                if r > p + 1 {
                    // §128: allocate from the top of node p.
                    self.set_node_size(p, r - p); // store the remaining size
                    self.rover = p; // start searching here next time
                    return Ok(self.found(r, s));
                }
                if r == p && self.rlink(p) != p {
                    // §129: allocate entire node p, deleting it from the ring.
                    self.rover = self.rlink(p);
                    let t = self.llink(p);
                    self.set_llink(self.rover, t);
                    self.set_rlink(t, self.rover);
                    return Ok(self.found(r, s));
                }
                self.set_node_size(p, q - p); // reset the size in case it grew
                p = self.rlink(p); // move to the next node in the ring
                if p == self.rover {
                    break; // repeat until the whole list has been traversed
                }
            }
            if s == 0o10000000000 {
                return Ok(MAX_HALFWORD);
            }
            if self.lo_mem_max + 2 < self.hi_mem_min
                && self.lo_mem_max + 2 <= self.mem_bot + MAX_HALFWORD
            {
                // §126: grow more variable-size memory (1000 words at a time).
                let mut t = if self.hi_mem_min - self.lo_mem_max >= 1998 {
                    self.lo_mem_max + 1000
                } else {
                    self.lo_mem_max + 1 + (self.hi_mem_min - self.lo_mem_max) / 2
                };
                let p = self.llink(self.rover);
                let q = self.lo_mem_max;
                self.set_rlink(p, q);
                self.set_llink(self.rover, q);
                if t > self.mem_bot + MAX_HALFWORD {
                    t = self.mem_bot + MAX_HALFWORD;
                }
                self.set_rlink(q, self.rover);
                self.set_llink(q, p);
                self.set_link(q, EMPTY_FLAG);
                self.set_node_size(q, t - self.lo_mem_max);
                self.lo_mem_max = t;
                self.set_link(self.lo_mem_max, NULL);
                self.set_info(self.lo_mem_max, NULL);
                self.rover = q;
                continue 'restart;
            }
            return Err(TexInterrupt::Overflow {
                what: "main memory size",
                size: self.mem_max + 1,
            });
        }
    }

    /// `found:` epilogue of `get_node` (§125).
    fn found(&mut self, r: Pointer, s: i32) -> Pointer {
        self.set_link(r, NULL); // this node is now nonempty
        self.var_used += s; // maintain usage statistics
        r
    }

    /// `free_node(p, s)` (tex.web §130): variable-size node liberation —
    /// inserts `p` as a new empty node just before where `rover` points.
    pub fn free_node(&mut self, p: Pointer, s: Halfword) {
        self.set_node_size(p, s);
        self.set_link(p, EMPTY_FLAG);
        let q = self.llink(self.rover);
        self.set_llink(p, q);
        self.set_rlink(p, self.rover); // set both links
        self.set_llink(self.rover, p);
        self.set_rlink(q, p); // insert p into the ring
        self.var_used -= s; // maintain statistics
    }

    /// `sort_avail` (tex.web §131, INITEX): sorts the ring of empty
    /// variable-size nodes by location, smallest first, just before dumping.
    pub fn sort_avail(&mut self) -> TexResult<()> {
        let _ = self.get_node(0o10000000000)?; // merge adjacent free areas
        let mut p = self.rlink(self.rover);
        self.set_rlink(self.rover, MAX_HALFWORD);
        let old_rover = self.rover;
        while p != old_rover {
            // §132: sort p into the list starting at rover.
            if p < self.rover {
                let q = p;
                p = self.rlink(q);
                self.set_rlink(q, self.rover);
                self.rover = q;
            } else {
                let mut q = self.rover;
                while self.rlink(q) < p {
                    q = self.rlink(q);
                }
                let r = self.rlink(p);
                let rq = self.rlink(q);
                self.set_rlink(p, rq);
                self.set_rlink(q, p);
                p = r;
            }
        }
        // Re-establish the llinks and close the ring.
        let mut p = self.rover;
        while self.rlink(p) != MAX_HALFWORD {
            let rp = self.rlink(p);
            self.set_llink(rp, p);
            p = rp;
        }
        self.set_rlink(p, self.rover);
        self.set_llink(self.rover, p);
        Ok(())
    }
}
