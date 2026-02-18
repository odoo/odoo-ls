use slotmap::{SlotMap, new_key_type};

new_key_type! { pub struct ExampleKey; }

enum Symbol {
    Variable(VariableSymbol),
    Function(FunctionSymbol),
}

struct VariableSymbol {
    pub name: String,
    pub is_parameter: bool,
}

struct FunctionSymbol {
    pub name: String,
    pub args: Vec<Argument>,
}

struct Argument {
    pub name: String,
}

fn main() {
    let mut sm = SlotMap::with_key();
    let k: ExampleKey = sm.insert(Symbol::Variable(VariableSymbol {
        name: "hello".to_string(),
        is_parameter: false,
    }));
    let l: ExampleKey = sm.insert(Symbol::Function(FunctionSymbol {
        name: "world".to_string(),
        args: vec![Argument {
            name: "arg1".to_string(),
        }],
    }));
    // println!("{:?} -> {:?}", k, sm[k]);
    println!("stop");
}
