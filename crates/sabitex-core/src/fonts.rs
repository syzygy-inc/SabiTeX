//! Font metric data.
//!
//! Ports tex.web Part 30 (§539-§582): the `font_info` memory, the per-font
//! directory arrays, the TFM reader (`read_font_info`), the font-parameter
//! accessors and `new_character`. TFM files arrive as whole byte slices
//! from `TexFs`; the §560 `fget`/`fbyte` cursor becomes an index.
//!
//! Deviation (recorded): `font_name`/`font_area` are host `String`s rather
//! than pool strings; all uses (diagnostics, DVI font definitions) print
//! identical bytes.

use crate::engine::Engine;
use crate::error::{TexInterrupt, TexResult};
use crate::memword::MemoryWord;
use crate::types::{Pointer, Scaled, NULL};

// §544: tag values.
pub const NO_TAG: u16 = 0;
pub const LIG_TAG: u16 = 1;
pub const LIST_TAG: u16 = 2;
pub const EXT_TAG: u16 = 3;

// §545: lig/kern program opcodes.
pub const STOP_FLAG: u16 = 128;
pub const KERN_FLAG: u16 = 128;

/// `kern_base_offset` (§557).
pub const KERN_BASE_OFFSET: i32 = 256 * 128;

/// `non_char` (§549): a code that can't match a real (TFM) character.
pub const NON_CHAR: i32 = 256;
/// `non_address` (§549).
pub const NON_ADDRESS: i32 = 0;

// §547: font parameter codes.
pub const SLANT_CODE: i32 = 1;
pub const SPACE_CODE: i32 = 2;
pub const SPACE_STRETCH_CODE: i32 = 3;
pub const SPACE_SHRINK_CODE: i32 = 4;
pub const X_HEIGHT_CODE: i32 = 5;
pub const QUAD_CODE: i32 = 6;
pub const EXTRA_SPACE_CODE: i32 = 7;

/// `null_font == font_base == 0`.
pub const NULL_FONT: i32 = 0;

/// The font arrays of §549-§550.
pub struct FontMem {
    /// `font_info`: the big collection of font data.
    pub info: Vec<MemoryWord>,
    /// `fmem_ptr`: first unused word of `font_info`.
    pub fmem_ptr: i32,
    /// `font_ptr`: largest internal font number in use.
    pub font_ptr: i32,
    pub font_mem_size: i32,
    pub font_max: i32,
    // Per-font arrays, indexed by internal font number.
    pub check: Vec<MemoryWord>,
    pub size: Vec<Scaled>,
    pub dsize: Vec<Scaled>,
    pub params: Vec<i32>,
    pub name: Vec<String>,
    pub area: Vec<String>,
    pub bc: Vec<i32>,
    pub ec: Vec<i32>,
    pub glue: Vec<Pointer>,
    pub used: Vec<bool>,
    pub hyphen_char: Vec<i32>,
    pub skew_char: Vec<i32>,
    pub bchar_label: Vec<i32>,
    pub bchar: Vec<i32>,
    pub false_bchar: Vec<i32>,
    pub char_base: Vec<i32>,
    pub width_base: Vec<i32>,
    pub height_base: Vec<i32>,
    pub depth_base: Vec<i32>,
    pub italic_base: Vec<i32>,
    pub lig_kern_base: Vec<i32>,
    pub kern_base: Vec<i32>,
    pub exten_base: Vec<i32>,
    pub param_base: Vec<i32>,
    /// XeTeX: the native (OpenType) instance behind font f, if any
    /// (None for TFM fonts). Replaces font_layout_engine + the
    /// aat/otgr font_area flag values.
    pub native: Vec<Option<Box<crate::native::NativeFont>>>,
    /// pTeX `font_dir`: 0 = default (alphabetic), 1 = yoko, 2 = tate.
    pub dir: Vec<u8>,
    /// pTeX `font_num_ext`: number of char_type entries (JFM `nt`).
    pub num_ext: Vec<i32>,
    /// pTeX `ctype_base`: start of the char_type table in `info`.
    pub ctype_base: Vec<i32>,
}

impl FontMem {
    /// §552: initialize with the null font.
    pub fn new(font_mem_size: usize, font_max: i32) -> FontMem {
        let n = font_max as usize + 1;
        let mut fm = FontMem {
            info: vec![MemoryWord::ZERO; font_mem_size],
            fmem_ptr: 7,
            font_ptr: NULL_FONT,
            font_mem_size: font_mem_size as i32,
            font_max,
            check: vec![MemoryWord::ZERO; n],
            size: vec![0; n],
            dsize: vec![0; n],
            params: vec![0; n],
            name: vec![String::new(); n],
            area: vec![String::new(); n],
            bc: vec![0; n],
            ec: vec![0; n],
            glue: vec![NULL; n],
            used: vec![false; n],
            hyphen_char: vec![0; n],
            skew_char: vec![0; n],
            bchar_label: vec![NON_ADDRESS; n],
            bchar: vec![NON_CHAR; n],
            false_bchar: vec![NON_CHAR; n],
            char_base: vec![0; n],
            width_base: vec![0; n],
            height_base: vec![0; n],
            depth_base: vec![0; n],
            italic_base: vec![0; n],
            lig_kern_base: vec![0; n],
            kern_base: vec![0; n],
            exten_base: vec![0; n],
            param_base: vec![0; n],
            native: std::iter::repeat_with(|| None).take(n).collect(),
            dir: vec![0; n],
            num_ext: vec![0; n],
            ctype_base: vec![0; n],
        };
        let f = NULL_FONT as usize;
        fm.name[f] = "nullfont".to_string();
        fm.hyphen_char[f] = '-' as i32;
        fm.skew_char[f] = -1;
        fm.bc[f] = 1;
        fm.ec[f] = 0;
        fm.params[f] = 7;
        fm.param_base[f] = -1;
        fm
    }

    /// §1320-§1322: dump the font information.
    pub fn dump(&self, w: &mut crate::fmt::FmtWriter) {
        // §1320: only the used part of font_info travels.
        w.words(&self.info[..self.fmem_ptr as usize]);
        w.i32(self.fmem_ptr);
        w.i32(self.font_ptr);
        // Only slots 0..=font_ptr are meaningful; font_max is ~9000.
        let fp = self.font_ptr as usize;
        w.words(&self.check[..=fp]);
        w.i32s(&self.size[..=fp]);
        w.i32s(&self.dsize[..=fp]);
        w.i32s(&self.params[..=fp]);
        w.len_of(fp + 1);
        for s in &self.name[..=fp] {
            w.str(s);
        }
        w.len_of(fp + 1);
        for s in &self.area[..=fp] {
            w.str(s);
        }
        w.i32s(&self.bc[..=fp]);
        w.i32s(&self.ec[..=fp]);
        w.i32s(&self.glue[..=fp]);
        w.i32s(&self.hyphen_char[..=fp]);
        w.i32s(&self.skew_char[..=fp]);
        w.i32s(&self.bchar_label[..=fp]);
        w.i32s(&self.bchar[..=fp]);
        w.i32s(&self.false_bchar[..=fp]);
        w.i32s(&self.char_base[..=fp]);
        w.i32s(&self.width_base[..=fp]);
        w.i32s(&self.height_base[..=fp]);
        w.i32s(&self.depth_base[..=fp]);
        w.i32s(&self.italic_base[..=fp]);
        w.i32s(&self.lig_kern_base[..=fp]);
        w.i32s(&self.kern_base[..=fp]);
        w.i32s(&self.exten_base[..=fp]);
        w.i32s(&self.param_base[..=fp]);
    }

    /// §1321-§1323: undump the font information.
    pub fn undump(
        &mut self,
        r: &mut crate::fmt::FmtReader,
        pristine: bool,
    ) -> crate::fmt::FmtResult<()> {
        let info = r.words()?;
        self.fmem_ptr = r.i32()?;
        if info.len() != self.fmem_ptr as usize || info.len() > self.info.len() {
            return Err("font mem size mismatch");
        }
        if !pristine {
            self.info.fill(crate::memword::MemoryWord::default());
        }
        self.info[..info.len()].copy_from_slice(&info);
        self.font_ptr = r.i32()?;
        let fp = self.font_ptr as usize;
        if fp > self.font_max as usize {
            return Err("font_ptr exceeds font_max");
        }
        fn splice<T: Clone + Default>(dst: &mut [T], src: Vec<T>) -> crate::fmt::FmtResult<()> {
            if src.len() > dst.len() {
                return Err("font array too long");
            }
            for v in dst.iter_mut() {
                *v = T::default();
            }
            dst[..src.len()].clone_from_slice(&src);
            Ok(())
        }
        let check = r.words()?;
        if check.len() != fp + 1 {
            return Err("font check size mismatch");
        }
        splice(&mut self.check, check)?;
        splice(&mut self.size, r.i32s()?)?;
        splice(&mut self.dsize, r.i32s()?)?;
        splice(&mut self.params, r.i32s()?)?;
        let n = r.seq_len()?;
        let mut name = Vec::with_capacity(n);
        for _ in 0..n {
            name.push(r.str()?);
        }
        splice(&mut self.name, name)?;
        let n = r.seq_len()?;
        let mut area = Vec::with_capacity(n);
        for _ in 0..n {
            area.push(r.str()?);
        }
        splice(&mut self.area, area)?;
        splice(&mut self.bc, r.i32s()?)?;
        splice(&mut self.ec, r.i32s()?)?;
        splice(&mut self.glue, r.i32s()?)?;
        splice(&mut self.hyphen_char, r.i32s()?)?;
        splice(&mut self.skew_char, r.i32s()?)?;
        splice(&mut self.bchar_label, r.i32s()?)?;
        splice(&mut self.bchar, r.i32s()?)?;
        splice(&mut self.false_bchar, r.i32s()?)?;
        splice(&mut self.char_base, r.i32s()?)?;
        splice(&mut self.width_base, r.i32s()?)?;
        splice(&mut self.height_base, r.i32s()?)?;
        splice(&mut self.depth_base, r.i32s()?)?;
        splice(&mut self.italic_base, r.i32s()?)?;
        splice(&mut self.lig_kern_base, r.i32s()?)?;
        splice(&mut self.kern_base, r.i32s()?)?;
        splice(&mut self.exten_base, r.i32s()?)?;
        splice(&mut self.param_base, r.i32s()?)?;
        // The dump preserves `used` as all-false: a fresh run re-defines
        // fonts in the DVI as they are first shipped out (§1322 does the
        // same by clearing font_used).
        self.used = vec![false; self.font_max as usize + 1];
        Ok(())
    }

    /// `char_info(f)(c)` (§554): the four_quarters word for character `c`.
    pub fn char_info(&self, f: i32, c: i32) -> MemoryWord {
        self.info[(self.char_base[f as usize] + c) as usize]
    }

    /// `char_exists(q)` (§554).
    pub fn char_exists(q: MemoryWord) -> bool {
        q.qqqq(0) > 0
    }

    /// `char_width(f)(q)` (§554).
    pub fn char_width(&self, f: i32, q: MemoryWord) -> Scaled {
        self.info[(self.width_base[f as usize] + i32::from(q.qqqq(0))) as usize].sc()
    }

    /// `char_italic(f)(q)` (§554).
    pub fn char_italic(&self, f: i32, q: MemoryWord) -> Scaled {
        self.info[(self.italic_base[f as usize] + i32::from(q.qqqq(2)) / 4) as usize].sc()
    }

    /// `height_depth(q)` (§554).
    pub fn height_depth(q: MemoryWord) -> i32 {
        i32::from(q.qqqq(1))
    }

    /// `char_height(f)(b)` (§554).
    pub fn char_height(&self, f: i32, b: i32) -> Scaled {
        self.info[(self.height_base[f as usize] + b / 16) as usize].sc()
    }

    /// `char_depth(f)(b)` (§554).
    pub fn char_depth(&self, f: i32, b: i32) -> Scaled {
        self.info[(self.depth_base[f as usize] + b % 16) as usize].sc()
    }

    /// `char_tag(q)` (§554).
    pub fn char_tag(q: MemoryWord) -> u16 {
        q.qqqq(2) % 4
    }

    /// `skip_byte`/`next_char`/`op_byte`/`rem_byte` of a lig/kern command
    /// word (§545).
    pub fn skip_byte(i: MemoryWord) -> u16 {
        i.qqqq(0)
    }

    pub fn next_char(i: MemoryWord) -> u16 {
        i.qqqq(1)
    }

    pub fn op_byte(i: MemoryWord) -> u16 {
        i.qqqq(2)
    }

    pub fn rem_byte(i: MemoryWord) -> u16 {
        i.qqqq(3)
    }

    /// `char_kern(f)(j)` (§557).
    pub fn char_kern(&self, f: i32, j: MemoryWord) -> Scaled {
        let idx = self.kern_base[f as usize]
            + 256 * i32::from(Self::op_byte(j))
            + i32::from(Self::rem_byte(j));
        self.info[idx as usize].sc()
    }

    /// `lig_kern_start(f)(j)` (§557).
    pub fn lig_kern_start(&self, f: i32, j: MemoryWord) -> i32 {
        self.lig_kern_base[f as usize] + i32::from(Self::rem_byte(j))
    }

    /// `lig_kern_restart(f)(i)` (§557).
    pub fn lig_kern_restart(&self, f: i32, i: MemoryWord) -> i32 {
        self.lig_kern_base[f as usize]
            + 256 * i32::from(Self::op_byte(i))
            + i32::from(Self::rem_byte(i))
            + 32768
            - KERN_BASE_OFFSET
    }

    /// `param(n)(f)` (§558).
    pub fn param(&self, n: i32, f: i32) -> Scaled {
        self.info[(self.param_base[f as usize] + n) as usize].sc()
    }

    pub fn set_param(&mut self, n: i32, f: i32, v: Scaled) {
        let idx = (self.param_base[f as usize] + n) as usize;
        self.info[idx].set_sc(v);
    }

    pub fn space(&self, f: i32) -> Scaled {
        self.param(SPACE_CODE, f)
    }

    pub fn space_stretch(&self, f: i32) -> Scaled {
        self.param(SPACE_STRETCH_CODE, f)
    }

    pub fn space_shrink(&self, f: i32) -> Scaled {
        self.param(SPACE_SHRINK_CODE, f)
    }

    pub fn x_height(&self, f: i32) -> Scaled {
        self.param(X_HEIGHT_CODE, f)
    }

    pub fn quad(&self, f: i32) -> Scaled {
        self.param(QUAD_CODE, f)
    }

    pub fn extra_space(&self, f: i32) -> Scaled {
        self.param(EXTRA_SPACE_CODE, f)
    }
}

/// Byte cursor over a TFM file with the §560 abort-on-EOF discipline.
struct TfmCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl TfmCursor<'_> {
    fn byte(&mut self) -> Result<i32, ()> {
        let b = *self.bytes.get(self.pos).ok_or(())?;
        self.pos += 1;
        Ok(i32::from(b))
    }

    /// `read_sixteen` (§560).
    fn sixteen(&mut self) -> Result<i32, ()> {
        let a = self.byte()?;
        if a > 127 {
            return Err(());
        }
        let b = self.byte()?;
        Ok(a * 256 + b)
    }

    /// `store_four_quarters` (§560): returns (word, a, b, c, d).
    fn four_quarters(&mut self) -> Result<(MemoryWord, i32, i32, i32, i32), ()> {
        let a = self.byte()?;
        let b = self.byte()?;
        let c = self.byte()?;
        let d = self.byte()?;
        let mut qw = MemoryWord::ZERO;
        qw.set_qqqq(0, a as u16);
        qw.set_qqqq(1, b as u16);
        qw.set_qqqq(2, c as u16);
        qw.set_qqqq(3, d as u16);
        Ok((qw, a, b, c, d))
    }

    /// `store_scaled` (§571): exact fix_word × z scaling.
    fn scaled(&mut self, z: Scaled, alpha: i32, beta: i32) -> Result<Scaled, ()> {
        let a = self.byte()?;
        let b = self.byte()?;
        let c = self.byte()?;
        let d = self.byte()?;
        let sw = (((d * z) / 0o400 + c * z) / 0o400 + b * z) / beta;
        if a == 0 {
            Ok(sw)
        } else if a == 255 {
            Ok(sw - alpha)
        } else {
            Err(())
        }
    }
}

impl Engine {
    /// `read_font_info(u, nom, aire, s)` (§560-§576): loads a TFM file,
    /// returning the internal font number (`null_font` on failure).
    /// `get_jfm_pos(kcode, f)` (ptex-base.ch): the JFM char_type class
    /// for a character, via binary search of the ctype table (entry 0 is
    /// the default class).
    pub fn get_jfm_pos(&self, kcode: i32, f: i32) -> i32 {
        if f == NULL_FONT {
            return 0;
        }
        let fu = f as usize;
        let base = self.fonts.ctype_base[fu];
        let nt = self.fonts.num_ext[fu];
        let code = |i: i32| self.fonts.info[(base + i) as usize].rh();
        let ty = |i: i32| self.fonts.info[(base + i) as usize].lh();
        let (mut sp, mut ep) = (1, nt - 1);
        if ep >= 1 && code(sp) <= kcode && kcode <= code(ep) {
            while sp <= ep {
                let mp = sp + (ep - sp) / 2;
                if kcode < code(mp) {
                    ep = mp - 1;
                } else if kcode > code(mp) {
                    sp = mp + 1;
                } else {
                    return ty(mp);
                }
            }
        }
        if nt > 0 {
            ty(0)
        } else {
            0
        }
    }

    pub fn read_font_info(
        &mut self,
        u: Pointer,
        nom: &str,
        aire: &str,
        s: Scaled,
    ) -> TexResult<i32> {
        // §563: open tfm_file.
        let file_name = if aire.is_empty() {
            format!("{nom}.tfm")
        } else {
            format!("{aire}{nom}.tfm")
        };
        let bytes = self.fs.read_file(&file_name, crate::io::FileKind::Tfm);
        let file_opened = bytes.is_some();
        let g = match bytes {
            None => Err(()),
            Some(bytes) => self.load_tfm(&bytes, s),
        };
        match g {
            Ok(f) => {
                // §576 tail: name/area, defaults.
                self.fonts.name[f as usize] = nom.to_string();
                self.fonts.area[f as usize] = aire.to_string();
                self.fonts.hyphen_char[f as usize] =
                    self.eqtb.int_par(crate::eqtb::DEFAULT_HYPHEN_CHAR_CODE);
                self.fonts.skew_char[f as usize] =
                    self.eqtb.int_par(crate::eqtb::DEFAULT_SKEW_CHAR_CODE);
                Ok(f)
            }
            Err(()) => {
                // §561: report that the font won't be loaded.
                self.print_err("Font ");
                self.sprint_cs(u);
                self.print_char('=' as i32);
                self.print_chars(nom);
                if s >= 0 {
                    self.print_chars(" at ");
                    self.print_scaled(s);
                    self.print_chars("pt");
                } else if s != -1000 {
                    self.print_chars(" scaled ");
                    self.print_int(-s);
                }
                if file_opened {
                    self.print_chars(" not loadable: Bad metric (TFM) file");
                } else {
                    self.print_chars(" not loadable: Metric (TFM) file not found");
                }
                self.help(&[
                    "I wasn't able to read the size data for this font,",
                    "so I will ignore the font specification.",
                    "[Wizards can fix TFM files using TFtoPL/PLtoTF.]",
                    "You might try inserting a different font spec;",
                    "e.g., type `I\\font<same font id>=<substitute font name>'.",
                ]);
                self.error()?;
                Ok(NULL_FONT)
            }
        }
    }

    /// §562-§576: the body of `read_font_info` once the bytes are in hand.
    /// `Err(())` is tex.web's `abort` (bad TFM).
    fn load_tfm(&mut self, bytes: &[u8], s: Scaled) -> Result<i32, ()> {
        let t = &mut TfmCursor { bytes, pos: 0 };
        // §565 (+ ptex-base.ch): read the TFM size fields; a JFM starts
        // with id (11 = yoko, 9 = tate) and nt instead.
        let mut lf = t.sixteen()?;
        let mut lh = t.sixteen()?;
        let mut jfm_dir: u8 = 0;
        let mut nt: i32 = 0;
        if lf == 11 || lf == 9 {
            jfm_dir = if lf == 11 { 1 } else { 2 };
            nt = lh;
            lf = t.sixteen()?;
            lh = t.sixteen()?;
        }
        let mut bc = t.sixteen()?;
        let mut ec = t.sixteen()?;
        if bc > ec + 1 || ec > 255 {
            return Err(());
        }
        if bc > 255 {
            bc = 1;
            ec = 0;
        }
        let nw = t.sixteen()?;
        let nh = t.sixteen()?;
        let nd = t.sixteen()?;
        let ni = t.sixteen()?;
        let nl = t.sixteen()?;
        let nk = t.sixteen()?;
        let ne = t.sixteen()?;
        let np = t.sixteen()?;
        let header = if jfm_dir != 0 { 7 + nt } else { 6 };
        if lf != header + lh + (ec - bc + 1) + nw + nh + nd + ni + nl + nk + ne + np {
            return Err(());
        }
        if nw == 0 || nh == 0 || nd == 0 || ni == 0 {
            return Err(());
        }
        // §566 (+ ptex-base.ch): use size fields to allocate font info.
        let mut lf = lf - header - lh + if jfm_dir != 0 { nt } else { 0 };
        if np < 7 {
            lf += 7 - np;
        }
        if self.fonts.font_ptr == self.fonts.font_max
            || self.fonts.fmem_ptr + lf > self.fonts.font_mem_size
        {
            // §567: not enough room (reported by the caller as bad TFM
            // would be wrong; tex prints its own message — simplified).
            return Err(());
        }
        let f = (self.fonts.font_ptr + 1) as usize;
        let fmem_ptr = self.fonts.fmem_ptr;
        self.fonts.dir[f] = jfm_dir;
        self.fonts.num_ext[f] = nt;
        self.fonts.ctype_base[f] = fmem_ptr;
        self.fonts.char_base[f] = fmem_ptr + nt - bc;
        self.fonts.width_base[f] = self.fonts.char_base[f] + ec + 1;
        self.fonts.height_base[f] = self.fonts.width_base[f] + nw;
        self.fonts.depth_base[f] = self.fonts.height_base[f] + nh;
        self.fonts.italic_base[f] = self.fonts.depth_base[f] + nd;
        self.fonts.lig_kern_base[f] = self.fonts.italic_base[f] + ni;
        self.fonts.kern_base[f] = self.fonts.lig_kern_base[f] + nl - KERN_BASE_OFFSET;
        self.fonts.exten_base[f] = self.fonts.kern_base[f] + KERN_BASE_OFFSET + nk;
        self.fonts.param_base[f] = self.fonts.exten_base[f] + ne;
        // §568: read the TFM header.
        if lh < 2 {
            return Err(());
        }
        let (check, _, _, _, _) = t.four_quarters()?;
        self.fonts.check[f] = check;
        let mut z = t.sixteen()?;
        z = z * 0o400 + t.byte()?;
        z = z * 0o20 + t.byte()? / 0o20;
        if z < crate::types::UNITY {
            return Err(());
        }
        for _ in 2..lh {
            for _ in 0..4 {
                t.byte()?;
            }
        }
        self.fonts.dsize[f] = z;
        if s != -1000 {
            z = if s >= 0 {
                s
            } else {
                crate::arith::xn_over_d(&mut self.arith, z, -s, 1000)
            };
        }
        self.fonts.size[f] = z;
        // ptex-base.ch [30.569]: read the char_type table (code in rh,
        // class in lh; upTeX stores a 24-bit USV plus an 8-bit class).
        if jfm_dir != 0 {
            for k in fmem_ptr..(fmem_ptr + nt) {
                let b0 = t.byte()?;
                let b1 = t.byte()?;
                let b2 = t.byte()?;
                let code = b0 * 0x100 + b1 + b2 * 0x10000;
                let ty = t.byte()?;
                self.fonts.info[k as usize].set_rh(code); // kchar_code
                self.fonts.info[k as usize].set_lh(ty); // kchar_type
            }
        }
        // §569: read character data.
        for k in (fmem_ptr + nt)..self.fonts.width_base[f] {
            let (qw, a, b, c, d) = t.four_quarters()?;
            self.fonts.info[k as usize] = qw;
            if a >= nw || b / 0o20 >= nh || b % 0o20 >= nd || c / 4 >= ni {
                return Err(());
            }
            match (c % 4) as u16 {
                LIG_TAG => {
                    // In a JFM this is gk_tag; the program is checked at
                    // the lig_kern pass below.
                    if d >= nl {
                        return Err(());
                    }
                }
                EXT_TAG => {
                    if d >= ne {
                        return Err(());
                    }
                }
                LIST_TAG => {
                    // §570: check for a charlist cycle.
                    if d < bc || d > ec {
                        return Err(());
                    }
                    let mut d = d;
                    let current = k + bc - fmem_ptr;
                    while d < current {
                        let qw = self.fonts.info[(self.fonts.char_base[f] + d) as usize];
                        if FontMem::char_tag(qw) != LIST_TAG {
                            break;
                        }
                        d = i32::from(FontMem::rem_byte(qw));
                    }
                    if d == current {
                        return Err(()); // yes, there's a cycle
                    }
                }
                _ => {}
            }
        }
        // §571-§572: read box dimensions.
        let (zp, alpha, beta) = {
            // §572: replace z by z' and compute alpha, beta.
            let mut alpha = 16;
            let mut z2 = z;
            while z2 >= 0o40000000 {
                z2 /= 2;
                alpha += alpha;
            }
            let beta = 256 / alpha;
            (z2, alpha * z2, beta)
        };
        for k in self.fonts.width_base[f]..self.fonts.lig_kern_base[f] {
            let sw = t.scaled(zp, alpha, beta)?;
            self.fonts.info[k as usize].set_sc(sw);
        }
        if self.fonts.info[self.fonts.width_base[f] as usize].sc() != 0
            || self.fonts.info[self.fonts.height_base[f] as usize].sc() != 0
            || self.fonts.info[self.fonts.depth_base[f] as usize].sc() != 0
            || self.fonts.info[self.fonts.italic_base[f] as usize].sc() != 0
        {
            return Err(());
        }
        // §573: read the ligature/kern program.
        let mut bch_label: i32 = 0o77777;
        let mut bchar: i32 = 256;
        let mut last_abcd = (0, 0, 0, 0);
        if nl > 0 {
            for k in self.fonts.lig_kern_base[f]..(self.fonts.kern_base[f] + KERN_BASE_OFFSET) {
                let (qw, a, b, c, d) = t.four_quarters()?;
                self.fonts.info[k as usize] = qw;
                last_abcd = (a, b, c, d);
                if a > 128 {
                    if 256 * c + d >= nl {
                        return Err(());
                    }
                    if a == 255 && k == self.fonts.lig_kern_base[f] {
                        bchar = b;
                    }
                } else {
                    if b != bchar {
                        // check_existence(b)
                        if b < bc || b > ec {
                            return Err(());
                        }
                        let qb = self.fonts.info[(self.fonts.char_base[f] + b) as usize];
                        if !FontMem::char_exists(qb) {
                            return Err(());
                        }
                    }
                    if c < 128 {
                        if jfm_dir != 0 {
                            // ptex-base.ch [30.573]: a glue program; the
                            // target indexes the (scaled) exten area.
                            if 256 * c + d >= ne {
                                return Err(());
                            }
                        } else {
                            // check ligature existence
                            if d < bc || d > ec {
                                return Err(());
                            }
                            let qd = self.fonts.info[(self.fonts.char_base[f] + d) as usize];
                            if !FontMem::char_exists(qd) {
                                return Err(());
                            }
                        }
                    } else if 256 * (c - 128) + d >= nk {
                        return Err(());
                    }
                    if a < 128 && k - self.fonts.lig_kern_base[f] + a + 1 >= nl {
                        return Err(());
                    }
                }
            }
            let (a, _, c, d) = last_abcd;
            if a == 255 {
                bch_label = 256 * c + d;
            }
        }
        for k in (self.fonts.kern_base[f] + KERN_BASE_OFFSET)..self.fonts.exten_base[f] {
            let sw = t.scaled(zp, alpha, beta)?;
            self.fonts.info[k as usize].set_sc(sw);
        }
        // §574 (+ ptex-base.ch): in a JFM the exten area holds the glue
        // programs as plain scaled words.
        if jfm_dir != 0 {
            for k in self.fonts.exten_base[f]..self.fonts.param_base[f] {
                let sw = t.scaled(zp, alpha, beta)?;
                self.fonts.info[k as usize].set_sc(sw);
            }
        } else {
            for k in self.fonts.exten_base[f]..self.fonts.param_base[f] {
                let (qw, a, b, c, d) = t.four_quarters()?;
                self.fonts.info[k as usize] = qw;
                for &cc in &[a, b, c] {
                    if cc != 0 {
                        if cc < bc || cc > ec {
                            return Err(());
                        }
                        let q = self.fonts.info[(self.fonts.char_base[f] + cc) as usize];
                        if !FontMem::char_exists(q) {
                            return Err(());
                        }
                    }
                }
                if d < bc || d > ec {
                    return Err(());
                }
                let q = self.fonts.info[(self.fonts.char_base[f] + d) as usize];
                if !FontMem::char_exists(q) {
                    return Err(());
                }
            }
        }
        // §575: read font parameters.
        for k in 1..=np {
            if k == 1 {
                // the slant parameter is a pure number
                let mut sw = t.byte()?;
                if sw > 127 {
                    sw -= 256;
                }
                sw = sw * 0o400 + t.byte()?;
                sw = sw * 0o400 + t.byte()?;
                let last = t.byte()?;
                self.fonts.info[self.fonts.param_base[f] as usize].set_sc(sw * 0o20 + last / 0o20);
            } else {
                let sw = t.scaled(zp, alpha, beta)?;
                self.fonts.info[(self.fonts.param_base[f] + k - 1) as usize].set_sc(sw);
            }
        }
        for k in (np + 1)..=7 {
            self.fonts.info[(self.fonts.param_base[f] + k - 1) as usize].set_sc(0);
        }
        // §576: final adjustments.
        self.fonts.params[f] = if np >= 7 { np } else { 7 };
        self.fonts.bchar_label[f] = if bch_label < nl {
            bch_label + self.fonts.lig_kern_base[f]
        } else {
            NON_ADDRESS
        };
        self.fonts.bchar[f] = bchar;
        self.fonts.false_bchar[f] = bchar;
        if bchar <= ec && bchar >= bc {
            let q = self.fonts.info[(self.fonts.char_base[f] + bchar) as usize];
            if FontMem::char_exists(q) {
                self.fonts.false_bchar[f] = NON_CHAR;
            }
        }
        self.fonts.bc[f] = bc;
        self.fonts.ec[f] = ec;
        self.fonts.glue[f] = NULL;
        self.fonts.param_base[f] -= 1;
        self.fonts.fmem_ptr += lf;
        self.fonts.font_ptr = f as i32;
        Ok(f as i32)
    }

    /// `char_warning(f, c)` (§581 + etex.ch): with tracinglostchars > 1
    /// the warning also goes to the terminal.
    pub fn char_warning(&mut self, f: i32, c: i32) {
        if self.eqtb.int_par(crate::eqtb::TRACING_LOST_CHARS_CODE) > 0 {
            let old_setting = self.eqtb.int_par(crate::eqtb::TRACING_ONLINE_CODE);
            if self.etex_ex() && self.eqtb.int_par(crate::eqtb::TRACING_LOST_CHARS_CODE) > 1 {
                let loc = self.eqtb.lay.int_base + crate::eqtb::TRACING_ONLINE_CODE;
                self.eqtb.word_mut(loc).set_int(1);
            }
            self.begin_diagnostic();
            self.print_nl_chars("Missing character: There is no ");
            self.print_char_code(c);
            self.print_chars(" in font ");
            let name = self.fonts.name[f as usize].clone();
            self.print_chars(&name);
            self.print_char('!' as i32);
            self.end_diagnostic(false);
            let loc = self.eqtb.lay.int_base + crate::eqtb::TRACING_ONLINE_CODE;
            self.eqtb.word_mut(loc).set_int(old_setting);
        }
    }

    /// `new_character(f, c)` (§582): a char node, or null if `c` doesn't
    /// exist in font `f`.
    pub fn new_character(&mut self, f: i32, c: i32) -> TexResult<Pointer> {
        if self.fonts.bc[f as usize] <= c && self.fonts.ec[f as usize] >= c {
            let q = self.fonts.char_info(f, c);
            if FontMem::char_exists(q) {
                let p = self.mem.get_avail()?;
                self.mem.set_font(p, f as u16);
                self.mem.set_character(p, c as u16);
                return Ok(p);
            }
        }
        self.char_warning(f, c);
        Ok(NULL)
    }

    /// `find_font_dimen(writing)` (§578): sets `cur_val` to a `font_info`
    /// location (or `fmem_ptr` on error).
    pub fn find_font_dimen(&mut self, writing: bool) -> TexResult<()> {
        self.scan_int()?;
        let n = self.cur_val;
        self.scan_font_ident()?;
        let f = self.cur_val;
        if n <= 0 {
            self.cur_val = self.fonts.fmem_ptr;
        } else {
            if writing
                && (SPACE_CODE..=SPACE_SHRINK_CODE).contains(&n)
                && self.fonts.glue[f as usize] != NULL
            {
                let g = self.fonts.glue[f as usize];
                self.mem.delete_glue_ref(g);
                self.fonts.glue[f as usize] = NULL;
            }
            if n > self.fonts.params[f as usize] {
                if f < self.fonts.font_ptr {
                    self.cur_val = self.fonts.fmem_ptr;
                } else {
                    // §580: increase the number of parameters in the last font.
                    loop {
                        if self.fonts.fmem_ptr == self.fonts.font_mem_size {
                            return Err(TexInterrupt::Overflow {
                                what: "font memory",
                                size: self.fonts.font_mem_size,
                            });
                        }
                        let fp = self.fonts.fmem_ptr as usize;
                        self.fonts.info[fp].set_sc(0);
                        self.fonts.fmem_ptr += 1;
                        self.fonts.params[f as usize] += 1;
                        if n == self.fonts.params[f as usize] {
                            break;
                        }
                    }
                    self.cur_val = self.fonts.fmem_ptr - 1;
                }
            } else {
                self.cur_val = n + self.fonts.param_base[f as usize];
            }
        }
        // §579: issue an error message if cur_val = fmem_ptr.
        if self.cur_val == self.fonts.fmem_ptr {
            self.print_err("Font ");
            let t = self.eqtb.font_id_text(f);
            self.print_esc(t);
            self.print_chars(" has only ");
            let np = self.fonts.params[f as usize];
            self.print_int(np);
            self.print_chars(" fontdimen parameters");
            self.help(&[
                "To increase the number of font parameters, you must",
                "use \\fontdimen immediately after the \\font is loaded.",
            ]);
            self.error()?;
        }
        Ok(())
    }
}
