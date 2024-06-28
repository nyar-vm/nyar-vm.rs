use std::collections::BTreeMap;

use nyar::{HostProjectionBoundary, NyarType, QualifiedName};
use nyar_emitter::{
    TargetExecutionPlan, FragmentSubmission,
    contracts::{Block, BlockRef, ExecutableFunction, Terminator},
    lowering::backends::wasm::validate_text_encoding_projection,
};

fn utf16_submission() -> FragmentSubmission {
    let operation = QualifiedName::new(vec![nyar::Identifier::new("entry")]);
    let function = ExecutableFunction {
        return_layout: None,
        value_layouts: BTreeMap::new(),
        symbol: "entry".to_string(),
        return_type: NyarType::Unit,
        param_types: vec![NyarType::Utf16],
        value_types: BTreeMap::new(),
        entry: BlockRef(0),
        values: Vec::new(),
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    FragmentSubmission::from_backend_plan(TargetExecutionPlan::from_functions(BTreeMap::from([(operation, function)])))
}

#[test]
fn rejects_utf16_without_a_declared_wasm_carrier_on_every_managed_boundary() {
    for boundary in [HostProjectionBoundary::WasmJsGlue, HostProjectionBoundary::WasiComponent] {
        let error = validate_text_encoding_projection(&utf16_submission(), boundary).expect_err("UTF-16 requires a declared carrier");
        let message = error.to_string();
        assert!(message.contains("WASM pre-emission verifier"), "{message}");
        assert!(message.contains("explicit UTF-16 ABI contract"), "{message}");
    }
}
