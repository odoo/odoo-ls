

#[derive(Debug)]
pub struct ModuleSymbol {
    root_path: String,
    loaded: bool,
    module_name: String,
    dir_name: String,
    depends: Vec<String>,
    data: Vec<String>, // TODO
}

impl ModuleSymbol {

    pub fn new() -> Self {
        ModuleSymbol {
            root_path: String::new(),
            loaded: false,
            module_name: String::new(),
            dir_name: String::new(),
            depends: vec!("base".to_string()),
            data: Vec::new(),
        }
    }

}