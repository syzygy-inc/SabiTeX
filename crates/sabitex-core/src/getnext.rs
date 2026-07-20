//! Getting the next token.
//!
//! Ports tex.web Part 24 (§332-§365): `get_next`, `check_outer_validity`
//! and `get_token`. The 48-way state switch of §344 is rendered as
//! comparisons on `state + cur_cmd`, exactly mirroring the Pascal cases.
//!
//! Unicode notes: catcodes index the full USV tables; the `^^` notation is
//! TeX82's (`^^X`, `^^xx`); XeTeX's `^^^^xxxx` arrives in M7.

use crate::cmds::*;
use crate::engine::Engine;
use crate::error::{TexInterrupt, TexResult};
use crate::input::*;
use crate::tokens::*;
use crate::types::{UnicodeChar, NULL};

/// `no_expand_flag` (§358): marks a special variant of `relax`. tex.web
/// uses 257; we use a value above any USV (cf. XeTeX).
pub const NO_EXPAND_FLAG: i32 = 0x11_0001;

/// `too_big_char`: one more than the biggest USV; used as the "not a
/// character" sentinel where tex.web uses 256 (XeTeX convention).
pub const TOO_BIG_CHAR: i32 = 0x11_0000;

fn is_hex(c: UnicodeChar) -> bool {
    c.is_positive()
        && (('0' as i32..='9' as i32).contains(&c) || ('a' as i32..='f' as i32).contains(&c))
}

fn hex_to_chr(c: UnicodeChar, cc: UnicodeChar) -> UnicodeChar {
    let hi = if c <= '9' as i32 {
        c - '0' as i32
    } else {
        c - 'a' as i32 + 10
    };
    let lo = if cc <= '9' as i32 {
        cc - '0' as i32
    } else {
        cc - 'a' as i32 + 10
    };
    16 * hi + lo
}

impl Engine {
    /// `check_outer_validity` (§336-§339): called when an `\outer` control
    /// sequence has been scanned or a file has ended (`cur_cs = 0`).
    pub fn check_outer_validity(&mut self) -> TexResult<()> {
        if self.inp.scanner_status != NORMAL_STATUS {
            self.deletions_allowed = false;
            // §337: back up an outer control sequence to be reread.
            if self.cur_cs != 0 {
                if self.inp.cur.state == TOKEN_LIST
                    || self.inp.cur.name < 1
                    || self.inp.cur.name > 17
                {
                    let p = self.mem.get_avail()?;
                    self.mem.set_info(p, CS_TOKEN_FLAG + self.cur_cs);
                    self.back_list(p)?; // prepare to read the cs again
                }
                self.cur_cmd = SPACER;
                self.cur_chr = ' ' as i32; // replace it by a space
            }
            if self.inp.scanner_status > SKIPPING {
                // §338: tell the user what has run away.
                self.runaway();
                if self.cur_cs == 0 {
                    self.print_err("File ended");
                } else {
                    self.cur_cs = 0;
                    self.print_err("Forbidden control sequence found");
                }
                self.print_chars(" while scanning ");
                // §339: insert tokens that should lead to recovery.
                let mut p = self.mem.get_avail()?;
                match self.inp.scanner_status {
                    DEFINING => {
                        self.print_chars("definition");
                        self.mem.set_info(p, RIGHT_BRACE_TOKEN + '}' as i32);
                    }
                    MATCHING => {
                        self.print_chars("use");
                        let t = self.par_token;
                        self.mem.set_info(p, t);
                        self.long_state = OUTER_CALL;
                    }
                    ALIGNING => {
                        self.print_chars("preamble");
                        self.mem.set_info(p, RIGHT_BRACE_TOKEN + '}' as i32);
                        let q = p;
                        p = self.mem.get_avail()?;
                        self.mem.set_link(p, q);
                        let cr = CS_TOKEN_FLAG + self.eqtb.lay.frozen_cr;
                        self.mem.set_info(p, cr);
                        self.inp.align_state = -1_000_000;
                    }
                    _ => {
                        self.print_chars("text"); // absorbing
                        self.mem.set_info(p, RIGHT_BRACE_TOKEN + '}' as i32);
                    }
                }
                self.ins_list(p)?;
                self.print_chars(" of ");
                let w = self.inp.warning_index;
                self.sprint_cs(w);
                self.help(&[
                    "I suspect you have forgotten a `}', causing me",
                    "to read past where you wanted me to stop.",
                    "I'll try to recover; but if the error is serious,",
                    "you'd better type `E' or `X' now and fix your file.",
                ]);
                self.error()?;
            } else {
                self.print_err("Incomplete ");
                let i = i32::from(self.cur_if);
                self.print_cmd_chr(IF_TEST, i);
                self.print_chars("; all text was ignored after line ");
                let sl = self.skip_line;
                self.print_int(sl);
                let first = if self.cur_cs != 0 {
                    self.cur_cs = 0;
                    "A forbidden control sequence occurred in skipped text."
                } else {
                    // §336: end of file, not a forbidden token.
                    "The file ended while I was skipping conditional text."
                };
                self.help(&[
                    first,
                    "This kind of error happens when you say `\\if...' and forget",
                    "the matching `\\fi'. I've inserted a `\\fi'; this might work.",
                ]);
                self.cur_tok = CS_TOKEN_FLAG + self.eqtb.lay.frozen_fi;
                self.ins_error()?;
            }
            self.deletions_allowed = true;
        }
        Ok(())
    }

    /// §355: if an expanded code (`^^X` / `^^xx`) appears at `buffer[k-1]`,
    /// reduce it in place (shifting the rest of the line left) and return
    /// true ("goto start_cs"). On entry `cur_chr = buffer[k-1]` and `cat`
    /// is its catcode.
    fn reduce_expanded_code(&mut self, k: i32, cat: u16) -> bool {
        let buf = |s: &Self, i: i32| s.inp.buffer[i as usize];
        if k < self.inp.cur.limit && buf(self, k) == self.cur_chr && cat == SUP_MARK {
            let c = buf(self, k + 1);
            if c < 0o200 {
                // yes, one is indeed present
                let mut d = 2;
                if is_hex(c) && k + 2 <= self.inp.cur.limit {
                    let cc = buf(self, k + 2);
                    if is_hex(cc) {
                        d += 1;
                    }
                }
                if d > 2 {
                    let cc = buf(self, k + 2);
                    self.cur_chr = hex_to_chr(c, cc);
                    let chr = self.cur_chr;
                    self.inp.buffer[(k - 1) as usize] = chr;
                } else if c < 0o100 {
                    self.inp.buffer[(k - 1) as usize] = c + 0o100;
                } else {
                    self.inp.buffer[(k - 1) as usize] = c - 0o100;
                }
                self.inp.cur.limit -= d;
                self.inp.first -= d;
                let mut k = k;
                while k <= self.inp.cur.limit {
                    self.inp.buffer[k as usize] = self.inp.buffer[(k + d) as usize];
                    k += 1;
                }
                return true;
            }
        }
        false
    }

    /// `get_next` (§341-§362): sets `cur_cmd`, `cur_chr`, `cur_cs` to the
    /// next input token.
    pub fn get_next(&mut self) -> TexResult<()> {
        'restart: loop {
            self.cur_cs = 0;
            if self.inp.cur.state != TOKEN_LIST {
                // §343: input from an external file.
                'switch: loop {
                    if self.inp.cur.loc <= self.inp.cur.limit {
                        // current line not yet finished
                        self.cur_chr = self.inp.buffer[self.inp.cur.loc as usize];
                        self.inp.cur.loc += 1;
                        'reswitch: loop {
                            self.cur_cmd = self.eqtb.cat_code(self.cur_chr) as u16;
                            // §344: change state if necessary. The Pascal
                            // case selector is state + cur_cmd.
                            let state = self.inp.cur.state;
                            let cmd = self.cur_cmd;
                            let sw = state + cmd;
                            let any = |c: u16| {
                                sw == MID_LINE + c || sw == SKIP_BLANKS + c || sw == NEW_LINE + c
                            };
                            // §345: cases where the character is ignored.
                            if any(IGNORE)
                                || (sw == SKIP_BLANKS + SPACER)
                                || (sw == NEW_LINE + SPACER)
                            {
                                continue 'switch;
                            }
                            if any(ESCAPE) {
                                // §354: scan a control sequence.
                                self.scan_control_sequence();
                                if std::env::var("SABITEX_DEBUG_CS").is_ok() {
                                    let lay = &self.eqtb.lay;
                                    let kind = if self.cur_cs >= lay.single_base
                                        && self.cur_cs < lay.null_cs
                                    {
                                        format!("single({})", self.cur_cs - lay.single_base)
                                    } else {
                                        "multi".to_string()
                                    };
                                    eprintln!(
                                        "DBG cs: cur_cs={} {kind} eq_type={}",
                                        self.cur_cs,
                                        self.eqtb.eq_type(self.cur_cs)
                                    );
                                }
                                self.cur_cmd = self.eqtb.eq_type(self.cur_cs);
                                self.cur_chr = self.eqtb.equiv(self.cur_cs);
                                if self.cur_cmd >= OUTER_CALL {
                                    self.check_outer_validity()?;
                                }
                            } else if any(ACTIVE_CHAR) {
                                // §353: process an active character.
                                self.cur_cs = self.cur_chr + self.eqtb.lay.active_base;
                                self.cur_cmd = self.eqtb.eq_type(self.cur_cs);
                                self.cur_chr = self.eqtb.equiv(self.cur_cs);
                                self.inp.cur.state = MID_LINE;
                                if self.cur_cmd >= OUTER_CALL {
                                    self.check_outer_validity()?;
                                }
                            } else if any(SUP_MARK) {
                                // §352: possible expanded character like ^^A.
                                let loc = self.inp.cur.loc;
                                if self.cur_chr == self.inp.buffer[loc as usize]
                                    && loc < self.inp.cur.limit
                                {
                                    let c = self.inp.buffer[(loc + 1) as usize];
                                    if c < 0o200 {
                                        self.inp.cur.loc = loc + 2;
                                        if is_hex(c) && self.inp.cur.loc <= self.inp.cur.limit {
                                            let cc = self.inp.buffer[self.inp.cur.loc as usize];
                                            if is_hex(cc) {
                                                self.inp.cur.loc += 1;
                                                self.cur_chr = hex_to_chr(c, cc);
                                                continue 'reswitch;
                                            }
                                        }
                                        self.cur_chr =
                                            if c < 0o100 { c + 0o100 } else { c - 0o100 };
                                        continue 'reswitch;
                                    }
                                }
                                self.inp.cur.state = MID_LINE;
                            } else if any(INVALID_CHAR) {
                                // §346: decry the invalid character.
                                self.print_err("Text line contains an invalid character");
                                self.help(&[
                                    "A funny symbol that I can't read has just been input.",
                                    "Continue, and I'll forget that it ever happened.",
                                ]);
                                self.deletions_allowed = false;
                                self.error()?;
                                self.deletions_allowed = true;
                                continue 'restart;
                            } else if sw == MID_LINE + SPACER {
                                // §349: enter skip_blanks, emit a space.
                                self.inp.cur.state = SKIP_BLANKS;
                                self.cur_chr = ' ' as i32;
                            } else if sw == MID_LINE + CAR_RET {
                                // §348: finish line, emit a space.
                                self.inp.cur.loc = self.inp.cur.limit + 1;
                                self.cur_cmd = SPACER;
                                self.cur_chr = ' ' as i32;
                            } else if sw == SKIP_BLANKS + CAR_RET || any(COMMENT) {
                                // §350: finish line, go to switch.
                                self.inp.cur.loc = self.inp.cur.limit + 1;
                                continue 'switch;
                            } else if sw == NEW_LINE + CAR_RET {
                                // §351: finish line, emit \par.
                                self.inp.cur.loc = self.inp.cur.limit + 1;
                                self.cur_cs = self.par_loc;
                                self.cur_cmd = self.eqtb.eq_type(self.cur_cs);
                                self.cur_chr = self.eqtb.equiv(self.cur_cs);
                                if self.cur_cmd >= OUTER_CALL {
                                    self.check_outer_validity()?;
                                }
                            } else if sw == MID_LINE + LEFT_BRACE {
                                self.inp.align_state += 1;
                            } else if sw == SKIP_BLANKS + LEFT_BRACE || sw == NEW_LINE + LEFT_BRACE
                            {
                                self.inp.cur.state = MID_LINE;
                                self.inp.align_state += 1;
                            } else if sw == MID_LINE + RIGHT_BRACE {
                                self.inp.align_state -= 1;
                            } else if sw == SKIP_BLANKS + RIGHT_BRACE
                                || sw == NEW_LINE + RIGHT_BRACE
                            {
                                self.inp.cur.state = MID_LINE;
                                self.inp.align_state -= 1;
                            } else if state != MID_LINE
                                && matches!(
                                    cmd,
                                    MATH_SHIFT
                                        | TAB_MARK
                                        | MAC_PARAM
                                        | SUB_MARK
                                        | LETTER
                                        | OTHER_CHAR
                                )
                            {
                                // add_delims_to(skip_blanks/new_line)
                                self.inp.cur.state = MID_LINE;
                            }
                            break 'reswitch;
                        }
                        break 'switch;
                    } else {
                        self.inp.cur.state = NEW_LINE;
                        // §360: move to the next line of the file.
                        if self.inp.cur.name > 17 {
                            // §362 (+ etex.ch): read the next line of the
                            // current (pseudo) file.
                            self.inp.line += 1;
                            self.inp.first = self.inp.cur.start;
                            if !self.inp.force_eof {
                                let got_line = if self.inp.cur.name <= 19 {
                                    self.pseudo_input()? // etex.ch \scantokens
                                } else {
                                    self.input_ln_file()?
                                };
                                if got_line {
                                    self.firm_up_the_line(); // sets limit
                                } else {
                                    // etex.ch: insert \everyeof before the
                                    // file actually ends.
                                    let ee = self.eqtb.equiv(
                                        self.eqtb.lay.local_base + crate::eqtb::EVERY_EOF_OFFSET,
                                    );
                                    let idx = self.inp.cur.index as usize;
                                    if ee != NULL && !self.eof_seen[idx] {
                                        self.inp.cur.limit = self.inp.first - 1;
                                        self.eof_seen[idx] = true; // fake one empty line
                                        self.begin_token_list(ee, crate::input::EVERY_EOF_TEXT)?;
                                        continue 'restart;
                                    }
                                    self.inp.force_eof = true;
                                }
                            }
                            if self.inp.force_eof {
                                // etex.ch: warn about groups/conditionals
                                // left incomplete by this file.
                                if self.eqtb.int_par(crate::eqtb::TRACING_NESTING_CODE) > 0
                                    && (self.grp_stack[self.inp.in_open] != self.save.cur_boundary
                                        || self.if_stack[self.inp.in_open] != self.cond_ptr)
                                {
                                    self.file_warning();
                                }
                                if self.inp.cur.name >= 19 {
                                    self.print_char(')' as i32);
                                    self.inp.open_parens -= 1;
                                }
                                self.inp.force_eof = false;
                                self.end_file_reading(); // resume previous level
                                self.check_outer_validity()?;
                                continue 'restart;
                            }
                            if self.end_line_char_inactive() {
                                self.inp.cur.limit -= 1;
                            } else {
                                let e = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
                                self.inp.buffer[self.inp.cur.limit as usize] = e;
                            }
                            self.inp.first = self.inp.cur.limit + 1;
                            self.inp.cur.loc = self.inp.cur.start; // ready to read
                        } else {
                            if !self.inp.terminal_input() {
                                // \read line has ended
                                self.cur_cmd = 0;
                                self.cur_chr = 0;
                                return Ok(());
                            }
                            if self.inp.input_ptr > 0 {
                                // text was inserted during error recovery
                                self.end_file_reading();
                                continue 'restart; // resume previous level
                            }
                            if self.prn.selector < crate::print::LOG_ONLY {
                                self.open_log_file()?;
                            }
                            if self.interaction > NONSTOP_MODE {
                                if self.end_line_char_inactive() {
                                    self.inp.cur.limit += 1;
                                }
                                if self.inp.cur.limit == self.inp.cur.start {
                                    // previous line was empty
                                    self.print_nl_chars("(Please type a command or say `\\end')");
                                }
                                self.print_ln();
                                self.inp.first = self.inp.cur.start;
                                // prompt_input("*")
                                self.print_char('*' as i32);
                                if !self.term_input_line()? {
                                    return Err(TexInterrupt::FatalError(
                                        "End of file on the terminal!",
                                    ));
                                }
                                self.inp.cur.limit = self.inp.last;
                                if self.end_line_char_inactive() {
                                    self.inp.cur.limit -= 1;
                                } else {
                                    let e = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
                                    self.inp.buffer[self.inp.cur.limit as usize] = e;
                                }
                                self.inp.first = self.inp.cur.limit + 1;
                                self.inp.cur.loc = self.inp.cur.start;
                            } else {
                                return Err(TexInterrupt::FatalError(
                                    "*** (job aborted, no legal \\end found)",
                                ));
                            }
                        }
                        continue 'switch;
                    }
                }
            } else {
                // §357: input from a token list.
                if self.inp.cur.loc != NULL {
                    let t = self.mem.info(self.inp.cur.loc);
                    self.inp.cur.loc = self.mem.link(self.inp.cur.loc); // move to next
                    if t >= CS_TOKEN_FLAG {
                        // a control sequence token
                        self.cur_cs = t - CS_TOKEN_FLAG;
                        self.cur_cmd = self.eqtb.eq_type(self.cur_cs);
                        self.cur_chr = self.eqtb.equiv(self.cur_cs);
                        if self.cur_cmd >= OUTER_CALL {
                            if self.cur_cmd == DONT_EXPAND {
                                // §358: get the next token, suppressing expansion.
                                self.cur_cs = self.mem.info(self.inp.cur.loc) - CS_TOKEN_FLAG;
                                self.inp.cur.loc = NULL;
                                self.cur_cmd = self.eqtb.eq_type(self.cur_cs);
                                self.cur_chr = self.eqtb.equiv(self.cur_cs);
                                if self.cur_cmd > MAX_COMMAND {
                                    self.cur_cmd = RELAX;
                                    self.cur_chr = NO_EXPAND_FLAG;
                                }
                            } else {
                                self.check_outer_validity()?;
                            }
                        }
                    } else {
                        self.cur_cmd = (t / MAX_CHAR_VAL) as u16;
                        self.cur_chr = t % MAX_CHAR_VAL;
                        match self.cur_cmd {
                            LEFT_BRACE => self.inp.align_state += 1,
                            RIGHT_BRACE => self.inp.align_state -= 1,
                            OUT_PARAM => {
                                // §359: insert macro parameter.
                                let i = (self.inp.cur.limit + self.cur_chr - 1) as usize;
                                let p = self.inp.param_stack[i];
                                self.begin_token_list(p, PARAMETER)?;
                                continue 'restart;
                            }
                            _ => {}
                        }
                    }
                } else {
                    // we are done with this token list
                    self.end_token_list()?;
                    continue 'restart; // resume previous level
                }
            }
            // §342: if an alignment entry has just ended, act accordingly.
            if self.cur_cmd <= CAR_RET && self.cur_cmd >= TAB_MARK && self.inp.align_state == 0 {
                // §791: insert the <v_j> template and goto restart.
                self.insert_vj_template()?;
                continue 'restart;
            }
            return Ok(());
        }
    }

    /// §354: scan a control sequence after an escape character; sets
    /// `cur_cs` and the scanner state.
    fn scan_control_sequence(&mut self) {
        if self.inp.cur.loc > self.inp.cur.limit {
            self.cur_cs = self.eqtb.lay.null_cs; // state is irrelevant
        } else {
            'start_cs: loop {
                let mut k = self.inp.cur.loc;
                self.cur_chr = self.inp.buffer[k as usize];
                let mut cat = self.eqtb.cat_code(self.cur_chr) as u16;
                k += 1;
                if cat == LETTER || cat == SPACER {
                    self.inp.cur.state = SKIP_BLANKS;
                } else {
                    self.inp.cur.state = MID_LINE;
                }
                if cat == LETTER && k <= self.inp.cur.limit {
                    // §356: scan ahead until finding a nonletter.
                    loop {
                        self.cur_chr = self.inp.buffer[k as usize];
                        cat = self.eqtb.cat_code(self.cur_chr) as u16;
                        k += 1;
                        if cat != LETTER || k > self.inp.cur.limit {
                            break;
                        }
                    }
                    if self.reduce_expanded_code(k, cat) {
                        continue 'start_cs;
                    }
                    if cat != LETTER {
                        k -= 1; // now k points to the first nonletter
                    }
                    if k > self.inp.cur.loc + 1 {
                        // multiletter control sequence
                        let loc = self.inp.cur.loc;
                        self.cur_cs = self.id_lookup(loc, k - loc);
                        self.inp.cur.loc = k;
                        return;
                    }
                } else if self.reduce_expanded_code(k, cat) {
                    continue 'start_cs;
                }
                self.cur_cs =
                    self.eqtb.lay.single_base + self.inp.buffer[self.inp.cur.loc as usize];
                self.inp.cur.loc += 1;
                return;
            }
        }
    }

    /// `get_token` (§365): sets `cur_cmd`, `cur_chr`, `cur_tok` (and may
    /// define a new control sequence).
    pub fn get_token(&mut self) -> TexResult<()> {
        self.eqtb.no_new_control_sequence = false;
        self.get_next()?;
        self.eqtb.no_new_control_sequence = true;
        self.cur_tok = if self.cur_cs == 0 {
            i32::from(self.cur_cmd) * MAX_CHAR_VAL + self.cur_chr
        } else {
            CS_TOKEN_FLAG + self.cur_cs
        };
        Ok(())
    }

    /// Packs `(cur_cmd, cur_chr, cur_cs)` into `cur_tok` (tail of §365).
    pub fn set_cur_tok(&mut self) {
        self.cur_tok = if self.cur_cs == 0 {
            i32::from(self.cur_cmd) * MAX_CHAR_VAL + self.cur_chr
        } else {
            CS_TOKEN_FLAG + self.cur_cs
        };
    }
}
