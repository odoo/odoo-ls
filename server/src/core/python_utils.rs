use ruff_python_ast::{Expr, ExprAttribute, ExprName};

#[derive(Debug, Clone)]
pub enum AssignTargetType {
    Name(ExprName),
    Attribute(ExprAttribute),
}

#[derive(Debug, Clone)]
pub struct Assign {
    pub target: AssignTargetType,
    pub value: Option<Expr>,
    pub annotation: Option<Expr>,
    pub index: Option<usize>, //If index is set, it means that value is not unpackable, and that the target should be associated to the 'index' element of value
}

fn _link_tuples(targets: &[Expr], values: &[Expr]) -> Vec<Assign> {
    let mut res: Vec<Assign> = Vec::new();
    if targets.len() != values.len() {
        return res;
    }
    for (index, target) in targets.iter().enumerate() {
        match target {
            Expr::Attribute(_) => {},
            Expr::Subscript(_) => {},
            Expr::Name(expr) => {
                res.push(Assign {
                    target: AssignTargetType::Name(expr.clone()),
                    annotation: None,
                    value: Some(values.get(index).unwrap().clone()),
                    index: None,
                });
            }
            Expr::Tuple(expr) => {
                let value = values.get(index).unwrap();
                match value {
                    Expr::Tuple(t) => {
                        let mut inner_unpack = _link_tuples(&expr.elts, &t.elts);
                        res.append(&mut inner_unpack);
                    },
                    Expr::List(l) => {
                        let mut inner_unpack = _link_tuples(&expr.elts, &l.elts);
                        res.append(&mut inner_unpack);
                    },
                    _ => {
                        for (index, target) in expr.elts.iter().enumerate() {
                            match target {
                                Expr::Name(tar) => {
                                    res.push(Assign {
                                        target: AssignTargetType::Name(tar.clone()),
                                        annotation: None,
                                        value: Some(value.clone()),
                                        index: Some(index),
                                    });
                                }
                                _ => {continue;}
                            }
                        }
                    }
                }
            },
            Expr::List(expr) => {
                let value = values.get(index).unwrap();
                match value {
                    Expr::Tuple(t) => {
                        let mut inner_unpack = _link_tuples(&expr.elts, &t.elts);
                        res.append(&mut inner_unpack);
                    },
                    Expr::List(l) => {
                        let mut inner_unpack = _link_tuples(&expr.elts, &l.elts);
                        res.append(&mut inner_unpack);
                    },
                    _ => {
                        for (index, target) in expr.elts.iter().enumerate() {
                            match target {
                                Expr::Name(tar) => {
                                    res.push(Assign {
                                        target: AssignTargetType::Name(tar.clone()),
                                        annotation: None,
                                        value: Some(value.clone()),
                                        index: Some(index),
                                    });
                                }
                                _ => {continue;}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    res
}

/*
Given an Expr, generate Assigns for each variable in the target, but as if targets are searching for an iterable value (as in a for-loop).
The assignations stop at 20 assigns. If more values are found, last assign will be a "unknown" value (None) and stop at 21
Ex: for a in [1, 2, 3], return [[("a", None, 1), ("a", None, 2), ("a", None, 3)]]
for a, b in [(1, 2), (3, 4)], return [[("a", None, 1), ("b", None, 2)], [("a", None, 3), ("b", None, 4)]]
*/
pub fn unpack_iter_assign(targets: &[Expr], annotation: Option<&Expr>, value: Option<&Expr>) -> Vec<Vec<Assign>> {
    const MAX_ASSIGN: usize = 20;
    let Some(value) = value else { return vec![unpack_assign(targets, annotation, None)]};
    let elts = match value {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::Set(set) => &set.elts,
        _ => { return vec![unpack_assign(targets, annotation, None)] }
    };
    let mut res = Vec::new();
    for elt in elts.iter().take(MAX_ASSIGN) {
        let unpacked = unpack_assign(targets, annotation, Some(elt));
        res.push(unpacked);
    }
    if elts.len() > MAX_ASSIGN {
        let unpacked = unpack_assign(targets, annotation, None);
        res.push(unpacked);
    }
    res
}

pub fn unpack_assign(targets: &[Expr], annotation: Option<&Expr>, value: Option<&Expr>) -> Vec<Assign> {
    //Given the target, the annotation and the values, return a list of tuples (variable: ExprName, annotation, value)
    //for each variable, associating annotation and value for the right variable
    // Ex: for "a = b = 1", return [("a", None, 1), ("b", , None, 1)]
    // Ex: for "a: int = b: int = 1", return [("a", "int", 1), ("b", "int", 1)]
    // Ex: for "a, b = 1, 2", return [("a", None, 1), ("b", None, 2)]
    // Ex: for "a: int", return [("a", "int", None)]
    // Ex: for "(a, (b, c)) = (1, (2, 3))", return [("a", None, 1), ("b", None, 2), ("c", None, 3)]
    // Ex: for "a, b = b, a = 1, 2" return [("a", None, 1), ("b", None, 2), ("a", None, 2), ("b", None, 1)]
    // Ex: for "a, *b, c, d = 1, 2, 3, 4, 5" return [("a", None, 1), ("b", None, (2, 3)), ("c", None, 4), ("d", None, 5)] //TODO
    let mut res: Vec<Assign> = Vec::new();

    for target in targets {
        match target {
            Expr::Attribute(expr) => {
                match value {
                    Some(value) => {
                        res.push(Assign {
                            target: AssignTargetType::Attribute(expr.clone()),
                            annotation: annotation.cloned(),
                            value: Some(value.clone()),
                            index: None,
                        });
                    },
                    None => {
                        res.push(Assign {
                            target: AssignTargetType::Attribute(expr.clone()),
                            annotation: annotation.cloned(),
                            value: None,
                            index: None,
                        });
                    }
                }
            },
            Expr::Subscript(_) => {},
            Expr::Name(expr) => {
                match value {
                    Some(value) => {
                        res.push(Assign {
                            target: AssignTargetType::Name(expr.clone()),
                            annotation: annotation.cloned(),
                            value: Some(value.clone()),
                            index: None,
                        });
                    },
                    None => {
                        res.push(Assign {
                            target: AssignTargetType::Name(expr.clone()),
                            annotation: annotation.cloned(),
                            value: None,
                            index: None,
                        });
                    }
                }
            }
            Expr::Tuple(expr) => {
                // if we have a tuple, we want to untuple the value if possible. If not or because we don't know
                // the type of the value, we return the value with an index
                if let Some(value) = value {
                    match value {
                        Expr::Tuple(t) => {
                            res.append(&mut _link_tuples(&expr.elts, &t.elts));
                            return res;
                        },
                        Expr::List(l) => {
                            res.append(&mut _link_tuples(&expr.elts, &l.elts));
                            return res;
                        },
                        Expr::Set(s) => {
                            res.append(&mut _link_tuples(&expr.elts, &s.elts));
                            return res;
                        },
                        _ => {}
                    }
                }
                for (index, target) in expr.elts.iter().enumerate() {
                    match target {
                        Expr::Name(tar) => {
                            res.push(Assign {
                                target: AssignTargetType::Name(tar.clone()),
                                annotation: None,
                                value: value.cloned(),
                                index: Some(index),
                            });
                        }
                        _ => {continue;}
                    }
                }
            }
            Expr::List(expr) => {
                // Same code than for Tuple
                if let Some(value) = value {
                    match value {
                        Expr::Tuple(t) => {
                            res.append(&mut _link_tuples(&expr.elts, &t.elts));
                            return res;
                        },
                        Expr::List(l) => {
                            res.append(&mut _link_tuples(&expr.elts, &l.elts));
                            return res;
                        },
                        Expr::Set(s) => {
                            res.append(&mut _link_tuples(&expr.elts, &s.elts));
                            return res;
                        },
                        _ => {}
                    }
                }
                for (index, target) in expr.elts.iter().enumerate() {
                    match target {
                        Expr::Name(tar) => {
                            res.push(Assign {
                                target: AssignTargetType::Name(tar.clone()),
                                annotation: None,
                                value: value.cloned(),
                                index: Some(index),
                            });
                        }
                        _ => {continue;}
                    }
                }
            },
            _ => {}
        }
    }

    res
}
