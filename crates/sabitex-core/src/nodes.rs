//! Data structures for boxes and their friends.
//!
//! Ports tex.web Part 10 (§133-§161): the node types that populate
//! horizontal and vertical lists, their field accessors, and the `new_*`
//! constructors. Field accessors live on [`Mem`]; constructors that need
//! eqtb (e.g. `new_param_glue`) are `Engine` methods.

use crate::engine::Engine;
use crate::error::TexResult;
use crate::mem::Mem;
use crate::types::{Pointer, Scaled, NULL};

// §134-§157: node type codes.
pub const HLIST_NODE: u16 = 0;
pub const VLIST_NODE: u16 = 1;
pub const RULE_NODE: u16 = 2;
pub const INS_NODE: u16 = 3;
pub const MARK_NODE: u16 = 4;
pub const ADJUST_NODE: u16 = 5;
pub const LIGATURE_NODE: u16 = 6;
pub const DISC_NODE: u16 = 7;
pub const WHATSIT_NODE: u16 = 8;
pub const MATH_NODE: u16 = 9;
pub const GLUE_NODE: u16 = 10;
pub const KERN_NODE: u16 = 11;
pub const PENALTY_NODE: u16 = 12;
pub const UNSET_NODE: u16 = 13;
/// pTeX `disp_node`: baseline displacement marker. ptex-base.ch gives
/// it type 5 and renumbers everything above; we keep tex.web's numbers
/// (TRIP), and 14..=31 belong to the math style/choice/noad space
/// (show_node_list serves hlists AND mlists — 15 once collided with
/// choice_node and broke TRIP), so it parks at 40. Size is
/// SMALL_NODE_SIZE; word 1 holds the displacement (sc).
pub const DISP_NODE: u16 = 40;

// Node sizes.
pub const BOX_NODE_SIZE: i32 = 7;
pub const RULE_NODE_SIZE: i32 = 4;
pub const INS_NODE_SIZE: i32 = 5;
pub const SMALL_NODE_SIZE: i32 = 2;

/// `null_flag == -2^30`: a missing ("running") dimension (§138).
pub const NULL_FLAG: Scaled = -0o10000000000;

// §135: glue_sign values.
pub const NORMAL: u16 = 0;
pub const STRETCHING: u16 = 1;
pub const SHRINKING: u16 = 2;

// §149: math node subtypes.
pub const BEFORE: u16 = 0;
pub const AFTER: u16 = 1;
// etex.ch §147: TeXXeT math-node subtypes. A math node with subtype >
// after and width = 0 records a reinserted math node or a text-direction
// primitive.
pub const M_CODE: u16 = 2;
pub const BEGIN_M_CODE: u16 = M_CODE + BEFORE; // \beginM
pub const END_M_CODE: u16 = M_CODE + AFTER; // \endM
pub const L_CODE: u16 = 4;
pub const BEGIN_L_CODE: u16 = L_CODE + BEGIN_M_CODE; // \beginL
pub const END_L_CODE: u16 = L_CODE + END_M_CODE; // \endL
pub const R_CODE: u16 = L_CODE + L_CODE;
pub const BEGIN_R_CODE: u16 = R_CODE + BEGIN_M_CODE; // \beginR
pub const END_R_CODE: u16 = R_CODE + END_M_CODE; // \endR

/// `end_LR(p)` (etex.ch): is this an end node?
pub fn end_lr(subtype: u16) -> bool {
    subtype % 2 == 1
}

/// `end_LR_type(p)` (etex.ch).
pub fn end_lr_type(subtype: u16) -> u16 {
    L_CODE * (subtype / L_CODE) + END_M_CODE
}

/// `begin_LR_type(t)` (etex.ch): from an end type to its begin type.
pub fn begin_lr_type(t: u16) -> u16 {
    t - AFTER + BEFORE
}

/// `LR_dir(p)` (etex.ch): the text direction of a direction node.
pub fn lr_dir(subtype: u16) -> u8 {
    (subtype / R_CODE) as u8
}

// etex.ch §616: box direction subtypes and directions.
pub const REVERSED: u16 = 1;
pub const DLIST: u16 = 2;
pub const LEFT_TO_RIGHT: u8 = 0;
pub const RIGHT_TO_LEFT: u8 = 1;

// §149-§155: glue/kern subtypes.
pub const COND_MATH_GLUE: u16 = 98;
pub const MU_GLUE: u16 = 99;
pub const A_LEADERS: u16 = 100;
pub const C_LEADERS: u16 = 101;
pub const X_LEADERS: u16 = 102;
pub const EXPLICIT: u16 = 1;
pub const ACC_KERN: u16 = 2;

/// `inf_penalty` / `eject_penalty` (§157).
pub const INF_PENALTY: i32 = crate::types::INF_BAD;
pub const EJECT_PENALTY: i32 = -INF_PENALTY;

impl Mem {
    /// `is_char_node(p)` (§134).
    pub fn is_char_node(&self, p: Pointer) -> bool {
        p >= self.hi_mem_min
    }

    /// `type(p)` (§133).
    pub fn node_type(&self, p: Pointer) -> u16 {
        self.word(p).b0()
    }

    pub fn set_node_type(&mut self, p: Pointer, t: u16) {
        self.word_mut(p).set_b0(t);
    }

    /// `subtype(p)` (§133).
    pub fn subtype(&self, p: Pointer) -> u16 {
        self.word(p).b1()
    }

    pub fn set_subtype(&mut self, p: Pointer, s: u16) {
        self.word_mut(p).set_b1(s);
    }

    /// `font(p)` of a char node (§134) — an alias of `type`.
    pub fn font(&self, p: Pointer) -> u16 {
        self.node_type(p)
    }

    pub fn set_font(&mut self, p: Pointer, f: u16) {
        self.set_node_type(p, f);
    }

    /// `character(p)` of a char node (§134) — an alias of `subtype`.
    pub fn character(&self, p: Pointer) -> u16 {
        self.subtype(p)
    }

    pub fn set_character(&mut self, p: Pointer, c: u16) {
        self.set_subtype(p, c);
    }

    // §135: box node fields. width/depth/height at offsets 1/2/3 are shared
    // with the glue-spec accessors already on Mem (`width`, offset 1).

    /// `depth(p) == mem[p+2].sc`.
    pub fn depth(&self, p: Pointer) -> Scaled {
        self.word(p + 2).sc()
    }

    pub fn set_depth(&mut self, p: Pointer, v: Scaled) {
        self.word_mut(p + 2).set_sc(v);
    }

    /// `height(p) == mem[p+3].sc`.
    pub fn height(&self, p: Pointer) -> Scaled {
        self.word(p + 3).sc()
    }

    pub fn set_height(&mut self, p: Pointer, v: Scaled) {
        self.word_mut(p + 3).set_sc(v);
    }

    /// `shift_amount(p) == mem[p+4].sc`.
    pub fn shift_amount(&self, p: Pointer) -> Scaled {
        self.word(p + 4).sc()
    }

    pub fn set_shift_amount(&mut self, p: Pointer, v: Scaled) {
        self.word_mut(p + 4).set_sc(v);
    }

    /// `list_ptr(p) == link(p+5)`.
    pub fn list_ptr(&self, p: Pointer) -> Pointer {
        self.link(p + 5)
    }

    pub fn set_list_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_link(p + 5, v);
    }

    /// `glue_order(p) == subtype(p+5)`.
    pub fn glue_order(&self, p: Pointer) -> u16 {
        self.subtype(p + 5)
    }

    pub fn set_glue_order(&mut self, p: Pointer, v: u16) {
        self.set_subtype(p + 5, v);
    }

    /// `glue_sign(p) == type(p+5)`.
    pub fn glue_sign(&self, p: Pointer) -> u16 {
        self.node_type(p + 5)
    }

    pub fn set_glue_sign(&mut self, p: Pointer, v: u16) {
        self.set_node_type(p + 5, v);
    }

    /// `glue_set(p) == mem[p+6].gr`.
    pub fn glue_set(&self, p: Pointer) -> f64 {
        self.word(p + 6).gr()
    }

    pub fn set_glue_set(&mut self, p: Pointer, v: f64) {
        if self.glue_ratio_wide {
            self.word_mut(p + 6).set_gr_wide(v);
        } else {
            self.word_mut(p + 6).set_gr(v);
        }
    }

    /// `is_running(d)` (§138).
    pub fn is_running(d: Scaled) -> bool {
        d == NULL_FLAG
    }

    /// `float_cost(p) == mem[p+1].int` (§140).
    pub fn float_cost(&self, p: Pointer) -> i32 {
        self.word(p + 1).int()
    }

    pub fn set_float_cost(&mut self, p: Pointer, v: i32) {
        self.word_mut(p + 1).set_int(v);
    }

    /// `ins_ptr(p) == info(p+4)` (§140).
    pub fn ins_ptr(&self, p: Pointer) -> Pointer {
        self.info(p + 4)
    }

    pub fn set_ins_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_info(p + 4, v);
    }

    /// `split_top_ptr(p) == link(p+4)` (§140).
    pub fn split_top_ptr(&self, p: Pointer) -> Pointer {
        self.link(p + 4)
    }

    pub fn set_split_top_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_link(p + 4, v);
    }

    /// `mark_ptr(p) == link(p+1)` (§141 + etex.ch, which packs the mark
    /// class into the other halfword).
    pub fn mark_ptr(&self, p: Pointer) -> Pointer {
        self.link(p + 1)
    }

    pub fn set_mark_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_link(p + 1, v);
    }

    /// `mark_class(p) == info(p+1)` (etex.ch).
    pub fn mark_class(&self, p: Pointer) -> i32 {
        self.info(p + 1)
    }

    pub fn set_mark_class(&mut self, p: Pointer, v: i32) {
        self.set_info(p + 1, v);
    }

    /// `adjust_ptr(p) == mem[p+1].int` (§142; etex.ch separates it from
    /// `mark_ptr`).
    pub fn adjust_ptr(&self, p: Pointer) -> Pointer {
        self.word(p + 1).int()
    }

    pub fn set_adjust_ptr(&mut self, p: Pointer, v: Pointer) {
        self.word_mut(p + 1).set_int(v);
    }

    /// `lig_char(p) == p+1` (§143): the word holding the ligature char.
    pub fn lig_char(&self, p: Pointer) -> Pointer {
        p + 1
    }

    /// `lig_ptr(p) == link(lig_char(p))` (§143).
    pub fn lig_ptr(&self, p: Pointer) -> Pointer {
        self.link(p + 1)
    }

    pub fn set_lig_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_link(p + 1, v);
    }

    /// `replace_count(p) == subtype(p)` (§145).
    pub fn replace_count(&self, p: Pointer) -> u16 {
        self.subtype(p)
    }

    pub fn set_replace_count(&mut self, p: Pointer, v: u16) {
        self.set_subtype(p, v);
    }

    /// `pre_break(p) == llink(p)` (§145).
    pub fn pre_break(&self, p: Pointer) -> Pointer {
        self.llink(p)
    }

    pub fn set_pre_break(&mut self, p: Pointer, v: Pointer) {
        self.set_llink(p, v);
    }

    /// `post_break(p) == rlink(p)` (§145).
    pub fn post_break(&self, p: Pointer) -> Pointer {
        self.rlink(p)
    }

    pub fn set_post_break(&mut self, p: Pointer, v: Pointer) {
        self.set_rlink(p, v);
    }

    /// `precedes_break(p)` / `non_discardable(p)` (§148).
    pub fn precedes_break(&self, p: Pointer) -> bool {
        self.node_type(p) < MATH_NODE
    }

    pub fn non_discardable(&self, p: Pointer) -> bool {
        self.node_type(p) < MATH_NODE
    }

    /// `glue_ptr(p) == llink(p)` (§149).
    pub fn glue_ptr(&self, p: Pointer) -> Pointer {
        self.llink(p)
    }

    pub fn set_glue_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_llink(p, v);
    }

    /// `leader_ptr(p) == rlink(p)` (§149).
    pub fn leader_ptr(&self, p: Pointer) -> Pointer {
        self.rlink(p)
    }

    pub fn set_leader_ptr(&mut self, p: Pointer, v: Pointer) {
        self.set_rlink(p, v);
    }

    /// `penalty(p) == mem[p+1].int` (§157).
    pub fn penalty(&self, p: Pointer) -> i32 {
        self.word(p + 1).int()
    }

    pub fn set_penalty(&mut self, p: Pointer, v: i32) {
        self.word_mut(p + 1).set_int(v);
    }

    /// `glue_stretch(p) == mem[p+glue_offset].sc` (§159, unset nodes).
    pub fn glue_stretch(&self, p: Pointer) -> Scaled {
        self.word(p + 6).sc()
    }

    pub fn set_glue_stretch(&mut self, p: Pointer, v: Scaled) {
        self.word_mut(p + 6).set_sc(v);
    }

    /// `glue_shrink(p) == shift_amount(p)` (§159).
    pub fn glue_shrink(&self, p: Pointer) -> Scaled {
        self.shift_amount(p)
    }

    pub fn set_glue_shrink(&mut self, p: Pointer, v: Scaled) {
        self.set_shift_amount(p, v);
    }

    /// `span_count(p) == subtype(p)` (§159).
    pub fn span_count(&self, p: Pointer) -> u16 {
        self.subtype(p)
    }

    pub fn set_span_count(&mut self, p: Pointer, v: u16) {
        self.set_subtype(p, v);
    }
}

/// `is_running(d)` (§138): tests for a running dimension.
pub fn is_running(d: Scaled) -> bool {
    d == NULL_FLAG
}

impl Engine {
    /// `new_null_box` (§136).
    pub fn new_null_box(&mut self) -> TexResult<Pointer> {
        let p = self.mem.get_node(BOX_NODE_SIZE)?;
        self.mem.set_node_type(p, HLIST_NODE);
        self.mem.set_subtype(p, 0);
        self.mem.set_width(p, 0);
        self.mem.set_depth(p, 0);
        self.mem.set_height(p, 0);
        self.mem.set_shift_amount(p, 0);
        self.mem.set_list_ptr(p, NULL);
        self.mem.set_glue_sign(p, NORMAL);
        self.mem.set_glue_order(p, NORMAL);
        self.mem.set_glue_set(p, 0.0);
        Ok(p)
    }

    /// `new_rule` (§139): all dimensions "running".
    pub fn new_rule(&mut self) -> TexResult<Pointer> {
        let p = self.mem.get_node(RULE_NODE_SIZE)?;
        self.mem.set_node_type(p, RULE_NODE);
        self.mem.set_subtype(p, 0);
        self.mem.set_width(p, NULL_FLAG);
        self.mem.set_depth(p, NULL_FLAG);
        self.mem.set_height(p, NULL_FLAG);
        Ok(p)
    }

    /// `new_ligature(f, c, q)` (§144).
    pub fn new_ligature(&mut self, f: u16, c: u16, q: Pointer) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, LIGATURE_NODE);
        self.mem.set_font(p + 1, f);
        self.mem.set_character(p + 1, c);
        self.mem.set_lig_ptr(p, q);
        self.mem.set_subtype(p, 0);
        Ok(p)
    }

    /// `new_lig_item(c)` (§144).
    pub fn new_lig_item(&mut self, c: u16) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_character(p, c);
        self.mem.set_lig_ptr(p, NULL);
        Ok(p)
    }

    /// `new_disc` (§145).
    pub fn new_disc(&mut self) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, DISC_NODE);
        self.mem.set_replace_count(p, 0);
        self.mem.set_pre_break(p, NULL);
        self.mem.set_post_break(p, NULL);
        Ok(p)
    }

    /// `new_math(w, s)` (§147).
    pub fn new_math(&mut self, w: Scaled, s: u16) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, MATH_NODE);
        self.mem.set_subtype(p, s);
        self.mem.set_width(p, w);
        Ok(p)
    }

    /// `new_param_glue(n)` (§152).
    pub fn new_param_glue(&mut self, n: i32) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, GLUE_NODE);
        self.mem.set_subtype(p, (n + 1) as u16);
        self.mem.set_leader_ptr(p, NULL);
        let q = self.eqtb.glue_par(n);
        self.mem.set_glue_ptr(p, q);
        let c = self.mem.glue_ref_count(q);
        self.mem.set_glue_ref_count(q, c + 1);
        Ok(p)
    }

    /// `new_glue(q)` (§153).
    pub fn new_glue(&mut self, q: Pointer) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, GLUE_NODE);
        self.mem.set_subtype(p, NORMAL);
        self.mem.set_leader_ptr(p, NULL);
        self.mem.set_glue_ptr(p, q);
        let c = self.mem.glue_ref_count(q);
        self.mem.set_glue_ref_count(q, c + 1);
        Ok(p)
    }

    /// `new_skip_param(n)` (§154): fresh spec copy; returns (node, spec).
    pub fn new_skip_param(&mut self, n: i32) -> TexResult<(Pointer, Pointer)> {
        let par = self.eqtb.glue_par(n);
        let spec = self.new_spec(par)?;
        let p = self.new_glue(spec)?;
        self.mem.set_glue_ref_count(spec, NULL);
        self.mem.set_subtype(p, (n + 1) as u16);
        Ok((p, spec))
    }

    /// `new_kern(w)` (§156).
    pub fn new_kern(&mut self, w: Scaled) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, KERN_NODE);
        self.mem.set_subtype(p, NORMAL);
        self.mem.set_width(p, w);
        Ok(p)
    }

    /// `new_penalty(m)` (§158).
    pub fn new_penalty(&mut self, m: i32) -> TexResult<Pointer> {
        let p = self.mem.get_node(SMALL_NODE_SIZE)?;
        self.mem.set_node_type(p, PENALTY_NODE);
        self.mem.set_subtype(p, 0);
        self.mem.set_penalty(p, m);
        Ok(p)
    }
}
