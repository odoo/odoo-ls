use crate::{
    constants::OYarn,
    core::{
        evaluation::{Evaluation, EvaluationValue},
        symbols::symbol_keys::SymbolKey,
    },
    oyarn,
    threads::SessionInfo,
};

use ruff_python_ast::Expr;
use ruff_text_size::TextSize;

/// `(key, label)`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectionValue(pub OYarn, pub OYarn);

/// The values of one `selection` or `selection_add` argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeclaredSelection {
    Known(Vec<SelectionValue>),
    /// A method, a lambda, or a list odoo-ls can't read
    Unknown,
}

impl DeclaredSelection {
    /// Reads a literal list, or a constant holding one.
    pub fn from_argument(
        session: &mut SessionInfo,
        argument: &Expr,
        parent: SymbolKey,
        max_infer: &TextSize,
    ) -> Self {
        // Literals don't need an evaluation
        if let Some(items) = sequence_items(argument) {
            return Self::from_items(items);
        }
        match Evaluation::expr_to_const(session, argument, parent, max_infer, false, &mut vec![]) {
            Some(EvaluationValue::LIST(items) | EvaluationValue::TUPLE(items)) => {
                Self::from_items(&items)
            }
            _ => Self::Unknown,
        }
    }

    fn from_items(items: &[Expr]) -> Self {
        let mut values = vec![];
        for item in items {
            match sequence_items(item) {
                // `selection_add` 1-tuples only reorder the values
                Some([Expr::StringLiteral(_)]) => {}
                Some([Expr::StringLiteral(key), Expr::StringLiteral(label)]) => values.push(
                    SelectionValue(oyarn!("{}", key.value), oyarn!("{}", label.value)),
                ),
                _ => return Self::Unknown,
            }
        }
        Self::Known(values)
    }
}

/// Returns the items of a list or tuple literal.
fn sequence_items(expr: &Expr) -> Option<&[Expr]> {
    match expr {
        Expr::List(list) => Some(&list.elts),
        Expr::Tuple(tuple) => Some(&tuple.elts),
        _ => None,
    }
}
