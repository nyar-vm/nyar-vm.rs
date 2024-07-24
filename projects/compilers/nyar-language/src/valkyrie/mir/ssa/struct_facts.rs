//! 结构体字段布局、继承索引与值类型事实。

use super::model::*;
use crate::types::hir::{HirModule, HirParent, ValkyrieType};
use std::collections::BTreeMap;

fn parent_namespace_matches(struct_namespace: &[crate::Identifier], parent: &HirParent) -> bool {
    let path = &parent.name;
    if path.parts().is_empty() {
        return struct_namespace.is_empty();
    }
    if path.parts().len() == 1 {
        return struct_namespace.is_empty();
    }
    let parent_namespace = path.parts()[..path.parts().len() - 1]
        .iter()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>();
    struct_namespace.len() == parent_namespace.len()
        && struct_namespace.iter().zip(parent_namespace).all(|(left, right)| left.as_str() == right)
}

fn resolve_parent_declarations(module: &HirModule, parents: &[HirParent]) -> Vec<nyar_types::ItemId> {
    parents
        .iter()
        .filter_map(|parent| {
            let name = parent.name.parts().last()?;
            module
                .structs
                .iter()
                .chain(module.imported_semantic_exports.iter().flat_map(|export| export.structs.iter()))
                .find(|structure| structure.name.as_str() == name.as_str() && parent_namespace_matches(&structure.namespace, parent))
                .and_then(|structure| structure.constructor_declaration)
        })
        .collect()
}

/// 将 `HirStruct` 降级为 `MirStruct`，保留字段类型供后端生成 `TypeDef` / `Field`。
pub(super) fn lower_struct(
    module: &HirModule,
    hir_struct: &crate::types::hir::HirStruct,
    nominal: usize,
    field_start: usize,
) -> MirStruct {
    let fields = hir_struct
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| MirField {
            id: nyar_types::FieldId::from_index((field_start + index) as u32).expect("field identity overflow"),
            name: field.name.to_string(),
            ty: field.ty.clone(),
        })
        .collect();
    let namespace = hir_struct.namespace.iter().map(|part| part.as_str().to_string()).collect::<Vec<_>>().join(".");
    MirStruct {
        nominal: nyar_types::NominalInstanceId::from_index(nominal as u32).expect("nominal identity overflow"),
        declaration: hir_struct.constructor_declaration,
        name: hir_struct.name.to_string(),
        namespace,
        generics: hir_struct.generics.clone(),
        fields,
        is_value_type: hir_struct.is_value_type,
        parent_declarations: resolve_parent_declarations(module, &hir_struct.parents),
    }
}

pub(super) fn collect_field_declarations(module: &HirModule) -> Vec<MirStruct> {
    fn collect(module: &HirModule, declarations: &mut Vec<MirStruct>, next_nominal: &mut usize, next_field: &mut usize) {
        for hir_struct in &module.structs {
            declarations.push(lower_struct(module, hir_struct, *next_nominal, *next_field));
            *next_nominal += 1;
            *next_field += hir_struct.fields.len();
        }
        for export in &module.imported_semantic_exports {
            for hir_struct in &export.structs {
                declarations.push(lower_struct(module, hir_struct, *next_nominal, *next_field));
                *next_nominal += 1;
                *next_field += hir_struct.fields.len();
            }
        }
        for singleton in &module.singletons {
            let fields = singleton
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| MirField {
                    id: nyar_types::FieldId::from_index((*next_field + index) as u32).expect("field identity overflow"),
                    name: field.name.to_string(),
                    ty: field.ty.clone(),
                })
                .collect();
            declarations.push(MirStruct {
                name: singleton.name.to_string(),
                namespace: singleton.namespace.iter().map(|part| part.as_str()).collect::<Vec<_>>().join("."),
                generics: singleton.generics.clone(),
                nominal: nyar_types::NominalInstanceId::from_index(*next_nominal as u32).expect("nominal identity overflow"),
                declaration: None,
                fields,
                is_value_type: false,
                parent_declarations: Vec::new(),
            });
            *next_nominal += 1;
            *next_field += singleton.fields.len();
        }
        for submodule in &module.submodules {
            collect(submodule, declarations, next_nominal, next_field);
        }
    }
    let mut declarations = Vec::new();
    let mut next_nominal = 0usize;
    let mut next_field = 0usize;
    collect(module, &mut declarations, &mut next_nominal, &mut next_field);
    let mut unique = Vec::new();
    for declaration in declarations {
        if !declaration.declaration.is_some_and(|identity| unique.iter().any(|existing: &MirStruct| existing.declaration == Some(identity))) {
            unique.push(declaration);
        }
    }
    unique
}

/// Replace `Self` / `Named("Self")` with the imply/struct owner type.
pub(super) fn resolve_self_type_with_owner(ty: &ValkyrieType, owner: Option<&ValkyrieType>) -> ValkyrieType {
    match (ty, owner) {
        (ValkyrieType::SelfType, Some(owner)) => owner.clone(),
        (ValkyrieType::Named(name), Some(owner)) if name.as_str() == "Self" => owner.clone(),
        (ValkyrieType::Nominal { declaration, arguments }, _) => ValkyrieType::Nominal {
            declaration: *declaration,
            arguments: arguments.iter().map(|arg| resolve_self_type_with_owner(arg, owner)).collect(),
        },
        (ValkyrieType::Array(inner), _) => ValkyrieType::Array(Box::new(resolve_self_type_with_owner(inner, owner))),
        (ValkyrieType::FixedArray { element, length }, _) => {
            ValkyrieType::FixedArray { element: Box::new(resolve_self_type_with_owner(element, owner)), length: *length }
        }
        (ValkyrieType::Nullable(inner), _) => ValkyrieType::Nullable(Box::new(resolve_self_type_with_owner(inner, owner))),
        (ValkyrieType::Apply(base, args), _) => ValkyrieType::Apply(
            Box::new(resolve_self_type_with_owner(base, owner)),
            args.iter().map(|arg| resolve_self_type_with_owner(arg, owner)).collect(),
        ),
        (ValkyrieType::Tuple(items), _) => ValkyrieType::Tuple(items.iter().map(|item| resolve_self_type_with_owner(item, owner)).collect()),
        (ValkyrieType::Union(items), _) => ValkyrieType::Union(items.iter().map(|item| resolve_self_type_with_owner(item, owner)).collect()),
        (ValkyrieType::Intersection(items), _) => {
            ValkyrieType::Intersection(items.iter().map(|item| resolve_self_type_with_owner(item, owner)).collect())
        }
        (ValkyrieType::Function(func), _) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: func.params.iter().map(|param| resolve_self_type_with_owner(param, owner)).collect(),
            return_type: resolve_self_type_with_owner(&func.return_type, owner),
        })),
        _ => ty.clone(),
    }
}

