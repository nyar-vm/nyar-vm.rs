use std::collections::{BTreeMap, BTreeSet};

use nyar_emitter::{nyar_backend_clr::MsilType, testing::build_clr_type_defs};
use nyar_language::{AggregateLayout, AggregateLayoutPlan, FieldLayout, MirStorageKind, types::hir::ValkyrieType};

#[test]
fn value_storage_emits_msil_value_type_def() {
    let plan = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Point".to_string(),
            namespace: "geom".to_string(),
            storage: MirStorageKind::Value,
            size: 8,
            align: 4,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
            ],
        }],
        value_type_names: BTreeSet::from(["Point".to_string()]),
        type_name_to_layout: BTreeMap::from([("Point".to_string(), 1)]),
    };

    let types = build_clr_type_defs(&plan);
    assert_eq!(types.len(), 1);
    let point = &types[0];
    assert_eq!(point.full_name, "Point");
    assert_eq!(point.namespace, "geom");
    assert!(point.is_value_type);
    assert_eq!(point.fields.len(), 2);
    assert!(matches!(point.fields[0].ty, MsilType::Int32 { signed: true }));
}

#[test]
fn reference_storage_emits_msil_reference_type_def() {
    let plan = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 2,
            name: "Widget".to_string(),
            namespace: String::new(),
            storage: MirStorageKind::Reference,
            size: 8,
            align: 8,
            fields: vec![FieldLayout { name: "label".to_string(), ty: nyar::NyarType::Utf8, offset: 0, size: 8, align: 8 }],
        }],
        value_type_names: BTreeSet::new(),
        type_name_to_layout: BTreeMap::from([("Widget".to_string(), 2)]),
    };

    let types = build_clr_type_defs(&plan);
    assert_eq!(types.len(), 1);
    assert!(!types[0].is_value_type);
}
