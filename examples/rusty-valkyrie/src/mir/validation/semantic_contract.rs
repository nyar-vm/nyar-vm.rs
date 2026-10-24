//! Semantic MIR 合同校验：stable-ID、聚合字段、名义结构与静态调用。

use super::type_inference::{
    bare_named_matches_generic_apply_return, call_site_type_compatible, infer_operand_static_type, mir_return_operand_type_compatible,
};
use crate::{
    mir::{MirConstant, MirFunction, MirModule, MirOperand, MirOperation, MirTerminator},
    types::hir::ValkyrieType,
};

/// Backend-independent Semantic MIR contract observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMirContractError {
    /// 合同违反码。
    pub code: &'static str,
    /// 所在函数符号。
    pub function: String,
    /// 违反位置描述。
    pub location: String,
    /// 补充说明。
    pub detail: String,
}

/// 将合同校验结果编码为观测字符串。
pub fn semantic_observation(case_id: &str, result: Result<(), &SemanticMirContractError>) -> String {
    match result {
        Ok(()) => format!("{case_id}|accept||"),
        Err(error) => format!("{case_id}|reject|{}|{}", error.code, observation_site(&error.location)),
    }
}

pub(super) fn observation_site(location: &str) -> &'static str {
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

/// 校验 Semantic MIR 模块是否满足后端无关合同。
pub fn validate_semantic_module(module: &MirModule) -> Result<(), SemanticMirContractError> {
    validate_nominal_sums(module)?;
    validate_nominal_structs(module)?;
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
pub(super) fn validate_aggregate_field_contracts(module: &MirModule, function: &MirFunction) -> Result<(), SemanticMirContractError> {
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let field_operation = match &instruction.kind {
                MirOperation::FieldGet { object, field } => Some((object, field, None)),
                MirOperation::FieldSet { object, field, value } => Some((object, field, Some(value))),
                _ => None,
            };
            if let Some((object, field, stored)) = field_operation {
                let location = format!("block {} instruction {index}", block.id.0);
                let failure = |detail: &str| SemanticMirContractError {
                    code: "SMIR006",
                    function: function.display_symbol.clone(),
                    location: location.clone(),
                    detail: detail.to_string(),
                };
                let owner = infer_operand_static_type(function, object).ok_or_else(|| failure("字段对象缺少完整语义类型"))?;
                let declaration_id = owner.nominal_declaration().ok_or_else(|| failure("字段对象不是已绑定 declaration 的名义实例"))?;
                let declaration = crate::mir::ssa::MirStruct::find_by_declaration(&module.structs, declaration_id)
                    .ok_or_else(|| failure("字段对象没有声明合同"))?;
                let declared_field = declaration
                    .fields
                    .iter()
                    .find(|candidate| candidate.id == *field)
                    .ok_or_else(|| failure("字段 identity 不属于对象声明"))?;
                let expected = declaration
                    .instantiate_field(&owner, declared_field.name.as_str())
                    .ok_or_else(|| failure("字段身份或完整类型代入与声明不一致"))?;
                if let Some(stored) = stored {
                    if !instruction.results.is_empty() || infer_operand_static_type(function, stored).as_ref() != Some(&expected) {
                        return Err(failure("字段写入值或结果数量与声明不一致"));
                    }
                }
                else if instruction.results.len() != 1 || function.value_types.get(&instruction.results[0]) != Some(&expected) {
                    return Err(failure("字段读取结果与完整实例声明不一致"));
                }
                continue;
            }
            if let MirOperation::SumNew { nominal, type_args, variant, payload_type, payload } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let Some(sum) = module.sum_types.iter().find(|sum| sum.nominal == *nominal)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "sum construction references an undeclared sum".to_string(),
                    });
                };
                let Some(declared) = declared_variant(&module.sum_types, *nominal, *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "sum construction references an undeclared variant".to_string(),
                    });
                };
                let expected_payload = sum.instantiate_payload(declared, type_args).ok_or_else(|| SemanticMirContractError {
                    code: "SMIR006",
                    function: function.display_symbol.clone(),
                    location: location.clone(),
                    detail: "sum 实例的类型实参数量与声明不一致".to_string(),
                })?;
                let expected_result = sum.instantiate_result(declared, type_args);
                let payload_value_type = match payload {
                    Some(MirOperand::Value(value)) => function.value_types.get(value),
                    Some(_) => None,
                    None => None,
                };
                let output_type = instruction.results.first().and_then(|output| function.value_types.get(output));
                if output_type != expected_result.as_ref()
                    || payload_type != &expected_payload
                    || match (&expected_payload, payload, payload_value_type) {
                        (None, None, _) => false,
                        (Some(expected), Some(MirOperand::Value(_)), Some(actual)) => expected != actual,
                        _ => true,
                    }
                {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "sum construction contract disagrees with declared sum metadata".to_string(),
                    });
                }
                continue;
            }
            if let MirOperation::SumPayloadGet { nominal, type_args, variant, payload_type, object } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let Some(sum) = module.sum_types.iter().find(|sum| sum.nominal == *nominal)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "sum payload extraction references undeclared nominal identity".to_string(),
                    });
                };
                let Some(declared) = declared_variant(&module.sum_types, *nominal, *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "sum has no payload-bearing variant with this identity".to_string(),
                    });
                };
                let receiver_type = match object {
                    MirOperand::Value(value) => function.value_types.get(value),
                    _ => None,
                };
                let output_type = instruction.results.first().and_then(|output| function.value_types.get(output));
                let expected_payload = sum.instantiate_payload(declared, type_args).flatten();
                let expected_receiver = sum.instantiate_result(declared, type_args);
                if expected_payload.as_ref() != Some(payload_type)
                    || output_type != Some(payload_type)
                    || receiver_type != expected_receiver.as_ref()
                {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "sum payload extraction contract disagrees with declared sum metadata".to_string(),
                    });
                }
                continue;
            }
            if let MirOperation::SumVariantIs { nominal, type_args, variant, object } = &instruction.kind {
                let location = format!("block {} instruction {index}", block.id.0);
                let Some(sum) = module.sum_types.iter().find(|sum| sum.nominal == *nominal)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "SumVariantIs references undeclared nominal identity".to_string(),
                    });
                };
                let Some(declared) = declared_variant(&module.sum_types, *nominal, *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
                        location,
                        detail: "SumVariantIs references an unknown variant identity".to_string(),
                    });
                };
                let receiver_type = match object {
                    MirOperand::Value(value) => function.value_types.get(value),
                    _ => None,
                };
                let output_type = instruction.results.first().and_then(|output| function.value_types.get(output));
                let expected_receiver = sum.instantiate_result(declared, type_args).ok_or_else(|| SemanticMirContractError {
                    code: "SMIR006",
                    function: function.display_symbol.clone(),
                    location: location.clone(),
                    detail: "sum 判别的类型实参数量与声明不一致".to_string(),
                })?;
                if output_type != Some(&ValkyrieType::Boolean) || receiver_type != Some(&expected_receiver) {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.display_symbol.clone(),
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

pub(super) fn validate_nominal_structs(module: &MirModule) -> Result<(), SemanticMirContractError> {
    for (index, declaration) in module.structs.iter().enumerate() {
        let duplicate_owner = module.structs[..index]
            .iter()
            .any(|prior| prior.nominal == declaration.nominal || prior.declaration.is_some() && prior.declaration == declaration.declaration);
        let duplicate_binder = declaration
            .generics
            .iter()
            .enumerate()
            .any(|(index, generic)| declaration.generics[..index].iter().any(|prior| prior.name == generic.name));
        let invalid_field = declaration
            .fields
            .iter()
            .enumerate()
            .any(|(index, field)| field.name.is_empty() || declaration.fields[..index].iter().any(|prior| prior.name == field.name));
        if declaration.name.is_empty() || duplicate_owner || duplicate_binder || invalid_field {
            return Err(SemanticMirContractError {
                code: "SMIR006",
                function: declaration.qualified_name(),
                location: "struct declaration".to_string(),
                detail: "结构声明、泛型 binder 与字段身份必须完整且唯一".to_string(),
            });
        }
    }
    Ok(())
}

pub(super) fn declared_variant<'a>(
    sum_types: &'a [crate::mir::MirSumDeclaration],
    nominal: nyar_types::NominalInstanceId,
    variant: nyar_types::VariantId,
) -> Option<&'a crate::mir::MirSumVariant> {
    sum_types.iter().find(|sum| sum.nominal == nominal)?.variants.iter().find(|declared| declared.id == variant)
}

pub(super) fn validate_nominal_sums(module: &MirModule) -> Result<(), SemanticMirContractError> {
    for (sum_index, sum) in module.sum_types.iter().enumerate() {
        if module.sum_types[..sum_index].iter().any(|prior| prior.name == sum.name)
            || sum.generics.iter().enumerate().any(|(index, generic)| sum.generics[..index].iter().any(|prior| prior.name == generic.name))
        {
            return Err(SemanticMirContractError {
                code: "SMIR006",
                function: sum.name.clone(),
                location: "sum declaration".to_string(),
                detail: "sum 声明或泛型 binder 重复".to_string(),
            });
        }
        if sum.name.is_empty() || sum.variants.is_empty() {
            return Err(SemanticMirContractError {
                code: "SMIR006",
                function: sum.name.clone(),
                location: "sum layout".to_string(),
                detail: "sum 声明必须有名称与 variant".to_string(),
            });
        }
        for (index, variant) in sum.variants.iter().enumerate() {
            if variant
                .fields
                .iter()
                .enumerate()
                .any(|(index, field)| field.name.is_empty() || variant.fields[..index].iter().any(|prior| prior.name == field.name))
            {
                return Err(SemanticMirContractError {
                    code: "SMIR006",
                    function: sum.name.clone(),
                    location: "sum variant".to_string(),
                    detail: "variant 字段身份必须非空且唯一".to_string(),
                });
            }
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

pub(super) fn validate_semantic_function(module: &MirModule, function: &MirFunction) -> Result<(), SemanticMirContractError> {
    let error = |code, location: String, detail| SemanticMirContractError { code, function: function.display_symbol.clone(), location, detail };
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
        MirOperand::Callable(_) => None,
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
            if let MirOperation::Call { callee, arguments } = &instruction.kind {
                validate_static_call_resolution(module, function, &instruction.kind, location.clone())?;
                // Callable 必须按 ItemInstanceId 取签名；禁止再按短名聚合多个实例后判歧义。
                let signature = match callee {
                    MirOperand::Callable(identity) => module
                        .functions
                        .iter()
                        .find(|candidate| candidate.instance == Some(*identity))
                        .map(|target| (&target.param_types, &target.return_type))
                        .or_else(|| {
                            module
                                .external_calls
                                .iter()
                                .find(|candidate| candidate.instance == Some(*identity))
                                .map(|target| (&target.parameter_types, &target.return_type))
                        }),
                    // `Symbol` 只保留给内建 intrinsic；不得按名称反查 external callable 签名。
                    _ => None,
                };
                if let Some((parameter_types, return_type)) = signature {
                    if arguments.len() != parameter_types.len() {
                        return Err(error("SMIR007", location.clone(), "call arguments differ from declared signature arity".to_owned()));
                    }
                    for (argument, expected) in arguments.iter().zip(parameter_types) {
                        let actual = value_type(argument)
                            .ok_or_else(|| error("SMIR001", location.clone(), "call argument has no semantic type".to_owned()))?;
                        if !call_site_type_compatible(expected, &actual) {
                            return Err(error(
                                "SMIR007",
                                location.clone(),
                                format!("call argument type differs from declared signature (actual={actual:?}, expected={expected:?})"),
                            ));
                        }
                    }
                    let expected_results = usize::from(*return_type != ValkyrieType::Unit);
                    if instruction.results.len() != expected_results {
                        return Err(error("SMIR007", location.clone(), "call result count differs from declared signature".to_owned()));
                    }
                    if let Some(result) = instruction.results.first() {
                        let actual = function.value_types.get(result);
                        if !actual.is_some_and(|actual| call_site_type_compatible(return_type, actual)) {
                            return Err(error("SMIR007", location.clone(), "call result type differs from declared signature".to_owned()));
                        }
                    }
                }
            }
        }
        let location = format!("block {} terminator", block.id.0);
        match &block.terminator {
            MirTerminator::Return { value: Some(value) } => {
                let Some(actual) = value_type(value)
                else {
                    return Err(error("SMIR001", location, "return operand has no SSA type".to_string()));
                };
                if !mir_return_operand_type_compatible(&actual, &function.return_type) {
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
                    let Some(actual) = value_type(argument)
                    else {
                        return Err(error("SMIR001", location.clone(), "jump argument has no SSA type".to_string()));
                    };
                    let Some(expected) = function.value_types.get(parameter)
                    else {
                        return Err(error("SMIR001", location.clone(), "jump target block parameter has no SSA type".to_string()));
                    };
                    if actual != *expected && !bare_named_matches_generic_apply_return(expected, &actual) {
                        return Err(error(
                            "SMIR007",
                            location,
                            format!("jump argument type differs from target block parameter (actual={actual:?}, expected={expected:?})"),
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
pub(super) fn validate_static_call_resolution(
    module: &MirModule,
    function: &MirFunction,
    kind: &MirOperation,
    location: String,
) -> Result<(), SemanticMirContractError> {
    let MirOperation::Call { callee, .. } = kind
    else {
        return Ok(());
    };
    if let MirOperand::Callable(identity) = callee {
        if module.contains_callable_instance(*identity) {
            return Ok(());
        }
        return Err(SemanticMirContractError {
            code: "SMIR003",
            function: function.display_symbol.clone(),
            location,
            detail: "callable identity is absent from the Compiler identity table".to_string(),
        });
    }
    if let MirOperand::Value(value) = callee {
        if function.value_types.get(value).is_some_and(|ty| matches!(ty, ValkyrieType::Function(_))) {
            return Ok(());
        }
        return Err(SemanticMirContractError {
            code: "SMIR003",
            function: function.display_symbol.clone(),
            location,
            detail: "indirect call requires a function-typed callee SSA value".to_string(),
        });
    }
    Err(SemanticMirContractError {
        code: "SMIR003",
        function: function.display_symbol.clone(),
        location,
        detail: "static call requires Callable(ItemInstanceId) or function-typed Value callee".to_string(),
    })
}

pub(super) fn text_constant_type(constant: &MirConstant) -> Option<ValkyrieType> {
    match constant {
        MirConstant::Utf8(_) => Some(ValkyrieType::Utf8),
        MirConstant::Utf16(_) => Some(ValkyrieType::Utf16),
        _ => None,
    }
}
