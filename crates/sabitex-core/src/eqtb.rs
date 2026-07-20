//! The table of equivalents, the hash table, and the save stack.
//!
//! Ports tex.web Part 17 (§220-§255), Part 18 (§256-§267) and Part 19
//! (§268-§282). The six-region layout of `eqtb` is kept exactly, but the
//! character-indexed regions (active characters, single-character control
//! sequences, `\catcode`/`\lccode`/`\uccode`/`\sfcode`/`\mathcode`/`\delcode`
//! tables) are sized for all Unicode scalar values (`NUMBER_USVS`), the
//! XeTeX layout. This costs ~70 MB per engine; a sparse representation can
//! replace it behind the same accessors later (wasm size work, M7+).
//!
//! Region boundaries are computed at engine construction (they depend on
//! the run-time `hash_size`/`font_max`) and live in [`Layout`].

use crate::memword::MemoryWord;
use crate::types::{Halfword, Pointer, Scaled, StrNumber, NULL};

/// Number of Unicode scalar values (XeTeX's `number_usvs`).
pub const NUMBER_USVS: i32 = 0x11_0000;

/// `level_zero`: level for undefined quantities (§221).
pub const LEVEL_ZERO: u16 = 0;
/// `level_one`: outermost level for defined quantities (§221).
pub const LEVEL_ONE: u16 = 1;

// §222: glue parameter codes (region 3).
pub const LINE_SKIP_CODE: i32 = 0;
pub const BASELINE_SKIP_CODE: i32 = 1;
pub const PAR_SKIP_CODE: i32 = 2;
pub const ABOVE_DISPLAY_SKIP_CODE: i32 = 3;
pub const BELOW_DISPLAY_SKIP_CODE: i32 = 4;
pub const ABOVE_DISPLAY_SHORT_SKIP_CODE: i32 = 5;
pub const BELOW_DISPLAY_SHORT_SKIP_CODE: i32 = 6;
pub const LEFT_SKIP_CODE: i32 = 7;
pub const RIGHT_SKIP_CODE: i32 = 8;
pub const TOP_SKIP_CODE: i32 = 9;
pub const SPLIT_TOP_SKIP_CODE: i32 = 10;
pub const TAB_SKIP_CODE: i32 = 11;
pub const SPACE_SKIP_CODE: i32 = 12;
pub const XSPACE_SKIP_CODE: i32 = 13;
pub const PAR_FILL_SKIP_CODE: i32 = 14;
pub const THIN_MU_SKIP_CODE: i32 = 15;
pub const MED_MU_SKIP_CODE: i32 = 16;
pub const THICK_MU_SKIP_CODE: i32 = 17;
// pTeX: the Japanese inter-character glues.
pub const KANJI_SKIP_CODE: i32 = 18;
pub const XKANJI_SKIP_CODE: i32 = 19;
pub const TEX_GLUE_PARS: i32 = 18;
pub const GLUE_PARS: i32 = 20;

/// §225: symbolic names of the glue parameters, indexed by code.
pub const SKIP_PARAM_NAMES: [&str; GLUE_PARS as usize] = [
    "lineskip",
    "baselineskip",
    "parskip",
    "abovedisplayskip",
    "belowdisplayskip",
    "abovedisplayshortskip",
    "belowdisplayshortskip",
    "leftskip",
    "rightskip",
    "topskip",
    "splittopskip",
    "tabskip",
    "spaceskip",
    "xspaceskip",
    "parfillskip",
    "thinmuskip",
    "medmuskip",
    "thickmuskip",
    "kanjiskip",
    "xkanjiskip",
];

// §230: region-4 local locations (offsets from local_base).
pub const PAR_SHAPE_OFFSET: i32 = 0;
pub const OUTPUT_ROUTINE_OFFSET: i32 = 1;
pub const EVERY_PAR_OFFSET: i32 = 2;
pub const EVERY_MATH_OFFSET: i32 = 3;
pub const EVERY_DISPLAY_OFFSET: i32 = 4;
pub const EVERY_HBOX_OFFSET: i32 = 5;
pub const EVERY_VBOX_OFFSET: i32 = 6;
pub const EVERY_JOB_OFFSET: i32 = 7;
pub const EVERY_CR_OFFSET: i32 = 8;
pub const ERR_HELP_OFFSET: i32 = 9;
/// etex.ch §230: e-TeX's token-list parameters sit between TeX82's and the
/// `\toks` registers (`etex_toks_base == local_base + 10`).
pub const EVERY_EOF_OFFSET: i32 = 10;
pub const TOKS_PARS: i32 = 10; // output_routine..every_eof

/// §231: token-list parameter names, indexed from `output_routine_loc`.
pub const TOKS_PARAM_NAMES: [&str; 10] = [
    "output",
    "everypar",
    "everymath",
    "everydisplay",
    "everyhbox",
    "everyvbox",
    "everyjob",
    "everycr",
    "errhelp",
    "everyeof",
];

/// `var_code` (§232): math code meaning "use the current family".
pub const VAR_CODE: i32 = 0o70000;

// §236: integer parameter codes (region 5).
pub const PRETOLERANCE_CODE: i32 = 0;
pub const TOLERANCE_CODE: i32 = 1;
pub const LINE_PENALTY_CODE: i32 = 2;
pub const HYPHEN_PENALTY_CODE: i32 = 3;
pub const EX_HYPHEN_PENALTY_CODE: i32 = 4;
pub const CLUB_PENALTY_CODE: i32 = 5;
pub const WIDOW_PENALTY_CODE: i32 = 6;
pub const DISPLAY_WIDOW_PENALTY_CODE: i32 = 7;
pub const BROKEN_PENALTY_CODE: i32 = 8;
pub const BIN_OP_PENALTY_CODE: i32 = 9;
pub const REL_PENALTY_CODE: i32 = 10;
pub const PRE_DISPLAY_PENALTY_CODE: i32 = 11;
pub const POST_DISPLAY_PENALTY_CODE: i32 = 12;
pub const INTER_LINE_PENALTY_CODE: i32 = 13;
pub const DOUBLE_HYPHEN_DEMERITS_CODE: i32 = 14;
pub const FINAL_HYPHEN_DEMERITS_CODE: i32 = 15;
pub const ADJ_DEMERITS_CODE: i32 = 16;
pub const MAG_CODE: i32 = 17;
pub const DELIMITER_FACTOR_CODE: i32 = 18;
pub const LOOSENESS_CODE: i32 = 19;
pub const TIME_CODE: i32 = 20;
pub const DAY_CODE: i32 = 21;
pub const MONTH_CODE: i32 = 22;
pub const YEAR_CODE: i32 = 23;
pub const SHOW_BOX_BREADTH_CODE: i32 = 24;
pub const SHOW_BOX_DEPTH_CODE: i32 = 25;
pub const HBADNESS_CODE: i32 = 26;
pub const VBADNESS_CODE: i32 = 27;
pub const PAUSING_CODE: i32 = 28;
pub const TRACING_ONLINE_CODE: i32 = 29;
pub const TRACING_MACROS_CODE: i32 = 30;
pub const TRACING_STATS_CODE: i32 = 31;
pub const TRACING_PARAGRAPHS_CODE: i32 = 32;
pub const TRACING_PAGES_CODE: i32 = 33;
pub const TRACING_OUTPUT_CODE: i32 = 34;
pub const TRACING_LOST_CHARS_CODE: i32 = 35;
pub const TRACING_COMMANDS_CODE: i32 = 36;
pub const TRACING_RESTORES_CODE: i32 = 37;
pub const UC_HYPH_CODE: i32 = 38;
pub const OUTPUT_PENALTY_CODE: i32 = 39;
pub const MAX_DEAD_CYCLES_CODE: i32 = 40;
pub const HANG_AFTER_CODE: i32 = 41;
pub const FLOATING_PENALTY_CODE: i32 = 42;
pub const GLOBAL_DEFS_CODE: i32 = 43;
pub const CUR_FAM_CODE: i32 = 44;
pub const ESCAPE_CHAR_CODE: i32 = 45;
pub const DEFAULT_HYPHEN_CHAR_CODE: i32 = 46;
pub const DEFAULT_SKEW_CHAR_CODE: i32 = 47;
pub const END_LINE_CHAR_CODE: i32 = 48;
pub const NEW_LINE_CHAR_CODE: i32 = 49;
pub const LANGUAGE_CODE: i32 = 50;
pub const LEFT_HYPHEN_MIN_CODE: i32 = 51;
pub const RIGHT_HYPHEN_MIN_CODE: i32 = 52;
pub const HOLDING_INSERTS_CODE: i32 = 53;
pub const ERROR_CONTEXT_LINES_CODE: i32 = 54;
/// `tex_int_pars` (etex.ch): end of TeX82's integer parameters.
pub const TEX_INT_PARS: i32 = 55;
// etex.ch: e-TeX's integer parameters (etex_int_base..).
pub const TRACING_ASSIGNS_CODE: i32 = TEX_INT_PARS;
pub const TRACING_GROUPS_CODE: i32 = TEX_INT_PARS + 1;
pub const TRACING_IFS_CODE: i32 = TEX_INT_PARS + 2;
pub const TRACING_SCAN_TOKENS_CODE: i32 = TEX_INT_PARS + 3;
pub const TRACING_NESTING_CODE: i32 = TEX_INT_PARS + 4;
pub const PRE_DISPLAY_DIRECTION_CODE: i32 = TEX_INT_PARS + 5;
pub const LAST_LINE_FIT_CODE: i32 = TEX_INT_PARS + 6;
pub const SAVING_VDISCARDS_CODE: i32 = TEX_INT_PARS + 7;
pub const SAVING_HYPH_CODES_CODE: i32 = TEX_INT_PARS + 8;
/// `eTeX_state_code` (etex.ch): the e-TeX state variables (1 so far).
pub const ETEX_STATE_CODE: i32 = TEX_INT_PARS + 9;
pub const ETEX_STATES: i32 = 1; // \TeXXeTstate
/// pTeX \jcharwidowpenalty.
pub const JCHR_WIDOW_PENALTY_CODE: i32 = ETEX_STATE_CODE + ETEX_STATES;
pub const INT_PARS: i32 = JCHR_WIDOW_PENALTY_CODE + 1;

/// §237: integer parameter names, indexed by code.
pub const INT_PARAM_NAMES: [&str; INT_PARS as usize] = [
    "pretolerance",
    "tolerance",
    "linepenalty",
    "hyphenpenalty",
    "exhyphenpenalty",
    "clubpenalty",
    "widowpenalty",
    "displaywidowpenalty",
    "brokenpenalty",
    "binoppenalty",
    "relpenalty",
    "predisplaypenalty",
    "postdisplaypenalty",
    "interlinepenalty",
    "doublehyphendemerits",
    "finalhyphendemerits",
    "adjdemerits",
    "mag",
    "delimiterfactor",
    "looseness",
    "time",
    "day",
    "month",
    "year",
    "showboxbreadth",
    "showboxdepth",
    "hbadness",
    "vbadness",
    "pausing",
    "tracingonline",
    "tracingmacros",
    "tracingstats",
    "tracingparagraphs",
    "tracingpages",
    "tracingoutput",
    "tracinglostchars",
    "tracingcommands",
    "tracingrestores",
    "uchyph",
    "outputpenalty",
    "maxdeadcycles",
    "hangafter",
    "floatingpenalty",
    "globaldefs",
    "fam",
    "escapechar",
    "defaulthyphenchar",
    "defaultskewchar",
    "endlinechar",
    "newlinechar",
    "language",
    "lefthyphenmin",
    "righthyphenmin",
    "holdinginserts",
    "errorcontextlines",
    // etex.ch print_param additions.
    "tracingassigns",
    "tracinggroups",
    "tracingifs",
    "tracingscantokens",
    "tracingnesting",
    "predisplaydirection",
    "lastlinefit",
    "savingvdiscards",
    "savinghyphcodes",
    "TeXXeTstate",
    "jcharwidowpenalty",
];

// §247: dimension parameter codes (region 6).
pub const PAR_INDENT_CODE: i32 = 0;
pub const MATH_SURROUND_CODE: i32 = 1;
pub const LINE_SKIP_LIMIT_CODE: i32 = 2;
pub const HSIZE_CODE: i32 = 3;
pub const VSIZE_CODE: i32 = 4;
pub const MAX_DEPTH_CODE: i32 = 5;
pub const SPLIT_MAX_DEPTH_CODE: i32 = 6;
pub const BOX_MAX_DEPTH_CODE: i32 = 7;
pub const HFUZZ_CODE: i32 = 8;
pub const VFUZZ_CODE: i32 = 9;
pub const DELIMITER_SHORTFALL_CODE: i32 = 10;
pub const NULL_DELIMITER_SPACE_CODE: i32 = 11;
pub const SCRIPT_SPACE_CODE: i32 = 12;
pub const PRE_DISPLAY_SIZE_CODE: i32 = 13;
pub const DISPLAY_WIDTH_CODE: i32 = 14;
pub const DISPLAY_INDENT_CODE: i32 = 15;
pub const OVERFULL_RULE_CODE: i32 = 16;
pub const HANG_INDENT_CODE: i32 = 17;
pub const H_OFFSET_CODE: i32 = 18;
pub const V_OFFSET_CODE: i32 = 19;
pub const EMERGENCY_STRETCH_CODE: i32 = 20;
// pdfTeX/XeTeX: the page dimensions (used by dvipdfmx drivers).
pub const PDF_PAGE_WIDTH_CODE: i32 = 21;
pub const PDF_PAGE_HEIGHT_CODE: i32 = 22;
pub const TEX_DIMEN_PARS: i32 = 21;
pub const DIMEN_PARS: i32 = 23;

/// §248: dimension parameter names, indexed by code.
pub const DIMEN_PARAM_NAMES: [&str; DIMEN_PARS as usize] = [
    "parindent",
    "mathsurround",
    "lineskiplimit",
    "hsize",
    "vsize",
    "maxdepth",
    "splitmaxdepth",
    "boxmaxdepth",
    "hfuzz",
    "vfuzz",
    "delimitershortfall",
    "nulldelimiterspace",
    "scriptspace",
    "predisplaysize",
    "displaywidth",
    "displayindent",
    "overfullrule",
    "hangindent",
    "hoffset",
    "voffset",
    "emergencystretch",
    "pdfpagewidth",
    "pdfpageheight",
];

/// The computed region boundaries of `eqtb` (§220-§247 `@d` constants).
/// All values follow the tex.web formulas with 256 → `NUMBER_USVS` for the
/// character-indexed tables. Registers stay at 256 (e-TeX widens them, M6).
#[derive(Clone, Debug)]
pub struct Layout {
    pub active_base: Pointer,
    pub single_base: Pointer,
    pub null_cs: Pointer,
    pub hash_base: Pointer,
    pub hash_size: i32,
    pub hash_prime: i32,
    pub frozen_control_sequence: Pointer,
    pub frozen_protection: Pointer,
    pub frozen_cr: Pointer,
    pub frozen_end_group: Pointer,
    pub frozen_right: Pointer,
    pub frozen_fi: Pointer,
    pub frozen_end_template: Pointer,
    pub frozen_endv: Pointer,
    pub frozen_relax: Pointer,
    pub end_write: Pointer,
    pub frozen_dont_expand: Pointer,
    pub frozen_null_font: Pointer,
    pub font_id_base: Pointer,
    pub undefined_control_sequence: Pointer,
    pub glue_base: Pointer,
    pub skip_base: Pointer,
    pub mu_skip_base: Pointer,
    pub local_base: Pointer,
    pub par_shape_loc: Pointer,
    pub output_routine_loc: Pointer,
    pub toks_base: Pointer,
    /// `etex_pen_base` (etex.ch): \interlinepenalties .. (4 shape slots).
    pub etex_pen_base: Pointer,
    pub box_base: Pointer,
    pub cur_font_loc: Pointer,
    /// pTeX `cur_jfont_loc`.
    pub cur_jfont_loc: Pointer,
    /// pTeX `auto_spacing` / `auto_xspacing` state slots.
    pub auto_spacing_loc: Pointer,
    pub auto_xspacing_loc: Pointer,
    /// pTeX kinsoku hash (1024) and its penalties (1024 words).
    pub kinsoku_base: Pointer,
    pub kinsoku_penalty_base: Pointer,
    /// pTeX \xspcode (256) and \inhibitxspcode hash (1024).
    pub auto_xsp_code_base: Pointer,
    pub inhibit_xsp_code_base: Pointer,
    /// pTeX \kansujichar (10 digits).
    pub kansuji_base: Pointer,
    /// upTeX `kcat_code_base` (512 entries by kcatcodekey).
    pub kcat_code_base: Pointer,
    pub math_font_base: Pointer,
    pub cat_code_base: Pointer,
    pub lc_code_base: Pointer,
    pub uc_code_base: Pointer,
    pub sf_code_base: Pointer,
    pub math_code_base: Pointer,
    pub int_base: Pointer,
    pub count_base: Pointer,
    pub del_code_base: Pointer,
    pub dimen_base: Pointer,
    pub scaled_base: Pointer,
    pub eqtb_size: Pointer,
}

impl Layout {
    pub fn new(hash_size: i32, hash_prime: i32, font_max: i32) -> Layout {
        let active_base = 1;
        let single_base = active_base + NUMBER_USVS;
        let null_cs = single_base + NUMBER_USVS;
        let hash_base = null_cs + 1;
        let frozen_control_sequence = hash_base + hash_size;
        let frozen_null_font = frozen_control_sequence + 10;
        let font_id_base = frozen_null_font; // font_base = 0
        let undefined_control_sequence = frozen_null_font + font_max + 2;
        let glue_base = undefined_control_sequence + 1;
        let skip_base = glue_base + GLUE_PARS;
        let mu_skip_base = skip_base + 256;
        let local_base = mu_skip_base + 256;
        // etex.ch: one extra token-list parameter (\everyeof) precedes the
        // \toks registers.
        let toks_base = local_base + 11;
        // etex.ch: four penalties arrays (\interlinepenalties etc.) sit
        // between the \toks and \box registers.
        let etex_pen_base = toks_base + 256;
        let box_base = etex_pen_base + 4;
        let cur_font_loc = box_base + 256;
        // pTeX: the current Japanese font sits beside \font's slot,
        // with the \autospacing / \autoxspacing state flags after it.
        let cur_jfont_loc = cur_font_loc + 1;
        let auto_spacing_loc = cur_jfont_loc + 1;
        let auto_xspacing_loc = auto_spacing_loc + 1;
        let math_font_base = auto_xspacing_loc + 1;
        let cat_code_base = math_font_base + 768; // 256 families x 3 sizes (xetex)
                                                  // upTeX: 512 \kcatcode entries, indexed by kcatcodekey.
        let kcat_code_base = cat_code_base + NUMBER_USVS;
        let lc_code_base = kcat_code_base + crate::kanji::KCAT_ENTRIES;
        let uc_code_base = lc_code_base + NUMBER_USVS;
        let sf_code_base = uc_code_base + NUMBER_USVS;
        let math_code_base = sf_code_base + NUMBER_USVS;
        let int_base = math_code_base + NUMBER_USVS;
        let count_base = int_base + INT_PARS;
        let del_code_base = count_base + 256;
        let dimen_base = del_code_base + NUMBER_USVS;
        let scaled_base = dimen_base + DIMEN_PARS;
        // pTeX: 1024 kinsoku hash slots (character in equiv, pre/post in
        // eq_type) plus their penalty values as word entries.
        let kinsoku_base = scaled_base + 256;
        let kinsoku_penalty_base = kinsoku_base + 1024;
        // pTeX: \xspcode per ASCII character and the \inhibitxspcode
        // hash (character in equiv — 0 means empty — and 0..4 in
        // eq_type).
        let auto_xsp_code_base = kinsoku_penalty_base + 1024;
        let inhibit_xsp_code_base = auto_xsp_code_base + 256;
        // pTeX: \kansujichar digits 0-9.
        let kansuji_base = inhibit_xsp_code_base + 1024;
        let eqtb_size = kansuji_base + 9;
        Layout {
            active_base,
            single_base,
            null_cs,
            hash_base,
            hash_size,
            hash_prime,
            frozen_control_sequence,
            frozen_protection: frozen_control_sequence,
            frozen_cr: frozen_control_sequence + 1,
            frozen_end_group: frozen_control_sequence + 2,
            frozen_right: frozen_control_sequence + 3,
            frozen_fi: frozen_control_sequence + 4,
            frozen_end_template: frozen_control_sequence + 5,
            frozen_endv: frozen_control_sequence + 6,
            frozen_relax: frozen_control_sequence + 7,
            end_write: frozen_control_sequence + 8,
            frozen_dont_expand: frozen_control_sequence + 9,
            frozen_null_font,
            font_id_base,
            undefined_control_sequence,
            glue_base,
            skip_base,
            mu_skip_base,
            local_base,
            par_shape_loc: local_base + PAR_SHAPE_OFFSET,
            output_routine_loc: local_base + OUTPUT_ROUTINE_OFFSET,
            toks_base,
            etex_pen_base,
            box_base,
            cur_font_loc,
            cur_jfont_loc,
            auto_spacing_loc,
            auto_xspacing_loc,
            kinsoku_base,
            kinsoku_penalty_base,
            auto_xsp_code_base,
            inhibit_xsp_code_base,
            kansuji_base,
            kcat_code_base,
            math_font_base,
            cat_code_base,
            lc_code_base,
            uc_code_base,
            sf_code_base,
            math_code_base,
            int_base,
            count_base,
            del_code_base,
            dimen_base,
            scaled_base,
            eqtb_size,
        }
    }
}

/// The table of equivalents plus the hash table (§253, §256).
pub struct Eqtb {
    pub lay: Layout,
    /// `eqtb[active_base..=eqtb_size]`, indexed directly by pointer
    /// (entry 0 is unused, like tex.web's).
    table: Vec<MemoryWord>,
    /// `xeq_level[int_base..=eqtb_size]`, indexed by `p - int_base` (§253).
    xeq_level: Vec<u16>,
    /// `hash[hash_base..undefined_control_sequence-1]`, indexed by
    /// `p - hash_base` (§256). `lh` = next, `rh` = text.
    hash: Vec<MemoryWord>,
    /// `hash_used`: allocation pointer for `hash` (§256).
    pub hash_used: Pointer,
    /// `no_new_control_sequence` (§256).
    pub no_new_control_sequence: bool,
    /// `cs_count`: total number of multiletter control sequences (§256).
    pub cs_count: i32,
}

impl Eqtb {
    /// Builds the table with every entry undefined; the INITEX defaults of
    /// §232/§240/§250/§254 are applied by `Engine::initialize_eqtb` (they
    /// need `mem` for `zero_glue`).
    pub fn new(lay: Layout) -> Eqtb {
        let size = lay.eqtb_size as usize + 1;
        let hash_len = (lay.undefined_control_sequence - lay.hash_base) as usize;
        let xeq_len = (lay.eqtb_size - lay.int_base + 1) as usize;
        Eqtb {
            hash_used: lay.frozen_control_sequence,
            no_new_control_sequence: true,
            cs_count: 0,
            table: vec![MemoryWord::ZERO; size],
            xeq_level: vec![LEVEL_ONE; xeq_len],
            hash: vec![MemoryWord::ZERO; hash_len],
            lay,
        }
    }

    /// `eqtb[p]` (whole word).
    pub fn word(&self, p: Pointer) -> MemoryWord {
        self.table[p as usize]
    }

    /// `eqtb[p]` for writing.
    pub fn word_mut(&mut self, p: Pointer) -> &mut MemoryWord {
        &mut self.table[p as usize]
    }

    /// `eq_level(p)` (§221).
    pub fn eq_level(&self, p: Pointer) -> u16 {
        self.table[p as usize].b1()
    }

    pub fn set_eq_level(&mut self, p: Pointer, l: u16) {
        self.table[p as usize].set_b1(l);
    }

    /// §1313-§1318: dump the table of equivalents and the hash table.
    pub fn dump(&self, w: &mut crate::fmt::FmtWriter) {
        // §1315-§1316 spirit: the table is mostly runs of identical
        // words (undefined regions, default codes) — run-length encode.
        w.len_of(self.table.len());
        let mut i = 0;
        while i < self.table.len() {
            // arithmetic run: bits step by a constant delta (covers the
            // constant case delta=0 and the math_code/lc_code ramps).
            let start = self.table[i].bits();
            let mut j = i + 1;
            let delta = if j < self.table.len() {
                self.table[j].bits().wrapping_sub(start)
            } else {
                0
            };
            let mut expect = start.wrapping_add(delta);
            while j < self.table.len() && self.table[j].bits() == expect {
                j += 1;
                expect = expect.wrapping_add(delta);
            }
            w.u64((j - i) as u64);
            w.u64(start);
            w.u64(delta);
            i = j;
        }
        w.len_of(self.xeq_level.len());
        let mut i = 0;
        while i < self.xeq_level.len() {
            let mut j = i + 1;
            while j < self.xeq_level.len() && self.xeq_level[j] == self.xeq_level[i] {
                j += 1;
            }
            w.u64((j - i) as u64);
            w.u16(self.xeq_level[i]);
            i = j;
        }
        w.words(&self.hash);
        w.i32(self.hash_used);
        w.i32(self.cs_count);
    }

    /// §1314-§1319: undump them (the layout must match).
    pub fn undump(&mut self, r: &mut crate::fmt::FmtReader) -> crate::fmt::FmtResult<()> {
        let n = r.seq_len()?;
        if n != self.table.len() {
            return Err("eqtb size mismatch");
        }
        let mut i = 0;
        while i < n {
            let run = r.u64()? as usize;
            let start = r.u64()?;
            let delta = r.u64()?;
            if i + run > n {
                return Err("eqtb run overflow");
            }
            if delta == 0 {
                self.table[i..i + run].fill(crate::memword::MemoryWord::from_bits(start));
            } else {
                let mut v = start;
                for k in i..i + run {
                    self.table[k] = crate::memword::MemoryWord::from_bits(v);
                    v = v.wrapping_add(delta);
                }
            }
            i += run;
        }
        let n = r.seq_len()?;
        if n != self.xeq_level.len() {
            return Err("xeq_level size mismatch");
        }
        let mut i = 0;
        while i < n {
            let run = r.u64()? as usize;
            let v = r.u16()?;
            if i + run > n {
                return Err("xeq run overflow");
            }
            self.xeq_level[i..i + run].fill(v);
            i += run;
        }
        let hash = r.words()?;
        if hash.len() != self.hash.len() {
            return Err("hash size mismatch");
        }
        self.hash = hash;
        self.hash_used = r.i32()?;
        self.cs_count = r.i32()?;
        Ok(())
    }

    /// `eq_type(p)` (§221).
    pub fn eq_type(&self, p: Pointer) -> u16 {
        self.table[p as usize].b0()
    }

    pub fn set_eq_type(&mut self, p: Pointer, t: u16) {
        self.table[p as usize].set_b0(t);
    }

    /// `equiv(p)` (§221).
    pub fn equiv(&self, p: Pointer) -> Halfword {
        self.table[p as usize].rh()
    }

    pub fn set_equiv(&mut self, p: Pointer, e: Halfword) {
        self.table[p as usize].set_rh(e);
    }

    /// `eqtb[p].int` (regions 5 and 6).
    pub fn int(&self, p: Pointer) -> i32 {
        self.table[p as usize].int()
    }

    pub fn set_int(&mut self, p: Pointer, v: i32) {
        self.table[p as usize].set_int(v);
    }

    /// `xeq_level[p]` (§253).
    pub fn xeq_level(&self, p: Pointer) -> u16 {
        self.xeq_level[(p - self.lay.int_base) as usize]
    }

    pub fn set_xeq_level(&mut self, p: Pointer, l: u16) {
        self.xeq_level[(p - self.lay.int_base) as usize] = l;
    }

    // Convenience accessors for the named tables (§224, §230, §236, §247).

    /// `int_par(n)`.
    pub fn int_par(&self, n: i32) -> i32 {
        self.int(self.lay.int_base + n)
    }

    /// `dimen_par(n)`.
    pub fn dimen_par(&self, n: i32) -> Scaled {
        self.int(self.lay.dimen_base + n)
    }

    /// `glue_par(n)`: `mem` location of the glue specification.
    pub fn glue_par(&self, n: i32) -> Pointer {
        self.equiv(self.lay.glue_base + n)
    }

    /// `count(n)`.
    pub fn count(&self, n: i32) -> i32 {
        self.int(self.lay.count_base + n)
    }

    /// `dimen(n)`.
    pub fn dimen(&self, n: i32) -> Scaled {
        self.int(self.lay.scaled_base + n)
    }

    /// upTeX `kcat_code(k)` — k is a kcatcodekey, not a USV.
    pub fn kcat_code(&self, k: i32) -> i32 {
        self.equiv(self.lay.kcat_code_base + k)
    }

    pub fn set_kcat_code(&mut self, k: i32, v: i32) {
        let loc = self.lay.kcat_code_base + k;
        self.set_equiv(loc, v);
    }

    /// `cat_code(c)`.
    pub fn cat_code(&self, c: i32) -> i32 {
        self.equiv(self.lay.cat_code_base + c)
    }

    pub fn set_cat_code(&mut self, c: i32, v: i32) {
        self.set_equiv(self.lay.cat_code_base + c, v);
    }

    /// `lc_code(c)`.
    pub fn lc_code(&self, c: i32) -> i32 {
        self.equiv(self.lay.lc_code_base + c)
    }

    /// `uc_code(c)`.
    pub fn uc_code(&self, c: i32) -> i32 {
        self.equiv(self.lay.uc_code_base + c)
    }

    /// `sf_code(c)`.
    pub fn sf_code(&self, c: i32) -> i32 {
        self.equiv(self.lay.sf_code_base + c)
    }

    /// `math_code(c)` (stored value; `min_halfword = 0` so `hi`/`ho` are
    /// identities in this port).
    pub fn math_code(&self, c: i32) -> i32 {
        self.equiv(self.lay.math_code_base + c)
    }

    /// `del_code(c)`.
    pub fn del_code(&self, c: i32) -> i32 {
        self.int(self.lay.del_code_base + c)
    }

    /// `box(n)`.
    pub fn box_reg(&self, n: i32) -> Pointer {
        self.equiv(self.lay.box_base + n)
    }

    /// `toks(n)`.
    pub fn toks(&self, n: i32) -> Pointer {
        self.equiv(self.lay.toks_base + n)
    }

    /// `cur_font`.
    pub fn cur_font(&self) -> i32 {
        self.equiv(self.lay.cur_font_loc)
    }

    // The hash table (§256-§259).

    /// `next(p)`: link for coalesced lists.
    pub fn next(&self, p: Pointer) -> Pointer {
        self.hash[(p - self.lay.hash_base) as usize].lh()
    }

    pub fn set_next(&mut self, p: Pointer, v: Pointer) {
        self.hash[(p - self.lay.hash_base) as usize].set_lh(v);
    }

    /// `text(p)`: string number for the control sequence name.
    pub fn text(&self, p: Pointer) -> StrNumber {
        self.hash[(p - self.lay.hash_base) as usize].rh()
    }

    pub fn set_text(&mut self, p: Pointer, s: StrNumber) {
        self.hash[(p - self.lay.hash_base) as usize].set_rh(s);
    }

    /// `hash_is_full` (§256).
    pub fn hash_is_full(&self) -> bool {
        self.hash_used == self.lay.hash_base
    }

    /// `font_id_text(f)` (§256).
    pub fn font_id_text(&self, f: i32) -> StrNumber {
        self.text(self.lay.font_id_base + f)
    }
}

// §268: save_type values.
pub const RESTORE_OLD_VALUE: u16 = 0;
pub const RESTORE_ZERO: u16 = 1;
pub const INSERT_TOKEN: u16 = 2;
pub const LEVEL_BOUNDARY: u16 = 3;
/// `restore_sa` (etex.ch): restore sparse array entries.
pub const RESTORE_SA: u16 = 4;

// §269: group codes.
pub const BOTTOM_LEVEL: u16 = 0;
pub const SIMPLE_GROUP: u16 = 1;
pub const HBOX_GROUP: u16 = 2;
pub const ADJUSTED_HBOX_GROUP: u16 = 3;
pub const VBOX_GROUP: u16 = 4;
pub const VTOP_GROUP: u16 = 5;
pub const ALIGN_GROUP: u16 = 6;
pub const NO_ALIGN_GROUP: u16 = 7;
pub const OUTPUT_GROUP: u16 = 8;
pub const MATH_GROUP: u16 = 9;
pub const DISC_GROUP: u16 = 10;
pub const INSERT_GROUP: u16 = 11;
pub const VCENTER_GROUP: u16 = 12;
pub const MATH_CHOICE_GROUP: u16 = 13;
pub const SEMI_SIMPLE_GROUP: u16 = 14;
pub const MATH_SHIFT_GROUP: u16 = 15;
pub const MATH_LEFT_GROUP: u16 = 16;
pub const MAX_GROUP_CODE: u16 = 16;

/// The save stack and grouping state (§270-§272).
pub struct SaveStack {
    /// `save_stack`.
    pub stack: Vec<MemoryWord>,
    /// `save_ptr`: first unused entry.
    pub save_ptr: usize,
    /// `max_save_stack`.
    pub max_save_stack: usize,
    /// `save_size` limit.
    pub save_size: usize,
    /// `cur_level`: current nesting level for groups.
    pub cur_level: u16,
    /// `cur_group`: current group type.
    pub cur_group: u16,
    /// `cur_boundary`: where the current level begins.
    pub cur_boundary: i32,
}

impl SaveStack {
    pub fn new(save_size: usize) -> SaveStack {
        SaveStack {
            stack: vec![MemoryWord::ZERO; save_size + 1],
            save_ptr: 0,
            max_save_stack: 0,
            save_size,
            cur_level: LEVEL_ONE,
            cur_group: BOTTOM_LEVEL,
            cur_boundary: 0,
        }
    }

    /// `save_type(p)` (§268).
    pub fn save_type(&self, p: usize) -> u16 {
        self.stack[p].b0()
    }

    pub fn set_save_type(&mut self, p: usize, v: u16) {
        self.stack[p].set_b0(v);
    }

    /// `save_level(p)` (§268).
    pub fn save_level(&self, p: usize) -> u16 {
        self.stack[p].b1()
    }

    pub fn set_save_level(&mut self, p: usize, v: u16) {
        self.stack[p].set_b1(v);
    }

    /// `save_index(p)` (§268).
    pub fn save_index(&self, p: usize) -> Halfword {
        self.stack[p].rh()
    }

    pub fn set_save_index(&mut self, p: usize, v: Halfword) {
        self.stack[p].set_rh(v);
    }

    /// `saved(k)` (§274).
    pub fn saved(&self, k: i32) -> i32 {
        self.stack[(self.save_ptr as i32 + k) as usize].int()
    }

    pub fn set_saved(&mut self, k: i32, v: i32) {
        let i = (self.save_ptr as i32 + k) as usize;
        self.stack[i].set_int(v);
    }
}

impl crate::engine::Engine {
    /// `check_full_save_stack` (§273).
    fn check_full_save_stack(&mut self) -> crate::error::TexResult<()> {
        if self.save.save_ptr > self.save.max_save_stack {
            self.save.max_save_stack = self.save.save_ptr;
            // etex.ch: room for up to seven more entries.
            if self.save.max_save_stack > self.save.save_size - 7 {
                return Err(crate::error::TexInterrupt::Overflow {
                    what: "save size",
                    size: self.save.save_size as i32,
                });
            }
        }
        Ok(())
    }

    /// `new_save_level(c)` (§274 + etex.ch): begin a new level of grouping.
    pub fn new_save_level(&mut self, c: u16) -> crate::error::TexResult<()> {
        self.check_full_save_stack()?;
        if self.etex_ex() {
            // etex.ch: record the line where this group was entered.
            let line = self.inp.line;
            self.save.set_saved(0, line);
            self.save.save_ptr += 1;
        }
        let p = self.save.save_ptr;
        self.save.set_save_type(p, LEVEL_BOUNDARY);
        let g = self.save.cur_group;
        self.save.set_save_level(p, g);
        let b = self.save.cur_boundary;
        self.save.set_save_index(p, b);
        if self.save.cur_level == u16::MAX {
            return Err(crate::error::TexInterrupt::Overflow {
                what: "grouping levels",
                size: i32::from(u16::MAX),
            });
        }
        self.save.cur_boundary = self.save.save_ptr as i32;
        self.save.cur_group = c;
        if self.eqtb.int_par(TRACING_GROUPS_CODE) > 0 {
            self.group_trace(false);
        }
        self.save.cur_level += 1;
        self.save.save_ptr += 1;
        Ok(())
    }

    /// `eq_destroy(w)` (§275): gets ready to forget an eqtb word.
    pub fn eq_destroy(&mut self, w: MemoryWord) {
        use crate::cmds::*;
        let t = w.b0();
        let q = w.rh();
        if t == CALL || t == LONG_CALL || t == OUTER_CALL || t == LONG_OUTER_CALL {
            self.delete_token_ref(q);
        } else if t == GLUE_REF {
            self.delete_glue_ref(q);
        } else if t == SHAPE_REF {
            if q != NULL {
                let n = self.mem.info(q);
                self.mem.free_node(q, n + n + 1); // a \parshape block is 2n+1 words
            }
        } else if t == BOX_REF {
            self.flush_node_list(q);
        } else if (t == REGISTER || t == TOKS_REGISTER)
            && (q < self.mem.mem_bot || q > self.mem.lo_mem_stat_max())
        {
            // etex.ch: drop a shorthand reference to a sparse element.
            self.delete_sa_ref(q);
        }
    }

    /// `eq_save(p, l)` (§276): saves `eqtb[p]` established at level `l`.
    fn eq_save(&mut self, p: Pointer, l: u16) -> crate::error::TexResult<()> {
        self.check_full_save_stack()?;
        if l == LEVEL_ZERO {
            let sp = self.save.save_ptr;
            self.save.set_save_type(sp, RESTORE_ZERO);
        } else {
            self.save.stack[self.save.save_ptr] = self.eqtb.word(p);
            self.save.save_ptr += 1;
            let sp = self.save.save_ptr;
            self.save.set_save_type(sp, RESTORE_OLD_VALUE);
        }
        let sp = self.save.save_ptr;
        self.save.set_save_level(sp, l);
        self.save.set_save_index(sp, p);
        self.save.save_ptr += 1;
        Ok(())
    }

    /// `assign_trace(p, s)` (etex.ch): trace an assignment when
    /// `\tracingassigns` is positive.
    fn assign_trace(&mut self, p: Pointer, s: &str) {
        if self.eqtb.int_par(TRACING_ASSIGNS_CODE) > 0 {
            self.restore_trace(p, s);
        }
    }

    /// `eq_define(p, t, e)` (§277 + etex.ch): new data for the
    /// eq_type/equiv regions. In extended mode a redundant assignment is
    /// skipped (and traced as "reassigning").
    pub fn eq_define(&mut self, p: Pointer, t: u16, e: Halfword) -> crate::error::TexResult<()> {
        if self.etex_ex() && self.eqtb.eq_type(p) == t && self.eqtb.equiv(p) == e {
            self.assign_trace(p, "reassigning");
            // The caller hands over one reference to e; dispose of it.
            let w = self.eqtb.word(p);
            self.eq_destroy(w);
            return Ok(());
        }
        self.assign_trace(p, "changing");
        if self.eqtb.eq_level(p) == self.save.cur_level {
            let w = self.eqtb.word(p);
            self.eq_destroy(w);
        } else if self.save.cur_level > LEVEL_ONE {
            let l = self.eqtb.eq_level(p);
            self.eq_save(p, l)?;
        }
        let l = self.save.cur_level;
        self.eqtb.set_eq_level(p, l);
        self.eqtb.set_eq_type(p, t);
        self.eqtb.set_equiv(p, e);
        self.assign_trace(p, "into");
        Ok(())
    }

    /// `eq_word_define(p, w)` (§278 + etex.ch): for the fullword regions
    /// 5 and 6.
    pub fn eq_word_define(&mut self, p: Pointer, w: i32) -> crate::error::TexResult<()> {
        if self.etex_ex() && self.eqtb.int(p) == w {
            self.assign_trace(p, "reassigning");
            return Ok(());
        }
        self.assign_trace(p, "changing");
        if self.eqtb.xeq_level(p) != self.save.cur_level {
            let l = self.eqtb.xeq_level(p);
            self.eq_save(p, l)?;
            let cl = self.save.cur_level;
            self.eqtb.set_xeq_level(p, cl);
        }
        self.eqtb.set_int(p, w);
        self.assign_trace(p, "into");
        Ok(())
    }

    /// `geq_define(p, t, e)` (§279 + etex.ch): global `eq_define`.
    pub fn geq_define(&mut self, p: Pointer, t: u16, e: Halfword) {
        self.assign_trace(p, "globally changing");
        let w = self.eqtb.word(p);
        self.eq_destroy(w);
        self.eqtb.set_eq_level(p, LEVEL_ONE);
        self.eqtb.set_eq_type(p, t);
        self.eqtb.set_equiv(p, e);
        self.assign_trace(p, "into");
    }

    /// `geq_word_define(p, w)` (§279 + etex.ch).
    pub fn geq_word_define(&mut self, p: Pointer, w: i32) {
        self.assign_trace(p, "globally changing");
        self.eqtb.set_int(p, w);
        self.eqtb.set_xeq_level(p, LEVEL_ONE);
        self.assign_trace(p, "into");
    }

    /// `save_for_after(t)` (§280).
    pub fn save_for_after(&mut self, t: Halfword) -> crate::error::TexResult<()> {
        if self.save.cur_level > LEVEL_ONE {
            self.check_full_save_stack()?;
            let sp = self.save.save_ptr;
            self.save.set_save_type(sp, INSERT_TOKEN);
            self.save.set_save_level(sp, LEVEL_ZERO);
            self.save.set_save_index(sp, t);
            self.save.save_ptr += 1;
        }
        Ok(())
    }

    /// `unsave` (§281-§282): pops the top level off the save stack,
    /// restoring outer values.
    pub fn unsave(&mut self) -> crate::error::TexResult<()> {
        // etex.ch: have we already processed an \aftergroup?
        let mut a = false;
        if self.save.cur_level > LEVEL_ONE {
            self.save.cur_level -= 1;
            // §282: clear off top level from save_stack.
            loop {
                self.save.save_ptr -= 1;
                let sp = self.save.save_ptr;
                if self.save.save_type(sp) == LEVEL_BOUNDARY {
                    break;
                }
                let p = self.save.save_index(sp);
                if self.save.save_type(sp) == RESTORE_SA {
                    // etex.ch: restore sparse array entries.
                    self.sa_restore();
                    self.sa_chain = p;
                    self.sa_level = self.save.save_level(sp);
                } else if self.save.save_type(sp) == INSERT_TOKEN {
                    // §326 (+ etex.ch): insert token p back into the input.
                    // From the second \aftergroup token on, splice directly
                    // ahead of the current token list, preserving order.
                    let t = self.cur_tok;
                    self.cur_tok = p;
                    if a {
                        let q = self.mem.get_avail()?;
                        let tk = self.cur_tok;
                        self.mem.set_info(q, tk);
                        let loc = self.inp.cur.loc;
                        self.mem.set_link(q, loc);
                        self.inp.cur.loc = q;
                        self.inp.cur.start = q;
                        if self.cur_tok < crate::tokens::RIGHT_BRACE_LIMIT {
                            if self.cur_tok < crate::tokens::LEFT_BRACE_LIMIT {
                                self.inp.align_state -= 1;
                            } else {
                                self.inp.align_state += 1;
                            }
                        }
                    } else {
                        self.back_input()?;
                        a = self.etex_ex();
                    }
                    self.cur_tok = t;
                } else {
                    let l;
                    if self.save.save_type(sp) == RESTORE_OLD_VALUE {
                        l = self.save.save_level(sp);
                        self.save.save_ptr -= 1;
                    } else {
                        l = LEVEL_ONE; // unused for restore_zero
                        let ucs = self.eqtb.lay.undefined_control_sequence;
                        self.save.stack[self.save.save_ptr] = self.eqtb.word(ucs);
                    }
                    // §283: store save_stack[save_ptr] in eqtb[p], unless
                    // eqtb[p] holds a global value.
                    // N.B. §283 tests 	racingrestores only AFTER the
                    // eqtb word is written back, so restoring the tracing
                    // parameter itself to 0 is not reported.
                    let saved = self.save.stack[self.save.save_ptr];
                    let tracing = |e: &crate::engine::Engine| {
                        e.eqtb.int_par(crate::eqtb::TRACING_RESTORES_CODE) > 0
                    };
                    if p < self.eqtb.lay.int_base {
                        if self.eqtb.eq_level(p) == LEVEL_ONE {
                            self.eq_destroy(saved); // destroy the saved value
                            if tracing(self) {
                                self.restore_trace(p, "retaining");
                            }
                        } else {
                            let w = self.eqtb.word(p);
                            self.eq_destroy(w); // destroy the current value
                            *self.eqtb.word_mut(p) = saved; // restore the saved value
                            if tracing(self) {
                                self.restore_trace(p, "restoring");
                            }
                        }
                    } else if self.eqtb.xeq_level(p) != LEVEL_ONE {
                        *self.eqtb.word_mut(p) = saved;
                        self.eqtb.set_xeq_level(p, l);
                        if tracing(self) {
                            self.restore_trace(p, "restoring");
                        }
                    } else if tracing(self) {
                        self.restore_trace(p, "retaining");
                    }
                }
            }
            // done: (etex.ch additions around §282's epilogue)
            if self.eqtb.int_par(TRACING_GROUPS_CODE) > 0 {
                self.group_trace(true);
            }
            if self.grp_stack[self.inp.in_open] == self.save.cur_boundary {
                self.group_warning(); // groups not properly nested with files
            }
            let sp = self.save.save_ptr;
            self.save.cur_group = self.save.save_level(sp);
            self.save.cur_boundary = self.save.save_index(sp);
            if self.etex_ex() {
                self.save.save_ptr -= 1; // drop the line-number word
            }
            Ok(())
        } else {
            self.confusion("curlevel")
        }
    }

    /// `print_group(e)` (etex.ch): the current level of grouping and the
    /// name of `cur_group`.
    pub fn print_group(&mut self, e: bool) {
        match self.save.cur_group {
            BOTTOM_LEVEL => {
                self.print_chars("bottom level");
                return;
            }
            SIMPLE_GROUP | SEMI_SIMPLE_GROUP => {
                if self.save.cur_group == SEMI_SIMPLE_GROUP {
                    self.print_chars("semi ");
                }
                self.print_chars("simple");
            }
            HBOX_GROUP | ADJUSTED_HBOX_GROUP => {
                if self.save.cur_group == ADJUSTED_HBOX_GROUP {
                    self.print_chars("adjusted ");
                }
                self.print_chars("hbox");
            }
            VBOX_GROUP => self.print_chars("vbox"),
            VTOP_GROUP => self.print_chars("vtop"),
            ALIGN_GROUP | NO_ALIGN_GROUP => {
                if self.save.cur_group == NO_ALIGN_GROUP {
                    self.print_chars("no ");
                }
                self.print_chars("align");
            }
            OUTPUT_GROUP => self.print_chars("output"),
            DISC_GROUP => self.print_chars("disc"),
            INSERT_GROUP => self.print_chars("insert"),
            VCENTER_GROUP => self.print_chars("vcenter"),
            _ => {
                self.print_chars("math");
                if self.save.cur_group == MATH_CHOICE_GROUP {
                    self.print_chars(" choice");
                } else if self.save.cur_group == MATH_SHIFT_GROUP {
                    self.print_chars(" shift");
                } else if self.save.cur_group == MATH_LEFT_GROUP {
                    self.print_chars(" left");
                }
            }
        }
        self.print_chars(" group (level ");
        let l = i32::from(self.save.cur_level);
        self.print_int(l);
        self.print_char(')' as i32);
        if self.save.saved(-1) != 0 {
            if e {
                self.print_chars(" entered at line ");
            } else {
                self.print_chars(" at line ");
            }
            let ln = self.save.saved(-1);
            self.print_int(ln);
        }
    }

    /// `show_save_groups` (etex.ch): the `\showgroups` display, walking
    /// the semantic nest and the save stack in parallel.
    pub fn show_save_groups(&mut self) {
        let np = self.nest.ptr;
        self.nest.stack[np] = self.nest.cur;
        let mut p = np;
        let v = self.save.save_ptr;
        let l = self.save.cur_level;
        let c = self.save.cur_group;
        self.save.save_ptr = self.save.cur_boundary as usize;
        self.save.cur_level -= 1;
        let mut a: i32 = 1;
        self.print_nl_chars("");
        self.print_ln();
        loop {
            self.print_nl_chars("### ");
            self.print_group(true);
            if self.save.cur_group == BOTTOM_LEVEL {
                break;
            }
            let mut m;
            loop {
                m = self.nest.stack[p].mode;
                if p > 0 {
                    p -= 1;
                } else {
                    m = crate::engine::VMODE;
                }
                if m != crate::engine::HMODE {
                    break;
                }
            }
            self.print_chars(" (");
            // 'found / 'found1 / 'found2 replace the Pascal gotos: the
            // levels of closing work fall through in that order.
            #[allow(unused_assignments)]
            let mut s: &str = "";
            'found: {
                'found1: {
                    'found2: {
                        match self.save.cur_group {
                            SIMPLE_GROUP => {
                                p += 1;
                                break 'found2;
                            }
                            HBOX_GROUP | ADJUSTED_HBOX_GROUP => s = "hbox",
                            VBOX_GROUP => s = "vbox",
                            VTOP_GROUP => s = "vtop",
                            ALIGN_GROUP => {
                                if a == 0 {
                                    s = if m == -crate::engine::VMODE {
                                        "halign"
                                    } else {
                                        "valign"
                                    };
                                    a = 1;
                                    break 'found1;
                                } else {
                                    if a == 1 {
                                        self.print_chars("align entry");
                                    } else {
                                        self.print_esc_str("cr");
                                    }
                                    // N.B. a may be -1 (after noalign):
                                    // p - a then INCREASES p (etex.ch).
                                    if p as i32 >= a {
                                        p = (p as i32 - a) as usize;
                                    }
                                    a = 0;
                                    break 'found;
                                }
                            }
                            NO_ALIGN_GROUP => {
                                p += 1;
                                a = -1;
                                self.print_esc_str("noalign");
                                break 'found2;
                            }
                            OUTPUT_GROUP => {
                                self.print_esc_str("output");
                                break 'found;
                            }
                            MATH_GROUP => break 'found2,
                            DISC_GROUP | MATH_CHOICE_GROUP => {
                                if self.save.cur_group == DISC_GROUP {
                                    self.print_esc_str("discretionary");
                                } else {
                                    self.print_esc_str("mathchoice");
                                }
                                for i in 1..=3 {
                                    if i <= self.save.saved(-2) {
                                        self.print_chars("{}");
                                    }
                                }
                                break 'found2;
                            }
                            INSERT_GROUP => {
                                if self.save.saved(-2) == 255 {
                                    self.print_esc_str("vadjust");
                                } else {
                                    self.print_esc_str("insert");
                                    let n = self.save.saved(-2);
                                    self.print_int(n);
                                }
                                break 'found2;
                            }
                            VCENTER_GROUP => {
                                s = "vcenter";
                                break 'found1;
                            }
                            SEMI_SIMPLE_GROUP => {
                                p += 1;
                                self.print_esc_str("begingroup");
                                break 'found;
                            }
                            MATH_SHIFT_GROUP => {
                                if m == crate::engine::MMODE {
                                    self.print_char('$' as i32);
                                } else if self.nest.stack[p].mode == crate::engine::MMODE {
                                    let v = self.save.saved(-2);
                                    self.print_cmd_chr(crate::cmds::EQ_NO, v);
                                    break 'found;
                                }
                                self.print_char('$' as i32);
                                break 'found;
                            }
                            _ => {
                                // math_left_group (etex.ch): delim_ptr tells
                                // whether \left or \middle opened it.
                                let dp = self.nest.stack[p + 1].etex_aux;
                                if dp != NULL && self.mem.node_type(dp) == crate::math::LEFT_NOAD {
                                    self.print_esc_str("left");
                                } else {
                                    self.print_esc_str("middle");
                                }
                                break 'found;
                            }
                        }
                        // Show the box context (saved(-4)).
                        let i = self.save.saved(-4);
                        if i != 0 {
                            if i < crate::control::BOX_FLAG {
                                let j = if self.nest.stack[p].mode.abs() == crate::engine::VMODE {
                                    crate::cmds::HMOVE
                                } else {
                                    crate::cmds::VMOVE
                                };
                                if i > 0 {
                                    self.print_cmd_chr(j, 0);
                                } else {
                                    self.print_cmd_chr(j, 1);
                                }
                                let av = i.abs();
                                self.print_scaled(av);
                                self.print_chars("pt");
                            } else if i < crate::control::SHIP_OUT_FLAG {
                                let mut i = i;
                                if i >= crate::control::GLOBAL_BOX_FLAG {
                                    self.print_esc_str("global");
                                    i -= crate::control::GLOBAL_BOX_FLAG - crate::control::BOX_FLAG;
                                }
                                self.print_esc_str("setbox");
                                self.print_int(i - crate::control::BOX_FLAG);
                                self.print_char('=' as i32);
                            } else {
                                let chr = i
                                    - (crate::control::LEADER_FLAG
                                        - i32::from(crate::nodes::A_LEADERS));
                                self.print_cmd_chr(crate::cmds::LEADER_SHIP, chr);
                            }
                        }
                        // found1: the box name and its packaging info.
                        self.print_esc_str(s);
                        if self.save.saved(-2) != 0 {
                            self.print_char(' ' as i32);
                            if self.save.saved(-3) == crate::pack::EXACTLY {
                                self.print_chars("to");
                            } else {
                                self.print_chars("spread");
                            }
                            let d = self.save.saved(-2);
                            self.print_scaled(d);
                            self.print_chars("pt");
                        }
                        break 'found2;
                    }
                    // found2:
                    self.print_char('{' as i32);
                    break 'found;
                }
                // found1 (from ALIGN_GROUP/VCENTER_GROUP):
                self.print_esc_str(s);
                if self.save.saved(-2) != 0 {
                    self.print_char(' ' as i32);
                    if self.save.saved(-3) == crate::pack::EXACTLY {
                        self.print_chars("to");
                    } else {
                        self.print_chars("spread");
                    }
                    let d = self.save.saved(-2);
                    self.print_scaled(d);
                    self.print_chars("pt");
                }
                self.print_char('{' as i32);
            }
            // found:
            self.print_char(')' as i32);
            self.save.cur_level -= 1;
            let sp = self.save.save_ptr;
            self.save.cur_group = self.save.save_level(sp);
            self.save.save_ptr = self.save.save_index(sp) as usize;
        }
        // done:
        self.save.save_ptr = v;
        self.save.cur_level = l;
        self.save.cur_group = c;
    }

    /// `group_trace(e)` (etex.ch): called when a level of grouping begins
    /// (e = false) or ends (e = true).
    pub fn group_trace(&mut self, e: bool) {
        self.begin_diagnostic();
        self.print_char('{' as i32);
        if e {
            self.print_chars("leaving ");
        } else {
            self.print_chars("entering ");
        }
        self.print_group(e);
        self.print_char('}' as i32);
        self.end_diagnostic(false);
    }
}
