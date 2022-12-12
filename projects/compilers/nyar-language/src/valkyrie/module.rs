//! 模块系统核心：依赖图与模块解析错误类型。
//!
//! 本模块提供 `DependencyGraph` 用于管理模块间的依赖关系，
//! 支持循环依赖检测和拓扑排序，为构建系统提供编译顺序决策。

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use nyar_types::QualifiedName;

/// 模块解析错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    /// 找不到指定模块。
    NotFound {
        /// 缺失模块的限定名。
        name: QualifiedName,
        /// 已搜索的路径列表。
        searched: Vec<PathBuf>,
    },
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleError::NotFound { name, searched } => {
                write!(f, "模块 {} 未找到，已搜索路径：", name)?;
                for path in searched {
                    write!(f, "\n  - {}", path.display())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ModuleError {}

/// 拓扑排序错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologicalSortError {
    /// 检测到的循环路径。
    pub cycle: Vec<QualifiedName>,
}

impl fmt::Display for TopologicalSortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "检测到循环依赖：")?;
        for (i, name) in self.cycle.iter().enumerate() {
            if i > 0 {
                write!(f, " -> ")?;
            }
            write!(f, "{}", name)?;
        }
        Ok(())
    }
}

impl std::error::Error for TopologicalSortError {}

/// 模块依赖图。
///
/// 管理模块间的依赖关系，支持：
/// - 添加模块节点和依赖边
/// - 检测循环依赖
/// - 拓扑排序（依赖在前，被依赖在后）
/// - 查询直接依赖和反向依赖
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// 邻接表：模块 -> 其依赖的模块集合。
    /// `edges[a]` 包含 `b` 表示 `a` 依赖 `b`。
    edges: BTreeMap<QualifiedName, BTreeSet<QualifiedName>>,
    /// 反向邻接表：模块 -> 依赖它的模块集合。
    /// `reverse_edges[a]` 包含 `b` 表示 `b` 依赖 `a`。
    reverse_edges: BTreeMap<QualifiedName, BTreeSet<QualifiedName>>,
}

impl DependencyGraph {
    /// 创建空的依赖图。
    pub fn new() -> Self {
        Self::default()
    }

    /// 判断图是否为空。
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// 返回图中的模块数量。
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// 判断模块是否存在于图中。
    pub fn contains(&self, name: &QualifiedName) -> bool {
        self.edges.contains_key(name)
    }

    /// 添加一个模块节点。
    ///
    /// 如果模块已存在，不做任何操作。
    pub fn add_module(&mut self, name: QualifiedName) {
        self.edges.entry(name.clone()).or_default();
        self.reverse_edges.entry(name).or_default();
    }

    /// 添加一条依赖边：`from` 依赖 `to`。
    ///
    /// 如果模块不存在，会自动添加。
    pub fn add_dependency(&mut self, from: QualifiedName, to: QualifiedName) {
        self.add_module(from.clone());
        self.add_module(to.clone());
        self.edges.get_mut(&from).unwrap().insert(to.clone());
        self.reverse_edges.get_mut(&to).unwrap().insert(from);
    }

    /// 返回模块的直接依赖列表。
    ///
    /// 如果模块不存在，返回 `None`。
    pub fn dependencies(&self, name: &QualifiedName) -> Option<Vec<QualifiedName>> {
        self.edges.get(name).map(|deps| deps.iter().cloned().collect())
    }

    /// 返回依赖该模块的模块列表。
    ///
    /// 如果模块不存在，返回空列表。
    pub fn dependents(&self, name: &QualifiedName) -> Vec<QualifiedName> {
        self.reverse_edges.get(name).map(|deps| deps.iter().cloned().collect()).unwrap_or_default()
    }

    /// 移除一个模块及其所有关联边。
    ///
    /// 返回模块是否曾存在。
    pub fn remove_module(&mut self, name: &QualifiedName) -> bool {
        let existed = self.edges.remove(name).is_some();

        // 从所有依赖该模块的节点中移除这条边
        if let Some(rev_deps) = self.reverse_edges.remove(name) {
            for rev_dep in rev_deps {
                if let Some(deps) = self.edges.get_mut(&rev_dep) {
                    deps.remove(name);
                }
            }
        }

        // 从该模块的依赖中移除反向边
        if let Some(deps) = self.edges.get(name) {
            for dep in deps {
                if let Some(rev) = self.reverse_edges.get_mut(dep) {
                    rev.remove(name);
                }
            }
        }

        existed
    }

    /// 清空图中的所有模块和边。
    pub fn clear(&mut self) {
        self.edges.clear();
        self.reverse_edges.clear();
    }

    /// 检测图中是否存在循环依赖。
    ///
    /// 如果存在循环，返回循环路径；否则返回 `None`。
    pub fn detect_cycle(&self) -> Option<Vec<QualifiedName>> {
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        let mut path = Vec::new();

        for node in self.edges.keys() {
            if !visited.contains(node) {
                if let Some(cycle) = self.dfs_detect_cycle(node, &mut visited, &mut stack, &mut path) {
                    return Some(cycle);
                }
            }
        }

        None
    }

    /// DFS 递归检测循环依赖。
    fn dfs_detect_cycle(
        &self,
        node: &QualifiedName,
        visited: &mut BTreeSet<QualifiedName>,
        stack: &mut Vec<QualifiedName>,
        path: &mut Vec<QualifiedName>,
    ) -> Option<Vec<QualifiedName>> {
        if stack.contains(node) {
            // 找到循环，提取循环路径
            let start = stack.iter().position(|n| n == node).unwrap();
            let mut cycle: Vec<QualifiedName> = stack[start..].to_vec();
            cycle.push(node.clone());
            return Some(cycle);
        }

        if visited.contains(node) {
            return None;
        }

        visited.insert(node.clone());
        stack.push(node.clone());
        path.push(node.clone());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                if let Some(cycle) = self.dfs_detect_cycle(dep, visited, stack, path) {
                    return Some(cycle);
                }
            }
        }

        stack.pop();
        None
    }

    /// 对图进行拓扑排序。
    ///
    /// 返回依赖在前、被依赖在后的顺序。
    /// 如果存在循环依赖，返回错误。
    pub fn topological_sort(&self) -> Result<Vec<QualifiedName>, TopologicalSortError> {
        if let Some(cycle) = self.detect_cycle() {
            return Err(TopologicalSortError { cycle });
        }

        let mut visited = BTreeSet::new();
        let mut result = Vec::new();

        for node in self.edges.keys() {
            if !visited.contains(node) {
                self.dfs_topo_sort(node, &mut visited, &mut result);
            }
        }

        Ok(result)
    }

    /// DFS 递归拓扑排序（后序遍历）。
    fn dfs_topo_sort(&self, node: &QualifiedName, visited: &mut BTreeSet<QualifiedName>, result: &mut Vec<QualifiedName>) {
        if visited.contains(node) {
            return;
        }

        visited.insert(node.clone());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                self.dfs_topo_sort(dep, visited, result);
            }
        }

        result.push(node.clone());
    }
}
