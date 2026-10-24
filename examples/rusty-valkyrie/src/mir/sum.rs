//! Semantic MIR 的 sum 声明合同；不持有物理 tag 宽度或目标布局。

use std::collections::BTreeMap;

use crate::types::hir::{GenericType, ValkyrieType};

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
            fields => Some(ValkyrieType::Tuple(fields.iter().map(|field| field.ty.clone()).collect())),
        }
    }
}

impl MirSumDeclaration {
    /// 按 HIR 冻结的 `ItemId` 查找唯一 sum 声明。禁止短名/限定名字符串回查。
    pub fn find_by_declaration<'a>(declarations: &'a [Self], declaration: nyar_types::ItemId) -> Option<&'a Self> {
        let matches: Vec<&Self> = declarations.iter().filter(|item| item.declaration == Some(declaration)).collect();
        if matches.len() == 1 { Some(matches[0]) } else { None }
    }

    /// 完成声明结果或 variant refinement 的同一泛型代入。
    pub fn instantiate_result(&self, variant: &MirSumVariant, arguments: &[ValkyrieType]) -> Option<ValkyrieType> {
        if self.generics.len() != arguments.len() {
            return None;
        }
        if let Some(result) = &variant.result_type {
            let substitutions = self
                .generics
                .iter()
                .zip(arguments)
                .map(|(generic, argument)| Some((generic.binder?, argument.clone())))
                .collect::<Option<BTreeMap<_, _>>>()?;
            return Some(crate::hir::overload::substitute_type_vars(result, &substitutions));
        }
        let declaration = self.declaration?;
        Some(ValkyrieType::Nominal { declaration, arguments: arguments.to_vec() })
    }
    /// 只按声明 binder 代入，不依据 variant 拼写选择实参位置。
    pub fn instantiate_payload(&self, variant: &MirSumVariant, arguments: &[ValkyrieType]) -> Option<Option<ValkyrieType>> {
        if self.generics.len() != arguments.len() {
            return None;
        }
        let substitutions = self
            .generics
            .iter()
            .zip(arguments)
            .map(|(generic, argument)| Some((generic.binder?, argument.clone())))
            .collect::<Option<BTreeMap<_, _>>>()?;
        Some(variant.payload_type().as_ref().map(|ty| crate::hir::overload::substitute_type_vars(ty, &substitutions)))
    }

    /// 向下投影目标布局；该结果不得作为语义 lowering 的输入。
    ///
    /// 开放泛型声明不携带未代入 payload（避免 binder 进入布局）；
    /// 具体 `Nominal{ItemId, args}` 实例的布局由 Canonical 注册。
    /// `SumVariantLayout.payload_type` 仍为 `NyarType`；语义身份以 `VariantRecord` 的 `TypeId` 为准。
    pub(crate) fn physical_layout(&self) -> Result<nyar_types::SumTypeLayout, crate::frontend_contract::ConcretizeError> {
        Ok(nyar_types::SumTypeLayout {
            nominal: self.nominal,
            name: self.name.clone(),
            is_unite: self.is_unite,
            tag_width: 4,
            variants: self
                .variants
                .iter()
                .map(|variant| {
                    let payload_type = if self.generics.is_empty() {
                        variant.payload_type().as_ref().map(crate::frontend_contract::concretize_type).transpose()?
                    }
                    else {
                        None
                    };
                    Ok(nyar_types::SumVariantLayout { id: variant.id, name: variant.name.clone(), tag: variant.tag, payload_type })
                })
                .collect::<Result<_, crate::frontend_contract::ConcretizeError>>()?,
        })
    }
}
