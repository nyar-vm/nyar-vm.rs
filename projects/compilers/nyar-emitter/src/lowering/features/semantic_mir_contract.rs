//! Semantic MIR boundary contracts shared by all backend lowerers.
//!
//! This module deliberately validates only semantic metadata completeness. It
//! does not choose JVM, WASM, CLR, or WASI representations; those decisions
//! belong to backend-local preparation after this gate succeeds.

use std::collections::BTreeMap;

use crate::{
    FragmentSubmission,
    backend_plan_views::{
        ExecutableFunction, ExecutableInstructionKind, ExecutableOperand,
    },
};
use nyar::QualifiedName;
use nyar_types::{AggregateLayout, Constant, NyarFunctionType, NyarType, ValueOrigin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticMirContractError {
    pub code: &'static str,
    pub function: String,
    pub location: String,
    pub detail: String,
}

/// Stable, backend-independent observation format used by the paired
/// Rust/Valkyrie conformance runner.
pub(crate) fn observation(case_id: &str, result: Result<(), &SemanticMirContractError>) -> String {
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

pub(crate) fn validate_submission(submission: &FragmentSubmission) -> Result<(), SemanticMirContractError> {
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
    for layout in &submission.aggregate_layouts.layouts {
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
                let Some(layout_id) = submission.aggregate_layout_by_nominal.get(nominal).copied()
                else {
                    return Err(SemanticMirContractError { code: "SMIR010", function: function.symbol.clone(), location, detail: "aggregate construction references an unknown nominal identity".to_string() });
                };
                let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|layout| layout.id == layout_id)
                else {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "aggregate construction references an unknown layout identity".to_string(),
                    });
                };
                let output_type = crate::contracts::instruction_primary_result(instruction)
                    .and_then(|output| function.value_types.get(&output))
                    .or(Some(&function.return_type));
                // 字段在「输出类型给出的同一 substitution」下比较，
                // 禁止依赖 Named("Self") / 类型名单字母特判放行。
                let fields_match = fields.len() == layout.fields.len()
                    && fields.iter().all(|(field_id, value)| {
                        submission.aggregate_layout_by_field.get(field_id).is_some_and(|(owner_layout, slot)| {
                            *owner_layout == layout_id && layout.fields.get(*slot as usize).is_some_and(|field| {
                            let declared = &field.ty;
                            matches!(
                                value,
                                ExecutableOperand::Value(value)
                                    if function
                                        .value_types
                                        .get(value)
                                        .is_some_and(|actual| aggregate_field_types_compatible(actual, &declared))
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
                if !submission.sum_variant_ids.contains(&(*nominal, *variant)) {
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
                if !submission.sum_variant_ids.contains(&(*nominal, *variant)) {
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
            let Some((layout_id, slot)) = submission.aggregate_layout_by_field.get(field).copied()
            else {
                return Err(SemanticMirContractError {
                    code: "SMIR010",
                    function: function.symbol.clone(),
                    location,
                    detail: "aggregate field access references an unknown field identity".to_string(),
                });
            };
            let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|layout| layout.id == layout_id)
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
                if function
                    .value_types
                    .get(&output)
                    .is_none_or(|actual| !aggregate_field_types_compatible(actual, &declared.ty))
                {
                    return Err(SemanticMirContractError {
                        code: "SMIR010",
                        function: function.symbol.clone(),
                        location,
                        detail: "FieldGet result type differs from declared aggregate field type".to_string(),
                    });
                }
            }
            if let Some(ExecutableOperand::Value(value)) = value {
                if function
                    .value_types
                    .get(value)
                    .is_none_or(|actual| !aggregate_field_types_compatible(actual, &declared.ty))
                {
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

/// 由 `Apply(Owner, [A, B, …])` 与 layout 字段中出现的类型形参名建立 substitution。
fn type_args_substitution(output_type: &NyarType, layout: &AggregateLayout) -> BTreeMap<String, NyarType> {
    let mut formals = Vec::new();
    for field in &layout.fields {
        collect_type_parameter_names(&field.ty, &mut formals);
    }
    let args = match output_type {
        NyarType::Apply(base, args) if aggregate_owner_name(base.as_ref()) == Some(layout.name.as_str()) => args.as_slice(),
        _ => return BTreeMap::new(),
    };
    formals.into_iter().zip(args.iter().cloned()).collect()
}

fn collect_type_parameter_names(ty: &NyarType, out: &mut Vec<String>) {
    match ty {
        NyarType::Named(name) if is_type_parameter(ty) => {
            let text = name.as_str().to_string();
            if !out.iter().any(|existing| existing == &text) {
                out.push(text);
            }
        }
        NyarType::Array(element) | NyarType::Nullable(element) => collect_type_parameter_names(element, out),
        NyarType::FixedArray { element, .. } => collect_type_parameter_names(element, out),
        NyarType::Apply(base, args) => {
            collect_type_parameter_names(base, out);
            for arg in args {
                collect_type_parameter_names(arg, out);
            }
        }
        NyarType::Tuple(elems) | NyarType::Union(elems) => {
            for elem in elems {
                collect_type_parameter_names(elem, out);
            }
        }
        NyarType::Function(func) => {
            for param in &func.params {
                collect_type_parameter_names(param, out);
            }
            collect_type_parameter_names(&func.return_type, out);
        }
        _ => {}
    }
}

fn substitute_nyar_type(ty: &NyarType, substitution: &BTreeMap<String, NyarType>) -> NyarType {
    if substitution.is_empty() {
        return ty.clone();
    }
    match ty {
        NyarType::Named(name) => substitution.get(name.as_str()).cloned().unwrap_or_else(|| ty.clone()),
        NyarType::Array(element) => NyarType::Array(Box::new(substitute_nyar_type(element, substitution))),
        NyarType::Nullable(element) => NyarType::Nullable(Box::new(substitute_nyar_type(element, substitution))),
        NyarType::FixedArray { element, length } => NyarType::FixedArray {
            element: Box::new(substitute_nyar_type(element, substitution)),
            length: *length,
        },
        NyarType::Apply(base, args) => NyarType::Apply(
            Box::new(substitute_nyar_type(base, substitution)),
            args.iter().map(|arg| substitute_nyar_type(arg, substitution)).collect(),
        ),
        NyarType::Tuple(elems) => NyarType::Tuple(elems.iter().map(|elem| substitute_nyar_type(elem, substitution)).collect()),
        NyarType::Union(elems) => NyarType::Union(elems.iter().map(|elem| substitute_nyar_type(elem, substitution)).collect()),
        NyarType::Function(func) => NyarType::Function(Box::new(NyarFunctionType {
            params: func.params.iter().map(|param| substitute_nyar_type(param, substitution)).collect(),
            return_type: substitute_nyar_type(&func.return_type, substitution),
        })),
        other => other.clone(),
    }
}

/// Align emitter StructNew / FieldGet / FieldSet checks with language MIR validation:
/// generic field slots and platform aliases must not require bitwise `NyarType` equality.
fn aggregate_field_types_compatible(actual: &NyarType, declared: &NyarType) -> bool {
    if actual == declared {
        return true;
    }
    if mir_return_types_compatible(actual, declared) {
        return true;
    }
    if is_type_parameter(declared) || is_erased_generic_placeholder(declared) {
        return true;
    }
    if is_erased_generic_placeholder(actual) && is_erased_generic_placeholder(declared) {
        return true;
    }
    match (normalize_platform_alias(actual), normalize_platform_alias(declared)) {
        (NyarType::Array(actual_element), NyarType::Array(declared_element)) => {
            aggregate_field_types_compatible(&actual_element, &declared_element)
        }
        (NyarType::FixedArray { element: actual_element, .. }, NyarType::Array(declared_element))
        | (NyarType::Array(actual_element), NyarType::FixedArray { element: declared_element, .. }) => {
            aggregate_field_types_compatible(&actual_element, &declared_element)
        }
        (NyarType::FixedArray { element: actual_element, .. }, NyarType::FixedArray { element: declared_element, .. }) => {
            aggregate_field_types_compatible(&actual_element, &declared_element)
        }
        (NyarType::Apply(actual_base, actual_args), NyarType::Apply(declared_base, declared_args)) => {
            aggregate_field_types_compatible(&actual_base, &declared_base)
                && actual_args.len() == declared_args.len()
                && actual_args
                    .iter()
                    .zip(declared_args.iter())
                    .all(|(actual, declared)| aggregate_field_types_compatible(actual, declared))
        }
        _ => false,
    }
}

fn is_erased_generic_placeholder(ty: &NyarType) -> bool {
    match ty {
        NyarType::TraitObject(object) if object.trait_path.as_str() == "__generic" && object.type_arguments.is_empty() => true,
        NyarType::Named(_) if is_type_parameter(ty) => true,
        _ => false,
    }
}

fn type_matches_sum_owner_nyar(ty: &NyarType, sum_type: &str) -> bool {
    match ty {
        NyarType::Named(name) => {
            name.as_str() == sum_type
                || (sum_type == "Result" && name.as_str().ends_with("Result"))
                || (sum_type == "Option" && matches!(name.as_str(), "Option" | "Nullable"))
        }
        NyarType::Apply(base, _) => type_matches_sum_owner_nyar(base, sum_type),
        NyarType::Nullable(_) => sum_type == "Option",
        _ => false,
    }
}

fn payload_type_compatible_nyar(actual: Option<&NyarType>, declared: Option<&NyarType>) -> bool {
    match (actual, declared) {
        (None, None) => true,
        (Some(actual), Some(declared)) => aggregate_field_types_compatible(actual, declared) || is_type_parameter(declared),
        _ => false,
    }
}

fn mir_return_types_compatible(actual: &NyarType, expected: &NyarType) -> bool {
    if actual == expected {
        return true;
    }
    if result_or_option_alias_compatible(actual, expected) {
        return true;
    }
    match (normalize_platform_alias(actual), normalize_platform_alias(expected)) {
        (left, right) if left == right => true,
        _ => false,
    }
}

fn normalize_platform_alias(ty: &NyarType) -> NyarType {
    match ty {
        NyarType::Named(name) => match name.as_str() {
            "usize" | "isize" => NyarType::Integer32 { signed: name.as_str() == "isize" },
            "bool" => NyarType::Boolean,
            "utf8" | "Utf8Text" => NyarType::Utf8,
            "utf16" | "Utf16Text" => NyarType::Utf16,
            _ => ty.clone(),
        },
        NyarType::Apply(base, args) => {
            NyarType::Apply(Box::new(normalize_platform_alias(base)), args.iter().map(normalize_platform_alias).collect())
        }
        NyarType::Array(element) => NyarType::Array(Box::new(normalize_platform_alias(element))),
        NyarType::FixedArray { element, length } => {
            NyarType::FixedArray { element: Box::new(normalize_platform_alias(element)), length: *length }
        }
        NyarType::Nullable(inner) => NyarType::Nullable(Box::new(normalize_platform_alias(inner))),
        NyarType::Union(arms) => NyarType::Union(arms.iter().map(normalize_platform_alias).collect()),
        NyarType::Tuple(elems) => NyarType::Tuple(elems.iter().map(normalize_platform_alias).collect()),
        _ => ty.clone(),
    }
}

fn sum_owner_name(ty: &NyarType) -> Option<&str> {
    match ty {
        NyarType::Named(name) => Some(name.as_str()),
        NyarType::Apply(base, _) => sum_owner_name(base),
        _ => None,
    }
}

fn aggregate_owner_name(ty: &NyarType) -> Option<&str> {
    sum_owner_name(ty)
}

pub(crate) fn is_option_shaped(ty: &NyarType) -> bool {
    matches!(ty, NyarType::Nullable(_)) || sum_owner_name(ty).is_some_and(|name| matches!(name, "Option" | "Nullable"))
}

fn is_type_parameter(ty: &NyarType) -> bool {
    match ty {
        NyarType::Named(name) => {
            let text = name.as_str();
            !text.is_empty() && text.chars().all(|ch| ch.is_ascii_uppercase())
        }
        _ => false,
    }
}

fn result_or_option_alias_compatible(actual: &NyarType, expected: &NyarType) -> bool {
    let payload = |ty: &NyarType| -> Option<NyarType> {
        match ty {
            NyarType::Apply(_, args) => args.first().cloned(),
            NyarType::Nullable(inner) => Some(*inner.clone()),
            _ => None,
        }
    };
    if !is_option_shaped(actual) || !is_option_shaped(expected) {
        return false;
    }
    match (payload(actual), payload(expected)) {
        (Some(left), Some(right)) => {
            normalize_platform_alias(&left) == normalize_platform_alias(&right) || (is_type_parameter(&left) && is_type_parameter(&right))
        }
        (None, None) => true,
        _ => false,
    }
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
    for sum in &submission.sum_types {
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

fn validate_function(function: &ExecutableFunction) -> Result<(), SemanticMirContractError> {
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
            let Some(actual) = value_type(value) else {
                return Err(SemanticMirContractError {
                    code: "SMIR001",
                    function: function.symbol.clone(),
                    location,
                    detail: "return operand has no SSA type".to_string(),
                });
            };
            if !mir_return_types_compatible(&actual, &function.return_type) {
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
                let Some(actual) = value_type(argument) else {
                    return Err(SemanticMirContractError {
                        code: "SMIR001",
                        function: function.symbol.clone(),
                        location,
                        detail: "jump argument has no SSA type".to_string(),
                    });
                };
                let Some(expected) = function.value_types.get(parameter) else {
                    return Err(SemanticMirContractError {
                        code: "SMIR001",
                        function: function.symbol.clone(),
                        location,
                        detail: "jump target block parameter has no SSA type".to_string(),
                    });
                };
                if !mir_return_types_compatible(&actual, expected) {
                    return Err(SemanticMirContractError {
                        code: "SMIR007",
                        function: function.symbol.clone(),
                        location,
                        detail: format!(
                            "jump argument type differs from target block parameter (actual={actual:?}, expected={expected:?})"
                        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Block, BlockRef, ExecutableFunction, Instruction, InstructionKind, Operand, Terminator, ValueRef};
    use nyar::{Identifier, NamePath};
    use nyar_types::{
        NyarType,
        layout::{SumTypeLayout, SumVariantLayout},
    };
    use std::collections::BTreeMap;

    /// 从 terminator 推断 `return_type`，避免 `Return { Some(...) }` 与 Unit 返回类型触发 SMIR007。
    fn function(instructions: Vec<Instruction>, terminator: Terminator, value_types: BTreeMap<ValueRef, NyarType>) -> ExecutableFunction {
        let return_type = match &terminator {
            Terminator::Return { value: Some(Operand::Value(v)) } => {
                value_types.get(v).cloned().unwrap_or(NyarType::Unit)
            }
            Terminator::Return { value: Some(Operand::Constant(c)) } => match c {
                crate::contracts::Constant::Bool(_) => NyarType::Boolean,
                crate::contracts::Constant::Int(_) => NyarType::Integer64 { signed: true },
                crate::contracts::Constant::Float64(_) => NyarType::Float64,
                crate::contracts::Constant::Utf8(_) => NyarType::Utf8,
                crate::contracts::Constant::Utf16(_) => NyarType::Utf16,
                crate::contracts::Constant::Unit => NyarType::Unit,
            },
            _ => NyarType::Unit,
        };
        ExecutableFunction {
            symbol: "contract_fixture".to_string(),
            return_type,
            param_types: Vec::new(),
            value_types,
            entry: BlockRef(0),
            values: Vec::new(),
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            blocks: vec![Block { id: BlockRef(0), label: "entry".to_string(), parameters: Vec::new(), instructions, terminator }],
            diagnostics: Vec::new(),
        }
    }

    fn instr(kind: InstructionKind, results: Vec<ValueRef>) -> Instruction {
        let mut instruction = Instruction::from_kind(kind);
        instruction.results = results;
        instruction
    }

    #[test]
    fn rejects_instruction_output_without_semantic_type() {
        let result = validate_function(&function(
            vec![instr(
                InstructionKind::Copy { source: Operand::Constant(crate::contracts::Constant::Unit) },
                vec![ValueRef(7)],
            )],
            Terminator::Return { value: None },
            BTreeMap::new(),
        ));
        assert_eq!(result.unwrap_err().code, "SMIR001");
    }

    #[test]
    fn rejects_residual_pattern_match() {
        let result = validate_function(&function(
            vec![instr(
                InstructionKind::PatternMatch {
                    value: Operand::Constant(crate::contracts::Constant::Unit),
                    pattern_debug: "fixture".to_string(),
                },
                Vec::new(),
            )],
            Terminator::Return { value: None },
            BTreeMap::new(),
        ));
        assert_eq!(result.unwrap_err().code, "SMIR008");
    }

    #[test]
    fn accepts_minimal_semantic_function() {
        let result = validate_function(&function(Vec::new(), Terminator::Return { value: None }, BTreeMap::new()));
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_nominal_sum_without_layout_metadata() {
        let mut submission = FragmentSubmission::default();
        submission.sum_types.push(SumTypeLayout {
            name: "Option".to_string(),
            is_unite: true,
            tag_width: 0,
            variants: vec![SumVariantLayout { name: "Some".to_string(), tag: 0, payload_type: Some(NyarType::Integer32 { signed: true }) }],
        });
        let result = validate_submission(&submission);
        assert_eq!(result.unwrap_err().code, "SMIR006");
    }

    #[test]
    fn rejects_nominal_sum_with_duplicate_variant_tag() {
        let mut submission = FragmentSubmission::default();
        submission.sum_types.push(SumTypeLayout {
            name: "Choice".to_string(),
            is_unite: true,
            tag_width: 32,
            variants: vec![
                SumVariantLayout { name: "Left".to_string(), tag: 0, payload_type: None },
                SumVariantLayout { name: "Right".to_string(), tag: 0, payload_type: None },
            ],
        });
        let result = validate_submission(&submission);
        assert_eq!(result.unwrap_err().code, "SMIR006");
    }

    #[test]
    fn observation_is_stable_and_does_not_include_backend_names() {
        let result = validate_function(&function(Vec::new(), Terminator::Return { value: None }, BTreeMap::new()));
        assert_eq!(observation("valid_minimal", result.as_ref().map(|_| ()).map_err(|error| error)), "valid_minimal|accept||");

        let result = validate_function(&function(
            vec![instr(
                InstructionKind::Copy { source: Operand::Constant(crate::contracts::Constant::Unit) },
                vec![ValueRef(7)],
            )],
            Terminator::Return { value: None },
            BTreeMap::new(),
        ));
        assert_eq!(
            observation("missing_value_type", result.as_ref().map(|_| ()).map_err(|error| error)),
            "missing_value_type|reject|SMIR001|instruction"
        );
    }

    #[test]
    fn accepts_structured_array_length() {
        let receiver = ValueRef(10);
        let output = ValueRef(11);
        let mut value_types = BTreeMap::new();
        value_types.insert(receiver, NyarType::Array(Box::new(NyarType::Integer32 { signed: true })));
        value_types.insert(output, NyarType::Integer32 { signed: true });
        let result = validate_function(&function(
            vec![instr(InstructionKind::ArrayLength { array: Operand::Value(receiver) }, vec![output])],
            Terminator::Return { value: Some(Operand::Value(output)) },
            value_types,
        ));
        assert_eq!(
            observation("aggregate_array_sum.valid_array_len", result.as_ref().map(|_| ()).map_err(|error| error)),
            "aggregate_array_sum.valid_array_len|accept||"
        );
    }

    #[test]
    fn rejects_array_length_without_result_type() {
        let receiver = ValueRef(10);
        let output = ValueRef(11);
        let mut value_types = BTreeMap::new();
        value_types.insert(receiver, NyarType::Array(Box::new(NyarType::Integer32 { signed: true })));
        let result = validate_function(&function(
            vec![instr(InstructionKind::ArrayLength { array: Operand::Value(receiver) }, vec![output])],
            Terminator::Return { value: None },
            value_types,
        ));
        assert_eq!(
            observation("aggregate_array_sum.missing_result_type", result.as_ref().map(|_| ()).map_err(|error| error)),
            "aggregate_array_sum.missing_result_type|reject|SMIR001|instruction"
        );
    }

    #[test]
    fn accepts_thin_call_with_typed_results() {
        let left = ValueRef(20);
        let right = ValueRef(21);
        let result_v = ValueRef(22);
        let ty = NyarType::Named(Identifier::new("TokenKind"));
        let mut value_types = BTreeMap::new();
        value_types.insert(left, ty.clone());
        value_types.insert(right, ty);
        value_types.insert(result_v, NyarType::Boolean);
        let result = validate_function(&function(
            vec![instr(
                InstructionKind::Call {
                    callee: Operand::Symbol(NamePath::new(vec![Identifier::new("primitive"), Identifier::new("sum_equal")])),
                    arguments: vec![Operand::Value(left), Operand::Value(right)],
                },
                vec![result_v],
            )],
            Terminator::Return { value: Some(Operand::Value(result_v)) },
            value_types,
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_call_result_without_semantic_type() {
        let left = ValueRef(20);
        let right = ValueRef(21);
        let result_v = ValueRef(22);
        let ty = NyarType::Named(Identifier::new("TokenKind"));
        let mut value_types = BTreeMap::new();
        value_types.insert(left, ty.clone());
        value_types.insert(right, ty);
        let result = validate_function(&function(
            vec![instr(
                InstructionKind::Call {
                    callee: Operand::Symbol(NamePath::new(vec![Identifier::new("primitive"), Identifier::new("sum_equal")])),
                    arguments: vec![Operand::Value(left), Operand::Value(right)],
                },
                vec![result_v],
            )],
            Terminator::Return { value: None },
            value_types,
        ));
        assert_eq!(result.unwrap_err().code, "SMIR001");
    }
}
