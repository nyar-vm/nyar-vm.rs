use rusty_valkyrie::{
    ValkyrieCompiler,
    mir::MirLowerer,
    compile_pipeline::{link_reachable_dependency_mir, merge_aggregate_layouts},
    *,
};

/// 同名、同形状不能证明是同一个类型；缺身份的重复布局必须失败。
#[test]
fn identical_layout_names_without_identity_do_not_merge() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler.compile_source("micro entry() -> unit { return }").expect("源码解析");
    let mut consumer = MirLowerer::lower_module_semantic(&hir);
    let layout = AggregateLayout {
        id: 1,
        name: "Record".into(),
        namespace: "example".into(),
        key: None,
        storage: nyar_types::layout::StorageKind::Value,
        size: 0,
        align: 1,
        fields: vec![],
    };
    consumer.aggregate_layouts = AggregateLayoutPlan::default();
    consumer.aggregate_layouts.layouts.push(layout.clone());
    let dependency = consumer.clone();
    let original = consumer.clone();
    assert!(merge_aggregate_layouts(&consumer, &[dependency]).is_err());
    assert_eq!(consumer, original);
}

/// 已冻结的同一声明合同允许重复引用，不能靠名称建立等价关系。
#[test]
fn repeated_layout_declaration_merges_by_identity() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler.compile_source("micro entry() -> unit { return }").expect("源码解析");
    let mut consumer = MirLowerer::lower_module_semantic(&hir);
    consumer.aggregate_layouts = AggregateLayoutPlan::default();
    consumer.aggregate_layouts.layouts.push(AggregateLayout {
        id: 1,
        name: "Record".into(),
        namespace: "example".into(),
        key: None,
        storage: nyar_types::layout::StorageKind::Value,
        size: 0,
        align: 1,
        fields: vec![],
    });
    consumer.aggregate_layouts.declaration_to_layout.insert(nyar_types::ItemId::from_index(1).unwrap(), 1);
    let dependency = consumer.clone();
    let merged = merge_aggregate_layouts(&consumer, &[dependency]).expect("同一声明合同");
    assert_eq!(merged.layouts.len(), 1);
}

#[test]
fn dependency_linking_without_shared_instances_fails_without_mutation() {
    let compiler = ValkyrieCompiler::default();
    let consumer_hir = compiler.compile_source("micro consumer() -> unit { return }").expect("源码解析");
    let dependency_hir = compiler.compile_source("micro helper() -> unit { return }").expect("依赖源码解析");
    let mut consumer = MirLowerer::lower_module_semantic(&consumer_hir);
    let dependency = MirLowerer::lower_module_semantic(&dependency_hir);
    let original = consumer.clone();
    let error = link_reachable_dependency_mir(&mut consumer, &[dependency]).expect_err("缺少统一实例表不能链接");
    assert!(error.to_string().contains("ItemInstanceId"), "实际链接失败：{error}");
    assert_eq!(consumer, original);
}
