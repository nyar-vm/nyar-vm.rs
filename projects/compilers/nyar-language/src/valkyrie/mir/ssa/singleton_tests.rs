//! MIR singleton lowering tests covering Task 3.4.
//!
//! Verifies that HIR singleton constructs lower correctly to MIR:
//! - Static method calls emit accessor call followed by method call
//! - Field reads emit accessor call followed by FieldGet
//! - Field writes emit accessor call followed by FieldSet
//! - Method bodies use the `self` parameter directly (no accessor call)
//! - Eager vs lazy singletons select the correct accessor symbol

use crate::{types::SourceID, valkyrie::hir::ValkyrieCompiler};

use super::{MirFunction, MirInstruction, MirLowerer, MirModule, MirOperand, MirOperation, MirValueOrigin};

/// Compiles source text into semantic MIR for singleton lowering inspection.
fn compile_mir(source: &str) -> MirModule {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 }).compile_source(source).expect("compile");
    MirLowerer::lower_module_semantic(&hir)
}

/// Finds a lowered MIR function by trailing symbol segment.
fn find_function<'a>(mir: &'a MirModule, name: &str) -> &'a MirFunction {
    mir.functions
        .iter()
        .find(|f| f.symbol == name || f.symbol.ends_with(&format!("::{name}")) || f.symbol.ends_with(&format!(".{name}")))
        .unwrap_or_else(|| {
            let symbols: Vec<_> = mir.functions.iter().map(|f| f.symbol.as_str()).collect();
            panic!("expected mir function {name}, got {symbols:?}")
        })
}

/// Iterates all instructions across every block in a function.
fn all_instructions<'a>(function: &'a MirFunction) -> impl Iterator<Item = &'a MirInstruction> {
    function.blocks.iter().flat_map(|block| block.instructions.iter())
}

/// Returns the callee symbol path string if the instruction is a call.
fn call_callee_path(instruction: &MirInstruction) -> Option<String> {
    match &instruction.kind {
        MirOperation::Call { callee: MirOperand::Symbol(path), .. } => Some(path.to_string()),
        _ => None,
    }
}

/// Collects callee path strings for every Call instruction in instruction order.
fn ordered_call_paths(function: &MirFunction) -> Vec<String> {
    all_instructions(function).filter_map(|ins| call_callee_path(ins)).collect()
}

/// Checks whether the function contains a call whose callee path ends with the suffix.
fn has_call_with_suffix(function: &MirFunction, suffix: &str) -> bool {
    all_instructions(function).any(|ins| call_callee_path(ins).is_some_and(|p| p.ends_with(suffix)))
}

#[test]
fn lower_singleton_static_method_call() {
    let mir = compile_mir(
        r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }
}

micro main() -> i64 {
    return Counter.increment();
}
"#,
    );
    let main = find_function(&mir, "main");

    let calls = ordered_call_paths(main);
    let accessor_idx = calls.iter().position(|p| p.ends_with("Counter.instance"));
    let method_idx = calls.iter().position(|p| p.ends_with("Counter.increment"));
    assert!(accessor_idx.is_some(), "expected accessor call Counter.instance, got {calls:?}");
    assert!(method_idx.is_some(), "expected method call Counter.increment, got {calls:?}");
    let accessor_idx = accessor_idx.expect("accessor index");
    let method_idx = method_idx.expect("method index");
    assert!(accessor_idx < method_idx, "accessor must precede method call, got {calls:?}");

    let method_call =
        all_instructions(main).find(|ins| call_callee_path(ins).is_some_and(|p| p.ends_with("Counter.increment"))).expect("method call");
    match &method_call.kind {
        MirOperation::Call { arguments, .. } => {
            assert_eq!(arguments.len(), 1, "method call must receive singleton instance as its single argument");
        }
        _ => unreachable!("verified as Call above"),
    }
}

#[test]
fn lower_singleton_field_read() {
    let mir = compile_mir(
        r#"
singleton Counter {
    mut total: i64 = 0
}

micro main() -> i64 {
    return Counter.total;
}
"#,
    );
    let main = find_function(&mir, "main");

    assert!(has_call_with_suffix(main, "Counter.instance"), "expected accessor call Counter.instance before field read");

    let all: Vec<_> = all_instructions(main).collect();
    let accessor_pos =
        all.iter().position(|ins| call_callee_path(ins).is_some_and(|p| p.ends_with("Counter.instance"))).expect("accessor call");
    let field_get_pos =
        all.iter().position(|ins| matches!(&ins.kind, MirOperation::FieldGet { field, .. } if field == "total")).expect("FieldGet for 'total'");
    assert!(
        accessor_pos < field_get_pos,
        "accessor call must precede FieldGet, got accessor at {accessor_pos} and FieldGet at {field_get_pos}"
    );
}

#[test]
fn lower_singleton_field_write() {
    let mir = compile_mir(
        r#"
singleton Counter {
    mut total: i64 = 0
}

micro main() {
    Counter.total = 42
    return
}
"#,
    );
    let main = find_function(&mir, "main");

    assert!(has_call_with_suffix(main, "Counter.instance"), "expected accessor call Counter.instance before field write");

    let all: Vec<_> = all_instructions(main).collect();
    let accessor_pos =
        all.iter().position(|ins| call_callee_path(ins).is_some_and(|p| p.ends_with("Counter.instance"))).expect("accessor call");
    let field_set_pos =
        all.iter().position(|ins| matches!(&ins.kind, MirOperation::FieldSet { field, .. } if field == "total")).expect("FieldSet for 'total'");
    assert!(
        accessor_pos < field_set_pos,
        "accessor call must precede FieldSet, got accessor at {accessor_pos} and FieldSet at {field_set_pos}"
    );
}

#[test]
fn lower_singleton_method_body_self_field() {
    let mir = compile_mir(
        r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }
}
"#,
    );
    let increment = find_function(&mir, "Counter.increment");

    assert!(
        !has_call_with_suffix(increment, "Counter.instance") && !has_call_with_suffix(increment, "Counter.get_instance"),
        "method body must use self parameter directly, not emit an accessor call"
    );

    let self_value = increment
        .values
        .iter()
        .find(|v| matches!(&v.origin, MirValueOrigin::Parameter { index: 0, name } if name.as_str() == "self"))
        .map(|v| v.id)
        .expect("self parameter value");

    let field_gets_using_self = all_instructions(increment)
        .filter(|ins| {
            matches!(
                &ins.kind,
                MirOperation::FieldGet { object: MirOperand::Value(v), field, .. }
                if *v == self_value && field == "total"
            )
        })
        .count();
    let field_sets_using_self = all_instructions(increment)
        .filter(|ins| {
            matches!(
                &ins.kind,
                MirOperation::FieldSet { object: MirOperand::Value(v), field, .. }
                if *v == self_value && field == "total"
            )
        })
        .count();

    assert!(field_gets_using_self >= 1, "expected at least one FieldGet on self.total, got {field_gets_using_self}");
    assert!(field_sets_using_self >= 1, "expected at least one FieldSet on self.total, got {field_sets_using_self}");
}

#[test]
fn lower_singleton_eager_vs_lazy_accessor() {
    let eager_mir = compile_mir(
        r#"
singleton Counter {
    mut total: i64 = 0

    micro get(self) -> i64 {
        return self.total;
    }
}

micro main() -> i64 {
    return Counter.get();
}
"#,
    );
    let lazy_mir = compile_mir(
        r#"
lazy singleton Counter {
    mut total: i64 = 0

    micro get(self) -> i64 {
        return self.total;
    }
}

micro main() -> i64 {
    return Counter.get();
}
"#,
    );

    let eager_main = find_function(&eager_mir, "main");
    let lazy_main = find_function(&lazy_mir, "main");

    assert!(has_call_with_suffix(eager_main, "Counter.instance"), "eager singleton must use 'instance' accessor");
    assert!(!has_call_with_suffix(eager_main, "Counter.get_instance"), "eager singleton must not use 'get_instance' accessor");

    assert!(has_call_with_suffix(lazy_main, "Counter.get_instance"), "lazy singleton must use 'get_instance' accessor");
    assert!(!has_call_with_suffix(lazy_main, "Counter.instance"), "lazy singleton must not use 'instance' accessor");
}

#[test]
fn singleton_trait_witness_entries_dispatch_serialize() {
    use crate::valkyrie::mir::collect_singleton_witness_entries;

    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 })
        .compile_source(
            r#"
trait Serializable {
    micro serialize(self) -> utf8
    micro deserialize(mut self, data: utf8)
}

singleton Settings: Serializable {
    mut theme: utf8 = "dark"

    micro serialize(self) -> utf8 {
        self.theme
    }

    micro deserialize(mut self, data: utf8) {
        self.theme = data
    }
}
"#,
        )
        .expect("compile");

    let entries = collect_singleton_witness_entries(&hir);
    assert_eq!(entries.len(), 1, "expected one singleton-trait witness entry, got {entries:?}");

    let entry = &entries[0];
    assert_eq!(entry.singleton_name.as_str(), "Settings");
    assert_eq!(entry.trait_name.as_str(), "Serializable");

    let method_names: Vec<_> = entry.method_entries.iter().map(|method| method.name.as_str().to_string()).collect();
    assert!(method_names.iter().any(|name| name == "serialize"), "expected serialize method in witness entries, got {method_names:?}");
    assert!(method_names.iter().any(|name| name == "deserialize"), "expected deserialize method in witness entries, got {method_names:?}");

    let serialize_entry = entry.method_entries.iter().find(|method| method.name.as_str() == "serialize").expect("serialize entry");
    assert_eq!(serialize_entry.implementation_path.container.as_str(), "Settings");
    assert_eq!(serialize_entry.implementation_path.method.as_str(), "serialize");
    assert!(!serialize_entry.is_default, "serialize must be a concrete singleton method, not a default trait method");
}

#[test]
fn demo_voa_todo_store_parses_through_hir() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 })
        .compile_source(
            r#"
singleton TodoStore {
    mut todos_sig: i32 = 0;
    mut filter_store_sig: i32 = 0;
    mut next_id: i32 = 4;

    micro init(mut self) {
        self.todos_sig = 0
        self.filter_store_sig = 0
    }

    micro get_filter(self) -> i32 {
        self.filter_store_sig
    }

    micro set_filter(mut self, f: i32) {
        self.filter_store_sig = f
    }

    micro add_todo(mut self, text: utf8) {
        let trimmed: utf8 = text
        if trimmed == "" { return }
        self.next_id = self.next_id + 1
    }

    micro count_all(self) -> i32 {
        self.next_id
    }
}
"#,
        )
        .expect("compile");

    assert_eq!(hir.singletons.len(), 1, "expected exactly one singleton in demo TodoStore module");
    let store = &hir.singletons[0];
    assert_eq!(store.name.as_str(), "TodoStore");
    assert_eq!(store.fields.len(), 3, "TodoStore should have 3 fields");
    // `init` 被路由到 `constructor` 字段，不再出现在 `methods` 中，因此普通方法数为 4。
    assert_eq!(store.methods.len(), 4, "TodoStore should have 4 ordinary methods (init routed to constructor)");
    assert!(store.constructor.is_some(), "TodoStore should have a constructor routed from init");
    assert!(store.finalizer.is_none(), "TodoStore has no finalize, so finalizer should be None");

    let method_names: Vec<_> = store.methods.iter().map(|method| method.name.as_str().to_string()).collect();
    assert!(!method_names.iter().any(|name| name == "init"), "init should not remain in ordinary methods, got {method_names:?}");
    assert!(method_names.iter().any(|name| name == "count_all"), "expected count_all method, got {method_names:?}");
}
