use ruff_text_size::{TextSize, TextRange};
use serde_json::{Value, json};
use tracing::{info, trace};

use crate::constants::*;
use crate::core::evaluation::{Context, Evaluation};
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval::PythonArchEval;
use crate::threads::SessionInfo;
use crate::utils::{MaxTextSize, PathSanitizer as _};
use crate::S;
use core::panic;
use std::collections::{HashMap, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::{u32, vec};
use lsp_types::Diagnostic;

use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::root_symbol::RootSymbol;

use super::class_symbol::ClassSymbol;
use super::compiled_symbol::CompiledSymbol;
use super::file_symbol::FileSymbol;
use super::namespace_symbol::{NamespaceDirectory, NamespaceSymbol};
use super::package_symbol::PackageSymbol;
use super::symbol_mgr::SymbolMgr;
use super::variable_symbol::VariableSymbol;

#[derive(Debug)]
pub enum MainSymbol {
    Root(RootSymbol),
    Namespace(NamespaceSymbol),
    Package(PackageSymbol),
    File(FileSymbol),
    Compiled(CompiledSymbol),
    Class(ClassSymbol),
    Function(FunctionSymbol),
    Variable(VariableSymbol),
}

impl MainSymbol {
    pub fn new_root() -> Rc<RefCell<Self>> {
        let root = Rc::new(RefCell::new(MainSymbol::Root(RootSymbol::new())));
        root.borrow_mut().set_weak_self(Rc::downgrade(&root));
        root
    }

    //Create a sub-symbol that is representing a file
    pub fn add_new_file(&self, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let file = Rc::new(RefCell::new(MainSymbol::File(FileSymbol::new(name.clone(), path.clone(), self.is_external()))));
        file.borrow_mut().set_weak_self(Rc::downgrade(&file));
        file.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::Namespace(n) => {
                n.add_file(file);
            },
            MainSymbol::Package(p) => {
                p.add_file(file);
            },
            _ => { panic!("Impossible to add a file to a {}", self.typ()); }
        }
        file
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let package = Rc::new(
            RefCell::new(
                MainSymbol::Package(
                    PackageSymbol::new_python_package(name.clone(), path.clone(), self.is_external())
                )
            )
        );
        package.borrow_mut().set_weak_self(Rc::downgrade(&package));
        package.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::Namespace(n) => {
                n.add_file(package);
            },
            MainSymbol::Package(p) => {
                p.add_file(package);
            },
            MainSymbol::Root(r) => {
                r.add_file(session, package)
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        package
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_module_package(&self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let package = Rc::new(
            RefCell::new(
                MainSymbol::Package(
                    PackageSymbol::new_module_package(name.clone(), path.clone(), self.is_external())
                )
            )
        );
        package.borrow_mut().set_weak_self(Rc::downgrade(&package));
        package.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::Namespace(n) => {
                n.add_file(package);
            },
            MainSymbol::Package(p) => {
                p.add_file(package);
            },
            MainSymbol::Root(r) => {
                r.add_file(session, package)
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        package
    }

    pub fn add_new_namespace(&self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let namespace = Rc::new(RefCell::new(MainSymbol::Namespace(NamespaceSymbol::new(name.clone(), vec![path.clone()], self.is_external()))));
        namespace.borrow_mut().set_weak_self(Rc::downgrade(&namespace));
        namespace.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::Namespace(n) => {
                n.add_file(namespace);
            },
            MainSymbol::Package(p) => {
                p.add_file(namespace);
            },
            MainSymbol::Root(r) => {
                r.add_file(session, namespace);
            }
            _ => { panic!("Impossible to add a namespace to a {}", self.typ()); }
        }
        namespace
    }

    pub fn add_new_compiled(&self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let compiled = Rc::new(RefCell::new(MainSymbol::Compiled(CompiledSymbol::new(name.clone(), path.clone(), self.is_external()))));
        compiled.borrow_mut().set_weak_self(Rc::downgrade(&compiled));
        compiled.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::Namespace(n) => {
                n.add_file(compiled);
            },
            MainSymbol::Package(p) => {
                p.add_file(compiled);
            },
            MainSymbol::Root(r) => {
                r.add_file(session, compiled);
            }
            _ => { panic!("Impossible to add a compiled to a {}", self.typ()); }
        }
        compiled
    }

    pub fn add_new_variable(&self, session: &mut SessionInfo, name: &String, range: &TextRange) -> Rc<RefCell<Self>> {
        let variable = Rc::new(RefCell::new(MainSymbol::Variable(VariableSymbol::new(name.clone(), range.clone(), self.is_external()))));
        variable.borrow_mut().set_weak_self(Rc::downgrade(&variable));
        variable.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(variable, section);
            },
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(variable, section);
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(variable, section);
            },
            _ => { panic!("Impossible to add a variable to a {}", self.typ()); }
        }
        variable
    }

    pub fn add_new_function(&self, session: &mut SessionInfo, name: &String, range: &TextRange) -> Rc<RefCell<Self>> {
        let function = Rc::new(RefCell::new(MainSymbol::Function(FunctionSymbol::new(name.clone(), range.clone(), self.is_external()))));
        function.borrow_mut().set_weak_self(Rc::downgrade(&function));
        function.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            MainSymbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(function, section);
            },
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(function, section);
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(function, section);
            },
            _ => { panic!("Impossible to add a function to a {}", self.typ()); }
        }
        function
    }

    pub fn as_root(&self) -> &RootSymbol {
        match self {
            MainSymbol::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }
    pub fn as_root_mut(&mut self) -> &mut RootSymbol {
        match self {
            MainSymbol::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }
    pub fn as_file(&self) -> &FileSymbol {
        match self {
            MainSymbol::File(f) => f,
            _ => {panic!("Not a File")}
        }
    }
    pub fn as_file_mut(&mut self) -> &mut FileSymbol {
        match self {
            MainSymbol::File(f) => f,
            _ => {panic!("Not a File")}
        }
    }
    pub fn as_package(&self) -> &PackageSymbol {
        match self {
            MainSymbol::Package(p) => p,
            _ => {panic!("Not a package")}
        }
    }
    pub fn as_package_mut(&mut self) -> &mut PackageSymbol {
        match self {
            MainSymbol::Package(p) => p,
            _ => {panic!("Not a package")}
        }
    }
    pub fn as_module_package(&self) -> &ModuleSymbol {
        match self {
            MainSymbol::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }
    pub fn as_module_package_mut(&mut self) -> &mut ModuleSymbol {
        match self {
            MainSymbol::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }

    pub fn as_variable(&self) -> &VariableSymbol {
        match self {
            MainSymbol::Variable(v) => v,
            _ => {panic!("Not a variable")}
        }
    }

    pub fn as_variable_mut(&mut self) -> &mut VariableSymbol {
        match self {
            MainSymbol::Variable(v) => v,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_func(&self) -> &FunctionSymbol {
        match self {
            MainSymbol::Function(f) => f,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_func_mut(&mut self) -> &mut FunctionSymbol {
        match self {
            MainSymbol::Function(f) => f,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_class_sym(&self) -> &ClassSymbol {
        match self {
            MainSymbol::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }

    pub fn as_class_sym_mut(&mut self) -> &mut ClassSymbol {
        match self {
            MainSymbol::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }

    pub fn typ(&self) -> SymType {
        match self {
            MainSymbol::Root(_) => SymType::ROOT,
            MainSymbol::Namespace(_) => SymType::NAMESPACE,
            MainSymbol::Package(_) => SymType::PACKAGE,
            MainSymbol::File(_) => SymType::FILE,
            MainSymbol::Compiled(_) => SymType::COMPILED,
            MainSymbol::Class(_) => SymType::CLASS,
            MainSymbol::Function(_) => SymType::FUNCTION,
            MainSymbol::Variable(_) => SymType::VARIABLE,
        }
    }

    pub fn name(&self) -> &String {
        match self {
            MainSymbol::Root(r) => &S!("root"),
            MainSymbol::Namespace(n) => &n.name,
            MainSymbol::Package(p) => &p.name(),
            MainSymbol::File(f) => &f.name,
            MainSymbol::Compiled(c) => &c.name,
            MainSymbol::Class(c) => &c.name,
            MainSymbol::Function(f) => &f.name,
            MainSymbol::Variable(v) => &v.name,
        }
    }

    pub fn doc_string(&self) -> &Option<String> {
        match self {
            MainSymbol::Root(r) => &None,
            MainSymbol::Namespace(n) => &None,
            MainSymbol::Package(p) => &None,
            MainSymbol::File(f) => &None,
            MainSymbol::Compiled(c) => &None,
            MainSymbol::Class(c) => &c.doc_string,
            MainSymbol::Function(f) => &f.doc_string,
            MainSymbol::Variable(v) => &v.doc_string,
        }
    }

    pub fn set_doc_string(&mut self, doc_string: Option<String>) {
        match self {
            MainSymbol::Root(r) => panic!(),
            MainSymbol::Namespace(n) => panic!(),
            MainSymbol::Package(p) => panic!(),
            MainSymbol::File(f) => panic!(),
            MainSymbol::Compiled(c) => panic!(),
            MainSymbol::Class(c) => c.doc_string = doc_string,
            MainSymbol::Function(f) => f.doc_string = doc_string,
            MainSymbol::Variable(v) => v.doc_string = doc_string,
        }
    }

    pub fn is_external(&self) -> bool {
        match self {
            MainSymbol::Root(r) => false,
            MainSymbol::Namespace(n) => n.is_external,
            MainSymbol::Package(p) => p.is_external(),
            MainSymbol::File(f) => f.is_external,
            MainSymbol::Compiled(c) => c.is_external,
            MainSymbol::Class(c) => c.is_external,
            MainSymbol::Function(f) => f.is_external,
            MainSymbol::Variable(v) => v.is_external,
        }
    }
    pub fn set_is_external(&self, external: bool) {
        match self {
            MainSymbol::Root(r) => {},
            MainSymbol::Namespace(n) => n.is_external = external,
            MainSymbol::Package(PackageSymbol::Module(m)) => m.is_external = external,
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => p.is_external = external,
            MainSymbol::File(f) => f.is_external = external,
            MainSymbol::Compiled(c) => c.is_external = external,
            MainSymbol::Class(c) => c.is_external = external,
            MainSymbol::Function(f) => f.is_external = external,
            MainSymbol::Variable(v) => v.is_external = external,
        }
    }

    pub fn range(&self) -> &TextRange {
        match self {
            MainSymbol::Root(r) => panic!(),
            MainSymbol::Namespace(n) => panic!(),
            MainSymbol::Package(p) => panic!(),
            MainSymbol::File(f) => panic!(),
            MainSymbol::Compiled(c) => panic!(),
            MainSymbol::Class(c) => &c.range,
            MainSymbol::Function(f) => &f.range,
            MainSymbol::Variable(v) => &v.range,
        }
    }

    pub fn has_ast_indexes(&self) -> bool {
        match self {
            MainSymbol::Variable(v) => true,
            MainSymbol::Class(c) => true,
            MainSymbol::Function(f) => true,
            MainSymbol::File(f) => false,
            MainSymbol::Compiled(c) => false,
            MainSymbol::Namespace(n) => false,
            MainSymbol::Package(p) => false,
            MainSymbol::Root(r) => false,
        }
    }

    pub fn ast_indexes(&self) -> &Vec<u16> {
        match self {
            MainSymbol::Variable(v) => &v.ast_indexes,
            MainSymbol::Class(c) => &c.ast_indexes,
            MainSymbol::Function(f) => &f.ast_indexes,
            MainSymbol::File(f) => &vec![],
            MainSymbol::Compiled(c) => &vec![],
            MainSymbol::Namespace(n) => &vec![],
            MainSymbol::Package(p) => &vec![],
            MainSymbol::Root(r) => &vec![],
        }
    }

    pub fn ast_indexes_mut(&mut self) -> &mut Vec<u16> {
        match self {
            MainSymbol::Variable(v) => &mut v.ast_indexes,
            MainSymbol::Class(c) => &mut c.ast_indexes,
            MainSymbol::Function(f) => &mut f.ast_indexes,
            MainSymbol::File(f) => panic!(),
            MainSymbol::Compiled(c) => panic!(),
            MainSymbol::Namespace(n) => panic!(),
            MainSymbol::Package(p) => panic!(),
            MainSymbol::Root(r) => panic!(),
        }
    }

    fn weak_self(&mut self) -> Option<Weak<RefCell<MainSymbol>>> {
        match self {
            MainSymbol::Root(r) => r.weak_self,
            MainSymbol::Namespace(n) => n.weak_self,
            MainSymbol::Package(PackageSymbol::Module(m)) => m.weak_self,
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self,
            MainSymbol::File(f) => f.weak_self,
            MainSymbol::Compiled(c) => c.weak_self,
            MainSymbol::Class(c) => c.weak_self,
            MainSymbol::Function(f) => f.weak_self,
            MainSymbol::Variable(v) => v.weak_self,
        }
    }

    pub fn parent(&mut self) -> Option<Weak<RefCell<MainSymbol>>> {
        match self {
            MainSymbol::Root(r) => r.parent,
            MainSymbol::Namespace(n) => n.parent,
            MainSymbol::Package(p) => p.parent(),
            MainSymbol::File(f) => f.parent,
            MainSymbol::Compiled(c) => c.parent,
            MainSymbol::Class(c) => c.parent,
            MainSymbol::Function(f) => f.parent,
            MainSymbol::Variable(v) => v.parent,
        }
    }

    fn set_parent(&mut self, parent: Option<Weak<RefCell<MainSymbol>>>) {
        match self {
            MainSymbol::Root(r) => panic!(),
            MainSymbol::Namespace(n) => n.parent = parent,
            MainSymbol::Package(p) => p.set_parent(parent),
            MainSymbol::File(f) => f.parent = parent,
            MainSymbol::Compiled(c) => c.parent = parent,
            MainSymbol::Class(c) => c.parent = parent,
            MainSymbol::Function(f) => f.parent = parent,
            MainSymbol::Variable(v) => v.parent = parent,
        }
    }
    
    pub fn paths(&self) -> Vec<String> {
        match self {
            MainSymbol::Root(r) => r.paths,
            MainSymbol::Namespace(n) => n.paths(),
            MainSymbol::Package(p) => p.paths(),
            MainSymbol::File(f) => vec![f.path],
            MainSymbol::Compiled(c) => vec![c.path],
            MainSymbol::Class(c) => vec![],
            MainSymbol::Function(f) => vec![],
            MainSymbol::Variable(v) => vec![],
        }
    }
    pub fn add_path(&mut self, path: String) {
        match self {
            MainSymbol::Root(r) => r.paths.push(path),
            MainSymbol::Namespace(n) => {
                n.directories.push(NamespaceDirectory { path: path, module_symbols: HashMap::new() });
            },
            MainSymbol::Package(p) => {},
            MainSymbol::File(f) => {},
            MainSymbol::Compiled(c) => {},
            MainSymbol::Class(c) => {},
            MainSymbol::Function(f) => {},
            MainSymbol::Variable(v) => {},
        }
    }

    pub fn dependencies_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 4] {
        match self {
            MainSymbol::Root(r) => panic!("No dependencies on Root"),
            MainSymbol::Namespace(n) => &mut n.dependencies,
            MainSymbol::Package(p) => p.dependencies_as_mut(),
            MainSymbol::File(f) => &mut f.dependencies,
            MainSymbol::Compiled(c) => panic!("No dependencies on Compiled"),
            MainSymbol::Class(c) => panic!("No dependencies on Class"),
            MainSymbol::Function(f) => panic!("No dependencies on Function"),
            MainSymbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependents(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 3] {
        match self {
            MainSymbol::Root(r) => panic!("No dependencies on Root"),
            MainSymbol::Namespace(n) => &n.dependents,
            MainSymbol::Package(p) => p.dependents(),
            MainSymbol::File(f) => &f.dependents,
            MainSymbol::Compiled(c) => panic!("No dependencies on Compiled"),
            MainSymbol::Class(c) => panic!("No dependencies on Class"),
            MainSymbol::Function(f) => panic!("No dependencies on Function"),
            MainSymbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>>; 3] {
        match self {
            MainSymbol::Root(r) => panic!("No dependencies on Root"),
            MainSymbol::Namespace(n) => &mut n.dependents,
            MainSymbol::Package(p) => p.dependents_as_mut(),
            MainSymbol::File(f) => &mut f.dependents,
            MainSymbol::Compiled(c) => panic!("No dependencies on Compiled"),
            MainSymbol::Class(c) => panic!("No dependencies on Class"),
            MainSymbol::Function(f) => panic!("No dependencies on Function"),
            MainSymbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn all_module_symbol(&self) -> impl Iterator<Item = &Rc<RefCell<MainSymbol>>> {
        match self {
            MainSymbol::Root(r) => r.module_symbols.values(),
            MainSymbol::Namespace(n) => n.module_symbols.values(),
            MainSymbol::Package(PackageSymbol::Module(m)) => m.module_symbols.values(),
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => p.module_symbols.values(),
            MainSymbol::File(f) => panic!("No module symbol on File"),
            MainSymbol::Compiled(c) => panic!("No module symbol on Compiled"),
            MainSymbol::Class(c) => panic!("No module symbol on Class"),
            MainSymbol::Function(f) => panic!("No module symbol on Function"),
            MainSymbol::Variable(v) => panic!("No module symbol on Variable"),
        }
    }
    pub fn in_workspace(&self) -> bool {
        match self {
            MainSymbol::Root(r) => false,
            MainSymbol::Namespace(n) => n.in_workspace,
            MainSymbol::Package(PackageSymbol::Module(m)) => m.in_workspace,
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace,
            MainSymbol::File(f) => f.in_workspace,
            MainSymbol::Compiled(c) => panic!(),
            MainSymbol::Class(c) => panic!(),
            MainSymbol::Function(f) => panic!(),
            MainSymbol::Variable(v) => panic!(),
        }
    }
    pub fn build_status(&self, step:BuildSteps) -> BuildStatus {
        match self {
            MainSymbol::Root(r) => {panic!()},
            MainSymbol::Namespace(n) => {panic!()},
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => m.arch_status,
                    BuildSteps::ARCH_EVAL => m.arch_eval_status,
                    BuildSteps::ODOO => m.odoo_status,
                    BuildSteps::VALIDATION => m.validation_status,
                }
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status,
                    BuildSteps::ODOO => p.odoo_status,
                    BuildSteps::VALIDATION => p.validation_status,
                }
            }
            MainSymbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
                    BuildSteps::ODOO => f.odoo_status,
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            MainSymbol::Compiled(_) => todo!(),
            MainSymbol::Class(c) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => c.arch_status,
                    BuildSteps::ARCH_EVAL => c.arch_eval_status,
                    BuildSteps::ODOO => c.odoo_status,
                    BuildSteps::VALIDATION => c.validation_status,
                }
            },
            MainSymbol::Function(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
                    BuildSteps::ODOO => f.odoo_status,
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            MainSymbol::Variable(_) => todo!(),
        }
    }
    pub fn set_build_status(&self, step:BuildSteps, status: BuildStatus) {
        match self {
            MainSymbol::Root(r) => {panic!()},
            MainSymbol::Namespace(n) => {panic!()},
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => m.arch_status = status,
                    BuildSteps::ARCH_EVAL => m.arch_eval_status = status,
                    BuildSteps::ODOO => m.odoo_status = status,
                    BuildSteps::VALIDATION => m.validation_status = status,
                }
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status = status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status = status,
                    BuildSteps::ODOO => p.odoo_status = status,
                    BuildSteps::VALIDATION => p.validation_status = status,
                }
            }
            MainSymbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
                    BuildSteps::ODOO => f.odoo_status = status,
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            MainSymbol::Compiled(_) => panic!(),
            MainSymbol::Class(c) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => c.arch_status = status,
                    BuildSteps::ARCH_EVAL => c.arch_eval_status = status,
                    BuildSteps::ODOO => c.odoo_status = status,
                    BuildSteps::VALIDATION => c.validation_status = status,
                }
            },
            MainSymbol::Function(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
                    BuildSteps::ODOO => f.odoo_status = status,
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            MainSymbol::Variable(_) => todo!(),
        }
    }

    pub fn iter_symbols(&self) -> std::collections::hash_map::IntoIter<String, HashMap<u32, Vec<Rc<RefCell<MainSymbol>>>>> {
        match self {
            MainSymbol::File(f) => {
                f.symbols.into_iter()
            }
            MainSymbol::Root(r) => todo!(),
            MainSymbol::Namespace(n) => todo!(),
            MainSymbol::Package(p) => todo!(),
            MainSymbol::Compiled(c) => todo!(),
            MainSymbol::Class(c) => todo!(),
            MainSymbol::Function(f) => todo!(),
            MainSymbol::Variable(v) => todo!(),
        }
    }
    pub fn evaluations(&self) -> &Vec<Evaluation>{
        match self {
            MainSymbol::File(f) => { &vec![] },
            MainSymbol::Root(r) => { &vec![] },
            MainSymbol::Namespace(n) => { &vec![] },
            MainSymbol::Package(p) => { &vec![] },
            MainSymbol::Compiled(c) => { &vec![] },
            MainSymbol::Class(c) => { &vec![] },
            MainSymbol::Function(f) => { &vec![] },
            MainSymbol::Variable(v) => &v.evaluations,
        }
    }
    pub fn set_evaluations(&mut self, data: Vec<Evaluation>) {
        match self {
            MainSymbol::File(f) => { panic!() },
            MainSymbol::Root(r) => { panic!() },
            MainSymbol::Namespace(n) => { panic!() },
            MainSymbol::Package(p) => { panic!() },
            MainSymbol::Compiled(c) => { panic!() },
            MainSymbol::Class(c) => { panic!() },
            MainSymbol::Function(f) => { panic!() },
            MainSymbol::Variable(v) => v.evaluations = data,
        }
    }

    pub fn not_found_paths(&self) -> &Vec<(BuildSteps, Vec<String>)> {
        match self {
            MainSymbol::File(f) => { &f.not_found_paths },
            MainSymbol::Root(r) => { &vec![] },
            MainSymbol::Namespace(n) => { &vec![] },
            MainSymbol::Package(PackageSymbol::Module(m)) => { &m.not_found_paths },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => { &p.not_found_paths },
            MainSymbol::Compiled(c) => { &vec![] },
            MainSymbol::Class(c) => { &vec![] },
            MainSymbol::Function(f) => { &vec![] },
            MainSymbol::Variable(v) => &vec![],
        }
    }

    pub fn not_found_paths_mut(&self) -> &mut Vec<(BuildSteps, Vec<String>)> {
        match self {
            MainSymbol::File(f) => { &mut f.not_found_paths },
            MainSymbol::Root(r) => { &mut vec![] },
            MainSymbol::Namespace(n) => { &mut vec![] },
            MainSymbol::Package(PackageSymbol::Module(m)) => { &mut m.not_found_paths },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => { &mut p.not_found_paths },
            MainSymbol::Compiled(c) => { &mut vec![] },
            MainSymbol::Class(c) => { &mut vec![] },
            MainSymbol::Function(f) => { &mut vec![] },
            MainSymbol::Variable(v) => &mut vec![],
        }
    }

    ///Given a path, create the appropriated symbol and attach it to the given parent
    pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: Rc<RefCell<MainSymbol>>, require_module: bool) -> Option<Rc<RefCell<MainSymbol>>> {
        let name: String = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.sanitize();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            let ref_sym = (*parent).borrow_mut().add_new_file(&name, &path_str);
            return Some(ref_sym);
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                if (*parent).borrow().get_tree().clone() == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
                    let module = (*parent).borrow_mut().add_new_module_package(session, &name, &path_str);
                    ModuleSymbol::load_module_info(module, session, parent);
                    //as the symbol has been added to parent before module creation, it has not been added to modules
                    session.sync_odoo.modules.insert(module.borrow().as_module_package().dir_name.clone(), Rc::downgrade(&module));
                    return Some(module);
                } else if require_module {
                    return None;
                } else {
                    let ref_sym = (*parent).borrow_mut().add_new_python_package(session, &name, &path_str);
                    if !path.join("__init__.py").exists() {
                        (*ref_sym).borrow_mut().as_package_mut().set_i_ext("i".to_string());
                    }
                    return Some(ref_sym);
                }
            } else if !require_module{ //TODO should handle module with only __manifest__.py (see odoo/addons/test_data-module)
                let ref_sym = (*parent).borrow_mut().add_new_namespace(session, &name, &path_str);
                return Some(ref_sym);
            } else {
                return None
            }
        }
    }

    pub fn get_tree(&self) -> Tree {
        let mut res = (vec![], vec![]);
        if self.is_file_content() {
            res.1.insert(0, self.name().clone());
        } else {
            res.0.insert(0, self.name().clone());
        }
        if self.typ() == SymType::ROOT || self.parent().is_none() {
            return res
        }
        let parent = self.parent().clone();
        let mut current_arc = parent.as_ref().unwrap().upgrade().unwrap();
        let mut current = current_arc.borrow_mut();
        while current.typ() != SymType::ROOT && current.parent().is_some() {
            if current.is_file_content() {
                res.1.insert(0, current.name().clone());
            } else {
                res.0.insert(0, current.name().clone());
            }
            let parent = current.parent().clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.borrow_mut();
        }
        res
    }

    pub fn get_symbol(&self, tree: &Tree, position: u32) -> Vec<Rc<RefCell<MainSymbol>>> {
        let symbol_tree_files: &Vec<String> = &tree.0;
        let symbol_tree_content: &Vec<String> = &tree.1;
        let mut iter_sym: Vec<Rc<RefCell<MainSymbol>>> = vec![];
        if symbol_tree_files.len() != 0 {
            let _mod_iter_sym = self.get_module_symbol(&symbol_tree_files[0]);
            if _mod_iter_sym.is_none() {
                return vec![];
            }
            if symbol_tree_files.len() > 1 {
                for fk in symbol_tree_files[1..symbol_tree_files.len()].iter() {
                    if let Some(s) = _mod_iter_sym.unwrap().borrow_mut().get_module_symbol(fk) {
                        iter_sym = vec![s.clone()];
                    } else {
                        return vec![];
                    }
                }
            }
            if symbol_tree_content.len() != 0 {
                for fk in symbol_tree_content.iter() {
                    if iter_sym.len() > 1 {
                        trace!("TODO: explore all implementation possibilities");
                    }
                    iter_sym = iter_sym[0].borrow_mut().get_content_symbol(fk, u32::MAX);
                    if iter_sym.is_empty() {
                        return vec![];
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 {
                return vec![];
            }
            iter_sym = self.get_content_symbol(&symbol_tree_content[0], u32::MAX);
            if iter_sym.is_empty() {
                return vec![];
            }
            if symbol_tree_content.len() > 1 {
                if iter_sym.len() > 1 {
                    trace!("TODO: explore all implementation possibilities");
                }
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    iter_sym = iter_sym[0].borrow_mut().get_content_symbol(fk, u32::MAX);
                    return iter_sym.clone();
                }
            }
        }
        iter_sym
    }

    pub fn get_module_symbol(&self, name: &str) -> Option<Rc<RefCell<MainSymbol>>> {
        match self {
            MainSymbol::Namespace(n) => {
                for dir in n.directories.iter() {
                    let result = dir.module_symbols.get(name);
                    if let Some(result) = result {
                        if !result.is_empty() {
                            return Some(result.last().unwrap().clone());
                        }
                    }
                }
                None
            },
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                m.module_symbols.get(name).cloned()
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.module_symbols.get(name).cloned()
            }
            _ => {None}
        }
    }

    pub fn get_content_symbol(&self, name: &str, position: u32) -> Vec<Rc<RefCell<MainSymbol>>> {
        match self {
            MainSymbol::Class(c) => {
                c.get_symbol(name, position)
            },
            MainSymbol::File(f) => {
                f.get_symbol(name, position)
            },
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                m.get_symbol(name, position)
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_symbol(name, position)
            },
            _ => {vec![]}
        }
    }

    //Return a HashSet of all symbols (constructed until 'level') that are dependencies for the 'step' of this symbol
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<MainSymbol>>> {
        if step == BuildSteps::SYNTAX || level == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        if level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
            }
            if level == BuildSteps::VALIDATION {
                panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
            }
        }
        match self {
            MainSymbol::Root(r) => panic!("There is no dependencies on Root Symbol"),
            MainSymbol::Namespace(n) => &n.dependencies[step as usize][level as usize],
            MainSymbol::Package(p) => &p.dependencies()[step as usize][level as usize],
            MainSymbol::File(f) => &f.dependencies[step as usize][level as usize],
            MainSymbol::Compiled(c) => panic!("There is no dependencies on Compiled Symbol"),
            MainSymbol::Class(c) => panic!("There is no dependencies on Class Symbol"),
            MainSymbol::Function(f) => panic!("There is no dependencies on Function Symbol"),
            MainSymbol::Variable(v) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    pub fn get_all_dependencies(&self, step: BuildSteps) -> &Vec<PtrWeakHashSet<Weak<RefCell<MainSymbol>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        match self {
            MainSymbol::Root(r) => panic!("There is no dependencies on Root Symbol"),
            MainSymbol::Namespace(n) => &n.dependencies[step as usize],
            MainSymbol::Package(p) => &p.dependencies()[step as usize],
            MainSymbol::File(f) => &f.dependencies[step as usize],
            MainSymbol::Compiled(c) => panic!("There is no dependencies on Compiled Symbol"),
            MainSymbol::Class(c) => panic!("There is no dependencies on Class Symbol"),
            MainSymbol::Function(f) => panic!("There is no dependencies on Function Symbol"),
            MainSymbol::Variable(v) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<MainSymbol>>> {
        if level == BuildSteps::SYNTAX || step == BuildSteps::SYNTAX {
            panic!("Can't get dependents for syntax step")
        }
        if level == BuildSteps::VALIDATION {
            panic!("Can't get dependents for level {:?}", level)
        }
        if level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't get dependents for step {:?} and level {:?}", step, level)
            }
        }
        match self {
            MainSymbol::Root(r) => panic!("There is no dependencies on Root Symbol"),
            MainSymbol::Namespace(n) => &n.dependents[level as usize][step as usize],
            MainSymbol::Package(p) => &p.dependents()[level as usize][step as usize],
            MainSymbol::File(f) => &f.dependents[level as usize][step as usize],
            MainSymbol::Compiled(c) => panic!("There is no dependencies on Compiled Symbol"),
            MainSymbol::Class(c) => panic!("There is no dependencies on Class Symbol"),
            MainSymbol::Function(f) => panic!("There is no dependencies on Function Symbol"),
            MainSymbol::Variable(v) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    //Add a symbol as dependency on the step of the other symbol for the build level.
    //-> The build of the 'step' of self requires the build of 'dep_level' of the other symbol to be done
    pub fn add_dependency(&mut self, symbol: &mut MainSymbol, step:BuildSteps, dep_level:BuildSteps) {
        if step == BuildSteps::SYNTAX || dep_level == BuildSteps::SYNTAX {
            panic!("Can't add dependency for syntax step")
        }
        if dep_level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
            }
            if dep_level == BuildSteps::VALIDATION {
                panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
            }
        }
        if self.typ() != SymType::FILE && self.typ() != SymType::PACKAGE {
            panic!("Dependencies should be only on files");
        }
        if symbol.typ() != SymType::FILE && symbol.typ() != SymType::PACKAGE {
            panic!("Dependencies should be only on files");
        }
        let step_i = step as usize;
        let level_i = dep_level as usize;
        self.dependencies_mut()[step_i][level_i].insert(symbol.get_rc().unwrap());
        symbol.dependents_as_mut()[level_i][step_i].insert(self.get_rc().unwrap());
    }

    pub fn invalidate(session: &mut SessionInfo, symbol: Rc<RefCell<MainSymbol>>, step: &BuildSteps) {
        //signals that a change occured to this symbol. "step" indicates which level of change occured.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate: VecDeque<Rc<RefCell<MainSymbol>>> = VecDeque::from([symbol.clone()]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let sym_to_inv = ref_to_inv.borrow();
            if [SymType::FILE, SymType::PACKAGE].contains(&sym_to_inv.typ()) {
                if *step == BuildSteps::ARCH {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH as usize].iter().enumerate() {
                        for sym in hashset {
                            if !MainSymbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ARCH as usize {
                                    session.sync_odoo.add_to_rebuild_arch(sym.clone());
                                } else if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(step) {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH_EVAL as usize].iter().enumerate() {
                        for sym in hashset {
                            if !MainSymbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::ODOO].contains(step) {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ODOO as usize].iter().enumerate() {
                        for sym in hashset {
                            if !MainSymbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
            }
            for sym in sym_to_inv.all_module_symbol() {
                vec_to_invalidate.push_back(sym.clone());
            }
        }
    }

    pub fn unload(session: &mut SessionInfo, symbol: Rc<RefCell<MainSymbol>>) {
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        let mut vec_to_unload: VecDeque<Rc<RefCell<MainSymbol>>> = VecDeque::from([symbol.clone()]);
        while vec_to_unload.len() > 0 {
            let ref_to_unload = vec_to_unload.front().unwrap().clone();
            let mut mut_symbol = ref_to_unload.borrow_mut();
            // Unload children first
            let mut found_one = false;
            for sym in mut_symbol.all_symbols() {
                found_one = true;
                vec_to_unload.push_front(sym.clone());
            }
            if found_one {
                continue;
            } else {
                vec_to_unload.pop_front();
            }
            if DEBUG_MEMORY && (mut_symbol.typ() == SymType::FILE || mut_symbol.typ() == SymType::PACKAGE) {
                info!("Unloading symbol {:?} at {:?}", mut_symbol.name(), mut_symbol.paths());
            }
            //unload symbol
            let parent = mut_symbol.parent().as_ref().unwrap().upgrade().unwrap().clone();
            let mut parent = parent.borrow_mut();
            drop(mut_symbol);
            parent.remove_symbol(ref_to_unload.clone());
            drop(parent);
            if vec![SymType::FILE, SymType::PACKAGE].contains(&ref_to_unload.borrow().typ()) {
                MainSymbol::invalidate(session, ref_to_unload.clone(), &BuildSteps::ARCH);
            }
            let mut mut_symbol = ref_to_unload.borrow_mut();
            match *mut_symbol {
                MainSymbol::Package(PackageSymbol::Module(m)) => {
                    session.sync_odoo.modules.remove(m.dir_name.as_str());
                },
                _ => {}
            }
        }
    }

    pub fn get_rc(&self) -> Option<Rc<RefCell<MainSymbol>>> {
        if self.weak_self().is_none() {
            return None;
        }
        if let Some(v) = &self.weak_self() {
            return Some(v.upgrade().unwrap());
        }
        None
    }

    pub fn is_file_content(&self) -> bool{
        match self {
            MainSymbol::Root(_) | MainSymbol::Namespace(_) | MainSymbol::Package(_) | MainSymbol::File(_) | MainSymbol::Compiled(_) => false,
            MainSymbol::Class(_) | MainSymbol::Function(_) | MainSymbol::Variable(_) => true
        }
    }

    ///return true if to_test is in parents of symbol or equal to it.
    pub fn is_symbol_in_parents(symbol: &Rc<RefCell<MainSymbol>>, to_test: &Rc<RefCell<MainSymbol>>) -> bool {
        if Rc::ptr_eq(symbol, to_test) {
            return true;
        }
        if symbol.borrow().parent().is_none() {
            return false;
        }
        let parent = symbol.borrow().parent().as_ref().unwrap().upgrade().unwrap();
        return MainSymbol::is_symbol_in_parents(&parent, to_test);
    }

    fn set_weak_self(&mut self, weak_self: Weak<RefCell<MainSymbol>>) {
        match self {
            MainSymbol::Root(r) => r.weak_self = Some(weak_self),
            MainSymbol::Namespace(n) => n.weak_self = Some(weak_self),
            MainSymbol::Package(PackageSymbol::Module(m)) => m.weak_self = Some(weak_self),
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self = Some(weak_self),
            MainSymbol::File(f) => f.weak_self = Some(weak_self),
            MainSymbol::Compiled(c) => c.weak_self = Some(weak_self),
            MainSymbol::Class(c) => c.weak_self = Some(weak_self),
            MainSymbol::Function(f) => f.weak_self = Some(weak_self),
            MainSymbol::Variable(v) => v.weak_self = Some(weak_self),
        }
    }

    pub fn get_in_parents(&self, sym_types: &Vec<SymType>, stop_same_file: bool) -> Option<Weak<RefCell<MainSymbol>>> {
        if sym_types.contains(&self.typ()) {
            return self.weak_self().clone();
        }
        if stop_same_file && vec![SymType::FILE, SymType::PACKAGE].contains(&self.typ()) {
            return None;
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().get_in_parents(sym_types, stop_same_file);
        }
        return None;
    }

    /// get a Symbol that has the same given range and name
    pub fn get_positioned_symbol(&self, name: &String, range: &TextRange) -> Option<Rc<RefCell<MainSymbol>>> {
        if let Some(symbols) = match self {
            MainSymbol::Class(c) => { c.symbols.get(name) },
            MainSymbol::File(f) => {f.symbols.get(name)},
            MainSymbol::Function(f) => {f.symbols.get(name)},
            MainSymbol::Package(PackageSymbol::Module(m)) => {m.symbols.get(name)},
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {p.symbols.get(name)},
            _ => {None}
        } {
            for sym_list in symbols.values() {
                for sym in sym_list.iter() {
                    if sym.borrow().range().start() == range.start() {
                        return Some(sym.clone());
                    }
                }
            }
        }
        None
    }

    pub fn remove_symbol(&mut self, symbol: Rc<RefCell<MainSymbol>>) {
        match self {
            MainSymbol::Class(c) => { c.symbols.remove(symbol.borrow().name()); },
            MainSymbol::File(f) => { f.symbols.remove(symbol.borrow().name()); },
            MainSymbol::Function(f) => { f.symbols.remove(symbol.borrow().name()); },
            MainSymbol::Package(PackageSymbol::Module(m)) => { m.symbols.remove(symbol.borrow().name()); },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => { p.symbols.remove(symbol.borrow().name()); },
            MainSymbol::Compiled(c) => {},
            MainSymbol::Namespace(n) => { n.module_symbols.remove(symbol.borrow().name());},
            MainSymbol::Root(r) => {},
            MainSymbol::Variable(v) => {}
        };
        symbol.borrow_mut().set_parent(None);
    }

    pub fn get_file(&self) -> Option<Weak<RefCell<MainSymbol>>> {
        if self.typ() == SymType::FILE || self.typ() == SymType::PACKAGE {
            return self.weak_self().clone();
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().get_file();
        }
        return None;
    }

    pub fn find_module(&self) -> Option<Rc<RefCell<MainSymbol>>> {
        match self {
            MainSymbol::Package(PackageSymbol::Module(m)) => {return self.get_rc();}
            _ => {}
        }
        if let Some(parent) = self.parent().as_ref() {
            return parent.upgrade().unwrap().borrow().find_module();
        }
        return None;
    }

    /*given a Symbol, give all the Symbol that are evaluated as valid evaluation for it.
    example:
    ====
    a = 5
    if X:
        a = Test()
    else:
        a = Object()
    print(a)
    ====
    next_refs on the 'a' in the print will return a SymbolRef to Test and one to Object
    */
    pub fn next_refs(session: &mut SessionInfo, symbol: &MainSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> VecDeque<(Weak<RefCell<MainSymbol>>, bool)> {
        match symbol {
            MainSymbol::Variable(v) => {
                let mut res = VecDeque::new();
                for eval in v.evaluations.iter() {
                    //TODO context is modified in each for loop, which is wrong !
                    res.push_back(eval.symbol.get_symbol(session, context, diagnostics));
                }
                return res
            },
            _ => {
                let mut vec = VecDeque::new();
                vec.push_back((symbol.weak_self().unwrap(), false));
                return vec
            }
        }
    }

    pub fn follow_ref(symbol: &Rc<RefCell<MainSymbol>>, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<(Weak<RefCell<MainSymbol>>, bool)> {
        //return a list of all possible evaluation: a weak ptr to the final symbol, and a bool indicating if this is an instance or not
        let mut results = MainSymbol::next_refs(session, &symbol.borrow(), &mut None, &mut vec![]);
        let can_eval_external = !symbol.borrow().is_external();
        let mut index = 0;
        while index < results.len() {
            let (sym, instance) = &results[index];
            let sym = sym.upgrade().unwrap().borrow();
            match *sym {
                MainSymbol::Variable(v) => {
                    if stop_on_type && !instance && !v.is_import_variable {
                        continue;
                    }
                    if stop_on_value && v.evaluations.len() == 1 && v.evaluations[0].value.is_some() {
                        continue;
                    }
                    if v.evaluations.is_empty() {
                        //no evaluation? let's check that the file has been evaluated
                        let file_symbol = sym.get_file();
                        match file_symbol {
                            Some(file_symbol) => {
                                drop(sym);
                                if file_symbol.upgrade().expect("invalid weak value").borrow().build_status(BuildSteps::ARCH) == BuildStatus::PENDING &&
                                session.sync_odoo.is_in_rebuild(&file_symbol.upgrade().unwrap(), BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                                    let mut builder = PythonArchEval::new(file_symbol.upgrade().unwrap());
                                    builder.eval_arch(session);
                                }
                            },
                            None => {}
                        }
                    }
                    let mut next_sym_refs = MainSymbol::next_refs(session, &sym, &mut None, &mut vec![]);
                    if next_sym_refs.len() >= 1 {
                        results.pop_front();
                        index -= 1;
                        for next_results in next_sym_refs {
                            results.push_back(next_results);
                        }
                    }
                    index += 1;
                },
                _ => {
                    index += 1;
                }
            }
        }
        return Vec::from(results) // :'( a whole copy?
    }

    pub fn all_symbols<'a>(&'a self) -> impl Iterator<Item= &'a Rc<RefCell<MainSymbol>>> + 'a {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned.
        let mut iter: Vec<Box<dyn Iterator<Item = &Rc<RefCell<MainSymbol>>>>> = Vec::new();
        match self {
            MainSymbol::File(f) => {
                iter.push(Box::new(self.iter_symbols().collect())); //TODO how does it work? :o
            },
            MainSymbol::Class(c) => {
                iter.push(Box::new(c.iter_symbols()));
            },
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                iter.push(Box::new(m.iter_symbols()));
                iter.push(Box::new(m.module_symbols.values()));
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                iter.push(Box::new(p.iter_symbols()));
                iter.push(Box::new(p.module_symbols.values()));
            },
            MainSymbol::Namespace(n) => {
                iter.push(Box::new(n.module_symbols.values()));
            },
            _ => {}
        }
        iter.into_iter().flatten()
    }

    /* return the Symbol (class, function or file) the closest to the given offset */
    pub fn get_scope_symbol(file_symbol: Rc<RefCell<MainSymbol>>, offset: u32) -> Rc<RefCell<MainSymbol>> {
        let mut result = file_symbol.clone();
        for s in file_symbol.borrow().iter_symbols() {
            let sym = s.borrow();
            match s {
                MainSymbol::Class(c) => {
                    if c.range().start().to_u32() < offset && c.range().end().to_u32() > offset {
                        result = MainSymbol::get_scope_symbol(c, offset);
                    }
                },
                MainSymbol::Function(f) => {
                    if f.range().start().to_u32() < offset && f.range().end().to_u32() > offset {
                        result = MainSymbol::get_scope_symbol(f, offset);
                    }
                },
                _ => {}
            }
        }
        return result
    }

    //infer a name, given a position
    pub fn infer_name(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<MainSymbol>>, name: &String, position: Option<TextSize>) -> Vec<Rc<RefCell<MainSymbol>>> {
        let mut results: Vec<Rc<RefCell<MainSymbol>>> = vec![];
        //TODO implement 'super' behaviour in hooks
        let on_symbol = on_symbol.borrow();
        let symbol_location = on_symbol.symbols.as_ref().unwrap();
        if let Some(symbol) = symbol_location.get(name) {
            results = symbol.borrow().get_loc_sym(position.unwrap_or(TextSize::MAX).to_u32());
        }
        if results.len() == 0 && !vec![SymType::FILE, SymType::PACKAGE, SymType::ROOT].contains(&on_symbol.sym_type) {
            let parent = on_symbol.parent.as_ref().unwrap().upgrade().unwrap();
            return MainSymbol::infer_name(odoo, &parent, name, position);
        }
        if results.len() == 0 && (on_symbol.name != "builtins" || on_symbol.sym_type != SymType::FILE) {
            let builtins = odoo.get_symbol(&(vec![S!("builtins")], vec![]), u32::MAX).as_ref().unwrap().clone();
            return MainSymbol::infer_name(odoo, &builtins, name, None);
        }
        results
    }

    pub fn get_sorted_symbols(&self) -> impl Iterator<Item = Rc<RefCell<MainSymbol>>> {
        let mut symbols: Vec<Rc<RefCell<MainSymbol>>> = Vec::new();
        match self {
            MainSymbol::Class(c) => {
                let syms = c.iter_symbols();
                for sym in syms {
                    symbols.push(sym.clone());
                }
            },
            MainSymbol::Function(c) => {
                let syms = c.iter_symbols();
                for sym in syms {
                    symbols.push(sym.clone());
                }
            },
            MainSymbol::File(f) => {
                let syms = f.iter_symbols();
                for sym in syms {
                    symbols.push(sym.clone());
                }
            },
            MainSymbol::Package(PackageSymbol::Module(m)) => {
                let syms = m.iter_symbols();
                for sym in syms {
                    symbols.push(sym.clone());
                }
            },
            MainSymbol::Package(PackageSymbol::PythonPackage(p)) => {
                let syms = p.iter_symbols();
                for sym in syms {
                    symbols.push(sym.clone());
                }
            },
            _ => {panic!()}
        }
        symbols.sort_by_key(|s| s.borrow().range().start().to_u32());
        symbols.into_iter()
    }

    /* similar to get_symbol: will return the symbol that is under this one with the specified name.
    However, if the symbol is a class or a model, it will search in the base class or in comodel classes
    if not all, it will return the first found. If all, the all found symbols are returned, but the first one
    is the one that is overriding others.
    :param: from_module: optional, can change the from_module of the given class */
    pub fn get_member_symbol(&self, session: &mut SessionInfo, name: &String, from_module: Option<Rc<RefCell<MainSymbol>>>, prevent_comodel: bool, all: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<Rc<RefCell<MainSymbol>>> {
        let mut result: Vec<Rc<RefCell<MainSymbol>>> = vec![];
        let mod_sym = self.get_module_symbol(name);
        if let Some(mod_sym) = mod_sym {
            if all {
                result.push(mod_sym);
            } else {
                return vec![mod_sym];
            }
        }
        let content_sym = self.get_content_symbol(name, u32::MAX);
        if content_sym.len() >= 1 {
            if all {
                result.extend(content_sym);
            } else {
                return content_sym;
            }
        }
        if self.typ() == SymType::CLASS && self.as_class_sym()._model.is_some() && !prevent_comodel {
            let model = session.sync_odoo.models.get(&self.as_class_sym()._model.as_ref().unwrap().name);
            if let Some(model) = model {
                let loc_symbols = model.clone().borrow().get_symbols(session, from_module.clone().unwrap_or(self.find_module().expect("unable to find module")));
                for loc_sym in loc_symbols {
                    if self.is_equal(&loc_sym) {
                        continue;
                    }
                    let attribut = loc_sym.borrow().get_member_symbol(session, name, None, true, all, diagnostics);
                    if all {
                        result.extend(attribut);
                    } else {
                        return attribut;
                    }
                }
            }
        }
        if !all && result.len() != 0 {
            return result;
        }
        if self.typ() == SymType::CLASS {
            for base in self.as_class_sym().bases.iter() {
                let s = base.borrow().get_member_symbol(session, name, from_module.clone(), prevent_comodel, all, diagnostics);
                if s.len() != 0 {
                    if all {
                        result.extend(s);
                    } else {
                        return s;
                    }
                }
            }
        }
        result
    }

    pub fn is_equal(&self, other: &Rc<RefCell<MainSymbol>>) -> bool {
        return Weak::ptr_eq(&self.weak_self().unwrap_or(Weak::new()), &Rc::downgrade(other));
    }

    fn _debug_print_graph_node(&self, acc: &mut String, level: u32) {
        for _ in 0..level {
            acc.push_str(" ");
        }
        acc.push_str(format!("{:?} {:?}\n", self.sym_type, self.name).as_str());
        for section in self.localized_sym.iter() {
            for local in section.iter() {
                for _ in 0..level {
                    acc.push_str(" ");
                }
                acc.push_str(format!("at {}", local.borrow().range.start().to_u32()).as_str());
            }
        }
        if let Some(symbol_location) = &self.symbols {
            if symbol_location.symbols().len() > 0 {
                for _ in 0..level {
                    acc.push_str(" ");
                }
                acc.push_str("SYMBOLS:\n");
                for sym in symbol_location.symbols().values() {
                    sym.borrow()._debug_print_graph_node(acc, level + 1);
                }
            }
        }
        if self.module_symbols.len() > 0 {
            for _ in 0..level {
                acc.push_str(" ");
            }
            acc.push_str("MODULES:\n");
            for (_, module) in self.module_symbols.iter() {
                module.borrow_mut()._debug_print_graph_node(acc, level + 1);
            }
        }
    }

    pub fn debug_to_json(&self) -> Value {
        let mut modules = vec![];
        let mut symbols = vec![];
        let mut offsets = vec![];
        for section in self.localized_sym.iter() {
            for local in section.iter() {
                offsets.push(local.borrow().range.start().to_u32());
            }
        }
        for s in self.module_symbols.values() {
            modules.push(s.borrow_mut().debug_to_json());
        }
        for s in self.symbols.as_ref().unwrap().symbols().values() {
            symbols.push(s.borrow_mut().debug_to_json());
        }
        json!({
            "name": self.name,
            "type": self.sym_type.to_string(),
            "offsets": offsets,
            "module_symbols": modules,
            "symbols": symbols,
        })
    }

    pub fn debug_print_graph(&self) -> String {
        info!("----Starting output of symbol debug display----");
        let mut res: String = String::new();
        self._debug_print_graph_node(&mut res, 0);
        info!("----End output of symbol debug display----");
        res
    }
}
