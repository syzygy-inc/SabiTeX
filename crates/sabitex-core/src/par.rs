//! Building paragraphs and vertical-list material.
//!
//! Ports the paragraph portions of tex.web Part 47 (§1090-§1110:
//! `new_graf`, `indent_in_hmode`, `head_for_vmode`, `end_graf`,
//! `begin_insert_or_adjust`, `make_mark`, `delete_last`, `unpackage`)
//! plus the `\language` whatsit machinery (§1341, §1376-§1377).

use crate::cmds::*;
use crate::engine::{Engine, HMODE, MMODE, VMODE};
use crate::eqtb::*;
use crate::error::TexResult;
use crate::hyph::norm_min;
use crate::nodes::*;
use crate::types::{Pointer, MAX_HALFWORD, NULL};

// §1341: subtype codes for whatsit nodes.
pub const OPEN_NODE: u16 = 0;
pub const WRITE_NODE: u16 = 1;
pub const CLOSE_NODE: u16 = 2;
pub const SPECIAL_NODE: u16 = 3;
pub const LANGUAGE_NODE: u16 = 4;
/// pdfTeX \pdfsavepos whatsit (also in xetex): ship-out position probe.
pub const SAVE_POS_NODE: u16 = 6;

impl Engine {
    /// `new_whatsit(s, w)` (§1349).
    pub fn new_whatsit(&mut self, s: u16, w: i32) -> TexResult<Pointer> {
        let p = self.mem.get_node(w)?;
        self.mem.set_node_type(p, WHATSIT_NODE);
        self.mem.set_subtype(p, s);
        self.tail_append(p);
        Ok(p)
    }

    /// `fix_language` (§1377): append a language whatsit if `\language`
    /// differs from the list's current language.
    pub fn fix_language(&mut self) -> TexResult<()> {
        let language = self.eqtb.int_par(LANGUAGE_CODE);
        let l = if (0..=255).contains(&language) && language > 0 {
            language
        } else {
            0
        };
        if l != self.clang() {
            let p = self.new_whatsit(LANGUAGE_NODE, SMALL_NODE_SIZE)?;
            self.mem.set_link(p + 1, l); // what_lang
            self.set_clang(l);
            let lhm = norm_min(self.eqtb.int_par(LEFT_HYPHEN_MIN_CODE));
            let rhm = norm_min(self.eqtb.int_par(RIGHT_HYPHEN_MIN_CODE));
            self.mem.word_mut(p + 1).set_b0(lhm as u16); // what_lhm
            self.mem.word_mut(p + 1).set_b1(rhm as u16); // what_rhm
        }
        Ok(())
    }

    /// `new_graf(indented)` (§1091).
    pub fn new_graf(&mut self, indented: bool) -> TexResult<()> {
        self.nest.cur.pg = 0;
        if self.mode() == VMODE || self.mem.link(self.nest.cur.head) != NULL {
            let g = self.new_param_glue(PAR_SKIP_CODE)?;
            self.tail_append(g);
        }
        self.push_nest()?;
        self.nest.cur.mode = HMODE;
        self.set_space_factor(1000);
        // set_cur_lang (§934).
        let language = self.eqtb.int_par(LANGUAGE_CODE);
        let cur_lang = if language <= 0 || language > 255 {
            0
        } else {
            language
        };
        self.hy.cur_lang = cur_lang;
        self.set_clang(cur_lang);
        self.nest.cur.pg = (norm_min(self.eqtb.int_par(LEFT_HYPHEN_MIN_CODE)) * 0o100
            + norm_min(self.eqtb.int_par(RIGHT_HYPHEN_MIN_CODE)))
            * 0o200000
            + cur_lang;
        if indented {
            let b = self.new_null_box()?;
            self.mem.set_link(self.nest.cur.head, b);
            self.nest.cur.tail = b;
            let pi = self.eqtb.dimen_par(PAR_INDENT_CODE);
            self.mem.set_width(b, pi);
        }
        let ep = self.eqtb.equiv(self.eqtb.lay.local_base + EVERY_PAR_OFFSET);
        if ep != NULL {
            self.begin_token_list(ep, crate::input::EVERY_PAR_TEXT)?;
        }
        if self.nest.ptr == 1 {
            self.build_page()?; // put the \parskip glue on the current page
        }
        Ok(())
    }

    /// `indent_in_hmode` (§1093): `\indent`/`\noindent` in (restricted)
    /// horizontal or math mode.
    pub fn indent_in_hmode(&mut self) -> TexResult<()> {
        if self.cur_chr > 0 {
            let mut p = self.new_null_box()?;
            let pi = self.eqtb.dimen_par(PAR_INDENT_CODE);
            self.mem.set_width(p, pi);
            if self.mode().abs() == HMODE {
                self.set_space_factor(1000);
            } else {
                let q = self.new_noad()?;
                self.mem.set_math_type(q + 1, crate::math::SUB_BOX);
                self.mem.set_info(q + 1, p);
                p = q;
            }
            self.tail_append(p);
        }
        Ok(())
    }

    /// `head_for_vmode` (§1094): a vertical command in horizontal mode.
    pub fn head_for_vmode(&mut self) -> TexResult<()> {
        if self.mode() < 0 {
            if self.cur_cmd != HRULE {
                self.off_save()
            } else {
                self.print_err("You can't use `");
                self.print_esc_str("hrule");
                self.print_chars("' here except with leaders");
                self.help(&[
                    "To put a horizontal rule in an hbox or an alignment,",
                    "you should use \\leaders or \\hrulefill (see The TeXbook).",
                ]);
                self.error()
            }
        } else {
            self.back_input()?;
            self.cur_tok = self.par_token;
            self.back_input()?;
            self.inp.cur.index = crate::input::INSERTED;
            Ok(())
        }
    }

    /// `end_graf` (§1096).
    pub fn end_graf(&mut self) -> TexResult<()> {
        if self.mode() == HMODE {
            if self.nest.cur.head == self.nest.cur.tail {
                self.pop_nest(); // null paragraphs are discarded
            } else {
                self.line_break(false)?;
                // etex.ch (§1096): no display follows; drop the LR stack
                // that line_break saved into the enclosing list.
                if self.nest.cur.etex_aux != NULL {
                    let lr = self.nest.cur.etex_aux;
                    self.mem.flush_list(lr);
                    self.nest.cur.etex_aux = NULL;
                }
            }
            self.normal_paragraph()?;
            self.error_count = 0;
        }
        Ok(())
    }

    /// `begin_insert_or_adjust` (§1099).
    pub fn begin_insert_or_adjust(&mut self) -> TexResult<()> {
        if self.cur_cmd == VADJUST {
            self.cur_val = 255;
        } else {
            self.scan_eight_bit_int()?;
            if self.cur_val == 255 {
                self.print_err("You can't ");
                self.print_esc_str("insert");
                self.print_int(255);
                self.help(&["I'm changing to \\insert0; box 255 is special."]);
                self.error()?;
                self.cur_val = 0;
            }
        }
        let v = self.cur_val;
        self.save.set_saved(0, v);
        self.save.save_ptr += 1;
        self.new_save_level(INSERT_GROUP)?;
        self.scan_left_brace()?;
        self.normal_paragraph()?;
        self.push_nest()?;
        self.nest.cur.mode = -VMODE;
        self.set_prev_depth(crate::nest::IGNORE_DEPTH);
        Ok(())
    }

    /// §1100: wrap up an insertion or `\vadjust` group (called from
    /// `handle_right_brace`).
    pub fn finish_insert_or_adjust(&mut self) -> TexResult<()> {
        self.end_graf()?;
        let q = self.eqtb.glue_par(SPLIT_TOP_SKIP_CODE);
        self.mem.add_glue_ref(q);
        let d = self.eqtb.dimen_par(SPLIT_MAX_DEPTH_CODE);
        let f = self.eqtb.int_par(FLOATING_PENALTY_CODE);
        self.unsave()?;
        self.save.save_ptr -= 1;
        let n = self.save.saved(0); // the insertion number, or 255 for \vadjust
        let h = self.nest.cur.head;
        let l = self.mem.link(h);
        let p = self.vpack(l, 0, crate::pack::ADDITIONAL)?;
        self.pop_nest();
        if n < 255 {
            let t = self.mem.get_node(INS_NODE_SIZE)?;
            self.tail_append(t);
            self.mem.set_node_type(t, INS_NODE);
            self.mem.set_subtype(t, n as u16);
            let hgt = self.mem.height(p) + self.mem.depth(p);
            self.mem.set_height(t, hgt);
            let lp = self.mem.list_ptr(p);
            self.mem.set_ins_ptr(t, lp);
            self.mem.set_split_top_ptr(t, q);
            self.mem.set_depth(t, d);
            self.mem.set_float_cost(t, f);
        } else {
            let t = self.mem.get_node(SMALL_NODE_SIZE)?;
            self.tail_append(t);
            self.mem.set_node_type(t, ADJUST_NODE);
            self.mem.set_subtype(t, 0);
            let lp = self.mem.list_ptr(p);
            self.mem.set_adjust_ptr(t, lp); // §142
            self.mem.delete_glue_ref(q);
        }
        self.mem.free_node(p, BOX_NODE_SIZE);
        if self.nest.ptr == 0 {
            self.build_page()?;
        }
        Ok(())
    }

    /// `make_mark` (§1101 + etex.ch: `\marks` carries a mark class).
    pub fn make_mark(&mut self) -> TexResult<()> {
        let c = if self.cur_chr == 0 {
            0
        } else {
            self.scan_register_num()?;
            self.cur_val
        };
        self.scan_toks(false, true)?;
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_mark_class(p, c);
        self.mem.set_node_type(p, MARK_NODE);
        self.mem.set_subtype(p, 0);
        let dr = self.inp.def_ref;
        self.mem.set_mark_ptr(p, dr);
        self.tail_append(p);
        Ok(())
    }

    /// `find_effective_tail` (etex.ch): the tail of the current list,
    /// looking through a final \endM math node.
    pub(crate) fn find_effective_tail(&self) -> Pointer {
        use crate::nodes::{END_M_CODE, MATH_NODE};
        let mut tx = self.nest.cur.tail;
        if !self.mem.is_char_node(tx)
            && self.mem.node_type(tx) == MATH_NODE
            && self.mem.subtype(tx) == END_M_CODE
        {
            let mut r = self.nest.cur.head;
            let mut q = r;
            while r != tx {
                q = r;
                r = self.mem.link(q);
            }
            tx = q;
        }
        tx
    }

    /// `fetch_effective_tail` (etex.ch): unlinks `tx` from the current
    /// list, dropping a final \beginM \endM pair. Returns `None` when `tx`
    /// belongs to a discretionary replacement list (nothing is removed).
    pub(crate) fn fetch_effective_tail(&mut self, tx: Pointer) -> TexResult<Option<Pointer>> {
        use crate::nodes::{BEGIN_M_CODE, DISC_NODE, MATH_NODE};
        let head = self.nest.cur.head;
        let mut q = head;
        let mut p = NULL;
        let mut r;
        let mut fm;
        loop {
            r = p;
            p = q;
            fm = false;
            if !self.mem.is_char_node(q) {
                let t = self.mem.node_type(q);
                if t == DISC_NODE {
                    for _ in 1..=self.mem.replace_count(q) {
                        p = self.mem.link(p);
                    }
                    if p == tx {
                        return Ok(None); // sequences from discretionaries stay
                    }
                } else if t == MATH_NODE && self.mem.subtype(q) == BEGIN_M_CODE {
                    fm = true;
                }
            }
            q = self.mem.link(p);
            if q == tx {
                break;
            }
        }
        // found: r -> p -> q == tx
        let q2 = self.mem.link(tx);
        self.mem.set_link(p, q2);
        self.mem.set_link(tx, NULL);
        if q2 == NULL {
            if fm {
                return self.confusion("tail1").map(|_| None);
            }
            self.nest.cur.tail = p;
        } else if fm {
            // r -> p == \beginM -> q2 == \endM: drop the pair.
            self.nest.cur.tail = r;
            self.mem.set_link(r, NULL);
            self.flush_node_list(p);
        }
        Ok(Some(tx))
    }

    /// `delete_last` (§1105-§1106): `\unpenalty`, `\unkern`, `\unskip`.
    pub fn delete_last(&mut self) -> TexResult<()> {
        if self.mode() == VMODE && self.nest.cur.tail == self.nest.cur.head {
            // §1106: apologize, unless \unskip follows non-glue.
            if self.cur_chr != i32::from(GLUE_NODE) || self.last_glue != MAX_HALFWORD {
                self.you_cant();
                let second = if self.cur_chr == i32::from(KERN_NODE) {
                    "Try `I\\kern-\\lastkern' instead."
                } else if self.cur_chr != i32::from(GLUE_NODE) {
                    "Perhaps you can make the output routine do it."
                } else {
                    "Try `I\\vskip-\\lastskip' instead."
                };
                self.help(&[
                    "Sorry...I usually can't take things from the current page.",
                    second,
                ]);
                self.error()?;
            }
            return Ok(());
        }
        // etex.ch §1105: work on the effective tail, transparent to a
        // final \beginM \endM pair.
        let tx = self.find_effective_tail();
        if !self.mem.is_char_node(tx) && i32::from(self.mem.node_type(tx)) == self.cur_chr {
            if let Some(tx) = self.fetch_effective_tail(tx)? {
                self.flush_node_list(tx);
            }
        }
        Ok(())
    }

    /// `append_italic_correction` (§1113): `\/`.
    pub fn append_italic_correction(&mut self) -> TexResult<()> {
        let tail = self.nest.cur.tail;
        if tail != self.nest.cur.head {
            let p = if self.mem.is_char_node(tail) {
                tail
            } else if self.mem.node_type(tail) == LIGATURE_NODE {
                self.mem.lig_char(tail)
            } else {
                return Ok(());
            };
            let f = i32::from(self.mem.font(p));
            let i = self.fonts.char_info(f, i32::from(self.mem.character(p)));
            let k = self.new_kern(self.fonts.char_italic(f, i))?;
            self.tail_append(k);
            let t = self.nest.cur.tail;
            self.mem.set_subtype(t, EXPLICIT);
        }
        Ok(())
    }

    /// `make_accent` (§1123-§1126): `\accent`.
    pub fn make_accent(&mut self) -> TexResult<()> {
        self.scan_char_num()?;
        let mut f = self.eqtb.cur_font();
        let mut p = self.new_character(f, self.cur_val)?;
        if p != NULL {
            let x = self.fonts.x_height(f);
            let s = f64::from(self.fonts.param(crate::fonts::SLANT_CODE, f)) / 65536.0;
            let a = self
                .fonts
                .char_width(f, self.fonts.char_info(f, i32::from(self.mem.character(p))));
            self.do_assignments()?;
            // §1124: create a character node q for the next character.
            let mut q = NULL;
            f = self.eqtb.cur_font();
            if self.cur_cmd == LETTER || self.cur_cmd == OTHER_CHAR || self.cur_cmd == CHAR_GIVEN {
                q = self.new_character(f, self.cur_chr)?;
            } else if self.cur_cmd == CHAR_NUM {
                self.scan_char_num()?;
                q = self.new_character(f, self.cur_val)?;
            } else {
                self.back_input()?;
            }
            if q != NULL {
                // §1125: append the accent with appropriate kerns.
                let t = f64::from(self.fonts.param(crate::fonts::SLANT_CODE, f)) / 65536.0;
                let i = self.fonts.char_info(f, i32::from(self.mem.character(q)));
                let w = self.fonts.char_width(f, i);
                let h = self
                    .fonts
                    .char_height(f, crate::fonts::FontMem::height_depth(i));
                if h != x {
                    // §1126: box the accent if it isn't at the right height.
                    p = self.hpack(p, 0, crate::pack::ADDITIONAL)?;
                    self.mem.set_shift_amount(p, x - h);
                }
                let delta =
                    (f64::from(w - a) / 2.0 + f64::from(h) * t - f64::from(x) * s).round() as i32;
                let r = self.new_kern(delta)?;
                self.mem.set_subtype(r, ACC_KERN);
                let tail = self.nest.cur.tail;
                self.mem.set_link(tail, r);
                self.mem.set_link(r, p);
                let k = self.new_kern(-a - delta)?;
                self.mem.set_subtype(k, ACC_KERN);
                self.mem.set_link(p, k);
                self.nest.cur.tail = k;
                p = q;
            }
            let tail = self.nest.cur.tail;
            let lt = self.mem.link(tail);
            self.mem.set_link(p, lt);
            self.mem.set_link(tail, p);
            self.nest.cur.tail = p;
            self.set_space_factor(1000);
        }
        Ok(())
    }

    /// `append_discretionary` (§1115): `\discretionary` and `\-`.
    pub fn append_discretionary(&mut self) -> TexResult<()> {
        let d = self.new_disc()?;
        self.tail_append(d);
        if self.cur_chr == 1 {
            let f = self.eqtb.cur_font();
            let c = self.fonts.hyphen_char[f as usize];
            if (0..256).contains(&c) {
                let p = self.new_character(f, c)?;
                let t = self.nest.cur.tail;
                self.mem.set_pre_break(t, p);
            }
        } else {
            self.save.save_ptr += 1;
            self.save.set_saved(-1, 0);
            self.new_save_level(DISC_GROUP)?;
            self.scan_left_brace()?;
            self.push_nest()?;
            self.nest.cur.mode = -HMODE;
            self.set_space_factor(1000);
        }
        Ok(())
    }

    /// `build_discretionary` (§1116-§1119): ends one of the three lists.
    pub fn build_discretionary(&mut self) -> TexResult<()> {
        self.unsave()?;
        // §1121: prune the current list to char/kern/box/rule/ligature.
        let mut q = self.nest.cur.head;
        let mut p = self.mem.link(q);
        let mut n = 0;
        while p != NULL {
            if !self.mem.is_char_node(p)
                && self.mem.node_type(p) > RULE_NODE
                && self.mem.node_type(p) != KERN_NODE
                && self.mem.node_type(p) != LIGATURE_NODE
            {
                self.print_err("Improper discretionary list");
                self.help(&["Discretionary lists must contain only boxes and kerns."]);
                self.error()?;
                self.begin_diagnostic();
                self.print_nl_chars("The following discretionary sublist has been deleted:");
                self.show_box(p);
                self.end_diagnostic(true);
                self.flush_node_list(p);
                self.mem.set_link(q, NULL);
                break;
            }
            q = p;
            p = self.mem.link(q);
            n += 1;
        }
        let p = self.mem.link(self.nest.cur.head);
        self.pop_nest();
        match self.save.saved(-1) {
            0 => {
                let t = self.nest.cur.tail;
                self.mem.set_pre_break(t, p);
            }
            1 => {
                let t = self.nest.cur.tail;
                self.mem.set_post_break(t, p);
            }
            _ => {
                // §1118: attach list p (the replacement text).
                if n > 0 && self.mode().abs() == MMODE {
                    self.print_err("Illegal math ");
                    self.print_esc_str("discretionary");
                    self.help(&[
                        "Sorry: The third part of a discretionary break must be",
                        "empty, in math formulas. I had to delete your third part.",
                    ]);
                    self.error()?;
                    self.flush_node_list(p);
                } else {
                    let t = self.nest.cur.tail;
                    self.mem.set_link(t, p);
                    if n <= 0xFFFF {
                        self.mem.set_replace_count(t, n as u16);
                    } else {
                        self.print_err("Discretionary list is too long");
                        self.help(&[
                            "Wow---I never thought anybody would tweak me here.",
                            "You can't seriously need such a huge discretionary list?",
                        ]);
                        self.error()?;
                    }
                    if n > 0 {
                        self.nest.cur.tail = q;
                    }
                }
                self.save.save_ptr -= 1;
                return Ok(());
            }
        }
        let s = self.save.saved(-1);
        self.save.set_saved(-1, s + 1);
        self.new_save_level(DISC_GROUP)?;
        self.scan_left_brace()?;
        self.push_nest()?;
        self.nest.cur.mode = -HMODE;
        self.set_space_factor(1000);
        Ok(())
    }

    /// `unpackage` (§1110): `\unhbox`, `\unhcopy`, `\unvbox`, `\unvcopy`.
    pub fn unpackage(&mut self) -> TexResult<()> {
        if self.cur_chr > crate::control::COPY_CODE {
            // etex.ch: \pagediscards / \splitdiscards.
            let c = self.cur_chr as usize;
            let t = self.nest.cur.tail;
            let d = self.disc_ptr[c];
            self.mem.set_link(t, d);
            self.disc_ptr[c] = NULL;
            while self.mem.link(self.nest.cur.tail) != NULL {
                let nx = self.mem.link(self.nest.cur.tail);
                self.nest.cur.tail = nx;
            }
            return Ok(());
        }
        let c = self.cur_chr;
        self.scan_register_num()?;
        let n = self.cur_val;
        let p = self.fetch_box(n)?;
        if p == NULL {
            return Ok(());
        }
        let m = self.mode().abs();
        if m == MMODE
            || (m == VMODE && self.mem.node_type(p) != VLIST_NODE)
            || (m == HMODE && self.mem.node_type(p) != HLIST_NODE)
        {
            self.print_err("Incompatible list can't be unboxed");
            self.help(&[
                "Sorry, Pandora. (You sneaky devil.)",
                "I refuse to unbox an \\hbox in vertical mode or vice versa.",
                "And I can't open any boxes in math mode.",
            ]);
            self.error()?;
            return Ok(());
        }
        let t = self.nest.cur.tail;
        if c == crate::control::COPY_CODE {
            let lp = self.mem.list_ptr(p);
            let l = self.copy_node_list(lp)?;
            self.mem.set_link(t, l);
        } else {
            let lp = self.mem.list_ptr(p);
            self.mem.set_link(t, lp);
            self.change_box(n, NULL)?;
            self.mem.free_node(p, BOX_NODE_SIZE);
        }
        while self.mem.link(self.nest.cur.tail) != NULL {
            let nx = self.mem.link(self.nest.cur.tail);
            self.nest.cur.tail = nx;
        }
        Ok(())
    }
}
