//! The command codes.
//!
//! A direct port of tex.web Part 15 (§207-§210). The numeric values are
//! load-bearing: catcode commands are fixed by the language (`\catcode`...`=3`
//! makes a math shift), and many scanning routines do range comparisons.

// §207: catcode commands (0..15).
pub const ESCAPE: u16 = 0; // escape delimiter \
pub const RELAX: u16 = 0; // \relax
pub const LEFT_BRACE: u16 = 1;
pub const RIGHT_BRACE: u16 = 2;
pub const MATH_SHIFT: u16 = 3;
pub const TAB_MARK: u16 = 4; // &, \span
pub const CAR_RET: u16 = 5; // carriage_return, \cr, \crcr
pub const OUT_PARAM: u16 = 5; // output a macro parameter
pub const MAC_PARAM: u16 = 6; // #
pub const SUP_MARK: u16 = 7; // ^
pub const SUB_MARK: u16 = 8; // _
pub const IGNORE: u16 = 9; // ^^@
pub const ENDV: u16 = 9; // end of v_j list in alignment template
pub const SPACER: u16 = 10;
pub const LETTER: u16 = 11;
pub const OTHER_CHAR: u16 = 12;
pub const ACTIVE_CHAR: u16 = 13;
pub const PAR_END: u16 = 13; // \par
pub const MATCH: u16 = 13; // match a macro parameter
pub const COMMENT: u16 = 14;
pub const END_MATCH: u16 = 14; // end of parameters to macro
pub const STOP: u16 = 14; // \end, \dump
pub const INVALID_CHAR: u16 = 15; // ^^?
pub const DELIM_NUM: u16 = 15; // \delimiter
pub const MAX_CHAR_CODE: u16 = 15; // largest catcode for individual characters

// §208: ordinary command codes.
pub const CHAR_NUM: u16 = 16; // \char
pub const MATH_CHAR_NUM: u16 = 17; // \mathchar
pub const MARK: u16 = 18; // \mark
pub const XRAY: u16 = 19; // \show, \showbox, ...
pub const MAKE_BOX: u16 = 20; // \box, \copy, \hbox, ...
pub const HMOVE: u16 = 21;
pub const VMOVE: u16 = 22;
pub const UN_HBOX: u16 = 23;
pub const UN_VBOX: u16 = 24;
pub const REMOVE_ITEM: u16 = 25; // \unpenalty, \unkern, \unskip
pub const HSKIP: u16 = 26;
pub const VSKIP: u16 = 27;
pub const MSKIP: u16 = 28;
pub const KERN: u16 = 29;
pub const MKERN: u16 = 30;
pub const LEADER_SHIP: u16 = 31; // \shipout, \leaders, ...
pub const HALIGN: u16 = 32;
pub const VALIGN: u16 = 33;
pub const NO_ALIGN: u16 = 34;
pub const VRULE: u16 = 35;
pub const HRULE: u16 = 36;
pub const INSERT: u16 = 37;
pub const VADJUST: u16 = 38;
pub const IGNORE_SPACES: u16 = 39;
pub const AFTER_ASSIGNMENT: u16 = 40;
pub const AFTER_GROUP: u16 = 41;
pub const BREAK_PENALTY: u16 = 42;
pub const START_PAR: u16 = 43; // \indent, \noindent
pub const ITAL_CORR: u16 = 44; // \/
pub const ACCENT: u16 = 45;
pub const MATH_ACCENT: u16 = 46;
pub const DISCRETIONARY: u16 = 47;
pub const EQ_NO: u16 = 48;
pub const LEFT_RIGHT: u16 = 49;
pub const MATH_COMP: u16 = 50;
pub const LIMIT_SWITCH: u16 = 51;
pub const ABOVE: u16 = 52;
pub const MATH_STYLE: u16 = 53;
pub const MATH_CHOICE: u16 = 54;
pub const NON_SCRIPT: u16 = 55;
pub const VCENTER: u16 = 56;
pub const CASE_SHIFT: u16 = 57; // \lowercase, \uppercase
pub const MESSAGE: u16 = 58; // \message, \errmessage
pub const EXTENSION: u16 = 59; // \write, \special, ...
pub const IN_STREAM: u16 = 60; // \openin, \closein
pub const BEGIN_GROUP: u16 = 61;
pub const END_GROUP: u16 = 62;
pub const OMIT: u16 = 63;
pub const EX_SPACE: u16 = 64; // "\ "
pub const NO_BOUNDARY: u16 = 65;
pub const RADICAL: u16 = 66;
pub const END_CS_NAME: u16 = 67;
pub const MIN_INTERNAL: u16 = 68; // smallest code that can follow \the
pub const CHAR_GIVEN: u16 = 68; // \chardef'd
pub const MATH_GIVEN: u16 = 69; // \mathchardef'd
pub const XETEX_MATH_GIVEN: u16 = 70; // \Umathchardef'd (xetex.web §5003)
pub const LAST_ITEM: u16 = 71; // \lastpenalty, \lastkern, \lastskip
pub const MAX_NON_PREFIXED_COMMAND: u16 = 71;

// §209: mode-independent assignment commands.
pub const TOKS_REGISTER: u16 = 72; // \toks
pub const ASSIGN_TOKS: u16 = 73; // \output, \everypar, ...
pub const ASSIGN_INT: u16 = 74; // \tolerance, \day, ...
pub const ASSIGN_DIMEN: u16 = 75; // \hsize, ...
pub const ASSIGN_GLUE: u16 = 76; // \baselineskip, ...
pub const ASSIGN_MU_GLUE: u16 = 77; // \thinmuskip, ...
pub const ASSIGN_FONT_DIMEN: u16 = 78; // \fontdimen
pub const ASSIGN_FONT_INT: u16 = 79; // \hyphenchar, \skewchar
pub const SET_AUX: u16 = 80; // \spacefactor, \prevdepth
pub const SET_PREV_GRAF: u16 = 81;
pub const SET_PAGE_DIMEN: u16 = 82;
pub const SET_PAGE_INT: u16 = 83; // \deadcycles, \insertpenalties
pub const SET_BOX_DIMEN: u16 = 84; // \wd, \ht, \dp
pub const SET_SHAPE: u16 = 85; // \parshape
pub const DEF_CODE: u16 = 86; // \catcode, ...
pub const XETEX_DEF_CODE: u16 = 87; // \Umathcode family (xetex.web §5032: internal)
pub const DEF_FAMILY: u16 = 88; // \textfont, ...
pub const SET_FONT: u16 = 89; // font identifiers
pub const DEF_FONT: u16 = 90; // \font
pub const REGISTER: u16 = 91; // \count, \dimen, ...
pub const MAX_INTERNAL: u16 = 91; // largest code that can follow \the
pub const ADVANCE: u16 = 92;
pub const MULTIPLY: u16 = 93;
pub const DIVIDE: u16 = 94;
pub const PREFIX: u16 = 95; // \global, \long, \outer
pub const LET: u16 = 96; // \let, \futurelet
pub const SHORTHAND_DEF: u16 = 97; // \chardef, \countdef, ...
pub const READ_TO_CS: u16 = 98; // \read
pub const DEF: u16 = 99; // \def, \gdef, \xdef, \edef
pub const SET_BOX: u16 = 100; // \setbox
pub const HYPH_DATA: u16 = 101; // \hyphenation, \patterns
pub const SET_INTERACTION: u16 = 102; // \batchmode, ...
pub const SET_AUTO_SPACING: u16 = 103; // pTeX autospacing family
pub const ASSIGN_KINSOKU: u16 = 104; // pTeX prebreakpenalty family
pub const ASSIGN_INHIBIT_XSP: u16 = 105; // pTeX inhibitxspcode
pub const INHIBIT_GLUE: u16 = 106; // pTeX inhibitglue
pub const KCHAR_NUM: u16 = 107; // pTeX kchar
pub const MAX_COMMAND: u16 = 107; // largest command code seen at big_switch

// NOTE: command codes are baked into format files (eq_type values).
// When adding a primitive command, bump the fmt magic in engine.rs
// (SabiTeXfmtN) so stale engine/format pairs are rejected instead of
// misbehaving (a mismatched pair once broke every \if in LaTeX).

// §210: command codes that never reach main control.
pub const UNDEFINED_CS: u16 = MAX_COMMAND + 1;
pub const EXPAND_AFTER: u16 = MAX_COMMAND + 2;
pub const NO_EXPAND: u16 = MAX_COMMAND + 3;
pub const INPUT: u16 = MAX_COMMAND + 4; // \input, \endinput
pub const IF_TEST: u16 = MAX_COMMAND + 5; // \if, \ifcase, ...
pub const FI_OR_ELSE: u16 = MAX_COMMAND + 6; // \else, \or, \fi
pub const CS_NAME: u16 = MAX_COMMAND + 7; // \csname
pub const CONVERT: u16 = MAX_COMMAND + 8; // \number, \string, ...
pub const THE: u16 = MAX_COMMAND + 9; // \the
pub const TOP_BOT_MARK: u16 = MAX_COMMAND + 10; // \topmark, ...
pub const CALL: u16 = MAX_COMMAND + 11; // non-long, non-outer macro
pub const LONG_CALL: u16 = MAX_COMMAND + 12;
pub const OUTER_CALL: u16 = MAX_COMMAND + 13;
pub const LONG_OUTER_CALL: u16 = MAX_COMMAND + 14;
pub const END_TEMPLATE: u16 = MAX_COMMAND + 15;
pub const DONT_EXPAND: u16 = MAX_COMMAND + 16; // token marked by \noexpand
pub const GLUE_REF: u16 = MAX_COMMAND + 17;
pub const SHAPE_REF: u16 = MAX_COMMAND + 18;
pub const BOX_REF: u16 = MAX_COMMAND + 19;
pub const DATA: u16 = MAX_COMMAND + 20;
