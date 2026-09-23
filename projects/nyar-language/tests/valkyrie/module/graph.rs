use nyar_language::module::*;

use nyar_types::{Identifier, QualifiedName};

fn qn(s: &str) -> QualifiedName {
    QualifiedName::new(s.split("::").map(Identifier::new).collect())
}

#[test]
fn test_empty_graph() {
    let graph = DependencyGraph::new();
    assert!(graph.is_empty());
    assert_eq!(graph.len(), 0);
    assert!(graph.detect_cycle().is_none());
    assert_eq!(graph.topological_sort().unwrap(), Vec::<QualifiedName>::new());
}

#[test]
fn test_single_module() {
    let mut graph = DependencyGraph::new();
    graph.add_module(qn("a"));
    assert!(!graph.is_empty());
    assert_eq!(graph.len(), 1);
    assert!(graph.detect_cycle().is_none());
    let sorted = graph.topological_sort().unwrap();
    assert_eq!(sorted.len(), 1);
}

#[test]
fn test_no_cycle() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.add_dependency(qn("b"), qn("c"));
    assert!(graph.detect_cycle().is_none());

    let sorted = graph.topological_sort().unwrap();
    assert!(sorted.contains(&qn("c")));
    assert!(sorted.contains(&qn("b")));
    assert!(sorted.contains(&qn("a")));
}

#[test]
fn test_simple_cycle() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.add_dependency(qn("b"), qn("a"));
    assert!(graph.detect_cycle().is_some());
    assert!(graph.topological_sort().is_err());
}

#[test]
fn test_longer_cycle() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.add_dependency(qn("b"), qn("c"));
    graph.add_dependency(qn("c"), qn("a"));
    let cycle = graph.detect_cycle();
    assert!(cycle.is_some());
    let cycle = cycle.unwrap();
    assert!(cycle.len() >= 3);
}

#[test]
fn test_self_cycle() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("a"));
    assert!(graph.detect_cycle().is_some());
}

#[test]
fn test_dependencies() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.add_dependency(qn("a"), qn("c"));

    let deps = graph.dependencies(&qn("a")).unwrap();
    assert_eq!(deps.len(), 2);
    assert!(deps.contains(&qn("b")));
    assert!(deps.contains(&qn("c")));

    assert!(graph.dependencies(&qn("b")).unwrap().is_empty());
}

#[test]
fn test_dependents() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("c"));
    graph.add_dependency(qn("b"), qn("c"));

    let dependents = graph.dependents(&qn("c"));
    assert_eq!(dependents.len(), 2);
    assert!(dependents.contains(&qn("a")));
    assert!(dependents.contains(&qn("b")));
}

#[test]
fn test_diamond_dependency() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.add_dependency(qn("a"), qn("c"));
    graph.add_dependency(qn("b"), qn("d"));
    graph.add_dependency(qn("c"), qn("d"));

    assert!(graph.detect_cycle().is_none());
    let sorted = graph.topological_sort().unwrap();

    let d_pos = sorted.iter().position(|n| *n == qn("d")).unwrap();
    let b_pos = sorted.iter().position(|n| *n == qn("b")).unwrap();
    let c_pos = sorted.iter().position(|n| *n == qn("c")).unwrap();
    let a_pos = sorted.iter().position(|n| *n == qn("a")).unwrap();

    assert!(d_pos < b_pos);
    assert!(d_pos < c_pos);
    assert!(b_pos < a_pos);
    assert!(c_pos < a_pos);
}

#[test]
fn test_remove_module() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.add_dependency(qn("b"), qn("c"));

    assert!(graph.remove_module(&qn("b")));
    assert!(!graph.contains(&qn("b")));
    assert!(graph.dependencies(&qn("a")).unwrap().is_empty());
}

#[test]
fn test_clear() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("a"), qn("b"));
    graph.clear();
    assert!(graph.is_empty());
}

#[test]
fn test_qualified_names_with_namespace() {
    let mut graph = DependencyGraph::new();
    graph.add_dependency(qn("std::collections::HashMap"), qn("std::hash::Hash"));
    graph.add_dependency(qn("std::collections::HashSet"), qn("std::hash::Hash"));

    assert!(graph.detect_cycle().is_none());
    assert_eq!(graph.len(), 3);
}
