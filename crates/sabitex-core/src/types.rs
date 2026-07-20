//! Basic types and word-size constants.
//!
//! Ports tex.web §101 (`scaled`), §108-§110 (halfword/quarterword bounds)
//! with the *XeTeX* word layout: a halfword is a 32-bit signed integer so
//! that Unicode characters (up to 0x10FFFF) and large `mem` arrays fit, and
//! a quarterword is 16 bits.
//!
//! `Scaled` and `Pointer` are deliberately plain type aliases, not newtypes:
//! tex.web freely mixes integers, scaled values and pointers (`sc == int`,
//! §113), and pointer arithmetic like `q := p + node_size(p)` is pervasive.
//! Safety is enforced instead by routing all multiplication/division of
//! scaled values through [`crate::arith`] and all `mem` access through
//! [`crate::mem::Mem`] accessor methods.

/// A fixed-point dimension in units of 2^-16 pt ("scaled points", tex.web §101).
pub type Scaled = i32;

/// `unity == @'200000`: 2^16, represents 1.00000 (tex.web §101).
pub const UNITY: Scaled = 1 << 16;

/// `two == @'400000`: 2^17, represents 2.00000 (tex.web §101).
pub const TWO: Scaled = 1 << 17;

/// `inf_bad = 10000`: infinitely bad badness value (tex.web §108).
pub const INF_BAD: i32 = 10_000;

/// A halfword: an index into `mem` / `eqtb`, or a flag (tex.web §115).
/// 32-bit signed, following XeTeX.
pub type Halfword = i32;

/// A quarterword (tex.web §113). 16 bits, following XeTeX.
pub type Quarterword = u16;

/// `pointer == halfword`: a location in `mem` or `eqtb` (tex.web §115).
pub type Pointer = Halfword;

/// A Unicode scalar value as handled by the engine (UTF-32). The engine is
/// Unicode-native from the start (XeTeX layout); TeX82's `ASCII_code`
/// (0..255) is a subrange of this type.
pub type UnicodeChar = i32;

/// A string number: an index into the string pool's `str_start` array.
/// Values 0..255 double as single-character strings (tex.web §38).
pub type StrNumber = i32;

/// `min_halfword` (tex.web §110). We keep 0 so that `null = 0` like the
/// recommended 32-bit TeX82 settings; revisit if XeTeX sparse-register
/// compatibility (M6+) requires a negative minimum.
pub const MIN_HALFWORD: Halfword = 0;

/// `max_halfword` (tex.web §110): the largest allowable halfword value.
/// 2^30 - 1, the XeTeX value, so that `mem_max < max_halfword` holds for
/// any realistic memory size and `empty_flag` never collides with a link.
pub const MAX_HALFWORD: Halfword = 0x3FFF_FFFF;

/// `null == min_halfword`: the null pointer (tex.web §115).
pub const NULL: Pointer = MIN_HALFWORD;
