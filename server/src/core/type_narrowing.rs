use ruff_python_ast::{Expr, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// A recognized `isinstance(x, T)` / `isinstance(x, (T, U))` check (or its `not`-negated
/// form) on a plain local variable or parameter. Attribute targets (`self.x`) aren't
/// recognized yet.
pub struct IsinstanceCheck<'a> {
    pub target_name: &'a str,
    pub target_range: TextRange,
    pub type_exprs: Vec<&'a Expr>,
    pub negated: bool,
}

/// Range for the synthetic narrowed re-declaration placed at the start of a body. Offset by
/// one byte from `body_start` so it can't collide with a real declaration starting exactly
/// there (e.g. the body's first statement reassigning the same name).
pub fn narrowing_range(body_start: TextSize) -> TextRange {
    let start = body_start + TextSize::new(1);
    TextRange::new(start, start)
}

pub fn match_isinstance_check(test: &Expr) -> Option<IsinstanceCheck<'_>> {
    if let Expr::UnaryOp(unary) = test {
        if !matches!(unary.op, UnaryOp::Not) {
            return None;
        }
        let mut check = match_isinstance_call(&unary.operand)?;
        check.negated = true;
        return Some(check);
    }
    match_isinstance_call(test)
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
    let type_exprs = match &call.arguments.args[1] {
        Expr::Tuple(tuple) => tuple.elts.iter().collect(),
        other => vec![other],
    };
    Some(IsinstanceCheck {
        target_name: target.id.as_str(),
        target_range: target.range(),
        type_exprs,
        negated: false,
    })
}
