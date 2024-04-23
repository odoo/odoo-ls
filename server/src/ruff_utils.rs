use ruff_text_size::TextRange;
use ruff_python_ast::Expr;

pub fn get_expr_range(expr: &Expr) -> &TextRange {
    match expr {
        Expr::BoolOp(e) => &e.range,
        Expr::Named(e) => &e.range,
        Expr::BinOp(e) => &e.range,
        Expr::UnaryOp(e) => &e.range,
        Expr::Lambda(e) => &e.range,
        Expr::If(e) => &e.range,
        Expr::Dict(e) => &e.range,
        Expr::Set(e) => &e.range,
        Expr::ListComp(e) => &e.range,
        Expr::SetComp(e) => &e.range,
        Expr::DictComp(e) => &e.range,
        Expr::Generator(e) => &e.range,
        Expr::Await(e) => &e.range,
        Expr::Yield(e) => &e.range,
        Expr::YieldFrom(e) => &e.range,
        Expr::Compare(e) => &e.range,
        Expr::Call(e) => &e.range,
        Expr::FString(e) => &e.range,
        Expr::StringLiteral(e) => &e.range,
        Expr::BytesLiteral(e) => &e.range,
        Expr::NumberLiteral(e) => &e.range,
        Expr::BooleanLiteral(e) => &e.range,
        Expr::NoneLiteral(e) => &e.range,
        Expr::EllipsisLiteral(e) => &e.range,
        Expr::Attribute(e) => &e.range,
        Expr::Subscript(e) => &e.range,
        Expr::Starred(e) => &e.range,
        Expr::Name(e) => &e.range,
        Expr::List(e) => &e.range,
        Expr::Tuple(e) => &e.range,
        Expr::Slice(e) => &e.range,
        Expr::IpyEscapeCommand(e) => &e.range,
    }
}