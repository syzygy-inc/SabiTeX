//! Expanding the next token.
//!
//! Ports tex.web Part 25 (§366-§401): `expand`, `insert_relax`,
//! `get_x_token`, `x_token` and `macro_call`.

use crate::cmds::*;
use crate::engine::Engine;
use crate::error::{TexInterrupt, TexResult};
use crate::input::*;
use crate::tokens::*;
use crate::types::{Pointer, NULL};

/// `marks_code` (etex.ch): added for `\topmarks` etc.
pub const MARKS_CODE: i32 = 5;

// §382: mark codes.
pub const TOP_MARK_CODE: usize = 0;
pub const FIRST_MARK_CODE: usize = 1;
pub const BOT_MARK_CODE: usize = 2;
pub const SPLIT_FIRST_MARK_CODE: usize = 3;
pub const SPLIT_BOT_MARK_CODE: usize = 4;

impl Engine {
    /// `expand` (§366): removes a "call" or a conditional or one of the
    /// other special operations; the next `get_next` will deliver the
    /// appropriate next token.
    pub fn expand(&mut self) -> TexResult<()> {
        // Save the global scanning values (§366).
        let cv_backup = self.cur_val;
        let cvl_backup = self.cur_val_level;
        let radix_backup = self.radix;
        let co_backup = self.cur_order;
        let bh = self.mem.backup_head();
        let backup_backup = self.mem.link(bh);
        'reswitch: loop {
            if self.cur_cmd >= CALL {
                if self.cur_cmd < END_TEMPLATE {
                    self.macro_call()?;
                } else {
                    // §375: insert a token containing frozen_endv.
                    self.cur_tok = CS_TOKEN_FLAG + self.eqtb.lay.frozen_endv;
                    self.back_input()?;
                }
                break;
            }
            // §367: expand a nonmacro.
            if self.eqtb.int_par(crate::eqtb::TRACING_COMMANDS_CODE) > 1 {
                self.show_cur_cmd_chr();
            }
            match self.cur_cmd {
                TOP_BOT_MARK => {
                    // §386 (+ etex.ch): insert the appropriate mark text,
                    // possibly for a nonzero mark class.
                    let t = (self.cur_chr % MARKS_CODE) as usize;
                    if self.cur_chr >= MARKS_CODE {
                        self.scan_register_num()?;
                    } else {
                        self.cur_val = 0;
                    }
                    let m = if self.cur_val == 0 {
                        self.cur_mark[t]
                    } else {
                        // Compute the mark pointer for class cur_val.
                        let v = self.cur_val;
                        self.find_sa_element(crate::sa::MARK_VAL, v, false)?;
                        if self.cur_ptr == NULL {
                            NULL
                        } else if t % 2 == 1 {
                            self.mem.link(self.cur_ptr + (t as i32 / 2) + 1)
                        } else {
                            self.mem.info(self.cur_ptr + (t as i32 / 2) + 1)
                        }
                    };
                    if m != NULL {
                        self.begin_token_list(m, MARK_TEXT)?;
                    }
                }
                EXPAND_AFTER if self.cur_chr == 0 => {
                    // §368: expand the token after the next token.
                    self.get_token()?;
                    let t = self.cur_tok;
                    self.get_token()?;
                    if self.cur_cmd > MAX_COMMAND {
                        self.expand()?;
                    } else {
                        self.back_input()?;
                    }
                    self.cur_tok = t;
                    self.back_input()?;
                }
                EXPAND_AFTER => {
                    // etex.ch: \unless negates the following boolean
                    // conditional.
                    self.get_token()?;
                    if self.cur_cmd == IF_TEST && self.cur_chr != crate::cond::IF_CASE_CODE {
                        self.cur_chr += crate::cond::UNLESS_CODE;
                        continue 'reswitch;
                    }
                    self.print_err("You can't use `");
                    self.print_esc_str("unless");
                    self.print_chars("' before `");
                    let (c, ch) = (self.cur_cmd, self.cur_chr);
                    self.print_cmd_chr(c, ch);
                    self.print_char('\'' as i32);
                    self.help(&["Continue, and I'll forget that it ever happened."]);
                    self.back_error()?;
                }
                NO_EXPAND => {
                    // §369: suppress expansion of the next token.
                    let save_scanner_status = self.inp.scanner_status;
                    self.inp.scanner_status = NORMAL_STATUS;
                    self.get_token()?;
                    self.inp.scanner_status = save_scanner_status;
                    let t = self.cur_tok;
                    self.back_input()?;
                    // now start and loc point to the backed-up token t
                    if t >= CS_TOKEN_FLAG {
                        let p = self.mem.get_avail()?;
                        let dx = CS_TOKEN_FLAG + self.eqtb.lay.frozen_dont_expand;
                        self.mem.set_info(p, dx);
                        let loc = self.inp.cur.loc;
                        self.mem.set_link(p, loc);
                        self.inp.cur.start = p;
                        self.inp.cur.loc = p;
                    }
                }
                CS_NAME => self.manufacture_csname()?,
                CONVERT => self.conv_toks()?,
                THE => self.ins_the_toks()?,
                IF_TEST => self.conditional()?,
                FI_OR_ELSE => {
                    // §510 (+ etex.ch \tracingifs).
                    if self.eqtb.int_par(crate::eqtb::TRACING_IFS_CODE) > 0
                        && self.eqtb.int_par(crate::eqtb::TRACING_COMMANDS_CODE) <= 1
                    {
                        self.show_cur_cmd_chr();
                    }
                    if self.cur_chr > i32::from(self.if_limit) {
                        if self.if_limit == crate::cond::IF_CODE {
                            self.insert_relax()?; // condition not yet evaluated
                        } else {
                            self.print_err("Extra ");
                            let c = self.cur_chr;
                            self.print_cmd_chr(FI_OR_ELSE, c);
                            self.help(&["I'm ignoring this; it doesn't match any \\if."]);
                            self.error()?;
                        }
                    } else {
                        while self.cur_chr != i32::from(crate::cond::FI_CODE) {
                            self.pass_text()?; // skip to \fi
                        }
                        self.pop_cond_stack();
                    }
                }
                INPUT => {
                    // §378 (+ etex.ch): initiate or terminate input.
                    if self.cur_chr == 2 {
                        self.pseudo_start()?; // \scantokens
                    } else if self.cur_chr > 0 {
                        self.inp.force_eof = true;
                    } else if self.name_in_progress {
                        self.insert_relax()?;
                    } else {
                        self.start_input()?;
                    }
                }
                _ => {
                    // §370: complain about an undefined macro.
                    self.print_err("Undefined control sequence");
                    self.help(&[
                        "The control sequence at the end of the top line",
                        "of your error message was never \\def'ed. If you have",
                        "misspelled it (e.g., `\\hobx'), type `I' and the correct",
                        "spelling (e.g., `I\\hbox'). Otherwise just continue,",
                        "and I'll forget about whatever was undefined.",
                    ]);
                    self.error()?;
                }
            }
            break;
        }
        self.cur_val = cv_backup;
        self.cur_val_level = cvl_backup;
        self.radix = radix_backup;
        self.cur_order = co_backup;
        let bh = self.mem.backup_head();
        self.mem.set_link(bh, backup_backup);
        Ok(())
    }

    /// §373: complain about a missing `\endcsname`.
    pub fn complain_missing_endcsname(&mut self) -> TexResult<()> {
        self.print_err("Missing ");
        self.print_esc_str("endcsname");
        self.print_chars(" inserted");
        self.help(&[
            "The control sequence marked <to be read again> should",
            "not appear between \\csname and \\endcsname.",
        ]);
        self.back_error()
    }

    /// §372-§374: `\csname` — manufacture a control sequence name.
    fn manufacture_csname(&mut self) -> TexResult<()> {
        let r = self.mem.get_avail()?;
        let mut p = r; // head of the list of characters
                       // pdftex.web: \ifincsname is true while this body is expanded.
        let saved_in_csname = self.in_csname;
        self.in_csname = true;
        loop {
            self.get_x_token()?;
            if self.cur_cs != 0 {
                break;
            }
            let q = self.mem.get_avail()?;
            self.mem.set_link(p, q);
            let t = self.cur_tok;
            self.mem.set_info(q, t);
            p = q;
        }
        if self.cur_cmd != END_CS_NAME {
            self.complain_missing_endcsname()?;
        }
        self.in_csname = saved_in_csname;
        // §374: look up the characters of list r in the hash table.
        let mut j = self.inp.first;
        let mut q = self.mem.link(r);
        while q != NULL {
            if j >= self.inp.max_buf_stack {
                self.inp.max_buf_stack = j + 1;
                if self.inp.max_buf_stack == self.inp.buf_size {
                    return Err(TexInterrupt::Overflow {
                        what: "buffer size",
                        size: self.inp.buf_size,
                    });
                }
            }
            self.inp.buffer[j as usize] = self.mem.info(q) % MAX_CHAR_VAL;
            j += 1;
            q = self.mem.link(q);
        }
        if j > self.inp.first + 1 {
            self.eqtb.no_new_control_sequence = false;
            let first = self.inp.first;
            self.cur_cs = self.id_lookup(first, j - first);
            self.eqtb.no_new_control_sequence = true;
        } else if j == self.inp.first {
            self.cur_cs = self.eqtb.lay.null_cs; // the list is empty
        } else {
            // the list has length one
            self.cur_cs = self.eqtb.lay.single_base + self.inp.buffer[self.inp.first as usize];
        }
        self.mem.flush_list(r);
        if self.eqtb.eq_type(self.cur_cs) == UNDEFINED_CS {
            // N.B.: the save_stack might change
            let cs = self.cur_cs;
            self.eq_define(cs, RELAX, crate::getnext::TOO_BIG_CHAR)?;
        }
        // the control sequence will now match \relax
        self.cur_tok = self.cur_cs + CS_TOKEN_FLAG;
        self.back_input()?;
        Ok(())
    }

    /// `insert_relax` (§379): inserts `\relax` after a too-far lookahead.
    pub fn insert_relax(&mut self) -> TexResult<()> {
        self.cur_tok = CS_TOKEN_FLAG + self.cur_cs;
        self.back_input()?;
        self.cur_tok = CS_TOKEN_FLAG + self.eqtb.lay.frozen_relax;
        self.back_input()?;
        self.inp.cur.index = INSERTED;
        Ok(())
    }

    /// `get_x_token` (§380): gets the next expanded token.
    pub fn get_x_token(&mut self) -> TexResult<()> {
        loop {
            self.get_next()?;
            if self.cur_cmd <= MAX_COMMAND {
                break;
            }
            if self.cur_cmd >= CALL {
                if self.cur_cmd < END_TEMPLATE {
                    self.macro_call()?;
                } else {
                    self.cur_cs = self.eqtb.lay.frozen_endv;
                    self.cur_cmd = ENDV;
                    break; // cur_chr = null_list
                }
            } else {
                self.expand()?;
            }
        }
        self.set_cur_tok();
        Ok(())
    }

    /// `x_token` (§381): `get_x_token` without the initial `get_next`.
    pub fn x_token(&mut self) -> TexResult<()> {
        while self.cur_cmd > MAX_COMMAND {
            self.expand()?;
            self.get_next()?;
        }
        self.set_cur_tok();
        Ok(())
    }

    /// `macro_call` (§389-§401): invokes a user-defined control sequence.
    pub fn macro_call(&mut self) -> TexResult<()> {
        let save_scanner_status = self.inp.scanner_status;
        let save_warning_index = self.inp.warning_index;
        self.inp.warning_index = self.cur_cs;
        let ref_count = self.cur_chr;
        let mut r = self.mem.link(ref_count);
        let mut n: usize = 0;
        let mut pstack: [Pointer; 9] = [NULL; 9];
        if self.eqtb.int_par(crate::eqtb::TRACING_MACROS_CODE) > 0 {
            // §401: show the text of the macro being expanded.
            self.begin_diagnostic();
            self.print_ln();
            let w = self.inp.warning_index;
            self.print_cs(w);
            self.token_show(ref_count);
            self.end_diagnostic(false);
        }
        // 'body replaces Pascal's `goto exit` cleanup discipline.
        'body: {
            // etex.ch §389: step over a \protected marker.
            if self.mem.info(r) == crate::tokens::PROTECTED_TOKEN {
                r = self.mem.link(r);
            }
            if self.mem.info(r) != END_MATCH_TOKEN {
                // §391: scan the parameters.
                self.inp.scanner_status = MATCHING;
                let mut unbalance: i32 = 0;
                self.long_state = self.eqtb.eq_type(self.cur_cs);
                if self.long_state >= OUTER_CALL {
                    self.long_state -= 2;
                }
                let mut m: i32 = 0;
                let mut p: Pointer = NULL;
                let mut rbrace_ptr: Pointer = NULL;
                let mut match_chr: i32 = 0;
                'params: loop {
                    let temp = self.mem.temp_head();
                    self.mem.set_link(temp, NULL);
                    let info_r = self.mem.info(r);
                    let s: Pointer = if !(MATCH_TOKEN..END_MATCH_TOKEN).contains(&info_r) {
                        NULL
                    } else {
                        match_chr = info_r - MATCH_TOKEN;
                        let s = self.mem.link(r);
                        r = s;
                        p = temp;
                        m = 0;
                        s
                    };
                    // §392: scan a parameter until its delimiter string has
                    // been found; or, if s = null, simply scan the delimiter.
                    'continue_: loop {
                        self.get_token()?;
                        if self.cur_tok == self.mem.info(r) {
                            // §394: advance r.
                            r = self.mem.link(r);
                            let ir = self.mem.info(r);
                            if (MATCH_TOKEN..=END_MATCH_TOKEN).contains(&ir) {
                                if self.cur_tok < LEFT_BRACE_LIMIT {
                                    self.inp.align_state -= 1;
                                }
                                break 'continue_; // found
                            }
                            continue 'continue_;
                        }
                        // §397: contribute the recently matched tokens.
                        if s != r {
                            if s == NULL {
                                // §398: report an improper use and abort.
                                self.print_err("Use of ");
                                let w = self.inp.warning_index;
                                self.sprint_cs(w);
                                self.print_chars(" doesn't match its definition");
                                self.help(&[
                                    "If you say, e.g., `\\def\\a1{...}', then you must always",
                                    "put `1' after `\\a', since control sequence names are",
                                    "made up of letters only. The macro here has not been",
                                    "followed by the required stuff, so I'm ignoring it.",
                                ]);
                                self.error()?;
                                break 'body;
                            } else {
                                let mut t = s;
                                loop {
                                    let it = self.mem.info(t);
                                    let q = self.mem.get_avail()?;
                                    self.mem.set_link(p, q);
                                    self.mem.set_info(q, it);
                                    p = q;
                                    m += 1;
                                    let mut u = self.mem.link(t);
                                    let mut v = s;
                                    let mut matched_continue = false;
                                    loop {
                                        if u == r {
                                            if self.cur_tok != self.mem.info(v) {
                                                break;
                                            }
                                            r = self.mem.link(v);
                                            matched_continue = true;
                                            break;
                                        }
                                        if self.mem.info(u) != self.mem.info(v) {
                                            break;
                                        }
                                        u = self.mem.link(u);
                                        v = self.mem.link(v);
                                    }
                                    if matched_continue {
                                        continue 'continue_;
                                    }
                                    t = self.mem.link(t);
                                    if t == r {
                                        break;
                                    }
                                }
                                r = s; // no tokens are recently matched
                            }
                        }
                        if self.cur_tok == self.par_token && self.long_state != LONG_CALL {
                            // §396: report a runaway argument and abort.
                            if self.long_state == CALL {
                                self.runaway();
                                self.print_err("Paragraph ended before ");
                                let w = self.inp.warning_index;
                                self.sprint_cs(w);
                                self.print_chars(" was complete");
                                self.help(&[
                                    "I suspect you've forgotten a `}', causing me to apply this",
                                    "control sequence to too much text. How can we recover?",
                                    "My plan is to forget the whole thing and hope for the best.",
                                ]);
                                self.back_error()?;
                            }
                            pstack[n] = self.mem.link(self.mem.temp_head());
                            self.inp.align_state -= unbalance;
                            for ps in pstack.iter().take(n + 1) {
                                self.mem.flush_list(*ps);
                            }
                            break 'body;
                        }
                        if self.cur_tok < RIGHT_BRACE_LIMIT {
                            if self.cur_tok < LEFT_BRACE_LIMIT {
                                // §399: contribute an entire group.
                                unbalance = 1;
                                loop {
                                    let q = self.mem.get_avail()?;
                                    self.mem.set_link(p, q);
                                    let ct = self.cur_tok;
                                    self.mem.set_info(q, ct);
                                    p = q;
                                    self.get_token()?;
                                    if self.cur_tok == self.par_token
                                        && self.long_state != LONG_CALL
                                    {
                                        // §396 again.
                                        if self.long_state == CALL {
                                            self.runaway();
                                            self.print_err("Paragraph ended before ");
                                            let w = self.inp.warning_index;
                                            self.sprint_cs(w);
                                            self.print_chars(" was complete");
                                            self.help(&[
                                                "I suspect you've forgotten a `}', causing me to apply this",
                                                "control sequence to too much text. How can we recover?",
                                                "My plan is to forget the whole thing and hope for the best.",
                                            ]);
                                            self.back_error()?;
                                        }
                                        pstack[n] = self.mem.link(self.mem.temp_head());
                                        self.inp.align_state -= unbalance;
                                        for ps in pstack.iter().take(n + 1) {
                                            self.mem.flush_list(*ps);
                                        }
                                        break 'body;
                                    }
                                    if self.cur_tok < RIGHT_BRACE_LIMIT {
                                        if self.cur_tok < LEFT_BRACE_LIMIT {
                                            unbalance += 1;
                                        } else {
                                            unbalance -= 1;
                                            if unbalance == 0 {
                                                break;
                                            }
                                        }
                                    }
                                }
                                rbrace_ptr = p;
                                let q = self.mem.get_avail()?;
                                self.mem.set_link(p, q);
                                let ct = self.cur_tok;
                                self.mem.set_info(q, ct);
                                p = q;
                            } else {
                                // §395: report an extra right brace.
                                self.back_input()?;
                                self.print_err("Argument of ");
                                let w = self.inp.warning_index;
                                self.sprint_cs(w);
                                self.print_chars(" has an extra }");
                                self.help(&[
                                    "I've run across a `}' that doesn't seem to match anything.",
                                    "For example, `\\def\\a#1{...}' and `\\a}' would produce",
                                    "this error. If you simply proceed now, the `\\par' that",
                                    "I've just inserted will cause me to report a runaway",
                                    "argument that might be the root of the problem. But if",
                                    "your `}' was spurious, just type `2' and it will go away.",
                                ]);
                                self.inp.align_state += 1;
                                self.long_state = CALL;
                                self.cur_tok = self.par_token;
                                self.ins_error()?;
                                continue 'continue_;
                            }
                        } else {
                            // §393: store the current token, unless it is a
                            // blank space that would become an undelimited
                            // parameter.
                            if self.cur_tok == SPACE_TOKEN {
                                let ir = self.mem.info(r);
                                if (MATCH_TOKEN..=END_MATCH_TOKEN).contains(&ir) {
                                    continue 'continue_;
                                }
                            }
                            let q = self.mem.get_avail()?;
                            self.mem.set_link(p, q);
                            let ct = self.cur_tok;
                            self.mem.set_info(q, ct);
                            p = q;
                        }
                        m += 1;
                        let ir = self.mem.info(r);
                        if ir > END_MATCH_TOKEN {
                            continue 'continue_;
                        }
                        if ir < MATCH_TOKEN {
                            continue 'continue_;
                        }
                        break 'continue_; // found
                    }
                    // §400: tidy up the parameter just scanned.
                    if s != NULL {
                        if m == 1 && self.mem.info(p) < RIGHT_BRACE_LIMIT {
                            // strip the enclosing braces
                            self.mem.set_link(rbrace_ptr, NULL);
                            self.mem.free_avail(p);
                            let th = self.mem.temp_head();
                            let p1 = self.mem.link(th);
                            pstack[n] = self.mem.link(p1);
                            self.mem.free_avail(p1);
                        } else {
                            pstack[n] = self.mem.link(self.mem.temp_head());
                        }
                        n += 1;
                        if self.eqtb.int_par(crate::eqtb::TRACING_MACROS_CODE) > 0 {
                            self.begin_diagnostic();
                            self.print_nl_chars("");
                            self.print_char_code(match_chr);
                            self.print_int(n as i32);
                            self.print_chars("<-");
                            self.show_token_list(pstack[n - 1], NULL, 1000);
                            self.end_diagnostic(false);
                        }
                    }
                    // now info(r) is a token whose command code is either
                    // match or end_match
                    if self.mem.info(r) == END_MATCH_TOKEN {
                        break 'params;
                    }
                }
            }
            // §390: feed the macro body and its parameters to the scanner.
            while self.inp.cur.state == TOKEN_LIST
                && self.inp.cur.loc == NULL
                && self.inp.cur.index != V_TEMPLATE
            {
                self.end_token_list()?; // conserve stack space
            }
            self.begin_token_list(ref_count, MACRO)?;
            self.inp.cur.name = self.inp.warning_index;
            self.inp.cur.loc = self.mem.link(r);
            if n > 0 {
                if self.inp.param_ptr + n > self.inp.max_param_stack {
                    self.inp.max_param_stack = self.inp.param_ptr + n;
                    if self.inp.max_param_stack > self.inp.param_size {
                        return Err(TexInterrupt::Overflow {
                            what: "parameter stack size",
                            size: self.inp.param_size as i32,
                        });
                    }
                }
                for (m0, ps) in pstack.iter().take(n).enumerate() {
                    self.inp.param_stack[self.inp.param_ptr + m0] = *ps;
                }
                self.inp.param_ptr += n;
            }
        }
        self.inp.scanner_status = save_scanner_status;
        self.inp.warning_index = save_warning_index;
        Ok(())
    }
}
