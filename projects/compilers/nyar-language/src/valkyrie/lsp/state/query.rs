use super::{AstTraverser, ClassInfo, DocumentState, GlobalSymbol, IdentifierFinder, MemberInfo, ParameterInfo, ServerState, SymbolFinder, SymbolInfo, TypeSignature, VisitorContext};
use crate::types::Position;
use oak_lsp::types::LocationRange;
use std::sync::Arc;

impl ServerState {
    /// 查找符号定义，并记录依赖关系
    pub async fn resolve_symbol(
        &self,
        name: &str,
        uri: &str,
        caller_ns: &str,
        caller_name: Option<&str>,
    ) -> Option<Arc<GlobalSymbol>> {
        let ast = self.documents.get(uri).and_then(|d| d.ast.clone())?;
        let mut current_namespace = String::new();

        for item in &ast.items {
            if let oak_valkyrie::ast::Item::Namespace(n) = item {
                current_namespace = n.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                break;
            }
        }

        if let Some(ns_cache) = self.semantic_cache.cache.get(&current_namespace) {
            if let Some(symbol_info) = ns_cache.get(name) {
                if let Some(c_name) = caller_name {
                    self.semantic_cache.update_dependencies(&symbol_info.namespace, &symbol_info.name, caller_ns, c_name);
                }
                if let Some(ns_map) = self.index.symbols.get(&symbol_info.namespace) {
                    if let Some(symbol) = ns_map.get(&symbol_info.name) {
                        return Some(symbol.clone());
                    }
                }
            }
        }

        if let Some(ns_map) = self.index.symbols.get(&current_namespace) {
            if let Some(symbol) = ns_map.get(name) {
                if let Some(c_name) = caller_name {
                    self.semantic_cache.update_dependencies(&current_namespace, name, caller_ns, c_name);
                }
                self.cache_symbol(&current_namespace, name, symbol.clone());
                return Some(symbol.clone());
            }
        }

        for item in &ast.items {
            if let oak_valkyrie::ast::Item::Using(i) = item {
                let ns = i.path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                if let Some(ns_map) = self.index.symbols.get(&ns) {
                    if let Some(symbol) = ns_map.get(name) {
                        if let Some(c_name) = caller_name {
                            self.semantic_cache.update_dependencies(&ns, name, caller_ns, c_name);
                        }
                        self.cache_symbol(&current_namespace, name, symbol.clone());
                        return Some(symbol.clone());
                    }
                }
            }
        }

        None
    }

    /// 查询指定位置的符号信息
    pub async fn query_symbol_at_position(&self, uri: &str, position: Position) -> Option<SymbolInfo> {
        let (doc, ast) = {
            let doc = self.documents.get(uri)?;
            let ast = doc.ast.clone()?;
            (doc.clone(), ast)
        };
        let offset = doc.position_to_offset(position) as usize;

        let mut current_namespace = String::new();
        for item in &ast.items {
            if let oak_valkyrie::ast::Item::Namespace(n) = item {
                current_namespace = n.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                break;
            }
        }

        let context = VisitorContext {
            uri,
            doc: &doc,
            offset,
            context_ns: &current_namespace,
            context_symbol: None,
        };

        // 先尝试使用 SymbolFinder
        let symbol_finder = SymbolFinder::new(self);
        if let Some(symbol) = AstTraverser::traverse_items(&ast.items, &symbol_finder, &context).await {
            return Some(symbol);
        }

        // 再尝试使用 IdentifierFinder
        let identifier_finder = IdentifierFinder::new(self);
        AstTraverser::traverse_items(&ast.items, &identifier_finder, &context).await
    }

    /// 查询 AWSL 文件 `<script>` 块内指定位置的 Valkyrie 符号
    pub async fn query_awsl_script_symbol_at_position(&self, uri: &str, position: Position) -> Option<SymbolInfo> {
        use crate::handlers::awsl::{map_script_range_to_file, resolve_script_view};

        let doc = self.documents.get(uri)?.clone();
        let view = resolve_script_view(&doc, position)?;
        let script_offset = view.script_doc.position_to_offset(view.script_position);

        let context = VisitorContext {
            uri,
            doc: &view.script_doc,
            offset: script_offset,
            context_ns: "",
            context_symbol: None,
        };

        let symbol_finder = SymbolFinder::new(self);
        let mut symbol = if let Some(s) = AstTraverser::traverse_items(&view.ast.items, &symbol_finder, &context).await {
            s
        }
        else {
            let identifier_finder = IdentifierFinder::new(self);
            AstTraverser::traverse_items(&view.ast.items, &identifier_finder, &context).await?
        };

        symbol.location.uri = doc.uri.clone().into();
        symbol.location.range = map_script_range_to_file(&view.file_range, symbol.location.range);
        Some(symbol)
    }



    /// 从 Micro 函数定义中提取类型签名
    fn extract_micro_signature(micro: &oak_valkyrie::ast::MicroDefinition) -> TypeSignature {
        let parameters: Vec<ParameterInfo> = micro
            .params
            .iter()
            .map(|p| ParameterInfo {
                name: p.name.name.clone(),
                ty: p.ty.as_ref().map(|t| Self::format_type_annotation(t)),
                is_optional: false,
                is_variadic: false,
            })
            .collect();

        let return_type = micro.return_type.as_ref().map(|t| Self::format_type_annotation(t));

        TypeSignature { parameters, return_type, type_parameters: vec![] }
    }

    /// 从 Function 定义中提取类型签名（用于 Trait 方法）
    fn extract_function_signature(func: &oak_valkyrie::ast::Function) -> TypeSignature {
        let parameters: Vec<ParameterInfo> = func
            .params
            .iter()
            .map(|p| ParameterInfo {
                name: p.name.name.clone(),
                ty: p.ty.as_ref().map(|t| Self::format_type_annotation(t)),
                is_optional: false,
                is_variadic: false,
            })
            .collect();

        let return_type = func.return_type.as_ref().map(|t| Self::format_type_annotation(t));

        TypeSignature { parameters, return_type, type_parameters: vec![] }
    }

    /// 从 TypeFunction 定义中提取类型签名
    fn extract_type_function_signature(tf: &oak_valkyrie::ast::TypeFunction) -> TypeSignature {
        let parameters: Vec<ParameterInfo> = tf
            .params
            .iter()
            .map(|p| ParameterInfo {
                name: p.name.name.clone(),
                ty: p.ty.as_ref().map(|t| Self::format_type_annotation(t)),
                is_optional: false,
                is_variadic: false,
            })
            .collect();

        let return_type = tf.return_type.as_ref().map(|t| Self::format_type_annotation(t));

        TypeSignature { parameters, return_type, type_parameters: vec![] }
    }

    /// 从 Class 定义中提取类信息
    fn extract_class_info(class: &oak_valkyrie::ast::Class) -> ClassInfo {
        let parent_class = class.parents.first().map(|p| {
            p.name.parts.iter().map(|part| part.name.clone()).collect::<Vec<_>>().join("::")
        });

        let implements: Vec<String> = class
            .parents
            .iter()
            .skip(1)
            .map(|impl_path| impl_path.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::"))
            .collect();

        let members: Vec<MemberInfo> = class
            .items
            .iter()
            .filter_map(|item| match item {
                oak_valkyrie::ast::Item::Micro(m) => Some(MemberInfo {
                    name: m.name.name.clone(),
                    kind: "method".to_string(),
                    type_info: Some(Self::format_function_signature(&Self::extract_micro_signature(m))),
                    documentation: None,
                }),
                oak_valkyrie::ast::Item::TypeFunction(tf) => Some(MemberInfo {
                    name: tf.name.name.clone(),
                    kind: "function".to_string(),
                    type_info: Some(Self::format_function_signature(&Self::extract_type_function_signature(tf))),
                    documentation: None,
                }),
                _ => None,
            })
            .collect();

        ClassInfo { parent_class, implements, members }
    }

    /// 格式化类型注解为字符串
    fn format_type_annotation(ty: &oak_valkyrie::ast::Type) -> String {
        match ty {
            oak_valkyrie::ast::Type::Named { path, .. } => {
                path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::")
            }
            oak_valkyrie::ast::Type::Generic { name, .. } => {
                name.name.clone()
            }
            oak_valkyrie::ast::Type::Function { params, return_type, .. } => {
                let param_strs: Vec<String> = params.iter().map(|p| Self::format_type_annotation(p)).collect();
                let ret_str = Self::format_type_annotation(return_type);
                format!("({}) -> {}", param_strs.join(", "), ret_str)
            }
            oak_valkyrie::ast::Type::Tuple { elements, .. } => {
                let elem_strs: Vec<String> = elements.iter().map(|e| Self::format_type_annotation(e)).collect();
                format!("({})", elem_strs.join(", "))
            }
            oak_valkyrie::ast::Type::Optional { inner, .. } => {
                format!("{}?", Self::format_type_annotation(inner))
            }
            oak_valkyrie::ast::Type::AssociatedType { base, name, .. } => {
                format!("{}::{}", base.name, name.name)
            }
            oak_valkyrie::ast::Type::QualifiedAssociatedType { ty, trait_path, name, .. } => {
                let ty_str = Self::format_type_annotation(ty);
                let trait_str = trait_path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                format!("<{} as {}>::{}", ty_str, trait_str, name.name)
            }
        }
    }

    /// 格式化函数签名为可读字符串
    fn format_function_signature(sig: &TypeSignature) -> String {
        let params: Vec<String> = sig
            .parameters
            .iter()
            .map(|p| match &p.ty {
                Some(ty) => format!("{}: {}", p.name, ty),
                None => p.name.clone(),
            })
            .collect();

        let return_part = match &sig.return_type {
            Some(ret) => format!(" -> {}", ret),
            None => String::new(),
        };

        format!("({}){}", params.join(", "), return_part)
    }

    async fn resolve_namespace_or_symbol(&self, path: &str, uri: &str, context_ns: &str) -> Option<Arc<GlobalSymbol>> {
        if let Some(ns_map) = self.index.symbols.get(path) {
            for symbol in ns_map.iter() {
                if symbol.name == path.split("::").last().unwrap_or(path) {
                    return Some(symbol.clone());
                }
            }
        }
        self.resolve_symbol(path, uri, context_ns, None).await
    }

    async fn get_expression_type(
        &self,
        expr: &oak_valkyrie::ast::Expr,
        uri: &str,
        context_ns: &str,
        context_symbol: Option<&str>,
    ) -> Option<Arc<GlobalSymbol>> {
        match expr {
            oak_valkyrie::ast::Expr::Ident(id) => {
                return self.resolve_symbol(&id.name, uri, context_ns, context_symbol).await;
            }
            oak_valkyrie::ast::Expr::Path(path) => {
                if let Some(last) = path.parts.last() {
                    return self.resolve_symbol(&last.name, uri, context_ns, context_symbol).await;
                }
                None
            }
            _ => None,
        }
    }

    fn find_member_in_type(&self, type_info: &GlobalSymbol, member_name: &str, _uri: &str) -> Option<SymbolInfo> {
        if type_info.kind != oak_lsp::types::SymbolKind::Class && type_info.kind != oak_lsp::types::SymbolKind::Interface {
            return None;
        }

        let doc = self.documents.get(&type_info.uri)?;
        let ast = doc.ast.as_ref()?;

        for item in &ast.items {
            if let oak_valkyrie::ast::Item::Class(c) = item {
                if c.name.name == type_info.name {
                    for member in &c.items {
                        match member {
                            oak_valkyrie::ast::Item::Micro(m) if m.name.name == member_name => {
                                let signature = Self::extract_micro_signature(m);
                                return Some(SymbolInfo {
                                    name: m.name.name.clone(),
                                    namespace: type_info.namespace.clone(),
                                    kind: "method".to_string(),
                                    type_info: Some(Self::format_function_signature(&signature)),
                                    documentation: None,
                                    location: LocationRange { uri: type_info.uri.clone().into(), range: m.span.clone() },
                                    signature: Some(signature),
                                    class_info: None,
                                });
                            }
                            oak_valkyrie::ast::Item::TypeFunction(tf) if tf.name.name == member_name => {
                                let signature = Self::extract_type_function_signature(tf);
                                return Some(SymbolInfo {
                                    name: tf.name.name.clone(),
                                    namespace: type_info.namespace.clone(),
                                    kind: "function".to_string(),
                                    type_info: Some(Self::format_function_signature(&signature)),
                                    documentation: None,
                                    location: LocationRange { uri: type_info.uri.clone().into(), range: tf.span.clone() },
                                    signature: Some(signature),
                                    class_info: None,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        None
    }
}
