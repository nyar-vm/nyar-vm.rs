//! 分层 parser tree 文本快照（CST 壳 + AST / markup 子树展开）。

use std::ops::Range;

use crate::transitional::cst::valkyrie::{
    ClassDeclaration, ClassLikeKind, DeclarationBody, FlagsDeclaration, FunctionDeclKind, FunctionDeclaration, FunctionParameter,
    FunctionStatement, ImplyDeclaration, LetStatement, LiteralExpression, ObjectBody, ObjectFieldDeclaration, ObjectMethodDeclaration,
    PatternExpression, RootStatement, SumTypeKind, TermExpression, TraitDeclaration, TypeExpression, UniteDeclaration, ValCstElement,
    ValCstParser, ValCstRoot,
    ast::{
        ArmStatement, ArrayPattern, ExtractPattern, IfLetStatement, IfStatement, LoopInStatement, LoopStatement, MatchObjectField,
        ObjectPattern, PatternOrExpression, SubscriptItem, TermAsExpression, TermBinaryExpression, TermCallArgument, TermCallExpression,
        TermDotExpression, TermIsExpression, TermSubscriptExpression, TermUnaryExpression, TryStatement, TuplePattern, TypeAliasDeclaration,
        UntilNotStatement, UntilStatement, WhileLetStatement, WhileStatement,
    },
    tgrammar::{TgIf, TgIfArm, TgLoop, TgMatch, TgMatchArm, TgNode, TgRoot, TgTextPart},
    xml::{XgAttrValue, XgElement, XgNode, XgRoot, XgTextPart},
};

/// 将 `.v` 源码格式化为分层 `*.parse` 树形文本快照。
pub fn dump_parse_tree(source: &str) -> String {
    let root = ValCstParser::parse(source).expect("parse cst");
    dump_cst_root(&root)
}

/// 将 `.vx` 源码格式化为分层 `*.parse` 树形文本快照（含 widget markup fixup）。
pub fn dump_parse_vx_tree(source: &str) -> String {
    let root = ValCstParser::parse_vx(source).expect("parse vx cst");
    dump_cst_root(&root)
}

fn dump_cst_root(root: &ValCstRoot) -> String {
    let mut dumper = Dumper::new();
    dumper.line(0, &format!("Root span={}", span_str(&root.span)));
    for element in &root.elements {
        dump_cst_element(&mut dumper, element, 1);
    }
    dumper.finish()
}

fn dump_cst_element(dumper: &mut Dumper, element: &ValCstElement, indent: usize) {
    match element {
        ValCstElement::Trivia { text, span } => {
            dumper.line(indent, &format!("Trivia span={} text={}", span_str(span), escape_text(text)));
        }
        ValCstElement::Statement { leading, ast, trailing } => {
            let stmt_span = ast.span();
            dumper.line(indent, &format!("Statement span={} kind={}", span_str(stmt_span), root_statement_kind(ast)));
            dumper.line(
                indent + 1,
                &format!(
                    "leading_trivia span={}..{} text={}",
                    stmt_span.start.saturating_sub(leading.len()),
                    stmt_span.start,
                    escape_text(leading)
                ),
            );
            dump_root_statement(dumper, ast, indent + 1);
            dumper.line(
                indent + 1,
                &format!("trailing_trivia span={}..{} text={}", stmt_span.end, stmt_span.end + trailing.len(), escape_text(trailing)),
            );
        }
        ValCstElement::Error { message, text, span } => {
            dumper.line(indent, &format!("Error span={} message={} text={}", span_str(span), escape_text(message), escape_text(text)));
        }
    }
}

fn dump_root_statement(dumper: &mut Dumper, statement: &RootStatement, indent: usize) {
    match statement {
        RootStatement::Namespace(decl) => {
            let name = decl.name.parts.join("::");
            dumper.named(indent, "NamespaceDeclaration", &decl.span, &name);
            if let Some(body) = &decl.body {
                dump_declaration_body(dumper, body, indent + 1);
            }
        }
        RootStatement::Using(decl) => {
            let path = decl.path.parts.join("::");
            dumper.named(indent, "UsingStatement", &decl.span, &path);
        }
        RootStatement::Function(decl) => dump_function_declaration(dumper, decl, indent),
        RootStatement::Class(decl) => dump_class_declaration(dumper, decl, indent),
        RootStatement::Trait(decl) => dump_trait_declaration(dumper, decl, indent),
        RootStatement::Imply(decl) => dump_imply_declaration(dumper, decl, indent),
        RootStatement::Unite(decl) => dump_unite_declaration(dumper, decl, indent),
        RootStatement::Attribute(decl) => dumper.named(indent, "AttributeDeclaration", &decl.span, decl.name.as_str()),
        RootStatement::TypeAlias(decl) => {
            dumper.named(indent, "TypeAliasDeclaration", &decl.span, decl.name.as_str());
            for parameter in &decl.generic_parameters {
                dumper.named(indent + 1, "GenericParameter", &parameter.span, parameter.name.as_str());
            }
            dump_type_expression(dumper, &decl.target, indent + 1);
        }
        RootStatement::Flags(decl) => dump_flags_declaration(dumper, decl, indent),
        RootStatement::MacroAssign(decl) => {
            dumper.named(indent, "MacroAssignDeclaration", &decl.span, decl.name.as_str());
            dump_term_expression(dumper, &decl.value, indent + 1);
        }
        RootStatement::Tests(decl) => {
            dumper.node(indent, "TestsDeclaration", &decl.span);
            dump_declaration_body(dumper, &decl.body, indent + 1);
        }
    }
}

fn dump_function_declaration(dumper: &mut Dumper, decl: &FunctionDeclaration, indent: usize) {
    dumper.line(
        indent,
        &format!(
            "FunctionDeclaration span={} name={} kind={}",
            span_str(&decl.span),
            escape_text(decl.name.as_str()),
            function_decl_kind(decl.kind)
        ),
    );
    for param in &decl.params {
        dump_function_parameter(dumper, param, indent + 1);
    }
    if let Some(return_type) = &decl.return_type {
        dumper.node(indent + 1, "ReturnType", return_type.span());
        dump_type_expression(dumper, return_type, indent + 2);
    }
    if let Some(body) = &decl.body {
        dump_declaration_body(dumper, body, indent + 1);
    }
}

fn dump_class_declaration(dumper: &mut Dumper, decl: &ClassDeclaration, indent: usize) {
    dumper.line(
        indent,
        &format!(
            "ClassDeclaration span={} name={} class_kind={}",
            span_str(&decl.span),
            escape_text(decl.name.as_str()),
            class_like_kind(decl.kind)
        ),
    );
    dump_object_body(dumper, &decl.body, indent + 1);
}

fn dump_trait_declaration(dumper: &mut Dumper, decl: &TraitDeclaration, indent: usize) {
    dumper.named(indent, "TraitDeclaration", &decl.span, decl.name.as_str());
    dump_object_body(dumper, &decl.body, indent + 1);
}

fn dump_imply_declaration(dumper: &mut Dumper, decl: &ImplyDeclaration, indent: usize) {
    dumper.node(indent, "ImplyDeclaration", &decl.span);
    dump_type_expression(dumper, &decl.target_type, indent + 1);
    if let Some(trait_type) = &decl.trait_type {
        dumper.node(indent + 1, "TraitType", trait_type.span());
        dump_type_expression(dumper, trait_type, indent + 2);
    }
    for method in &decl.methods {
        dump_object_method(dumper, method, indent + 1);
    }
}

fn dump_unite_declaration(dumper: &mut Dumper, decl: &UniteDeclaration, indent: usize) {
    dumper.line(
        indent,
        &format!(
            "UniteDeclaration span={} name={} sum_kind={}",
            span_str(&decl.span),
            escape_text(decl.name.as_str()),
            sum_type_kind(decl.kind)
        ),
    );
    for variant in &decl.variants {
        dumper.named(indent + 1, "UniteVariant", &variant.span, variant.name.as_str());
        if let Some(value) = &variant.value {
            dump_term_expression(dumper, value, indent + 2);
        }
    }
}

fn dump_flags_declaration(dumper: &mut Dumper, decl: &FlagsDeclaration, indent: usize) {
    dumper.named(indent, "FlagsDeclaration", &decl.span, decl.name.as_str());
    for member in &decl.members {
        dumper.named(indent + 1, "FlagsMember", &member.span, member.name.as_str());
        if let Some(value) = &member.value {
            dump_term_expression(dumper, value, indent + 2);
        }
    }
}

fn dump_object_body(dumper: &mut Dumper, body: &ObjectBody, indent: usize) {
    dumper.line(indent, "ObjectBody");
    for field in &body.fields {
        dump_object_field(dumper, field, indent + 1);
    }
    for method in &body.methods {
        dump_object_method(dumper, method, indent + 1);
    }
    for statement in &body.script_statements {
        dump_function_statement(dumper, statement, indent + 1);
    }
}

fn dump_object_field(dumper: &mut Dumper, field: &ObjectFieldDeclaration, indent: usize) {
    dumper.named(indent, "ObjectField", &field.span, field.name.as_str());
    dump_type_expression(dumper, &field.field_type, indent + 1);
    if let Some(default_value) = &field.default_value {
        dump_term_expression(dumper, default_value, indent + 1);
    }
}

fn dump_object_method(dumper: &mut Dumper, method: &ObjectMethodDeclaration, indent: usize) {
    dumper.named(indent, "ObjectMethod", &method.span, method.name.as_str());
    for param in &method.params {
        dump_function_parameter(dumper, param, indent + 1);
    }
    if let Some(return_type) = &method.return_type {
        dump_type_expression(dumper, return_type, indent + 1);
    }
    if let Some(body) = &method.body {
        dump_declaration_body(dumper, body, indent + 1);
    }
}

fn dump_function_parameter(dumper: &mut Dumper, param: &FunctionParameter, indent: usize) {
    dumper.line(
        indent,
        &format!(
            "Parameter span={} name={} variadic={}",
            span_str(&param.span),
            escape_text(param.name.as_str()),
            parameter_variadic_kind(param.variadic)
        ),
    );
    if let Some(parameter_type) = &param.parameter_type {
        dump_type_expression(dumper, parameter_type, indent + 1);
    }
    if let Some(default_value) = &param.default_value {
        dump_term_expression(dumper, default_value, indent + 1);
    }
}

fn dump_declaration_body(dumper: &mut Dumper, body: &DeclarationBody, indent: usize) {
    dumper.node(indent, "DeclarationBody", &body.span);
    for statement in &body.statements {
        dump_function_statement(dumper, statement, indent + 1);
    }
    if let Some(tail) = &body.tail_expression {
        dumper.node(indent + 1, "TailExpression", tail.span());
        dump_term_expression(dumper, tail, indent + 2);
    }
}

fn dump_function_statement(dumper: &mut Dumper, statement: &FunctionStatement, indent: usize) {
    match statement {
        FunctionStatement::Let(let_stmt) => dump_let_statement(dumper, let_stmt, indent),
        FunctionStatement::Term { expression, span } => {
            dumper.node(indent, "TermStatement", span);
            dump_term_expression(dumper, expression, indent + 1);
        }
        FunctionStatement::Function { function, span } => {
            dumper.node(indent, "NestedFunctionStatement", span);
            dump_function_declaration(dumper, function, indent + 1);
        }
        FunctionStatement::Break(stmt) => {
            dumper.node(indent, "BreakStatement", &stmt.span);
            if let Some(value) = &stmt.value {
                dump_term_expression(dumper, value, indent + 1);
            }
        }
        FunctionStatement::Continue(stmt) => dumper.node(indent, "ContinueStatement", &stmt.span),
        FunctionStatement::Yield(stmt) => {
            dumper.node(indent, "YieldStatement", &stmt.span);
            if let Some(value) = &stmt.value {
                dump_term_expression(dumper, value, indent + 1);
            }
        }
        FunctionStatement::YieldFrom(stmt) => {
            dumper.node(indent, "YieldFromStatement", &stmt.span);
            dump_term_expression(dumper, &stmt.value, indent + 1);
        }
        FunctionStatement::Return(stmt) => {
            dumper.node(indent, "ReturnStatement", &stmt.span);
            if let Some(value) = &stmt.value {
                dump_term_expression(dumper, value, indent + 1);
            }
        }
        FunctionStatement::Resume(stmt) => {
            dumper.node(indent, "ResumeStatement", &stmt.span);
            if let Some(value) = &stmt.value {
                dump_term_expression(dumper, value, indent + 1);
            }
        }
        FunctionStatement::Fallthrough(stmt) => dumper.node(indent, "FallthroughStatement", &stmt.span),
    }
}

fn dump_let_statement(dumper: &mut Dumper, statement: &LetStatement, indent: usize) {
    dumper.line(indent, &format!("LetStatement span={} mutable={}", span_str(&statement.span), statement.is_mutable));
    dump_pattern_expression(dumper, &statement.pattern, indent + 1);
    if let Some(ty) = &statement.ty {
        dump_type_expression(dumper, ty, indent + 1);
    }
    if let Some(initializer) = &statement.initializer {
        dump_term_expression(dumper, initializer, indent + 1);
    }
}

fn dump_term_expression(dumper: &mut Dumper, expression: &TermExpression, indent: usize) {
    match expression {
        TermExpression::Name { path, span } => {
            dumper.named(indent, "TermName", span, &path.parts.join("::"));
        }
        TermExpression::Literal { literal, span } => {
            dumper.line(indent, &format!("TermLiteral span={} kind={}", span_str(span), literal_kind(literal)));
        }
        TermExpression::Unary(unary) => dump_term_unary(dumper, unary, indent),
        TermExpression::Binary(binary) => dump_term_binary(dumper, binary, indent),
        TermExpression::Call(call) => dump_term_call(dumper, call, indent),
        TermExpression::DotCall(dot) => dump_term_dot(dumper, dot, indent),
        TermExpression::Dereference(deref) => {
            dumper.node(indent, "TermDereference", &deref.span);
            dump_term_expression(dumper, &deref.base, indent + 1);
        }
        TermExpression::Subscript(subscript) => dump_term_subscript(dumper, subscript, indent),
        TermExpression::Tuple { items, span } => {
            dumper.node(indent, "TermTuple", span);
            for item in items {
                dump_term_expression(dumper, item, indent + 1);
            }
        }
        TermExpression::Array { items, span } => {
            dumper.node(indent, "TermArray", span);
            for item in items {
                dump_term_expression(dumper, item, indent + 1);
            }
        }
        TermExpression::As(as_expr) => dump_term_as(dumper, as_expr, indent),
        TermExpression::Is(is_expr) => dump_term_is(dumper, is_expr, indent),
        TermExpression::Turbofish { expr, arguments, span } => {
            dumper.node(indent, "TermTurbofish", span);
            dump_term_expression(dumper, expr, indent + 1);
            for argument in arguments {
                dump_type_expression(dumper, argument, indent + 1);
            }
        }
        TermExpression::Assign { target, value, span } => {
            dumper.node(indent, "TermAssign", span);
            dump_term_expression(dumper, target, indent + 1);
            dump_term_expression(dumper, value, indent + 1);
        }
        TermExpression::Raise { value, span } => {
            dumper.node(indent, "TermRaise", span);
            dump_term_expression(dumper, value, indent + 1);
        }
        TermExpression::If(if_stmt) => dump_if_statement(dumper, if_stmt, indent),
        TermExpression::IfLet(if_let) => dump_if_let_statement(dumper, if_let, indent),
        TermExpression::Loop(loop_stmt) => dump_loop_statement(dumper, loop_stmt, indent),
        TermExpression::LoopIn(loop_in) => dump_loop_in_statement(dumper, loop_in, indent),
        TermExpression::While(while_stmt) => dump_while_statement(dumper, while_stmt, indent),
        TermExpression::WhileLet(while_let) => dump_while_let_statement(dumper, while_let, indent),
        TermExpression::Until(until_stmt) => dump_until_statement(dumper, until_stmt, indent),
        TermExpression::UntilNot(until_not) => dump_until_not_statement(dumper, until_not, indent),
        TermExpression::Try(try_stmt) => dump_try_statement(dumper, try_stmt, indent),
        TermExpression::Match { scrutinee, arms, span } => {
            dumper.node(indent, "TermMatch", span);
            dump_term_expression(dumper, scrutinee, indent + 1);
            for arm in arms {
                dump_arm_statement(dumper, arm, indent + 1);
            }
        }
        TermExpression::Catch { expr, arms, span } => {
            dumper.node(indent, "TermCatch", span);
            dump_term_expression(dumper, expr, indent + 1);
            for arm in arms {
                dump_arm_statement(dumper, arm, indent + 1);
            }
        }
        TermExpression::TryPropagate { base, span } => {
            dumper.node(indent, "TermTryPropagate", span);
            dump_term_expression(dumper, base, indent + 1);
        }
        TermExpression::MacroInvoke { path, args, span } => {
            dumper.named(indent, "TermMacroInvoke", span, &path.parts.join("::"));
            for arg in args {
                dump_term_expression(dumper, arg, indent + 1);
            }
        }
        TermExpression::PostfixMatch { base, arms, span } => {
            dumper.node(indent, "TermPostfixMatch", span);
            dump_term_expression(dumper, base, indent + 1);
            for arm in arms {
                dump_arm_statement(dumper, arm, indent + 1);
            }
        }
        TermExpression::PostfixCatch { base, arms, span } => {
            dumper.node(indent, "TermPostfixCatch", span);
            dump_term_expression(dumper, base, indent + 1);
            for arm in arms {
                dump_arm_statement(dumper, arm, indent + 1);
            }
        }
        TermExpression::Construct { path, fields, span } => {
            dumper.named(indent, "TermConstruct", span, &path.parts.join("::"));
            for (name, value) in fields {
                dumper.line(indent + 1, &format!("ConstructField name={}", escape_text(name)));
                dump_term_expression(dumper, value, indent + 2);
            }
        }
        TermExpression::Lambda { params, return_type, body, span } => {
            dumper.node(indent, "TermLambda", span);
            for param in params {
                dump_function_parameter(dumper, param, indent + 1);
            }
            if let Some(return_type) = return_type {
                dump_type_expression(dumper, return_type, indent + 1);
            }
            dump_declaration_body(dumper, body, indent + 1);
        }
        TermExpression::Block { body, span, .. } => {
            dumper.node(indent, "TermBlock", span);
            dump_declaration_body(dumper, body, indent + 1);
        }
        TermExpression::XmlMarkup { nodes, span } => {
            dumper.node(indent, "TermXmlMarkup", span);
            dump_xg_root(dumper, nodes, indent + 1);
        }
        TermExpression::Template { nodes, span } => {
            dumper.node(indent, "TermTemplate", span);
            dump_tg_root(dumper, nodes, indent + 1);
        }
        TermExpression::AnonymousClass { is_value_type, body, span, .. } => {
            dumper.line(indent, &format!("TermAnonymousClass span={} is_value_type={is_value_type}", span_str(span)));
            dump_object_body(dumper, body, indent + 1);
        }
    }
}

fn dump_term_unary(dumper: &mut Dumper, unary: &TermUnaryExpression, indent: usize) {
    dumper.node(indent, "TermUnary", &unary.span);
    dump_term_expression(dumper, &unary.base, indent + 1);
}

fn dump_term_binary(dumper: &mut Dumper, binary: &TermBinaryExpression, indent: usize) {
    dumper.node(indent, "TermBinary", &binary.span);
    dump_term_expression(dumper, &binary.lhs, indent + 1);
    dump_term_expression(dumper, &binary.rhs, indent + 1);
}

fn dump_term_call(dumper: &mut Dumper, call: &TermCallExpression, indent: usize) {
    dumper.node(indent, "TermCall", &call.span);
    dump_term_expression(dumper, &call.callee, indent + 1);
    for argument in &call.args.arguments {
        dump_term_call_argument(dumper, argument, indent + 1);
    }
}

fn dump_term_call_argument(dumper: &mut Dumper, argument: &TermCallArgument, indent: usize) {
    if let Some(key) = &argument.key {
        dumper.line(indent, &format!("CallArgument name={}", escape_text(key)));
    }
    else {
        dumper.line(indent, "CallArgument");
    }
    dump_term_expression(dumper, &argument.value, indent + 1);
}

fn dump_term_dot(dumper: &mut Dumper, dot: &TermDotExpression, indent: usize) {
    dumper.named(indent, "TermDotCall", &dot.span, &dot.caller.parts.join("::"));
    dump_term_expression(dumper, &dot.base, indent + 1);
}

fn dump_term_subscript(dumper: &mut Dumper, subscript: &TermSubscriptExpression, indent: usize) {
    dumper.node(indent, "TermSubscript", &subscript.span);
    dump_term_expression(dumper, &subscript.base, indent + 1);
    for item in &subscript.subscripts {
        match item {
            SubscriptItem::Index { term, span } => {
                dumper.node(indent + 1, "SubscriptIndex", span);
                dump_term_expression(dumper, term, indent + 2);
            }
            SubscriptItem::Slice { start, end, step, span } => {
                dumper.node(indent + 1, "SubscriptSlice", span);
                if let Some(start) = start {
                    dump_term_expression(dumper, start, indent + 2);
                }
                if let Some(end) = end {
                    dump_term_expression(dumper, end, indent + 2);
                }
                if let Some(step) = step {
                    dump_term_expression(dumper, step, indent + 2);
                }
            }
        }
    }
}

fn dump_term_as(dumper: &mut Dumper, as_expr: &TermAsExpression, indent: usize) {
    dumper.node(indent, "TermAs", &as_expr.span);
    dump_term_expression(dumper, &as_expr.base, indent + 1);
    dump_type_expression(dumper, &as_expr.target, indent + 1);
}

fn dump_term_is(dumper: &mut Dumper, is_expr: &TermIsExpression, indent: usize) {
    dumper.node(indent, "TermIs", &is_expr.span);
    dump_term_expression(dumper, &is_expr.base, indent + 1);
    dump_pattern_expression(dumper, &is_expr.target, indent + 1);
}

fn dump_if_statement(dumper: &mut Dumper, statement: &IfStatement, indent: usize) {
    dumper.node(indent, "IfStatement", &statement.span);
    dump_term_expression(dumper, &statement.condition, indent + 1);
    dump_declaration_body(dumper, &statement.then_body, indent + 1);
    if let Some(else_body) = &statement.else_body {
        dump_declaration_body(dumper, else_body, indent + 1);
    }
}

fn dump_if_let_statement(dumper: &mut Dumper, statement: &IfLetStatement, indent: usize) {
    dumper.node(indent, "IfLetStatement", &statement.span);
    dump_pattern_expression(dumper, &statement.pattern, indent + 1);
    dump_term_expression(dumper, &statement.item, indent + 1);
    dump_declaration_body(dumper, &statement.then_body, indent + 1);
    if let Some(else_body) = &statement.else_body {
        dump_declaration_body(dumper, else_body, indent + 1);
    }
}

fn dump_loop_statement(dumper: &mut Dumper, statement: &LoopStatement, indent: usize) {
    dumper.node(indent, "LoopStatement", &statement.span);
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_loop_in_statement(dumper: &mut Dumper, statement: &LoopInStatement, indent: usize) {
    dumper.node(indent, "LoopInStatement", &statement.span);
    if let Some(pattern) = &statement.pattern {
        dump_pattern_expression(dumper, pattern, indent + 1);
    }
    if let Some(iterator) = &statement.iterator {
        dump_term_expression(dumper, iterator, indent + 1);
    }
    if let Some(condition) = &statement.condition {
        dump_term_expression(dumper, condition, indent + 1);
    }
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_while_statement(dumper: &mut Dumper, statement: &WhileStatement, indent: usize) {
    dumper.node(indent, "WhileStatement", &statement.span);
    if let Some(condition) = &statement.condition {
        dump_term_expression(dumper, condition, indent + 1);
    }
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_while_let_statement(dumper: &mut Dumper, statement: &WhileLetStatement, indent: usize) {
    dumper.node(indent, "WhileLetStatement", &statement.span);
    dump_pattern_expression(dumper, &statement.pattern, indent + 1);
    dump_term_expression(dumper, &statement.scrutinee, indent + 1);
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_until_statement(dumper: &mut Dumper, statement: &UntilStatement, indent: usize) {
    dumper.node(indent, "UntilStatement", &statement.span);
    if let Some(pattern) = &statement.pattern {
        dump_pattern_expression(dumper, pattern, indent + 1);
    }
    if let Some(iterator) = &statement.iterator {
        dump_term_expression(dumper, iterator, indent + 1);
    }
    if let Some(condition) = &statement.condition {
        dump_term_expression(dumper, condition, indent + 1);
    }
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_until_not_statement(dumper: &mut Dumper, statement: &UntilNotStatement, indent: usize) {
    dumper.node(indent, "UntilNotStatement", &statement.span);
    if let Some(pattern) = &statement.pattern {
        dump_pattern_expression(dumper, pattern, indent + 1);
    }
    if let Some(iterator) = &statement.iterator {
        dump_term_expression(dumper, iterator, indent + 1);
    }
    if let Some(condition) = &statement.condition {
        dump_term_expression(dumper, condition, indent + 1);
    }
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_try_statement(dumper: &mut Dumper, statement: &TryStatement, indent: usize) {
    dumper.node(indent, "TryStatement", &statement.span);
    dump_declaration_body(dumper, &statement.body, indent + 1);
}

fn dump_arm_statement(dumper: &mut Dumper, arm: &ArmStatement, indent: usize) {
    match arm {
        ArmStatement::Case(case) => {
            dumper.node(indent, "CaseArm", &case.span);
            if let Some(pattern) = &case.pattern {
                dump_pattern_expression(dumper, pattern, indent + 1);
            }
            dump_declaration_body(dumper, &case.body, indent + 1);
        }
        ArmStatement::Type(type_arm) => {
            dumper.node(indent, "TypeArm", &type_arm.span);
            dump_type_expression(dumper, &type_arm.typing, indent + 1);
            dump_declaration_body(dumper, &type_arm.body, indent + 1);
        }
        ArmStatement::Else(else_arm) => dumper.node(indent, "ElseArm", &else_arm.span),
    }
}

fn dump_type_expression(dumper: &mut Dumper, expression: &TypeExpression, indent: usize) {
    match expression {
        TypeExpression::Path(path) => dumper.named(indent, "TypePath", &path.span, &path.name.parts.join("::")),
        TypeExpression::Array { item, span } => {
            dumper.node(indent, "TypeArray", span);
            dump_type_expression(dumper, item, indent + 1);
        }
        TypeExpression::FixedArray { item, length, span } => {
            dumper.line(indent, &format!("TypeFixedArray span={} length={length}", span_str(span)));
            dump_type_expression(dumper, item, indent + 1);
        }
        TypeExpression::Tuple { items, span } => {
            dumper.node(indent, "TypeTuple", span);
            for item in items {
                dump_type_expression(dumper, item, indent + 1);
            }
        }
        TypeExpression::Row { methods, span } => {
            dumper.node(indent, "TypeRow", span);
            for method in methods {
                dumper.named(indent + 1, "RowMethod", &method.span, method.name.as_str());
            }
        }
        TypeExpression::Pointer { item, span, .. } => {
            dumper.node(indent, "TypePointer", span);
            dump_type_expression(dumper, item, indent + 1);
        }
        TypeExpression::Associated { name, ty, span } => {
            dumper.named(indent, "TypeAssociated", span, name.as_str());
            dump_type_expression(dumper, ty, indent + 1);
        }
        TypeExpression::Nullable { item, span } => {
            dumper.node(indent, "TypeNullable", span);
            dump_type_expression(dumper, item, indent + 1);
        }
        TypeExpression::Union { items, span } => {
            dumper.node(indent, "TypeUnion", span);
            for item in items {
                dump_type_expression(dumper, item, indent + 1);
            }
        }
        TypeExpression::Intersection { items, span } => {
            dumper.node(indent, "TypeIntersection", span);
            for item in items {
                dump_type_expression(dumper, item, indent + 1);
            }
        }
        TypeExpression::Function { params, return_type, span } => {
            dumper.node(indent, "TypeFunction", span);
            for param in params {
                dump_type_expression(dumper, param, indent + 1);
            }
            dump_type_expression(dumper, return_type, indent + 1);
        }
    }
}

fn dump_pattern_expression(dumper: &mut Dumper, pattern: &PatternExpression, indent: usize) {
    match pattern {
        PatternExpression::Variable { name, span } => dumper.named(indent, "PatternVariable", span, name),
        PatternExpression::Name { path, span } => dumper.named(indent, "PatternName", span, &path.parts.join("::")),
        PatternExpression::Wildcard { span } => dumper.node(indent, "PatternWildcard", span),
        PatternExpression::Literal { literal, span } => {
            dumper.line(indent, &format!("PatternLiteral span={} kind={}", span_str(span), literal_kind(literal)));
        }
        PatternExpression::Tuple(tuple) => dump_tuple_pattern(dumper, tuple, indent),
        PatternExpression::Extract(extract) => dump_extract_pattern(dumper, extract, indent),
        PatternExpression::Object(object) => dump_object_pattern(dumper, object, indent),
        PatternExpression::Array(array) => dump_array_pattern(dumper, array, indent),
        PatternExpression::Range { span, .. } => dumper.node(indent, "PatternRange", span),
        PatternExpression::TypedBind { name, ty, span } => {
            dumper.named(indent, "PatternTypedBind", span, name);
            dumper.named(indent + 1, "PatternTypedBindType", &ty.span, &ty.parts.join("::"));
        }
        PatternExpression::Or(or_pattern) => dump_pattern_or(dumper, or_pattern, indent),
        PatternExpression::Bind { name, pattern, span } => {
            dumper.named(indent, "PatternBind", span, name);
            dump_pattern_expression(dumper, pattern, indent + 1);
        }
        PatternExpression::Mut { pattern, span } => {
            dumper.node(indent, "PatternMut", span);
            dump_pattern_expression(dumper, pattern, indent + 1);
        }
        PatternExpression::Pin { pattern, span, .. } => {
            dumper.node(indent, "PatternPin", span);
            dump_pattern_expression(dumper, pattern, indent + 1);
        }
    }
}

fn dump_tuple_pattern(dumper: &mut Dumper, pattern: &TuplePattern, indent: usize) {
    dumper.node(indent, "PatternTuple", &pattern.span);
    for item in &pattern.items {
        dump_pattern_expression(dumper, item, indent + 1);
    }
}

fn dump_extract_pattern(dumper: &mut Dumper, pattern: &ExtractPattern, indent: usize) {
    dumper.named(indent, "PatternExtract", &pattern.span, &pattern.name.parts.join("::"));
    for field in &pattern.fields {
        dump_pattern_expression(dumper, field, indent + 1);
    }
}

fn dump_object_pattern(dumper: &mut Dumper, pattern: &ObjectPattern, indent: usize) {
    if let Some(name) = &pattern.name {
        dumper.named(indent, "PatternObject", &pattern.span, &name.parts.join("::"));
    }
    else {
        dumper.node(indent, "PatternObject", &pattern.span);
    }
    for field in &pattern.fields {
        dumper.named(indent + 1, "PatternObjectField", &field.span, &field.name);
        dump_pattern_expression(dumper, &field.pattern, indent + 2);
    }
}

fn dump_array_pattern(dumper: &mut Dumper, pattern: &ArrayPattern, indent: usize) {
    dumper.node(indent, "PatternArray", &pattern.span);
    for item in &pattern.prefix {
        dump_pattern_expression(dumper, item, indent + 1);
    }
    if let Some(rest) = &pattern.rest {
        dumper.named(indent + 1, "PatternArrayRest", &pattern.span, rest.as_str());
    }
    for item in &pattern.suffix {
        dump_pattern_expression(dumper, item, indent + 1);
    }
}

fn dump_pattern_or(dumper: &mut Dumper, pattern: &PatternOrExpression, indent: usize) {
    dumper.node(indent, "PatternOr", &pattern.span);
    for item in &pattern.patterns {
        dump_pattern_expression(dumper, item, indent + 1);
    }
}

fn dump_xg_root(dumper: &mut Dumper, nodes: &XgRoot, indent: usize) {
    for node in nodes {
        dump_xg_node(dumper, node, indent);
    }
}

fn dump_xg_node(dumper: &mut Dumper, node: &XgNode, indent: usize) {
    match node {
        XgNode::Element(element) => dump_xg_element(dumper, element, indent),
        XgNode::Text { parts, span } => {
            dumper.node(indent, "XgText", span);
            for part in parts {
                dump_xg_text_part(dumper, part, indent + 1);
            }
        }
        XgNode::Meta { nodes, span } => {
            dumper.node(indent, "XgMeta", span);
            dump_tg_root(dumper, nodes, indent + 1);
        }
    }
}

fn dump_xg_element(dumper: &mut Dumper, element: &XgElement, indent: usize) {
    dumper.line(
        indent,
        &format!("XgElement span={} tag={} self_closing={}", span_str(&element.span), escape_text(&element.tag), element.self_closing),
    );
    for (name, value) in &element.attrs {
        let value_kind = match value {
            XgAttrValue::Literal(_) => "Literal",
            XgAttrValue::Expression(_) => "Expression",
        };
        dumper.line(indent + 1, &format!("XgAttribute name={} value_kind={value_kind}", escape_text(name)));
    }
    dump_xg_root(dumper, &element.children, indent + 1);
}

fn dump_xg_text_part(dumper: &mut Dumper, part: &XgTextPart, indent: usize) {
    match part {
        XgTextPart::Static(text) => dumper.line(indent, &format!("XgTextStatic text={}", escape_text(text))),
        XgTextPart::Expression(text) => dumper.line(indent, &format!("XgTextExpression text={}", escape_text(text))),
    }
}

fn dump_tg_root(dumper: &mut Dumper, nodes: &TgRoot, indent: usize) {
    for node in nodes {
        dump_tg_node(dumper, node, indent);
    }
}

fn dump_tg_node(dumper: &mut Dumper, node: &TgNode, indent: usize) {
    match node {
        TgNode::Text { parts, span } => {
            dumper.node(indent, "TgText", span);
            for part in parts {
                dump_tg_text_part(dumper, part, indent + 1);
            }
        }
        TgNode::Stmt { span, .. } => dumper.node(indent, "TgStmt", span),
        TgNode::If(if_block) => dump_tg_if(dumper, if_block, indent),
        TgNode::Loop(loop_block) => dump_tg_loop(dumper, loop_block, indent),
        TgNode::Match(match_block) => dump_tg_match(dumper, match_block, indent),
        TgNode::Comment { span } => dumper.node(indent, "TgComment", span),
    }
}

fn dump_tg_if(dumper: &mut Dumper, if_block: &TgIf, indent: usize) {
    dumper.node(indent, "TgIf", &if_block.span);
    for arm in &if_block.arms {
        dump_tg_if_arm(dumper, arm, indent + 1);
    }
}

fn dump_tg_if_arm(dumper: &mut Dumper, arm: &TgIfArm, indent: usize) {
    dumper.node(indent, "TgIfArm", &arm.span);
    dump_tg_root(dumper, &arm.body, indent + 1);
}

fn dump_tg_loop(dumper: &mut Dumper, loop_block: &TgLoop, indent: usize) {
    dumper.node(indent, "TgLoop", &loop_block.span);
    dump_tg_root(dumper, &loop_block.body, indent + 1);
}

fn dump_tg_match(dumper: &mut Dumper, match_block: &TgMatch, indent: usize) {
    dumper.node(indent, "TgMatch", &match_block.span);
    for arm in &match_block.arms {
        dump_tg_match_arm(dumper, arm, indent + 1);
    }
}

fn dump_tg_match_arm(dumper: &mut Dumper, arm: &TgMatchArm, indent: usize) {
    dumper.node(indent, "TgMatchArm", &arm.span);
    dump_tg_root(dumper, &arm.body, indent + 1);
}

fn dump_tg_text_part(dumper: &mut Dumper, part: &TgTextPart, indent: usize) {
    match part {
        TgTextPart::Static(text) => dumper.line(indent, &format!("TgTextStatic text={}", escape_text(text))),
        TgTextPart::Expression(text) => dumper.line(indent, &format!("TgTextExpression text={}", escape_text(text))),
    }
}

struct Dumper {
    lines: Vec<String>,
}

impl Dumper {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.lines.push(format!("{}{text}", "  ".repeat(indent)));
    }

    fn node(&mut self, indent: usize, kind: &str, span: &Range<usize>) {
        self.line(indent, &format!("{kind} span={}", span_str(span)));
    }

    fn named(&mut self, indent: usize, kind: &str, span: &Range<usize>, name: &str) {
        self.line(indent, &format!("{kind} span={} name={}", span_str(span), escape_text(name)));
    }

    fn finish(self) -> String {
        self.lines.join("\n")
    }
}

fn span_str(span: &Range<usize>) -> String {
    format!("{}..{}", span.start, span.end)
}

fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{{{:04x}}}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn root_statement_kind(statement: &RootStatement) -> &'static str {
    match statement {
        RootStatement::Namespace(_) => "Namespace",
        RootStatement::Using(_) => "Using",
        RootStatement::Function(_) => "Function",
        RootStatement::Class(_) => "Class",
        RootStatement::Trait(_) => "Trait",
        RootStatement::Imply(_) => "Imply",
        RootStatement::Unite(_) => "Unite",
        RootStatement::Attribute(_) => "Attribute",
        RootStatement::TypeAlias(_) => "TypeAlias",
        RootStatement::Flags(_) => "Flags",
        RootStatement::MacroAssign(_) => "MacroAssign",
        RootStatement::Tests(_) => "Tests",
    }
}

fn function_decl_kind(kind: FunctionDeclKind) -> &'static str {
    match kind {
        FunctionDeclKind::Micro => "Micro",
        FunctionDeclKind::Mezzo => "Mezzo",
        FunctionDeclKind::Macro => "Macro",
    }
}

fn class_like_kind(kind: ClassLikeKind) -> &'static str {
    match kind {
        ClassLikeKind::Class => "Class",
        ClassLikeKind::Structure => "Structure",
        ClassLikeKind::Widget => "Widget",
        ClassLikeKind::Singleton => "Singleton",
        ClassLikeKind::Neural => "Neural",
    }
}

fn sum_type_kind(kind: SumTypeKind) -> &'static str {
    match kind {
        SumTypeKind::Unite => "Unite",
        SumTypeKind::Union => "Union",
        SumTypeKind::Enum => "Enum",
    }
}

fn parameter_variadic_kind(kind: crate::transitional::cst::valkyrie::ParameterVariadicKind) -> &'static str {
    match kind {
        crate::transitional::cst::valkyrie::ParameterVariadicKind::None => "None",
        crate::transitional::cst::valkyrie::ParameterVariadicKind::PositionalRest => "PositionalRest",
        crate::transitional::cst::valkyrie::ParameterVariadicKind::KeywordRest => "KeywordRest",
    }
}

fn literal_kind(literal: &LiteralExpression) -> &'static str {
    match literal {
        LiteralExpression::Integer(_) => "Integer",
        LiteralExpression::Float(_) => "Float",
        LiteralExpression::String(_) => "String",
        LiteralExpression::Bool(_) => "Bool",
        LiteralExpression::Unit => "Unit",
        LiteralExpression::Null => "Null",
    }
}
