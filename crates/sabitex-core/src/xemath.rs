//! XeTeX extended math codes.
//!
//! xetex.h packs a math code into 32 bits: character in bits 0..20
//! (up to U+10FFFF), class in bits 21..23, family in bits 24..31.
//! `\mathcode c = "8000` maps to the "active" marker, whose character
//! field is all ones (xetex.web §5728). Family values >= 128 set the
//! sign bit; the fields must therefore always be read through these
//! helpers, never by arithmetic on the raw value.

pub const ACTIVE_MATH_CHAR: i32 = 0x1F_FFFF;
pub const VAR_FAM_CLASS: i32 = 7;
/// `\XeTeXcharclass` bound (xetex.web `char_class_limit`).
pub const CHAR_CLASS_LIMIT: i32 = 4096;

#[inline]
pub fn math_char_field(x: i32) -> i32 {
    x & 0x1F_FFFF
}

#[inline]
pub fn math_class_field(x: i32) -> i32 {
    (x >> 21) & 0x07
}

#[inline]
pub fn math_fam_field(x: i32) -> i32 {
    (((x as u32) >> 24) & 0xFF) as i32
}

#[inline]
pub fn set_class_field(x: i32) -> i32 {
    (x & 0x07) << 21
}

#[inline]
pub fn set_family_field(x: i32) -> i32 {
    (((x & 0xFF) as u32) << 24) as i32
}

#[inline]
pub fn is_active_math_char(x: i32) -> bool {
    math_char_field(x) == ACTIVE_MATH_CHAR
}

#[inline]
pub fn is_var_family(x: i32) -> bool {
    math_class_field(x) == VAR_FAM_CLASS
}

/// Converts a TeX82 15-bit math code to the extended form
/// (xetex.web §1232; `"8000` becomes the active marker).
#[inline]
pub fn from_classic(v: i32) -> i32 {
    if v == 0x8000 {
        ACTIVE_MATH_CHAR
    } else {
        set_class_field(v / 0x1000) + set_family_field((v % 0x1000) / 0x100) + (v % 0x100)
    }
}

/// Converts an extended math code back to the 15-bit form, if it fits
/// (xetex.web §9838: `\the\mathcode` errors otherwise).
#[inline]
pub fn to_classic(x: i32) -> Option<i32> {
    if is_active_math_char(x) {
        return Some(0x8000);
    }
    if math_class_field(x) > 7 || math_fam_field(x) > 15 || math_char_field(x) > 255 {
        return None;
    }
    Some(math_class_field(x) * 0x1000 + math_fam_field(x) * 0x100 + math_char_field(x))
}
