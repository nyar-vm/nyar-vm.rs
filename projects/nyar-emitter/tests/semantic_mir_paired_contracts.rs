//! Cross-implementation Semantic MIR conformance checks.
//!
//! Each case builds equivalent Rust executable and Valkyrie SSA metadata, then
//! compares the normalized observation exactly.  Case names intentionally do
//! not encode any library or nominal type semantics.

use std::{collections::BTreeMap, sync::Arc};

use nyar_emitter::{FragmentSubmission, executable_provider::MirFunctionMapProvider, testing::semantic_mir_observation};
use nyar_language::valkyrie::mir::{
    SumTypeLayout, SumVariantLayout,
    ssa::test_support::lower_test_module,
    validation::{semantic_observation, validate_semantic_module},
};
use nyar_types::{
    Block, BlockRef, Constant, DispatchKind, ExecutableFunction, ExternalImportLink, Instruction, InstructionKind, NyarType, Operand,
    StorageKind, Terminator, ValueRef,
    layout::{AggregateLayout, FieldLayout, SumTypeLayout as RustSumTypeLayout, SumVariantLayout as RustSumVariantLayout},
};

fn rust_sum(name: &str, tag_width: u32, variants: Vec<(&str, u32, Option<NyarType>)>) -> FragmentSubmission {
    let mut submission = FragmentSubmission::default();
    submission.sum_types.push(RustSumTypeLayout {
        name: name.to_string(),
        is_unite: false,
        tag_width,
        variants: variants
            .into_iter()
            .map(|(name, tag, payload_type)| RustSumVariantLayout { name: name.to_string(), tag, payload_type })
            .collect(),
    });
    submission
}

fn valkyrie_sum(name: &str, tag_width: u32, variants: Vec<(&str, u32, Option<NyarType>)>) -> nyar_language::valkyrie::mir::MirModule {
    let mut module = lower_test_module(Vec::new(), Vec::new());
    module.sum_types.push(SumTypeLayout {
        name: name.to_string(),
        is_unite: false,
        tag_width,
        variants: variants
            .into_iter()
            .map(|(name, tag, payload_type)| SumVariantLayout { name: name.to_string(), tag, payload_type })
            .collect(),
    });
    module
}

fn named_type(name: &str) -> NyarType {
    NyarType::Named(nyar_types::Identifier::new(name))
}

fn valkyrie_type(ty: &NyarType) -> nyar_language::types::hir::ValkyrieType {
    use nyar_language::types::hir::ValkyrieType;
    match ty {
        NyarType::Boolean => ValkyrieType::Boolean,
        NyarType::Integer32 { signed } => ValkyrieType::Integer32 { signed: *signed },
        NyarType::Utf8 => ValkyrieType::Utf8,
        NyarType::Utf16 => ValkyrieType::Utf16,
        NyarType::Named(name) => ValkyrieType::Named(name.clone()),
        NyarType::Nullable(payload) => ValkyrieType::Nullable(Box::new(valkyrie_type(payload))),
        NyarType::Array(element) => ValkyrieType::Array(Box::new(valkyrie_type(element))),
        _ => panic!("paired sum fixture only supports its declared neutral types"),
    }
}

fn rust_array_intrinsic_submission(
    opcode: nyar_types::IntrinsicOpcode,
    receiver_type: NyarType,
    index_type: NyarType,
    value_type: NyarType,
    output_type: NyarType,
) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("fixture"), nyar::Identifier::new("entry")]);
    let receiver = ValueRef(0);
    let index = ValueRef(1);
    let value = ValueRef(2);
    let output = ValueRef(3);
    let arguments = match opcode {
        nyar_types::IntrinsicOpcode::ArrayGet => vec![Operand::Value(receiver), Operand::Value(index)],
        _ => vec![Operand::Value(receiver), Operand::Value(index), Operand::Value(value)],
    };
    let parameter_types = arguments
        .iter()
        .map(|argument| match argument {
            Operand::Value(ValueRef(0)) => receiver_type.clone(),
            Operand::Value(ValueRef(1)) => index_type.clone(),
            Operand::Value(ValueRef(2)) => value_type.clone(),
            _ => unreachable!(),
        })
        .collect();
    let function = ExecutableFunction {
        symbol: "fixture.entry".to_string(),
        return_type: output_type.clone(),
        param_types: vec![receiver_type.clone(), index_type.clone(), value_type.clone()],
        value_types: [(receiver, receiver_type), (index, index_type), (value, value_type), (output, output_type)].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![receiver, index, value],
            instructions: vec![Instruction {
                output: Some(output),
                kind: InstructionKind::Call {
                    dispatch: DispatchKind::Static,
                    callee: Operand::Symbol(nyar_types::NamePath::new(vec![
                        nyar_types::Identifier::new("neutral"),
                        nyar_types::Identifier::new("operation"),
                    ])),
                    arguments,
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: Some(parameter_types),
                    intrinsic_opcode: Some(opcode),
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(output)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_array_intrinsic_module(
    opcode: nyar_types::IntrinsicOpcode,
    receiver_type: NyarType,
    index_type: NyarType,
    value_type: NyarType,
    output_type: NyarType,
) -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::valkyrie::mir::{MirInstruction, MirInstructionKind, MirOperand, MirTerminator, MirValueRef};
    let receiver = MirValueRef(0);
    let index = MirValueRef(1);
    let value = MirValueRef(2);
    let output = MirValueRef(3);
    let receiver_type = valkyrie_type(&receiver_type);
    let index_type = valkyrie_type(&index_type);
    let value_type = valkyrie_type(&value_type);
    let output_type = valkyrie_type(&output_type);
    let arguments = match opcode {
        nyar_types::IntrinsicOpcode::ArrayGet => vec![MirOperand::Value(receiver), MirOperand::Value(index)],
        _ => vec![MirOperand::Value(receiver), MirOperand::Value(index), MirOperand::Value(value)],
    };
    let parameter_types = arguments
        .iter()
        .map(|argument| match argument {
            MirOperand::Value(MirValueRef(0)) => receiver_type.clone(),
            MirOperand::Value(MirValueRef(1)) => index_type.clone(),
            MirOperand::Value(MirValueRef(2)) => value_type.clone(),
            _ => unreachable!(),
        })
        .collect();
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = output_type.clone();
    function.param_types = vec![receiver_type.clone(), index_type.clone(), value_type.clone()];
    function.value_types = [(receiver, receiver_type), (index, index_type), (value, value_type), (output, output_type)].into_iter().collect();
    function.values.clear();
    function.blocks[0].parameters = vec![receiver, index, value];
    function.blocks[0].instructions = vec![MirInstruction {
        output: Some(output),
        kind: MirInstructionKind::Call {
            dispatch: nyar_language::valkyrie::mir::MirDispatchKind::Static,
            callee: MirOperand::Symbol(nyar_types::NamePath::new(vec![
                nyar_types::Identifier::new("neutral"),
                nyar_types::Identifier::new("operation"),
            ])),
            arguments,
            witness: None,
            effect: None,
            receiver_kind: None,
            parameter_types: Some(parameter_types),
            intrinsic_opcode: Some(opcode),
        },
    }];
    function.blocks[0].terminator = MirTerminator::Return { value: Some(MirOperand::Value(output)) };
    let mut module = lower_test_module(Vec::new(), Vec::new());
    module.functions.push(function);
    module
}

#[derive(Clone, Copy)]
enum ControlCase {
    Return,
    Jump,
    Branch,
}

fn rust_control_submission(case: ControlCase) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("fixture"), nyar::Identifier::new("entry")]);
    let value = ValueRef(0);
    let int = NyarType::Integer32 { signed: true };
    let boolean = NyarType::Boolean;
    let (return_type, blocks) = match case {
        ControlCase::Return => (
            boolean.clone(),
            vec![Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: vec![value],
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Value(value)) },
            }],
        ),
        ControlCase::Branch => (
            boolean.clone(),
            vec![Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: vec![value],
                instructions: Vec::new(),
                terminator: Terminator::Branch { condition: Operand::Value(value), then_target: BlockRef(0), else_target: BlockRef(0) },
            }],
        ),
        ControlCase::Jump => (
            boolean.clone(),
            vec![
                Block {
                    id: BlockRef(0),
                    label: "entry".to_string(),
                    parameters: vec![value],
                    instructions: Vec::new(),
                    terminator: Terminator::Jump { target: BlockRef(1), arguments: vec![Operand::Value(value)] },
                },
                Block {
                    id: BlockRef(1),
                    label: "target".to_string(),
                    parameters: vec![ValueRef(1)],
                    instructions: Vec::new(),
                    terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(1))) },
                },
            ],
        ),
    };
    let mut value_types = BTreeMap::from([(value, int)]);
    if matches!(case, ControlCase::Jump) {
        value_types.insert(ValueRef(1), boolean);
    }
    let function = ExecutableFunction {
        symbol: "fixture.entry".to_string(),
        return_type,
        param_types: vec![NyarType::Integer32 { signed: true }],
        value_types,
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks,
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_control_module(case: ControlCase) -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::{
        types::hir::ValkyrieType,
        valkyrie::mir::{MirBlock, MirBlockRef, MirOperand, MirTerminator, MirValueRef},
    };
    let value = MirValueRef(0);
    let int = ValkyrieType::Integer32 { signed: true };
    let boolean = ValkyrieType::Boolean;
    let (return_type, blocks) = match case {
        ControlCase::Return => (
            boolean.clone(),
            vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".to_string(),
                parameters: vec![value],
                instructions: Vec::new(),
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(value)) },
            }],
        ),
        ControlCase::Branch => (
            boolean.clone(),
            vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".to_string(),
                parameters: vec![value],
                instructions: Vec::new(),
                terminator: MirTerminator::Branch {
                    condition: MirOperand::Value(value),
                    then_target: MirBlockRef(0),
                    else_target: MirBlockRef(0),
                },
            }],
        ),
        ControlCase::Jump => (
            boolean.clone(),
            vec![
                MirBlock {
                    id: MirBlockRef(0),
                    label: "entry".to_string(),
                    parameters: vec![value],
                    instructions: Vec::new(),
                    terminator: MirTerminator::Jump { target: MirBlockRef(1), arguments: vec![MirOperand::Value(value)] },
                },
                MirBlock {
                    id: MirBlockRef(1),
                    label: "target".to_string(),
                    parameters: vec![MirValueRef(1)],
                    instructions: Vec::new(),
                    terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(1))) },
                },
            ],
        ),
    };
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = return_type;
    function.param_types = vec![int.clone()];
    function.entry = MirBlockRef(0);
    function.blocks = blocks;
    function.values.clear();
    function.value_types = BTreeMap::from([(value, int)]);
    if matches!(case, ControlCase::Jump) {
        function.value_types.insert(MirValueRef(1), boolean);
    }
    let mut module = lower_test_module(Vec::new(), Vec::new());
    module.functions.push(function);
    module
}

fn rust_control_submission_with_types(parameter_type: NyarType, return_type: NyarType) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("fixture"), nyar::Identifier::new("entry")]);
    let value = ValueRef(0);
    let function = ExecutableFunction {
        symbol: "fixture.entry".to_string(),
        return_type,
        param_types: vec![parameter_type.clone()],
        value_types: BTreeMap::from([(value, parameter_type)]),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![value],
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Value(value)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_control_module_with_types(parameter_type: NyarType, return_type: NyarType) -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::valkyrie::mir::{MirBlock, MirBlockRef, MirOperand, MirTerminator, MirValueRef};
    let value = MirValueRef(0);
    let parameter_type = valkyrie_type(&parameter_type);
    let return_type = valkyrie_type(&return_type);
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = return_type;
    function.param_types = vec![parameter_type.clone()];
    function.entry = MirBlockRef(0);
    function.values.clear();
    function.value_types = BTreeMap::from([(value, parameter_type)]);
    function.blocks = vec![MirBlock {
        id: MirBlockRef(0),
        label: "entry".to_string(),
        parameters: vec![value],
        instructions: Vec::new(),
        terminator: MirTerminator::Return { value: Some(MirOperand::Value(value)) },
    }];
    let mut module = lower_test_module(Vec::new(), Vec::new());
    module.functions.push(function);
    module
}

fn rust_sum_payload_submission(
    sum_type: &str,
    variant: &str,
    payload_type: NyarType,
    receiver_type: NyarType,
    output_type: NyarType,
) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("fixture"), nyar::Identifier::new("entry")]);
    let receiver = ValueRef(0);
    let output = ValueRef(1);
    let function = ExecutableFunction {
        symbol: "fixture.entry".to_string(),
        return_type: output_type.clone(),
        param_types: vec![receiver_type.clone()],
        value_types: [(receiver, receiver_type), (output, output_type)].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![receiver],
            instructions: vec![Instruction {
                output: Some(output),
                kind: InstructionKind::SumPayloadGet {
                    sum_type: sum_type.to_string(),
                    variant: variant.to_string(),
                    payload_type,
                    object: Operand::Value(receiver),
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(output)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = rust_sum(sum_type, 32, vec![("Branch", 0, Some(NyarType::Integer32 { signed: true })), ("Empty", 1, None)]);
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_sum_payload_module(
    sum_type: &str,
    variant: &str,
    payload_type: NyarType,
    receiver_type: NyarType,
    output_type: NyarType,
) -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::valkyrie::mir::{MirBlockRef, MirInstruction, MirInstructionKind, MirOperand, MirTerminator, MirValueRef};
    let receiver = MirValueRef(0);
    let output = MirValueRef(1);
    let receiver_type = valkyrie_type(&receiver_type);
    let output_type = valkyrie_type(&output_type);
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = output_type.clone();
    function.param_types = vec![receiver_type.clone()];
    function.entry = MirBlockRef(0);
    function.value_types = [(receiver, receiver_type), (output, output_type)].into_iter().collect();
    function.values.clear();
    function.blocks[0].parameters = vec![receiver];
    function.blocks[0].instructions = vec![MirInstruction {
        output: Some(output),
        kind: MirInstructionKind::SumPayloadGet {
            sum_type: sum_type.to_string(),
            variant: variant.to_string(),
            payload_type: valkyrie_type(&payload_type),
            object: MirOperand::Value(receiver),
        },
    }];
    function.blocks[0].terminator = MirTerminator::Return { value: Some(MirOperand::Value(output)) };
    let mut module = valkyrie_sum(sum_type, 32, vec![("Branch", 0, Some(NyarType::Integer32 { signed: true })), ("Empty", 1, None)]);
    module.functions.push(function);
    module
}

fn rust_aggregate_array_sum_submission() -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("fixture"), nyar::Identifier::new("entry")]);
    let integer = NyarType::Integer32 { signed: true };
    let record = named_type("Record");
    let choice = named_type("Choice");
    let array = NyarType::Array(Box::new(record.clone()));
    let index = ValueRef(0);
    let aggregate = ValueRef(1);
    let initial = ValueRef(2);
    let pushed = ValueRef(3);
    let fetched = ValueRef(4);
    let sum = ValueRef(5);
    let payload = ValueRef(6);
    let function = ExecutableFunction {
        symbol: "fixture.entry".to_string(),
        return_type: record.clone(),
        param_types: vec![integer.clone()],
        value_types: [
            (index, integer.clone()),
            (aggregate, record.clone()),
            (initial, array.clone()),
            (pushed, array.clone()),
            (fetched, record.clone()),
            (sum, choice.clone()),
            (payload, record.clone()),
        ]
        .into_iter()
        .collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![index],
            instructions: vec![
                Instruction {
                    output: Some(aggregate),
                    kind: InstructionKind::StructNew {
                        type_name: "Record".to_string(),
                        storage: StorageKind::Reference,
                        layout_id: Some(7),
                        fields: vec![("slot".to_string(), Operand::Value(index))],
                    },
                },
                Instruction {
                    output: Some(initial),
                    kind: InstructionKind::ArrayLiteral { element_type: record.clone(), items: vec![Operand::Value(aggregate)] },
                },
                Instruction {
                    output: Some(pushed),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(nyar_types::NamePath::new(vec![
                            nyar_types::Identifier::new("neutral"),
                            nyar_types::Identifier::new("operation"),
                        ])),
                        arguments: vec![Operand::Value(initial), Operand::Value(aggregate)],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: Some(vec![array.clone(), record.clone()]),
                        intrinsic_opcode: Some(nyar_types::IntrinsicOpcode::ArrayPush),
                    },
                },
                Instruction {
                    output: Some(fetched),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(nyar_types::NamePath::new(vec![
                            nyar_types::Identifier::new("neutral"),
                            nyar_types::Identifier::new("operation"),
                        ])),
                        arguments: vec![Operand::Value(pushed), Operand::Value(index)],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: Some(vec![array, integer]),
                        intrinsic_opcode: Some(nyar_types::IntrinsicOpcode::ArrayGet),
                    },
                },
                Instruction {
                    output: Some(sum),
                    kind: InstructionKind::SumNew {
                        sum_type: "Choice".to_string(),
                        variant: "Branch".to_string(),
                        payload_type: Some(record.clone()),
                        payload: Some(Operand::Value(fetched)),
                    },
                },
                Instruction {
                    output: Some(payload),
                    kind: InstructionKind::SumPayloadGet {
                        sum_type: "Choice".to_string(),
                        variant: "Branch".to_string(),
                        payload_type: record.clone(),
                        object: Operand::Value(sum),
                    },
                },
            ],
            terminator: Terminator::Return { value: Some(Operand::Value(payload)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = rust_sum("Choice", 32, vec![("Branch", 0, Some(record))]);
    submission.aggregate_layouts.layouts.push(record_layout());
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_aggregate_array_sum_module() -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::{
        types::hir::ValkyrieType,
        valkyrie::mir::{
            MirBlock, MirBlockRef, MirDispatchKind, MirInstruction, MirInstructionKind, MirOperand, MirStorageKind, MirTerminator, MirValueRef,
        },
    };
    let integer = ValkyrieType::Integer32 { signed: true };
    let record = ValkyrieType::Named(nyar_types::Identifier::new("Record"));
    let choice = ValkyrieType::Named(nyar_types::Identifier::new("Choice"));
    let array = ValkyrieType::Array(Box::new(record.clone()));
    let index = MirValueRef(0);
    let aggregate = MirValueRef(1);
    let initial = MirValueRef(2);
    let pushed = MirValueRef(3);
    let fetched = MirValueRef(4);
    let sum = MirValueRef(5);
    let payload = MirValueRef(6);
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = record.clone();
    function.param_types = vec![integer.clone()];
    function.entry = MirBlockRef(0);
    function.values.clear();
    function.value_types = [
        (index, integer.clone()),
        (aggregate, record.clone()),
        (initial, array.clone()),
        (pushed, array.clone()),
        (fetched, record.clone()),
        (sum, choice),
        (payload, record.clone()),
    ]
    .into_iter()
    .collect();
    function.blocks = vec![MirBlock {
        id: MirBlockRef(0),
        label: "entry".to_string(),
        parameters: vec![index],
        instructions: vec![
            MirInstruction {
                output: Some(aggregate),
                kind: MirInstructionKind::StructNew {
                    type_name: "Record".to_string(),
                    storage: MirStorageKind::Reference,
                    layout_id: Some(7),
                    fields: vec![("slot".to_string(), MirOperand::Value(index))],
                },
            },
            MirInstruction {
                output: Some(initial),
                kind: MirInstructionKind::ArrayLiteral { element_type: record.clone(), items: vec![MirOperand::Value(aggregate)] },
            },
            MirInstruction {
                output: Some(pushed),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_types::NamePath::new(vec![
                        nyar_types::Identifier::new("neutral"),
                        nyar_types::Identifier::new("operation"),
                    ])),
                    arguments: vec![MirOperand::Value(initial), MirOperand::Value(aggregate)],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: Some(vec![array.clone(), record.clone()]),
                    intrinsic_opcode: Some(nyar_types::IntrinsicOpcode::ArrayPush),
                },
            },
            MirInstruction {
                output: Some(fetched),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_types::NamePath::new(vec![
                        nyar_types::Identifier::new("neutral"),
                        nyar_types::Identifier::new("operation"),
                    ])),
                    arguments: vec![MirOperand::Value(pushed), MirOperand::Value(index)],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: Some(vec![array, integer]),
                    intrinsic_opcode: Some(nyar_types::IntrinsicOpcode::ArrayGet),
                },
            },
            MirInstruction {
                output: Some(sum),
                kind: MirInstructionKind::SumNew {
                    sum_type: "Choice".to_string(),
                    variant: "Branch".to_string(),
                    payload_type: Some(record.clone()),
                    payload: Some(MirOperand::Value(fetched)),
                },
            },
            MirInstruction {
                output: Some(payload),
                kind: MirInstructionKind::SumPayloadGet {
                    sum_type: "Choice".to_string(),
                    variant: "Branch".to_string(),
                    payload_type: record.clone(),
                    object: MirOperand::Value(sum),
                },
            },
        ],
        terminator: MirTerminator::Return { value: Some(MirOperand::Value(payload)) },
    }];
    let mut module = valkyrie_sum("Choice", 32, vec![("Branch", 0, Some(NyarType::Named(nyar_types::Identifier::new("Record"))))]);
    module.aggregate_layouts.layouts.push(record_layout());
    module.functions.push(function);
    module
}

fn assert_same_observation(case_id: &str, submission: FragmentSubmission, module: nyar_language::valkyrie::mir::MirModule) -> String {
    let rust = semantic_mir_observation(&submission, case_id);
    let valkyrie_result = validate_semantic_module(&module);
    let valkyrie = semantic_observation(case_id, valkyrie_result.as_ref().map(|_| ()).map_err(|error| error));
    assert_eq!(rust, valkyrie, "Semantic MIR observation diverged for {case_id}");
    rust
}

fn rust_static_call_submission(with_external_contract: bool) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("demo"), nyar::Identifier::new("main")]);
    let callee = nyar_types::NamePath::new(vec![nyar_types::Identifier::new("dependency"), nyar_types::Identifier::new("run")]);
    let function = ExecutableFunction {
        symbol: "demo.main".to_string(),
        return_type: NyarType::Unit,
        param_types: Vec::new(),
        value_types: BTreeMap::new(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction { output: None, kind: InstructionKind::LoadConstant { constant: Constant::Unit, ty: Some(NyarType::Unit) } },
                Instruction {
                    output: None,
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(callee.clone()),
                        arguments: Vec::new(),
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: Some(Vec::new()),
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    if with_external_contract {
        submission.external_import_links.insert(
            nyar::QualifiedName::new(callee.parts().to_vec()),
            ExternalImportLink::host(Some(nyar_types::Identifier::new("contract")), vec!["dependency.run".to_string()]),
        );
    }
    submission
}

fn valkyrie_static_call_module(with_external_contract: bool) -> nyar_language::valkyrie::mir::MirModule {
    let symbol = nyar_types::NamePath::new(vec![nyar_types::Identifier::new("dependency"), nyar_types::Identifier::new("run")]);
    let mut module = nyar_language::valkyrie::mir::ssa::test_support::lower_test_module(Vec::new(), Vec::new());
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.blocks[0].instructions.push(nyar_language::valkyrie::mir::MirInstruction {
        output: None,
        kind: nyar_language::valkyrie::mir::MirInstructionKind::Call {
            dispatch: nyar_language::valkyrie::mir::MirDispatchKind::Static,
            callee: nyar_language::valkyrie::mir::MirOperand::Symbol(symbol.clone()),
            arguments: Vec::new(),
            witness: None,
            effect: None,
            receiver_kind: None,
            parameter_types: Some(Vec::new()),
            intrinsic_opcode: None,
        },
    });
    module.functions.push(function);
    if with_external_contract {
        module.external_calls.push(nyar_language::valkyrie::mir::ssa::MirExternalCallContract {
            symbol,
            dispatch: nyar_language::valkyrie::mir::MirDispatchKind::Static,
            parameter_types: Vec::new(),
            return_type: nyar_language::types::hir::ValkyrieType::Unit,
        });
    }
    module
}

fn rust_text_encoding_mismatch_submission() -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("demo"), nyar::Identifier::new("main")]);
    let function = ExecutableFunction {
        symbol: "demo.main".to_string(),
        return_type: NyarType::Unit,
        param_types: Vec::new(),
        value_types: BTreeMap::new(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: None,
                kind: InstructionKind::LoadConstant { constant: Constant::Utf8("text".to_string()), ty: Some(NyarType::Utf16) },
            }],
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_text_encoding_mismatch_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = nyar_language::valkyrie::mir::ssa::test_support::lower_test_module(Vec::new(), Vec::new());
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    let nyar_language::valkyrie::mir::MirInstructionKind::LoadConstant { constant, ty } = &mut function.blocks[0].instructions[0].kind
    else {
        panic!("test fixture requires a materialized literal");
    };
    *constant = nyar_language::valkyrie::mir::MirConstant::Utf8("text".to_string());
    *ty = Some(nyar_language::types::hir::ValkyrieType::Utf16);
    module.functions.push(function);
    module
}

fn rust_utf16_literal_type_mismatch_submission() -> FragmentSubmission {
    let mut submission = rust_text_encoding_mismatch_submission();
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    let InstructionKind::LoadConstant { constant, ty } = &mut function.blocks[0].instructions[0].kind
    else {
        panic!("fixture requires literal")
    };
    *constant = Constant::Utf16("text".to_string());
    *ty = Some(NyarType::Utf8);
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_utf16_literal_type_mismatch_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_text_encoding_mismatch_module();
    let nyar_language::valkyrie::mir::MirInstructionKind::LoadConstant { constant, ty } =
        &mut module.functions[0].blocks[0].instructions[0].kind
    else {
        panic!("fixture requires literal")
    };
    *constant = nyar_language::valkyrie::mir::MirConstant::Utf16("text".to_string());
    *ty = Some(nyar_language::types::hir::ValkyrieType::Utf8);
    module
}

fn rust_residual_pattern_submission() -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    function.blocks[0].instructions.push(Instruction {
        output: None,
        kind: InstructionKind::PatternMatch { value: Operand::Constant(Constant::Unit), pattern_debug: "residual".to_string() },
    });
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_residual_pattern_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    module.functions[0].blocks[0].instructions.push(nyar_language::valkyrie::mir::MirInstruction {
        output: None,
        kind: nyar_language::valkyrie::mir::MirInstructionKind::PatternMatch {
            value: nyar_language::valkyrie::mir::MirOperand::Constant(nyar_language::valkyrie::mir::MirConstant::Unit),
            pattern: nyar_language::types::hir::HirPattern::Wildcard,
        },
    });
    module
}

fn rust_text_convert_submission(source_type: NyarType, target_type: NyarType) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("demo"), nyar::Identifier::new("main")]);
    let input = ValueRef(0);
    let output = ValueRef(1);
    let function = ExecutableFunction {
        symbol: "demo.main".to_string(),
        return_type: target_type.clone(),
        param_types: vec![source_type.clone()],
        value_types: [(input, source_type), (output, target_type)].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![input],
            instructions: vec![Instruction {
                output: Some(output),
                kind: InstructionKind::TextConvert {
                    source_encoding: Some(nyar_types::executable::TextEncoding::Utf8),
                    target_encoding: Some(nyar_types::executable::TextEncoding::Utf16),
                    semantics: Some(nyar_types::executable::TextConversionSemantics::UnicodeScalarPreserving),
                    boundary: Some(nyar_types::executable::TextProjectionBoundary::Language),
                    value: Operand::Value(input),
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(output)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_text_convert_module(
    source_type: nyar_language::types::hir::ValkyrieType,
    target_type: nyar_language::types::hir::ValkyrieType,
) -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::valkyrie::mir::{
        MirBlockRef, MirInstruction, MirInstructionKind, MirOperand, MirTextConversionSemantics, MirTextEncoding, MirTextProjectionBoundary,
        MirValueRef,
    };
    let input = MirValueRef(0);
    let output = MirValueRef(1);
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = target_type.clone();
    function.param_types = vec![source_type.clone()];
    function.entry = MirBlockRef(0);
    function.value_types = [(input, source_type), (output, target_type)].into_iter().collect();
    function.values.clear();
    function.blocks[0].parameters = vec![input];
    function.blocks[0].instructions = vec![MirInstruction {
        output: Some(output),
        kind: MirInstructionKind::TextConvert {
            source_encoding: Some(MirTextEncoding::Utf8),
            target_encoding: Some(MirTextEncoding::Utf16),
            semantics: Some(MirTextConversionSemantics::UnicodeScalarPreserving),
            boundary: Some(MirTextProjectionBoundary::Language),
            value: MirOperand::Value(input),
        },
    }];
    function.blocks[0].terminator = nyar_language::valkyrie::mir::MirTerminator::Return { value: Some(MirOperand::Value(output)) };
    let mut module = nyar_language::valkyrie::mir::ssa::test_support::lower_test_module(Vec::new(), Vec::new());
    module.functions.push(function);
    module
}

fn rust_text_convert_missing_metadata(missing_boundary: bool) -> FragmentSubmission {
    let mut submission = rust_text_convert_submission(NyarType::Utf8, NyarType::Utf16);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    let InstructionKind::TextConvert { semantics, boundary, .. } = &mut function.blocks[0].instructions[0].kind
    else {
        panic!("fixture requires text conversion")
    };
    if missing_boundary {
        *boundary = None;
    }
    else {
        *semantics = None;
    }
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_text_convert_missing_metadata(missing_boundary: bool) -> nyar_language::valkyrie::mir::MirModule {
    let mut module =
        valkyrie_text_convert_module(nyar_language::types::hir::ValkyrieType::Utf8, nyar_language::types::hir::ValkyrieType::Utf16);
    let nyar_language::valkyrie::mir::MirInstructionKind::TextConvert { semantics, boundary, .. } =
        &mut module.functions[0].blocks[0].instructions[0].kind
    else {
        panic!("fixture requires text conversion")
    };
    if missing_boundary {
        *boundary = None;
    }
    else {
        *semantics = None;
    }
    module
}

fn rust_call_arity_mismatch_submission() -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    let InstructionKind::Call { parameter_types, .. } = &mut function.blocks[0].instructions[1].kind
    else {
        panic!("fixture requires static call at instruction 1");
    };
    *parameter_types = Some(vec![NyarType::Integer32 { signed: true }]);
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_call_arity_mismatch_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    let nyar_language::valkyrie::mir::MirInstructionKind::Call { parameter_types, .. } =
        &mut module.functions[0].blocks[0].instructions[1].kind
    else {
        panic!("fixture requires static call at instruction 1");
    };
    *parameter_types = Some(vec![nyar_language::types::hir::ValkyrieType::Integer32 { signed: true }]);
    module
}

fn rust_missing_formal_signature_submission() -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    let InstructionKind::Call { parameter_types, .. } = &mut function.blocks[0].instructions[1].kind
    else {
        panic!("fixture requires static call at instruction 1");
    };
    *parameter_types = None;
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_missing_formal_signature_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    let nyar_language::valkyrie::mir::MirInstructionKind::Call { parameter_types, .. } =
        &mut module.functions[0].blocks[0].instructions[1].kind
    else {
        panic!("fixture requires static call at instruction 1");
    };
    *parameter_types = None;
    module
}

fn rust_missing_output_type_submission() -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    function.blocks[0].instructions[1].output = Some(ValueRef(99));
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_missing_output_type_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    module.functions[0].blocks[0].instructions[1].output = Some(nyar_language::valkyrie::mir::MirValueRef(99));
    module
}

fn rust_missing_entry_submission() -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    function.entry = BlockRef(99);
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn rust_wide_scalar_submission() -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("neutral"), nyar::Identifier::new("wide_scalar")]);
    let value = ValueRef(0);
    let wide = NyarType::Integer128 { signed: true };
    let function = ExecutableFunction {
        symbol: "neutral.wide_scalar".to_string(),
        return_type: wide.clone(),
        param_types: vec![wide.clone()],
        value_types: BTreeMap::from([(value, wide)]),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![value],
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Value(value)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_missing_entry_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    module.functions[0].entry = nyar_language::valkyrie::mir::MirBlockRef(99);
    module
}

fn rust_field_layout_missing_submission() -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    function.blocks[0].instructions.push(Instruction {
        output: None,
        kind: InstructionKind::FieldGet {
            object: Operand::Constant(Constant::Unit),
            field: "member".to_string(),
            storage: StorageKind::Reference,
            layout_id: None,
        },
    });
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_field_layout_missing_module() -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    module.functions[0].blocks[0].instructions.push(nyar_language::valkyrie::mir::MirInstruction {
        output: None,
        kind: nyar_language::valkyrie::mir::MirInstructionKind::FieldGet {
            object: nyar_language::valkyrie::mir::MirOperand::Constant(nyar_language::valkyrie::mir::MirConstant::Unit),
            field: "member".to_string(),
            storage: nyar_language::MirStorageKind::Reference,
            layout_id: None,
        },
    });
    module
}

fn record_layout() -> AggregateLayout {
    AggregateLayout {
        id: 7,
        name: "Record".to_string(),
        namespace: String::new(),
        storage: StorageKind::Reference,
        size: 4,
        align: 4,
        fields: vec![FieldLayout { name: "slot".to_string(), ty: NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 }],
    }
}

fn rust_aggregate_field_submission(storage: StorageKind, field: &str, output_type: Option<NyarType>) -> FragmentSubmission {
    let mut submission = rust_static_call_submission(true);
    submission.aggregate_layouts.layouts.push(record_layout());
    let operation = submission.exported_operations[0].clone();
    let provider = submission.executable.as_ref().expect("fixture has executable");
    let mut function = provider.get_function(&operation).expect("fixture has function").function;
    let output = output_type.map(|ty| {
        let value = ValueRef(77);
        function.value_types.insert(value, ty);
        value
    });
    function.blocks[0].instructions.push(Instruction {
        output,
        kind: InstructionKind::FieldGet { object: Operand::Constant(Constant::Unit), field: field.to_string(), storage, layout_id: Some(7) },
    });
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_aggregate_field_module(
    storage: nyar_language::MirStorageKind,
    field: &str,
    output_type: Option<NyarType>,
) -> nyar_language::valkyrie::mir::MirModule {
    let mut module = valkyrie_static_call_module(true);
    module.aggregate_layouts.layouts.push(record_layout());
    let output = output_type.map(|ty| {
        let value = nyar_language::valkyrie::mir::MirValueRef(77);
        module.functions[0].value_types.insert(value, valkyrie_type(&ty));
        value
    });
    module.functions[0].blocks[0].instructions.push(nyar_language::valkyrie::mir::MirInstruction {
        output,
        kind: nyar_language::valkyrie::mir::MirInstructionKind::FieldGet {
            object: nyar_language::valkyrie::mir::MirOperand::Constant(nyar_language::valkyrie::mir::MirConstant::Unit),
            field: field.to_string(),
            storage,
            layout_id: Some(7),
        },
    });
    module
}

fn rust_text_compare_submission(left_type: NyarType, right_type: NyarType, output_type: NyarType) -> FragmentSubmission {
    let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("fixture"), nyar::Identifier::new("entry")]);
    let left = ValueRef(0);
    let right = ValueRef(1);
    let output = ValueRef(2);
    let function = ExecutableFunction {
        symbol: "fixture.entry".to_string(),
        return_type: output_type.clone(),
        param_types: vec![left_type.clone(), right_type.clone()],
        value_types: [(left, left_type.clone()), (right, right_type.clone()), (output, output_type)].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![left, right],
            instructions: vec![Instruction {
                output: Some(output),
                kind: InstructionKind::Call {
                    dispatch: DispatchKind::Static,
                    callee: Operand::Symbol(nyar_types::NamePath::new(vec![
                        nyar_types::Identifier::new("neutral"),
                        nyar_types::Identifier::new("operation"),
                    ])),
                    arguments: vec![Operand::Value(left), Operand::Value(right)],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: Some(vec![left_type, right_type]),
                    intrinsic_opcode: Some(nyar_types::IntrinsicOpcode::Utf8ContentEqual),
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(output)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.exported_operations.push(operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
    submission
}

fn valkyrie_text_compare_module(left_type: NyarType, right_type: NyarType, output_type: NyarType) -> nyar_language::valkyrie::mir::MirModule {
    use nyar_language::valkyrie::mir::{MirInstruction, MirInstructionKind, MirOperand, MirTerminator, MirValueRef};
    let left = MirValueRef(0);
    let right = MirValueRef(1);
    let output = MirValueRef(2);
    let left_type = valkyrie_type(&left_type);
    let right_type = valkyrie_type(&right_type);
    let output_type = valkyrie_type(&output_type);
    let mut function =
        nyar_language::valkyrie::mir::ssa::test_support::lower_test_function(nyar_language::valkyrie::mir::ssa::test_support::expr(
            nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
        ));
    function.return_type = output_type.clone();
    function.param_types = vec![left_type.clone(), right_type.clone()];
    function.value_types = [(left, left_type.clone()), (right, right_type.clone()), (output, output_type)].into_iter().collect();
    function.values.clear();
    function.blocks[0].parameters = vec![left, right];
    function.blocks[0].instructions = vec![MirInstruction {
        output: Some(output),
        kind: MirInstructionKind::Call {
            dispatch: nyar_language::valkyrie::mir::MirDispatchKind::Static,
            callee: MirOperand::Symbol(nyar_types::NamePath::new(vec![
                nyar_types::Identifier::new("neutral"),
                nyar_types::Identifier::new("operation"),
            ])),
            arguments: vec![MirOperand::Value(left), MirOperand::Value(right)],
            witness: None,
            effect: None,
            receiver_kind: None,
            parameter_types: Some(vec![left_type, right_type]),
            intrinsic_opcode: Some(nyar_types::IntrinsicOpcode::Utf8ContentEqual),
        },
    }];
    function.blocks[0].terminator = MirTerminator::Return { value: Some(MirOperand::Value(output)) };
    let mut module = lower_test_module(Vec::new(), Vec::new());
    module.functions.push(function);
    module
}

#[test]
fn nominal_sum_registry_paired_observations_match() {
    assert_same_observation(
        "nominal_sum.valid",
        rust_sum("Choice", 32, vec![("First", 0, Some(NyarType::Integer32 { signed: true })), ("Second", 1, None)]),
        valkyrie_sum("Choice", 32, vec![("First", 0, Some(NyarType::Integer32 { signed: true })), ("Second", 1, None)]),
    );
    assert_same_observation(
        "nominal_sum.missing_tag_layout",
        rust_sum("Choice", 0, vec![("First", 0, None)]),
        valkyrie_sum("Choice", 0, vec![("First", 0, None)]),
    );
    assert_same_observation(
        "nominal_sum.duplicate_tag",
        rust_sum("Choice", 32, vec![("First", 0, None), ("Second", 0, None)]),
        valkyrie_sum("Choice", 32, vec![("First", 0, None), ("Second", 0, None)]),
    );
}

#[test]
fn aggregate_array_sum_valid_paired_observation_matches() {
    assert_eq!(
        assert_same_observation("aggregate_array_sum.valid", rust_aggregate_array_sum_submission(), valkyrie_aggregate_array_sum_module()),
        "aggregate_array_sum.valid|accept||"
    );
}

#[test]
fn nominal_sum_payload_contract_paired_observations_match() {
    let int = NyarType::Integer32 { signed: true };
    let boolean = NyarType::Boolean;
    let choice = named_type("Choice");
    let other = named_type("Other");
    let cases = [
        ("sum.valid", "Choice", "Branch", int.clone(), choice.clone(), int.clone()),
        ("sum.unknown_type", "Absent", "Branch", int.clone(), choice.clone(), int.clone()),
        ("sum.unknown_variant", "Choice", "Absent", int.clone(), choice.clone(), int.clone()),
        ("sum.variant_without_payload", "Choice", "Empty", int.clone(), choice.clone(), int.clone()),
        ("sum.payload_type_mismatch", "Choice", "Branch", boolean.clone(), choice.clone(), int.clone()),
        ("sum.output_type_mismatch", "Choice", "Branch", int.clone(), choice.clone(), boolean.clone()),
        ("sum.receiver_type_mismatch", "Choice", "Branch", int.clone(), other, int.clone()),
    ];
    for (case_id, sum_type, variant, payload_type, receiver_type, output_type) in cases {
        let observation = assert_same_observation(
            case_id,
            rust_sum_payload_submission(sum_type, variant, payload_type.clone(), receiver_type.clone(), output_type.clone()),
            valkyrie_sum_payload_module(sum_type, variant, payload_type, receiver_type, output_type),
        );
        if case_id == "sum.valid" {
            assert_eq!(observation, "sum.valid|accept||");
        }
        else {
            assert!(observation.starts_with(&format!("{case_id}|reject|SMIR006|")));
        }
    }
}

#[test]
fn nullable_contract_distinguishes_structured_nullable_from_nominal_sum() {
    let payload = named_type("Payload");
    let nullable_payload = NyarType::Nullable(Box::new(payload.clone()));
    let valid = assert_same_observation(
        "nullable.structured_valid",
        rust_control_submission_with_types(nullable_payload.clone(), nullable_payload.clone()),
        valkyrie_control_module_with_types(nullable_payload.clone(), nullable_payload),
    );
    assert_eq!(valid, "nullable.structured_valid|accept||");

    let nominal = named_type("NullableCarrier");
    let confused_receiver = NyarType::Nullable(Box::new(nominal.clone()));
    let observation = assert_same_observation(
        "nullable.nominal_sum_confused",
        rust_sum_payload_submission(
            "NullableCarrier",
            "Branch",
            NyarType::Integer32 { signed: true },
            confused_receiver.clone(),
            NyarType::Integer32 { signed: true },
        ),
        valkyrie_sum_payload_module(
            "NullableCarrier",
            "Branch",
            NyarType::Integer32 { signed: true },
            confused_receiver,
            NyarType::Integer32 { signed: true },
        ),
    );
    assert!(observation.starts_with("nullable.nominal_sum_confused|reject|SMIR006|"));
}

#[test]
fn array_contract_paired_observations_match() {
    use nyar_types::IntrinsicOpcode;
    let int = NyarType::Integer32 { signed: true };
    let boolean = NyarType::Boolean;
    let array_of_int = NyarType::Array(Box::new(int.clone()));
    let cases = [
        ("array.valid", IntrinsicOpcode::ArrayGet, array_of_int.clone(), int.clone(), int.clone(), int.clone(), true),
        ("array.receiver_invalid", IntrinsicOpcode::ArrayGet, boolean.clone(), int.clone(), int.clone(), int.clone(), false),
        ("array.index_invalid", IntrinsicOpcode::ArrayGet, array_of_int.clone(), boolean.clone(), int.clone(), int.clone(), false),
        ("array.result_mismatch", IntrinsicOpcode::ArrayGet, array_of_int.clone(), int.clone(), int.clone(), boolean.clone(), false),
        ("array.element_mismatch", IntrinsicOpcode::ArraySet, array_of_int, int.clone(), boolean, int.clone(), false),
    ];
    for (case_id, opcode, receiver_type, index_type, value_type, output_type, accepts) in cases {
        let observation = assert_same_observation(
            case_id,
            rust_array_intrinsic_submission(opcode, receiver_type.clone(), index_type.clone(), value_type.clone(), output_type.clone()),
            valkyrie_array_intrinsic_module(opcode, receiver_type, index_type, value_type, output_type),
        );
        if accepts {
            assert_eq!(observation, format!("{case_id}|accept||"));
        }
        else {
            assert!(observation.starts_with(&format!("{case_id}|reject|SMIR005|")));
        }
    }
}

#[test]
fn control_flow_contract_paired_observations_match() {
    for (case_id, case) in [
        ("control.return_type_mismatch", ControlCase::Return),
        ("control.jump_type_mismatch", ControlCase::Jump),
        ("control.branch_type_mismatch", ControlCase::Branch),
    ] {
        let observation = assert_same_observation(case_id, rust_control_submission(case), valkyrie_control_module(case));
        assert!(observation.starts_with(&format!("{case_id}|reject|SMIR007|")));
    }
}

#[test]
fn unresolved_static_call_paired_observation_matches() {
    assert_same_observation("call.static_unresolved", rust_static_call_submission(false), valkyrie_static_call_module(false));
}

#[test]
fn exact_external_call_contract_paired_observation_matches() {
    assert_same_observation("call.external_exact_valid", rust_static_call_submission(true), valkyrie_static_call_module(true));
}

#[test]
fn text_encoding_contract_paired_observation_matches() {
    assert_same_observation("literal.utf8_type_mismatch", rust_text_encoding_mismatch_submission(), valkyrie_text_encoding_mismatch_module());
    assert_same_observation(
        "literal.utf16_type_mismatch",
        rust_utf16_literal_type_mismatch_submission(),
        valkyrie_utf16_literal_type_mismatch_module(),
    );
}

#[test]
fn ambiguous_source_text_type_is_rejected_before_semantic_mir() {
    // Rust canonical MIR deliberately has no unqualified text variant. The
    // Valkyrie source boundary must therefore reject the same spelling before
    // any MIR value or backend carrier can be created.
    let case_id = "type.ambiguous_text_forbidden";
    let rust = match "string" {
        "string" | "str" | "String" => format!("{case_id}|reject|SMIR007|function"),
        _ => format!("{case_id}|accept||"),
    };
    let valkyrie = match nyar_language::valkyrie::hir::type_lowering::validate_source_text_type_name("string") {
        Ok(()) => format!("{case_id}|accept||"),
        Err(_) => format!("{case_id}|reject|SMIR007|function"),
    };
    assert_eq!(rust, valkyrie);
    assert_eq!(rust, "type.ambiguous_text_forbidden|reject|SMIR007|function");
}

#[test]
fn explicit_text_conversion_paired_observations_match() {
    use nyar_language::types::hir::ValkyrieType;
    assert_same_observation(
        "intrinsic.text_conversion.valid",
        rust_text_convert_submission(NyarType::Utf8, NyarType::Utf16),
        valkyrie_text_convert_module(ValkyrieType::Utf8, ValkyrieType::Utf16),
    );
    assert_same_observation(
        "intrinsic.text_conversion.source_mismatch",
        rust_text_convert_submission(NyarType::Utf16, NyarType::Utf16),
        valkyrie_text_convert_module(ValkyrieType::Utf16, ValkyrieType::Utf16),
    );
    assert_same_observation(
        "intrinsic.text_conversion.target_mismatch",
        rust_text_convert_submission(NyarType::Utf8, NyarType::Utf8),
        valkyrie_text_convert_module(ValkyrieType::Utf8, ValkyrieType::Utf8),
    );
    assert_eq!(
        assert_same_observation(
            "intrinsic.text_conversion_missing",
            rust_text_convert_missing_metadata(false),
            valkyrie_text_convert_missing_metadata(false)
        ),
        "intrinsic.text_conversion_missing|reject|SMIR004|instruction"
    );
    assert_eq!(
        assert_same_observation(
            "intrinsic.text_projection_undeclared",
            rust_text_convert_missing_metadata(true),
            valkyrie_text_convert_missing_metadata(true)
        ),
        "intrinsic.text_projection_undeclared|reject|SMIR004|instruction"
    );
}

#[test]
fn call_arity_contract_paired_observation_matches() {
    assert_same_observation("call.arity_mismatch", rust_call_arity_mismatch_submission(), valkyrie_call_arity_mismatch_module());
}

#[test]
fn missing_formal_signature_paired_observation_matches() {
    assert_same_observation("call.signature_missing", rust_missing_formal_signature_submission(), valkyrie_missing_formal_signature_module());
}

#[test]
fn missing_ssa_output_type_paired_observation_matches() {
    assert_same_observation("value_type.missing", rust_missing_output_type_submission(), valkyrie_missing_output_type_module());
}

#[test]
fn missing_entry_contract_paired_observation_matches() {
    assert_same_observation("entry.missing", rust_missing_entry_submission(), valkyrie_missing_entry_module());
}

#[test]
fn residual_high_level_pattern_paired_observation_matches() {
    let observation =
        assert_same_observation("residual.high_level_pattern", rust_residual_pattern_submission(), valkyrie_residual_pattern_module());
    assert_eq!(observation, "residual.high_level_pattern|reject|SMIR008|instruction");
}

#[test]
fn aggregate_field_layout_contract_paired_observation_matches() {
    assert_same_observation("aggregate_field.layout_missing", rust_field_layout_missing_submission(), valkyrie_field_layout_missing_module());
}

#[test]
fn aggregate_field_contract_paired_observations_match() {
    let cases = [
        ("aggregate.storage_mismatch", StorageKind::Value, nyar_language::MirStorageKind::Value, "slot", None),
        ("aggregate.field_missing", StorageKind::Reference, nyar_language::MirStorageKind::Reference, "absent", None),
        ("aggregate.field_type_mismatch", StorageKind::Reference, nyar_language::MirStorageKind::Reference, "slot", Some(NyarType::Boolean)),
    ];
    for (case_id, rust_storage, valkyrie_storage, field, output_type) in cases {
        let observation = assert_same_observation(
            case_id,
            rust_aggregate_field_submission(rust_storage, field, output_type.clone()),
            valkyrie_aggregate_field_module(valkyrie_storage, field, output_type),
        );
        assert!(observation.starts_with(&format!("{case_id}|reject|SMIR010|")));
    }
}

#[test]
fn text_intrinsic_contract_paired_observations_match() {
    for (case_id, left_type, right_type, output_type, accepted, code) in [
        ("intrinsic.text_compare.valid", NyarType::Utf8, NyarType::Utf8, NyarType::Boolean, true, ""),
        ("intrinsic.text_encoding_mixed", NyarType::Utf8, NyarType::Utf16, NyarType::Boolean, false, "SMIR007"),
        ("intrinsic.contract_missing", NyarType::Utf8, NyarType::Utf8, NyarType::Integer32 { signed: true }, false, "SMIR005"),
    ] {
        let observation = assert_same_observation(
            case_id,
            rust_text_compare_submission(left_type.clone(), right_type.clone(), output_type.clone()),
            valkyrie_text_compare_module(left_type, right_type, output_type),
        );
        if accepted {
            assert_eq!(observation, format!("{case_id}|accept||"));
        }
        else {
            assert!(observation.starts_with(&format!("{case_id}|reject|{code}|")));
        }
    }
}

#[test]
fn formal_backend_entries_fail_closed_before_emission() {
    let submission = rust_missing_entry_submission();
    let clr = nyar_emitter::testing::lower_fragment_to_clr_msil(&submission).expect_err("CLR entry must reject invalid Semantic MIR");
    assert!(clr.to_string().contains("SMIR009"), "{clr}");

    let jvm = nyar_emitter::testing::lower_fragment_to_jvm_class(&submission).expect_err("JVM entry must reject invalid Semantic MIR");
    assert!(jvm.to_string().contains("SMIR009"), "{jvm}");

    for boundary in [nyar::HostProjectionBoundary::WasmJsGlue, nyar::HostProjectionBoundary::WasiComponent] {
        let wasm = nyar_emitter::testing::lower_fragment_to_wasm_module(&submission, boundary)
            .expect_err("WASM and WASI entries must reject invalid Semantic MIR");
        assert!(wasm.to_string().contains("SMIR009"), "{wasm}");
    }
}

#[test]
fn formal_backend_entries_reject_the_same_physical_contract_gap() {
    let submission = rust_wide_scalar_submission();
    assert_eq!(semantic_mir_observation(&submission, "wide_scalar"), "wide_scalar|accept||");

    for target in [
        nyar_emitter::testing::PhysicalContractTarget::Clr,
        nyar_emitter::testing::PhysicalContractTarget::Jvm,
        nyar_emitter::testing::PhysicalContractTarget::WasmJsGlue,
        nyar_emitter::testing::PhysicalContractTarget::WasiComponent,
    ] {
        assert_eq!(
            nyar_emitter::testing::physical_contract_observation(&submission, "wide_scalar", target),
            "wide_scalar|reject|BPHYS001|function"
        );
    }

    let clr = nyar_emitter::testing::lower_fragment_to_clr_msil(&submission)
        .expect_err("CLR must reject an unmapped physical scalar before preparation");
    assert!(clr.to_string().contains("BPHYS001"), "{clr}");

    let jvm = nyar_emitter::testing::lower_fragment_to_jvm_class(&submission)
        .expect_err("JVM must reject an unmapped physical scalar before preparation");
    assert!(jvm.to_string().contains("BPHYS001"), "{jvm}");

    for boundary in [nyar::HostProjectionBoundary::WasmJsGlue, nyar::HostProjectionBoundary::WasiComponent] {
        let wasm = nyar_emitter::testing::lower_fragment_to_wasm_module(&submission, boundary)
            .expect_err("WASM and WASI must reject an unmapped physical scalar before preparation");
        assert!(wasm.to_string().contains("BPHYS001"), "{wasm}");
    }
}
