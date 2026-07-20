//! Extensions: `\openout`, `\write`, `\closeout`, `\special`, `\immediate`,
//! `\setlanguage`.
//!
//! Ports the whatsit portions of tex.web Part 53 (§1340-§1379). The
//! `\language` whatsit lives in `par.rs`; this module covers the output
//! streams and their interaction with `ship_out`.

use crate::cmds::*;
use crate::engine::{Engine, HMODE};
use crate::error::TexResult;
use crate::nodes::*;
use crate::par::{CLOSE_NODE, LANGUAGE_NODE, OPEN_NODE, SPECIAL_NODE, WRITE_NODE};
use crate::print::{LOG_ONLY, NEW_STRING, TERM_AND_LOG};
use crate::tokens::{CS_TOKEN_FLAG, LEFT_BRACE_TOKEN, RIGHT_BRACE_TOKEN};
use crate::types::{Pointer, NULL};

// §1341: the \immediate and \setlanguage chr codes follow the node types.
pub const IMMEDIATE_CODE: i32 = 4;
pub const SET_LANGUAGE_CODE: i32 = 5;

// §1342: whatsit node sizes.
pub const OPEN_NODE_SIZE: i32 = 3;
pub const WRITE_NODE_SIZE: i32 = 2;

impl Engine {
    /// `do_extension` (§1348).
    pub fn do_extension(&mut self) -> TexResult<()> {
        match self.cur_chr {
            c if c == i32::from(OPEN_NODE) => {
                // §1351.
                let p = self.new_write_whatsit(OPEN_NODE_SIZE)?;
                self.scan_optional_equals()?;
                self.scan_file_name()?;
                let name = std::mem::take(&mut self.cur_name);
                let s = self.strings.intern(&name)?;
                self.mem.set_link(p + 1, s); // open_name
                self.mem.set_info(p + 2, 0); // open_area (folded into the name)
                self.mem.set_link(p + 2, 0); // open_ext
            }
            c if c == i32::from(WRITE_NODE) => {
                // §1352.
                let k = self.cur_cs;
                let p = self.new_write_whatsit(WRITE_NODE_SIZE)?;
                self.cur_cs = k;
                self.scan_toks(false, false)?;
                let dr = self.inp.def_ref;
                self.mem.set_link(p + 1, dr); // write_tokens
            }
            c if c == i32::from(CLOSE_NODE) => {
                // §1353.
                let p = self.new_write_whatsit(WRITE_NODE_SIZE)?;
                self.mem.set_link(p + 1, NULL);
            }
            c if c == i32::from(crate::par::SAVE_POS_NODE) => {
                // pdftex.web: a position probe, resolved at ship-out.
                let p =
                    self.new_whatsit(crate::par::SAVE_POS_NODE, crate::nodes::SMALL_NODE_SIZE)?;
                let _ = p;
            }
            c if c == i32::from(SPECIAL_NODE) => {
                // §1354.
                let p = self.new_whatsit(SPECIAL_NODE, WRITE_NODE_SIZE)?;
                self.mem.set_info(p + 1, NULL); // write_stream
                self.scan_toks(false, true)?;
                let dr = self.inp.def_ref;
                self.mem.set_link(p + 1, dr);
            }
            IMMEDIATE_CODE => {
                // §1375.
                self.get_x_token()?;
                if self.cur_cmd == EXTENSION && self.cur_chr <= i32::from(CLOSE_NODE) {
                    let p = self.nest.cur.tail;
                    self.do_extension()?; // append a whatsit node
                    let t = self.nest.cur.tail;
                    self.out_what(t)?; // do the action immediately
                    self.flush_node_list(t);
                    self.nest.cur.tail = p;
                    self.mem.set_link(p, NULL);
                } else {
                    self.back_input()?;
                }
            }
            SET_LANGUAGE_CODE => {
                // §1377.
                if self.mode().abs() != HMODE {
                    self.report_illegal_case()?;
                } else {
                    let p = self.new_whatsit(LANGUAGE_NODE, SMALL_NODE_SIZE)?;
                    self.scan_int()?;
                    let l = if self.cur_val <= 0 || self.cur_val > 255 {
                        0
                    } else {
                        self.cur_val
                    };
                    self.set_clang(l);
                    self.mem.set_link(p + 1, l); // what_lang
                    let lhm =
                        crate::hyph::norm_min(self.eqtb.int_par(crate::eqtb::LEFT_HYPHEN_MIN_CODE));
                    let rhm = crate::hyph::norm_min(
                        self.eqtb.int_par(crate::eqtb::RIGHT_HYPHEN_MIN_CODE),
                    );
                    self.mem.word_mut(p + 1).set_b0(lhm as u16);
                    self.mem.word_mut(p + 1).set_b1(rhm as u16);
                }
            }
            _ => {
                return self.confusion("ext1");
            }
        }
        Ok(())
    }

    /// `new_write_whatsit(w)` (§1350).
    fn new_write_whatsit(&mut self, w: i32) -> TexResult<Pointer> {
        let s = self.cur_chr as u16;
        let p = self.new_whatsit(s, w)?;
        if w != WRITE_NODE_SIZE {
            self.scan_four_bit_int()?;
        } else {
            self.scan_int()?;
            if self.cur_val < 0 {
                self.cur_val = 17;
            } else if self.cur_val > 15 {
                self.cur_val = 16;
            }
        }
        let v = self.cur_val;
        self.mem.set_info(p + 1, v); // write_stream
        Ok(p)
    }

    /// `write_out(p)` (§1370-§1372).
    pub fn write_out(&mut self, p: Pointer) -> TexResult<()> {
        // §1371: expand macros in the token list; link(def_ref) is the result.
        let q = self.mem.get_avail()?;
        self.mem.set_info(q, RIGHT_BRACE_TOKEN + '}' as i32);
        let r = self.mem.get_avail()?;
        self.mem.set_link(q, r);
        let ew = CS_TOKEN_FLAG + self.eqtb.lay.end_write;
        self.mem.set_info(r, ew); // end_write_token
        self.ins_list(q)?;
        let wt = self.mem.link(p + 1); // write_tokens
        self.begin_token_list(wt, crate::input::WRITE_TEXT)?;
        let q = self.mem.get_avail()?;
        self.mem.set_info(q, LEFT_BRACE_TOKEN + '{' as i32);
        self.ins_list(q)?;
        // now we have copied the token list
        let old_mode = self.nest.cur.mode;
        self.nest.cur.mode = 0; // disable \prevdepth, \spacefactor, ...
        self.cur_cs = self.write_loc;
        self.scan_toks(false, true)?; // expand macros, etc.
        self.get_token()?;
        if self.cur_tok != ew {
            // §1372: recover from an unbalanced write command.
            self.print_err("Unbalanced write command");
            self.help(&[
                "On this page there's a \\write with fewer real {'s than }'s.",
                "I can't handle that very well; good luck.",
            ]);
            self.error()?;
            loop {
                self.get_token()?;
                if self.cur_tok == ew {
                    break;
                }
            }
        }
        self.nest.cur.mode = old_mode;
        self.end_token_list()?; // conserve stack space
                                // §1370: print the token list.
        let old_setting = self.prn.selector;
        let j = self.mem.info(p + 1); // write_stream
        if (0..16).contains(&j) && self.write_open[j as usize] {
            self.prn.selector = j as u8;
        } else {
            // write to the terminal if file isn't open
            if j == 17 && self.prn.selector == TERM_AND_LOG {
                self.prn.selector = LOG_ONLY;
            }
            self.print_nl_chars("");
        }
        let dr = self.inp.def_ref;
        self.token_show(dr);
        self.print_ln();
        let dr = self.inp.def_ref;
        self.mem.flush_list(dr);
        self.prn.selector = old_setting;
        Ok(())
    }

    /// `special_out(p)` (§1368).
    pub fn special_out(&mut self, p: Pointer) -> TexResult<()> {
        self.synch_h()?;
        self.synch_v()?;
        let old_setting = self.prn.selector;
        self.prn.selector = NEW_STRING;
        let wt = self.mem.link(p + 1);
        let lk = self.mem.link(wt);
        self.show_token_list(lk, NULL, (self.sizes.pool_size as i32) / 2);
        self.prn.selector = old_setting;
        // §1368 writes the string as raw bytes; characters 0..255 stay
        // single bytes (TRIP uses ^^80). Wider USVs go out as UTF-8 until
        // the XeTeX layer (M7) revisits \special encoding.
        let mut bytes: Vec<u8> = Vec::new();
        for c in char::decode_utf16(self.strings.take_cur_string())
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        {
            if (c as u32) < 256 {
                bytes.push(c as u32 as u8);
            } else {
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        let bytes = &bytes[..];
        if bytes.len() < 256 {
            self.dvi.out(crate::dvi::XXX1);
            self.dvi.out(bytes.len() as u8);
        } else {
            self.dvi.out(crate::dvi::XXX4);
            self.dvi.four(bytes.len() as i32);
        }
        for &b in bytes {
            self.dvi.out(b);
        }
        Ok(())
    }

    /// `out_what(p)` (§1373-§1374): run a whatsit's action during shipout.
    pub fn out_what(&mut self, p: Pointer) -> TexResult<()> {
        match self.mem.subtype(p) {
            OPEN_NODE | WRITE_NODE | CLOSE_NODE => {
                // §1374: queued-up \write work.
                if !self.dvi.doing_leaders {
                    let j = self.mem.info(p + 1); // write_stream
                    if self.mem.subtype(p) == WRITE_NODE {
                        self.write_out(p)?;
                    } else {
                        if (0..16).contains(&j) && self.write_open[j as usize] {
                            self.close_write_stream(j as usize);
                        }
                        if self.mem.subtype(p) == CLOSE_NODE {
                            if (0..18).contains(&j) {
                                self.write_open[j as usize] = false;
                            }
                        } else if (0..16).contains(&j) {
                            let s = self.mem.link(p + 1); // open_name
                            let mut name: String =
                                char::decode_utf16(self.strings.str(s).iter().copied())
                                    .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                                    .collect();
                            if !name.contains('.') {
                                name.push_str(".tex");
                            }
                            // §1374 `while not a_open_out(...)`: TeX opens
                            // (and truncates) the file here; while that
                            // fails it prompts for another name — which in
                            // batch/nonstop mode is a fatal error (§530).
                            while !self.fs.write_file(&name, crate::io::OutKind::OpenOut, &[]) {
                                name = self.prompt_output_file_name(&name)?;
                            }
                            self.write_file_name[j as usize] = name;
                            self.write_buf[j as usize].clear();
                            self.write_open[j as usize] = true;
                        }
                    }
                }
                Ok(())
            }
            SPECIAL_NODE => self.special_out(p),
            crate::par::SAVE_POS_NODE => {
                // pdfTeX coordinates: sp from the paper's lower-left
                // corner, including the classic 1in origin offset.
                const ONE_INCH: i32 = 4_736_286; // pdftex-measured
                let page_h = {
                    let v = self.eqtb.dimen_par(crate::eqtb::PDF_PAGE_HEIGHT_CODE);
                    if v > 0 {
                        v
                    } else {
                        // A4 height (TeX Live pdftex default paper),
                        // pdftex-measured: 297mm = 845.0466pt.
                        55_380_990
                    }
                };
                self.last_x_pos = self.dvi.cur_h + ONE_INCH;
                self.last_y_pos = page_h - (self.dvi.cur_v + ONE_INCH);
                Ok(())
            }
            LANGUAGE_NODE => Ok(()),
            _ => self.confusion("ext4"),
        }
    }

    /// `a_close(write_file[j])`: hand the buffered stream to the host.
    pub fn close_write_stream(&mut self, j: usize) {
        let name = std::mem::take(&mut self.write_file_name[j]);
        let buf = std::mem::take(&mut self.write_buf[j]);
        if !name.is_empty() {
            self.fs.write_file(&name, crate::io::OutKind::OpenOut, &buf);
        }
    }

    /// `print_write_whatsit(s, p)` (§1355).
    fn print_write_whatsit(&mut self, s: &str, p: Pointer) {
        self.print_esc_str(s);
        let w = self.mem.info(p + 1); // write_stream
        if w < 16 {
            self.print_int(w);
        } else if w == 16 {
            self.print_char('*' as i32);
        } else {
            self.print_char('-' as i32);
        }
    }

    /// §1356: display the whatsit node `p` (called from `show_box`).
    pub fn show_whatsit(&mut self, p: Pointer) {
        // xetex.web §1356 additions: native words and glyphs.
        if self.mem.is_native_word_node(p) {
            let f = self.mem.native_font(p);
            self.print_esc_str("");
            let fid = self.eqtb.lay.font_id_base + f;
            let t = self.eqtb.text(fid);
            if t > 0 {
                self.print(t);
            } else {
                self.print_chars("FONT");
                self.print_int(f);
            }
            self.print_char(' ' as i32);
            let text = self.native_text(p);
            for ch in text.chars() {
                self.print_char(ch as i32);
            }
            return;
        }
        if self.mem.is_glyph_node(p) {
            let f = self.mem.native_font(p);
            self.print_esc_str("");
            let fid = self.eqtb.lay.font_id_base + f;
            let t = self.eqtb.text(fid);
            if t > 0 {
                self.print(t);
            } else {
                self.print_chars("FONT");
                self.print_int(f);
            }
            self.print_chars(" glyph#");
            let g = self.mem.native_length(p); // native_glyph
            self.print_int(g);
            return;
        }
        match self.mem.subtype(p) {
            OPEN_NODE => {
                self.print_write_whatsit("openout", p);
                self.print_char('=' as i32);
                let s = self.mem.link(p + 1);
                self.slow_print(s);
            }
            WRITE_NODE => {
                self.print_write_whatsit("write", p);
                let wt = self.mem.link(p + 1);
                self.print_mark(wt);
            }
            CLOSE_NODE => self.print_write_whatsit("closeout", p),
            SPECIAL_NODE => {
                self.print_esc_str("special");
                let wt = self.mem.link(p + 1);
                self.print_mark(wt);
            }
            crate::par::SAVE_POS_NODE => {
                self.print_esc_str("pdfsavepos");
            }
            LANGUAGE_NODE => {
                self.print_esc_str("setlanguage");
                let l = self.mem.link(p + 1);
                self.print_int(l);
                self.print_chars(" (hyphenmin ");
                let lhm = i32::from(self.mem.word(p + 1).b0());
                self.print_int(lhm);
                self.print_char(',' as i32);
                let rhm = i32::from(self.mem.word(p + 1).b1());
                self.print_int(rhm);
                self.print_char(')' as i32);
            }
            _ => self.print_chars("whatsit?"),
        }
    }

    /// §1357: make a copy of whatsit `p` into `r`; returns the node size.
    pub fn copy_whatsit(&mut self, p: Pointer) -> TexResult<(Pointer, i32)> {
        // xetex.web: native whatsits are variable size, and the glyph
        // records in the side table must be duplicated with the node.
        if self.mem.is_native_word_node(p) {
            let sz = self.mem.native_size(p);
            let r = self.mem.get_node(sz)?;
            for k in 0..sz {
                *self.mem.word_mut(r + k) = self.mem.word(p + k);
            }
            self.copy_native_glyph_info(p, r);
            return Ok((r, 0));
        }
        if self.mem.is_glyph_node(p) {
            let r = self.mem.get_node(crate::native::GLYPH_NODE_SIZE)?;
            for k in 0..crate::native::GLYPH_NODE_SIZE {
                *self.mem.word_mut(r + k) = self.mem.word(p + k);
            }
            return Ok((r, 0));
        }
        match self.mem.subtype(p) {
            OPEN_NODE => {
                let r = self.mem.get_node(OPEN_NODE_SIZE)?;
                Ok((r, OPEN_NODE_SIZE))
            }
            WRITE_NODE | SPECIAL_NODE => {
                let r = self.mem.get_node(WRITE_NODE_SIZE)?;
                let wt = self.mem.link(p + 1);
                self.add_token_ref(wt);
                Ok((r, WRITE_NODE_SIZE))
            }
            CLOSE_NODE | LANGUAGE_NODE | crate::par::SAVE_POS_NODE => {
                let r = self.mem.get_node(SMALL_NODE_SIZE)?;
                Ok((r, SMALL_NODE_SIZE))
            }
            _ => self.confusion("ext2").map(|_| (NULL, 0)),
        }
    }

    /// §1358: wipe out whatsit `p` (called from `flush_node_list`).
    pub fn free_whatsit(&mut self, p: Pointer) {
        match self.mem.subtype(p) {
            OPEN_NODE => self.mem.free_node(p, OPEN_NODE_SIZE),
            WRITE_NODE | SPECIAL_NODE => {
                let wt = self.mem.link(p + 1);
                self.delete_token_ref(wt);
                self.mem.free_node(p, WRITE_NODE_SIZE);
            }
            CLOSE_NODE | LANGUAGE_NODE => self.mem.free_node(p, SMALL_NODE_SIZE),
            crate::native::NATIVE_WORD_NODE
            | crate::native::NATIVE_WORD_NODE_AT
            | crate::native::GLYPH_NODE => self.free_native_node(p),
            _ => {
                // confusion("ext3") — but destructors must not error.
                self.mem.free_node(p, SMALL_NODE_SIZE);
            }
        }
    }
}
