use core::range::Range;
use dashmap::DashMap;
use oak_lsp::types::{LocationRange, SymbolKind};
use std::sync::Arc;

/// 函数参数信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParameterInfo {
    /// 参数名称
    pub name: String,
    /// 参数类型（可选，因为可能没有显式类型注解）
    pub ty: Option<String>,
    /// 参数是否可选
    pub is_optional: bool,
    /// 参数是否是可变参数
    pub is_variadic: bool,
}

/// 类成员信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemberInfo {
    /// 成员名称
    pub name: String,
    /// 成员类型
    pub kind: String,
    /// 成员类型信息
    pub type_info: Option<String>,
    /// 成员文档
    pub documentation: Option<String>,
}

/// 详细类型签名信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeSignature {
    /// 函数参数列表
    pub parameters: Vec<ParameterInfo>,
    /// 返回类型
    pub return_type: Option<String>,
    /// 泛型参数
    pub type_parameters: Vec<String>,
}

/// 类类型信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassInfo {
    /// 父类名称
    pub parent_class: Option<String>,
    /// 实现的接口列表
    pub implements: Vec<String>,
    /// 成员概览
    pub members: Vec<MemberInfo>,
}

/// 符号查询结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolInfo {
    /// 符号名称
    pub name: String,
    /// 所属命名空间
    pub namespace: String,
    /// 符号类型（function, class, variable 等）
    pub kind: String,
    /// 简单类型信息（向后兼容）
    pub type_info: Option<String>,
    /// 文档注释
    pub documentation: Option<String>,
    /// 符号位置
    pub location: LocationRange,
    /// 详细类型签名（函数专用）
    pub signature: Option<TypeSignature>,
    /// 类详细信息（类专用）
    pub class_info: Option<ClassInfo>,
}

/// Trait 实现信息
#[derive(Debug, Clone)]
pub struct TraitImplementation {
    /// 实现 Trait 的类型名称
    pub implementor: String,
    /// 实现 Trait 的命名空间
    pub implementor_namespace: String,
    /// 实现位置 URI
    pub uri: String,
    /// 实现位置范围
    pub range: Range<usize>,
}

/// 全局符号信息
#[derive(Debug, Clone)]
pub struct GlobalSymbol {
    pub name: String,
    pub namespace: String,
    pub kind: SymbolKind,
    pub uri: String,
    pub range: Range<usize>,
    pub documentation: Option<String>,
    pub hash: u64,
    /// 父类名称（用于类继承）
    pub parent_class: Option<String>,
    /// 实现的 Trait 列表（用于 imply 实现）
    pub implemented_traits: Vec<String>,
    /// 类型别名目标（用于类型别名）
    pub type_alias_target: Option<String>,
    /// 原定义位置（用于重导出符号）
    pub original_definition: Option<LocationRange>,
}

/// 文件索引状态，用于增量索引
#[derive(Debug, Clone)]
pub struct FileIndexState {
    pub uri: String,
    pub content_hash: u64,
    pub symbol_hashes: Vec<(String, u64)>,
    pub namespace: String,
}

/// 符号哈希计算工具
pub struct SymbolHasher;

impl SymbolHasher {
    /// 计算符号内容的哈希值
    pub fn compute_symbol_hash(name: &str, namespace: &str, kind: SymbolKind, source_text: &str, range: &Range<usize>) -> u64 {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        namespace.hash(&mut hasher);
        format!("{:?}", kind).hash(&mut hasher);
        if range.start < source_text.len() && range.end <= source_text.len() {
            source_text[range.start..range.end].hash(&mut hasher);
        }
        hasher.finish()
    }

    /// 计算文件内容哈希
    pub fn compute_content_hash(content: &str) -> u64 {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}

/// 全局符号索引
pub struct GlobalIndex {
    /// 命名空间 -> 符号名 -> 符号信息
    /// 键为 "std.math" 这种格式
    pub symbols: DashMap<String, DashMap<String, Arc<GlobalSymbol>>>,
    /// 文件 URI -> 该文件定义的符号列表
    pub file_symbols: DashMap<String, Vec<Arc<GlobalSymbol>>>,
    /// 文件 URI -> 文件索引状态（用于增量索引）
    pub file_states: DashMap<String, FileIndexState>,
    /// Trait 名称 -> 实现该 Trait 的类型列表
    /// 键为 "namespace::TraitName" 格式
    pub trait_to_implementations: DashMap<String, Vec<TraitImplementation>>,
    /// 类名称 -> 继承该类的子类列表
    /// 键为 "namespace::ClassName" 格式
    pub class_to_children: DashMap<String, Vec<String>>,
    /// 类型别名 -> 原类型名称
    /// 键为 "namespace::AliasName" 格式
    pub type_aliases: DashMap<String, String>,
}

impl GlobalIndex {
    pub fn new() -> Self {
        Self {
            symbols: DashMap::new(),
            file_symbols: DashMap::new(),
            file_states: DashMap::new(),
            trait_to_implementations: DashMap::new(),
            class_to_children: DashMap::new(),
            type_aliases: DashMap::new(),
        }
    }

    /// 检查文件是否需要重新索引
    pub fn needs_reindex(&self, uri: &str, content_hash: u64) -> bool {
        match self.file_states.get(uri) {
            Some(state) => state.content_hash != content_hash,
            None => true,
        }
    }

    /// 获取文件的旧符号哈希映射
    pub fn get_old_symbol_hashes(&self, uri: &str) -> Option<Vec<(String, u64)>> {
        self.file_states.get(uri).map(|s| s.symbol_hashes.clone())
    }

    /// 注册 Trait 实现
    pub fn register_trait_implementation(&self, trait_full_name: &str, impl_info: TraitImplementation) {
        let mut impls = self.trait_to_implementations.entry(trait_full_name.to_string()).or_insert_with(Vec::new);
        if !impls.iter().any(|i| i.implementor == impl_info.implementor && i.uri == impl_info.uri) {
            impls.push(impl_info);
        }
    }

    /// 注册类继承关系
    pub fn register_class_inheritance(&self, parent_full_name: &str, child_full_name: &str) {
        let mut children = self.class_to_children.entry(parent_full_name.to_string()).or_insert_with(Vec::new);
        if !children.contains(&child_full_name.to_string()) {
            children.push(child_full_name.to_string());
        }
    }

    /// 注册类型别名
    pub fn register_type_alias(&self, alias_full_name: &str, target_type: &str) {
        self.type_aliases.insert(alias_full_name.to_string(), target_type.to_string());
    }

    /// 获取 Trait 的所有实现
    pub fn get_trait_implementations(&self, trait_full_name: &str) -> Option<Vec<TraitImplementation>> {
        self.trait_to_implementations.get(trait_full_name).map(|v| v.clone())
    }

    /// 获取类的所有子类
    pub fn get_class_children(&self, class_full_name: &str) -> Option<Vec<String>> {
        self.class_to_children.get(class_full_name).map(|v| v.clone())
    }

    /// 获取类型别名的目标类型
    pub fn get_type_alias_target(&self, alias_full_name: &str) -> Option<String> {
        self.type_aliases.get(alias_full_name).map(|v| v.clone())
    }

    pub fn update_file_symbols(&self, uri: &str, namespace: &str, symbols: Vec<GlobalSymbol>, content_hash: u64) {
        if let Some((_, old_symbols)) = self.file_symbols.remove(uri) {
            for symbol in old_symbols {
                if let Some(ns_map) = self.symbols.get(&symbol.namespace) {
                    ns_map.remove(&symbol.name);
                }
                if let Some(ref parent) = symbol.parent_class {
                    let parent_key = format!("{}::{}", symbol.namespace, parent);
                    if let Some(mut children) = self.class_to_children.get_mut(&parent_key) {
                        children.retain(|c| c != &format!("{}::{}", symbol.namespace, symbol.name));
                    }
                }
                for trait_name in &symbol.implemented_traits {
                    let trait_key = format!("{}::{}", symbol.namespace, trait_name);
                    if let Some(mut impls) = self.trait_to_implementations.get_mut(&trait_key) {
                        impls.retain(|i| i.implementor != symbol.name);
                    }
                }
            }
        }

        let mut arc_symbols = Vec::with_capacity(symbols.len());
        let mut symbol_hashes = Vec::with_capacity(symbols.len());
        let ns_map = self.symbols.entry(namespace.to_string()).or_insert_with(DashMap::new);

        for symbol in symbols {
            if let Some(ref parent) = symbol.parent_class {
                let parent_key = format!("{}::{}", symbol.namespace, parent);
                let child_key = format!("{}::{}", symbol.namespace, symbol.name);
                self.register_class_inheritance(&parent_key, &child_key);
            }
            for trait_name in &symbol.implemented_traits {
                let trait_key = if trait_name.contains("::") {
                    trait_name.clone()
                } else {
                    format!("{}::{}", symbol.namespace, trait_name)
                };
                let impl_info = TraitImplementation {
                    implementor: symbol.name.clone(),
                    implementor_namespace: symbol.namespace.clone(),
                    uri: symbol.uri.clone(),
                    range: symbol.range.clone(),
                };
                self.register_trait_implementation(&trait_key, impl_info);
            }
            if let Some(ref target) = symbol.type_alias_target {
                let alias_key = format!("{}::{}", symbol.namespace, symbol.name);
                self.register_type_alias(&alias_key, target);
            }

            symbol_hashes.push((symbol.name.clone(), symbol.hash));
            let arc_symbol = Arc::new(symbol);
            ns_map.insert(arc_symbol.name.clone(), arc_symbol.clone());
            arc_symbols.push(arc_symbol);
        }

        self.file_symbols.insert(uri.to_string(), arc_symbols);
        self.file_states.insert(
            uri.to_string(),
            FileIndexState { uri: uri.to_string(), content_hash, symbol_hashes, namespace: namespace.to_string() },
        );
    }

    /// 移除文件的符号索引
    pub fn remove_file(&self, uri: &str) {
        if let Some((_, old_symbols)) = self.file_symbols.remove(uri) {
            for symbol in old_symbols {
                if let Some(ns_map) = self.symbols.get(&symbol.namespace) {
                    ns_map.remove(&symbol.name);
                }
            }
        }
        self.file_states.remove(uri);
    }
}
