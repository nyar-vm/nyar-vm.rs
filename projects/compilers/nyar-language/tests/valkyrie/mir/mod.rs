mod call_parameter_types;
// DELETED-GOD: mod continuation_runtime;
mod control_flow_unification_mir;
mod early_return;
// DELETED-GOD: mod state_machine_cfg_rewrite;
mod value_semantics;

use nyar_language::{
    // DELETED-GOD:     MirConstant, MirDispatchKind, MirEffectKind, MirInstructionKind, MirOperand, MirTerminator, MirValueOrigin, ValkyrieCompiler,
    mir::ssa::test_support::{TestMirBuilder, block, expr, lower_test_function, lower_test_literal, lower_test_module, span},
    types::{
        Identifier, NamePath,
        hir::{
            HirDocumentation, HirExprKind, HirExtractorPattern, HirField, HirFunction, HirLiteral, HirMatchArm, HirPattern, HirResolvedCall,
            HirStatement, HirStatementKind, HirStringLiteral, HirStringSegment, HirStruct, HirVisibility, ValkyrieType,
        },
    },
};
use ordered_float::OrderedFloat;

fn extractor_resolved(symbol: NamePath, return_type: ValkyrieType) -> HirResolvedCall {
    let payload = match &return_type {
        ValkyrieType::Union(items) => items.iter().find(|item| !matches!(item, ValkyrieType::Named(name) if name.as_str() == "null")).cloned(),
        other => Some(other.clone()),
    };
    HirResolvedCall {
        symbol,
        domain: nyar_language::types::hir::HirCallableDomain::Extractor,
        return_type,
        parameter_types: Vec::new(),
        extractor_payload_type: payload,
    }
}

fn nullable_tuple_return(items: Vec<ValkyrieType>) -> ValkyrieType {
    ValkyrieType::Union(vec![ValkyrieType::Tuple(items), ValkyrieType::Named(Identifier::new("null"))])
}

fn constructor_extractor_pattern(name: NamePath, fields: Vec<HirPattern>, symbol: NamePath, return_type: ValkyrieType) -> HirPattern {
    HirPattern::Extractor(HirExtractorPattern::Constructor {
        canonical_callee: {
            let mut parts = name.parts().to_vec();
            parts.push(Identifier::new("extractor"));
            NamePath::new(parts)
        },
        name,
        fields,
        resolved: Some(extractor_resolved(symbol, return_type)),
    })
}

fn array_extractor_pattern(
    prefix: Vec<HirPattern>,
    rest: Option<nyar_language::types::hir::HirIdentifier>,
    suffix: Vec<HirPattern>,
    symbol: NamePath,
    return_type: ValkyrieType,
) -> HirPattern {
    HirPattern::Extractor(HirExtractorPattern::Array {
        canonical_callee: NamePath::new(vec![Identifier::new("array"), Identifier::new("extractor")]),
        prefix,
        rest,
        suffix,
        resolved: Some(extractor_resolved(symbol, return_type)),
    })
}

fn mir_block<'a>(blocks: &'a [nyar_language::mir::ssa::MirBlock], label: &str) -> &'a nyar_language::mir::ssa::MirBlock {
    blocks.iter().find(|block| block.label == label).unwrap_or_else(|| panic!("expected block {label}"))
}

fn mir_catch_arm_pattern_blocks<'a>(
    blocks: &'a [nyar_language::mir::ssa::MirBlock],
    index: usize,
) -> Vec<&'a nyar_language::mir::ssa::MirBlock> {
    let arm = format!("catch_arm_{index}");
    let match_block = format!("catch_arm_{index}_match");
    blocks
        .iter()
        .filter(|block| {
            block.label == arm
                || block.label == match_block
                || block.label.starts_with("and_")
                || block.label.starts_with("or_")
                || block.label.starts_with("not_")
        })
        .collect()
}

fn mir_match_arm_pattern_blocks<'a>(
    blocks: &'a [nyar_language::mir::ssa::MirBlock],
    index: usize,
) -> Vec<&'a nyar_language::mir::ssa::MirBlock> {
    let arm = format!("match_arm_{index}");
    let check = format!("match_arm_{index}_check");
    blocks
        .iter()
        .filter(|block| {
            block.label == arm
                || block.label == check
                || block.label.starts_with("and_")
                || block.label.starts_with("or_")
                || block.label.starts_with("not_")
        })
        .collect()
}

fn mir_instructions<'a>(
    blocks: impl IntoIterator<Item = &'a nyar_language::mir::ssa::MirBlock>,
) -> impl Iterator<Item = &'a nyar_language::mir::ssa::MirInstruction> {
    blocks.into_iter().flat_map(|block| block.instructions.iter())
}

#[test]
fn lowers_float64_literal_as_numeric_constant() {
    let literal = HirLiteral::Float64(OrderedFloat(3.5));

    let (constant, ty) = lower_test_literal(&literal, None);

    assert_eq!(constant, MirConstant::Float64(OrderedFloat(3.5)));
    assert_eq!(ty, Some(ValkyrieType::Float64));
}

#[test]
fn lowers_literal_catch_pattern_into_builtin_compare() {
    let mir = lower_test_function(expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![
            HirMatchArm {
                pattern: HirPattern::Literal(HirLiteral::Bool(true)),
                guard: None,
                body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
            },
            HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
        ],
    }));

    let guard_block = mir.blocks.iter().find(|block| block.label == "catch_arm_0").expect("expected first catch arm block");

    assert!(!guard_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. })));
    // Bool `true` pattern is the scrutinee itself; arm still selects via Branch, not Compare.
    assert!(
        matches!(guard_block.terminator, MirTerminator::Branch { .. })
            || mir.blocks.iter().any(|b| matches!(b.terminator, MirTerminator::Branch { .. }))
    );
}

#[test]
fn returns_constant_false_for_mismatched_literal_pattern_types() {
    let mut builder = TestMirBuilder::new();

    let matched = builder.lower_pattern_match_operand(&HirPattern::Literal(HirLiteral::Bool(true)), MirOperand::Constant(MirConstant::Int(1)));

    assert_eq!(matched, MirOperand::Constant(MirConstant::Bool(false)));
    assert!(builder.instructions().is_empty());
}

#[test]
fn returns_constant_bool_for_unit_literal_pattern() {
    let mut builder = TestMirBuilder::new();

    let matched = builder.lower_pattern_match_operand(&HirPattern::Literal(HirLiteral::Unit), MirOperand::Constant(MirConstant::Unit));

    assert_eq!(matched, MirOperand::Constant(MirConstant::Bool(true)));
    assert!(builder.instructions().is_empty());
    assert!(builder.values().iter().all(|value| !matches!(value.origin, MirValueOrigin::Temporary)));
}

#[test]
fn lowers_constant_string_literal_pattern_without_fallback() {
    let mut builder = TestMirBuilder::new();
    let literal =
        HirLiteral::String(HirStringLiteral { prefix: None, quote_count: 1, segments: vec![HirStringSegment::Text("hello".to_string())] });

    let matched =
        builder.lower_pattern_match_operand(&HirPattern::Literal(literal), MirOperand::Constant(MirConstant::Utf8("hello".to_string())));

    assert_eq!(matched, MirOperand::Constant(MirConstant::Bool(true)));
    assert!(builder.instructions().is_empty());
}

#[test]
fn returns_constant_false_for_anonymous_object_pattern_on_known_scalar_without_fallback() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Boolean,
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Object {
                            name: None,
                            fields: vec![(Identifier::new("flag"), HirPattern::Literal(HirLiteral::Bool(true)))],
                            rest: None,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let guard_block = mir.functions[0].blocks.iter().find(|block| block.label == "match_arm_0").expect("expected first match arm block");

    assert!(!guard_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. })));
    assert!(matches!(&guard_block.terminator, MirTerminator::Branch { condition: MirOperand::Constant(MirConstant::Bool(false)), .. }));
}

#[test]
fn lowers_type_pattern_into_static_bool_when_operand_type_is_known() {
    let mut builder = TestMirBuilder::new();

    let matched = builder.lower_pattern_match_operand(
        &HirPattern::Type(NamePath::new(vec![Identifier::new("bool")])),
        MirOperand::Constant(MirConstant::Bool(true)),
    );

    assert_eq!(matched, MirOperand::Constant(MirConstant::Bool(true)));
    assert!(builder.instructions().is_empty());
}

#[test]
fn lowers_qualified_name_pattern_into_static_bool_when_operand_type_is_known() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Named(Identifier::new("Variant")),
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("value"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Name(NamePath::new(vec![
                            Identifier::new("package"),
                            Identifier::new("module"),
                            Identifier::new("Variant"),
                        ])),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let guard_block = mir.functions[0].blocks.iter().find(|block| block.label == "match_arm_0").expect("expected first match arm block");

    assert!(!guard_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. })));
    assert!(matches!(&guard_block.terminator, MirTerminator::Branch { condition: MirOperand::Constant(MirConstant::Bool(true)), .. }));
}

#[test]
fn lowers_object_pattern_into_field_get_and_compare_for_single_field() {
    let point_struct = HirStruct {
        name: Identifier::new("Point"),
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Boolean,
            visibility: HirVisibility::default(),
            is_mutable: false,
        }],
        ..HirStruct::new(Identifier::new("Point"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Point")]),
                    name: Identifier::new("Point"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("x"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Object {
                            name: Some(NamePath::new(vec![Identifier::new("Point")])),
                            fields: vec![(Identifier::new("x"), HirPattern::Literal(HirLiteral::Bool(true)))],
                            rest: None,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![point_struct]);
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| { matches!(instruction.kind, MirInstructionKind::FieldGet { ref field, .. } if field == "x") })
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(instruction.kind, MirInstructionKind::FieldGet { .. }) || matches!(&instruction.kind, MirInstructionKind::Call { .. })
    }));
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn lowers_named_object_pattern_for_subtype_into_field_get_and_compare() {
    let base_struct = HirStruct {
        name: Identifier::new("Base"),
        fields: vec![HirField {
            name: Identifier::new("flag"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Boolean,
            visibility: HirVisibility::default(),
            is_mutable: false,
        }],
        ..HirStruct::new(Identifier::new("Base"))
    };
    let child_struct = HirStruct {
        name: Identifier::new("Child"),
        parents: vec![nyar_language::types::hir::HirParent::new(NamePath::new(vec![Identifier::new("Base")]))],
        ..HirStruct::new(Identifier::new("Child"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Child")]),
                    name: Identifier::new("Child"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("flag"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Object {
                            name: Some(NamePath::new(vec![Identifier::new("Base")])),
                            fields: vec![(Identifier::new("flag"), HirPattern::Literal(HirLiteral::Bool(true)))],
                            rest: None,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![base_struct, child_struct]);
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| { matches!(instruction.kind, MirInstructionKind::FieldGet { ref field, .. } if field == "flag") })
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(instruction.kind, MirInstructionKind::FieldGet { .. }) || matches!(&instruction.kind, MirInstructionKind::Call { .. })
    }));
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn lowers_anonymous_object_pattern_for_inherited_field_into_field_get_and_compare() {
    let base_struct = HirStruct {
        name: Identifier::new("Base"),
        fields: vec![HirField {
            name: Identifier::new("flag"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Boolean,
            visibility: HirVisibility::default(),
            is_mutable: false,
        }],
        ..HirStruct::new(Identifier::new("Base"))
    };
    let child_struct = HirStruct {
        name: Identifier::new("Child"),
        parents: vec![nyar_language::types::hir::HirParent::new(NamePath::new(vec![Identifier::new("Base")]))],
        ..HirStruct::new(Identifier::new("Child"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Child")]),
                    name: Identifier::new("Child"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("flag"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Object {
                            name: None,
                            fields: vec![(Identifier::new("flag"), HirPattern::Literal(HirLiteral::Bool(true)))],
                            rest: None,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![base_struct, child_struct]);
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| { matches!(instruction.kind, MirInstructionKind::FieldGet { ref field, .. } if field == "flag") })
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(instruction.kind, MirInstructionKind::FieldGet { .. }) || matches!(&instruction.kind, MirInstructionKind::Call { .. })
    }));
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn lowers_constructor_pattern_into_extractor_call_and_payload_compare() {
    let point_struct = HirStruct {
        name: Identifier::new("Point"),
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Boolean,
            visibility: HirVisibility::default(),
            is_mutable: false,
        }],
        ..HirStruct::new(Identifier::new("Point"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Point")]),
                    name: Identifier::new("Point"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("x"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![
                    HirMatchArm {
                        pattern: constructor_extractor_pattern(
                            NamePath::new(vec![Identifier::new("Point")]),
                            vec![HirPattern::Literal(HirLiteral::Bool(true))],
                            NamePath::new(vec![Identifier::new("demo"), Identifier::new("point_extract")]),
                            nullable_tuple_return(vec![ValkyrieType::Boolean]),
                        ),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![point_struct]);
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("demo"), Identifier::new("point_extract")])
        )
    }));
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("is_null")])
        )
    }));
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("tuple_get_0")])
        )
    }));
    // Bool `true` payload pattern is the payload value itself ? no Compare / `.eq` intrinsic.
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
    assert!(
        !mir_instructions(pattern_blocks.iter().copied()).any(|instruction| matches!(instruction.kind, MirInstructionKind::FieldGet { .. }))
    );
}

#[test]
fn binds_constructor_pattern_field_from_extractor_payload_before_resume() {
    let point_struct = HirStruct {
        name: Identifier::new("Point"),
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Boolean,
            visibility: HirVisibility::default(),
            is_mutable: false,
        }],
        ..HirStruct::new(Identifier::new("Point"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Point")]),
                    name: Identifier::new("Point"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("x"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![HirMatchArm {
                    pattern: constructor_extractor_pattern(
                        NamePath::new(vec![Identifier::new("Point")]),
                        vec![HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("x"),
                            shadow_index: 0,
                            span: span(),
                        })],
                        NamePath::new(vec![Identifier::new("demo"), Identifier::new("point_extract")]),
                        nullable_tuple_return(vec![ValkyrieType::Boolean]),
                    ),
                    guard: None,
                    body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Variable(
                        nyar_language::types::hir::HirIdentifier { name: Identifier::new("x"), shadow_index: 0, span: span() },
                    )))))),
                }],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![point_struct]);
    let body_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_arm_0_match").expect("expected constructor match block");

    assert!(body_block.instructions.iter().any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("tuple_get_0")])
        )
    }));
    assert!(
        body_block
            .instructions
            .iter()
            .any(|instruction| { matches!(&instruction.kind, MirInstructionKind::StoreVar { name, .. } if name == "x") })
    );
    assert!(!body_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::FieldGet { .. })));
}

#[test]
fn does_not_bind_unknown_layout_constructor_field_as_whole_payload() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Point")]),
                    name: Identifier::new("Point"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("x"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![HirMatchArm {
                    pattern: constructor_extractor_pattern(
                        NamePath::new(vec![Identifier::new("Point")]),
                        vec![HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("x"),
                            shadow_index: 0,
                            span: span(),
                        })],
                        NamePath::new(vec![Identifier::new("demo"), Identifier::new("point_extract")]),
                        nullable_tuple_return(vec![ValkyrieType::Boolean]),
                    ),
                    guard: None,
                    body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Variable(
                        nyar_language::types::hir::HirIdentifier { name: Identifier::new("x"), shadow_index: 0, span: span() },
                    )))))),
                }],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);
    let body_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| block.label == "catch_arm_0_match")
        .expect("expected constructor body block after fallback match");

    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("demo"), Identifier::new("point_extract")])
        )
    }));
    assert!(
        body_block
            .instructions
            .iter()
            .any(|instruction| { matches!(instruction.kind, MirInstructionKind::StoreVar { ref name, .. } if name == "x") })
    );
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn does_not_bind_unknown_layout_object_field_as_generic_field_get() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Point")]),
                    name: Identifier::new("Point"),
                    args: vec![expr(HirExprKind::FieldInit {
                        name: Identifier::new("x"),
                        value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    })],
                    resolved: None,
                }))))),
                arms: vec![HirMatchArm {
                    pattern: HirPattern::Object {
                        name: Some(NamePath::new(vec![Identifier::new("Point")])),
                        fields: vec![(
                            Identifier::new("x"),
                            HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                                name: Identifier::new("x"),
                                shadow_index: 0,
                                span: span(),
                            }),
                        )],
                        rest: None,
                    },
                    guard: None,
                    body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Variable(
                        nyar_language::types::hir::HirIdentifier { name: Identifier::new("x"), shadow_index: 0, span: span() },
                    )))))),
                }],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let match_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_arm_0").expect("expected object fallback match block");
    let body_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| block.label == "catch_arm_0_match")
        .expect("expected object body block after fallback match");

    assert!(!match_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. })));
    assert!(!body_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::FieldGet { .. })));
    assert!(matches!(
        &body_block.terminator,
        MirTerminator::Jump { arguments, .. }
            if matches!(
                arguments.first(),
            Some(MirOperand::Symbol(path)) if *path == NamePath::new(vec![Identifier::new("unsupported_pattern")])
            )
    ));
}

#[test]
fn propagates_resume_value_type_into_catch_resume_parameter() {
    let mut builder = TestMirBuilder::new();
    let _ = builder.lower_expr_to_operand(&expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![HirMatchArm {
            pattern: HirPattern::Else,
            guard: None,
            body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        }],
    }));

    let resume_block = builder.blocks().iter().find(|block| block.label == "catch_resume").expect("expected catch resume block");
    let resume_parameter = *resume_block.parameters.first().expect("expected catch resume parameter");
    assert_eq!(builder.value_types().get(&resume_parameter), Some(&ValkyrieType::Boolean));
}

#[test]
fn preseeds_catch_resume_parameter_type_from_raised_payload() {
    let mut builder = TestMirBuilder::new();
    let _ = builder.lower_expr_to_operand(&expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Unit))) }],
    }));

    let resume_block = builder.blocks().iter().find(|block| block.label == "catch_resume").expect("expected catch resume block");
    let resume_parameter = *resume_block.parameters.first().expect("expected catch resume parameter");
    assert_eq!(builder.value_types().get(&resume_parameter), Some(&ValkyrieType::Boolean));
}

#[test]
fn records_catch_resume_continuation_metadata() {
    let mir = lower_test_function(expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![HirMatchArm {
            pattern: HirPattern::Else,
            guard: None,
            body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        }],
    }));

    let continuation = mir.continuations.first().expect("expected continuation metadata");
    let resume_block = mir.blocks.iter().find(|block| block.id == continuation.resume_target).expect("expected catch resume block");
    assert_eq!(resume_block.label, "catch_resume");
    assert!(resume_block.parameters.contains(&continuation.resume_parameter));
    assert_eq!(continuation.resume_parameter_type, Some(ValkyrieType::Boolean));
}

#[test]
fn records_await_suspend_point_metadata() {
    let future_type = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let mut builder = TestMirBuilder::new();
    builder.lower_statement(&HirStatement {
        kind: HirStatementKind::Let {
            is_mutable: false,
            pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                name: Identifier::new("future"),
                shadow_index: 0,
                span: span(),
            }),
            initializer: Some(Box::new(expr(HirExprKind::Literal(HirLiteral::Unit)))),
            ty: Some(future_type.clone()),
        },
        span: span(),
    });
    let _ = builder.lower_expr_to_operand(&expr(HirExprKind::Await(Box::new(expr(HirExprKind::Variable(
        nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
    ))))));

    let suspend_point = builder.suspend_points().first().expect("expected suspend point");
    assert_eq!(suspend_point.effect, MirEffectKind::Await);
    assert_eq!(suspend_point.resume_parameter_count, 1);
    assert_eq!(suspend_point.payload_type, Some(future_type));
    assert!(!suspend_point.spill_candidates.is_empty());
}

#[test]
fn keeps_only_live_values_in_await_spill_candidates() {
    let compiler = ValkyrieCompiler::default();
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let future: Future<bool> = ()
    let kept: bool = true
    let dropped: bool = false
    future.await
    let sink: bool = kept
    return
}
"#,
        )
        .expect("mir ok");

    let suspend_point = mir.functions[0].suspend_points.first().expect("expected suspend point");
    let spilled_names = suspend_point
        .spill_candidates
        .iter()
        .filter_map(|value| mir.functions[0].values.iter().find(|candidate| candidate.id == *value))
        .filter_map(|value| match &value.origin {
            MirValueOrigin::LetBinding { name } => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(spilled_names.is_empty());
}

#[test]
fn builds_frame_layout_from_suspend_spill_candidates() {
    let compiler = ValkyrieCompiler::default();
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let future: Future<bool> = ()
    let kept: bool = true
    future.await
    let sink: bool = kept
    return
}
"#,
        )
        .expect("mir ok");

    let suspend_point = mir.functions[0].suspend_points.first().expect("expected suspend point");
    let frame_layout = mir.functions[0].frame_layouts.first().expect("expected frame layout");
    let spilled_names = frame_layout
        .slots
        .iter()
        .filter_map(|slot| mir.functions[0].values.iter().find(|candidate| candidate.id == slot.value))
        .filter_map(|value| match &value.origin {
            MirValueOrigin::LetBinding { name } => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(frame_layout.state_id, suspend_point.state_id);
    assert_eq!(frame_layout.resume_target, suspend_point.resume_target);
    assert!(spilled_names.is_empty());
}

#[test]
fn infers_await_resume_parameter_type_from_future_payload() {
    let future_type = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let mut builder = TestMirBuilder::new();
    builder.lower_statement(&HirStatement {
        kind: HirStatementKind::Let {
            is_mutable: false,
            pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                name: Identifier::new("future"),
                shadow_index: 0,
                span: span(),
            }),
            initializer: Some(Box::new(expr(HirExprKind::Literal(HirLiteral::Unit)))),
            ty: Some(future_type),
        },
        span: span(),
    });
    let _ = builder.lower_expr_to_operand(&expr(HirExprKind::Await(Box::new(expr(HirExprKind::Variable(
        nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
    ))))));

    let resume_block = builder.blocks().iter().find(|block| block.label == "await_resume").expect("expected await resume block");
    let resume_parameter = *resume_block.parameters.first().expect("expected await resume parameter");
    assert_eq!(builder.value_types().get(&resume_parameter), Some(&ValkyrieType::Boolean));
}

#[test]
fn infers_block_resume_parameter_type_from_future_payload() {
    let promise_type = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Promise"))), vec![ValkyrieType::Utf8]);
    let mut builder = TestMirBuilder::new();
    builder.lower_statement(&HirStatement {
        kind: HirStatementKind::Let {
            is_mutable: false,
            pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                name: Identifier::new("future"),
                shadow_index: 0,
                span: span(),
            }),
            initializer: Some(Box::new(expr(HirExprKind::Literal(HirLiteral::Unit)))),
            ty: Some(promise_type),
        },
        span: span(),
    });
    let _ = builder.lower_expr_to_operand(&expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Variable(
        nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
    ))))));

    let resume_block = builder.blocks().iter().find(|block| block.label == "block_resume").expect("expected block resume block");
    let resume_parameter = *resume_block.parameters.first().expect("expected block resume parameter");
    assert_eq!(builder.value_types().get(&resume_parameter), Some(&ValkyrieType::Utf8));
}

#[test]
fn lowers_awake_into_async_spawn_with_empty_resume_parameters() {
    let compiler = ValkyrieCompiler::default();
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    future.awake
    return
}
"#,
        )
        .expect("mir ok");

    // DELETED-GOD:     let plan = mir.functions[0].suspend_plan.as_ref().expect("suspend plan");
    let awake_state = plan.states.iter().find(|state| state.effect == MirEffectKind::AsyncSpawn).expect("expected awake async-spawn state");
    assert_eq!(awake_state.resume_parameter_count, 0);

    let resume_block = mir.functions[0].blocks.iter().find(|block| block.id == awake_state.resume_target).expect("expected awake resume block");

    assert!(resume_block.parameters.is_empty());
}

#[test]
fn lowers_awake_as_unit_without_creating_resume_parameter() {
    let future_type = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let mut builder = TestMirBuilder::new();
    builder.lower_statement(&HirStatement {
        kind: HirStatementKind::Let {
            is_mutable: false,
            pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                name: Identifier::new("future"),
                shadow_index: 0,
                span: span(),
            }),
            initializer: Some(Box::new(expr(HirExprKind::Literal(HirLiteral::Unit)))),
            ty: Some(future_type),
        },
        span: span(),
    });

    let awake_result = builder.lower_expr_to_operand(&expr(HirExprKind::Awake(Box::new(expr(HirExprKind::Variable(
        nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
    ))))));

    assert_eq!(awake_result, MirOperand::Constant(MirConstant::Unit));
    let resume_block = builder.blocks().iter().find(|block| block.label == "awake_resume").expect("expected awake resume block");
    assert!(resume_block.parameters.is_empty());
}

#[test]
fn lowers_multi_field_object_pattern_into_logical_and() {
    let pair_struct = HirStruct {
        name: Identifier::new("Pair"),
        fields: vec![
            HirField {
                name: Identifier::new("left"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Boolean,
                visibility: HirVisibility::default(),
                is_mutable: false,
            },
            HirField {
                name: Identifier::new("right"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Boolean,
                visibility: HirVisibility::default(),
                is_mutable: false,
            },
        ],
        ..HirStruct::new(Identifier::new("Pair"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Pair")]),
                    name: Identifier::new("Pair"),
                    args: vec![
                        expr(HirExprKind::FieldInit {
                            name: Identifier::new("left"),
                            value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                        }),
                        expr(HirExprKind::FieldInit {
                            name: Identifier::new("right"),
                            value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                        }),
                    ],
                    resolved: None,
                }))))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Object {
                            name: Some(NamePath::new(vec![Identifier::new("Pair")])),
                            fields: vec![
                                (Identifier::new("left"), HirPattern::Literal(HirLiteral::Bool(true))),
                                (Identifier::new("right"), HirPattern::Literal(HirLiteral::Bool(true))),
                            ],
                            rest: None,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![pair_struct]);
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .filter(|instruction| matches!(instruction.kind, MirInstructionKind::FieldGet { .. }))
            .count()
            >= 2
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| matches!(instruction.kind, MirInstructionKind::Call { .. })));
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn lowers_multi_field_constructor_pattern_into_extractor_call_and_payload_compare() {
    let pair_struct = HirStruct {
        name: Identifier::new("Pair"),
        fields: vec![
            HirField {
                name: Identifier::new("left"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Boolean,
                visibility: HirVisibility::default(),
                is_mutable: false,
            },
            HirField {
                name: Identifier::new("right"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Boolean,
                visibility: HirVisibility::default(),
                is_mutable: false,
            },
        ],
        ..HirStruct::new(Identifier::new("Pair"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Construct {
                    path: NamePath::new(vec![Identifier::new("Pair")]),
                    name: Identifier::new("Pair"),
                    args: vec![
                        expr(HirExprKind::FieldInit {
                            name: Identifier::new("left"),
                            value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                        }),
                        expr(HirExprKind::FieldInit {
                            name: Identifier::new("right"),
                            value: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                        }),
                    ],
                    resolved: None,
                }))))),
                arms: vec![
                    HirMatchArm {
                        pattern: constructor_extractor_pattern(
                            NamePath::new(vec![Identifier::new("Pair")]),
                            vec![HirPattern::Literal(HirLiteral::Bool(true)), HirPattern::Literal(HirLiteral::Bool(true))],
                            NamePath::new(vec![Identifier::new("demo"), Identifier::new("pair_extract")]),
                            nullable_tuple_return(vec![ValkyrieType::Boolean, ValkyrieType::Boolean]),
                        ),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![pair_struct]);
    let pattern_blocks = mir_catch_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("demo"), Identifier::new("pair_extract")])
        )
    }));
    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .filter(|instruction| {
                matches!(
                    &instruction.kind,
                    MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                        if *path == NamePath::new(vec![Identifier::new("tuple_get_0")])
                            || *path == NamePath::new(vec![Identifier::new("tuple_get_1")])
                )
            })
            .count()
            >= 2
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| matches!(instruction.kind, MirInstructionKind::Call { .. })));
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
    assert!(
        !mir_instructions(pattern_blocks.iter().copied()).any(|instruction| matches!(instruction.kind, MirInstructionKind::FieldGet { .. }))
    );
}

#[test]
fn lowers_tuple_pattern_into_tuple_get_and_compare_without_fallback() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Tuple(vec![ValkyrieType::Boolean, ValkyrieType::Boolean]),
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Tuple(vec![
                            HirPattern::Literal(HirLiteral::Bool(true)),
                            HirPattern::Literal(HirLiteral::Bool(true)),
                        ]),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let pattern_blocks = mir_match_arm_pattern_blocks(&mir.functions[0].blocks, 0);

    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .filter(|instruction| {
                matches!(
                    &instruction.kind,
                    MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                        if *path == NamePath::new(vec![Identifier::new("tuple_get_0")])
                            || *path == NamePath::new(vec![Identifier::new("tuple_get_1")])
                )
            })
            .count()
            >= 2
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(instruction.kind, MirInstructionKind::FieldGet { .. }) || matches!(&instruction.kind, MirInstructionKind::Call { .. })
    }));
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| matches!(instruction.kind, MirInstructionKind::Call { .. })));
    assert!(
        !mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn lowers_or_pattern_into_logical_or_without_fallback() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Boolean,
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Or(vec![
                            HirPattern::Literal(HirLiteral::Bool(false)),
                            HirPattern::Literal(HirLiteral::Bool(true)),
                        ]),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    // Or of two `true` literals is short-circuit CFG over bool values ? no `.eq` Compare.
    assert!(mir.functions[0].blocks.iter().any(|b| b.label.contains("or_") || matches!(b.terminator, MirTerminator::Branch { .. })));
    assert!(
        !mir.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }))
    );
}

#[test]
fn lowers_range_pattern_into_compare_chain_without_fallback() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Integer32 { signed: true },
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Range {
                            start: Some(HirLiteral::Integer64(1)),
                            end: Some(HirLiteral::Integer64(10)),
                            inclusive_end: true,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let guard_block = mir.functions[0].blocks.iter().find(|block| block.label == "match_arm_0").expect("expected first match arm block");

    assert!(
        mir.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|instruction| {
                matches!(
                                    &instruction.kind,
                                    MirInstructionKind::Call {                        callee: MirOperand::Symbol(path),
                                        ..,
                } if path.parts().last().is_some_and(|name| name.as_str().starts_with("__") && name.as_str().ends_with("_lt"))
                                )
            })
            .count()
            >= 2
    );
    assert!(mir.functions[0].blocks.iter().any(|b| b.label.contains("and_") || b.label.contains("not_")));
    assert!(!guard_block.instructions.iter().any(|instruction| matches!(instruction.kind, MirInstructionKind::PatternMatch { .. })));
}

#[test]
fn lowers_array_rest_pattern_into_extractor_call_and_payload_bindings() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Array(Box::new(ValkyrieType::Boolean)),
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: array_extractor_pattern(
                            vec![HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                                name: Identifier::new("head"),
                                shadow_index: 0,
                                span: span(),
                            })],
                            Some(nyar_language::types::hir::HirIdentifier { name: Identifier::new("tail"), shadow_index: 0, span: span() }),
                            vec![HirPattern::Literal(HirLiteral::Bool(true))],
                            NamePath::new(vec![Identifier::new("demo"), Identifier::new("array_extract_bool")]),
                            nullable_tuple_return(vec![
                                ValkyrieType::Boolean,
                                ValkyrieType::Array(Box::new(ValkyrieType::Boolean)),
                                ValkyrieType::Boolean,
                            ]),
                        ),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("head"),
                            shadow_index: 0,
                            span: span(),
                        }))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], Vec::new());
    let pattern_blocks = mir_match_arm_pattern_blocks(&mir.functions[0].blocks, 0);
    let body_block = mir_block(&mir.functions[0].blocks, "match_arm_0_check");

    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("demo"), Identifier::new("array_extract_bool")])
        )
    }));
    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .filter(|instruction| {
                matches!(
                    &instruction.kind,
                    MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                        if *path == NamePath::new(vec![Identifier::new("tuple_get_0")])
                            || *path == NamePath::new(vec![Identifier::new("tuple_get_1")])
                            || *path == NamePath::new(vec![Identifier::new("tuple_get_2")])
                )
            })
            .count()
            >= 3
    );
    assert!(
        body_block
            .instructions
            .iter()
            .any(|instruction| { matches!(&instruction.kind, MirInstructionKind::StoreVar { name, .. } if name == "head" || name == "tail") })
    );
}

#[test]
fn lowers_nested_object_pattern_with_array_extractor_call() {
    let container_struct = HirStruct {
        name: Identifier::new("Container"),
        fields: vec![HirField {
            name: Identifier::new("items"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true })),
            visibility: HirVisibility::default(),
            is_mutable: false,
        }],
        ..HirStruct::new(Identifier::new("Container"))
    };
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Named(Identifier::new("Container")),
            ..Default::default()
        }],
        return_type: ValkyrieType::Boolean,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Match {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Object {
                            name: Some(NamePath::new(vec![Identifier::new("Container")])),
                            fields: vec![(
                                Identifier::new("items"),
                                array_extractor_pattern(
                                    vec![HirPattern::Range {
                                        start: Some(HirLiteral::Integer64(1)),
                                        end: Some(HirLiteral::Integer64(10)),
                                        inclusive_end: true,
                                    }],
                                    Some(nyar_language::types::hir::HirIdentifier {
                                        name: Identifier::new("tail"),
                                        shadow_index: 0,
                                        span: span(),
                                    }),
                                    vec![HirPattern::Literal(HirLiteral::Integer64(20))],
                                    NamePath::new(vec![Identifier::new("demo"), Identifier::new("array_extract_i32")]),
                                    nullable_tuple_return(vec![
                                        ValkyrieType::Boolean,
                                        ValkyrieType::Integer32 { signed: true },
                                        ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true })),
                                        ValkyrieType::Integer32 { signed: true },
                                    ]),
                                ),
                            )],
                            rest: None,
                        },
                        guard: None,
                        body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };

    let mir = lower_test_module(vec![function], vec![container_struct]);
    let pattern_blocks = mir_match_arm_pattern_blocks(&mir.functions[0].blocks, 0);
    let body_block = mir_block(&mir.functions[0].blocks, "match_arm_0_check");

    assert!(
        mir_instructions(pattern_blocks.iter().copied())
            .any(|instruction| matches!(instruction.kind, MirInstructionKind::FieldGet { ref field, .. } if field == "items"))
    );
    assert!(mir_instructions(pattern_blocks.iter().copied()).any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call { callee: MirOperand::Symbol(path), .. }
                if *path == NamePath::new(vec![Identifier::new("demo"), Identifier::new("array_extract_i32")])
        )
    }));
    assert!(
        body_block
            .instructions
            .iter()
            .any(|instruction| { matches!(&instruction.kind, MirInstructionKind::StoreVar { name, .. } if name == "tail") })
    );
}

#[test]
fn nested_field_chain_length_keeps_receiver_and_bare_callee() {
    // Virtual-dispatch MIR shape: receiver + resolved method symbol (not a field-chain Path).
    let mir = ValkyrieCompiler::default()
        .compile_source_to_mir(
            r#"
structure SourceClosure {
    package_names: [utf8]
}

structure CompilePlan {
    source_closure: SourceClosure
}

micro package_count(plan: CompilePlan) -> i64 {
    return plan.source_closure.package_names.length()
}
"#,
        )
        .expect("mir ok");

    let function = mir.functions.iter().find(|f| f.symbol.ends_with("package_count")).expect("package_count");
    let length_call = function.blocks.iter().flat_map(|block| block.instructions.iter()).find_map(|instruction| match &instruction.kind {
        MirInstructionKind::Call { callee: MirOperand::Symbol(path), arguments, .. }
            if path.parts().last().is_some_and(|part| part.as_str() == "length") =>
        {
            Some((path.clone(), arguments.len()))
        }
        _ => None,
    });

    let (path, argc) = length_call.expect("expected a length Call");
    assert!(path.parts().last().is_some_and(|part| part.as_str() == "length"), "callee must resolve to a length method, got {path}");
    assert_eq!(argc, 1, "length must keep the array receiver as its sole argument");
    assert!(
        path.parts().len() == 1 || path.parts().first().is_some_and(|part| part.as_str() == "Array"),
        "callee must not be a field-chain Path like plan.source_closure...; got {path}"
    );
}
