use slotmap::{SlotMap, new_key_type};

new_key_type! { pub struct VariableKey; }
new_key_type! { pub struct FunctionKey; }

#[derive(Clone, Copy)]
enum SymbolKey {
    Variable(VariableKey),
    Function(FunctionKey),
}

struct SymbolTable {
    variables: SlotMap<VariableKey, VariableSymbol>,
    functions: SlotMap<FunctionKey, FunctionSymbol>,
}

struct VariableSymbol {
    pub name: String,
    pub is_parameter: bool,
    pub parent: Option<SymbolKey>,
}

struct FunctionSymbol {
    pub name: String,
    pub args: Vec<Argument>,
    pub parent: Option<SymbolKey>,
}
enum Symbol<'a> {
    Variable(&'a VariableSymbol),
    Function(&'a FunctionSymbol),
}

enum SymbolMut<'a> {
    Variable(&'a mut VariableSymbol),
    Function(&'a mut FunctionSymbol),
}

impl<'a> Symbol<'a> {
    pub fn name(&self) -> &str {
        match self {
            Symbol::Variable(var) => &var.name,
            Symbol::Function(func) => &func.name,
        }
    }
    
    pub fn parent(&self) -> Option<SymbolKey> {
        match self {
            Symbol::Variable(var) => var.parent,
            Symbol::Function(func) => func.parent,
        }
    }

    pub fn parent_symbol<'b>(&self, symbol_table: &'b SymbolTable) -> Option<Symbol<'b>> {
        self.parent().and_then(|key| symbol_table.get_symbol(key))
    }
}

struct Argument {
    pub name: String,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            variables: SlotMap::with_key(),
            functions: SlotMap::with_key(),
        }
    }

    pub fn insert_variable(&mut self, symbol: VariableSymbol) -> SymbolKey {
        let key =self.variables.insert(symbol);
        SymbolKey::Variable(key)
    }

    pub fn insert_function(&mut self, symbol: FunctionSymbol) -> SymbolKey {
        let key = self.functions.insert(symbol);
        SymbolKey::Function(key)
    }

    pub fn get_symbol(&self, key: SymbolKey) -> Option<Symbol<'_>> {
        match key {
            SymbolKey::Variable(k) => self.variables.get(k).map(Symbol::Variable),
            SymbolKey::Function(k) => self.functions.get(k).map(Symbol::Function),
        }
    }
    pub fn get_symbol_mut(&mut self, key: SymbolKey) -> Option<SymbolMut<'_>> {
        match key {
            SymbolKey::Variable(k) => self.variables.get_mut(k).map(SymbolMut::Variable),
            SymbolKey::Function(k) => self.functions.get_mut(k).map(SymbolMut::Function),
        }
    }
}

fn main() {
    let mut symbol_table = SymbolTable::new();
    let func_key = symbol_table.insert_function(FunctionSymbol {
        name: "world".to_string(),
        args: vec![Argument {
            name: "arg1".to_string(),
        }],
        parent: None,
    });
    let var_key = symbol_table.insert_variable(VariableSymbol {
        name: "hello".to_string(),
        is_parameter: false,
        parent: Some(func_key),
    });

    if let Some(var_symbol) = symbol_table.get_symbol(var_key) {
        println!("Variable name: {}", var_symbol.name());
        if let Some(parent_symbol) = var_symbol.parent_symbol(&symbol_table) {
            println!("Parent symbol name: {}", parent_symbol.name());
        }
        let parent_key = var_symbol.parent().unwrap();
        let parent_mut = symbol_table.get_symbol_mut(parent_key).unwrap();
        match parent_mut {
            SymbolMut::Variable(var) => var.name = "changed".to_string(),
            SymbolMut::Function(func) => func.name = "changed".to_string(),
        }
    }
    println!("After mutation: {}", symbol_table.get_symbol(func_key).unwrap().name());
    // println!("{:?} -> {:?}", k, sm[k]);
    // println!("key is copiable: {:?}", k);
}
