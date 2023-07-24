use nyar_language::{ValkyrieCompiler, types::SourceID};

#[test]
fn generic_call_rejects_unsatisfied_declared_trait_bound() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4300 });
    let result = compiler.compile_source(r#"
trait Required {}
micro constrained<T>(value: T) -> T where T: Required { return value; }
micro invoke(value: bool) -> bool { return constrained(value); }
"#);
    assert!(result.is_err(), "缺少 Required evidence 的调用不得成为前端成功结果");
}
