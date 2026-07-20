//! e-TeX expressions: `\numexpr`, `\dimexpr`, `\glueexpr`, `\muexpr`.
//!
//! Ports etex.ch's `scan_expr` with its exact-arithmetic helpers
//! `add_or_sub`, `quotient` and `fract` (double-precision simulation; no
//! floating point, so results are identical on every platform).

use crate::engine::Engine;
use crate::error::TexResult;
use crate::scan::{DIMEN_VAL, GLUE_VAL, INT_VAL, MAX_DIMEN, MU_VAL};
use crate::tokens::OTHER_TOKEN;
use crate::types::{Pointer, NULL};

/// `infinity` (§445).
const INFINITY: i32 = 0o17777777777;

// etex.ch: states of the expression evaluator.
const EXPR_NONE: i32 = 0;
const EXPR_ADD: i32 = 1;
const EXPR_SUB: i32 = 2;
const EXPR_MULT: i32 = 3;
const EXPR_DIV: i32 = 4;
const EXPR_SCALE: i32 = 5;

/// `expr_node_size` (etex.ch): stack entry for parenthesized
/// subexpressions; e/t/n live in words 1..3.
const EXPR_NODE_SIZE: i32 = 4;

impl Engine {
    /// `scan_normal_glue` (etex.ch).
    pub fn scan_normal_glue(&mut self) -> TexResult<()> {
        self.scan_glue(GLUE_VAL)
    }

    /// `scan_mu_glue` (etex.ch).
    pub fn scan_mu_glue(&mut self) -> TexResult<()> {
        self.scan_glue(MU_VAL)
    }

    /// `add_or_sub(x, y, max_answer, negative)` (etex.ch).
    fn add_or_sub(&mut self, x: i32, y: i32, max_answer: i32, negative: bool) -> i32 {
        let y = if negative { -y } else { y };
        if x >= 0 {
            if y <= max_answer - x {
                x + y
            } else {
                self.arith.arith_error = true;
                0
            }
        } else if y >= -max_answer - x {
            x + y
        } else {
            self.arith.arith_error = true;
            0
        }
    }

    /// `quotient(n, d)` (etex.ch): the rounded quotient.
    fn quotient(&mut self, n: i32, d: i32) -> i32 {
        if d == 0 {
            self.arith.arith_error = true;
            return 0;
        }
        let (mut n, mut d) = (n, d);
        let mut negative = false;
        if d < 0 {
            d = -d;
            negative = true;
        }
        if n < 0 {
            n = -n;
            negative = !negative;
        }
        let mut a = n / d;
        let n = n - a * d;
        let d = n - d; // avoid certain compiler optimizations!
        if d + n >= 0 {
            a += 1;
        }
        if negative {
            -a
        } else {
            a
        }
    }

    /// `fract(x, n, d, max_answer)` (etex.ch): ⌊xn/d + 1/2⌋ in simulated
    /// double precision.
    // `x = t + x` mirrors etex.ch's "(x-d)+x", which avoids overflow where
    // "(x+x)-d" would not.
    #[allow(clippy::assign_op_pattern)]
    fn fract(&mut self, x: i32, n: i32, d: i32, max_answer: i32) -> i32 {
        let too_big = |e: &mut Engine| {
            e.arith.arith_error = true;
            0
        };
        if d == 0 {
            return too_big(self);
        }
        let (mut x, mut n, mut d) = (x, n, d);
        let mut a: i32;
        let mut negative = false;
        if d < 0 {
            d = -d;
            negative = true;
        }
        match x.cmp(&0) {
            std::cmp::Ordering::Less => {
                x = -x;
                negative = !negative;
            }
            std::cmp::Ordering::Equal => return 0,
            std::cmp::Ordering::Greater => {}
        }
        if n < 0 {
            n = -n;
            negative = !negative;
        }
        'done: {
            let mut t = n / d;
            if t > max_answer / x {
                return too_big(self);
            }
            a = t * x;
            n -= t * d;
            if n == 0 {
                break 'done;
            }
            t = x / d;
            if t > (max_answer - a) / n {
                return too_big(self);
            }
            a += t * n;
            x -= t * d;
            if x == 0 {
                break 'done;
            }
            if x < n {
                std::mem::swap(&mut x, &mut n);
            }
            // now 0 < n <= x < d: compute f = ⌊xn/d + 1/2⌋.
            let mut f: i32 = 0;
            let mut r = (d / 2) - d;
            let h = -r;
            loop {
                if n % 2 == 1 {
                    r += x;
                    if r >= 0 {
                        r -= d;
                        f += 1;
                    }
                }
                n /= 2;
                if n == 0 {
                    break;
                }
                if x < h {
                    x += x;
                } else {
                    let t = x - d;
                    x = t + x;
                    f += n;
                    if x < n {
                        if x == 0 {
                            break;
                        }
                        std::mem::swap(&mut x, &mut n);
                    }
                }
            }
            if f > max_answer - a {
                return too_big(self);
            }
            a += f;
        }
        if negative {
            -a
        } else {
            a
        }
    }

    /// `fract` for callers outside this module (\lastlinefit).
    pub(crate) fn expr_fract(&mut self, x: i32, n: i32, d: i32, max_answer: i32) -> i32 {
        self.fract(x, n, d, max_answer)
    }

    /// `normalize_glue` (etex.ch).
    fn normalize_glue(&mut self, p: Pointer) {
        if self.mem.stretch(p) == 0 {
            self.mem.set_stretch_order(p, crate::mem::NORMAL);
        }
        if self.mem.shrink(p) == 0 {
            self.mem.set_shrink_order(p, crate::mem::NORMAL);
        }
    }

    /// `scan_expr` (etex.ch): scans and evaluates an expression of the
    /// type in `cur_val_level`.
    pub fn scan_expr(&mut self) -> TexResult<()> {
        let mut l = self.cur_val_level; // type of expression
        let a = self.arith.arith_error;
        let mut b = false;
        let mut p: Pointer = NULL; // top of the expression stack
        let mut r: i32; // state of the expression so far
        let mut s: i32; // state of the term so far
        let mut e: i32; // expression so far
        let mut t: i32; // term so far
        let mut n: i32; // numerator of combined multiply/divide
        'restart: loop {
            r = EXPR_NONE;
            e = 0;
            s = EXPR_NONE;
            t = 0;
            n = 0;
            'cont: loop {
                let o_type = if s == EXPR_NONE { l } else { INT_VAL };
                // Scan a factor f of type o_type or start a subexpression.
                self.get_next_nonblank_noncall()?;
                if self.cur_tok == OTHER_TOKEN + '(' as i32 {
                    // Push the expression stack and restart.
                    let q = self.mem.get_node(EXPR_NODE_SIZE)?;
                    self.mem.set_link(q, p);
                    self.mem.set_node_type(q, u16::from(l));
                    self.mem.set_subtype(q, (4 * s + r) as u16);
                    self.mem.word_mut(q + 1).set_int(e);
                    self.mem.word_mut(q + 2).set_int(t);
                    self.mem.word_mut(q + 3).set_int(n);
                    p = q;
                    l = o_type;
                    continue 'restart;
                }
                self.back_input()?;
                match o_type {
                    INT_VAL => self.scan_int()?,
                    DIMEN_VAL => self.scan_normal_dimen()?,
                    GLUE_VAL => self.scan_normal_glue()?,
                    _ => self.scan_mu_glue()?,
                }
                let mut f = self.cur_val;
                'found: loop {
                    // Scan the next operator and set o.
                    self.get_next_nonblank_noncall()?;
                    let mut o = if self.cur_tok == OTHER_TOKEN + '+' as i32 {
                        EXPR_ADD
                    } else if self.cur_tok == OTHER_TOKEN + '-' as i32 {
                        EXPR_SUB
                    } else if self.cur_tok == OTHER_TOKEN + '*' as i32 {
                        EXPR_MULT
                    } else if self.cur_tok == OTHER_TOKEN + '/' as i32 {
                        EXPR_DIV
                    } else {
                        if p == NULL {
                            if self.cur_cmd != crate::cmds::RELAX {
                                self.back_input()?;
                            }
                        } else if self.cur_tok != OTHER_TOKEN + ')' as i32 {
                            self.print_err("Missing ) inserted for expression");
                            self.help(&[
                                "I was expecting to see `+', `-', `*', `/', or `)'. Didn't.",
                            ]);
                            self.back_error()?;
                        }
                        EXPR_NONE
                    };
                    self.arith.arith_error = b;
                    // Make sure that f is in the proper range. (Integers
                    // can't exceed |infinity| = i32::MAX here, so etex.ch's
                    // first check is vacuous in this port.)
                    if l == INT_VAL || s > EXPR_SUB {
                    } else if l == DIMEN_VAL {
                        if f.abs() > MAX_DIMEN {
                            self.arith.arith_error = true;
                            f = 0;
                        }
                    } else if self.mem.width(f).abs() > MAX_DIMEN
                        || self.mem.stretch(f).abs() > MAX_DIMEN
                        || self.mem.shrink(f).abs() > MAX_DIMEN
                    {
                        self.arith.arith_error = true;
                        self.mem.delete_glue_ref(f);
                        let zg = self.mem.zero_glue();
                        f = self.new_spec(zg)?;
                    }
                    // Cases for evaluation of the current term.
                    match s {
                        EXPR_NONE => {
                            if l >= GLUE_VAL && o != EXPR_NONE {
                                t = self.new_spec(f)?;
                                self.mem.delete_glue_ref(f);
                                self.normalize_glue(t);
                            } else {
                                t = f;
                            }
                        }
                        EXPR_MULT => {
                            if o == EXPR_DIV {
                                n = f;
                                o = EXPR_SCALE;
                            } else if l == INT_VAL {
                                t = crate::arith::mult_integers(&mut self.arith, t, f);
                            } else if l == DIMEN_VAL {
                                t = crate::arith::nx_plus_y(&mut self.arith, t, f, 0);
                            } else {
                                let w = crate::arith::nx_plus_y(
                                    &mut self.arith,
                                    self.mem.width(t),
                                    f,
                                    0,
                                );
                                self.mem.set_width(t, w);
                                let st = crate::arith::nx_plus_y(
                                    &mut self.arith,
                                    self.mem.stretch(t),
                                    f,
                                    0,
                                );
                                self.mem.set_stretch(t, st);
                                let sh = crate::arith::nx_plus_y(
                                    &mut self.arith,
                                    self.mem.shrink(t),
                                    f,
                                    0,
                                );
                                self.mem.set_shrink(t, sh);
                            }
                        }
                        EXPR_DIV => {
                            if l < GLUE_VAL {
                                t = self.quotient(t, f);
                            } else {
                                let w = self.quotient(self.mem.width(t), f);
                                self.mem.set_width(t, w);
                                let st = self.quotient(self.mem.stretch(t), f);
                                self.mem.set_stretch(t, st);
                                let sh = self.quotient(self.mem.shrink(t), f);
                                self.mem.set_shrink(t, sh);
                            }
                        }
                        EXPR_SCALE => {
                            if l == INT_VAL {
                                t = self.fract(t, n, f, INFINITY);
                            } else if l == DIMEN_VAL {
                                t = self.fract(t, n, f, MAX_DIMEN);
                            } else {
                                let w = self.fract(self.mem.width(t), n, f, MAX_DIMEN);
                                self.mem.set_width(t, w);
                                let st = self.fract(self.mem.stretch(t), n, f, MAX_DIMEN);
                                self.mem.set_stretch(t, st);
                                let sh = self.fract(self.mem.shrink(t), n, f, MAX_DIMEN);
                                self.mem.set_shrink(t, sh);
                            }
                        }
                        _ => {}
                    }
                    if o > EXPR_SUB {
                        s = o;
                    } else {
                        // Evaluate the current expression.
                        s = EXPR_NONE;
                        if r == EXPR_NONE {
                            e = t;
                        } else if l == INT_VAL {
                            e = self.add_or_sub(e, t, INFINITY, r == EXPR_SUB);
                        } else if l == DIMEN_VAL {
                            e = self.add_or_sub(e, t, MAX_DIMEN, r == EXPR_SUB);
                        } else {
                            // Compute the sum or difference of two glue specs.
                            let neg = r == EXPR_SUB;
                            let w = self.add_or_sub(
                                self.mem.width(e),
                                self.mem.width(t),
                                MAX_DIMEN,
                                neg,
                            );
                            self.mem.set_width(e, w);
                            if self.mem.stretch_order(e) == self.mem.stretch_order(t) {
                                let v = self.add_or_sub(
                                    self.mem.stretch(e),
                                    self.mem.stretch(t),
                                    MAX_DIMEN,
                                    neg,
                                );
                                self.mem.set_stretch(e, v);
                            } else if self.mem.stretch_order(e) < self.mem.stretch_order(t)
                                && self.mem.stretch(t) != 0
                            {
                                // N.B. etex.ch copies stretch(t) verbatim
                                // here: a differing order ignores the
                                // subtraction sign (cf. etrip l.806).
                                let v = self.mem.stretch(t);
                                self.mem.set_stretch(e, v);
                                let so = self.mem.stretch_order(t);
                                self.mem.set_stretch_order(e, so);
                            }
                            if self.mem.shrink_order(e) == self.mem.shrink_order(t) {
                                let v = self.add_or_sub(
                                    self.mem.shrink(e),
                                    self.mem.shrink(t),
                                    MAX_DIMEN,
                                    neg,
                                );
                                self.mem.set_shrink(e, v);
                            } else if self.mem.shrink_order(e) < self.mem.shrink_order(t)
                                && self.mem.shrink(t) != 0
                            {
                                let v = self.mem.shrink(t);
                                self.mem.set_shrink(e, v);
                                let so = self.mem.shrink_order(t);
                                self.mem.set_shrink_order(e, so);
                            }
                            self.mem.delete_glue_ref(t);
                            self.normalize_glue(e);
                        }
                        r = o;
                    }
                    b = self.arith.arith_error;
                    if o != EXPR_NONE {
                        continue 'cont;
                    }
                    if p != NULL {
                        // Pop the expression stack and continue at "found".
                        f = e;
                        let q = p;
                        e = self.mem.word(q + 1).int();
                        t = self.mem.word(q + 2).int();
                        n = self.mem.word(q + 3).int();
                        s = i32::from(self.mem.subtype(q)) / 4;
                        r = i32::from(self.mem.subtype(q)) % 4;
                        l = self.mem.node_type(q) as u8;
                        p = self.mem.link(q);
                        self.mem.free_node(q, EXPR_NODE_SIZE);
                        continue 'found;
                    }
                    break 'restart;
                }
            }
        }
        if b {
            self.print_err("Arithmetic overflow");
            self.help(&[
                "I can't evaluate this expression,",
                "since the result is out of range.",
            ]);
            self.error()?;
            if l >= GLUE_VAL {
                self.mem.delete_glue_ref(e);
                e = self.mem.zero_glue();
                self.mem.add_glue_ref(e);
            } else {
                e = 0;
            }
        }
        self.arith.arith_error = a;
        self.cur_val = e;
        self.cur_val_level = l;
        Ok(())
    }
}
