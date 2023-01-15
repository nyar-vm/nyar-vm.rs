use std::collections::{BTreeMap, BTreeSet};

use crate::{concretize_type, types::{NamePath, hir::ValkyrieType}};
use nyar_types::NyarType;
use std_data::text::valkyrie::ParseError;

use crate::mir::{
    MirBlock, MirBlockRef, MirConstant, MirDiagnostic, MirEffectKind, MirFunction, MirModule, MirOperand, MirOperation, MirTerminator,
    MirValueOrigin, MirValueRef,
};

/// Backend-independent Semantic MIR contract observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMirContractError {
    pub code: &'static str,
    pub function: String,
    pub location: String,
    pub detail: String,
}

pub fn semantic_observation(case_id: &str, result: Result<(), &SemanticMirContractError>) -> String {
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
    else if location.contains("layout") || location.contains("sum") {
        "module"
    }
    else {
        "function"
    }
}

pub fn validate_semantic_module(module: &MirModule) -> Result<(), SemanticMirContractError> {
    validate_nominal_sums(module)?;
    for layout in &module.aggregate_layouts.layouts {
        if layout.name.is_empty() || layout.align == 0 || layout.size == 0 {
            return Err(SemanticMirContractError {
                code: "SMIR010",
                function: layout.name.clone(),
                location: "aggregate layout".to_string(),
                detail: "aggregate layout requires name, non-zero size, and non-zero alignment".to_string(),
            });
        }
        if layout.fields.iter().any(|field| field.name.is_empty())
            || layout.fields.iter().enumerate().any(|(index, field)| layout.fields[..index].iter().any(|prior| prior.name == field.name))
        {
            return Err(SemanticMirContractError {
                code: "SMIR010",
                function: layout.name.clone(),
                location: "aggregate layout".to_string(),
                detail: "aggregate layout fields require unique non-empty names".to_string(),
            });
        }
    }
    for function in &module.functions {
        validate_semantic_function(module, function)?;
        validate_aggregate_field_contracts(module, function)?;
    }
    Ok(())
}

/// Field access must carry its aggregate identity. Backend carriers and field
/// spellings are deliberately not usable as a recovery mechanism.
fn validate_aggregate_field_contracts(module: &MirModule, function: &MirFunction) -> Result<(), SemanticMirContractError> {
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let MirOperation::SumNew { sum_type, type_args, variant, payload_type, payload } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let _ = type_args;
                let Some(sum) = module.sum_types.iter().find(|sum| sum.name == *sum_type)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum construction references an undeclared sum".to_string(),
                    });
                };
                let Some(declared) = sum.variants.iter().find(|candidate| candidate.name == *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum construction references an undeclared variant".to_string(),
                    });
                };
                let payload_value_type = match payload {
                    Some(MirOperand::Value(value)) => function.value_types.get(value).and_then(|type_| concretize_type(type_).ok()),
                    Some(_) => None,
                    None => None,
                };
                let output_type = instruction.results.first().and_then(|output| function.value_types.get(output));
                if output_type.is_none_or(|output_ty| !type_matches_sum_owner_valkyrie(output_ty, sum_type))
                    || !payload_type_compatible_valkyrie(payload_type.as_ref(), declared.payload_type.as_ref())
                    || match (&declared.payload_type, payload, payload_value_type.as_ref()) {
                        (None, None, _) => false,
                        (Some(expected), Some(MirOperand::Value(_)), Some(actual)) => !payload_type_slot_compatible_nyar(expected, actual),
                        _ => true,
                    }
                {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum construction contract disagrees with declared sum metadata".to_string(),
                    });
                }
                continue;
            }
            if let MirOperation::SumPayloadGet { sum_type, type_args, variant, payload_type, object } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let _ = type_args;
                let Some(sum) = module.sum_types.iter().find(|sum| sum.name == *sum_type)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: format!("sum payload extraction references undeclared sum `{sum_type}`"),
                    });
                };
                let Some(declared) =
                    sum.variants.iter().find(|candidate| candidate.name == *variant).and_then(|candidate| candidate.payload_type.as_ref())
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: format!("sum `{sum_type}` has no payload-bearing variant `{variant}`"),
                    });
                };
                let receiver_type = match object {
                    MirOperand::Value(value) => function.value_types.get(value),
                    _ => None,
                };
                let output_type = instruction.results.first().and_then(|output| function.value_types.get(output));
                let declared_payload = concretize_type(payload_type).ok();
                if !declared_payload.as_ref().is_some_and(|actual| payload_type_slot_compatible_nyar(declared, actual))
                    || output_type != Some(payload_type)
                    || receiver_type.is_none_or(|ty| !type_matches_sum_owner_valkyrie(ty, sum_type))
                {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum payload extraction contract disagrees with declared sum metadata".to_string(),
                    });
                }
                continue;
            }
            if let MirOperation::SumVariantIs { sum_type, type_args, variant, object } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let Some(sum) = module.sum_types.iter().find(|sum| sum.name == *sum_type)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: format!("SumVariantIs references undeclared sum `{sum_type}`"),
                    });
                };
                if !sum.variants.iter().any(|candidate| candidate.name == *variant) {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: format!("SumVariantIs unknown variant `{sum_type}::{variant}`"),
                    });
                }
                let receiver_type = match object {
                    MirOperand::Value(value) => function.value_types.get(value),
                    _ => None,
                };
                let output_type = instruction.results.first().and_then(|output| function.value_types.get(output));
                if let Some(ValkyrieType::Apply(base, apply_args)) = receiver_type {
                    if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == sum_type.as_str())
                        && type_args.as_slice() != apply_args.as_slice()
                    {
                        return Err(SemanticMirContractError {
                            code: "SMIR006",
                            function: function.symbol.clone(),
                            location,
                            detail: format!("SumVariantIs type_args disagree with receiver NominalInstanceKey (sum={sum_type})"),
                        });
                    }
                    if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == sum_type.as_str())
                        && !apply_args.is_empty()
                        && type_args.is_empty()
                    {
                        return Err(SemanticMirContractError {
                            code: "SMIR006",
                            function: function.symbol.clone(),
                            location,
                            detail: format!("SumVariantIs missing type arguments for generic sum instance `{sum_type}`"),
                        });
                    }
                }
                if output_type != Some(&ValkyrieType::Boolean) || receiver_type.is_none_or(|ty| !type_matches_sum_owner_valkyrie(ty, sum_type))
                {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "SumVariantIs contract incomplete".to_string(),
                    });
                }
                continue;
            }
        }
    }
    Ok(())
}

fn type_matches_sum_owner_valkyrie(ty: &ValkyrieType, sum_type: &str) -> bool {
    match ty {
        ValkyrieType::Named(name) => {
            name.as_str() == sum_type
                || (sum_type == "Result" && name.as_str().ends_with("Result"))
                || (sum_type == "Option" && matches!(name.as_str(), "Option" | "Nullable"))
        }
        ValkyrieType::Apply(base, _) => type_matches_sum_owner_valkyrie(base, sum_type),
        ValkyrieType::Nullable(_) => sum_type == "Option",
        _ => false,
    }
}

fn is_type_parameter_nyar(ty: &NyarType) -> bool {
    match ty {
        NyarType::Named(name) => {
            let text = name.as_str();
            !text.is_empty() && text.chars().all(|ch| ch.is_ascii_uppercase())
        }
        _ => false,
    }
}

/// Return/operand SMIR007: accept equal Valkyrie shapes or the same concretized
/// platform type (`Utf16` ≡ `Named(Utf16Text)`, `usize` ≡ `i32`, …), and
/// Result/Option alias spelling (`VonParseResult<T>` ≡ `Result<T, E>`).
fn mir_return_types_compatible(actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
    if actual == expected {
        return true;
    }
    if result_or_option_alias_compatible(actual, expected) {
        return true;
    }
    match (concretize_type(actual), concretize_type(expected)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn sum_owner_name_valkyrie(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => sum_owner_name_valkyrie(base),
        _ => None,
    }
}

fn is_result_shaped_valkyrie(ty: &ValkyrieType) -> bool {
    sum_owner_name_valkyrie(ty).is_some_and(|name| name == "Result" || name.ends_with("Result"))
}

fn is_option_shaped_valkyrie(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Nullable(_) => true,
        _ => sum_owner_name_valkyrie(ty).is_some_and(|name| matches!(name, "Option" | "Nullable")),
    }
}

fn result_or_option_alias_compatible(actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
    let fine_payload = |ty: &ValkyrieType| -> Option<ValkyrieType> {
        match ty {
            ValkyrieType::Apply(_, args) => args.first().cloned(),
            _ => None,
        }
    };
    let payloads_match = match (fine_payload(actual), fine_payload(expected)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    payloads_match
        && ((is_result_shaped_valkyrie(actual) && is_result_shaped_valkyrie(expected))
            || (is_option_shaped_valkyrie(actual) && is_option_shaped_valkyrie(expected)))
}

fn payload_type_slot_compatible_nyar(declared: &NyarType, actual: &NyarType) -> bool {
    declared == actual || is_type_parameter_nyar(declared)
}

fn payload_type_compatible_valkyrie(actual: Option<&ValkyrieType>, declared: Option<&NyarType>) -> bool {
    match (actual, declared) {
        (None, None) => true,
        (Some(actual), Some(declared)) => {
            concretize_type(actual).ok().is_some_and(|actual| payload_type_slot_compatible_nyar(declared, &actual))
        }
        _ => false,
    }
}

fn validate_nominal_sums(module: &MirModule) -> Result<(), SemanticMirContractError> {
    for sum in &module.sum_types {
        if sum.name.is_empty() || sum.variants.is_empty() || sum.tag_width == 0 {
            return Err(SemanticMirContractError {
                code: "SMIR006",
                function: sum.name.clone(),
                location: "sum layout".to_string(),
                detail: "nominal sum layout requires a name, tag width, and at least one variant".to_string(),
            });
        }
        for (index, variant) in sum.variants.iter().enumerate() {
            if variant.name.is_empty() {
                return Err(SemanticMirContractError {
                    code: "SMIR006",
                    function: sum.name.clone(),
                    location: "sum variant".to_string(),
                    detail: format!("nominal sum variant {index} has no name"),
                });
            }
            if sum.variants[..index].iter().any(|prior| prior.name == variant.name || prior.tag == variant.tag) {
                return Err(SemanticMirContractError {
                    code: "SMIR006",
                    function: sum.name.clone(),
                    location: "sum variant".to_string(),
                    detail: format!("nominal sum variant {} duplicates a prior name or tag", variant.name),
                });
            }
        }
    }
    Ok(())
}

fn validate_semantic_function(module: &MirModule, function: &MirFunction) -> Result<(), SemanticMirContractError> {
    let error = |code, location: String, detail| SemanticMirContractError { code, function: function.symbol.clone(), location, detail };
    if !function.blocks.iter().any(|block| block.id == function.entry) {
        return Err(error("SMIR009", "entry".to_string(), "entry block is absent".to_string()));
    }
    let value_type = |operand: &MirOperand| match operand {
        MirOperand::Value(value) => function.value_types.get(value).cloned(),
        MirOperand::Constant(MirConstant::Utf8(_)) => Some(ValkyrieType::Utf8),
        MirOperand::Constant(MirConstant::Utf16(_)) => Some(ValkyrieType::Utf16),
        MirOperand::Constant(MirConstant::Bool(_)) => Some(ValkyrieType::Boolean),
        MirOperand::Constant(MirConstant::Int(_)) => Some(ValkyrieType::Integer64 { signed: true }),
        MirOperand::Constant(MirConstant::Float64(_)) => Some(ValkyrieType::Float64),
        MirOperand::Constant(MirConstant::Unit) => Some(ValkyrieType::Unit),
        MirOperand::Symbol(_) => None,
    };
    for block in &function.blocks {
        for (index, parameter) in block.parameters.iter().enumerate() {
            if !function.value_types.contains_key(parameter) {
                return Err(error("SMIR001", format!("block {} parameter {index}", block.id.0), "block parameter has no SSA type".to_string()));
            }
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            let location = format!("block {} instruction {index}", block.id.0);
            if matches!(instruction.kind, MirOperation::PatternMatch { .. }) {
                return Err(error("SMIR008", location, "residual PatternMatch instruction".to_string()));
            }
            for result in &instruction.results {
                if !function.value_types.contains_key(result) {
                    return Err(error("SMIR001", location.clone(), "instruction output has no SSA type".to_string()));
                }
            }
            if let MirOperation::LoadConstant { constant, ty } = &instruction.kind {
                if let Some(literal_type) = text_constant_type(constant) {
                    if ty.as_ref() != Some(&literal_type) {
                        return Err(error(
                            "SMIR007",
                            location.clone(),
                            "text constant encoding/type contract is absent or disagrees with the literal".to_string(),
                        ));
                    }
                    if let Some(output) = instruction.results.first() {
                        if function.value_types.get(output) != Some(&literal_type) {
                            return Err(error(
                                "SMIR007",
                                location.clone(),
                                "text constant result SSA type disagrees with the literal encoding".to_string(),
                            ));
                        }
                    }
                }
            }
            if let MirOperation::Call { .. } = &instruction.kind {
                validate_static_call_resolution(module, function, &instruction.kind, location.clone())?;
            }
        }
        let location = format!("block {} terminator", block.id.0);
        match &block.terminator {
            MirTerminator::Return { value: Some(value) } => {
                let Some(actual) = value_type(value)
                else {
                    return Err(error("SMIR001", location, "return operand has no SSA type".to_string()));
                };
                if !mir_return_types_compatible(&actual, &function.return_type) {
                    return Err(error(
                        "SMIR007",
                        location,
                        format!(
                            "return operand type differs from function return type (actual={actual:?}, expected={:?})",
                            function.return_type
                        ),
                    ));
                }
            }
            MirTerminator::Jump { target, arguments } => {
                let Some(destination) = function.blocks.iter().find(|candidate| candidate.id == *target)
                else {
                    return Err(error("SMIR007", location, "jump target is absent".to_string()));
                };
                if destination.parameters.len() != arguments.len() {
                    return Err(error("SMIR007", location, "jump arity differs from target block parameters".to_string()));
                }
                for (argument, parameter) in arguments.iter().zip(&destination.parameters) {
                    let Some(actual) = value_type(argument) else {
                        return Err(error("SMIR001", location.clone(), "jump argument has no SSA type".to_string()));
                    };
                    let Some(expected) = function.value_types.get(parameter) else {
                        return Err(error("SMIR001", location.clone(), "jump target block parameter has no SSA type".to_string()));
                    };
                    if !mir_return_types_compatible(&actual, expected) {
                        return Err(error(
                            "SMIR007",
                            location,
                            format!(
                                "jump argument type differs from target block parameter (actual={actual:?}, expected={expected:?})"
                            ),
                        ));
                    }
                }
            }
            MirTerminator::Branch { condition, .. } => {
                if value_type(condition) != Some(ValkyrieType::Boolean) {
                    return Err(error("SMIR007", location, "branch condition must be bool".to_string()));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Language operators lower as `Call` to `infix +` / `prefix !` (sometimes `primitive.infix +`).
/// Backends expand them to target primitives — they are not registry-linked functions.
pub fn is_language_operator_symbol(symbol: &NamePath) -> bool {
    is_language_operator_name(symbol.parts().last().map(|part| part.as_str()).unwrap_or(""))
}

pub fn is_language_operator_name(name: &str) -> bool {
    matches!(
        name,
        "infix ==" | "infix !="
            | "infix <" | "infix <=" | "infix >" | "infix >="
            | "infix +" | "infix -" | "infix *" | "infix /" | "infix %"
            | "infix &" | "infix |" | "infix ^" | "infix <<" | "infix >>"
            | "prefix !" | "prefix -" | "prefix +"
    )
}

/// Language builtins (`builtin.array.push`, …) lower as `Call` but expand in backends — not registry-linked.
pub fn is_language_builtin_symbol(symbol: &NamePath) -> bool {
    let parts = symbol.parts();
    parts.len() == 3
        && parts[0].as_str() == "builtin"
        && parts[1].as_str() == "array"
        && parts[2].as_str() == "push"
}

fn validate_static_call_resolution(
    module: &MirModule,
    function: &MirFunction,
    kind: &MirOperation,
    location: String,
) -> Result<(), SemanticMirContractError> {
    let MirOperation::Call { callee, .. } = kind
    else {
        return Ok(());
    };
    let MirOperand::Symbol(symbol) = callee
    else {
        return Err(SemanticMirContractError {
            code: "SMIR003",
            function: function.symbol.clone(),
            location,
            detail: "static call requires an explicit callee symbol".to_string(),
        });
    };
    if is_language_operator_symbol(symbol) || is_language_builtin_symbol(symbol) {
        return Ok(());
    }
    let exact_local = module.functions.iter().any(|candidate| candidate.symbol == symbol.to_string());
    let exact_external = module.external_calls.iter().any(|candidate| candidate.symbol == *symbol);
    if exact_local || exact_external {
        Ok(())
    }
    else {
        Err(SemanticMirContractError {
            code: "SMIR003",
            function: function.symbol.clone(),
            location,
            detail: format!("static callee `{symbol}` is absent from the exact local and dependency-export registries"),
        })
    }
}

fn text_constant_type(constant: &MirConstant) -> Option<ValkyrieType> {
    match constant {
        MirConstant::Utf8(_) => Some(ValkyrieType::Utf8),
        MirConstant::Utf16(_) => Some(ValkyrieType::Utf16),
        _ => None,
    }
}

pub fn validate_module(module: &MirModule) -> Result<(), ParseError> {
    validate_semantic_module(module)
        .map_err(|error| ParseError::invalid(format!("{} {} at {}: {}", error.code, error.function, error.location, error.detail)))?;
    for diagnostic in &module.diagnostics {
        match diagnostic {
            MirDiagnostic::PatternLoweringFailed { reason, .. } => {
                return Err(ParseError::invalid(format!("MIR lowering 失败：{reason}")));
            }
            MirDiagnostic::UnsupportedExpression { span, kind } => {
                return Err(ParseError::invalid(format!("MIR lowering rejected unsupported HIR expression `{kind}` at source span {span:?}")));
            }
        }
        if let MirDiagnostic::UnsupportedExpression { span, kind } = diagnostic {
            return Err(ParseError::invalid(format!("MIR lowering rejected unsupported HIR expression `{kind}` at source span {span:?}")));
        }
    }
    for function in &module.functions {
        validate_function(function)?;
    }
    Ok(())
}

fn validate_function(function: &MirFunction) -> Result<(), ParseError> {
    let blocks: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block.parameters.len())).collect();
    let block_map: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block)).collect();
    if !blocks.contains_key(&function.entry) {
        return Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{}` 的 entry block 不存在", function.symbol)));
    }

    let reachable_blocks = collect_reachable_blocks(function);
    for block in &function.blocks {
        if !reachable_blocks.contains(&block.id) {
            continue;
        }
        match &block.terminator {
            MirTerminator::Jump { target, arguments } => validate_jump_target(function, block, *target, arguments, &blocks, &block_map)?,
            MirTerminator::Branch { then_target, else_target, .. } => {
                ensure_target_exists(&function.symbol, block.label.as_str(), *then_target, &blocks)?;
                ensure_target_exists(&function.symbol, block.label.as_str(), *else_target, &blocks)?;
            }
            MirTerminator::PerformEffect { effect, payload, resume_target, .. } => {
                validate_effect_payload(&function.symbol, block.label.as_str(), *effect, payload.is_some())?;
                validate_effect_resume_target(&function.symbol, block.label.as_str(), *effect, *resume_target, &blocks)?;
                if let Some(payload_type) = payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)) {
                    validate_effect_payload_static_type(&function.symbol, block.label.as_str(), *effect, &payload_type)?;
                }
                if let (Some(expected_type), Some(resume_block)) = (
                    infer_effect_resume_static_type(
                        *effect,
                        payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)).as_ref(),
                        carrier_type_for_block(function, *resume_target),
                    ),
                    block_map.get(resume_target),
                ) {
                    validate_resume_block_parameter_static_type(
                        &function.symbol,
                        block.label.as_str(),
                        function,
                        *resume_block,
                        &expected_type,
                    )?;
                }
            }
            MirTerminator::StateDispatch { state, cases, default_target } => {
                ensure_target_exists(&function.symbol, block.label.as_str(), *default_target, &blocks)?;
                if !function.value_types.contains_key(state) {
                    return Err(ParseError::invalid(format!(
                        "控制流调度校验失败：函数 `{}` 的 dispatch block `{}` 引用了未知 state 值",
                        function.symbol, block.label
                    )));
                }
                for (case_key, target) in cases {
                    ensure_target_exists(&function.symbol, block.label.as_str(), *target, &blocks)?;
                    let _ = case_key;
                }
            }
            // ADR 0011: suspend_plan / frame_layouts no longer live on Semantic MirFunction.
            MirTerminator::YieldToRuntime { effect, payload, resume_state: _ } => {
                validate_effect_payload(&function.symbol, block.label.as_str(), *effect, payload.is_some())?;
                if let Some(payload_type) = payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)) {
                    validate_effect_payload_static_type(&function.symbol, block.label.as_str(), *effect, &payload_type)?;
                }
            }
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }
    Ok(())
}

fn collect_reachable_blocks(function: &MirFunction) -> BTreeSet<MirBlockRef> {
    let mut reachable = BTreeSet::new();
    let mut worklist = vec![function.entry];
    let blocks: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block)).collect();

    while let Some(block_id) = worklist.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        let Some(block) = blocks.get(&block_id)
        else {
            continue;
        };
        match &block.terminator {
            MirTerminator::Jump { target, .. } => worklist.push(*target),
            MirTerminator::Branch { then_target, else_target, .. } => {
                worklist.push(*then_target);
                worklist.push(*else_target);
            }
            MirTerminator::PerformEffect { resume_target, .. } => worklist.push(*resume_target),
            MirTerminator::StateDispatch { cases, default_target, .. } => {
                worklist.push(*default_target);
                for (_, target) in cases {
                    worklist.push(*target);
                }
            }
            MirTerminator::YieldToRuntime { .. } => {}
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }

    reachable
}

fn validate_jump_target(
    function: &MirFunction,
    block: &MirBlock,
    target: MirBlockRef,
    arguments: &[MirOperand],
    blocks: &BTreeMap<MirBlockRef, usize>,
    block_map: &BTreeMap<MirBlockRef, &MirBlock>,
) -> Result<(), ParseError> {
    validate_jump_shape(&function.symbol, block.label.as_str(), target, arguments.len(), blocks)?;
    let Some(target_block) = block_map.get(&target)
    else {
        return Ok(());
    };
    for (index, (argument, parameter)) in arguments.iter().zip(target_block.parameters.iter()).enumerate() {
        let Some(argument_type) = infer_operand_static_type(function, argument)
        else {
            continue;
        };
        let Some(parameter_type) = function.value_types.get(parameter)
        else {
            continue;
        };
        if argument_type != *parameter_type {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：`MIR` 函数 `{}` 的 block `{}` 跳向 `{}` 时，第 {} 个 Jump 参数类型为 `{}`，目标参数类型为 `{}`",
                function.symbol,
                block.label,
                target_block.label,
                index + 1,
                display_type(&argument_type),
                display_type(parameter_type)
            )));
        }
    }
    Ok(())
}

fn validate_effect_resume_target(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    resume_target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    ensure_target_exists(function_name, block_label, resume_target, blocks)?;
    let actual_parameter_count = blocks.get(&resume_target).copied().unwrap_or_default();
    let expected_parameter_count = expected_effect_resume_parameter_count(effect);
    if actual_parameter_count != expected_parameter_count {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 所指向的 effect 恢复点参数个数不合法，期望 {expected_parameter_count} 个，当前为 {actual_parameter_count}"
        )));
    }
    Ok(())
}

fn validate_effect_payload(function_name: &str, block_label: &str, effect: MirEffectKind, has_payload: bool) -> Result<(), ParseError> {
    if payload_required(effect) || has_payload {
        if has_payload {
            return Ok(());
        }
        if payload_required(effect) {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 缺少 effect payload"
            )));
        }
    }
    Ok(())
}

fn validate_effect_payload_static_type(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    payload_type: &ValkyrieType,
) -> Result<(), ParseError> {
    if matches!(effect, MirEffectKind::Await | MirEffectKind::AsyncSpawn | MirEffectKind::AsyncBlock)
        && future_resume_type(payload_type).is_none()
    {
        let effect_name = match effect {
            MirEffectKind::Await => "`await`",
            MirEffectKind::AsyncSpawn => "`awake`",
            MirEffectKind::AsyncBlock => "`block`",
            _ => unreachable!(),
        };
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 的 {effect_name} effect payload 类型 `{}` 不满足 `Future<T>` / `Promise<T>` 形状",
            display_type(payload_type)
        )));
    }
    if matches!(effect, MirEffectKind::DelegateYield) && generator_source_type(payload_type).is_none() {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 的 `yield from` effect payload 类型 `{}` 不满足 `Generator<T>` / `Iterator<T>` 形状",
            display_type(payload_type)
        )));
    }
    Ok(())
}

fn validate_resume_block_parameter_static_type(
    function_name: &str,
    block_label: &str,
    function: &MirFunction,
    resume_block: &MirBlock,
    expected_type: &ValkyrieType,
) -> Result<(), ParseError> {
    let Some(parameter) = resume_block.parameters.first()
    else {
        return Ok(());
    };
    let Some(actual_type) = function.value_types.get(parameter)
    else {
        return Ok(());
    };
    if actual_type == expected_type {
        return Ok(());
    }
    Err(ParseError::invalid(format!(
        "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 所指向恢复点参数类型为 `{}`，期望 `{}`",
        display_type(actual_type),
        display_type(expected_type)
    )))
}

fn infer_operand_static_type(function: &MirFunction, operand: &MirOperand) -> Option<ValkyrieType> {
    match operand {
        MirOperand::Constant(constant) => Some(infer_constant_type(constant)),
        MirOperand::Value(value_ref) => function.value_types.get(value_ref).cloned().or_else(|| {
            let value = function.values.iter().find(|value| value.id == *value_ref)?;
            match &value.origin {
                MirValueOrigin::Parameter { index, .. } => function.param_types.get(*index).cloned(),
                _ => None,
            }
        }),
        MirOperand::Symbol(_) => None,
    }
}

fn infer_effect_resume_static_type(
    effect: MirEffectKind,
    payload_type: Option<&ValkyrieType>,
    carrier_type: Option<&ValkyrieType>,
) -> Option<ValkyrieType> {
    match effect {
        MirEffectKind::Yield | MirEffectKind::DelegateYield => Some(ValkyrieType::Unit),
        MirEffectKind::Await | MirEffectKind::AsyncBlock => payload_type.and_then(future_resume_type),
        MirEffectKind::AsyncSpawn => None,
        MirEffectKind::Raise => carrier_type.map(|_ty| ValkyrieType::Named(crate::types::Identifier::new("Never"))),
    }
}

/// 查找与给定 resume_target 关联的 suspend point 的 carrier_type。
///
/// 校验阶段需要 carrier_type 来推断 `Raise` 路径的 resume 参数类型；该函数从函数的
/// `suspend_points` 中匹配 `resume_target` 一致的挂起点并返回其 carrier_type。
fn carrier_type_for_block(_function: &MirFunction, _resume_target: MirBlockRef) -> Option<&ValkyrieType> {
    // ADR 0011: suspend_points no longer live on Semantic MirFunction.
    None
}

fn infer_constant_type(constant: &MirConstant) -> ValkyrieType {
    match constant {
        MirConstant::Int(_) => ValkyrieType::Integer64 { signed: true },
        MirConstant::Float64(_) => ValkyrieType::Float64,
        MirConstant::Bool(_) => ValkyrieType::Boolean,
        MirConstant::Utf8(_) => ValkyrieType::Utf8,
        MirConstant::Utf16(_) => ValkyrieType::Utf16,
        MirConstant::Unit => ValkyrieType::Unit,
    }
}

fn future_resume_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Future" | "Promise")) => {
            arguments.first().cloned()
        }
        _ => None,
    }
}

fn generator_source_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments)
            if !arguments.is_empty() && matches!(named_type_name(base), Some("Generator" | "Iterator" | "Coroutine")) =>
        {
            arguments.first().cloned()
        }
        ValkyrieType::Named(name) if matches!(name.as_str(), "Generator" | "Iterator" | "Coroutine") => Some(ValkyrieType::Unit),
        _ => None,
    }
}

fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

fn display_type(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Void => "void".to_string(),
        ValkyrieType::Unit => "unit".to_string(),
        ValkyrieType::Boolean => "bool".to_string(),
        ValkyrieType::Integer8 { signed } => integer_type_name(*signed, 8),
        ValkyrieType::Integer16 { signed } => integer_type_name(*signed, 16),
        ValkyrieType::Integer32 { signed } => integer_type_name(*signed, 32),
        ValkyrieType::Integer64 { signed } => integer_type_name(*signed, 64),
        ValkyrieType::Integer128 { signed } => integer_type_name(*signed, 128),
        ValkyrieType::Float32 => "f32".to_string(),
        ValkyrieType::Float64 => "f64".to_string(),
        ValkyrieType::Character => "char".to_string(),
        ValkyrieType::Utf8 => "utf8".to_string(),
        ValkyrieType::Utf16 => "utf16".to_string(),
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::Apply(base, arguments) => {
            format!("{}<{}>", display_type(base), arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Generic(generic) => generic.name.to_string(),
        ValkyrieType::Function(function) => format!(
            "micro({}) -> {}",
            function.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
            display_type(&function.return_type)
        ),
        ValkyrieType::Tuple(items) => format!("({})", items.iter().map(display_type).collect::<Vec<_>>().join(", ")),
        ValkyrieType::Row(row) => format!(
            "{{ {} }}",
            row.methods
                .iter()
                .map(|method| {
                    format!(
                        "{}({}) -> {}",
                        method.name,
                        method.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
                        display_type(&method.return_type)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ValkyrieType::Array(item) => format!("[{}]", display_type(item)),
        ValkyrieType::FixedArray { element, length } => format!("[{}; {}]", display_type(element), length),
        ValkyrieType::TypeLambda(lambda) => format!(
            "type lambda({}) -> {}",
            lambda.params.iter().map(|item| item.name.to_string()).collect::<Vec<_>>().join(", "),
            display_type(&lambda.body)
        ),
        ValkyrieType::TraitObject(object) => {
            format!("{}<{}>", object.trait_path, object.type_arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Associated(associated) => format!("{}::{}", display_type(&associated.base), associated.name),
        ValkyrieType::AutoType => "auto".to_string(),
        ValkyrieType::SelfType => "Self".to_string(),
        ValkyrieType::Nullable(payload) => format!("{}?", display_type(payload)),
        ValkyrieType::Union(items) => items.iter().map(display_type).collect::<Vec<_>>().join(" | "),
        ValkyrieType::Intersection(items) => items.iter().map(display_type).collect::<Vec<_>>().join(" & "),
    }
}

fn integer_type_name(signed: bool, bits: u16) -> String {
    if signed { format!("i{bits}") } else { format!("u{bits}") }
}

fn expected_effect_resume_parameter_count(effect: MirEffectKind) -> usize {
    match effect {
        MirEffectKind::AsyncSpawn => 0,
        MirEffectKind::Raise | MirEffectKind::Yield | MirEffectKind::DelegateYield | MirEffectKind::Await | MirEffectKind::AsyncBlock => 1,
    }
}

fn payload_required(effect: MirEffectKind) -> bool {
    let _ = effect;
    true
}

fn validate_jump_shape(
    function_name: &str,
    block_label: &str,
    target: MirBlockRef,
    argument_count: usize,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    let Some(expected_parameter_count) = blocks.get(&target).copied()
    else {
        return Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{function_name}` 的 block `{block_label}` 跳转到了不存在的目标块")));
    };
    if expected_parameter_count != argument_count {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的 block `{block_label}` 传出的参数数量与目标块参数数量不一致"
        )));
    }
    Ok(())
}

fn ensure_target_exists(
    function_name: &str,
    block_label: &str,
    target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    if blocks.contains_key(&target) {
        Ok(())
    }
    else {
        Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{function_name}` �?block `{block_label}` 指向了不存在的目标块")))
    }
}

#[cfg(test)]
mod semantic_contract_tests {
    use super::*;
    use crate::{
        mir::{MirInstruction, MirOperand, MirOperation, SumTypeLayout, SumVariantLayout, ssa::MirExternalCallContract},
        types::{Identifier, NamePath, hir::HirExprKind},
    };

    fn empty_module() -> MirModule {
        crate::mir::ssa::test_support::lower_test_module(Vec::new(), Vec::new())
    }

    #[test]
    fn semantic_contract_rejects_nominal_sum_without_tag_layout() {
        let mut module = empty_module();
        module.sum_types.push(SumTypeLayout {
            name: "Choice".to_string(),
            is_unite: true,
            tag_width: 0,
            variants: vec![SumVariantLayout { name: "First".to_string(), tag: 0, payload_type: None }],
        });

        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR006");
    }

    #[test]
    fn semantic_contract_rejects_duplicate_nominal_sum_variant_tag() {
        let mut module = empty_module();
        module.sum_types.push(SumTypeLayout {
            name: "Choice".to_string(),
            is_unite: false,
            tag_width: 32,
            variants: vec![
                SumVariantLayout { name: "First".to_string(), tag: 0, payload_type: None },
                SumVariantLayout { name: "Second".to_string(), tag: 0, payload_type: None },
            ],
        });

        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR006");
    }

    fn module_with_static_call(symbol: NamePath) -> MirModule {
        let mut module = empty_module();
        let mut function = crate::mir::ssa::test_support::lower_test_function(crate::mir::ssa::test_support::expr(HirExprKind::Literal(
            crate::types::hir::HirLiteral::Bool(true),
        )));
        function.blocks[0]
            .instructions
            .push(MirInstruction::from_operation(MirOperation::Call { callee: MirOperand::Symbol(symbol), arguments: Vec::new() }));
        module.functions.push(function);
        module
    }

    #[test]
    fn semantic_contract_rejects_unresolved_static_call() {
        let module = module_with_static_call(NamePath::new(vec![Identifier::new("dependency"), Identifier::new("run")]));
        let error = validate_semantic_module(&module).unwrap_err();
        assert_eq!(error.code, "SMIR003");
        assert_eq!(error.location, "block 0 instruction 1");
    }

    #[test]
    fn semantic_contract_accepts_language_operator_call_without_registry_entry() {
        let module = module_with_static_call(NamePath::new(vec![Identifier::new("primitive"), Identifier::new("infix +")]));
        validate_semantic_module(&module).unwrap();
    }

    #[test]
    fn semantic_contract_accepts_exact_dependency_export_call() {
        let symbol = NamePath::new(vec![Identifier::new("dependency"), Identifier::new("run")]);
        let mut module = module_with_static_call(symbol.clone());
        module.external_calls.push(MirExternalCallContract { symbol });

        validate_semantic_module(&module).unwrap();
    }
}
