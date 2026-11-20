use crate::{
    constants::OYarn,
    core::{
        evaluation::{Evaluation, EvaluationValue},
        evaluation_context::{Context, ContextKey, ContextValue},
        python_arch_eval::PythonArchEval,
        symbols::{
            SymbolTable,
            storage::{FileContentParent, xml::xml_field_symbol::XmlFieldName},
            symbol_keys::{ModelSymbolKey, ModuleKey, SymbolKey, VariableKey, XmlRecordKey},
        },
    },
    oyarn,
    threads::SessionInfo,
    utils::HashSet,
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

    /// Reads a list written as Python source, as in XML.
    fn from_text(text: &str) -> Self {
        ruff_python_parser::parse_expression(text.trim())
            .ok()
            .and_then(|parsed| sequence_items(parsed.expr()).map(Self::from_items))
            .unwrap_or(Self::Unknown)
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

/// The values of a Selection field, merged from all its declarations.
#[derive(Debug, Clone)]
pub struct MergedSelection {
    pub values: Vec<SelectionValue>,
    /// Whether `values` holds all possible values
    pub complete: bool,
}

impl MergedSelection {
    /// Returns `None` if `field_name` isn't a Selection field.
    pub fn from_model_field(
        session: &mut SessionInfo,
        model: ModelSymbolKey,
        field_name: &str,
        from_module: Option<ModuleKey>,
    ) -> Option<Self> {
        let declarations = SymbolTable::get_member_symbol(
            session,
            model.into(),
            field_name,
            from_module,
            false,
            true,
            false,
            true,
            false,
        )
        .0;
        Self::from_declarations(session, &declarations, from_module, &mut HashSet::default())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.values
            .iter()
            .any(|SelectionValue(known, _)| known.as_str() == key)
    }

    /// Merges all `declarations`, as their runtime order is unknown.
    /// `visited` stops `related` loops.
    fn from_declarations(
        session: &mut SessionInfo,
        declarations: &[SymbolKey],
        from_module: Option<ModuleKey>,
        visited: &mut HashSet<SymbolKey>,
    ) -> Option<Self> {
        declarations
            .iter()
            .fold(SelectionMerge::default(), |merge, &declaration| {
                merge.add_declaration(session, declaration, from_module, visited)
            })
            .finish()
    }
}

#[derive(Default)]
struct SelectionMerge {
    values: Vec<SelectionValue>,
    is_selection: bool,
    /// Has `selection` or `related`, not only `selection_add`
    has_base: bool,
    /// Has values odoo-ls can't read
    has_unknown: bool,
}

impl SelectionMerge {
    fn add_declaration(
        mut self,
        session: &mut SessionInfo,
        declaration: SymbolKey,
        from_module: Option<ModuleKey>,
        visited: &mut HashSet<SymbolKey>,
    ) -> Self {
        if visited.insert(declaration) {
            match declaration {
                SymbolKey::XmlRecord(record) => self.add_xml_field(session, record),
                SymbolKey::Variable(variable) => {
                    self.add_python_field(session, variable, from_module, visited)
                }
                _ => {}
            }
        }
        self
    }

    fn add_xml_field(&mut self, session: &SessionInfo, record: XmlRecordKey) {
        let record = &session.st()[record];
        if record
            .get_field_text(XmlFieldName::Type, session.st())
            .as_deref()
            != Some("selection")
        {
            return;
        }
        self.is_selection = true;
        // `ir.model.fields.selection` records can add values
        self.has_unknown = true;
        if let Some(text) = record.get_field_text(XmlFieldName::Selection, session.st()) {
            self.add(&DeclaredSelection::from_text(&text));
        }
    }

    fn add_python_field(
        &mut self,
        session: &mut SessionInfo,
        variable: VariableKey,
        from_module: Option<ModuleKey>,
        visited: &mut HashSet<SymbolKey>,
    ) {
        let FileContentParent::Class(class) = session.st()[variable].parent() else {
            return;
        };
        let contexts = selection_field_contexts(session, variable);
        self.is_selection |= !contexts.is_empty();
        for entry in contexts.iter().flat_map(Context::iter) {
            match entry {
                (ContextKey::SelectionValues, ContextValue::SELECTION(declared)) => {
                    self.has_base = true;
                    self.add(declared);
                }
                (ContextKey::SelectionAdd, ContextValue::SELECTION(declared)) => self.add(declared),
                (ContextKey::Related, ContextValue::STRING(related)) => {
                    self.has_base = true;
                    let targets =
                        PythonArchEval::get_nested_sub_field(session, related, class, from_module);
                    match MergedSelection::from_declarations(
                        session,
                        &targets,
                        from_module,
                        visited,
                    ) {
                        Some(target) => {
                            self.has_unknown |= !target.complete;
                            self.add_values(target.values);
                        }
                        None => self.has_unknown = true,
                    }
                }
                _ => {}
            }
        }
    }

    fn add(&mut self, declared: &DeclaredSelection) {
        match declared {
            DeclaredSelection::Known(values) => self.add_values(values.iter().cloned()),
            DeclaredSelection::Unknown => self.has_unknown = true,
        }
    }

    /// Replaces the labels of known keys.
    fn add_values(&mut self, values: impl IntoIterator<Item = SelectionValue>) {
        for SelectionValue(key, label) in values {
            match self
                .values
                .iter_mut()
                .find(|SelectionValue(known, _)| *known == key)
            {
                Some(SelectionValue(_, known_label)) => *known_label = label,
                None => self.values.push(SelectionValue(key, label)),
            }
        }
    }

    /// Returns `None` if no declaration is a Selection field.
    fn finish(self) -> Option<MergedSelection> {
        self.is_selection.then(|| MergedSelection {
            values: self.values,
            complete: self.has_base && !self.has_unknown,
        })
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

/// Returns the contexts (arguments) of the `fields.Selection(...)` calls `variable` evaluates to.
fn selection_field_contexts(session: &mut SessionInfo, variable: VariableKey) -> Vec<Context> {
    let mut contexts = vec![];
    for evaluation in session.st()[variable].evaluations.clone() {
        let symbol = evaluation
            .symbol
            .get_symbol(session, None, &mut vec![], None);
        for field in SymbolTable::follow_ref(&symbol, session, None, true, false, None, None) {
            if let Some(field_key) = field.upgrade_weak(session.st())
                && SymbolTable::is_specific_field_class(session, field_key, &["Selection"])
            {
                contexts.push(field.get_weak().context.clone());
            }
        }
    }
    contexts
}
