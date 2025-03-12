use ruff_text_size::{TextSize, TextRange};
use tracing::{info, trace};
use weak_table::traits::WeakElement;

use crate::constants::*;
use crate::core::entry_point::EntryPoint;
use crate::core::evaluation::{Context, ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak};
use crate::core::model::Model;
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval::PythonArchEval;
use crate::threads::SessionInfo;
use crate::utils::{PathSanitizer as _};
use crate::S;
use core::panic;
use std::collections::{HashMap, HashSet, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::vec;
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range};

use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::root_symbol::RootSymbol;

use super::class_symbol::ClassSymbol;
use super::compiled_symbol::CompiledSymbol;
use super::disk_dir_symbol::DiskDirSymbol;
use super::file_symbol::FileSymbol;
use super::namespace_symbol::{NamespaceDirectory, NamespaceSymbol};
use super::package_symbol::{PackageSymbol, PythonPackageSymbol};
use super::symbol_mgr::SymbolMgr;
use super::variable_symbol::VariableSymbol;

#[derive(Debug)]
pub enum Symbol {
    Root(RootSymbol),
    DiskDir(DiskDirSymbol),
    Namespace(NamespaceSymbol),
    Package(PackageSymbol),
    File(FileSymbol),
    Compiled(CompiledSymbol),
    Class(ClassSymbol),
    Function(FunctionSymbol),
    Variable(VariableSymbol),
}

impl Symbol {
    pub fn new_root() -> Rc<RefCell<Self>> {
        let root = Rc::new(RefCell::new(Symbol::Root(RootSymbol::new())));
        root.borrow_mut().set_weak_self(Rc::downgrade(&root));
        root
    }

    //Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let file = Rc::new(RefCell::new(Symbol::File(FileSymbol::new(name.clone(), path.clone(), self.is_external()))));
        file.borrow_mut().set_weak_self(Rc::downgrade(&file));
        file.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&file);
            },
            Symbol::Package(p) => {
                p.add_file(&file);
            },
            Symbol::Root(r) => {
                r.add_file(&file);
            },
            Symbol::DiskDir(d) => {
                d.add_file(&file);
            },
            _ => { panic!("Impossible to add a file to a {}", self.typ()); }
        }
        file
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let package = Rc::new(
            RefCell::new(
                Symbol::Package(
                    PackageSymbol::new_python_package(name.clone(), path.clone(), self.is_external())
                )
            )
        );
        package.borrow_mut().set_weak_self(Rc::downgrade(&package));
        package.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&package);
            },
            Symbol::Package(p) => {
                p.add_file(&package);
            },
            Symbol::Root(r) => {
                r.add_file(&package)
            },
            Symbol::DiskDir(d) => {
                d.add_file(&package)
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        package
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_module_package(&mut self, session: &mut SessionInfo, name: &String, path: &PathBuf) -> Option<Rc<RefCell<Self>>> {
        let module = PackageSymbol::new_module_package(session, name.clone(), path, self.is_external());
        if module.is_none() {
            return None;
        }
        let module = module.unwrap();
        let package = Rc::new(
            RefCell::new(
                Symbol::Package(
                    module
                )
            )
        );
        package.borrow_mut().set_weak_self(Rc::downgrade(&package));
        package.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&package);
            },
            Symbol::Package(p) => {
                p.add_file(&package);
            },
            Symbol::Root(r) => {
                r.add_file(&package)
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        Some(package)
    }

    pub fn add_new_namespace(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let namespace = Rc::new(RefCell::new(Symbol::Namespace(NamespaceSymbol::new(name.clone(), vec![path.clone()], self.is_external()))));
        namespace.borrow_mut().set_weak_self(Rc::downgrade(&namespace));
        namespace.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&namespace);
            },
            Symbol::Package(p) => {
                p.add_file(&namespace);
            },
            Symbol::Root(r) => {
                r.add_file(&namespace);
            }
            _ => { panic!("Impossible to add a namespace to a {}", self.typ()); }
        }
        namespace
    }

    pub fn add_new_disk_dir(&mut self, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let namespace = Rc::new(RefCell::new(Symbol::DiskDir(DiskDirSymbol::new(name.clone(), path.clone(), self.is_external()))));
        namespace.borrow_mut().set_weak_self(Rc::downgrade(&namespace));
        namespace.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&namespace);
            },
            Symbol::Package(p) => {
                p.add_file(&namespace);
            },
            Symbol::Root(r) => {
                r.add_file(&namespace);
            },
            Symbol::DiskDir(d) => {
                d.add_file(&namespace);
            }
            _ => { panic!("Impossible to add a namespace to a {}", self.typ()); }
        }
        namespace
    }

    pub fn add_new_compiled(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let compiled = Rc::new(RefCell::new(Symbol::Compiled(CompiledSymbol::new(name.clone(), path.clone(), self.is_external()))));
        compiled.borrow_mut().set_weak_self(Rc::downgrade(&compiled));
        compiled.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&compiled);
            },
            Symbol::Package(p) => {
                p.add_file(&compiled);
            },
            Symbol::Root(r) => {
                r.add_file(&compiled);
            },
            Symbol::Compiled(c) => {
                c.add_compiled(&compiled);
            },
            Symbol::DiskDir(d) => {
                d.add_file(&compiled);
            }
            _ => { panic!("Impossible to add a compiled to a {}", self.typ()); }
        }
        compiled
    }

    pub fn add_new_variable(&mut self, _session: &mut SessionInfo, name: &String, range: &TextRange) -> Rc<RefCell<Self>> {
        let variable = Rc::new(RefCell::new(Symbol::Variable(VariableSymbol::new(name.clone(), range.clone(), self.is_external()))));
        variable.borrow_mut().set_weak_self(Rc::downgrade(&variable));
        variable.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&variable, section);
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(&variable, section);
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(&variable, section);
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                c.add_symbol(&variable, section);
            },
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&variable, section);
            }
            _ => { panic!("Impossible to add a variable to a {}", self.typ()); }
        }
        variable
    }

    pub fn add_new_function(&mut self, _session: &mut SessionInfo, name: &String, range: &TextRange, body_start: &TextSize) -> Rc<RefCell<Self>> {
        let function = Rc::new(RefCell::new(Symbol::Function(FunctionSymbol::new(name.clone(), range.clone(), body_start.clone(), self.is_external()))));
        function.borrow_mut().set_weak_self(Rc::downgrade(&function));
        function.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&function, section);
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(&function, section);
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(&function, section);
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                c.add_symbol(&function, section);
            }
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&function, section);
            }
            _ => { panic!("Impossible to add a function to a {}", self.typ()); }
        }
        function
    }

    pub fn add_new_class(&mut self, _session: &mut SessionInfo, name: &String, range: &TextRange, body_start: &TextSize) -> Rc<RefCell<Self>> {
        let class = Rc::new(RefCell::new(Symbol::Class(ClassSymbol::new(name.clone(), range.clone(), body_start.clone(), self.is_external()))));
        class.borrow_mut().set_weak_self(Rc::downgrade(&class));
        class.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&class, section);
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(&class, section);
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(&class, section);
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                c.add_symbol(&class, section);
            }
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&class, section);
            }
            _ => { panic!("Impossible to add a class to a {}", self.typ()); }
        }
        class
    }

    pub fn as_root(&self) -> &RootSymbol {
        match self {
            Symbol::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }
    pub fn as_root_mut(&mut self) -> &mut RootSymbol {
        match self {
            Symbol::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }
    pub fn as_file(&self) -> &FileSymbol {
        match self {
            Symbol::File(f) => f,
            _ => {panic!("Not a File")}
        }
    }
    pub fn as_file_mut(&mut self) -> &mut FileSymbol {
        match self {
            Symbol::File(f) => f,
            _ => {panic!("Not a File")}
        }
    }
    pub fn as_package(&self) -> &PackageSymbol {
        match self {
            Symbol::Package(p) => p,
            _ => {panic!("Not a package")}
        }
    }
    pub fn as_package_mut(&mut self) -> &mut PackageSymbol {
        match self {
            Symbol::Package(p) => p,
            _ => {panic!("Not a package")}
        }
    }
    pub fn as_module_package(&self) -> &ModuleSymbol {
        match self {
            Symbol::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }
    pub fn as_module_package_mut(&mut self) -> &mut ModuleSymbol {
        match self {
            Symbol::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }

    pub fn as_python_package(&self) -> &PythonPackageSymbol {
        match self {
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p,
            _ => {panic!("Not a python package")}
        }
    }
    pub fn as_python_package_mut(&mut self) -> &mut PythonPackageSymbol {
        match self {
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p,
            _ => {panic!("Not a python package")}
        }
    }

    pub fn as_namespace(&self) -> &NamespaceSymbol {
        match self {
            Symbol::Namespace(n) => n,
            _ => {panic!("Not a namespace")}
        }
    }

    pub fn as_namespace_mut(&mut self) -> &mut NamespaceSymbol {
        match self {
            Symbol::Namespace(n) => n,
            _ => {panic!("Not a namespace")}
        }
    }

    pub fn as_variable(&self) -> &VariableSymbol {
        match self {
            Symbol::Variable(v) => v,
            _ => {panic!("Not a variable")}
        }
    }

    pub fn as_variable_mut(&mut self) -> &mut VariableSymbol {
        match self {
            Symbol::Variable(v) => v,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_func(&self) -> &FunctionSymbol {
        match self {
            Symbol::Function(f) => f,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_func_mut(&mut self) -> &mut FunctionSymbol {
        match self {
            Symbol::Function(f) => f,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_class_sym(&self) -> &ClassSymbol {
        match self {
            Symbol::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }

    pub fn as_class_sym_mut(&mut self) -> &mut ClassSymbol {
        match self {
            Symbol::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }

    pub fn as_disk_dir_sym(&self) -> &DiskDirSymbol {
        match self {
            Symbol::DiskDir(d) => d,
            _ => {panic!("Not a disk_dir")}
        }
    }

    pub fn as_disk_dir_sym_mut(&mut self) -> &mut DiskDirSymbol {
        match self {
            Symbol::DiskDir(d) => d,
            _ => {panic!("Not a disk_dir")}
        }
    }

    pub fn as_symbol_mgr(&self) -> &dyn SymbolMgr {
        match self {
            Symbol::File(f) => f,
            Symbol::Class(c) => c,
            Symbol::Function(f) => f,
            Symbol::Package(PackageSymbol::Module(m)) => m,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p,
            _ => {panic!("Not a symbol Mgr");}
        }
    }

    pub fn typ(&self) -> SymType {
        match self {
            Symbol::Root(_) => SymType::ROOT,
            Symbol::Namespace(_) => SymType::NAMESPACE,
            Symbol::DiskDir(_) => SymType::DISK_DIR,
            Symbol::Package(PackageSymbol::Module(_)) => SymType::PACKAGE(PackageType::MODULE),
            Symbol::Package(PackageSymbol::PythonPackage(_)) => SymType::PACKAGE(PackageType::PYTHON_PACKAGE),
            Symbol::File(_) => SymType::FILE,
            Symbol::Compiled(_) => SymType::COMPILED,
            Symbol::Class(_) => SymType::CLASS,
            Symbol::Function(_) => SymType::FUNCTION,
            Symbol::Variable(_) => SymType::VARIABLE,
        }
    }

    pub fn name(&self) -> &String {
        match self {
            Symbol::Root(r) => &r.name,
            Symbol::DiskDir(d) => &d.name,
            Symbol::Namespace(n) => &n.name,
            Symbol::Package(p) => &p.name(),
            Symbol::File(f) => &f.name,
            Symbol::Compiled(c) => &c.name,
            Symbol::Class(c) => &c.name,
            Symbol::Function(f) => &f.name,
            Symbol::Variable(v) => &v.name,
        }
    }

    pub fn doc_string(&self) -> &Option<String> {
        match self {
            Symbol::Root(_) => &None,
            Self::DiskDir(_) => &None,
            Symbol::Namespace(_) => &None,
            Symbol::Package(_) => &None,
            Symbol::File(_) => &None,
            Symbol::Compiled(_) => &None,
            Symbol::Class(c) => &c.doc_string,
            Symbol::Function(f) => &f.doc_string,
            Symbol::Variable(v) => &v.doc_string,
        }
    }

    pub fn set_doc_string(&mut self, doc_string: Option<String>) {
        match self {
            Symbol::Root(_) => panic!(),
            Self::DiskDir(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::Package(_) => panic!(),
            Symbol::File(_) => panic!(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(c) => c.doc_string = doc_string,
            Symbol::Function(f) => f.doc_string = doc_string,
            Symbol::Variable(v) => v.doc_string = doc_string,
        }
    }

    pub fn is_external(&self) -> bool {
        match self {
            Symbol::Root(_) => false,
            Self::DiskDir(d) => d.is_external,
            Symbol::Namespace(n) => n.is_external,
            Symbol::Package(p) => p.is_external(),
            Symbol::File(f) => f.is_external,
            Symbol::Compiled(c) => c.is_external,
            Symbol::Class(c) => c.is_external,
            Symbol::Function(f) => f.is_external,
            Symbol::Variable(v) => v.is_external,
        }
    }
    pub fn set_is_external(&mut self, external: bool) {
        match self {
            Symbol::Root(_) => {},
            Self::DiskDir(d) => d.is_external = external,
            Symbol::Namespace(n) => n.is_external = external,
            Symbol::Package(PackageSymbol::Module(m)) => m.is_external = external,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.is_external = external,
            Symbol::File(f) => f.is_external = external,
            Symbol::Compiled(c) => c.is_external = external,
            Symbol::Class(c) => c.is_external = external,
            Symbol::Function(f) => f.is_external = external,
            Symbol::Variable(v) => v.is_external = external,
        }
    }

    pub fn has_range(&self) -> bool {
        match self {
            Symbol::Root(_) => false,
            Self::DiskDir(_) => false,
            Symbol::Namespace(_) => false,
            Symbol::Package(_) => false,
            Symbol::File(_) => false,
            Symbol::Compiled(_) => false,
            Symbol::Class(_) => true,
            Symbol::Function(_) => true,
            Symbol::Variable(_) => true,
        }
    }

    pub fn range(&self) -> &TextRange {
        match self {
            Symbol::Root(_) => panic!(),
            Self::DiskDir(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::Package(_) => panic!(),
            Symbol::File(_) => panic!(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(c) => &c.range,
            Symbol::Function(f) => &f.range,
            Symbol::Variable(v) => &v.range,
        }
    }

    pub fn body_range(&self) -> &TextRange {
        match self {
            Symbol::Root(_) => panic!(),
            Self::DiskDir(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::Package(_) => panic!(),
            Symbol::File(_) => panic!(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(c) => &c.body_range,
            Symbol::Function(f) => &f.body_range,
            Symbol::Variable(_) => panic!(),
        }
    }

    pub fn has_ast_indexes(&self) -> bool {
        match self {
            Symbol::Variable(_) => true,
            Symbol::Class(_) => true,
            Symbol::Function(_) => true,
            Self::DiskDir(_) => false,
            Symbol::File(_) => false,
            Symbol::Compiled(_) => false,
            Symbol::Namespace(_) => false,
            Symbol::Package(_) => false,
            Symbol::Root(_) => false,
        }
    }

    pub fn ast_indexes(&self) -> Option<&Vec<u16>> {
        match self {
            Symbol::Variable(v) => Some(&v.ast_indexes),
            Symbol::Class(c) => Some(&c.ast_indexes),
            Symbol::Function(f) => Some(&f.ast_indexes),
            Self::DiskDir(_) => None,
            Symbol::File(_) => None,
            Symbol::Compiled(_) => None,
            Symbol::Namespace(_) => None,
            Symbol::Package(_) => None,
            Symbol::Root(_) => None,
        }
    }

    pub fn ast_indexes_mut(&mut self) -> &mut Vec<u16> {
        match self {
            Symbol::Variable(v) => &mut v.ast_indexes,
            Symbol::Class(c) => &mut c.ast_indexes,
            Symbol::Function(f) => &mut f.ast_indexes,
            Self::DiskDir(_) => panic!(),
            Symbol::File(_) => panic!(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::Package(_) => panic!(),
            Symbol::Root(_) => panic!(),
        }
    }

    pub fn weak_self(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            Symbol::Root(r) => r.weak_self.clone(),
            Symbol::Namespace(n) => n.weak_self.clone(),
            Self::DiskDir(d) => d.weak_self.clone(),
            Symbol::Package(PackageSymbol::Module(m)) => m.weak_self.clone(),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self.clone(),
            Symbol::File(f) => f.weak_self.clone(),
            Symbol::Compiled(c) => c.weak_self.clone(),
            Symbol::Class(c) => c.weak_self.clone(),
            Symbol::Function(f) => f.weak_self.clone(),
            Symbol::Variable(v) => v.weak_self.clone(),
        }
    }

    pub fn parent(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            Symbol::Root(r) => r.parent.clone(),
            Symbol::Namespace(n) => n.parent.clone(),
            Self::DiskDir(d) => d.parent.clone(),
            Symbol::Package(p) => p.parent(),
            Symbol::File(f) => f.parent.clone(),
            Symbol::Compiled(c) => c.parent.clone(),
            Symbol::Class(c) => c.parent.clone(),
            Symbol::Function(f) => f.parent.clone(),
            Symbol::Variable(v) => v.parent.clone(),
        }
    }

    fn set_parent(&mut self, parent: Option<Weak<RefCell<Symbol>>>) {
        match self {
            Symbol::Root(_) => panic!(),
            Symbol::Namespace(n) => n.parent = parent,
            Self::DiskDir(d) => d.parent = parent,
            Symbol::Package(p) => p.set_parent(parent),
            Symbol::File(f) => f.parent = parent,
            Symbol::Compiled(c) => c.parent = parent,
            Symbol::Class(c) => c.parent = parent,
            Symbol::Function(f) => f.parent = parent,
            Symbol::Variable(v) => v.parent = parent,
        }
    }

    pub fn paths(&self) -> Vec<String> {
        match self {
            Symbol::Root(r) => r.paths.clone(),
            Symbol::Namespace(n) => n.paths(),
            Symbol::DiskDir(d) => vec![d.path.clone()],
            Symbol::Package(p) => p.paths(),
            Symbol::File(f) => vec![f.path.clone()],
            Symbol::Compiled(c) => vec![c.path.clone()],
            Symbol::Class(_) => vec![],
            Symbol::Function(_) => vec![],
            Symbol::Variable(_) => vec![],
        }
    }
    pub fn add_path(&mut self, path: String) {
        match self {
            Symbol::Root(r) => r.paths.push(path),
            Symbol::Namespace(n) => {
                n.directories.push(NamespaceDirectory { path: path, module_symbols: HashMap::new() });
            },
            Symbol::DiskDir(_) => {},
            Symbol::Package(_) => {},
            Symbol::File(_) => {},
            Symbol::Compiled(_) => {},
            Symbol::Class(_) => {},
            Symbol::Function(_) => {},
            Symbol::Variable(_) => {},
        }
    }

    pub fn get_symbol_first_path(&self) -> String{
        match self{
            Symbol::Package(p) => PathBuf::from(p.paths()[0].clone()).join("__init__.py").sanitize() + p.i_ext().as_str(),
            Symbol::File(f) => f.path.clone(),
            Symbol::DiskDir(d) => panic!("invalid symbol type to extract path"),
            Symbol::Root(_) => panic!("invalid symbol type to extract path"),
            Symbol::Namespace(_) => panic!("invalid symbol type to extract path"),
            Symbol::Compiled(_) => panic!("invalid symbol type to extract path"),
            Symbol::Class(_) => panic!("invalid symbol type to extract path"),
            Symbol::Function(_) => panic!("invalid symbol type to extract path"),
            Symbol::Variable(_) => panic!("invalid symbol type to extract path"),
        }
    }

    pub fn dependencies(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4] {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &n.dependencies,
            Self::DiskDir(d) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependencies(),
            Symbol::File(f) => &f.dependencies,
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependencies_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4] {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &mut n.dependencies,
            Self::DiskDir(d) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependencies_as_mut(),
            Symbol::File(f) => &mut f.dependencies,
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependents(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3] {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &n.dependents,
            Self::DiskDir(d) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependents(),
            Symbol::File(f) => &f.dependents,
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3] {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &mut n.dependents,
            Self::DiskDir(d) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependents_as_mut(),
            Symbol::File(f) => &mut f.dependents,
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
        }
    }
    pub fn has_modules(&self) -> bool {
        match self {
            Symbol::Root(_) | Symbol::Namespace(_) | Symbol::Package(_) | Symbol::DiskDir(_) => true,
            _ => {false}
        }
    }
    pub fn all_module_symbol(&self) -> Box<dyn Iterator<Item = &Rc<RefCell<Symbol>>> + '_> {
        match self {
            Symbol::Root(r) => Box::new(r.module_symbols.values()),
            Symbol::Namespace(n) => {
                Box::new(n.directories.iter().flat_map(|x| x.module_symbols.values()))
            },
            Symbol::DiskDir(d) => Box::new(d.module_symbols.values()),
            Symbol::Package(PackageSymbol::Module(m)) => Box::new(m.module_symbols.values()),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => Box::new(p.module_symbols.values()),
            Symbol::File(_) => panic!("No module symbol on File"),
            Symbol::Compiled(_) => panic!("No module symbol on Compiled"),
            Symbol::Class(_c) => panic!("No module symbol on Class"),
            Symbol::Function(_) => panic!("No module symbol on Function"),
            Symbol::Variable(_) => panic!("No module symbol on Variable"),
        }
    }
    pub fn in_workspace(&self) -> bool {
        match self {
            Symbol::Root(_) => false,
            Symbol::Namespace(n) => n.in_workspace,
            Symbol::DiskDir(d) => d.in_workspace,
            Symbol::Package(PackageSymbol::Module(m)) => m.in_workspace,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace,
            Symbol::File(f) => f.in_workspace,
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(_) => panic!(),
            Symbol::Function(_) => panic!(),
            Symbol::Variable(_) => panic!(),
        }
    }
    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        match self {
            Symbol::Root(_) => panic!(),
            Symbol::Namespace(n) => n.in_workspace = in_workspace,
            Symbol::DiskDir(d) => d.in_workspace = in_workspace,
            Symbol::Package(PackageSymbol::Module(m)) => m.in_workspace = in_workspace,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace = in_workspace,
            Symbol::File(f) => f.in_workspace = in_workspace,
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(_) => panic!(),
            Symbol::Function(_) => panic!(),
            Symbol::Variable(_) => panic!(),
        }
    }
    pub fn build_status(&self, step:BuildSteps) -> BuildStatus {
        match self {
            Symbol::Root(_) => {panic!()},
            Symbol::Namespace(_) => {panic!()},
            Symbol::DiskDir(_) => {panic!()},
            Symbol::Package(PackageSymbol::Module(m)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => m.arch_status,
                    BuildSteps::ARCH_EVAL => m.arch_eval_status,
                    BuildSteps::ODOO => m.odoo_status,
                    BuildSteps::VALIDATION => m.validation_status,
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status,
                    BuildSteps::ODOO => p.odoo_status,
                    BuildSteps::VALIDATION => p.validation_status,
                }
            }
            Symbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
                    BuildSteps::ODOO => f.odoo_status,
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            Symbol::Compiled(_) => todo!(),
            Symbol::Class(_) => todo!(),
            Symbol::Function(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
                    BuildSteps::ODOO => f.odoo_status,
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            Symbol::Variable(_) => todo!(),
        }
    }
    pub fn set_build_status(&mut self, step:BuildSteps, status: BuildStatus) {
        match self {
            Symbol::Root(_) => {panic!()},
            Symbol::Namespace(_) => {panic!()},
            Symbol::DiskDir(_) => {panic!()},
            Symbol::Package(PackageSymbol::Module(m)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => m.arch_status = status,
                    BuildSteps::ARCH_EVAL => m.arch_eval_status = status,
                    BuildSteps::ODOO => m.odoo_status = status,
                    BuildSteps::VALIDATION => m.validation_status = status,
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status = status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status = status,
                    BuildSteps::ODOO => p.odoo_status = status,
                    BuildSteps::VALIDATION => p.validation_status = status,
                }
            }
            Symbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
                    BuildSteps::ODOO => f.odoo_status = status,
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(_) => panic!(),
            Symbol::Function(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
                    BuildSteps::ODOO => f.odoo_status = status,
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            Symbol::Variable(_) => todo!(),
        }
    }

    pub fn iter_symbols(&self) -> std::collections::hash_map::Iter<String, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>> {
        match self {
            Symbol::File(f) => {
                f.symbols.iter()
            }
            Symbol::Root(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::DiskDir(_) => panic!(),
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.symbols.iter()
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.symbols.iter()
            }
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(c) => {
                c.symbols.iter()
            },
            Symbol::Function(f) => {
                f.symbols.iter()
            },
            Symbol::Variable(_) => panic!(),
        }
    }
    pub fn evaluations(&self) -> Option<&Vec<Evaluation>> {
        match self {
            Symbol::File(_) => { None },
            Symbol::Root(_) => { None },
            Symbol::Namespace(_) => { None },
            Symbol::DiskDir(_) => { None },
            Symbol::Package(_) => { None },
            Symbol::Compiled(_) => { None },
            Symbol::Class(_) => { None },
            Symbol::Function(f) => Some(&f.evaluations),
            Symbol::Variable(v) => Some(&v.evaluations),
        }
    }
    pub fn evaluations_mut(&mut self) -> Option<&mut Vec<Evaluation>> {
        match self {
            Symbol::File(_) => { None },
            Symbol::Root(_) => { None },
            Symbol::Namespace(_) => { None },
            Symbol::DiskDir(_) => { None },
            Symbol::Package(_) => { None },
            Symbol::Compiled(_) => { None },
            Symbol::Class(_) => { None },
            Symbol::Function(f) => Some(&mut f.evaluations),
            Symbol::Variable(v) => Some(&mut v.evaluations),
        }
    }
    pub fn set_evaluations(&mut self, data: Vec<Evaluation>) {
        match self {
            Symbol::File(_) => { panic!() },
            Symbol::Root(_) => { panic!() },
            Symbol::Namespace(_) => { panic!() },
            Symbol::DiskDir(_) => { panic!() },
            Symbol::Package(_) => { panic!() },
            Symbol::Compiled(_) => { panic!() },
            Symbol::Class(_) => { panic!() },
            Symbol::Function(f) => { f.evaluations = data; },
            Symbol::Variable(v) => v.evaluations = data,
        }
    }

    pub fn not_found_paths(&self) -> &Vec<(BuildSteps, Vec<String>)> {
        static EMPTY_VEC: Vec<(BuildSteps, Vec<String>)> = Vec::new();
        match self {
            Symbol::File(f) => { &f.not_found_paths },
            Symbol::Root(_) => { &EMPTY_VEC },
            Symbol::Namespace(_) => { &EMPTY_VEC },
            Symbol::DiskDir(_) => { &EMPTY_VEC },
            Symbol::Package(PackageSymbol::Module(m)) => { &m.not_found_paths },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => { &p.not_found_paths },
            Symbol::Compiled(_) => { &EMPTY_VEC },
            Symbol::Class(_) => { &EMPTY_VEC },
            Symbol::Function(_) => { &EMPTY_VEC },
            Symbol::Variable(_) => &EMPTY_VEC,
        }
    }

    pub fn not_found_paths_mut(&mut self) -> &mut Vec<(BuildSteps, Vec<String>)> {
        match self {
            Symbol::File(f) => { &mut f.not_found_paths },
            Symbol::Root(_) => { panic!("no not_found_path on Root") },
            Symbol::Namespace(_) => { panic!("no not_found_path on Namespace") },
            Symbol::DiskDir(_) => { panic!("no not_found_path on DiskDir") },
            Symbol::Package(PackageSymbol::Module(m)) => { &mut m.not_found_paths },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => { &mut p.not_found_paths },
            Symbol::Compiled(_) => { panic!("no not_found_path on Compiled") },
            Symbol::Class(_) => { panic!("no not_found_path on Class") },
            Symbol::Function(_) => { panic!("no not_found_path on Function") },
            Symbol::Variable(_) => panic!("no not_found_path on Variable"),
        }
    }

    pub fn get_main_entry_tree(&self, session: &mut SessionInfo) -> (Vec<String>, Vec<String>) {
        let mut tree = self.get_tree();
        let len_first_part = tree.0.len();
        let odoo_tree = &session.sync_odoo.main_entry_tree;
        if len_first_part >= odoo_tree.len() {
            for component in odoo_tree.iter() {
                if tree.0.len() > 0 && &tree.0[0] == component {
                    tree.0.remove(0);
                } else {
                    return self.get_tree();
                }
            }
        }
        tree
    }

    ///Given a path, create the appropriated symbol and attach it to the given parent
    pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<Rc<RefCell<Symbol>>> {
        let name: String = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.sanitize();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            return Some(parent.borrow_mut().add_new_file(session, &name, &path_str));
        }
        if parent.borrow().get_main_entry_tree(session) == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
            let module = parent.borrow_mut().add_new_module_package(session, &name, path);
            if let Some(module) = module {
                ModuleSymbol::load_module_info(module.clone(), session, parent.clone());
                session.sync_odoo.modules.insert(module.borrow().as_module_package().dir_name.clone(), Rc::downgrade(&module));
                return Some(module);
            } else if require_module {
                return None;
            } else {
                if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                    let ref_sym = (*parent).borrow_mut().add_new_python_package(session, &name, &path_str);
                    if !path.join("__init__.py").exists() {
                        (*ref_sym).borrow_mut().as_package_mut().set_i_ext("i".to_string());
                    }
                    return Some(ref_sym);
                } else {
                    return None;
                }
            }
        } else if require_module {
            return None;
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                if parent.borrow().get_main_entry_tree(session) == tree(vec!["odoo"], vec![]) && path_str.ends_with("addons") {
                    //Force namespace for odoo/addons
                    let ref_sym = (*parent).borrow_mut().add_new_namespace(session, &name, &path_str);
                    return Some(ref_sym);
                } else {
                    let ref_sym = parent.borrow_mut().add_new_python_package(session, &name, &path_str);
                    if !path.join("__init__.py").exists() {
                        ref_sym.borrow_mut().as_package_mut().set_i_ext("i".to_string());
                    }
                    return Some(ref_sym);
                }
            } else if path.is_dir() {
                let ref_sym = (*parent).borrow_mut().add_new_namespace(session, &name, &path_str);
                return Some(ref_sym);
            }
        }
        None
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

    

    pub fn get_tree_and_entry(&self) -> (Tree, Option<Rc<RefCell<EntryPoint>>>) {
        let mut res = ((vec![], vec![]), None);
        if self.is_file_content() {
            res.0.1.insert(0, self.name().clone());
        } else {
            res.0.0.insert(0, self.name().clone());
        }
        if self.typ() == SymType::ROOT || self.parent().is_none() {
            return res
        }
        let parent = self.parent().clone();
        let mut current_arc = parent.as_ref().unwrap().upgrade().unwrap();
        let mut current = current_arc.borrow_mut();
        while current.typ() != SymType::ROOT && current.parent().is_some() {
            if current.is_file_content() {
                res.0.1.insert(0, current.name().clone());
            } else {
                res.0.0.insert(0, current.name().clone());
            }
            let parent = current.parent().clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.borrow_mut();
        }
        if current.typ() == SymType::ROOT {
            res.1 = current.as_root().entry_point.clone();
        }
        res
    }

    pub fn get_symbol(&self, tree: &Tree, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        let symbol_tree_files: &Vec<String> = &tree.0;
        let symbol_tree_content: &Vec<String> = &tree.1;
        let mut iter_sym: Vec<Rc<RefCell<Symbol>>> = vec![];
        if symbol_tree_files.len() != 0 {
            let _mod_iter_sym = self.get_module_symbol(&symbol_tree_files[0]);
            if _mod_iter_sym.is_none() {
                return vec![];
            }
            iter_sym = vec![_mod_iter_sym.unwrap()];
            if symbol_tree_files.len() > 1 {
                for fk in symbol_tree_files[1..symbol_tree_files.len()].iter() {
                    if let Some(s) = iter_sym.last().unwrap().clone().borrow_mut().get_module_symbol(fk) {
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
                    let _iter_sym = iter_sym[0].borrow_mut().get_sub_symbol(fk, position);
                    iter_sym = _iter_sym;
                    if iter_sym.is_empty() {
                        return vec![];
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 {
                return vec![];
            }
            iter_sym = self.get_sub_symbol(&symbol_tree_content[0], position);
            if iter_sym.is_empty() {
                return vec![];
            }
            if symbol_tree_content.len() > 1 {
                if iter_sym.len() > 1 {
                    trace!("TODO: explore all implementation possibilities");
                }
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    let _iter_sym = iter_sym[0].borrow_mut().get_sub_symbol(fk, position);
                    iter_sym = _iter_sym;
                    return iter_sym.clone();
                }
            }
        }
        iter_sym
    }

    /*
    Return a symbol that is in module symbols (symbol that represent something on disk - file, package, namespace)
     */
    pub fn get_module_symbol(&self, name: &str) -> Option<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Namespace(n) => {
                for dir in n.directories.iter() {
                    let result = dir.module_symbols.get(name);
                    if let Some(result) = result {
                        return Some(result.clone());
                    }
                }
                None
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.module_symbols.get(name).cloned()
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.module_symbols.get(name).cloned()
            }
            Symbol::Root(r) => {
                r.module_symbols.get(name).cloned()
            },
            Symbol::DiskDir(d) => {
                d.module_symbols.get(name).cloned()
            }
            _ => {None}
        }
    }

    /**
     * Return all symbol before the given position that match the name in the body of the symbol
     */
    pub fn get_content_symbol(&self, name: &str, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Class(c) => {
                c.get_symbol(name.to_string(), position)
            },
            Symbol::File(f) => {
                f.get_symbol(name.to_string(), position)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.get_symbol(name.to_string(), position)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_symbol(name.to_string(), position)
            },
            Symbol::Function(f) => {
                f.get_symbol(name.to_string(), position)
            },
            _ => {vec![]}
        }
    }

    /**
     * Return a symbol that can be called from outside of the body of the symbol
     */
    pub fn get_sub_symbol(&self, name: &str, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Class(c) => {
                c.get_symbol(name.to_string(), position)
            },
            Symbol::File(f) => {
                f.get_symbol(name.to_string(), position)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.get_symbol(name.to_string(), position)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_symbol(name.to_string(), position)
            },
            Symbol::Function(f) => {
                if let Some(vec) = f.get_ext_symbol(name.to_string()) {
                    return vec.clone();
                }
                vec![]
            },
            _ => {vec![]}
        }
    }

    //Return a HashSet of all symbols (constructed until 'level') that are dependencies for the 'step' of this symbol
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<Symbol>>> {
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
            Symbol::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => &n.dependencies[step as usize][level as usize],
            Symbol::DiskDir(d) => panic!("There is no dependencies on DiskDir Symbol"),
            Symbol::Package(p) => &p.dependencies()[step as usize][level as usize],
            Symbol::File(f) => &f.dependencies[step as usize][level as usize],
            Symbol::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    pub fn get_all_dependencies(&self, step: BuildSteps) -> &Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        match self {
            Symbol::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => &n.dependencies[step as usize],
            Symbol::DiskDir(d) => panic!("There is no dependencies on DiskDir Symbol"),
            Symbol::Package(p) => &p.dependencies()[step as usize],
            Symbol::File(f) => &f.dependencies[step as usize],
            Symbol::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<Symbol>>> {
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
            Symbol::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => &n.dependents[level as usize][step as usize],
            Symbol::DiskDir(d) => panic!("There is no dependencies on DiskDir Symbol"),
            Symbol::Package(p) => &p.dependents()[level as usize][step as usize],
            Symbol::File(f) => &f.dependents[level as usize][step as usize],
            Symbol::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    //Add a symbol as dependency on the step of the other symbol for the build level.
    //-> The build of the 'step' of self requires the build of 'dep_level' of the other symbol to be done
    pub fn add_dependency(&mut self, symbol: &mut Symbol, step:BuildSteps, dep_level:BuildSteps) {
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
        let step_i = step as usize;
        let level_i = dep_level as usize;
        self.dependencies_mut()[step_i][level_i].insert(symbol.get_rc().unwrap());
        symbol.dependents_as_mut()[level_i][step_i].insert(self.get_rc().unwrap());
    }

    pub fn add_model_dependencies(&mut self, model: &Rc<RefCell<Model>>) {
        match self {
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.model_dependencies.insert(model.clone());
                model.borrow_mut().add_dependent(&self.weak_self().unwrap().upgrade().unwrap());
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.model_dependencies.insert(model.clone());
                model.borrow_mut().add_dependent(&self.weak_self().unwrap().upgrade().unwrap());
            }
            Symbol::File(f) => {
                f.model_dependencies.insert(model.clone());
                model.borrow_mut().add_dependent(&self.weak_self().unwrap().upgrade().unwrap());
            },
            Symbol::Function(f) => {
                f.model_dependencies.insert(model.clone());
                model.borrow_mut().add_dependent(&self.weak_self().unwrap().upgrade().unwrap());
            }
            _ => {}
        }
    }

    pub fn invalidate(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>, step: &BuildSteps) {
        //signals that a change occured to this symbol. "step" indicates which level of change occured.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let sym_to_inv = ref_to_inv.borrow();
            if matches!(&sym_to_inv.typ(), SymType::FILE | SymType::PACKAGE(_)) {
                if *step == BuildSteps::ARCH {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ARCH as usize {
                                    session.sync_odoo.add_to_rebuild_arch(sym.clone());
                                } else if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    sym.borrow_mut().invalidate_sub_functions(session);
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(step) {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH_EVAL as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    sym.borrow_mut().invalidate_sub_functions(session);
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::ODOO].contains(step) {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ODOO as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    sym.borrow_mut().invalidate_sub_functions(session);
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                    for class in sym_to_inv.iter_classes() {
                        if let Some(model_data) = &class.borrow().as_class_sym()._model {
                            let model = session.sync_odoo.models.get(&model_data.name).cloned();
                            if let Some(model) = model {
                                let from_module = class.borrow().find_module();
                                model.borrow().add_dependents_to_validation(session, from_module);
                            }
                        }
                    }
                }
            }
            if sym_to_inv.has_modules() {
                for sym in sym_to_inv.all_module_symbol() {
                    vec_to_invalidate.push_back(sym.clone());
                }
            }
        }
    }

    pub fn invalidate_sub_functions(&mut self, _session: &mut SessionInfo) {
        if matches!(&self.typ(), SymType::FILE | SymType::PACKAGE(_)) {
            for func in self.iter_inner_functions() {
                func.borrow_mut().evaluations_mut().unwrap().clear();
                func.borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
                func.borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            }
        }
    }

    //unload a symbol and subsymbols. Return a list of paths of files and packages that have been deleted
    pub fn unload(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        let mut vec_to_unload: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while !vec_to_unload.is_empty() {
            let ref_to_unload = vec_to_unload.front().unwrap().clone();
            let mut_symbol = ref_to_unload.borrow_mut();
            // Unload children first
            let mut found_one = false;
            for sym in mut_symbol.all_symbols() {
                found_one = true;
                vec_to_unload.push_front(sym.clone());
            }
            if found_one {
                continue;
            }
            vec_to_unload.pop_front();
            if DEBUG_MEMORY && (mut_symbol.typ() == SymType::FILE || matches!(mut_symbol.typ(), SymType::PACKAGE(_))) {
                info!("Unloading symbol {:?} at {:?}", mut_symbol.name(), mut_symbol.paths());
            }
            let module = mut_symbol.find_module();
            //unload symbol
            let parent = mut_symbol.parent().as_ref().unwrap().upgrade().unwrap().clone();
            let mut parent_bw = parent.borrow_mut();
            drop(mut_symbol);
            parent_bw.remove_symbol(ref_to_unload.clone());
            drop(parent_bw);
            if matches!(&ref_to_unload.borrow().typ(), SymType::FILE | SymType::PACKAGE(_)) {
                Symbol::invalidate(session, ref_to_unload.clone(), &BuildSteps::ARCH);
            }
            //check if we should not reimport automatically
            match ref_to_unload.borrow().typ() {
                SymType::PACKAGE(PackageType::PYTHON_PACKAGE) => {
                    if ref_to_unload.borrow().as_python_package().self_import {
                        session.sync_odoo.must_reload_paths.push((Rc::downgrade(&parent), ref_to_unload.borrow().paths().first().unwrap().clone()));
                    }
                },
                SymType::FILE => {
                    if ref_to_unload.borrow().as_file().self_import {
                        session.sync_odoo.must_reload_paths.push((Rc::downgrade(&parent), ref_to_unload.borrow().paths().first().unwrap().clone()));
                    }
                }
                _ => {}
            }
            match *ref_to_unload.borrow_mut() {
                Symbol::Package(PackageSymbol::Module(ref mut m)) => {
                    session.sync_odoo.modules.remove(m.dir_name.as_str());
                },
                Symbol::Class(ref mut c) => {
                    if let Some(model_data) = c._model.as_ref() {
                        let model = session.sync_odoo.models.get(&model_data.name).cloned();
                        if let Some(model) = model {
                            model.borrow_mut().remove_symbol(session, &ref_to_unload, module);
                        }
                    }
                },
                _ => {}
            }
            drop(ref_to_unload);
        }
    }

    pub fn get_rc(&self) -> Option<Rc<RefCell<Symbol>>> {
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
            Symbol::Root(_) | Symbol::Namespace(_) | Symbol::DiskDir(_) | Symbol::Package(_) | Symbol::File(_) | Symbol::Compiled(_) => false,
            Symbol::Class(_) | Symbol::Function(_) | Symbol::Variable(_) => true
        }
    }

    ///return true if to_test is in parents of symbol or equal to it.
    pub fn is_symbol_in_parents(symbol: &Rc<RefCell<Symbol>>, to_test: &Rc<RefCell<Symbol>>) -> bool {
        if Rc::ptr_eq(symbol, to_test) {
            return true;
        }
        if symbol.borrow().parent().is_none() {
            return false;
        }
        let parent = symbol.borrow().parent().as_ref().unwrap().upgrade().unwrap();
        Symbol::is_symbol_in_parents(&parent, to_test)
    }

    fn set_weak_self(&mut self, weak_self: Weak<RefCell<Symbol>>) {
        match self {
            Symbol::Root(r) => r.weak_self = Some(weak_self),
            Symbol::Namespace(n) => n.weak_self = Some(weak_self),
            Symbol::DiskDir(d) => d.weak_self = Some(weak_self),
            Symbol::Package(PackageSymbol::Module(m)) => m.weak_self = Some(weak_self),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self = Some(weak_self),
            Symbol::File(f) => f.weak_self = Some(weak_self),
            Symbol::Compiled(c) => c.weak_self = Some(weak_self),
            Symbol::Class(c) => c.weak_self = Some(weak_self),
            Symbol::Function(f) => f.weak_self = Some(weak_self),
            Symbol::Variable(v) => v.weak_self = Some(weak_self),
        }
    }

    pub fn set_processed_text_hash(&mut self, hash: u64){
        match self {
            Symbol::File(f) => f.processed_text_hash = hash,
            Symbol::DiskDir(_) => panic!("set_processed_text_hash called on DiskDir"),
            Symbol::Package(PackageSymbol::Module(m)) => m.processed_text_hash = hash,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.processed_text_hash = hash,
            Symbol::Function(_) => panic!("set_processed_text_hash called on Function"),
            Symbol::Root(_) => panic!("set_processed_text_hash called on Root"),
            Symbol::Namespace(_) => panic!("set_processed_text_hash called on Namespace"),
            Symbol::Compiled(_) => panic!("set_processed_text_hash called on Compiled"),
            Symbol::Class(_) => panic!("set_processed_text_hash called on Class"),
            Symbol::Variable(_) => panic!("set_processed_text_hash called on Variable"),
        }
    }

    pub fn get_processed_text_hash(&self) -> u64{
        match self {
            Symbol::File(f) => f.processed_text_hash,
            Symbol::Package(PackageSymbol::Module(m)) => m.processed_text_hash,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.processed_text_hash,
            Symbol::DiskDir(_) => panic!("get_processed_text_hash called on DiskDir"),
            Symbol::Function(_) => panic!("get_processed_text_hash called on Function"),
            Symbol::Root(_) => panic!("get_processed_text_hash called on Root"),
            Symbol::Namespace(_) => panic!("get_processed_text_hash called on Namespace"),
            Symbol::Compiled(_) => panic!("get_processed_text_hash called on Compiled"),
            Symbol::Class(_) => panic!("get_processed_text_hash called on Class"),
            Symbol::Variable(_) => panic!("get_processed_text_hash called on Variable"),
        }
    }

    pub fn get_in_parents(&self, sym_types: &Vec<SymType>, stop_same_file: bool) -> Option<Weak<RefCell<Symbol>>> {
        if sym_types.contains(&self.typ()) {
            return self.weak_self().clone();
        }
        if stop_same_file && matches!(&self.typ(), SymType::FILE | SymType::PACKAGE(_)) {
            return None;
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().get_in_parents(sym_types, stop_same_file);
        }
        return None;
    }

    pub fn get_root(&self) -> Option<Weak<RefCell<Symbol>>> {
        self.get_in_parents(&vec![SymType::ROOT], false)
    }

    pub fn get_entry(&self) -> Option<Rc<RefCell<EntryPoint>>> {
        if let Some(root) = self.get_root() {
            if let Some(root) = root.upgrade() {
                return root.borrow().as_root().entry_point.clone();
            }
        }
        None
    }

    pub fn has_rc_in_parents(&self, rc: Rc<RefCell<Symbol>>, stop_same_file: bool) -> bool {
        if Rc::ptr_eq(&self.weak_self().unwrap().upgrade().unwrap(), &rc) {
            return true;
        }
        if stop_same_file && matches!(&self.typ(), SymType::FILE | SymType::PACKAGE(_)) {
            return false;
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().has_rc_in_parents(rc, stop_same_file);
        }
        false
    }

    /// get a Symbol that has the same given range and name
    pub fn get_positioned_symbol(&self, name: &String, range: &TextRange) -> Option<Rc<RefCell<Symbol>>> {
        if let Some(symbols) = match self {
            Symbol::Class(c) => { c.symbols.get(name) },
            Symbol::File(f) => {f.symbols.get(name)},
            Symbol::Function(f) => {f.symbols.get(name)},
            Symbol::Package(PackageSymbol::Module(m)) => {m.symbols.get(name)},
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {p.symbols.get(name)},
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

    pub fn remove_symbol(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if symbol.borrow().is_file_content() {
            match self {
                Symbol::Class(c) => { c.symbols.remove(symbol.borrow().name()); },
                Symbol::File(f) => { f.symbols.remove(symbol.borrow().name()); },
                Symbol::Function(f) => { f.symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::Module(m)) => { m.symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::PythonPackage(p)) => { p.symbols.remove(symbol.borrow().name()); },
                Symbol::DiskDir(_) => { panic!("A disk directory can not contain python code") },
                Symbol::Compiled(_) => { panic!("A compiled symbol can not contain python code") },
                Symbol::Namespace(_) => { panic!("A namespace can not contain python code") },
                Symbol::Root(_) => { panic!("Root can not contain python code") },
                Symbol::Variable(_) => { panic!("A variable can not contain python code") }
            };
        } else {
            match self {
                Symbol::Class(_) => { panic!("A class can not contain a file structure") },
                Symbol::File(_) => { panic!("A file can not contain a file structure"); },
                Symbol::Function(_) => { panic!("A function can not contain a file structure") },
                Symbol::DiskDir(d) => { d.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::Module(m)) => { m.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::PythonPackage(p)) => { p.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Compiled(c) => { c.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Namespace(n) => {
                    for directory in n.directories.iter_mut() {
                        directory.module_symbols.remove(symbol.borrow().name());
                    }
                },
                Symbol::Root(r) => { r.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Variable(_) => { panic!("A variable can not contain a file structure"); }
            };
        }
        symbol.borrow_mut().set_parent(None);
    }

    pub fn get_file(&self) -> Option<Weak<RefCell<Symbol>>> {
        if self.typ() == SymType::FILE || matches!(self.typ(), SymType::PACKAGE(_)) {
            return self.weak_self().clone();
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow().get_file();
        }
        None
    }

    pub fn parent_file_or_function(&self) -> Option<Weak<RefCell<Symbol>>> {
        if self.typ() == SymType::FILE || matches!(self.typ(), SymType::PACKAGE(_)) || self.typ() == SymType::FUNCTION {
            return self.weak_self().clone();
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().parent_file_or_function();
        }
        None
    }

    pub fn find_module(&self) -> Option<Rc<RefCell<Symbol>>> {
        if let Symbol::Package(PackageSymbol::Module(m)) = self {return self.get_rc();}
        if let Some(parent) = self.parent().as_ref() {
            return parent.upgrade().unwrap().borrow().find_module();
        }
        None
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
    pub fn next_refs(session: &mut SessionInfo, symbol: &Symbol, context: &mut Option<Context>, symbol_context: &Context, stop_on_type: bool, diagnostics: &mut Vec<Diagnostic>) -> VecDeque<EvaluationSymbolPtr> {
        //if current symbol is a descriptor, we have to resolve __get__ method before going further
        if let Some(base_attr) = symbol_context.get(&S!("base_attr")) {
            let base_attr = base_attr.as_symbol().upgrade();
            if let Some(base_attr) = base_attr {
                let attribute_type_sym = symbol;
                //TODO shouldn't we set the from_module in the call to get_member_symbol?
                let get_method = attribute_type_sym.get_member_symbol(session, &S!("__get__"), None, true, false, true, false).0.first().cloned();
                match get_method {
                    Some(get_method) if (base_attr.borrow().typ() == SymType::CLASS) => {
                        let get_method = get_method.borrow();
                        if get_method.evaluations().is_some() {
                            let mut res = VecDeque::new();
                            if context.is_none() {
                                *context = Some(HashMap::new());
                            }
                            for get_method_eval in get_method.evaluations().unwrap().iter() {
                                context.as_mut().unwrap().extend(symbol_context.clone().into_iter());
                                let get_result = get_method_eval.symbol.get_symbol_as_weak(session, context, diagnostics, None);
                                if !get_result.weak.is_expired() {
                                    let mut eval = Evaluation::eval_from_symbol(&get_result.weak, get_result.instance);
                                    match eval.symbol.get_mut_symbol_ptr() {
                                        EvaluationSymbolPtr::WEAK(ref mut weak) => {
                                            weak.context.insert(S!("base_attr"), ContextValue::SYMBOL(Rc::downgrade(&base_attr)));
                                            res.push_back(eval.symbol.get_symbol_ptr().clone());
                                        },
                                        _ => {}
                                    }
                                }
                                context.as_mut().unwrap().retain(|k, _| !symbol_context.contains_key(k));
                            }
                            return res;
                        }
                    },
                    _ => {}
                }
            }
        }
        match symbol {
            Symbol::Variable(v) => {
                let mut res = VecDeque::new();
                for eval in v.evaluations.iter() {
                    let ctx = &mut Some(symbol_context.clone().into_iter().chain(context.clone().unwrap_or(HashMap::new()).into_iter()).collect::<HashMap<_, _>>());
                    let mut sym = eval.symbol.get_symbol(session, ctx, diagnostics, None);
                    match sym {
                        EvaluationSymbolPtr::WEAK(ref mut w) => {
                            if let Some(base_attr) = symbol_context.get(&S!("base_attr")) {
                                w.context.insert(S!("base_attr"), base_attr.clone());
                            }
                        },
                        _ => {}
                    }
                    if !sym.is_expired_if_weak() {
                        res.push_back(sym);
                    }
                }
                res
            },
            _ => {
                let vec = VecDeque::new();
                vec
            }
        }
    }

    /*
    Follow evaluation of current symbol until type, value or end of the chain, depending or the parameters.
    If a symbol in the chain is a descriptor, return the __get__ return evaluation.
     */
    pub fn follow_ref(evaluation: &EvaluationSymbolPtr, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, max_scope: Option<Rc<RefCell<Symbol>>>, diagnostics: &mut Vec<Diagnostic>) -> Vec<EvaluationSymbolPtr> {
        match evaluation {
            EvaluationSymbolPtr::WEAK(w) => {
                let Some(symbol) = w.weak.upgrade() else {
                    return vec![evaluation.clone()];
                };
                if stop_on_value {
                    if let Some(evals) = symbol.borrow().evaluations() {
                        for eval in evals.iter() {
                            if eval.value.is_some() {
                                return vec![evaluation.clone()];
                            }
                        }
                    }
                }
                //return a list of all possible evaluation: a weak ptr to the final symbol, and a bool indicating if this is an instance or not
                //TODO there is no loop detection
                let mut results = Symbol::next_refs(session, &symbol.borrow(), context, &w.context, stop_on_type, &mut vec![]);
                if results.is_empty() {
                    return vec![evaluation.clone()];
                }
                let can_eval_external = !symbol.borrow().is_external();
                let mut index = 0;
                while index < results.len() {
                    let next_ref = &results[index];
                    match next_ref {
                        EvaluationSymbolPtr::WEAK(next_ref_weak) => {
                            let sym = next_ref_weak.weak.upgrade();
                            if sym.is_none() {
                                index += 1;
                                continue;
                            }
                            let sym = sym.unwrap();
                            let sym = sym.borrow();
                            match *sym {
                                Symbol::Variable(ref v) => {
                                    if stop_on_type && matches!(next_ref_weak.is_instance(), Some(false)) && !v.is_import_variable {
                                        index += 1;
                                        continue;
                                    }
                                    if stop_on_value && v.evaluations.len() == 1 && v.evaluations[0].value.is_some() {
                                        index += 1;
                                        continue;
                                    }
                                    if max_scope.is_some() && !sym.has_rc_in_parents(max_scope.as_ref().unwrap().clone(), true) {
                                        index += 1;
                                        continue;
                                    }
                                    if v.evaluations.is_empty() && can_eval_external {
                                        //no evaluation? let's check that the file has been evaluated
                                        let file_symbol = sym.get_file();
                                        if let Some(file_symbol) = file_symbol {
                                            if file_symbol.upgrade().expect("invalid weak value").borrow().build_status(BuildSteps::ARCH) == BuildStatus::PENDING &&
                                            session.sync_odoo.is_in_rebuild(&file_symbol.upgrade().unwrap(), BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                                                let file_entry = file_symbol.upgrade().unwrap().borrow().get_entry().unwrap();
                                                let mut builder = PythonArchEval::new(file_entry, file_symbol.upgrade().unwrap());
                                                builder.eval_arch(session);
                                            }
                                        }
                                    }
                                    let next_sym_refs = Symbol::next_refs(session, &sym, context, &next_ref_weak.context, stop_on_type, &mut vec![]);
                                    index += 1;
                                    if !next_sym_refs.is_empty() {
                                        results.pop_front();
                                        index -= 1;
                                        for next_results in next_sym_refs {
                                            results.push_back(next_results);
                                        }
                                    }
                                },
                                Symbol::Class(ref c) => {
                                    //On class, follow descriptor declarations
                                    let next_sym_refs = Symbol::next_refs(session, &sym, context, &next_ref_weak.context, stop_on_type, &mut vec![]);
                                    index += 1;
                                    if !next_sym_refs.is_empty() {
                                        results.pop_front();
                                        index -= 1;
                                        for next_results in next_sym_refs {
                                            results.push_back(next_results);
                                        }
                                    }
                                },
                                _ => {
                                    index += 1;
                                }
                            }
                        },
                        _ => {index += 1}
                    }
                    
                }
                Vec::from(results) // :'( a whole copy?
            },
            _ => {
                return vec![evaluation.clone()];
            }
        }
    }

    pub fn all_symbols(&self) -> impl Iterator<Item= Rc<RefCell<Symbol>>> {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned.
        let mut iter: Vec<Rc<RefCell<Symbol>>> = Vec::new();
        match self {
            Symbol::File(_) => {
                for symbol in self.iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Class(_) => {
                for symbol in self.iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Function(_) => {
                for symbol in self.iter_symbols().flat_map(|(name, hashmap)| hashmap.iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                for symbol in self.iter_symbols().flat_map(|(name, hashmap)| hashmap.iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
                for symbol in m.module_symbols.values().cloned() {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                for symbol in self.iter_symbols().flat_map(|(name, hashmap)| hashmap.iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
                for symbol in p.module_symbols.values().cloned() {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Namespace(n) => {
                for symbol in n.directories.iter().flat_map(|x| x.module_symbols.values().cloned()) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Root(root) => {
                for symbol in root.module_symbols.values().cloned() {
                    iter.push(symbol.clone());
                }
            }
            _ => {}
        }
        iter.into_iter()
    }

    //store in result all available members for self: sub symbols, base class elements and models symbols
    //TODO is order right of Vec in HashMap? if we take first or last in it, do we have the last effective value?
    pub fn all_members(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo, result: &mut HashMap<String, Vec<(Rc<RefCell<Symbol>>, Option<String>)>>, with_co_models: bool, from_module: Option<Rc<RefCell<Symbol>>>, acc: &mut Option<HashSet<Tree>>, is_super: bool) {
        if acc.is_none() {
            *acc = Some(HashSet::new());
        }
        let tree = symbol.borrow().get_tree();
        if acc.as_mut().unwrap().contains(&tree) {
            return;
        }
        acc.as_mut().unwrap().insert(tree);
        let typ = symbol.borrow().typ();
        match typ {
            SymType::CLASS => {
                // Skip current class symbols for super
                if !is_super{
                    for symbol in symbol.borrow().all_symbols() {
                        let name = symbol.borrow().name().clone();
                        if let Some(vec) = result.get_mut(&name) {
                            vec.push((symbol, None));
                        } else {
                            result.insert(name.clone(), vec![(symbol, None)]);
                        }
                    }
                }
                if with_co_models {
                    let sym = symbol.borrow();
                    let model_data =  sym.as_class_sym()._model.as_ref();
                    if let Some(model_data) = model_data {
                        if let Some(model) = session.sync_odoo.models.get(&model_data.name).cloned() {
                            for (model_sym, dependency) in model.borrow().all_symbols(session, from_module.clone()) {
                                if dependency.is_none() && !Rc::ptr_eq(symbol, &model_sym) {
                                    for s in model_sym.borrow().all_symbols() {
                                        let name = s.borrow().name().clone();
                                        if let Some(vec) = result.get_mut(&name) {
                                            vec.push((s, Some(model_sym.borrow().name().clone())));
                                        } else {
                                            result.insert(name.clone(), vec![(s, Some(model_sym.borrow().name().clone()))]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let bases = symbol.borrow().as_class_sym().bases.clone();
                for base in bases.iter() {
                    //no comodel as we will process only model in base class (overrided _name?)
                    if let Some(base) = base.upgrade() {
                        Symbol::all_members(&base, session, result, false, from_module.clone(), acc, false);
                    }
                }
            },
            _ => {
                for symbol in symbol.borrow().all_symbols() {
                    let name = symbol.borrow().name().clone();
                    if let Some(vec) = result.get_mut(&name) {
                        vec.push((symbol, None));
                    } else {
                        result.insert(name.clone(), vec![(symbol, None)]);
                    }
                }
            }
        }
    }
    /* return the Symbol (class, function or file) the closest to the given offset */
    pub fn get_scope_symbol(file_symbol: Rc<RefCell<Symbol>>, offset: u32, is_param: bool) -> Rc<RefCell<Symbol>> {
        let mut result = file_symbol.clone();
        let section_id = file_symbol.borrow().as_symbol_mgr().get_section_for(offset);
        for (sym_name, sym_map) in file_symbol.borrow().iter_symbols() {
            match sym_map.get(&section_id.index) {
                Some(symbols) => {
                    for symbol in symbols.iter() {
                        let typ = symbol.borrow().typ();
                        match typ {
                            SymType::CLASS => {
                                let range = match is_param {
                                    true => symbol.borrow().range().start().to_u32(),
                                    false => symbol.borrow().body_range().start().to_u32(),
                                };
                                if range <= offset && symbol.borrow().body_range().end().to_u32() > offset {
                                    result = Symbol::get_scope_symbol(symbol.clone(), offset, is_param);
                                }
                            },
                            SymType::FUNCTION => {
                                let range = match is_param {
                                    true => symbol.borrow().range().start().to_u32(),
                                    false => symbol.borrow().body_range().start().to_u32(),
                                };
                                if range <= offset && symbol.borrow().body_range().end().to_u32() > offset {
                                    result = Symbol::get_scope_symbol(symbol.clone(), offset, is_param);
                                }
                            }
                            _ => {}
                        }
                    }
                },
                None => {}
            }
        }
        result
    }

    /*
    Return all the symbols that are available at a given position or in a scope for a given start name
     */
    pub fn get_all_infered_names(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: Option<u32>) -> Vec<Rc<RefCell<Symbol>>> {
        let mut results = vec![];
        //get local symbols
        on_symbol.borrow().all_symbols().for_each(|sym| {
            if sym.borrow().name().starts_with(name) {
                if position.is_none() || !sym.borrow().has_range() || position.unwrap() > sym.borrow().range().end().to_u32() {
                    results.push(sym.clone());
                }
            }
        });
        //get global symbols
        if let Some(file) = on_symbol.borrow().get_file().clone() {
            let file = file.upgrade().unwrap();
            file.borrow().all_symbols().for_each(|sym| {
                if sym.borrow().name().starts_with(name) {
                    if position.is_none() || !sym.borrow().has_range() || position.unwrap() > sym.borrow().range().end().to_u32() {
                        results.push(sym.clone());
                    }
                }
            });
        }
        results
    }

    //infer a name, given a position
    pub fn infer_name(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: Option<u32>) -> Vec<Rc<RefCell<Symbol>>> {
        let mut results: Vec<Rc<RefCell<Symbol>>> = vec![];
        //TODO implement 'super' behaviour in hooks
        let on_symbol = on_symbol.borrow();
        results = on_symbol.get_content_symbol(name, position.unwrap_or(u32::MAX));
        if results.is_empty() && !matches!(&on_symbol.typ(), SymType::FILE | SymType::PACKAGE(_) | SymType::ROOT) {
            let mut parent = on_symbol.parent().as_ref().unwrap().upgrade().unwrap();
            while parent.borrow().typ() == SymType::CLASS {
                let _parent = parent.borrow().parent().unwrap().upgrade().unwrap();
                parent = _parent;
            }
            return Symbol::infer_name(odoo, &parent, name, position);
        }
        if results.is_empty() && (on_symbol.name() != "builtins" || on_symbol.typ() != SymType::FILE) {
            let builtins = odoo.get_symbol("", &(vec![S!("builtins")], vec![]), u32::MAX)[0].clone();
            return Symbol::infer_name(odoo, &builtins, name, None);
        }
        results
    }

    pub fn get_sorted_symbols(&self) -> impl Iterator<Item = Rc<RefCell<Symbol>>> {
        let mut symbols: Vec<Rc<RefCell<Symbol>>> = Vec::new();
        match self {
            Symbol::Class(_) | Symbol::Function(_) | Symbol::File(_) | Symbol::Package(PackageSymbol::Module(_)) |
            Symbol::Package(PackageSymbol::PythonPackage(_)) => {
                let syms = self.iter_symbols();
                for (sym_name, map) in syms {
                    for (index, syms) in map.iter() {
                        for sym in syms.iter() {
                            symbols.push(sym.clone());
                        }
                    }
                }
            },
            _ => {panic!()}
        }
        symbols.sort_by_key(|s| s.borrow().range().start().to_u32());
        symbols.into_iter()
    }

    /* Hook for get_member_symbol
    Position is set to [0,0], because inside the method there is no concept of the current position.
    The setting of the position is then delegated to the calling function.
    TODO Consider refactoring.
     */
    fn member_symbol_hook(&self, session: &SessionInfo, name: &String, diagnostics: &mut Vec<Diagnostic>){
        if session.sync_odoo.version_major >= 17 && name == "Form"{
            let tree = self.get_tree();
            if tree == (vec![S!("odoo"), S!("tests"), S!("common")], vec!()){
                diagnostics.push(Diagnostic::new(Range::new(Position::new(0,0),Position::new(0,0)),
                    Some(DiagnosticSeverity::WARNING),
                    Some(NumberOrString::String(S!("OLS20006"))),
                    Some(EXTENSION_NAME.to_string()),
                    S!("Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form"),
                    None,
                    Some(vec![DiagnosticTag::DEPRECATED]),
                )
                );
            }
        }
    }

    pub fn is_field(&self, session: &mut SessionInfo) -> bool {
        match self.typ() {
            SymType::VARIABLE => {
                if let Some(evals) = self.evaluations().as_ref() {
                    for eval in evals.iter() {
                        let symbol = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
                        let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None, &mut vec![]);
                        for eval_weak in eval_weaks.iter() {
                            if let Some(symbol) = eval_weak.upgrade_weak() {
                                if symbol.borrow().is_field_class(session){
                                    return true;
                                }
                            }
                        }
                    }
                }
                false
            },
            _ => false
        }
    }

    pub fn is_field_class(&self, session: &mut SessionInfo) -> bool {
        let tree = flatten_tree(&self.get_main_entry_tree(session));
        if tree.len() == 3 && tree[0] == "odoo" && tree[1] == "fields" {
            if matches!(tree[2].as_str(), "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
        "Binary" | "Image" | "Selection" | "Reference" | "Json" | "Properties" | "PropertiesDefinition" | "Id" | "Many2one" | "One2many" | "Many2many" | "Many2oneReference") {
                return true;
            }
        }
        false
    }

    pub fn is_specific_field(&self, session: &mut SessionInfo, field_names: &[&str]) -> bool {
        match self.typ() {
            SymType::VARIABLE => {
                if let Some(evals) = self.evaluations().as_ref() {
                    for eval in evals.iter() {
                        let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
                        let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None, &mut vec![]);
                        for eval_weak in eval_weaks.iter() {
                            if let Some(symbol) = eval_weak.upgrade_weak() {
                                let tree = flatten_tree(&symbol.borrow().get_main_entry_tree(session));
                                if tree.len() == 3 && tree[0] == "odoo" && tree[1] == "fields" {
                                    if field_names.contains(&tree[2].as_str()) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
                false
            },
            _ => {false}
        }
    }

    pub fn match_tree_from_any_entry(&self, session: &mut SessionInfo, tree: &Tree) -> bool {
        let (mut self_tree, entry) = self.get_tree_and_entry();
        'outer: for entry in session.sync_odoo.entry_point_mgr.borrow().iter_for_import(&entry.unwrap()) {
            if entry.borrow().tree.len() > self_tree.0.len() {
                continue;
            }
            for (index, tree_el) in entry.borrow().tree.iter().enumerate() {
                if &self_tree.0[index] != tree_el {
                    continue 'outer;
                }
            }
            return (self_tree.0.split_off(entry.borrow().tree.len()), self_tree.1) == *tree;
        }
        false
    }

    /* similar to get_symbol: will return the symbol that is under this one with the specified name.
    However, if the symbol is a class or a model, it will search in the base class or in comodel classes
    if not all, it will return the first found. If all, the all found symbols are returned, but the first one
    is the one that is overriding others.
    :param: from_module: optional, can change the from_module of the given class */
    pub fn get_member_symbol(&self, session: &mut SessionInfo, name: &String, from_module: Option<Rc<RefCell<Symbol>>>, prevent_comodel: bool, only_fields: bool, all: bool, is_super: bool) -> (Vec<Rc<RefCell<Symbol>>>, Vec<Diagnostic>) {
        let mut visited_classes: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();
        return self._get_member_symbol_helper(session, name, from_module, prevent_comodel, only_fields, all, is_super, &mut visited_classes);
    }

    fn _get_member_symbol_helper(&self, session: &mut SessionInfo, name: &String, from_module: Option<Rc<RefCell<Symbol>>>, prevent_comodel: bool, only_fields: bool, all: bool, is_super: bool, visited_classes: &mut PtrWeakHashSet<Weak<RefCell<Symbol>>>) -> (Vec<Rc<RefCell<Symbol>>>, Vec<Diagnostic>) {
        let mut result: Vec<Rc<RefCell<Symbol>>> = vec![];
        let mut visited_symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();
        let mut extend_result = |syms: Vec<Rc<RefCell<Symbol>>>| {
            syms.iter().for_each(|sym|{
                if !visited_symbols.contains(sym){
                    visited_symbols.insert(sym.clone());
                    result.push(sym.clone());
                }
            });
        };
        let mut diagnostics: Vec<Diagnostic> = vec![];
        self.member_symbol_hook(session, name, &mut diagnostics);
        let mod_sym = self.get_module_symbol(name);
        if let Some(mod_sym) = mod_sym {
            if !only_fields {
                if all {
                    extend_result(vec![mod_sym]);
                } else {
                    return (vec![mod_sym], diagnostics);
                }
            }
        }
        if !is_super{
            let mut content_syms = self.get_sub_symbol(name, u32::MAX);
            if only_fields {
                content_syms = content_syms.iter().filter(|x| x.borrow().is_field(session)).cloned().collect();
            }
            if !content_syms.is_empty() {
                if all {
                    extend_result(content_syms);
                } else {
                    return (content_syms, diagnostics);
                }
            }
        }
        if self.typ() == SymType::CLASS && self.as_class_sym()._model.is_some() && !prevent_comodel {
            let model = session.sync_odoo.models.get(&self.as_class_sym()._model.as_ref().unwrap().name).cloned();
            if let Some(model) = model {
                let mut from_module = from_module.clone();
                if from_module.is_none() {
                    from_module = self.find_module();
                }
                if let Some(from_module) = from_module {
                    let model_symbols = model.clone().borrow().get_full_model_symbols(session, from_module.clone());
                    for model_symbol in model_symbols {
                        if self.is_equal(&model_symbol) || visited_classes.contains(&model_symbol){
                            continue;
                        }
                        visited_classes.insert(model_symbol.clone());
                        let (attributs, att_diagnostic) = model_symbol.borrow()._get_member_symbol_helper(session, name, None, true, only_fields, all, false, visited_classes);
                        diagnostics.extend(att_diagnostic);
                        if all {
                            extend_result(attributs);
                        } else {
                            if !attributs.is_empty() {
                                return (attributs, diagnostics);
                            }
                        }
                    }
                    for model_inherits_symbol in model.clone().borrow().get_inherits_models(session, Some(from_module.clone())) {
                        //only fields are visibles on inherits, not methods
                        let model_symbols = model_inherits_symbol.borrow().get_full_model_symbols(session, from_module.clone());
                        for model_symbol in model_symbols {
                            if self.is_equal(&model_symbol) || visited_classes.contains(&model_symbol){
                                continue;
                            }
                            visited_classes.insert(model_symbol.clone());
                            let (attributs, att_diagnostic) = model_symbol.borrow()._get_member_symbol_helper(session, name, None, true, only_fields, all, false, visited_classes);
                            diagnostics.extend(att_diagnostic);
                            if all {
                                extend_result(attributs);
                            } else {
                                if !attributs.is_empty() {
                                    return (attributs, diagnostics);
                                }
                            }
                        }
                    }
                }
            }
        }
        if self.typ() == SymType::CLASS {
            for base in self.as_class_sym().bases.iter() {
                let base = match base.upgrade(){
                    Some(b) => b,
                    None => continue
                };
                if visited_classes.contains(&base){
                    continue;
                }
                visited_classes.insert(base.clone());
                let (s, s_diagnostic) = base.borrow().get_member_symbol(session, name, from_module.clone(), prevent_comodel, only_fields, all, false);
                    diagnostics.extend(s_diagnostic);
                if !s.is_empty() {
                    if all {
                        extend_result(s);
                    } else {
                        return (s, diagnostics);
                    }
                }
            }
        }
        (result, diagnostics)
    }

    pub fn is_equal(&self, other: &Rc<RefCell<Symbol>>) -> bool {
        Weak::ptr_eq(&self.weak_self().unwrap_or_default(), &Rc::downgrade(other))
    }

    /**
     * Only browse file content, do not use on namespace or packages to browse disk
     * return a list of functions under Class symbol
     */
    pub fn iter_inner_functions(&self) -> Vec<Rc<RefCell<Symbol>>> {
        let mut res = vec![];

        fn iter_recursive(symbol: &Symbol, res: &mut Vec<Rc<RefCell<Symbol>>>) {
            match symbol {
                Symbol::Class(c) => {
                    for (_name, section) in c.symbols.iter() {
                        for (_position, symbol_list) in section.iter() {
                            for symbol in symbol_list.iter() {
                                if let Symbol::Function(_) = *symbol.borrow() {
                                    res.push(symbol.clone())
                                }
                            }
                        }
                    }
                },
                Symbol::File(f) => {
                    for (_name, section) in f.symbols.iter() {
                        for (_position, symbol_list) in section.iter() {
                            for symbol in symbol_list.iter() {
                                iter_recursive(&symbol.borrow(), res);
                            }
                        }
                    }
                },
                Symbol::Function(f) => {
                    for (_name, section) in f.symbols.iter() {
                        for (_position, symbol_list) in section.iter() {
                            for symbol in symbol_list.iter() {
                                iter_recursive(&symbol.borrow(), res);
                            }
                        }
                    }
                },
                Symbol::DiskDir(_) => {},
                Symbol::Root(_) => {},
                Symbol::Namespace(_) => {},
                Symbol::Package(_) => {},
                Symbol::Compiled(_) => {},
                Symbol::Variable(_) => {},
            }
        }

        iter_recursive(self, &mut res);

        res
    }

    pub fn iter_classes(&self) -> Vec<Rc<RefCell<Symbol>>> {
        let mut res = vec![];

        fn iter_recursive(symbol: &Symbol, res: &mut Vec<Rc<RefCell<Symbol>>>) {
            match symbol {
                Symbol::Class(c) => {
                    res.push(c.weak_self.as_ref().unwrap().upgrade().unwrap().clone());
                    for (_name, section) in c.symbols.iter() {
                        for (_position, symbol_list) in section.iter() {
                            for symbol in symbol_list.iter() {
                                iter_recursive(&symbol.borrow(), res);
                            }
                        }
                    }
                },
                Symbol::File(f) => {
                    for (_name, section) in f.symbols.iter() {
                        for (_position, symbol_list) in section.iter() {
                            for symbol in symbol_list.iter() {
                                iter_recursive(&symbol.borrow(), res);
                            }
                        }
                    }
                },
                Symbol::Function(f) => {
                    for (_name, section) in f.symbols.iter() {
                        for (_position, symbol_list) in section.iter() {
                            for symbol in symbol_list.iter() {
                                iter_recursive(&symbol.borrow(), res);
                            }
                        }
                    }
                },
                Symbol::DiskDir(d) => {},
                Symbol::Root(_) => {},
                Symbol::Namespace(_) => {},
                Symbol::Package(_) => {},
                Symbol::Compiled(_) => {},
                Symbol::Variable(_) => {},
            }
        }

        iter_recursive(self, &mut res);

        res
    }

    pub fn print_dependencies(&self) {
        println!("------- Output dependencies of {} -------", self.name());
        println!("--- ARCH");
        println!("--- on ARCH");
        for sym in self.dependencies()[0][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- ARCH EVAL");
        println!("--- on ARCH");
        for sym in self.dependencies()[1][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- ODOO");
        println!("--- on ARCH");
        for sym in self.dependencies()[2][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ARCH EVAL");
        for sym in self.dependencies()[2][1].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ODOO");
        for sym in self.dependencies()[2][2].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- VALIDATION");
        println!("--- on ARCH");
        for sym in self.dependencies()[3][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ARCH EVAL");
        for sym in self.dependencies()[3][1].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ODOO");
        for sym in self.dependencies()[3][2].iter() {
            println!("{:?}", sym.borrow().paths());
        }
    }

    pub fn get_base_distance(&self, base_name: &String, level: i32) -> i32 {
        if self.name().eq(base_name) {
            return level;
        }
        if self.typ() == SymType::CLASS {
            for base in self.as_class_sym().bases.iter() {
                let base = match base.upgrade(){
                    Some(b) => b,
                    None => continue
                };
                let base = base.borrow();
                let res = base.get_base_distance(base_name, level + 1);
                if res != -1 {
                    return res;
                }
            }
        }
        return -1;
    }

    /*fn _debug_print_graph_node(&self, acc: &mut String, level: u32) {
        for _ in 0..level {
            acc.push_str(" ");
        }
        acc.push_str(format!("{:?} {:?}\n", self.typ(), self.name()).as_str());
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
    }*/
}
