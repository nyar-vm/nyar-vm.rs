//! Backend-private physical planning derived from Canonical Semantic MIR.
//!
//! This is deliberately a verifier input, not another executable IR. It
//! records only the physical category required for each already-typed SSA
//! value and exact call identities. Backend emitters must consume this plan;
//! they must not rediscover language semantics from descriptors or stack use.

use std::collections::{BTreeMap, BTreeSet};

use nyar::QualifiedName;
use nyar_types::{NamePath, NyarType};

use crate::{
    BackendPrivatePlan,
    FragmentSubmission,
    backend_plan_views::{
        ExecutableFunction, ExecutableInstructionKind, ExecutableOperand, ExecutableValueRef,
    },
};

/// Managed physical target selected before backend preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalBackend {
    Jvm,
    Clr,
    WasmCore,
    WasmJsGlue,
    WasiComponent,
}

/// A verifier category, not a source-language type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalValueCategory {
    Void,
    I32,
    I64,
    F32,
    F64,
    Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalCallContract {
    pub callee: QualifiedName,
    pub parameters: Vec<PhysicalValueCategory>,
}

/// 后端私有文本投影占位。
///
/// Semantic MIR 不再携带 TextConvert / TextEncoding God 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalTextProjection {
    pub authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalFunctionPlan {
    pub symbol: String,
    pub parameters: Vec<PhysicalValueCategory>,
    pub result: PhysicalValueCategory,
    pub values: BTreeMap<ExecutableValueRef, PhysicalValueCategory>,
    pub calls: BTreeMap<(u32, usize), PhysicalCallContract>,
    pub text_projections: BTreeMap<ExecutableValueRef, PhysicalTextProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalPlanError {
    pub code: &'static str,
    pub function: String,
    pub location: String,
    pub detail: String,
}

impl PhysicalPlanError {
    fn new(code: &'static str, function: &ExecutableFunction, location: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { code, function: function.symbol.clone(), location: location.into(), detail: detail.into() }
    }
}

/// Stable, backend-independent observation for the paired Rust/Valkyrie
/// physical contract suite. Backend identities and physical carrier details
/// deliberately do not enter this record.
pub(crate) fn observation(case_id: &str, result: Result<(), &PhysicalPlanError>) -> String {
    match result {
        Ok(()) => format!("{case_id}|accept||"),
        Err(error) => format!("{case_id}|reject|{}|{}", error.code, observation_site(&error.location)),
    }
}

fn observation_site(location: &str) -> &'static str {
    if location == "entry" {
        "entry"
    }
    else if location.contains("instruction") {
        "instruction"
    }
    else if location.contains("block") {
        "block"
    }
    else if location.contains("function") || location.contains("value") || location.contains("parameter") {
        "function"
    }
    else {
        "module"
    }
}

/// Build a backend-private physical plan without consulting backend artifacts,
/// descriptors, local slots, or host carriers.
pub(crate) fn build_physical_plan(
    submission: &FragmentSubmission,
    _backend: PhysicalBackend,
) -> Result<Vec<PhysicalFunctionPlan>, PhysicalPlanError> {
    let executable = &submission.backend_plan;
    executable
        .operations()
        .into_iter()
        .map(|operation| {
            let view = executable.get_function(&operation).ok_or_else(|| PhysicalPlanError {
                code: "BPHYS004",
                function: operation.to_string(),
                location: "function".to_string(),
                detail: "physical planning requires an exact executable function".to_string(),
            })?;
            build_function_plan(executable.as_ref(), &view.function, _backend)
        })
        .collect()
}

/// Require a complete physical plan before any backend-local preparation.
///
/// The plan is intentionally discarded here. Individual backends will consume
/// their own plan in later migration steps; this gate first makes incomplete
/// legacy submissions fail closed at the common boundary.
pub(crate) fn validate_physical_submission(submission: &FragmentSubmission, backend: PhysicalBackend) -> Result<(), PhysicalPlanError> {
    let plans = build_physical_plan(submission, backend)?;
    if let Some(entry) = &submission.entry_operation {
        let executable = &submission.backend_plan;
        if executable.get_function(entry).is_none() {
            return Err(PhysicalPlanError {
                code: "BPHYS008",
                function: entry.to_string(),
                location: "entry".to_string(),
                detail: "entry projection has no exact semantic function".to_string(),
            });
        }
    }
    let _ = plans;
    Ok(())
}

fn build_function_plan(
    executable: &BackendPrivatePlan,
    function: &ExecutableFunction,
    backend: PhysicalBackend,
) -> Result<PhysicalFunctionPlan, PhysicalPlanError> {
    let (text_values, text_projections) = collect_text_projections(function, backend)?;
    let parameters = function
        .param_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            physical_category(backend, ty, false, text_values.contains(&crate::contracts::ValueRef(index as u32)), function, "function")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_text_authorized = function.blocks.iter().any(|block| {
        matches!(
            block.terminator,
            crate::contracts::Terminator::Return { value: Some(ExecutableOperand::Value(value)) } if text_values.contains(&value)
        )
    });
    let result = physical_category(backend, &function.return_type, true, return_text_authorized, function, "function")?;
    let values = function
        .value_types
        .iter()
        .map(|(value, ty)| {
            physical_category(backend, ty, false, text_values.contains(value), function, format!("value {}", value.0))
                .map(|category| (*value, category))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut calls = BTreeMap::new();
    for block in &function.blocks {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let ExecutableInstructionKind::Call { callee, arguments, .. } = &instruction.kind
            else {
                continue;
            };
            let location = format!("block {} instruction {instruction_index}", block.id.0);
            let ExecutableOperand::Symbol(path) = callee
            else {
                // Indirect / value callees are planned later via BackendPrivatePlan.
                continue;
            };
            if is_language_operator_symbol(path) || is_language_builtin_symbol(path) {
                continue;
            }
            let callee = QualifiedName::new(path.parts().to_vec());
            if executable.get_function(&callee).is_none() {
                return Err(PhysicalPlanError::new(
                    "BPHYS004",
                    function,
                    location.clone(),
                    "static call target is not an exact local semantic function",
                ));
            }
            let callee_param_types = executable.get_function(&callee).ok_or_else(|| {
                PhysicalPlanError::new(
                    "BPHYS004",
                    function,
                    location.clone(),
                    "exact static call target disappeared before physical planning",
                )
            })?.function.param_types.clone();
            let parameters = arguments
                .iter()
                .enumerate()
                .map(|(arg_index, arg)| {
                    let owned = match arg {
                        ExecutableOperand::Value(v) => function.value_types.get(v).cloned().ok_or_else(|| {
                            PhysicalPlanError::new("BPHYS004", function, location.clone(), "call argument missing semantic type")
                        })?,
                        ExecutableOperand::Constant(constant) => constant_nyar_type(constant),
                        // 调用点残留的裸 Symbol（如未类型化的 `None`）不得再当成 Unit；
                        // 物理类别以 callee 精确形参合同为准，不向上猜语义。
                        ExecutableOperand::Symbol(_) => callee_param_types.get(arg_index).cloned().ok_or_else(|| {
                            PhysicalPlanError::new(
                                "BPHYS004",
                                function,
                                location.clone(),
                                format!("bare symbol argument has no callee parameter type at index {arg_index}"),
                            )
                        })?,
                    };
                    physical_category(
                        backend,
                        &owned,
                        false,
                        false,
                        function,
                        format!("call parameter[{arg_index}] callee={callee} ty={owned:?}"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            calls.insert((block.id.0, instruction_index), PhysicalCallContract { callee, parameters });
        }
    }
    Ok(PhysicalFunctionPlan { symbol: function.symbol.clone(), parameters, result, values, calls, text_projections })
}

/// Map an executable immediate to its Semantic MIR type for physical planning.
fn constant_nyar_type(constant: &crate::contracts::Constant) -> NyarType {
    match constant {
        crate::contracts::Constant::Int(_) => NyarType::Integer64 { signed: true },
        crate::contracts::Constant::Float64(_) => NyarType::Float64,
        crate::contracts::Constant::Bool(_) => NyarType::Boolean,
        crate::contracts::Constant::Utf8(_) => NyarType::Utf8,
        crate::contracts::Constant::Utf16(_) => NyarType::Utf16,
        crate::contracts::Constant::Unit => NyarType::Unit,
    }
}

/// Keep aligned with `semantic_mir_contract::is_language_operator_symbol`.
fn is_language_operator_symbol(path: &NamePath) -> bool {
    let name = path.parts().last().map(|part| part.as_str()).unwrap_or("");
    nyar_types::builtin_operator::lookup_display_name(name).is_some()
}

/// Keep aligned with `semantic_mir_contract::is_language_builtin_symbol`.
fn is_language_builtin_symbol(path: &NamePath) -> bool {
    let parts = path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    nyar_types::IntrinsicId::resolve_from_segments(&parts).is_some()
}

fn collect_text_projections(
    _function: &ExecutableFunction,
    _backend: PhysicalBackend,
) -> Result<(BTreeSet<ExecutableValueRef>, BTreeMap<ExecutableValueRef, PhysicalTextProjection>), PhysicalPlanError> {
    // TextConvert deleted from Semantic MIR; Utf8/Utf16 identity lives on NyarType.
    Ok((BTreeSet::new(), BTreeMap::new()))
}

fn physical_category(
    _backend: PhysicalBackend,
    ty: &NyarType,
    is_result: bool,
    text_authorized: bool,
    function: &ExecutableFunction,
    location: impl Into<String>,
) -> Result<PhysicalValueCategory, PhysicalPlanError> {
    let category = match ty {
        NyarType::Bottom | NyarType::Unit if is_result => PhysicalValueCategory::Void,
        NyarType::Bottom | NyarType::Unit => {
            return Err(PhysicalPlanError::new("BPHYS001", function, location, "void/unit cannot occupy a physical value slot"));
        }
        NyarType::Boolean | NyarType::Character | NyarType::Integer8 { .. } | NyarType::Integer16 { .. } | NyarType::Integer32 { .. } => {
            PhysicalValueCategory::I32
        }
        NyarType::Integer64 { .. } => PhysicalValueCategory::I64,
        NyarType::Float32 => PhysicalValueCategory::F32,
        NyarType::Float64 => PhysicalValueCategory::F64,
        NyarType::Integer128 { .. } => {
            return Err(PhysicalPlanError::new("BPHYS001", function, location, "i128 has no declared managed physical category"));
        }
        // Utf8 vs Utf16 remain distinct NyarType identities (no TextConvert God opcode).
        NyarType::Utf8 | NyarType::Utf16 => {
            let _ = text_authorized;
            PhysicalValueCategory::Reference
        }
        NyarType::Named(_)
        | NyarType::Apply(_, _)
        | NyarType::Function(_)
        | NyarType::Tuple(_)
        | NyarType::Array(_)
        | NyarType::FixedArray { .. }
        | NyarType::TraitObject(_)
        | NyarType::Nullable(_)
        | NyarType::Union(_) => PhysicalValueCategory::Reference,
    };
    Ok(category)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nyar::{Identifier, QualifiedName};
    use nyar_types::{Block, BlockRef, ExecutableFunction, Instruction, InstructionKind, NyarType, Operand, Terminator, ValueRef};


    use super::{PhysicalBackend, PhysicalValueCategory, build_physical_plan, validate_physical_submission};

    fn function(symbol: &str, return_type: NyarType, parameters: Vec<NyarType>) -> ExecutableFunction {
        let values = parameters.iter().enumerate().map(|(index, ty)| (ValueRef(index as u32), ty.clone())).collect();
        ExecutableFunction {
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

    fn submission(functions: Vec<(QualifiedName, ExecutableFunction)>) -> crate::FragmentSubmission {
        crate::FragmentSubmission {
            backend_plan: Arc::new(crate::BackendPrivatePlan::from_functions(functions.into_iter().collect())),
            ..Default::default()
        }
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
        assert_eq!(
            caller_plan.calls.values().next().expect("call").parameters,
            vec![PhysicalValueCategory::I64]
        );
    }

    #[test]
    fn bare_symbol_call_argument_uses_callee_parameter_type() {
        let target = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("ArrayList"), Identifier::new("push")]);
        let caller = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("SwissTable"), Identifier::new("new")]);
        let mut caller_function = function(
            "std.SwissTable.new",
            NyarType::Named(Identifier::new("SwissTable")),
            vec![NyarType::Integer64 { signed: true }],
        );
        let list = ValueRef(1);
        caller_function.value_types.insert(list, NyarType::Named(Identifier::new("ArrayList")));
        caller_function.blocks[0].instructions.push(instr(
            InstructionKind::Call {
                callee: Operand::Symbol(nyar::NamePath::new(target.parts().to_vec())),
                arguments: vec![
                    Operand::Value(list),
                    Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("None")])),
                ],
            },
            Vec::new(),
        ));
        let option_ty = NyarType::Apply(
            Box::new(NyarType::Named(Identifier::new("Option"))),
            vec![NyarType::Named(Identifier::new("Entry"))],
        );
        let submission = submission(vec![
            (
                target.clone(),
                function(
                    "std.ArrayList.push",
                    NyarType::Unit,
                    vec![NyarType::Named(Identifier::new("ArrayList")), option_ty],
                ),
            ),
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
                callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("infix !=")])),
                arguments: vec![Operand::Value(value), Operand::Value(ValueRef(2))],
            },
            vec![ValueRef(1)],
        ));
        caller.value_types.insert(ValueRef(2), NyarType::Integer64 { signed: true });
        caller.value_types.insert(ValueRef(1), NyarType::Boolean);
        let submission = submission(vec![
            (answer_op, function("main::answer", NyarType::Integer64 { signed: true }, vec![])),
            (main_op, caller),
        ]);
        let plans = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect("operators must not require registry entries");
        let caller_plan = plans.iter().find(|plan| plan.symbol == "main::main").expect("caller plan");
        assert_eq!(caller_plan.calls.len(), 1);
    }

    #[test]
    fn bare_module_helper_call_is_rejected_without_exact_identity() {
        let main_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("main")]);
        let answer_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("answer")]);
        let mut caller = function("main::main", NyarType::Integer64 { signed: true }, vec![]);
        caller.value_types.insert(ValueRef(0), NyarType::Integer64 { signed: true });
        caller.blocks[0].instructions.push(instr(
            InstructionKind::Call {
                callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("answer")])),
                arguments: vec![],
            },
            vec![ValueRef(0)],
        ));
        let submission = submission(vec![
            (answer_op.clone(), function("main::answer", NyarType::Integer64 { signed: true }, vec![])),
            (main_op, caller),
        ]);
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

    #[test]
    fn entry_requires_an_exact_semantic_function() {
        let mut submission = submission(Vec::new());
        submission.entry_operation = Some(QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("entry")]));
        let error = validate_physical_submission(&submission, PhysicalBackend::WasiComponent)
            .expect_err("entry cannot be synthesized without Semantic MIR");
        assert_eq!(error.code, "BPHYS008");
    }
}
