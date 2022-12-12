//! Tests for closed class optimization.

use nyar_language::{
    optimizer::{
        ClosedClassOptimizations, ClosedClassOptimizer, DeadCodeEliminationAnalyzer, MethodInlineAnalyzer, OptimizationResult,
        StackAllocationAnalyzer, StackAllocationRejectionReason, WitnessEliminationPass,
    },
    types::{
        Identifier,
        hir::{HirDocumentation, HirField, HirStruct, HirVisibility, ValkyrieType},
    },
};

fn int64_type() -> ValkyrieType {
    ValkyrieType::Integer64 { signed: true }
}

fn create_closed_class(name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: int64_type(),
            visibility: HirVisibility::public(),
            is_mutable: false,
        }],
        methods: vec![],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: false,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn create_open_class(name: &str) -> HirStruct {
    let mut class = create_closed_class(name);
    class.is_open = true;
    class
}

fn create_sealed_class(name: &str) -> HirStruct {
    let mut class = create_closed_class(name);
    class.is_sealed = true;
    class
}

fn create_abstract_class(name: &str) -> HirStruct {
    let mut class = create_closed_class(name);
    class.is_abstract = true;
    class
}

fn create_final_class(name: &str) -> HirStruct {
    let mut class = create_closed_class(name);
    class.is_final = true;
    class
}

#[test]
fn test_closed_class_can_inline_methods() {
    let class = create_closed_class("ClosedClass");
    let opts = ClosedClassOptimizations::analyze(&class);

    assert!(opts.can_inline_all_methods);
    assert!(opts.no_witness_dispatch_needed);
    assert!(opts.can_stack_allocate);
    assert!(opts.dead_code_elimination);
}

#[test]
fn test_open_class_cannot_inline() {
    let class = create_open_class("OpenClass");
    let opts = ClosedClassOptimizations::analyze(&class);

    assert!(!opts.can_inline_all_methods);
    assert!(!opts.no_witness_dispatch_needed);
    assert!(!opts.can_stack_allocate);
    assert!(!opts.dead_code_elimination);
}

#[test]
fn test_sealed_class_cannot_inline() {
    let class = create_sealed_class("SealedClass");
    let opts = ClosedClassOptimizations::analyze(&class);

    assert!(!opts.can_inline_all_methods);
    assert!(!opts.no_witness_dispatch_needed);
}

#[test]
fn test_abstract_class_cannot_inline() {
    let class = create_abstract_class("AbstractClass");
    let opts = ClosedClassOptimizations::analyze(&class);

    assert!(!opts.can_inline_all_methods);
    assert!(!opts.no_witness_dispatch_needed);
}

#[test]
fn test_final_class_is_closed() {
    let class = create_final_class("FinalClass");
    assert!(class.is_closed());

    let opts = ClosedClassOptimizations::analyze(&class);
    assert!(opts.can_inline_all_methods);
    assert!(opts.no_witness_dispatch_needed);
}

#[test]
fn test_method_inline_analyzer() {
    let mut analyzer = MethodInlineAnalyzer::new();
    let closed_class = create_closed_class("Closed");
    let open_class = create_open_class("Open");

    analyzer.register_closed_class(&closed_class);
    analyzer.register_closed_class(&open_class);

    assert!(analyzer.can_inline_method(&Identifier::new("Closed")));
    assert!(!analyzer.can_inline_method(&Identifier::new("Open")));
    assert_eq!(analyzer.closed_class_count(), 1);
}

#[test]
fn test_method_inline_analyzer_clear() {
    let mut analyzer = MethodInlineAnalyzer::new();
    analyzer.register_closed_class(&create_closed_class("Closed"));

    assert_eq!(analyzer.closed_class_count(), 1);

    analyzer.clear();
    assert_eq!(analyzer.closed_class_count(), 0);
}

#[test]
fn test_witness_elimination_pass() {
    let mut pass = WitnessEliminationPass::new();
    let classes = vec![create_closed_class("A"), create_open_class("B"), create_closed_class("C")];

    let count = pass.run(&classes);
    assert_eq!(count, 2);
    assert!(pass.is_eliminated(&Identifier::new("A")));
    assert!(!pass.is_eliminated(&Identifier::new("B")));
    assert!(pass.is_eliminated(&Identifier::new("C")));
}

#[test]
fn test_witness_elimination_pass_clear() {
    let mut pass = WitnessEliminationPass::new();
    pass.run(&[create_closed_class("A")]);

    assert_eq!(pass.eliminated_witness_tables().len(), 1);

    pass.clear();
    assert_eq!(pass.eliminated_witness_tables().len(), 0);
}

#[test]
fn test_stack_allocation_analyzer() {
    let mut analyzer = StackAllocationAnalyzer::new();
    let closed_class = create_closed_class("Closed");
    let open_class = create_open_class("Open");

    assert!(analyzer.analyze(&closed_class));
    assert!(!analyzer.analyze(&open_class));

    assert_eq!(analyzer.stack_allocation_candidates().len(), 1);
    assert_eq!(analyzer.rejected_classes().len(), 1);

    match &analyzer.rejected_classes()[0].1 {
        StackAllocationRejectionReason::NotClosed => {}
        _ => panic!("Expected NotClosed rejection reason"),
    }
}

#[test]
fn test_stack_allocation_analyzer_with_size() {
    let mut analyzer = StackAllocationAnalyzer::new();

    let small_class = create_closed_class("Small");
    assert!(analyzer.analyze_with_size(&small_class, 1024));

    let mut large_class = create_closed_class("Large");
    large_class.fields = vec![
        HirField {
            name: Identifier::new("f1"),
            doc: HirDocumentation::default(),
            ty: int64_type(),
            visibility: HirVisibility::public(),
            is_mutable: false,
        };
        200
    ];

    assert!(!analyzer.analyze_with_size(&large_class, 100));
}

#[test]
fn test_stack_allocation_is_candidate() {
    let mut analyzer = StackAllocationAnalyzer::new();
    let class = create_closed_class("Candidate");

    analyzer.analyze(&class);
    assert!(analyzer.is_candidate(&Identifier::new("Candidate")));
    assert!(!analyzer.is_candidate(&Identifier::new("Unknown")));
}

#[test]
fn test_dead_code_elimination_analyzer() {
    let mut analyzer = DeadCodeEliminationAnalyzer::new();

    let closed_class = create_closed_class("Closed");
    let open_class = create_open_class("Open");

    analyzer.register(&closed_class);
    analyzer.register(&open_class);

    assert!(analyzer.can_eliminate(&Identifier::new("Closed")));
    assert!(!analyzer.can_eliminate(&Identifier::new("Open")));
}

#[test]
fn test_dead_code_record_unused_methods() {
    let mut analyzer = DeadCodeEliminationAnalyzer::new();
    analyzer.register(&create_closed_class("Closed"));

    analyzer.record_unused_methods(Identifier::new("Closed"), vec![Identifier::new("unused1"), Identifier::new("unused2")]);

    assert_eq!(analyzer.total_unused_count(), 2);
}

#[test]
fn test_closed_class_optimizer() {
    let mut optimizer = ClosedClassOptimizer::new();
    let class = create_closed_class("TestClass");

    let result = optimizer.analyze_class(&class);

    assert_eq!(result.class_name, Identifier::new("TestClass"));
    assert!(result.optimizations.can_inline_all_methods);
    assert!(result.can_stack_allocate);
}

#[test]
fn test_closed_class_optimizer_multiple_classes() {
    let mut optimizer = ClosedClassOptimizer::new();
    let classes = vec![create_closed_class("A"), create_open_class("B"), create_closed_class("C")];

    let results = optimizer.analyze_classes(&classes);

    assert_eq!(results.len(), 3);
    assert!(results[0].optimizations.can_inline_all_methods);
    assert!(!results[1].optimizations.can_inline_all_methods);
    assert!(results[2].optimizations.can_inline_all_methods);
}

#[test]
fn test_closed_class_optimizer_witness_elimination() {
    let mut optimizer = ClosedClassOptimizer::new();
    let classes = vec![create_closed_class("A"), create_open_class("B"), create_closed_class("C")];

    let count = optimizer.run_witness_elimination(&classes);
    assert_eq!(count, 2);
}

#[test]
fn test_closed_class_optimizer_clear() {
    let mut optimizer = ClosedClassOptimizer::new();
    optimizer.analyze_class(&create_closed_class("A"));
    optimizer.run_witness_elimination(&[create_closed_class("B")]);

    optimizer.clear();

    assert_eq!(optimizer.method_inline_analyzer().closed_class_count(), 0);
    assert_eq!(optimizer.witness_elimination().eliminated_witness_tables().len(), 0);
    assert_eq!(optimizer.stack_allocator().stack_allocation_candidates().len(), 0);
}

#[test]
fn test_optimization_count() {
    let closed_opts = ClosedClassOptimizations::analyze(&create_closed_class("Closed"));
    assert_eq!(closed_opts.optimization_count(), 4);

    let open_opts = ClosedClassOptimizations::analyze(&create_open_class("Open"));
    assert_eq!(open_opts.optimization_count(), 0);
}

#[test]
fn test_has_optimizations() {
    let closed_opts = ClosedClassOptimizations::analyze(&create_closed_class("Closed"));
    assert!(closed_opts.has_optimizations());

    let open_opts = ClosedClassOptimizations::analyze(&create_open_class("Open"));
    assert!(!open_opts.has_optimizations());
}
