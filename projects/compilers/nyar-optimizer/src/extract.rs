//! 基于成本模型的 `E-Graph` 抽取。

use crate::{egraph::EGraphHost, term::AlgebraicTerm};
use std::collections::HashMap;

/// 供抽取算法使用的只读成本视图。
pub trait ExtractCost {
    /// 解析等价类代表元。
    fn find_class(&self, class: usize) -> usize;

    /// 返回等价类中的 `E-Node` 索引。
    fn enodes_in_class(&self, class: usize) -> Vec<usize>;

    /// 返回单个 `E-Node` 的基础成本。
    fn enode_kind_cost(&self, enode: usize) -> u32;

    /// 返回 `E-Node` 子等价类。
    fn enode_children(&self, enode: usize) -> Vec<usize>;

    /// 将 `E-Node` 直接转为结构化项。
    fn enode_to_term(&self, enode: usize) -> AlgebraicTerm;
}

/// 从等价类中抽取成本最低的 structured term。
pub fn extract_class(graph: &EGraphHost, class: usize) -> AlgebraicTerm {
    let root = graph.find_class(class);
    let mut class_memo = HashMap::new();
    let mut enode_memo = HashMap::new();
    extract_class_inner(graph, root, &mut class_memo, &mut enode_memo).1
}

fn extract_class_inner(
    graph: &EGraphHost,
    class: usize,
    class_memo: &mut HashMap<usize, (u32, AlgebraicTerm)>,
    enode_memo: &mut HashMap<usize, (u32, AlgebraicTerm)>,
) -> (u32, AlgebraicTerm) {
    let root = graph.find_class(class);
    if let Some(existing) = class_memo.get(&root) {
        return existing.clone();
    }

    // Placeholder while extracting cyclic classes (e.g. identity rewrites).
    class_memo.insert(root, (u32::MAX, AlgebraicTerm::Literal(0)));

    let enodes = graph.enodes_in_class(root);
    let mut best = (u32::MAX, AlgebraicTerm::Literal(0));
    for enode in enodes {
        let candidate = extract_enode(graph, enode, class_memo, enode_memo);
        if candidate.0 < best.0 || (candidate.0 == best.0 && term_to_key(&candidate.1) < term_to_key(&best.1)) {
            best = candidate;
        }
    }

    class_memo.insert(root, best.clone());
    best
}

fn extract_enode(
    graph: &EGraphHost,
    enode: usize,
    class_memo: &mut HashMap<usize, (u32, AlgebraicTerm)>,
    enode_memo: &mut HashMap<usize, (u32, AlgebraicTerm)>,
) -> (u32, AlgebraicTerm) {
    if let Some(existing) = enode_memo.get(&enode) {
        return existing.clone();
    }

    let base = graph.enode_kind_cost(enode);
    let mut total = base;
    let mut child_terms = Vec::new();
    for child in graph.enode_children(enode) {
        let (child_cost, child_term) = extract_class_inner(graph, child, class_memo, enode_memo);
        total = total.saturating_add(child_cost);
        child_terms.push(child_term);
    }
    let term = graph.enode_to_term_from_children(enode, child_terms);
    let result = (total, term);
    enode_memo.insert(enode, result.clone());
    result
}

fn term_to_key(term: &AlgebraicTerm) -> String {
    term.to_string()
}
