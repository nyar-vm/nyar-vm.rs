//! Canonical Semantic MIR 的 M2 验证器。
//!
//! 本模块只拒绝不合法 MIR：验证 stable-ID 覆盖、SSA 定义与支配关系、
//! 调用与聚合合同。它不创建、不修正 callable、layout 或类型等语义事实。

use crate::semantic_ids::MirValueId;
use std::collections::BTreeMap;

use super::{
    canonical_mir::{
        CanonicalArrayInitialization, CanonicalCallee, CanonicalEffectKind, CanonicalMirError, CanonicalOperation, CanonicalSemanticMir,
        CanonicalTerminator,
    },
    linked_program::{
        CanonicalPrimitiveType, CanonicalTypeKind, LinkedSemanticProgram, canonical_nominal_owner_matches, canonical_types_compatible,
    },
};

impl CanonicalSemanticMir {
    /// 在进入 RepresentationPlan 前验证 stable-ID、链接和 SSA 合同。
    pub fn validate(&self, linked: &LinkedSemanticProgram) -> Result<(), CanonicalMirError> {
        linked.validate_types()?;
        let nominal_types = linked.nominal_instances.values().map(|record| record.ty).collect();
        let mut export_names = std::collections::BTreeSet::new();
        for (function, export) in &linked.exports {
            if !linked.item_instances.contains_key(function) || !self.functions.contains_key(function) {
                return Err(CanonicalMirError::MissingSurfaceBody { function: *function });
            }
            if !export_names.insert(&export.exported_name) {
                return Err(CanonicalMirError::DuplicateExportName { name: export.exported_name.clone() });
            }
        }
        for function in linked.entries.keys() {
            if !linked.item_instances.contains_key(function) || !self.functions.contains_key(function) {
                return Err(CanonicalMirError::MissingSurfaceBody { function: *function });
            }
        }
        for (key, function) in &self.functions {
            if key != &function.instance {
                return Err(CanonicalMirError::FunctionKeyMismatch { key: *key, instance: function.instance });
            }
            let Some(owner) = linked.item_instances.get(key)
            else {
                return Err(CanonicalMirError::UnknownFunction { function: *key });
            };
            if function.parameters.iter().map(|(_, ty)| *ty).collect::<Vec<_>>() != owner.parameter_types
                || function.return_type != owner.return_type
            {
                return Err(CanonicalMirError::FunctionSignatureMismatch { function: *key });
            }
            for ty in function.value_types.values() {
                if !linked.types.contains_key(ty) {
                    return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                }
                linked.validate_nominal_value_type(*ty, &nominal_types)?;
            }
            let mut defined = std::collections::BTreeSet::new();
            for (value, ty) in &function.parameters {
                if !linked.types.contains_key(ty) {
                    return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                }
                if !defined.insert(*value) {
                    return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                }
                if function.value_types.get(value) != Some(ty) {
                    return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                }
            }
            if !linked.types.contains_key(&function.return_type) {
                return Err(CanonicalMirError::UnknownType { function: *key, ty: function.return_type });
            }
            if !function.blocks.contains_key(&function.entry) {
                return Err(CanonicalMirError::UnknownBlock { function: *key, block: function.entry });
            }
            let all_blocks = function.blocks.keys().copied().collect::<std::collections::BTreeSet<_>>();
            let mut predecessors = all_blocks.iter().map(|block| (*block, std::collections::BTreeSet::new())).collect::<BTreeMap<_, _>>();
            for block in function.blocks.values() {
                let targets = match &block.terminator {
                    CanonicalTerminator::Jump { target, .. } => vec![*target],
                    CanonicalTerminator::Branch { then_target, else_target, .. } => vec![*then_target, *else_target],
                    CanonicalTerminator::PerformEffect { resume_target, .. } => {
                        vec![*resume_target]
                    }
                    CanonicalTerminator::StateDispatch { cases, default_target, .. } => {
                        cases.iter().map(|(_, target)| *target).chain(std::iter::once(*default_target)).collect()
                    }
                    _ => Vec::new(),
                };
                for target in targets {
                    if let Some(preds) = predecessors.get_mut(&target) {
                        preds.insert(block.id);
                    }
                }
            }
            let mut dominators = all_blocks
                .iter()
                .map(|block| {
                    let initial = if *block == function.entry { std::collections::BTreeSet::from([*block]) } else { all_blocks.clone() };
                    (*block, initial)
                })
                .collect::<BTreeMap<_, _>>();
            loop {
                let mut changed = false;
                for block in all_blocks.iter().copied().filter(|block| *block != function.entry) {
                    let mut next = all_blocks.clone();
                    for predecessor in predecessors.get(&block).into_iter().flat_map(|values| values.iter()) {
                        next = next.intersection(dominators.get(predecessor).unwrap()).copied().collect();
                    }
                    next.insert(block);
                    if next != dominators[&block] {
                        dominators.insert(block, next);
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            let mut definitions = function.parameters.iter().map(|(value, _)| (*value, None)).collect::<BTreeMap<_, _>>();
            for block in function.blocks.values() {
                for (value, _) in &block.parameters {
                    if block.id == function.entry {
                        if !function.parameters.iter().any(|(parameter, _)| parameter == value) {
                            return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                        }
                        continue;
                    }
                    if definitions.insert(*value, Some(block.id)).is_some() {
                        return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                    }
                }
                for value in block.instructions.iter().flat_map(|instruction| instruction.results.iter()) {
                    if definitions.insert(*value, Some(block.id)).is_some() {
                        return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                    }
                }
            }
            for (block_id, block) in &function.blocks {
                let mut block_defined = defined.clone();
                let value_is_available = |value: &MirValueId| match definitions.get(value) {
                    Some(None) => true,
                    Some(Some(definition_block)) => *definition_block == *block_id || dominators[block_id].contains(definition_block),
                    None => false,
                };
                for (index, (value, ty)) in block.parameters.iter().enumerate() {
                    if !linked.types.contains_key(ty) {
                        return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                    }
                    if block.id == function.entry {
                        if function.parameters.get(index) != Some(&(*value, *ty)) {
                            return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                        }
                        continue;
                    }
                    if !block_defined.insert(*value) {
                        return Err(CanonicalMirError::DuplicateBlockDefinition { function: *key, value: *value });
                    }
                    if function.value_types.get(value) != Some(ty) {
                        return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                    }
                }
                for instruction in &block.instructions {
                    let uses = match &instruction.operation {
                        CanonicalOperation::Invoke { callee, arguments } => {
                            match callee {
                                CanonicalCallee::Item(callee) => {
                                    let Some(callee_record) = linked.item_instances.get(callee)
                                    else {
                                        return Err(CanonicalMirError::UnknownCallee { function: *key, callee: *callee });
                                    };
                                    for ty in callee_record.parameter_types.iter().chain(std::iter::once(&callee_record.return_type)) {
                                        if !linked.types.contains_key(ty) {
                                            return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                                        }
                                    }
                                    if arguments.len() != callee_record.parameter_types.len() {
                                        return Err(CanonicalMirError::CallArityMismatch { function: *key, callee: *callee });
                                    }
                                    for (value, expected) in arguments.iter().zip(&callee_record.parameter_types) {
                                        if function.value_types.get(value) != Some(expected) {
                                            return Err(CanonicalMirError::CallArgumentTypeMismatch {
                                                function: *key,
                                                callee: *callee,
                                                value: *value,
                                            });
                                        }
                                    }
                                    if let Some(result) = instruction.results.first() {
                                        if function.value_types.get(result) != Some(&callee_record.return_type) {
                                            return Err(CanonicalMirError::CallResultTypeMismatch {
                                                function: *key,
                                                callee: *callee,
                                                value: *result,
                                            });
                                        }
                                    }
                                    let returns_unit = matches!(
                                        linked.types.get(&callee_record.return_type).map(|row| &row.kind),
                                        Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit | CanonicalPrimitiveType::Void))
                                    );
                                    if instruction.results.len() > 1 || (!returns_unit && instruction.results.is_empty()) {
                                        return Err(CanonicalMirError::OperationResultArityMismatch {
                                            function: *key,
                                            instruction: instruction.id,
                                        });
                                    }
                                }
                                CanonicalCallee::Value(callee) => {
                                    let Some(callee_type) = function.value_types.get(callee)
                                    else {
                                        return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *callee });
                                    };
                                    let Some(CanonicalTypeKind::Function { parameters, return_type }) =
                                        linked.types.get(callee_type).map(|record| &record.kind)
                                    else {
                                        return Err(CanonicalMirError::ValueCalleeNotFunction { function: *key, value: *callee });
                                    };
                                    if arguments.len() != parameters.len() {
                                        return Err(CanonicalMirError::ValueCalleeArityMismatch { function: *key, value: *callee });
                                    }
                                    for (argument, expected) in arguments.iter().zip(parameters) {
                                        if function.value_types.get(argument) != Some(expected) {
                                            return Err(CanonicalMirError::ValueCalleeArgumentTypeMismatch {
                                                function: *key,
                                                value: *callee,
                                                argument: *argument,
                                            });
                                        }
                                    }
                                    if let Some(result) = instruction.results.first() {
                                        if function.value_types.get(result) != Some(return_type) {
                                            return Err(CanonicalMirError::ValueCalleeResultTypeMismatch {
                                                function: *key,
                                                value: *callee,
                                                result: *result,
                                            });
                                        }
                                    }
                                    let returns_unit = matches!(
                                        linked.types.get(return_type).map(|row| &row.kind),
                                        Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit | CanonicalPrimitiveType::Void))
                                    );
                                    if instruction.results.len() > 1 || (!returns_unit && instruction.results.is_empty()) {
                                        return Err(CanonicalMirError::OperationResultArityMismatch {
                                            function: *key,
                                            instruction: instruction.id,
                                        });
                                    }
                                }
                            }
                            let mut uses = arguments.clone();
                            if let CanonicalCallee::Value(callee) = callee {
                                uses.push(*callee);
                            }
                            uses
                        }
                        CanonicalOperation::Copy { source } => {
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value))
                                    != function.value_types.get(source)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*source]
                        }
                        CanonicalOperation::Cast { source, ty } => {
                            if !linked.types.contains_key(ty) {
                                return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                            }
                            if function.value_types.get(source).is_none() {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *source,
                                });
                            }
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(ty)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*source]
                        }
                        CanonicalOperation::StoreVar { name: _, value, ty } => {
                            if instruction.results.len() > 1 {
                                return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                            }
                            if let Some(result) = instruction.results.first() {
                                if function.value_types.get(result) != function.value_types.get(value) {
                                    return Err(CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id });
                                }
                            }
                            if let Some(expected_ty) = ty {
                                if !linked.types.contains_key(expected_ty) {
                                    return Err(CanonicalMirError::UnknownType { function: *key, ty: *expected_ty });
                                }
                                if function.value_types.get(value).is_some_and(|actual| actual != expected_ty) {
                                    return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                        function: *key,
                                        instruction: instruction.id,
                                        value: *value,
                                    });
                                }
                            }
                            vec![*value]
                        }
                        CanonicalOperation::AggregateCopy { source, destination } => {
                            let source_type = function.value_types.get(source);
                            let destination_type = function.value_types.get(destination);
                            if source_type.is_none() || source_type != destination_type {
                                return Err(CanonicalMirError::ValueTypeMismatch {
                                    function: *key,
                                    value: *destination,
                                    expected: source_type.copied().unwrap_or(function.return_type),
                                });
                            }
                            if instruction.results.as_slice() != [*destination] {
                                return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *destination });
                            }
                            vec![*source]
                        }
                        CanonicalOperation::LoadConstant { .. } => {
                            if instruction.results.len() != 1 {
                                return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                            }
                            Vec::new()
                        }
                        CanonicalOperation::SumNew { nominal, variant, payload } => {
                            let Some(record) = linked.variants.get(&(*nominal, *variant))
                            else {
                                return Err(CanonicalMirError::UnknownVariant { function: *key, variant: *variant });
                            };
                            let Some(owner) = linked.nominal_instances.get(nominal)
                            else {
                                return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                            };
                            let result_type = instruction.results.first().and_then(|value| function.value_types.get(value));
                            let result_is_owner = result_type.is_some_and(|ty| canonical_nominal_owner_matches(&linked, *ty, owner.ty));
                            if instruction.results.len() != 1 || !result_is_owner {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            match (record.payload_type, payload) {
                                (Some(expected), Some(value)) if function.value_types.get(value) == Some(&expected) => {
                                    vec![*value]
                                }
                                (None, None) => Vec::new(),
                                _ => {
                                    return Err(CanonicalMirError::VariantPayloadMismatch { function: *key, variant: *variant });
                                }
                            }
                        }
                        CanonicalOperation::SumPayloadGet { nominal, variant, object } => {
                            let Some(record) = linked.variants.get(&(*nominal, *variant))
                            else {
                                return Err(CanonicalMirError::UnknownVariant { function: *key, variant: *variant });
                            };
                            if record.payload_type.is_none() {
                                return Err(CanonicalMirError::VariantPayloadMismatch { function: *key, variant: *variant });
                            }
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != record.payload_type.as_ref()
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            let Some(object_type) = function.value_types.get(object)
                            else {
                                return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                            };
                            let Some(owner) = linked.nominal_instances.get(nominal)
                            else {
                                return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                            };
                            if !canonical_nominal_owner_matches(&linked, *object_type, owner.ty) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *object,
                                });
                            }
                            vec![*object]
                        }
                        CanonicalOperation::SumVariantIs { nominal, variant, object } => {
                            let Some(_record) = linked.variants.get(&(*nominal, *variant))
                            else {
                                return Err(CanonicalMirError::UnknownVariant { function: *key, variant: *variant });
                            };
                            let bool_result = instruction.results.len() == 1
                                && instruction.results.first().and_then(|value| function.value_types.get(value)).is_some_and(|ty| {
                                    matches!(
                                        linked.types.get(ty).map(|row| &row.kind),
                                        Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool))
                                    )
                                });
                            if !bool_result {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            let Some(object_type) = function.value_types.get(object)
                            else {
                                return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                            };
                            let Some(owner) = linked.nominal_instances.get(nominal)
                            else {
                                return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                            };
                            if !canonical_nominal_owner_matches(&linked, *object_type, owner.ty) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *object,
                                });
                            }
                            vec![*object]
                        }
                        CanonicalOperation::StructNew { nominal, fields } => {
                            let Some(nominal_record) = linked.nominal_instances.get(nominal)
                            else {
                                return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                            };
                            let mut seen = std::collections::BTreeSet::new();
                            for (field, value) in fields {
                                if !seen.insert(*field) {
                                    return Err(CanonicalMirError::DuplicateStructField { function: *key, nominal: *nominal, field: *field });
                                }
                                let Some(record) = linked.fields.get(field)
                                else {
                                    return Err(CanonicalMirError::UnknownField { function: *key, field: *field });
                                };
                                if record.owner != *nominal {
                                    return Err(CanonicalMirError::FieldOwnerMismatch { function: *key, field: *field, nominal: *nominal });
                                }
                                if function.value_types.get(value) != Some(&record.ty) {
                                    return Err(CanonicalMirError::FieldTypeMismatch { function: *key, field: *field, value: *value });
                                }
                            }
                            for field in &nominal_record.fields {
                                if !seen.contains(field) {
                                    return Err(CanonicalMirError::MissingStructField { function: *key, nominal: *nominal, field: *field });
                                }
                            }
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(&nominal_record.ty)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            fields.iter().map(|(_, value)| *value).collect()
                        }
                        CanonicalOperation::FieldGet { object, field } => {
                            let Some(record) = linked.fields.get(field)
                            else {
                                return Err(CanonicalMirError::UnknownField { function: *key, field: *field });
                            };
                            let Some(object_ty) = function.value_types.get(object)
                            else {
                                return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                            };
                            let Some(owner) = linked.nominal_instances.get(&record.owner)
                            else {
                                return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: record.owner });
                            };
                            if !canonical_nominal_owner_matches(&linked, *object_ty, owner.ty) {
                                return Err(CanonicalMirError::FieldOwnerMismatch { function: *key, field: *field, nominal: record.owner });
                            }
                            if instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(&record.ty) {
                                return Err(CanonicalMirError::FieldTypeMismatch {
                                    function: *key,
                                    field: *field,
                                    value: instruction.results.first().copied().unwrap_or(*object),
                                });
                            }
                            vec![*object]
                        }
                        CanonicalOperation::FieldSet { object, field, value } => {
                            if !instruction.results.is_empty() {
                                return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                            }
                            let Some(record) = linked.fields.get(field)
                            else {
                                return Err(CanonicalMirError::UnknownField { function: *key, field: *field });
                            };
                            let Some(object_ty) = function.value_types.get(object)
                            else {
                                return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                            };
                            let Some(owner) = linked.nominal_instances.get(&record.owner)
                            else {
                                return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: record.owner });
                            };
                            if !canonical_nominal_owner_matches(&linked, *object_ty, owner.ty) {
                                return Err(CanonicalMirError::FieldOwnerMismatch { function: *key, field: *field, nominal: record.owner });
                            }
                            if function.value_types.get(value) != Some(&record.ty) {
                                return Err(CanonicalMirError::FieldTypeMismatch { function: *key, field: *field, value: *value });
                            }
                            vec![*object, *value]
                        }
                        CanonicalOperation::ArrayGet { array, index } => {
                            let Some(CanonicalTypeKind::Array { element, .. }) =
                                function.value_types.get(array).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind)
                            else {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *array,
                                });
                            };
                            let index_is_integer =
                                function.value_types.get(index).and_then(|ty| linked.types.get(ty)).is_some_and(|row| {
                                    matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. }))
                                });
                            if !index_is_integer {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *index,
                                });
                            }
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(element)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*array, *index]
                        }
                        CanonicalOperation::ArrayNew { array_type, length, initialization } => {
                            let Some(CanonicalTypeKind::Array { element, .. }) = linked.types.get(array_type).map(|row| &row.kind)
                            else {
                                return Err(CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id });
                            };
                            let length_is_integer =
                                function.value_types.get(length).and_then(|ty| linked.types.get(ty)).is_some_and(|row| {
                                    matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. }))
                                });
                            let fill_is_valid = match initialization {
                                CanonicalArrayInitialization::Default => true,
                                CanonicalArrayInitialization::Fill(value) => function.value_types.get(value) == Some(element),
                            };
                            if !length_is_integer || !fill_is_valid {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *length,
                                });
                            }
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(array_type)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            match initialization {
                                CanonicalArrayInitialization::Default => vec![*length],
                                CanonicalArrayInitialization::Fill(value) => vec![*length, *value],
                            }
                        }
                        CanonicalOperation::ArrayFromElements { array_type, elements } => {
                            let Some(CanonicalTypeKind::Array { element, .. }) = linked.types.get(array_type).map(|row| &row.kind)
                            else {
                                return Err(CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id });
                            };
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(array_type)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            if elements.iter().any(|value| function.value_types.get(value) != Some(element)) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *elements.iter().find(|value| function.value_types.get(value) != Some(element)).unwrap(),
                                });
                            }
                            elements.clone()
                        }
                        CanonicalOperation::ArraySet { array, index, value } => {
                            let Some(CanonicalTypeKind::Array { element, .. }) =
                                function.value_types.get(array).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind)
                            else {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *array,
                                });
                            };
                            let index_is_integer =
                                function.value_types.get(index).and_then(|ty| linked.types.get(ty)).is_some_and(|row| {
                                    matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. }))
                                });
                            if !index_is_integer || function.value_types.get(value) != Some(element) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: if !index_is_integer { *index } else { *value },
                                });
                            }
                            if !instruction.results.is_empty() {
                                return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                            }
                            vec![*array, *index, *value]
                        }
                        CanonicalOperation::ArrayLength { array } => {
                            if !matches!(
                                function.value_types.get(array).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind),
                                Some(CanonicalTypeKind::Array { .. })
                            ) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *array,
                                });
                            }
                            let result_is_integer = instruction.results.len() == 1
                                && instruction
                                    .results
                                    .first()
                                    .and_then(|value| function.value_types.get(value))
                                    .and_then(|ty| linked.types.get(ty))
                                    .is_some_and(|row| {
                                        matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. }))
                                    });
                            if !result_is_integer {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*array]
                        }
                        CanonicalOperation::ArrayPush { array, value } => {
                            let Some(array_type) = function.value_types.get(array)
                            else {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *array,
                                });
                            };
                            let Some(CanonicalTypeKind::Array { element, .. }) = linked.types.get(array_type).map(|row| &row.kind)
                            else {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *array,
                                });
                            };
                            if function.value_types.get(value) != Some(element) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *value,
                                });
                            }
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(array_type)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*array, *value]
                        }
                        CanonicalOperation::NullIsNull { value: operand } => {
                            if !matches!(
                                function.value_types.get(operand).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind),
                                Some(CanonicalTypeKind::Nullable(_))
                            ) {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *operand,
                                });
                            }
                            let result_is_boolean = instruction.results.len() == 1
                                && instruction
                                    .results
                                    .first()
                                    .and_then(|value| function.value_types.get(value))
                                    .and_then(|ty| linked.types.get(ty))
                                    .is_some_and(|row| matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool)));
                            if !result_is_boolean {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*operand]
                        }
                        CanonicalOperation::NullUnwrap { value: operand } => {
                            let Some(CanonicalTypeKind::Nullable(payload)) =
                                function.value_types.get(operand).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind)
                            else {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *operand,
                                });
                            };
                            if instruction.results.len() != 1
                                || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(payload)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            vec![*operand]
                        }
                        CanonicalOperation::TupleNew { element_types, fields } => {
                            let Some(result) = instruction.results.first().and_then(|value| function.value_types.get(value))
                            else {
                                return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                            };
                            if instruction.results.len() != 1
                                || !matches!(linked.types.get(result).map(|row| &row.kind), Some(CanonicalTypeKind::Tuple(members)) if members == element_types)
                            {
                                return Err(if instruction.results.len() == 1 {
                                    CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                                }
                                else {
                                    CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                                });
                            }
                            if fields.len() != element_types.len() {
                                return Err(CanonicalMirError::OperationOperandArityMismatch { function: *key, instruction: instruction.id });
                            }
                            if let Some((value, _)) =
                                fields.iter().zip(element_types).find(|(value, expected)| function.value_types.get(value) != Some(expected))
                            {
                                return Err(CanonicalMirError::OperationOperandTypeMismatch {
                                    function: *key,
                                    instruction: instruction.id,
                                    value: *value,
                                });
                            }
                            fields.clone()
                        }
                    };
                    if uses.iter().any(|value| !value_is_available(value)) {
                        let value = *uses.iter().find(|value| !value_is_available(value)).unwrap();
                        return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value });
                    }
                    for value in &uses {
                        if let Some(Some(definition_block)) = definitions.get(value) {
                            if definition_block != block_id && !dominators[block_id].contains(definition_block) {
                                return Err(CanonicalMirError::NonDominatingUse { function: *key, value: *value, block: *block_id });
                            }
                        }
                    }
                    for result in &instruction.results {
                        if !block_defined.insert(*result) {
                            return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *result });
                        }
                        if !function.value_types.contains_key(result) {
                            return Err(CanonicalMirError::ValueTypeMismatch {
                                function: *key,
                                value: *result,
                                expected: function.return_type,
                            });
                        }
                    }
                }
                let (targets, terminator_values): (Vec<_>, Vec<_>) = match &block.terminator {
                    CanonicalTerminator::Return { value } => {
                        if let Some(value) = value {
                            if !function
                                .value_types
                                .get(value)
                                .is_some_and(|actual| canonical_types_compatible(&linked, *actual, function.return_type))
                            {
                                return Err(CanonicalMirError::ReturnTypeMismatch { function: *key, value: *value });
                            }
                        }
                        (Vec::new(), value.iter().copied().collect())
                    }
                    CanonicalTerminator::Jump { target, arguments } => (vec![(*target, arguments.len())], arguments.clone()),
                    CanonicalTerminator::Branch { condition, then_target, else_target } => {
                        (vec![(*then_target, 0), (*else_target, 0)], vec![*condition])
                    }
                    CanonicalTerminator::PerformEffect { effect, resume_target, payload } => {
                        let payload = payload.ok_or(CanonicalMirError::MissingEffectPayload { function: *key, block: *block_id })?;
                        let resume_arity = usize::from(*effect != CanonicalEffectKind::AsyncSpawn);
                        (vec![(*resume_target, resume_arity)], vec![payload])
                    }
                    CanonicalTerminator::StateDispatch { state, cases, default_target } => {
                        let state_type = function.value_types.get(state).and_then(|ty| linked.types.get(ty));
                        if !matches!(
                            state_type.map(|record| &record.kind),
                            Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. }))
                        ) {
                            return Err(CanonicalMirError::InvalidDispatchState { function: *key, value: *state });
                        }
                        let mut states = std::collections::BTreeSet::new();
                        for (state, _) in cases {
                            if !states.insert(*state) {
                                return Err(CanonicalMirError::DuplicateDispatchState { function: *key, block: *block_id, state: *state });
                            }
                        }
                        (cases.iter().map(|(_, target)| (*target, 0)).chain(std::iter::once((*default_target, 0))).collect(), vec![*state])
                    }
                    CanonicalTerminator::YieldToRuntime { payload, .. } => {
                        let payload = payload.ok_or(CanonicalMirError::MissingEffectPayload { function: *key, block: *block_id })?;
                        (Vec::new(), vec![payload])
                    }
                    CanonicalTerminator::Unreachable => (Vec::new(), Vec::new()),
                };
                for value in terminator_values {
                    if !value_is_available(&value) {
                        return Err(CanonicalMirError::TerminatorUseBeforeDefinition { function: *key, value });
                    }
                    if let Some(Some(definition_block)) = definitions.get(&value) {
                        if definition_block != block_id && !dominators[block_id].contains(definition_block) {
                            return Err(CanonicalMirError::NonDominatingUse { function: *key, value, block: *block_id });
                        }
                    }
                }
                for (target, arity) in targets {
                    let Some(target_block) = function.blocks.get(&target)
                    else {
                        return Err(CanonicalMirError::UnknownBlock { function: *key, block: target });
                    };
                    if target_block.parameters.len() != arity {
                        return Err(CanonicalMirError::BlockParameterArityMismatch { function: *key, block: target });
                    }
                    if let CanonicalTerminator::Jump { arguments, .. } = &block.terminator {
                        for (value, (_, expected)) in arguments.iter().zip(&target_block.parameters) {
                            if function.value_types.get(value) != Some(expected) {
                                return Err(CanonicalMirError::BlockParameterTypeMismatch { function: *key, block: target, value: *value });
                            }
                        }
                    }
                }
                let _ = block_id;
            }
        }
        Ok(())
    }
}
