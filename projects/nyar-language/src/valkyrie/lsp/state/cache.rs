use super::symbol::SymbolInfo;
use dashmap::DashMap;
use std::sync::Arc;

/// 语义缓存，存储已解析的类型和跨命名空间的引用
pub struct SemanticCache {
    /// 命名空间 -> 符号名 -> 语义信息 (SymbolInfo)
    pub cache: DashMap<String, DashMap<String, Arc<SymbolInfo>>>,
    /// 依赖图：符号标识符 (ns.name) -> 依赖它的符号列表 (ns.name)
    pub symbol_dependencies: DashMap<String, Vec<String>>,
    /// 文件 URI -> 该文件定义的符号标识符列表 (ns.name)
    pub file_to_symbols: DashMap<String, Vec<String>>,
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

        // 级联失效：找到所有依赖于此符号的项并移除
        let symbol_id = format!("{}.{}", namespace, name);
        if let Some((_, dependents)) = self.symbol_dependencies.remove(&symbol_id) {
            for dep in dependents {
                let parts: Vec<&str> = dep.splitn(2, '.').collect();
                if parts.len() == 2 {
                    self.invalidate_symbol(parts[0], parts[1]);
                }
            }
        }
    }

    /// 更新依赖关系
    pub fn update_dependencies(&self, dependency_ns: &str, dependency_name: &str, dependent_ns: &str, dependent_name: &str) {
        let dependency_id = format!("{}.{}", dependency_ns, dependency_name);
        let dependent_id = format!("{}.{}", dependent_ns, dependent_name);

        // 避免自依赖
        if dependency_id == dependent_id {
            return;
        }

        self.symbol_dependencies.entry(dependency_id).or_insert_with(Vec::new).push(dependent_id);
    }

    pub fn invalidate_namespace(&self, namespace: &str) {
        if let Some((_, symbols)) = self.cache.remove(namespace) {
            for (name, _) in symbols {
                let symbol_id = format!("{}.{}", namespace, name);
                self.symbol_dependencies.remove(&symbol_id);
            }
        }
    }
}
