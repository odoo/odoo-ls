use rustpython_parser::ast::Expr;

pub fn unpack_assign<R>(targets: Vec<&Expr<R>>, values: Option<Vec<&Expr<R>>>) -> Vec<(String, String)> {
    vec![(String::new(), String::new())]
}