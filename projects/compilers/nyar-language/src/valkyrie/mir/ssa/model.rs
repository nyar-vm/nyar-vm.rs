//! Semantic MIR 的数据模型。
//!
//! 本模块只拥有模块、函数、块、指令、操作、终结符与诊断等纯结构，
//! 不包含 lowering 行为；`MirStruct` / `MirInstruction` 的只读访问器随类型保留。

use super::{MirSumDeclaration, struct_facts::resolve_self_type_with_owner, value_semantics, value_semantics::FlagsLayout};
use crate::valkyrie::semantic_closure::DeclarationKey;

use crate::types::{
    NamePath,
    hir::{HirPattern, ValkyrieType},
};
use nyar::SemanticFragment;
use nyar_types::{ItemInstanceId, VariantId};
use std::collections::BTreeMap;

/// `MIR` lowering 阶段产生的编译期诊断。
///
/// 当 lowering 过程中遇到可在编译期判定为无法处理的构造时，
/// 会向模块中追加一条诊断，最终由校验层转化为编译错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirDiagnostic {
    /// 模式匹配无法 lowering 到具体 `MIR` 指令。
    ///
    /// 触发场景包括：extractor 未 resolved、scrutinee 类型未知导致无法选择
    /// probe 策略、tuple/object 类型推断失败等。用户需补类型注解或 extractor
    /// 定义后才能继续 lowering。
    PatternLoweringFailed {
        /// 触发诊断的模式。
        pattern: HirPattern,
        /// 诊断原因描述。
        reason: String,
    },
    /// 表达式在当前 lowering 中不受支持。
    UnsupportedExpression {
        /// 触发诊断的源码区间。
        span: crate::SourceSpan,
        /// 不受支持的表达式种类描述。
        kind: String,
    },
    /// 操作数的类型事实缺失，不能用 `Unit` 或其他默认类型继续 lowering。
    UnresolvedValueType {
        /// 缺失类型事实的语义位置。
        context: String,
    },
    /// 语义 sum registry 中不存在 lowering 请求的 variant。
    UnresolvedVariantIdentity {
        /// 声明的 sum owner。
        sum_type: String,
        /// 请求的 variant 名称。
        variant: String,
    },
    /// 聚合操作缺少已冻结的名义实例身份。
    UnresolvedNominalIdentity {
        /// 仅用于诊断的表面类型名。
        type_name: String,
    },
    /// 字段操作缺少已冻结的字段身份。
    UnresolvedFieldIdentity {
        /// 仅用于诊断的字段名。
        field: String,
    },
    /// 完整依赖闭包冻结后仍无法绑定的 callable identity。
    UnresolvedCallableIdentity {
        /// 上游解析保留的限定名称，仅用于诊断。
        symbol: String,
    },
}

/// `SSA` 形式的 `MIR` 模块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirModule {
    /// 模块名。
    pub name: String,
    /// 模块内 lowering 得到的全部函数。
    pub functions: Vec<MirFunction>,
    /// 结构体定义，保留字段类型供后端继续推导 `TypeDef` / `Field` 信息。
    pub structs: Vec<MirStruct>,
    /// `using` 导入列表，记录模块依赖的外部命名空间与类型别名。
    pub imports: Vec<String>,
    /// Exact call contracts exported by resolved dependency modules.
    ///
    /// This is deliberately not derived from [`Self::imports`]: an import
    /// path proves neither that a symbol exists nor its call signature.
    /// Backend preparation may consume these contracts, but must not create
    /// new language semantics from its host ABI.
    pub external_calls: Vec<MirExternalCallContract>,
    /// Compiler 在 Semantic MIR 边界确定的公开导出合同。
    pub exports: Vec<MirExportContract>,
    /// Compiler 在 Semantic MIR 边界确定的入口合同。
    pub entries: Vec<MirEntryContract>,
    /// Compiler 在 Semantic MIR 边界确定的 callable 诊断符号表（`ItemInstanceId` → ABI 标签）。
    ///
    /// 语义查找必须以 instance / declaration 为主键；本表仅供诊断与外部 ABI 投影，
    /// 不得反向按字符串标签恢复 callable identity。
    pub callable_symbols: BTreeMap<ItemInstanceId, String>,
    /// Compiler 在 Semantic MIR 边界确定的类型 identity 表。
    /// Canonical producer 不得重新收集或按排序结果重编号。
    pub type_identities: BTreeMap<ValkyrieType, nyar_types::TypeId>,
    /// Value/reference aggregate inline layout plan.
    pub aggregate_layouts: value_semantics::AggregateLayoutPlan,
    /// Compiler 保留的 sum 声明、泛型 binder、字段类型与 variant refinement。
    /// nullable 类型不冒充名义 sum；物理布局不能成为本表的事实来源。
    pub sum_types: Vec<MirSumDeclaration>,
    /// `MIR` 阶段确定的 flags 名义布局；装配和后端不得从 HIR 重新收集。
    pub flags_types: Vec<FlagsLayout>,
    /// Compiler 在 Semantic MIR 边界确定的 singleton 生命周期合同。
    pub singleton_instances: Vec<nyar_types::SingletonInstancePlan>,
    /// 与本模块 Semantic MIR 同步生成的唯一分区语义合同。后续阶段不得从 HIR 旁路重新生成片段。
    pub semantic_fragments: Vec<SemanticFragment>,
    /// `MIR` lowering 过程中收集的编译期诊断，由校验层转化为编译错误。
    pub diagnostics: Vec<MirDiagnostic>,
}

/// 向 callable 诊断表写入条目；instance 与 ABI 标签均必须唯一。
pub(crate) fn register_callable_symbol_entry(
    table: &mut BTreeMap<ItemInstanceId, String>,
    instance: ItemInstanceId,
    symbol: String,
) -> Result<(), String> {
    if let Some(previous) = table.get(&instance) {
        if previous != &symbol {
            return Err(format!("callable identity `{instance}` 对应多个 ABI 标签"));
        }
        return Ok(());
    }
    if table.values().any(|existing| existing == &symbol) {
        return Err(format!("callable ABI 标签 `{symbol}` 重复"));
    }
    table.insert(instance, symbol);
    Ok(())
}

impl MirModule {
    /// 登记 callable 的 ABI 诊断标签；instance 与标签均必须唯一。
    pub(crate) fn register_callable_symbol(&mut self, instance: ItemInstanceId, symbol: String) -> Result<(), String> {
        register_callable_symbol_entry(&mut self.callable_symbols, instance, symbol)
    }

    pub(crate) fn contains_callable_instance(&self, instance: ItemInstanceId) -> bool {
        self.callable_symbols.contains_key(&instance)
            || self.functions.iter().any(|function| function.instance == Some(instance))
            || self.external_calls.iter().any(|contract| contract.instance == Some(instance))
    }
}

/// 由导入语义导出持有的静态解析调用合同。
///
/// 声明签名属于模块级 callable 合同，不复制到单条 Call 上；不得挂分派旁路。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirExternalCallContract {
    /// 闭包规划冻结的声明身份；host linker 只按此匹配合同。
    pub declaration_key: Option<DeclarationKey>,
    /// HIR 导出的原始声明身份；导入实例不得按导入顺序重新生成声明。
    pub declaration: Option<nyar_types::ItemId>,
    /// 实例化后的 callable 身份；声明存在但实例缺失即为合同错误。
    pub instance: Option<nyar_types::ItemInstanceId>,
    /// HIR 选择的精确源码身份。
    pub symbol: NamePath,
    /// 声明阶段形成的显式外部链接合同。
    pub link: nyar_types::ExternalImportLink,
    /// 语义导出声明的参数类型；禁止缺失后默认成空参数列表。
    pub parameter_types: Vec<ValkyrieType>,
    /// 语义导出声明的返回类型。
    pub return_type: ValkyrieType,
}

/// 已解析 callable 的公开导出合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirExportContract {
    /// HIR 已绑定的公开项实例，名称只能用于诊断和 ABI。
    pub instance: Option<ItemInstanceId>,
    /// 已解析的 callable 符号；Canonical producer 只用它查 Compiler identity。
    pub symbol: NamePath,
    /// 公开 ABI 名称。
    pub exported_name: String,
    /// 由声明合同确定的目标分区。
    pub partition: String,
}

/// 已解析 callable 的程序入口合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirEntryContract {
    /// HIR 已绑定的入口实例，不能由入口名称恢复。
    pub instance: Option<ItemInstanceId>,
    /// 已解析的 callable 符号。
    pub symbol: NamePath,
}

/// `MIR` 结构体定义。
/// 保留字段类型，供后端生成 `TypeDef`/`Field` 并推导布局。
/// `is_value_type` 区分值类型聚合与引用类型聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirStruct {
    /// Semantic MIR 中冻结的名义声明身份。
    pub nominal: nyar_types::NominalInstanceId,
    /// HIR 冻结的结构声明身份。
    pub declaration: Option<nyar_types::ItemId>,
    /// 结构体名称。
    pub name: String,
    /// 结构体命名空间（点分路径，例如 `core.text`），用于稳定符号与布局查找。
    pub namespace: String,
    /// 声明的泛型形参列表；非空表示开放泛型，不生成平台布局。
    pub generics: Vec<crate::types::hir::GenericType>,
    /// 字段列表。
    pub fields: Vec<MirField>,
    /// 是否为值类型：`true` 表示 `structure` 等按值布局的聚合；
    /// `false` 表示引用类型，例如 `class` 等按引用布局的聚合。
    pub is_value_type: bool,
    /// 已冻结的父类型声明身份；禁止再用短名索引继承链。
    pub parent_declarations: Vec<nyar_types::ItemId>,
}

impl MirStruct {
    /// 返回 `namespace.name` 形式的限定名；`namespace` 为空时仅返回名称。
    pub fn qualified_name(&self) -> String {
        if self.namespace.is_empty() { self.name.clone() } else { format!("{}.{}", self.namespace, self.name) }
    }

    /// 按 HIR 冻结的 `ItemId` 查找唯一聚合声明。禁止短名/限定名字符串回查。
    pub fn find_by_declaration<'a>(declarations: &'a [MirStruct], declaration: nyar_types::ItemId) -> Option<&'a MirStruct> {
        let matches: Vec<&MirStruct> = declarations.iter().filter(|item| item.declaration == Some(declaration)).collect();
        if matches.len() == 1 { Some(matches[0]) } else { None }
    }

    /// 按 `owner` 的泛型实参代入后返回字段类型；`owner` 或字段不匹配时返回 `None`。
    pub fn instantiate_field(&self, owner: &ValkyrieType, field: &str) -> Option<ValkyrieType> {
        let (declaration, arguments) = match owner {
            ValkyrieType::Nominal { declaration, arguments } => (*declaration, arguments.as_slice()),
            _ => return None,
        };
        if self.declaration != Some(declaration) || arguments.len() != self.generics.len() {
            return None;
        }
        let substitutions = self
            .generics
            .iter()
            .zip(arguments)
            .map(|(generic, argument)| Some((generic.binder?, argument.clone())))
            .collect::<Option<BTreeMap<_, _>>>()?;
        if substitutions.len() != self.generics.len() {
            return None;
        }
        let mut matching = self.fields.iter().filter(|candidate| candidate.name == field);
        let declared = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        let ty = crate::valkyrie::hir::overload::substitute_type_vars(&declared.ty, &substitutions);
        Some(resolve_self_type_with_owner(&ty, Some(owner)))
    }

    /// 按声明短名在已冻结的聚合表中查找（仅用于模式 lowering 的遗留表面名输入）。
    pub fn find_by_name<'a>(declarations: &'a [MirStruct], name: &str) -> Option<&'a MirStruct> {
        let matches: Vec<&MirStruct> = declarations.iter().filter(|item| item.name == name).collect();
        if matches.len() == 1 { Some(matches[0]) } else { None }
    }

    /// 在声明表与父链上查找字段类型；`owner` 提供泛型代入上下文。
    pub fn lookup_field_type<'a>(
        declarations: &'a [MirStruct],
        owner: &ValkyrieType,
        field_name: &str,
        visiting: &mut std::collections::BTreeSet<nyar_types::ItemId>,
    ) -> Option<ValkyrieType> {
        let declaration = owner.nominal_declaration()?;
        if !visiting.insert(declaration) {
            return None;
        }
        let structure = Self::find_by_declaration(declarations, declaration)?;
        if let Some(ty) = structure.instantiate_field(owner, field_name) {
            visiting.remove(&declaration);
            return Some(ty);
        }
        let inherited = structure.parent_declarations.iter().find_map(|parent| {
            let parent_owner = ValkyrieType::Nominal { declaration: *parent, arguments: Vec::new() };
            Self::lookup_field_type(declarations, &parent_owner, field_name, visiting)
        });
        visiting.remove(&declaration);
        inherited
    }

    /// 判断短名在聚合声明表中是否唯一存在（仅用于模式 lowering 的遗留表面名输入）。
    pub fn has_layout(declarations: &[MirStruct], name: &str) -> bool {
        Self::find_by_name(declarations, name).is_some()
    }

    /// 按已冻结的 `parent_declarations` 判断表面短名继承关系（仅用于模式 lowering）。
    pub fn inherits_from(declarations: &[MirStruct], actual_name: &str, expected_name: &str, visiting: &mut std::collections::BTreeSet<String>) -> bool {
        if actual_name == expected_name {
            return true;
        }
        if !visiting.insert(actual_name.to_string()) {
            return false;
        }
        let Some(structure) = Self::find_by_name(declarations, actual_name) else {
            visiting.remove(actual_name);
            return false;
        };
        let found = structure.parent_declarations.iter().any(|parent| {
            Self::find_by_declaration(declarations, *parent).is_some_and(|parent_struct| {
                parent_struct.name == expected_name
                    || Self::inherits_from(declarations, parent_struct.name.as_str(), expected_name, visiting)
            })
        });
        visiting.remove(actual_name);
        found
    }
}

/// `MIR` 结构体字段定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirField {
    /// Semantic MIR 中冻结的字段身份。
    pub id: nyar_types::FieldId,
    /// 字段名。
    pub name: String,
    /// 字段静态类型。
    pub ty: ValkyrieType,
}

/// `SSA` 形式的 `MIR` 函数定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFunction {
    /// 诊断与 ABI 投影用显示名；语义身份以 `declaration` / `instance` 为准。
    pub display_symbol: String,
    /// 闭包规划冻结的声明身份。
    pub declaration_key: Option<DeclarationKey>,
    /// HIR 声明对应的 Compiler ItemId；MIR 不再按符号重新铸造实例身份。
    pub declaration: Option<nyar_types::ItemId>,
    /// 由 Compiler 独立实例化阶段分配，不能由声明编号转换。
    pub instance: Option<nyar_types::ItemInstanceId>,
    /// 函数返回类型，用于后端判断调用是否返回 `void`。
    pub return_type: ValkyrieType,
    /// 函数参数类型列表，用于后端生成调用约定与方法签名。
    pub param_types: Vec<ValkyrieType>,
    /// `SSA` 值的静态类型表，供调度校验与后续 lowering 使用。
    pub value_types: BTreeMap<MirValueRef, ValkyrieType>,
    /// 函数入口基本块引用。
    pub entry: MirBlockRef,
    /// 函数内全部 SSA 值。
    pub values: Vec<MirValue>,
    /// 函数内全部基本块。
    pub blocks: Vec<MirBlock>,
}

/// `MIR` 基本块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirBlock {
    /// 基本块引用。
    pub id: MirBlockRef,
    /// 调试用块标签。
    pub label: String,
    /// 块入口的 SSA 参数。
    pub parameters: Vec<MirValueRef>,
    /// 块内顺序执行的指令。
    pub instructions: Vec<MirInstruction>,
    /// 块终结符，决定后继控制流。
    pub terminator: MirTerminator,
}

/// `MIR` SSA 值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirValue {
    /// 值引用。
    pub id: MirValueRef,
    /// 值的来源。
    pub origin: MirValueOrigin,
}

/// `catch / resume` 上下文中下推的 continuation 元数据，承载 resume 跳转与类型信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirContinuation {
    /// 该 continuation 对应的 handler dispatch block。
    pub dispatch_block: MirBlockRef,
    /// Continuation 恢复时跳转的 resume block。
    pub resume_target: MirBlockRef,
    /// Resume block 入口参数对应的 SSA 值。
    pub resume_parameter: MirValueRef,
    /// Resume 参数静态类型，未确定时为 `None`。
    pub resume_parameter_type: Option<ValkyrieType>,
    /// Handler 退出块。
    pub handler_exit: MirBlockRef,
    /// 触发该 continuation 的用户 effect 载体类型。
    ///
    /// 仅在 `MirEffectKind::Raise` 且被 raise 值具备可识别的载体类型时填入；
    /// 用于查询 `Effectful::Resume` 关联类型。其他 effect 或纯异常路径保持 `None`。
    pub carrier_type: Option<ValkyrieType>,
}

/// `match / case` 链路元数据，描述分发、汇合与各 arm 结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirCaseChain {
    /// 进入 case-like lowering 的分发块。
    pub dispatch_block: MirBlockRef,
    /// 第一条 arm 的入口块。
    pub first_arm: MirBlockRef,
    /// 全部 arm 未匹配时跳入的块。
    pub no_match_block: MirBlockRef,
    /// Case-like 控制流最终汇入的 exit 块。
    pub exit_block: MirBlockRef,
    /// 是否为值语义 `match`。
    pub produce_value: bool,
    /// 各个 arm 的显式链路信息。
    pub arms: Vec<MirCaseArm>,
}

/// 单个 `case / match arm` 的控制流元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirCaseArm {
    /// Arm 入口块。
    pub entry_block: MirBlockRef,
    /// 可选的 pattern check 块。
    pub check_block: Option<MirBlockRef>,
    /// 可选的 guard 求值块。
    pub guard_block: Option<MirBlockRef>,
    /// Arm body 实际执行块。
    pub body_block: MirBlockRef,
    /// Pattern / guard 失败后跳向的下一目标。
    pub next_arm_target: MirBlockRef,
    /// 正常完成后汇入的 exit 目标。
    pub exit_target: MirBlockRef,
    /// `fallthrough` 允许时应跳入的下一 arm 入口。
    pub fallthrough_target: Option<MirBlockRef>,
}

/// `yield / await / block / raise` 等 suspend 点的元数据，记录状态机改写所需的全部信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirSuspendPoint {
    /// 状态机编号，由 lowering 分配且全局唯一。
    pub state_id: u32,
    /// 该挂起点对应的 effect 类别。
    pub effect: MirEffectKind,
    /// 挂起所在 block。
    pub suspend_block: MirBlockRef,
    /// 恢复时跳转到的 block。
    pub resume_target: MirBlockRef,
    /// 恢复参数数量。
    pub resume_parameter_count: usize,
    /// 触发该挂起点的 payload 静态类型（即被 raise/yield 的值的类型）。
    pub payload_type: Option<ValkyrieType>,
    /// 跨挂起点需要 spill 到 frame 的 SSA 值。
    pub spill_candidates: Vec<MirValueRef>,
    /// 若该挂起点位于 catch arm body 内，对应 continuation 的索引。
    pub continuation_index: Option<usize>,
    /// 触发该挂起点的用户 effect 载体类型。
    ///
    /// 仅 `MirEffectKind::Raise` 且被 raise 值具备可识别载体类型时填入；
    /// 用于查询 `Effectful::Resume` 关联类型以推断 resume 参数类型。
    /// 其他 effect 类别或纯异常路径保持 `None`。
    pub carrier_type: Option<ValkyrieType>,
}

/// 单个 suspend 点对应的 frame layout，描述跨挂起点需要保留的槽位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFrameLayout {
    /// 该 layout 所属的状态机编号。
    pub state_id: u32,
    /// 该 layout 所属的 effect 类别。
    pub effect: MirEffectKind,
    /// 恢复时跳转到的 block。
    pub resume_target: MirBlockRef,
    /// Frame 中需要保留的 spill 槽位。
    pub slots: Vec<MirFrameSlot>,
    /// 触发该 frame 的用户 effect 载体类型（仅 `Raise` 时填入）。
    pub carrier_type: Option<ValkyrieType>,
}

/// Frame 中的单个 spill 槽位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFrameSlot {
    /// Frame 中的稳定槽位序号。
    pub slot_index: usize,
    /// 被保存的 SSA 值。
    pub value: MirValueRef,
    /// 当前已知的槽位静态类型。
    pub value_type: Option<ValkyrieType>,
}

/// `MIR` SSA 值引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirValueRef(pub u32);

/// `MIR` 基本块引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirBlockRef(pub u32);

/// `MIR` 值的来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirValueOrigin {
    /// 函数形参。
    Parameter {
        /// 形参序号。
        index: usize,
        /// 形参名。
        name: String,
    },
    /// 基本块的 SSA 参数。
    BlockParameter {
        /// 声明该参数的块。
        block: MirBlockRef,
        /// 参数名。
        name: String,
    },
    /// `let` 绑定引入的局部值。
    LetBinding {
        /// 绑定名。
        name: String,
    },
    /// Pattern `mut pat`：可变借用绑定（类似 Rust `ref mut`）。
    MutRefBinding {
        /// 绑定名。
        name: String,
    },
    /// Pattern `pin mut pat`：固定的可变借用绑定。
    PinMutRefBinding {
        /// 绑定名。
        name: String,
    },
    /// 字面量产生的临时值。
    Literal,
    /// 路径引用产生的值。
    Path,
    /// 调用结果产生的值。
    CallResult,
    /// 编译器合成的临时值。
    Temporary,
}

/// 声明期无法闭合、待单态化按完整代入选择 witness 的调用要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirCallRequirement {
    /// 待代入的接收者类型（通常是泛型 binder）。
    pub subject: ValkyrieType,
    /// 成员选择名；运算符使用声明侧成员名。
    pub member: crate::Identifier,
    /// 已声明 bound 的 trait 身份；无 trait 的成员/运算符为 `None`。
    pub trait_declaration: Option<nyar_types::ItemId>,
}

/// `MIR` 操作数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperand {
    /// 引用一个 SSA 值。
    Value(MirValueRef),
    /// 内联常量。
    Constant(MirConstant),
    /// Compiler 在依赖闭包冻结后分配的 callable identity。
    Callable(ItemInstanceId),
    /// 待单态化解析的成员要求；不得进入 Canonical 成功值。
    Requirement(MirCallRequirement),
    /// HIR/链接阶段尚未完成 identity 冻结的结构化名称；不得进入 Canonical 成功值。
    Symbol(NamePath),
}

/// `MIR` 常量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirConstant {
    /// 有符号 64 位整型字面量。
    Int(i64),
    /// 64 位浮点字面量。
    Float64(ordered_float::OrderedFloat<f64>),
    /// 布尔字面量。
    Bool(bool),
    /// UTF-8 text literal. The language has no encoding-neutral string type.
    Utf8(String),
    /// UTF-16 text literal. This is a language-level encoding tag, not a
    /// request to use any backend's default string representation.
    Utf16(String),
    /// 单元类型常量。
    Unit,
}

/// `MIR` effect terminator category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirEffectKind {
    /// `raise expr`
    Raise,
    /// `yield expr`
    Yield,
    /// `yield from expr`
    DelegateYield,
    /// `expr.await`
    Await,
    /// `expr.awake`
    AsyncSpawn,
    /// `expr.block`
    AsyncBlock,
}

/// `MIR` 指令信封。
///
/// 字段 `kind` 为过渡名；语义角色是 **operation**。
/// 结果类型只住在值表（`MirValueDefinition`），不在信封上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirInstruction {
    /// 稳定指令身份（规划 / 校验键）。
    pub id: nyar_types::InstructionId,
    /// 本指令产生的 SSA 结果（0..n）。
    pub results: Vec<MirValueRef>,
    /// 操作载荷（仅 `MirOperation` — 不含结果类型 / 布局）。
    pub kind: MirOperation,
    /// 源码或合成 provenance。
    pub provenance: nyar_types::ProvenanceId,
}

impl MirInstruction {
    /// 借用操作载荷。
    pub fn operation(&self) -> &MirOperation {
        &self.kind
    }

    /// Wrap an operation in a synthetic envelope (tests / transitional sites).
    ///
    /// Prefer [`MirBuilder::push_instruction`] so InstructionId is dense and unique.
    pub fn from_operation(operation: MirOperation) -> Self {
        Self {
            id: nyar_types::InstructionId::from_index(0).expect("stub InstructionId"),
            results: Vec::new(),
            kind: operation,
            provenance: nyar_types::ProvenanceId::from_index(0).expect("synthetic ProvenanceId"),
        }
    }

    /// Wrap an operation that produces SSA results.
    pub fn from_operation_with_results(operation: MirOperation, results: Vec<MirValueRef>) -> Self {
        Self {
            id: nyar_types::InstructionId::from_index(0).expect("stub InstructionId"),
            results,
            kind: operation,
            provenance: nyar_types::ProvenanceId::from_index(0).expect("synthetic ProvenanceId"),
        }
    }
}

/// `MIR` 指令种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperation {
    /// 载入常量（可附静态类型）。
    LoadConstant {
        /// 常量值。
        constant: MirConstant,
        /// 可选静态类型；缺失时由操作数推断。
        ty: Option<ValkyrieType>,
    },
    /// 载入符号路径。
    LoadSymbol {
        /// 符号路径。
        path: NamePath,
    },
    /// 值拷贝。
    Copy {
        /// 拷贝来源。
        source: MirOperand,
    },
    /// 语言层类型转换（`expr as T`）。结果 SSA 类型必须是 `ty`，不得伪装成 [`Self::Copy`]。
    Cast {
        /// 待转换的操作数。
        source: MirOperand,
        /// 目标类型。
        ty: ValkyrieType,
    },
    /// 将值写入具名局部变量（`let` 绑定）。
    ///
    /// `name` 为变量名，`value` 为待写入的操作数。
    /// `ty` 为可选静态类型，供后端槽位分配与调试信息使用；
    /// 若缺失，后端可回退到操作数已推断类型或 `header` 中的类型线索。
    StoreVar {
        /// 变量名。
        name: String,
        /// 待写入的操作数。
        value: MirOperand,
        /// 可选静态类型；来自 `let` 注解时优先使用该 `ty`。
        ty: Option<ValkyrieType>,
    },
    /// 调用指令，统一语义为 `Invoke(callee, arguments)`。
    Call {
        /// 可调用操作数。过渡期日后成为 `Invoke.callee`（`Item` | `Value`）。
        /// 不得携带 dispatch / witness / intrinsic / generic 旁路。
        callee: MirOperand,
        /// 语义实参（有 receiver 时显式为第 0 个）。
        arguments: Vec<MirOperand>,
    },
    /// 构造名义聚合。物理 StorageKind/LayoutId 属于 RepresentationPlan。
    StructNew {
        /// 目标名义实例身份。
        nominal: nyar_types::NominalInstanceId,
        /// 按字段身份给出的初始化值。
        fields: Vec<(nyar_types::FieldId, MirOperand)>,
    },
    /// 构造元组值。
    TupleNew {
        /// 元组元素值。
        fields: Vec<MirOperand>,
    },
    /// 聚合值整体拷贝。
    AggregateCopy {
        /// 拷贝来源。
        source: MirOperand,
        /// 拷贝目标。
        dest: MirOperand,
    },
    /// 读取聚合字段值。
    FieldGet {
        /// 目标对象。
        object: MirOperand,
        /// 字段身份。
        field: nyar_types::FieldId,
    },
    /// 写入聚合字段值。
    FieldSet {
        /// 目标对象。
        object: MirOperand,
        /// 字段身份。
        field: nyar_types::FieldId,
        /// 待写入的值。
        value: MirOperand,
    },
    /// Construct a value of an explicitly declared nominal sum variant.
    SumNew {
        /// 目标名义实例身份。
        nominal: nyar_types::NominalInstanceId,
        /// 完成代入后的实参，供 Semantic MIR 校验 payload 合同。
        type_args: Vec<ValkyrieType>,
        /// 目标 variant 身份。
        variant: VariantId,
        /// 可选 payload 静态类型。
        payload_type: Option<ValkyrieType>,
        /// 可选 payload 操作数。
        payload: Option<MirOperand>,
    },
    /// Extract a payload from an explicitly identified nominal sum variant.
    /// This must not be represented as a field named `value`, `error`, or
    /// `payload`: those spellings carry no language semantics by themselves.
    SumPayloadGet {
        /// 目标名义实例身份。
        nominal: nyar_types::NominalInstanceId,
        /// 完成代入后的实参，供 Semantic MIR 校验 payload 合同。
        type_args: Vec<ValkyrieType>,
        /// 目标 variant 身份。
        variant: VariantId,
        /// payload 的静态类型。
        payload_type: ValkyrieType,
        /// 被解包的 sum 值。
        object: MirOperand,
    },
    /// Test whether a sum value is the given declared variant (`case Variant:`).
    /// Carries the same NominalInstanceKey as [`Self::SumNew`] / [`Self::SumPayloadGet`].
    SumVariantIs {
        /// 目标名义实例身份。
        nominal: nyar_types::NominalInstanceId,
        /// 完成代入后的实参，供 Semantic MIR 校验实例类型。
        type_args: Vec<ValkyrieType>,
        /// 待判定的 variant 身份。
        variant: VariantId,
        /// 被检查的 sum 值。
        object: MirOperand,
    },
    /// 模式探测指令，用于 handler/case dispatch 的匹配检查。
    PatternMatch {
        /// 被匹配的值（scrutinee）。
        value: MirOperand,
        /// 待检查的模式。
        pattern: HirPattern,
    },
    /// 按运行时长度 + 规定初始化构造语言数组。
    ///
    /// 定长与运行时长度由 `array_type`（完整数组类型 / `TypeId` 过渡替身）表达，
    /// **不是**单独指令。除非 valkyrie-2020 定义可验证的不可观察初始化阶段，
    /// 不得发明 `Uninitialized`。物理堆 / 内联 / GC 载体属于 RepresentationPlan。
    ArrayNew {
        /// 完整数组类型（`Array<T>` / `FixedArray<T,N>` …），非仅元素类型。
        array_type: ValkyrieType,
        /// 运行时长度操作数。
        length: MirOperand,
        /// 槽位初始化方式。
        initialization: ArrayInitialization,
    },
    /// 由完整元素序列构造语言数组。
    ///
    /// 取代旧的 `FixedArrayNew` + `ArrayLiteral` 分裂（类型类 × 初始化风格 × 表面语法）。
    /// 源码「字面量」不是 Semantic MIR 范畴。定长数组由 M2 按 `array_type` 中的长度
    /// 核对 `elements.len()`。
    ArrayFromElements {
        /// 完整数组类型；元素类型与定长来自类型表。
        array_type: ValkyrieType,
        /// 源元素序列。
        elements: Vec<MirOperand>,
    },
    /// 读取数组元素。
    ArrayGet {
        /// 目标数组。
        array: MirOperand,
        /// 下标操作数。
        index: MirOperand,
    },
    /// 写入数组元素。
    ArraySet {
        /// 目标数组。
        array: MirOperand,
        /// 下标操作数。
        index: MirOperand,
        /// 待写入的值。
        value: MirOperand,
    },
    /// 读取数组长度。
    ArrayLength {
        /// 目标数组。
        array: MirOperand,
    },
    /// 向可增长数组末尾追加元素；返回数组引用（语言 `push` intrinsic）。
    ArrayPush {
        /// 目标数组。
        array: MirOperand,
        /// 追加的元素值。
        value: MirOperand,
    },
    /// 检测可空值为 null（`builtin.null.is_null`）。
    NullIsNull {
        /// 待检测的可空值。
        value: MirOperand,
    },
    /// 解包非 null 可空载荷（`builtin.null.unwrap`）。
    NullUnwrap {
        /// 待解包的可空值。
        value: MirOperand,
    },
}

/// How [`MirOperation::ArrayNew`] initializes slots (must be defined by valkyrie-2020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayInitialization {
    /// Language-defined default for the element type (not host CLR/JVM null invent).
    Default,
    /// Fill every slot with this value (evaluation order is CFG/SSA, not this enum).
    Fill(MirOperand),
}

/// `MIR` 基本块终结符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTerminator {
    /// 从函数返回，可携带返回值。
    Return {
        /// 可选返回值操作数。
        value: Option<MirOperand>,
    },
    /// 无条件跳转到目标块，并传递块参数。
    Jump {
        /// 目标块引用。
        target: MirBlockRef,
        /// 传给目标块参数的实参。
        arguments: Vec<MirOperand>,
    },
    /// 按条件在 then / else 目标间分支。
    Branch {
        /// 条件操作数。
        condition: MirOperand,
        /// 条件为真时的目标块。
        then_target: MirBlockRef,
        /// 条件为假时的目标块。
        else_target: MirBlockRef,
    },
    /// 执行一次 effect 并按 resume 目标继续。
    PerformEffect {
        /// effect 类别。
        effect: MirEffectKind,
        /// 可选 payload 操作数。
        payload: Option<MirOperand>,
        /// 恢复时跳转的目标块。
        resume_target: MirBlockRef,
    },
    /// 状态机分发：按 `switch state` 跳转到 resume / entry 目标。
    StateDispatch {
        /// 当前状态值。
        state: MirValueRef,
        /// 状态号到目标块的映射。
        cases: Vec<(u32, MirBlockRef)>,
        /// 无匹配状态时的默认目标块。
        default_target: MirBlockRef,
    },
    /// 将控制权交还 runtime，并携带 `resume_state` 供后续 dispatch 恢复。
    YieldToRuntime {
        /// effect 类别。
        effect: MirEffectKind,
        /// 可选 payload 操作数。
        payload: Option<MirOperand>,
        /// 恢复用的状态号。
        resume_state: u32,
    },
    /// 不可达终结符。
    Unreachable,
}
