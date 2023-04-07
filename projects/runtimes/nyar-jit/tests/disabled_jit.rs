use nyar_jit::{DisabledJit, JitCompileRequest, JitCompiler, JitError, JitFunctionSpec, StackMapJit};

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
            safepoint_indices: vec![0],
        },
    };
    assert_eq!(jit.compile_function(&request), Err(JitError::Unsupported));
}

#[test]
fn stack_map_jit_emits_conservative_maps() {
    let mut jit = StackMapJit;
    assert!(jit.enabled());
    let request = JitCompileRequest {
        module_version: 1,
        module_name: "test".to_string(),
        code_bytes: vec![0],
        function_index: 7,
        function: JitFunctionSpec {
            code_offset: 0,
            code_length: 1,
            local_count: 3,
            arity: 1,
            safepoint_indices: vec![0, 4],
        },
    };
    let artifact = jit.compile_function(&request).expect("stack map jit");
    assert_eq!(artifact.function_index, 7);
    assert_eq!(artifact.stack_maps.entries.len(), 2);
    assert_eq!(artifact.stack_maps.entries[0].local_root_slots, vec![0, 1, 2]);
}
