//! Japanese character classification (upTeX): the Unicode block table
//! behind `kcatcodekey` (uptexdir/kanji.c, Blocks 17.0.0) and the default
//! \kcatcode assignments (uptex-m.ch). Values: latin_ucs=14, not_cjk=15,
//! kanji=16, kana=17, other_kchar=18, hangul=19, modifier=20.

pub const LATIN_UCS: i32 = 14;
pub const NOT_CJK: i32 = 15;
pub const KANJI: i32 = 16;
pub const KANA: i32 = 17;
pub const OTHER_KCHAR: i32 = 18;
pub const HANGUL: i32 = 19;
pub const MODIFIER: i32 = 20;

/// Number of \kcatcode table entries (uptex-m.ch: 512).
pub const KCAT_ENTRIES: i32 = 512;

pub const UCS_RANGE: [i32; 383] = [
    0x0, 0x80, 0x100, 0x180, 0x250, 0x2B0, 0x300, 0x370, 0x400, 0x500, 0x530, 0x590, 0x600, 0x700,
    0x750, 0x780, 0x7C0, 0x800, 0x840, 0x860, 0x870, 0x8A0, 0x900, 0x980, 0xA00, 0xA80, 0xB00,
    0xB80, 0xC00, 0xC80, 0xD00, 0xD80, 0xE00, 0xE80, 0xF00, 0x1000, 0x10A0, 0x1100, 0x1200, 0x1380,
    0x13A0, 0x1400, 0x1680, 0x16A0, 0x1700, 0x1720, 0x1740, 0x1760, 0x1780, 0x1800, 0x18B0, 0x1900,
    0x1950, 0x1980, 0x19E0, 0x1A00, 0x1A20, 0x1AB0, 0x1B00, 0x1B80, 0x1BC0, 0x1C00, 0x1C50, 0x1C80,
    0x1C90, 0x1CC0, 0x1CD0, 0x1D00, 0x1D80, 0x1DC0, 0x1E00, 0x1F00, 0x2000, 0x2070, 0x20A0, 0x20D0,
    0x2100, 0x2150, 0x2190, 0x2200, 0x2300, 0x2400, 0x2440, 0x2460, 0x2500, 0x2580, 0x25A0, 0x2600,
    0x2700, 0x27C0, 0x27F0, 0x2800, 0x2900, 0x2980, 0x2A00, 0x2B00, 0x2C00, 0x2C60, 0x2C80, 0x2D00,
    0x2D30, 0x2D80, 0x2DE0, 0x2E00, 0x2E80, 0x2F00, 0x2FF0, 0x3000, 0x3040, 0x30A0, 0x3100, 0x3130,
    0x3190, 0x31A0, 0x31C0, 0x31F0, 0x3200, 0x3300, 0x3400, 0x4DC0, 0x4E00, 0xA000, 0xA490, 0xA4D0,
    0xA500, 0xA640, 0xA6A0, 0xA700, 0xA720, 0xA800, 0xA830, 0xA840, 0xA880, 0xA8E0, 0xA900, 0xA930,
    0xA960, 0xA980, 0xA9E0, 0xAA00, 0xAA60, 0xAA80, 0xAAE0, 0xAB00, 0xAB30, 0xAB70, 0xABC0, 0xAC00,
    0xD7B0, 0xD800, 0xDB80, 0xDC00, 0xE000, 0xF900, 0xFB00, 0xFB50, 0xFE00, 0xFE10, 0xFE20, 0xFE30,
    0xFE50, 0xFE70, 0xFF00, 0xFFF0, 0x10000, 0x10080, 0x10100, 0x10140, 0x10190, 0x101D0, 0x10280,
    0x102A0, 0x102E0, 0x10300, 0x10330, 0x10350, 0x10380, 0x103A0, 0x10400, 0x10450, 0x10480,
    0x104B0, 0x10500, 0x10530, 0x10570, 0x105C0, 0x10600, 0x10780, 0x10800, 0x10840, 0x10860,
    0x10880, 0x108E0, 0x10900, 0x10920, 0x10940, 0x10980, 0x109A0, 0x10A00, 0x10A60, 0x10A80,
    0x10AC0, 0x10B00, 0x10B40, 0x10B60, 0x10B80, 0x10C00, 0x10C80, 0x10D00, 0x10D40, 0x10E60,
    0x10E80, 0x10EC0, 0x10F00, 0x10F30, 0x10F70, 0x10FB0, 0x10FE0, 0x11000, 0x11080, 0x110D0,
    0x11100, 0x11150, 0x11180, 0x111E0, 0x11200, 0x11280, 0x112B0, 0x11300, 0x11380, 0x11400,
    0x11480, 0x11580, 0x11600, 0x11660, 0x11680, 0x116D0, 0x11700, 0x11800, 0x118A0, 0x11900,
    0x119A0, 0x11A00, 0x11A50, 0x11AB0, 0x11AC0, 0x11B00, 0x11B60, 0x11BC0, 0x11C00, 0x11C70,
    0x11D00, 0x11D60, 0x11DB0, 0x11EE0, 0x11F00, 0x11FB0, 0x11FC0, 0x12000, 0x12400, 0x12480,
    0x12F90, 0x13000, 0x13430, 0x13460, 0x14400, 0x16100, 0x16800, 0x16A40, 0x16A70, 0x16AD0,
    0x16B00, 0x16D40, 0x16E40, 0x16EA0, 0x16F00, 0x16FE0, 0x17000, 0x18800, 0x18B00, 0x18D00,
    0x18D80, 0x1AFF0, 0x1B000, 0x1B100, 0x1B130, 0x1B170, 0x1BC00, 0x1BCA0, 0x1CC00, 0x1CEC0,
    0x1CF00, 0x1D000, 0x1D100, 0x1D200, 0x1D2C0, 0x1D2E0, 0x1D300, 0x1D360, 0x1D400, 0x1D800,
    0x1DF00, 0x1E000, 0x1E030, 0x1E100, 0x1E290, 0x1E2C0, 0x1E4D0, 0x1E5D0, 0x1E6C0, 0x1E7E0,
    0x1E800, 0x1E900, 0x1EC70, 0x1ED00, 0x1EE00, 0x1F000, 0x1F030, 0x1F0A0, 0x1F100, 0x1F200,
    0x1F300, 0x1F600, 0x1F650, 0x1F680, 0x1F700, 0x1F780, 0x1F800, 0x1F900, 0x1FA00, 0x1FA70,
    0x1FB00, 0x20000, 0x2A700, 0x2B740, 0x2B820, 0x2CEB0, 0x2EBF0, 0x2F800, 0x30000, 0x31350,
    0x323B0, 0x33480, 0x40000, 0x50000, 0x60000, 0x70000, 0x80000, 0x90000, 0xA0000, 0xB0000,
    0xC0000, 0xD0000, 0xE0000, 0xE0100, 0xE01F0, 0xF0000, 0x100000, 0x110000, 0x120000, 0x130000,
    0x140000, 0x150000, 0x160000, 0x170000, 0x180000, 0x190000, 0x1A0000, 0x1B0000, 0x1C0000,
    0x1D0000, 0x1E0000, 0x1F0000, 0x200000, 0x210000, 0x220000, 0x240000, 0x25E6E6, 0x260000,
    0x300000, 0x400000, 0x800000, 0x800080,
];

/// `kcatcodekey(c)` (uptexdir/kanji.c): maps a USV to its \kcatcode
/// table index — the Unicode block number, with a few sub-block
/// special cases mapped to pseudo-keys 0x1F9..0x1FF.
pub fn kcatcodekey(c: i32) -> i32 {
    let block = match UCS_RANGE.binary_search(&c) {
        Ok(i) => i as i32,
        Err(i) => i as i32 - 1,
    };
    match block {
        0x01 => {
            // Latin-1 Supplement: the Latin-1 letters.
            if c == 0xAA
                || c == 0xBA
                || (0xC0..=0xD6).contains(&c)
                || (0xD8..=0xF6).contains(&c)
                || (0xF8..=0xFF).contains(&c)
            {
                return 0x1FD;
            }
        }
        0xA2 => {
            // Halfwidth and Fullwidth Forms.
            if (0xFF10..=0xFF19).contains(&c)
                || (0xFF21..=0xFF3A).contains(&c)
                || (0xFF41..=0xFF5A).contains(&c)
            {
                return 0x1FE; // fullwidth ASCII variants
            }
            if (0xFF66..=0xFF6F).contains(&c) || (0xFF71..=0xFF9D).contains(&c) {
                return 0x1FF; // halfwidth katakana variants
            }
        }
        0x6C if c == 0x3099 || c == 0x309A => {
            return 0x1F9; // combining kana voiced sound marks
        }
        0x4B if c == 0x20E3 => {
            return 0x1FA; // combining enclosing keycap
        }
        0x13F if (0x1F1E6..=0x1F1FF).contains(&c) => {
            return 0x1FB; // regional indicators
        }
        0x141 if (0x1F3FB..=0x1F3FF).contains(&c) => {
            return 0x1FC; // emoji modifiers
        }
        _ => {}
    }
    block
}

/// The default \kcatcode table (uptex-m.ch, internal upTeX branch).
pub fn default_kcat_codes() -> [i32; KCAT_ENTRIES as usize] {
    let mut k = [OTHER_KCHAR; KCAT_ENTRIES as usize];
    let mut set = |range: std::ops::RangeInclusive<usize>, v: i32| {
        for i in range {
            k[i] = v;
        }
    };
    set(0x0..=0x0, NOT_CJK);
    set(0x2..=0x3, NOT_CJK); // Latin Extended-A/B
    set(0x25..=0x25, HANGUL); // Hangul Jamo
    set(0x46..=0x46, NOT_CJK); // Latin Extended Additional
    set(0x68..=0x69, KANJI); // CJK Radicals Supplement .. Kangxi Radicals
    set(0x6C..=0x6D, KANA); // Hiragana, Katakana
    set(0x6E..=0x6E, KANJI); // Bopomofo
    set(0x6F..=0x6F, HANGUL); // Hangul Compatibility Jamo
    set(0x70..=0x72, KANJI); // Kanbun .. CJK Strokes
    set(0x73..=0x73, KANA); // Katakana Phonetic Extensions
    set(0x76..=0x76, KANJI); // CJK Unified Ideographs Extension A
    set(0x78..=0x78, KANJI); // CJK Unified Ideographs
    set(0x88..=0x88, HANGUL); // Hangul Jamo Extended-A
    set(0x93..=0x93, HANGUL); // Hangul Syllables
    set(0x94..=0x94, HANGUL); // Hangul Jamo Extended-B
    set(0x99..=0x99, KANJI); // CJK Compatibility Ideographs
    set(0x9C..=0x9C, MODIFIER); // Variation Selectors
    set(0x11A..=0x11D, KANA); // Kana Extended-B .. Small Kana Extension
    set(0x14C..=0x156, KANJI); // CJK Unified Ideographs Extension B .. J
    set(0x162..=0x162, MODIFIER); // Variation Selectors Supplement
    set(0x166..=0x169, KANJI); // for japanese-otf(-uptex)
    set(0x177..=0x178, KANA); // Kana with (Semi-)Voiced Sound Mark
    set(0x17C..=0x17C, KANJI); // Standardized Variation Sequence
    set(0x17E..=0x17F, KANJI); // Ideographic Variation Sequence
    set(0x1F9..=0x1FC, MODIFIER);
    set(0x1FD..=0x1FD, NOT_CJK); // Latin-1 letters
    set(0x1FE..=0x1FF, KANA); // full/halfwidth variants
    k
}

impl crate::engine::Engine {
    /// Is character `c` typeset as Japanese (with a Japanese current
    /// font loaded)? Transparency rule: without a \jfont in effect the
    /// engine behaves exactly like XeTeX.
    pub fn is_japanese_char(&self, c: i32) -> bool {
        if c < 0x80 {
            return false;
        }
        let f = self.eqtb.equiv(self.eqtb.lay.cur_jfont_loc);
        if f == 0 {
            return false;
        }
        matches!(
            self.eqtb.kcat_code(kcatcodekey(c)),
            KANJI | KANA | OTHER_KCHAR | HANGUL
        )
    }

    /// pTeX: appends the two-node pair for a Japanese character. The
    /// first node carries the font and the JFM char_type class; the
    /// second carries the character code in its info field.
    pub fn append_kanji(&mut self) -> crate::error::TexResult<()> {
        use crate::eqtb::LANGUAGE_CODE;
        if self.mode() > 0 && self.eqtb.int_par(LANGUAGE_CODE) != self.clang() {
            self.fix_language()?;
        }
        let f = self.eqtb.equiv(self.eqtb.lay.cur_jfont_loc);
        let c = self.cur_chr;
        let ct = self.get_jfm_pos(c, f);
        // pTeX: if the previous item is a Japanese character in the same
        // font, consult the JFM glue/kern program for the class pair and
        // insert the glue (subtype jfm_skip+1) or kern node.
        let tail = self.nest.cur.tail;
        let prev_head = self.prev_kanji_head(tail);
        if prev_head != crate::types::NULL
            && i32::from(self.mem.font(prev_head)) == f
            && !self.inhibit_glue_flag
        {
            let prev_ct = i32::from(self.mem.character(prev_head));
            self.append_jfm_glue_kern(f, prev_ct, ct)?;
        }
        self.inhibit_glue_flag = false;
        // ptex-base.ch (main_loop_j+1): the first Japanese character of
        // a list is preceded by a disp_node (baseline displacement; 0 in
        // horizontal-only typesetting). TRIP-safe: only Japanese text
        // reaches this point.
        if !self.nest.cur.disp_called {
            let d = self.mem.get_node(crate::nodes::SMALL_NODE_SIZE)?;
            self.mem.set_node_type(d, crate::nodes::DISP_NODE);
            self.mem.set_subtype(d, 0);
            self.mem.set_width(d, 0); // disp_dimen
            self.tail_append(d);
            self.nest.cur.disp_called = true;
        }
        // kinsoku: pre penalty goes before the character (merging into
        // an existing penalty node), post penalty after it.
        self.insert_pre_break_penalty(c)?;
        let p = self.mem.get_avail()?;
        self.mem.set_font(p, f as u16);
        self.mem.set_character(p, ct as u16);
        let q = self.mem.get_avail()?;
        self.mem.set_info(q, c); // KANJI code
        self.tail_append(p);
        self.last_jchr = p;
        self.tail_append(q);
        self.insert_post_break_penalty(c)?;
        self.set_space_factor(1000);
        Ok(())
    }

    /// Is `p` the CODE node (second word) of a Japanese pair? True when
    /// it is a char node whose predecessor made it one — approximated by
    /// checking that it is a char node that is NOT itself a kanji head
    /// but whose info looks like a stored USV. Callers only use this
    /// where `p` is known to be either a pair's code node or an
    /// alphabetic char node, so testing the preceding pair is enough.
    fn is_kanji_code_node(&self, p: crate::types::Pointer) -> bool {
        // In adjust_hlist's walk, `last_jk_prev` is the node before the
        // last pair head; if IT is a char node, the pair-head test on
        // the node before it is unavailable (singly-linked list), but
        // the only char node that can directly precede a pair head is
        // either an alphabetic char or another pair's code node. Both
        // mean "text flows into the kanji", and pTeX materialises the
        // skip in both cases via the surround logic; we approximate
        // with the Japanese case (kanjiskip), which euptex shows for
        // kanji-kanji.
        self.mem.is_char_node(p)
    }

    /// pTeX `font_dir[f] <> dir_default` — is p (a char node) the head
    /// of a Japanese pair?
    pub fn is_kanji_head(&self, p: crate::types::Pointer) -> bool {
        self.mem.is_char_node(p) && self.fonts.dir[self.mem.font(p) as usize] != 0
    }
}

impl crate::engine::Engine {
    /// pTeX: latch cur_kanji_skip / cur_xkanji_skip from the glue
    /// parameters, honoring \autospacing / \autoxspacing. Called just
    /// before a box is packaged.
    pub fn latch_kanji_skips(&mut self) {
        let zg = self.mem.zero_glue();
        let lay = self.eqtb.lay.clone();
        self.cur_kanji_skip = if self.eqtb.equiv(lay.auto_spacing_loc) > 0 {
            self.eqtb.glue_par(crate::eqtb::KANJI_SKIP_CODE)
        } else {
            zg
        };
        self.cur_xkanji_skip = if self.eqtb.equiv(lay.auto_xspacing_loc) > 0 {
            self.eqtb.glue_par(crate::eqtb::XKANJI_SKIP_CODE)
        } else {
            zg
        };
    }

    /// The implicit glue spec between two adjacent character nodes
    /// (`p` then `q`), or NULL. `p`/`q` must be the HEADS of their
    /// pairs (for Japanese) or plain char nodes.
    pub fn implicit_skip_between(
        &self,
        p: crate::types::Pointer,
        q: crate::types::Pointer,
    ) -> crate::types::Pointer {
        use crate::types::NULL;
        if q == NULL || !self.mem.is_char_node(q) {
            return NULL;
        }
        let pj = self.is_kanji_head(p);
        let qj = self.is_kanji_head(q);
        if pj && qj {
            self.cur_kanji_skip
        } else if pj != qj {
            self.cur_xkanji_skip
        } else {
            NULL
        }
    }
}

pub const JFM_SKIP: u16 = 20;

impl crate::engine::Engine {
    /// If `tail` is the code node of a Japanese pair, returns the pair
    /// head; NULL otherwise. (The head is the node BEFORE tail, so we
    /// check that tail follows a kanji head — cheap test: tail is a char
    /// node and the node before it, reachable only via the list, would
    /// be O(n). Instead pTeX tracks last_jchr; we do the same.)
    pub fn prev_kanji_head(&self, tail: crate::types::Pointer) -> crate::types::Pointer {
        if self.last_jchr != crate::types::NULL
            && self.mem.link(self.last_jchr) == tail
            && self.is_kanji_head(self.last_jchr)
        {
            self.last_jchr
        } else {
            crate::types::NULL
        }
    }

    /// Looks up the JFM glue/kern program of font `f` for the class
    /// pair (l, r) and appends the resulting node, if any.
    fn append_jfm_glue_kern(&mut self, f: i32, l: i32, r: i32) -> crate::error::TexResult<()> {
        let fu = f as usize;
        let qw = self.fonts.char_info(f, l);
        if crate::fonts::FontMem::char_tag(qw) != 1 {
            return Ok(()); // no glue/kern program (gk_tag)
        }
        let mut k = self.fonts.lig_kern_base[fu] + i32::from(crate::fonts::FontMem::rem_byte(qw));
        let mut w = self.fonts.info[k as usize];
        if w.qqqq(0) > 128 {
            // huge program: restart
            k = self.fonts.lig_kern_base[fu] + 256 * i32::from(w.qqqq(2)) + i32::from(w.qqqq(3));
            w = self.fonts.info[k as usize];
        }
        loop {
            let (skip, next, op, rem) = (
                i32::from(w.qqqq(0)),
                i32::from(w.qqqq(1)),
                i32::from(w.qqqq(2)),
                i32::from(w.qqqq(3)),
            );
            if next == r && skip <= 128 {
                if op < 128 {
                    // glue: three scaled words in the exten area.
                    let base = self.fonts.exten_base[fu] + (op * 256 + rem) * 3;
                    let spec = self.mem.get_node(crate::mem::GLUE_SPEC_SIZE)?;
                    let wd = self.fonts.info[base as usize].sc();
                    let st = self.fonts.info[base as usize + 1].sc();
                    let sh = self.fonts.info[base as usize + 2].sc();
                    self.mem.set_glue_ref_count(spec, crate::types::NULL);
                    self.mem.set_width(spec, wd);
                    self.mem.set_stretch(spec, st);
                    self.mem.set_shrink(spec, sh);
                    self.mem.set_stretch_order(spec, crate::mem::NORMAL);
                    self.mem.set_shrink_order(spec, crate::mem::NORMAL);
                    let g = self.new_glue(spec)?;
                    self.mem.set_subtype(g, JFM_SKIP + 1);
                    self.tail_append(g);
                } else {
                    // kern.
                    let kb = self.fonts.kern_base[fu]
                        + crate::fonts::KERN_BASE_OFFSET
                        + 256 * (op - 128)
                        + rem;
                    let kv = self.fonts.info[kb as usize].sc();
                    let kn = self.new_kern(kv)?;
                    self.tail_append(kn);
                }
                return Ok(());
            }
            if skip >= 128 {
                return Ok(());
            }
            k += skip + 1;
            w = self.fonts.info[k as usize];
        }
    }
}

// pTeX kinsoku table codes.
pub const PRE_BREAK_PENALTY_CODE: u16 = 1;
pub const POST_BREAK_PENALTY_CODE: u16 = 2;
pub const KINSOKU_UNUSED_CODE: u16 = 3;
pub const NO_ENTRY: i32 = 10000;
/// penalty-node subtypes (ptex-base.ch).
pub const WIDOW_PENA: u16 = 1;
pub const KINSOKU_PENA: u16 = 2;

/// `calc_pos(c)` (uptexdir/kanji.c): initial probe of the kinsoku hash.
pub fn calc_pos(c: i32) -> i32 {
    if (0..=255).contains(&c) {
        c
    } else {
        let c1 = ((c >> 8) & 0xFF) % 4 * 64;
        let c2 = (c & 0xFF) % 64;
        c1 + c2
    }
}

impl crate::engine::Engine {
    /// `get_kinsoku_pos(c, n)` (ptex-base.ch): finds c's slot in the
    /// kinsoku hash. `new_pos` (creating) also accepts an unused slot.
    pub fn get_kinsoku_pos(&self, c: i32, creating: bool) -> i32 {
        let base = self.eqtb.lay.kinsoku_base;
        let s = calc_pos(c);
        let mut p = s;
        let mut pp = NO_ENTRY;
        loop {
            let ty = self.eqtb.eq_type(base + p);
            let code = self.eqtb.equiv(base + p);
            if creating {
                if code == c {
                    return p;
                }
                if ty == 0 {
                    return if pp != NO_ENTRY { pp } else { p };
                }
                if ty == KINSOKU_UNUSED_CODE && pp == NO_ENTRY {
                    pp = p;
                }
            } else {
                if ty == 0 {
                    return NO_ENTRY;
                }
                if code == c {
                    return p;
                }
            }
            p += 1;
            if p > 1023 {
                p = 0;
            }
            if p == s {
                return if creating { pp } else { NO_ENTRY };
            }
        }
    }

    pub fn kinsoku_penalty(&self, pos: i32) -> i32 {
        self.eqtb
            .word(self.eqtb.lay.kinsoku_penalty_base + pos)
            .int()
    }

    /// pTeX `@<Insert pre_break_penalty of c@>` — before the character
    /// about to be appended (merging into a preceding penalty node).
    pub fn insert_pre_break_penalty(&mut self, c: i32) -> crate::error::TexResult<()> {
        let kp = self.get_kinsoku_pos(c, false);
        if kp == NO_ENTRY || self.kinsoku_penalty(kp) == 0 {
            return Ok(());
        }
        if self.eqtb.eq_type(self.eqtb.lay.kinsoku_base + kp) != PRE_BREAK_PENALTY_CODE {
            return Ok(());
        }
        let pen = self.kinsoku_penalty(kp);
        let tail = self.nest.cur.tail;
        if !self.mem.is_char_node(tail)
            && tail != self.nest.cur.head
            && self.mem.node_type(tail) == crate::nodes::PENALTY_NODE
        {
            let v = self.mem.penalty(tail);
            self.mem.set_penalty(tail, v + pen);
        } else {
            let p = self.new_penalty(pen)?;
            self.mem.set_subtype(p, KINSOKU_PENA);
            self.tail_append(p);
        }
        Ok(())
    }

    /// pTeX `@<Insert post_break_penalty@>` — after the character just
    /// appended.
    pub fn insert_post_break_penalty(&mut self, c: i32) -> crate::error::TexResult<()> {
        let kp = self.get_kinsoku_pos(c, false);
        if kp == NO_ENTRY || self.kinsoku_penalty(kp) == 0 {
            return Ok(());
        }
        if self.eqtb.eq_type(self.eqtb.lay.kinsoku_base + kp) != POST_BREAK_PENALTY_CODE {
            return Ok(());
        }
        let pen = self.kinsoku_penalty(kp);
        let p = self.new_penalty(pen)?;
        self.mem.set_subtype(p, KINSOKU_PENA);
        self.tail_append(p);
        Ok(())
    }
}

use crate::types::{Pointer, NULL};

/// pTeX `insert_skip` states.
#[derive(PartialEq, Clone, Copy)]
enum InsertSkip {
    NoSkip,
    AfterSchar,
    AfterWchar,
}

/// pTeX \inhibitxspcode values.
pub const INHIBIT_BOTH: u16 = 0;
pub const INHIBIT_PREVIOUS: u16 = 1;
pub const INHIBIT_AFTER: u16 = 2;
pub const INHIBIT_UNUSED: u16 = 4;

impl crate::engine::Engine {
    /// `get_inhibit_pos(c, n)` (ptex-base.ch): the \inhibitxspcode hash
    /// slot for c. Unlike the kinsoku hash, EMPTINESS is equiv == 0
    /// (the type field legitimately holds 0 = inhibit_both).
    pub fn get_inhibit_pos(&self, c: i32, creating: bool) -> i32 {
        let base = self.eqtb.lay.inhibit_xsp_code_base;
        let s = calc_pos(c);
        let mut p = s;
        let mut pp = NO_ENTRY;
        loop {
            let ty = self.eqtb.eq_type(base + p);
            let code = self.eqtb.equiv(base + p);
            if creating {
                if code == c {
                    return p;
                }
                if code == 0 {
                    return if pp != NO_ENTRY { pp } else { p };
                }
                if ty == INHIBIT_UNUSED && pp == NO_ENTRY {
                    pp = p;
                }
            } else {
                if code == 0 {
                    return NO_ENTRY;
                }
                if code == c {
                    return p;
                }
            }
            p += 1;
            if p > 1023 {
                p = 0;
            }
            if p == s {
                return if creating { pp } else { NO_ENTRY };
            }
        }
    }

    /// \the\inhibitxspcode readback: unset characters answer 3
    /// (allow both sides; verified against euptex).
    pub fn inhibit_xsp_code_of(&self, c: i32) -> i32 {
        let pos = self.get_inhibit_pos(c, false);
        if pos == NO_ENTRY {
            3
        } else {
            i32::from(self.inhibit_type(pos))
        }
    }

    fn inhibit_type(&self, pos: i32) -> u16 {
        self.eqtb.eq_type(self.eqtb.lay.inhibit_xsp_code_base + pos)
    }

    /// May \xkanjiskip appear BEFORE Japanese character cx (A-K)?
    fn xsp_ok_before(&self, cx: i32) -> bool {
        let x = self.get_inhibit_pos(cx, false);
        x == NO_ENTRY
            || !(self.inhibit_type(x) == INHIBIT_BOTH || self.inhibit_type(x) == INHIBIT_PREVIOUS)
    }

    /// May \xkanjiskip appear AFTER Japanese character cx (K-A)?
    fn xsp_ok_after(&self, cx: i32) -> bool {
        let x = self.get_inhibit_pos(cx, false);
        x == NO_ENTRY
            || !(self.inhibit_type(x) == INHIBIT_BOTH || self.inhibit_type(x) == INHIBIT_AFTER)
    }

    fn auto_xsp_code(&self, ax: i32) -> i32 {
        if (0..256).contains(&ax) {
            self.eqtb.equiv(self.eqtb.lay.auto_xsp_code_base + ax)
        } else {
            0
        }
    }

    /// pTeX `adjust_hlist(p, pf)` — K-K subset (see specification/japanese.md).
    pub fn adjust_hlist(&mut self, head: Pointer, pf: bool) -> crate::error::TexResult<()> {
        use crate::nodes::{GLUE_NODE, KERN_NODE, PENALTY_NODE};
        if self.mem.link(head) == NULL {
            return Ok(());
        }
        self.latch_kanji_skips();
        let u = self.cur_kanji_skip;
        // Strip a leading JFM glue (directly or after a kinsoku penalty).
        let first = self.mem.link(head);
        if !self.mem.is_char_node(first) {
            if self.mem.node_type(first) == GLUE_NODE && self.mem.subtype(first) == JFM_SKIP + 1 {
                let nxt = self.mem.link(first);
                self.mem.set_link(head, nxt);
                let spec = self.mem.glue_ptr(first);
                self.delete_glue_ref(spec);
                self.mem.free_node(first, crate::nodes::SMALL_NODE_SIZE);
            } else if self.mem.node_type(first) == PENALTY_NODE
                && self.mem.subtype(first) == KINSOKU_PENA
            {
                let v = self.mem.link(first);
                if v != NULL
                    && !self.mem.is_char_node(v)
                    && self.mem.node_type(v) == GLUE_NODE
                    && self.mem.subtype(v) == JFM_SKIP + 1
                {
                    self.mem.set_link(first, self.mem.link(v));
                    let spec = self.mem.glue_ptr(v);
                    self.delete_glue_ref(spec);
                    self.mem.free_node(v, crate::nodes::SMALL_NODE_SIZE);
                }
            }
        }
        let xs = self.cur_xkanji_skip;
        let mut insert_skip = InsertSkip::NoSkip;
        let mut cx: i32 = 0; // last Japanese character seen
        let mut p = self.mem.link(head);
        let mut q = p;
        // A5 (\jcharwidowpenalty): track the node BEFORE the last
        // Japanese pair and the character count, for the widow pass.
        let mut char_count: i32 = 0;
        let mut last_jk_prev: Pointer = NULL;
        let mut prev: Pointer = head;
        while p != NULL {
            if self.mem.is_char_node(p) {
                loop {
                    char_count += 1;
                    if self.is_kanji_head(p) {
                        last_jk_prev = prev;
                        let code = self.mem.info(self.mem.link(p));
                        if insert_skip == InsertSkip::AfterSchar && self.xsp_ok_before(code) {
                            // ASCII-KANJI spacing: real \xkanjiskip.
                            let z = self.new_glue(xs)?;
                            self.mem
                                .set_subtype(z, crate::eqtb::XKANJI_SKIP_CODE as u16 + 1);
                            self.mem.set_link(z, p);
                            self.mem.set_link(q, z);
                        }
                        cx = code;
                        p = self.mem.link(p); // code node
                        insert_skip = InsertSkip::AfterWchar;
                    } else {
                        let ax = i32::from(self.mem.character(p));
                        if insert_skip == InsertSkip::AfterWchar
                            && self.auto_xsp_code(ax) % 2 == 1
                            && self.xsp_ok_after(cx)
                        {
                            // KANJI-ASCII spacing: real \xkanjiskip.
                            let z = self.new_glue(xs)?;
                            self.mem
                                .set_subtype(z, crate::eqtb::XKANJI_SKIP_CODE as u16 + 1);
                            self.mem.set_link(z, p);
                            self.mem.set_link(q, z);
                        }
                        insert_skip = if self.auto_xsp_code(ax) >= 2 {
                            InsertSkip::AfterSchar
                        } else {
                            InsertSkip::NoSkip
                        };
                    }
                    q = p;
                    prev = p;
                    p = self.mem.link(p);
                    if !self.mem.is_char_node(p) {
                        break;
                    }
                }
            } else {
                match self.mem.node_type(p) {
                    PENALTY_NODE | crate::nodes::DISP_NODE => {
                        // pTeX @<Insert penalty or displace surround
                        // spacing@>: spacing crosses the penalty (or the
                        // displacement marker), materialising the skip
                        // right after it.
                        let nxt = self.mem.link(p);
                        if nxt != NULL && self.mem.is_char_node(nxt) {
                            if self.is_kanji_head(nxt) {
                                let code = self.mem.info(self.mem.link(nxt));
                                if insert_skip == InsertSkip::AfterWchar {
                                    // real \kanjiskip after the penalty
                                    let z = self.new_glue(u)?;
                                    self.mem
                                        .set_subtype(z, crate::eqtb::KANJI_SKIP_CODE as u16 + 1);
                                    self.mem.set_link(z, nxt);
                                    self.mem.set_link(p, z);
                                } else if insert_skip == InsertSkip::AfterSchar
                                    && self.xsp_ok_before(code)
                                {
                                    let z = self.new_glue(xs)?;
                                    self.mem
                                        .set_subtype(z, crate::eqtb::XKANJI_SKIP_CODE as u16 + 1);
                                    self.mem.set_link(z, nxt);
                                    self.mem.set_link(p, z);
                                }
                                cx = code;
                                q = self.mem.link(nxt); // code node
                                insert_skip = InsertSkip::AfterWchar;
                            } else {
                                let ax = i32::from(self.mem.character(nxt));
                                if insert_skip == InsertSkip::AfterWchar
                                    && self.auto_xsp_code(ax) % 2 == 1
                                    && self.xsp_ok_after(cx)
                                {
                                    let z = self.new_glue(xs)?;
                                    self.mem
                                        .set_subtype(z, crate::eqtb::XKANJI_SKIP_CODE as u16 + 1);
                                    self.mem.set_link(z, nxt);
                                    self.mem.set_link(p, z);
                                }
                                q = nxt;
                                insert_skip = if self.auto_xsp_code(ax) >= 2 {
                                    InsertSkip::AfterSchar
                                } else {
                                    InsertSkip::NoSkip
                                };
                            }
                            p = self.mem.link(q);
                            continue;
                        }
                    }
                    crate::nodes::LIGATURE_NODE => {
                        // @<Insert ligature surround spacing@>: judge by
                        // the ligature's first and last constituent.
                        let t = self.mem.lig_ptr(p);
                        if t != NULL && self.mem.is_char_node(t) {
                            let ax = i32::from(self.mem.character(t));
                            if insert_skip == InsertSkip::AfterWchar
                                && self.auto_xsp_code(ax) % 2 == 1
                                && self.xsp_ok_after(cx)
                            {
                                let z = self.new_glue(xs)?;
                                self.mem
                                    .set_subtype(z, crate::eqtb::XKANJI_SKIP_CODE as u16 + 1);
                                self.mem.set_link(z, p);
                                self.mem.set_link(q, z);
                            }
                            let mut t = t;
                            while self.mem.link(t) != NULL {
                                t = self.mem.link(t);
                            }
                            insert_skip = if self.mem.is_char_node(t)
                                && self.auto_xsp_code(i32::from(self.mem.character(t))) >= 2
                            {
                                InsertSkip::AfterSchar
                            } else {
                                InsertSkip::NoSkip
                            };
                        }
                    }
                    KERN_NODE => {
                        if self.mem.subtype(p) == crate::nodes::EXPLICIT {
                            insert_skip = InsertSkip::NoSkip;
                        }
                    }
                    crate::nodes::MARK_NODE
                    | crate::nodes::ADJUST_NODE
                    | crate::nodes::INS_NODE
                    | crate::nodes::WHATSIT_NODE => {} // vanish when typeset
                    _ => insert_skip = InsertSkip::NoSkip,
                }
                q = p;
                prev = p;
                p = self.mem.link(p);
            }
        }
        // A5: \jcharwidowpenalty before the paragraph's last Japanese
        // character (pTeX inserts it only for paragraphs of >5 chars).
        if pf && char_count > 5 && last_jk_prev != NULL {
            let pen = self.eqtb.int_par(crate::eqtb::JCHR_WIDOW_PENALTY_CODE);
            let target = self.mem.link(last_jk_prev);
            if pen != 0 && target != NULL && self.is_kanji_head(target) {
                let z = self.new_penalty(pen)?;
                self.mem.set_subtype(z, WIDOW_PENA);
                self.mem.set_link(z, target);
                self.mem.set_link(last_jk_prev, z);
                // The penalty now separates two Japanese characters:
                // materialise the \kanjiskip like the K-K pass does.
                if self.is_kanji_code_node(last_jk_prev) {
                    let g = self.new_glue(u)?;
                    self.mem
                        .set_subtype(g, crate::eqtb::KANJI_SKIP_CODE as u16 + 1);
                    self.mem.set_link(g, target);
                    self.mem.set_link(z, g);
                }
            }
        }
        // A JFM glue left at the very end of the list gets a zero spec.
        if q != NULL
            && !self.mem.is_char_node(q)
            && self.mem.node_type(q) == GLUE_NODE
            && self.mem.subtype(q) == JFM_SKIP + 1
        {
            let spec = self.mem.glue_ptr(q);
            self.delete_glue_ref(spec);
            let zg = self.mem.zero_glue();
            self.mem.set_glue_ptr(q, zg);
            let c = self.mem.glue_ref_count(zg);
            self.mem.set_glue_ref_count(zg, c + 1);
        }
        Ok(())
    }
}

/// Default \kansujichar digits (ptex-base.ch: kansuji_char(0..9)).
pub const KANSUJI_DEFAULTS: [i32; 10] = [
    0x3007, 0x4E00, 0x4E8C, 0x4E09, 0x56DB, 0x4E94, 0x516D, 0x4E03, 0x516B, 0x4E5D,
];

impl crate::engine::Engine {
    /// pTeX `print_kansuji(n)`: prints n using the \kansujichar digits.
    pub fn print_kansuji(&mut self, n: i32) {
        let mut n = n;
        if n < 0 {
            return; // pTeX prints nothing for negatives
        }
        let base = self.eqtb.lay.kansuji_base;
        let mut digits = Vec::new();
        loop {
            digits.push((n % 10) as usize);
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for &d in digits.iter().rev() {
            let c = self.eqtb.equiv(base + d as i32);
            self.print_char(c);
        }
    }
}
