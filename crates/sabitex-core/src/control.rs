//! The chief executive: `main_control`.
//!
//! Ports tex.web Part 46 (§1029-§1054) — including the character/ligature/
//! kern inner loop — and the box-building subset of Part 47 (§1055-§1086):
//! `append_glue`, `append_kern`, `box_end`, `begin_box`, `scan_box`,
//! `package` and `handle_right_brace`. Paragraph building (`new_graf`),
//! alignments, math and the page builder arrive in M3/M4; their dispatch
//! arms report a milestone error instead.

use crate::cmds::*;
use crate::engine::{Engine, HMODE, MMODE, VMODE};
use crate::eqtb::{
    ADJUSTED_HBOX_GROUP, BOTTOM_LEVEL, HBOX_GROUP, INSERT_GROUP, OUTPUT_GROUP, SEMI_SIMPLE_GROUP,
    SIMPLE_GROUP, VBOX_GROUP, VTOP_GROUP,
};
use crate::error::TexResult;
use crate::fonts::{FontMem, LIG_TAG, NON_ADDRESS, NON_CHAR, STOP_FLAG};
use crate::input::NEW_LINE;
use crate::nest::IGNORE_DEPTH;
use crate::nodes::*;
use crate::scan::{GLUE_VAL, MU_VAL};
use crate::tokens::CS_TOKEN_FLAG;
use crate::types::{Pointer, NULL};

// §1071: box context codes.
// §1071 (+ etex.ch): box context codes, widened for 32768 registers.
pub const BOX_FLAG: i32 = 0o10000000000; // 2^30
pub const GLOBAL_BOX_FLAG: i32 = BOX_FLAG + 0o100000;
pub const SHIP_OUT_FLAG: i32 = BOX_FLAG + 0o200000;
pub const LEADER_FLAG: i32 = SHIP_OUT_FLAG + 1;
// §1071: make_box chr codes.
pub const BOX_CODE: i32 = 0;
pub const COPY_CODE: i32 = 1;
pub const LAST_BOX_CODE: i32 = 2;
pub const VSPLIT_CODE: i32 = 3;
pub const VTOP_CODE: i32 = 4;
// §1058: skip codes.
pub const FIL_CODE: i32 = 0;
pub const FILL_CODE: i32 = 1;
pub const SS_CODE: i32 = 2;
pub const FIL_NEG_CODE: i32 = 3;
pub const SKIP_CODE: i32 = 4;
pub const MSKIP_CODE: i32 = 5;

/// Where the main-loop spaghetti goes next (tex.web's labels).
#[derive(Copy, Clone, PartialEq)]
enum L {
    Wrapup,
    Move,
    Move1,
    Move2,
    Lookahead,
    Lookahead1,
    LigLoop,
    LigLoop1,
    LigLoop2,
    MoveLig,
}

/// What the dispatcher should do after an action.
enum Next {
    BigSwitch,
    Reswitch,
    Done,
}

impl Engine {
    /// Starts reading from a "file" provided via `TexFs` under `name`, then
    /// runs the main loop until `\end` or end of input.
    pub fn run_file(&mut self, name: &str) -> TexResult<()> {
        // §1337: the first line arrives through the terminal buffer and is
        // tokenized by main_control, so buffer offsets (`first`, and with
        // them `max_buf_stack`) match tex.web exactly.
        self.first_input_line = format!("\\input {name}");
        let line: Vec<i32> = self.first_input_line.chars().map(|c| c as i32).collect();
        self.inp.first = 1;
        self.copy_line_to_buffer(&line)?;
        self.inp.cur.state = NEW_LINE;
        self.inp.cur.start = 1;
        self.inp.cur.index = 0;
        self.inp.cur.name = 0;
        self.inp.line = 0;
        self.inp.cur.loc = 1;
        self.inp.cur.limit = self.inp.last;
        if self.end_line_char_inactive() {
            self.inp.cur.limit -= 1;
        } else {
            let elc = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
            self.inp.buffer[self.inp.cur.limit as usize] = elc;
        }
        self.inp.first = self.inp.cur.limit + 1;
        self.finish_run()
    }

    /// Runs a job whose first line is "typed" at the `**` prompt
    /// (§37 + §1332 + §1337): prints the terminal banner and prompt, reads
    /// the line from the [`crate::io::Terminal`], skips a `&format` spec
    /// (the host has already called `load_fmt`), and — when the rest of the
    /// line does not start with an escape character — opens that file
    /// directly, *before* `main_control` runs `\everyjob`.
    pub fn run_terminal_job(&mut self) -> TexResult<()> {
        // Arenas are about to be written: format loads may no longer
        // assume all-zero memory (A14 zero-fill skip).
        self.pristine = false;
        // §1332: the banner, to the terminal only (the log echoes it later).
        self.term.write_str(crate::BANNER);
        self.term.write_str(" (INITEX)\n");
        self.term.write_str("**");
        let Some(typed) = self.term.read_line() else {
            return Err(crate::error::TexInterrupt::FatalError(
                "End of file on the terminal!",
            ));
        };
        // A real terminal displays the user's typing; emulate that so the
        // captured terminal output matches a live session (cf. trip.fot).
        // With redirected input (cf. etrip) nothing is echoed.
        if self.terminal_echo {
            self.term.write_str(&typed);
            self.term.write_str("\n");
        }
        let line: Vec<i32> = typed.chars().map(|c| c as i32).collect();
        self.inp.first = 1;
        self.copy_line_to_buffer(&line)?;
        self.inp.cur.state = NEW_LINE;
        self.inp.cur.start = 1;
        self.inp.cur.index = 0;
        self.inp.cur.name = 0;
        self.inp.line = 0;
        // §37: loc skips leading blanks.
        let mut loc = 1;
        while loc < self.inp.last && self.inp.buffer[loc as usize] == ' ' as i32 {
            loc += 1;
        }
        // §1337: skip over a "&format" specification (already loaded).
        if loc < self.inp.last && self.inp.buffer[loc as usize] == '&' as i32 {
            while loc < self.inp.last && self.inp.buffer[loc as usize] != ' ' as i32 {
                loc += 1;
            }
        }
        // etex.ch §1337 @<Enable \eTeX, if requested@>: "*" in a virgin
        // INITEX enters extended mode and generates the new primitives.
        if loc < self.inp.last
            && self.inp.buffer[loc as usize] == '*' as i32
            && self.format_ident.is_empty()
        {
            self.generate_etex_primitives()?;
            loc += 1;
            self.etex_mode = true;
            self.init_etex_mode_vars();
        }
        if self.etex_ex() {
            self.term.write_str("entering extended mode\n");
        }
        // §534 echoes the whole first line after `**` in the transcript.
        self.first_input_line = self.inp.buffer[1..self.inp.last as usize]
            .iter()
            .map(|&c| char::from_u32(c as u32).unwrap_or('?'))
            .collect();
        self.inp.cur.loc = loc;
        self.inp.cur.limit = self.inp.last;
        if self.end_line_char_inactive() {
            self.inp.cur.limit -= 1;
        } else {
            let elc = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
            self.inp.buffer[self.inp.cur.limit as usize] = elc;
        }
        self.inp.first = self.inp.cur.limit + 1;
        // §1337: an implied \input.
        if self.inp.cur.loc < self.inp.cur.limit {
            let c = self.inp.buffer[self.inp.cur.loc as usize];
            if self.eqtb.cat_code(c) != i32::from(crate::cmds::ESCAPE) {
                self.start_input()?;
            }
        }
        self.finish_run()
    }

    /// `main_control` plus the end-of-job work shared by all entry points.
    fn finish_run(&mut self) -> TexResult<()> {
        self.main_control()?;
        // §1335: final_cleanup.
        while self.inp.input_ptr > 0 {
            if self.inp.cur.state == crate::input::TOKEN_LIST {
                self.end_token_list()?;
            } else {
                self.end_file_reading();
            }
        }
        while self.inp.open_parens > 0 {
            self.print_chars(" )");
            self.inp.open_parens -= 1;
        }
        if self.save.cur_level > crate::eqtb::LEVEL_ONE {
            self.print_nl_chars("(");
            self.print_esc_str("end occurred ");
            self.print_chars("inside a group at level ");
            let l = i32::from(self.save.cur_level - crate::eqtb::LEVEL_ONE);
            self.print_int(l);
            self.print_char(')' as i32);
        }
        while self.cond_ptr != NULL {
            self.print_nl_chars("(");
            self.print_esc_str("end occurred ");
            self.print_chars("when ");
            let ci = self.cur_if;
            self.print_cmd_chr(IF_TEST, i32::from(ci));
            if self.if_line != 0 {
                self.print_chars(" on line ");
                let l = self.if_line;
                self.print_int(l);
            }
            self.print_chars(" was incomplete)");
            self.if_line = self.mem.word(self.cond_ptr + 1).int();
            self.cur_if = self.mem.subtype(self.cond_ptr);
            let temp = self.cond_ptr;
            self.cond_ptr = self.mem.link(temp);
            self.mem.free_node(temp, crate::cond::IF_NODE_SIZE);
        }
        // §1333: close the \write streams.
        for k in 0..16 {
            if self.write_open[k] {
                self.close_write_stream(k);
                self.write_open[k] = false;
            }
        }
        // §1335: point the user to the transcript if it has more details.
        if self.history != crate::error::History::Spotless
            && (self.history == crate::error::History::WarningIssued
                || self.interaction < crate::input::ERROR_STOP_MODE)
            && self.prn.selector == crate::print::TERM_AND_LOG
        {
            self.prn.selector = crate::print::TERM_ONLY;
            self.print_nl_chars("(see the transcript file for additional information)");
            self.prn.selector = crate::print::TERM_AND_LOG;
        }
        // etex.ch §1335: flush the saved discard lists.
        for c in (LAST_BOX_CODE as usize)..=(VSPLIT_CODE as usize) {
            let d = self.disc_ptr[c];
            self.flush_node_list(d);
            self.disc_ptr[c] = NULL;
        }
        if self.dump_requested {
            // etex.ch §1335: destroy the sparse mark classes before \dump.
            if self.sa_root[crate::sa::MARK_VAL as usize] != NULL {
                let m = self.sa_root[crate::sa::MARK_VAL as usize];
                if self.do_marks(crate::sa::DESTROY_MARKS, 0, m) {
                    self.sa_root[crate::sa::MARK_VAL as usize] = NULL;
                }
            }
            // §1335 (INITEX): store the format file.
            let fmt = self.store_fmt()?;
            let name = format!("{}.fmt", self.job_name.as_deref().unwrap_or("texput"));
            self.fs.write_file(&name, crate::io::OutKind::Fmt, &fmt);
        }
        // §1334: output statistics about this job (to the log only).
        if self.eqtb.int_par(crate::eqtb::TRACING_STATS_CODE) > 0 && self.log_opened {
            let old_setting = self.prn.selector;
            self.prn.selector = crate::print::LOG_ONLY;
            // §1334 writes through wlog/wlog_ln, which bypass file_offset;
            // restoring it afterwards reproduces the stray blank line that
            // the next print_nl leaves in real transcripts.
            let saved_file_offset = self.prn.file_offset;
            // §1334 wlog_ln(' '): a space lands on the still-open line.
            self.print_chars(" ");
            self.print_ln();
            self.print_nl_chars("Here is how much of TeX's memory you used:");
            let s = self.strings.str_ptr() - self.strings.init_str_ptr;
            self.print_nl_chars(" ");
            self.print_int(s as i32);
            self.print_chars(" string");
            if s != 1 {
                self.print_char('s' as i32);
            }
            self.print_chars(" out of ");
            let ms = (self.sizes.max_strings - self.strings.init_str_ptr) as i32;
            self.print_int(ms);
            self.print_nl_chars(" ");
            let pc = (self.strings.pool_ptr() - self.strings.init_pool_ptr) as i32;
            self.print_int(pc);
            self.print_chars(" string characters out of ");
            let ps = (self.sizes.pool_size - self.strings.init_pool_ptr) as i32;
            self.print_int(ps);
            self.print_nl_chars(" ");
            let words =
                self.mem.lo_mem_max - self.mem.mem_bot + self.mem.mem_end - self.mem.hi_mem_min + 2;
            self.print_int(words);
            self.print_chars(" words of memory out of ");
            let total = self.mem.mem_end + 1 - self.mem.mem_bot;
            self.print_int(total);
            self.print_nl_chars(" ");
            let cs = self.eqtb.cs_count;
            self.print_int(cs);
            self.print_chars(" multiletter control sequences out of ");
            let hs = self.sizes.hash_size;
            self.print_int(hs);
            self.print_nl_chars(" ");
            let fm = self.fonts.fmem_ptr;
            self.print_int(fm);
            self.print_chars(" words of font info for ");
            let fp = self.fonts.font_ptr;
            self.print_int(fp);
            self.print_chars(" font");
            if fp != 1 {
                self.print_char('s' as i32);
            }
            self.print_chars(", out of ");
            let fms = self.sizes.font_mem_size as i32;
            self.print_int(fms);
            self.print_chars(" for ");
            let fx = self.sizes.font_max;
            self.print_int(fx);
            self.print_nl_chars(" ");
            let hc = self.hy.hyph_count;
            self.print_int(hc);
            self.print_chars(" hyphenation exception");
            if hc != 1 {
                self.print_char('s' as i32);
            }
            self.print_chars(" out of ");
            let hz = self.sizes.hyph_size;
            self.print_int(hz);
            self.print_nl_chars(" ");
            let mis = self.inp.max_in_stack as i32;
            self.print_int(mis);
            self.print_chars("i,");
            let mns = self.nest.max_nest_stack as i32;
            self.print_int(mns);
            self.print_chars("n,");
            let mps = self.inp.max_param_stack as i32;
            self.print_int(mps);
            self.print_chars("p,");
            let mbs = self.inp.max_buf_stack + 1;
            self.print_int(mbs);
            self.print_chars("b,");
            let mss = (self.save.max_save_stack + 6) as i32;
            self.print_int(mss);
            self.print_chars("s stack positions out of ");
            let ss = self.sizes.stack_size as i32;
            self.print_int(ss);
            self.print_chars("i,");
            let ns = self.sizes.nest_size as i32;
            self.print_int(ns);
            self.print_chars("n,");
            let psz = self.sizes.param_size as i32;
            self.print_int(psz);
            self.print_chars("p,");
            let bs = self.inp.buf_size;
            self.print_int(bs);
            self.print_chars("b,");
            let svs = self.sizes.save_size as i32;
            self.print_int(svs);
            self.print_char('s' as i32);
            self.print_ln();
            self.prn.file_offset = saved_file_offset;
            self.prn.selector = old_setting;
        }
        self.finish_dvi()?;
        // §1333: close the log file.
        if self.log_opened {
            let old = self.prn.selector;
            self.prn.selector = crate::print::LOG_ONLY;
            self.print_ln(); // wlog_cr
            self.prn.selector = old - 2;
            if self.prn.selector == crate::print::TERM_ONLY {
                self.print_nl_chars("Transcript written on ");
                let log_name = format!("{}.log", self.job_name.as_deref().unwrap_or("texput"));
                self.print_chars(&log_name);
                self.print_char('.' as i32);
                self.print_ln();
            }
        }
        Ok(())
    }

    /// `start_input` body, given an already-resolved name.
    pub fn start_input_named(&mut self, name: &str) -> TexResult<()> {
        self.cur_name = name.to_string();
        self.start_input_resolved()
    }

    /// `main_control` (§1030).
    pub fn main_control(&mut self) -> TexResult<()> {
        let ej = self
            .eqtb
            .equiv(self.eqtb.lay.local_base + crate::eqtb::EVERY_JOB_OFFSET);
        if ej != NULL {
            self.begin_token_list(ej, 12)?; // every_job_text (§307)
        }
        'big_switch: loop {
            self.get_x_token()?;
            'reswitch: loop {
                // §1031: give diagnostic information, if requested.
                if self.eqtb.int_par(crate::eqtb::TRACING_COMMANDS_CODE) > 0 {
                    self.show_cur_cmd_chr();
                }
                match self.dispatch()? {
                    Next::BigSwitch => continue 'big_switch,
                    Next::Reswitch => continue 'reswitch,
                    Next::Done => return Ok(()),
                }
            }
        }
    }

    /// The big case statement on `abs(mode) + cur_cmd` (§1030, §1045-§1056).
    fn dispatch(&mut self) -> TexResult<Next> {
        let m = self.mode().abs();
        let cmd = self.cur_cmd;
        // The hmode inner loop entries.
        if m == HMODE {
            match cmd {
                LETTER | OTHER_CHAR | CHAR_GIVEN => {
                    // pTeX: Japanese characters build two-node pairs.
                    if self.is_japanese_char(self.cur_chr) {
                        self.append_kanji()?;
                        return Ok(Next::BigSwitch);
                    }
                    // xetex.web §24400: native fonts collect words.
                    if self.is_native_font(self.eqtb.cur_font()) {
                        self.collect_native()?;
                        return Ok(Next::Reswitch);
                    }
                    return self.main_loop();
                }
                CHAR_NUM => {
                    self.scan_char_num()?;
                    self.cur_chr = self.cur_val;
                    if self.is_native_font(self.eqtb.cur_font()) {
                        self.collect_native()?;
                        return Ok(Next::Reswitch);
                    }
                    return self.main_loop();
                }
                NO_BOUNDARY => {
                    self.get_x_token()?;
                    if matches!(self.cur_cmd, LETTER | OTHER_CHAR | CHAR_GIVEN | CHAR_NUM) {
                        self.cancel_boundary = true;
                    }
                    return Ok(Next::Reswitch);
                }
                SPACER => {
                    if self.space_factor() == 1000 {
                        return self.append_normal_space();
                    } else {
                        self.app_space()?;
                        return Ok(Next::BigSwitch);
                    }
                }
                EX_SPACE => return self.append_normal_space(),
                _ => {}
            }
        }
        if m == MMODE && cmd == EX_SPACE {
            return self.append_normal_space();
        }
        // Mode-independent and structural commands.
        match cmd {
            RELAX => Ok(Next::BigSwitch),
            SPACER if m == VMODE => Ok(Next::BigSwitch),
            IGNORE_SPACES => {
                self.get_next_nonblank_noncall()?;
                Ok(Next::Reswitch)
            }
            STOP if m == VMODE => {
                if self.its_all_over()? {
                    Ok(Next::Done)
                } else {
                    Ok(Next::BigSwitch)
                }
            }
            LEFT_BRACE if m != MMODE => {
                self.new_save_level(SIMPLE_GROUP)?;
                Ok(Next::BigSwitch)
            }
            BEGIN_GROUP => {
                self.new_save_level(SEMI_SIMPLE_GROUP)?;
                Ok(Next::BigSwitch)
            }
            END_GROUP => {
                if self.save.cur_group == SEMI_SIMPLE_GROUP {
                    self.unsave()?;
                } else {
                    self.off_save()?;
                }
                Ok(Next::BigSwitch)
            }
            RIGHT_BRACE => {
                self.handle_right_brace()?;
                Ok(Next::BigSwitch)
            }
            HMOVE if m == VMODE => self.move_box(),
            VMOVE if m != VMODE => self.move_box(),
            LEADER_SHIP => {
                let chr = self.cur_chr;
                self.scan_box(LEADER_FLAG - i32::from(A_LEADERS) + chr)?;
                Ok(Next::BigSwitch)
            }
            MAKE_BOX => {
                self.begin_box(0)?;
                Ok(Next::BigSwitch)
            }
            HSKIP if m != VMODE => {
                self.append_glue()?;
                Ok(Next::BigSwitch)
            }
            VSKIP if m == VMODE => {
                self.append_glue()?;
                Ok(Next::BigSwitch)
            }
            KERN => {
                self.append_kern()?;
                Ok(Next::BigSwitch)
            }
            BREAK_PENALTY => {
                // §1103.
                self.scan_int()?;
                let v = self.cur_val;
                let p = self.new_penalty(v)?;
                self.tail_append(p);
                if self.mode() == VMODE {
                    self.build_page()?;
                }
                Ok(Next::BigSwitch)
            }
            HRULE if m == VMODE => {
                // §1056: append a rule and forget prev_depth.
                let r = self.scan_rule_spec()?;
                self.tail_append(r);
                self.set_prev_depth(IGNORE_DEPTH);
                Ok(Next::BigSwitch)
            }
            VRULE if m == HMODE || m == MMODE => {
                // §1056: a rule forgets the space factor.
                let r = self.scan_rule_spec()?;
                self.tail_append(r);
                if m == HMODE {
                    self.set_space_factor(1000);
                }
                Ok(Next::BigSwitch)
            }
            PAR_END if m == VMODE => {
                // §1070.
                self.normal_paragraph()?;
                if self.mode() > 0 {
                    self.build_page()?;
                }
                Ok(Next::BigSwitch)
            }
            PAR_END if m == HMODE => {
                // §1095.
                if self.inp.align_state < 0 {
                    self.off_save()?; // this tries to recover from an alignment that didn't end properly
                }
                self.end_graf()?; // this takes us to the enclosing mode, if mode > 0
                if self.mode() == VMODE {
                    self.build_page()?;
                }
                Ok(Next::BigSwitch)
            }
            // §1090: a paragraph begins.
            LETTER | OTHER_CHAR | CHAR_NUM | CHAR_GIVEN | MATH_SHIFT | UN_HBOX | VRULE | ACCENT
            | DISCRETIONARY | HSKIP | VALIGN | EX_SPACE | NO_BOUNDARY
                if m == VMODE =>
            {
                self.back_input()?;
                self.new_graf(true)?;
                Ok(Next::BigSwitch)
            }
            START_PAR if m == VMODE => {
                let indent = self.cur_chr > 0;
                self.new_graf(indent)?;
                Ok(Next::BigSwitch)
            }
            START_PAR if m == HMODE || m == MMODE => {
                self.indent_in_hmode()?;
                Ok(Next::BigSwitch)
            }
            // §1092: vertical commands in horizontal mode.
            STOP | VSKIP | HRULE | UN_VBOX | HALIGN if m == HMODE => {
                self.head_for_vmode()?;
                Ok(Next::BigSwitch)
            }
            // §1107: \unpenalty, \unkern, \unskip.
            REMOVE_ITEM => {
                self.delete_last()?;
                Ok(Next::BigSwitch)
            }
            // §1108: \unhbox, \unhcopy, \unvbox, \unvcopy.
            UN_VBOX if m == VMODE => {
                self.unpackage()?;
                Ok(Next::BigSwitch)
            }
            UN_HBOX if m == HMODE || m == MMODE => {
                self.unpackage()?;
                Ok(Next::BigSwitch)
            }
            // §1114: \discretionary, \- (hmode/mmode).
            DISCRETIONARY if m == HMODE || m == MMODE => {
                self.append_discretionary()?;
                Ok(Next::BigSwitch)
            }
            // §1122: \accent (hmode; in vmode it starts a paragraph above).
            ACCENT if m == HMODE => {
                self.make_accent()?;
                Ok(Next::BigSwitch)
            }
            // §1112: \/.
            ITAL_CORR if m == HMODE => {
                self.append_italic_correction()?;
                Ok(Next::BigSwitch)
            }
            ITAL_CORR if m == MMODE => {
                // §1113: in math, \/ appends a zero kern.
                let k = self.new_kern(0)?;
                self.tail_append(k);
                Ok(Next::BigSwitch)
            }
            // §1097: \insert, \vadjust, \mark.
            INSERT => {
                self.begin_insert_or_adjust()?;
                Ok(Next::BigSwitch)
            }
            VADJUST if m != VMODE => {
                self.begin_insert_or_adjust()?;
                Ok(Next::BigSwitch)
            }
            MARK => {
                self.make_mark()?;
                Ok(Next::BigSwitch)
            }
            // §1347: the extensions (\openout, \write, ...).
            EXTENSION => {
                self.do_extension()?;
                Ok(Next::BigSwitch)
            }
            // §1137: $ in horizontal mode begins a formula.
            MATH_SHIFT if m == HMODE => {
                self.init_math()?;
                Ok(Next::BigSwitch)
            }
            // §1193: the only way out of math mode.
            MATH_SHIFT if m == MMODE => {
                if self.save.cur_group == crate::eqtb::MATH_SHIFT_GROUP {
                    self.after_math()?;
                } else {
                    self.off_save()?;
                }
                Ok(Next::BigSwitch)
            }
            SPACER | NO_BOUNDARY if m == MMODE => Ok(Next::BigSwitch), // §1046
            // §1150, §1154-§1180: math-mode constructions.
            LETTER | OTHER_CHAR | CHAR_GIVEN if m == MMODE => {
                let mc = self.eqtb.math_code(self.cur_chr);
                self.set_math_char(mc)?;
                Ok(Next::BigSwitch)
            }
            CHAR_NUM if m == MMODE => {
                self.scan_char_num()?;
                self.cur_chr = self.cur_val;
                let mc = self.eqtb.math_code(self.cur_chr);
                self.set_math_char(mc)?;
                Ok(Next::BigSwitch)
            }
            MATH_CHAR_NUM if m == MMODE => {
                // xetex.web: chr 2 = \Umathchar, 1 = \Umathcharnum,
                // 0 = classic \mathchar (converted to extended form).
                let v = if self.cur_chr == 2 {
                    self.scan_math_class_int()?;
                    let mut v = crate::xemath::set_class_field(self.cur_val);
                    self.scan_math_fam_int()?;
                    v += crate::xemath::set_family_field(self.cur_val);
                    self.scan_char_num()?;
                    v + self.cur_val
                } else if self.cur_chr == 1 {
                    self.scan_xetex_math_char_int()?;
                    self.cur_val
                } else {
                    self.scan_fifteen_bit_int()?;
                    crate::xemath::from_classic(self.cur_val)
                };
                self.set_math_char(v)?;
                Ok(Next::BigSwitch)
            }
            MATH_GIVEN if m == MMODE => {
                let c = crate::xemath::from_classic(self.cur_chr);
                self.set_math_char(c)?;
                Ok(Next::BigSwitch)
            }
            XETEX_MATH_GIVEN if m == MMODE => {
                let c = self.cur_chr;
                self.set_math_char(c)?;
                Ok(Next::BigSwitch)
            }
            DELIM_NUM if m == MMODE => {
                let v = if self.cur_chr == 1 {
                    // \Udelimiter <class> <fam> <usv>.
                    self.scan_math_class_int()?;
                    let mut v = crate::xemath::set_class_field(self.cur_val);
                    self.scan_math_fam_int()?;
                    v += crate::xemath::set_family_field(self.cur_val);
                    self.scan_char_num()?;
                    v + self.cur_val
                } else {
                    self.scan_twenty_seven_bit_int()?;
                    crate::xemath::from_classic(self.cur_val / 0o10000)
                };
                self.set_math_char(v)?;
                Ok(Next::BigSwitch)
            }
            LEFT_BRACE if m == MMODE => {
                // §1150: a subformula in braces.
                let n = self.new_noad()?;
                self.tail_append(n);
                self.back_input()?;
                let t = self.nest.cur.tail;
                self.scan_math(t + 1)?;
                Ok(Next::BigSwitch)
            }
            MATH_COMP if m == MMODE => {
                // §1158.
                let n = self.new_noad()?;
                self.tail_append(n);
                let c = self.cur_chr as u16;
                self.mem.set_node_type(n, c);
                self.scan_math(n + 1)?;
                Ok(Next::BigSwitch)
            }
            LIMIT_SWITCH if m == MMODE => {
                self.math_limit_switch()?;
                Ok(Next::BigSwitch)
            }
            RADICAL if m == MMODE => {
                self.math_radical()?;
                Ok(Next::BigSwitch)
            }
            ACCENT | MATH_ACCENT if m == MMODE => {
                self.math_ac()?;
                Ok(Next::BigSwitch)
            }
            VCENTER if m == MMODE => {
                self.begin_vcenter()?;
                Ok(Next::BigSwitch)
            }
            MATH_STYLE if m == MMODE => {
                let c = self.cur_chr as u16;
                let s = self.new_style(c)?;
                self.tail_append(s);
                Ok(Next::BigSwitch)
            }
            NON_SCRIPT if m == MMODE => {
                let zg = self.mem.zero_glue();
                let g = self.new_glue(zg)?;
                self.tail_append(g);
                let t = self.nest.cur.tail;
                self.mem.set_subtype(t, COND_MATH_GLUE);
                Ok(Next::BigSwitch)
            }
            MATH_CHOICE if m == MMODE => {
                self.append_choices()?;
                Ok(Next::BigSwitch)
            }
            SUB_MARK | SUP_MARK if m == MMODE => {
                self.sub_sup()?;
                Ok(Next::BigSwitch)
            }
            ABOVE if m == MMODE => {
                self.math_fraction()?;
                Ok(Next::BigSwitch)
            }
            LEFT_RIGHT if m == MMODE => {
                self.math_left_right()?;
                Ok(Next::BigSwitch)
            }
            EQ_NO if m == MMODE => {
                // §1140.
                if self.mode() <= 0 {
                    self.report_illegal_case()?;
                } else if self.save.cur_group == crate::eqtb::MATH_SHIFT_GROUP {
                    self.start_eq_no()?;
                } else {
                    self.off_save()?;
                }
                Ok(Next::BigSwitch)
            }
            MKERN if m == MMODE => {
                self.append_kern()?;
                Ok(Next::BigSwitch)
            }
            MSKIP if m == MMODE => {
                self.append_glue()?;
                Ok(Next::BigSwitch)
            }
            // §1130: alignment.
            HALIGN if m == VMODE => {
                self.init_align()?;
                Ok(Next::BigSwitch)
            }
            VALIGN if m == HMODE => {
                // etex.ch: chr > 0 are the text-direction primitives.
                if self.cur_chr > 0 {
                    let (c, ch) = (self.cur_cmd, self.cur_chr);
                    if self.etex_enabled(self.texxet_en(), c, ch)? {
                        let n = self.new_math(0, ch as u16)?;
                        self.tail_append(n);
                    }
                } else {
                    self.init_align()?;
                }
                Ok(Next::BigSwitch)
            }
            HALIGN if m == MMODE => {
                if self.mode() <= 0 {
                    self.report_illegal_case()?;
                } else if self.save.cur_group == crate::eqtb::MATH_SHIFT_GROUP {
                    self.init_align()?;
                } else {
                    self.off_save()?;
                }
                Ok(Next::BigSwitch)
            }
            ENDV if m == VMODE || m == HMODE => {
                self.do_endv()?;
                Ok(Next::BigSwitch)
            }
            // §1126-§1129: alignment material out of context.
            TAB_MARK | CAR_RET => {
                self.align_error()?;
                Ok(Next::BigSwitch)
            }
            NO_ALIGN => {
                if std::env::var("SABITEX_DEBUG_SCAN").is_ok() {
                    eprintln!(
                        "DBG noalign: mode={} group={} align_state={}",
                        self.mode(),
                        self.save.cur_group,
                        self.inp.align_state
                    );
                }
                self.print_err("Misplaced ");
                self.print_esc_str("noalign");
                self.help(&[
                    "I expect to see \\noalign only after the \\cr of",
                    "an alignment. Proceed, and I'll ignore this case.",
                ]);
                self.error()?;
                Ok(Next::BigSwitch)
            }
            OMIT => {
                self.print_err("Misplaced ");
                self.print_esc_str("omit");
                self.help(&[
                    "I expect to see \\omit only after tab marks or the \\cr of",
                    "an alignment. Proceed, and I'll ignore this case.",
                ]);
                self.error()?;
                Ok(Next::BigSwitch)
            }
            // §1046: math-only commands outside math, or vice versa.
            SUP_MARK | SUB_MARK | MATH_CHAR_NUM | MATH_GIVEN | MATH_COMP | DELIM_NUM
            | LEFT_RIGHT | ABOVE | RADICAL | MATH_STYLE | MATH_CHOICE | VCENTER | NON_SCRIPT
            | LIMIT_SWITCH | MATH_ACCENT | MKERN | MSKIP => {
                self.insert_dollar_sign()?;
                Ok(Next::BigSwitch)
            }
            // §1048 + §1141: forbidden cases (report_illegal_case).
            EQ_NO => {
                self.report_illegal_case()?;
                Ok(Next::BigSwitch)
            }
            STOP | VSKIP | HRULE | UN_VBOX | VALIGN | PAR_END if m == MMODE => {
                self.insert_dollar_sign()?;
                Ok(Next::BigSwitch)
            }
            END_CS_NAME => {
                self.print_err("Extra ");
                self.print_esc_str("endcsname");
                self.help(&["I'm ignoring this, since I wasn't doing a \\csname."]);
                self.error()?;
                Ok(Next::BigSwitch)
            }
            AFTER_ASSIGNMENT => {
                self.get_token()?;
                self.after_token = self.cur_tok;
                Ok(Next::BigSwitch)
            }
            AFTER_GROUP => {
                self.get_token()?;
                let t = self.cur_tok;
                self.save_for_after(t)?;
                Ok(Next::BigSwitch)
            }
            MESSAGE => {
                self.issue_message()?;
                Ok(Next::BigSwitch)
            }
            // §1274: \openin, \closein.
            IN_STREAM => {
                self.open_or_close_in()?;
                Ok(Next::BigSwitch)
            }
            CASE_SHIFT => {
                self.shift_case()?;
                Ok(Next::BigSwitch)
            }
            XRAY => {
                self.show_whatever()?;
                Ok(Next::BigSwitch)
            }
            crate::cmds::INHIBIT_GLUE => {
                // pTeX \inhibitglue: suppress the next JFM glue/kern.
                self.inhibit_glue_flag = true;
                Ok(Next::BigSwitch)
            }
            crate::cmds::KCHAR_NUM if self.mode().abs() == crate::engine::HMODE => {
                // pTeX \kchar<code>: a Japanese character by code
                // (horizontal modes, restricted included).
                self.scan_char_num()?;
                self.cur_chr = self.cur_val;
                self.append_kanji()?;
                Ok(Next::BigSwitch)
            }
            crate::cmds::KCHAR_NUM if self.mode().abs() == crate::engine::VMODE => {
                // vertical mode: starts a paragraph like any letter.
                self.back_input()?;
                self.new_graf(true)?;
                Ok(Next::BigSwitch)
            }
            crate::cmds::KCHAR_NUM => {
                self.report_illegal_case()?;
                Ok(Next::BigSwitch)
            }
            MAC_PARAM => {
                self.report_illegal_case()?;
                Ok(Next::BigSwitch)
            }
            LAST_ITEM | VMOVE | HMOVE => {
                // §1048 forbidden cases.
                self.report_illegal_case()?;
                Ok(Next::BigSwitch)
            }
            TOKS_REGISTER | ASSIGN_TOKS | ASSIGN_INT | ASSIGN_DIMEN | ASSIGN_GLUE
            | ASSIGN_MU_GLUE | ASSIGN_FONT_DIMEN | ASSIGN_FONT_INT | SET_AUX | SET_PREV_GRAF
            | SET_PAGE_DIMEN | SET_PAGE_INT | SET_BOX_DIMEN | SET_SHAPE | DEF_CODE | DEF_FAMILY
            | SET_FONT | DEF_FONT | REGISTER | ADVANCE | MULTIPLY | DIVIDE | PREFIX | LET
            | SHORTHAND_DEF | READ_TO_CS | DEF | SET_BOX | HYPH_DATA | SET_INTERACTION
            | SET_AUTO_SPACING | ASSIGN_KINSOKU | ASSIGN_INHIBIT_XSP | XETEX_DEF_CODE => {
                self.prefixed_command()?;
                Ok(Next::BigSwitch)
            }
            _ => {
                // Everything else is a later milestone (paragraphs M3,
                // math M4, alignment M4, insertions M3, ...).
                self.print_err("Not yet implemented in SabiTeX (M3+): `");
                let (c, ch) = (self.cur_cmd, self.cur_chr);
                self.print_cmd_chr(c, ch);
                self.print_chars("' in ");
                let md = self.mode();
                self.print_mode(md);
                self.error()?;
                Ok(Next::BigSwitch)
            }
        }
    }

    /// §1048/§1073: `\moveleft`, `\moveright`, `\raise`, `\lower`.
    fn move_box(&mut self) -> TexResult<Next> {
        let t = self.cur_chr;
        self.scan_normal_dimen()?;
        let v = self.cur_val;
        if t == 0 {
            self.scan_box(v)?;
        } else {
            self.scan_box(-v)?;
        }
        Ok(Next::BigSwitch)
    }

    /// `its_all_over` (§1054).
    fn its_all_over(&mut self) -> TexResult<bool> {
        if self.mode() <= 0 {
            // `privileged` (§1051).
            self.report_illegal_case()?;
            return Ok(false);
        }
        let ph = self.mem.page_head();
        if ph == self.page_tail && self.nest.cur.head == self.nest.cur.tail && self.dead_cycles == 0
        {
            if self.cur_chr == 1 {
                self.dump_requested = true; // \dump (INITEX, §1335)
            }
            return Ok(true);
        }
        self.back_input()?; // we will try to end again after ejecting residual material
        let b = self.new_null_box()?;
        self.tail_append(b);
        let hsize = self.eqtb.dimen_par(crate::eqtb::HSIZE_CODE);
        let t = self.nest.cur.tail;
        self.mem.set_width(t, hsize);
        let fill = self.mem.fill_glue();
        let g = self.new_glue(fill)?;
        self.tail_append(g);
        let p = self.new_penalty(-0o10000000000)?;
        self.tail_append(p);
        self.build_page()?; // append \hbox to \hsize{}, \vfill, \penalty-'10000000000
        Ok(false)
    }

    /// `normal_paragraph` (§1070).
    pub fn normal_paragraph(&mut self) -> TexResult<()> {
        let lay = self.eqtb.lay.clone();
        if self.eqtb.int_par(crate::eqtb::LOOSENESS_CODE) != 0 {
            self.eq_word_define(lay.int_base + crate::eqtb::LOOSENESS_CODE, 0)?;
        }
        if self.eqtb.dimen_par(crate::eqtb::HANG_INDENT_CODE) != 0 {
            self.eq_word_define(lay.dimen_base + crate::eqtb::HANG_INDENT_CODE, 0)?;
        }
        if self.eqtb.int_par(crate::eqtb::HANG_AFTER_CODE) != 1 {
            self.eq_word_define(lay.int_base + crate::eqtb::HANG_AFTER_CODE, 1)?;
        }
        if self.eqtb.equiv(lay.par_shape_loc) != NULL {
            self.eq_define(lay.par_shape_loc, SHAPE_REF, NULL)?;
        }
        // etex.ch §1070: \interlinepenalties is also a paragraph parameter.
        if self.eqtb.equiv(lay.etex_pen_base) != NULL {
            self.eq_define(lay.etex_pen_base, SHAPE_REF, NULL)?;
        }
        Ok(())
    }

    /// `handle_right_brace` (§1068) with the M2 group cases.
    fn handle_right_brace(&mut self) -> TexResult<()> {
        match self.save.cur_group {
            SIMPLE_GROUP => self.unsave(),
            BOTTOM_LEVEL => {
                self.print_err("Too many }'s");
                self.help(&[
                    "You've closed more groups than you opened.",
                    "Such booboos are generally harmless, so keep going.",
                ]);
                self.error()
            }
            SEMI_SIMPLE_GROUP | crate::eqtb::MATH_SHIFT_GROUP | crate::eqtb::MATH_LEFT_GROUP => {
                self.extra_right_brace()
            }
            HBOX_GROUP => self.package(0),
            ADJUSTED_HBOX_GROUP => {
                self.adjust_tail = self.mem.adjust_head();
                self.package(0)
            }
            VBOX_GROUP => {
                self.end_graf()?;
                self.package(0)
            }
            VTOP_GROUP => {
                self.end_graf()?;
                self.package(VTOP_CODE)
            }
            INSERT_GROUP => self.finish_insert_or_adjust(),
            OUTPUT_GROUP => self.resume_after_output(),
            crate::eqtb::MATH_GROUP => self.finish_math_group(),
            crate::eqtb::MATH_CHOICE_GROUP => self.build_choices(),
            crate::eqtb::VCENTER_GROUP => self.finish_vcenter(),
            crate::eqtb::ALIGN_GROUP => {
                // §1132: missing \cr inserted.
                self.back_input()?;
                self.cur_tok = CS_TOKEN_FLAG + self.eqtb.lay.frozen_cr;
                self.print_err("Missing ");
                self.print_esc_str("cr");
                self.print_chars(" inserted");
                self.help(&["I'm guessing that you meant to end an alignment here."]);
                // ins_error (§327).
                self.back_input()?;
                self.inp.cur.index = crate::input::INSERTED;
                self.error()
            }
            crate::eqtb::NO_ALIGN_GROUP => {
                // §1133.
                self.end_graf()?;
                self.unsave()?;
                self.align_peek()
            }
            crate::eqtb::DISC_GROUP => self.build_discretionary(),
            _ => self.confusion("rightbrace"),
        }
    }

    /// §1069: `extra_right_brace`.
    fn extra_right_brace(&mut self) -> TexResult<()> {
        self.print_err("Extra }, or forgotten ");
        match self.save.cur_group {
            SEMI_SIMPLE_GROUP => self.print_esc_str("endgroup"),
            crate::eqtb::MATH_SHIFT_GROUP => self.print_char('$' as i32),
            crate::eqtb::MATH_LEFT_GROUP => self.print_esc_str("right."),
            _ => {}
        }
        self.help(&[
            "I've deleted a group-closing symbol because it seems to be",
            "spurious, as in `$x}$'. But perhaps the } is legitimate and",
            "you forgot something else, as in `\\hbox{$x}'. In such cases",
            "the way to recover is to insert both the forgotten and the",
            "deleted material, e.g., by typing `I$}'.",
        ]);
        self.error()?;
        self.inp.align_state += 1;
        Ok(())
    }

    /// `align_error` (§1127): a misplaced tab mark or \cr.
    fn align_error(&mut self) -> TexResult<()> {
        if self.inp.align_state.abs() > 2 {
            // §1128: express consternation.
            self.print_err("Misplaced ");
            let (c, ch) = (self.cur_cmd, self.cur_chr);
            self.print_cmd_chr(c, ch);
            if self.cur_tok == crate::tokens::TAB_TOKEN + '&' as i32 {
                self.help(&[
                    "I can't figure out why you would want to use a tab mark",
                    "here. If you just want an ampersand, the remedy is",
                    "simple: Just type `I\\&' now. But if some right brace",
                    "up above has ended a previous alignment prematurely,",
                    "you're probably due for more error messages, and you",
                    "might try typing `S' now just to see what is salvageable.",
                ]);
            } else {
                self.help(&[
                    "I can't figure out why you would want to use a tab mark",
                    "or \\cr or \\span just now. If something like a right brace",
                    "up above has ended a previous alignment prematurely,",
                    "you're probably due for more error messages, and you",
                    "might try typing `S' now just to see what is salvageable.",
                ]);
            }
            self.error()
        } else {
            self.back_input()?;
            if self.inp.align_state < 0 {
                self.print_err("Missing { inserted");
                self.inp.align_state += 1;
                self.cur_tok = crate::tokens::LEFT_BRACE_TOKEN + '{' as i32;
            } else {
                self.print_err("Missing } inserted");
                self.inp.align_state -= 1;
                self.cur_tok = crate::tokens::RIGHT_BRACE_TOKEN + '}' as i32;
            }
            self.help(&[
                "I've put in what seems to be necessary to fix",
                "the current column of the current alignment.",
                "Try to go on, since this might almost work.",
            ]);
            // ins_error (§327).
            self.back_input()?;
            self.inp.cur.index = crate::input::INSERTED;
            self.error()
        }
    }

    /// `insert_dollar_sign` (§1047).
    fn insert_dollar_sign(&mut self) -> TexResult<()> {
        self.back_input()?;
        self.cur_tok = crate::tokens::MATH_SHIFT_TOKEN + '$' as i32;
        self.print_err("Missing $ inserted");
        self.help(&[
            "I've inserted a begin-math/end-math symbol since I think",
            "you left one out. Proceed, with fingers crossed.",
        ]);
        // ins_error (§327).
        self.back_input()?;
        self.inp.cur.index = crate::input::INSERTED;
        self.error()
    }

    /// `off_save` (§1064), reduced to the M2 groups.
    pub fn off_save(&mut self) -> TexResult<()> {
        if self.save.cur_group == BOTTOM_LEVEL {
            // §1066: drop the token.
            self.print_err("Extra ");
            let (c, ch) = (self.cur_cmd, self.cur_chr);
            self.print_cmd_chr(c, ch);
            self.help(&["Things are pretty mixed up, but I think the worst is over."]);
            self.error()
        } else {
            self.back_input()?;
            let p = self.mem.get_avail()?;
            self.print_err("Missing ");
            match self.save.cur_group {
                SEMI_SIMPLE_GROUP => {
                    let t = CS_TOKEN_FLAG + self.eqtb.lay.frozen_end_group;
                    self.mem.set_info(p, t);
                    self.print_esc_str("endgroup");
                }
                crate::eqtb::MATH_SHIFT_GROUP => {
                    self.mem
                        .set_info(p, crate::tokens::MATH_SHIFT_TOKEN + '$' as i32);
                    self.print_char('$' as i32);
                }
                crate::eqtb::MATH_LEFT_GROUP => {
                    let t = CS_TOKEN_FLAG + self.eqtb.lay.frozen_right;
                    self.mem.set_info(p, t);
                    let q = self.mem.get_avail()?;
                    self.mem.set_link(p, q);
                    self.mem
                        .set_info(q, crate::tokens::OTHER_TOKEN + '.' as i32);
                    self.print_esc_str("right.");
                }
                _ => {
                    self.mem
                        .set_info(p, crate::tokens::RIGHT_BRACE_TOKEN + '}' as i32);
                    self.print_char('}' as i32);
                }
            }
            self.print_chars(" inserted");
            self.help(&[
                "I've inserted something that you may have forgotten.",
                "(See the <inserted text> above.)",
                "With luck, this will get me unwedged. But if you",
                "really didn't forget anything, try typing `2' now; then",
                "my insertion and my current dilemma will both disappear.",
            ]);
            self.ins_list(p)?;
            self.error()
        }
    }

    /// `append_glue` (§1060).
    fn append_glue(&mut self) -> TexResult<()> {
        let s = self.cur_chr;
        match s {
            FIL_CODE => self.cur_val = self.mem.fil_glue(),
            FILL_CODE => self.cur_val = self.mem.fill_glue(),
            SS_CODE => self.cur_val = self.mem.ss_glue(),
            FIL_NEG_CODE => self.cur_val = self.mem.fil_neg_glue(),
            SKIP_CODE => self.scan_glue(GLUE_VAL)?,
            _ => self.scan_glue(MU_VAL)?,
        }
        let v = self.cur_val;
        let g = self.new_glue(v)?;
        self.tail_append(g);
        if s >= SKIP_CODE {
            let c = self.mem.glue_ref_count(v);
            self.mem.set_glue_ref_count(v, c - 1);
            if s > SKIP_CODE {
                let t = self.nest.cur.tail;
                self.mem.set_subtype(t, MU_GLUE);
            }
        }
        Ok(())
    }

    /// `append_kern` (§1061).
    fn append_kern(&mut self) -> TexResult<()> {
        let q = self.cur_chr;
        self.scan_dimen(q == i32::from(MU_GLUE), false, false)?;
        let v = self.cur_val;
        let k = self.new_kern(v)?;
        self.tail_append(k);
        let t = self.nest.cur.tail;
        self.mem.set_subtype(t, q as u16);
        Ok(())
    }

    /// `scan_rule_spec` (§463).
    pub fn scan_rule_spec(&mut self) -> TexResult<Pointer> {
        let q = self.new_rule()?;
        if self.cur_cmd == VRULE {
            self.mem.set_width(q, 26214); // default_rule = 0.4pt
        } else {
            self.mem.set_height(q, 26214);
            self.mem.set_depth(q, 0);
        }
        loop {
            if self.scan_keyword("width")? {
                self.scan_normal_dimen()?;
                let v = self.cur_val;
                self.mem.set_width(q, v);
                continue;
            }
            if self.scan_keyword("height")? {
                self.scan_normal_dimen()?;
                let v = self.cur_val;
                self.mem.set_height(q, v);
                continue;
            }
            if self.scan_keyword("depth")? {
                self.scan_normal_dimen()?;
                let v = self.cur_val;
                self.mem.set_depth(q, v);
                continue;
            }
            break;
        }
        Ok(q)
    }

    /// `box_end(box_context)` (§1075-§1078).
    pub fn box_end(&mut self, box_context: i32) -> TexResult<()> {
        if box_context < BOX_FLAG {
            // §1076: append cur_box to the current list, shifted.
            if self.cur_box != NULL {
                let b = self.cur_box;
                self.mem.set_shift_amount(b, box_context);
                if self.mode().abs() == VMODE {
                    self.append_to_vlist(b)?;
                    if self.adjust_tail != NULL {
                        let ah = self.mem.adjust_head();
                        if ah != self.adjust_tail {
                            let t = self.nest.cur.tail;
                            let l = self.mem.link(ah);
                            self.mem.set_link(t, l);
                            self.nest.cur.tail = self.adjust_tail;
                        }
                        self.adjust_tail = NULL;
                    }
                    if self.mode() > 0 {
                        self.build_page()?;
                    }
                } else {
                    let mut b = b;
                    if self.mode().abs() == HMODE {
                        self.set_space_factor(1000);
                    } else {
                        // §1076: in math, wrap the box in an Ord noad.
                        let p = self.new_noad()?;
                        self.mem.set_math_type(p + 1, crate::math::SUB_BOX);
                        self.mem.set_info(p + 1, b);
                        b = p;
                    }
                    self.tail_append(b);
                }
            }
        } else if box_context < SHIP_OUT_FLAG {
            // §1077 (+ etex.ch): store cur_box in a box register, possibly
            // a sparse one.
            let b = self.cur_box;
            let (n, global) = if box_context < GLOBAL_BOX_FLAG {
                (box_context - BOX_FLAG, false)
            } else {
                (box_context - GLOBAL_BOX_FLAG, true)
            };
            if n < 256 {
                let loc = self.eqtb.lay.box_base + n;
                if global {
                    self.geq_define(loc, BOX_REF, b);
                } else {
                    self.eq_define(loc, BOX_REF, b)?;
                }
            } else {
                // sa_def_box
                self.find_sa_element(crate::sa::BOX_VAL, n, true)?;
                let p = self.cur_ptr;
                if global {
                    self.gsa_def(p, b)?;
                } else {
                    self.sa_def(p, b)?;
                }
            }
        } else if self.cur_box != NULL {
            if box_context > SHIP_OUT_FLAG {
                // §1078: append a new leader node.
                self.get_next_nonblank_nonrelax_noncall()?;
                if (self.cur_cmd == HSKIP && self.mode().abs() != VMODE)
                    || (self.cur_cmd == VSKIP && self.mode().abs() == VMODE)
                {
                    self.append_glue()?;
                    let t = self.nest.cur.tail;
                    self.mem.set_subtype(
                        t,
                        (box_context - (LEADER_FLAG - i32::from(A_LEADERS))) as u16,
                    );
                    let b = self.cur_box;
                    self.mem.set_leader_ptr(t, b);
                } else {
                    self.print_err("Leaders not followed by proper glue");
                    self.help(&[
                        "You should say `\\leaders <box or rule><hskip or vskip>'.",
                        "I found the <box or rule>, but there's no suitable",
                        "<hskip or vskip>, so I'm ignoring these leaders.",
                    ]);
                    self.back_error()?;
                    let b = self.cur_box;
                    self.flush_node_list(b);
                }
            } else {
                let b = self.cur_box;
                self.ship_out(b)?;
            }
        }
        Ok(())
    }

    /// `begin_box(box_context)` (§1079-§1084).
    pub fn begin_box(&mut self, box_context: i32) -> TexResult<()> {
        match self.cur_chr {
            BOX_CODE => {
                self.scan_register_num()?;
                let n = self.cur_val;
                self.cur_box = self.fetch_box(n)?;
                // the box becomes void, at the same level
                self.change_box(n, NULL)?;
            }
            COPY_CODE => {
                self.scan_register_num()?;
                let n = self.cur_val;
                let b = self.fetch_box(n)?;
                self.cur_box = self.copy_node_list(b)?;
            }
            LAST_BOX_CODE => {
                // §1080.
                self.cur_box = NULL;
                if self.mode().abs() == MMODE {
                    self.you_cant();
                    self.help(&["Sorry; this \\lastbox will be void."]);
                    self.error()?;
                } else if self.mode() == VMODE && self.nest.cur.head == self.nest.cur.tail {
                    self.you_cant();
                    self.help(&[
                        "Sorry...I usually can't take things from the current page.",
                        "This \\lastbox will therefore be void.",
                    ]);
                    self.error()?;
                } else {
                    // §1080-§1081 (+ etex.ch): remove the effective last
                    // box, transparent to a final \beginM \endM pair.
                    let tx = self.find_effective_tail();
                    if !self.mem.is_char_node(tx)
                        && (self.mem.node_type(tx) == HLIST_NODE
                            || self.mem.node_type(tx) == VLIST_NODE)
                    {
                        if let Some(tx) = self.fetch_effective_tail(tx)? {
                            self.cur_box = tx;
                            self.mem.set_shift_amount(tx, 0);
                        }
                    }
                }
            }
            VSPLIT_CODE => {
                // §1082 (+ etex.ch).
                self.scan_register_num()?;
                let n = self.cur_val;
                if !self.scan_keyword("to")? {
                    self.print_err("Missing `to' inserted");
                    self.help(&[
                        "I'm working on `\\vsplit<box number> to <dimen>';",
                        "will look for the <dimen> next.",
                    ]);
                    self.error()?;
                }
                self.scan_normal_dimen()?;
                let h = self.cur_val;
                self.cur_box = self.vsplit(n, h)?;
            }
            _ => {
                // §1083: initiate the construction of an hbox or vbox.
                let mut k = self.cur_chr - VTOP_CODE;
                self.save.set_saved(0, box_context);
                if k == HMODE {
                    if box_context < BOX_FLAG && self.mode().abs() == VMODE {
                        self.scan_spec(ADJUSTED_HBOX_GROUP, true)?;
                    } else {
                        self.scan_spec(HBOX_GROUP, true)?;
                    }
                } else {
                    if k == VMODE {
                        self.scan_spec(VBOX_GROUP, true)?;
                    } else {
                        self.scan_spec(VTOP_GROUP, true)?;
                        k = VMODE;
                    }
                    self.normal_paragraph()?;
                }
                self.push_nest()?;
                self.nest.cur.mode = -k;
                if k == VMODE {
                    self.set_prev_depth(IGNORE_DEPTH);
                    let ev = self
                        .eqtb
                        .equiv(self.eqtb.lay.local_base + crate::eqtb::EVERY_VBOX_OFFSET);
                    if ev != NULL {
                        self.begin_token_list(ev, 11)?; // every_vbox_text
                    }
                } else {
                    self.set_space_factor(1000);
                    let eh = self
                        .eqtb
                        .equiv(self.eqtb.lay.local_base + crate::eqtb::EVERY_HBOX_OFFSET);
                    if eh != NULL {
                        self.begin_token_list(eh, 10)?; // every_hbox_text
                    }
                }
                return Ok(());
            }
        }
        self.box_end(box_context)
    }

    /// `scan_box(box_context)` (§1084).
    pub fn scan_box(&mut self, box_context: i32) -> TexResult<()> {
        self.get_next_nonblank_nonrelax_noncall()?;
        if self.cur_cmd == MAKE_BOX {
            self.begin_box(box_context)
        } else if box_context >= LEADER_FLAG && (self.cur_cmd == HRULE || self.cur_cmd == VRULE) {
            self.cur_box = self.scan_rule_spec()?;
            self.box_end(box_context)
        } else {
            self.print_err("A <box> was supposed to be here");
            self.help(&[
                "I was expecting to see \\hbox or \\vbox or \\copy or \\box or",
                "something like that. So you might find something missing in",
                "your output. But keep trying; you can fix this later.",
            ]);
            self.back_error()
        }
    }

    /// `package(c)` (§1086).
    fn package(&mut self, c: i32) -> TexResult<()> {
        self.latch_kanji_skips(); // pTeX [47.1086]
        if self.mode() == -HMODE {
            self.adjust_hlist(self.nest.cur.head, false)?;
        }
        let d = self.eqtb.dimen_par(crate::eqtb::BOX_MAX_DEPTH_CODE);
        self.unsave()?;
        self.save.save_ptr -= 3;
        if self.mode() == -HMODE {
            let h = self.nest.cur.head;
            let l = self.mem.link(h);
            let (w, m) = (self.save.saved(2), self.save.saved(1));
            self.cur_box = self.hpack(l, w, m)?;
        } else {
            let h = self.nest.cur.head;
            let l = self.mem.link(h);
            let (hh, m) = (self.save.saved(2), self.save.saved(1));
            self.cur_box = self.vpackage(l, hh, m, d)?;
            if c == VTOP_CODE {
                // §1087: readjust for \vtop.
                let b = self.cur_box;
                let mut h2 = 0;
                let p = self.mem.list_ptr(b);
                if p != NULL && self.mem.node_type(p) <= RULE_NODE && !self.mem.is_char_node(p) {
                    h2 = self.mem.height(p);
                }
                let dd = self.mem.depth(b) - h2 + self.mem.height(b);
                self.mem.set_depth(b, dd);
                self.mem.set_height(b, h2);
            }
        }
        self.pop_nest();
        let ctx = self.save.saved(0);
        self.box_end(ctx)
    }

    /// `you_cant` (§1049).
    pub fn you_cant(&mut self) {
        self.print_err("You can't use `");
        let (c, ch) = (self.cur_cmd, self.cur_chr);
        self.print_cmd_chr(c, ch);
        self.print_chars("' in ");
        let m = self.mode();
        self.print_mode(m);
    }

    // ----- The character/ligature/kern inner loop (§1034-§1040). -----

    /// `adjust_space_factor` (§1034).
    pub(crate) fn adjust_space_factor(&mut self) {
        let main_s = self.eqtb.sf_code(self.cur_chr);
        if main_s == 1000 {
            self.set_space_factor(1000);
        } else if main_s < 1000 {
            if main_s > 0 {
                self.set_space_factor(main_s);
            }
        } else if self.space_factor() < 1000 {
            self.set_space_factor(1000);
        } else {
            self.set_space_factor(main_s);
        }
    }

    /// `pack_lig(rt)` (§1035).
    fn pack_lig(&mut self, rt: bool) -> TexResult<()> {
        let lk = self.mem.link(self.cur_q);
        let main_p = self.new_ligature(self.main_f as u16, self.cur_l as u16, lk)?;
        if self.lft_hit {
            self.mem.set_subtype(main_p, 2);
            self.lft_hit = false;
        }
        if rt && self.lig_stack == NULL {
            let s = self.mem.subtype(main_p);
            self.mem.set_subtype(main_p, s + 1);
            self.rt_hit = false;
        }
        let q = self.cur_q;
        self.mem.set_link(q, main_p);
        self.nest.cur.tail = main_p;
        self.ligature_present = false;
        Ok(())
    }

    /// `wrapup(rt)` (§1035).
    fn wrapup(&mut self, rt: bool) -> TexResult<()> {
        if self.cur_l < NON_CHAR {
            if self.mem.link(self.cur_q) > NULL {
                let t = self.nest.cur.tail;
                if i32::from(self.mem.character(t)) == self.fonts.hyphen_char[self.main_f as usize]
                {
                    self.ins_disc = true;
                }
            }
            if self.ligature_present {
                self.pack_lig(rt)?;
            }
            if self.ins_disc {
                self.ins_disc = false;
                if self.mode() > 0 {
                    let d = self.new_disc()?;
                    self.tail_append(d);
                }
            }
        }
        Ok(())
    }

    /// §1034-§1040: the main loop, entered with `cur_chr` holding a
    /// character to typeset in the current font.
    fn main_loop(&mut self) -> TexResult<Next> {
        self.adjust_space_factor();
        self.main_f = self.eqtb.cur_font();
        self.lig_bchar = self.fonts.bchar[self.main_f as usize];
        self.lig_false_bchar = self.fonts.false_bchar[self.main_f as usize];
        // §1034: append a language whatsit if \language has changed.
        if self.mode() > 0 && self.eqtb.int_par(crate::eqtb::LANGUAGE_CODE) != self.clang() {
            self.fix_language()?;
        }
        self.lig_stack = self.mem.get_avail()?;
        let ls = self.lig_stack;
        self.mem.set_font(ls, self.main_f as u16);
        self.cur_l = self.cur_chr;
        self.mem.set_character(ls, self.cur_l as u16);
        self.cur_q = self.nest.cur.tail;
        let mut state: L;
        if self.cancel_boundary {
            self.cancel_boundary = false;
            self.main_k = NON_ADDRESS;
        } else {
            self.main_k = self.fonts.bchar_label[self.main_f as usize];
        }
        if self.main_k == NON_ADDRESS {
            state = L::Move2; // no left boundary processing
        } else {
            self.cur_r = self.cur_l;
            self.cur_l = NON_CHAR;
            state = L::LigLoop1; // begin with cursor after left boundary
        }
        loop {
            match state {
                L::Wrapup => {
                    let rt = self.rt_hit;
                    self.wrapup(rt)?;
                    state = L::Move;
                }
                L::Move => {
                    // §1036.
                    if self.lig_stack == NULL {
                        return Ok(Next::Reswitch);
                    }
                    self.cur_q = self.nest.cur.tail;
                    self.cur_l = i32::from(self.mem.character(self.lig_stack));
                    state = L::Move1;
                }
                L::Move1 => {
                    if !self.mem.is_char_node(self.lig_stack) {
                        state = L::MoveLig;
                    } else {
                        state = L::Move2;
                    }
                }
                L::Move2 => {
                    if self.cur_chr < self.fonts.bc[self.main_f as usize]
                        || self.cur_chr > self.fonts.ec[self.main_f as usize]
                    {
                        let (f, c) = (self.main_f, self.cur_chr);
                        self.char_warning(f, c);
                        let ls = self.lig_stack;
                        self.mem.free_avail(ls);
                        return Ok(Next::BigSwitch);
                    }
                    self.main_i = self.fonts.char_info(self.main_f, self.cur_l);
                    if !FontMem::char_exists(self.main_i) {
                        let (f, c) = (self.main_f, self.cur_chr);
                        self.char_warning(f, c);
                        let ls = self.lig_stack;
                        self.mem.free_avail(ls);
                        return Ok(Next::BigSwitch);
                    }
                    let ls = self.lig_stack;
                    self.tail_append(ls);
                    state = L::Lookahead;
                }
                L::Lookahead => {
                    // §1038 (+ pTeX: a Japanese character leaves the
                    // alphabetic loop and reswitches to append_kanji).
                    self.get_next()?;
                    if matches!(self.cur_cmd, LETTER | OTHER_CHAR | CHAR_GIVEN) {
                        if self.is_japanese_char(self.cur_chr) {
                            // uptex-m.ch: treat the boundary like a
                            // non-character and let main_loop_move's
                            // reswitch dispatch to append_kanji.
                            self.cur_r = NON_CHAR;
                            self.lig_stack = NULL;
                            state = L::LigLoop;
                            continue;
                        }
                        state = L::Lookahead1;
                        continue;
                    }
                    self.x_token()?;
                    if matches!(self.cur_cmd, LETTER | OTHER_CHAR | CHAR_GIVEN) {
                        if self.is_japanese_char(self.cur_chr) {
                            self.cur_r = NON_CHAR;
                            self.lig_stack = NULL;
                            state = L::LigLoop;
                            continue;
                        }
                        state = L::Lookahead1;
                        continue;
                    }
                    if self.cur_cmd == CHAR_NUM {
                        self.scan_char_num()?;
                        self.cur_chr = self.cur_val;
                        state = L::Lookahead1;
                        continue;
                    }
                    if self.cur_cmd == NO_BOUNDARY {
                        self.lig_bchar = NON_CHAR;
                    }
                    self.cur_r = self.lig_bchar;
                    self.lig_stack = NULL;
                    state = L::LigLoop;
                }
                L::Lookahead1 => {
                    self.adjust_space_factor();
                    self.lig_stack = self.mem.get_avail()?;
                    let ls = self.lig_stack;
                    self.mem.set_font(ls, self.main_f as u16);
                    self.cur_r = self.cur_chr;
                    self.mem.set_character(ls, self.cur_r as u16);
                    if self.cur_r == self.lig_false_bchar {
                        self.cur_r = NON_CHAR; // prevent spurious ligatures
                    }
                    state = L::LigLoop;
                }
                L::LigLoop => {
                    // §1039.
                    if FontMem::char_tag(self.main_i) != LIG_TAG {
                        state = L::Wrapup;
                        continue;
                    }
                    if self.cur_r == NON_CHAR {
                        state = L::Wrapup;
                        continue;
                    }
                    self.main_k = self.fonts.lig_kern_start(self.main_f, self.main_i);
                    self.main_j = self.fonts.info[self.main_k as usize];
                    if FontMem::skip_byte(self.main_j) <= STOP_FLAG {
                        state = L::LigLoop2;
                        continue;
                    }
                    self.main_k = self.fonts.lig_kern_restart(self.main_f, self.main_j);
                    state = L::LigLoop1;
                }
                L::LigLoop1 => {
                    self.main_j = self.fonts.info[self.main_k as usize];
                    state = L::LigLoop2;
                }
                L::LigLoop2 => {
                    if i32::from(FontMem::next_char(self.main_j)) == self.cur_r
                        && FontMem::skip_byte(self.main_j) <= STOP_FLAG
                    {
                        // §1040: do the ligature or kern command.
                        if FontMem::op_byte(self.main_j) >= crate::fonts::KERN_FLAG {
                            let rt = self.rt_hit;
                            self.wrapup(rt)?;
                            let k = self.fonts.char_kern(self.main_f, self.main_j);
                            let kn = self.new_kern(k)?;
                            self.tail_append(kn);
                            state = L::Move;
                            continue;
                        }
                        if self.cur_l == NON_CHAR {
                            self.lft_hit = true;
                        } else if self.lig_stack == NULL {
                            self.rt_hit = true;
                        }
                        let op = FontMem::op_byte(self.main_j);
                        let rem = i32::from(FontMem::rem_byte(self.main_j));
                        match op {
                            1 | 5 => {
                                // =:| , =:|>
                                self.cur_l = rem;
                                self.main_i = self.fonts.char_info(self.main_f, self.cur_l);
                                self.ligature_present = true;
                            }
                            2 | 6 => {
                                // |=: , |=:>
                                self.cur_r = rem;
                                if self.lig_stack == NULL {
                                    // right boundary character is consumed
                                    self.lig_stack = self.new_lig_item(self.cur_r as u16)?;
                                    self.lig_bchar = NON_CHAR;
                                } else if self.mem.is_char_node(self.lig_stack) {
                                    let main_p = self.lig_stack;
                                    self.lig_stack = self.new_lig_item(self.cur_r as u16)?;
                                    let ls = self.lig_stack;
                                    self.mem.set_lig_ptr(ls, main_p);
                                } else {
                                    let ls = self.lig_stack;
                                    self.mem.set_character(ls, self.cur_r as u16);
                                }
                            }
                            3 => {
                                // |=:|
                                self.cur_r = rem;
                                let main_p = self.lig_stack;
                                self.lig_stack = self.new_lig_item(self.cur_r as u16)?;
                                let ls = self.lig_stack;
                                self.mem.set_link(ls, main_p);
                            }
                            7 | 11 => {
                                // |=:|> , |=:|>>
                                self.wrapup(false)?;
                                self.cur_q = self.nest.cur.tail;
                                self.cur_l = rem;
                                self.main_i = self.fonts.char_info(self.main_f, self.cur_l);
                                self.ligature_present = true;
                            }
                            _ => {
                                // =:
                                self.cur_l = rem;
                                self.ligature_present = true;
                                if self.lig_stack == NULL {
                                    state = L::Wrapup;
                                } else {
                                    state = L::Move1;
                                }
                                continue;
                            }
                        }
                        if op > 4 && op != 7 {
                            state = L::Wrapup;
                            continue;
                        }
                        if self.cur_l < NON_CHAR {
                            state = L::LigLoop;
                            continue;
                        }
                        self.main_k = self.fonts.bchar_label[self.main_f as usize];
                        state = L::LigLoop1;
                        continue;
                    }
                    if FontMem::skip_byte(self.main_j) == 0 {
                        self.main_k += 1;
                    } else {
                        if FontMem::skip_byte(self.main_j) >= STOP_FLAG {
                            state = L::Wrapup;
                            continue;
                        }
                        self.main_k += i32::from(FontMem::skip_byte(self.main_j)) + 1;
                    }
                    state = L::LigLoop1;
                }
                L::MoveLig => {
                    // §1037.
                    let main_p = self.mem.lig_ptr(self.lig_stack);
                    if main_p > NULL {
                        self.tail_append(main_p); // append a single character
                    }
                    let temp = self.lig_stack;
                    self.lig_stack = self.mem.link(temp);
                    self.mem.free_node(temp, SMALL_NODE_SIZE);
                    self.main_i = self.fonts.char_info(self.main_f, self.cur_l);
                    self.ligature_present = true;
                    if self.lig_stack == NULL {
                        if main_p > NULL {
                            state = L::Lookahead;
                        } else {
                            self.cur_r = self.lig_bchar;
                            state = L::LigLoop;
                        }
                    } else {
                        self.cur_r = i32::from(self.mem.character(self.lig_stack));
                        state = L::LigLoop;
                    }
                }
            }
        }
    }

    /// §1041-§1042: append a normal inter-word space.
    fn append_normal_space(&mut self) -> TexResult<Next> {
        let ss = self.eqtb.glue_par(crate::eqtb::SPACE_SKIP_CODE);
        let temp_ptr = if ss == self.mem.zero_glue() {
            let main_p = self.font_glue_spec()?;
            self.new_glue(main_p)?
        } else {
            self.new_param_glue(crate::eqtb::SPACE_SKIP_CODE)?
        };
        self.tail_append(temp_ptr);
        Ok(Next::BigSwitch)
    }

    /// §1042: find (or build) the interword glue spec for the current font.
    fn font_glue_spec(&mut self) -> TexResult<Pointer> {
        let f = self.eqtb.cur_font();
        let mut main_p = self.fonts.glue[f as usize];
        if main_p == NULL {
            let zg = self.mem.zero_glue();
            main_p = self.new_spec(zg)?;
            let main_k = self.fonts.param_base[f as usize] + crate::fonts::SPACE_CODE;
            let w = self.fonts.info[main_k as usize].sc();
            let st = self.fonts.info[main_k as usize + 1].sc();
            let sh = self.fonts.info[main_k as usize + 2].sc();
            self.mem.set_width(main_p, w);
            self.mem.set_stretch(main_p, st);
            self.mem.set_shrink(main_p, sh);
            self.fonts.glue[f as usize] = main_p;
        }
        Ok(main_p)
    }

    /// `app_space` (§1043-§1044): spaces when `space_factor <> 1000`.
    fn app_space(&mut self) -> TexResult<()> {
        let sf = self.space_factor();
        let xss = self.eqtb.glue_par(crate::eqtb::XSPACE_SKIP_CODE);
        let q;
        if sf >= 2000 && xss != self.mem.zero_glue() {
            q = self.new_param_glue(crate::eqtb::XSPACE_SKIP_CODE)?;
        } else {
            let ss = self.eqtb.glue_par(crate::eqtb::SPACE_SKIP_CODE);
            let main_p = if ss != self.mem.zero_glue() {
                ss
            } else {
                self.font_glue_spec()?
            };
            let main_p = self.new_spec(main_p)?;
            // §1044: modify the glue by the space factor.
            let f = self.eqtb.cur_font();
            if sf >= 2000 {
                let w = self.mem.width(main_p) + self.fonts.extra_space(f);
                self.mem.set_width(main_p, w);
            }
            let st = crate::arith::xn_over_d(&mut self.arith, self.mem.stretch(main_p), sf, 1000);
            self.mem.set_stretch(main_p, st);
            let sh = crate::arith::xn_over_d(&mut self.arith, self.mem.shrink(main_p), 1000, sf);
            self.mem.set_shrink(main_p, sh);
            q = self.new_glue(main_p)?;
            self.mem.set_glue_ref_count(main_p, NULL);
        }
        self.tail_append(q);
        Ok(())
    }
}
