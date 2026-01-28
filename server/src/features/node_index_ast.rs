/// From https://github.com/astral-sh/ruff/blob/c433865801e2ee06902199d6b0a223cfca52269b/crates/ruff_db/src/parsed.rs#L160
/// License MIT: https://github.com/astral-sh/ruff?tab=MIT-1-ov-file#readme

use std::sync::Arc;

use ruff_python_ast::visitor::source_order::*;
use ruff_python_ast::*;
use ruff_python_parser::Parsed;

/// A visitor that collects nodes in source order.
pub struct Visitor<'a> {
    pub index: u32,
    pub nodes: Vec<AnyRootNodeRef<'a>>,
}

impl<'a> Visitor<'a> {
    fn visit_node<T>(&mut self, node: &'a T)
    where
        T: HasNodeIndex + std::fmt::Debug,
        AnyRootNodeRef<'a>: From<&'a T>,
    {
        node.node_index().set(NodeIndex::from(self.index));
        self.nodes.push(AnyRootNodeRef::from(node));
        self.index += 1;
    }
}

impl<'a> SourceOrderVisitor<'a> for Visitor<'a> {
    #[inline]
    fn visit_mod(&mut self, module: &'a Mod) {
        self.visit_node(module);
        walk_module(self, module);
    }

    #[inline]
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        self.visit_node(stmt);
        walk_stmt(self, stmt);
    }

    #[inline]
    fn visit_annotation(&mut self, expr: &'a Expr) {
        self.visit_node(expr);
        walk_annotation(self, expr);
    }

    #[inline]
    fn visit_expr(&mut self, expr: &'a Expr) {
        self.visit_node(expr);
        walk_expr(self, expr);
    }

    #[inline]
    fn visit_decorator(&mut self, decorator: &'a Decorator) {
        self.visit_node(decorator);
        walk_decorator(self, decorator);
    }

    #[inline]
    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        self.visit_node(comprehension);
        walk_comprehension(self, comprehension);
    }

    #[inline]
    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        self.visit_node(except_handler);
        walk_except_handler(self, except_handler);
    }

    #[inline]
    fn visit_arguments(&mut self, arguments: &'a Arguments) {
        self.visit_node(arguments);
        walk_arguments(self, arguments);
    }

    #[inline]
    fn visit_parameters(&mut self, parameters: &'a Parameters) {
        self.visit_node(parameters);
        walk_parameters(self, parameters);
    }

    #[inline]
    fn visit_parameter(&mut self, arg: &'a Parameter) {
        self.visit_node(arg);
        walk_parameter(self, arg);
    }

    fn visit_parameter_with_default(
        &mut self,
        parameter_with_default: &'a ParameterWithDefault,
    ) {
        self.visit_node(parameter_with_default);
        walk_parameter_with_default(self, parameter_with_default);
    }

    #[inline]
    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        self.visit_node(keyword);
        walk_keyword(self, keyword);
    }

    #[inline]
    fn visit_alias(&mut self, alias: &'a Alias) {
        self.visit_node(alias);
        walk_alias(self, alias);
    }

    #[inline]
    fn visit_with_item(&mut self, with_item: &'a WithItem) {
        self.visit_node(with_item);
        walk_with_item(self, with_item);
    }

    #[inline]
    fn visit_type_params(&mut self, type_params: &'a TypeParams) {
        self.visit_node(type_params);
        walk_type_params(self, type_params);
    }

    #[inline]
    fn visit_type_param(&mut self, type_param: &'a TypeParam) {
        self.visit_node(type_param);
        walk_type_param(self, type_param);
    }

    #[inline]
    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        self.visit_node(match_case);
        walk_match_case(self, match_case);
    }

    #[inline]
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        self.visit_node(pattern);
        walk_pattern(self, pattern);
    }

    #[inline]
    fn visit_pattern_arguments(&mut self, pattern_arguments: &'a PatternArguments) {
        self.visit_node(pattern_arguments);
        walk_pattern_arguments(self, pattern_arguments);
    }

    #[inline]
    fn visit_pattern_keyword(&mut self, pattern_keyword: &'a PatternKeyword) {
        self.visit_node(pattern_keyword);
        walk_pattern_keyword(self, pattern_keyword);
    }

    #[inline]
    fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause) {
        self.visit_node(elif_else_clause);
        walk_elif_else_clause(self, elif_else_clause);
    }

    #[inline]
    fn visit_f_string(&mut self, f_string: &'a FString) {
        self.visit_node(f_string);
        walk_f_string(self, f_string);
    }

    #[inline]
    fn visit_interpolated_string_element(
        &mut self,
        interpolated_string_element: &'a InterpolatedStringElement,
    ) {
        self.visit_node(interpolated_string_element);
        walk_interpolated_string_element(self, interpolated_string_element);
    }

    #[inline]
    fn visit_t_string(&mut self, t_string: &'a TString) {
        self.visit_node(t_string);
        walk_t_string(self, t_string);
    }

    #[inline]
    fn visit_string_literal(&mut self, string_literal: &'a StringLiteral) {
        self.visit_node(string_literal);
        walk_string_literal(self, string_literal);
    }

    #[inline]
    fn visit_bytes_literal(&mut self, bytes_literal: &'a BytesLiteral) {
        self.visit_node(bytes_literal);
        walk_bytes_literal(self, bytes_literal);
    }

    #[inline]
    fn visit_identifier(&mut self, identifier: &'a Identifier) {
        self.visit_node(identifier);
        walk_identifier(self, identifier);
    }
}

/// A wrapper around the AST that allows access to AST nodes by index.
#[derive(Debug)]
pub struct IndexedModule {
    index: Box<[AnyRootNodeRef<'static>]>,
    pub parsed: Parsed<ModModule>,
}

impl IndexedModule {
    /// Create a new [`IndexedModule`] from the given AST.
    #[allow(clippy::unnecessary_cast)]
    pub fn new(parsed: Parsed<ModModule>) -> Arc<Self> {
        let mut visitor = Visitor {
            nodes: Vec::new(),
            index: 0,
        };

        let mut inner = Arc::new(IndexedModule {
            parsed,
            index: Box::new([]),
        });

        AnyNodeRef::from(inner.parsed.syntax()).visit_source_order(&mut visitor);

        let index: Box<[AnyRootNodeRef<'_>]> = visitor.nodes.into_boxed_slice();

        // SAFETY: We cast from `Box<[AnyRootNodeRef<'_>]>` to `Box<[AnyRootNodeRef<'static>]>`,
        // faking the 'static lifetime to create the self-referential struct. The node references
        // are into the `Arc<Parsed<ModModule>>`, so are valid for as long as the `IndexedModule`
        // is alive. We make sure to restore the correct lifetime in `get_by_index`.
        //
        // Note that we can never move the data within the `Arc` after this point.
        Arc::get_mut(&mut inner).unwrap().index =
            unsafe { Box::from_raw(Box::into_raw(index) as *mut [AnyRootNodeRef<'static>]) };

        inner
    }

    /// Returns the node at the given index.
    pub fn get_by_index<'ast>(&'ast self, index: NodeIndex) -> AnyRootNodeRef<'ast> {
        let index = index
            .as_u32()
            .expect("attempted to access uninitialized `NodeIndex`");

        // Note that this method restores the correct lifetime: the nodes are valid for as
        // long as the reference to `IndexedModule` is alive.
        self.index[index as usize]
    }
}
