use ruff_python_ast::{Expr, ExprName};

#[derive(Debug, Clone)]
pub struct Assign {
    pub target: ExprName,
    pub value: Option<Expr>,
    pub annotation: Option<Expr>,
    pub index: Option<usize>,
}

fn _link_tuples(targets: Vec<Expr>, values: Vec<Expr>) -> Vec<Assign> {
    let mut res: Vec<Assign> = Vec::new();
    if targets.len() != values.len() {
        println!("Invalid stmt: can't unpack a tuple with a different number of elements");
        return res;
    }
    for (index, target) in targets.iter().enumerate() {
        match target {
            Expr::Attribute(expr) => {},
            Expr::Subscript(expr) => {},
            Expr::Name(expr) => {
                res.push(Assign {
                    target: expr.clone(),
                    annotation: None,
                    value: Some(values.get(index).unwrap().clone()),
                    index: None,
                });
            }
            Expr::Tuple(expr) => {
                let value = values.get(index).unwrap();
                if value.is_tuple_expr() {
                    let mut inner_unpack = _link_tuples(expr.elts.clone(), value.clone().tuple_expr().unwrap().elts.clone());
                    res.append(&mut inner_unpack);
                } else if value.is_list_expr() {
                    let mut inner_unpack = _link_tuples(expr.elts.clone(), value.clone().list_expr().unwrap().elts.clone());
                    res.append(&mut inner_unpack);
                } else {
                    for (index, target) in expr.elts.iter().enumerate() {
                        match target {
                            Expr::Name(tar) => {
                                res.push(Assign {
                                    target: tar.clone(),
                                    annotation: None,
                                    value: Some(value.clone()),
                                    index: Some(index),
                                });
                            }
                            _ => {continue;}
                        }
                    }
                }
            },
            Expr::List(expr) => {
                let value = values.get(index).unwrap();
                if value.is_tuple_expr() {
                    let mut inner_unpack = _link_tuples(expr.elts.clone(), value.clone().tuple_expr().unwrap().elts.clone());
                    res.append(&mut inner_unpack);
                } else if value.is_list_expr() {
                    let mut inner_unpack = _link_tuples(expr.elts.clone(), value.clone().list_expr().unwrap().elts.clone());
                    res.append(&mut inner_unpack);
                } else {
                    for (index, target) in expr.elts.iter().enumerate() {
                        match target {
                            Expr::Name(tar) => {
                                res.push(Assign {
                                    target: tar.clone(),
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
            _ => {}
        }
    }
    res
}

pub fn unpack_assign(targets: &Vec<Expr>, annotation: Option<&Box<Expr>>, value: Option<&Box<Expr>>) -> Vec<Assign> {
    //Given the target, the annotation and the values, return a list of tuples (variable: ExprName, annotation, value)
    //for each variable, associating annotation and value for the right variable
    // Ex: for "a = b = 1", return [("a", None, 1), ("b", , None, 1)]
    // Ex: for "a: int = b: int = 1", return [("a", "int", 1), ("b", "int", 1)]
    // Ex: for "a, b = 1, 2", return [("a", None, 1), ("b", None, 2)]
    // Ex: for "a: int", return [("a", "int", None)]
    // Ex: for "(a, (b, c)) = (1, (2, 3))", return [("a", None, 1), ("b", None, 2), ("c", None, 3)]
    // Ex: for "a, b = b, a = 1, 2" return [("a", None, 1), ("b", None, 2), ("a", None, 2), ("b", None, 1)]
    let mut res: Vec<Assign> = Vec::new();

    for target in targets.iter() {
        match target {
            Expr::Attribute(expr) => {},
            Expr::Subscript(expr) => {},
            Expr::Name(expr) => {
                match value {
                    Some(value) => {
                        res.push(Assign {
                            target: expr.clone(),
                            annotation: match annotation {
                                Some(annotation) => Some(*annotation.clone()),
                                None => None,
                            },
                            value: Some(*value.clone()),
                            index: None,
                        });
                    },
                    None => {
                        res.push(Assign {
                            target: expr.clone(),
                            annotation: match annotation {
                                Some(annotation) => Some(*annotation.clone()),
                                None => None,
                            },
                            value: None,
                            index: None,
                        });
                    }
                }
            }
            Expr::Tuple(expr) => {
                // if we have a tuple, we want to untuple the value if possible. If not or because we don't know
                // the type of the value, we return the value with an index
                if value.is_none() {
                    println!("Invalid stmt: can't annotate a tuple");
                    continue;
                }
                let value = value.unwrap();
                if value.is_tuple_expr() {
                    res.append(&mut _link_tuples(expr.elts.clone(), value.clone().tuple_expr().unwrap().elts.clone()));
                } else if value.is_list_expr() {
                    res.append(&mut _link_tuples(expr.elts.clone(), value.clone().list_expr().unwrap().elts.clone()));
                } else {
                    for (index, target) in expr.elts.iter().enumerate() {
                        match target {
                            Expr::Name(tar) => {
                                res.push(Assign {
                                    target: tar.clone(),
                                    annotation: None,
                                    value: Some(*value.clone()),
                                    index: Some(index),
                                });
                            }
                            _ => {continue;}
                        }
                    }
                }
            }
            Expr::List(expr) => {
                // Same code than for Tuple
                if value.is_none() {
                    println!("Invalid stmt: can't annotate a List");
                    continue;
                }
                let value = value.unwrap();
                if value.is_tuple_expr() {
                    res.append(&mut _link_tuples(expr.elts.clone(), value.clone().tuple_expr().unwrap().elts.clone()));
                } else if value.is_list_expr() {
                    res.append(&mut _link_tuples(expr.elts.clone(), value.clone().list_expr().unwrap().elts.clone()));
                } else {
                    for (index, target) in expr.elts.iter().enumerate() {
                        match target {
                            Expr::Name(tar) => {
                                res.push(Assign {
                                    target: tar.clone(),
                                    annotation: None,
                                    value: Some(*value.clone()),
                                    index: Some(index),
                                });
                            }
                            _ => {continue;}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    res
}