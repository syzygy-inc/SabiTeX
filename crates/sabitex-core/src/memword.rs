//! Packed data: `memory_word` and its variant views.
//!
//! Ports tex.web Part 8 (§113-§118) with the XeTeX word layout: one
//! `MemoryWord` is 64 bits, a halfword is a signed 32-bit integer and a
//! quarterword is 16 bits.
//!
//! tex.web §113: "TeX makes no assumptions about the relative positions of
//! the fields within a word", so we are free to fix a deterministic layout
//! (deterministic bytes matter for format dumping):
//!
//! ```text
//!   bits  0..32   rh   (halfword)        -- two_halves.rh
//!   bits 32..64   lh   (halfword)        -- two_halves.lh
//!   bits 32..48   b0   (quarterword)     -- two_halves.b0 (overlays lh)
//!   bits 48..64   b1   (quarterword)     -- two_halves.b1 (overlays lh)
//!   bits  0..32   int  (whole-word integer variant; setter zeroes bits 32..64)
//!   bits  0..64   gr   (glue_ratio, f64 bit pattern)
//!   bits  0..64   qqqq (four quarterwords q0..q3, low to high)
//! ```
//!
//! Like the Pascal variant record, each word is meaningful only through the
//! variant it was written with; no code may rely on cross-variant aliasing.
//! `glue_ratio` is `f64` (the web2c default), tex.web §109.

use crate::types::{Halfword, Quarterword, Scaled};

/// One word of `mem`, `eqtb`, `font_info`, etc. (tex.web §118).
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub struct MemoryWord(u64);

impl MemoryWord {
    /// A word with all bits zero.
    pub const ZERO: MemoryWord = MemoryWord(0);

    /// The raw 64-bit value (for dumping; little-endian on disk).
    pub fn bits(self) -> u64 {
        self.0
    }

    /// Reconstructs a word from its raw bits (for undumping).
    pub fn from_bits(bits: u64) -> Self {
        MemoryWord(bits)
    }

    /// `w.int` / `w.sc` (the integer variant).
    pub fn int(self) -> i32 {
        self.0 as u32 as i32
    }

    /// Sets the integer variant. Writes the whole word (high bits zeroed) so
    /// that dumped words are deterministic.
    pub fn set_int(&mut self, v: i32) {
        self.0 = u64::from(v as u32);
    }

    /// `w.sc` — scaled data is equivalent to integer (tex.web §113).
    pub fn sc(self) -> Scaled {
        self.int()
    }

    /// Sets the scaled variant.
    pub fn set_sc(&mut self, v: Scaled) {
        self.set_int(v);
    }

    /// `w.gr` (the glue_ratio variant).
    pub fn gr(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Sets the glue_ratio variant (writes the whole word). tex.web §109's
    /// `unfloat` narrows to `glue_ratio`, which web2c defines as a
    /// single-precision float; rounding through f32 here keeps glue-set
    /// arithmetic bit-compatible with the TeX Live reference binaries.
    pub fn set_gr(&mut self, v: f64) {
        self.0 = f64::from(v as f32).to_bits();
    }

    /// Sets the glue_ratio variant at full double precision (TeX Live
    /// binaries define glue_ratio as double; cf. the etrip reference).
    pub fn set_gr_wide(&mut self, v: f64) {
        self.0 = v.to_bits();
    }

    /// `w.hh.rh`.
    pub fn rh(self) -> Halfword {
        self.0 as u32 as i32
    }

    /// Sets `w.hh.rh`.
    pub fn set_rh(&mut self, v: Halfword) {
        self.0 = (self.0 & 0xFFFF_FFFF_0000_0000) | u64::from(v as u32);
    }

    /// `w.hh.lh`.
    pub fn lh(self) -> Halfword {
        (self.0 >> 32) as u32 as i32
    }

    /// Sets `w.hh.lh`.
    pub fn set_lh(&mut self, v: Halfword) {
        self.0 = (self.0 & 0x0000_0000_FFFF_FFFF) | (u64::from(v as u32) << 32);
    }

    /// `w.hh.b0` (overlays the low 16 bits of `lh`).
    pub fn b0(self) -> Quarterword {
        (self.0 >> 32) as u16
    }

    /// Sets `w.hh.b0`.
    pub fn set_b0(&mut self, v: Quarterword) {
        self.0 = (self.0 & 0xFFFF_0000_FFFF_FFFF) | (u64::from(v) << 32);
    }

    /// `w.hh.b1` (overlays the high 16 bits of `lh`).
    pub fn b1(self) -> Quarterword {
        (self.0 >> 48) as u16
    }

    /// Sets `w.hh.b1`.
    pub fn set_b1(&mut self, v: Quarterword) {
        self.0 = (self.0 & 0x0000_FFFF_FFFF_FFFF) | (u64::from(v) << 48);
    }

    /// `w.qqqq.b0` .. `w.qqqq.b3`: the four-quarterword variant, indexed 0..4
    /// from the low end of the word.
    pub fn qqqq(self, i: usize) -> Quarterword {
        debug_assert!(i < 4);
        (self.0 >> (16 * i)) as u16
    }

    /// Sets one quarter of the four-quarterword variant.
    pub fn set_qqqq(&mut self, i: usize, v: Quarterword) {
        debug_assert!(i < 4);
        let shift = 16 * i;
        self.0 = (self.0 & !(0xFFFFu64 << shift)) | (u64::from(v) << shift);
    }
}

impl core::fmt::Debug for MemoryWord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Modeled on print_word (tex.web §114): show the main variants.
        write!(
            f,
            "MemoryWord(int={}, lh={}, rh={}, b0={}, b1={})",
            self.int(),
            self.lh(),
            self.rh(),
            self.b0(),
            self.b1()
        )
    }
}
