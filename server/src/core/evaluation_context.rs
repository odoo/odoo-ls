use ruff_python_ast::Arguments;
use ruff_text_size::TextRange;
use thin_vec::ThinVec;

use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey, Wk};


/** A context can contain: (non-exhaustive)
* module: the current module the file belongs to
* parent: in an expression, like self.test, the parent is the base attribute, so 'self' for test
* object: the object the expression is executed on (useful if function is defined in parent object).
*/
#[derive(Debug, Clone, Default)]
pub struct Context {
    // ThinVec is a single pointer (8 B inline, x 24 B for a Vec), with no heap
    // allocation while empty. Most contexts are empty.
    entries: ThinVec<(ContextKey, ContextValue)>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextKey {
    Args,
    BaseAttr,
    BaseAttrInserted,
    BaseCall,
    BaseIsSelf,
    Compute,
    ComputeArgRange,
    ComodelName,
    ComodelNameArgRange,
    ConstructingClass,
    Default,
    Delegate,
    FieldParent,
    IsAttrOfInstance,
    IsInValidation,
    Inverse,
    InverseArgRange,
    InverseName,
    InverseNameArgRange,
    Module,
    Parameters,
    ParentFor,
    ParentInstance,
    Range,
    Related,
    RelatedArgRange,
    Required,
    Search,
    SearchArgRange,
    // placeholder after removal
    EMPTY,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextValue {
    BOOLEAN(bool),
    STRING(String),
    MODULE(Wk<ModuleKey>),
    SYMBOL(Wk<SymbolKey>),
    ARGUMENTS(Arguments),
    RANGE(TextRange),
    // empty value after removal
    EMPTY,
}


impl Context {
    pub fn from_iter(entries: impl IntoIterator<Item = (ContextKey, ContextValue)>) -> Self {
        Context { entries: entries.into_iter().collect() }
    }

    pub fn insert(&mut self, key: ContextKey, value: ContextValue) {
        let mut empty_slot = None;
        // update value if key already exists
        for (i, (k, v)) in self.entries.iter_mut().enumerate() {
            if *k == key {
                *v = value;
                return;
            }
            if empty_slot.is_none() && *k == ContextKey::EMPTY {
                empty_slot = Some(i)
            }
        }
        // otherwise, insert new entry
        // reuse empty slot if any
        if let Some(i) = empty_slot {
            self.entries[i] = (key, value);
        } else {
            self.entries.push((key, value));
        }
    }

    /// Removing an entry that was added last is (usually) more efficient.
    pub fn remove(&mut self, key: ContextKey) -> Option<ContextValue> {
        while let Some((k, _)) = self.entries.last() {
            if *k == key {
                // key matches last entry, simply pop it
                return self.entries.pop().map(|(_, v)| v);
            } else if *k == ContextKey::EMPTY {
                // last entry is empty: clean it up and keep seaching
                self.entries.pop();
            } else {
                break;
            }
        }
        // We keep seaching in reverse order, skip last entry (it was checked above)
        for (k, v) in self.entries.iter_mut().rev().skip(1) {
            if *k == key {
                *k = ContextKey::EMPTY; // mark as empty
                let value = std::mem::replace(v, ContextValue::EMPTY);
                return Some(value);
            }
        }
        None
    }

    pub fn get(&self, key: ContextKey) -> Option<&ContextValue> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &ContextKey) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(ContextKey, ContextValue)> {
        self.entries.iter().filter(|(k, _)| *k != ContextKey::EMPTY)
    }

    /// Merges two Contexts into a new one.
    /// When a key is present in both, value from `b` has precedence.
    pub fn merge(a: &Self, b: &Self) -> Self {
        let mut result = ThinVec::with_capacity(a.entries.len() + b.entries.len());
        result.extend(a.iter().filter(|(key, _)| !b.contains_key(key)).cloned());
        result.extend(b.iter().cloned());
        Self { entries: result }
    }
}

impl ContextValue {
    pub fn as_bool(&self) -> bool {
        match self {
            ContextValue::BOOLEAN(b) => *b,
            _ => panic!("Not a boolean")
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ContextValue::STRING(s) => s,
            _ => panic!("Not a string")
        }
    }

    pub fn as_symbol(&self) -> Wk<SymbolKey> {
        match self {
            ContextValue::SYMBOL(s) => *s,
            _ => panic!("Not a symbol")
        }
    }

    pub fn as_text_range(&self) -> TextRange {
        match self {
            ContextValue::RANGE(r) => r.clone(),
            _ => panic!("Not a TextRange")
        }
    }

    pub fn as_arguments(&self) -> Arguments {
        match self {
            ContextValue::ARGUMENTS(a) => a.clone(),
            _ => panic!("Not an arguments")
        }
    }
}

impl PartialEq for Context {
    fn eq(&self, other: &Self) -> bool {
        self.iter().count() == other.iter().count()
            && self.iter().all(|(k, v)| other.get(*k) == Some(v))
    }
}
