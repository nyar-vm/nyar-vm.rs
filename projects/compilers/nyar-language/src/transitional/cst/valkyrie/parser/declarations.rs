use crate::transitional::cst::valkyrie::{
    ast::{
        Annotations, AttributeArgument, AttributeDeclaration, AttributeItem, AttributeList, ClassDeclaration, ClassLikeKind, DeclarationBody,
        FlagsDeclaration, FlagsMemberDeclaration, FunctionDeclKind, FunctionDeclaration, FunctionParameter, FunctionStatement, IdentifierNode,
        ImplyAssociatedConstBinding, ImplyAssociatedTypeBinding, ImplyDeclaration, InheritanceItem, MacroAssignDeclaration, NamePath,
        NamespaceDeclaration, ObjectBody, ObjectFieldDeclaration, ObjectMethodDeclaration, ParameterBindingKind, ParameterPassingKind,
        ParameterVariadicKind, RootStatement, SumTypeKind, TestsDeclaration, TraitAssociatedConstDeclaration, TraitAssociatedTypeDeclaration,
        TraitDeclaration, TypeAliasDeclaration, UniteDeclaration, UniteVariantDeclaration, UsingStatement,
    },
    lexer::{Keyword, TokenKind},
};

use super::{DECLARATION_MODIFIERS, ParseError, Parser, span};

enum ParamListItem {
    Param(FunctionParameter),
    Lt,
    Gt,
}

fn finalize_parameter_binding_kinds(items: Vec<ParamListItem>) -> Result<Vec<FunctionParameter>, ParseError> {
    let mut params = Vec::new();
    let mut positional_only_end = None;
    let mut keyword_only_start = None;
    let mut saw_lt = false;
    let mut saw_gt = false;

    for item in items {
        match item {
            ParamListItem::Lt => {
                if saw_lt {
                    return Err(ParseError::invalid("parameter list may contain at most one '<' marker"));
                }
                if saw_gt {
                    return Err(ParseError::invalid("parameter list '<' marker must appear before '>'"));
                }
                saw_lt = true;
                positional_only_end = Some(params.len());
            }
            ParamListItem::Gt => {
                if saw_gt {
                    return Err(ParseError::invalid("parameter list may contain at most one '>' marker"));
                }
                if !saw_lt {
                    return Err(ParseError::invalid("parameter list '>' marker requires a preceding '<' marker"));
                }
                saw_gt = true;
                keyword_only_start = Some(params.len());
            }
            ParamListItem::Param(param) => params.push(param),
        }
    }

    if let (Some(end), Some(start)) = (positional_only_end, keyword_only_start) {
        if end > start {
            return Err(ParseError::invalid("parameter list '<' marker must appear before '>'"));
        }
    }

    for (index, param) in params.iter_mut().enumerate() {
        param.binding_kind = parameter_binding_kind(index, positional_only_end, keyword_only_start);
    }
    validate_variadic_params(&params)?;
    Ok(params)
}

fn validate_variadic_params(params: &[FunctionParameter]) -> Result<(), ParseError> {
    let mut saw_positional_rest = false;
    let mut saw_keyword_rest = false;
    for param in params {
        match param.variadic {
            ParameterVariadicKind::None => {}
            ParameterVariadicKind::PositionalRest => {
                if saw_positional_rest {
                    return Err(ParseError::invalid("parameter list may contain at most one '..' rest parameter"));
                }
                saw_positional_rest = true;
                if param.binding_kind == ParameterBindingKind::KeywordOnly {
                    return Err(ParseError::invalid("positional rest parameter '..' must appear before '>'"));
                }
                if param.binding_kind == ParameterBindingKind::PositionalOnly {
                    return Err(ParseError::invalid("positional rest parameter '..' must appear in the positional-or-keyword section"));
                }
            }
            ParameterVariadicKind::KeywordRest => {
                if saw_keyword_rest {
                    return Err(ParseError::invalid("parameter list may contain at most one '...' rest parameter"));
                }
                saw_keyword_rest = true;
                if param.binding_kind != ParameterBindingKind::KeywordOnly {
                    return Err(ParseError::invalid("keyword rest parameter '...' must appear after '>'"));
                }
            }
        }
        if param.is_mutable && param.default_value.is_some() {
            return Err(ParseError::invalid("mutable parameters cannot have default values"));
        }
    }
    Ok(())
}

fn parameter_binding_kind(index: usize, positional_only_end: Option<usize>, keyword_only_start: Option<usize>) -> ParameterBindingKind {
    match (positional_only_end, keyword_only_start) {
        (None, None) => ParameterBindingKind::PositionalOrKeyword,
        (Some(end), None) => {
            if index < end {
                ParameterBindingKind::PositionalOnly
            }
            else {
                ParameterBindingKind::PositionalOrKeyword
            }
        }
        (None, Some(start)) => {
            if index >= start {
                ParameterBindingKind::KeywordOnly
            }
            else {
                ParameterBindingKind::PositionalOrKeyword
            }
        }
        (Some(end), Some(start)) => {
            if index < end {
                ParameterBindingKind::PositionalOnly
            }
            else if index >= start {
                ParameterBindingKind::KeywordOnly
            }
            else {
                ParameterBindingKind::PositionalOrKeyword
            }
        }
    }
}

impl<'a> Parser<'a> {
    pub(super) fn parse_annotations(&mut self) -> Result<Annotations, ParseError> {
        let mut attribute_lists = Vec::new();
        let mut modifiers = Vec::new();

        while self.check_symbol(TokenKind::LBracket) {
            attribute_lists.push(self.parse_attribute_list()?);
        }

        while let Some(text) = self.current_modifier_text() {
            if !DECLARATION_MODIFIERS.contains(&text) {
                break;
            }
            // Soft keywords may also be field names (`sealed: bool`, `static = ...`).
            // Do not consume them as modifiers when immediately followed by `:` / `=`.
            if matches!(self.peek().kind, TokenKind::Colon | TokenKind::Equal) {
                break;
            }
            let modifier_start = self.current().span.start;
            let modifier_text = text.to_string();
            self.bump();
            modifiers.push(IdentifierNode::new(nyar_types::Identifier::new(&modifier_text), span(modifier_start, self.previous().span.end)));
        }

        Ok(Annotations { documents: Vec::new(), attribute_lists, modifiers })
    }

    pub(super) fn parse_declaration(&mut self, annotations: Annotations) -> Result<RootStatement, ParseError> {
        if self.check_token_keyword(Keyword::Namespace) {
            return Ok(RootStatement::Namespace(self.parse_namespace()?));
        }
        if self.check_token_keyword(Keyword::Using) {
            return Ok(RootStatement::Using(self.parse_using()?));
        }
        if self.check_function_decl_keyword() {
            return Ok(RootStatement::Function(self.parse_function_declaration(annotations)?));
        }
        if self.check_class_like_keyword() {
            return Ok(RootStatement::Class(self.parse_class_like_declaration(annotations)?));
        }
        if self.check_token_keyword(Keyword::Trait) {
            return Ok(RootStatement::Trait(self.parse_trait_declaration(annotations)?));
        }
        if self.check_token_keyword(Keyword::Imply) {
            return Ok(RootStatement::Imply(self.parse_imply_declaration(annotations)?));
        }
        if self.check_sum_type_keyword() {
            return Ok(RootStatement::Unite(self.parse_sum_type_declaration(annotations)?));
        }
        if self.check_token_keyword(Keyword::Flags) {
            return Ok(RootStatement::Flags(self.parse_flags_declaration(annotations)?));
        }
        if self.check_identifier_text_eq("attribute") {
            return Ok(RootStatement::Attribute(self.parse_attribute_declaration(annotations)?));
        }
        if self.check_token_keyword(Keyword::Type) {
            return Ok(RootStatement::TypeAlias(self.parse_type_alias_declaration(annotations)?));
        }
        if self.check_token_keyword(Keyword::Macro) && self.nth_is_symbol(1, TokenKind::Identifier) {
            return Ok(RootStatement::MacroAssign(self.parse_macro_assign_declaration(annotations)?));
        }
        if self.check_token_keyword(Keyword::Tests) {
            return Ok(RootStatement::Tests(self.parse_tests_declaration(annotations)?));
        }
        Err(self.error_here("expected declaration"))
    }

    pub(super) fn check_function_decl_keyword(&self) -> bool {
        matches!(self.current().kind, TokenKind::Keyword(Keyword::Micro | Keyword::Mezzo | Keyword::Macro))
    }

    fn check_class_like_keyword(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Keyword(Keyword::Class | Keyword::Structure | Keyword::Widget | Keyword::Singleton | Keyword::Neural)
        )
    }

    fn check_sum_type_keyword(&self) -> bool {
        matches!(self.current().kind, TokenKind::Keyword(Keyword::Unite | Keyword::Union | Keyword::Enums))
    }

    fn parse_namespace(&mut self) -> Result<NamespaceDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Namespace)?.span.start;
        // 支持 `namespace!` 宏式语法，`!` 被消费后按普通 `namespace` 处理。
        self.match_symbol(TokenKind::Bang);
        // 源码使用 `.` 作为命名空间路径分隔符（如 `namespace std.data.text.wit;`）。
        let name = self.parse_dotted_name_path()?;
        let body = if self.check_symbol(TokenKind::LBrace) {
            let open_start = self.bump().span.start;
            let mut statements = Vec::new();
            let tail_expression = None;

            while !self.check_symbol(TokenKind::RBrace) {
                if self.is_eof() {
                    return Err(ParseError::invalid("unterminated namespace body"));
                }
                if self.match_symbol(TokenKind::Semicolon) {
                    continue;
                }
                let decl = self.parse_declaration(Annotations::default())?;
                match decl {
                    RootStatement::Function(func) => {
                        let path = NamePath { parts: vec![func.name.as_str().to_string()], span: func.span.clone() };
                        statements.push(FunctionStatement::Term {
                            span: func.span.clone(),
                            expression: crate::transitional::cst::valkyrie::ast::TermExpression::Name { path, span: func.span.clone() },
                        });
                    }
                    _ => {
                        return Err(self.error_here("unexpected declaration in namespace body"));
                    }
                }
            }
            let close_end = self.expect_symbol(TokenKind::RBrace)?.span.end;
            Some(DeclarationBody { statements, tail_expression, span: span(open_start, close_end) })
        }
        else {
            self.expect_symbol(TokenKind::Semicolon)?;
            None
        };
        Ok(NamespaceDeclaration { name, body, span: span(start, self.previous().span.end) })
    }

    fn parse_using(&mut self) -> Result<UsingStatement, ParseError> {
        use crate::transitional::cst::valkyrie::ast::UsingImportItem;
        let start = self.expect_token_keyword(Keyword::Using)?.span.start;
        // 支持 `using!` 宏式语法，`!` 被消费后按普通 `using` 处理。
        self.match_symbol(TokenKind::Bang);
        let path = self.parse_dotted_name_path()?;

        // 支持选择性导入：`using a.b.{C, D};` / `using a::b::{C, D as E};`
        // 支持通配导入：`using a.b.*;` / `using a::b::*;`
        // 同时保留裸导入：`using a.b;` / `using a.b as Alias;`
        let mut selective_imports = Vec::new();
        let mut glob_import = false;
        let mut alias = None;
        if self.match_symbol(TokenKind::Dot) || self.match_symbol(TokenKind::DoubleColon) {
            if self.match_symbol(TokenKind::LBrace) {
                selective_imports = self.parse_comma_separated_until(TokenKind::RBrace, |parser| {
                    let item_start = parser.current().span.start;
                    let name = parser.expect_member_name_text()?;
                    let item_alias = if parser.match_token_keyword(Keyword::As) { Some(parser.expect_member_name_text()?) } else { None };
                    Ok(UsingImportItem { name, alias: item_alias, span: span(item_start, parser.previous().span.end) })
                })?;
                self.expect_symbol(TokenKind::RBrace)?;
            }
            else if self.match_symbol(TokenKind::Star) {
                glob_import = true;
            }
            else {
                return Err(self.error_here("expected `{` or `*` after `using <module>.` / `using <module>::`"));
            }
        }
        else if self.match_token_keyword(Keyword::As) {
            alias = Some(self.expect_member_name_text()?);
        }

        // 分号可选：`using!` 宏式语法常省略分号，以下一个声明边界作为隐式终止。
        self.match_symbol(TokenKind::Semicolon);
        Ok(UsingStatement { path, alias, selective_imports, glob_import, span: span(start, self.previous().span.end) })
    }

    pub(super) fn parse_function_declaration(&mut self, annotations: Annotations) -> Result<FunctionDeclaration, ParseError> {
        let kind = match self.current().kind {
            TokenKind::Keyword(Keyword::Mezzo) => FunctionDeclKind::Mezzo,
            TokenKind::Keyword(Keyword::Macro) => FunctionDeclKind::Macro,
            _ => FunctionDeclKind::Micro,
        };
        let start = self.bump().span.start;
        let name_start = self.current().span.start;
        // 允许关键字作函数名（如 `micro yield()`、`micro match()`）。
        let name = self.expect_member_name_text()?;
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        let params = self.parse_parameter_list()?;
        let return_type = if self.match_symbol(TokenKind::Arrow) || self.match_symbol(TokenKind::Colon) {
            Some(self.parse_intersection_type_expression()?)
        }
        else {
            None
        };

        let where_constraints = self.parse_where_constraints()?;

        let signature_end = self.current().span.start;
        let body = if self.check_symbol(TokenKind::LBrace) {
            Some(self.parse_block_body()?)
        }
        else if self.match_symbol(TokenKind::Equal) {
            let expr = self.parse_expression_bp(0)?;
            let end = expr.span().end;
            Some(DeclarationBody { statements: Vec::new(), tail_expression: Some(expr), span: span(signature_end, end) })
        }
        else {
            self.expect_implicit_or_explicit_terminator(&[
                "micro",
                "mezzo",
                "macro",
                "class",
                "structure",
                "widget",
                "singleton",
                "trait",
                "imply",
                "unite",
                "union",
                "enums",
                "flags",
                "namespace",
                "using",
                "tests",
            ])?;
            None
        };

        Ok(FunctionDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            annotations,
            signature: self.slice(span(start, signature_end)).trim().to_string(),
            params,
            return_type,
            body,
            kind,
            generic_parameters,
            where_constraints,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_class_like_declaration(&mut self, annotations: Annotations) -> Result<ClassDeclaration, ParseError> {
        let (start, kind) = match self.current().kind {
            TokenKind::Keyword(Keyword::Structure) => (self.expect_token_keyword(Keyword::Structure)?.span.start, ClassLikeKind::Structure),
            TokenKind::Keyword(Keyword::Widget) => (self.expect_token_keyword(Keyword::Widget)?.span.start, ClassLikeKind::Widget),
            TokenKind::Keyword(Keyword::Singleton) => (self.expect_token_keyword(Keyword::Singleton)?.span.start, ClassLikeKind::Singleton),
            TokenKind::Keyword(Keyword::Neural) => (self.expect_token_keyword(Keyword::Neural)?.span.start, ClassLikeKind::Neural),
            _ => (self.expect_token_keyword(Keyword::Class)?.span.start, ClassLikeKind::Class),
        };
        let name_start = self.current().span.start;
        let name = self.expect_member_name_text()?;
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        let inheritance = if self.match_symbol(TokenKind::LParen) {
            let items = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_inheritance_item())?;
            self.expect_symbol(TokenKind::RParen)?;
            items
        }
        else if self.match_symbol(TokenKind::Colon) {
            self.parse_trait_inheritance_list()?
        }
        else {
            Vec::new()
        };
        let is_sealed_header = annotations.modifiers.iter().any(|modifier| modifier.as_str() == "sealed");
        let body = if self.check_symbol(TokenKind::LBrace) {
            self.parse_object_body(true, is_sealed_header)?
        }
        else if is_sealed_header {
            ObjectBody::default()
        }
        else {
            self.parse_object_body(true, false)?
        };
        Ok(ClassDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            annotations,
            inheritance,
            body,
            is_value_type: kind.is_value_type(),
            kind,
            generic_parameters,
            span: span(start, self.previous().span.end),
        })
    }

    #[allow(dead_code)]
    fn parse_class_declaration(&mut self, annotations: Annotations) -> Result<ClassDeclaration, ParseError> {
        self.parse_class_like_declaration(annotations)
    }

    fn parse_trait_declaration(&mut self, annotations: Annotations) -> Result<TraitDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Trait)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_name_text()?;
        let generic_parameters = self.parse_generic_parameter_clause()?;
        if self.match_symbol(TokenKind::Equal) {
            let alias_targets = self.parse_trait_inheritance_list()?;
            self.expect_implicit_or_explicit_terminator(&["namespace", "using", "micro", "class", "trait", "imply", "unite"])?;
            return Ok(TraitDeclaration {
                name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
                annotations,
                generic_parameters,
                inheritance: Vec::new(),
                alias_targets,
                is_alias: true,
                body: ObjectBody::default(),
                span: span(start, self.previous().span.end),
            });
        }

        let inheritance = if self.match_symbol(TokenKind::Colon) { self.parse_trait_inheritance_list()? } else { Vec::new() };
        let body = self.parse_object_body(false, false)?;
        Ok(TraitDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            annotations,
            generic_parameters,
            inheritance,
            alias_targets: Vec::new(),
            is_alias: false,
            body,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_imply_declaration(&mut self, annotations: Annotations) -> Result<ImplyDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Imply)?.span.start;
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        let target_type = self.parse_intersection_type_expression()?;
        let trait_type = if self.match_symbol(TokenKind::Colon) { Some(self.parse_intersection_type_expression()?) } else { None };
        let where_constraints = self.parse_where_constraints()?;
        let (methods, associated_type_bindings, associated_const_bindings) = self.parse_imply_body()?;

        Ok(ImplyDeclaration {
            annotations,
            generic_parameters,
            target_type,
            trait_type,
            where_constraints,
            methods,
            associated_type_bindings,
            associated_const_bindings,
            span: span(start, self.previous().span.end),
        })
    }

    pub(super) fn parse_object_body(&mut self, allow_fields: bool, is_sealed: bool) -> Result<ObjectBody, ParseError> {
        self.expect_symbol(TokenKind::LBrace)?;
        let mut script_statements = Vec::new();
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut associated_types = Vec::new();
        let mut associated_constants = Vec::new();
        let mut variants = Vec::new();

        while !self.check_symbol(TokenKind::RBrace) {
            let annotations = self.parse_annotations()?;
            if self.check_symbol(TokenKind::RBrace) {
                break;
            }

            if is_sealed
                && allow_fields
                && matches!(self.current().kind, TokenKind::Identifier)
                && self.looks_like_sealed_variant_declaration()
                && !self.check_token_keyword(Keyword::Micro)
                && !self.check_identifier_text_eq("get")
                && !self.check_identifier_text_eq("set")
                && !self.check_identifier_text_eq("on")
                && !annotations.modifiers.iter().any(|modifier| modifier.as_str() == "abstract" || modifier.as_str() == "override")
            {
                variants.push(self.parse_unite_variant(annotations)?);
                continue;
            }

            if allow_fields && (self.check_token_keyword(Keyword::Let) || self.check_token_keyword(Keyword::Mut)) {
                script_statements.push(self.parse_let_statement(annotations)?);
                continue;
            }

            if self.check_token_keyword(Keyword::Micro)
                || self.check_function_decl_keyword()
                || self.check_identifier_text_eq("infix")
                || self.check_identifier_text_eq("prefix")
                || self.check_identifier_text_eq("postfix")
                || self.check_identifier_text_eq("suffix")
                || self.check_identifier_text_eq("get")
                || self.check_identifier_text_eq("set")
                || self.check_identifier_text_eq("on")
                || annotations.modifiers.iter().any(|modifier| modifier.as_str() == "get" || modifier.as_str() == "set")
            {
                methods.push(self.parse_object_method_declaration(annotations)?);
                continue;
            }

            if !allow_fields && self.check_token_keyword(Keyword::Type) {
                associated_types.push(self.parse_trait_associated_type(annotations)?);
                continue;
            }

            if allow_fields && self.check_field_name_start() {
                if matches!(self.current().kind, TokenKind::Identifier) && matches!(self.peek().kind, TokenKind::LParen) {
                    methods.push(self.parse_object_method_declaration(annotations)?);
                }
                else {
                    fields.push(self.parse_object_field(annotations)?);
                }
                continue;
            }

            if allow_fields && self.check_token_keyword(Keyword::Type) && !matches!(self.peek().kind, TokenKind::Colon) {
                associated_types.push(self.parse_class_associated_type_binding(annotations)?);
                continue;
            }

            if !allow_fields && self.check_token_keyword(Keyword::Const) {
                associated_constants.push(self.parse_trait_associated_const(annotations)?);
                continue;
            }

            if allow_fields && self.check_identifier_text_eq("on") {
                methods.push(self.parse_object_method_declaration(annotations)?);
                continue;
            }

            if allow_fields {
                if !self.check_symbol(TokenKind::RBrace)
                    && !self.check_token_keyword(Keyword::Micro)
                    && !self.check_function_decl_keyword()
                    && !self.check_token_keyword(Keyword::Let)
                    && !self.check_token_keyword(Keyword::Mut)
                    && !self.check_field_name_start()
                {
                    if !annotations.attribute_lists.is_empty() || !annotations.modifiers.is_empty() {
                        return Err(self.error_here("dangling attributes before expression statement"));
                    }
                    let expr = self.parse_expression_bp(0)?;
                    let expr_span = expr.span().clone();
                    self.match_symbol(TokenKind::Semicolon);
                    script_statements.push(FunctionStatement::Term { expression: expr, span: expr_span });
                    continue;
                }
            }

            if !annotations.attribute_lists.is_empty() || !annotations.modifiers.is_empty() {
                return Err(self.error_here("dangling attributes before object member"));
            }

            // 当不允许字段时（trait 体），标识符是方法。
            if !allow_fields
                && matches!(self.current().kind, TokenKind::Identifier)
                && !self.check_token_keyword(Keyword::Type)
                && !self.check_token_keyword(Keyword::Const)
            {
                methods.push(self.parse_object_method_declaration(annotations)?);
                continue;
            }

            return Err(self.error_here("expected object field or method"));
        }

        self.expect_symbol(TokenKind::RBrace)?;
        Ok(ObjectBody { script_statements, fields, methods, associated_types, associated_constants, variants })
    }

    /// Lookahead: sealed ADT variant declaration (`Number { value: f64 }`, `None`), not a method.
    fn looks_like_sealed_variant_declaration(&self) -> bool {
        if !matches!(self.current().kind, TokenKind::Identifier) {
            return false;
        }

        match self.peek().kind {
            TokenKind::LBrace | TokenKind::Colon | TokenKind::Comma | TokenKind::Semicolon => return true,
            TokenKind::LParen => return self.looks_like_sealed_variant_payload(),
            _ => return false,
        }
    }

    /// Lookahead: `Ident(...)` is a sealed ADT variant payload (`Number(f64)`), not a method
    /// (`initiate(mut self, …) { … }` / `compute(x) -> i32`).
    fn looks_like_sealed_variant_payload(&self) -> bool {
        if !matches!(self.current().kind, TokenKind::Identifier) || !matches!(self.peek().kind, TokenKind::LParen) {
            return false;
        }

        let name = self.slice(self.current().span.clone());
        if name == "initiate" || name == "constructor" {
            return false;
        }

        // Method receiver / ownership: `mut` / `ref` / `own` / `self` / `Self`.
        if let Some(first) = self.tokens.get(self.index + 2) {
            match &first.kind {
                TokenKind::Keyword(Keyword::Mut | Keyword::Ref | Keyword::Own | Keyword::KwSelf | Keyword::KwSelfType) => {
                    return false;
                }
                TokenKind::Identifier => {
                    let text = self.slice(first.span.clone());
                    if text == "self" || text == "Self" {
                        return false;
                    }
                }
                _ => {}
            }
        }

        // Balance to the matching `)`; method members continue with `{` or `->`.
        let mut depth = 0usize;
        let mut i = self.index + 1;
        let mut after_paren = None;
        while let Some(token) = self.tokens.get(i) {
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        after_paren = Some(i + 1);
                        break;
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
            i += 1;
        }

        if let Some(after) = after_paren {
            if let Some(next) = self.tokens.get(after) {
                if matches!(next.kind, TokenKind::LBrace | TokenKind::Arrow) {
                    return false;
                }
            }
        }

        true
    }

    fn parse_object_field(&mut self, annotations: Annotations) -> Result<ObjectFieldDeclaration, ParseError> {
        let start = self.current().span.start;
        // `static` is a field-level modifier only when followed by a name (`static foo: T`).
        // A bare `static: T` is the field name itself.
        if self.check_identifier_text_eq("static") && !matches!(self.peek().kind, TokenKind::Colon | TokenKind::Equal) {
            self.bump();
        }
        let name_start = self.current().span.start;
        let name_text = self.expect_member_name_text()?;
        let name = IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end));
        self.expect_symbol(TokenKind::Colon)?;
        let field_type = self.parse_intersection_type_expression()?;
        let default_value = if self.match_symbol(TokenKind::Equal) {
            let value = self.parse_expression_bp(0)?;
            Some(value)
        }
        else {
            None
        };
        // 对象字段可省略分号，以换行分隔。下一个字段以标识符/关键字字段开头，或以 `}` 结束。
        // 也支持逗号分隔的字段列表。
        if !self.match_symbol(TokenKind::Semicolon)
            && !self.match_symbol(TokenKind::Comma)
            && !self.check_symbol(TokenKind::RBrace)
            && !self.is_eof()
            && !self.check_symbol(TokenKind::LBracket)
            && !self.check_field_name_start()
            && !self.check_function_decl_keyword()
            && !self.check_identifier_text_eq("get")
            && !self.check_identifier_text_eq("set")
        {
            return Err(self.error_here("expected ';' or '}' or next field after object field"));
        }

        Ok(ObjectFieldDeclaration { annotations, name, field_type, default_value, span: span(start, self.previous().span.end) })
    }

    pub(super) fn parse_object_method_declaration(&mut self, annotations: Annotations) -> Result<ObjectMethodDeclaration, ParseError> {
        let is_event_handler = self.check_identifier_text_eq("on");
        if is_event_handler {
            let start = self.current().span.start;
            self.bump();
            let event_start = self.current().span.start;
            let event_name = self.expect_member_name_text()?;
            let method_name = format!("on_{event_name}");
            let body = Some(self.parse_block_body()?);
            let end = self.previous().span.end;
            return Ok(ObjectMethodDeclaration {
                generic_parameters: Vec::new(),
                where_constraints: Vec::new(),
                name: IdentifierNode::new(nyar_types::Identifier::new(&method_name), span(event_start, end)),
                annotations,
                signature: format!("on {event_name}"),
                params: Vec::new(),
                return_type: None,
                body,
                span: span(start, end),
            });
        }

        let is_accessor = annotations.modifiers.iter().any(|modifier| modifier.as_str() == "get" || modifier.as_str() == "set")
            || self.check_identifier_text_eq("get")
            || self.check_identifier_text_eq("set");
        if is_accessor && (self.check_identifier_text_eq("get") || self.check_identifier_text_eq("set")) {
            self.bump();
        }
        let is_operator = self.check_identifier_text_eq("infix")
            || self.check_identifier_text_eq("prefix")
            || self.check_identifier_text_eq("postfix")
            || self.check_identifier_text_eq("suffix");
        // 支持不带 `micro` 前缀的普通方法名（如 `bit_and(self, ...)`）。
        let is_plain_method = matches!(self.current().kind, TokenKind::Identifier)
            && !self.check_token_keyword(Keyword::Micro)
            && !self.check_token_keyword(Keyword::Type)
            && !self.check_token_keyword(Keyword::Const)
            && !self.check_identifier_text_eq("infix")
            && !self.check_identifier_text_eq("prefix")
            && !self.check_identifier_text_eq("postfix")
            && !self.check_identifier_text_eq("suffix");
        let start = if is_accessor || is_operator || is_plain_method {
            self.current().span.start
        }
        else {
            self.expect_token_keyword(Keyword::Micro)?.span.start
        };
        if self.check_identifier_text_eq("static") {
            self.bump();
        }
        let name = if is_accessor {
            let name_start = self.current().span.start;
            let name_text = self.expect_member_name_text()?.to_string();
            IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end))
        }
        else {
            let parsed_name = self.parse_method_name()?;
            IdentifierNode::new(nyar_types::Identifier::new(&parsed_name), span(start, self.previous().span.end))
        };
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        let params = self.parse_parameter_list()?;
        let return_type = if self.match_symbol(TokenKind::Arrow) || self.match_symbol(TokenKind::Colon) {
            Some(self.parse_intersection_type_expression()?)
        }
        else {
            None
        };

        let where_constraints = self.parse_where_constraints()?;

        let signature_end = self.current().span.start;
        let body = if self.check_symbol(TokenKind::LBrace) {
            Some(self.parse_block_body()?)
        }
        else {
            self.expect_implicit_or_explicit_terminator(&["micro", "type", "const", "infix", "prefix", "postfix", "suffix"])?;
            None
        };

        Ok(ObjectMethodDeclaration {
            generic_parameters,
            where_constraints,
            name,
            annotations,
            signature: self.slice(span(start, signature_end)).trim().to_string(),
            params,
            return_type,
            body,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_method_name(&mut self) -> Result<String, ParseError> {
        if self.check_identifier_text_eq("infix")
            || self.check_identifier_text_eq("prefix")
            || self.check_identifier_text_eq("postfix")
            || self.check_identifier_text_eq("suffix")
        {
            let fixity = self.expect_identifier_text()?.to_string();
            let operator = self.parse_operator_method_symbol()?;
            return Ok(format!("{fixity} {operator}"));
        }

        Ok(self.expect_member_name_text()?)
    }

    /// 解析运算符方法符号。
    ///
    /// 支持两种形式：
    /// - 预定义运算符 token（如 `+`、`==`）
    /// - 反引号包裹的任意运算符（如 `` `+=` ``、`` `>>=` ``）
    fn parse_operator_method_symbol(&mut self) -> Result<String, ParseError> {
        if matches!(self.current().kind, TokenKind::BacktickSymbol) {
            let span = self.bump().span.clone();
            let text = self.slice(span);
            // 剥离首尾反引号，提取运算符文本。
            let trimmed = text.trim_start_matches('`').trim_end_matches('`');
            return Ok(trimmed.to_string());
        }

        let operator = match self.current().kind {
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Bang => "!",
            TokenKind::EqEq => "==",
            TokenKind::NotEq => "!=",
            TokenKind::LAngle => "<",
            TokenKind::RAngle => ">",
            TokenKind::LessEq => "<=",
            TokenKind::GreaterEq => ">=",
            _ => {
                return Err(self.error_here("expected operator symbol"));
            }
        };
        self.bump();
        Ok(operator.to_string())
    }

    fn parse_imply_body(
        &mut self,
    ) -> Result<(Vec<ObjectMethodDeclaration>, Vec<ImplyAssociatedTypeBinding>, Vec<ImplyAssociatedConstBinding>), ParseError> {
        self.expect_symbol(TokenKind::LBrace)?;
        let mut methods = Vec::new();
        let mut associated_type_bindings = Vec::new();
        let mut associated_const_bindings = Vec::new();

        while !self.check_symbol(TokenKind::RBrace) {
            let annotations = self.parse_annotations()?;
            if self.check_symbol(TokenKind::RBrace) {
                break;
            }

            if self.check_token_keyword(Keyword::Micro)
                || self.check_function_decl_keyword()
                || self.check_identifier_text_eq("infix")
                || self.check_identifier_text_eq("prefix")
                || self.check_identifier_text_eq("postfix")
                || self.check_identifier_text_eq("suffix")
                || (matches!(self.current().kind, TokenKind::Identifier)
                    && !self.check_token_keyword(Keyword::Type)
                    && !self.check_token_keyword(Keyword::Const))
            {
                methods.push(self.parse_object_method_declaration(annotations)?);
                continue;
            }

            if self.check_token_keyword(Keyword::Type) {
                associated_type_bindings.push(self.parse_imply_associated_type_binding(annotations)?);
                continue;
            }

            if self.check_token_keyword(Keyword::Const) {
                associated_const_bindings.push(self.parse_imply_associated_const_binding(annotations)?);
                continue;
            }

            return Err(self.error_here("expected imply method or associated member binding"));
        }

        self.expect_symbol(TokenKind::RBrace)?;
        Ok((methods, associated_type_bindings, associated_const_bindings))
    }

    fn parse_imply_associated_type_binding(&mut self, annotations: Annotations) -> Result<ImplyAssociatedTypeBinding, ParseError> {
        let start = self.expect_token_keyword(Keyword::Type)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        self.expect_symbol(TokenKind::Equal)?;
        let concrete_type = self.parse_intersection_type_expression()?;
        self.expect_implicit_or_explicit_terminator(&["micro", "type", "const"])?;

        Ok(ImplyAssociatedTypeBinding {
            annotations,
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            generic_parameters,
            concrete_type,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_imply_associated_const_binding(&mut self, annotations: Annotations) -> Result<ImplyAssociatedConstBinding, ParseError> {
        let start = self.expect_token_keyword(Keyword::Const)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        let const_type = if self.match_symbol(TokenKind::Colon) { Some(self.parse_intersection_type_expression()?) } else { None };
        self.expect_symbol(TokenKind::Equal)?;
        let value = self.parse_expression_bp(0)?;
        self.expect_implicit_or_explicit_terminator(&["micro", "type", "const"])?;

        Ok(ImplyAssociatedConstBinding {
            annotations,
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            const_type,
            value,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_class_associated_type_binding(&mut self, annotations: Annotations) -> Result<TraitAssociatedTypeDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Type)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        let generic_parameters = self.parse_generic_parameter_clause()?;
        self.expect_symbol(TokenKind::Equal)?;
        let concrete_type = self.parse_intersection_type_expression()?;
        if !self.match_symbol(TokenKind::Semicolon)
            && !self.match_symbol(TokenKind::Comma)
            && !self.check_symbol(TokenKind::RBrace)
            && !self.is_eof()
            && !self.check_symbol(TokenKind::LBracket)
            && !self.check_field_name_start()
            && !self.check_function_decl_keyword()
            && !self.check_identifier_text_eq("get")
            && !self.check_identifier_text_eq("set")
            && !self.check_identifier_text_eq("on")
            && !self.check_token_keyword(Keyword::Type)
            && !self.check_token_keyword(Keyword::Micro)
        {
            return Err(self.error_here("expected ';' or '}' or next member after associated type binding"));
        }

        Ok(TraitAssociatedTypeDeclaration {
            annotations,
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            generic_parameters,
            bounds: Vec::new(),
            default_type: Some(concrete_type),
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_trait_associated_type(&mut self, annotations: Annotations) -> Result<TraitAssociatedTypeDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Type)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        let generic_parameters = self.parse_generic_parameter_clause()?;
        let bounds = if self.match_symbol(TokenKind::Colon) { self.parse_trait_bound_list()? } else { Vec::new() };
        let default_type = if self.match_symbol(TokenKind::Equal) { Some(self.parse_intersection_type_expression()?) } else { None };
        self.expect_implicit_or_explicit_terminator(&["micro", "type", "const"])?;

        Ok(TraitAssociatedTypeDeclaration {
            annotations,
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            generic_parameters,
            bounds,
            default_type,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_trait_associated_const(&mut self, annotations: Annotations) -> Result<TraitAssociatedConstDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Const)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        self.expect_symbol(TokenKind::Colon)?;
        let const_type = self.parse_intersection_type_expression()?;
        let default_value = if self.match_symbol(TokenKind::Equal) { Some(self.parse_expression_bp(0)?) } else { None };
        self.expect_implicit_or_explicit_terminator(&["micro", "type", "const"])?;

        Ok(TraitAssociatedConstDeclaration {
            annotations,
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            const_type,
            default_value,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_sum_type_declaration(&mut self, annotations: Annotations) -> Result<UniteDeclaration, ParseError> {
        let kind = match self.current().kind {
            TokenKind::Keyword(Keyword::Union) => SumTypeKind::Union,
            TokenKind::Keyword(Keyword::Enums) => SumTypeKind::Enum,
            _ => SumTypeKind::Unite,
        };
        let start = self.bump().span.start;
        let name_start = self.current().span.start;
        let name_text = self.expect_identifier_text()?.to_string();
        let name = IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end));
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        if self.match_symbol(TokenKind::Colon) {
            let _ = self.parse_trait_inheritance_list()?;
        }
        self.expect_symbol(TokenKind::LBrace)?;
        let mut variants = Vec::new();

        while !self.check_symbol(TokenKind::RBrace) {
            let variant_annotations = self.parse_annotations()?;
            if self.check_symbol(TokenKind::RBrace) {
                break;
            }
            variants.push(self.parse_unite_variant(variant_annotations)?);
        }

        self.expect_symbol(TokenKind::RBrace)?;
        Ok(UniteDeclaration { name, annotations, generic_parameters, variants, kind, span: span(start, self.previous().span.end) })
    }

    fn parse_unite_declaration(&mut self, annotations: Annotations) -> Result<UniteDeclaration, ParseError> {
        self.parse_sum_type_declaration(annotations)
    }

    fn parse_flags_declaration(&mut self, annotations: Annotations) -> Result<FlagsDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Flags)?.span.start;
        let name_start = self.current().span.start;
        let name_text = self.expect_identifier_text()?.to_string();
        let name = IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end));
        if self.match_symbol(TokenKind::Colon) {
            let _ = self.parse_trait_inheritance_list()?;
        }
        self.expect_symbol(TokenKind::LBrace)?;
        let mut members = Vec::new();
        while !self.check_symbol(TokenKind::RBrace) {
            let member_annotations = self.parse_annotations()?;
            if self.check_symbol(TokenKind::RBrace) {
                break;
            }
            members.push(self.parse_flags_member(member_annotations)?);
        }
        self.expect_symbol(TokenKind::RBrace)?;
        Ok(FlagsDeclaration { name, annotations, inheritance: Vec::new(), members, span: span(start, self.previous().span.end) })
    }

    fn parse_flags_member(&mut self, annotations: Annotations) -> Result<FlagsMemberDeclaration, ParseError> {
        let start = self.current().span.start;
        let name_start = self.current().span.start;
        let name_text = self.expect_identifier_text()?.to_string();
        let value = if self.match_symbol(TokenKind::Equal) { Some(self.parse_expression_bp(0)?) } else { None };
        self.match_symbol(TokenKind::Comma);
        self.match_symbol(TokenKind::Semicolon);
        Ok(FlagsMemberDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end)),
            annotations,
            value,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_macro_assign_declaration(&mut self, annotations: Annotations) -> Result<MacroAssignDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Macro)?.span.start;
        let name_start = self.current().span.start;
        let name_text = self.expect_identifier_text()?.to_string();
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        self.expect_symbol(TokenKind::Equal)?;
        let value = self.parse_expression_bp(0)?;
        self.match_symbol(TokenKind::Semicolon);
        Ok(MacroAssignDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end)),
            annotations,
            generic_parameters,
            value,
            span: span(start, self.previous().span.end),
        })
    }

    fn parse_tests_declaration(&mut self, annotations: Annotations) -> Result<TestsDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Tests)?.span.start;
        let body = self.parse_block_body()?;
        Ok(TestsDeclaration { annotations, body, span: span(start, self.previous().span.end) })
    }

    fn parse_unite_variant(&mut self, annotations: Annotations) -> Result<UniteVariantDeclaration, ParseError> {
        let start = self.current().span.start;
        let name_start = self.current().span.start;
        // `Some` / `None` 等关键字也可以作为合类型变体名。
        let name = self.expect_member_name_text()?;
        let result_type = if self.match_symbol(TokenKind::Colon) { Some(self.parse_intersection_type_expression()?) } else { None };
        let mut fields = Vec::new();

        if self.check_symbol(TokenKind::LParen) {
            let paren_span = self.current().span.clone();
            return Err(ParseError::invalid_at(
                "Expected `{` for variant body; `(T)` is not valid variant declaration syntax. \
                 Use record-style fields, e.g. `Some { value: T }`; use `Some(x)` for construction \
                 and `case Some(x)` for matching.",
                paren_span,
            ));
        }

        if self.match_symbol(TokenKind::LBrace) {
            while !self.check_symbol(TokenKind::RBrace) {
                let field_annotations = self.parse_annotations()?;
                if self.check_symbol(TokenKind::RBrace) {
                    break;
                }
                let field_start = self.current().span.start;
                let field_name_start = self.current().span.start;
                let field_name_text = self.expect_member_name_text()?;
                let field_name =
                    IdentifierNode::new(nyar_types::Identifier::new(&field_name_text), span(field_name_start, self.previous().span.end));
                self.expect_symbol(TokenKind::Colon)?;
                let field_type = self.parse_intersection_type_expression()?;
                let default_value = if self.match_symbol(TokenKind::Equal) { Some(self.parse_expression_bp(0)?) } else { None };
                if self.match_symbol(TokenKind::Comma) || self.match_symbol(TokenKind::Semicolon) {}
                fields.push(ObjectFieldDeclaration {
                    annotations: field_annotations,
                    name: field_name,
                    field_type,
                    default_value,
                    span: span(field_start, self.previous().span.end),
                });
            }
            self.expect_symbol(TokenKind::RBrace)?;
        }

        let value = if self.match_symbol(TokenKind::Equal) { Some(self.parse_expression_bp(0)?) } else { None };

        // 消费可选的尾随逗号或分号。
        self.match_symbol(TokenKind::Comma);
        self.match_symbol(TokenKind::Semicolon);

        Ok(UniteVariantDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            annotations,
            fields,
            result_type,
            value,
            span: span(start, self.previous().span.end),
        })
    }

    /// 解析 `attribute name;` 标记属性声明。
    ///
    /// 语法：`attribute <identifier>;`
    /// 用于声明可在类型上使用的标记属性。
    fn parse_attribute_declaration(&mut self, _annotations: Annotations) -> Result<AttributeDeclaration, ParseError> {
        let start = self.expect_identifier_text_eq("attribute")?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        self.expect_symbol(TokenKind::Semicolon)?;
        Ok(AttributeDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            span: span(start, self.previous().span.end),
        })
    }

    /// 解析 `type Name[<T, ...>] = Target;` 类型别名声明。
    fn parse_type_alias_declaration(&mut self, _annotations: Annotations) -> Result<TypeAliasDeclaration, ParseError> {
        let start = self.expect_token_keyword(Keyword::Type)?.span.start;
        let name_start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        let name_node = IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end));
        let generic_parameters = self.parse_structured_generic_parameter_clause()?;
        self.expect_symbol(TokenKind::Equal)?;
        let target = self.parse_intersection_type_expression()?;
        // 分号可选：某些源文件省略分号。
        self.match_symbol(TokenKind::Semicolon);
        Ok(TypeAliasDeclaration { name: name_node, generic_parameters, target, span: span(start, self.previous().span.end) })
    }

    fn parse_attribute_list(&mut self) -> Result<AttributeList, ParseError> {
        self.expect_symbol(TokenKind::LBracket)?;
        let items = self.parse_comma_separated_until(TokenKind::RBracket, |parser| parser.parse_attribute_item())?;
        self.expect_symbol(TokenKind::RBracket)?;
        Ok(AttributeList { items })
    }

    fn parse_attribute_item(&mut self) -> Result<AttributeItem, ParseError> {
        let start = self.current().span.start;
        let name = self.parse_name_path()?;
        let arguments = if self.match_symbol(TokenKind::LParen) {
            let args = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_attribute_argument())?;
            self.expect_symbol(TokenKind::RParen)?;
            args
        }
        else {
            Vec::new()
        };
        Ok(AttributeItem { name, arguments, span: span(start, self.previous().span.end) })
    }

    fn parse_attribute_argument(&mut self) -> Result<AttributeArgument, ParseError> {
        if self.check_attribute_named_argument_start() {
            let key = self.expect_member_name_text()?;
            if self.match_symbol(TokenKind::Equal) || self.match_symbol(TokenKind::Colon) {
                let value = self.parse_expression_bp(0)?;
                return Ok(AttributeArgument { key: Some(key), value });
            }
            return Err(self.error_here("expected '=' or ':' after attribute argument key"));
        }

        let value = self.parse_expression_bp(0)?;
        Ok(AttributeArgument { key: None, value })
    }

    pub(super) fn parse_call_argument(&mut self) -> Result<crate::transitional::cst::valkyrie::ast::TermCallArgument, ParseError> {
        use crate::transitional::cst::valkyrie::ast::TermCallArgument;
        if matches!(self.current().kind, TokenKind::Identifier) && self.nth_is_symbol(1, TokenKind::Equal) {
            let key = self.expect_identifier_text()?.to_string();
            self.expect_symbol(TokenKind::Equal)?;
            let value = self.parse_value_expression_bp(0)?;
            return Ok(TermCallArgument { key: Some(key), value });
        }

        let value = self.parse_value_expression_bp(0)?;
        Ok(TermCallArgument { key: None, value })
    }

    pub(super) fn parse_parameter_list(&mut self) -> Result<Vec<FunctionParameter>, ParseError> {
        self.parse_parameter_list_with_markers(true)
    }

    /// Lambda parameters do not support `<` / `>` binding markers.
    pub(super) fn parse_lambda_parameter_list(&mut self) -> Result<Vec<FunctionParameter>, ParseError> {
        self.parse_parameter_list_with_markers(false)
    }

    fn parse_parameter_list_with_markers(&mut self, allow_markers: bool) -> Result<Vec<FunctionParameter>, ParseError> {
        self.expect_symbol(TokenKind::LParen)?;
        let mut items = Vec::new();
        if !self.check_symbol(TokenKind::RParen) {
            loop {
                if self.match_symbol(TokenKind::LAngle) {
                    if !allow_markers {
                        return Err(self.error_here("lambda parameter lists do not support '<' markers"));
                    }
                    items.push(ParamListItem::Lt);
                }
                else if self.match_symbol(TokenKind::RAngle) {
                    if !allow_markers {
                        return Err(self.error_here("lambda parameter lists do not support '>' markers"));
                    }
                    items.push(ParamListItem::Gt);
                }
                else {
                    items.push(ParamListItem::Param(self.parse_parameter()?));
                }

                if self.match_symbol(TokenKind::Comma) {
                    if self.check_symbol(TokenKind::RParen) {
                        return Err(self.error_here("expected parameter or binding marker after ','"));
                    }
                    continue;
                }
                break;
            }
        }
        self.expect_symbol(TokenKind::RParen)?;
        finalize_parameter_binding_kinds(items)
    }

    fn parse_parameter(&mut self) -> Result<FunctionParameter, ParseError> {
        let start = self.current().span.start;
        let variadic = if self.match_symbol(TokenKind::Ellipsis) {
            ParameterVariadicKind::KeywordRest
        }
        else if self.match_symbol(TokenKind::DotDot) {
            ParameterVariadicKind::PositionalRest
        }
        else {
            ParameterVariadicKind::None
        };
        let passing = if self.match_token_keyword(Keyword::Own) {
            ParameterPassingKind::Own
        }
        else if self.match_token_keyword(Keyword::Mut) {
            ParameterPassingKind::Mut
        }
        else {
            self.match_token_keyword(Keyword::Ref);
            ParameterPassingKind::Ref
        };
        let is_mutable = matches!(passing, ParameterPassingKind::Mut);
        let name_start = self.current().span.start;
        // `self` / `Self` 为关键字，参数位置仍可作为绑定名。
        let name = self.expect_member_name_text()?;
        let parameter_type = if self.match_symbol(TokenKind::Colon) { Some(self.parse_intersection_type_expression()?) } else { None };
        let default_value = if self.match_symbol(TokenKind::Equal) { Some(self.parse_expression_bp(0)?) } else { None };

        Ok(FunctionParameter {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(name_start, self.previous().span.end)),
            parameter_type,
            is_mutable,
            passing,
            binding_kind: ParameterBindingKind::PositionalOrKeyword,
            default_value,
            variadic,
            span: span(start, self.previous().span.end),
        })
    }

    pub(super) fn parse_inheritance_item(&mut self) -> Result<InheritanceItem, ParseError> {
        let start = self.current().span.start;
        let alias = if matches!(self.current().kind, TokenKind::Identifier) && self.nth_is_symbol(1, TokenKind::Colon) {
            let value = self.expect_identifier_text()?.to_string();
            self.expect_symbol(TokenKind::Colon)?;
            Some(value)
        }
        else {
            None
        };
        let base_type = self.parse_intersection_type_expression()?;
        Ok(InheritanceItem { alias, base_type, span: span(start, self.previous().span.end) })
    }
}
