use ruff_python_ast::AtomicNodeIndex;
use ruff_text_size::{TextSize, TextRange};
use tracing::{info, trace};
use weak_table::traits::WeakElement;

use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::file_mgr::NoqaInfo;
use crate::core::xml_data::OdooData;
use crate::{constants::*, oyarn, Sy};
use crate::core::entry_point::EntryPoint;
use crate::core::evaluation::{Context, ContextValue, Evaluation, EvaluationSymbolPtr};
use crate::core::model::Model;
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::utils::{compare_semver, PathSanitizer as _};
use crate::S;
use core::panic;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::vec;
use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};

use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::root_symbol::RootSymbol;

use super::class_symbol::ClassSymbol;
use super::compiled_symbol::CompiledSymbol;
use super::csv_file_symbol::CsvFileSymbol;
use super::disk_dir_symbol::DiskDirSymbol;
use super::file_symbol::FileSymbol;
use super::namespace_symbol::{NamespaceDirectory, NamespaceSymbol};
use super::package_symbol::{PackageSymbol, PythonPackageSymbol};
use super::symbol_mgr::{ContentSymbols, SymbolMgr};
use super::variable_symbol::VariableSymbol;
use super::xml_file_symbol::XmlFileSymbol;

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
    XmlFileSymbol(XmlFileSymbol),
    CsvFileSymbol(CsvFileSymbol),
}

impl Symbol {
    /// Checks if weak references of symbol are equal
    /// Attempts to upgrade both (false upon failure) and does pointer equality
    pub fn weak_ptr_eq(me: &Weak<RefCell<Symbol>>, them: &Weak<RefCell<Symbol>>) -> bool{
        me.upgrade().and_then(|me_rc| them.upgrade().map(|them_rc| Rc::ptr_eq(&me_rc, &them_rc))).unwrap_or(false)
    }
    pub fn new_root() -> Rc<RefCell<Self>> {
        let root = Rc::new(RefCell::new(Symbol::Root(RootSymbol::new())));
        root.borrow_mut().set_weak_self(Rc::downgrade(&root));
        root
    }

    //Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, _session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
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
    pub fn add_new_python_package(&mut self, _session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
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
            },
            Symbol::DiskDir(d) => {
                d.add_file(&package);
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        Some(package)
    }

    pub fn add_new_namespace(&mut self, _session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
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
            },
            Symbol::DiskDir(d) => {
                d.add_file(&namespace);
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

    pub fn add_new_compiled(&mut self, _session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
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

    pub fn add_new_variable(&mut self, _session: &mut SessionInfo, name: OYarn, range: &TextRange) -> Rc<RefCell<Self>> {
        let variable = Rc::new(RefCell::new(Symbol::Variable(VariableSymbol::new(name, range.clone(), self.is_external()))));
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

    pub fn add_new_ext_symbol(&mut self, _session: &mut SessionInfo, name: OYarn, range: &TextRange, owner: &Rc<RefCell<Symbol>>) -> Rc<RefCell<Symbol>> {
        let variable = Rc::new(RefCell::new(Symbol::Variable(VariableSymbol::new(name.clone(), range.clone(), self.is_external()))));
        variable.borrow_mut().set_weak_self(Rc::downgrade(&variable));
        variable.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let set = f.ext_symbols.entry(name.clone()).or_insert_with(|| PtrWeakHashSet::new());
                set.insert(owner.clone());
                owner.borrow_mut().add_decl_ext_symbol(&self.weak_self().unwrap().upgrade().unwrap(), &variable, name.clone(), range)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let set = m.ext_symbols.entry(name.clone()).or_insert_with(|| PtrWeakHashSet::new());
                set.insert(owner.clone());
                owner.borrow_mut().add_decl_ext_symbol(&self.weak_self().unwrap().upgrade().unwrap(), &variable, name.clone(), range)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let set = p.ext_symbols.entry(name.clone()).or_insert_with(|| PtrWeakHashSet::new());
                set.insert(owner.clone());
                owner.borrow_mut().add_decl_ext_symbol(&self.weak_self().unwrap().upgrade().unwrap(), &variable, name.clone(), range)
            },
            Symbol::Class(c) => {
                let set = c.ext_symbols.entry(name.clone()).or_insert_with(|| PtrWeakHashSet::new());
                set.insert(owner.clone());
                owner.borrow_mut().add_decl_ext_symbol(&self.weak_self().unwrap().upgrade().unwrap(), &variable, name.clone(), range)
            },
            Symbol::Function(f) => {
                let set = f.ext_symbols.entry(name.clone()).or_insert_with(|| PtrWeakHashSet::new());
                set.insert(owner.clone());
                owner.borrow_mut().add_decl_ext_symbol(&self.weak_self().unwrap().upgrade().unwrap(), &variable, name.clone(), range)
            },
            Symbol::Namespace(n) => {
                let set = n.ext_symbols.entry(name.clone()).or_insert_with(|| PtrWeakHashSet::new());
                set.insert(owner.clone());
                owner.borrow_mut().add_decl_ext_symbol(&self.weak_self().unwrap().upgrade().unwrap(), &variable, name.clone(), range)
            }
            _ => {
                panic!("Impossible to add an extern symbol to a {}", self.typ());
            }
        }
        variable
    }

    /* used by add_new_ext_symbol. Do not call directly */
    pub fn add_decl_ext_symbol(&mut self, object: &Rc<RefCell<Symbol>>, symbol: &Rc<RefCell<Symbol>>, name: OYarn, range: &TextRange) {
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                let map_for_obj = f.decl_ext_symbols.entry(object.clone()).or_insert_with(|| HashMap::new());
                let sections = map_for_obj.entry(name.clone()).or_insert_with(|| HashMap::new());
                let section_vec = sections.entry(section).or_insert_with(|| vec![]);
                section_vec.push(symbol.clone());
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                let map_for_obj = m.decl_ext_symbols.entry(object.clone()).or_insert_with(|| HashMap::new());
                let sections = map_for_obj.entry(name.clone()).or_insert_with(|| HashMap::new());
                let section_vec = sections.entry(section).or_insert_with(|| vec![]);
                section_vec.push(symbol.clone());
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                let map_for_obj = p.decl_ext_symbols.entry(object.clone()).or_insert_with(|| HashMap::new());
                let sections = map_for_obj.entry(name.clone()).or_insert_with(|| HashMap::new());
                let section_vec = sections.entry(section).or_insert_with(|| vec![]);
                section_vec.push(symbol.clone());
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                let map_for_obj = c.decl_ext_symbols.entry(object.clone()).or_insert_with(|| HashMap::new());
                let sections = map_for_obj.entry(name.clone()).or_insert_with(|| HashMap::new());
                let section_vec = sections.entry(section).or_insert_with(|| vec![]);
                section_vec.push(symbol.clone());
            },
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                let map_for_obj = f.decl_ext_symbols.entry(object.clone()).or_insert_with(|| HashMap::new());
                let sections = map_for_obj.entry(name.clone()).or_insert_with(|| HashMap::new());
                let section_vec = sections.entry(section).or_insert_with(|| vec![]);
                section_vec.push(symbol.clone());
            }
            _ => {
                panic!("Impossible to add a declaration of external symbol to a {}", self.typ());
            }
        }
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

    pub fn add_new_xml_file(&mut self, _session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let xml_sym = Rc::new(RefCell::new(Symbol::XmlFileSymbol(XmlFileSymbol::new(name.clone(), path.clone(), self.is_external()))));
        xml_sym.borrow_mut().set_weak_self(Rc::downgrade(&xml_sym));
        xml_sym.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        xml_sym.borrow_mut().set_in_workspace(self.in_workspace());
        let entry = self.get_entry().unwrap();
        entry.borrow_mut().data_symbols.insert(path.clone(), Rc::downgrade(&xml_sym));
        self.as_module_package_mut().data_symbols.insert(path.clone(), xml_sym.clone());
        xml_sym
    }

    pub fn add_new_csv_file(&mut self, _session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let csv_sym = Rc::new(RefCell::new(Symbol::CsvFileSymbol(CsvFileSymbol::new(name.clone(), path.clone(), self.is_external()))));
        csv_sym.borrow_mut().set_weak_self(Rc::downgrade(&csv_sym));
        csv_sym.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        csv_sym.borrow_mut().set_in_workspace(self.in_workspace());
        let entry = self.get_entry().unwrap();
        entry.borrow_mut().data_symbols.insert(path.clone(), Rc::downgrade(&csv_sym));
        self.as_module_package_mut().data_symbols.insert(path.clone(), csv_sym.clone());
        csv_sym
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

    pub fn as_xml_file_sym(&self) -> &XmlFileSymbol {
        match self {
            Symbol::XmlFileSymbol(x) => x,
            _ => {panic!("Not an XML file symbol")}
        }
    }

    pub fn as_xml_file_sym_mut(&mut self) -> &mut XmlFileSymbol {
        match self {
            Symbol::XmlFileSymbol(x) => x,
            _ => {panic!("Not an XML file symbol")}
        }
    }

    pub fn as_csv_file_sym(&self) -> &CsvFileSymbol {
        match self {
            Symbol::CsvFileSymbol(x) => x,
            _ => {panic!("Not a CSV file symbol")}
        }
    }

    pub fn as_csv_file_sym_mut(&mut self) -> &mut CsvFileSymbol {
        match self {
            Symbol::CsvFileSymbol(x) => x,
            _ => {panic!("Not a CSV file symbol")}
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

    pub fn as_mut_symbol_mgr(&mut self) -> &mut dyn SymbolMgr {
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
            Symbol::XmlFileSymbol(_) => SymType::XML_FILE,
            Symbol::CsvFileSymbol(_) => SymType::CSV_FILE,
        }
    }

    pub fn name(&self) -> &OYarn {
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
            Symbol::XmlFileSymbol(x) => &x.name,
            Symbol::CsvFileSymbol(c) => &c.name,
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
            Symbol::XmlFileSymbol(_) => &None,
            Symbol::CsvFileSymbol(_) => &None,
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
            Symbol::XmlFileSymbol(_) => panic!(),
            Symbol::CsvFileSymbol(_) => panic!(),
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
            Symbol::XmlFileSymbol(x) => x.is_external,
            Symbol::CsvFileSymbol(c) => c.is_external,
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
            Symbol::XmlFileSymbol(x) => x.is_external = external,
            Symbol::CsvFileSymbol(c) => c.is_external = external,
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
            Symbol::XmlFileSymbol(_) => false,
            Symbol::CsvFileSymbol(_) => false,
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
            Symbol::XmlFileSymbol(_) => panic!(),
            Symbol::CsvFileSymbol(_) => panic!(),
        }
    }

    pub fn body_range(&self) -> &TextRange {
        match self {
            Symbol::Root(_) => panic!(),
            Symbol::DiskDir(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::Package(_) => panic!(),
            Symbol::File(_) => panic!(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(c) => &c.body_range,
            Symbol::Function(f) => &f.body_range,
            Symbol::Variable(_) => panic!(),
            Symbol::XmlFileSymbol(_) => panic!(),
            Symbol::CsvFileSymbol(_) => panic!(),
        }
    }

    pub fn has_node_index(&self) -> bool {
        match self {
            Symbol::Variable(_) => false,
            Symbol::Class(_) => false,
            Symbol::Function(_) => true,
            Symbol::DiskDir(_) => false,
            Symbol::File(_) => false,
            Symbol::Compiled(_) => false,
            Symbol::Namespace(_) => false,
            Symbol::Package(_) => false,
            Symbol::Root(_) => false,
            Symbol::XmlFileSymbol(_) => false,
            Symbol::CsvFileSymbol(_) => false,
        }
    }

    pub fn node_index(&self) -> Option<&AtomicNodeIndex> {
        match self {
            Symbol::Variable(_) => None,
            Symbol::Class(_) => None,
            Symbol::Function(f) => Some(&f.node_index),
            Symbol::DiskDir(_) => None,
            Symbol::File(_) => None,
            Symbol::Compiled(_) => None,
            Symbol::Namespace(_) => None,
            Symbol::Package(_) => None,
            Symbol::Root(_) => None,
            Symbol::XmlFileSymbol(_) => None,
            Symbol::CsvFileSymbol(_) => None,
        }
    }

    pub fn node_index_mut(&mut self) -> &mut AtomicNodeIndex {
        match self {
            Symbol::Variable(_) => panic!(),
            Symbol::Class(_) => panic!(),
            Symbol::Function(f) => &mut f.node_index,
            Symbol::DiskDir(_) => panic!(),
            Symbol::File(_) => panic!(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Namespace(_) => panic!(),
            Symbol::Package(_) => panic!(),
            Symbol::Root(_) => panic!(),
            Symbol::XmlFileSymbol(_) => panic!(),
            Symbol::CsvFileSymbol(_) => panic!(),
        }
    }

    pub fn weak_self(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            Symbol::Root(r) => r.weak_self.clone(),
            Symbol::Namespace(n) => n.weak_self.clone(),
            Symbol::DiskDir(d) => d.weak_self.clone(),
            Symbol::Package(PackageSymbol::Module(m)) => m.weak_self.clone(),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self.clone(),
            Symbol::File(f) => f.weak_self.clone(),
            Symbol::Compiled(c) => c.weak_self.clone(),
            Symbol::Class(c) => c.weak_self.clone(),
            Symbol::Function(f) => f.weak_self.clone(),
            Symbol::Variable(v) => v.weak_self.clone(),
            Symbol::XmlFileSymbol(x) => x.weak_self.clone(),
            Symbol::CsvFileSymbol(c) => c.weak_self.clone(),
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
            Symbol::XmlFileSymbol(x) => x.parent.clone(),
            Symbol::CsvFileSymbol(c) => c.parent.clone(),
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
            Symbol::XmlFileSymbol(x) => x.parent = parent,
            Symbol::CsvFileSymbol(c) => c.parent = parent,
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
            Symbol::XmlFileSymbol(x) => vec![x.path.clone()],
            Symbol::CsvFileSymbol(c) => vec![c.path.clone()],
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
            Symbol::XmlFileSymbol(_) => {},
            Symbol::CsvFileSymbol(_) => {},
        }
    }

    pub fn get_symbol_first_path(&self) -> String{
        match self{
            Symbol::Package(p) => PathBuf::from(p.paths()[0].clone()).join("__init__.py").sanitize() + p.i_ext().as_str(),
            Symbol::File(f) => f.path.clone(),
            Symbol::DiskDir(_) => panic!("invalid symbol type to extract path"),
            Symbol::Root(_) => panic!("invalid symbol type to extract path"),
            Symbol::Namespace(_) => panic!("invalid symbol type to extract path"),
            Symbol::Compiled(_) => panic!("invalid symbol type to extract path"),
            Symbol::Class(_) => panic!("invalid symbol type to extract path"),
            Symbol::Function(_) => panic!("invalid symbol type to extract path"),
            Symbol::Variable(_) => panic!("invalid symbol type to extract path"),
            Symbol::XmlFileSymbol(x) => x.path.clone(),
            Symbol::CsvFileSymbol(c) => c.path.clone(),
        }
    }

    pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &n.dependencies(),
            Symbol::DiskDir(_) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependencies(),
            Symbol::File(f) => &f.dependencies(),
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
            Symbol::XmlFileSymbol(x) => &x.dependencies(),
            Symbol::CsvFileSymbol(c) => &c.dependencies(),
        }
    }
    pub fn dependencies_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => n.dependencies_mut(),
            Symbol::DiskDir(_) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependencies_as_mut(),
            Symbol::File(f) => f.dependencies_mut(),
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
            Symbol::XmlFileSymbol(x) => x.dependencies_mut(),
            Symbol::CsvFileSymbol(c) => c.dependencies_mut(),
        }
    }
    pub fn dependents(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => n.dependents(),
            Symbol::DiskDir(_) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependents(),
            Symbol::File(f) => f.dependents(),
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
            Symbol::XmlFileSymbol(x) => x.dependents(),
            Symbol::CsvFileSymbol(c) => c.dependents(),
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        match self {
            Symbol::Root(_) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => n.dependents_mut(),
            Self::DiskDir(_) => panic!("No dependencies on DiskDir"),
            Symbol::Package(p) => p.dependents_as_mut(),
            Symbol::File(f) => f.dependents_mut(),
            Symbol::Compiled(_) => panic!("No dependencies on Compiled"),
            Symbol::Class(_) => panic!("No dependencies on Class"),
            Symbol::Function(_) => panic!("No dependencies on Function"),
            Symbol::Variable(_) => panic!("No dependencies on Variable"),
            Symbol::XmlFileSymbol(x) => x.dependents_mut(),
            Symbol::CsvFileSymbol(c) => c.dependents_mut(),
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
            Symbol::XmlFileSymbol(_) => panic!("No module symbol on XmlFileSymbol"),
            Symbol::CsvFileSymbol(_) => panic!("No module symbol on CsvFileSymbol"),
        }
    }
    pub fn in_workspace(&self) -> bool {
        match self {
            Symbol::Root(_) => false,
            Symbol::Namespace(n) => n.is_in_workspace(),
            Symbol::DiskDir(d) => d.in_workspace,
            Symbol::Package(PackageSymbol::Module(m)) => m.in_workspace,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace,
            Symbol::File(f) => f.is_in_workspace(),
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(_) => panic!(),
            Symbol::Function(_) => panic!(),
            Symbol::Variable(_) => panic!(),
            Symbol::XmlFileSymbol(x) => x.is_in_workspace(),
            Symbol::CsvFileSymbol(c) => c.is_in_workspace(),
        }
    }
    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        match self {
            Symbol::Root(_) => panic!(),
            Symbol::Namespace(n) => n.set_in_workspace(in_workspace),
            Symbol::DiskDir(d) => d.in_workspace = in_workspace,
            Symbol::Package(PackageSymbol::Module(m)) => m.set_in_workspace(in_workspace),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.set_in_workspace(in_workspace),
            Symbol::File(f) => f.set_in_workspace(in_workspace),
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(_) => panic!(),
            Symbol::Function(_) => panic!(),
            Symbol::Variable(_) => panic!(),
            Symbol::XmlFileSymbol(x) => x.set_in_workspace(in_workspace),
            Symbol::CsvFileSymbol(c) => c.set_in_workspace(in_workspace),
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
                    BuildSteps::VALIDATION => m.validation_status,
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status,
                    BuildSteps::VALIDATION => p.validation_status,
                }
            }
            Symbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
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
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            Symbol::Variable(_) => todo!(),
            Symbol::XmlFileSymbol(x) => match step {
                BuildSteps::SYNTAX => panic!(),
                BuildSteps::ARCH => x.arch_status,
                BuildSteps::ARCH_EVAL => x.arch_status,
                BuildSteps::VALIDATION => x.validation_status,
            },
            Symbol::CsvFileSymbol(c) => match step {
                BuildSteps::SYNTAX => panic!(),
                BuildSteps::ARCH => c.arch_status,
                BuildSteps::ARCH_EVAL => c.arch_status,
                BuildSteps::VALIDATION => c.validation_status,
            },
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
                    BuildSteps::VALIDATION => m.validation_status = status,
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status = status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status = status,
                    BuildSteps::VALIDATION => p.validation_status = status,
                }
            }
            Symbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
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
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            Symbol::Variable(_) => todo!(),
            Symbol::XmlFileSymbol(x) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => x.arch_status = status,
                    BuildSteps::ARCH_EVAL => {},
                    BuildSteps::VALIDATION => x.validation_status = status,
                }
            },
            Symbol::CsvFileSymbol(c) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => c.arch_status = status,
                    BuildSteps::ARCH_EVAL => panic!(),
                    BuildSteps::VALIDATION => c.validation_status = status,
                }
            },
        }
    }

    pub fn iter_symbols(&self) -> std::collections::hash_map::Iter<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>> {
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
            Symbol::XmlFileSymbol(_) => panic!(),
            Symbol::CsvFileSymbol(_) => panic!(),
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
            Symbol::XmlFileSymbol(_) => None,
            Symbol::CsvFileSymbol(_) => None,
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
            Symbol::XmlFileSymbol(_) => None,
            Symbol::CsvFileSymbol(_) => None,
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
            Symbol::XmlFileSymbol(_) => { panic!() },
            Symbol::CsvFileSymbol(_) => { panic!() },
        }
    }

    pub fn not_found_paths(&self) -> &Vec<(BuildSteps, Vec<OYarn>)> {
        static EMPTY_VEC: Vec<(BuildSteps, Vec<OYarn>)> = Vec::new();
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
            Symbol::XmlFileSymbol(_) => { &EMPTY_VEC },
            Symbol::CsvFileSymbol(_) => { &EMPTY_VEC },
        }
    }

    pub fn not_found_paths_mut(&mut self) -> &mut Vec<(BuildSteps, Vec<OYarn>)> {
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
            Symbol::XmlFileSymbol(_) => { panic!("no not_found_path on XmlFileSymbol") },
            Symbol::CsvFileSymbol(_) => { panic!("no not_found_path on CsvFileSymbol") },
        }
    }

    pub fn not_found_models(&self) -> Option<&HashMap<OYarn, BuildSteps>> {
        match self {
            Symbol::File(f) => Some(&f.not_found_models),
            Symbol::XmlFileSymbol(f) => Some(&f.not_found_models),
            Symbol::Root(_) => None,
            Symbol::Namespace(_) => None,
            Symbol::DiskDir(_) => None,
            Symbol::Package(_) => None,
            Symbol::Compiled(_) => None,
            Symbol::Class(_) => None,
            Symbol::Function(_) => None,
            Symbol::Variable(_) => None,
            Symbol::CsvFileSymbol(_) => None,
        }
    }

    pub fn not_found_models_mut(&mut self) -> Option<&mut HashMap<OYarn, BuildSteps>> {
        match self {
            Symbol::File(f) => Some(&mut f.not_found_models),
            Symbol::XmlFileSymbol(f) => Some(&mut f.not_found_models),
            Symbol::Root(_) => None,
            Symbol::Namespace(_) => None,
            Symbol::DiskDir(_) => None,
            Symbol::Package(_) => None,
            Symbol::Compiled(_) => None,
            Symbol::Class(_) => None,
            Symbol::Function(_) => None,
            Symbol::Variable(_) => None,
            Symbol::CsvFileSymbol(_) => None,
        }
    }

    pub fn get_main_entry_tree(&self, session: &mut SessionInfo) -> Tree {
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

    /* Helper to merge dependencies eval_from_ast will fill when called. To be called on a file/package... */
    pub fn insert_dependencies(file: &Rc<RefCell<Symbol>>, deps: &mut Vec<Vec<Rc<RefCell<Symbol>>>>, current_step: BuildSteps) {
        for (step, dep) in deps.iter().enumerate() {
            for file_to_add in dep.iter() {
                if !Rc::ptr_eq(&file, &file_to_add) {
                    file.borrow_mut().add_dependency(&mut file_to_add.borrow_mut(), current_step, BuildSteps::from(step as i32));
                }
            }
        }
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
        let mut current = current_arc.borrow();
        while current.typ() != SymType::ROOT && current.parent().is_some() {
            if current.is_file_content() {
                res.1.insert(0, current.name().clone());
            } else {
                res.0.insert(0, current.name().clone());
            }
            let parent = current.parent().clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.borrow();
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
        let mut current = current_arc.borrow();
        while current.typ() != SymType::ROOT && current.parent().is_some() {
            if current.is_file_content() {
                res.0.1.insert(0, current.name().clone());
            } else {
                res.0.0.insert(0, current.name().clone());
            }
            let parent = current.parent().clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.borrow();
        }
        if current.typ() == SymType::ROOT {
            res.1 = current.as_root().entry_point.clone();
        }
        res
    }

    /**
     * Return the tree without the entrypoint tree.
     */
    pub fn get_local_tree(&self) -> Tree {
        let (mut tree, entry) = self.get_tree_and_entry();
        if let Some(entry) = entry {
            tree.0.drain(0..entry.borrow().tree.len());
        }
        tree
    }

    pub fn get_symbol(&self, tree: &Tree, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        let symbol_tree_files: &Vec<OYarn> = &tree.0;
        let symbol_tree_content: &Vec<OYarn> = &tree.1;
        let mut iter_sym: Vec<Rc<RefCell<Symbol>>>;
        if symbol_tree_files.len() != 0 {
            let _mod_iter_sym = self.get_module_symbol(&symbol_tree_files[0]);
            if _mod_iter_sym.is_none() {
                return vec![];
            }
            iter_sym = vec![_mod_iter_sym.unwrap()];
            if symbol_tree_files.len() > 1 {
                for fk in symbol_tree_files[1..symbol_tree_files.len()].iter() {
                    if let Some(s) = iter_sym.last().unwrap().clone().borrow().get_module_symbol(fk) {
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
                    let _iter_sym = iter_sym[0].borrow().get_sub_symbol(fk, position);
                    iter_sym = _iter_sym.symbols;
                    if iter_sym.is_empty() {
                        return vec![];
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 {
                return vec![];
            }
            iter_sym = self.get_sub_symbol(&symbol_tree_content[0], position).symbols;
            if iter_sym.is_empty() {
                return vec![];
            }
            if symbol_tree_content.len() > 1 {
                if iter_sym.len() > 1 {
                    trace!("TODO: explore all implementation possibilities");
                }
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    let _iter_sym = iter_sym[0].borrow().get_sub_symbol(fk, position);
                    iter_sym = _iter_sym.symbols;
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
    pub fn get_content_symbol(&self, name: &str, position: u32) -> ContentSymbols {
        match self {
            Symbol::Class(c) => {
                c.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::File(f) => {
                f.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::Function(f) => {
                f.get_content_symbol(oyarn!("{}", name), position)
            },
            _ => ContentSymbols::default()
        }
    }

    /// Return all symbols before the given position that are visible in the body of this symbol.
    pub fn get_all_visible_symbols(&self, name_prefix: &String, position: u32) -> HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>> {
        match self {
            Symbol::Class(c) => c.get_all_visible_symbols(name_prefix, position),
            Symbol::File(f) => f.get_all_visible_symbols(name_prefix, position),
            Symbol::Package(PackageSymbol::Module(m)) => m.get_all_visible_symbols(name_prefix, position),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.get_all_visible_symbols(name_prefix, position),
            Symbol::Function(f) => f.get_all_visible_symbols(name_prefix, position),
            _ => HashMap::new(),
        }
    }

    /**
     * Return a symbol that can be called from outside of the body of the symbol
     */
    pub fn get_sub_symbol(&self, name: &str, position: u32) -> ContentSymbols {
        match self {
            Symbol::Class(c) => {
                c.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::File(f) => {
                f.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_content_symbol(oyarn!("{}", name), position)
            },
            Symbol::Function(f) => {
                return ContentSymbols{
                    symbols: f.get_ext_symbol(&oyarn!("{}", name)),
                    always_defined: true,
                };
            },
            Symbol::Namespace(n) => {
                return ContentSymbols{
                    symbols: n.get_ext_symbol(&oyarn!("{}", name)),
                    always_defined: true,
                };
            }
            _ => {ContentSymbols::default()}
        }
    }

    pub fn get_decl_ext_symbol(&self, symbol: &Rc<RefCell<Symbol>>, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Class(c) => {
                c.get_decl_ext_symbol(symbol, name)
            },
            Symbol::File(f) => {
                f.get_decl_ext_symbol(symbol, name)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.get_decl_ext_symbol(symbol, name)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_decl_ext_symbol(symbol, name)
            },
            Symbol::Function(f) => {
                f.get_decl_ext_symbol(symbol, name)
            },
            _ => {vec![]}
        }
    }

    //Return a HashSet of all symbols (constructed until 'level') that are dependencies for the 'step' of this symbol
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>> {
        if step == BuildSteps::SYNTAX || level == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        if level > step {
            panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
        }
        match self {
            Symbol::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => n.get_dependencies(step as usize, level as usize),
            Symbol::DiskDir(_) => panic!("There is no dependencies on DiskDir Symbol"),
            Symbol::Package(PackageSymbol::Module(m)) => m.get_dependencies(step as usize, level as usize),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.get_dependencies(step as usize, level as usize),
            Symbol::File(f) => f.get_dependencies(step as usize, level as usize),
            Symbol::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
            Symbol::XmlFileSymbol(x) => x.get_dependencies(step as usize, level as usize),
            Symbol::CsvFileSymbol(c) => c.get_dependencies(step as usize, level as usize),
        }
    }

    pub fn get_all_dependencies(&self, step: BuildSteps) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        match self {
            Symbol::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => n.get_all_dependencies(step as usize),
            Symbol::DiskDir(_) => panic!("There is no dependencies on DiskDir Symbol"),
            Symbol::Package(PackageSymbol::Module(m)) => m.get_all_dependencies(step as usize),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.get_all_dependencies(step as usize),
            Symbol::File(f) => f.get_all_dependencies(step as usize),
            Symbol::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
            Symbol::XmlFileSymbol(x) => x.get_all_dependencies(step as usize),
            Symbol::CsvFileSymbol(c) => c.get_all_dependencies(step as usize),
        }
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>> {
        if level == BuildSteps::SYNTAX || step == BuildSteps::SYNTAX {
            panic!("Can't get dependents for syntax step")
        }
        if level < step {
            panic!("Can't get dependents for step {:?} and level {:?}", step, level)
        }
        match self {
            Symbol::Root(_) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => n.get_dependents(level as usize, step as usize),
            Symbol::DiskDir(_) => panic!("There is no dependencies on DiskDir Symbol"),
            Symbol::Package(PackageSymbol::Module(m)) => m.get_dependents(level as usize, step as usize),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.get_dependents(level as usize, step as usize),
            Symbol::File(f) => f.get_dependents(level as usize, step as usize),
            Symbol::Compiled(_) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(_) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(_) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(_) => panic!("There is no dependencies on Variable Symbol"),
            Symbol::XmlFileSymbol(x) => x.get_dependents(level as usize, step as usize),
            Symbol::CsvFileSymbol(c) => c.get_dependents(level as usize, step as usize),
        }
    }

    /**Add a symbol as dependency on the step of the other symbol for the build level.
    * -> The build of the 'step' of self requires the build of 'dep_level' of the other symbol to be done */
    pub fn add_dependency(&mut self, symbol: &mut Symbol, step:BuildSteps, dep_level:BuildSteps) {
        if step == BuildSteps::SYNTAX || dep_level == BuildSteps::SYNTAX {
            panic!("Can't add dependency for syntax step")
        }
        if !self.in_workspace() || !symbol.in_workspace() {
            return;
        }
        if dep_level > step {
            panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
        }
        let step_i = step as usize;
        let level_i = dep_level as usize;
        let mut set = &mut self.dependencies_mut()[step_i][level_i];
        if set.is_none() {
            self.dependencies_mut()[step_i][level_i] = Some(PtrWeakHashSet::new());
            set = &mut self.dependencies_mut()[step_i][level_i];
        }
        set.as_mut().unwrap().insert(symbol.get_rc().unwrap());
        let mut set = &mut symbol.dependents_as_mut()[level_i][step_i - level_i];
        if set.is_none() {
            symbol.dependents_as_mut()[level_i][step_i - level_i] = Some(PtrWeakHashSet::new());
            set = &mut symbol.dependents_as_mut()[level_i][step_i - level_i];
        }
        set.as_mut().unwrap().insert(self.get_rc().unwrap().clone());
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
        //signals that a change occurred to this symbol. "step" indicates which level of change occurred.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let sym_to_inv = ref_to_inv.borrow();
            if matches!(&sym_to_inv.typ(), SymType::FILE | SymType::PACKAGE(_) | SymType::XML_FILE | SymType::CSV_FILE) {
                if *step == BuildSteps::ARCH && sym_to_inv.dependents().len() > 0 {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH as usize].iter().enumerate() {
                        if let Some(hashset) = hashset {
                            for sym in hashset {
                                if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                    if index == BuildSteps::ARCH as usize {
                                        session.sync_odoo.add_to_rebuild_arch(sym.clone());
                                    } else if index == BuildSteps::ARCH_EVAL as usize {
                                        session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                    } else if index == BuildSteps::VALIDATION as usize {
                                        sym.borrow_mut().invalidate_sub_functions(session);
                                        session.sync_odoo.add_to_validations(sym.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(step) && sym_to_inv.dependents().len() > 1 {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH_EVAL as usize].iter().enumerate() {
                        if let Some(hashset) = hashset {
                            for sym in hashset {
                                if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                    if index + 1 == BuildSteps::ARCH_EVAL as usize {
                                        session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                    } else if index + 1 == BuildSteps::VALIDATION as usize {
                                        sym.borrow_mut().invalidate_sub_functions(session);
                                        session.sync_odoo.add_to_validations(sym.clone());
                                    }
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
            if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::VALIDATION].contains(step) && sym_to_inv.dependents().len() > 2 {
                for sym in sym_to_inv.dependents()[BuildSteps::VALIDATION as usize].iter().flatten().flatten() {
                    if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                        sym.borrow_mut().invalidate_sub_functions(session);
                        session.sync_odoo.add_to_validations(sym.clone());
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

    //unload a symbol and subsymbols.
    pub fn unload(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        let mut vec_to_unload: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while !vec_to_unload.is_empty() {
            let ref_to_unload = vec_to_unload.front().unwrap().clone();
            let sym_ref = ref_to_unload.borrow();
            // Unload children first
            let mut found_one = false;
            for sym in sym_ref.all_symbols() {
                found_one = true;
                vec_to_unload.push_front(sym.clone());
            }
            if found_one {
                continue;
            }
            vec_to_unload.pop_front();
            if DEBUG_MEMORY && (sym_ref.typ() == SymType::FILE || matches!(sym_ref.typ(), SymType::PACKAGE(_))) {
                info!("Unloading symbol {:?} at {:?}", sym_ref.name(), sym_ref.paths());
            }
            let module = sym_ref.find_module();
            //unload symbol
            let parent = sym_ref.parent().as_ref().unwrap().upgrade().unwrap().clone();
            let mut parent_bw = parent.borrow_mut();
            drop(sym_ref);
            parent_bw.remove_symbol(ref_to_unload.clone());
            drop(parent_bw);
            if matches!(&ref_to_unload.borrow().typ(), SymType::FILE | SymType::PACKAGE(_) | SymType::XML_FILE | SymType::CSV_FILE) {
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

    pub fn previous_step_done(&self, step: BuildSteps) -> bool {
        if step == BuildSteps::SYNTAX {
            panic!("Can't check previous step for syntax step")
        }
        for i in 0 .. step as usize {
            if self.build_status(BuildSteps::from(i as i32)) != BuildStatus::DONE {
                return false;
            }
        }
        true
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
            Symbol::Root(_) | Symbol::Namespace(_) | Symbol::DiskDir(_) | Symbol::Package(_) |
            Symbol::File(_) | Symbol::Compiled(_) | Symbol::XmlFileSymbol(_) | Symbol::CsvFileSymbol(_) => false,
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
            Symbol::XmlFileSymbol(x) => x.weak_self = Some(weak_self),
            Symbol::CsvFileSymbol(c) => c.weak_self = Some(weak_self),
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
            Symbol::XmlFileSymbol(x) => x.processed_text_hash = hash,
            Symbol::CsvFileSymbol(c) => c.processed_text_hash = hash,
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
            Symbol::XmlFileSymbol(x) => x.processed_text_hash,
            Symbol::CsvFileSymbol(c) => c.processed_text_hash,
        }
    }

    pub fn set_noqas(&mut self, noqa: NoqaInfo) {
        match self {
            Symbol::File(f) => f.noqas = noqa,
            Symbol::DiskDir(_) => panic!("set_noqas called on DiskDir"),
            Symbol::Package(PackageSymbol::Module(m)) => m.noqas = noqa,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.noqas = noqa,
            Symbol::Function(f) => f.noqas = noqa,
            Symbol::Root(_) => panic!("set_noqas called on Root"),
            Symbol::Namespace(_) => panic!("set_noqas called on Namespace"),
            Symbol::Compiled(_) => panic!("set_noqas called on Compiled"),
            Symbol::Class(c) => c.noqas = noqa,
            Symbol::Variable(_) => panic!("set_noqas called on Variable"),
            Symbol::XmlFileSymbol(x) => x.noqas = noqa,
            Symbol::CsvFileSymbol(c) => c.noqas = noqa,
        }
    }

    pub fn get_noqas(&self) -> NoqaInfo {
        match self {
            Symbol::File(f) => f.noqas.clone(),
            Symbol::Package(PackageSymbol::Module(m)) => m.noqas.clone(),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.noqas.clone(),
            Symbol::DiskDir(_) => panic!("get_noqas called on DiskDir"),
            Symbol::Function(f) => f.noqas.clone(),
            Symbol::Root(_) => panic!("get_noqas called on Root"),
            Symbol::Namespace(_) => panic!("get_noqas called on Namespace"),
            Symbol::Compiled(_) => panic!("get_noqas called on Compiled"),
            Symbol::Class(c) => c.noqas.clone(),
            Symbol::Variable(_) => panic!("get_noqas called on Variable"),
            Symbol::XmlFileSymbol(x) => x.noqas.clone(),
            Symbol::CsvFileSymbol(c) => c.noqas.clone(),
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
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow().get_in_parents(sym_types, stop_same_file);
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
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow().has_rc_in_parents(rc, stop_same_file);
        }
        false
    }

    /// get a Symbol that has the same given range and name
    pub fn get_positioned_symbol(&self, name: &OYarn, range: &TextRange) -> Option<Rc<RefCell<Symbol>>> {
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
                Symbol::XmlFileSymbol(_) => { panic!("An XML file symbol can not contain python code") }
                Symbol::CsvFileSymbol(_) => { panic!("A CSV file symbol can not contain python code") }
            };
        } else {
            match self {
                Symbol::Class(_) => { panic!("A class can not contain a file structure") },
                Symbol::File(_) => { panic!("A file can not contain a file structure"); },
                Symbol::Function(_) => { panic!("A function can not contain a file structure") },
                Symbol::DiskDir(d) => { d.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::Module(m)) => {
                    if symbol.borrow().typ() == SymType::XML_FILE || symbol.borrow().typ() == SymType::CSV_FILE {
                        m.data_symbols.remove(symbol.borrow().paths()[0].as_str());
                    } else {
                        m.module_symbols.remove(symbol.borrow().name());
                    }
                },
                Symbol::Package(PackageSymbol::PythonPackage(p)) => { p.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Compiled(c) => { c.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Namespace(n) => {
                    for directory in n.directories.iter_mut() {
                        directory.module_symbols.remove(symbol.borrow().name());
                    }
                },
                Symbol::Root(r) => { r.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Variable(_) => { panic!("A variable can not contain a file structure"); }
                Symbol::XmlFileSymbol(_) => { panic!("An XML file symbol can not contain a file structure") }
                Symbol::CsvFileSymbol(_) => { panic!("A CSV file symbol can not contain a file structure") }
            };
        }
        symbol.borrow_mut().set_parent(None);
    }

    pub fn get_file(&self) -> Option<Weak<RefCell<Symbol>>> {
        if self.typ() == SymType::FILE || matches!(self.typ(), SymType::PACKAGE(_)) || self.typ() == SymType::XML_FILE || self.typ() == SymType::CSV_FILE {
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
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow().parent_file_or_function();
        }
        None
    }

    pub fn find_module(&self) -> Option<Rc<RefCell<Symbol>>> {
        if let Symbol::Package(PackageSymbol::Module(_)) = self {return self.get_rc();}
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
    pub fn next_refs(session: &mut SessionInfo, symbol_rc: Rc<RefCell<Symbol>>, context: &mut Option<Context>, symbol_context: &Context, stop_on_type: bool, diagnostics: &mut Vec<Diagnostic>) -> VecDeque<EvaluationSymbolPtr> {
        //if current symbol is a descriptor, we have to resolve __get__ method before going further
        let mut res = VecDeque::new();
        let symbol = &*symbol_rc.borrow();
        if !stop_on_type {
            let mut base_attr = symbol_context.get(&S!("base_attr"));
            if base_attr.is_none() {
                //search in context (used in decorators to indicate on which base the field is searched)
                if let Some(context) = context.as_ref() {
                    base_attr = context.get(&S!("base_attr"));
                }
            }
            if let Some(base_attr) = base_attr {
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
                                            EvaluationSymbolPtr::WEAK(weak) => {
                                                if let Some(eval_sym_rc) = weak.weak.upgrade(){
                                                    if Rc::ptr_eq(&eval_sym_rc, &symbol_rc){
                                                        continue;
                                                    }
                                                }
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
        }
        if let Symbol::Variable(v) = symbol {
            for eval in v.evaluations.iter() {
                let ctx = &mut Some(symbol_context.clone().into_iter().chain(context.clone().unwrap_or(HashMap::new()).into_iter()).collect::<HashMap<_, _>>());
                let mut sym = eval.symbol.get_symbol(session, ctx, diagnostics, None);
                match sym {
                    EvaluationSymbolPtr::WEAK(ref mut w) => {
                        if let Some(base_attr) = symbol_context.get(&S!("base_attr")) {
                            if !w.context.get(&S!("is_attr_of_instance")).map(|x| x.as_bool()).unwrap_or(false) {
                                w.context.insert(S!("base_attr"), base_attr.clone());
                            }
                        }
                        if let Some(base_attr) = symbol_context.get(&S!("is_attr_of_instance")) {
                            if !w.context.get(&S!("is_attr_of_instance")).map(|x| x.as_bool()).unwrap_or(false) {
                                w.context.insert(S!("is_attr_of_instance"), base_attr.clone());
                            }
                        }
                    },
                    _ => {}
                }
                if !sym.is_expired_if_weak() {
                    res.push_back(sym);
                }
            }
        }
        res
    }

    /*
    Follow evaluation of current symbol until type, value or end of the chain, depending or the parameters.
    If a symbol in the chain is a descriptor, return the __get__ return evaluation.
     */
    pub fn follow_ref(evaluation: &EvaluationSymbolPtr, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, max_scope: Option<Rc<RefCell<Symbol>>>) -> Vec<EvaluationSymbolPtr> {
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
                let mut results = Symbol::next_refs(session, symbol.clone(), context, &w.context, stop_on_type, &mut vec![]);
                if results.is_empty() {
                    return vec![evaluation.clone()];
                }
                if w.instance.is_some_and(|v| v) {
                    //if the previous evaluation was set to True, we want to keep it
                    results = results.into_iter().map(|mut r| {
                        if let EvaluationSymbolPtr::WEAK(ref mut weak) = r {
                            weak.instance = Some(true);
                        }
                        r
                    }).collect();
                }
                let mut acc: PtrWeakHashSet<Weak<RefCell<Symbol>>>  = PtrWeakHashSet::new();
                let can_eval_external = !symbol.borrow().is_external();
                let mut index = 0;
                while index < results.len() {
                    let next_ref = &results[index];
                    index += 1;
                    match next_ref {
                        EvaluationSymbolPtr::WEAK(next_ref_weak) => {
                            let sym = next_ref_weak.weak.upgrade();
                            let next_ref_weak_instance = next_ref_weak.instance.clone();
                            if sym.is_none() {
                                index += 1;
                                continue;
                            }
                            let sym_rc = sym.unwrap();
                            if acc.contains(&sym_rc) {
                                index -= 1;
                                results.remove(index);
                                continue;
                            }
                            acc.insert(sym_rc.clone());
                            let sym_type = sym_rc.borrow().typ();
                            match sym_type {
                                SymType::VARIABLE => {
                                    {
                                        let sym = sym_rc.borrow();
                                        let var = sym.as_variable();
                                        if stop_on_type && matches!(next_ref_weak.is_instance(), Some(false)) && !var.is_import_variable {
                                            continue;
                                        }
                                        if stop_on_value && var.evaluations.len() == 1 && var.evaluations[0].value.is_some() {
                                            continue;
                                        }
                                        if max_scope.is_some() && !sym.has_rc_in_parents(max_scope.as_ref().unwrap().clone(), true) {
                                            continue;
                                        }
                                    }
                                    if sym_rc.borrow().as_variable().evaluations.is_empty() && sym_rc.borrow().name() != "__all__" && can_eval_external {
                                        //no evaluation? let's check that the file has been evaluated
                                        let file_symbol = sym_rc.borrow().get_file();
                                        if let Some(file_symbol) = file_symbol {
                                            if let Some(file) = file_symbol.upgrade() {
                                                SyncOdoo::build_now(session, &file, BuildSteps::ARCH_EVAL);
                                            }
                                        }
                                    }
                                    let mut next_sym_refs = Symbol::next_refs(session, sym_rc.clone(), context, &next_ref_weak.context, stop_on_type, &mut vec![]);
                                    if !next_sym_refs.is_empty() {
                                        results.pop_front();
                                        index -= 1;
                                        // /!\ we want to keep instance = True if previous evaluation was set to True!
                                        if next_ref_weak_instance.is_some_and(|v| v) {
                                            next_sym_refs = next_sym_refs.into_iter().map(|mut next_results| {
                                                if let EvaluationSymbolPtr::WEAK(weak) = &mut next_results {
                                                    weak.instance = Some(true);
                                                }
                                                next_results
                                            }).collect();
                                        }
                                        for next_results in next_sym_refs {
                                            results.push_back(next_results);
                                        }
                                    }
                                },
                                SymType::CLASS => {
                                    //On class, follow descriptor declarations
                                    let next_sym_refs = Symbol::next_refs(session, sym_rc.clone(), context, &next_ref_weak.context, stop_on_type, &mut vec![]);
                                    if !next_sym_refs.is_empty() {
                                        results.pop_front();
                                        index -= 1;
                                        for next_results in next_sym_refs {
                                            results.push_back(next_results);
                                        }
                                    }
                                },
                                _ => {}
                            }
                        },
                        _ => {}
                    }
                }
                Vec::from(results) // :'( a whole copy?
            },
            _ => {
                return vec![evaluation.clone()];
            }
        }
    }

    pub fn all_symbols(&self) -> impl Iterator<Item= Rc<RefCell<Symbol>>> + use<> {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned.
        let mut iter: Vec<Rc<RefCell<Symbol>>> = Vec::new();
        match self {
            Symbol::File(_) => {
                for symbol in self.iter_symbols().flat_map(|(_, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Class(_) => {
                for symbol in self.iter_symbols().flat_map(|(_, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Function(_) => {
                for symbol in self.iter_symbols().flat_map(|(_, hashmap)| hashmap.iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                for symbol in self.iter_symbols().flat_map(|(_, hashmap)| hashmap.iter().flat_map(|(_, vec)| vec.clone())) {
                    iter.push(symbol.clone());
                }
                for symbol in m.module_symbols.values().cloned() {
                    iter.push(symbol.clone());
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                for symbol in self.iter_symbols().flat_map(|(_, hashmap)| hashmap.iter().flat_map(|(_, vec)| vec.clone())) {
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

    //store in result all available members for symbol: sub symbols, base class elements and models symbols
    //TODO is order right of Vec in HashMap? if we take first or last in it, do we have the last effective value?
    pub fn all_members(
        symbol: &Rc<RefCell<Symbol>>,
        session: &mut SessionInfo,
        with_co_models: bool,
        only_fields: bool,
        only_methods: bool,
        from_module: Option<Rc<RefCell<Symbol>>>,
        is_super: bool) -> HashMap<OYarn, Vec<(Rc<RefCell<Symbol>>, Option<OYarn>)>>{
        let mut result: HashMap<OYarn, Vec<(Rc<RefCell<Symbol>>, Option<OYarn>)>> = HashMap::new();
        let mut acc: HashSet<Tree> = HashSet::new();
        Symbol::_all_members(symbol, session, &mut result, with_co_models, only_fields, only_methods, from_module, &mut acc, is_super);
        return  result;
    }
    fn _all_members(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo, result: &mut HashMap<OYarn, Vec<(Rc<RefCell<Symbol>>, Option<OYarn>)>>, with_co_models: bool, only_fields: bool, only_methods: bool, from_module: Option<Rc<RefCell<Symbol>>>, acc: &mut HashSet<Tree>, is_super: bool) {
        let tree = symbol.borrow().get_tree();
        if acc.contains(&tree) {
            return;
        }
        acc.insert(tree);
        let mut append_result = |symbol: Rc<RefCell<Symbol>>, dep: Option<OYarn>| {
            let name = symbol.borrow().name().clone();
            if let Some(vec) = result.get_mut(&name) {
                vec.push((symbol, dep));
            } else {
                result.insert(name.clone(), vec![(symbol, dep)]);
            }
        };
        let typ = symbol.borrow().typ();
        match typ {
            SymType::CLASS => {
                // Skip current class symbols for super
                if !is_super{
                    for symbol in symbol.borrow().all_symbols() {
                        if (only_fields && !symbol.borrow().is_field(session)) || (only_methods && symbol.borrow().typ() != SymType::FUNCTION) {
                            continue;
                        }
                        append_result(symbol, None);
                    }
                }
                let mut bases: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();
                symbol.borrow().as_class_sym().bases.iter().for_each(|base| {
                    base.upgrade().map(|b| bases.insert(b));
                });
                if with_co_models {
                    let Some(model) = symbol.borrow().as_class_sym()._model.as_ref().and_then(|model_data|
                        session.sync_odoo.models.get(&model_data.name).cloned()
                    ) else {
                        return;
                    };
                    // no recursion because it is handled in all_symbols_inherits
                    let (model_symbols, model_inherits_symbols) = model.borrow().all_symbols_inherits(session, from_module.clone());
                    for (model_sym, dependency) in model_symbols {
                        if dependency.is_some() || Rc::ptr_eq(symbol, &model_sym) {
                            continue;
                        }
                        model_sym.borrow().as_class_sym().bases.iter().for_each(|base| {
                            base.upgrade().map(|b| bases.insert(b));
                        });
                        for s in model_sym.borrow().all_symbols() {
                            if (only_fields && !s.borrow().is_field(session)) || (only_methods && s.borrow().typ() != SymType::FUNCTION) {
                                continue;
                            }
                            append_result(s, Some(model_sym.borrow().name().clone()));
                        }
                    }
                    for (model_sym, dependency) in model_inherits_symbols {
                        if dependency.is_some() || Rc::ptr_eq(symbol, &model_sym) {
                            continue;
                        }
                        model_sym.borrow().as_class_sym().bases.iter().for_each(|base| {
                            base.upgrade().map(|b| bases.insert(b));
                        });
                        // for inherits symbols, we only add fields
                        for s in model_sym.borrow().all_symbols().filter(|s| s.borrow().is_field(session)) {
                            append_result(s, Some(model_sym.borrow().name().clone()));
                        }
                    }
                }
                let bases = symbol.borrow().as_class_sym().bases.clone();
                for base in bases.iter() {
                    //no comodel as we will search for co-model from original class (what about overrided _name?)
                    //TODO what about base of co-models classes?
                    if let Some(base) = base.upgrade() {
                        Symbol::_all_members(&base, session, result, false, only_fields, only_methods, from_module.clone(), acc, false);
                    }
                }
            },
            // if not class just add it to result
            _ => symbol.borrow().all_symbols().for_each(|s|
                if !(only_fields && !s.borrow().is_field(session)) {append_result(s, None)}
            )
        }
    }



    /* return the Symbol (class, function or file) the closest to the given offset */
    pub fn get_scope_symbol(file_symbol: Rc<RefCell<Symbol>>, offset: u32, is_param: bool) -> Rc<RefCell<Symbol>> {
        let mut result = file_symbol.clone();
        let section_id = file_symbol.borrow().as_symbol_mgr().get_section_for(offset);
        for (_, sym_map) in file_symbol.borrow().iter_symbols() {
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
    pub fn get_all_inferred_names(on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: u32) -> HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>> {
        fn helper(
            on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: u32, acc: &mut HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>
        ) {
            // Add symbols from files and functions
            if matches!(on_symbol.borrow().typ(), SymType::FILE | SymType::FUNCTION) {
                let symbols_map = on_symbol.borrow().get_all_visible_symbols(name, position);
                for (sym_name, sym_vec) in symbols_map {
                    acc.entry(sym_name)
                        .or_default()
                        .extend(sym_vec);
                }
            }
            // Traverse upwards if we are under a class or a function
            if matches!(on_symbol.borrow().typ(), SymType::CLASS | SymType::FUNCTION) {
                if let Some(parent) = on_symbol.borrow().parent().as_ref().and_then(|parent_weak| parent_weak.upgrade()) {
                    helper(&parent, name, position, acc);
                }
            }
        }
        let mut results= HashMap::new();
        helper(on_symbol, name, position, &mut results);
        results
    }

    //infer a name, given a position
    pub fn infer_name(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: Option<u32>) -> ContentSymbols {
        let on_symbol = on_symbol.borrow();
        let results = on_symbol.get_content_symbol(name, position.unwrap_or(u32::MAX));
        if !results.symbols.is_empty(){
            results
        } else if !matches!(&on_symbol.typ(), SymType::FILE | SymType::PACKAGE(_) | SymType::ROOT) {
            let mut parent = on_symbol.parent().as_ref().unwrap().upgrade().unwrap();
            while parent.borrow().typ() == SymType::CLASS {
                let _parent = parent.borrow().parent().unwrap().upgrade().unwrap();
                parent = _parent;
            }
            // A function can reference another name from the full outer scope so no position is needed
            Symbol::infer_name(odoo, &parent, name, None)
        } else if on_symbol.name() != "builtins" || on_symbol.typ() != SymType::FILE {
            let builtins = odoo.get_symbol("", &(vec![Sy!("builtins")], vec![]), u32::MAX)[0].clone();
            Symbol::infer_name(odoo, &builtins, name, None)
        } else {
            ContentSymbols::default()
        }
    }

    pub fn get_sorted_symbols(&self) -> impl Iterator<Item = Rc<RefCell<Symbol>>> + use<> {
        let mut symbols: Vec<Rc<RefCell<Symbol>>> = Vec::new();
        match self {
            Symbol::Class(_) | Symbol::Function(_) | Symbol::File(_) | Symbol::Package(PackageSymbol::Module(_)) |
            Symbol::Package(PackageSymbol::PythonPackage(_)) => {
                let syms = self.iter_symbols();
                for (_, map) in syms {
                    for sym in map.values().flatten() {
                            symbols.push(sym.clone());
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
            if tree == (vec![Sy!("odoo"), Sy!("tests"), Sy!("common")], vec!()) {
                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03301, &[]) {
                    diagnostics.push(
                        Diagnostic {
                            range: Range::new(Position::new(0,0),Position::new(0,0)),
                            tags: Some(vec![DiagnosticTag::DEPRECATED]),
                            ..diagnostic_base.clone()
                        }
                    );
                }
            }
        }
    }

    pub fn is_field(&self, session: &mut SessionInfo) -> bool {
        match self.typ() {
            SymType::VARIABLE => {
                if let Some(evals) = self.evaluations().as_ref() {
                    for eval in evals.iter() {
                        let symbol = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
                        let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None);
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

    pub fn is_inheriting_from_field(&self, session: &mut SessionInfo) -> bool {
        // if not class return false
        if !matches!(self.typ(), SymType::CLASS) {
            return false;
        }
        let tree = flatten_tree(&self.get_main_entry_tree(session));
        if compare_semver(session.sync_odoo.full_version.as_str(), "18.0") <= Ordering::Equal {
            if tree.len() == 3 && tree[0] == "odoo" && tree[1] == "fields" {
                if tree[2].as_str() == "Field" {
                    return true;
                }
            }

        } else {
            if tree.len() == 4 && tree[0] == "odoo" && tree[1] == "orm" && (
                    tree[2] == "fields" && tree[3] == "Field"
            ){
                return true;
            }
        }
        // Follow class inheritance
        for base in self.as_class_sym().bases.iter().map(|weak_base| weak_base.upgrade()).flatten() {
            if base.borrow().is_inheriting_from_field(session) {
                return true;
            }
        }
        false
    }

    pub fn is_field_class(&self, session: &mut SessionInfo) -> bool {
        // if not class return false
        if !matches!(self.typ(), SymType::CLASS) {
            return false;
        }
        let mut cache = self.as_class_sym()._is_field_class.borrow_mut();
        if let Some(is_field_class) = cache.as_ref() {
            return *is_field_class;
        } else {
            let tree = &self.get_main_entry_tree(session);
            if compare_semver(session.sync_odoo.full_version.as_str(), "18.1.0") >= Ordering::Equal {
                if tree.0.len() == 3 && tree.1.len() == 1 && tree.0[0] == "odoo" && tree.0[1] == "orm" && (
                        tree.0[2] == "fields_misc" && tree.1[0] == "Boolean" ||
                        tree.0[2] == "fields_numeric" && tree.1[0] == "Integer" ||
                        tree.0[2] == "fields_numeric" && tree.1[0] == "Float" ||
                        tree.0[2] == "fields_numeric" && tree.1[0] == "Monetary" ||
                        tree.0[2] == "fields_textual" && tree.1[0] == "Char" ||
                        tree.0[2] == "fields_textual" && tree.1[0] == "Text" ||
                        tree.0[2] == "fields_textual" && tree.1[0] == "Html" ||
                        tree.0[2] == "fields_temporal" && tree.1[0] == "Date" ||
                        tree.0[2] == "fields_temporal" && tree.1[0] == "Datetime" ||
                        tree.0[2] == "fields_binary" && tree.1[0] == "Binary" ||
                        tree.0[2] == "fields_binary" && tree.1[0] == "Image" ||
                        tree.0[2] == "fields_selection" && tree.1[0] == "Selection" ||
                        tree.0[2] == "fields_reference" && tree.1[0] == "Reference" ||
                        tree.0[2] == "fields_relational" && tree.1[0] == "Many2one" ||
                        tree.0[2] == "fields_reference" && tree.1[0] == "Many2oneReference" ||
                        tree.0[2] == "fields_misc" && tree.1[0] == "Json" ||
                        tree.0[2] == "fields_properties" && tree.1[0] == "Properties" ||
                        tree.0[2] == "fields_properties" && tree.1[0] == "PropertiesDefinition" ||
                        tree.0[2] == "fields_relational" && tree.1[0] == "One2many" ||
                        tree.0[2] == "fields_relational" && tree.1[0] == "Many2many" ||
                        tree.0[2] == "fields_misc" && tree.1[0] == "Id"
                ){
                    cache.replace(true);
                    return true;
                }
            } else {
                if tree.0.len() == 2 && tree.1.len() == 1 && tree.0[0] == "odoo" && tree.0[1] == "fields" {
                    if matches!(tree.1[0].as_str(), "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
                "Binary" | "Image" | "Selection" | "Reference" | "Json" | "Properties" | "PropertiesDefinition" | "Id" | "Many2one" | "One2many" | "Many2many" | "Many2oneReference") {
                        cache.replace(true);
                        return true;
                    }
                }
            }
            if self.is_inheriting_from_field(session) {
                cache.replace(true);
                return true;
            }
        }
        cache.replace(false);
        false
    }

    pub fn is_specific_field_class(&self, session: &mut SessionInfo, field_names: &[&str]) -> bool {
        let tree = flatten_tree(&self.get_main_entry_tree(session));
        return self.is_field_class(session) && field_names.iter().any(|&name| {
            tree.last().unwrap() == name
        })
    }

    pub fn is_specific_field(&self, session: &mut SessionInfo, field_names: &[&str]) -> bool {
        match self.typ() {
            SymType::VARIABLE => {
                if let Some(evals) = self.evaluations().as_ref() {
                    for eval in evals.iter() {
                        let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
                        let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None);
                        for eval_weak in eval_weaks.iter() {
                            if let Some(symbol) = eval_weak.upgrade_weak() {
                                if symbol.borrow().is_specific_field_class(session, field_names){
                                    return true;
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

    pub fn all_fields(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>) -> HashMap<OYarn, Vec<(Rc<RefCell<Symbol>>, Option<OYarn>)>> {
        Symbol::all_members(symbol, session, true, true, false, from_module.clone(), false)
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
            let mut content_syms = self.get_sub_symbol(name, u32::MAX).symbols;
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
                    let model_symbols = Model::get_full_model_symbols(model.clone(), session, from_module.clone());
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
                        //only fields are visible on inherits, not methods
                        let model_symbols = Model::get_full_model_symbols(model_inherits_symbol, session, from_module.clone());
                        for model_symbol in model_symbols {
                            if self.is_equal(&model_symbol) || visited_classes.contains(&model_symbol){
                                continue;
                            }
                            visited_classes.insert(model_symbol.clone());
                            let (attributs, att_diagnostic) = model_symbol.borrow()._get_member_symbol_helper(session, name, None, true, true, all, false, visited_classes);
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
                Symbol::XmlFileSymbol(_) => {},
                Symbol::CsvFileSymbol(_) => {},
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
                Symbol::DiskDir(_) => {},
                Symbol::Root(_) => {},
                Symbol::Namespace(_) => {},
                Symbol::Package(_) => {},
                Symbol::Compiled(_) => {},
                Symbol::Variable(_) => {},
                Symbol::XmlFileSymbol(_) => {},
                Symbol::CsvFileSymbol(_) => {},
            }
        }

        iter_recursive(self, &mut res);

        res
    }

    pub fn get_xml_id(&self, xml_id: &OYarn) -> Option<Vec<OdooData>> {
        match self {
            Symbol::XmlFileSymbol(xml_file) => xml_file.xml_ids.get(xml_id).cloned(),
            Symbol::Package(PackageSymbol::Module(module)) => module.xml_ids.get(xml_id).cloned(),
            Symbol::Package(PackageSymbol::PythonPackage(package)) => package.xml_ids.get(xml_id).cloned(),
            Symbol::File(file) => file.xml_ids.get(xml_id).cloned(),
            Symbol::CsvFileSymbol(file) => file.xml_ids.get(xml_id).cloned(),
            _ => None,
        }
    }

    pub fn insert_xml_id(&mut self, xml_id: OYarn, xml_data: OdooData) {
        match self {
            Symbol::File(file) => {
                file.xml_ids.entry(xml_id).or_insert(vec![]).push(xml_data);
            },
            Symbol::Package(PackageSymbol::Module(module)) => {
                module.xml_ids.entry(xml_id).or_insert(vec![]).push(xml_data);
            },
            Symbol::Package(PackageSymbol::PythonPackage(package)) => {
                package.xml_ids.entry(xml_id).or_insert(vec![]).push(xml_data);
            },
            _ => {}
        }
    }

    pub fn print_dependencies(&self) {
        /*println!("------- Output dependencies of {} -------", self.name());
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
        }*/
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
