use rusty_valkyrie::{ValkyrieCompiler, types::hir::HirModule, *};

use rusty_valkyrie::frontend_contract::control_flow_payload::{collect_witness_binding, resolved_witness_binding};

fn module() -> HirModule {
    ValkyrieCompiler::default()
        .compile_source(
            r#"
class Task { }
trait Future {
    micro output(self): i64
    micro poll(self): bool
}
imply Task: Future {
    micro output(self) -> i64 { return 1 }
    micro poll(self) -> bool { return true }
}
"#,
        )
        .expect("夹具方法来自当前源码")
}

#[test]
fn missing_witness_method_cannot_bind_the_first_impl_method() {
    let hir = module();
    assert!(resolved_witness_binding(&hir, "Future", "awake").is_none());
    let mut diagnostics = Vec::new();
    assert!(collect_witness_binding(&hir, "Future", "awake", None, &mut diagnostics).is_empty());
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn witness_slot_comes_from_the_matched_method_not_a_default_index() {
    let hir = module();
    let binding = resolved_witness_binding(&hir, "Future", "poll").unwrap();
    assert_eq!(binding.method_index, 1);
    assert_eq!(binding.impl_symbol.as_deref(), Some("Task.poll"));
}
