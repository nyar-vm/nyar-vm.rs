use super::symbol::SymbolInfo;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SymbolRef {
    namespace: String,
    name: String,
}

impl SymbolRef {
    fn new(namespace: &str, name: &str) -> Self {
        Self { namespace: namespace.to_string(), name: name.to_string() }
    }
}

/// 语义缓存，存储已解析的类型和跨命名空间的引用
pub struct SemanticCache {
    /// 命名空间 -> 符号名 -> 语义信息 (SymbolInfo)
    pub cache: DashMap<String, DashMap<String, Arc<SymbolInfo>>>,
    /// 依赖图：被依赖符号 -> 依赖它的符号列表
    pub symbol_dependencies: DashMap<SymbolRef, Vec<SymbolRef>>,
    /// 文件 URI -> 该文件定义的符号标识符列表
    pub file_to_symbols: DashMap<String, Vec<SymbolRef>>,
}

impl SemanticCache {
    pub fn new() -> Self {
        Self { cache: DashMap::new(), symbol_dependencies: DashMap::new(), file_to_symbols: DashMap::new() }
    }

    /// 失效特定命名空间下的特定符号
    pub fn invalidate_symbol(&self, namespace: &str, name: &str) {
        if let Some(ns_map) = self.cache.get(namespace) {
            ns_map.remove(name);
        }

        let symbol_id = SymbolRef::new(namespace, name);
        if let Some((_, dependents)) = self.symbol_dependencies.remove(&symbol_id) {
            for dep in dependents {
                self.invalidate_symbol(&dep.namespace, &dep.name);
            }
        }
    }

    /// 更新依赖关系
    pub fn update_dependencies(&self, dependency_ns: &str, dependency_name: &str, dependent_ns: &str, dependent_name: &str) {
        let dependency_id = SymbolRef::new(dependency_ns, dependency_name);
        let dependent_id = SymbolRef::new(dependent_ns, dependent_name);

        if dependency_id == dependent_id {
            return;
        }

        self.symbol_dependencies.entry(dependency_id).or_insert_with(Vec::new).push(dependent_id);
    }

    pub fn invalidate_namespace(&self, namespace: &str) {
        if let Some((_, symbols)) = self.cache.remove(namespace) {
            for (name, _) in symbols {
                let symbol_id = SymbolRef::new(namespace, &name);
                self.symbol_dependencies.remove(&symbol_id);
            }
        }
    }
}
