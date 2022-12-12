//! Task 2.4 — Future protocol validation tests.
//!
//! These tests exercise [`validate_future_protocol`] and
//! [`witness_bindings_for_effect_with_diagnostics`] directly on hand-built
//! [`HirModule`] instances, mirroring the spec's Task 2.3 contract: an `impl
//! TargetType: Future { ... }` must exist in HIR and must declare both `poll`
//! and `output` methods, otherwise the validator returns a diagnostic and the
//! lowering pipeline returns empty witness bindings instead of synthesizing a
//! fake witness.

use nyar_language::{
    MirEffectKind,
    types::{
        Identifier, NamePath,
        hir::{HirDocumentation, HirFunction, HirImpl, HirModule, HirVisibility, ValkyrieType},
    },
    valkyrie::frontend_contract::{ProtocolDiagnostic, validate_future_protocol, witness_bindings_for_effect_with_diagnostics},
};

fn empty_module() -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: Vec::new(),
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    }
}

fn empty_method(name: &str) -> HirFunction {
    HirFunction {
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: nyar_language::types::hir::HirBlock {
            statements: Vec::new(),
            expr: None,
            span: nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0),
        },
        span: nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

fn future_impl(target_name: &str, method_names: &[&str]) -> HirImpl {
    HirImpl {
        generics: Vec::new(),
        where_constraints: Vec::new(),
        target: ValkyrieType::Named(Identifier::new(target_name)),
        trait_path: Some(NamePath::new(vec![Identifier::new("Future")])),
        methods: method_names.iter().map(|name| empty_method(name)).collect(),
        associated_type_impls: Vec::new(),
        associated_const_impls: Vec::new(),
    }
}

fn future_payload_bool() -> ValkyrieType {
    ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean])
}

#[test]
fn validate_future_protocol_accepts_impl_with_poll_and_output() {
    let mut module = empty_module();
    module.impls.push(future_impl("ReadyFuture", &["poll", "output"]));

    let outcome = validate_future_protocol(&module, &future_payload_bool());
    assert!(outcome.is_ok(), "expected Ok for impl declaring both poll and output, got {:?}", outcome.err());
}

#[test]
fn validate_future_protocol_rejects_impl_missing_output() {
    let mut module = empty_module();
    module.impls.push(future_impl("ReadyFuture", &["poll"]));

    let err = validate_future_protocol(&module, &future_payload_bool()).expect_err("expected MissingMethod diagnostic");
    match err {
        ProtocolDiagnostic::MissingMethod { type_name, trait_name, missing_method } => {
            assert_eq!(type_name, "Future");
            assert_eq!(trait_name, "Future");
            assert_eq!(missing_method, "output");
        }
        other => panic!("expected MissingMethod, got {:?}", other),
    }
}

#[test]
fn validate_future_protocol_rejects_unresolved_impl() {
    let module = empty_module();

    let err = validate_future_protocol(&module, &future_payload_bool()).expect_err("expected UnresolvedImpl diagnostic");
    match err {
        ProtocolDiagnostic::UnresolvedImpl { type_name, trait_name } => {
            assert_eq!(type_name, "Future");
            assert_eq!(trait_name, "Future");
        }
        other => panic!("expected UnresolvedImpl, got {:?}", other),
    }
}

/// 假闭环修复：当 Future impl 缺失时，`witness_bindings_for_effect_with_diagnostics`
/// 不再合成假绑定，而是返回空绑定列表与 `WitnessMethodUnresolved` 诊断。
/// 后端对空绑定的处理是安全的——降级为普通 yield 路径，不会调用不存在的符号。
#[test]
fn witness_bindings_emit_unresolved_diagnostic_when_impl_missing() {
    let module = empty_module();
    let payload = future_payload_bool();

    let (bindings, diagnostics) = witness_bindings_for_effect_with_diagnostics(&module, MirEffectKind::Await, Some(&payload));

    assert!(bindings.is_empty(), "expected no bindings when Future impl is missing, got {bindings:?}");

    assert_eq!(diagnostics.len(), 2);
    let mut saw_poll_unresolved = false;
    let mut saw_output_unresolved = false;
    for diagnostic in &diagnostics {
        match diagnostic {
            ProtocolDiagnostic::WitnessMethodUnresolved { type_name, trait_name, method_name } => {
                assert_eq!(type_name.as_deref(), Some("Future"));
                assert_eq!(trait_name, "Future");
                match method_name.as_str() {
                    "poll" => saw_poll_unresolved = true,
                    "output" => saw_output_unresolved = true,
                    other => panic!("unexpected unresolved method name {other}"),
                }
            }
            other => panic!("expected WitnessMethodUnresolved, got {:?}", other),
        }
    }
    assert!(saw_poll_unresolved, "expected an unresolved diagnostic for poll");
    assert!(saw_output_unresolved, "expected an unresolved diagnostic for output");
}

#[test]
fn witness_bindings_emit_no_diagnostic_when_impl_resolves() {
    let mut module = empty_module();
    module.impls.push(future_impl("ReadyFuture", &["poll", "output"]));
    let payload = future_payload_bool();

    let (bindings, diagnostics) = witness_bindings_for_effect_with_diagnostics(&module, MirEffectKind::Await, Some(&payload));

    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].method_name, "poll");
    assert_eq!(bindings[1].method_name, "output");
    assert!(bindings[0].impl_symbol.is_some(), "expected resolved poll binding with impl_symbol");
    assert!(bindings[1].impl_symbol.is_some(), "expected resolved output binding with impl_symbol");
    assert!(diagnostics.is_empty(), "expected no synthetic-witness diagnostics, got {:?}", diagnostics);
}

/// spec Task 5.4：当 `Future` impl 同时声明 `is_cancelled` 方法时，前端应为 `Await` effect
/// 发射三条 witness 绑定：`poll`（index 0）、`output`（index 1）、`is_cancelled`（index 2）。
#[test]
fn witness_bindings_emit_three_bindings_when_is_cancelled_present() {
    let mut module = empty_module();
    module.impls.push(future_impl("ReadyFuture", &["poll", "output", "is_cancelled"]));
    let payload = future_payload_bool();

    let (bindings, diagnostics) = witness_bindings_for_effect_with_diagnostics(&module, MirEffectKind::Await, Some(&payload));

    assert_eq!(bindings.len(), 3, "expected three bindings when is_cancelled is declared");
    assert_eq!(bindings[0].method_name, "poll");
    assert_eq!(bindings[0].method_index, 0);
    assert_eq!(bindings[1].method_name, "output");
    assert_eq!(bindings[1].method_index, 1);
    assert_eq!(bindings[2].method_name, "is_cancelled");
    assert_eq!(bindings[2].method_index, 2);
    assert!(bindings[2].impl_symbol.is_some(), "expected resolved is_cancelled binding with impl_symbol");
    assert!(diagnostics.is_empty(), "expected no diagnostics when all methods resolve, got {:?}", diagnostics);
}

/// spec Task 5.4：当 `Future` impl 未声明 `is_cancelled` 方法时，前端应保持原有行为，
/// 只发射两条 witness 绑定：`poll`（index 0）、`output`（index 1），完全向后兼容。
#[test]
fn witness_bindings_emit_two_bindings_when_is_cancelled_absent() {
    let mut module = empty_module();
    module.impls.push(future_impl("ReadyFuture", &["poll", "output"]));
    let payload = future_payload_bool();

    let (bindings, diagnostics) = witness_bindings_for_effect_with_diagnostics(&module, MirEffectKind::Await, Some(&payload));

    assert_eq!(bindings.len(), 2, "expected two bindings when is_cancelled is not declared");
    assert_eq!(bindings[0].method_name, "poll");
    assert_eq!(bindings[1].method_name, "output");
    assert!(diagnostics.is_empty(), "expected no diagnostics when all methods resolve, got {:?}", diagnostics);
}
