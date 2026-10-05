//! `[workload_phase]` → `begin_phase` / `end_phase` Call 展开。

use crate::{types::SourceID, valkyrie::hir::ValkyrieCompiler};

use super::{MirConstant, MirFunction, MirLowerer, MirModule, MirOperand, MirOperation};

fn compile_mir(source: &str) -> MirModule {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9501 }).compile_source(source).expect("compile");
    MirLowerer::lower_module_semantic(&hir)
}

fn find_function<'a>(mir: &'a MirModule, name: &str) -> &'a MirFunction {
    mir.functions
        .iter()
        .find(|f| f.symbol == name || f.symbol.ends_with(&format!("::{name}")) || f.symbol.ends_with(&format!(".{name}")))
        .unwrap_or_else(|| panic!("missing function {name}"))
}

#[test]
fn workload_phase_attribute_emits_begin_and_end_calls() {
    let mir = compile_mir(
        r#"
[workload_phase("request")]
micro handle() -> i32 {
    1
}
"#,
    );
    let function = find_function(&mir, "handle");
    let entry = &function.blocks[function.entry.0 as usize];

    let mut saw_begin = false;
    let mut saw_utf8_request = false;
    for instruction in &entry.instructions {
        match &instruction.kind {
            MirOperation::LoadConstant { constant: MirConstant::Utf8(text), .. } if text == "request" => {
                saw_utf8_request = true;
            }
            MirOperation::Call { callee: MirOperand::Symbol(path), .. } => {
                let name = path.to_string();
                if name.ends_with("begin_phase") || name == "begin_phase" {
                    saw_begin = true;
                }
            }
            _ => {}
        }
    }
    assert!(saw_utf8_request, "expected Utf8 request constant in entry");
    assert!(saw_begin, "expected begin_phase call in entry");

    let mut saw_end = false;
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let MirOperation::Call { callee: MirOperand::Symbol(path), .. } = &instruction.kind {
                let name = path.to_string();
                if name.ends_with("end_phase") || name == "end_phase" {
                    saw_end = true;
                }
            }
        }
    }
    assert!(saw_end, "expected end_phase call before return");
}

#[test]
fn workload_phase_keeps_open_across_yield_terminator_shape() {
    use super::{MirEffectKind, MirTerminator};

    // 纯合同：Yield 不得关闭阶段；Raise→runtime 必须关闭。
    assert!(!matches!(
        MirTerminator::YieldToRuntime { effect: MirEffectKind::Yield, payload: None, resume_state: 0 },
        MirTerminator::Return { .. } | MirTerminator::Unreachable
    ));
    assert!(matches!(
        MirTerminator::YieldToRuntime { effect: MirEffectKind::Raise, payload: None, resume_state: 0 },
        MirTerminator::YieldToRuntime { effect: MirEffectKind::Raise, .. }
    ));
}
