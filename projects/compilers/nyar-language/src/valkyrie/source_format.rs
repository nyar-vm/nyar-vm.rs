//! Valkyrie (`.v` / `.vx`) source formatter.

use std_data::text::valkyrie::{
    Annotations, BinaryOperator, ClassDeclaration, ClassLikeKind, DeclarationBody, FlagsDeclaration, FunctionDeclKind, FunctionDeclaration,
    FunctionParameter, FunctionStatement, GenericParameterDeclaration, ImplyDeclaration, InheritanceItem, LetStatement, LiteralExpression,
    NamePath, ObjectBody, ObjectFieldDeclaration, ObjectMethodDeclaration, PatternExpression, RootStatement, StringLiteral, StringSegment,
    SumTypeKind, TermCallArgument, TermExpression, TraitDeclaration, TypeExpression, UnaryOperator, UniteDeclaration, UniteVariantDeclaration,
    ValkyrieRoot, WhereConstraintDeclaration,
    ast::{
        ArmStatement, DereferenceKind, ParameterBindingKind, ParameterPassingKind, ParameterVariadicKind, PointerKind, SubscriptItem,
        SubscriptKind,
    },
    xml::{XgAttrValue, XgElement, XgNode, XgTextPart},
};

use crate::formatter::{FormatBuffer, FormatError, FormatOptions, FormattedOutput};

pub(crate) fn format_valkyrie(source: &str, options: &FormatOptions, vx: bool) -> Result<FormattedOutput, FormatError> {
    crate::text::valkyrie::format_valkyrie_cst(source, options, vx)
}

/// 格式化单条顶层语句（供 CST formatter 调用）。
pub(crate) fn format_statement(stmt: &RootStatement, options: &FormatOptions) -> String {
    let mut buf = FormatBuffer::new(options);
    write_root_statement(&mut buf, stmt);
    buf.finish()
}

#[allow(dead_code)]
fn write_root(buf: &mut FormatBuffer, root: &ValkyrieRoot) {
    for (index, statement) in root.statements.iter().enumerate() {
        if index > 0 {
            buf.newline();
            buf.newline();
        }
        write_root_statement(buf, statement);
    }
}

fn write_root_statement(buf: &mut FormatBuffer, statement: &RootStatement) {
    match statement {
        RootStatement::Namespace(ns) => {
            buf.write("namespace ");
            write_name_path(buf, &ns.name);
            match &ns.body {
                Some(body) => {
                    buf.write(" ");
                    write_body(buf, body);
                }
                None => buf.write(";"),
            }
        }
        RootStatement::Using(u) => {
            buf.write("using ");
            write_name_path(buf, &u.path);
            if u.glob_import {
                buf.write(".*;");
            }
            else if !u.selective_imports.is_empty() {
                buf.write(".{");
                for (i, item) in u.selective_imports.iter().enumerate() {
                    if i > 0 {
                        buf.write(", ");
                    }
                    buf.write(&item.name);
                    if let Some(alias) = &item.alias {
                        buf.write(" as ");
                        buf.write(alias);
                    }
                }
                buf.write("};");
            }
            else if let Some(alias) = &u.alias {
                buf.write(" as ");
                buf.write(alias);
                buf.write(";");
            }
            else {
                buf.write(";");
            }
        }
        RootStatement::Function(f) => write_function(buf, f),
        RootStatement::Class(c) => write_class(buf, c),
        RootStatement::Trait(t) => write_trait_def(buf, t),
        RootStatement::Imply(i) => write_imply(buf, i),
        RootStatement::Unite(u) => write_unite(buf, u),
        RootStatement::Attribute(a) => {
            buf.write("attribute ");
            buf.write(a.name.as_str());
            buf.write(";");
        }
        RootStatement::TypeAlias(a) => {
            buf.write("type ");
            buf.write(a.name.as_str());
            buf.write(" = ");
            write_type(buf, &a.target);
            buf.write(";");
        }
        RootStatement::Flags(f) => write_flags(buf, f),
        RootStatement::MacroAssign(m) => {
            write_annotations(buf, &m.annotations);
            buf.write("macro ");
            buf.write(m.name.as_str());
            write_generics(buf, &m.generic_parameters);
            buf.write(" = ");
            write_term(buf, &m.value);
            buf.write(";");
        }
        RootStatement::Tests(t) => {
            write_annotations(buf, &t.annotations);
            buf.write("tests ");
            write_body(buf, &t.body);
        }
    }
}

fn write_function(buf: &mut FormatBuffer, f: &FunctionDeclaration) {
    write_annotations(buf, &f.annotations);
    buf.write(match f.kind {
        FunctionDeclKind::Micro => "micro",
        FunctionDeclKind::Mezzo => "mezzo",
        FunctionDeclKind::Macro => "macro",
    });
    buf.write(" ");
    buf.write(f.name.as_str());
    write_generics(buf, &f.generic_parameters);
    buf.write("(");
    write_params(buf, &f.params);
    buf.write(")");
    if let Some(ret) = &f.return_type {
        buf.write(" -> ");
        write_type(buf, ret);
    }
    write_where(buf, &f.where_constraints);
    match &f.body {
        Some(body) => {
            buf.write(" ");
            write_body(buf, body);
        }
        None => buf.write(";"),
    }
}

fn write_class(buf: &mut FormatBuffer, c: &ClassDeclaration) {
    write_annotations(buf, &c.annotations);
    buf.write(match c.kind {
        ClassLikeKind::Class => "class",
        ClassLikeKind::Structure => "structure",
        ClassLikeKind::Widget => "widget",
        ClassLikeKind::Singleton => "singleton",
        ClassLikeKind::Neural => "neural",
    });
    buf.write(" ");
    buf.write(c.name.as_str());
    write_generics(buf, &c.generic_parameters);
    if !c.inheritance.is_empty() {
        buf.write("(");
        for (i, item) in c.inheritance.iter().enumerate() {
            if i > 0 {
                buf.write(", ");
            }
            write_inheritance(buf, item);
        }
        buf.write(")");
    }
    buf.write(" ");
    write_object_body(buf, &c.body);
}

fn write_trait_def(buf: &mut FormatBuffer, t: &TraitDeclaration) {
    write_annotations(buf, &t.annotations);
    buf.write("trait ");
    buf.write(t.name.as_str());
    if !t.generic_parameters.is_empty() {
        buf.write("<");
        for (i, g) in t.generic_parameters.iter().enumerate() {
            if i > 0 {
                buf.write(", ");
            }
            buf.write(g);
        }
        buf.write(">");
    }
    if t.is_alias {
        buf.write(" = ");
        for (i, item) in t.alias_targets.iter().enumerate() {
            if i > 0 {
                buf.write(" + ");
            }
            write_inheritance(buf, item);
        }
        buf.write(";");
        return;
    }
    if !t.inheritance.is_empty() {
        buf.write(": ");
        for (i, item) in t.inheritance.iter().enumerate() {
            if i > 0 {
                buf.write(", ");
            }
            write_inheritance(buf, item);
        }
    }
    buf.write(" ");
    write_object_body(buf, &t.body);
}

fn write_imply(buf: &mut FormatBuffer, d: &ImplyDeclaration) {
    write_annotations(buf, &d.annotations);
    buf.write("imply ");
    write_generics(buf, &d.generic_parameters);
    write_type(buf, &d.target_type);
    if let Some(trait_ty) = &d.trait_type {
        buf.write(": ");
        write_type(buf, trait_ty);
    }
    write_where(buf, &d.where_constraints);
    buf.write(" {");
    buf.newline();
    buf.indent();
    for binding in &d.associated_type_bindings {
        write_annotations(buf, &binding.annotations);
        buf.write("type ");
        buf.write(binding.name.as_str());
        write_generics(buf, &binding.generic_parameters);
        buf.write(" = ");
        write_type(buf, &binding.concrete_type);
        buf.write(";");
        buf.newline();
    }
    for binding in &d.associated_const_bindings {
        write_annotations(buf, &binding.annotations);
        buf.write("const ");
        buf.write(binding.name.as_str());
        if let Some(ty) = &binding.const_type {
            buf.write(": ");
            write_type(buf, ty);
        }
        buf.write(" = ");
        write_term(buf, &binding.value);
        buf.write(";");
        buf.newline();
    }
    for method in &d.methods {
        write_object_method(buf, method);
        buf.newline();
        buf.newline();
    }
    buf.dedent();
    buf.write("}");
}

fn write_unite(buf: &mut FormatBuffer, u: &UniteDeclaration) {
    write_annotations(buf, &u.annotations);
    buf.write(match u.kind {
        SumTypeKind::Unite => "unite",
        SumTypeKind::Union => "union",
        SumTypeKind::Enum => "enums",
    });
    buf.write(" ");
    buf.write(u.name.as_str());
    write_generics(buf, &u.generic_parameters);
    buf.write(" {");
    buf.newline();
    buf.indent();
    let value_align = unite_variant_value_align(u);
    for variant in &u.variants {
        write_unite_variant(buf, variant, value_align);
    }
    buf.dedent();
    buf.write("}");
}

fn unite_variant_value_align(u: &UniteDeclaration) -> Option<usize> {
    if u.kind != SumTypeKind::Enum || !u.variants.iter().any(|variant| variant.value.is_some()) {
        return None;
    }
    Some(u.variants.iter().filter(|variant| variant.value.is_some()).map(|variant| variant.name.as_str().len()).max().unwrap_or(0))
}

fn write_unite_variant(buf: &mut FormatBuffer, v: &UniteVariantDeclaration, value_align: Option<usize>) {
    write_annotations(buf, &v.annotations);
    buf.write(v.name.as_str());
    if !v.fields.is_empty() {
        buf.write(" {");
        buf.newline();
        buf.indent();
        for field in &v.fields {
            write_object_field(buf, field);
            buf.newline();
        }
        buf.dedent();
        buf.write("}");
    }
    if let Some(result) = &v.result_type {
        buf.write(" -> ");
        write_type(buf, result);
    }
    if let Some(value) = &v.value {
        write_name_value_padding(buf, v.name.as_str(), value_align);
        buf.write(" = ");
        write_term(buf, value);
    }
    buf.write(",");
    buf.newline();
}

fn write_name_value_padding(buf: &mut FormatBuffer, name: &str, max_name_len: Option<usize>) {
    if let Some(max_name_len) = max_name_len.filter(|max| *max > name.len()) {
        buf.write_raw(&" ".repeat(max_name_len - name.len()));
    }
}

fn write_flags(buf: &mut FormatBuffer, f: &FlagsDeclaration) {
    write_annotations(buf, &f.annotations);
    buf.write("flags ");
    buf.write(f.name.as_str());
    if !f.inheritance.is_empty() {
        buf.write(": ");
        for (i, item) in f.inheritance.iter().enumerate() {
            if i > 0 {
                buf.write(", ");
            }
            write_inheritance(buf, item);
        }
    }
    buf.write(" {");
    buf.newline();
    buf.indent();
    let max_name_len = flags_member_value_align(f);
    for member in &f.members {
        write_annotations(buf, &member.annotations);
        buf.write(member.name.as_str());
        if let Some(value) = &member.value {
            write_name_value_padding(buf, member.name.as_str(), max_name_len);
            buf.write(" = ");
            write_term(buf, value);
        }
        buf.write(",");
        buf.newline();
    }
    buf.dedent();
    buf.write("}");
}

fn flags_member_value_align(f: &FlagsDeclaration) -> Option<usize> {
    if !f.members.iter().any(|member| member.value.is_some()) {
        return None;
    }
    Some(f.members.iter().filter(|member| member.value.is_some()).map(|member| member.name.as_str().len()).max().unwrap_or(0))
}

fn write_object_body(buf: &mut FormatBuffer, body: &ObjectBody) {
    buf.write("{");
    let empty = body.fields.is_empty()
        && body.methods.is_empty()
        && body.associated_types.is_empty()
        && body.associated_constants.is_empty()
        && body.variants.is_empty();
    if empty {
        buf.write("}");
        return;
    }
    buf.newline();
    buf.indent();
    for field in &body.fields {
        write_object_field(buf, field);
        buf.newline();
    }
    for assoc in &body.associated_types {
        write_annotations(buf, &assoc.annotations);
        buf.write("type ");
        buf.write(assoc.name.as_str());
        if !assoc.generic_parameters.is_empty() {
            buf.write("<");
            for (i, g) in assoc.generic_parameters.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                buf.write(g);
            }
            buf.write(">");
        }
        if !assoc.bounds.is_empty() {
            buf.write(": ");
            for (i, b) in assoc.bounds.iter().enumerate() {
                if i > 0 {
                    buf.write(" + ");
                }
                write_type(buf, b);
            }
        }
        if let Some(default) = &assoc.default_type {
            buf.write(" = ");
            write_type(buf, default);
        }
        buf.write(";");
        buf.newline();
    }
    for assoc in &body.associated_constants {
        write_annotations(buf, &assoc.annotations);
        buf.write("const ");
        buf.write(assoc.name.as_str());
        buf.write(": ");
        write_type(buf, &assoc.const_type);
        if let Some(default) = &assoc.default_value {
            buf.write(" = ");
            write_term(buf, default);
        }
        buf.write(";");
        buf.newline();
    }
    for method in &body.methods {
        write_object_method(buf, method);
        buf.newline();
        buf.newline();
    }
    let variant_value_align = if body.variants.iter().any(|item| item.value.is_some()) {
        Some(body.variants.iter().filter(|item| item.value.is_some()).map(|item| item.name.as_str().len()).max().unwrap_or(0))
    }
    else {
        None
    };
    for variant in &body.variants {
        write_unite_variant(buf, variant, variant_value_align);
    }
    buf.dedent();
    buf.write("}");
}

fn write_object_field(buf: &mut FormatBuffer, field: &ObjectFieldDeclaration) {
    write_annotations(buf, &field.annotations);
    buf.write(field.name.as_str());
    buf.write(": ");
    write_type(buf, &field.field_type);
    if let Some(default) = &field.default_value {
        buf.write(" = ");
        write_term(buf, default);
    }
    buf.write(",");
}

fn write_object_method(buf: &mut FormatBuffer, method: &ObjectMethodDeclaration) {
    write_annotations(buf, &method.annotations);
    buf.write("micro ");
    buf.write(method.name.as_str());
    buf.write("(");
    write_params(buf, &method.params);
    buf.write(")");
    if let Some(ret) = &method.return_type {
        buf.write(" -> ");
        write_type(buf, ret);
    }
    match &method.body {
        Some(body) => {
            buf.write(" ");
            write_body(buf, body);
        }
        None => buf.write(";"),
    }
}

fn write_annotations(buf: &mut FormatBuffer, ann: &Annotations) {
    for doc in &ann.documents {
        buf.write("/// ");
        buf.write(doc.trim_start_matches('/').trim());
        buf.newline();
    }
    for list in &ann.attribute_lists {
        buf.write("[");
        for (i, item) in list.items.iter().enumerate() {
            if i > 0 {
                buf.write(", ");
            }
            write_name_path(buf, &item.name);
            if !item.arguments.is_empty() {
                buf.write("(");
                for (j, arg) in item.arguments.iter().enumerate() {
                    if j > 0 {
                        buf.write(", ");
                    }
                    if let Some(key) = &arg.key {
                        buf.write(key);
                        buf.write(": ");
                    }
                    write_term(buf, &arg.value);
                }
                buf.write(")");
            }
        }
        buf.write("]");
        buf.newline();
    }
    for modifier in &ann.modifiers {
        buf.write(modifier.as_str());
        buf.write(" ");
    }
}

fn write_body(buf: &mut FormatBuffer, body: &DeclarationBody) {
    buf.write("{");
    if body.statements.is_empty() && body.tail_expression.is_none() {
        buf.write("}");
        return;
    }
    buf.newline();
    buf.indent();
    for stmt in &body.statements {
        write_function_statement(buf, stmt);
        buf.newline();
    }
    if let Some(tail) = &body.tail_expression {
        write_term(buf, tail);
        buf.newline();
    }
    buf.dedent();
    buf.write("}");
}

fn write_function_statement(buf: &mut FormatBuffer, stmt: &FunctionStatement) {
    match stmt {
        FunctionStatement::Let(let_stmt) => {
            write_let(buf, let_stmt);
            buf.write(";");
        }
        FunctionStatement::Term { expression, .. } => {
            write_term(buf, expression);
            buf.write(";");
        }
        FunctionStatement::Function { function, .. } => write_function(buf, function),
        FunctionStatement::Break(b) => {
            buf.write("break");
            if let Some(label) = &b.label {
                buf.write(" ");
                buf.write(label.as_str());
            }
            if let Some(value) = &b.value {
                buf.write(" ");
                write_term(buf, value);
            }
            buf.write(";");
        }
        FunctionStatement::Continue(c) => {
            buf.write("continue");
            if let Some(label) = &c.label {
                buf.write(" ");
                buf.write(label.as_str());
            }
            buf.write(";");
        }
        FunctionStatement::Yield(y) => {
            buf.write("yield");
            if let Some(value) = &y.value {
                buf.write(" ");
                write_term(buf, value);
            }
            buf.write(";");
        }
        FunctionStatement::YieldFrom(y) => {
            buf.write("yield from ");
            write_term(buf, &y.value);
            buf.write(";");
        }
        FunctionStatement::Return(r) => {
            buf.write("return");
            if let Some(value) = &r.value {
                buf.write(" ");
                write_term(buf, value);
            }
            buf.write(";");
        }
        FunctionStatement::Resume(r) => {
            buf.write("resume");
            if let Some(value) = &r.value {
                buf.write(" ");
                write_term(buf, value);
            }
            buf.write(";");
        }
        FunctionStatement::Fallthrough(_) => buf.write("fallthrough;"),
    }
}

fn write_let(buf: &mut FormatBuffer, stmt: &LetStatement) {
    buf.write("let ");
    if stmt.is_mutable {
        buf.write("mut ");
    }
    write_pattern(buf, &stmt.pattern);
    if let Some(ty) = &stmt.ty {
        buf.write(": ");
        write_type(buf, ty);
    }
    if let Some(init) = &stmt.initializer {
        buf.write(" = ");
        write_term(buf, init);
    }
}

fn write_params(buf: &mut FormatBuffer, params: &[FunctionParameter]) {
    let (lt_index, gt_index) = parameter_marker_indices(params);
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            buf.write(", ");
        }
        if Some(i) == lt_index {
            buf.write("<, ");
        }
        if Some(i) == gt_index {
            buf.write(">, ");
        }
        match p.variadic {
            ParameterVariadicKind::PositionalRest => buf.write(".."),
            ParameterVariadicKind::KeywordRest => buf.write("..."),
            ParameterVariadicKind::None => {}
        }
        match p.passing {
            ParameterPassingKind::Ref => {}
            ParameterPassingKind::Mut => buf.write("mut "),
            ParameterPassingKind::Own => buf.write("own "),
        }
        buf.write(p.name.as_str());
        if let Some(ty) = &p.parameter_type {
            buf.write(": ");
            write_type(buf, ty);
        }
        if let Some(default) = &p.default_value {
            buf.write(" = ");
            write_term(buf, default);
        }
    }
}

fn parameter_marker_indices(params: &[FunctionParameter]) -> (Option<usize>, Option<usize>) {
    let gt_index = params.iter().position(|p| p.binding_kind == ParameterBindingKind::KeywordOnly);
    let positional_only_count = params.iter().take_while(|p| p.binding_kind == ParameterBindingKind::PositionalOnly).count();
    let lt_index = if positional_only_count > 0 {
        Some(positional_only_count)
    }
    else if gt_index.is_some() {
        Some(0)
    }
    else {
        None
    };
    (lt_index, gt_index)
}

fn write_call_argument(buf: &mut FormatBuffer, arg: &TermCallArgument) {
    if let Some(name) = &arg.key {
        buf.write(name);
        buf.write(" = ");
    }
    write_term(buf, &arg.value);
}

fn write_generics(buf: &mut FormatBuffer, params: &[GenericParameterDeclaration]) {
    if params.is_empty() {
        return;
    }
    buf.write("<");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            buf.write(", ");
        }
        buf.write(p.name.as_str());
        if !p.bounds.is_empty() {
            buf.write(": ");
            for (j, b) in p.bounds.iter().enumerate() {
                if i > 0 && j > 0 {}
                if j > 0 {
                    buf.write(" + ");
                }
                write_type(buf, b);
            }
        }
        if let Some(default) = &p.default_type {
            buf.write(" = ");
            write_type(buf, default);
        }
    }
    buf.write(">");
}

fn write_where(buf: &mut FormatBuffer, constraints: &[WhereConstraintDeclaration]) {
    if constraints.is_empty() {
        return;
    }
    buf.write(" where ");
    for (i, c) in constraints.iter().enumerate() {
        if i > 0 {
            buf.write(", ");
        }
        write_type(buf, &c.target_type);
        buf.write(": ");
        for (j, b) in c.bounds.iter().enumerate() {
            if j > 0 {
                buf.write(" + ");
            }
            write_type(buf, b);
        }
    }
}

fn write_inheritance(buf: &mut FormatBuffer, item: &InheritanceItem) {
    if let Some(alias) = &item.alias {
        buf.write(alias);
        buf.write(" = ");
    }
    write_type(buf, &item.base_type);
}

fn write_name_path(buf: &mut FormatBuffer, path: &NamePath) {
    for (i, part) in path.parts.iter().enumerate() {
        if i > 0 {
            buf.write(".");
        }
        buf.write(part);
    }
}

fn write_type(buf: &mut FormatBuffer, ty: &TypeExpression) {
    match ty {
        TypeExpression::Path(path) => {
            write_name_path(buf, &path.name);
            if !path.arguments.is_empty() {
                buf.write("<");
                for (i, arg) in path.arguments.iter().enumerate() {
                    if i > 0 {
                        buf.write(", ");
                    }
                    write_type(buf, arg);
                }
                buf.write(">");
            }
        }
        TypeExpression::Array { item, .. } => {
            buf.write("[");
            write_type(buf, item);
            buf.write("]");
        }
        TypeExpression::FixedArray { item, length, .. } => {
            buf.write("[");
            write_type(buf, item);
            buf.write(&format!("; {length}]"));
        }
        TypeExpression::Tuple { items, .. } => {
            buf.write("(");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_type(buf, item);
            }
            if items.len() == 1 {
                buf.write(",");
            }
            buf.write(")");
        }
        TypeExpression::Row { methods, .. } => {
            buf.write("{ ");
            for (i, m) in methods.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                buf.write(m.name.as_str());
                buf.write("(");
                for (j, p) in m.params.iter().enumerate() {
                    if j > 0 {
                        buf.write(", ");
                    }
                    write_type(buf, p);
                }
                buf.write(") -> ");
                write_type(buf, &m.return_type);
            }
            buf.write(" }");
        }
        TypeExpression::Pointer { kind, item, .. } => {
            match kind {
                PointerKind::ReadOnly => buf.write("◇"),
                PointerKind::Mutable => buf.write("◆"),
            }
            write_type(buf, item);
        }
        TypeExpression::Associated { name, ty, .. } => {
            buf.write(name.as_str());
            buf.write(" = ");
            write_type(buf, ty);
        }
        TypeExpression::Nullable { item, .. } => {
            write_type(buf, item);
            buf.write("?");
        }
        TypeExpression::Function { params, return_type, .. } => {
            buf.write("micro(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_type(buf, p);
            }
            buf.write(") -> ");
            write_type(buf, return_type);
        }
        TypeExpression::Union { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.write(" | ");
                }
                write_type(buf, item);
            }
        }
        TypeExpression::Intersection { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.write(" & ");
                }
                write_type(buf, item);
            }
        }
    }
}

fn write_term(buf: &mut FormatBuffer, expr: &TermExpression) {
    match expr {
        TermExpression::Name { path, .. } => write_name_path(buf, path),
        TermExpression::Literal { literal, .. } => write_literal(buf, literal),
        TermExpression::Unary(u) => {
            buf.write(match u.operator {
                UnaryOperator::Neg => "-",
                UnaryOperator::Not => "!",
            });
            write_term(buf, &u.base);
        }
        TermExpression::Binary(b) => {
            write_term(buf, &b.lhs);
            buf.write(" ");
            buf.write(binary_op(&b.operator));
            buf.write(" ");
            write_term(buf, &b.rhs);
        }
        TermExpression::Call(c) => {
            write_term(buf, &c.callee);
            buf.write("(");
            for (i, arg) in c.args.arguments.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_call_argument(buf, arg);
            }
            buf.write(")");
        }
        TermExpression::DotCall(d) => {
            write_term(buf, &d.base);
            buf.write(".");
            write_name_path(buf, &d.caller);
            if !d.arguments.arguments.is_empty() {
                buf.write("(");
                for (i, arg) in d.arguments.arguments.iter().enumerate() {
                    if i > 0 {
                        buf.write(", ");
                    }
                    write_call_argument(buf, arg);
                }
                buf.write(")");
            }
        }
        TermExpression::Dereference(d) => {
            write_term(buf, &d.base);
            match d.kind {
                DereferenceKind::ReadOnly => buf.write(".◇"),
                DereferenceKind::Mutable => buf.write(".◆"),
            }
        }
        TermExpression::Subscript(s) => {
            write_term(buf, &s.base);
            match s.kind {
                SubscriptKind::Ordinal => buf.write("["),
                SubscriptKind::Cardinal => buf.write("⁅"),
            }
            for (i, item) in s.subscripts.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_subscript_item(buf, item);
            }
            match s.kind {
                SubscriptKind::Ordinal => buf.write("]"),
                SubscriptKind::Cardinal => buf.write("⁆"),
            }
        }
        TermExpression::Tuple { items, .. } => {
            buf.write("(");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_term(buf, item);
            }
            if items.len() == 1 {
                buf.write(",");
            }
            buf.write(")");
        }
        TermExpression::Array { items, .. } => {
            buf.write("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_term(buf, item);
            }
            buf.write("]");
        }
        TermExpression::As(a) => {
            write_term(buf, &a.base);
            buf.write(" as ");
            write_type(buf, &a.target);
        }
        TermExpression::Is(term) => {
            write_term(buf, &term.base);
            buf.write(" is ");
            write_pattern(buf, &term.target);
        }
        TermExpression::Turbofish { expr, arguments, .. } => {
            write_term(buf, expr);
            buf.write("::<");
            for (i, arg) in arguments.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_type(buf, arg);
            }
            buf.write(">");
        }
        TermExpression::Assign { target, value, .. } => {
            write_term(buf, target);
            buf.write(" = ");
            write_term(buf, value);
        }
        TermExpression::Raise { value, .. } => {
            buf.write("raise ");
            write_term(buf, value);
        }
        TermExpression::If(stmt) => {
            buf.write("if ");
            write_term(buf, &stmt.condition);
            buf.write(" ");
            write_body(buf, &stmt.then_body);
            if let Some(else_body) = &stmt.else_body {
                buf.write(" else ");
                write_body(buf, else_body);
            }
        }
        TermExpression::IfLet(stmt) => {
            buf.write("if let ");
            write_pattern(buf, &stmt.pattern);
            buf.write(" = ");
            write_term(buf, &stmt.item);
            buf.write(" ");
            write_body(buf, &stmt.then_body);
            if let Some(else_body) = &stmt.else_body {
                buf.write(" else ");
                write_body(buf, else_body);
            }
        }
        TermExpression::Loop(stmt) => {
            buf.write("loop ");
            write_body(buf, &stmt.body);
        }
        TermExpression::LoopIn(stmt) => {
            buf.write("loop ");
            if let Some(pattern) = &stmt.pattern {
                write_pattern(buf, pattern);
                buf.write(" in ");
            }
            if let Some(iter) = &stmt.iterator {
                write_term(buf, iter);
                buf.write(" ");
            }
            if let Some(cond) = &stmt.condition {
                buf.write("if ");
                write_term(buf, cond);
                buf.write(" ");
            }
            write_body(buf, &stmt.body);
        }
        TermExpression::While(stmt) => {
            buf.write("while ");
            if let Some(cond) = &stmt.condition {
                write_term(buf, cond);
                buf.write(" ");
            }
            write_body(buf, &stmt.body);
        }
        TermExpression::WhileLet(stmt) => {
            buf.write("while let ");
            write_pattern(buf, &stmt.pattern);
            buf.write(" = ");
            write_term(buf, &stmt.scrutinee);
            if let Some(guard) = &stmt.guard {
                buf.write(" if ");
                write_term(buf, guard);
            }
            buf.write(" ");
            write_body(buf, &stmt.body);
        }
        TermExpression::Until(stmt) => {
            buf.write("until ");
            if let Some(cond) = &stmt.condition {
                write_term(buf, cond);
                buf.write(" ");
            }
            write_body(buf, &stmt.body);
        }
        TermExpression::UntilNot(stmt) => {
            buf.write("until not ");
            if let Some(cond) = &stmt.condition {
                write_term(buf, cond);
                buf.write(" ");
            }
            write_body(buf, &stmt.body);
        }
        TermExpression::Try(stmt) => {
            if stmt.is_forced {
                buf.write("try! ");
            }
            else if stmt.is_optional {
                buf.write("try? ");
            }
            else {
                buf.write("try ");
            }
            if let Some(ty) = &stmt.result_type {
                write_type(buf, ty);
                buf.write(" ");
            }
            write_body(buf, &stmt.body);
        }
        TermExpression::Match { scrutinee, arms, .. } => {
            buf.write("match ");
            write_term(buf, scrutinee);
            buf.write(" {");
            buf.newline();
            buf.indent();
            for arm in arms {
                write_arm(buf, arm);
                buf.newline();
            }
            buf.dedent();
            buf.write("}");
        }
        TermExpression::Catch { expr, arms, .. } => {
            buf.write("catch ");
            write_term(buf, expr);
            buf.write(" {");
            buf.newline();
            buf.indent();
            for arm in arms {
                write_arm(buf, arm);
            }
            buf.dedent();
            buf.write("}");
        }
        TermExpression::PostfixMatch { base, arms, .. } => {
            write_term(buf, base);
            buf.write(".match {");
            buf.newline();
            buf.indent();
            for arm in arms {
                write_arm(buf, arm);
                buf.newline();
            }
            buf.dedent();
            buf.write("}");
        }
        TermExpression::PostfixCatch { base, arms, .. } => {
            write_term(buf, base);
            buf.write(".catch {");
            buf.newline();
            buf.indent();
            for arm in arms {
                write_arm(buf, arm);
            }
            buf.dedent();
            buf.write("}");
        }
        TermExpression::TryPropagate { base, .. } => {
            write_term(buf, base);
            buf.write("?");
        }
        TermExpression::MacroInvoke { path, args, .. } => {
            buf.write("@");
            write_name_path(buf, path);
            buf.write("(");
            for (index, arg) in args.iter().enumerate() {
                if index > 0 {
                    buf.write(", ");
                }
                write_term(buf, arg);
            }
            buf.write(")");
        }
        TermExpression::Construct { path, fields, .. } => {
            write_name_path(buf, path);
            buf.write(" { ");
            for (i, (name, value)) in fields.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                buf.write(name);
                buf.write(": ");
                write_term(buf, value);
            }
            buf.write(" }");
        }
        TermExpression::Lambda { params, return_type, body, .. } => {
            buf.write("micro(");
            write_params(buf, params);
            buf.write(")");
            if let Some(ret) = return_type {
                buf.write(" -> ");
                write_type(buf, ret);
            }
            buf.write(" ");
            write_body(buf, body);
        }
        TermExpression::Block { body, .. } => write_body(buf, body),
        TermExpression::XmlMarkup { nodes, .. } => {
            for node in nodes {
                write_xg_node(buf, node);
            }
        }
        TermExpression::Template { nodes, .. } => {
            buf.write("t\"");
            // T-Grammar round-trip is best-effort via span-less reconstruction.
            buf.write("...\"");
            let _ = nodes;
        }
        TermExpression::AnonymousClass { is_value_type, parents, body, .. } => {
            buf.write(if *is_value_type { "structure" } else { "class" });
            if !parents.is_empty() {
                buf.write(": ");
                for (i, p) in parents.iter().enumerate() {
                    if i > 0 {
                        buf.write(", ");
                    }
                    write_inheritance(buf, p);
                }
            }
            buf.write(" ");
            write_object_body(buf, body);
        }
    }
}

fn write_subscript_item(buf: &mut FormatBuffer, item: &SubscriptItem) {
    match item {
        SubscriptItem::Index { term, .. } => write_term(buf, term),
        SubscriptItem::Slice { start, end, step, .. } => {
            if let Some(s) = start {
                write_term(buf, s);
            }
            buf.write(":");
            if let Some(e) = end {
                write_term(buf, e);
            }
            if let Some(st) = step {
                buf.write(":");
                write_term(buf, st);
            }
        }
    }
}

fn write_arm(buf: &mut FormatBuffer, arm: &ArmStatement) {
    match arm {
        ArmStatement::Case(c) => {
            buf.write("case ");
            if let Some(pattern) = &c.pattern {
                write_pattern(buf, pattern);
            }
            if let Some(guard) = &c.guard {
                buf.write(" if ");
                write_term(buf, guard);
            }
            buf.write(": ");
            write_body(buf, &c.body);
            buf.newline();
        }
        ArmStatement::Type(t) => {
            buf.write("type ");
            write_type(buf, &t.typing);
            if let Some(guard) = &t.guard {
                buf.write(" if ");
                write_term(buf, guard);
            }
            buf.write(": ");
            write_body(buf, &t.body);
            buf.newline();
        }
        ArmStatement::Else(_) => {
            buf.write("else:");
            buf.newline();
        }
    }
}

fn write_pattern(buf: &mut FormatBuffer, pattern: &PatternExpression) {
    match pattern {
        PatternExpression::Variable { name, .. } => buf.write(name),
        PatternExpression::Name { path, .. } => write_name_path(buf, path),
        PatternExpression::Wildcard { .. } => buf.write("_"),
        PatternExpression::Literal { literal, .. } => write_literal(buf, literal),
        PatternExpression::Tuple(t) => {
            buf.write("(");
            for (i, item) in t.items.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_pattern(buf, item);
            }
            buf.write(")");
        }
        PatternExpression::Extract(e) => {
            write_name_path(buf, &e.name);
            buf.write("(");
            for (i, f) in e.fields.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_pattern(buf, f);
            }
            buf.write(")");
        }
        PatternExpression::Object(o) => {
            if let Some(name) = &o.name {
                write_name_path(buf, name);
                buf.write(" ");
            }
            buf.write("{ ");
            for (i, f) in o.fields.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                buf.write(&f.name);
                buf.write(": ");
                write_pattern(buf, &f.pattern);
            }
            if let Some(rest) = &o.rest {
                if !o.fields.is_empty() {
                    buf.write(", ");
                }
                buf.write("...");
                buf.write(rest.as_str());
            }
            buf.write(" }");
        }
        PatternExpression::Array(a) => {
            buf.write("[");
            for (i, item) in a.prefix.iter().enumerate() {
                if i > 0 {
                    buf.write(", ");
                }
                write_pattern(buf, item);
            }
            if a.rest.is_some() || !a.suffix.is_empty() {
                if !a.prefix.is_empty() {
                    buf.write(", ");
                }
                buf.write("..");
                if let Some(rest) = &a.rest {
                    buf.write(rest.as_str());
                }
            }
            for (i, item) in a.suffix.iter().enumerate() {
                buf.write(", ");
                let _ = i;
                write_pattern(buf, item);
            }
            buf.write("]");
        }
        PatternExpression::Range { start, end, inclusive_end, .. } => {
            if let Some(s) = start {
                write_literal(buf, s);
            }
            buf.write(if *inclusive_end { "..=" } else { ".." });
            if let Some(e) = end {
                write_literal(buf, e);
            }
        }
        PatternExpression::TypedBind { name, ty, .. } => {
            buf.write(name);
            buf.write(" as ");
            write_name_path(buf, ty);
        }
        PatternExpression::Or(o) => {
            for (i, p) in o.patterns.iter().enumerate() {
                if i > 0 {
                    buf.write(" | ");
                }
                write_pattern(buf, p);
            }
        }
        PatternExpression::Bind { name, pattern, .. } => {
            buf.write(name);
            buf.write(" <- ");
            write_pattern(buf, pattern);
        }
        PatternExpression::Mut { pattern, .. } => {
            buf.write("mut ");
            write_pattern(buf, pattern);
        }
        PatternExpression::Pin { mutable, pattern, .. } => {
            buf.write("pin ");
            if *mutable {
                buf.write("mut ");
            }
            write_pattern(buf, pattern);
        }
    }
}

fn write_literal(buf: &mut FormatBuffer, lit: &LiteralExpression) {
    match lit {
        LiteralExpression::Integer(s) | LiteralExpression::Float(s) => buf.write(s),
        LiteralExpression::String(s) => write_string_literal(buf, s),
        LiteralExpression::Bool(true) => buf.write("true"),
        LiteralExpression::Bool(false) => buf.write("false"),
        LiteralExpression::Unit => buf.write("()"),
        LiteralExpression::Null => buf.write("null"),
    }
}

fn write_string_literal(buf: &mut FormatBuffer, s: &StringLiteral) {
    if let Some(prefix) = &s.prefix {
        buf.write(prefix);
    }
    let quote = if s.quote_count >= 3 { "\"\"\"" } else { "\"" };
    buf.write(quote);
    for seg in &s.segments {
        match seg {
            StringSegment::Text(t) => buf.write_raw(&escape_string(t)),
            StringSegment::Interpolation { expression, is_fluent } => {
                if *is_fluent {
                    buf.write("{$}");
                }
                else {
                    buf.write("{");
                    write_term(buf, expression);
                    buf.write("}");
                }
            }
        }
    }
    buf.write(quote);
}

fn write_xg_node(buf: &mut FormatBuffer, node: &XgNode) {
    match node {
        XgNode::Element(el) => write_xg_element(buf, el),
        XgNode::Text { parts, .. } => {
            for part in parts {
                match part {
                    XgTextPart::Static(t) => buf.write(t),
                    XgTextPart::Expression(e) => {
                        buf.write("{");
                        buf.write(e);
                        buf.write("}");
                    }
                }
            }
        }
        XgNode::Meta { .. } => buf.write("<% ... %>"),
    }
}

fn write_xg_element(buf: &mut FormatBuffer, el: &XgElement) {
    buf.write("<");
    buf.write(&el.tag);
    for (name, value) in &el.attrs {
        buf.write(" ");
        buf.write(name);
        buf.write("=");
        match value {
            XgAttrValue::Literal(v) => {
                buf.write("\"");
                buf.write(&escape_string(v));
                buf.write("\"");
            }
            XgAttrValue::Expression(e) => {
                buf.write("{");
                buf.write(e);
                buf.write("}");
            }
        }
    }
    if el.self_closing {
        buf.write(" />");
        return;
    }
    buf.write(">");
    for child in &el.children {
        write_xg_node(buf, child);
    }
    buf.write("</");
    buf.write(&el.tag);
    buf.write(">");
}

fn binary_op(op: &BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
        BinaryOperator::Add => "+",
        BinaryOperator::Sub => "-",
        BinaryOperator::Mul => "*",
        BinaryOperator::Div => "/",
        BinaryOperator::Rem => "%",
        BinaryOperator::Power => "^",
        BinaryOperator::Eq => "==",
        BinaryOperator::Ne => "!=",
        BinaryOperator::Lt => "<",
        BinaryOperator::Le => "<=",
        BinaryOperator::Gt => ">",
        BinaryOperator::Ge => ">=",
        BinaryOperator::Pipe => "|>",
        BinaryOperator::Shl => "<<",
        BinaryOperator::Shr => ">>",
        BinaryOperator::BitAnd => "&",
        BinaryOperator::BitOr => "|",
        BinaryOperator::Range => "..",
        BinaryOperator::RangeInclusive => "..=",
        BinaryOperator::RangeTo => "..<",
    }
}

fn escape_string(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            other => other.to_string(),
        })
        .collect()
}
