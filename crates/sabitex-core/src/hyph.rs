//! Hyphenation.
//!
//! Ports tex.web Parts 40-43 (§891-§966): the pre-hyphenation scan,
//! `reconstitute`, `hyphenate`, the exception dictionary
//! (`\hyphenation`), and the pattern trie (`\patterns`, `init_trie`).
//!
//! The trie arrays are sized by `Sizes` (`trie_size`, `trie_op_size`,
//! `hyph_size`); overflow aborts exactly as in tex.web so that patterns
//! behave identically across installations (§944 rationale).

use crate::engine::Engine;
use crate::error::{TexInterrupt, TexResult};
use crate::fonts::{FontMem, KERN_FLAG, LIG_TAG, NON_ADDRESS, NON_CHAR, STOP_FLAG};
use crate::memword::MemoryWord;
use crate::nodes::*;
use crate::types::{Pointer, StrNumber, NULL};

/// All hyphenation state (§892, §900-§903, §920-§926, §943-§952).
pub struct Hyph {
    /// `hc[0..65]`: the word, lowercased (256 = impossible).
    pub hc: [i32; 66],
    /// `hn`: number of positions used in `hc`.
    pub hn: i32,
    /// `ha`, `hb`: replace nodes `ha..hb` with the hyphenated result.
    pub ha: Pointer,
    pub hb: Pointer,
    /// `hf`: the font of the word.
    pub hf: i32,
    /// `hu[0..63]`: like `hc` before lowercasing.
    pub hu: [i32; 64],
    /// `hyf[0..64]`: odd values mark discretionary hyphen positions.
    pub hyf: [u8; 65],
    pub hyf_char: i32,
    pub cur_lang: i32,
    pub init_cur_lang: i32,
    pub l_hyf: i32,
    pub r_hyf: i32,
    pub init_l_hyf: i32,
    pub init_r_hyf: i32,
    pub hyf_bchar: i32,
    /// `hyphen_passed` (§902).
    pub hyphen_passed: i32,
    /// `init_list`/`init_lig`/`init_lft` (§900).
    pub init_list: Pointer,
    pub init_lig: bool,
    pub init_lft: bool,
    // §921: the packed trie.
    pub trie: Vec<MemoryWord>, // rh = trie_link, b1 = trie_char, b0 = trie_op
    pub hyf_distance: Vec<u8>,
    pub hyf_num: Vec<u8>,
    pub hyf_next: Vec<u16>,
    pub op_start: [i32; 256],
    // §926: the exception dictionary.
    pub hyph_word: Vec<StrNumber>,
    pub hyph_list: Vec<Pointer>,
    pub hyph_count: i32,
    // §943: INITEX pattern-building tables.
    pub trie_op_hash: Vec<i32>, // indexed by h + trie_op_size
    pub trie_used: [u16; 256],
    pub trie_op_lang: Vec<i32>,
    pub trie_op_val: Vec<u16>,
    pub trie_op_ptr: i32,
    pub trie_c: Vec<i32>,
    pub trie_o: Vec<u16>,
    pub trie_l: Vec<i32>,
    pub trie_r: Vec<i32>,
    pub trie_ptr: i32,
    pub trie_hash: Vec<i32>,
    pub trie_taken: Vec<bool>,
    pub trie_min: [i32; 256],
    pub trie_max: i32,
    pub trie_not_ready: bool,
    /// `hyph_start` (etex.ch): root of the packed trie for hyph_codes.
    pub hyph_start: i32,
    /// `hyph_index` (etex.ch): packed hyph codes for `cur_lang`, or 0.
    pub hyph_index: i32,
    pub trie_size: i32,
    pub trie_op_size: i32,
    pub hyph_size: i32,
}

impl Hyph {
    pub fn new(trie_size: i32, trie_op_size: i32, hyph_size: i32) -> Hyph {
        Hyph {
            hc: [0; 66],
            hn: 0,
            ha: NULL,
            hb: NULL,
            hf: 0,
            hu: [0; 64],
            hyf: [0; 65],
            hyf_char: 0,
            cur_lang: 0,
            init_cur_lang: 0,
            l_hyf: 0,
            r_hyf: 0,
            init_l_hyf: 0,
            init_r_hyf: 0,
            hyf_bchar: NON_CHAR,
            hyphen_passed: 0,
            init_list: NULL,
            init_lig: false,
            init_lft: false,
            trie: vec![MemoryWord::ZERO; trie_size as usize + 1],
            hyf_distance: vec![0; trie_op_size as usize + 1],
            hyf_num: vec![0; trie_op_size as usize + 1],
            hyf_next: vec![0; trie_op_size as usize + 1],
            op_start: [0; 256],
            hyph_word: vec![0; hyph_size as usize + 1],
            hyph_list: vec![NULL; hyph_size as usize + 1],
            hyph_count: 0,
            trie_op_hash: vec![0; 2 * trie_op_size as usize + 1],
            trie_used: [0; 256],
            trie_op_lang: vec![0; trie_op_size as usize + 1],
            trie_op_val: vec![0; trie_op_size as usize + 1],
            trie_op_ptr: 0,
            trie_c: vec![0; trie_size as usize + 1],
            trie_o: vec![0; trie_size as usize + 1],
            trie_l: vec![0; trie_size as usize + 1],
            trie_r: vec![0; trie_size as usize + 1],
            trie_ptr: 0,
            trie_hash: vec![0; trie_size as usize + 1],
            trie_taken: vec![false; trie_size as usize + 1],
            trie_min: [0; 256],
            hyph_start: 0,
            hyph_index: 0,
            trie_max: 0,
            trie_not_ready: true,
            trie_size,
            trie_op_size,
            hyph_size,
        }
    }

    /// §1324-§1325: dump the hyphenation tables.
    pub fn dump(&self, w: &mut crate::fmt::FmtWriter) {
        w.i32(self.hyph_count);
        w.i32s(&self.hyph_word);
        w.i32s(&self.hyph_list);
        w.i32(self.trie_max);
        w.i32(self.hyph_start); // etex.ch §1324
        w.words(&self.trie[..=self.trie_max as usize]);
        w.i32(self.trie_op_ptr);
        w.u8s(&self.hyf_distance);
        w.u8s(&self.hyf_num);
        w.u16s(&self.hyf_next);
        w.i32s(&self.op_start);
    }

    /// §1325-§1327: undump the hyphenation tables.
    pub fn undump(
        &mut self,
        r: &mut crate::fmt::FmtReader,
        pristine: bool,
    ) -> crate::fmt::FmtResult<()> {
        self.hyph_count = r.i32()?;
        self.hyph_word = r.i32s()?;
        self.hyph_list = r.i32s()?;
        if self.hyph_word.len() != self.hyph_size as usize + 1 {
            return Err("hyph_size mismatch");
        }
        self.trie_max = r.i32()?;
        self.hyph_start = r.i32()?; // etex.ch §1325
        let trie = r.words()?;
        if trie.len() != self.trie_max as usize + 1 || trie.len() > self.trie.len() {
            return Err("trie size mismatch");
        }
        if !pristine {
            self.trie.fill(crate::memword::MemoryWord::default());
        }
        self.trie[..trie.len()].copy_from_slice(&trie);
        self.trie_op_ptr = r.i32()?;
        self.hyf_distance = r.u8s()?;
        self.hyf_num = r.u8s()?;
        self.hyf_next = r.u16s()?;
        let op_start = r.i32s()?;
        if op_start.len() != 256 {
            return Err("op_start size mismatch");
        }
        self.op_start.copy_from_slice(&op_start);
        self.trie_not_ready = false; // §1327: undumped formats are ready
        Ok(())
    }

    /// `trie_link(p)` / `trie_char(p)` / `trie_op(p)` (§921).
    pub(crate) fn trie_link(&self, p: i32) -> i32 {
        self.trie[p as usize].rh()
    }

    fn set_trie_link(&mut self, p: i32, v: i32) {
        self.trie[p as usize].set_rh(v);
    }

    pub(crate) fn trie_char(&self, p: i32) -> i32 {
        i32::from(self.trie[p as usize].b1())
    }

    pub(crate) fn trie_op(&self, p: i32) -> u16 {
        self.trie[p as usize].b0()
    }

    /// `trie_back(p) == trie[p].lh` (§950).
    fn trie_back(&self, p: i32) -> i32 {
        self.trie[p as usize].lh()
    }

    fn set_trie_back(&mut self, p: i32, v: i32) {
        self.trie[p as usize].set_lh(v);
    }
}

/// `set_cur_lang` (§934).
pub fn norm_lang(language: i32) -> i32 {
    if !(1..=255).contains(&language) {
        0
    } else {
        language
    }
}

/// `norm_min` (§1091).
pub fn norm_min(h: i32) -> i32 {
    if h <= 0 {
        1
    } else if h >= 63 {
        63
    } else {
        h
    }
}

impl Engine {
    /// `reconstitute(j, n, bchar, hchar)` (§906-§910): translates
    /// `hu[j..=n]` into nodes at `link(hold_head)`, returning the cut index.
    pub fn reconstitute(&mut self, j: i32, n: i32, bchar: i32, hchar: i32) -> TexResult<i32> {
        let mut j = j;
        let mut bchar = bchar;
        let mut hchar = hchar;
        self.hy.hyphen_passed = 0;
        let mut t = self.mem.hold_head();
        let mut w: crate::types::Scaled = 0;
        let hh = self.mem.hold_head();
        self.mem.set_link(hh, NULL);
        let hf = self.hy.hf;
        // §908: set up data structures with the cursor following j.
        self.cur_l = self.hy.hu[j as usize];
        self.cur_q = t;
        if j == 0 {
            self.ligature_present = self.hy.init_lig;
            let mut p = self.hy.init_list;
            if self.ligature_present {
                self.lft_hit = self.hy.init_lft;
            }
            while p > NULL {
                let c = self.mem.character(p);
                let q = self.mem.get_avail()?;
                self.mem.set_link(t, q);
                t = q;
                self.mem.set_font(t, hf as u16);
                self.mem.set_character(t, c);
                p = self.mem.link(p);
            }
        } else if self.cur_l < NON_CHAR {
            let q = self.mem.get_avail()?;
            self.mem.set_link(t, q);
            t = q;
            self.mem.set_font(t, hf as u16);
            self.mem.set_character(t, self.cur_l as u16);
        }
        self.lig_stack = NULL;
        // set_cur_r (§908).
        macro_rules! set_cur_r {
            ($s:ident, $cur_rh:ident) => {
                if j < n {
                    $s.cur_r = $s.hy.hu[(j + 1) as usize];
                } else {
                    $s.cur_r = bchar;
                }
                if $s.hy.hyf[j as usize] % 2 == 1 {
                    $cur_rh = hchar;
                } else {
                    $cur_rh = NON_CHAR;
                }
            };
        }
        let mut cur_rh: i32;
        {
            let s = &mut *self;
            set_cur_r!(s, cur_rh);
        }
        'continue_: loop {
            // §909: if there's a ligature or kern at the cursor, update.
            let mut k: i32;
            let mut q: MemoryWord;
            let mut done = false;
            if self.cur_l == NON_CHAR {
                k = self.fonts.bchar_label[hf as usize];
                if k == NON_ADDRESS {
                    done = true;
                    q = MemoryWord::ZERO;
                } else {
                    q = self.fonts.info[k as usize];
                }
            } else {
                q = self.fonts.char_info(hf, self.cur_l);
                if FontMem::char_tag(q) != LIG_TAG {
                    done = true;
                    k = 0;
                } else {
                    k = self.fonts.lig_kern_start(hf, q);
                    q = self.fonts.info[k as usize];
                    if FontMem::skip_byte(q) > STOP_FLAG {
                        k = self.fonts.lig_kern_restart(hf, q);
                        q = self.fonts.info[k as usize];
                    }
                }
            }
            if !done {
                let test_char = if cur_rh < NON_CHAR {
                    cur_rh
                } else {
                    self.cur_r
                };
                loop {
                    if i32::from(FontMem::next_char(q)) == test_char
                        && FontMem::skip_byte(q) <= STOP_FLAG
                    {
                        if cur_rh < NON_CHAR {
                            self.hy.hyphen_passed = j;
                            hchar = NON_CHAR;
                            cur_rh = NON_CHAR;
                            continue 'continue_;
                        }
                        if hchar < NON_CHAR && self.hy.hyf[j as usize] % 2 == 1 {
                            self.hy.hyphen_passed = j;
                            hchar = NON_CHAR;
                        }
                        if FontMem::op_byte(q) < KERN_FLAG {
                            // §910: carry out a ligature replacement.
                            if self.cur_l == NON_CHAR {
                                self.lft_hit = true;
                            }
                            if j == n && self.lig_stack == NULL {
                                self.rt_hit = true;
                            }
                            let op = FontMem::op_byte(q);
                            let rem = i32::from(FontMem::rem_byte(q));
                            let mut goto_done = false;
                            match op {
                                1 | 5 => {
                                    self.cur_l = rem;
                                    self.ligature_present = true;
                                }
                                2 | 6 => {
                                    self.cur_r = rem;
                                    if self.lig_stack > NULL {
                                        let ls = self.lig_stack;
                                        self.mem.set_character(ls, self.cur_r as u16);
                                    } else {
                                        self.lig_stack = self.new_lig_item(self.cur_r as u16)?;
                                        if j == n {
                                            bchar = NON_CHAR;
                                        } else {
                                            let p = self.mem.get_avail()?;
                                            let ls = self.lig_stack;
                                            self.mem.set_lig_ptr(ls, p);
                                            let c = self.hy.hu[(j + 1) as usize] as u16;
                                            self.mem.set_character(p, c);
                                            self.mem.set_font(p, hf as u16);
                                        }
                                    }
                                }
                                3 => {
                                    self.cur_r = rem;
                                    let p = self.lig_stack;
                                    self.lig_stack = self.new_lig_item(self.cur_r as u16)?;
                                    let ls = self.lig_stack;
                                    self.mem.set_link(ls, p);
                                }
                                7 | 11 => {
                                    self.wrap_lig_hyph(false, &mut t)?;
                                    self.cur_q = t;
                                    self.cur_l = rem;
                                    self.ligature_present = true;
                                }
                                _ => {
                                    // =:
                                    self.cur_l = rem;
                                    self.ligature_present = true;
                                    if self.lig_stack > NULL {
                                        self.pop_lig_stack_hyph(&mut j, &mut t)?;
                                        if self.lig_stack == NULL {
                                            let s = &mut *self;
                                            set_cur_r!(s, cur_rh);
                                        } else {
                                            self.cur_r =
                                                i32::from(self.mem.character(self.lig_stack));
                                            cur_rh = NON_CHAR;
                                        }
                                    } else if j == n {
                                        goto_done = true;
                                    } else {
                                        let q2 = self.mem.get_avail()?;
                                        self.mem.set_link(t, q2);
                                        t = q2;
                                        self.mem.set_font(t, hf as u16);
                                        self.mem.set_character(t, self.cur_r as u16);
                                        j += 1;
                                        let s = &mut *self;
                                        set_cur_r!(s, cur_rh);
                                    }
                                }
                            }
                            if !goto_done && FontMem::op_byte(q) > 4 && FontMem::op_byte(q) != 7 {
                                goto_done = true;
                            }
                            if goto_done {
                                break; // goto done
                            }
                            continue 'continue_;
                        }
                        w = self.fonts.char_kern(hf, q);
                        break; // goto done (kern inserted below)
                    }
                    if FontMem::skip_byte(q) >= STOP_FLAG {
                        if cur_rh == NON_CHAR {
                            break; // done
                        }
                        cur_rh = NON_CHAR;
                        continue 'continue_;
                    }
                    k += i32::from(FontMem::skip_byte(q)) + 1;
                    q = self.fonts.info[k as usize];
                }
            }
            // done (§911): append a ligature and/or kern to the translation.
            let rt = self.rt_hit;
            self.wrap_lig_hyph(rt, &mut t)?;
            if w != 0 {
                let kn = self.new_kern(w)?;
                self.mem.set_link(t, kn);
                t = kn;
                w = 0;
            }
            if self.lig_stack > NULL {
                self.cur_q = t;
                self.cur_l = i32::from(self.mem.character(self.lig_stack));
                self.ligature_present = true;
                self.pop_lig_stack_hyph(&mut j, &mut t)?;
                if self.lig_stack == NULL {
                    let s = &mut *self;
                    set_cur_r!(s, cur_rh);
                } else {
                    self.cur_r = i32::from(self.mem.character(self.lig_stack));
                    cur_rh = NON_CHAR;
                }
                continue 'continue_;
            }
            return Ok(j);
        }
    }

    /// `wrap_lig` (§910) for the hyphenation pass, threading `t`.
    fn wrap_lig_hyph(&mut self, rt: bool, t: &mut Pointer) -> TexResult<()> {
        if self.ligature_present {
            let lk = self.mem.link(self.cur_q);
            let p = self.new_ligature(self.hy.hf as u16, self.cur_l as u16, lk)?;
            if self.lft_hit {
                self.mem.set_subtype(p, 2);
                self.lft_hit = false;
            }
            if rt && self.lig_stack == NULL {
                let s = self.mem.subtype(p);
                self.mem.set_subtype(p, s + 1);
                self.rt_hit = false;
            }
            let q = self.cur_q;
            self.mem.set_link(q, p);
            *t = p;
            self.ligature_present = false;
        }
        Ok(())
    }

    /// `pop_lig_stack` (§910).
    fn pop_lig_stack_hyph(&mut self, j: &mut i32, t: &mut Pointer) -> TexResult<()> {
        if self.mem.lig_ptr(self.lig_stack) > NULL {
            let lp = self.mem.lig_ptr(self.lig_stack);
            self.mem.set_link(*t, lp); // charnode for hu[j+1]
            *t = self.mem.link(*t);
            *j += 1;
        }
        let p = self.lig_stack;
        self.lig_stack = self.mem.link(p);
        self.mem.free_node(p, SMALL_NODE_SIZE);
        Ok(())
    }

    /// `hyphenate` (§895-§918): the word is in `hc[1..=hn]`.
    pub fn hyphenate(&mut self, cur_p: Pointer) -> TexResult<()> {
        // §923: find hyphen locations, or return.
        for j in 0..=self.hy.hn {
            self.hy.hyf[j as usize] = 0;
        }
        // §929-§931: look in the exception table.
        let mut found_exception = false;
        'exceptions: {
            let mut h = self.hy.hc[1];
            self.hy.hn += 1;
            let hn = self.hy.hn;
            self.hy.hc[hn as usize] = self.hy.cur_lang;
            for j in 2..=hn {
                h = (h + h + self.hy.hc[j as usize]) % self.hy.hyph_size;
            }
            loop {
                // §930.
                let k = self.hy.hyph_word[h as usize];
                if k == 0 {
                    break; // not_found
                }
                if (self.strings.length(k) as i32) < hn {
                    break; // not_found
                }
                if self.strings.length(k) as i32 == hn {
                    let mut j = 1;
                    let mut u = 0usize;
                    let mut mismatch = false;
                    loop {
                        let pc = i32::from(self.strings.str(k)[u]);
                        if pc < self.hy.hc[j as usize] {
                            // hyph_word[h] < hc → not_found
                            self.hy.hn -= 1;
                            break 'exceptions;
                        }
                        if pc > self.hy.hc[j as usize] {
                            mismatch = true;
                            break;
                        }
                        j += 1;
                        u += 1;
                        if j > hn {
                            break;
                        }
                    }
                    if !mismatch {
                        // §932: insert hyphens as specified.
                        let mut s = self.hy.hyph_list[h as usize];
                        while s != NULL {
                            let i = self.mem.info(s);
                            self.hy.hyf[i as usize] = 1;
                            s = self.mem.link(s);
                        }
                        self.hy.hn -= 1;
                        found_exception = true;
                        break 'exceptions;
                    }
                }
                if h > 0 {
                    h -= 1;
                } else {
                    h = self.hy.hyph_size;
                }
            }
            self.hy.hn -= 1; // not_found
        }
        if !found_exception {
            let cl = self.hy.cur_lang;
            if self.hy.trie_char(cl + 1) != cl {
                return Ok(()); // no patterns for cur_lang
            }
            self.hy.hc[0] = 0;
            let hn = self.hy.hn;
            self.hy.hc[(hn + 1) as usize] = 0;
            self.hy.hc[(hn + 2) as usize] = 256; // insert delimiters
            for j in 0..=(hn - self.hy.r_hyf + 1) {
                let mut z = self.hy.trie_link(cl + 1) + self.hy.hc[j as usize];
                let mut l = j;
                while self.hy.hc[l as usize] == self.hy.trie_char(z) {
                    if self.hy.trie_op(z) != 0 {
                        // §924: store maximum values in hyf.
                        let mut v = i32::from(self.hy.trie_op(z));
                        loop {
                            v += self.hy.op_start[self.hy.cur_lang as usize];
                            let i = l - i32::from(self.hy.hyf_distance[v as usize]);
                            if self.hy.hyf_num[v as usize] > self.hy.hyf[i as usize] {
                                self.hy.hyf[i as usize] = self.hy.hyf_num[v as usize];
                            }
                            v = i32::from(self.hy.hyf_next[v as usize]);
                            if v == 0 {
                                break;
                            }
                        }
                    }
                    l += 1;
                    z = self.hy.trie_link(z) + self.hy.hc[l as usize];
                }
            }
        }
        // found:
        for j in 0..self.hy.l_hyf {
            self.hy.hyf[j as usize] = 0;
        }
        for j in 0..self.hy.r_hyf {
            self.hy.hyf[(self.hy.hn - j) as usize] = 0;
        }
        // §902: if no hyphens were found, return.
        let mut any = false;
        for j in self.hy.l_hyf..=(self.hy.hn - self.hy.r_hyf) {
            if self.hy.hyf[j as usize] % 2 == 1 {
                any = true;
                break;
            }
        }
        if !any {
            return Ok(());
        }
        // §903: replace nodes ha..hb by the hyphenated sequence.
        let ha = self.hy.ha;
        let hb = self.hy.hb;
        let hf = self.hy.hf;
        let q = self.mem.link(hb);
        self.mem.set_link(hb, NULL);
        let r = self.mem.link(ha);
        self.mem.set_link(ha, NULL);
        let bchar = self.hy.hyf_bchar;
        let mut j: i32;
        let mut s: Pointer;
        'common_ending: {
            'found2: {
                if self.mem.is_char_node(ha) {
                    if i32::from(self.mem.font(ha)) != hf {
                        break 'found2;
                    }
                    self.hy.init_list = ha;
                    self.hy.init_lig = false;
                    self.hy.hu[0] = i32::from(self.mem.character(ha));
                } else if self.mem.node_type(ha) == LIGATURE_NODE {
                    if i32::from(self.mem.font(ha + 1)) != hf {
                        break 'found2;
                    }
                    self.hy.init_list = self.mem.lig_ptr(ha);
                    self.hy.init_lig = true;
                    self.hy.init_lft = self.mem.subtype(ha) > 1;
                    self.hy.hu[0] = i32::from(self.mem.character(ha + 1));
                    if self.hy.init_list == NULL && self.hy.init_lft {
                        self.hy.hu[0] = 256;
                        self.hy.init_lig = false;
                    }
                    self.mem.free_node(ha, SMALL_NODE_SIZE);
                } else {
                    // no punctuation found; look for left boundary.
                    if !self.mem.is_char_node(r)
                        && self.mem.node_type(r) == LIGATURE_NODE
                        && self.mem.subtype(r) > 1
                    {
                        break 'found2;
                    }
                    j = 1;
                    s = ha;
                    self.hy.init_list = NULL;
                    break 'common_ending;
                }
                s = cur_p; // cur_p != ha because type(cur_p) = glue_node
                while self.mem.link(s) != ha {
                    s = self.mem.link(s);
                }
                j = 0;
                break 'common_ending;
            }
            // found2:
            s = ha;
            j = 0;
            self.hy.hu[0] = 256;
            self.hy.init_lig = false;
            self.hy.init_list = NULL;
        }
        // common_ending:
        self.flush_node_list(r);
        // §913: reconstitute nodes, inserting discretionaries.
        // (`l` is a single variable across the whole repeat-loop, like
        // tex.web's: §916 leaves it positioned for the next §914 round.)
        let mut l: i32;
        loop {
            l = j;
            j = self.reconstitute(j, self.hy.hn, bchar, self.hy.hyf_char)? + 1;
            if self.hy.hyphen_passed == 0 {
                let hh = self.mem.hold_head();
                let lk = self.mem.link(hh);
                self.mem.set_link(s, lk);
                while self.mem.link(s) > NULL {
                    s = self.mem.link(s);
                }
                if self.hy.hyf[(j - 1) as usize] % 2 == 1 {
                    l = j;
                    self.hy.hyphen_passed = j - 1;
                    let hh = self.mem.hold_head();
                    self.mem.set_link(hh, NULL);
                }
            }
            if self.hy.hyphen_passed > 0 {
                // §914: create and append a discretionary node.
                loop {
                    let r = self.mem.get_node(SMALL_NODE_SIZE)?;
                    let hh = self.mem.hold_head();
                    let lk = self.mem.link(hh);
                    self.mem.set_link(r, lk);
                    self.mem.set_node_type(r, DISC_NODE);
                    let mut major_tail = r;
                    let mut r_count = 0;
                    while self.mem.link(major_tail) > NULL {
                        major_tail = self.mem.link(major_tail);
                        r_count += 1;
                    }
                    let mut i = self.hy.hyphen_passed;
                    self.hy.hyf[i as usize] = 0;
                    // §915: pre_break gets hu[l..=i] plus a hyphen.
                    let mut minor_tail: Pointer = NULL;
                    self.mem.set_pre_break(r, NULL);
                    let hyf_node = self.new_character(hf, self.hy.hyf_char)?;
                    let mut c: i32 = 0;
                    if hyf_node != NULL {
                        i += 1;
                        c = self.hy.hu[i as usize];
                        self.hy.hu[i as usize] = self.hy.hyf_char;
                        self.mem.free_avail(hyf_node);
                    }
                    while l <= i {
                        let fb = self.fonts.bchar[hf as usize];
                        l = self.reconstitute(l, i, fb, NON_CHAR)? + 1;
                        let hh = self.mem.hold_head();
                        if self.mem.link(hh) > NULL {
                            let lk = self.mem.link(hh);
                            if minor_tail == NULL {
                                self.mem.set_pre_break(r, lk);
                            } else {
                                self.mem.set_link(minor_tail, lk);
                            }
                            minor_tail = lk;
                            while self.mem.link(minor_tail) > NULL {
                                minor_tail = self.mem.link(minor_tail);
                            }
                        }
                    }
                    if hyf_node != NULL {
                        self.hy.hu[i as usize] = c; // restore the character
                        l = i;
                        // (§915 also does `decr(i)`, maintaining l = i+1;
                        // i is not read again on this path.)
                    }
                    // §916: post_break gets hu[i+1..], synchronizing.
                    minor_tail = NULL;
                    self.mem.set_post_break(r, NULL);
                    let mut c_loc = 0;
                    if self.fonts.bchar_label[hf as usize] != NON_ADDRESS {
                        l -= 1;
                        c = self.hy.hu[l as usize];
                        c_loc = l;
                        self.hy.hu[l as usize] = 256;
                    }
                    while l < j {
                        loop {
                            l = self.reconstitute(l, self.hy.hn, bchar, NON_CHAR)? + 1;
                            if c_loc > 0 {
                                self.hy.hu[c_loc as usize] = c;
                                c_loc = 0;
                            }
                            let hh = self.mem.hold_head();
                            if self.mem.link(hh) > NULL {
                                let lk = self.mem.link(hh);
                                if minor_tail == NULL {
                                    self.mem.set_post_break(r, lk);
                                } else {
                                    self.mem.set_link(minor_tail, lk);
                                }
                                minor_tail = lk;
                                while self.mem.link(minor_tail) > NULL {
                                    minor_tail = self.mem.link(minor_tail);
                                }
                            }
                            if l >= j {
                                break;
                            }
                        }
                        while l > j {
                            // §917: append characters of hu[j..] to major_tail.
                            j = self.reconstitute(j, self.hy.hn, bchar, NON_CHAR)? + 1;
                            let hh = self.mem.hold_head();
                            let lk = self.mem.link(hh);
                            self.mem.set_link(major_tail, lk);
                            while self.mem.link(major_tail) > NULL {
                                major_tail = self.mem.link(major_tail);
                                r_count += 1;
                            }
                        }
                    }
                    // §918: move s to the end, set replace_count.
                    if r_count > 127 {
                        let lk = self.mem.link(r);
                        self.mem.set_link(s, lk);
                        self.mem.set_link(r, NULL);
                        self.flush_node_list(r);
                    } else {
                        self.mem.set_link(s, r);
                        self.mem.set_replace_count(r, r_count as u16);
                    }
                    s = major_tail;
                    self.hy.hyphen_passed = j - 1;
                    let hh = self.mem.hold_head();
                    self.mem.set_link(hh, NULL);
                    if self.hy.hyf[(j - 1) as usize] % 2 != 1 {
                        break;
                    }
                }
            }
            if j > self.hy.hn {
                break;
            }
        }
        self.mem.set_link(s, q);
        let il = self.hy.init_list;
        self.mem.flush_list(il);
        Ok(())
    }

    /// `set_hyph_index` (etex.ch): point `hyph_index` at the packed
    /// hyphenation codes for `cur_lang`, or 0 when there are none.
    pub fn set_hyph_index(&mut self) {
        let hs = self.hy.hyph_start;
        let cl = self.hy.cur_lang;
        if self.hy.trie_char(hs + cl) != cl {
            self.hy.hyph_index = 0;
        } else {
            self.hy.hyph_index = self.hy.trie_link(hs + cl);
        }
    }

    /// `set_lc_code(c)` (etex.ch): the hyphenation or `\lccode` value.
    pub fn set_lc_code(&mut self, c: i32) -> i32 {
        if self.hy.hyph_index == 0 {
            self.eqtb.lc_code(c)
        } else if self.hy.trie_char(self.hy.hyph_index + c) != c {
            0
        } else {
            i32::from(self.hy.trie_op(self.hy.hyph_index + c))
        }
    }

    /// etex.ch: store the current `\lccode` values in the linked trie
    /// (under `hyph_root = trie_r[0]`) for the current language.
    fn store_hyph_codes(&mut self) -> TexResult<()> {
        let c = self.hy.cur_lang;
        let mut first_child = false;
        let mut p: i32 = 0;
        let mut q: i32;
        loop {
            q = p;
            p = self.hy.trie_r[q as usize];
            if p == 0 || c <= self.hy.trie_c[p as usize] {
                break;
            }
        }
        if p == 0 || c < self.hy.trie_c[p as usize] {
            // Insert a new trie node between q and p.
            if self.hy.trie_ptr == self.hy.trie_size {
                return Err(TexInterrupt::Overflow {
                    what: "pattern memory",
                    size: self.hy.trie_size,
                });
            }
            self.hy.trie_ptr += 1;
            let tp = self.hy.trie_ptr as usize;
            self.hy.trie_r[tp] = p;
            p = self.hy.trie_ptr;
            self.hy.trie_l[p as usize] = 0;
            if first_child {
                self.hy.trie_l[q as usize] = p;
            } else {
                self.hy.trie_r[q as usize] = p;
            }
            self.hy.trie_c[p as usize] = c;
            self.hy.trie_o[p as usize] = 0;
        }
        q = p; // node q represents cur_lang
               // Store all current lc_code values.
        p = self.hy.trie_l[q as usize];
        first_child = true;
        for c in 0..=255 {
            if self.eqtb.lc_code(c) > 0 || (c == 255 && first_child) {
                if p == 0 {
                    // Insert a new trie node between q and p.
                    if self.hy.trie_ptr == self.hy.trie_size {
                        return Err(TexInterrupt::Overflow {
                            what: "pattern memory",
                            size: self.hy.trie_size,
                        });
                    }
                    self.hy.trie_ptr += 1;
                    let tp = self.hy.trie_ptr as usize;
                    self.hy.trie_r[tp] = p;
                    p = self.hy.trie_ptr;
                    self.hy.trie_l[p as usize] = 0;
                    if first_child {
                        self.hy.trie_l[q as usize] = p;
                    } else {
                        self.hy.trie_r[q as usize] = p;
                    }
                    self.hy.trie_c[p as usize] = c;
                    self.hy.trie_o[p as usize] = 0;
                } else {
                    self.hy.trie_c[p as usize] = c;
                }
                self.hy.trie_o[p as usize] = self.eqtb.lc_code(c) as u16;
                q = p;
                p = self.hy.trie_r[q as usize];
                first_child = false;
            }
        }
        if first_child {
            self.hy.trie_l[q as usize] = 0;
        } else {
            self.hy.trie_r[q as usize] = 0;
        }
        Ok(())
    }

    /// `new_hyph_exceptions` (§934-§940): `\hyphenation{...}`.
    pub fn new_hyph_exceptions(&mut self) -> TexResult<()> {
        use crate::cmds::*;
        self.scan_left_brace()?;
        let language = self.eqtb.int_par(crate::eqtb::LANGUAGE_CODE);
        self.hy.cur_lang = norm_lang(language);
        // etex.ch: use the saved hyphenation codes when available.
        if self.hy.trie_not_ready {
            self.hy.hyph_index = 0;
        } else {
            self.set_hyph_index();
        }
        let mut n: i32 = 0;
        let mut p: Pointer = NULL;
        loop {
            self.get_x_token()?;
            loop {
                // reswitch:
                match self.cur_cmd {
                    LETTER | OTHER_CHAR | CHAR_GIVEN => {
                        // §937.
                        if self.cur_chr == '-' as i32 {
                            if n < 63 {
                                let q = self.mem.get_avail()?;
                                self.mem.set_link(q, p);
                                self.mem.set_info(q, n);
                                p = q;
                            }
                        } else if self.set_lc_code(self.cur_chr) == 0 {
                            self.print_err("Not a letter");
                            self.help(&[
                                "Letters in \\hyphenation words must have \\lccode>0.",
                                "Proceed; I'll ignore the character I just read.",
                            ]);
                            self.error()?;
                        } else if n < 63 {
                            n += 1;
                            self.hy.hc[n as usize] = self.set_lc_code(self.cur_chr);
                        }
                    }
                    CHAR_NUM => {
                        self.scan_char_num()?;
                        self.cur_chr = self.cur_val;
                        self.cur_cmd = CHAR_GIVEN;
                        continue; // reswitch
                    }
                    SPACER | RIGHT_BRACE => {
                        if n > 1 {
                            // §939: enter a hyphenation exception.
                            n += 1;
                            self.hy.hc[n as usize] = self.hy.cur_lang;
                            self.strings.str_room(n as usize)?;
                            let mut h: i32 = 0;
                            for j in 1..=n {
                                h = (h + h + self.hy.hc[j as usize]) % self.hy.hyph_size;
                                self.strings.append_char(self.hy.hc[j as usize]);
                            }
                            let mut s = self.strings.make_string()?;
                            // §940: insert (s, p) into the exception table.
                            if self.hy.hyph_count == self.hy.hyph_size {
                                return Err(TexInterrupt::Overflow {
                                    what: "exception dictionary",
                                    size: self.hy.hyph_size,
                                });
                            }
                            self.hy.hyph_count += 1;
                            while self.hy.hyph_word[h as usize] != 0 {
                                // §941: keep the table ordered.
                                let k = self.hy.hyph_word[h as usize];
                                let swap = if self.strings.length(k) < self.strings.length(s) {
                                    true
                                } else if self.strings.length(k) > self.strings.length(s) {
                                    false
                                } else {
                                    let mut res = false; // equal: "goto found"
                                    let (ks, ss) = (self.strings.str(k), self.strings.str(s));
                                    for (a, b) in ks.iter().zip(ss.iter()) {
                                        if a < b {
                                            res = true;
                                            break;
                                        }
                                        if a > b {
                                            res = false;
                                            break;
                                        }
                                    }
                                    res || ks == ss
                                };
                                if swap {
                                    std::mem::swap(&mut self.hy.hyph_list[h as usize], &mut p);
                                    std::mem::swap(&mut self.hy.hyph_word[h as usize], &mut s);
                                }
                                if h > 0 {
                                    h -= 1;
                                } else {
                                    h = self.hy.hyph_size;
                                }
                            }
                            self.hy.hyph_word[h as usize] = s;
                            self.hy.hyph_list[h as usize] = p;
                        }
                        if self.cur_cmd == RIGHT_BRACE {
                            return Ok(());
                        }
                        n = 0;
                        p = NULL;
                    }
                    _ => {
                        // §936.
                        self.print_err("Improper ");
                        self.print_esc_str("hyphenation");
                        self.print_chars(" will be flushed");
                        self.help(&[
                            "Hyphenation exceptions must contain only letters",
                            "and hyphens. But continue; I'll forgive and forget.",
                        ]);
                        self.error()?;
                    }
                }
                break;
            }
        }
    }

    /// `new_trie_op(d, n, v)` (§944).
    fn new_trie_op(&mut self, d: i32, n: i32, v: u16) -> TexResult<u16> {
        let tos = self.hy.trie_op_size;
        let mut h =
            (n + 313 * d + 361 * i32::from(v) + 1009 * self.hy.cur_lang).abs() % (tos + tos) - tos;
        loop {
            let l = self.hy.trie_op_hash[(h + tos) as usize];
            if l == 0 {
                if self.hy.trie_op_ptr == tos {
                    return Err(TexInterrupt::Overflow {
                        what: "pattern memory ops",
                        size: tos,
                    });
                }
                let mut u = self.hy.trie_used[self.hy.cur_lang as usize];
                if u == u16::MAX {
                    return Err(TexInterrupt::Overflow {
                        what: "pattern memory ops per language",
                        size: i32::from(u16::MAX),
                    });
                }
                self.hy.trie_op_ptr += 1;
                u += 1;
                self.hy.trie_used[self.hy.cur_lang as usize] = u;
                let top = self.hy.trie_op_ptr as usize;
                self.hy.hyf_distance[top] = d as u8;
                self.hy.hyf_num[top] = n as u8;
                self.hy.hyf_next[top] = v;
                self.hy.trie_op_lang[top] = self.hy.cur_lang;
                self.hy.trie_op_hash[(h + tos) as usize] = self.hy.trie_op_ptr;
                self.hy.trie_op_val[top] = u;
                return Ok(u);
            }
            let l = l as usize;
            if i32::from(self.hy.hyf_distance[l]) == d
                && i32::from(self.hy.hyf_num[l]) == n
                && self.hy.hyf_next[l] == v
                && self.hy.trie_op_lang[l] == self.hy.cur_lang
            {
                return Ok(self.hy.trie_op_val[l]);
            }
            if h > -tos {
                h -= 1;
            } else {
                h = tos;
            }
        }
    }

    /// `new_patterns` (§960-§965): `\patterns{...}` (INITEX).
    pub fn new_patterns(&mut self) -> TexResult<()> {
        use crate::cmds::*;
        if !self.hy.trie_not_ready {
            self.print_err("Too late for ");
            self.print_esc_str("patterns");
            self.help(&["All patterns must be given before typesetting begins."]);
            self.error()?;
            let _ = self.scan_toks(false, false)?;
            let dr = self.inp.def_ref;
            self.mem.flush_list(dr);
            return Ok(());
        }
        let language = self.eqtb.int_par(crate::eqtb::LANGUAGE_CODE);
        self.hy.cur_lang = norm_lang(language);
        self.scan_left_brace()?;
        let mut k: i32 = 0;
        self.hy.hyf[0] = 0;
        let mut digit_sensed = false;
        loop {
            self.get_x_token()?;
            match self.cur_cmd {
                LETTER | OTHER_CHAR => {
                    // §961: append a new letter or a hyphen level.
                    if digit_sensed || self.cur_chr < '0' as i32 || self.cur_chr > '9' as i32 {
                        let c = if self.cur_chr == '.' as i32 {
                            0 // edge-of-word delimiter
                        } else {
                            let c = self.eqtb.lc_code(self.cur_chr);
                            if c == 0 {
                                self.print_err("Nonletter");
                                self.help(&["(See Appendix H.)"]);
                                self.error()?;
                            }
                            c
                        };
                        if k < 63 {
                            k += 1;
                            self.hy.hc[k as usize] = c;
                            self.hy.hyf[k as usize] = 0;
                            digit_sensed = false;
                        }
                    } else if k < 63 {
                        self.hy.hyf[k as usize] = (self.cur_chr - '0' as i32) as u8;
                        digit_sensed = true;
                    }
                }
                SPACER | RIGHT_BRACE => {
                    if k > 0 {
                        // §962: insert a new pattern into the linked trie.
                        // §965: compute the trie op code v.
                        if self.hy.hc[1] == 0 {
                            self.hy.hyf[0] = 0;
                        }
                        if self.hy.hc[k as usize] == 0 {
                            self.hy.hyf[k as usize] = 0;
                        }
                        let mut l = k;
                        let mut v: u16 = 0;
                        loop {
                            if self.hy.hyf[l as usize] != 0 {
                                v =
                                    self.new_trie_op(k - l, i32::from(self.hy.hyf[l as usize]), v)?;
                            }
                            if l > 0 {
                                l -= 1;
                            } else {
                                break;
                            }
                        }
                        let mut q: i32 = 0;
                        self.hy.hc[0] = self.hy.cur_lang;
                        let mut l: i32 = 0;
                        while l <= k {
                            let c = self.hy.hc[l as usize];
                            l += 1;
                            let mut p = self.hy.trie_l[q as usize];
                            let mut first_child = true;
                            while p > 0 && c > self.hy.trie_c[p as usize] {
                                q = p;
                                p = self.hy.trie_r[q as usize];
                                first_child = false;
                            }
                            if p == 0 || c < self.hy.trie_c[p as usize] {
                                // §963: insert a new trie node.
                                if self.hy.trie_ptr == self.hy.trie_size {
                                    return Err(TexInterrupt::Overflow {
                                        what: "pattern memory",
                                        size: self.hy.trie_size,
                                    });
                                }
                                self.hy.trie_ptr += 1;
                                let tp = self.hy.trie_ptr as usize;
                                self.hy.trie_r[tp] = p;
                                p = self.hy.trie_ptr;
                                self.hy.trie_l[p as usize] = 0;
                                if first_child {
                                    self.hy.trie_l[q as usize] = p;
                                } else {
                                    self.hy.trie_r[q as usize] = p;
                                }
                                self.hy.trie_c[p as usize] = c;
                                self.hy.trie_o[p as usize] = 0;
                            }
                            q = p;
                        }
                        if self.hy.trie_o[q as usize] != 0 {
                            self.print_err("Duplicate pattern");
                            self.help(&["(See Appendix H.)"]);
                            self.error()?;
                        }
                        self.hy.trie_o[q as usize] = v;
                    }
                    if self.cur_cmd == RIGHT_BRACE {
                        // etex.ch §960: save the lc codes with the patterns.
                        if self.eqtb.int_par(crate::eqtb::SAVING_HYPH_CODES_CODE) > 0 {
                            self.store_hyph_codes()?;
                        }
                        return Ok(());
                    }
                    k = 0;
                    self.hy.hyf[0] = 0;
                    digit_sensed = false;
                }
                _ => {
                    self.print_err("Bad ");
                    self.print_esc_str("patterns");
                    self.help(&["(See Appendix H.)"]);
                    self.error()?;
                }
            }
        }
    }

    /// `trie_node(p)` (§945).
    fn trie_node(&mut self, p: i32) -> i32 {
        let mut h = (self.hy.trie_c[p as usize]
            + 1009 * i32::from(self.hy.trie_o[p as usize])
            + 2718 * self.hy.trie_l[p as usize]
            + 3142 * self.hy.trie_r[p as usize])
            .abs()
            % self.hy.trie_size;
        loop {
            let q = self.hy.trie_hash[h as usize];
            if q == 0 {
                self.hy.trie_hash[h as usize] = p;
                return p;
            }
            if self.hy.trie_c[q as usize] == self.hy.trie_c[p as usize]
                && self.hy.trie_o[q as usize] == self.hy.trie_o[p as usize]
                && self.hy.trie_l[q as usize] == self.hy.trie_l[p as usize]
                && self.hy.trie_r[q as usize] == self.hy.trie_r[p as usize]
            {
                return q;
            }
            if h > 0 {
                h -= 1;
            } else {
                h = self.hy.trie_size;
            }
        }
    }

    /// `compress_trie(p)` (§946).
    fn compress_trie(&mut self, p: i32) -> i32 {
        if p == 0 {
            0
        } else {
            let l = self.compress_trie(self.hy.trie_l[p as usize]);
            self.hy.trie_l[p as usize] = l;
            let r = self.compress_trie(self.hy.trie_r[p as usize]);
            self.hy.trie_r[p as usize] = r;
            self.trie_node(p)
        }
    }

    /// `first_fit(p)` (§953-§956).
    fn first_fit(&mut self, p: i32) -> TexResult<()> {
        let c = self.hy.trie_c[p as usize];
        let mut z = self.hy.trie_min[c as usize];
        loop {
            let h = z - c;
            // §954: ensure trie_max >= h + 256.
            if self.hy.trie_max < h + 256 {
                if self.hy.trie_size <= h + 256 {
                    return Err(TexInterrupt::Overflow {
                        what: "pattern memory",
                        size: self.hy.trie_size,
                    });
                }
                loop {
                    self.hy.trie_max += 1;
                    let tm = self.hy.trie_max;
                    self.hy.trie_taken[tm as usize] = false;
                    self.hy.set_trie_link(tm, tm + 1);
                    self.hy.set_trie_back(tm, tm - 1);
                    if tm == h + 256 {
                        break;
                    }
                }
            }
            let mut fits = !self.hy.trie_taken[h as usize];
            if fits {
                // §955: do all family characters fit?
                let mut q = self.hy.trie_r[p as usize];
                while q > 0 {
                    if self.hy.trie_link(h + self.hy.trie_c[q as usize]) == 0 {
                        fits = false;
                        break;
                    }
                    q = self.hy.trie_r[q as usize];
                }
            }
            if fits {
                break;
            }
            z = self.hy.trie_link(z); // move to the next hole
        }
        // found (§956): pack the family relative to h.
        let h = z - c;
        self.hy.trie_taken[h as usize] = true;
        self.hy.trie_hash[p as usize] = h; // trie_ref[p] := h
        let mut q = p;
        loop {
            let z = h + self.hy.trie_c[q as usize];
            let mut l = self.hy.trie_back(z);
            let r = self.hy.trie_link(z);
            self.hy.set_trie_back(r, l);
            self.hy.set_trie_link(l, r);
            self.hy.set_trie_link(z, 0);
            if l < 256 {
                let ll = if z < 256 { z } else { 256 };
                loop {
                    self.hy.trie_min[l as usize] = r;
                    l += 1;
                    if l == ll {
                        break;
                    }
                }
            }
            q = self.hy.trie_r[q as usize];
            if q == 0 {
                break;
            }
        }
        Ok(())
    }

    /// `trie_pack(p)` (§957).
    fn trie_pack(&mut self, p: i32) -> TexResult<()> {
        let mut p = p;
        loop {
            let q = self.hy.trie_l[p as usize];
            if q > 0 && self.hy.trie_hash[q as usize] == 0 {
                self.first_fit(q)?;
                self.trie_pack(q)?;
            }
            p = self.hy.trie_r[p as usize];
            if p == 0 {
                break;
            }
        }
        Ok(())
    }

    /// `trie_fix(p)` (§959).
    fn trie_fix(&mut self, p: i32) {
        let z = self.hy.trie_hash[p as usize]; // trie_ref[p]
        let mut p = p;
        loop {
            let q = self.hy.trie_l[p as usize];
            let c = self.hy.trie_c[p as usize];
            let zc = (z + c) as usize;
            // trie_link(z+c) := trie_ref[q] (0 when q = 0)
            let link = if q > 0 {
                self.hy.trie_hash[q as usize]
            } else {
                0
            };
            self.hy.trie[zc].set_rh(link);
            self.hy.trie[zc].set_b1(c as u16);
            self.hy.trie[zc].set_b0(self.hy.trie_o[p as usize]);
            if q > 0 {
                self.trie_fix(q);
            }
            p = self.hy.trie_r[p as usize];
            if p == 0 {
                break;
            }
        }
    }

    /// `init_trie` (§966): compress and pack the trie.
    pub fn init_trie(&mut self) -> TexResult<()> {
        // §947/§952: get ready to compress.
        // Sort the hyphenation op tables into proper order (§945a).
        self.hy.op_start[0] = 0;
        for j in 1..256 {
            self.hy.op_start[j] = self.hy.op_start[j - 1] + i32::from(self.hy.trie_used[j - 1]);
        }
        for j in 1..=(self.hy.trie_op_ptr as usize) {
            self.hy.trie_op_hash[j] = self.hy.op_start[self.hy.trie_op_lang[j] as usize]
                + i32::from(self.hy.trie_op_val[j]);
        }
        for j in 1..=(self.hy.trie_op_ptr as usize) {
            while self.hy.trie_op_hash[j] > j as i32 {
                let k = self.hy.trie_op_hash[j] as usize;
                self.hy.hyf_distance.swap(k, j);
                self.hy.hyf_num.swap(k, j);
                self.hy.hyf_next.swap(k, j);
                self.hy.trie_op_hash[j] = self.hy.trie_op_hash[k];
                self.hy.trie_op_hash[k] = k as i32;
            }
        }
        for p in 0..=(self.hy.trie_size as usize) {
            self.hy.trie_hash[p] = 0;
        }
        // etex.ch §952: compress the hyph_codes trie first.
        let hr = self.compress_trie(self.hy.trie_r[0]);
        self.hy.trie_r[0] = hr;
        let root = self.compress_trie(self.hy.trie_l[0]);
        self.hy.trie_l[0] = root;
        for p in 0..=(self.hy.trie_ptr as usize) {
            self.hy.trie_hash[p] = 0; // trie_ref
        }
        for p in 0..256 {
            self.hy.trie_min[p] = p as i32 + 1;
        }
        self.hy.set_trie_link(0, 1);
        self.hy.trie_max = 0;
        if self.hy.trie_l[0] != 0 {
            let root = self.hy.trie_l[0];
            self.first_fit(root)?;
            self.trie_pack(root)?;
        }
        if self.hy.trie_r[0] != 0 {
            // etex.ch: pack all stored hyph_codes (avoiding location 1, to
            // distinguish lc_code values from patterns).
            if self.hy.trie_l[0] == 0 {
                for p in 0..256 {
                    self.hy.trie_min[p] = p as i32 + 2;
                }
            }
            let hr = self.hy.trie_r[0];
            self.first_fit(hr)?;
            self.trie_pack(hr)?;
            self.hy.hyph_start = self.hy.trie_hash[hr as usize]; // trie_ref
        }
        // §958 (+ etex.ch): move the data into trie.
        if self.hy.trie_max == 0 {
            for r in 0..=256 {
                self.hy.trie[r] = MemoryWord::ZERO;
            }
            self.hy.trie_max = 256;
        } else {
            if self.hy.trie_r[0] > 0 {
                let hr = self.hy.trie_r[0];
                self.trie_fix(hr);
            }
            if self.hy.trie_l[0] > 0 {
                let root = self.hy.trie_l[0];
                self.trie_fix(root);
            }
            let mut r = 0;
            loop {
                let s = self.hy.trie_link(r);
                self.hy.trie[r as usize] = MemoryWord::ZERO;
                r = s;
                if r > self.hy.trie_max {
                    break;
                }
            }
        }
        self.hy.trie[0].set_b1('?' as u16); // make trie_char(c) <> c for all c
        self.hy.trie_not_ready = false;
        Ok(())
    }
}
