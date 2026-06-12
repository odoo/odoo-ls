use crate::{
    core::symbols::{
        SymbolTable, VariableSymbol,
        storage::xml::xml_field_symbol::XmlFieldName,
        symbol_keys::{ModuleKey, SymbolKey},
    },
    threads::SessionInfo,
};

/// Helper struct to walk through relational fields of a model in order to find the symbol of a nested field
/// It follows the chain of relational fields using field string names
/// It is used by alternating the call between get_model_symbol and get_model_fields,
/// first to find the model symbol of the next field, then to find the field symbols of the next field.
/// Gives the possibility to process the fields or the intermediate models at each step.
/// last_field_is_relational can be used after get_model_fields to test whether the fetched
/// fields are relational without resolving their comodel.
pub struct DeepFieldEvalWalker {
    base_object: SymbolKey,
    from_module: Option<ModuleKey>,
    prev_field_symbols: Option<Vec<SymbolKey>>, // field_symbols from the previous iteration, used to find the next base object
}

impl DeepFieldEvalWalker {
    pub fn new(base_object: SymbolKey, from_module: Option<ModuleKey>) -> Self {
        Self {
            base_object,
            from_module,
            prev_field_symbols: None,
        }
    }

    /// Find the next model symbol by looking for relational fields in the current field symbols
    /// Initial step of the walker
    /// Returns None if the previous step yielded a non relational field
    pub fn get_model_symbol(&mut self, session: &mut SessionInfo) -> Option<SymbolKey> {
        let field_symbols = match &self.prev_field_symbols {
            Some(field_symbols) => field_symbols,
            None => return Some(self.base_object), // First iteration, use the initial base object
        };
        for field_symbol_key in field_symbols.iter() {
            let model_symbols = match field_symbol_key {
                SymbolKey::XmlRecord(key) => {
                    let Some(ttype) =
                        session.st()[*key].get_field_text(XmlFieldName::Type, session.st())
                    else {
                        continue;
                    };
                    if !["many2one", "many2many", "one2many"].contains(&ttype.as_str()) {
                        continue;
                    }
                    let Some(relation) =
                        session.st()[*key].get_field_text(XmlFieldName::Relation, session.st())
                    else {
                        continue;
                    };
                    let Some(related_models) = session.sync_odoo.models.get(relation.as_str())
                    else {
                        continue;
                    };
                    related_models
                        .borrow()
                        .get_main_symbols(session, self.from_module)
                        .collect::<Vec<_>>()
                }
                SymbolKey::Variable(v) => {
                    if SymbolTable::is_specific_field(
                        session,
                        *field_symbol_key,
                        &["Many2one", "One2many", "Many2many"],
                    ) {
                        VariableSymbol::get_relational_model(*v, session, self.from_module)
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            };
            //only handle it if there is only one main symbol for this model
            if model_symbols.len() == 1 {
                return Some(model_symbols[0].into()); // TODO; handle multuple values
            }
        }
        None
    }

    /// Returns true if any field symbol from the previous get_model_fields call is relational.
    /// Cheaper than get_model_symbol: it does not resolve the comodel, and it is also correct
    /// for the multi-comodel case where get_model_symbol would return None.
    pub fn last_field_is_relational(&self, session: &mut SessionInfo) -> bool {
        let Some(field_symbols) = &self.prev_field_symbols else {
            return false;
        };
        field_symbols.iter().any(|field_symbol_key| match field_symbol_key {
            SymbolKey::XmlRecord(key) => session.st()[*key]
                .get_field_text(XmlFieldName::Type, session.st())
                .is_some_and(|ttype| ["many2one", "many2many", "one2many"].contains(&ttype.as_str())),
            SymbolKey::Variable(_) => SymbolTable::is_specific_field(
                session,
                *field_symbol_key,
                &["Many2one", "One2many", "Many2many"],
            ),
            _ => false,
        })
    }

    /// Get model fields with the given name
    /// Returns an empty vec if the base object is not a model or if there are no fields with the given name
    pub fn get_model_fields(
        &mut self,
        session: &mut SessionInfo,
        base_object: SymbolKey,
        name: &str,
    ) -> Vec<SymbolKey> {
        let symbols = match base_object {
            SymbolKey::XmlRecord(xml_record_key) => {
                if let Some(model) = SymbolTable::get_xml_defined_model(session, xml_record_key) {
                    model
                        .borrow()
                        .get_xml_model_field_symbols(session.st(), self.from_module)
                        .filter_map(|f_key| {
                            let field_name = session.st()[f_key]
                                .get_field_text(XmlFieldName::Name, session.st())?;
                            if field_name == name {
                                Some(SymbolKey::from(f_key))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            }
            SymbolKey::Class(_) => {
                let (symbols, _diagnostics) = SymbolTable::get_member_symbol(
                    session,
                    base_object,
                    name,
                    self.from_module,
                    false,
                    true,
                    false,
                    true,
                    false,
                );
                symbols
            }
            _ => Vec::new(),
        };
        self.prev_field_symbols = Some(symbols.clone());
        symbols
    }
}
