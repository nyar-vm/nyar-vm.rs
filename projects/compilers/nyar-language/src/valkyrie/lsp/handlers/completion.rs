use crate::{errors::LspResult, state::ServerState, types::Position};
use crate::handlers;
use oak_lsp::types::{CompletionItem, CompletionItemKind};
use oak_valkyrie::ast::{Block, Expr, Item, Pattern, Statement};

/// 补全上下文类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContext {
    /// 声明上下文（命名空间顶层、类体内等）
    Declaration,
    /// 表达式上下文（函数体内、表达式位置等）
    Expression,
    /// 命名空间成员访问上下文（using namespace:: 后）
    NamespaceAccess,
    /// 类型成员访问上下文（对象. 后）
    MemberAccess,
}

/// 补全处理器
pub struct CompletionHandler;

impl CompletionHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> LspResult<Vec<CompletionItem>> {
        if handlers::awsl::document::is_awsl_uri(uri) {
            return Ok(handlers::awsl::AwslCompletionHandler::handle(state, uri, position).await);
        }

        let doc = match state.documents.get(uri) {
            Some(d) => d.clone(),
            None => return Ok(Vec::new()),
        };

        let ast = match &doc.ast {
            Some(a) => a.clone(),
            None => return Ok(Vec::new()),
        };

        Self::complete_at(state, &doc, &ast, position).await
    }

    /// 对已解析文档在指定位置提供补全（供 AWSL script 委托复用）
    pub(crate) async fn complete_at(
        state: &ServerState,
        doc: &crate::state::DocumentState,
        ast: &oak_valkyrie::ast::ValkyrieRoot,
        position: Position,
    ) -> LspResult<Vec<CompletionItem>> {
        let offset = doc.position_to_offset(position);
        let text_before = if offset > 0 { &doc.text[..offset] } else { "" };

        let context = Self::determine_context(&ast.items, offset, text_before);

        let mut items = Vec::new();

        match context {
            CompletionContext::Declaration => {
                Self::add_declaration_keywords(&mut items);
                Self::add_symbols_from_workspace(state, &mut items).await;
            }
            CompletionContext::Expression => {
                Self::add_expression_keywords(&mut items);
                Self::add_symbols_from_workspace(state, &mut items).await;
                Self::add_local_symbols(&ast.items, offset, &mut items);
            }
            CompletionContext::NamespaceAccess => {
                if let Some(path) = Self::extract_namespace_before(text_before) {
                    if !path.contains("::") {
                        Self::add_type_members(state, &doc.uri, &path, &mut items).await;
                    }
                    if items.is_empty() {
                        Self::add_namespace_members(state, &path, &mut items);
                    }
                }
            }
            CompletionContext::MemberAccess => {
                if let Some(expr_text) = Self::extract_expression_before_dot(text_before) {
                    Self::add_type_members(state, &doc.uri, &expr_text, &mut items).await;
                }
            }
        }

        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.dedup_by(|a, b| a.label == b.label);

        Ok(items)
    }

    /// 确定补全上下文
    fn determine_context(items: &[Item], offset: usize, text_before: &str) -> CompletionContext {
        if Self::is_namespace_access_context(text_before) {
            return CompletionContext::NamespaceAccess;
        }

        if Self::is_member_access_context(text_before) {
            return CompletionContext::MemberAccess;
        }

        if Self::is_in_expression_context(items, offset) {
            return CompletionContext::Expression;
        }

        CompletionContext::Declaration
    }

    /// 检查是否在命名空间访问上下文（using namespace:: 后）
    fn is_namespace_access_context(text_before: &str) -> bool {
        let trimmed = text_before.trim_end();
        if trimmed.ends_with("::") {
            let before_colon = &trimmed[..trimmed.len() - 2];
            if let Some(last_space) = before_colon.rfind(|c: char| c.is_whitespace() || c == '(' || c == '{' || c == ';') {
                let word = &before_colon[last_space + 1..];
                if word.contains("::") || word.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return true;
                }
            }
            else if !before_colon.is_empty() {
                return true;
            }
        }
        false
    }

    /// 检查是否在成员访问上下文（. 后）
    fn is_member_access_context(text_before: &str) -> bool {
        let trimmed = text_before.trim_end();
        trimmed.ends_with('.')
    }

    /// 检查是否在表达式上下文中
    fn is_in_expression_context(items: &[Item], offset: usize) -> bool {
        for item in items {
            if Self::is_offset_in_item(item, offset) {
                return Self::check_item_for_expression_context(item, offset);
            }
        }
        false
    }

    /// 检查偏移量是否在 Item 范围内
    fn is_offset_in_item(item: &Item, offset: usize) -> bool {
        let span = match item {
            Item::Namespace(n) => &n.span,
            Item::Class(c) => &c.span,
            Item::Micro(m) => &m.span,
            Item::TypeFunction(t) => &t.span,
            Item::Widget(w) => &w.span,
            Item::Statement(s) => match s {
                Statement::Let { span, .. } => span,
                Statement::ExprStmt { span, .. } => span,
            },
            Item::Using(u) => &u.span,
            _ => return false,
        };
        offset >= span.start && offset <= span.end
    }

    /// 检查 Item 内部是否为表达式上下文
    fn check_item_for_expression_context(item: &Item, offset: usize) -> bool {
        match item {
            Item::Namespace(n) => {
                for child in &n.items {
                    if Self::is_offset_in_item(child, offset) {
                        return Self::check_item_for_expression_context(child, offset);
                    }
                }
                false
            }
            Item::Class(c) => {
                for child in &c.items {
                    if Self::is_offset_in_item(child, offset) {
                        return Self::check_item_for_expression_context(child, offset);
                    }
                }
                false
            }
            Item::Widget(w) => {
                for child in &w.items {
                    if Self::is_offset_in_item(child, offset) {
                        return Self::check_item_for_expression_context(child, offset);
                    }
                }
                false
            }
            Item::Micro(m) => {
                if offset >= m.body.span.start && offset <= m.body.span.end {
                    return true;
                }
                false
            }
            Item::TypeFunction(t) => {
                if offset >= t.body.span.start && offset <= t.body.span.end {
                    return true;
                }
                false
            }
            Item::Statement(s) => match s {
                Statement::Let { expr, span, .. } => {
                    if offset >= span.start && offset <= span.end {
                        return true;
                    }
                    false
                }
                Statement::ExprStmt { expr, span, .. } => {
                    if offset >= span.start && offset <= span.end {
                        return true;
                    }
                    false
                }
            },
            _ => false,
        }
    }

    /// 提取 :: 前的命名空间路径
    fn extract_namespace_before(text_before: &str) -> Option<String> {
        let trimmed = text_before.trim_end();
        if !trimmed.ends_with("::") {
            return None;
        }
        let before_colon = &trimmed[..trimmed.len() - 2];

        if let Some(last_space) = before_colon.rfind(|c: char| c.is_whitespace() || c == '(' || c == '{' || c == ';') {
            let path = &before_colon[last_space + 1..];
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
        else if !before_colon.is_empty() {
            return Some(before_colon.to_string());
        }
        None
    }

    /// 提取 . 前的表达式文本
    fn extract_expression_before_dot(text_before: &str) -> Option<String> {
        let trimmed = text_before.trim_end();
        if !trimmed.ends_with('.') {
            return None;
        }
        let before_dot = &trimmed[..trimmed.len() - 1];

        let mut end = before_dot.len();
        let mut depth = 0;
        let mut chars: Vec<char> = before_dot.chars().collect();

        for i in (0..chars.len()).rev() {
            let c = chars[i];
            if c == ')' || c == ']' || c == '}' {
                depth += 1;
            }
            else if c == '(' || c == '[' || c == '{' {
                if depth > 0 {
                    depth -= 1;
                }
                else {
                    end = i + 1;
                    break;
                }
            }
            else if depth == 0 && (c.is_whitespace() || c == '(' || c == '{' || c == ';' || c == ',') {
                end = i + 1;
                break;
            }
        }

        if end < before_dot.len() {
            let expr = before_dot[end..].trim();
            if !expr.is_empty() {
                return Some(expr.to_string());
            }
        }
        else if !before_dot.trim().is_empty() {
            return Some(before_dot.trim().to_string());
        }
        None
    }

    /// 添加声明上下文关键字
    fn add_declaration_keywords(items: &mut Vec<CompletionItem>) {
        let keywords = vec![
            ("namespace", "命名空间声明"),
            ("import", "导入声明"),
            ("using", "使用声明"),
            ("class", "类声明"),
            ("trait", "特质声明"),
            ("union", "联合类型声明"),
            ("fn", "函数声明"),
            ("micro", "微函数声明"),
            ("let", "变量声明"),
            ("mut", "可变变量声明"),
            ("widget", "组件声明"),
            ("enum", "枚举声明"),
            ("flags", "标志位声明"),
            ("variant", "变体声明"),
            ("effect", "效果声明"),
            ("theorem", "定理声明"),
            ("macro", "宏声明"),
        ];

        for (kw, doc) in keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some(format!("关键字 {}", kw)),
                documentation: Some(doc.to_string()),
                insert_text: None,
            });
        }
    }

    /// 添加表达式上下文关键字
    fn add_expression_keywords(items: &mut Vec<CompletionItem>) {
        let keywords = vec![
            ("if", "条件表达式"),
            ("else", "否则分支"),
            ("match", "模式匹配"),
            ("loop", "循环"),
            ("while", "当循环"),
            ("for", "遍历循环"),
            ("return", "返回"),
            ("break", "跳出循环"),
            ("continue", "继续循环"),
            ("yield", "产出值"),
            ("raise", "抛出异常"),
            ("catch", "捕获异常"),
            ("true", "布尔真值"),
            ("false", "布尔假值"),
            ("forall", "全称量词"),
            ("exists", "存在量词"),
            ("where", "约束条件"),
        ];

        for (kw, doc) in keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some(format!("关键字 {}", kw)),
                documentation: Some(doc.to_string()),
                insert_text: None,
            });
        }
    }

    /// 从工作区添加符号
    async fn add_symbols_from_workspace(state: &ServerState, items: &mut Vec<CompletionItem>) {
        for doc_ref in state.documents.iter() {
            let doc = doc_ref.value();
            if let Some(ast) = &doc.ast {
                Self::extract_items(&ast.items, items, None);
            }
        }

        for ns_entry in state.index.symbols.iter() {
            let ns_map = ns_entry.value();
            for symbol_entry in ns_map.iter() {
                let symbol = symbol_entry.value();
                items.push(CompletionItem {
                    label: symbol.name.clone(),
                    kind: Some(Self::symbol_kind_to_completion_kind(symbol.kind)),
                    detail: Some(format!("{:?} {}", symbol.kind, symbol.name)),
                    documentation: symbol.documentation.clone(),
                    insert_text: None,
                });
            }
        }
    }

    /// 添加命名空间成员
    fn add_namespace_members(state: &ServerState, namespace: &str, items: &mut Vec<CompletionItem>) {
        if let Some(ns_map) = state.index.symbols.get(namespace) {
            for symbol_entry in ns_map.iter() {
                let symbol = symbol_entry.value();
                items.push(CompletionItem {
                    label: symbol.name.clone(),
                    kind: Some(Self::symbol_kind_to_completion_kind(symbol.kind)),
                    detail: Some(format!("{:?} {}", symbol.kind, symbol.name)),
                    documentation: symbol.documentation.clone(),
                    insert_text: None,
                });
            }
        }

        let parts: Vec<&str> = namespace.split("::").collect();
        if let Some(&prefix) = parts.first() {
            for doc_ref in state.documents.iter() {
                let doc = doc_ref.value();
                if let Some(ast) = &doc.ast {
                    Self::extract_items_in_namespace(&ast.items, namespace, items);
                }
            }
        }
    }

    /// 在指定命名空间中提取符号
    fn extract_items_in_namespace(items: &[Item], target_ns: &str, result: &mut Vec<CompletionItem>) {
        for item in items {
            if let Item::Namespace(n) = item {
                let ns_name = n.name.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("::");
                if ns_name == target_ns {
                    Self::extract_items(&n.items, result, Some(target_ns));
                }
                else if target_ns.starts_with(&format!("{}::", ns_name)) {
                    let remaining = &target_ns[ns_name.len() + 2..];
                    Self::extract_items_in_namespace(&n.items, remaining, result);
                }
            }
        }
    }

    /// 添加类型成员
    async fn add_type_members(state: &ServerState, uri: &str, expr_text: &str, items: &mut Vec<CompletionItem>) {
        if let Some(type_symbol) = Self::resolve_expression_type(state, uri, expr_text).await {
            Self::extract_type_members(state, &type_symbol, items);
        }
    }

    /// 解析表达式的类型
    async fn resolve_expression_type(state: &ServerState, uri: &str, expr_text: &str) -> Option<crate::state::GlobalSymbol> {
        let simple_name = expr_text
            .split(['.', ':'])
            .next()
            .unwrap_or(expr_text)
            .trim_end_matches(':');

        for doc_ref in state.documents.iter() {
            let doc = doc_ref.value();
            if let Some(ast) = &doc.ast {
                if let Some(mut symbol) = Self::find_symbol_in_ast(&ast.items, simple_name) {
                    if symbol.uri.is_empty() {
                        symbol.uri = doc.uri.clone();
                    }
                    return Some(symbol);
                }
            }
        }

        for ns_entry in state.index.symbols.iter() {
            let ns_map = ns_entry.value();
            if let Some(symbol) = ns_map.get(simple_name) {
                return Some((**symbol).clone());
            }
        }

        None
    }

    /// 在 AST 中查找符号
    fn find_symbol_in_ast(items: &[Item], name: &str) -> Option<crate::state::GlobalSymbol> {
        for item in items {
            match item {
                Item::Namespace(n) => {
                    let ns_name = n.name.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("::");
                    if let Some(symbol) = Self::find_symbol_in_ast(&n.items, name) {
                        return Some(symbol);
                    }
                }
                Item::Class(c) => {
                    if c.name.name == name {
                        return Some(crate::state::GlobalSymbol {
                            name: c.name.name.clone(),
                            namespace: String::new(),
                            kind: oak_lsp::types::SymbolKind::Class,
                            uri: String::new(),
                            range: c.span.clone(),
                            documentation: None,
                            hash: 0,
                            parent_class: None,
                            implemented_traits: vec![],
                            type_alias_target: None,
                            original_definition: None,
                        });
                    }
                }
                Item::Micro(m) => {
                    if m.name.name == name {
                        return Some(crate::state::GlobalSymbol {
                            name: m.name.name.clone(),
                            namespace: String::new(),
                            kind: oak_lsp::types::SymbolKind::Function,
                            uri: String::new(),
                            range: m.span.clone(),
                            documentation: None,
                            hash: 0,
                            parent_class: None,
                            implemented_traits: vec![],
                            type_alias_target: None,
                            original_definition: None,
                        });
                    }
                }
                Item::Flags(f) => {
                    if f.name.name == name {
                        return Some(crate::state::GlobalSymbol {
                            name: f.name.name.clone(),
                            namespace: String::new(),
                            kind: oak_lsp::types::SymbolKind::Enum,
                            uri: String::new(),
                            range: f.span.clone(),
                            documentation: None,
                            hash: 0,
                            parent_class: None,
                            implemented_traits: vec![],
                            type_alias_target: None,
                            original_definition: None,
                        });
                    }
                }
                Item::Enums(e) => {
                    if e.name.name == name {
                        return Some(crate::state::GlobalSymbol {
                            name: e.name.name.clone(),
                            namespace: String::new(),
                            kind: oak_lsp::types::SymbolKind::Enum,
                            uri: String::new(),
                            range: e.span.clone(),
                            documentation: None,
                            hash: 0,
                            parent_class: None,
                            implemented_traits: vec![],
                            type_alias_target: None,
                            original_definition: None,
                        });
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// 提取类型的成员
    fn extract_type_members(state: &ServerState, type_symbol: &crate::state::GlobalSymbol, items: &mut Vec<CompletionItem>) {
        if type_symbol.kind != oak_lsp::types::SymbolKind::Class
            && type_symbol.kind != oak_lsp::types::SymbolKind::Interface
            && type_symbol.kind != oak_lsp::types::SymbolKind::Enum
        {
            return;
        }

        let doc = match state.documents.get(&type_symbol.uri) {
            Some(d) => d,
            None => return,
        };

        let ast = match &doc.ast {
            Some(a) => a,
            None => return,
        };

        for item in &ast.items {
            match item {
                Item::Class(c) if c.name.name == type_symbol.name => {
                    Self::extract_class_members(&c.items, items);
                    return;
                }
                Item::Flags(f) if f.name.name == type_symbol.name => {
                    for variant in &f.variants {
                        items.push(CompletionItem {
                            label: variant.name.name.clone(),
                            kind: Some(CompletionItemKind::EnumMember),
                            detail: Some(format!("flags {}", variant.name.name)),
                            documentation: None,
                            insert_text: None,
                        });
                    }
                    return;
                }
                Item::Enums(e) if e.name.name == type_symbol.name => {
                    for variant in &e.variants {
                        items.push(CompletionItem {
                            label: variant.name.name.clone(),
                            kind: Some(CompletionItemKind::EnumMember),
                            detail: Some(format!("enum {}", variant.name.name)),
                            documentation: None,
                            insert_text: None,
                        });
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    /// 提取类成员
    fn extract_class_members(items: &[Item], result: &mut Vec<CompletionItem>) {
        for item in items {
            match item {
                Item::Micro(m) => {
                    let doc = Self::extract_function_documentation(m);
                    result.push(CompletionItem {
                        label: m.name.name.clone(),
                        kind: Some(CompletionItemKind::Method),
                        detail: Some(format!("method {}", m.name.name)),
                        documentation: doc,
                        insert_text: None,
                    });
                }
                Item::Statement(Statement::Let { pattern, .. }) => {
                    if let Pattern::Variable { name, .. } = pattern {
                        result.push(CompletionItem {
                            label: name.name.clone(),
                            kind: Some(CompletionItemKind::Field),
                            detail: Some(format!("field {}", name.name)),
                            documentation: None,
                            insert_text: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// 提取函数文档
    fn extract_function_documentation(m: &oak_valkyrie::ast::MicroDefinition) -> Option<String> {
        let mut parts = Vec::new();

        if !m.params.is_empty() {
            let params: Vec<String> = m.params.iter().map(|p| p.name.name.clone()).collect();
            parts.push(format!("参数: {}", params.join(", ")));
        }

        if let Some(ret) = &m.return_type {
            parts.push(format!("返回: {}", Self::format_type(ret)));
        }

        if parts.is_empty() {
            None
        }
        else {
            Some(parts.join("\n"))
        }
    }

    /// 格式化类型
    fn format_type(ty: &oak_valkyrie::ast::Type) -> String {
        match ty {
            oak_valkyrie::ast::Type::Named { path, .. } => {
                path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::")
            }
            oak_valkyrie::ast::Type::Generic { name, .. } => name.name.clone(),
            oak_valkyrie::ast::Type::Function { params, return_type, .. } => {
                let param_strs: Vec<String> = params.iter().map(|p| Self::format_type(p)).collect();
                format!("({}) -> {}", param_strs.join(", "), Self::format_type(return_type))
            }
            oak_valkyrie::ast::Type::Tuple { elements, .. } => {
                let elem_strs: Vec<String> = elements.iter().map(|e| Self::format_type(e)).collect();
                format!("({})", elem_strs.join(", "))
            }
            oak_valkyrie::ast::Type::Optional { inner, .. } => {
                format!("{}?", Self::format_type(inner))
            }
            _ => "unknown".to_string(),
        }
    }

    /// 添加局部符号
    fn add_local_symbols(items: &[Item], offset: usize, result: &mut Vec<CompletionItem>) {
        for item in items {
            Self::extract_local_symbols_from_item(item, offset, result);
        }
    }

    /// 从 Item 提取局部符号
    fn extract_local_symbols_from_item(item: &Item, offset: usize, result: &mut Vec<CompletionItem>) {
        match item {
            Item::Namespace(n) => {
                if offset >= n.span.start && offset <= n.span.end {
                    Self::extract_local_symbols(&n.items, offset, result);
                }
            }
            Item::Class(c) => {
                if offset >= c.span.start && offset <= c.span.end {
                    Self::extract_local_symbols(&c.items, offset, result);
                }
            }
            Item::Micro(m) => {
                if offset >= m.body.span.start && offset <= m.body.span.end {
                    Self::extract_local_symbols_from_block(&m.body, result);
                }
            }
            Item::TypeFunction(t) => {
                if offset >= t.body.span.start && offset <= t.body.span.end {
                    Self::extract_local_symbols_from_block(&t.body, result);
                }
            }
            _ => {}
        }
    }

    /// 从 Block 提取局部符号
    fn extract_local_symbols_from_block(block: &Block, result: &mut Vec<CompletionItem>) {
        for stmt in &block.statements {
            if let Statement::Let { pattern, .. } = stmt {
                if let Pattern::Variable { name, .. } = pattern {
                    result.push(CompletionItem {
                        label: name.name.clone(),
                        kind: Some(CompletionItemKind::Variable),
                        detail: Some(format!("局部变量 {}", name.name)),
                        documentation: None,
                        insert_text: None,
                    });
                }
            }
        }
    }

    /// 从 Items 提取局部符号
    fn extract_local_symbols(items: &[Item], offset: usize, result: &mut Vec<CompletionItem>) {
        for item in items {
            Self::extract_local_symbols_from_item(item, offset, result);
        }
    }

    /// 从 AST 提取符号
    fn extract_items(ast_items: &[Item], items: &mut Vec<CompletionItem>, _namespace: Option<&str>) {
        for item in ast_items {
            match item {
                Item::TypeFunction(f) => {
                    let doc = Self::extract_type_function_documentation(f);
                    items.push(CompletionItem {
                        label: f.name.name.clone(),
                        kind: Some(CompletionItemKind::Function),
                        detail: Some(format!("fn {}", f.name.name)),
                        documentation: doc,
                        insert_text: None,
                    });
                }
                Item::Micro(m) => {
                    let doc = Self::extract_function_documentation(m);
                    items.push(CompletionItem {
                        label: m.name.name.clone(),
                        kind: Some(CompletionItemKind::Function),
                        detail: Some(format!("micro {}", m.name.name)),
                        documentation: doc,
                        insert_text: None,
                    });
                }
                Item::Class(c) => {
                    items.push(CompletionItem {
                        label: c.name.name.clone(),
                        kind: Some(CompletionItemKind::Class),
                        detail: Some(format!("class {}", c.name.name)),
                        documentation: None,
                        insert_text: None,
                    });
                }
                Item::Namespace(n) => {
                    let name = n.name.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("::");
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::Module),
                        detail: Some(format!("namespace {}", name)),
                        documentation: None,
                        insert_text: None,
                    });
                }
                Item::Statement(stmt) => {
                    if let Statement::Let { pattern, .. } = stmt {
                        if let Pattern::Variable { name, .. } = pattern {
                            items.push(CompletionItem {
                                label: name.name.clone(),
                                kind: Some(CompletionItemKind::Variable),
                                detail: Some(format!("let {}", name.name)),
                                documentation: None,
                                insert_text: None,
                            });
                        }
                    }
                }
                Item::Widget(w) => {
                    items.push(CompletionItem {
                        label: w.name.name.clone(),
                        kind: Some(CompletionItemKind::Struct),
                        detail: Some(format!("widget {}", w.name.name)),
                        documentation: None,
                        insert_text: None,
                    });
                }
                Item::Trait(t) => {
                    items.push(CompletionItem {
                        label: t.name.name.clone(),
                        kind: Some(CompletionItemKind::Interface),
                        detail: Some(format!("trait {}", t.name.name)),
                        documentation: None,
                        insert_text: None,
                    });
                }
                Item::Enums(e) => {
                    items.push(CompletionItem {
                        label: e.name.name.clone(),
                        kind: Some(CompletionItemKind::Enum),
                        detail: Some(format!("enum {}", e.name.name)),
                        documentation: None,
                        insert_text: None,
                    });
                }
                _ => {}
            }
        }
    }

    /// 提取类型函数文档
    fn extract_type_function_documentation(f: &oak_valkyrie::ast::TypeFunction) -> Option<String> {
        let mut parts = Vec::new();

        if !f.params.is_empty() {
            let params: Vec<String> = f.params.iter().map(|p| p.name.name.clone()).collect();
            parts.push(format!("参数: {}", params.join(", ")));
        }

        if let Some(ret) = &f.return_type {
            let type_str = match ret {
                oak_valkyrie::ast::Type::Named { path, .. } => {
                    path.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("::")
                }
                oak_valkyrie::ast::Type::Generic { name, .. } => name.name.clone(),
                oak_valkyrie::ast::Type::Tuple { elements, .. } => {
                    let elems: Vec<String> = elements.iter().map(|t| format!("{:?}", t)).collect();
                    format!("({})", elems.join(", "))
                }
                oak_valkyrie::ast::Type::Function { return_type, .. } => format!("{:?}", return_type),
                oak_valkyrie::ast::Type::Optional { inner, .. } => format!("{:?}?", inner),
                oak_valkyrie::ast::Type::AssociatedType { name, .. } => name.name.clone(),
                oak_valkyrie::ast::Type::QualifiedAssociatedType { name, .. } => name.name.clone(),
            };
            parts.push(format!("返回: {}", type_str));
        }

        if parts.is_empty() {
            None
        }
        else {
            Some(parts.join("\n"))
        }
    }

    /// 将 SymbolKind 转换为 CompletionItemKind
    fn symbol_kind_to_completion_kind(kind: oak_lsp::types::SymbolKind) -> CompletionItemKind {
        use oak_lsp::types::SymbolKind;
        match kind {
            SymbolKind::File => CompletionItemKind::File,
            SymbolKind::Module => CompletionItemKind::Module,
            SymbolKind::Namespace => CompletionItemKind::Module,
            SymbolKind::Package => CompletionItemKind::Module,
            SymbolKind::Class => CompletionItemKind::Class,
            SymbolKind::Method => CompletionItemKind::Method,
            SymbolKind::Property => CompletionItemKind::Property,
            SymbolKind::Field => CompletionItemKind::Field,
            SymbolKind::Constructor => CompletionItemKind::Constructor,
            SymbolKind::Enum => CompletionItemKind::Enum,
            SymbolKind::Interface => CompletionItemKind::Interface,
            SymbolKind::Function => CompletionItemKind::Function,
            SymbolKind::Variable => CompletionItemKind::Variable,
            SymbolKind::Constant => CompletionItemKind::Constant,
            SymbolKind::String => CompletionItemKind::Text,
            SymbolKind::Number => CompletionItemKind::Value,
            SymbolKind::Boolean => CompletionItemKind::Value,
            SymbolKind::Array => CompletionItemKind::Value,
            SymbolKind::Object => CompletionItemKind::Class,
            SymbolKind::Key => CompletionItemKind::Keyword,
            SymbolKind::Null => CompletionItemKind::Value,
            SymbolKind::EnumMember => CompletionItemKind::EnumMember,
            SymbolKind::Struct => CompletionItemKind::Struct,
            SymbolKind::Event => CompletionItemKind::Event,
            SymbolKind::Operator => CompletionItemKind::Operator,
            SymbolKind::TypeParameter => CompletionItemKind::TypeParameter,
        }
    }
}
