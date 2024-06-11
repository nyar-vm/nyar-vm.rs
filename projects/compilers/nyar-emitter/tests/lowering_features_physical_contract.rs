use std::collections::BTreeMap;

use nyar::{Identifier, QualifiedName};
use nyar_emitter::{
    BackendPrivatePlan, FragmentSubmission,
    lowering::features::physical_contract::{PhysicalBackend, PhysicalValueCategory, build_physical_plan},
};
use nyar_types::{Block, BlockRef, ExecutableFunction, Instruction, InstructionKind, ItemInstanceId, NyarType, Operand, Terminator, ValueRef};

fn function(symbol: &str, return_type: NyarType, parameters: Vec<NyarType>) -> ExecutableFunction {
    let values = parameters.iter().enumerate().map(|(index, ty)| (ValueRef(index as u32), ty.clone())).collect();
    ExecutableFunction {
        return_layout: None,
        value_layouts: BTreeMap::new(),
        symbol: symbol.to_string(),
        return_type,
        param_types: parameters,
        value_types: values,
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
            parameters: vec![],
            instructions: vec![],
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    }
}

fn instr(kind: InstructionKind, results: Vec<ValueRef>) -> Instruction {
    let mut instruction = Instruction::from_kind(kind);
    instruction.results = results;
    instruction
}

fn submission(functions: Vec<(QualifiedName, ExecutableFunction)>) -> FragmentSubmission {
    let mut functions = functions.into_iter().collect::<BTreeMap<_, _>>();
    let identities = functions
        .keys()
        .enumerate()
        .map(|(index, name)| (name.clone(), ItemInstanceId::from_index(index as u32).expect("测试函数身份")))
        .collect::<BTreeMap<_, _>>();
    for function in functions.values_mut() {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let InstructionKind::Call { callee, .. } = &mut instruction.kind {
                    let instance = match &*callee {
                        Operand::Symbol(path) => {
                            let operation = QualifiedName::new(path.parts().to_vec());
                            identities.get(&operation).copied()
                        }
                        _ => None,
                    };
                    if let Some(instance) = instance {
                        *callee = Operand::Item(instance);
                    }
                }
            }
        }
    }
    FragmentSubmission::from_backend_plan(BackendPrivatePlan::from_functions(functions))
}

#[test]
fn constant_int_call_argument_is_not_unit() {
    let target = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("SwissTable"), Identifier::new("new")]);
    let caller = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("HashMap"), Identifier::new("new")]);
    let mut caller_function = function("std.HashMap.new", NyarType::Named(Identifier::new("HashMap")), vec![]);
    caller_function.blocks[0].instructions.push(instr(
        InstructionKind::Call {
            callee: Operand::Symbol(nyar::NamePath::new(target.parts().to_vec())),
            arguments: vec![Operand::Constant(nyar_types::Constant::Int(0))],
        },
        Vec::new(),
    ));
    let submission = submission(vec![
        (
            target.clone(),
            function("std.SwissTable.new", NyarType::Named(Identifier::new("SwissTable")), vec![NyarType::Integer64 { signed: true }]),
        ),
        (caller, caller_function),
    ]);
    let plans = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect("Int(0) capacity must not be planned as Unit");
    let caller_plan = plans.iter().find(|plan| plan.symbol == "std.HashMap.new").expect("caller plan");
    assert_eq!(caller_plan.calls.len(), 1);
    assert_eq!(caller_plan.calls.values().next().expect("call").parameters, vec![PhysicalValueCategory::I64]);
}

#[test]
fn bare_symbol_call_argument_uses_callee_parameter_type() {
    let target = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("ArrayList"), Identifier::new("push")]);
    let caller = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("SwissTable"), Identifier::new("new")]);
    let mut caller_function =
        function("std.SwissTable.new", NyarType::Named(Identifier::new("SwissTable")), vec![NyarType::Integer64 { signed: true }]);
    let list = ValueRef(1);
    caller_function.value_types.insert(list, NyarType::Named(Identifier::new("ArrayList")));
    caller_function.blocks[0].instructions.push(instr(
        InstructionKind::Call {
            callee: Operand::Symbol(nyar::NamePath::new(target.parts().to_vec())),
            arguments: vec![Operand::Value(list), Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("None")]))],
        },
        Vec::new(),
    ));
    let option_ty = NyarType::Apply(Box::new(NyarType::Named(Identifier::new("Option"))), vec![NyarType::Named(Identifier::new("Entry"))]);
    let submission = submission(vec![
        (target.clone(), function("std.ArrayList.push", NyarType::Unit, vec![NyarType::Named(Identifier::new("ArrayList")), option_ty])),
        (caller, caller_function),
    ]);
    let plans = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue)
        .expect("bare Symbol None must take ArrayList.push value parameter category");
    let caller_plan = plans.iter().find(|plan| plan.symbol == "std.SwissTable.new").expect("caller plan");
    assert_eq!(
        caller_plan.calls.values().next().expect("call").parameters,
        vec![PhysicalValueCategory::Reference, PhysicalValueCategory::Reference]
    );
}

#[test]
fn scalar_call_has_exact_backend_independent_categories() {
    let target = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("target")]);
    let caller = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("caller")]);
    let mut caller_function = function("neutral.caller", NyarType::Integer64 { signed: true }, vec![NyarType::Integer64 { signed: true }]);
    caller_function.blocks[0].instructions.push(instr(
        InstructionKind::Call {
            callee: Operand::Symbol(nyar::NamePath::new(target.parts().to_vec())),
            arguments: vec![Operand::Value(ValueRef(0))],
        },
        Vec::new(),
    ));
    let submission = submission(vec![
        (target.clone(), function("neutral.target", NyarType::Integer64 { signed: true }, vec![NyarType::Integer64 { signed: true }])),
        (caller, caller_function),
    ]);
    for backend in
        [PhysicalBackend::Jvm, PhysicalBackend::Clr, PhysicalBackend::WasmCore, PhysicalBackend::WasmJsGlue, PhysicalBackend::WasiComponent]
    {
        let plans = build_physical_plan(&submission, backend).expect("typed scalar calls must plan for every managed backend");
        assert_eq!(plans.iter().find(|plan| plan.symbol == "neutral.caller").expect("caller plan").calls.len(), 1);
    }
}

#[test]
fn exact_helper_and_operator_calls_plan_without_suffix_resolution() {
    let main_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("main")]);
    let answer_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("answer")]);
    let operator_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("infix !=")]);
    let mut caller = function("main::main", NyarType::Integer64 { signed: true }, vec![]);
    let value = ValueRef(0);
    caller.value_types.insert(value, NyarType::Integer64 { signed: true });
    caller.blocks[0].instructions.push(instr(
        InstructionKind::Call {
            callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("main"), Identifier::new("answer")])),
            arguments: vec![],
        },
        vec![value],
    ));
    caller.blocks[0].instructions.push(instr(
        InstructionKind::Call {
            // 运算符按**精确身份**解析；不得再用裸拼写靠后缀猜测。
            callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("main"), Identifier::new("infix !=")])),
            arguments: vec![Operand::Value(value), Operand::Value(ValueRef(2))],
        },
        vec![ValueRef(1)],
    ));
    caller.value_types.insert(ValueRef(2), NyarType::Integer64 { signed: true });
    caller.value_types.insert(ValueRef(1), NyarType::Boolean);
    let submission = submission(vec![
        (answer_op, function("main::answer", NyarType::Integer64 { signed: true }, vec![])),
        (
            operator_op,
            function("main::infix !=", NyarType::Boolean, vec![NyarType::Integer64 { signed: true }, NyarType::Integer64 { signed: true }]),
        ),
        (main_op, caller),
    ]);
    let plans = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect("operators must not require registry entries");
    let caller_plan = plans.iter().find(|plan| plan.symbol == "main::main").expect("caller plan");
    assert_eq!(caller_plan.calls.len(), 2);
}

#[test]
fn bare_module_helper_call_is_rejected_without_exact_identity() {
    let main_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("main")]);
    let answer_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("answer")]);
    let mut caller = function("main::main", NyarType::Integer64 { signed: true }, vec![]);
    caller.value_types.insert(ValueRef(0), NyarType::Integer64 { signed: true });
    caller.blocks[0].instructions.push(instr(
        InstructionKind::Call { callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("answer")])), arguments: vec![] },
        vec![ValueRef(0)],
    ));
    let submission =
        submission(vec![(answer_op.clone(), function("main::answer", NyarType::Integer64 { signed: true }, vec![])), (main_op, caller)]);
    let error = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect_err("bare helper must fail before physical planning");
    assert_eq!(error.code, "BPHYS004");
}

#[test]
fn utf8_and_utf16_map_to_reference_without_text_projection() {
    let utf8 = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("text8")]);
    let utf16 = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("text16")]);
    let submission = submission(vec![
        (utf8, function("neutral.text8", NyarType::Utf8, vec![])),
        (utf16, function("neutral.text16", NyarType::Utf16, vec![])),
    ]);
    for backend in
        [PhysicalBackend::Jvm, PhysicalBackend::Clr, PhysicalBackend::WasmCore, PhysicalBackend::WasmJsGlue, PhysicalBackend::WasiComponent]
    {
        let plans = build_physical_plan(&submission, backend).expect("Utf8/Utf16 identity maps to Reference");
        assert_eq!(plans.iter().find(|plan| plan.symbol == "neutral.text8").expect("utf8").result, PhysicalValueCategory::Reference);
        assert_eq!(plans.iter().find(|plan| plan.symbol == "neutral.text16").expect("utf16").result, PhysicalValueCategory::Reference);
    }
}

#[test]
fn unresolved_call_is_rejected_before_backend_emission() {
    let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("caller")]);
    let mut caller = function("neutral.caller", NyarType::Unit, vec![]);
    caller.blocks[0].instructions.push(instr(
        InstructionKind::Call {
            callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("neutral"), Identifier::new("missing")])),
            arguments: vec![],
        },
        Vec::new(),
    ));
    let error =
        build_physical_plan(&submission(vec![(operation, caller)]), PhysicalBackend::Clr).expect_err("unresolved call must fail closed");
    assert_eq!(error.code, "BPHYS004");
}

#[test]
fn unsupported_wide_scalar_is_rejected_without_a_backend_default() {
    let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("wide")]);
    let submission = submission(vec![(operation, function("neutral.wide", NyarType::Integer128 { signed: true }, vec![]))]);
    let error = build_physical_plan(&submission, PhysicalBackend::Jvm).expect_err("i128 requires an explicit JVM physical contract");
    assert_eq!(error.code, "BPHYS001");
}
