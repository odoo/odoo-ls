use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use lsp_types::Diagnostic;
use ruff_text_size::{TextRange, TextSize};

use crate::{constants::{BuildStatus, BuildSteps, SymType}, core::evaluation::{Context, Evaluation}, threads::SessionInfo};

use super::{symbol::Symbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug, PartialEq)]
pub enum ArgumentType {
    POS_ONLY,
    ARG,
    KWARG,
    VARARG,
    KWORD_ONLY,
}

#[derive(Debug)]
pub struct Argument {
    pub symbol: Weak<RefCell<Symbol>>, //always a weak to a symbol of the function
    //other informations about arg
    pub default_value: Option<Evaluation>,
    pub arg_type: ArgumentType,
}

#[derive(Debug)]
pub struct FunctionSymbol {
    pub name: String,
    pub is_external: bool,
    pub is_static: bool,
    pub is_property: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub diagnostics: HashMap<BuildSteps, Vec<Diagnostic>>, //only temporary used for CLASS and FUNCTION to be collected like others are stored on FileInfo
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub range: TextRange,
    pub body_range: TextRange,
    pub args: Vec<Argument>,
    pub is_overloaded: bool, //used for @overload decorator. Only indicates if the decorator is present. Use is_overloaded() to know if this function is overloaded
    pub is_class_method: bool, //used for @classmethod decorator

    //Trait SymbolMgr
    //--- Body content
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<String, Vec<Rc<RefCell<Symbol>>>>,

}

impl FunctionSymbol {

    pub fn new(name: String, range: TextRange, body_start: TextSize, is_external: bool) -> Self {
        let mut res = Self {
            name,
            is_external,
            weak_self: None,
            parent: None,
            range,
            body_range: TextRange::new(body_start, range.end()),
            is_static: false,
            is_property: false,
            diagnostics: HashMap::new(),
            ast_indexes: vec![],
            doc_string: None,
            evaluations: vec![],
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            args: vec![],
            is_overloaded: false,
            is_class_method: false,
        };
        res._init_symbol_mgr();
        res
    }

    pub fn replace_diagnostics(&mut self, step: BuildSteps, diagnostics: Vec<Diagnostic>) {
        self.diagnostics.insert(step, diagnostics);
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert(HashMap::new());
        let section_vec = sections.entry(section).or_insert(vec![]);
        section_vec.push(content.clone());
    }

    /*
    Add evaluations to possible return type of this function
     */
    pub fn add_return_evaluations(function: Rc<RefCell<Symbol>>, session: &mut SessionInfo, evals: Vec<Evaluation>) {
        for new_eval in evals {
            let out_scope = new_eval.get_eval_out_of_function_scope(session, &function);
            for new_eval in out_scope {
                let mut found = false;
                for old_eval in function.borrow().as_func().evaluations.iter() {
                    if old_eval.eq_type(&new_eval) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    function.borrow_mut().as_func_mut().evaluations.push(new_eval);
                }
            }
        }
    }

    pub fn can_be_in_class(&self) -> bool {
        for arg in self.args.iter() {
            if arg.arg_type != ArgumentType::KWARG && arg.arg_type != ArgumentType::KWORD_ONLY {
                return true;
            }
        }
        false
    }

    /* Return true if a previous implementation has the @overload decorator or has it itself */
    pub fn is_overloaded(&self) -> bool {
        if self.is_overloaded {
            return true;
        }
        if let Some(parent) = &self.parent {
            if let Some(parent) = parent.upgrade() {
                let previous_defs = parent.borrow().get_content_symbol(&self.name, self.range.start().to_u32());
                if previous_defs.len() > 1 && previous_defs.last().unwrap().borrow().typ() == SymType::FUNCTION {
                    return previous_defs.last().unwrap().borrow().as_func().is_overloaded;
                }
            }
        }
        false
    }

    /**
     * Given a specific context (with args, parent), adapt the evaluations of the function to get a more precise answer
     */
    pub fn get_return_type(&self, session: &mut SessionInfo, func_context: Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> Vec<Evaluation> {
        let mut res = vec![];
        /*for eval in self.evaluations.iter() {
            let mut new_eval = eval.clone();
            let symbol = new_eval.symbol.get_symbol(session, func_context.clone(), diagnostics);
            new_eval.symbol.symbol = symbol.0.clone();
            new_eval.symbol.instance = symbol.1;
            new_eval.symbol.get_symbol_hook = None;
            res.push(new_eval);
        }*/
        res
    }
}
