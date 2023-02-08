use nyar_jit::{DisabledJit, JitCompileRequest, JitCompiler, JitError, JitFunctionSpec};

#[test]
fn disabled_jit_reports_unsupported() {
    let mut jit = DisabledJit;
    assert!(!jit.enabled());
    let request = JitCompileRequest {
        module_version: 1,
        module_name: "test".to_string(),
        code_bytes: vec![0],
        function_index: 0,
        function: JitFunctionSpec {
            code_offset: 0,
            code_length: 1,
            local_count: 0,
            arity: 0,
        },
    };
    assert_eq!(jit.compile_function(&request), Err(JitError::Unsupported));
}
