use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mir::ssa::builtin_helpers::resolve_intrinsic_id,
    types::{NamePath, hir::ValkyrieType},
};
use nyar_types::builtin_operator;
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
fn validate_aggregate_field_contracts(module: &MirModule, function: &MirFunction) -> Result<(), SemanticMirContractError> {
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
                    function: function.symbol.clone(),
                    location: location.clone(),
                    detail: detail.to_string(),
                };
                let owner = infer_operand_static_type(function, object).ok_or_else(|| failure("字段对象缺少完整语义类型"))?;
                let base = match &owner {
                    ValkyrieType::Apply(base, _) => base.as_ref(),
                    other => other,
                };
                let ValkyrieType::Named(name) = base
                else {
                    return Err(failure("字段对象不是已解析名义实例"));
                };
                let declaration = module
                    .structs
                    .iter()
                    .find(|declaration| declaration.qualified_name() == name.as_str())
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
                        function: function.symbol.clone(),
                        location,
                        detail: "sum construction references an undeclared sum".to_string(),
                    });
                };
                let Some(declared) = declared_variant(&module.sum_types, *nominal, *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
                        location,
                        detail: "sum construction references an undeclared variant".to_string(),
                    });
                };
                let expected_payload = sum.instantiate_payload(declared, type_args).ok_or_else(|| SemanticMirContractError {
                    code: "SMIR006",
                    function: function.symbol.clone(),
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
                        function: function.symbol.clone(),
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
                        function: function.symbol.clone(),
                        location,
                        detail: "sum payload extraction references undeclared nominal identity".to_string(),
                    });
                };
                let Some(declared) = declared_variant(&module.sum_types, *nominal, *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
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
                        function: function.symbol.clone(),
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
                        function: function.symbol.clone(),
                        location,
                        detail: "SumVariantIs references undeclared nominal identity".to_string(),
                    });
                };
                let Some(declared) = declared_variant(&module.sum_types, *nominal, *variant)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR006",
                        function: function.symbol.clone(),
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
                    function: function.symbol.clone(),
                    location: location.clone(),
                    detail: "sum 判别的类型实参数量与声明不一致".to_string(),
                })?;
                if output_type != Some(&ValkyrieType::Boolean) || receiver_type != Some(&expected_receiver) {
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

fn validate_nominal_structs(module: &MirModule) -> Result<(), SemanticMirContractError> {
    for (index, declaration) in module.structs.iter().enumerate() {
        let duplicate_owner = module.structs[..index].iter().any(|prior| prior.qualified_name() == declaration.qualified_name());
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

fn declared_variant<'a>(
    sum_types: &'a [crate::mir::MirSumDeclaration],
    nominal: nyar_types::NominalInstanceId,
    variant: nyar_types::VariantId,
) -> Option<&'a crate::mir::MirSumVariant> {
    sum_types.iter().find(|sum| sum.nominal == nominal)?.variants.iter().find(|declared| declared.id == variant)
}

fn validate_nominal_sums(module: &MirModule) -> Result<(), SemanticMirContractError> {
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
        MirOperand::Callable(_) => None,
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
            if let MirOperation::Call { callee, arguments } = &instruction.kind {
                validate_static_call_resolution(module, function, &instruction.kind, location.clone())?;
                let target_symbol = match callee {
                    MirOperand::Symbol(symbol) => Some(symbol.to_string()),
                    MirOperand::Callable(identity) => {
                        module.callable_identities.iter().find_map(|(symbol, candidate)| (candidate == identity).then_some(symbol.clone()))
                    }
                    _ => None,
                };
                if let Some(target_symbol) = target_symbol {
                    let candidates: Vec<_> = module.functions.iter().filter(|candidate| candidate.symbol == target_symbol).collect();
                    if candidates.len() > 1 {
                        return Err(error("SMIR003", location.clone(), "local callable identity is ambiguous".to_owned()));
                    }
                    let external: Vec<_> =
                        module.external_calls.iter().filter(|candidate| candidate.symbol.to_string() == target_symbol).collect();
                    if external.len() > 1 {
                        return Err(error("SMIR003", location.clone(), "callable identity has multiple contracts".to_owned()));
                    }
                    if let (Some(definition), Some(declaration)) = (candidates.first(), external.first()) {
                        if definition.param_types != declaration.parameter_types || definition.return_type != declaration.return_type {
                            return Err(error(
                                "SMIR007",
                                location.clone(),
                                "linked definition disagrees with external declaration signature".to_owned(),
                            ));
                        }
                    }
                    let signature = candidates
                        .first()
                        .map(|target| (&target.param_types, &target.return_type))
                        .or_else(|| external.first().map(|target| (&target.parameter_types, &target.return_type)));
                    if let Some((parameter_types, return_type)) = signature {
                        if arguments.len() != parameter_types.len() {
                            return Err(error("SMIR007", location.clone(), "call arguments differ from declared signature arity".to_owned()));
                        }
                        for (argument, expected) in arguments.iter().zip(parameter_types) {
                            let actual = value_type(argument)
                                .ok_or_else(|| error("SMIR001", location.clone(), "call argument has no semantic type".to_owned()))?;
                            if actual != *expected {
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
                            if function.value_types.get(result) != Some(return_type) {
                                return Err(error("SMIR007", location.clone(), "call result type differs from declared signature".to_owned()));
                            }
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
                if actual != function.return_type {
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
                    if actual != *expected {
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

/// Language operators lower as `Call` to display names（迁移期）；后端展开，不进函数注册表。
///
/// 身份由 [`builtin_operator`] 注册表判定，不再维护封闭字符串表。
pub fn is_language_operator_symbol(symbol: &NamePath) -> bool {
    is_language_operator_name(symbol.parts().last().map(|part| part.as_str()).unwrap_or(""))
}

/// 显示名是否为已播种内建运算符（迁移期查找键）。
pub fn is_language_operator_name(name: &str) -> bool {
    builtin_operator::lookup_display_name(name).is_some()
}

/// Language builtins / private intrinsic seeds lower as `Call` but expand in backends。
///
/// 身份由 [`resolve_intrinsic_id`] → [`nyar_types::IntrinsicId`] 判定。
pub fn is_language_builtin_symbol(symbol: &NamePath) -> bool {
    resolve_intrinsic_id(symbol).is_some()
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
    if let MirOperand::Callable(identity) = callee {
        if module.callable_identities.values().any(|candidate| candidate == identity) {
            return Ok(());
        }
        return Err(SemanticMirContractError {
            code: "SMIR003",
            function: function.symbol.clone(),
            location,
            detail: "callable identity is absent from the Compiler identity table".to_string(),
        });
    }
    let MirOperand::Symbol(symbol) = callee
    else {
        return Err(SemanticMirContractError {
            code: "SMIR003",
            function: function.symbol.clone(),
            location,
            detail: "static call requires a frozen callable identity".to_string(),
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
    for diagnostic in &module.diagnostics {
        match diagnostic {
            MirDiagnostic::PatternLoweringFailed { reason, .. } => {
                return Err(ParseError::invalid(format!("MIR lowering 失败：{reason}")));
            }
            MirDiagnostic::UnsupportedExpression { span, kind } => {
                return Err(ParseError::invalid(format!("MIR lowering rejected unsupported HIR expression `{kind}` at source span {span:?}")));
            }
            MirDiagnostic::UnresolvedValueType { context } => {
                return Err(ParseError::invalid(format!("MIR lowering missing value type fact: {context}")));
            }
            MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant } => {
                return Err(ParseError::invalid(format!("MIR lowering unresolved variant identity `{sum_type}::{variant}`")));
            }
            MirDiagnostic::UnresolvedNominalIdentity { type_name } => {
                return Err(ParseError::invalid(format!("MIR lowering unresolved nominal identity `{type_name}`")));
            }
            MirDiagnostic::UnresolvedFieldIdentity { field } => {
                return Err(ParseError::invalid(format!("MIR lowering unresolved field identity `{field}`")));
            }
            MirDiagnostic::UnresolvedCallableIdentity { symbol } => {
                return Err(ParseError::invalid(format!("MIR lowering unresolved callable identity `{symbol}`")));
            }
            MirDiagnostic::UnresolvedOperatorCallable { operator } => {
                return Err(ParseError::invalid(format!("MIR lowering unresolved operator callable `{operator:?}`")));
            }
        }
    }
    validate_semantic_module(module)
        .map_err(|error| ParseError::invalid(format!("{} {} at {}: {}", error.code, error.function, error.location, error.detail)))?;
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
            // suspend_plan / frame_layouts 不再挂在 Semantic MirFunction 上。
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
        MirOperand::Callable(_) => None,
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
    // suspend_points 不再挂在 Semantic MirFunction 上。
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
        ValkyrieType::FixedArray { element, length } => {
            format!("[{}; {}]", display_type(element), length)
        }
        ValkyrieType::TypeLambda(lambda) => format!(
            "type lambda({}) -> {}",
            lambda.params.iter().map(|item| item.name.to_string()).collect::<Vec<_>>().join(", "),
            display_type(&lambda.body)
        ),
        ValkyrieType::TraitObject(object) => {
            format!("{}<{}>", object.trait_path, object.type_arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Associated(associated) => {
            format!("{}::{}", display_type(&associated.base), associated.name)
        }
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
        mir::{MirInstruction, MirOperand, MirOperation, MirSumDeclaration, MirSumVariant, ssa::MirExternalCallContract},
        types::{Identifier, NamePath, hir::HirExprKind},
    };

    fn empty_module() -> MirModule {
        crate::mir::ssa::test_support::lower_test_module(Vec::new(), Vec::new())
    }

    fn source_identity_module(actual: &ValkyrieType, expected: &ValkyrieType) -> MirModule {
        let hir = crate::ValkyrieCompiler::default()
            .compile_source("micro identity(value: bool) -> bool { return value }")
            .expect("当前源码解析后构造类型合同负向输入");
        let mut module = crate::MirLowerer::lower_module_semantic(&hir);
        let function = &mut module.functions[0];
        function.param_types = vec![actual.clone()];
        for ty in function.value_types.values_mut() {
            *ty = actual.clone();
        }
        function.return_type = expected.clone();
        module
    }

    fn mismatched_nominal_contracts() -> Vec<(ValkyrieType, ValkyrieType)> {
        let applied = |name, arguments| ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new(name))), arguments);
        vec![
            (applied("FirstResult", vec![ValkyrieType::Boolean]), applied("SecondResult", vec![ValkyrieType::Boolean])),
            (
                applied("Result", vec![ValkyrieType::Boolean, ValkyrieType::Utf8]),
                applied("Result", vec![ValkyrieType::Boolean, ValkyrieType::Utf16]),
            ),
            (applied("Option", vec![ValkyrieType::Boolean]), applied("Nullable", vec![ValkyrieType::Boolean])),
            (ValkyrieType::Integer32 { signed: true }, ValkyrieType::Named(Identifier::new("usize"))),
            (ValkyrieType::Utf16, ValkyrieType::Named(Identifier::new("Utf16Text"))),
        ]
    }

    #[test]
    fn semantic_return_requires_exact_resolved_type_without_name_or_carrier_recovery() {
        validate_semantic_module(&source_identity_module(&ValkyrieType::Boolean, &ValkyrieType::Boolean))
            .expect("当前源码的完整 bool 返回合同");
        for (actual, expected) in mismatched_nominal_contracts() {
            let error = validate_semantic_module(&source_identity_module(&actual, &expected))
                .expect_err("同名模式、首个 payload 或物理表示相同都不能替代类型合同");
            assert_eq!(error.code, "SMIR007");
            assert!(error.detail.starts_with("return operand type"));
        }
    }

    #[test]
    fn semantic_jump_requires_exact_resolved_block_parameter_type() {
        for (actual, expected) in mismatched_nominal_contracts() {
            let mut module = source_identity_module(&actual, &actual);
            let function = &mut module.functions[0];
            let source = function.blocks[0].parameters[0];
            let parameter = MirValueRef(function.value_types.keys().map(|value| value.0).max().unwrap() + 1);
            function.return_type = ValkyrieType::Unit;
            function.value_types.insert(parameter, expected);
            function.blocks[0].terminator = MirTerminator::Jump { target: MirBlockRef(1), arguments: vec![MirOperand::Value(source)] };
            function.blocks.push(MirBlock {
                id: MirBlockRef(1),
                label: "destination".into(),
                parameters: vec![parameter],
                instructions: Vec::new(),
                terminator: MirTerminator::Return { value: None },
            });
            let error = validate_semantic_module(&module).expect_err("块参数不能按名称或物理类型恢复");
            assert_eq!(error.code, "SMIR007");
            assert!(error.detail.starts_with("jump argument type"));
        }
    }

    #[test]
    fn semantic_contract_rejects_nominal_sum_without_variants() {
        let mut module = empty_module();
        module.sum_types.push(MirSumDeclaration {
            nominal: nyar_types::NominalInstanceId::from_index(0).expect("测试 nominal identity"),
            declaration: None,
            name: "Choice".to_string(),
            is_unite: true,
            generics: Vec::new(),
            variants: Vec::new(),
        });

        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR006");
    }

    #[test]
    fn semantic_contract_rejects_duplicate_nominal_sum_variant_tag() {
        let mut module = empty_module();
        module.sum_types.push(MirSumDeclaration {
            nominal: nyar_types::NominalInstanceId::from_index(0).expect("测试 nominal identity"),
            declaration: None,
            name: "Choice".to_string(),
            is_unite: false,
            generics: Vec::new(),
            variants: vec![
                MirSumVariant {
                    id: nyar_types::VariantId::from_index(0).expect("测试 variant identity"),
                    declaration: None,
                    name: "First".to_string(),
                    tag: 0,
                    fields: Vec::new(),
                    result_type: None,
                },
                MirSumVariant {
                    id: nyar_types::VariantId::from_index(1).expect("测试 variant identity"),
                    declaration: None,
                    name: "Second".to_string(),
                    tag: 0,
                    fields: Vec::new(),
                    result_type: None,
                },
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
    fn semantic_contract_checks_local_call_arguments_and_results() {
        let mut module = module_with_static_call(NamePath::new(vec![Identifier::new("target")]));
        let mut target = module.functions[0].clone();
        target.symbol = "target".to_owned();
        target.return_type = ValkyrieType::Unit;
        target.blocks[0].instructions.clear();
        target.blocks[0].terminator = MirTerminator::Return { value: None };
        module.functions.push(target);
        validate_semantic_module(&module).expect("exact zero-argument unit call");
        module.functions[1].param_types.push(ValkyrieType::Boolean);
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR007");
        if let MirOperation::Call { arguments, .. } = &mut module.functions[0].blocks[0].instructions[1].kind {
            arguments.push(MirOperand::Constant(MirConstant::Unit));
        }
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR007");
        if let MirOperation::Call { arguments, .. } = &mut module.functions[0].blocks[0].instructions[1].kind {
            arguments[0] = MirOperand::Constant(MirConstant::Bool(true));
        }
        validate_semantic_module(&module).expect("exact boolean argument");
        module.functions[1].return_type = ValkyrieType::Boolean;
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR007");
    }

    #[test]
    fn semantic_contract_rejects_duplicate_local_callable_identity() {
        let mut module = module_with_static_call(NamePath::new(vec![Identifier::new("target")]));
        let mut target = module.functions[0].clone();
        target.symbol = "target".to_owned();
        module.functions.push(target.clone());
        module.functions.push(target);
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR003");
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
        module.external_calls.push(MirExternalCallContract {
            declaration: None,
            instance: None,
            symbol,
            link: nyar_types::ExternalImportLink::host(None, vec!["dependency".to_owned(), "run".to_owned()]),
            parameter_types: Vec::new(),
            return_type: ValkyrieType::Unit,
        });

        validate_semantic_module(&module).unwrap();
        module.external_calls[0].parameter_types.push(ValkyrieType::Boolean);
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR007");
        if let MirOperation::Call { arguments, .. } = &mut module.functions[0].blocks[0].instructions[1].kind {
            arguments.push(MirOperand::Constant(MirConstant::Bool(true)));
        }
        validate_semantic_module(&module).expect("exact external argument contract");
        let mut definition = module.functions[0].clone();
        definition.symbol = module.external_calls[0].symbol.to_string();
        definition.param_types = vec![ValkyrieType::Boolean];
        definition.return_type = ValkyrieType::Unit;
        definition.blocks[0].instructions.clear();
        definition.blocks[0].terminator = MirTerminator::Return { value: None };
        module.functions.push(definition);
        validate_semantic_module(&module).expect("matching linked declaration and definition");
        module.functions[1].param_types.clear();
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR007");
        module.functions.pop();
        let dependency = module.clone();
        let before = module.clone();
        assert!(crate::valkyrie::compile_pipeline::link_reachable_dependency_mir(&mut module, &[dependency.clone(), dependency]).is_err());
        assert_eq!(module, before);
        module.external_calls[0].return_type = ValkyrieType::Boolean;
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR007");
        module.external_calls[0].return_type = ValkyrieType::Unit;
        module.external_calls.push(module.external_calls[0].clone());
        assert_eq!(validate_semantic_module(&module).unwrap_err().code, "SMIR003");
    }
}
