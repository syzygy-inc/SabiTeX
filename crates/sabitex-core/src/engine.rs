//! The global state of the engine.
//!
//! tex.web keeps several hundred global variables; this port collects them
//! into one `Engine` struct, split into subsystems. Every tex.web procedure
//! becomes a method `fn ...(&mut self)` on `Engine` (or on a subsystem when
//! it touches only that subsystem).
//!
//! `Sizes` ports the compile-time constants of tex.web §11-§12. They are
//! run-time parameters here because the TRIP test must run with small
//! memory settings against the same binary.

use crate::arith::ArithState;
use crate::cmds::*;
use crate::cond::CLOSED;
use crate::eqtb::{Eqtb, Layout, SaveStack, LEVEL_ONE, LEVEL_ZERO};
use crate::error::{History, TexResult};
use crate::getnext::TOO_BIG_CHAR;
use crate::input::{Input, ERROR_STOP_MODE};
use crate::io::{Terminal, TexFs};
use crate::mem::Mem;
use crate::print::PrintState;
use crate::scan::{DIMEN_VAL, GLUE_VAL, INT_VAL, MU_VAL};
use crate::strings::StringPool;
use crate::tokens::CS_TOKEN_FLAG;
use crate::types::{Halfword, Pointer, Scaled, MAX_HALFWORD, NULL};

// §211: mode codes.
pub const VMODE: i32 = 1;
pub const HMODE: i32 = VMODE + MAX_COMMAND as i32 + 1;

/// `eTeX_version` (etex.ch): the value of `\eTeXversion`.
pub const ETEX_VERSION: i32 = 2;
/// `eTeX_revision` (etex.ch): printed by `\eTeXrevision`.
pub const ETEX_REVISION: &str = ".6";
pub const MMODE: i32 = HMODE + MAX_COMMAND as i32 + 1;

/// The "constants in the outer block" (tex.web §11-§12) that M0/M1 need.
#[derive(Clone, Debug)]
pub struct Sizes {
    /// `mem_top` / `mem_max`: greatest index in `mem` (kept equal for now).
    pub mem_top: Pointer,
    /// `mem_min` / `mem_bot`: smallest index in `mem` (kept equal; 0 or 1 —
    /// the TRIP test requires 1).
    pub mem_bot: Pointer,
    /// `pool_size`: maximum number of string-pool units.
    pub pool_size: usize,
    /// `max_strings`: maximum number of strings.
    pub max_strings: usize,
    /// `max_print_line`: width of longest text lines output.
    pub max_print_line: usize,
    /// `error_line`: width of context lines on terminal error messages.
    pub error_line: usize,
    /// `half_error_line`: width of first lines of contexts.
    pub half_error_line: usize,
    /// `hash_size` / `hash_prime` (§12, §261).
    pub hash_size: i32,
    pub hash_prime: i32,
    /// `font_max` (§12).
    pub font_max: i32,
    /// `save_size`: space for saving values outside of current group (§12).
    pub save_size: usize,
    /// `stack_size`: maximum number of simultaneous input sources (§11).
    pub stack_size: usize,
    /// `max_in_open`: maximum number of input files open simultaneously.
    pub max_in_open: usize,
    /// `param_size`: maximum number of simultaneous macro parameters.
    pub param_size: usize,
    /// `nest_size`: maximum number of semantic levels simultaneously
    /// active (§11).
    pub nest_size: usize,
    /// `font_mem_size`: number of words of `font_info` for all fonts (§11).
    pub font_mem_size: usize,
    /// `buf_size`: maximum number of characters simultaneously present in
    /// current lines of open files.
    pub buf_size: i32,
    /// `trie_size`: space for hyphenation patterns (§11).
    pub trie_size: i32,
    /// `trie_op_size`: space for "opcodes" in the hyphenation patterns.
    pub trie_op_size: i32,
    /// `hyph_size`: prime; the exception dictionary capacity (§12).
    pub hyph_size: i32,
}

impl Sizes {
    /// Production sizes, comparable to a TeX Live web2c configuration
    /// (texmf.cnf): what the CLI and wasm builds use. Tests that need
    /// the tex.web §11-§12 reference values use `Sizes::default()`.
    pub fn production() -> Sizes {
        // Sized from measured LaTeX + TikZ jobs (tracingstats: mem
        // 628k, font_info 628k, ~38k control sequences) with >=3x
        // headroom. Oversizing is not free: Engine::new zero-fills
        // every arena, which dominated wasm compile time at 8M words.
        Sizes {
            // pgfmanual needs >2M words of node memory (TeX Live ships
            // main_memory=5000000); the zero-fill cost this buys back is
            // ~12ms of wasm Engine::new.
            mem_top: 5_000_000,
            mem_bot: 0,
            pool_size: 8_000_000,
            max_strings: 200_000,
            max_print_line: 79,
            error_line: 79,
            half_error_line: 50,
            // TeX Live ships hash_extra=600000 on top of 15000; a full
            // pgfmanual build defines >60k control sequences, and our
            // id_lookup's overflow fallback (undefined_control_sequence)
            // corrupts a run long before it stops.
            hash_size: 200_000,
            hash_prime: 169_991,
            font_max: 2000,
            save_size: 200_000,
            stack_size: 10_000,
            max_in_open: 50,
            param_size: 20_000,
            nest_size: 1_000,
            font_mem_size: 2_000_000,
            buf_size: 200_000,
            trie_size: 262_144,
            trie_op_size: 35_111,
            hyph_size: 8_191,
        }
    }
}

impl Default for Sizes {
    /// The tex.web §11-§12 reference values (with the string pool sized for
    /// engine-interned strings; there is no TEX.POOL preload here).
    fn default() -> Sizes {
        Sizes {
            mem_top: 30000,
            mem_bot: 0,
            pool_size: 32000,
            max_strings: 3000,
            max_print_line: 79,
            error_line: 72,
            half_error_line: 42,
            hash_size: 2100,
            hash_prime: 1777,
            font_max: 255,
            save_size: 600,
            stack_size: 200,
            max_in_open: 6,
            param_size: 60,
            nest_size: 40,
            font_mem_size: 20000,
            buf_size: 500,
            trie_size: 8000,
            trie_op_size: 500,
            hyph_size: 307,
        }
    }
}

/// The engine: all of tex.web's global state, plus the host interfaces.
pub struct Engine {
    pub sizes: Sizes,
    /// `arith_error` / `remainder` (Part 7).
    pub arith: ArithState,
    /// The `mem` array and its allocation state (Parts 9-12).
    pub mem: Mem,
    /// The string pool (Part 4).
    pub strings: StringPool,
    /// Printing state (Part 5).
    pub prn: PrintState,
    /// The table of equivalents + hash table (Parts 17-18).
    pub eqtb: Eqtb,
    /// The save stack and grouping state (Part 19).
    pub save: SaveStack,
    /// Input stacks and states (Parts 22-23).
    pub inp: Input,
    /// `history` (Part 6).
    pub history: History,
    /// `interaction` (§73).
    pub interaction: u8,
    /// The transcript file, buffered in memory until the job ends.
    pub log: Vec<u8>,
    pub log_opened: bool,
    /// Bytes of `log` already streamed to the file system (A2). Zero
    /// means nothing was streamed (backend without append support).
    pub log_streamed: usize,
    /// True until the first job runs: arenas are still all-zero from
    /// construction, so format loading may skip its zero-fills.
    pub pristine: bool,
    /// pTeX `inhibit_glue_flag`: set by \inhibitglue, consumed by the
    /// next append_kanji's JFM glue decision.
    pub inhibit_glue_flag: bool,
    /// True once any Japanese font (JFM) has been loaded. Gates the
    /// pTeX-style ", yoko direction" box annotations: euptex prints
    /// them unconditionally, but our TRIP/e-TRIP logs must stay
    /// tex.web-identical, and those runs never load a JFM.
    pub jfont_seen: bool,
    /// One-shot flag: the hash-full condition was already reported.
    pub hash_overflow_reported: bool,
    /// pdfTeX `is_in_csname`: expanding inside \csname...\endcsname.
    pub in_csname: bool,
    /// pdfTeX \pdflastxpos / \pdflastypos (sp, lower-left origin).
    pub last_x_pos: i32,
    pub last_y_pos: i32,
    /// Suppresses the ", yoko direction" annotation while showing a box
    /// from a packing diagnostic (euptex omits it there).
    pub in_pack_diagnostic: bool,
    /// File system and terminal interfaces.
    pub fs: Box<dyn TexFs>,
    pub term: Box<dyn Terminal>,
    /// `job_name` (§527).
    pub job_name: Option<String>,
    /// `format_ident` (§1299): empty in virgin INITEX, else a description
    /// of the preloaded format.
    pub format_ident: String,
    /// The first input line, echoed after `**` in the transcript (§534).
    pub first_input_line: String,
    /// XeTeX: the last scan_file_name saw a quoted ("...") name.
    pub quoted_filename: bool,
    /// pTeX `cur_kanji_skip` / `cur_xkanji_skip`: glue specs applied
    /// implicitly between Japanese characters (no nodes appear in the
    /// list). Latched from \kanjiskip/\xkanjiskip when a box is
    /// packaged, if \autospacing/\autoxspacing are on.
    pub cur_kanji_skip: Pointer,
    /// pTeX `last_jchr`: head of the most recently appended Japanese
    /// pair (used to detect an adjacent pair without walking the list).
    pub last_jchr: Pointer,
    pub cur_xkanji_skip: Pointer,
    /// pTeX box space_ptr/xspace_ptr, held OUTSIDE the box node (the
    /// node stays 7 words for TRIP/etrip memory-statistics fidelity).
    /// Maps a box to the (kanji, xkanji) specs its contents were
    /// measured with; hlist_out consults this.
    pub box_spacing: std::collections::BTreeMap<Pointer, (Pointer, Pointer)>,
    /// XeTeX: shaped glyph records of native_word nodes (replaces the
    /// C-side native_glyph_info_ptr). Keyed by node address; entries are
    /// dropped in free_native_node and duplicated in copy_node_list.
    pub native_glyph_infos: std::collections::BTreeMap<Pointer, Vec<crate::native::GlyphInfo>>,
    /// Whether the terminal echoes typed lines (false emulates a job run
    /// with redirected standard input, as the etrip test prescribes).
    pub terminal_echo: bool,
    /// Set when `\dump` ends the job; `final_cleanup` then stores the
    /// format (§1335).
    pub dump_requested: bool,
    // §297: the scanner registers.
    pub cur_cmd: u16,
    pub cur_chr: i32,
    pub cur_cs: Pointer,
    pub cur_tok: Halfword,
    // §410: value scanning.
    pub cur_val: i32,
    pub cur_val_level: u8,
    pub radix: i32,
    pub cur_order: u16,
    // §333: \par.
    pub par_loc: Pointer,
    pub par_token: Halfword,
    /// `long_state` (§387).
    pub long_state: u16,
    /// `cur_mark` (§382).
    pub cur_mark: [Pointer; 5],
    // §489: the condition stack.
    pub cond_ptr: Pointer,
    pub if_limit: u8,
    pub cur_if: u16,
    pub if_line: i32,
    /// `skip_line` (§493).
    pub skip_line: i32,
    // §76, §96: error reporting state.
    pub error_count: i32,
    /// `help_line[0..5]` (§79), stored first-to-last as written.
    pub help_lines: Vec<&'static str>,
    pub deletions_allowed: bool,
    pub use_err_help: bool,
    pub long_help_seen: bool,
    pub old_setting: u8,
    /// `after_token` (§1266).
    pub after_token: Halfword,
    // §527: file-name scanning.
    pub name_in_progress: bool,
    pub cur_name: String,
    /// The semantic nest (Part 16).
    pub nest: crate::nest::Nest,
    /// Font memory (Part 30).
    pub fonts: crate::fonts::FontMem,
    /// Line-breaking state (Parts 38-39).
    pub lb: crate::linebreak::LineBreak,
    /// Hyphenation state: trie, exceptions, scratch arrays (Parts 40-43).
    pub hy: crate::hyph::Hyph,
    // Part 45: the page builder's state (§980-§982).
    pub dead_cycles: i32,
    pub insert_penalties: i32,
    pub last_badness: i32,
    pub last_penalty: i32,
    pub last_kern: Scaled,
    pub last_glue: Pointer,
    /// `last_node_type` (etex.ch §982): implements `\lastnodetype`.
    pub last_node_type: i32,
    /// `eTeX_mode` (etex.ch §1337): false = compatibility, true = extended.
    pub etex_mode: bool,
    /// `pseudo_files` (etex.ch): stack of `\scantokens` pseudo files.
    pub pseudo_files: Pointer,
    /// `sa_root[int_val..mark_val]` (etex.ch): sparse array tree roots.
    pub sa_root: [Pointer; 7],
    /// `cur_ptr` (etex.ch): result of `new_index`/`find_sa_element`.
    pub cur_ptr: Pointer,
    /// `sa_chain` / `sa_level` (etex.ch): saved sparse array entries.
    pub sa_chain: Pointer,
    pub sa_level: u16,
    /// `LR_ptr` / `LR_problems` / `cur_dir` (etex.ch): TeX--XeT state for
    /// hpack, ship_out, and init_math.
    pub lr_ptr: Pointer,
    pub lr_problems: i32,
    pub cur_dir: u8,
    /// `disc_ptr[copy_code..vsplit_code]` (etex.ch): items discarded by
    /// the page builder (`\pagediscards`) and `\vsplit`
    /// (`\splitdiscards`); index 1 is `tail_page_disc`.
    pub disc_ptr: [Pointer; 4],
    /// `eof_seen[index]` (etex.ch): has `\everyeof` fired for this file?
    pub eof_seen: Vec<bool>,
    /// `grp_stack[index]` (etex.ch): `cur_boundary` when the file opened.
    pub grp_stack: Vec<i32>,
    /// `if_stack[index]` (etex.ch): `cond_ptr` when the file opened.
    pub if_stack: Vec<Pointer>,
    /// `max_reg_num` / `max_reg_help_line` (etex.ch): 255 in compatibility
    /// mode, 32767 in extended mode.
    pub max_reg_num: i32,
    /// `page_so_far[0..7]`: goal, total, stretch×4, shrink, depth (§982).
    pub page_so_far: [Scaled; 8],
    /// `page_tail`: the final node on the current page (§980).
    pub page_tail: Pointer,
    /// `page_contents`: empty / inserts_only / box_there (§980).
    pub page_contents: u8,
    /// `page_max_depth` (§980).
    pub page_max_depth: Scaled,
    /// `best_page_break`, `least_page_cost`, `best_size` (§980).
    pub best_page_break: Pointer,
    pub least_page_cost: i32,
    pub best_size: Scaled,
    /// `output_active` (§989).
    pub output_active: bool,
    /// `best_height_plus_depth` (§971).
    pub best_height_plus_depth: Scaled,
    // Part 12 display globals (§173, §181).
    pub font_in_short_display: i32,
    pub depth_threshold: i32,
    pub breadth_max: i32,
    /// `adjust_tail` (§647): set non-null by hpack callers collecting
    /// \vadjust material.
    pub adjust_tail: Pointer,
    /// `total_stretch`, `total_shrink` (§646): glue found by `hpack` or
    /// `vpack`, consulted afterwards (e.g. §1201).
    pub total_stretch: [Scaled; 4],
    pub total_shrink: [Scaled; 4],
    // §719: the implicit parameters of `mlist_to_hlist`.
    pub cur_mlist: Pointer,
    pub cur_style: u16,
    pub cur_size: i32,
    pub cur_mu: Scaled,
    pub mlist_penalties: bool,
    /// `cur_f`, `cur_c`, `cur_i` (§724): the outputs of `fetch`.
    pub cur_f: i32,
    pub cur_c: i32,
    pub cur_i: crate::memword::MemoryWord,
    /// `pack_begin_line` (§661).
    pub pack_begin_line: i32,
    /// `cur_s`: current depth of output box nesting (§616).
    pub cur_s: i32,
    /// `dead_cycles` companion: total pages shipped (§592 `total_pages`).
    pub total_pages: i32,
    /// DVI output state (Part 31-32).
    pub dvi: crate::dvi::DviState,
    /// `cur_box` (§1074).
    pub cur_box: Pointer,
    // §1032-§1033: the main-loop ligature/kern machinery.
    pub main_f: i32,
    pub main_i: crate::memword::MemoryWord,
    pub main_j: crate::memword::MemoryWord,
    pub main_k: i32,
    pub lig_stack: Pointer,
    pub cur_l: i32,
    pub cur_r: i32,
    pub cur_q: Pointer,
    pub ligature_present: bool,
    pub cancel_boundary: bool,
    pub lft_hit: bool,
    pub rt_hit: bool,
    pub ins_disc: bool,
    /// `bchar` / `false_bchar` of the current main-loop font (§1032).
    pub lig_bchar: i32,
    pub lig_false_bchar: i32,
    // §770: the alignment state.
    pub cur_align: Pointer,
    pub cur_span: Pointer,
    pub cur_loop: Pointer,
    pub align_ptr: Pointer,
    pub cur_head: Pointer,
    pub cur_tail: Pointer,
    /// `read_open` (§480).
    pub read_open: [u8; 17],
    /// `write_loc` (§1345): eqtb location of `\write`.
    pub write_loc: Pointer,
    /// `write_open` (§1342).
    pub write_open: [bool; 18],
    /// `write_file[j]` (§1342), buffered until close.
    pub write_file_name: [String; 16],
    pub write_buf: [Vec<u8>; 16],
    /// `mag_set` (§286).
    pub mag_set: i32,
    pub set_box_allowed: bool,
}

impl Engine {
    /// Builds an engine with freshly initialized tables: the INITEX "slow
    /// way" (§8 `init`, §164, §232-§254, §1336), including all the
    /// primitives implemented so far. Format loading arrives with M5.
    pub fn new(sizes: Sizes, fs: Box<dyn TexFs>, term: Box<dyn Terminal>) -> Engine {
        let mem = Mem::new(sizes.mem_top, sizes.mem_bot);
        let contrib_head = mem.mem_top - 1;
        let strings = StringPool::new(sizes.pool_size, sizes.max_strings);
        let prn = PrintState::new(sizes.error_line);
        let lay = Layout::new(sizes.hash_size, sizes.hash_prime, sizes.font_max);
        let eqtb = Eqtb::new(lay);
        let save = SaveStack::new(sizes.save_size);
        let inp = Input::new(
            sizes.stack_size,
            sizes.max_in_open,
            sizes.param_size,
            sizes.buf_size,
        );
        let mut e = Engine {
            arith: ArithState::default(),
            mem,
            strings,
            prn,
            eqtb,
            save,
            inp,
            history: History::FatalErrorStop, // §76: in case we quit early
            interaction: ERROR_STOP_MODE,
            log: Vec::new(),
            log_opened: false,
            log_streamed: 0,
            pristine: true,
            inhibit_glue_flag: false,
            jfont_seen: false,
            hash_overflow_reported: false,
            in_csname: false,
            last_x_pos: 0,
            last_y_pos: 0,
            in_pack_diagnostic: false,
            fs,
            term,
            job_name: None,
            format_ident: String::new(),
            first_input_line: String::new(),
            quoted_filename: false,
            cur_kanji_skip: NULL,
            last_jchr: NULL,
            cur_xkanji_skip: NULL,
            box_spacing: std::collections::BTreeMap::new(),
            native_glyph_infos: std::collections::BTreeMap::new(),
            terminal_echo: true,
            dump_requested: false,
            cur_cmd: 0,
            cur_chr: 0,
            cur_cs: 0,
            cur_tok: 0,
            cur_val: 0,
            cur_val_level: INT_VAL,
            radix: 0,
            cur_order: 0,
            par_loc: NULL,
            par_token: 0,
            long_state: 0,
            cur_mark: [NULL; 5],
            cond_ptr: NULL,
            if_limit: 0, // normal
            cur_if: 0,
            if_line: 0,
            skip_line: 0,
            error_count: 0,
            help_lines: Vec::new(),
            deletions_allowed: true,
            use_err_help: false,
            long_help_seen: false,
            old_setting: 0,
            after_token: 0,
            name_in_progress: false,
            cur_name: String::new(),
            nest: crate::nest::Nest::new(sizes.nest_size, contrib_head),
            fonts: crate::fonts::FontMem::new(sizes.font_mem_size, sizes.font_max),
            lb: crate::linebreak::LineBreak::default(),
            hy: crate::hyph::Hyph::new(sizes.trie_size, sizes.trie_op_size, sizes.hyph_size),
            dead_cycles: 0,
            insert_penalties: 0,
            last_badness: 0,
            last_penalty: 0,
            last_kern: 0,
            last_glue: MAX_HALFWORD,
            last_node_type: -1,
            etex_mode: false,
            pseudo_files: NULL,
            sa_root: [NULL; 7],
            cur_ptr: NULL,
            sa_chain: NULL,
            sa_level: crate::eqtb::LEVEL_ZERO,
            lr_ptr: NULL,
            lr_problems: 0,
            cur_dir: 0, // left_to_right
            disc_ptr: [NULL; 4],
            eof_seen: vec![false; sizes.max_in_open + 1],
            grp_stack: vec![0; sizes.max_in_open + 1],
            if_stack: vec![NULL; sizes.max_in_open + 1],
            max_reg_num: 255,
            page_so_far: [0; 8],
            page_tail: contrib_head - 1, // page_head == mem_top - 2 (§982)
            page_contents: crate::page::EMPTY,
            page_max_depth: 0,
            best_page_break: NULL,
            least_page_cost: 0,
            best_size: 0,
            output_active: false,
            best_height_plus_depth: 0,
            font_in_short_display: 0,
            depth_threshold: 0,
            breadth_max: 0,
            adjust_tail: NULL,
            total_stretch: [0; 4],
            total_shrink: [0; 4],
            cur_mlist: NULL,
            cur_style: 0,
            cur_size: 0,
            cur_mu: 0,
            mlist_penalties: false,
            cur_f: 0,
            cur_c: 0,
            cur_i: crate::memword::MemoryWord::ZERO,
            pack_begin_line: 0,
            cur_s: -1,
            total_pages: 0,
            dvi: crate::dvi::DviState::new(),
            cur_box: NULL,
            main_f: 0,
            main_i: crate::memword::MemoryWord::ZERO,
            main_j: crate::memword::MemoryWord::ZERO,
            main_k: 0,
            lig_stack: NULL,
            cur_l: 0,
            cur_r: 0,
            cur_q: NULL,
            ligature_present: false,
            cancel_boundary: false,
            lft_hit: false,
            rt_hit: false,
            ins_disc: false,
            lig_bchar: crate::fonts::NON_CHAR,
            lig_false_bchar: crate::fonts::NON_CHAR,
            cur_align: NULL,
            cur_span: NULL,
            cur_loop: NULL,
            align_ptr: NULL,
            cur_head: NULL,
            cur_tail: NULL,
            read_open: [CLOSED; 17],
            write_loc: NULL,
            write_open: [false; 18],
            write_file_name: Default::default(),
            write_buf: Default::default(),
            mag_set: 0,
            set_box_allowed: true,
            sizes,
        };
        e.initialize_eqtb();
        e.install_primitives()
            .expect("primitive installation cannot overflow fresh tables");
        e.fix_date_and_time();
        e.history = History::Spotless;
        e
    }

    /// §232-§254 + §240 + §250 + §258: the INITEX table entries.
    fn initialize_eqtb(&mut self) {
        let lay = self.eqtb.lay.clone();
        // §232: undefined_control_sequence, copied over regions 1-2.
        let ucs = lay.undefined_control_sequence;
        self.eqtb.set_eq_type(ucs, UNDEFINED_CS);
        self.eqtb.set_equiv(ucs, NULL);
        self.eqtb.set_eq_level(ucs, LEVEL_ZERO);
        let undef = self.eqtb.word(ucs);
        for k in lay.active_base..ucs {
            *self.eqtb.word_mut(k) = undef;
        }
        // §228: region 3 — all glue parameters and registers are zero_glue.
        let zg = self.mem.zero_glue();
        self.eqtb.set_equiv(lay.glue_base, zg);
        self.eqtb.set_eq_level(lay.glue_base, LEVEL_ONE);
        self.eqtb.set_eq_type(lay.glue_base, GLUE_REF);
        let glue_word = self.eqtb.word(lay.glue_base);
        for k in (lay.glue_base + 1)..lay.local_base {
            *self.eqtb.word_mut(k) = glue_word;
        }
        let refs = self.mem.glue_ref_count(zg) + lay.local_base - lay.glue_base;
        self.mem.set_glue_ref_count(zg, refs);
        // §232: region 4.
        self.eqtb.set_equiv(lay.par_shape_loc, NULL);
        self.eqtb.set_eq_type(lay.par_shape_loc, SHAPE_REF);
        self.eqtb.set_eq_level(lay.par_shape_loc, LEVEL_ONE);
        // etex.ch §232: the four penalties arrays behave like \parshape.
        let ps_word = self.eqtb.word(lay.par_shape_loc);
        for k in lay.etex_pen_base..(lay.etex_pen_base + 4) {
            *self.eqtb.word_mut(k) = ps_word;
        }
        for k in lay.output_routine_loc..(lay.toks_base + 256) {
            *self.eqtb.word_mut(k) = undef;
        }
        self.eqtb.set_equiv(lay.box_base, NULL);
        self.eqtb.set_eq_type(lay.box_base, BOX_REF);
        self.eqtb.set_eq_level(lay.box_base, LEVEL_ONE);
        let box_word = self.eqtb.word(lay.box_base);
        for k in (lay.box_base + 1)..(lay.box_base + 256) {
            *self.eqtb.word_mut(k) = box_word;
        }
        self.eqtb.set_equiv(lay.cur_font_loc, 0); // null_font
        self.eqtb.set_eq_type(lay.cur_font_loc, DATA);
        self.eqtb.set_eq_level(lay.cur_font_loc, LEVEL_ONE);
        let font_word = self.eqtb.word(lay.cur_font_loc);
        for k in lay.math_font_base..(lay.math_font_base + 768) {
            *self.eqtb.word_mut(k) = font_word;
        }
        self.eqtb.set_equiv(lay.cat_code_base, 0);
        self.eqtb.set_eq_type(lay.cat_code_base, DATA);
        self.eqtb.set_eq_level(lay.cat_code_base, LEVEL_ONE);
        let code_word = self.eqtb.word(lay.cat_code_base);
        for k in (lay.cat_code_base + 1)..lay.int_base {
            *self.eqtb.word_mut(k) = code_word;
        }
        for k in 0..crate::eqtb::NUMBER_USVS {
            self.eqtb.set_cat_code(k, i32::from(OTHER_CHAR));
            self.eqtb.set_equiv(lay.math_code_base + k, k);
            self.eqtb.set_equiv(lay.sf_code_base + k, 1000);
        }
        // upTeX: default \kcatcode assignments by Unicode block.
        for (k, v) in crate::kanji::default_kcat_codes().iter().enumerate() {
            self.eqtb.set_kcat_code(k as i32, *v);
        }
        // pTeX: \xspcode defaults to 3 for ASCII digits and letters.
        for k in '0' as i32..='9' as i32 {
            self.eqtb.set_equiv(lay.auto_xsp_code_base + k, 3);
        }
        for k in 'A' as i32..='Z' as i32 {
            self.eqtb.set_equiv(lay.auto_xsp_code_base + k, 3);
            self.eqtb.set_equiv(lay.auto_xsp_code_base + k + 0x20, 3);
        }
        // pTeX: the \kansujichar digits.
        for (k, &c) in crate::kanji::KANSUJI_DEFAULTS.iter().enumerate() {
            self.eqtb.set_equiv(lay.kansuji_base + k as i32, c);
        }
        // pTeX: \jfont's current-font slot starts at null_font too.
        self.eqtb.set_equiv(lay.cur_jfont_loc, 0);
        self.eqtb.set_eq_type(lay.cur_jfont_loc, DATA);
        self.eqtb.set_eq_level(lay.cur_jfont_loc, LEVEL_ONE);
        self.eqtb.set_cat_code(13, i32::from(CAR_RET)); // carriage_return
        self.eqtb.set_cat_code(' ' as i32, i32::from(SPACER));
        self.eqtb.set_cat_code('\\' as i32, i32::from(ESCAPE));
        self.eqtb.set_cat_code('%' as i32, i32::from(COMMENT));
        self.eqtb.set_cat_code(127, i32::from(INVALID_CHAR));
        self.eqtb.set_cat_code(0, i32::from(IGNORE));
        for k in '0' as i32..='9' as i32 {
            // xetex.web §5752: digits are var-family class in the
            // extended packing (class 7 at bit 21).
            self.eqtb.set_equiv(
                lay.math_code_base + k,
                k + crate::xemath::set_class_field(crate::xemath::VAR_FAM_CLASS),
            );
        }
        for k in 'A' as i32..='Z' as i32 {
            let lc = k + 0x20;
            self.eqtb.set_cat_code(k, i32::from(LETTER));
            self.eqtb.set_cat_code(lc, i32::from(LETTER));
            let letter_mc = crate::xemath::set_family_field(1)
                + crate::xemath::set_class_field(crate::xemath::VAR_FAM_CLASS);
            self.eqtb.set_equiv(lay.math_code_base + k, k + letter_mc);
            self.eqtb.set_equiv(lay.math_code_base + lc, lc + letter_mc);
            self.eqtb.set_equiv(lay.lc_code_base + k, lc);
            self.eqtb.set_equiv(lay.lc_code_base + lc, lc);
            self.eqtb.set_equiv(lay.uc_code_base + k, k);
            self.eqtb.set_equiv(lay.uc_code_base + lc, k);
            self.eqtb.set_equiv(lay.sf_code_base + k, 999);
        }
        // §240: region 5 (ints are already zero).
        self.eqtb
            .set_int(lay.int_base + crate::eqtb::MAG_CODE, 1000);
        self.eqtb
            .set_int(lay.int_base + crate::eqtb::TOLERANCE_CODE, 10000);
        self.eqtb
            .set_int(lay.int_base + crate::eqtb::HANG_AFTER_CODE, 1);
        self.eqtb
            .set_int(lay.int_base + crate::eqtb::MAX_DEAD_CYCLES_CODE, 25);
        self.eqtb
            .set_int(lay.int_base + crate::eqtb::ESCAPE_CHAR_CODE, '\\' as i32);
        self.eqtb
            .set_int(lay.int_base + crate::eqtb::END_LINE_CHAR_CODE, 13);
        for k in 0..crate::eqtb::NUMBER_USVS {
            self.eqtb.set_int(lay.del_code_base + k, -1);
        }
        self.eqtb.set_int(lay.del_code_base + '.' as i32, 0);
        // §250: region 6 is already zero.
        // §258: hash initialization.
        self.eqtb.hash_used = lay.frozen_control_sequence;
        self.eqtb.cs_count = 0;
        self.eqtb.set_eq_type(lay.frozen_dont_expand, DONT_EXPAND);
        let s = self
            .strings
            .intern("notexpanded:")
            .expect("pool space for fixed strings");
        self.eqtb.set_text(lay.frozen_dont_expand, s);
        // §1216.
        let s = self
            .strings
            .intern("inaccessible")
            .expect("pool space for fixed strings");
        self.eqtb.set_text(lay.frozen_protection, s);
    }

    /// Retrieves an output file from the host file system, if it keeps them.
    pub fn take_output(&mut self, name: &str) -> Option<Vec<u8>> {
        self.fs.take_output(name)
    }

    /// `store_fmt_file` (§1302-§1329): serializes the engine state that
    /// constitutes a format. The byte layout is this port's own (see
    /// `fmt.rs`); the contents mirror what tex.web dumps.
    pub fn store_fmt(&mut self) -> TexResult<Vec<u8>> {
        // §1302: a format may not be dumped inside a group.
        if self.save.save_ptr != 0 {
            self.print_err("You can't dump inside a group");
            self.help(&["`{...\\dump}' is a no-no."]);
            return Err(self.fatal_error("\\dump inside a group"));
        }
        // §1324: the trie must be packed before it can be dumped.
        if self.hy.trie_not_ready {
            self.init_trie()?;
        }
        // §1328: create the format_ident and inform the user.
        let ident = format!(
            " (preloaded format={} {}.{}.{})",
            self.job_name.as_deref().unwrap_or("texput"),
            self.eqtb.int_par(crate::eqtb::YEAR_CODE),
            self.eqtb.int_par(crate::eqtb::MONTH_CODE),
            self.eqtb.int_par(crate::eqtb::DAY_CODE),
        );
        let fmt_name = format!("{}.fmt", self.job_name.as_deref().unwrap_or("texput"));
        self.print_nl_chars("Beginning to dump on file ");
        self.print_chars(&fmt_name);
        self.print_nl_chars("");
        self.print_chars(&ident);
        let mut w = crate::fmt::FmtWriter::default();
        w.buf.extend_from_slice(b"SabiTeXfmt4");
        // §1307: the check constants.
        for v in [
            self.sizes.mem_top,
            self.sizes.hash_size,
            self.sizes.hash_prime,
            self.sizes.font_max,
            self.sizes.font_mem_size as i32,
            self.sizes.max_strings as i32,
            self.sizes.pool_size as i32,
            self.sizes.trie_size,
            self.sizes.trie_op_size,
            self.sizes.hyph_size,
        ] {
            w.i32(v);
        }
        // etex.ch: flush pseudo files, then dump the e-TeX state.
        while self.pseudo_files != crate::types::NULL {
            self.pseudo_close();
        }
        w.u8(u8::from(self.etex_mode));
        // etex.ch: disable all enhancements in the dumped state.
        let st = self.eqtb.lay.int_base + crate::eqtb::ETEX_STATE_CODE;
        for j in 0..crate::eqtb::ETEX_STATES {
            self.eqtb.set_int(st + j, 0);
        }
        let __b = w.buf.len();
        self.strings.dump(&mut w);
        if std::env::var("SABITEX_FMT_SIZES").is_ok() {
            eprintln!("FMT strings.dump: {} bytes", w.buf.len() - __b);
        }
        // §1309: log the string statistics.
        self.print_ln();
        let sp = self.strings.str_ptr() as i32;
        self.print_int(sp);
        self.print_chars(" strings of total length ");
        let pp = self.strings.pool_ptr() as i32;
        self.print_int(pp);
        let __b = w.buf.len();
        self.mem.dump(&mut w);
        if std::env::var("SABITEX_FMT_SIZES").is_ok() {
            eprintln!("FMT mem.dump: {} bytes", w.buf.len() - __b);
        }
        // §1311: log the memory usage.
        self.print_ln();
        let dumped =
            self.mem.lo_mem_max + 1 - self.mem.mem_bot + self.mem.mem_end - self.mem.hi_mem_min + 1;
        self.print_int(dumped);
        self.print_chars(" memory locations dumped; current usage is ");
        let vu = self.mem.var_used;
        self.print_int(vu);
        self.print_char('&' as i32);
        let du = self.mem.dyn_used;
        self.print_int(du);
        // etex.ch §1311: the sparse array roots (int_val..tok_val).
        for k in 0..=5 {
            w.i32(self.sa_root[k]);
        }
        let __b = w.buf.len();
        self.eqtb.dump(&mut w);
        if std::env::var("SABITEX_FMT_SIZES").is_ok() {
            eprintln!("FMT eqtb.dump: {} bytes", w.buf.len() - __b);
        }
        w.i32(self.par_loc);
        w.i32(self.write_loc);
        // §1318: log the control-sequence count.
        self.print_ln();
        let cs = self.eqtb.cs_count;
        self.print_int(cs);
        self.print_chars(" multiletter control sequences");
        let __b = w.buf.len();
        self.fonts.dump(&mut w);
        if std::env::var("SABITEX_FMT_SIZES").is_ok() {
            eprintln!("FMT fonts.dump: {} bytes", w.buf.len() - __b);
        }
        // §1320-§1322: log the font identities.
        for k in 0..=self.fonts.font_ptr {
            self.print_nl_chars("\\font");
            let t = self.eqtb.font_id_text(k);
            self.print_esc(t);
            self.print_char('=' as i32);
            let name = self.fonts.name[k as usize].clone();
            self.print_chars(&name);
            if self.fonts.size[k as usize] != self.fonts.dsize[k as usize] {
                self.print_chars(" at ");
                let s = self.fonts.size[k as usize];
                self.print_scaled(s);
                self.print_chars("pt");
            }
        }
        self.print_ln();
        let fm = self.fonts.fmem_ptr;
        self.print_int(fm);
        self.print_chars(" words of font info for ");
        let fp = self.fonts.font_ptr;
        self.print_int(fp);
        if fp == 1 {
            self.print_chars(" preloaded font");
        } else {
            self.print_chars(" preloaded fonts");
        }
        self.hy.dump(&mut w);
        // §1324: log the hyphenation statistics.
        self.print_ln();
        let hc = self.hy.hyph_count;
        self.print_int(hc);
        if hc == 1 {
            self.print_chars(" hyphenation exception");
        } else {
            self.print_chars(" hyphenation exceptions");
        }
        self.print_nl_chars("Hyphenation trie of length ");
        let tm = self.hy.trie_max;
        self.print_int(tm);
        self.print_chars(" has ");
        let tp = self.hy.trie_op_ptr;
        self.print_int(tp);
        self.print_chars(" ops out of ");
        let ts = self.sizes.trie_op_size;
        self.print_int(ts);
        for k in (0..=255).rev() {
            if self.hy.trie_used[k as usize] > 0 {
                self.print_nl_chars("  ");
                let u = i32::from(self.hy.trie_used[k as usize]);
                self.print_int(u);
                self.print_chars(" for language ");
                self.print_int(k);
            }
        }
        // §1326: a couple more things and the closing check word.
        w.u8(self.interaction);
        w.str(&ident);
        self.format_ident = ident;
        w.i32(69069);
        // §1326: we have already printed a lot of statistics, so prevent
        // them from appearing again at close_files time.
        let loc = self.eqtb.lay.int_base + crate::eqtb::TRACING_STATS_CODE;
        self.eqtb.set_int(loc, 0);
        Ok(w.buf)
    }

    /// `load_fmt_file` (§1303 + the undump halves): restores a format
    /// produced by [`Engine::store_fmt`] into a freshly built engine with
    /// the same `Sizes`.
    /// Streams the pending transcript to the file system if the backend
    /// supports appends. Called at line boundaries once the buffer
    /// grows, and by the driver at job end (`final_flush`).
    pub fn flush_log_stream(&mut self) {
        if !self.log_opened || self.log.len() <= self.log_streamed {
            return;
        }
        let name = format!(
            "{}.log",
            self.job_name.clone().unwrap_or_else(|| "texput".into())
        );
        let chunk = self.log[self.log_streamed..].to_vec();
        // The FIRST flush must truncate: append_file alone would extend
        // a transcript left over from a previous run of the same job
        // (which once made us chase phantom errors in a stale log).
        let ok = if self.log_streamed == 0 {
            self.fs.write_file(&name, crate::io::OutKind::Log, &chunk)
        } else {
            self.fs.append_file(&name, crate::io::OutKind::Log, &chunk)
        };
        if ok {
            self.log_streamed = self.log.len();
        }
    }

    /// Final transcript write: whatever was not streamed goes out as
    /// one append (or, without append support, one whole-file write).
    pub fn write_log_file(&mut self) {
        let name = format!(
            "{}.log",
            self.job_name.clone().unwrap_or_else(|| "texput".into())
        );
        if self.log_streamed > 0 {
            let chunk = self.log[self.log_streamed..].to_vec();
            self.fs.append_file(&name, crate::io::OutKind::Log, &chunk);
            self.log_streamed = self.log.len();
        } else {
            let data = std::mem::take(&mut self.log);
            self.fs.write_file(&name, crate::io::OutKind::Log, &data);
            self.log = data;
        }
    }

    /// Presets the interaction mode before the job starts (the CLI's
    /// -interaction option; \batchmode..\errorstopmode = 0..3).
    pub fn set_interaction(&mut self, mode: u8) {
        self.interaction = mode.min(3);
        if self.interaction == 0 {
            self.prn.selector = crate::print::NO_PRINT;
        } else {
            self.prn.selector = crate::print::TERM_ONLY;
        }
    }

    /// Sets \time/\day/\month/\year (tex.web fix_date_and_time). The
    /// engine's default is a fixed date for determinism; hosts opt in
    /// to real time explicitly.
    pub fn set_date_and_time(&mut self, year: i32, month: i32, day: i32, minutes: i32) {
        let base = self.eqtb.lay.int_base;
        self.eqtb.set_int(base + crate::eqtb::TIME_CODE, minutes);
        self.eqtb.set_int(base + crate::eqtb::DAY_CODE, day);
        self.eqtb.set_int(base + crate::eqtb::MONTH_CODE, month);
        self.eqtb.set_int(base + crate::eqtb::YEAR_CODE, year);
    }

    pub fn load_fmt(&mut self, bytes: &[u8]) -> TexResult<()> {
        let mut r = crate::fmt::FmtReader::new(bytes);
        let res = self.load_fmt_inner(&mut r);
        match res {
            Ok(()) => Ok(()),
            Err(msg) => Err(self.fatal_error(msg)),
        }
    }

    fn load_fmt_inner(&mut self, r: &mut crate::fmt::FmtReader) -> crate::fmt::FmtResult<()> {
        let magic = b"SabiTeXfmt4";
        for &b in magic {
            if r.u8()? != b {
                return Err("not a SabiTeX format file");
            }
        }
        for expect in [
            self.sizes.mem_top,
            self.sizes.hash_size,
            self.sizes.hash_prime,
            self.sizes.font_max,
            self.sizes.font_mem_size as i32,
            self.sizes.max_strings as i32,
            self.sizes.pool_size as i32,
            self.sizes.trie_size,
            self.sizes.trie_op_size,
            self.sizes.hyph_size,
        ] {
            if r.i32()? != expect {
                return Err("format file made with different constants");
            }
        }
        // etex.ch: undump the e-TeX state and re-initialize the per-mode
        // variables.
        self.etex_mode = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err("bad eTeX mode in format"),
        };
        self.init_etex_mode_vars();
        // A14 probe: Instant::now() panics on wasm32-unknown-unknown,
        // so the clock only starts when the env var asks for it.
        #[cfg(not(target_arch = "wasm32"))]
        let __t = std::env::var("SABITEX_TIME_UNDUMP")
            .ok()
            .map(|_| std::time::Instant::now());
        self.strings.undump(r)?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(t) = __t {
            eprintln!("UNDUMP strings: {:?}", t.elapsed());
        }
        // A14 probe: Instant::now() panics on wasm32-unknown-unknown,
        // so the clock only starts when the env var asks for it.
        #[cfg(not(target_arch = "wasm32"))]
        let __t = std::env::var("SABITEX_TIME_UNDUMP")
            .ok()
            .map(|_| std::time::Instant::now());
        self.mem.undump(r, self.pristine)?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(t) = __t {
            eprintln!("UNDUMP mem: {:?}", t.elapsed());
        }
        for k in 0..=5 {
            self.sa_root[k] = r.i32()?;
        }
        // A14 probe: Instant::now() panics on wasm32-unknown-unknown,
        // so the clock only starts when the env var asks for it.
        #[cfg(not(target_arch = "wasm32"))]
        let __t = std::env::var("SABITEX_TIME_UNDUMP")
            .ok()
            .map(|_| std::time::Instant::now());
        self.eqtb.undump(r)?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(t) = __t {
            eprintln!("UNDUMP eqtb: {:?}", t.elapsed());
        }
        self.par_loc = r.i32()?;
        self.par_token = crate::tokens::CS_TOKEN_FLAG + self.par_loc;
        self.write_loc = r.i32()?;
        // A14 probe: Instant::now() panics on wasm32-unknown-unknown,
        // so the clock only starts when the env var asks for it.
        #[cfg(not(target_arch = "wasm32"))]
        let __t = std::env::var("SABITEX_TIME_UNDUMP")
            .ok()
            .map(|_| std::time::Instant::now());
        self.fonts.undump(r, self.pristine)?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(t) = __t {
            eprintln!("UNDUMP fonts: {:?}", t.elapsed());
        }
        self.hy.undump(r, self.pristine)?;
        self.interaction = r.u8()?;
        self.format_ident = r.str()?;
        if r.i32()? != 69069 {
            return Err("format file check word is wrong");
        }
        r.done()?;
        Ok(())
    }

    /// `eTeX_ex` (etex.ch): are the extended features enabled?
    pub fn etex_ex(&self) -> bool {
        self.etex_mode
    }

    /// `TeXXeT_en` (etex.ch): is TeX--XeT enabled?
    pub fn texxet_en(&self) -> bool {
        self.eqtb
            .int_par(crate::eqtb::ETEX_STATE_CODE /* + TeXXeT_code (0) */)
            > 0
    }

    /// `eTeX_enabled(b, j, k)` (etex.ch): complain when an optional
    /// feature is disabled.
    pub fn etex_enabled(&mut self, b: bool, j: u16, k: i32) -> TexResult<bool> {
        if !b {
            self.print_err("Improper ");
            self.print_cmd_chr(j, k);
            self.help(&["Sorry, this optional e-TeX feature has been disabled."]);
            self.error()?;
        }
        Ok(b)
    }

    /// etex.ch: per-mode variables (`max_reg_num` and friends).
    pub fn init_etex_mode_vars(&mut self) {
        self.max_reg_num = if self.etex_mode { 32767 } else { 255 };
    }

    /// etex.ch §1337 `@<Generate all eTeX primitives@>`: defined only when
    /// a virgin INITEX enters extended mode (a format then carries them in
    /// its dumped hash table).
    pub fn generate_etex_primitives(&mut self) -> TexResult<()> {
        use crate::cmds::{CONVERT, EXPAND_AFTER, IF_TEST, LAST_ITEM, PREFIX, SET_PAGE_INT, THE};
        self.primitive("lastnodetype", LAST_ITEM, crate::scan::LAST_NODE_TYPE_CODE)?;
        self.primitive("eTeXversion", LAST_ITEM, crate::scan::ETEX_VERSION_CODE)?;
        self.primitive("eTeXrevision", CONVERT, crate::toks::ETEX_REVISION_CODE)?;
        // pdfTeX utilities required by expl3 (sabitex policy: extended
        // mode only, like the e-TeX/pTeX primitives).
        self.primitive("expanded", CONVERT, crate::toks::EXPANDED_CODE)?;
        self.primitive("pdfstrcmp", CONVERT, crate::toks::PDF_STRCMP_CODE)?;
        self.primitive("strcmp", CONVERT, crate::toks::PDF_STRCMP_CODE)?;
        self.primitive("pdffilesize", CONVERT, crate::toks::PDF_FILE_SIZE_CODE)?;
        self.primitive("XeTeXversion", LAST_ITEM, crate::scan::XETEX_VERSION_CODE)?;
        self.primitive("XeTeXrevision", CONVERT, crate::toks::XETEX_REVISION_CODE)?;
        self.primitive(
            "pdfpagewidth",
            crate::cmds::ASSIGN_DIMEN,
            self.eqtb.lay.dimen_base + crate::eqtb::PDF_PAGE_WIDTH_CODE,
        )?;
        self.primitive(
            "pdfpageheight",
            crate::cmds::ASSIGN_DIMEN,
            self.eqtb.lay.dimen_base + crate::eqtb::PDF_PAGE_HEIGHT_CODE,
        )?;
        self.primitive("shellescape", LAST_ITEM, crate::scan::SHELL_ESCAPE_CODE)?;
        self.primitive("pdfshellescape", LAST_ITEM, crate::scan::SHELL_ESCAPE_CODE)?;
        self.primitive(
            "pdfcreationdate",
            CONVERT,
            crate::toks::PDF_CREATION_DATE_CODE,
        )?;
        self.primitive("creationdate", CONVERT, crate::toks::PDF_CREATION_DATE_CODE)?;
        // The additional conditionals and the \unless prefix.
        self.primitive("unless", EXPAND_AFTER, 1)?;
        self.primitive("ifdefined", IF_TEST, crate::cond::IF_DEF_CODE)?;
        self.primitive("ifcsname", IF_TEST, crate::cond::IF_CS_CODE)?;
        self.primitive("ifincsname", IF_TEST, crate::cond::IF_IN_CSNAME_CODE)?;
        self.primitive("iftdir", IF_TEST, crate::cond::IF_TDIR_CODE)?;
        self.primitive("ifydir", IF_TEST, crate::cond::IF_YDIR_CODE)?;
        self.primitive("ifmdir", IF_TEST, crate::cond::IF_MDIR_CODE)?;
        self.primitive("iffontchar", IF_TEST, crate::cond::IF_FONT_CHAR_CODE)?;
        // \protected macros.
        self.primitive("protected", PREFIX, 8)?;
        // Token-list helpers.
        self.primitive("unexpanded", THE, 1)?;
        self.primitive("detokenize", THE, crate::toks::SHOW_TOKENS)?;
        // Input extensions.
        self.primitive("scantokens", crate::cmds::INPUT, 2)?;
        self.primitive("readline", crate::cmds::READ_TO_CS, 1)?;
        let eel = self.eqtb.lay.local_base + crate::eqtb::EVERY_EOF_OFFSET;
        self.primitive("everyeof", crate::cmds::ASSIGN_TOKS, eel)?;
        // Interaction and current-state queries.
        self.primitive("interactionmode", SET_PAGE_INT, 2)?;
        self.primitive(
            "currentgrouplevel",
            LAST_ITEM,
            crate::scan::CURRENT_GROUP_LEVEL_CODE,
        )?;
        self.primitive(
            "currentgrouptype",
            LAST_ITEM,
            crate::scan::CURRENT_GROUP_TYPE_CODE,
        )?;
        self.primitive(
            "currentiflevel",
            LAST_ITEM,
            crate::scan::CURRENT_IF_LEVEL_CODE,
        )?;
        self.primitive(
            "currentiftype",
            LAST_ITEM,
            crate::scan::CURRENT_IF_TYPE_CODE,
        )?;
        self.primitive(
            "currentifbranch",
            LAST_ITEM,
            crate::scan::CURRENT_IF_BRANCH_CODE,
        )?;
        // Expressions and glue dissection.
        self.primitive("numexpr", LAST_ITEM, crate::scan::NUMEXPR_CODE)?;
        self.primitive("dimexpr", LAST_ITEM, crate::scan::DIMEXPR_CODE)?;
        self.primitive("glueexpr", LAST_ITEM, crate::scan::GLUEEXPR_CODE)?;
        self.primitive("muexpr", LAST_ITEM, crate::scan::MUEXPR_CODE)?;
        self.primitive(
            "gluestretchorder",
            LAST_ITEM,
            crate::scan::GLUE_STRETCH_ORDER_CODE,
        )?;
        self.primitive(
            "glueshrinkorder",
            LAST_ITEM,
            crate::scan::GLUE_SHRINK_ORDER_CODE,
        )?;
        self.primitive("gluestretch", LAST_ITEM, crate::scan::GLUE_STRETCH_CODE)?;
        self.primitive("glueshrink", LAST_ITEM, crate::scan::GLUE_SHRINK_CODE)?;
        self.primitive("mutoglue", LAST_ITEM, crate::scan::MU_TO_GLUE_CODE)?;
        self.primitive("gluetomu", LAST_ITEM, crate::scan::GLUE_TO_MU_CODE)?;
        // Font and \parshape queries.
        self.primitive("fontcharwd", LAST_ITEM, crate::scan::FONT_CHAR_WD_CODE)?;
        self.primitive("fontcharht", LAST_ITEM, crate::scan::FONT_CHAR_HT_CODE)?;
        self.primitive("fontchardp", LAST_ITEM, crate::scan::FONT_CHAR_DP_CODE)?;
        self.primitive("fontcharic", LAST_ITEM, crate::scan::FONT_CHAR_IC_CODE)?;
        self.primitive(
            "parshapelength",
            LAST_ITEM,
            crate::scan::PAR_SHAPE_LENGTH_CODE,
        )?;
        self.primitive(
            "parshapeindent",
            LAST_ITEM,
            crate::scan::PAR_SHAPE_INDENT_CODE,
        )?;
        self.primitive(
            "parshapedimen",
            LAST_ITEM,
            crate::scan::PAR_SHAPE_DIMEN_CODE,
        )?;
        // The e-TeX integer parameters (etex.ch: etex_int_base..).
        {
            use crate::cmds::{ASSIGN_INT, SET_SHAPE};
            let int_base = self.eqtb.lay.int_base;
            for (i, name) in crate::eqtb::INT_PARAM_NAMES.iter().enumerate() {
                if (i as i32) >= crate::eqtb::TEX_INT_PARS {
                    self.primitive(name, ASSIGN_INT, int_base + i as i32)?;
                }
            }
            // The penalties arrays (set_shape commands like \parshape).
            let pen = self.eqtb.lay.etex_pen_base;
            self.primitive("interlinepenalties", SET_SHAPE, pen)?;
            self.primitive("clubpenalties", SET_SHAPE, pen + 1)?;
            self.primitive("widowpenalties", SET_SHAPE, pen + 2)?;
            self.primitive("displaywidowpenalties", SET_SHAPE, pen + 3)?;
            // The \show extensions.
            use crate::cmds::XRAY;
            self.primitive("showgroups", XRAY, crate::cmdchr::SHOW_GROUPS_CODE)?;
            self.primitive("showtokens", XRAY, crate::toks::SHOW_TOKENS)?;
            self.primitive("showifs", XRAY, crate::cmdchr::SHOW_IFS_CODE)?;
            // \middle (a right noad with subtype middle_noad).
            use crate::cmds::LEFT_RIGHT;
            self.primitive("middle", LEFT_RIGHT, i32::from(crate::math::MIDDLE_NOAD))?;
            // pTeX/upTeX (sabitex policy: the Japanese primitives are
            // part of extended mode, keeping compatibility mode — and
            // with it TRIP — indistinguishable from TeX82).
            self.primitive(
                "kcatcode",
                crate::cmds::DEF_CODE,
                self.eqtb.lay.kcat_code_base,
            )?;
            self.primitive("jfont", crate::cmds::DEF_FONT, 1)?;
            let gb = self.eqtb.lay.glue_base;
            self.primitive(
                "kanjiskip",
                crate::cmds::ASSIGN_GLUE,
                gb + crate::eqtb::KANJI_SKIP_CODE,
            )?;
            self.primitive(
                "xkanjiskip",
                crate::cmds::ASSIGN_GLUE,
                gb + crate::eqtb::XKANJI_SKIP_CODE,
            )?;
            // utospacing family: SET_AUTO cmd toggles the eqtb slots.
            self.primitive(
                "xspcode",
                crate::cmds::DEF_CODE,
                self.eqtb.lay.auto_xsp_code_base,
            )?;
            self.primitive("inhibitxspcode", crate::cmds::ASSIGN_INHIBIT_XSP, 0)?;
            // XeTeX Unicode math primitives (xetex.web §27671-§27916).
            // Defining the REAL names makes the LaTeX kernel take its
            // Unicode-engine branches (fonttext.ltx: TU encoding, Latin
            // Modern defaults), matching real xelatex.
            use crate::cmds::XETEX_DEF_CODE;
            let lay = self.eqtb.lay.clone();
            self.primitive("Umathcodenum", XETEX_DEF_CODE, lay.math_code_base)?;
            self.primitive("XeTeXmathcodenum", XETEX_DEF_CODE, lay.math_code_base)?;
            self.primitive("Umathcode", XETEX_DEF_CODE, lay.math_code_base + 1)?;
            self.primitive("XeTeXmathcode", XETEX_DEF_CODE, lay.math_code_base + 1)?;
            self.primitive("XeTeXcharclass", XETEX_DEF_CODE, lay.sf_code_base)?;
            self.primitive("Udelcodenum", XETEX_DEF_CODE, lay.del_code_base)?;
            self.primitive("XeTeXdelcodenum", XETEX_DEF_CODE, lay.del_code_base)?;
            self.primitive("Udelcode", XETEX_DEF_CODE, lay.del_code_base + 1)?;
            self.primitive("XeTeXdelcode", XETEX_DEF_CODE, lay.del_code_base + 1)?;
            self.primitive("Umathcharnum", MATH_CHAR_NUM, 1)?;
            self.primitive("XeTeXmathcharnum", MATH_CHAR_NUM, 1)?;
            self.primitive("Umathchar", MATH_CHAR_NUM, 2)?;
            self.primitive("XeTeXmathchar", MATH_CHAR_NUM, 2)?;
            self.primitive("Udelimiter", DELIM_NUM, 1)?;
            self.primitive("XeTeXdelimiter", DELIM_NUM, 1)?;
            self.primitive("Uradical", RADICAL, 1)?;
            self.primitive("XeTeXradical", RADICAL, 1)?;
            self.primitive("Umathaccent", MATH_ACCENT, 1)?;
            self.primitive("XeTeXmathaccent", MATH_ACCENT, 1)?;
            self.primitive(
                "Umathcharnumdef",
                SHORTHAND_DEF,
                crate::prefix::XETEX_MATH_CHAR_NUM_DEF_CODE,
            )?;
            self.primitive(
                "XeTeXmathcharnumdef",
                SHORTHAND_DEF,
                crate::prefix::XETEX_MATH_CHAR_NUM_DEF_CODE,
            )?;
            self.primitive(
                "Umathchardef",
                SHORTHAND_DEF,
                crate::prefix::XETEX_MATH_CHAR_DEF_CODE,
            )?;
            self.primitive(
                "XeTeXmathchardef",
                SHORTHAND_DEF,
                crate::prefix::XETEX_MATH_CHAR_DEF_CODE,
            )?;
            self.primitive("Uchar", CONVERT, crate::toks::XETEX_UCHAR_CODE)?;
            self.primitive("Ucharcat", CONVERT, crate::toks::XETEX_UCHARCAT_CODE)?;
            self.primitive(
                "pdfsavepos",
                EXTENSION,
                i32::from(crate::par::SAVE_POS_NODE),
            )?;
            self.primitive("pdflastxpos", LAST_ITEM, crate::scan::PDF_LAST_X_POS_CODE)?;
            self.primitive("pdflastypos", LAST_ITEM, crate::scan::PDF_LAST_Y_POS_CODE)?;
            self.primitive(
                "kansujichar",
                crate::cmds::DEF_CODE,
                self.eqtb.lay.kansuji_base,
            )?;
            self.primitive("kansuji", CONVERT, crate::toks::KANSUJI_CODE)?;
            self.primitive("inhibitglue", crate::cmds::INHIBIT_GLUE, 0)?;
            self.primitive("kchar", crate::cmds::KCHAR_NUM, 0)?;
            use crate::cmds::ASSIGN_KINSOKU;
            self.primitive(
                "prebreakpenalty",
                ASSIGN_KINSOKU,
                i32::from(crate::kanji::PRE_BREAK_PENALTY_CODE),
            )?;
            self.primitive(
                "postbreakpenalty",
                ASSIGN_KINSOKU,
                i32::from(crate::kanji::POST_BREAK_PENALTY_CODE),
            )?;
            self.primitive(
                "jcharwidowpenalty",
                crate::cmds::ASSIGN_INT,
                self.eqtb.lay.int_base + crate::eqtb::JCHR_WIDOW_PENALTY_CODE,
            )?;
            use crate::cmds::SET_AUTO_SPACING;
            self.primitive("autospacing", SET_AUTO_SPACING, 1)?;
            self.primitive("noautospacing", SET_AUTO_SPACING, 0)?;
            self.primitive("autoxspacing", SET_AUTO_SPACING, 3)?;
            self.primitive("noautoxspacing", SET_AUTO_SPACING, 2)?;
            // Mark classes.
            use crate::cmds::{MARK, TOP_BOT_MARK};
            use crate::expand::MARKS_CODE;
            self.primitive("marks", MARK, MARKS_CODE)?;
            self.primitive("topmarks", TOP_BOT_MARK, MARKS_CODE)?;
            self.primitive("firstmarks", TOP_BOT_MARK, 1 + MARKS_CODE)?;
            self.primitive("botmarks", TOP_BOT_MARK, 2 + MARKS_CODE)?;
            self.primitive("splitfirstmarks", TOP_BOT_MARK, 3 + MARKS_CODE)?;
            self.primitive("splitbotmarks", TOP_BOT_MARK, 4 + MARKS_CODE)?;
            // TeX--XeT text-direction primitives.
            use crate::cmds::VALIGN;
            self.primitive("beginL", VALIGN, i32::from(crate::nodes::BEGIN_L_CODE))?;
            self.primitive("endL", VALIGN, i32::from(crate::nodes::END_L_CODE))?;
            self.primitive("beginR", VALIGN, i32::from(crate::nodes::BEGIN_R_CODE))?;
            self.primitive("endR", VALIGN, i32::from(crate::nodes::END_R_CODE))?;
            // Saved items (\savingvdiscards).
            use crate::cmds::UN_VBOX;
            self.primitive("pagediscards", UN_VBOX, crate::control::LAST_BOX_CODE)?;
            self.primitive("splitdiscards", UN_VBOX, crate::control::VSPLIT_CODE)?;
        }
        Ok(())
    }

    /// `fix_date_and_time` (§241/§1337): tex.web's "self-evident truths"
    /// defaults; hosts may overwrite the four parameters afterwards.
    pub fn fix_date_and_time(&mut self) {
        let base = self.eqtb.lay.int_base;
        self.eqtb.set_int(base + crate::eqtb::TIME_CODE, 12 * 60);
        self.eqtb.set_int(base + crate::eqtb::DAY_CODE, 4);
        self.eqtb.set_int(base + crate::eqtb::MONTH_CODE, 7);
        self.eqtb.set_int(base + crate::eqtb::YEAR_CODE, 1776);
    }

    /// §226-§1265 and friends: "Put each of TeX's primitives into the hash
    /// table" — every `primitive(...)` call read so far, in Part order.
    fn install_primitives(&mut self) -> TexResult<()> {
        use crate::cmdchr::*;
        use crate::cond::*;
        use crate::scan::{BADNESS_CODE, INPUT_LINE_NO_CODE};
        use crate::toks::*;
        let lay = self.eqtb.lay.clone();
        // §226: glue parameters.
        for (i, name) in crate::eqtb::SKIP_PARAM_NAMES.iter().enumerate() {
            if (i as i32) >= crate::eqtb::TEX_GLUE_PARS {
                continue; // pTeX glues register in extended mode only
            }
            let cmd = if (i as i32) < crate::eqtb::THIN_MU_SKIP_CODE {
                ASSIGN_GLUE
            } else {
                ASSIGN_MU_GLUE
            };
            self.primitive(name, cmd, lay.glue_base + i as i32)?;
        }
        // §230: token-list parameters. \everyeof (the last entry) is an
        // e-TeX primitive, generated only in extended mode.
        for (i, name) in crate::eqtb::TOKS_PARAM_NAMES.iter().enumerate() {
            if *name == "everyeof" {
                continue;
            }
            self.primitive(name, ASSIGN_TOKS, lay.output_routine_loc + i as i32)?;
        }
        // §238: integer parameters. Codes >= tex_int_pars are e-TeX
        // primitives, generated only in extended mode.
        for (i, name) in crate::eqtb::INT_PARAM_NAMES.iter().enumerate() {
            if i as i32 >= crate::eqtb::TEX_INT_PARS {
                break;
            }
            self.primitive(name, ASSIGN_INT, lay.int_base + i as i32)?;
        }
        // §248: dimension parameters.
        for (i, name) in crate::eqtb::DIMEN_PARAM_NAMES.iter().enumerate() {
            if (i as i32) >= crate::eqtb::TEX_DIMEN_PARS {
                continue; // pdfTeX dimens register in extended mode only
            }
            self.primitive(name, ASSIGN_DIMEN, lay.dimen_base + i as i32)?;
        }
        // §265: assorted primitives.
        self.primitive(" ", EX_SPACE, 0)?;
        self.primitive("/", ITAL_CORR, 0)?;
        self.primitive("accent", ACCENT, 0)?;
        self.primitive("advance", ADVANCE, 0)?;
        self.primitive("afterassignment", AFTER_ASSIGNMENT, 0)?;
        self.primitive("aftergroup", AFTER_GROUP, 0)?;
        self.primitive("begingroup", BEGIN_GROUP, 0)?;
        self.primitive("char", CHAR_NUM, 0)?;
        self.primitive("csname", CS_NAME, 0)?;
        self.primitive("delimiter", DELIM_NUM, 0)?;
        self.primitive("divide", DIVIDE, 0)?;
        self.primitive("endcsname", END_CS_NAME, 0)?;
        let p = self.primitive("endgroup", END_GROUP, 0)?;
        let s = self.strings.intern("endgroup")?;
        self.eqtb.set_text(lay.frozen_end_group, s);
        *self.eqtb.word_mut(lay.frozen_end_group) = self.eqtb.word(p);
        self.primitive("expandafter", EXPAND_AFTER, 0)?;
        self.primitive("font", DEF_FONT, 0)?;
        self.primitive("fontdimen", ASSIGN_FONT_DIMEN, 0)?;
        self.primitive("hyphenchar", ASSIGN_FONT_INT, 0)?;
        self.primitive("skewchar", ASSIGN_FONT_INT, 1)?;
        self.primitive("halign", HALIGN, 0)?;
        self.primitive("hrule", HRULE, 0)?;
        self.primitive("ignorespaces", IGNORE_SPACES, 0)?;
        self.primitive("insert", INSERT, 0)?;
        self.primitive("mark", MARK, 0)?;
        self.primitive("mathaccent", MATH_ACCENT, 0)?;
        self.primitive("mathchar", MATH_CHAR_NUM, 0)?;
        self.primitive("mathchoice", MATH_CHOICE, 0)?;
        self.primitive("multiply", MULTIPLY, 0)?;
        self.primitive("noalign", NO_ALIGN, 0)?;
        self.primitive("noboundary", NO_BOUNDARY, 0)?;
        self.primitive("noexpand", NO_EXPAND, 0)?;
        self.primitive("nonscript", NON_SCRIPT, 0)?;
        self.primitive("omit", OMIT, 0)?;
        // etex.ch: set_shape commands carry their eqtb location as chr.
        self.primitive("parshape", SET_SHAPE, lay.par_shape_loc)?;
        self.primitive("penalty", BREAK_PENALTY, 0)?;
        self.primitive("prevgraf", SET_PREV_GRAF, 0)?;
        self.primitive("radical", RADICAL, 0)?;
        self.primitive("read", READ_TO_CS, 0)?;
        let p = self.primitive("relax", RELAX, TOO_BIG_CHAR)?; // cf. scan_file_name
        let s = self.strings.intern("relax")?;
        self.eqtb.set_text(lay.frozen_relax, s);
        *self.eqtb.word_mut(lay.frozen_relax) = self.eqtb.word(p);
        self.primitive("setbox", SET_BOX, 0)?;
        self.primitive("the", THE, 0)?;
        // etex.ch: register commands carry mem_bot(+type); chr values
        // outside mem_bot..lo_mem_stat_max are sparse-array leaf pointers.
        let mb = self.mem.mem_bot;
        self.primitive("toks", TOKS_REGISTER, mb)?;
        self.primitive("vadjust", VADJUST, 0)?;
        self.primitive("valign", VALIGN, 0)?;
        self.primitive("vcenter", VCENTER, 0)?;
        self.primitive("vrule", VRULE, 0)?;
        // §334: \par.
        let p = self.primitive("par", PAR_END, TOO_BIG_CHAR)?;
        self.par_loc = p;
        self.par_token = CS_TOKEN_FLAG + p;
        // §376: \input, \endinput.
        self.primitive("input", INPUT, 0)?;
        self.primitive("endinput", INPUT, 1)?;
        // §1272: \openin, \closein.
        self.primitive("openin", IN_STREAM, 1)?;
        self.primitive("closein", IN_STREAM, 0)?;
        // §384: marks.
        self.primitive("topmark", TOP_BOT_MARK, 0)?;
        self.primitive("firstmark", TOP_BOT_MARK, 1)?;
        self.primitive("botmark", TOP_BOT_MARK, 2)?;
        self.primitive("splitfirstmark", TOP_BOT_MARK, 3)?;
        self.primitive("splitbotmark", TOP_BOT_MARK, 4)?;
        // §411: registers.
        let mb = self.mem.mem_bot;
        self.primitive("count", REGISTER, mb + i32::from(INT_VAL))?;
        self.primitive("dimen", REGISTER, mb + i32::from(DIMEN_VAL))?;
        self.primitive("skip", REGISTER, mb + i32::from(GLUE_VAL))?;
        self.primitive("muskip", REGISTER, mb + i32::from(MU_VAL))?;
        // §416: state queries.
        self.primitive("spacefactor", SET_AUX, HMODE)?;
        self.primitive("prevdepth", SET_AUX, VMODE)?;
        self.primitive("deadcycles", SET_PAGE_INT, 0)?;
        self.primitive("insertpenalties", SET_PAGE_INT, 1)?;
        self.primitive("wd", SET_BOX_DIMEN, WIDTH_OFFSET)?;
        self.primitive("ht", SET_BOX_DIMEN, HEIGHT_OFFSET)?;
        self.primitive("dp", SET_BOX_DIMEN, DEPTH_OFFSET)?;
        self.primitive("lastpenalty", LAST_ITEM, i32::from(INT_VAL))?;
        self.primitive("lastkern", LAST_ITEM, i32::from(DIMEN_VAL))?;
        self.primitive("lastskip", LAST_ITEM, i32::from(GLUE_VAL))?;
        self.primitive("inputlineno", LAST_ITEM, INPUT_LINE_NO_CODE)?;
        self.primitive("badness", LAST_ITEM, BADNESS_CODE)?;
        // §468: convert codes.
        self.primitive("number", CONVERT, NUMBER_CODE)?;
        self.primitive("romannumeral", CONVERT, ROMAN_NUMERAL_CODE)?;
        self.primitive("string", CONVERT, STRING_CODE)?;
        self.primitive("meaning", CONVERT, MEANING_CODE)?;
        self.primitive("fontname", CONVERT, FONT_NAME_CODE)?;
        self.primitive("jobname", CONVERT, JOB_NAME_CODE)?;
        // §487: conditionals.
        self.primitive("if", IF_TEST, IF_CHAR_CODE)?;
        self.primitive("ifcat", IF_TEST, IF_CAT_CODE)?;
        self.primitive("ifnum", IF_TEST, IF_INT_CODE)?;
        self.primitive("ifdim", IF_TEST, IF_DIM_CODE)?;
        self.primitive("ifodd", IF_TEST, IF_ODD_CODE)?;
        self.primitive("ifvmode", IF_TEST, IF_VMODE_CODE)?;
        self.primitive("ifhmode", IF_TEST, IF_HMODE_CODE)?;
        self.primitive("ifmmode", IF_TEST, IF_MMODE_CODE)?;
        self.primitive("ifinner", IF_TEST, IF_INNER_CODE)?;
        self.primitive("ifvoid", IF_TEST, IF_VOID_CODE)?;
        self.primitive("ifhbox", IF_TEST, IF_HBOX_CODE)?;
        self.primitive("ifvbox", IF_TEST, IF_VBOX_CODE)?;
        self.primitive("ifx", IF_TEST, IFX_CODE)?;
        self.primitive("ifeof", IF_TEST, IF_EOF_CODE)?;
        self.primitive("iftrue", IF_TEST, IF_TRUE_CODE)?;
        self.primitive("iffalse", IF_TEST, IF_FALSE_CODE)?;
        self.primitive("ifcase", IF_TEST, IF_CASE_CODE)?;
        // §491: \fi, \or, \else.
        let p = self.primitive("fi", FI_OR_ELSE, i32::from(FI_CODE))?;
        let s = self.strings.intern("fi")?;
        self.eqtb.set_text(lay.frozen_fi, s);
        *self.eqtb.word_mut(lay.frozen_fi) = self.eqtb.word(p);
        self.primitive("or", FI_OR_ELSE, i32::from(OR_CODE))?;
        self.primitive("else", FI_OR_ELSE, i32::from(ELSE_CODE))?;
        // §1052: \end, \dump.
        self.primitive("end", STOP, 0)?;
        self.primitive("dump", STOP, 1)?;
        // §1208: prefixes and \def family.
        self.primitive("long", PREFIX, 1)?;
        self.primitive("outer", PREFIX, 2)?;
        self.primitive("global", PREFIX, 4)?;
        self.primitive("def", DEF, 0)?;
        self.primitive("gdef", DEF, 1)?;
        self.primitive("edef", DEF, 2)?;
        self.primitive("xdef", DEF, 3)?;
        // §1219: \let, \futurelet.
        self.primitive("let", LET, 0)?;
        self.primitive("futurelet", LET, 1)?;
        // §1222: shorthand definitions.
        self.primitive("chardef", SHORTHAND_DEF, crate::prefix::CHAR_DEF_CODE)?;
        self.primitive(
            "mathchardef",
            SHORTHAND_DEF,
            crate::prefix::MATH_CHAR_DEF_CODE,
        )?;
        self.primitive("countdef", SHORTHAND_DEF, crate::prefix::COUNT_DEF_CODE)?;
        self.primitive("dimendef", SHORTHAND_DEF, crate::prefix::DIMEN_DEF_CODE)?;
        self.primitive("skipdef", SHORTHAND_DEF, crate::prefix::SKIP_DEF_CODE)?;
        self.primitive("muskipdef", SHORTHAND_DEF, crate::prefix::MU_SKIP_DEF_CODE)?;
        self.primitive("toksdef", SHORTHAND_DEF, crate::prefix::TOKS_DEF_CODE)?;
        // §1230: code tables.
        self.primitive("catcode", DEF_CODE, lay.cat_code_base)?;
        self.primitive("mathcode", DEF_CODE, lay.math_code_base)?;
        self.primitive("lccode", DEF_CODE, lay.lc_code_base)?;
        self.primitive("uccode", DEF_CODE, lay.uc_code_base)?;
        self.primitive("sfcode", DEF_CODE, lay.sf_code_base)?;
        self.primitive("delcode", DEF_CODE, lay.del_code_base)?;
        self.primitive("textfont", DEF_FAMILY, lay.math_font_base)?;
        self.primitive("scriptfont", DEF_FAMILY, lay.math_font_base + 256)?;
        self.primitive("scriptscriptfont", DEF_FAMILY, lay.math_font_base + 512)?;
        // §1265: interaction modes.
        self.primitive("batchmode", SET_INTERACTION, 0)?;
        self.primitive("nonstopmode", SET_INTERACTION, 1)?;
        self.primitive("scrollmode", SET_INTERACTION, 2)?;
        self.primitive("errorstopmode", SET_INTERACTION, 3)?;
        // §1277: \message, \errmessage.
        self.primitive("message", MESSAGE, 0)?;
        self.primitive("errmessage", MESSAGE, 1)?;
        // §1286: \lowercase, \uppercase.
        self.primitive("lowercase", CASE_SHIFT, lay.lc_code_base)?;
        self.primitive("uppercase", CASE_SHIFT, lay.uc_code_base)?;
        // §1291: \show family.
        self.primitive("show", XRAY, SHOW_CODE)?;
        self.primitive("showbox", XRAY, SHOW_BOX_CODE)?;
        self.primitive("showthe", XRAY, SHOW_THE_CODE)?;
        self.primitive("showlists", XRAY, SHOW_LISTS_CODE)?;
        // §553: \nullfont.
        let p = self.primitive("nullfont", SET_FONT, 0)?;
        let s = self.strings.intern("nullfont")?;
        self.eqtb.set_text(lay.frozen_null_font, s);
        *self.eqtb.word_mut(lay.frozen_null_font) = self.eqtb.word(p);
        // §1058: glue commands.
        self.primitive("hskip", HSKIP, crate::control::SKIP_CODE)?;
        self.primitive("hfil", HSKIP, crate::control::FIL_CODE)?;
        self.primitive("hfill", HSKIP, crate::control::FILL_CODE)?;
        self.primitive("hss", HSKIP, crate::control::SS_CODE)?;
        self.primitive("hfilneg", HSKIP, crate::control::FIL_NEG_CODE)?;
        self.primitive("vskip", VSKIP, crate::control::SKIP_CODE)?;
        self.primitive("vfil", VSKIP, crate::control::FIL_CODE)?;
        self.primitive("vfill", VSKIP, crate::control::FILL_CODE)?;
        self.primitive("vss", VSKIP, crate::control::SS_CODE)?;
        self.primitive("vfilneg", VSKIP, crate::control::FIL_NEG_CODE)?;
        self.primitive("mskip", MSKIP, crate::control::MSKIP_CODE)?;
        // §1114: \discretionary, \-.
        self.primitive("discretionary", DISCRETIONARY, 0)?;
        self.primitive("-", DISCRETIONARY, 1)?;
        self.primitive("kern", KERN, i32::from(crate::nodes::EXPLICIT))?;
        self.primitive("mkern", MKERN, i32::from(crate::nodes::MU_GLUE))?;
        // §1071: box-making commands.
        self.primitive("moveleft", HMOVE, 1)?;
        self.primitive("moveright", HMOVE, 0)?;
        self.primitive("raise", VMOVE, 1)?;
        self.primitive("lower", VMOVE, 0)?;
        self.primitive("box", MAKE_BOX, crate::control::BOX_CODE)?;
        self.primitive("copy", MAKE_BOX, crate::control::COPY_CODE)?;
        self.primitive("lastbox", MAKE_BOX, crate::control::LAST_BOX_CODE)?;
        self.primitive("vsplit", MAKE_BOX, crate::control::VSPLIT_CODE)?;
        self.primitive("vtop", MAKE_BOX, crate::control::VTOP_CODE)?;
        self.primitive("vbox", MAKE_BOX, crate::control::VTOP_CODE + VMODE)?;
        self.primitive("hbox", MAKE_BOX, crate::control::VTOP_CODE + HMODE)?;
        self.primitive(
            "shipout",
            LEADER_SHIP,
            i32::from(crate::nodes::A_LEADERS) - 1,
        )?;
        self.primitive("leaders", LEADER_SHIP, i32::from(crate::nodes::A_LEADERS))?;
        self.primitive("cleaders", LEADER_SHIP, i32::from(crate::nodes::C_LEADERS))?;
        self.primitive("xleaders", LEADER_SHIP, i32::from(crate::nodes::X_LEADERS))?;
        // §1088: \indent, \noindent.
        self.primitive("indent", START_PAR, 1)?;
        self.primitive("noindent", START_PAR, 0)?;
        // §1107: \unpenalty, \unkern, \unskip.
        self.primitive(
            "unpenalty",
            REMOVE_ITEM,
            i32::from(crate::nodes::PENALTY_NODE),
        )?;
        self.primitive("unkern", REMOVE_ITEM, i32::from(crate::nodes::KERN_NODE))?;
        self.primitive("unskip", REMOVE_ITEM, i32::from(crate::nodes::GLUE_NODE))?;
        // §1108: \unhbox, \unhcopy, \unvbox, \unvcopy.
        self.primitive("unhbox", UN_HBOX, crate::control::BOX_CODE)?;
        self.primitive("unhcopy", UN_HBOX, crate::control::COPY_CODE)?;
        self.primitive("unvbox", UN_VBOX, crate::control::BOX_CODE)?;
        self.primitive("unvcopy", UN_VBOX, crate::control::COPY_CODE)?;
        // §983: \pagegoal .. \pagedepth.
        self.primitive("pagegoal", SET_PAGE_DIMEN, 0)?;
        self.primitive("pagetotal", SET_PAGE_DIMEN, 1)?;
        self.primitive("pagestretch", SET_PAGE_DIMEN, 2)?;
        self.primitive("pagefilstretch", SET_PAGE_DIMEN, 3)?;
        self.primitive("pagefillstretch", SET_PAGE_DIMEN, 4)?;
        self.primitive("pagefilllstretch", SET_PAGE_DIMEN, 5)?;
        self.primitive("pageshrink", SET_PAGE_DIMEN, 6)?;
        self.primitive("pagedepth", SET_PAGE_DIMEN, 7)?;
        // §1250: \hyphenation, \patterns.
        self.primitive("hyphenation", HYPH_DATA, 0)?;
        self.primitive("patterns", HYPH_DATA, 1)?;
        // §780: \span, \cr, \crcr, and the frozen \cr / \endtemplate.
        self.primitive("span", TAB_MARK, crate::align::SPAN_CODE)?;
        let p = self.primitive("cr", CAR_RET, crate::align::CR_CODE)?;
        let s = self.strings.intern("cr")?;
        self.eqtb.set_text(lay.frozen_cr, s);
        *self.eqtb.word_mut(lay.frozen_cr) = self.eqtb.word(p);
        self.primitive("crcr", CAR_RET, crate::align::CR_CR_CODE)?;
        let s = self.strings.intern("endtemplate")?;
        self.eqtb.set_text(lay.frozen_end_template, s);
        self.eqtb.set_text(lay.frozen_endv, s);
        self.eqtb.set_eq_type(lay.frozen_endv, ENDV);
        let nl = self.mem.null_list();
        self.eqtb.set_equiv(lay.frozen_endv, nl);
        self.eqtb.set_eq_level(lay.frozen_endv, LEVEL_ONE);
        *self.eqtb.word_mut(lay.frozen_end_template) = self.eqtb.word(lay.frozen_endv);
        self.eqtb.set_eq_type(lay.frozen_end_template, END_TEMPLATE);
        // §790: the constant token list \endtemplate (omit_template).
        let ot = self.mem.omit_template();
        self.mem
            .set_info(ot, CS_TOKEN_FLAG + lay.frozen_end_template);
        // §1141: \eqno, \leqno.
        self.primitive("eqno", EQ_NO, 0)?;
        self.primitive("leqno", EQ_NO, 1)?;
        // §1156: \mathord .. \overline.
        {
            use crate::math::*;
            self.primitive("mathord", MATH_COMP, i32::from(ORD_NOAD))?;
            self.primitive("mathop", MATH_COMP, i32::from(OP_NOAD))?;
            self.primitive("mathbin", MATH_COMP, i32::from(BIN_NOAD))?;
            self.primitive("mathrel", MATH_COMP, i32::from(REL_NOAD))?;
            self.primitive("mathopen", MATH_COMP, i32::from(OPEN_NOAD))?;
            self.primitive("mathclose", MATH_COMP, i32::from(CLOSE_NOAD))?;
            self.primitive("mathpunct", MATH_COMP, i32::from(PUNCT_NOAD))?;
            self.primitive("mathinner", MATH_COMP, i32::from(INNER_NOAD))?;
            self.primitive("underline", MATH_COMP, i32::from(UNDER_NOAD))?;
            self.primitive("overline", MATH_COMP, i32::from(OVER_NOAD))?;
            self.primitive("displaylimits", LIMIT_SWITCH, i32::from(crate::mem::NORMAL))?;
            self.primitive("limits", LIMIT_SWITCH, i32::from(LIMITS))?;
            self.primitive("nolimits", LIMIT_SWITCH, i32::from(NO_LIMITS))?;
            // §1169: the style primitives.
            self.primitive("displaystyle", MATH_STYLE, i32::from(DISPLAY_STYLE))?;
            self.primitive("textstyle", MATH_STYLE, i32::from(TEXT_STYLE))?;
            self.primitive("scriptstyle", MATH_STYLE, i32::from(SCRIPT_STYLE))?;
            self.primitive(
                "scriptscriptstyle",
                MATH_STYLE,
                i32::from(SCRIPT_SCRIPT_STYLE),
            )?;
            // §1178: generalized fractions.
            use crate::mathlist::*;
            self.primitive("above", ABOVE, ABOVE_CODE)?;
            self.primitive("over", ABOVE, OVER_CODE)?;
            self.primitive("atop", ABOVE, ATOP_CODE)?;
            self.primitive("abovewithdelims", ABOVE, DELIMITED_CODE + ABOVE_CODE)?;
            self.primitive("overwithdelims", ABOVE, DELIMITED_CODE + OVER_CODE)?;
            self.primitive("atopwithdelims", ABOVE, DELIMITED_CODE + ATOP_CODE)?;
            // §1188: \left, \right (with the frozen \right).
            self.primitive("left", LEFT_RIGHT, i32::from(LEFT_NOAD))?;
            let p = self.primitive("right", LEFT_RIGHT, i32::from(RIGHT_NOAD))?;
            let s = self.strings.intern("right")?;
            self.eqtb.set_text(lay.frozen_right, s);
            *self.eqtb.word_mut(lay.frozen_right) = self.eqtb.word(p);
        }
        // §1344: the extensions.
        self.primitive("openout", EXTENSION, i32::from(crate::par::OPEN_NODE))?;
        let p = self.primitive("write", EXTENSION, i32::from(crate::par::WRITE_NODE))?;
        self.write_loc = p;
        self.primitive("closeout", EXTENSION, i32::from(crate::par::CLOSE_NODE))?;
        self.primitive("special", EXTENSION, i32::from(crate::par::SPECIAL_NODE))?;
        self.primitive("immediate", EXTENSION, crate::ext::IMMEDIATE_CODE)?;
        self.primitive("setlanguage", EXTENSION, crate::ext::SET_LANGUAGE_CODE)?;
        // §1369: the frozen \endwrite, an "outer macro" that ends \write
        // expansion (its appearance outside write_out is an error).
        let s = self.strings.intern("endwrite")?;
        self.eqtb.set_text(lay.end_write, s);
        self.eqtb.set_eq_level(lay.end_write, LEVEL_ONE);
        self.eqtb.set_eq_type(lay.end_write, OUTER_CALL);
        self.eqtb.set_equiv(lay.end_write, NULL);
        Ok(())
    }
}
