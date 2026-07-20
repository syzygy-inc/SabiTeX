//! Shipping pages out: the DVI back end.
//!
//! Ports tex.web Part 31 (the DVI opcodes) and Part 32 (§592-§644): the
//! half-buffer output discipline, the movement-optimization stacks
//! (`movement`, `prune_movements`), `hlist_out`/`vlist_out` and `ship_out`.
//! Every rounding and buffering decision is kept bit-exact because the
//! w/x/y/z reuse depends on which bytes are still in the buffer.
//!
//! The "file" behind `write_dvi` is an in-memory `Vec<u8>`, written out via
//! `TexFs` when the job ends (XDV arrives with M7 as a second id_byte).

use crate::engine::Engine;
use crate::error::TexResult;
use crate::nodes::*;
use crate::types::{Pointer, Scaled, NULL};

// Part 31: DVI opcodes.
pub const SET1: u8 = 128;
pub const SET_RULE: u8 = 132;
pub const PUT_RULE: u8 = 137;
pub const BOP: u8 = 139;
pub const EOP: u8 = 140;
pub const PUSH: u8 = 141;
pub const POP: u8 = 142;
pub const RIGHT1: u8 = 143;
pub const W0: u8 = 147;
pub const W1: u8 = 148;
pub const X0: u8 = 152;
pub const X1: u8 = 153;
pub const DOWN1: u8 = 157;
pub const Y0: u8 = 161;
pub const Y1: u8 = 162;
pub const Z0: u8 = 166;
pub const Z1: u8 = 167;
pub const FNT_NUM_0: u8 = 171;
pub const FNT1: u8 = 235;
pub const XXX1: u8 = 239;
pub const XXX4: u8 = 242;
pub const FNT_DEF1: u8 = 243;
pub const PRE: u8 = 247;
pub const POST: u8 = 248;
pub const POST_POST: u8 = 249;

/// `id_byte = 2` (§587): the DVI format version. XDV files (xetex.web)
/// use 7 instead; see [`Engine::dvi_id_byte`].
pub const ID_BYTE: u8 = 2;
/// XDV (xetex.web): the extended format written when native fonts are
/// in use.
pub const XDV_ID_BYTE: u8 = 7;

// XDV opcodes (xetex.web).
pub const DEFINE_NATIVE_FONT: u8 = 252;
pub const SET_GLYPHS: u8 = 253;

// XDV define_native_font flag bits (XeTeX_ext.c).
pub const XDV_FLAG_COLORED: u16 = 0x0200;

/// `dvi_buf_size` (§11).
pub const DVI_BUF_SIZE: i32 = 800;

// §611: movement-stack info codes.
pub const Y_HERE: i32 = 1;
pub const Z_HERE: i32 = 2;
pub const YZ_OK: i32 = 3;
pub const Y_OK: i32 = 4;
pub const Z_OK: i32 = 5;
pub const D_FIXED: i32 = 6;
// §615: search states.
pub const NONE_SEEN: i32 = 0;
pub const Y_SEEN: i32 = 6;
pub const Z_SEEN: i32 = 12;

/// `movement_node_size = 3` (§608); `location(p) == mem[p+2].int`.
pub const MOVEMENT_NODE_SIZE: i32 = 3;

/// All DVI output state (§592, §595, §605, §616).
pub struct DviState {
    /// `dvi_buf` and its half-buffer bookkeeping.
    pub buf: Vec<u8>,
    pub half_buf: i32,
    pub limit: i32,
    pub ptr: i32,
    pub offset: i32,
    pub gone: i32,
    /// The "file" contents.
    pub file: Vec<u8>,
    /// §592 statistics.
    pub max_v: Scaled,
    pub max_h: Scaled,
    pub max_push: i32,
    pub last_bop: i32,
    pub doing_leaders: bool,
    /// §605: the down and right stacks.
    pub down_ptr: Pointer,
    pub right_ptr: Pointer,
    /// §616: coordinates.
    pub dvi_h: Scaled,
    pub dvi_v: Scaled,
    pub cur_h: Scaled,
    pub cur_v: Scaled,
    pub dvi_f: i32,
}

impl DviState {
    pub fn new() -> DviState {
        DviState {
            buf: vec![0; DVI_BUF_SIZE as usize + 1],
            half_buf: DVI_BUF_SIZE / 2,
            limit: DVI_BUF_SIZE,
            ptr: 0,
            offset: 0,
            gone: 0,
            file: Vec::new(),
            max_v: 0,
            max_h: 0,
            max_push: 0,
            last_bop: -1,
            doing_leaders: false,
            down_ptr: NULL,
            right_ptr: NULL,
            dvi_h: 0,
            dvi_v: 0,
            cur_h: 0,
            cur_v: 0,
            dvi_f: 0,
        }
    }

    /// `write_dvi(a, b)` (§597).
    fn write_dvi(&mut self, a: i32, b: i32) {
        for k in a..=b {
            self.file.push(self.buf[k as usize]);
        }
    }

    /// `dvi_swap` (§598).
    fn swap(&mut self) {
        if self.limit == DVI_BUF_SIZE {
            self.write_dvi(0, self.half_buf - 1);
            self.limit = self.half_buf;
            self.offset += DVI_BUF_SIZE;
            self.ptr = 0;
        } else {
            self.write_dvi(self.half_buf, DVI_BUF_SIZE - 1);
            self.limit = DVI_BUF_SIZE;
        }
        self.gone += self.half_buf;
    }

    /// `dvi_out(b)` (§598).
    pub fn out(&mut self, b: u8) {
        self.buf[self.ptr as usize] = b;
        self.ptr += 1;
        if self.ptr == self.limit {
            self.swap();
        }
    }

    /// `dvi_four(x)` (§600).
    pub fn four(&mut self, x: i32) {
        let mut x = i64::from(x);
        if x >= 0 {
            self.out((x / 0o100000000) as u8);
        } else {
            x += 0o10000000000;
            x += 0o10000000000;
            self.out((x / 0o100000000 + 128) as u8);
        }
        x %= 0o100000000;
        self.out((x / 0o200000) as u8);
        x %= 0o200000;
        self.out((x / 0o400) as u8);
        self.out((x % 0o400) as u8);
    }

    /// `dvi_pop(l)` (§601).
    pub fn pop(&mut self, l: i32) {
        if l == self.offset + self.ptr && self.ptr > 0 {
            self.ptr -= 1;
        } else {
            self.out(POP);
        }
    }

    /// §599: empty the last bytes out of `dvi_buf`.
    pub fn flush_buffer(&mut self) {
        if self.limit == self.half_buf {
            self.write_dvi(self.half_buf, DVI_BUF_SIZE - 1);
        }
        if self.ptr > 0 {
            self.write_dvi(0, self.ptr - 1);
        }
    }
}

impl Engine {
    /// XDV when any native font is loaded, classic DVI otherwise. TRIP,
    /// etrip and all TFM-only jobs keep byte-identical DVI output.
    pub fn dvi_id_byte(&self) -> u8 {
        if self
            .fonts
            .native
            .iter()
            .take(self.fonts.font_ptr as usize + 1)
            .any(|n| n.is_some())
        {
            XDV_ID_BYTE
        } else {
            ID_BYTE
        }
    }

    /// `dvi_native_font_def(f)` (xetex.web + XeTeX_ext.c makefontdef):
    /// define_native_font k[4] s[4] flags[2] l[1] n[l] i[4] [rgba[4]].
    fn dvi_native_font_def(&mut self, f: i32) {
        let Some(nf) = self.fonts.native[f as usize].as_deref() else {
            return;
        };
        let (size, flags16, filename, index, rgba) = (
            nf.size,
            if nf.flags & crate::native::FONT_FLAGS_COLORED != 0 {
                XDV_FLAG_COLORED
            } else {
                0
            },
            nf.filename.clone(),
            nf.index,
            nf.rgba,
        );
        self.dvi.out(DEFINE_NATIVE_FONT);
        self.dvi.four(f - 1); // f - font_base - 1
        self.dvi.four(size);
        self.dvi.out((flags16 >> 8) as u8);
        self.dvi.out(flags16 as u8);
        let name = filename.as_bytes();
        self.dvi.out(name.len() as u8);
        for &b in name {
            self.dvi.out(b);
        }
        self.dvi.four(index as i32);
        if flags16 & XDV_FLAG_COLORED != 0 {
            self.dvi.four(rgba as i32);
        }
    }

    /// `dvi_font_def(f)` (§602 + xetex.web).
    fn dvi_font_def(&mut self, f: i32) {
        if self.fonts.native[f as usize].is_some() {
            self.dvi_native_font_def(f);
            return;
        }
        self.dvi.out(FNT_DEF1);
        self.dvi.out((f - 1) as u8);
        let chk = self.fonts.check[f as usize];
        self.dvi.out(chk.qqqq(0) as u8);
        self.dvi.out(chk.qqqq(1) as u8);
        self.dvi.out(chk.qqqq(2) as u8);
        self.dvi.out(chk.qqqq(3) as u8);
        let (sz, dsz) = (self.fonts.size[f as usize], self.fonts.dsize[f as usize]);
        self.dvi.four(sz);
        self.dvi.four(dsz);
        let area = self.fonts.area[f as usize].clone();
        let name = self.fonts.name[f as usize].clone();
        self.dvi.out(area.len() as u8);
        self.dvi.out(name.len() as u8);
        for b in area.bytes().chain(name.bytes()) {
            self.dvi.out(b);
        }
    }

    /// `movement(w, o)` (§607-§615): output a down/right command, reusing
    /// w/x/y/z registers when the stacks permit.
    fn movement(&mut self, w: Scaled, o: u8) -> TexResult<()> {
        let q = self.mem.get_node(MOVEMENT_NODE_SIZE)?;
        self.mem.set_width(q, w);
        let loc = self.dvi.offset + self.dvi.ptr;
        self.mem.word_mut(q + 2).set_int(loc);
        if o == DOWN1 {
            let d = self.dvi.down_ptr;
            self.mem.set_link(q, d);
            self.dvi.down_ptr = q;
        } else {
            let r = self.dvi.right_ptr;
            self.mem.set_link(q, r);
            self.dvi.right_ptr = q;
        }
        // §612-§613: look at the other stack entries.
        let mut p = self.mem.link(q);
        let mut mstate = NONE_SEEN;
        let mut hit: Pointer = NULL;
        'not_found: {
            while p != NULL {
                if self.mem.width(p) == w {
                    // §613: a node with matching width.
                    let case = mstate + self.mem.info(p);
                    if case == NONE_SEEN + YZ_OK
                        || case == NONE_SEEN + Y_OK
                        || case == Z_SEEN + YZ_OK
                        || case == Z_SEEN + Y_OK
                    {
                        if self.mem.word(p + 2).int() < self.dvi.gone {
                            break 'not_found;
                        }
                        // §614: change the buffered instruction to y or w.
                        let mut k = self.mem.word(p + 2).int() - self.dvi.offset;
                        if k < 0 {
                            k += DVI_BUF_SIZE;
                        }
                        self.dvi.buf[k as usize] += Y1 - DOWN1;
                        self.mem.set_info(p, Y_HERE);
                        hit = p;
                        break;
                    } else if case == NONE_SEEN + Z_OK
                        || case == Y_SEEN + YZ_OK
                        || case == Y_SEEN + Z_OK
                    {
                        if self.mem.word(p + 2).int() < self.dvi.gone {
                            break 'not_found;
                        }
                        // §615: change the buffered instruction to z or x.
                        let mut k = self.mem.word(p + 2).int() - self.dvi.offset;
                        if k < 0 {
                            k += DVI_BUF_SIZE;
                        }
                        self.dvi.buf[k as usize] += Z1 - DOWN1;
                        self.mem.set_info(p, Z_HERE);
                        hit = p;
                        break;
                    } else if case == NONE_SEEN + Y_HERE
                        || case == NONE_SEEN + Z_HERE
                        || case == Y_SEEN + Z_HERE
                        || case == Z_SEEN + Y_HERE
                    {
                        hit = p;
                        break;
                    }
                } else {
                    match mstate + self.mem.info(p) {
                        x if x == NONE_SEEN + Y_HERE => mstate = Y_SEEN,
                        x if x == NONE_SEEN + Z_HERE => mstate = Z_SEEN,
                        x if x == Y_SEEN + Z_HERE || x == Z_SEEN + Y_HERE => {
                            break 'not_found;
                        }
                        _ => {}
                    }
                }
                p = self.mem.link(p);
            }
        }
        if hit != NULL {
            // §611 found: generate a y0/z0 (or w0/x0) command.
            let p = hit;
            let i = self.mem.info(p);
            self.mem.set_info(q, i);
            let mut qq = q;
            if i == Y_HERE {
                self.dvi.out(o + (Y0 - DOWN1)); // y0 or w0
                while self.mem.link(qq) != p {
                    qq = self.mem.link(qq);
                    match self.mem.info(qq) {
                        YZ_OK => self.mem.set_info(qq, Z_OK),
                        Y_OK => self.mem.set_info(qq, D_FIXED),
                        _ => {}
                    }
                }
            } else {
                self.dvi.out(o + (Z0 - DOWN1)); // z0 or x0
                while self.mem.link(qq) != p {
                    qq = self.mem.link(qq);
                    match self.mem.info(qq) {
                        YZ_OK => self.mem.set_info(qq, Y_OK),
                        Z_OK => self.mem.set_info(qq, D_FIXED),
                        _ => {}
                    }
                }
            }
            return Ok(());
        }
        // §610: generate a down or right command for w.
        self.mem.set_info(q, YZ_OK);
        let mut w = w;
        if w.abs() >= 0o40000000 {
            self.dvi.out(o + 3); // down4 or right4
            self.dvi.four(w);
            return Ok(());
        }
        if w.abs() >= 0o100000 {
            self.dvi.out(o + 2); // down3 or right3
            if w < 0 {
                w += 0o100000000;
            }
            self.dvi.out((w / 0o200000) as u8);
            w %= 0o200000;
            self.dvi.out((w / 0o400) as u8);
            self.dvi.out((w % 0o400) as u8);
            return Ok(());
        }
        if w.abs() >= 0o200 {
            self.dvi.out(o + 1); // down2 or right2
            if w < 0 {
                w += 0o200000;
            }
            self.dvi.out((w / 0o400) as u8);
            self.dvi.out((w % 0o400) as u8);
            return Ok(());
        }
        self.dvi.out(o); // down1 or right1
        if w < 0 {
            w += 0o400;
        }
        self.dvi.out((w % 0o400) as u8);
        Ok(())
    }

    /// `prune_movements(l)` (§615a/§607 tail).
    fn prune_movements(&mut self, l: i32) {
        while self.dvi.down_ptr != NULL {
            if self.mem.word(self.dvi.down_ptr + 2).int() < l {
                break;
            }
            let p = self.dvi.down_ptr;
            self.dvi.down_ptr = self.mem.link(p);
            self.mem.free_node(p, MOVEMENT_NODE_SIZE);
        }
        while self.dvi.right_ptr != NULL {
            if self.mem.word(self.dvi.right_ptr + 2).int() < l {
                return;
            }
            let p = self.dvi.right_ptr;
            self.dvi.right_ptr = self.mem.link(p);
            self.mem.free_node(p, MOVEMENT_NODE_SIZE);
        }
    }

    /// `synch_h` / `synch_v` (§616).
    pub(crate) fn synch_h(&mut self) -> TexResult<()> {
        if self.dvi.cur_h != self.dvi.dvi_h {
            let d = self.dvi.cur_h - self.dvi.dvi_h;
            self.movement(d, RIGHT1)?;
            self.dvi.dvi_h = self.dvi.cur_h;
        }
        Ok(())
    }

    pub(crate) fn synch_v(&mut self) -> TexResult<()> {
        if self.dvi.cur_v != self.dvi.dvi_v {
            let d = self.dvi.cur_v - self.dvi.dvi_v;
            self.movement(d, DOWN1)?;
            self.dvi.dvi_v = self.dvi.cur_v;
        }
        Ok(())
    }

    /// `hlist_out` (§619-§625): output the hlist box `this_box`.
    /// `new_edge(s, w)` (etex.ch): creates an edge node (reusing the
    /// style-node layout, which never occurs in hlists).
    fn new_edge(&mut self, s: u16, w: Scaled) -> TexResult<Pointer> {
        let p = self.mem.get_node(crate::math::STYLE_NODE_SIZE)?;
        self.mem.set_node_type(p, crate::math::STYLE_NODE); // edge_node
        self.mem.set_subtype(p, s);
        self.mem.set_width(p, w);
        self.mem.set_depth(p, 0); // edge_dist
        Ok(p)
    }

    /// `reverse(this_box, t)` (etex.ch): reverses the hlist starting at
    /// `head`; `t` becomes the tail of the reversed list (NULL when the
    /// complete hlist is reversed).
    fn reverse(
        &mut self,
        this_box: Pointer,
        t: Pointer,
        head: Pointer,
        cur_g: &mut Scaled,
        cur_glue: &mut f64,
    ) -> TexResult<Pointer> {
        use crate::nodes::*;
        let g_order = self.mem.glue_order(this_box);
        let g_sign = self.mem.glue_sign(this_box);
        let mut l = t;
        let mut p = head;
        let mut m: i32 = 0; // unmatched math nodes (kern-converted)
        let mut n: i32 = 0; // unmatched math nodes (direction-flipped)
        loop {
            while p != NULL {
                'reswitch: loop {
                    if self.mem.is_char_node(p) {
                        loop {
                            let f = i32::from(self.mem.font(p));
                            let c = i32::from(self.mem.character(p));
                            let i = self.fonts.char_info(f, c);
                            self.dvi.cur_h += self.fonts.char_width(f, i);
                            let q = self.mem.link(p);
                            self.mem.set_link(p, l);
                            l = p;
                            p = q;
                            if !self.mem.is_char_node(p) {
                                break;
                            }
                        }
                        break 'reswitch;
                    }
                    let q = self.mem.link(p);
                    let mut rule_wd: Scaled = 0;
                    let mut add_width = true;
                    match self.mem.node_type(p) {
                        HLIST_NODE | VLIST_NODE | RULE_NODE | KERN_NODE => {
                            rule_wd = self.mem.width(p);
                        }
                        GLUE_NODE => {
                            // round_glue, as in hlist_out.
                            let g = self.mem.glue_ptr(p);
                            rule_wd = self.mem.width(g) - *cur_g;
                            if g_sign != NORMAL {
                                if g_sign == STRETCHING {
                                    if self.mem.stretch_order(g) == g_order {
                                        *cur_glue += f64::from(self.mem.stretch(g));
                                        let gt = (self.mem.glue_set(this_box) * *cur_glue)
                                            .clamp(-1e9, 1e9);
                                        *cur_g = gt.round() as Scaled;
                                    }
                                } else if self.mem.shrink_order(g) == g_order {
                                    *cur_glue -= f64::from(self.mem.shrink(g));
                                    let gt =
                                        (self.mem.glue_set(this_box) * *cur_glue).clamp(-1e9, 1e9);
                                    *cur_g = gt.round() as Scaled;
                                }
                            }
                            rule_wd += *cur_g;
                            // Handle a glue node for mixed direction text.
                            if (g_sign == STRETCHING && self.mem.stretch_order(g) == g_order)
                                || (g_sign == SHRINKING && self.mem.shrink_order(g) == g_order)
                            {
                                self.mem.delete_glue_ref(g);
                                if self.mem.subtype(p) < A_LEADERS {
                                    self.mem.set_node_type(p, KERN_NODE);
                                    self.mem.set_width(p, rule_wd);
                                } else {
                                    let ng = self.mem.get_node(crate::mem::GLUE_SPEC_SIZE)?;
                                    // orders that will never match:
                                    self.mem.set_stretch_order(ng, crate::mem::FILLL + 1);
                                    self.mem.set_shrink_order(ng, crate::mem::FILLL + 1);
                                    self.mem.set_width(ng, rule_wd);
                                    self.mem.set_stretch(ng, 0);
                                    self.mem.set_shrink(ng, 0);
                                    self.mem.set_glue_ptr(p, ng);
                                }
                            }
                        }
                        LIGATURE_NODE => {
                            // Replace the ligature by a char node.
                            let lp = self.mem.lig_ptr(p);
                            self.flush_node_list(lp);
                            let temp = p;
                            p = self.mem.get_avail()?;
                            *self.mem.word_mut(p) = self.mem.word(temp + 1); // lig_char
                            self.mem.set_link(p, q);
                            self.mem.free_node(temp, SMALL_NODE_SIZE);
                            continue 'reswitch;
                        }
                        MATH_NODE => {
                            rule_wd = self.mem.width(p);
                            if end_lr(self.mem.subtype(p)) {
                                if self.mem.info(self.lr_ptr)
                                    != i32::from(end_lr_type(self.mem.subtype(p)))
                                {
                                    self.mem.set_node_type(p, KERN_NODE);
                                    self.lr_problems += 1;
                                } else {
                                    self.pop_lr();
                                    if n > 0 {
                                        n -= 1;
                                        let st = self.mem.subtype(p);
                                        self.mem.set_subtype(p, st - 1); // after -> before
                                    } else {
                                        self.mem.set_node_type(p, KERN_NODE);
                                        if m > 0 {
                                            m -= 1;
                                        } else {
                                            // Finish the reversed segment.
                                            self.mem.free_node(p, SMALL_NODE_SIZE);
                                            self.mem.set_link(t, q);
                                            self.mem.set_width(t, rule_wd);
                                            let d = -self.dvi.cur_h - rule_wd;
                                            self.mem.set_depth(t, d); // edge_dist
                                            return Ok(l);
                                        }
                                    }
                                }
                            } else {
                                self.push_lr(p)?;
                                if n > 0 || lr_dir(self.mem.subtype(p)) != self.cur_dir {
                                    n += 1;
                                    let st = self.mem.subtype(p);
                                    self.mem.set_subtype(p, st + 1); // before -> after
                                } else {
                                    self.mem.set_node_type(p, KERN_NODE);
                                    m += 1;
                                }
                            }
                        }
                        tp if tp == crate::math::STYLE_NODE => {
                            return self.confusion("LR2").map(|_| NULL);
                        }
                        _ => {
                            add_width = false;
                        }
                    }
                    if add_width {
                        self.dvi.cur_h += rule_wd;
                    }
                    // next_p:
                    self.mem.set_link(p, l);
                    if self.mem.node_type(p) == KERN_NODE && (rule_wd == 0 || l == NULL) {
                        self.mem.free_node(p, SMALL_NODE_SIZE);
                        p = l;
                    }
                    l = p;
                    p = q;
                    break 'reswitch;
                }
            }
            if t == NULL && m == 0 && n == 0 {
                break; // done
            }
            // Manufacture one missing math node.
            let info = self.mem.info(self.lr_ptr) as u16;
            p = self.new_math(0, info)?;
            self.lr_problems += 10000;
        }
        Ok(l)
    }

    fn hlist_out(&mut self, this_box: Pointer) -> TexResult<()> {
        let mut cur_g: Scaled = 0;
        let mut cur_glue: f64 = 0.0;
        let g_order = self.mem.glue_order(this_box);
        let g_sign = self.mem.glue_sign(this_box);
        let mut p = self.mem.list_ptr(this_box);
        self.cur_s += 1;
        if self.cur_s > 0 {
            self.dvi.out(PUSH);
        }
        if self.cur_s > self.dvi.max_push {
            self.dvi.max_push = self.cur_s;
        }
        let save_loc = self.dvi.offset + self.dvi.ptr;
        let base_line = self.dvi.cur_v;
        let mut prev_p = this_box + 5; // list_offset

        // etex.ch: initialize hlist_out for mixed direction typesetting.
        if self.etex_ex() {
            self.put_lr(i32::from(crate::nodes::BEFORE))?;
            if self.mem.subtype(this_box) == crate::nodes::DLIST {
                if self.cur_dir == crate::nodes::RIGHT_TO_LEFT {
                    self.cur_dir = crate::nodes::LEFT_TO_RIGHT;
                    self.dvi.cur_h -= self.mem.width(this_box);
                } else {
                    self.mem.set_subtype(this_box, 0);
                }
            }
            if self.cur_dir == crate::nodes::RIGHT_TO_LEFT
                && self.mem.subtype(this_box) != crate::nodes::REVERSED
            {
                // Reverse the complete hlist.
                let save_h = self.dvi.cur_h;
                let head = p;
                p = self.new_kern(0)?;
                self.mem.set_link(prev_p, p);
                self.dvi.cur_h = 0;
                let rev = self.reverse(this_box, NULL, head, &mut cur_g, &mut cur_glue)?;
                self.mem.set_link(p, rev);
                let w = -self.dvi.cur_h;
                self.mem.set_width(p, w);
                self.dvi.cur_h = save_h;
                self.mem.set_subtype(this_box, crate::nodes::REVERSED);
            }
        }
        let mut left_edge = self.dvi.cur_h;
        while p != NULL {
            // §620: output node p.
            'reswitch: loop {
                if self.mem.is_char_node(p) {
                    self.synch_h()?;
                    self.synch_v()?;
                    loop {
                        prev_p = self.mem.link(prev_p); // N.B.: p may be lig_trick
                        let f = i32::from(self.mem.font(p));
                        let c = i32::from(self.mem.character(p));
                        if f != self.dvi.dvi_f {
                            // §621: change font.
                            if !self.fonts.used[f as usize] {
                                self.dvi_font_def(f);
                                self.fonts.used[f as usize] = true;
                            }
                            if f <= 64 {
                                self.dvi.out((f - 1) as u8 + FNT_NUM_0);
                            } else {
                                self.dvi.out(FNT1);
                                self.dvi.out((f - 1) as u8);
                            }
                            self.dvi.dvi_f = f;
                        }
                        let p_is_kanji = self.fonts.dir[f as usize] != 0;
                        if p_is_kanji {
                            // pTeX/upTeX DVI: the character code (from
                            // the pair's code node) goes out as set2 or
                            // set3; the metrics come from the JFM class
                            // held in c.
                            let code = self.mem.info(self.mem.link(p));
                            if code <= 0xFFFF {
                                self.dvi.out(SET1 + 1); // set2
                                self.dvi.out((code >> 8) as u8);
                                self.dvi.out(code as u8);
                            } else {
                                self.dvi.out(SET1 + 2); // set3
                                self.dvi.out((code >> 16) as u8);
                                self.dvi.out((code >> 8) as u8);
                                self.dvi.out(code as u8);
                            }
                        } else {
                            if c >= 128 {
                                self.dvi.out(SET1);
                            }
                            self.dvi.out(c as u8);
                        }
                        let i = self.fonts.char_info(f, c);
                        self.dvi.cur_h += self.fonts.char_width(f, i);
                        if p_is_kanji {
                            p = self.mem.link(p); // skip the KANJI code node
                        }
                        p = self.mem.link(p);
                        // pTeX: the implicit inter-character glue the box
                        // was measured with also moves the reference
                        // point, scaled by the box glue setting.
                        if p != NULL
                            && self.mem.is_char_node(p)
                            && p_is_kanji
                            && self.is_kanji_head(p)
                        {
                            let (ks, _xks) = self
                                .box_spacing
                                .get(&this_box)
                                .copied()
                                .unwrap_or((NULL, NULL));
                            let g = ks;
                            if g != NULL && g != self.mem.zero_glue() {
                                let mut rule_wd = self.mem.width(g) - cur_g;
                                if g_sign != NORMAL {
                                    if g_sign == STRETCHING {
                                        if self.mem.stretch_order(g) == g_order {
                                            cur_glue += f64::from(self.mem.stretch(g));
                                            let gt = (self.mem.glue_set(this_box) * cur_glue)
                                                .clamp(-1e9, 1e9);
                                            cur_g = gt.round() as Scaled;
                                        }
                                    } else if self.mem.shrink_order(g) == g_order {
                                        cur_glue -= f64::from(self.mem.shrink(g));
                                        let gt = (self.mem.glue_set(this_box) * cur_glue)
                                            .clamp(-1e9, 1e9);
                                        cur_g = gt.round() as Scaled;
                                    }
                                }
                                rule_wd += cur_g;
                                self.dvi.cur_h += rule_wd;
                            }
                        }
                        if !self.mem.is_char_node(p) {
                            break;
                        }
                    }
                    self.dvi.dvi_h = self.dvi.cur_h;
                    break 'reswitch; // p advanced; outer loop continues
                }
                // §622: the non-char_node cases.
                let mut rule_ht: Scaled = 0;
                let mut rule_dp: Scaled = 0;
                let mut rule_wd: Scaled = 0;
                let mut fin_rule = false;
                let mut move_past = false;
                match self.mem.node_type(p) {
                    HLIST_NODE | VLIST_NODE => {
                        // §623: output a box in an hlist.
                        if self.mem.list_ptr(p) == NULL {
                            self.dvi.cur_h += self.mem.width(p);
                        } else {
                            let save_h = self.dvi.dvi_h;
                            let save_v = self.dvi.dvi_v;
                            self.dvi.cur_v = base_line + self.mem.shift_amount(p);
                            // etex.ch §623: in R-text, boxes hang leftwards.
                            let edge = self.dvi.cur_h + self.mem.width(p);
                            if self.cur_dir == crate::nodes::RIGHT_TO_LEFT {
                                self.dvi.cur_h = edge;
                            }
                            if self.mem.node_type(p) == VLIST_NODE {
                                self.vlist_out(p)?;
                            } else {
                                self.hlist_out(p)?;
                            }
                            self.dvi.dvi_h = save_h;
                            self.dvi.dvi_v = save_v;
                            self.dvi.cur_h = edge;
                            self.dvi.cur_v = base_line;
                        }
                    }
                    RULE_NODE => {
                        rule_ht = self.mem.height(p);
                        rule_dp = self.mem.depth(p);
                        rule_wd = self.mem.width(p);
                        fin_rule = true;
                    }
                    WHATSIT_NODE => {
                        if self.mem.is_native_word_node(p) || self.mem.is_glyph_node(p) {
                            // xetex.web: output the whatsit node p in an
                            // hlist (set_glyphs).
                            self.synch_h()?;
                            self.synch_v()?;
                            let f = self.mem.native_font(p);
                            if f != self.dvi.dvi_f {
                                if !self.fonts.used[f as usize] {
                                    self.dvi_font_def(f);
                                    self.fonts.used[f as usize] = true;
                                }
                                if f <= 64 {
                                    self.dvi.out((f - 1) as u8 + FNT_NUM_0);
                                } else {
                                    self.dvi.out(FNT1);
                                    self.dvi.out((f - 1) as u8);
                                }
                                self.dvi.dvi_f = f;
                            }
                            if self.mem.is_glyph_node(p) {
                                self.dvi.out(SET_GLYPHS);
                                let w = self.mem.width(p);
                                self.dvi.four(w);
                                self.dvi.out(0);
                                self.dvi.out(1); // glyph count = 1
                                self.dvi.four(0); // x
                                self.dvi.four(0); // y
                                let g = self.mem.native_length(p); // native_glyph
                                self.dvi.out((g >> 8) as u8);
                                self.dvi.out(g as u8);
                            } else {
                                let w = self.mem.width(p);
                                let glyphs =
                                    self.native_glyph_infos.get(&p).cloned().unwrap_or_default();
                                self.dvi.out(SET_GLYPHS);
                                self.dvi.four(w);
                                let k = glyphs.len() as u16;
                                self.dvi.out((k >> 8) as u8);
                                self.dvi.out(k as u8);
                                for gi in &glyphs {
                                    self.dvi.four(gi.x);
                                    self.dvi.four(gi.y);
                                }
                                for gi in &glyphs {
                                    self.dvi.out((gi.gid >> 8) as u8);
                                    self.dvi.out(gi.gid as u8);
                                }
                            }
                            self.dvi.cur_h += self.mem.width(p);
                            self.dvi.dvi_h = self.dvi.cur_h;
                        } else {
                            // §1367.
                            self.out_what(p)?;
                        }
                    }
                    GLUE_NODE => {
                        // §625: move right or output leaders.
                        let g = self.mem.glue_ptr(p);
                        rule_wd = self.mem.width(g) - cur_g;
                        if g_sign != NORMAL {
                            if g_sign == STRETCHING {
                                if self.mem.stretch_order(g) == g_order {
                                    cur_glue += f64::from(self.mem.stretch(g));
                                    let mut glue_temp = self.mem.glue_set(this_box) * cur_glue;
                                    glue_temp = glue_temp.clamp(-1e9, 1e9);
                                    cur_g = glue_temp.round() as Scaled;
                                }
                            } else if self.mem.shrink_order(g) == g_order {
                                cur_glue -= f64::from(self.mem.shrink(g));
                                let mut glue_temp = self.mem.glue_set(this_box) * cur_glue;
                                glue_temp = glue_temp.clamp(-1e9, 1e9);
                                cur_g = glue_temp.round() as Scaled;
                            }
                        }
                        rule_wd += cur_g;
                        // etex.ch: convert stretched/shrunk glue to a kern
                        // (or a rigid spec, for leaders) in extended mode.
                        if self.etex_ex()
                            && ((g_sign == STRETCHING && self.mem.stretch_order(g) == g_order)
                                || (g_sign == SHRINKING && self.mem.shrink_order(g) == g_order))
                        {
                            self.mem.delete_glue_ref(g);
                            if self.mem.subtype(p) < A_LEADERS {
                                self.mem.set_node_type(p, KERN_NODE);
                                self.mem.set_width(p, rule_wd);
                            } else {
                                let ng = self.mem.get_node(crate::mem::GLUE_SPEC_SIZE)?;
                                self.mem.set_stretch_order(ng, crate::mem::FILLL + 1);
                                self.mem.set_shrink_order(ng, crate::mem::FILLL + 1);
                                self.mem.set_width(ng, rule_wd);
                                self.mem.set_stretch(ng, 0);
                                self.mem.set_shrink(ng, 0);
                                self.mem.set_glue_ptr(p, ng);
                            }
                        }
                        if self.mem.subtype(p) >= A_LEADERS {
                            // §626: output leaders in an hlist.
                            let leader_box = self.mem.leader_ptr(p);
                            if self.mem.node_type(leader_box) == RULE_NODE {
                                rule_ht = self.mem.height(leader_box);
                                rule_dp = self.mem.depth(leader_box);
                                fin_rule = true;
                            } else {
                                let leader_wd = self.mem.width(leader_box);
                                if leader_wd > 0 && rule_wd > 0 {
                                    rule_wd += 10;
                                    if self.cur_dir == crate::nodes::RIGHT_TO_LEFT {
                                        self.dvi.cur_h -= 10; // etex.ch §626
                                    }
                                    let edge = self.dvi.cur_h + rule_wd;
                                    let mut lx = 0;
                                    // §627: position of the first box.
                                    if self.mem.subtype(p) == A_LEADERS {
                                        let save_h = self.dvi.cur_h;
                                        self.dvi.cur_h = left_edge
                                            + leader_wd
                                                * ((self.dvi.cur_h - left_edge) / leader_wd);
                                        if self.dvi.cur_h < save_h {
                                            self.dvi.cur_h += leader_wd;
                                        }
                                    } else {
                                        let lq = rule_wd / leader_wd;
                                        let lr = rule_wd % leader_wd;
                                        if self.mem.subtype(p) == C_LEADERS {
                                            self.dvi.cur_h += lr / 2;
                                        } else {
                                            lx = lr / (lq + 1);
                                            self.dvi.cur_h += (lr - (lq - 1) * lx) / 2;
                                        }
                                    }
                                    while self.dvi.cur_h + leader_wd <= edge {
                                        // §628: output a leader box.
                                        self.dvi.cur_v =
                                            base_line + self.mem.shift_amount(leader_box);
                                        self.synch_v()?;
                                        let save_v = self.dvi.dvi_v;
                                        self.synch_h()?;
                                        let save_h = self.dvi.dvi_h;
                                        if self.cur_dir == crate::nodes::RIGHT_TO_LEFT {
                                            self.dvi.cur_h += leader_wd; // etex.ch §628
                                        }
                                        let outer = self.dvi.doing_leaders;
                                        self.dvi.doing_leaders = true;
                                        if self.mem.node_type(leader_box) == VLIST_NODE {
                                            self.vlist_out(leader_box)?;
                                        } else {
                                            self.hlist_out(leader_box)?;
                                        }
                                        self.dvi.doing_leaders = outer;
                                        self.dvi.dvi_v = save_v;
                                        self.dvi.dvi_h = save_h;
                                        self.dvi.cur_v = base_line;
                                        self.dvi.cur_h = save_h + leader_wd + lx;
                                    }
                                    self.dvi.cur_h = if self.cur_dir == crate::nodes::RIGHT_TO_LEFT
                                    {
                                        edge // etex.ch §626
                                    } else {
                                        edge - 10
                                    };
                                    // goto next_p
                                    fin_rule = false;
                                    move_past = false;
                                    rule_wd = 0;
                                }
                            }
                        }
                        if !fin_rule {
                            move_past = true;
                        }
                    }
                    KERN_NODE => {
                        self.dvi.cur_h += self.mem.width(p);
                    }
                    MATH_NODE => {
                        // etex.ch: adjust the LR stack; possibly reverse an
                        // hlist segment.
                        if self.etex_ex() {
                            use crate::nodes::{end_lr, end_lr_type, lr_dir};
                            if end_lr(self.mem.subtype(p)) {
                                if self.mem.info(self.lr_ptr)
                                    == i32::from(end_lr_type(self.mem.subtype(p)))
                                {
                                    self.pop_lr();
                                } else if self.mem.subtype(p) > crate::nodes::L_CODE {
                                    self.lr_problems += 1;
                                }
                            } else {
                                self.push_lr(p)?;
                                if lr_dir(self.mem.subtype(p)) != self.cur_dir {
                                    // Reverse an hlist segment.
                                    let save_h = self.dvi.cur_h;
                                    let head = self.mem.link(p);
                                    let rule_wd2 = self.mem.width(p);
                                    self.mem.free_node(p, SMALL_NODE_SIZE);
                                    self.cur_dir = 1 - self.cur_dir;
                                    p = self.new_edge(u16::from(self.cur_dir), rule_wd2)?;
                                    self.mem.set_link(prev_p, p);
                                    self.dvi.cur_h = self.dvi.cur_h - left_edge + rule_wd2;
                                    let t2 = self.new_edge(u16::from(1 - self.cur_dir), 0)?;
                                    let rev = self.reverse(
                                        this_box,
                                        t2,
                                        head,
                                        &mut cur_g,
                                        &mut cur_glue,
                                    )?;
                                    self.mem.set_link(p, rev);
                                    let d = self.dvi.cur_h;
                                    self.mem.set_depth(p, d); // edge_dist
                                    self.cur_dir = 1 - self.cur_dir;
                                    self.dvi.cur_h = save_h;
                                    continue 'reswitch;
                                }
                            }
                            self.mem.set_node_type(p, KERN_NODE);
                        }
                        self.dvi.cur_h += self.mem.width(p);
                    }
                    tp if tp == crate::math::STYLE_NODE => {
                        // etex.ch: an edge node changes the direction.
                        self.dvi.cur_h += self.mem.width(p);
                        left_edge = self.dvi.cur_h + self.mem.depth(p);
                        self.cur_dir = self.mem.subtype(p) as u8;
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
                if fin_rule {
                    // §624: output a rule in an hlist.
                    if rule_ht == NULL_FLAG {
                        rule_ht = self.mem.height(this_box);
                    }
                    if rule_dp == NULL_FLAG {
                        rule_dp = self.mem.depth(this_box);
                    }
                    rule_ht += rule_dp; // this is the rule thickness
                    if rule_ht > 0 && rule_wd > 0 {
                        self.synch_h()?;
                        self.dvi.cur_v = base_line + rule_dp;
                        self.synch_v()?;
                        self.dvi.out(SET_RULE);
                        self.dvi.four(rule_ht);
                        self.dvi.four(rule_wd);
                        self.dvi.cur_v = base_line;
                        self.dvi.dvi_h += rule_wd;
                    }
                    move_past = true;
                }
                if move_past {
                    self.dvi.cur_h += rule_wd;
                }
                prev_p = p;
                p = self.mem.link(p);
                break 'reswitch;
            }
        }
        // etex.ch: finish hlist_out for mixed direction typesetting.
        if self.etex_ex() {
            while self.mem.info(self.lr_ptr) != i32::from(crate::nodes::BEFORE) {
                if self.mem.info(self.lr_ptr) > i32::from(crate::nodes::L_CODE) {
                    self.lr_problems += 10000;
                }
                self.pop_lr();
            }
            self.pop_lr();
            if self.mem.subtype(this_box) == crate::nodes::DLIST {
                self.cur_dir = crate::nodes::RIGHT_TO_LEFT;
            }
        }
        self.prune_movements(save_loc);
        if self.cur_s > 0 {
            self.dvi.pop(save_loc);
        }
        self.cur_s -= 1;
        Ok(())
    }

    /// `vlist_out` (§629-§637): output the vlist box `this_box`.
    fn vlist_out(&mut self, this_box: Pointer) -> TexResult<()> {
        let mut cur_g: Scaled = 0;
        let mut cur_glue: f64 = 0.0;
        let g_order = self.mem.glue_order(this_box);
        let g_sign = self.mem.glue_sign(this_box);
        let mut p = self.mem.list_ptr(this_box);
        self.cur_s += 1;
        if self.cur_s > 0 {
            self.dvi.out(PUSH);
        }
        if self.cur_s > self.dvi.max_push {
            self.dvi.max_push = self.cur_s;
        }
        let save_loc = self.dvi.offset + self.dvi.ptr;
        let left_edge = self.dvi.cur_h;
        self.dvi.cur_v -= self.mem.height(this_box);
        let top_edge = self.dvi.cur_v;
        while p != NULL {
            // §630-§631.
            if self.mem.is_char_node(p) {
                return self.confusion("vlistout");
            }
            let mut rule_ht: Scaled = 0;
            let mut rule_dp: Scaled = 0;
            let mut rule_wd: Scaled = 0;
            let mut fin_rule = false;
            let mut move_past = false;
            match self.mem.node_type(p) {
                HLIST_NODE | VLIST_NODE => {
                    // §632: output a box in a vlist.
                    if self.mem.list_ptr(p) == NULL {
                        self.dvi.cur_v += self.mem.height(p) + self.mem.depth(p);
                    } else {
                        self.dvi.cur_v += self.mem.height(p);
                        self.synch_v()?;
                        let save_h = self.dvi.dvi_h;
                        let save_v = self.dvi.dvi_v;
                        // etex.ch §632: in R-text, shift the box leftwards.
                        self.dvi.cur_h = if self.cur_dir == crate::nodes::RIGHT_TO_LEFT {
                            left_edge - self.mem.shift_amount(p)
                        } else {
                            left_edge + self.mem.shift_amount(p)
                        };
                        if self.mem.node_type(p) == VLIST_NODE {
                            self.vlist_out(p)?;
                        } else {
                            self.hlist_out(p)?;
                        }
                        self.dvi.dvi_h = save_h;
                        self.dvi.dvi_v = save_v;
                        self.dvi.cur_v = save_v + self.mem.depth(p);
                        self.dvi.cur_h = left_edge;
                    }
                }
                RULE_NODE => {
                    rule_ht = self.mem.height(p);
                    rule_dp = self.mem.depth(p);
                    rule_wd = self.mem.width(p);
                    fin_rule = true;
                }
                WHATSIT_NODE => {
                    // §1366.
                    self.out_what(p)?;
                }
                GLUE_NODE => {
                    // §634: move down or output leaders.
                    let g = self.mem.glue_ptr(p);
                    rule_ht = self.mem.width(g) - cur_g;
                    if g_sign != NORMAL {
                        if g_sign == STRETCHING {
                            if self.mem.stretch_order(g) == g_order {
                                cur_glue += f64::from(self.mem.stretch(g));
                                let mut glue_temp = self.mem.glue_set(this_box) * cur_glue;
                                glue_temp = glue_temp.clamp(-1e9, 1e9);
                                cur_g = glue_temp.round() as Scaled;
                            }
                        } else if self.mem.shrink_order(g) == g_order {
                            cur_glue -= f64::from(self.mem.shrink(g));
                            let mut glue_temp = self.mem.glue_set(this_box) * cur_glue;
                            glue_temp = glue_temp.clamp(-1e9, 1e9);
                            cur_g = glue_temp.round() as Scaled;
                        }
                    }
                    rule_ht += cur_g;
                    if self.mem.subtype(p) >= A_LEADERS {
                        // §635: output leaders in a vlist.
                        let leader_box = self.mem.leader_ptr(p);
                        if self.mem.node_type(leader_box) == RULE_NODE {
                            rule_wd = self.mem.width(leader_box);
                            rule_dp = 0;
                            fin_rule = true;
                        } else {
                            let leader_ht =
                                self.mem.height(leader_box) + self.mem.depth(leader_box);
                            if leader_ht > 0 && rule_ht > 0 {
                                rule_ht += 10;
                                let edge = self.dvi.cur_v + rule_ht;
                                let mut lx = 0;
                                // §636.
                                if self.mem.subtype(p) == A_LEADERS {
                                    let save_v = self.dvi.cur_v;
                                    self.dvi.cur_v = top_edge
                                        + leader_ht * ((self.dvi.cur_v - top_edge) / leader_ht);
                                    if self.dvi.cur_v < save_v {
                                        self.dvi.cur_v += leader_ht;
                                    }
                                } else {
                                    let lq = rule_ht / leader_ht;
                                    let lr = rule_ht % leader_ht;
                                    if self.mem.subtype(p) == C_LEADERS {
                                        self.dvi.cur_v += lr / 2;
                                    } else {
                                        lx = lr / (lq + 1);
                                        self.dvi.cur_v += (lr - (lq - 1) * lx) / 2;
                                    }
                                }
                                while self.dvi.cur_v + leader_ht <= edge {
                                    // §637.
                                    // etex.ch §637: leader shift and cur_dir.
                                    self.dvi.cur_h = if self.cur_dir == crate::nodes::RIGHT_TO_LEFT
                                    {
                                        left_edge - self.mem.shift_amount(leader_box)
                                    } else {
                                        left_edge + self.mem.shift_amount(leader_box)
                                    };
                                    self.synch_h()?;
                                    let save_h = self.dvi.dvi_h;
                                    self.dvi.cur_v += self.mem.height(leader_box);
                                    self.synch_v()?;
                                    let save_v = self.dvi.dvi_v;
                                    let outer = self.dvi.doing_leaders;
                                    self.dvi.doing_leaders = true;
                                    if self.mem.node_type(leader_box) == VLIST_NODE {
                                        self.vlist_out(leader_box)?;
                                    } else {
                                        self.hlist_out(leader_box)?;
                                    }
                                    self.dvi.doing_leaders = outer;
                                    self.dvi.dvi_v = save_v;
                                    self.dvi.dvi_h = save_h;
                                    self.dvi.cur_h = left_edge;
                                    self.dvi.cur_v =
                                        save_v - self.mem.height(leader_box) + leader_ht + lx;
                                }
                                self.dvi.cur_v = edge - 10;
                                fin_rule = false;
                                move_past = false;
                                rule_ht = 0;
                            }
                        }
                    }
                    if !fin_rule {
                        move_past = true;
                    }
                }
                KERN_NODE => {
                    self.dvi.cur_v += self.mem.width(p);
                }
                _ => {}
            }
            if fin_rule {
                // §633: output a rule in a vlist.
                if rule_wd == NULL_FLAG {
                    rule_wd = self.mem.width(this_box);
                }
                rule_ht += rule_dp; // this is the rule thickness
                self.dvi.cur_v += rule_ht;
                if rule_ht > 0 && rule_wd > 0 {
                    // etex.ch §633: in R-text the rule hangs leftwards.
                    if self.cur_dir == crate::nodes::RIGHT_TO_LEFT {
                        self.dvi.cur_h -= rule_wd;
                    }
                    self.synch_h()?;
                    self.synch_v()?;
                    self.dvi.out(PUT_RULE);
                    self.dvi.four(rule_ht);
                    self.dvi.four(rule_wd);
                    self.dvi.cur_h = left_edge;
                }
                move_past = false;
            }
            if move_past {
                self.dvi.cur_v += rule_ht;
            }
            p = self.mem.link(p);
        }
        self.prune_movements(save_loc);
        if self.cur_s > 0 {
            self.dvi.pop(save_loc);
        }
        self.cur_s -= 1;
        Ok(())
    }

    /// `ship_out(p)` (§638-§640): output box `p` as a DVI page.
    pub fn ship_out(&mut self, p: Pointer) -> TexResult<()> {
        if self.eqtb.int_par(crate::eqtb::TRACING_OUTPUT_CODE) > 0 {
            self.print_nl_chars("");
            self.print_ln();
            self.print_chars("Completed box being shipped out");
        }
        if self.prn.term_offset > self.sizes.max_print_line - 9 {
            self.print_ln();
        } else if self.prn.term_offset > 0 || self.prn.file_offset > 0 {
            self.print_char(' ' as i32);
        }
        self.print_char('[' as i32);
        let mut j = 9;
        while j > 0 && self.eqtb.count(j) == 0 {
            j -= 1;
        }
        for k in 0..=j {
            let c = self.eqtb.count(k);
            self.print_int(c);
            if k < j {
                self.print_char('.' as i32);
            }
        }
        if self.eqtb.int_par(crate::eqtb::TRACING_OUTPUT_CODE) > 0 {
            self.print_char(']' as i32);
            self.begin_diagnostic();
            self.show_box(p);
            self.end_diagnostic(true);
        }
        // §640: ship box p out.
        'done: {
            // §641: update max_h/max_v, reject huge pages.
            let v_offset = self.eqtb.dimen_par(crate::eqtb::V_OFFSET_CODE);
            let h_offset = self.eqtb.dimen_par(crate::eqtb::H_OFFSET_CODE);
            let max_dimen = crate::scan::MAX_DIMEN;
            if self.mem.height(p) > max_dimen
                || self.mem.depth(p) > max_dimen
                || self.mem.height(p) + self.mem.depth(p) + v_offset > max_dimen
                || self.mem.width(p) + h_offset > max_dimen
            {
                self.print_err("Huge page cannot be shipped out");
                self.help(&[
                    "The page just created is more than 18 feet tall or",
                    "more than 18 feet wide, so I suspect something went wrong.",
                ]);
                self.error()?;
                if self.eqtb.int_par(crate::eqtb::TRACING_OUTPUT_CODE) <= 0 {
                    self.begin_diagnostic();
                    self.print_nl_chars("The following box has been deleted:");
                    self.show_box(p);
                    self.end_diagnostic(true);
                }
                break 'done;
            }
            if self.mem.height(p) + self.mem.depth(p) + v_offset > self.dvi.max_v {
                self.dvi.max_v = self.mem.height(p) + self.mem.depth(p) + v_offset;
            }
            if self.mem.width(p) + h_offset > self.dvi.max_h {
                self.dvi.max_h = self.mem.width(p) + h_offset;
            }
            // §617: initialize variables as ship_out begins.
            self.dvi.dvi_h = 0;
            self.dvi.dvi_v = 0;
            self.dvi.cur_h = h_offset;
            self.dvi.dvi_f = crate::fonts::NULL_FONT;
            if self.job_name.is_none() {
                self.open_log_file()?;
            }
            if self.total_pages == 0 {
                self.dvi.out(PRE);
                let idb = self.dvi_id_byte();
                self.dvi.out(idb);
                self.dvi.four(25_400_000);
                self.dvi.four(473_628_672); // conversion ratio for sp
                self.prepare_mag()?;
                let mag = self.eqtb.int_par(crate::eqtb::MAG_CODE);
                self.dvi.four(mag);
                let old_setting = self.prn.selector;
                self.prn.selector = crate::print::NEW_STRING;
                self.print_chars(" TeX output ");
                let year = self.eqtb.int_par(crate::eqtb::YEAR_CODE);
                self.print_int(year);
                self.print_char('.' as i32);
                let month = self.eqtb.int_par(crate::eqtb::MONTH_CODE);
                self.print_two(month);
                self.print_char('.' as i32);
                let day = self.eqtb.int_par(crate::eqtb::DAY_CODE);
                self.print_two(day);
                self.print_char(':' as i32);
                let time = self.eqtb.int_par(crate::eqtb::TIME_CODE);
                self.print_two(time / 60);
                self.print_two(time % 60);
                self.prn.selector = old_setting;
                self.dvi.out(self.strings.cur_length() as u8);
                let units: Vec<u16> = self.strings.cur_str().to_vec();
                for u in units {
                    self.dvi.out(u as u8);
                }
                let b = self.strings.pool_ptr() - self.strings.cur_length();
                self.strings.pool_truncate(b); // flush the current string
            }
            let page_loc = self.dvi.offset + self.dvi.ptr;
            self.dvi.out(BOP);
            for k in 0..=9 {
                let c = self.eqtb.count(k);
                self.dvi.four(c);
            }
            let lb = self.dvi.last_bop;
            self.dvi.four(lb);
            self.dvi.last_bop = page_loc;
            self.dvi.cur_v = self.mem.height(p) + v_offset;
            if self.mem.node_type(p) == VLIST_NODE {
                self.vlist_out(p)?;
            } else {
                self.hlist_out(p)?;
            }
            self.dvi.out(EOP);
            self.total_pages += 1;
            self.cur_s = -1;
        }
        // etex.ch: check for LR anomalies at the end of ship_out.
        if self.etex_ex() {
            if self.lr_problems > 0 {
                self.print_ln();
                self.print_nl_chars("\\endL or \\endR problem (");
                let miss = self.lr_problems / 10000;
                self.print_int(miss);
                self.print_chars(" missing, ");
                let extra = self.lr_problems % 10000;
                self.print_int(extra);
                self.print_chars(" extra");
                self.lr_problems = 0;
                self.print_char(')' as i32);
                self.print_ln();
            }
            if self.lr_ptr != NULL || self.cur_dir != crate::nodes::LEFT_TO_RIGHT {
                let mut q = self.lr_ptr;
                let mut infos = Vec::new();
                while q != NULL {
                    infos.push(self.mem.info(q));
                    q = self.mem.link(q);
                }
                eprintln!("LR3 debug: cur_dir={} lr stack={infos:?}", self.cur_dir);
                self.confusion("LR3")?;
            }
        }
        if self.eqtb.int_par(crate::eqtb::TRACING_OUTPUT_CODE) <= 0 {
            self.print_char(']' as i32);
        }
        self.dead_cycles = 0;
        // §639: flush the box from memory, showing statistics if requested.
        let stats = self.eqtb.int_par(crate::eqtb::TRACING_STATS_CODE) > 1;
        if stats {
            self.print_nl_chars("Memory usage before: ");
            let vu = self.mem.var_used;
            self.print_int(vu);
            self.print_char('&' as i32);
            let du = self.mem.dyn_used;
            self.print_int(du);
            self.print_char(';' as i32);
        }
        self.flush_node_list(p);
        if stats {
            self.print_chars(" after: ");
            let vu = self.mem.var_used;
            self.print_int(vu);
            self.print_char('&' as i32);
            let du = self.mem.dyn_used;
            self.print_int(du);
            self.print_chars("; still untouched: ");
            let untouched = self.mem.hi_mem_min - self.mem.lo_mem_max - 1;
            self.print_int(untouched);
            self.print_ln();
        }
        Ok(())
    }

    /// §642-§643: finish the DVI file and hand it to the host.
    pub fn finish_dvi(&mut self) -> TexResult<()> {
        while self.cur_s > -1 {
            if self.cur_s > 0 {
                self.dvi.out(POP);
            } else {
                self.dvi.out(EOP);
                self.total_pages += 1;
            }
            self.cur_s -= 1;
        }
        if self.total_pages == 0 {
            self.print_nl_chars("No pages of output.");
            return Ok(());
        }
        self.dvi.out(POST); // beginning of the postamble
        let lb = self.dvi.last_bop;
        self.dvi.four(lb);
        self.dvi.last_bop = self.dvi.offset + self.dvi.ptr - 5;
        self.dvi.four(25_400_000);
        self.dvi.four(473_628_672);
        self.prepare_mag()?;
        let mag = self.eqtb.int_par(crate::eqtb::MAG_CODE);
        self.dvi.four(mag);
        let (mv, mh) = (self.dvi.max_v, self.dvi.max_h);
        self.dvi.four(mv);
        self.dvi.four(mh);
        let mp = self.dvi.max_push;
        self.dvi.out((mp / 256) as u8);
        self.dvi.out((mp % 256) as u8);
        let tp = self.total_pages;
        self.dvi.out(((tp / 256) % 256) as u8);
        self.dvi.out((tp % 256) as u8);
        // §643: font definitions for all fonts that were used.
        let mut f = self.fonts.font_ptr;
        while f > 0 {
            if self.fonts.used[f as usize] {
                self.dvi_font_def(f);
            }
            f -= 1;
        }
        self.dvi.out(POST_POST);
        let lb = self.dvi.last_bop;
        self.dvi.four(lb);
        let idb = self.dvi_id_byte();
        self.dvi.out(idb);
        let mut k = 4 + (DVI_BUF_SIZE - self.dvi.ptr) % 4; // the number of 223's
        while k > 0 {
            self.dvi.out(223);
            k -= 1;
        }
        self.dvi.flush_buffer();
        // Hand the file to the host.
        let name = format!(
            "{}.dvi",
            self.job_name.clone().unwrap_or_else(|| "texput".into())
        );
        let data = std::mem::take(&mut self.dvi.file);
        self.fs.write_file(&name, crate::io::OutKind::Dvi, &data);
        self.print_nl_chars("Output written on ");
        self.print_chars(&name);
        self.print_chars(" (");
        let tp = self.total_pages;
        self.print_int(tp);
        self.print_chars(" page");
        if tp != 1 {
            self.print_char('s' as i32);
        }
        self.print_chars(", ");
        self.print_int(data.len() as i32);
        self.print_chars(" bytes).");
        Ok(())
    }
}

impl Default for DviState {
    fn default() -> Self {
        Self::new()
    }
}
