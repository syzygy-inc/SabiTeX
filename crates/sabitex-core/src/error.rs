//! Error reporting scaffolding.
//!
//! Ports the skeleton of tex.web Part 6: the `history` variable (§76) and
//! the non-local exits (`jump_out`, `overflow` §94, `fatal_error` §93).
//! Pascal's `goto end_of_TEX` becomes a `Result` that unwinds to the main
//! control loop; the full interactive `error()` routine arrives with M1.

use core::fmt;

/// `history` (tex.web §76): how bad was this run?
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum History {
    /// `spotless`: nothing has been amiss yet.
    Spotless = 0,
    /// `warning_issued`: `begin_diagnostic` has been called.
    WarningIssued = 1,
    /// `error_message_issued`: `error` has been called.
    ErrorMessageIssued = 2,
    /// `fatal_error_stop`: termination was premature.
    FatalErrorStop = 3,
}

/// A non-local exit out of the engine, replacing tex.web's `goto end_of_TEX`
/// / `goto final_end`. Every routine that can abort returns
/// `TexResult<T>` and the condition propagates to the caller of
/// `main_control` (M1+), which finishes the log and terminates.
#[derive(Debug)]
pub enum TexInterrupt {
    /// `overflow(s, n)` (tex.web §94): TeX capacity exceeded.
    Overflow {
        /// Which capacity was exceeded, e.g. `"main memory size"`.
        what: &'static str,
        /// The current maximum value of that capacity.
        size: i32,
    },
    /// `fatal_error(s)` (tex.web §93): the job has to be aborted right now.
    FatalError(&'static str),
}

impl fmt::Display for TexInterrupt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TexInterrupt::Overflow { what, size } => {
                write!(f, "TeX capacity exceeded, sorry [{what}={size}]")
            }
            TexInterrupt::FatalError(s) => write!(f, "Emergency stop: {s}"),
        }
    }
}

/// Result type used by every engine routine that can abort the run.
pub type TexResult<T> = Result<T, TexInterrupt>;

use crate::engine::Engine;
use crate::input::{BATCH_MODE, ERROR_STOP_MODE, SCROLL_MODE, TOKEN_LIST};
use crate::print::{LOG_ONLY, PSEUDO, TERM_AND_LOG};

impl Engine {
    /// `print_err(s)` (§73).
    pub fn print_err(&mut self, s: &str) {
        self.print_nl_chars("! ");
        self.print_chars(s);
    }

    /// `helpN(...)` (§79): record the help for the next `error`.
    pub fn help(&mut self, lines: &[&'static str]) {
        self.help_lines.clear();
        self.help_lines.extend_from_slice(lines);
    }

    /// `give_err_help` (§1284): `token_show(err_help)`.
    fn give_err_help(&mut self) {
        let eh = self
            .eqtb
            .equiv(self.eqtb.lay.local_base + crate::eqtb::ERR_HELP_OFFSET);
        self.token_show(eh);
    }

    /// `error` (§82-§90): completes the job of error reporting.
    pub fn error(&mut self) -> TexResult<()> {
        if self.history < History::ErrorMessageIssued {
            self.history = History::ErrorMessageIssued;
        }
        self.print_char('.' as i32);
        self.show_context();
        if self.interaction == ERROR_STOP_MODE {
            // §83: get the user's advice.
            return self.get_users_advice();
        }
        self.error_count += 1;
        if self.error_count == 100 {
            // §88.
            self.print_nl_chars("(That makes 100 errors; please try again.)");
            self.history = History::FatalErrorStop;
            return Err(TexInterrupt::FatalError("(That makes 100 errors)"));
        }
        // §90: put the help message on the transcript file.
        if self.interaction > BATCH_MODE {
            self.prn.selector -= 1; // avoid terminal output
        }
        if self.use_err_help {
            self.print_ln();
            self.give_err_help();
        } else {
            let lines = std::mem::take(&mut self.help_lines);
            for l in lines {
                self.print_nl_chars(l);
            }
        }
        self.print_ln();
        if self.interaction > BATCH_MODE {
            self.prn.selector += 1; // re-enable terminal output
        }
        self.print_ln();
        Ok(())
    }

    /// §83-§87: prompt the user in errorstop mode and act on the reply.
    fn get_users_advice(&mut self) -> TexResult<()> {
        loop {
            // continue:
            if self.interaction != ERROR_STOP_MODE {
                return Ok(());
            }
            self.clear_for_error_prompt();
            self.prompt_input("? ")?;
            if self.inp.last == self.inp.first {
                return Ok(());
            }
            let mut c = self.inp.buffer[self.inp.first as usize];
            if ('a' as i32..='z' as i32).contains(&c) {
                c -= 32; // convert to uppercase
            }
            // §84: interpret code c.
            match c {
                d if ('0' as i32..='9' as i32).contains(&d) && self.deletions_allowed => {
                    // §86: delete c-"0" tokens.
                    let (s1, s2, s3, s4) = (
                        self.cur_tok,
                        self.cur_cmd,
                        self.cur_chr,
                        self.inp.align_state,
                    );
                    self.inp.align_state = 1_000_000;
                    let mut n = d - '0' as i32;
                    let second = self.inp.first + 1;
                    if self.inp.last > second
                        && ('0' as i32..='9' as i32).contains(&self.inp.buffer[second as usize])
                    {
                        n = n * 10 + self.inp.buffer[second as usize] - '0' as i32;
                    }
                    while n > 0 {
                        self.get_token()?;
                        n -= 1;
                    }
                    self.cur_tok = s1;
                    self.cur_cmd = s2;
                    self.cur_chr = s3;
                    self.inp.align_state = s4;
                    self.help(&[
                        "I have just deleted some text, as you asked.",
                        "You can now delete more, or insert, or whatever.",
                    ]);
                    self.show_context();
                    continue;
                }
                c if c == 'H' as i32 => {
                    // §89: print the help information.
                    if self.use_err_help {
                        self.give_err_help();
                        self.use_err_help = false;
                    } else {
                        if self.help_lines.is_empty() {
                            self.help(&[
                                "Sorry, I don't know how to help in this situation.",
                                "Maybe you should try asking a human?",
                            ]);
                        }
                        let lines = std::mem::take(&mut self.help_lines);
                        for l in lines {
                            self.print_chars(l);
                            self.print_ln();
                        }
                    }
                    self.help(&[
                        "Sorry, I already gave what help I could...",
                        "Maybe you should try asking a human?",
                        "An error might have occurred before I noticed any problems.",
                        "``If all else fails, read the instructions.''",
                    ]);
                    continue;
                }
                c if c == 'I' as i32 => {
                    // §87: introduce new material from the terminal.
                    self.begin_file_reading()?;
                    if self.inp.last > self.inp.first + 1 {
                        self.inp.cur.loc = self.inp.first + 1;
                        self.inp.buffer[self.inp.first as usize] = ' ' as i32;
                    } else {
                        self.prompt_input("insert>")?;
                        self.inp.cur.loc = self.inp.first;
                    }
                    self.inp.first = self.inp.last;
                    self.inp.cur.limit = self.inp.last - 1; // no end_line_char
                    return Ok(());
                }
                c if c == 'Q' as i32 || c == 'R' as i32 || c == 'S' as i32 => {
                    // §86: change the interaction level.
                    self.error_count = 0;
                    self.interaction = BATCH_MODE + (c - 'Q' as i32) as u8;
                    self.print_chars("OK, entering ");
                    if c == 'Q' as i32 {
                        self.print_esc_str("batchmode");
                        self.prn.selector -= 1;
                    } else if c == 'R' as i32 {
                        self.print_esc_str("nonstopmode");
                    } else {
                        self.print_esc_str("scrollmode");
                    }
                    self.print_chars("...");
                    self.print_ln();
                    return Ok(());
                }
                c if c == 'X' as i32 => {
                    self.interaction = SCROLL_MODE;
                    return Err(TexInterrupt::FatalError("user asked to quit")); // jump_out
                }
                _ => {}
            }
            // §85: print the menu of available options.
            self.print_chars("Type <return> to proceed, S to scroll future error messages,");
            self.print_nl_chars("R to run without stopping, Q to run quietly,");
            self.print_nl_chars("I to insert something, ");
            if self.deletions_allowed {
                self.print_nl_chars("1 or ... or 9 to ignore the next 1 to 9 tokens of input,");
            }
            self.print_nl_chars("H for help, X to quit.");
        }
    }

    /// `clear_for_error_prompt` (§330).
    fn clear_for_error_prompt(&mut self) {
        while self.inp.cur.state != TOKEN_LIST
            && self.inp.cur.name == 0
            && self.inp.input_ptr > 0
            && self.inp.cur.loc > self.inp.cur.limit
        {
            self.end_file_reading();
        }
        self.print_ln();
    }

    /// `int_error(n)` (§91).
    pub fn int_error(&mut self, n: i32) -> TexResult<()> {
        self.print_chars(" (");
        self.print_int(n);
        self.print_char(')' as i32);
        self.error()
    }

    /// `confusion(s)` (§95): consistency check violated.
    pub fn confusion(&mut self, s: &str) -> TexResult<()> {
        self.print_err("This can't happen (");
        self.print_chars(s);
        self.print_char(')' as i32);
        self.history = History::FatalErrorStop;
        Err(TexInterrupt::FatalError("This can't happen"))
    }

    /// `begin_diagnostic` (§245).
    pub fn begin_diagnostic(&mut self) {
        self.old_setting = self.prn.selector;
        if self.eqtb.int_par(crate::eqtb::TRACING_ONLINE_CODE) <= 0
            && self.prn.selector == TERM_AND_LOG
        {
            self.prn.selector -= 1; // log_only
            if self.history == History::Spotless {
                self.history = History::WarningIssued;
            }
        }
    }

    /// `end_diagnostic(blank_line)` (§245).
    pub fn end_diagnostic(&mut self, blank_line: bool) {
        self.print_nl_chars("");
        if blank_line {
            self.print_ln();
        }
        self.prn.selector = self.old_setting;
    }

    /// `show_cur_cmd_chr` (§299).
    pub fn show_cur_cmd_chr(&mut self) {
        self.begin_diagnostic();
        self.print_nl_chars("{");
        if self.nest.cur.mode != self.nest.shown_mode {
            let m = self.nest.cur.mode;
            self.print_mode(m);
            self.print_chars(": ");
            self.nest.shown_mode = self.nest.cur.mode;
        }
        let (c, ch) = (self.cur_cmd, self.cur_chr);
        self.print_cmd_chr(c, ch);
        // etex.ch §299: \tracingifs annotates conditional commands with
        // the nesting level and originating line.
        if self.eqtb.int_par(crate::eqtb::TRACING_IFS_CODE) > 0
            && self.cur_cmd >= crate::cmds::IF_TEST
            && self.cur_cmd <= crate::cmds::FI_OR_ELSE
        {
            self.print_chars(": ");
            let (mut n, l);
            if self.cur_cmd == crate::cmds::FI_OR_ELSE {
                let ci = self.cur_if;
                self.print_cmd_chr(crate::cmds::IF_TEST, i32::from(ci));
                self.print_char(' ' as i32);
                n = 0;
                l = self.if_line;
            } else {
                n = 1;
                l = self.inp.line;
            }
            let mut p = self.cond_ptr;
            while p != crate::types::NULL {
                n += 1;
                p = self.mem.link(p);
            }
            self.print_chars("(level ");
            self.print_int(n);
            self.print_char(')' as i32);
            self.print_if_line(l);
        }
        self.print_char('}' as i32);
        self.end_diagnostic(false);
    }

    /// `print_if_line(l)` (etex.ch).
    pub fn print_if_line(&mut self, l: i32) {
        if l != 0 {
            self.print_chars(" entered on line ");
            self.print_int(l);
        }
    }

    /// §316: the "magic computation" for pseudoprinting.
    pub fn set_trick_count(&mut self) {
        self.prn.first_count = self.prn.tally;
        self.prn.trick_count = (self.prn.tally + 1 + self.sizes.error_line as i32
            - self.sizes.half_error_line as i32)
            .max(self.sizes.error_line as i32);
    }

    /// `show_context` (§310-§318): prints where the scanner is, on all
    /// levels down to the most recent line of characters from a file.
    pub fn show_context(&mut self) {
        self.inp.stack[self.inp.input_ptr] = self.inp.cur; // store current state
        let mut base_ptr = self.inp.input_ptr;
        let mut nn: i32 = -1;
        let mut bottom_line = false;
        let ecl = self.eqtb.int_par(crate::eqtb::ERROR_CONTEXT_LINES_CODE);
        loop {
            let rec = self.inp.stack[base_ptr]; // enter into the context
                                                // etex.ch §311: pseudo files (names 18/19) are not the
                                                // bottom line; the enclosing real file still shows.
            if rec.state != TOKEN_LIST && (rec.name > 19 || base_ptr == 0) {
                bottom_line = true;
            }
            if base_ptr == self.inp.input_ptr || bottom_line || nn < ecl {
                // §312: display the current context.
                if base_ptr == self.inp.input_ptr
                    || rec.state != TOKEN_LIST
                    || rec.index != crate::input::BACKED_UP
                    || rec.loc != crate::types::NULL
                {
                    // we omit backed-up token lists that have been read
                    self.prn.tally = 0;
                    let old_setting = self.prn.selector;
                    let l: i32;
                    if rec.state != TOKEN_LIST {
                        // §313: print the location of the current line.
                        if rec.name <= 17 {
                            if rec.name == 0 {
                                // terminal_input
                                if base_ptr == 0 {
                                    self.print_nl_chars("<*>");
                                } else {
                                    self.print_nl_chars("<insert> ");
                                }
                            } else {
                                self.print_nl_chars("<read ");
                                if rec.name == 17 {
                                    self.print_char('*' as i32);
                                } else {
                                    let n = rec.name - 1;
                                    self.print_int(n);
                                }
                                self.print_char('>' as i32);
                            }
                        } else {
                            self.print_nl_chars("l.");
                            // etex.ch (§313): for a line below the top of
                            // the in_open stack (e.g. under a pseudo
                            // file), its line number was pushed onto
                            // line_stack by the file opened above it.
                            let line = if rec.index as usize == self.inp.in_open {
                                self.inp.line
                            } else {
                                self.inp.line_stack[rec.index as usize + 1]
                            };
                            self.print_int(line);
                        }
                        self.print_char(' ' as i32);
                        // §318: pseudoprint the line.
                        l = self.prn.tally;
                        self.prn.tally = 0;
                        self.prn.selector = PSEUDO;
                        self.prn.trick_count = 1_000_000;
                        let elc = self.eqtb.int_par(crate::eqtb::END_LINE_CHAR_CODE);
                        let j = if rec.limit >= 0 && self.inp.buffer[rec.limit as usize] == elc {
                            rec.limit
                        } else {
                            rec.limit + 1 // the effective end of the line
                        };
                        if j > 0 {
                            for i in rec.start..j {
                                if i == rec.loc {
                                    self.set_trick_count();
                                }
                                let c = self.inp.buffer[i as usize];
                                self.print(c);
                            }
                        }
                    } else {
                        // §314: print the type of token list.
                        match rec.index {
                            crate::input::PARAMETER => self.print_nl_chars("<argument> "),
                            crate::input::U_TEMPLATE | crate::input::V_TEMPLATE => {
                                self.print_nl_chars("<template> ")
                            }
                            crate::input::BACKED_UP => {
                                if rec.loc == crate::types::NULL {
                                    self.print_nl_chars("<recently read> ");
                                } else {
                                    self.print_nl_chars("<to be read again> ");
                                }
                            }
                            crate::input::INSERTED => self.print_nl_chars("<inserted text> "),
                            crate::input::MACRO => {
                                self.print_ln();
                                let name = rec.name;
                                self.print_cs(name);
                            }
                            crate::input::OUTPUT_TEXT => self.print_nl_chars("<output> "),
                            crate::input::EVERY_PAR_TEXT => self.print_nl_chars("<everypar> "),
                            8 => self.print_nl_chars("<everymath> "),
                            9 => self.print_nl_chars("<everydisplay> "),
                            10 => self.print_nl_chars("<everyhbox> "),
                            11 => self.print_nl_chars("<everyvbox> "),
                            12 => self.print_nl_chars("<everyjob> "),
                            13 => self.print_nl_chars("<everycr> "),
                            14 => self.print_nl_chars("<mark> "),
                            crate::input::EVERY_EOF_TEXT => {
                                self.print_nl_chars("<everyeof> ") // etex.ch
                            }
                            crate::input::WRITE_TEXT => self.print_nl_chars("<write> "),
                            _ => self.print_nl_chars("?"), // this should never happen
                        }
                        // §319: pseudoprint the token list.
                        l = self.prn.tally;
                        self.prn.tally = 0;
                        self.prn.selector = PSEUDO;
                        self.prn.trick_count = 1_000_000;
                        if rec.index < crate::input::MACRO {
                            self.show_token_list(rec.start, rec.loc, 100_000);
                        } else {
                            let lk = self.mem.link(rec.start);
                            self.show_token_list(lk, rec.loc, 100_000); // avoid refcount
                        }
                    }
                    self.prn.selector = old_setting; // stop pseudoprinting
                                                     // §317: print the two lines.
                    if self.prn.trick_count == 1_000_000 {
                        self.set_trick_count();
                    }
                    let error_line = self.sizes.error_line as i32;
                    let half_error_line = self.sizes.half_error_line as i32;
                    let m = if self.prn.tally < self.prn.trick_count {
                        self.prn.tally - self.prn.first_count
                    } else {
                        self.prn.trick_count - self.prn.first_count
                    };
                    let (p, n);
                    if l + self.prn.first_count <= half_error_line {
                        p = 0;
                        n = l + self.prn.first_count;
                    } else {
                        self.print_chars("...");
                        p = l + self.prn.first_count - half_error_line + 3;
                        n = half_error_line;
                    }
                    for q in p..self.prn.first_count {
                        let c = i32::from(self.prn.trick_buf[(q % error_line) as usize]);
                        self.print_char(c);
                    }
                    self.print_ln();
                    for _ in 1..=n {
                        self.print_char(' ' as i32); // n spaces to begin line 2
                    }
                    let p2 = if m + n <= error_line {
                        self.prn.first_count + m
                    } else {
                        self.prn.first_count + (error_line - n - 3)
                    };
                    for q in self.prn.first_count..p2 {
                        let c = i32::from(self.prn.trick_buf[(q % error_line) as usize]);
                        self.print_char(c);
                    }
                    if m + n > error_line {
                        self.print_chars("...");
                    }
                    nn += 1;
                }
            } else if nn == ecl {
                self.print_nl_chars("...");
                nn += 1; // omitted if error_context_lines < 0
            }
            if bottom_line {
                break;
            }
            base_ptr -= 1;
        }
    }

    /// `open_log_file` (§534-§536): starts the transcript with the banner
    /// line — including the format ident, date and time — and an echo of
    /// the first input line.
    pub fn open_log_file(&mut self) -> TexResult<()> {
        if self.log_opened {
            return Ok(());
        }
        let old_setting = self.prn.selector;
        if self.job_name.is_none() {
            self.job_name = Some("texput".to_string());
        }
        self.prn.selector = LOG_ONLY;
        self.log_opened = true;
        // §536: the banner goes out through wlog — raw bytes that neither
        // wrap nor count toward file_offset (the rest of the line then
        // starts printing at offset 0, exactly as in tex.web).
        self.log.extend_from_slice(crate::BANNER.as_bytes());
        if self.format_ident.is_empty() {
            self.print_chars(" (INITEX)");
        } else {
            let ident = self.format_ident.clone();
            self.print_chars(&ident);
        }
        self.print_chars("  ");
        let day = self.eqtb.int_par(crate::eqtb::DAY_CODE);
        self.print_int(day);
        self.print_char(' ' as i32);
        let months = "JANFEBMARAPRMAYJUNJULAUGSEPOCTNOVDEC";
        let month = self.eqtb.int_par(crate::eqtb::MONTH_CODE).clamp(1, 12) as usize;
        self.print_chars(&months[3 * month - 3..3 * month]);
        self.print_char(' ' as i32);
        let year = self.eqtb.int_par(crate::eqtb::YEAR_CODE);
        self.print_int(year);
        self.print_char(' ' as i32);
        let time = self.eqtb.int_par(crate::eqtb::TIME_CODE);
        self.print_two(time / 60);
        self.print_char(':' as i32);
        self.print_two(time % 60);
        // etex.ch §536: announce extended mode, via wlog (raw bytes that
        // leave file_offset alone, like the banner).
        if self.etex_mode {
            self.log.push(b'\n');
            self.log.extend_from_slice(b"entering extended mode");
        }
        // §534: echo the first command line as "**...".
        self.print_nl_chars("**");
        let first_line = self.first_input_line.clone();
        self.print_chars(&first_line);
        self.print_ln();
        // log_only or term_and_log, §75-style (batch mode stays log-only).
        self.prn.selector = old_setting + 2;
        Ok(())
    }

    /// `new_interaction` (§1265).
    pub fn new_interaction(&mut self) {
        self.print_ln();
        self.interaction = self.cur_chr as u8;
        // §75: initialize the print selector based on interaction.
        self.prn.selector = if self.interaction == crate::input::BATCH_MODE {
            crate::print::NO_PRINT
        } else {
            crate::print::TERM_ONLY
        };
        if self.log_opened {
            self.prn.selector += 2;
        }
    }

    /// `fatal_error(s)` (§93): prints "! Emergency stop." with `s` as the
    /// help, then gives up (`succumb`). The interrupt carries `s` so the
    /// host can report the cause on exit.
    pub fn fatal_error(&mut self, s: &'static str) -> TexInterrupt {
        // normalize_selector (§92).
        self.prn.selector = if self.log_opened {
            crate::print::TERM_AND_LOG
        } else {
            crate::print::TERM_ONLY
        };
        if self.job_name.is_none() {
            // §92 calls open_log_file; the port defers logs to the host,
            // so just keep printing to the terminal.
        }
        if self.interaction == ERROR_STOP_MODE {
            self.interaction = SCROLL_MODE; // succumb: no more interaction
        }
        self.print_err("Emergency stop");
        self.help(&[s]);
        if self.log_opened {
            let _ = self.error(); // succumb: show context, log the help
        }
        self.history = History::FatalErrorStop;
        TexInterrupt::FatalError(s)
    }
}
