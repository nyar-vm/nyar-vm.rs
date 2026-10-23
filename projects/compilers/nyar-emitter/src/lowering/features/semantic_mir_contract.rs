//! Semantic MIR boundary contracts shared by all backend lowerers.
//!
//! This module deliberately validates only semantic metadata completeness. It
//! does not choose JVM, WASM, CLR, or WASI representations; those decisions
//! belong to backend-local preparation after this gate succeeds.

use crate::{
    FragmentSubmission,
    backend_plan_views::{ExecutableFunction, ExecutableInstructionKind, ExecutableOperand},
};
use nyar::QualifiedName;
use nyar_types::{Constant, NyarType, ValueOrigin};

/// Semantic MIR 合同拒绝的错误定位（集成测试专用）。
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMirContractError {
    pub code: &'static str,
    pub function: String,
    pub location: String,
    pub detail: String,
}

/// Stable, backend-independent observation format used by the paired
/// Rust/Valkyrie conformance runner.
#[doc(hidden)]
pub fn observation(case_id: &str, result: Result<(), &SemanticMirContractError>) -> String {
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

#[doc(hidden)]
pub fn validate_submission(submission: &FragmentSubmission) -> Result<(), SemanticMirContractError> {
    validate_nominal_sums(submission)?;
    validate_aggregate_layouts(submission)?;

    let executable = &submission.backend_plan;

    for operation in executable.instances() {
        let Some(view) = executable.get_function(&operation)
        else {
            return Err(SemanticMirContractError {
                code: "SMIR003",
                function: operation.to_string(),
                location: "operation".to_string(),
                detail: "executable operation has no function payload".to_string(),
            });
        };
        validate_static_call_resolution(submission, &view.function)?;
        validate_function(&view.function)?;
        validate_aggregate_field_contracts(submission, &view.function)?;
    }
    Ok(())
}

fn validate_aggregate_layouts(submission: &FragmentSubmission) -> Result<(), SemanticMirContractError> {
    for layout in &submission.backend_plan.aggregate_layouts().layouts {
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
    Ok(())
}

/// Aggregate field instructions are Semantic MIR, not a request for a backend
/// to discover an owner from a field spelling or physical object carrier.
fn validate_aggregate_field_contracts(submission: &FragmentSubmission, function: &ExecutableFunction) -> Result<(), SemanticMirContractError> {
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let ExecutableInstructionKind::StructNew { nominal, fields } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let Some(layout_id) = submission.backend_plan.aggregate_layout_by_nominal().get(nominal).copied()
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "aggregate construction references an unknown nominal identity".to_string(),
                    });
                };
                let Some(layout) = submission.backend_plan.aggregate_layouts().layouts.iter().find(|layout| layout.id == layout_id)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "aggregate construction references an unknown layout identity".to_string(),
                    });
                };
                // 实例字段类型已由上游冻结；backend 不再推导 substitution 或平台别名。
                let fields_match = fields.len() == layout.fields.len()
                    && fields.iter().all(|(field_id, value)| {
                        submission.backend_plan.aggregate_layout_by_field().get(field_id).is_some_and(|(owner_layout, slot)| {
                            *owner_layout == layout_id
                                && layout.fields.get(*slot as usize).is_some_and(|field| {
                                    let declared = &field.ty;
                                    matches!(
                                        value,
                                        ExecutableOperand::Value(value)
                                            if function
                                                .value_types
                                                .get(value)
                                                .is_some_and(|actual| actual == declared)
                                    )
                                })
                        })
                    });
                if !fields_match {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "aggregate construction contract disagrees with declared layout metadata".to_string(),
                    });
                }
                continue;
            }
            if let ExecutableInstructionKind::SumNew { nominal, variant, .. } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                if !submission.backend_plan.sum_representations().get(nominal).is_some_and(|sum| sum.variants.contains_key(variant)) {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum construction references an unknown nominal variant identity".to_string(),
                    });
                }
                continue;
            }
            if let ExecutableInstructionKind::SumPayloadGet { nominal, variant, object } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                if !submission.backend_plan.sum_representations().get(nominal).is_some_and(|sum| sum.variants.contains_key(variant)) {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum payload extraction references an unknown nominal variant identity".to_string(),
                    });
                }
                continue;
            }
            let (object, field, value) = match &instruction.kind {
                ExecutableInstructionKind::FieldGet { object, field, .. } => (object, field, None),
                ExecutableInstructionKind::FieldSet { object, field, value, .. } => (object, field, Some(value)),
                _ => continue,
            };
            let location = format!("block {} instruction {index}", block.id.0);
            let Some((layout_id, slot)) = submission.backend_plan.aggregate_layout_by_field().get(field).copied()
            else {
                return Err(SemanticMirContractError {
                    code: "SMIR010",
                    function: function.symbol.clone(),
                    location,
                    detail: "aggregate field access references an unknown field identity".to_string(),
                });
            };
            let Some(layout) = submission.backend_plan.aggregate_layouts().layouts.iter().find(|layout| layout.id == layout_id)
            else {
                return Err(SemanticMirContractError {
                    code: "SMIR010",
                    function: function.symbol.clone(),
                    location,
                    detail: "aggregate field access references an unknown layout identity".to_string(),
                });
            };
            let Some(declared) = layout.fields.get(slot as usize)
            else {
                return Err(SemanticMirContractError {
                    code: "SMIR010",
                    function: function.symbol.clone(),
                    location,
                    detail: "aggregate field identity is outside its layout".to_string(),
                });
            };
            if let Some(output) = crate::contracts::instruction_primary_result(instruction) {
                if function.value_types.get(&output).is_none_or(|actual| actual != &declared.ty) {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "FieldGet result type differs from declared aggregate field type".to_string(),
                    });
                }
            }
            if let Some(ExecutableOperand::Value(value)) = value {
                if function.value_types.get(value).is_none_or(|actual| actual != &declared.ty) {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "FieldSet value type differs from declared aggregate field type".to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_static_call_resolution(submission: &FragmentSubmission, function: &ExecutableFunction) -> Result<(), SemanticMirContractError> {
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let ExecutableInstructionKind::Call { callee, .. } = &instruction.kind
            else {
                continue;
            };
            let location = format!("block {} instruction {index}", block.id.0);
            match callee {
                ExecutableOperand::Item(instance) => {
                    let local = submission.backend_plan.get_function(instance).is_some();
                    let external = submission.backend_plan.imports().contains_key(instance);
                    if !local && !external {
                        return Err(SemanticMirContractError {
                            code: "SMIR003",
                            function: function.symbol.clone(),
                            location,
                            detail: format!("call identity `{instance:?}` is absent from the exact function and import registries"),
                        });
                    }
                }
                ExecutableOperand::Symbol(path) => {
                    return Err(SemanticMirContractError {
                        code: "SMIR003",
                        function: function.symbol.clone(),
                        location,
                        detail: format!("ordinary callable `{path}` must carry a resolved item identity"),
                    });
                }
                ExecutableOperand::Value(_) | ExecutableOperand::Constant(_) => {
                    return Err(SemanticMirContractError {
                        code: "SMIR003",
                        function: function.symbol.clone(),
                        location,
                        detail: "call callee must be a resolved item identity or function value".to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_nominal_sums(submission: &FragmentSubmission) -> Result<(), SemanticMirContractError> {
    for (nominal, sum) in submission.backend_plan.sum_representations() {
        if *nominal != sum.nominal || sum.variants.is_empty() || sum.tag_width == 0 {
            return Err(SemanticMirContractError {
                code: "SMIR006",
                function: format!("{nominal:?}"),
                location: "sum layout".to_string(),
                detail: "nominal sum layout requires identity, tag width, and at least one variant".to_string(),
            });
        }
        let mut tags = std::collections::BTreeSet::new();
        for variant in sum.variants.values() {
            if !tags.insert(variant.tag) {
                return Err(SemanticMirContractError {
                    code: "SMIR006",
                    function: format!("{nominal:?}"),
                    location: "sum variant".to_string(),
                    detail: "nominal sum variant tag is duplicated".to_string(),
                });
            }
        }
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_function(function: &ExecutableFunction) -> Result<(), SemanticMirContractError> {
    if !function.blocks.iter().any(|block| block.id == function.entry) {
        return Err(SemanticMirContractError {
            code: "SMIR009",
            function: function.symbol.clone(),
            location: "entry".to_string(),
            detail: format!("entry block {:?} is not present in the function", function.entry),
        });
    }
    let missing = |value, location: String| SemanticMirContractError {
        code: "SMIR001",
        function: function.symbol.clone(),
        location: location.clone(),
        detail: format!("semantic type missing for value {value:?} at {location}"),
    };

    for block in &function.blocks {
        for (index, parameter) in block.parameters.iter().enumerate() {
            if !function.value_types.contains_key(parameter) {
                return Err(missing(parameter, format!("block {} parameter {index}", block.id.0)));
            }
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            // A residual high-level pattern is the primary contract failure.
            // Report SMIR008 before secondary metadata checks so malformed
            // residual MIR cannot be misreported as a missing value type.
            if let ExecutableInstructionKind::PatternMatch { .. } = instruction.kind {
                return Err(SemanticMirContractError {
                    code: "SMIR008",
                    function: function.symbol.clone(),
                    location: format!("block {} instruction {index}", block.id.0),
                    detail: format!("residual PatternMatch instruction at block {} instruction {index}", block.id.0),
                });
            }
            if let Some(output) = crate::contracts::instruction_primary_result(instruction) {
                if !function.value_types.contains_key(&output) {
                    return Err(SemanticMirContractError {
                        code: "SMIR001",
                        function: function.symbol.clone(),
                        location: format!("block {} instruction {index}", block.id.0),
                        detail: format!(
                            "semantic type missing for value {output:?} at block {} instruction {index}; kind={:?}",
                            block.id.0, instruction.kind
                        ),
                    });
                }
            }
            if let ExecutableInstructionKind::LoadConstant { constant, ty } = &instruction.kind {
                if let Some(literal_type) = text_constant_type(constant) {
                    let location = format!("block {} instruction {index}", block.id.0);
                    if ty.as_ref() != Some(&literal_type) {
                        return Err(SemanticMirContractError {
                            code: "SMIR007",
                            function: function.symbol.clone(),
                            location,
                            detail: "text constant encoding/type contract is absent or disagrees with the literal".to_string(),
                        });
                    }
                    if let Some(output) = crate::contracts::instruction_primary_result(instruction) {
                        if function.value_types.get(&output) != Some(&literal_type) {
                            return Err(SemanticMirContractError {
                                code: "SMIR007",
                                function: function.symbol.clone(),
                                location: format!("block {} instruction {index}", block.id.0),
                                detail: "text constant result SSA type disagrees with the literal encoding".to_string(),
                            });
                        }
                    }
                }
            }
            for (operand_index, operand) in instruction_operands(&instruction.kind).into_iter().enumerate() {
                if let ExecutableOperand::Value(value) = operand {
                    if !function.value_types.contains_key(value) {
                        return Err(missing(value, format!("block {} instruction {index} operand {operand_index}", block.id.0)));
                    }
                }
            }
        }
        validate_terminator(function, block)?;
    }
    Ok(())
}

fn text_constant_type(constant: &Constant) -> Option<NyarType> {
    match constant {
        Constant::Utf8(_) => Some(NyarType::Utf8),
        Constant::Utf16(_) => Some(NyarType::Utf16),
        _ => None,
    }
}

fn instruction_operands(kind: &ExecutableInstructionKind) -> Vec<&ExecutableOperand> {
    use crate::contracts::InstructionKind::*;
    match kind {
        LoadConstant { .. } | LoadSymbol { .. } => Vec::new(),
        Copy { source } => vec![source],
        StoreVar { value, .. } => vec![value],
        Call { callee, arguments } => {
            let mut values = vec![callee];
            values.extend(arguments);
            values
        }
        StructNew { fields, .. } => fields.iter().map(|(_, value)| value).collect(),
        TupleNew { fields, .. } => fields.iter().collect(),
        AggregateCopy { source, dest, .. } => vec![source, dest],
        FieldGet { object, .. } => vec![object],
        FieldSet { object, value, .. } => vec![object, value],
        SumNew { payload, .. } => payload.iter().collect(),
        SumPayloadGet { object, .. } | SumVariantIs { object, .. } => vec![object],
        PatternMatch { value, .. } => vec![value],
        ArrayNew { length, initialization, .. } => {
            let mut values = vec![length];
            if let crate::contracts::ArrayInitialization::Fill(fill) = initialization {
                values.push(fill);
            }
            values
        }
        ArrayFromElements { elements, .. } => elements.iter().collect(),
        ArrayGet { array, index } => vec![array, index],
        ArraySet { array, index, value } => vec![array, index, value],
        ArrayLength { array } => vec![array],
        ArrayPush { array, value } => vec![array, value],
        NullIsNull { value } => vec![value],
        NullUnwrap { value } => vec![value],
    }
}

fn terminator_operand_type(function: &ExecutableFunction, operand: &ExecutableOperand) -> Option<NyarType> {
    match operand {
        ExecutableOperand::Value(value) => function.value_types.get(value).cloned(),
        ExecutableOperand::Constant(Constant::Bool(_)) => Some(NyarType::Boolean),
        ExecutableOperand::Constant(Constant::Int(_)) => Some(NyarType::Integer64 { signed: true }),
        ExecutableOperand::Constant(Constant::Float64(_)) => Some(NyarType::Float64),
        ExecutableOperand::Constant(Constant::Utf8(_)) => Some(NyarType::Utf8),
        ExecutableOperand::Constant(Constant::Utf16(_)) => Some(NyarType::Utf16),
        ExecutableOperand::Constant(Constant::Unit) => Some(NyarType::Unit),
        ExecutableOperand::Symbol(_) => None,
        ExecutableOperand::Item(_) => None,
    }
}

fn validate_terminator(function: &ExecutableFunction, block: &crate::contracts::Block) -> Result<(), SemanticMirContractError> {
    let location = format!("block {} terminator", block.id.0);
    let value_type = |operand: &ExecutableOperand| terminator_operand_type(function, operand);
    let operands: Vec<&ExecutableOperand> = match &block.terminator {
        crate::contracts::Terminator::Return { value: Some(value) } => vec![value],
        crate::contracts::Terminator::Jump { arguments, .. } => arguments.iter().collect(),
        crate::contracts::Terminator::Branch { condition, .. } => vec![condition],
        crate::contracts::Terminator::PerformEffect { payload: Some(payload), .. } => vec![payload],
        crate::contracts::Terminator::YieldToRuntime { payload: Some(payload), .. } => vec![payload],
        crate::contracts::Terminator::StateDispatch { .. } => Vec::new(),
        _ => Vec::new(),
    };
    for operand in operands {
        if let ExecutableOperand::Value(value) = operand {
            if !function.value_types.contains_key(value) {
                return Err(SemanticMirContractError {
                    code: "SMIR001",
                    function: function.symbol.clone(),
                    location: location.clone(),
                    detail: format!("semantic type missing for terminator value {value:?} in block {}", block.id.0),
                });
            }
        }
    }
    if let crate::contracts::Terminator::StateDispatch { state, .. } = &block.terminator {
        if !function.value_types.contains_key(state) {
            return Err(SemanticMirContractError {
                code: "SMIR001",
                function: function.symbol.clone(),
                location: format!("block {} terminator state", block.id.0),
                detail: format!("semantic type missing for terminator state {state:?} in block {}", block.id.0),
            });
        }
    }
    match &block.terminator {
        crate::contracts::Terminator::Return { value: Some(value) } => {
            let Some(actual) = value_type(value)
            else {
                return Err(SemanticMirContractError {
                    code: "SMIR001",
                    function: function.symbol.clone(),
                    location,
                    detail: "return operand has no SSA type".to_string(),
                });
            };
            if actual != function.return_type {
                return Err(SemanticMirContractError {
                    code: "SMIR007",
                    function: function.symbol.clone(),
                    location,
                    detail: format!(
                        "return operand type differs from function return type (actual={actual:?}, expected={:?})",
                        function.return_type
                    ),
                });
            }
        }
        crate::contracts::Terminator::Jump { target, arguments } => {
            let Some(destination) = function.blocks.iter().find(|candidate| candidate.id == *target)
            else {
                return Err(SemanticMirContractError {
                    code: "SMIR007",
                    function: function.symbol.clone(),
                    location,
                    detail: "jump target is absent".to_string(),
                });
            };
            if destination.parameters.len() != arguments.len() {
                return Err(SemanticMirContractError {
                    code: "SMIR007",
                    function: function.symbol.clone(),
                    location,
                    detail: "jump arity differs from target block parameters".to_string(),
                });
            }
            for (argument, parameter) in arguments.iter().zip(&destination.parameters) {
                let Some(actual) = value_type(argument)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR001",
                        function: function.symbol.clone(),
                        location,
                        detail: "jump argument has no SSA type".to_string(),
                    });
                };
                let Some(expected) = function.value_types.get(parameter)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR001",
                        function: function.symbol.clone(),
                        location,
                        detail: "jump target block parameter has no SSA type".to_string(),
                    });
                };
                if &actual != expected {
                    return Err(SemanticMirContractError {
                        code: "SMIR007",
                        function: function.symbol.clone(),
                        location,
                        detail: format!("jump argument type differs from target block parameter (actual={actual:?}, expected={expected:?})"),
                    });
                }
            }
        }
        crate::contracts::Terminator::Return { value: None } if !matches!(function.return_type, NyarType::Unit | NyarType::Bottom) => {
            return Err(SemanticMirContractError {
                code: "SMIR007",
                function: function.symbol.clone(),
                location,
                detail: "non-unit return requires a typed SSA result".to_string(),
            });
        }
        crate::contracts::Terminator::Branch { condition, then_target, else_target } => {
            if value_type(condition).as_ref() != Some(&NyarType::Boolean) {
                return Err(SemanticMirContractError {
                    code: "SMIR007",
                    function: function.symbol.clone(),
                    location,
                    detail: "branch condition must be bool".to_string(),
                });
            }
            for target in [then_target, else_target] {
                if !function.blocks.iter().any(|block| block.id == *target && block.parameters.is_empty()) {
                    return Err(SemanticMirContractError {
                        code: "SMIR007",
                        function: function.symbol.clone(),
                        location,
                        detail: "branch requires an existing target without block parameters".to_string(),
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}
