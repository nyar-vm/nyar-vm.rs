//! Semantic MIR 的 sum 声明合同；不持有物理 tag 宽度或目标布局。

use crate::types::{
    Identifier,
    hir::{GenericType, ValkyrieType},
};

/// 保留声明的泛型参数与 variant 类型；实例化必须先于物理表示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirSumDeclaration {
    /// Semantic MIR 冻结的名义声明身份。
    pub nominal: nyar_types::NominalInstanceId,
    /// HIR 冻结的 sum 声明身份。
    pub declaration: Option<nyar_types::ItemId>,
    /// 声明 owner；身份迁移不得由布局反推。
    pub name: String,
    /// 语言声明是否允许 variant 子类型。
    pub is_unite: bool,
    /// 声明 binder、kind 与 bounds。
    pub generics: Vec<GenericType>,
    /// 声明顺序中的 variant。
    pub variants: Vec<MirSumVariant>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ValkyrieCompiler,
        mir::{MirLowerer, MirOperation, validation::validate_semantic_module},
    };

    #[test]
    fn sum_declaration_preserves_generic_fields_and_instantiates_without_variant_spelling() {
        let hir = ValkyrieCompiler::default()
            .compile_source(
                r#"
unite Parcel<First, Second> {
    Arbitrary { head: Second, tail: [First] },
    Empty,
}
"#,
            )
            .expect("当前源码声明");
        let mir = MirLowerer::lower_module_semantic(&hir);
        let sum = &mir.sum_types[0];
        let variant = &sum.variants[0];
        assert_eq!(sum.generics, hir.enums[0].generics);
        assert_eq!(variant.fields[0].name, "head");
        assert_eq!(variant.fields[1].name, "tail");
        assert_eq!(
            sum.instantiate_payload(variant, &[ValkyrieType::Boolean, ValkyrieType::Utf8]),
            Some(Some(ValkyrieType::Tuple(vec![
                ValkyrieType::Utf8,
                ValkyrieType::Array(Box::new(ValkyrieType::Boolean))
            ])))
        );
        assert!(
            sum.instantiate_payload(variant, &[ValkyrieType::Boolean])
                .is_none()
        );
        validate_semantic_module(&mir).expect("声明合同");
    }

    #[test]
    fn sum_declaration_rejects_duplicate_owner_binder_and_field() {
        let hir = ValkyrieCompiler::default()
            .compile_source("unite Parcel<T> { Arbitrary { value: T } }")
            .expect("当前源码声明");
        let mir = MirLowerer::lower_module_semantic(&hir);
        let mut duplicate_owner = mir.clone();
        duplicate_owner.sum_types.push(mir.sum_types[0].clone());
        assert_eq!(
            validate_semantic_module(&duplicate_owner).unwrap_err().code,
            "SMIR006"
        );
        let mut duplicate_binder = mir.clone();
        duplicate_binder.sum_types[0]
            .generics
            .push(mir.sum_types[0].generics[0].clone());
        assert_eq!(
            validate_semantic_module(&duplicate_binder)
                .unwrap_err()
                .code,
            "SMIR006"
        );
        let mut duplicate_field = mir.clone();
        duplicate_field.sum_types[0].variants[0]
            .fields
            .push(mir.sum_types[0].variants[0].fields[0].clone());
        assert_eq!(
            validate_semantic_module(&duplicate_field).unwrap_err().code,
            "SMIR006"
        );
    }

    #[test]
    fn sum_declaration_preserves_variant_result_refinement() {
        let mut hir = ValkyrieCompiler::default()
            .compile_source("unite Parcel<T> { Arbitrary { value: T } }")
            .expect("当前源码声明");
        let refined = ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("Parcel"))),
            vec![ValkyrieType::Boolean],
        );
        hir.enums[0].variants[0].result_type = Some(refined.clone());
        let mir = MirLowerer::lower_module_semantic(&hir);
        let sum = &mir.sum_types[0];
        assert_eq!(sum.variants[0].result_type, Some(refined.clone()));
        assert_eq!(
            sum.instantiate_result(&sum.variants[0], &[ValkyrieType::Utf8]),
            Some(refined)
        );
    }

    #[test]
    fn sum_operations_from_source_reject_missing_substitution_and_wrong_payload() {
        let mir = ValkyrieCompiler::default()
            .compile_source_to_mir(
                r#"
unite Envelope<T> { Present { value: T }, Absent }
micro wrap(value: utf8) -> Envelope<utf8> {
    return Present { value: value }
}
"#,
            )
            .expect("当前源码构造合同");
        validate_semantic_module(&mir).expect("正确实例化");
        let mut missing = mir.clone();
        let operation = missing
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions)
            .find_map(|instruction| match &mut instruction.kind {
                MirOperation::SumNew { type_args, .. } => Some(type_args),
                _ => None,
            })
            .expect("SumNew");
        operation.clear();
        assert_eq!(
            validate_semantic_module(&missing).unwrap_err().code,
            "SMIR006"
        );
        let mut wrong = mir;
        let operation = wrong
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions)
            .find_map(|instruction| match &mut instruction.kind {
                MirOperation::SumNew { payload_type, .. } => Some(payload_type),
                _ => None,
            })
            .expect("SumNew");
        *operation = Some(ValkyrieType::Boolean);
        assert_eq!(
            validate_semantic_module(&wrong).unwrap_err().code,
            "SMIR006"
        );
    }
}

/// variant 的 payload 与 GADT 结果类型属于声明事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirSumVariant {
    /// Semantic MIR 冻结的 variant 身份。
    pub id: nyar_types::VariantId,
    /// HIR 冻结的 variant 构造声明身份。
    pub declaration: Option<nyar_types::ItemId>,
    /// 声明名称。
    pub name: String,
    /// 源码判别值。
    pub tag: u32,
    /// 原始字段身份与语义类型。
    pub fields: Vec<super::MirField>,
    /// 显式 GADT 结果约束。
    pub result_type: Option<ValkyrieType>,
}

impl MirSumVariant {
    /// 多字段 payload 是类型化元组，不丢弃任何字段。
    pub fn payload_type(&self) -> Option<ValkyrieType> {
        match self.fields.as_slice() {
            [] => None,
            [field] => Some(field.ty.clone()),
            fields => Some(ValkyrieType::Tuple(
                fields.iter().map(|field| field.ty.clone()).collect(),
            )),
        }
    }
}

impl MirSumDeclaration {
    /// 完成声明结果或 variant refinement 的同一泛型代入。
    pub fn instantiate_result(
        &self,
        variant: &MirSumVariant,
        arguments: &[ValkyrieType],
    ) -> Option<ValkyrieType> {
        if self.generics.len() != arguments.len() {
            return None;
        }
        if let Some(result) = &variant.result_type {
            let substitutions = self
                .generics
                .iter()
                .zip(arguments)
                .map(|(generic, argument)| (generic.name.clone(), argument.clone()))
                .collect();
            return Some(crate::valkyrie::hir::overload::substitute_type_vars(
                result,
                &substitutions,
            ));
        }
        let owner = ValkyrieType::Named(Identifier::new(&self.name));
        Some(if arguments.is_empty() {
            owner
        } else {
            ValkyrieType::Apply(Box::new(owner), arguments.to_vec())
        })
    }
    /// 只按声明 binder 代入，不依据 variant 拼写选择实参位置。
    pub fn instantiate_payload(
        &self,
        variant: &MirSumVariant,
        arguments: &[ValkyrieType],
    ) -> Option<Option<ValkyrieType>> {
        if self.generics.len() != arguments.len() {
            return None;
        }
        let substitutions = self
            .generics
            .iter()
            .zip(arguments)
            .map(|(generic, argument)| (generic.name.clone(), argument.clone()))
            .collect();
        Some(
            variant
                .payload_type()
                .as_ref()
                .map(|ty| crate::valkyrie::hir::overload::substitute_type_vars(ty, &substitutions)),
        )
    }

    /// 向下投影目标布局；该结果不得作为语义 lowering 的输入。
    pub(crate) fn physical_layout(&self) -> nyar_types::SumTypeLayout {
        nyar_types::SumTypeLayout {
            nominal: self.nominal,
            name: self.name.clone(),
            is_unite: self.is_unite,
            tag_width: 4,
            variants: self
                .variants
                .iter()
                .map(|variant| nyar_types::SumVariantLayout {
                    id: variant.id,
                    name: variant.name.clone(),
                    tag: variant.tag,
                    payload_type: variant
                        .payload_type()
                        .as_ref()
                        .map(crate::frontend_contract::concretize_type_lossy),
                })
                .collect(),
        }
    }
}
