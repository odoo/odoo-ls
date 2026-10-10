//! Shared vocabulary for the two build phases: ARCH declares the synthetic narrowing symbols,
//! ARCH_EVAL finds them again *by position*, so every anchor must be derived identically by both.

use ruff_python_ast::{BoolOp, Expr, ExprBoolOp, Operator, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// An `isinstance` check on a plain name. Attribute targets (`self.x`) aren't recognized yet.
pub struct IsinstanceCheck<'a> {
    pub target_name: &'a str,
    pub target_range: TextRange,
    pub type_exprs: Vec<&'a Expr>,
}

/// Empty range for a synthetic narrowed re-declaration, at `pos`.
pub fn narrowing_range(pos: TextSize) -> TextRange {
    TextRange::new(pos, pos)
}

/// Anchor for a narrowing covering everything after a statement, in the dead space past it.
pub fn narrowing_anchor_after(stmt_end: TextSize) -> TextSize {
    stmt_end + TextSize::new(1)
}

/// Anchor for a loop's normal exit: before `orelse` if there is one, else past the loop.
pub fn loop_exit_anchor(orelse: &[Stmt], loop_end: TextSize) -> TextSize {
    orelse.first().map_or_else(|| narrowing_anchor_after(loop_end), |stmt| stmt.range().start())
}

/// The checks that hold when `test` evaluated to `!want_negated`.
pub fn match_narrowing_checks(test: &Expr, want_negated: bool) -> Vec<IsinstanceCheck<'_>> {
    let mut checks = Vec::new();
    collect_narrowing_checks(test, want_negated, &mut checks);
    // Two declarations at one position are indistinguishable to the lookup, so one name keeps
    // only its last check. Leaves `deduped` in reverse source order.
    let mut deduped: Vec<IsinstanceCheck<'_>> = Vec::new();
    for check in checks.into_iter().rev() {
        if !deduped.iter().any(|kept| kept.target_name == check.target_name) {
            deduped.push(check);
        }
    }
    deduped
}

fn collect_narrowing_checks<'a>(test: &'a Expr, want_negated: bool, out: &mut Vec<IsinstanceCheck<'a>>) {
    if let Expr::BoolOp(bool_op) = test {
        if matches!(bool_op.op, BoolOp::And) == !want_negated {
            // A true `and` or a false `or`: every operand's outcome is known.
            for value in bool_op.values.iter() {
                collect_narrowing_checks(value, want_negated, out);
            }
        } else {
            intersect_operand_checks(bool_op, want_negated, out);
        }
        return;
    }
    if let Some((negated, check)) = match_isinstance_check(test)
        && negated == want_negated {
            out.push(check);
        }
}

/// A true `or` or a false `and` says only that *some* operand decided it. Any of them could
/// have, so a name is narrowed only if every operand narrows it, to the union of what they allow:
/// `isinstance(a, Dog) or isinstance(a, Cat)` gives `Dog | Cat`, `or flag` gives nothing.
fn intersect_operand_checks<'a>(bool_op: &'a ExprBoolOp, want_negated: bool, out: &mut Vec<IsinstanceCheck<'a>>) {
    let per_operand: Vec<Vec<IsinstanceCheck<'a>>> = bool_op.values.iter().map(|value| {
        let mut checks = Vec::new();
        collect_narrowing_checks(value, want_negated, &mut checks);
        checks
    }).collect();
    let Some((first, rest)) = per_operand.split_first() else { return };
    for check in first.iter() {
        let Some(same_name) = rest.iter()
            .map(|operand| operand.iter().find(|other| other.target_name == check.target_name))
            .collect::<Option<Vec<_>>>() else { continue };
        out.push(IsinstanceCheck {
            target_name: check.target_name,
            target_range: check.target_range,
            type_exprs: check.type_exprs.iter()
                .chain(same_name.iter().flat_map(|other| other.type_exprs.iter()))
                .copied().collect(),
        });
    }
}

/// `(negated, check)`
fn match_isinstance_check(test: &Expr) -> Option<(bool, IsinstanceCheck<'_>)> {
    if let Expr::UnaryOp(unary) = test {
        if !matches!(unary.op, UnaryOp::Not) {
            return None;
        }
        return Some((true, match_isinstance_call(&unary.operand)?));
    }
    Some((false, match_isinstance_call(test)?))
}

fn match_isinstance_call(expr: &Expr) -> Option<IsinstanceCheck<'_>> {
    let Expr::Call(call) = expr else { return None };
    let Expr::Name(func_name) = call.func.as_ref() else { return None };
    if func_name.id.as_str() != "isinstance" {
        return None;
    }
    if call.arguments.args.len() != 2 || !call.arguments.keywords.is_empty() {
        return None;
    }
    let Expr::Name(target) = &call.arguments.args[0] else { return None };
    let mut type_exprs = Vec::new();
    collect_type_exprs(&call.arguments.args[1], &mut type_exprs);
    Some(IsinstanceCheck {
        target_name: target.id.as_str(),
        target_range: target.range(),
        type_exprs,
    })
}

/// The types `isinstance` will accept: a tuple or a `X | Y` union, nested either way.
fn collect_type_exprs<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    match expr {
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| collect_type_exprs(elt, out)),
        Expr::BinOp(bin_op) if matches!(bin_op.op, Operator::BitOr) => {
            collect_type_exprs(&bin_op.left, out);
            collect_type_exprs(&bin_op.right, out);
        },
        other => out.push(other),
    }
}
