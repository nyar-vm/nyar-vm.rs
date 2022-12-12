use super::{DocumentState, GlobalSymbol, ServerState, SymbolHasher};
use crate::handlers;
use oak_lsp::types::SymbolKind;
use oak_valkyrie::ast::ValkyrieRoot as ProgramRoot;
use tracing::{debug, error, info};
use nyar_language::ValkyrieCompiler;
use crate::types::{SourceID, ValkyrieError};

impl ServerState {
    /// 设置工作区根目录
    pub async fn set_workspace_root(&self, root: String) {
        *self.workspace_root.write() = Some(root.clone());
        if let Ok(url) = url::Url::parse(&root) {
            if let Ok(path) = url.to_file_path() {
                self.legion.write().set_workspace_root(path);
                self.scan_workspace().await;
            }
        }
    }

    /// 扫描工作区中的所有文�?    pub async fn scan_workspace(&self) {
        let packages = self.legion.read().get_all_packages();
        info!("Scanning workspace: found {} packages", packages.len());

        for pkg_dir in &packages {
            self.index_package(pkg_dir).await;
        }
    }

    /// 索引指定包中的所有文�?    pub async fn index_package(&self, pkg_dir: &std::path::Path) {
        let sources = self.legion.read().get_package_sources(pkg_dir);
        for source in sources {
            if let Ok(uri) = url::Url::from_file_path(&source) {
                let uri_str = uri.to_string();
                if !self.documents.contains_key(&uri_str) {
                    if let Ok(text) = std::fs::read_to_string(&source) {
                        if let Err(e) = self.compile_document(&uri_str, &text) {
                            error!("Failed to index {}: {}", uri_str, e);
                        }
                    }
                }
            }
        }
    }

    /// 编译文档
    pub fn compile_document(
        &self,
        uri: &str,
        text: &str,
    ) -> Result<Vec<ValkyrieError>, Box<dyn std::error::Error + Send + Sync>> {
        let new_hash = DocumentState::compute_hash(text);

        if let Some(doc) = self.documents.get(uri) {
            if doc.hash == new_hash && doc.ast.is_some() {
                debug!("Incremental: skipping compilation for {}", uri);
                return Ok(doc.diagnostics.clone());
            }
        }

        debug!("Compiling document: {}", uri);

        if handlers::awsl::document::is_awsl_uri(uri) {
            let mut doc_state = DocumentState::new(uri.to_string(), 1, text.to_string());
            doc_state.hash = new_hash;
            doc_state.file_id = Some(SourceID::default());
            let diagnostics = handlers::awsl::compile_awsl_document(uri, text, &mut doc_state, Some(self));
            self.documents.insert(uri.to_string(), doc_state);

            if let Some(doc) = self.documents.get(uri) {
                if doc.ast.is_some() {
                    if let Some((synthetic, _)) = doc.awsl_root.as_ref().and_then(|root| {
                        handlers::awsl::synthetic_widget_source(
                            root,
                            &handlers::awsl::component_stem_from_uri(uri),
                        )
                    }) {
                        self.reindex_document(uri, &synthetic);
                    }
                }
            }

            return Ok(diagnostics);
        }

        let mut doc_state = DocumentState::new(uri.to_string(), 1, text.to_string());
        doc_state.hash = new_hash;
        let source_id = SourceID::default();
        doc_state.file_id = Some(source_id);

        let mut compiler = ValkyrieCompiler::new(text.to_string());

        match compiler.parse() {
            Ok(ast) => {
                doc_state.ast = Some(ast.clone());

                let mut _old_namespace = None;
                if let Some(old_doc) = self.documents.get(uri) {
                    if let Some(old_ast) = &old_doc.ast {
                        for item in &old_ast.items {
                            if let oak_valkyrie::ast::Item::Namespace(n) = item {
                                _old_namespace =
                                    Some(n.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::"));
                                break;
                            }
                        }
                    }
                }

                if let Ok(hir) = compiler.lower_hir(ast.clone()) {
                    doc_state.hir = Some(hir);
                }

                doc_state.diagnostics = Vec::new();
                doc_state.diagnostics.extend(handlers::naming::collect_naming_diagnostics(
                    text,
                    is_vx_uri(uri),
                    source_id,
                    0,
                ));

                self.documents.insert(uri.to_string(), doc_state);

                self.reindex_document(uri, text);

                let self_clone = self.clone();
                let uri_clone = uri.to_string();
                tokio::spawn(async move {
                    self_clone.extract_dependencies_from_ast(&uri_clone).await;
                });

                Ok(Vec::new())
            }
            Err(e) => {
                doc_state.diagnostics = vec![e.clone()];

                self.documents.insert(uri.to_string(), doc_state);

                self.reindex_document(uri, text);

                Ok(vec![e])
            }
        }
    }

    /// 重新索引文档中的所有全局符号
    pub fn reindex_document(&self, uri: &str, source_text: &str) {
        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return,
        };
        let ast = match &doc.ast {
            Some(a) => a,
            None => return,
        };

        let content_hash = SymbolHasher::compute_content_hash(source_text);

        if !self.index.needs_reindex(uri, content_hash) {
            debug!("Skipping reindex for unchanged file: {}", uri);
            return;
        }

        let mut current_namespace = String::new();
        let mut symbols = Vec::new();

        for item in &ast.items {
            self.extract_symbols_from_item(item, uri, source_text, &mut current_namespace, &mut symbols);
        }

        if handlers::awsl::document::is_awsl_uri(uri) {
            if let (Some(base), Some(prefix)) = (doc.awsl_script_base, doc.awsl_synthetic_prefix) {
                for sym in &mut symbols {
                    sym.range = handlers::awsl::map_synthetic_span_to_file(base, prefix, sym.range.clone());
                }
            }
        }

        self.semantic_cache.invalidate_namespace(&current_namespace);

        self.index.update_file_symbols(uri, &current_namespace, symbols, content_hash);
    }

    /// �?AST 项中提取符号
    fn extract_symbols_from_item(
        &self,
        item: &oak_valkyrie::ast::Item,
        uri: &str,
        source_text: &str,
        current_namespace: &mut String,
        symbols: &mut Vec<GlobalSymbol>,
    ) {
        match item {
            oak_valkyrie::ast::Item::Namespace(n) => {
                *current_namespace = n.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                for nested_item in &n.items {
                    self.extract_symbols_from_item(nested_item, uri, source_text, current_namespace, symbols);
                }
            }
            oak_valkyrie::ast::Item::Micro(f) => {
                let doc = Self::extract_doc_from_annotations(&f.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, f.span.start));
                let hash = SymbolHasher::compute_symbol_hash(
                    &f.name.name,
                    current_namespace,
                    SymbolKind::Function,
                    source_text,
                    &f.span,
                );
                symbols.push(GlobalSymbol {
                    name: f.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Function,
                    uri: uri.to_string(),
                    range: f.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
            }
            oak_valkyrie::ast::Item::Class(c) => {
                let doc = Self::extract_doc_from_annotations(&c.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, c.span.start));
                let hash =
                    SymbolHasher::compute_symbol_hash(&c.name.name, current_namespace, SymbolKind::Class, source_text, &c.span);
                let parent_class = c.parents.first().map(|p| {
                    p.name.parts.iter().map(|part| part.name.clone()).collect::<Vec<_>>().join("::")
                });
                let implemented_traits: Vec<String> = c.parents
                    .iter()
                    .skip(1)
                    .map(|impl_path| impl_path.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::"))
                    .collect();
                symbols.push(GlobalSymbol {
                    name: c.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Class,
                    uri: uri.to_string(),
                    range: c.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class,
                    implemented_traits,
                    type_alias_target: None,
                    original_definition: None,
                });
                for nested_item in &c.items {
                    self.extract_class_member(nested_item, uri, source_text, current_namespace, &c.name.name, symbols);
                }
            }
            oak_valkyrie::ast::Item::Trait(t) => {
                let doc = Self::extract_doc_from_annotations(&t.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, t.span.start));
                let hash = SymbolHasher::compute_symbol_hash(
                    &t.name.name,
                    current_namespace,
                    SymbolKind::Interface,
                    source_text,
                    &t.span,
                );
                symbols.push(GlobalSymbol {
                    name: t.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Interface,
                    uri: uri.to_string(),
                    range: t.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
                for method in &t.methods {
                    self.extract_trait_method(method, uri, source_text, current_namespace, &t.name.name, symbols);
                }
            }
            oak_valkyrie::ast::Item::Enums(e) => {
                let doc = Self::extract_doc_from_annotations(&e.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, e.span.start));
                let hash =
                    SymbolHasher::compute_symbol_hash(&e.name.name, current_namespace, SymbolKind::Enum, source_text, &e.span);
                symbols.push(GlobalSymbol {
                    name: e.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Enum,
                    uri: uri.to_string(),
                    range: e.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
                for variant in &e.variants {
                    self.extract_enum_variant(variant, uri, source_text, current_namespace, &e.name.name, symbols);
                }
            }
            oak_valkyrie::ast::Item::Variant(v) => {
                let doc = Self::extract_doc_from_annotations(&v.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, v.span.start));
                let hash = SymbolHasher::compute_symbol_hash(
                    &v.name.name,
                    current_namespace,
                    SymbolKind::EnumMember,
                    source_text,
                    &v.span,
                );
                symbols.push(GlobalSymbol {
                    name: v.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::EnumMember,
                    uri: uri.to_string(),
                    range: v.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
            }
            oak_valkyrie::ast::Item::Effect(e) => {
                let doc = Self::extract_doc_from_annotations(&e.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, e.span.start));
                let hash = SymbolHasher::compute_symbol_hash(
                    &e.name.name,
                    current_namespace,
                    SymbolKind::Interface,
                    source_text,
                    &e.span,
                );
                symbols.push(GlobalSymbol {
                    name: e.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Interface,
                    uri: uri.to_string(),
                    range: e.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
                for op in &e.operations {
                    self.extract_effect_operation(op, uri, source_text, current_namespace, &e.name.name, symbols);
                }
            }
            oak_valkyrie::ast::Item::Flags(f) => {
                let doc = Self::extract_doc_from_annotations(&f.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, f.span.start));
                let hash =
                    SymbolHasher::compute_symbol_hash(&f.name.name, current_namespace, SymbolKind::Enum, source_text, &f.span);
                symbols.push(GlobalSymbol {
                    name: f.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Enum,
                    uri: uri.to_string(),
                    range: f.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
                for flag_variant in &f.variants {
                    let variant_doc = Self::extract_doc_from_annotations(&flag_variant.annotations)
                        .or_else(|| self.extract_doc_comment_before(source_text, flag_variant.span.start));
                    let variant_hash = SymbolHasher::compute_symbol_hash(
                        &flag_variant.name.name,
                        current_namespace,
                        SymbolKind::EnumMember,
                        source_text,
                        &flag_variant.span,
                    );
                    symbols.push(GlobalSymbol {
                        name: flag_variant.name.name.clone(),
                        namespace: current_namespace.clone(),
                        kind: SymbolKind::EnumMember,
                        uri: uri.to_string(),
                        range: flag_variant.span.clone(),
                        documentation: variant_doc,
                        hash: variant_hash,
                        parent_class: None,
                        implemented_traits: vec![],
                        type_alias_target: None,
                        original_definition: None,
                    });
                }
            }
            oak_valkyrie::ast::Item::Widget(w) => {
                let doc = Self::extract_doc_from_annotations(&w.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, w.span.start));
                let hash =
                    SymbolHasher::compute_symbol_hash(&w.name.name, current_namespace, SymbolKind::Class, source_text, &w.span);
                symbols.push(GlobalSymbol {
                    name: w.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Class,
                    uri: uri.to_string(),
                    range: w.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
                for nested_item in &w.items {
                    self.extract_class_member(nested_item, uri, source_text, current_namespace, &w.name.name, symbols);
                }
            }
            oak_valkyrie::ast::Item::TypeFunction(tf) => {
                let doc = Self::extract_doc_from_annotations(&tf.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, tf.span.start));
                let hash = SymbolHasher::compute_symbol_hash(
                    &tf.name.name,
                    current_namespace,
                    SymbolKind::Function,
                    source_text,
                    &tf.span,
                );
                symbols.push(GlobalSymbol {
                    name: tf.name.name.clone(),
                    namespace: current_namespace.clone(),
                    kind: SymbolKind::Function,
                    uri: uri.to_string(),
                    range: tf.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: None,
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
            }
            _ => {}
        }
    }

    /// 提取类成员符�?    fn extract_class_member(
        &self,
        item: &oak_valkyrie::ast::Item,
        uri: &str,
        source_text: &str,
        namespace: &str,
        class_name: &str,
        symbols: &mut Vec<GlobalSymbol>,
    ) {
        match item {
            oak_valkyrie::ast::Item::Micro(m) => {
                let doc = Self::extract_doc_from_annotations(&m.annotations)
                    .or_else(|| self.extract_doc_comment_before(source_text, m.span.start));
                let full_name = format!("{}::{}", class_name, m.name.name);
                let hash = SymbolHasher::compute_symbol_hash(&full_name, namespace, SymbolKind::Method, source_text, &m.span);
                symbols.push(GlobalSymbol {
                    name: full_name,
                    namespace: namespace.to_string(),
                    kind: SymbolKind::Method,
                    uri: uri.to_string(),
                    range: m.span.clone(),
                    documentation: doc,
                    hash,
                    parent_class: Some(class_name.to_string()),
                    implemented_traits: vec![],
                    type_alias_target: None,
                    original_definition: None,
                });
            }
            _ => {}
        }
    }

    /// 提取 Trait 方法符号
    fn extract_trait_method(
        &self,
        method: &oak_valkyrie::ast::Function,
        uri: &str,
        source_text: &str,
        namespace: &str,
        trait_name: &str,
        symbols: &mut Vec<GlobalSymbol>,
    ) {
        let doc = Self::extract_doc_from_annotations(&method.annotations)
            .or_else(|| self.extract_doc_comment_before(source_text, method.span.start));
        let full_name = format!("{}::{}", trait_name, method.name.name);
        let hash = SymbolHasher::compute_symbol_hash(&full_name, namespace, SymbolKind::Method, source_text, &method.span);
        symbols.push(GlobalSymbol {
            name: full_name,
            namespace: namespace.to_string(),
            kind: SymbolKind::Method,
            uri: uri.to_string(),
            range: method.span.clone(),
            documentation: doc,
            hash,
            parent_class: None,
            implemented_traits: vec![],
            type_alias_target: None,
            original_definition: None,
        });
    }

    /// 提取 Enum 变体符号
    fn extract_enum_variant(
        &self,
        variant: &oak_valkyrie::ast::EnumVariant,
        uri: &str,
        source_text: &str,
        namespace: &str,
        enum_name: &str,
        symbols: &mut Vec<GlobalSymbol>,
    ) {
        let doc = Self::extract_doc_from_annotations(&variant.annotations)
            .or_else(|| self.extract_doc_comment_before(source_text, variant.span.start));
        let full_name = format!("{}::{}", enum_name, variant.name.name);
        let hash = SymbolHasher::compute_symbol_hash(&full_name, namespace, SymbolKind::EnumMember, source_text, &variant.span);
        symbols.push(GlobalSymbol {
            name: full_name,
            namespace: namespace.to_string(),
            kind: SymbolKind::EnumMember,
            uri: uri.to_string(),
            range: variant.span.clone(),
            documentation: doc,
            hash,
            parent_class: None,
            implemented_traits: vec![],
            type_alias_target: None,
            original_definition: None,
        });
    }

    /// 提取 Effect 操作符号
    fn extract_effect_operation(
        &self,
        op: &oak_valkyrie::ast::Function,
        uri: &str,
        source_text: &str,
        namespace: &str,
        effect_name: &str,
        symbols: &mut Vec<GlobalSymbol>,
    ) {
        let doc = Self::extract_doc_from_annotations(&op.annotations)
            .or_else(|| self.extract_doc_comment_before(source_text, op.span.start));
        let full_name = format!("{}::{}", effect_name, op.name.name);
        let hash = SymbolHasher::compute_symbol_hash(&full_name, namespace, SymbolKind::Method, source_text, &op.span);
        symbols.push(GlobalSymbol {
            name: full_name,
            namespace: namespace.to_string(),
            kind: SymbolKind::Method,
            uri: uri.to_string(),
            range: op.span.clone(),
            documentation: doc,
            hash,
            parent_class: None,
            implemented_traits: vec![],
            type_alias_target: None,
            original_definition: None,
        });
    }

    /// 从注解中提取文档注释
    fn extract_doc_from_annotations(annotations: &[oak_valkyrie::ast::Attribute]) -> Option<String> {
        for attr in annotations {
            if attr.name.name == "doc" {
                if let Some(first_arg) = attr.args.first() {
                    if let oak_valkyrie::ast::Expr::StringLiteral(s) = first_arg {
                        if let Some(segment) = s.segments.first() {
                            if let oak_valkyrie::ast::StringSegment::Text { content, .. } = segment {
                                return Some(content.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 提取符号前的文档注释（Valkyrie 风格�?    /// 支持的文档注释格式：
    /// - `⍝` APL 风格文档注释
    /// - `#?` 问号文档注释
    /// - `@doc` 注解形式文档
    /// - `<#? ?#>` 文档型块注释
    fn extract_doc_comment_before(&self, source_text: &str, position: usize) -> Option<String> {
        if position == 0 || position > source_text.len() {
            return None;
        }

        let before = &source_text[..position];
        let lines: Vec<&str> = before.lines().collect();
        let mut doc_lines = Vec::new();
        let mut in_doc_block = false;
        let mut block_content = String::new();

        for line in lines.iter().rev() {
            let trimmed = line.trim();

            if in_doc_block {
                if let Some(start_idx) = trimmed.find("<#?") {
                    let content = trimmed[start_idx + 3..].trim();
                    block_content = format!("{}{}", content, block_content);
                    in_doc_block = false;
                    break;
                }
                else {
                    if trimmed.ends_with("?#>") {
                        continue;
                    }
                    block_content = format!("{} {}", trimmed, block_content);
                }
            }
            else if trimmed.ends_with("?#>") {
                in_doc_block = true;
                if let Some(content) = trimmed.strip_suffix("?#>") {
                    let content = content.trim();
                    if content.starts_with("<#?") {
                        return Some(content[3..].trim().to_string());
                    }
                    block_content = content.to_string();
                }
            }
            else if trimmed.starts_with("�?) {
                let content = trimmed[3..].trim();
                doc_lines.push(content.to_string());
            }
            else if trimmed.starts_with("#?") {
                let content = trimmed[2..].trim();
                doc_lines.push(content.to_string());
            }
            else if trimmed.starts_with("@doc") {
                let content = trimmed[4..].trim();
                doc_lines.push(content.to_string());
            }
            else if trimmed.starts_with('#') && !trimmed.starts_with("#?") && !trimmed.starts_with("#<") {
                continue;
            }
            else if trimmed.starts_with("�?) || trimmed.starts_with("@") {
                continue;
            }
            else if trimmed.is_empty() {
                continue;
            }
            else {
                break;
            }
        }

        if in_doc_block && !block_content.is_empty() {
            return Some(block_content.trim().to_string());
        }

        if !doc_lines.is_empty() {
            doc_lines.reverse();
            Some(doc_lines.join("\n"))
        }
        else {
            None
        }
    }

    /// �?AST 中提取跨文档的依赖关�?    pub async fn extract_dependencies_from_ast(&self, uri: &str) {
        let ast = match self.documents.get(uri).and_then(|d| d.ast.clone()) {
            Some(a) => a,
            None => return,
        };

        let mut _current_namespace = String::new();
        for item in &ast.items {
            if let oak_valkyrie::ast::Item::Namespace(n) = item {
                _current_namespace = n.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
            }
            if let oak_valkyrie::ast::Item::Using(u) = item {
                let ns = u.path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                if let Some(first) = u.path.parts.first() {
                    let pkg_name = first.name.clone();
                    let pkg_path = self.legion.read().resolve_package(&pkg_name, uri);
                    if let Some(pkg_path) = pkg_path {
                        self.index_package(&pkg_path).await;
                    }
                }
                debug!("Found import dependency: {} -> {}", _current_namespace, ns);
            }
        }
    }

    /// 获取指定文档�?AST
    pub fn get_ast(&self, uri: &str) -> Option<ProgramRoot> {
        self.documents.get(uri).and_then(|doc| doc.ast.clone())
    }

    /// 获取指定文档�?HIR
    pub fn get_hir(&self, uri: &str) -> Option<crate::types::hir::HirModule> {
        self.documents.get(uri).and_then(|doc| doc.hir.clone())
    }

    pub fn get_diagnostics(&self, uri: &str) -> Option<Vec<ValkyrieError>> {
        self.documents.get(uri).map(|doc| doc.diagnostics.clone())
    }
}

fn is_vx_uri(uri: &str) -> bool {
    uri.ends_with(".vx") || uri.ends_with(".valkyrie") || uri.contains(".vx?") || uri.contains(".valkyrie?")
}
