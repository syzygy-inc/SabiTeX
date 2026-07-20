//! sabitex-core — a faithful Rust port of the TeX typesetting engine.
//!
//! The single source of truth for every algorithm in this crate is
//! `reference/tex/tex.web` (Knuth's TeX82). Each module header states which Part /
//! sections of tex.web it ports; `specification/texweb-map.md` keeps the full map.
//!
//! Porting rules (see `specification/architecture.md`):
//! - `mem[]` is ported as a flat array of [`memword::MemoryWord`]; pointers
//!   are `i32` indices, exactly as in tex.web. No Rust enum nodes.
//! - Scaled arithmetic never uses raw `*` / `div`; it goes through
//!   [`arith`] so that results are bit-identical to tex.web (TRIP goal).
//! - All I/O goes through the [`io::TexFs`] / [`io::Terminal`] traits so the
//!   engine runs unchanged on native and wasm32 targets.

#![forbid(unsafe_code)]

pub mod align;
pub mod arith;
pub mod boxops;
pub mod cmdchr;
pub mod cmds;
pub mod cond;
pub mod control;
pub mod dvi;
pub mod engine;
pub mod eqtb;
pub mod error;
pub mod expand;
pub mod expr;
pub mod ext;
pub mod fmt;
pub mod fonts;
pub mod getnext;
pub mod hyph;
pub mod input;
pub mod io;
pub mod kanji;
pub mod linebreak;
pub mod math;
pub mod mathlist;
pub mod mem;
pub mod memword;
pub mod native;
pub mod nest;
pub mod nodes;
pub mod pack;
pub mod page;
pub mod par;
pub mod prefix;
pub mod print;
pub mod sa;
pub mod scan;
pub mod strings;
pub mod tokens;
pub mod toks;
pub mod types;
pub mod xemath;

pub use engine::{Engine, Sizes};
pub use error::{History, TexInterrupt, TexResult};

/// Engine banner, printed at start-up (tex.web §2 `banner`).
///
/// TRIP comparison masks the banner line, so the exact text is free.
pub const BANNER: &str = "This is SabiTeX, Version 0.0.1";
