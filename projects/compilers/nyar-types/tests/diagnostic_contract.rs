//! DiagnosticRow JSON round-trip 稳定性验证。
//!
//! 验收点：
//! 1. 同一个 DiagnosticRow 序列化为 JSON → 反序列化回来，code / args / primary_span /
//!    related_spans / severity / owner / phase 必须完全一致；
//! 2. 不同 locale / 展示文本（internal_message）不影响结构化字段；
//! 3. args 是结构化 key-value（DiagnosticArg 枚举），不是已格式化的 String；
//! 4. related_spans / notes / args 为空时不写入 JSON（skip_serializing_if）。

use nyar_types::{
    DiagnosticArg, DiagnosticOwner, DiagnosticPhase, DiagnosticRow, DiagnosticSeverity, DiagnosticSource, DiagnosticSpan,
};

#[test]
fn diagnostic_row_json_round_trip_preserves_all_structured_fields() {
    let row = DiagnosticRow::new(
        "valkyrie.hir.unresolved_call",
        DiagnosticSeverity::Error,
        DiagnosticPhase::Hir,
        DiagnosticOwner::ValkyrieFrontend,
        DiagnosticSource { id: "std/collection/v2/hash_utf8.v".into(), revision: Some("abc123def".into()) },
        DiagnosticSpan { start: 365, end: 415 },
    )
    .with_related(DiagnosticSpan { start: 120, end: 130 })
    .with_related(DiagnosticSpan { start: 200, end: 210 })
    .with_arg("selector", DiagnosticArg::String("as_range".into()))
    .with_arg("context", DiagnosticArg::String("as_range".into()))
    .with_arg("owner_type", DiagnosticArg::TypeName("TextSpan".into()))
    .with_arg("defining_trait", DiagnosticArg::SymbolPath(vec!["std".into(), "collection".into(), "Rangeable".into()]))
    .with_note("跨 namespace 绑定需要显式 trait 约束")
    .with_internal_message("跨 namespace TextSpan.as_range 必须通过: SMIR003 unresolved call contract `Range` in `as_range`")
    .with_help("在调用方显式绑定 `use std::collection::Rangeable`")
    .with_cause("HIR lowering 未找到 trait 约束")
    .with_internal_message("TextSpan.as_range 必须通过 trait 约束解析");

    // 序列化
    let json = serde_json::to_string_pretty(&row).expect("序列化成功");

    // 反序列化回来
    let restored: DiagnosticRow = serde_json::from_str(&json).expect("反序列化成功");

    // 结构化字段必须完全一致——即使 internal_message 变了，也不影响
    assert_eq!(restored.code, "valkyrie.hir.unresolved_call", "code 必须一致");
    assert_eq!(restored.severity, DiagnosticSeverity::Error, "severity 必须一致");
    assert_eq!(restored.phase, DiagnosticPhase::Hir, "phase 必须一致");
    assert_eq!(restored.owner, DiagnosticOwner::ValkyrieFrontend, "owner 必须一致");
    assert_eq!(restored.source.id, "std/collection/v2/hash_utf8.v", "source.id 必须一致");
    assert_eq!(restored.source.revision.as_deref(), Some("abc123def"), "source.revision 必须一致");
    assert_eq!(restored.primary_span, DiagnosticSpan { start: 365, end: 415 }, "primary_span 必须一致");
    assert_eq!(restored.related_spans.len(), 2, "related_spans 必须有 2 条");
    assert_eq!(restored.args.len(), 4, "args 必须有 4 条");
    assert!(restored.notes.iter().any(|n| n.contains("跨 namespace")), "notes 必须保留");
    assert!(restored.help.as_ref().unwrap().contains("显式绑定"), "help 必须保留");
    assert!(restored.cause.as_ref().unwrap().contains("HIR lowering"), "cause 必须保留");
    assert_eq!(restored.internal_message.as_ref().unwrap(), "TextSpan.as_range 必须通过 trait 约束解析", "internal_message 最后设置的值应被保留");

    // args 必须是结构化 DiagnosticArg 枚举值，不是 String
    assert!(
        matches!(restored.args.get("owner_type").unwrap(), DiagnosticArg::TypeName(_)),
        "owner_type 必须是 DiagnosticArg::TypeName"
    );
    assert!(
        matches!(restored.args.get("defining_trait").unwrap(), DiagnosticArg::SymbolPath(_)),
        "defining_trait 必须是 DiagnosticArg::SymbolPath"
    );
}

#[test]
fn diagnostic_row_empty_optional_fields_are_omitted_from_json() {
    let row = DiagnosticRow::new(
        "oak.parse.unexpected_token",
        DiagnosticSeverity::Error,
        DiagnosticPhase::Parsing,
        DiagnosticOwner::OakParser,
        DiagnosticSource { id: "hello.v".into(), revision: None },
        DiagnosticSpan { start: 42, end: 43 },
    );

    let json = serde_json::to_string(&row).expect("序列化成功");

    // 空的可选字段（related_spans / args / notes / help / cause / internal_message / revision）必须从 JSON 中省略
    assert!(!json.contains("revision"), "空 revision 不应出现在 JSON 中");
    assert!(!json.contains("relatedSpans"), "空 related_spans 不应出现在 JSON 中");
    assert!(!json.contains("args"), "空 args 不应出现在 JSON 中");
    assert!(!json.contains("notes"), "空 notes 不应出现在 JSON 中");
    assert!(!json.contains("help"), "None help 不应出现在 JSON 中");
    assert!(!json.contains("cause"), "None cause 不应出现在 JSON 中");
    assert!(!json.contains("internalMessage"), "None internal_message 不应出现在 JSON 中");

    // 基础字段必须存在
    assert!(json.contains("oak.parse.unexpected_token"), "code 必须存在");
    assert!(json.contains("Parsing"), "phase 必须存在");
    assert!(json.contains("OakParser"), "owner 必须存在");
}

#[test]
fn compile_result_success_vs_failure_separate() {
    let success: nyar_types::CompileResult<String> = nyar_types::CompileResult::Success("hello artifact".into());
    let failure: nyar_types::CompileResult<String> = nyar_types::CompileResult::Failure(vec![
        DiagnosticRow::new(
            "backend.contract.missing_carrier",
            DiagnosticSeverity::Error,
            DiagnosticPhase::Backend,
            DiagnosticOwner::Backend,
            DiagnosticSource { id: "wasm.wasm".into(), revision: None },
            DiagnosticSpan { start: 0, end: 0 },
        )
        .with_arg("type", DiagnosticArg::TypeName("SwissTableEntry".into()))
        .with_arg("missing_carrier", DiagnosticArg::String("i64".into())),
    ]);

    let success_json = serde_json::to_string(&success).unwrap();
    let failure_json = serde_json::to_string(&failure).unwrap();

    assert!(success.is_success(), "Success 必须 is_success");
    assert!(failure.is_failure(), "Failure 必须 is_failure");
    assert!(success.diagnostics().is_none(), "Success 无 diagnostics");
    assert_eq!(failure.diagnostics().unwrap().len(), 1, "Failure 有 1 条诊断");

    // Success 载荷必须可序列化，载荷内容正确
    assert!(success_json.contains("hello artifact"));
    assert!(failure_json.contains("backend.contract.missing_carrier"));

    // Failure 载荷必须能反序列化回来且 code / args 完整
    let restored: nyar_types::CompileResult<String> = serde_json::from_str(&failure_json).unwrap();
    match restored {
        nyar_types::CompileResult::Failure(rows) => {
            let row = &rows[0];
            assert_eq!(row.code, "backend.contract.missing_carrier");
            assert_eq!(row.owner, DiagnosticOwner::Backend);
            assert_eq!(row.phase, DiagnosticPhase::Backend);
            assert!(matches!(row.args.get("type").unwrap(), DiagnosticArg::TypeName(_)));
            assert!(matches!(row.args.get("missing_carrier").unwrap(), DiagnosticArg::String(_)));
        }
        _ => panic!("反序列化后应为 Failure"),
    }
}

#[test]
fn diagnostic_owner_is_honest_and_never_rewritten() {
    // 这条测试确保 owner 字段是诚实的——不同 owner 产出的诊断，即使同一个 code，
    // 也必须在 JSON 里区分开
    let oak_row = DiagnosticRow::new(
        "valkyrie.unknown",
        DiagnosticSeverity::Error,
        DiagnosticPhase::Parsing,
        DiagnosticOwner::OakParser,
        DiagnosticSource { id: "hello.v".into(), revision: None },
        DiagnosticSpan { start: 0, end: 0 },
    );
    let valkyrie_row = DiagnosticRow::new(
        "valkyrie.unknown",
        DiagnosticSeverity::Error,
        DiagnosticPhase::Hir,
        DiagnosticOwner::ValkyrieFrontend,
        DiagnosticSource { id: "hello.v".into(), revision: None },
        DiagnosticSpan { start: 0, end: 0 },
    );
    let nyar_row = DiagnosticRow::new(
        "valkyrie.unknown",
        DiagnosticSeverity::Error,
        DiagnosticPhase::Mir,
        DiagnosticOwner::NyarCompiler,
        DiagnosticSource { id: "hello.v".into(), revision: None },
        DiagnosticSpan { start: 0, end: 0 },
    );

    let oak_json = serde_json::to_string(&oak_row).unwrap();
    let valkyrie_json = serde_json::to_string(&valkyrie_row).unwrap();
    let nyar_json = serde_json::to_string(&nyar_row).unwrap();

    assert!(oak_json.contains("OakParser"), "Oak 产出的诊断必须带 owner = OakParser");
    assert!(valkyrie_json.contains("ValkyrieFrontend"), "Valkyrie 产出的诊断必须带 owner = ValkyrieFrontend");
    assert!(nyar_json.contains("NyarCompiler"), "Nyar 产出的诊断必须带 owner = NyarCompiler");
}
