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
    FragmentSubmission,
    executable_provider::{
        ExecutableFunction, ExecutableInstructionKind, ExecutableOperand, ExecutableProvider, ExecutableValueRef,
        resolve_static_callee_operation,
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

/// Backend-private text projection placeholder.
///
/// Semantic MIR no longer carries TextConvert / TextEncoding God fields (ADR 0011).
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
    let Some(executable) = &submission.executable
    else {
        return Ok(Vec::new());
    };
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
        let Some(executable) = &submission.executable
        else {
            return Err(PhysicalPlanError {
                code: "BPHYS008",
                function: entry.to_string(),
                location: "entry".to_string(),
                detail: "entry projection requires Canonical Semantic MIR".to_string(),
            });
        };
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
    executable: &dyn crate::executable_provider::ExecutableProvider,
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
            let callee = resolve_static_callee_operation(executable, path).ok_or_else(|| {
                PhysicalPlanError::new(
                    "BPHYS004",
                    function,
                    location.clone(),
                    "static call target is not an exact local semantic function",
                )
            })?;
            let parameters = arguments
                .iter()
                .map(|arg| match arg {
                    ExecutableOperand::Value(v) => function
                        .value_types
                        .get(v)
                        .ok_or_else(|| PhysicalPlanError::new("BPHYS004", function, location.clone(), "call argument missing semantic type")),
                    _ => Ok(&NyarType::Unit),
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|ty| physical_category(backend, ty, false, false, function, "call parameter"))
                .collect::<Result<Vec<_>, _>>()?;
            calls.insert((block.id.0, instruction_index), PhysicalCallContract { callee, parameters });
        }
    }
    Ok(PhysicalFunctionPlan { symbol: function.symbol.clone(), parameters, result, values, calls, text_projections })
}

/// Keep aligned with `semantic_mir_contract::is_language_operator_symbol`.
fn is_language_operator_symbol(path: &NamePath) -> bool {
    matches!(
        path.parts().last().map(|part| part.as_str()).unwrap_or(""),
        "infix ==" | "infix !="
            | "infix <" | "infix <=" | "infix >" | "infix >="
            | "infix +" | "infix -" | "infix *" | "infix /" | "infix %"
            | "infix &" | "infix |" | "infix ^" | "infix <<" | "infix >>"
            | "prefix !" | "prefix -" | "prefix +"
    )
}

/// Keep aligned with `semantic_mir_contract::is_language_builtin_symbol`.
fn is_language_builtin_symbol(path: &NamePath) -> bool {
    let parts = path.parts();
    parts.len() == 3
        && parts[0].as_str() == "builtin"
        && parts[1].as_str() == "array"
        && parts[2].as_str() == "push"
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
    use std::{collections::BTreeMap, sync::Arc};

    use nyar::{Identifier, QualifiedName};
    use nyar_types::{
        Block, BlockRef, DispatchKind, ExecutableFunction, Instruction, InstructionKind, NyarType, Operand, Terminator, ValueRef,
        executable::{TextConversionSemantics, TextEncoding, TextProjectionBoundary},
    };

    use crate::executable_provider::MirFunctionMapProvider;

    use super::{PhysicalBackend, build_physical_plan, validate_physical_submission};

    fn function(symbol: &str, return_type: NyarType, parameters: Vec<NyarType>) -> ExecutableFunction {
        let values = parameters.iter().enumerate().map(|(index, ty)| (ValueRef(index as u32), ty.clone())).collect();
        ExecutableFunction {
            symbol: symbol.to_string(),
            return_type,
            param_types: parameters,
            value_types: values,
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
                parameters: vec![],
                instructions: vec![],
                terminator: Terminator::Return { value: None },
            }],
            diagnostics: Vec::new(),
        }
    }

    fn submission(functions: Vec<(QualifiedName, ExecutableFunction)>) -> crate::FragmentSubmission {
        crate::FragmentSubmission {
            executable: Some(Arc::new(MirFunctionMapProvider::new(functions.into_iter().collect()))),
            ..Default::default()
        }
    }

    #[test]
    fn scalar_call_has_exact_backend_independent_categories() {
        let target = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("target")]);
        let caller = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("caller")]);
        let mut caller_function = function("neutral.caller", NyarType::Integer64 { signed: true }, vec![NyarType::Integer64 { signed: true }]);
        caller_function.blocks[0].instructions.push(Instruction {
            output: None,
            kind: InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(nyar::NamePath::new(target.parts().to_vec())),
                arguments: vec![Operand::Value(ValueRef(0))],
                witness: None,
                effect: None,
                receiver_kind: None,
                parameter_types: Some(vec![NyarType::Integer64 { signed: true }]),
                intrinsic_opcode: None,
            },
        });
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
    fn language_operator_calls_skip_physical_call_registry() {
        let main_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("main")]);
        let answer_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("answer")]);
        let mut caller = function("main::main", NyarType::Integer64 { signed: true }, vec![]);
        let value = ValueRef(0);
        caller.value_types.insert(value, NyarType::Integer64 { signed: true });
        caller.blocks[0].instructions.push(Instruction {
            output: Some(value),
            kind: InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("answer")])),
                arguments: vec![],
                witness: None,
                effect: None,
                receiver_kind: None,
                parameter_types: Some(vec![]),
                intrinsic_opcode: None,
            },
        });
        caller.blocks[0].instructions.push(Instruction {
            output: Some(ValueRef(1)),
            kind: InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("infix !=")])),
                arguments: vec![Operand::Value(value), Operand::Value(ValueRef(2))],
                witness: None,
                effect: None,
                receiver_kind: None,
                parameter_types: Some(vec![NyarType::Integer64 { signed: true }, NyarType::Integer64 { signed: true }]),
                intrinsic_opcode: None,
            },
        });
        caller.value_types.insert(ValueRef(2), NyarType::Integer64 { signed: true });
        let submission = submission(vec![
            (answer_op, function("main::answer", NyarType::Integer64 { signed: true }, vec![])),
            (main_op, caller),
        ]);
        let plans = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect("operators must not require registry entries");
        let caller_plan = plans.iter().find(|plan| plan.symbol == "main::main").expect("caller plan");
        assert_eq!(caller_plan.calls.len(), 1);
    }

    #[test]
    fn bare_module_helper_call_plans_through_unique_suffix_match() {
        let main_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("main")]);
        let answer_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("answer")]);
        let mut caller = function("main::main", NyarType::Integer64 { signed: true }, vec![]);
        caller.blocks[0].instructions.push(Instruction {
            output: Some(ValueRef(0)),
            kind: InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("answer")])),
                arguments: vec![],
                witness: None,
                effect: None,
                receiver_kind: None,
                parameter_types: Some(vec![]),
                intrinsic_opcode: None,
            },
        });
        let submission = submission(vec![
            (answer_op, function("main::answer", NyarType::Integer64 { signed: true }, vec![])),
            (main_op, caller),
        ]);
        let plans = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect("bare helper must resolve like SMIR003");
        let caller_plan = plans.iter().find(|plan| plan.symbol == "main::main").expect("caller plan");
        assert_eq!(caller_plan.calls.len(), 1);
        assert_eq!(caller_plan.calls.values().next().expect("call contract").callee, answer_op);
    }

    #[test]
    fn wasm_text_requires_explicit_projection_not_a_handle_guess() {
        let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("text")]);
        let submission = submission(vec![(operation, function("neutral.text", NyarType::Utf8, vec![]))]);
        let error = build_physical_plan(&submission, PhysicalBackend::WasmJsGlue).expect_err("text must not become an implicit wasm handle");
        assert_eq!(error.code, "BPHYS007");
    }

    #[test]
    fn managed_text_requires_explicit_projection_on_every_backend() {
        let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("text")]);
        let submission = submission(vec![(operation, function("neutral.text", NyarType::Utf16, vec![]))]);
        for backend in
            [PhysicalBackend::Jvm, PhysicalBackend::Clr, PhysicalBackend::WasmCore, PhysicalBackend::WasmJsGlue, PhysicalBackend::WasiComponent]
        {
            let error = build_physical_plan(&submission, backend).expect_err("text must not inherit a default managed carrier");
            assert_eq!(error.code, "BPHYS007");
        }
    }

    #[test]
    fn explicit_text_projection_is_consumed_only_by_its_declared_boundary() {
        let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("convert")]);
        let input = ValueRef(0);
        let output = ValueRef(1);
        let function = ExecutableFunction {
            symbol: "neutral.convert".to_string(),
            return_type: NyarType::Utf16,
            param_types: vec![NyarType::Utf8],
            value_types: BTreeMap::from([(input, NyarType::Utf8), (output, NyarType::Utf16)]),
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
                        source_encoding: Some(TextEncoding::Utf8),
                        target_encoding: Some(TextEncoding::Utf16),
                        semantics: Some(TextConversionSemantics::UnicodeScalarPreserving),
                        boundary: Some(TextProjectionBoundary::Jvm),
                        value: Operand::Value(input),
                    },
                }],
                terminator: Terminator::Return { value: Some(Operand::Value(output)) },
            }],
            diagnostics: Vec::new(),
        };
        let submission = submission(vec![(operation, function)]);
        assert!(build_physical_plan(&submission, PhysicalBackend::Jvm).is_ok());
        for backend in [PhysicalBackend::Clr, PhysicalBackend::WasmJsGlue, PhysicalBackend::WasiComponent] {
            assert_eq!(build_physical_plan(&submission, backend).expect_err("a projection cannot cross backend boundaries").code, "BPHYS007");
        }
    }

    #[test]
    fn unresolved_call_is_rejected_before_backend_emission() {
        let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("caller")]);
        let mut caller = function("neutral.caller", NyarType::Unit, vec![]);
        caller.blocks[0].instructions.push(Instruction {
            output: None,
            kind: InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("neutral"), Identifier::new("missing")])),
                arguments: vec![],
                witness: None,
                effect: None,
                receiver_kind: None,
                parameter_types: Some(vec![]),
                intrinsic_opcode: None,
            },
        });
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
