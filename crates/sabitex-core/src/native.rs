//! Native (OpenType) fonts: XeTeX's `load_native_font` and glyph metrics,
//! backed by ttf-parser + rustybuzz instead of the C font manager, ICU,
//! and HarfBuzz (see specification/xetex.md and reference/xetex/xetex.web §16747).
//!
//! Everything here is gated behind the `shaping` cargo feature; without
//! it, quoted font names simply fail to load as native fonts and fall
//! through to the usual "Font ... not loadable" error.

use crate::engine::Engine;
use crate::error::TexResult;
use crate::types::Scaled;

/// `font_flags` bits (XeTeX_ext.h).
pub const FONT_FLAGS_COLORED: u8 = 0x01;
pub const FONT_FLAGS_VERTICAL: u8 = 0x02;

// xetex.web: whatsit subtypes for native-font material.
pub const NATIVE_WORD_NODE: u16 = 40;
pub const NATIVE_WORD_NODE_AT: u16 = 41;
pub const GLYPH_NODE: u16 = 42;
/// `native_node_size`: fixed words of a native_word node; the UTF-16
/// text follows at 4 units per memory word (as in pseudo files).
pub const NATIVE_NODE_SIZE: i32 = 6;
pub const GLYPH_NODE_SIZE: i32 = 5;

/// Total node size for a native_word with `n` UTF-16 units.
pub fn native_word_size(n: i32) -> i32 {
    NATIVE_NODE_SIZE + (2 * n + 7) / 8
}

/// One shaped glyph of a native word: position (sp, relative to the node
/// origin, y grows downward as in XDV) and glyph ID. This mirrors the
/// 10-byte glyph info records of XeTeX (`native_glyph_info_size`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphInfo {
    pub x: Scaled,
    pub y: Scaled,
    pub gid: u16,
}

/// A loaded native font instance (XeTeX keeps these behind the opaque
/// `font_layout_engine` pointer). The face is re-parsed from `data` on
/// demand — rustybuzz faces borrow the byte buffer, and correctness
/// beats caching for now.
pub struct NativeFont {
    /// The font file, owned.
    pub data: Vec<u8>,
    /// Face index within the file (TTC).
    pub index: u32,
    /// `font_size[f]`: the "at" size in sp.
    pub size: Scaled,
    /// The file name as it goes into the XDV `define_native_font`.
    pub filename: String,
    /// OpenType feature settings from the `:features` part of the spec.
    #[cfg(feature = "shaping")]
    pub features: Vec<rustybuzz::Feature>,
    /// `font_letter_space[f]` (sp).
    pub letter_space: Scaled,
    /// `font_flags[f]`: FONT_FLAGS_*.
    pub flags: u8,
    /// RGBA color when FONT_FLAGS_COLORED.
    pub rgba: u32,
    /// Cached design metrics in sp at `size`.
    pub ascent: Scaled,
    pub descent: Scaled,
    pub x_height: Scaled,
    pub cap_height: Scaled,
    pub slant: Scaled,
    pub units_per_em: i32,
}

#[cfg(feature = "shaping")]
impl NativeFont {
    /// Converts a font-unit quantity to sp at the instance size.
    pub fn units_to_sp(&self, v: f64) -> Scaled {
        (v * f64::from(self.size) / f64::from(self.units_per_em as u32)).round() as Scaled
    }

    pub fn face(&self) -> Option<rustybuzz::Face<'_>> {
        rustybuzz::Face::from_slice(&self.data, self.index)
    }

    /// Shapes `text` and returns (total advance width, glyph records).
    /// Mirrors XeTeX's `set_native_metrics`: x/y are absolute positions
    /// within the word, y grows downward.
    pub fn shape(&self, text: &str) -> (Scaled, Vec<GlyphInfo>) {
        let Some(face) = self.face() else {
            return (0, Vec::new());
        };
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        let out = rustybuzz::shape(&face, &self.features, buf);
        let mut glyphs = Vec::with_capacity(out.len());
        let mut x: Scaled = 0;
        for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
            glyphs.push(GlyphInfo {
                x: x + self.units_to_sp(f64::from(pos.x_offset)),
                y: -self.units_to_sp(f64::from(pos.y_offset)),
                gid: info.glyph_id as u16,
            });
            x += self.units_to_sp(f64::from(pos.x_advance));
            if self.letter_space != 0 {
                x += self.letter_space;
            }
        }
        // XeTeX drops one trailing letter-space (the word ends).
        if self.letter_space != 0 && !glyphs.is_empty() {
            x -= self.letter_space;
        }
        (x, glyphs)
    }

    /// Width of a single glyph (for glyph_node metrics).
    pub fn glyph_advance(&self, gid: u16) -> Scaled {
        let Some(face) = self.face() else { return 0 };
        let adv = face.glyph_hor_advance(ttf_parser_glyph(gid)).unwrap_or(0);
        self.units_to_sp(f64::from(adv))
    }
}

#[cfg(feature = "shaping")]
fn ttf_parser_glyph(gid: u16) -> rustybuzz::ttf_parser::GlyphId {
    rustybuzz::ttf_parser::GlyphId(gid)
}

/// Parses XeTeX's quoted font spec `name:feature;feature=value;...`.
/// Returns (file spec, feature list, letter_space (sp per em/100 units
/// come later — stored raw for now), color).
#[cfg(feature = "shaping")]
pub fn parse_font_spec(spec: &str) -> (String, Vec<rustybuzz::Feature>, Option<u32>) {
    let (name, feats) = match spec.split_once(':') {
        Some((n, f)) => (n, f),
        None => (spec, ""),
    };
    let mut features = Vec::new();
    let mut color = None;
    for item in feats.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(c) = item.strip_prefix("color=") {
            if let Ok(v) = u32::from_str_radix(c, 16) {
                // RRGGBB or RRGGBBAA; default alpha FF.
                color = Some(if c.len() <= 6 { (v << 8) | 0xFF } else { v });
            }
        } else if let Some(f) = item.strip_prefix('+') {
            if let Some(feat) = make_feature(f, 1) {
                features.push(feat);
            }
        } else if let Some(f) = item.strip_prefix('-') {
            if let Some(feat) = make_feature(f, 0) {
                features.push(feat);
            }
        } else if let Some((k, v)) = item.split_once('=') {
            if let (Some(feat), Ok(val)) = (make_feature(k, 0), v.parse::<u32>()) {
                let mut feat = feat;
                feat.value = val;
                features.push(feat);
            }
        }
    }
    (name.to_string(), features, color)
}

#[cfg(feature = "shaping")]
fn make_feature(tag: &str, value: u32) -> Option<rustybuzz::Feature> {
    let tag = tag.trim();
    if tag.is_empty() || tag.len() > 4 || !tag.is_ascii() {
        return None;
    }
    let mut bytes = [b' '; 4];
    bytes[..tag.len()].copy_from_slice(tag.as_bytes());
    Some(rustybuzz::Feature::new(
        rustybuzz::ttf_parser::Tag::from_bytes(&bytes),
        value,
        ..,
    ))
}

impl Engine {
    /// `load_native_font` (xetex.web §16747): tries to load the quoted
    /// name as an OpenType font via TexFs. Returns None when the file is
    /// not found or the `shaping` feature is off.
    #[cfg(feature = "shaping")]
    pub fn load_native_font(
        &mut self,
        u: crate::types::Pointer,
        spec: &str,
        s: Scaled,
    ) -> TexResult<Option<i32>> {
        let (name, features, color) = parse_font_spec(spec);
        // "[path]" means an explicit file; a plain name tries common
        // extensions through TexFs (manifest-based, no system lookup).
        let path = name
            .strip_prefix('[')
            .and_then(|n| n.strip_suffix(']'))
            .map(str::to_string);
        let mut candidates: Vec<String> = Vec::new();
        if let Some(p) = &path {
            // XeTeX's bracketed form is a file name; findNativeFont tries
            // the name as given and with the sfnt extensions.
            if p.contains('.') {
                candidates.push(p.clone());
            } else {
                for ext in ["", ".otf", ".ttf"] {
                    candidates.push(format!("{p}{ext}"));
                }
            }
        } else {
            for ext in ["", ".otf", ".ttf"] {
                candidates.push(format!("{name}{ext}"));
            }
        }
        let mut found: Option<(String, Vec<u8>)> = None;
        for c in &candidates {
            if let Some(bytes) = self.fs.read_file(c, crate::io::FileKind::Font) {
                found = Some((c.clone(), bytes));
                break;
            }
        }
        let Some((filename, data)) = found else {
            return Ok(None);
        };
        let Some(face) = rustybuzz::Face::from_slice(&data, 0) else {
            return Ok(None);
        };
        let upem: i32 = face.units_per_em();
        // Design size: XeTeX uses the point size from the spec; OpenType
        // has no design size, so dsize = 10pt as XeTeX's D2Fix(10.0)?
        // In fact loaded_font_design_size defaults to 655360 (10pt).
        let dsize: Scaled = 655360;
        let actual_size = if s >= 0 {
            s
        } else if s != -1000 {
            crate::arith::xn_over_d(&mut self.arith, dsize, -s, 1000)
        } else {
            dsize
        };
        let to_sp =
            |v: f64| (v * f64::from(actual_size) / f64::from(upem as u32)).round() as Scaled;
        let ascent = to_sp(f64::from(face.ascender()));
        let descent = to_sp(f64::from(face.descender())); // negative
        let x_height = to_sp(f64::from(face.x_height().unwrap_or(0)));
        let cap_height = to_sp(f64::from(face.capital_height().unwrap_or(0)));
        let italic_angle = face.italic_angle();
        // slant = tan(angle); Fixed like XeTeX's Fix2D dance, in sp/pt.
        let slant = ((-f64::from(italic_angle)).to_radians().tan() * 65536.0).round() as Scaled;
        drop(face);

        let mut flags = 0u8;
        let mut rgba = 0u32;
        if let Some(c) = color {
            flags |= FONT_FLAGS_COLORED;
            rgba = c;
        }
        let nf = NativeFont {
            data,
            index: 0,
            size: actual_size,
            filename,
            features,
            letter_space: 0,
            flags,
            rgba,
            ascent,
            descent,
            x_height,
            cap_height,
            slant,
            units_per_em: upem,
        };

        // Room check and registration mirror read_font_info's tail.
        let num_font_dimens = 8;
        if self.fonts.font_ptr == self.fonts.font_max
            || self.fonts.fmem_ptr + num_font_dimens > self.fonts.font_mem_size
        {
            return Ok(None); // apologize path lands with the TFM error
        }
        // Reuse when already loaded (same canonical spec and size).
        for f in 1..=self.fonts.font_ptr {
            if self.fonts.native[f as usize].is_some()
                && self.fonts.name[f as usize] == spec
                && self.fonts.size[f as usize] == actual_size
            {
                return Ok(Some(f));
            }
        }
        self.fonts.font_ptr += 1;
        let f = self.fonts.font_ptr;
        let fu = f as usize;
        self.fonts.name[fu] = spec.to_string();
        self.fonts.area[fu] = String::new();
        self.fonts.check[fu] = crate::memword::MemoryWord::ZERO;
        self.fonts.glue[fu] = crate::types::NULL;
        self.fonts.dsize[fu] = dsize;
        self.fonts.size[fu] = actual_size;
        self.fonts.height_base[fu] = nf.ascent;
        self.fonts.depth_base[fu] = -nf.descent;
        self.fonts.params[fu] = num_font_dimens;
        self.fonts.bc[fu] = 0;
        self.fonts.ec[fu] = 65535;
        self.fonts.used[fu] = false;
        self.fonts.hyphen_char[fu] = self.eqtb.int_par(crate::eqtb::DEFAULT_HYPHEN_CHAR_CODE);
        self.fonts.skew_char[fu] = self.eqtb.int_par(crate::eqtb::DEFAULT_SKEW_CHAR_CODE);
        self.fonts.param_base[fu] = self.fonts.fmem_ptr - 1;

        // Space width from the shaped space character.
        let (space, _) = nf.shape(" ");
        let slant_p = nf.slant;
        let x_ht = nf.x_height;
        let cap_ht = nf.cap_height;
        let quad = actual_size;
        self.fonts.native[fu] = Some(Box::new(nf));
        for v in [
            slant_p,
            space,
            space / 2,
            space / 3,
            x_ht,
            quad,
            space / 3,
            cap_ht,
        ] {
            let ptr = self.fonts.fmem_ptr;
            self.fonts.info[ptr as usize].set_sc(v);
            self.fonts.fmem_ptr += 1;
        }
        let _ = u;
        Ok(Some(f))
    }

    /// Without the `shaping` feature native fonts never load.
    #[cfg(not(feature = "shaping"))]
    pub fn load_native_font(
        &mut self,
        _u: crate::types::Pointer,
        _spec: &str,
        _s: Scaled,
    ) -> TexResult<Option<i32>> {
        Ok(None)
    }
}

impl crate::mem::Mem {
    /// `native_size(p) == mem[p+4].qqqq.b0` — the node size in words.
    pub fn native_size(&self, p: crate::types::Pointer) -> i32 {
        i32::from(self.word(p + 4).qqqq(0))
    }

    pub fn set_native_size(&mut self, p: crate::types::Pointer, v: i32) {
        self.word_mut(p + 4).set_qqqq(0, v as u16);
    }

    /// `native_font(p) == mem[p+4].qqqq.b1`.
    pub fn native_font(&self, p: crate::types::Pointer) -> i32 {
        i32::from(self.word(p + 4).qqqq(1))
    }

    pub fn set_native_font(&mut self, p: crate::types::Pointer, v: i32) {
        self.word_mut(p + 4).set_qqqq(1, v as u16);
    }

    /// `native_length(p) == mem[p+4].qqqq.b2` (UTF-16 units). For glyph
    /// nodes this same field is `native_glyph` (the glyph ID).
    pub fn native_length(&self, p: crate::types::Pointer) -> i32 {
        i32::from(self.word(p + 4).qqqq(2))
    }

    pub fn set_native_length(&mut self, p: crate::types::Pointer, v: i32) {
        self.word_mut(p + 4).set_qqqq(2, v as u16);
    }

    /// `native_glyph_count(p) == mem[p+4].qqqq.b3`.
    pub fn native_glyph_count(&self, p: crate::types::Pointer) -> i32 {
        i32::from(self.word(p + 4).qqqq(3))
    }

    pub fn set_native_glyph_count(&mut self, p: crate::types::Pointer, v: i32) {
        self.word_mut(p + 4).set_qqqq(3, v as u16);
    }

    /// `get_native_char(p, i)`: UTF-16 unit `i` of the node text.
    pub fn get_native_char(&self, p: crate::types::Pointer, i: i32) -> u16 {
        self.word(p + NATIVE_NODE_SIZE + i / 4)
            .qqqq((i % 4) as usize)
    }

    pub fn set_native_char(&mut self, p: crate::types::Pointer, i: i32, c: u16) {
        self.word_mut(p + NATIVE_NODE_SIZE + i / 4)
            .set_qqqq((i % 4) as usize, c);
    }

    /// Is `p` a native_word whatsit?
    pub fn is_native_word_node(&self, p: crate::types::Pointer) -> bool {
        p != crate::types::NULL
            && !self.is_char_node(p)
            && self.node_type(p) == crate::nodes::WHATSIT_NODE
            && (self.subtype(p) == NATIVE_WORD_NODE || self.subtype(p) == NATIVE_WORD_NODE_AT)
    }

    /// Is `p` a glyph whatsit?
    pub fn is_glyph_node(&self, p: crate::types::Pointer) -> bool {
        p != crate::types::NULL
            && !self.is_char_node(p)
            && self.node_type(p) == crate::nodes::WHATSIT_NODE
            && self.subtype(p) == GLYPH_NODE
    }
}

impl Engine {
    /// Does font `f` render through the native (OpenType) path?
    pub fn is_native_font(&self, f: i32) -> bool {
        self.fonts.native[f as usize].is_some()
    }

    /// `new_native_word_node(f, n)` (xetex.web §16595). Metrics are not
    /// set; call `set_native_metrics` afterwards.
    pub fn new_native_word_node(&mut self, f: i32, n: i32) -> TexResult<crate::types::Pointer> {
        let l = native_word_size(n);
        let q = self.mem.get_node(l)?;
        self.mem.set_node_type(q, crate::nodes::WHATSIT_NODE);
        self.mem.set_subtype(q, NATIVE_WORD_NODE);
        self.mem.set_width(q, 0);
        self.mem.set_depth(q, 0);
        self.mem.set_height(q, 0);
        self.mem.set_native_size(q, l);
        self.mem.set_native_font(q, f);
        self.mem.set_native_length(q, n);
        self.mem.set_native_glyph_count(q, 0);
        self.mem.set_link(q + 5, crate::types::NULL); // glyph info "ptr" word unused
        self.mem.set_info(q + 5, crate::types::NULL);
        Ok(q)
    }

    /// Frees a native_word or glyph whatsit, dropping its glyph records.
    pub fn free_native_node(&mut self, p: crate::types::Pointer) {
        self.native_glyph_infos.remove(&p);
        let s = if self.mem.subtype(p) == GLYPH_NODE {
            GLYPH_NODE_SIZE
        } else {
            self.mem.native_size(p)
        };
        self.mem.free_node(p, s);
    }

    /// `copy_native_glyph_info` + node copy support: called by
    /// copy_node_list after words have been copied.
    pub fn copy_native_glyph_info(
        &mut self,
        src: crate::types::Pointer,
        dest: crate::types::Pointer,
    ) {
        if let Some(v) = self.native_glyph_infos.get(&src) {
            let v = v.clone();
            self.native_glyph_infos.insert(dest, v);
        }
    }

    /// The node text as a Rust string.
    pub fn native_text(&self, p: crate::types::Pointer) -> String {
        let n = self.mem.native_length(p);
        let units: Vec<u16> = (0..n).map(|i| self.mem.get_native_char(p, i)).collect();
        String::from_utf16_lossy(&units)
    }

    /// `set_native_metrics(p)` (XeTeX_ext.c): shapes the text and stores
    /// width/height/depth plus the glyph records.
    #[cfg(feature = "shaping")]
    pub fn set_native_metrics(&mut self, p: crate::types::Pointer) {
        let f = self.mem.native_font(p) as usize;
        let text = self.native_text(p);
        let Some(nf) = &self.fonts.native[f] else {
            return;
        };
        let (w, glyphs) = nf.shape(&text);
        let (h, d) = (nf.ascent, -nf.descent);
        self.mem.set_width(p, w);
        self.mem.set_height(p, h);
        self.mem.set_depth(p, d);
        self.mem.set_native_glyph_count(p, glyphs.len() as i32);
        self.native_glyph_infos.insert(p, glyphs);
    }

    #[cfg(not(feature = "shaping"))]
    pub fn set_native_metrics(&mut self, _p: crate::types::Pointer) {}

    /// `do_locale_linebreaks(text[s..s+len])` with locale 0 (xetex.web
    /// §16876): append one native_word node holding the fragment.
    fn append_native_word(&mut self, f: i32, text: &[u16]) -> TexResult<()> {
        let q = self.new_native_word_node(f, text.len() as i32)?;
        for (i, &c) in text.iter().enumerate() {
            self.mem.set_native_char(q, i as i32, c);
        }
        self.set_native_metrics(q);
        self.tail_append(q);
        Ok(())
    }

    /// `collect_native` (xetex.web §24400-): gathers consecutive
    /// characters of a native font into native_word nodes, splitting at
    /// hyphen_char with discretionaries in unrestricted hmode. On return
    /// cur_cmd/cur_chr hold the next unprocessed token (reswitch).
    ///
    /// Simplifications vs XeTeX (documented in specification/xetex.md): no
    /// font_mapping (TECkit), no \XeTeXlinebreaklocale, no dash-break
    /// state, no interword space shaping, no merge with a preceding
    /// native_word node.
    pub fn collect_native(&mut self) -> TexResult<()> {
        use crate::cmds::{CHAR_GIVEN, CHAR_NUM, LETTER, OTHER_CHAR};
        if self.mode() > 0 && self.eqtb.int_par(crate::eqtb::LANGUAGE_CODE) != self.clang() {
            self.fix_language()?;
        }
        let f = self.eqtb.cur_font();
        self.main_f = f;
        let hyphen = self.fonts.hyphen_char[f as usize];
        let mut text: Vec<u16> = Vec::new();
        #[allow(unused_assignments)] // set in each loop pass, read after
        let mut is_hyph = false;
        loop {
            self.adjust_space_factor();
            let c = self.cur_chr;
            if c > 0xFFFF {
                text.push(((c - 0x10000) / 1024 + 0xD800) as u16);
                text.push(((c - 0x10000) % 1024 + 0xDC00) as u16);
            } else {
                text.push(c as u16);
            }
            is_hyph = c == hyphen;
            // Try to collect as many chars as possible in the same font.
            self.get_next()?;
            if matches!(self.cur_cmd, LETTER | OTHER_CHAR | CHAR_GIVEN) {
                continue;
            }
            self.x_token()?;
            if matches!(self.cur_cmd, LETTER | OTHER_CHAR | CHAR_GIVEN) {
                continue;
            }
            if self.cur_cmd == CHAR_NUM {
                self.scan_char_num()?;
                self.cur_chr = self.cur_val;
                continue;
            }
            break;
        }
        // \tracinglostchars: warn for unmapped characters.
        if self.eqtb.int_par(crate::eqtb::TRACING_LOST_CHARS_CODE) > 0 {
            let chars: Vec<char> = String::from_utf16_lossy(&text).chars().collect();
            for ch in chars {
                if !self.native_has_glyph(f, ch) {
                    self.char_warning(f, ch as i32);
                }
            }
        }
        if self.mode() == crate::engine::HMODE {
            // Unrestricted: split fragments at hyphens; a discretionary
            // follows each fragment that ends with a hyphen.
            let mut rest: &[u16] = &text;
            loop {
                let mut h = 0;
                while h < rest.len() && rest[h] as i32 != hyphen {
                    h += 1;
                }
                if h < rest.len() {
                    h += 1; // include the hyphen
                }
                let (frag, tail) = rest.split_at(h);
                self.append_native_word(f, frag)?;
                rest = tail;
                if !rest.is_empty() || is_hyph {
                    let d = self.new_disc()?;
                    self.tail_append(d);
                }
                if rest.is_empty() {
                    break;
                }
            }
        } else {
            // Restricted hmode: a single node, no discretionaries.
            self.append_native_word(f, &text)?;
        }
        Ok(())
    }

    /// Does the font map this character to a glyph?
    #[cfg(feature = "shaping")]
    pub fn native_has_glyph(&self, f: i32, c: char) -> bool {
        self.fonts.native[f as usize]
            .as_deref()
            .and_then(|nf| nf.face())
            .and_then(|face| face.glyph_index(c))
            .is_some()
    }

    #[cfg(not(feature = "shaping"))]
    pub fn native_has_glyph(&self, _f: i32, _c: char) -> bool {
        true
    }
}
