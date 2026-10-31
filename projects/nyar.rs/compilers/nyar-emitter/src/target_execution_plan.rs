//! Compiler 成功载荷到目标私有执行计划的唯一边界。

use std::collections::{BTreeMap, BTreeSet};

use crate::{EmitterError, Result};
use nyar::{ExternalImportLink, QualifiedName};
use nyar_types::{
    CanonicalArrayInitialization, CanonicalCallee, CanonicalConstant, CanonicalOperation, CanonicalProgram, CanonicalTerminator,
    CanonicalTypeKind, CompiledProgram, Constant, Instruction, InstructionKind, ItemInstanceId, LayoutId, NyarFunctionType, NyarType, Operand,
    Terminator, Value, ValueOrigin,
    layout_choice::{InvokeLowering, ValueRepresentation},
};

use crate::{
    backend_plan_views::{ExecutableFunction, FunctionView, SuspendMetadataView},
    contracts::{Block, BlockRef, ValueRef},
    target_operator::{TargetOperandStep, TargetOperandType, TargetOperatorBinding, TargetOperatorPlan, TargetOperatorRecipe},
};

/// 已完成 callable、类型、CFG 和表示合同绑定的目标私有计划。
#[derive(Debug, Clone, Default)]
pub struct TargetExecutionPlan {
    module_name: String,
    fragment_id: nyar::Identifier,
    exported_operations: Vec<ItemInstanceId>,
    required_capabilities: Vec<nyar::CapabilityTag>,
    theory_bundle: nyar::TheoryBundle,
    entry_operation: Option<ItemInstanceId>,
    wasm_export_names: BTreeMap<ItemInstanceId, String>,
    external_import_links: BTreeMap<ItemInstanceId, nyar::ExternalImportLink>,
    external_call_edges: Vec<nyar_types::CanonicalExternalCallEdge>,
    internal_call_edges: Vec<nyar_types::CanonicalCallEdge>,
    aggregate_layouts: nyar_types::AggregateLayoutPlan,
    aggregate_layout_by_nominal: BTreeMap<nyar_types::NominalInstanceId, nyar_types::LayoutId>,
    aggregate_layout_by_field: BTreeMap<nyar_types::FieldId, (nyar_types::LayoutId, u32)>,
    aggregate_layout_by_type: BTreeMap<nyar_types::TypeId, nyar_types::LayoutId>,
    aggregate_operations: Vec<AggregateOperationPlan>,
    flags_types: Vec<nyar_types::FlagsLayout>,
    singleton_instances: Vec<nyar_types::SingletonInstancePlan>,
    functions: BTreeMap<ItemInstanceId, ExecutableFunction>,
    library_export_abis: BTreeMap<ItemInstanceId, LibraryExportAbi>,
    abi_names: BTreeMap<ItemInstanceId, QualifiedName>,
    imports: BTreeMap<ItemInstanceId, BackendImport>,
    sum_reps: BTreeMap<nyar_types::NominalInstanceId, nyar_types::layout_choice::SumRepresentation>,
}

/// 已由 Compiler 绑定身份、签名和链接合同的导入。
#[derive(Debug, Clone)]
pub struct BackendImport {
    /// Canonical 导入索引。
    pub index: nyar_types::ImportIndex,
    /// 外部链接合同。
    pub link: nyar_types::ExternalImportLink,
    /// 参数类型列表。
    pub parameter_types: Vec<NyarType>,
    /// 返回类型。
    pub return_type: NyarType,
    /// 计划冻结的目标 operator 绑定；`None` 表示该导入不是目标 operator 绑定。
    pub target_operator: Option<TargetOperatorBinding>,
}

/// 聚合 helper operation 的物理种类。
///
/// 本类型是 [`nyar_types::LibraryGlueOperation`] 的别名，不保留独立枚举：operation
/// 身份来自语言声明标注 `[library_glue(construct|push|length|at)]`，由前端冻结进
/// canonical 片段，属于 canonical 事实而非目标私有事实。目标层只消费它，不得再维护
/// 第二套枚举与之同步。具体参数的语义由聚合布局与 ABI 结构事实决定，名称字符串
/// 仅用于产物合同序列化，由别名类型自带的 `as_str` 提供。
pub use nyar_types::LibraryGlueOperation as AggregateOperation;

/// 聚合 helper operation 在 Wasm 层的参数或返回载体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AggregateOperationCarrier {
    /// `i32` 载体。
    I32,
    /// `i64` 载体。
    I64,
    /// `f32` 载体。
    F32,
    /// `f64` 载体。
    F64,
    /// 宿主持有的聚合句柄。
    ///
    /// 与 `AbiCarrier::Handle` 是同一物理载体：库导出 ABI 把具名聚合与容器一律交给
    /// 宿主作为不透明对象（Wasm GC 引用在 JS 侧就是不可构造、只能传递的对象）。
    /// glue operation 的聚合槽位必须用它，不能写成 `i32`——`i32` 是线性内存句柄
    /// 时代的残留，与导出函数形参的真实载体不一致。
    Handle,
    /// 无返回值。
    Void,
}

impl AggregateOperationCarrier {
    /// 返回 Wasm 载体名称（lowercase）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Handle => "handle",
            Self::Void => "void",
        }
    }
}

/// 聚合 helper operation 的物理生产者。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AggregateOperationProducer {
    /// plan 中真实函数体实例；其 plan 导出名必须等于 operation 的 `export_name`。
    FunctionBody(ItemInstanceId),
}

impl AggregateOperationProducer {
    /// 返回承载该 operation 的函数体实例。
    pub fn instance(self) -> ItemInstanceId {
        match self {
            Self::FunctionBody(instance) => instance,
        }
    }
}

/// plan 冻结的聚合 helper operation。
///
/// 一个 operation 绑定聚合布局身份、operation 种类、参数/返回载体、真实 Wasm 导出名
/// 与唯一物理生产者。`glue_entries` 只能消费这里的 operation，backend 不得自行合成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateOperationPlan {
    /// 目标聚合布局身份。
    pub layout_id: LayoutId,
    /// operation 种类。
    pub operation: AggregateOperation,
    /// 按顺序排列的参数载体。
    pub param_carriers: Vec<AggregateOperationCarrier>,
    /// 返回载体；无返回值时为 [`AggregateOperationCarrier::Void`]。
    pub return_carrier: AggregateOperationCarrier,
    /// 真实 Wasm 导出名；必须与生产者在 plan 中的导出名一致。
    pub export_name: String,
    /// 物理生产者。
    pub producer: AggregateOperationProducer,
}

/// 库导出参数或返回值的 Wasm 物理载体。
///
/// 载体是目标执行计划的冻结事实；backend 只能序列化这里的值，不得再从
/// `WasmAbiType` schema 现场推导载体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AbiCarrier {
    /// `i32` 槽位。
    I32,
    /// `i64` 槽位。
    I64,
    /// `f32` 槽位。
    F32,
    /// `f64` 槽位。
    F64,
    /// 宿主句柄（数组 / Option / Tuple / 具名聚合）。
    Handle,
    /// 无返回值。
    Void,
}

impl AbiCarrier {
    /// 返回载体名称（lowercase）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Handle => "handle",
            Self::Void => "void",
        }
    }
}

/// 一个库导出实例的冻结 ABI 载体合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportAbi {
    /// 按声明顺序排列的参数载体。
    pub params: Vec<AbiCarrier>,
    /// 返回值载体；无返回值时为 [`AbiCarrier::Void`]。
    pub returns: AbiCarrier,
}

impl TargetExecutionPlan {
    pub(crate) fn from_entry_function(function: ExecutableFunction) -> Result<Self> {
        let entry = ItemInstanceId::from_index(0).expect("测试入口身份");
        let name = QualifiedName::new(vec![nyar::Identifier::new("main")]);
        let abi = freeze_library_export_abi(&function)?;
        Ok(Self {
            entry_operation: Some(entry),
            functions: BTreeMap::from([(entry, function)]),
            library_export_abis: BTreeMap::from([(entry, abi)]),
            abi_names: BTreeMap::from([(entry, name)]),
            ..Self::default()
        })
    }

    /// 从 ABI 名称表构造测试用目标私有计划。
    ///
    /// 导出名尚未登记，因此库导出 ABI 载体合同也尚未冻结；测试在
    /// [`Self::testing_wasm_export_names_mut`] 登记导出名后必须调用
    /// [`Self::testing_freeze_library_export_abis`] 补齐冻结，不能依赖后端猜测载体。
    #[doc(hidden)]
    pub fn from_functions(functions: BTreeMap<QualifiedName, ExecutableFunction>) -> Self {
        let mut plan = Self::default();
        for (index, (name, function)) in functions.into_iter().enumerate() {
            let instance = ItemInstanceId::from_index(u32::try_from(index).expect("测试函数身份溢出")).expect("测试函数身份溢出");
            plan.abi_names.insert(instance, name);
            plan.functions.insert(instance, function);
        }
        plan
    }

    /// 返回全部可执行函数表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_functions(&self) -> &BTreeMap<ItemInstanceId, ExecutableFunction> {
        &self.functions
    }

    /// 返回可写函数表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_functions_mut(&mut self) -> &mut BTreeMap<ItemInstanceId, ExecutableFunction> {
        &mut self.functions
    }

    /// 返回可写 wasm 导出名表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_wasm_export_names_mut(&mut self) -> &mut BTreeMap<ItemInstanceId, String> {
        &mut self.wasm_export_names
    }

    /// 返回可写导出实例表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_exported_operations_mut(&mut self) -> &mut Vec<ItemInstanceId> {
        &mut self.exported_operations
    }

    /// 设置模块名（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_module_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.module_name = name.into();
        self
    }

    /// 设置入口操作（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_entry_operation(&mut self, name: QualifiedName) -> &mut Self {
        if let Some((instance, _)) = self.abi_names.iter().find(|(_, abi)| **abi == name) {
            self.entry_operation = Some(*instance);
        }
        self
    }

    /// 设置聚合布局计划（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_aggregate_layouts(&mut self, layouts: nyar_types::AggregateLayoutPlan) -> &mut Self {
        self.aggregate_layouts = layouts;
        self
    }

    /// 登记 plan 冻结的聚合 helper operation（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_register_aggregate_operation(&mut self, operation: AggregateOperationPlan) -> &mut Self {
        self.aggregate_operations.push(operation);
        self
    }

    /// 登记 nominal → layout，并写入 `aggregate_layouts`（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_register_nominal_layout(
        &mut self,
        nominal: nyar_types::NominalInstanceId,
        layout: nyar_types::AggregateLayout,
    ) -> &mut Self {
        let layout_id = layout.id;
        if !self.aggregate_layouts.layouts.iter().any(|item| item.id == layout_id) {
            self.aggregate_layouts.layouts.push(layout);
        }
        self.aggregate_layout_by_nominal.insert(nominal, layout_id);
        self
    }

    /// 登记字段布局索引（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_register_field(&mut self, field: nyar_types::FieldId, layout_id: nyar_types::LayoutId, slot: u32) -> &mut Self {
        self.aggregate_layout_by_field.insert(field, (layout_id, slot));
        self
    }

    /// 登记外部导入 callable（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_register_import(&mut self, instance: ItemInstanceId, import: BackendImport) -> &mut Self {
        self.imports.insert(instance, import);
        self
    }

    /// 从完整 `CompiledProgram` 生成闭包；任何无法无损投影的语义都失败。
    pub fn from_compiled_program(
        program: &CompiledProgram,
        fragment_id: &nyar::Identifier,
        theory_bundle: nyar::TheoryBundle,
        roots: &[ItemInstanceId],
    ) -> Result<Self> {
        let canonical = program.canonical();
        let fragment = canonical.linked.fragments.get(fragment_id).ok_or_else(|| format!("Canonical 片段 `{fragment_id}` 不存在"))?;
        let mut declared_imports = BTreeMap::new();
        for (index, import) in &canonical.linked.imports {
            if canonical.mir.functions.contains_key(&import.callee) {
                return Err(format!("callable 实例 `{:?}` 同时绑定函数体与导入", import.callee));
            }
            if declared_imports.insert(import.callee, (*index, import)).is_some() {
                return Err(format!("callable 实例 `{:?}` 绑定多个 ImportIndex", import.callee));
            }
        }
        for (callee, (idx, import)) in &declared_imports {
            let link_str = format!("{:?}", import.link);
            let symbol = canonical.linked.callable_names.get(callee).cloned().unwrap_or_default();
        }
        let mut pending = roots
            .iter()
            .copied()
            .map(|instance| {
                if !canonical.mir.functions.contains_key(&instance) {
                    return Err(format!("Compiler callable 实例 `{instance:?}` 缺少 canonical 函数体"));
                }
                Ok(instance)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut functions = BTreeMap::new();
        let mut abi_names = BTreeMap::new();
        let mut imports = BTreeMap::new();
        while let Some(instance) = pending.pop() {
            if !seen.insert(instance) {
                continue;
            }
            if let Some((index, import)) = declared_imports.get(&instance) {
                let parameter_types = import.parameter_types.iter().map(|ty| lower_type(canonical, *ty)).collect::<Result<Vec<_>>>()?;
                let return_type = lower_type(canonical, import.return_type)?;
                let target_operator = freeze_target_operator(&import.link);
                if let Some(TargetOperatorBinding::Frozen(plan)) = target_operator {
                    let symbol = canonical.linked.callable_names.get(&instance).map(|name| name.to_string());
                    validate_frozen_operator(instance, symbol.as_deref(), &import.link, &parameter_types, &return_type, &plan)?;
                }
                imports.insert(
                    instance,
                    BackendImport { index: *index, link: import.link.clone(), parameter_types, return_type, target_operator },
                );
                continue;
            }
            let name = canonical.linked.callable_names.get(&instance).ok_or_else(|| format!("callable 实例 `{instance:?}` 缺少 ABI 名称"))?;
            abi_names.insert(instance, name.clone());
            let function = canonical.mir.functions.get(&instance).ok_or_else(|| format!("callable `{name}` 缺少 canonical 函数体"))?;
            // 可达调用的目标必须已有闭合实例。签名仍含 Generic 的声明模板没有被前端证明
            // 不可达，只是从前端单态化里漏掉；把它从闭包队列里滤掉等于把这处缺口伪装成
            // 编译成功，因此这里失败闭合，要求前端提供单态化/evidence 合同。
            if signature_contains_generic(canonical, function) {
                return Err(format!(
                    "可达调用的目标缺少闭合实例: instance={instance:?} symbol={name} 的声明签名仍含 Generic 类型参数，\
                     backend 无法编码；需前端提供单态化/evidence 合同"
                ));
            }
            let (lowered, callees) = lower_function(program, function)?;
            pending.extend(callees);
            functions.insert(instance, lowered);
        }
        let mut plan = Self {
            module_name: canonical.linked.module_name.clone(),
            fragment_id: fragment.id.clone(),
            exported_operations: fragment.exported_operations.clone(),
            required_capabilities: fragment.required_capabilities.clone(),
            theory_bundle,
            entry_operation: fragment.entry_operation,
            wasm_export_names: fragment.wasm_export_names.clone(),
            external_import_links: fragment.external_imports.clone(),
            external_call_edges: fragment.external_call_edges.clone(),
            internal_call_edges: fragment.internal_call_edges.clone(),
            aggregate_layouts: canonical.linked.aggregate_layouts.clone(),
            aggregate_layout_by_nominal: canonical.linked.aggregate_layout_by_nominal.clone(),
            aggregate_layout_by_field: canonical.linked.aggregate_layout_by_field.clone(),
            aggregate_layout_by_type: canonical.linked.aggregate_layout_by_type.clone(),
            // 聚合 helper operation 的载体必须先冻结 producer 的库导出 ABI 再派生，
            // 因此在构造阶段留空，由 `freeze_aggregate_operations` 补齐。
            aggregate_operations: Vec::new(),
            flags_types: canonical.linked.flags_types.clone(),
            singleton_instances: canonical.linked.singleton_instances.clone(),
            functions,
            library_export_abis: BTreeMap::new(),
            abi_names,
            imports,
            sum_reps: program.representation().sum_reps.clone(),
        };
        plan.freeze_library_export_abis()?;
        plan.freeze_aggregate_operations(&fragment.aggregate_operations)?;
        plan.validate_aggregate_operations()?;
        Ok(plan)
    }

    /// 按实例查询已冻结的库导出 ABI 载体合同。
    pub fn library_export_abi(&self, instance: &ItemInstanceId) -> Option<&LibraryExportAbi> {
        self.library_export_abis.get(instance)
    }

    /// 在测试登记导出名后补齐库导出 ABI 载体冻结（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_freeze_library_export_abis(&mut self) -> Result<()> {
        self.freeze_library_export_abis()
    }

    /// 为所有已声明导出名的实例冻结参数/返回载体。
    ///
    /// 聚合 helper 的 producer 也在 `wasm_export_names` 中，因此同样被冻结；缺少
    /// 函数体或类型没有稳定载体时结构化失败。
    fn freeze_library_export_abis(&mut self) -> Result<()> {
        let mut abis = BTreeMap::new();
        for (instance, export_name) in &self.wasm_export_names {
            let function = self
                .functions
                .get(instance)
                .ok_or_else(|| format!("库导出 `{instance}`（export_name={export_name}）缺少 Compiler 函数体，无法冻结 ABI 载体合同"))?;
            let abi = freeze_library_export_abi(function)
                .map_err(|reason| format!("库导出 `{instance}`（export_name={export_name}）冻结 ABI 载体合同失败: {reason}"))?;
            abis.insert(*instance, abi);
        }
        self.library_export_abis = abis;
        Ok(())
    }

    /// 把 canonical 冻结的聚合宿主直通操作映射为 plan 事实。
    ///
    /// operation 身份、布局身份、导出名与 producer 全部来自前端；plan 只补充物理载体，
    /// 且载体必须从 producer 的**真实签名**（已由 [`Self::freeze_library_export_abis`]
    /// 冻结的 [`LibraryExportAbi`]）逐一映射得到，不得在此写死 `i32` / handle 列表。
    /// producer 没有已冻结 ABI 说明它不在 `wasm_export_names` 里，合同已坏，失败闭合。
    /// operation 的形状（如 `construct` 必须是 `[i32] -> handle`）留给
    /// [`Self::validate_aggregate_operations`] 统一判定，避免两处规则漂移。
    fn freeze_aggregate_operations(&mut self, declared: &[nyar_types::CanonicalAggregateOperation]) -> Result<()> {
        let mut operations = Vec::with_capacity(declared.len());
        for operation in declared {
            let producer = operation.producer;
            let Some(abi) = self.library_export_abis.get(&producer)
            else {
                return Err(format!(
                    "聚合 helper operation 的 producer 缺少已冻结库导出 ABI: layout_id={} operation={} producer={producer:?} \
                     export_name={}",
                    operation.layout_id,
                    operation.operation.as_str(),
                    operation.export_name
                ));
            };
            operations.push(AggregateOperationPlan {
                layout_id: operation.layout_id,
                operation: operation.operation,
                param_carriers: abi.params.iter().copied().map(aggregate_operation_carrier).collect(),
                return_carrier: aggregate_operation_carrier(abi.returns),
                export_name: operation.export_name.clone(),
                producer: AggregateOperationProducer::FunctionBody(producer),
            });
        }
        self.aggregate_operations = operations;
        Ok(())
    }

    /// 返回模块名。
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// 返回片段标识。
    pub fn fragment_id(&self) -> &nyar::Identifier {
        &self.fragment_id
    }

    /// 返回已导出的 callable 实例列表。
    pub fn exported_operations(&self) -> &[ItemInstanceId] {
        &self.exported_operations
    }

    /// 返回片段所需 capability 标签。
    pub fn required_capabilities(&self) -> &[nyar::CapabilityTag] {
        &self.required_capabilities
    }

    /// 返回理论束合同。
    pub fn theory_bundle(&self) -> &nyar::TheoryBundle {
        &self.theory_bundle
    }

    /// 返回入口 callable 实例（若有）。
    pub fn entry_operation(&self) -> Option<ItemInstanceId> {
        self.entry_operation
    }

    /// 返回 wasm 导出名映射。
    pub fn wasm_export_names(&self) -> &BTreeMap<ItemInstanceId, String> {
        &self.wasm_export_names
    }

    /// 返回外部导入链接映射。
    pub fn external_import_links(&self) -> &BTreeMap<ItemInstanceId, nyar::ExternalImportLink> {
        &self.external_import_links
    }

    /// 返回外部调用边列表。
    pub fn external_call_edges(&self) -> &[nyar_types::CanonicalExternalCallEdge] {
        &self.external_call_edges
    }

    /// 返回内部调用边列表。
    pub fn internal_call_edges(&self) -> &[nyar_types::CanonicalCallEdge] {
        &self.internal_call_edges
    }

    /// 返回聚合布局计划。
    pub fn aggregate_layouts(&self) -> &nyar_types::AggregateLayoutPlan {
        &self.aggregate_layouts
    }

    /// 返回 plan 冻结的聚合 helper operation。
    ///
    /// `glue_entries` 只能消费这里的 operation；backend 不得在 emit 阶段自行合成。
    pub fn aggregate_operations(&self) -> &[AggregateOperationPlan] {
        &self.aggregate_operations
    }

    /// 校验聚合 helper operation 的布局身份、载体形状与真实生产者。
    ///
    /// 任何缺失事实（未登记布局、producer 缺少函数体或导出名、operation 载体与
    /// operation 形状不符、同一布局/operation 重复）都在此结构化失败。
    pub fn validate_aggregate_operations(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        for operation in &self.aggregate_operations {
            let layout = self.aggregate_layouts.layouts.iter().find(|layout| layout.id == operation.layout_id).ok_or_else(|| {
                format!(
                    "聚合 helper operation 缺少布局事实: layout_id={} operation={} export_name={}",
                    operation.layout_id,
                    operation.operation.as_str(),
                    operation.export_name
                )
            })?;
            let identity = format!("{}.{}<layout_id={}>", layout.namespace, layout.name, layout.id);
            if !seen.insert((operation.layout_id, operation.operation)) {
                return Err(format!(
                    "聚合 helper operation 重复: aggregate={identity} operation={} export_name={}",
                    operation.operation.as_str(),
                    operation.export_name
                ));
            }
            let producer = operation.producer.instance();
            let Some(declared_export) = self.wasm_export_names.get(&producer)
            else {
                return Err(format!(
                    "聚合 helper operation 的 producer 未声明 plan 导出: aggregate={identity} operation={} producer={producer:?} \
                     export_name={} param_carriers={:?} return_carrier={}",
                    operation.operation.as_str(),
                    operation.export_name,
                    operation.param_carriers.iter().map(|carrier| carrier.as_str()).collect::<Vec<_>>(),
                    operation.return_carrier.as_str()
                ));
            };
            if declared_export != &operation.export_name {
                return Err(format!(
                    "聚合 helper operation 的 export 身份与 producer 不一致: aggregate={identity} operation={} \
                     export_name={} producer_export_name={declared_export} producer={producer:?}",
                    operation.operation.as_str(),
                    operation.export_name
                ));
            }
            if !self.functions.contains_key(&producer) {
                return Err(format!(
                    "聚合 helper operation 的 producer 缺少 Compiler 函数体: aggregate={identity} operation={} producer={producer:?} \
                     export_name={}",
                    operation.operation.as_str(),
                    operation.export_name
                ));
            }
            validate_aggregate_operation_shape(operation, &identity)?;
        }
        Ok(())
    }

    /// 返回 nominal 实例到布局 ID 的映射。
    pub fn aggregate_layout_by_nominal(&self) -> &BTreeMap<nyar_types::NominalInstanceId, nyar_types::LayoutId> {
        &self.aggregate_layout_by_nominal
    }

    /// 返回字段 ID 到布局槽位的映射。
    pub fn aggregate_layout_by_field(&self) -> &BTreeMap<nyar_types::FieldId, (nyar_types::LayoutId, u32)> {
        &self.aggregate_layout_by_field
    }

    /// 返回类型 ID 到布局 ID 的映射。
    pub fn aggregate_layout_by_type(&self) -> &BTreeMap<nyar_types::TypeId, nyar_types::LayoutId> {
        &self.aggregate_layout_by_type
    }

    /// 返回 flags 类型布局列表。
    pub fn flags_types(&self) -> &[nyar_types::FlagsLayout] {
        &self.flags_types
    }

    /// 返回单例实例计划列表。
    pub fn singleton_instances(&self) -> &[nyar_types::SingletonInstancePlan] {
        &self.singleton_instances
    }

    /// 返回已声明导入映射。
    pub fn imports(&self) -> &BTreeMap<ItemInstanceId, BackendImport> {
        &self.imports
    }

    /// 返回全部 callable 实例 ID。
    pub fn instances(&self) -> Vec<ItemInstanceId> {
        self.functions.keys().copied().collect()
    }

    /// 按实例 ID 查询函数视图。
    pub fn get_function(&self, instance: &ItemInstanceId) -> Option<FunctionView> {
        self.functions.get(instance).cloned().map(|function| FunctionView { function })
    }

    /// 按实例 ID 查询 ABI 名称。
    pub fn abi_name_for_instance(&self, instance: ItemInstanceId) -> Option<QualifiedName> {
        self.abi_names.get(&instance).cloned()
    }

    /// 按实例 ID 查询挂起元数据视图。
    pub fn suspend_metadata(&self, instance: ItemInstanceId) -> Option<SuspendMetadataView> {
        self.functions.get(&instance).and_then(SuspendMetadataView::from_function)
    }

    /// 返回 sum 类型表示合同映射。
    pub fn sum_representations(&self) -> &BTreeMap<nyar_types::NominalInstanceId, nyar_types::layout_choice::SumRepresentation> {
        &self.sum_reps
    }
}

/// 冻结一个库导出函数的参数与返回载体。
///
/// 参数按 `param_types` 声明顺序逐个映射；返回值按 `return_type` 映射。映射语义
/// 必须与 `library_glue` 消费的 `WasmAbiType` 载体保持一一对应。没有稳定 Wasm
/// 载体的类型（`Integer128` / `Utf16` / `Bottom` / `Function` / `TraitObject` /
/// `Union`）在此结构化失败，不得回落为默认载体。
fn freeze_library_export_abi(function: &ExecutableFunction) -> Result<LibraryExportAbi> {
    let mut params = Vec::with_capacity(function.param_types.len());
    for (index, ty) in function.param_types.iter().enumerate() {
        let carrier = freeze_abi_carrier(ty)
            .map_err(|reason| format!("函数 `{}` 参数 {index} 类型 `{ty}` 没有库导出 ABI 载体: {reason}", function.symbol))?;
        if carrier == AbiCarrier::Void {
            return Err(format!("函数 `{}` 参数 {index} 类型 `{ty}` 映射为 Void 载体，Void 只允许出现在返回位", function.symbol));
        }
        params.push(carrier);
    }
    let returns = freeze_abi_carrier(&function.return_type)
        .map_err(|reason| format!("函数 `{}` 返回类型 `{}` 没有库导出 ABI 载体: {reason}", function.symbol, function.return_type))?;
    Ok(LibraryExportAbi { params, returns })
}

/// 把单个 `NyarType` 映射为 Wasm 物理载体。
///
/// 语义与 `abi_schema_for_type` + `abi_carrier_name` 的组合结果一致：标量按 Wasm
/// 原生槽位，容器与具名聚合统一为宿主句柄。
fn freeze_abi_carrier(ty: &NyarType) -> Result<AbiCarrier> {
    Ok(match ty {
        NyarType::Unit => AbiCarrier::Void,
        NyarType::Boolean | NyarType::Character => AbiCarrier::I32,
        NyarType::Integer8 { .. } | NyarType::Integer16 { .. } | NyarType::Integer32 { .. } => AbiCarrier::I32,
        NyarType::Integer64 { .. } => AbiCarrier::I64,
        NyarType::Float32 => AbiCarrier::F32,
        NyarType::Float64 => AbiCarrier::F64,
        NyarType::Utf8 => AbiCarrier::I32,
        NyarType::Array(_) | NyarType::FixedArray { .. } | NyarType::Nullable(_) | NyarType::Tuple(_) => AbiCarrier::Handle,
        NyarType::Named(_) | NyarType::Apply(_, _) => AbiCarrier::Handle,
        NyarType::Integer128 { .. } => return Err("Integer128 没有原生 Wasm 载体，需要显式拆分合同".to_string()),
        NyarType::Utf16 => return Err("Utf16 在当前 JS glue 轨没有稳定的宿主 ABI 合同".to_string()),
        NyarType::Bottom => return Err("`void` / bottom 类型不能出现在库导出 ABI 位置".to_string()),
        NyarType::Function(_) => return Err("函数类型不能出现在库导出 ABI 位置".to_string()),
        NyarType::TraitObject(_) => return Err("TraitObject（dynamic dispatch）没有稳定的 Wasm 载体".to_string()),
        NyarType::Union(_) => return Err("匿名 union 的 Wasm ABI 合同尚未定义".to_string()),
    })
}

/// 把已冻结的库导出 ABI 载体映射为 glue operation 载体。
///
/// 两个枚举一一对应且都是全函数；这里的 `match` 必须穷尽，不允许 `_` 兜底猜测载体。
fn aggregate_operation_carrier(carrier: AbiCarrier) -> AggregateOperationCarrier {
    match carrier {
        AbiCarrier::I32 => AggregateOperationCarrier::I32,
        AbiCarrier::I64 => AggregateOperationCarrier::I64,
        AbiCarrier::F32 => AggregateOperationCarrier::F32,
        AbiCarrier::F64 => AggregateOperationCarrier::F64,
        AbiCarrier::Handle => AggregateOperationCarrier::Handle,
        AbiCarrier::Void => AggregateOperationCarrier::Void,
    }
}

/// 校验聚合 helper operation 的载体形状。
///
/// `construct` 是 `[i32] -> handle`（宿主用元素数量请求一个空聚合）；
/// `length` 是 `[handle] -> i32`；`push` 是 `[handle, E] -> void`；
/// `at` 是 `[handle, i32] -> E`。聚合槽位一律是宿主句柄，与导出函数形参的
/// 物理载体一致；任何偏离都在计划的物理解释层失败。
fn validate_aggregate_operation_shape(operation: &AggregateOperationPlan, identity: &str) -> Result<()> {
    use AggregateOperationCarrier::{Handle, I32, Void};
    let fail = |expected: &str| {
        Err(format!(
            "聚合 helper operation 载体形状不合法: aggregate={identity} operation={} expected={expected} actual_param_carriers={:?} \
             actual_return_carrier={}",
            operation.operation.as_str(),
            operation.param_carriers.iter().map(|carrier| carrier.as_str()).collect::<Vec<_>>(),
            operation.return_carrier.as_str()
        ))
    };
    match operation.operation {
        AggregateOperation::Construct => {
            if operation.param_carriers.as_slice() != [I32] || operation.return_carrier != Handle {
                return fail("[i32] -> handle");
            }
        }
        AggregateOperation::Length => {
            if operation.param_carriers.as_slice() != [Handle] || operation.return_carrier != I32 {
                return fail("[handle] -> i32");
            }
        }
        AggregateOperation::Push => {
            if operation.param_carriers.len() != 2
                || operation.param_carriers[0] != Handle
                || operation.param_carriers[1] == Void
                || operation.return_carrier != Void
            {
                return fail("[handle, E] -> void");
            }
        }
        AggregateOperation::At => {
            if operation.param_carriers.as_slice() != [Handle, I32] || operation.return_carrier == Void {
                return fail("[handle, i32] -> E");
            }
        }
    }
    Ok(())
}

/// 计划构造时冻结 `[wasm_operator]` / `[ovm_operator]` 的完整 operator 计划。
///
/// 助记符只在此处解析一次；backend 之后只消费 [`TargetOperatorBinding`]，不得再从
/// 助记符字符串推断 arity、操作数改写或结果类型。未解析的绑定保留为
/// [`TargetOperatorBinding::Unfrozen`]，由 emit 侧结构化失败。
fn freeze_target_operator(link: &ExternalImportLink) -> Option<TargetOperatorBinding> {
    if let Some(mnemonic) = crate::lowering::shared::interop::wasm_operator_target(link) {
        let resolved = crate::lowering::backends::wasm::mir::numeric::freeze_wasm_numeric_binding(mnemonic);
        return Some(frozen_operator_binding(resolved));
    }
    let vm_mnemonic = crate::lowering::shared::interop::ovm_operator_target(link);
    #[cfg(feature = "ovm-lane")]
    if let Some(mnemonic) = vm_mnemonic {
        let resolved = crate::lowering::backends::ovm_mir::freeze_ovm_operator_binding(mnemonic);
        return Some(frozen_operator_binding(resolved));
    }
    #[cfg(not(feature = "ovm-lane"))]
    if vm_mnemonic.is_some() {
        return Some(TargetOperatorBinding::Unfrozen);
    }
    None
}

fn frozen_operator_binding(resolved: Option<TargetOperatorPlan>) -> TargetOperatorBinding {
    match resolved {
        Some(plan) => TargetOperatorBinding::Frozen(plan),
        None => TargetOperatorBinding::Unfrozen,
    }
}

/// 校验冻结计划与导入声明在 arity、操作数载体和结果槽载体上一致。
///
/// 导入声明是 Compiler 已绑定的调用合同，冻结计划是目标 operator 的物理形状；二者不一致
/// 时计划不成立，必须在此结构化失败，不得让后端按声明槽位发射后再改写或补默认值。
fn validate_frozen_operator(
    instance: ItemInstanceId,
    symbol: Option<&str>,
    link: &ExternalImportLink,
    parameter_types: &[NyarType],
    return_type: &NyarType,
    plan: &TargetOperatorPlan,
) -> Result<()> {
    let mnemonic = crate::lowering::shared::interop::wasm_operator_target(link)
        .or_else(|| crate::lowering::shared::interop::ovm_operator_target(link))
        .unwrap_or("<unknown>");
    let function = symbol.unwrap_or("<unknown>");
    if parameter_types.len() != plan.arity {
        return Err(format!(
            "目标 operator 绑定 `{mnemonic}` 的声明 arity 与冻结计划不符: function={function} expected={} actual={} instance={instance:?} link={link}",
            plan.arity,
            parameter_types.len()
        ));
    }
    for (index, ty) in parameter_types.iter().enumerate() {
        let Some(carrier) = operand_carrier(ty)
        else {
            return Err(format!(
                "目标 operator 绑定 `{mnemonic}` 的参数 {index} 类型没有目标载体: function={function} type={ty:?} instance={instance:?} link={link}"
            ));
        };
        if carrier != plan.operand_type {
            return Err(format!(
                "目标 operator 绑定 `{mnemonic}` 的参数 {index} 操作数载体与冻结计划不符: function={function} expected={:?} actual={:?} instance={instance:?} link={link}",
                plan.operand_type, carrier
            ));
        }
    }
    let Some(result_carrier) = result_carrier(return_type)
    else {
        return Err(format!(
            "目标 operator 绑定 `{mnemonic}` 的返回类型没有结果槽载体: function={function} type={return_type:?} instance={instance:?} link={link}"
        ));
    };
    if result_carrier != plan.result_type {
        return Err(format!(
            "目标 operator 绑定 `{mnemonic}` 的结果槽载体与冻结计划不符: function={function} expected={:?} actual={:?} instance={instance:?} link={link}",
            plan.result_type, result_carrier
        ));
    }
    if let TargetOperatorRecipe::Steps(steps) = plan.recipe {
        for step in steps {
            if let TargetOperandStep::IntegerConstant { operand_type: constant_type, .. } = step {
                if *constant_type != plan.operand_type {
                    return Err(format!(
                        "目标 operator 绑定 `{mnemonic}` 的常量配方载体与冻结操作数载体不符: function={function} expected={:?} actual={:?} instance={instance:?} link={link}",
                        plan.operand_type, constant_type
                    ));
                }
            }
        }
    }
    Ok(())
}

fn operand_carrier(ty: &NyarType) -> Option<TargetOperandType> {
    match ty {
        NyarType::Integer32 { .. } => Some(TargetOperandType::I32),
        NyarType::Integer64 { .. } => Some(TargetOperandType::I64),
        NyarType::Float64 => Some(TargetOperandType::F64),
        _ => None,
    }
}

fn result_carrier(ty: &NyarType) -> Option<TargetOperandType> {
    match ty {
        NyarType::Boolean => Some(TargetOperandType::I32),
        _ => operand_carrier(ty),
    }
}

fn lower_function(program: &CompiledProgram, function: &nyar_types::CanonicalFunction) -> Result<(ExecutableFunction, Vec<ItemInstanceId>)> {
    let canonical = program.canonical();
    for (value, type_id) in &function.value_types {
        let identity = nyar_types::ValueIdentity::new(function.instance, *value);
        match program.representation().value_reps.get(&identity) {
            Some(ValueRepresentation::Specialized) => {}
            // 实例级布局已由 canonical 冻结为 Reference 时，GC struct 就是该值的
            // 目标私有载体；没有实例级 Reference 布局仍失败关闭。
            Some(ValueRepresentation::Reified) if has_reference_instance_layout(canonical, *type_id) => {}
            Some(representation) => {
                return Err(format!("值 `{identity:?}` 的表示 `{representation:?}` 尚无目标私有载体合同，拒绝按语义类型重新选择表示"));
            }
            None => {
                return Err(format!("值 `{identity:?}` 缺少 RepresentationPlan 载体合同"));
            }
        }
    }
    for block in function.blocks.values() {
        for instruction in &block.instructions {
            for (nominal, variant) in canonical_sum_operations(instruction) {
                let Some(sum) = program.representation().sum_reps.get(&nominal)
                else {
                    return Err(format!("sum `{nominal:?}` 缺少 RepresentationPlan 布局合同"));
                };
                if !sum.variants.contains_key(&variant) {
                    return Err(format!("sum variant `{nominal:?}/{variant:?}` 缺少 RepresentationPlan 布局合同"));
                }
            }
            if let CanonicalOperation::Invoke { callee, .. } = &instruction.operation {
                match (callee, program.representation().invoke_lowerings.get(&instruction.id)) {
                    (CanonicalCallee::Item(_), Some(InvokeLowering::Direct)) => {}
                    (CanonicalCallee::Witness(_), Some(InvokeLowering::TypedWitness)) => {
                        // 放行！在 lower_operation 里尝试消去 Witness 为具体 Item 调用。
                        // self_type 仍开放时（`None`）由 lower_operation 返回结构化失败。
                    }
                    (_, None) => {
                        return Err(format!("调用指令 `{}` 缺少 RepresentationPlan 降低合同", instruction.id.index()));
                    }
                    (_, Some(lowering)) => {
                        return Err(format!(
                            "调用指令 `{}` 的表示 `{lowering:?}` 与 callee `{callee:?}` 尚无目标私有调用合同，拒绝改用普通调用",
                            instruction.id.index()
                        ));
                    }
                }
            }
        }
    }
    let mut values =
        function.value_types.keys().map(|value| Value { id: ValueRef(value.index()), origin: ValueOrigin::Temporary }).collect::<Vec<_>>();
    for (index, (value, _)) in function.parameters.iter().enumerate() {
        let row = values.iter_mut().find(|row| row.id.0 == value.index()).ok_or_else(|| format!("入口参数缺少 SSA 值合同"))?;
        // canonical 已保留 compiler schema 的声明参数名；缺失只是防御性回落。
        let name = function.parameter_names.get(index).cloned().unwrap_or_else(|| format!("arg{index}"));
        row.origin = ValueOrigin::Parameter { index, name };
    }
    let mut callees = Vec::new();
    let mut blocks = Vec::new();
    for block in function.blocks.values() {
        let instructions = block
            .instructions
            .iter()
            .map(|instruction| {
                Ok(Instruction {
                    id: instruction.id,
                    results: instruction.results.iter().map(|value| ValueRef(value.index())).collect(),
                    kind: lower_operation(canonical, function, instruction, &mut callees)?,
                    provenance: nyar_types::ProvenanceId::from_index(instruction.id.index()).ok_or_else(|| format!("指令 identity 溢出"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        blocks.push(Block {
            id: BlockRef(block.id.0),
            label: format!("block_{}", block.id.0),
            parameters: block.parameters.iter().map(|(value, _)| ValueRef(value.index())).collect(),
            instructions,
            terminator: lower_terminator(block.terminator.clone())?,
        });
    }
    let symbol = canonical.linked.callable_names.get(&function.instance).ok_or_else(|| format!("函数实例缺少 ABI 名称"))?.to_string();
    let type_of = |id| lower_type(canonical, id);
    let value_layouts = function
        .value_types
        .iter()
        .filter_map(|(value, type_id)| {
            canonical.linked.aggregate_layout_by_type.get(type_id).copied().map(|layout| (ValueRef(value.index()), layout))
        })
        .collect();
    let return_layout = canonical.linked.aggregate_layout_by_type.get(&function.return_type).copied();
    Ok((
        ExecutableFunction {
            symbol,
            return_type: type_of(function.return_type)?,
            param_types: function.parameters.iter().map(|(_, id)| type_of(*id)).collect::<Result<_>>()?,
            value_types: function.value_types.iter().map(|(value, id)| Ok((ValueRef(value.index()), type_of(*id)?))).collect::<Result<_>>()?,
            value_layouts,
            return_layout,
            entry: BlockRef(function.entry.0),
            values,
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            blocks,
            diagnostics: Vec::new(),
        },
        callees,
    ))
}

/// 该类型是否有 canonical 冻结的实例级 Reference 布局（GC struct 载体）。
fn has_reference_instance_layout(program: &CanonicalProgram, type_id: nyar_types::TypeId) -> bool {
    let Some(layout_id) = program.linked.aggregate_layout_by_type.get(&type_id).copied()
    else {
        return false;
    };
    program
        .linked
        .aggregate_layouts
        .layouts
        .iter()
        .find(|layout| layout.id == layout_id)
        .is_some_and(|layout| layout.storage == nyar_types::StorageKind::Reference)
}

fn canonical_sum_operations(instruction: &nyar_types::CanonicalInstruction) -> Vec<(nyar_types::NominalInstanceId, nyar_types::VariantId)> {
    match instruction.operation {
        CanonicalOperation::SumNew { nominal, variant, .. }
        | CanonicalOperation::SumPayloadGet { nominal, variant, .. }
        | CanonicalOperation::SumVariantIs { nominal, variant, .. } => vec![(nominal, variant)],
        _ => Vec::new(),
    }
}

/// 在 target_execution_plan 边界把 trait Witness 分派降为具体 Item 调用。
///
/// 策略（按优先级）：
/// 1. 直接用 trait 方法槽 ItemId 在 `linked.item_instances` 查 —— 当 impl 方法的
///    declaration 正好等于 trait 方法槽时命中；
/// 2. 在 `mir.functions` 里找第一个参数（self）类型等于 witness.self_type 的函数实例；
///    多个候选时返回结构化失败，避免按名字/短名猜测。
/// 在 target_execution_plan 边界把 trait Witness 分派降为具体 Item 调用。
///
/// witness 的实例身份是前端事实：`self` 类型具体化后，前端本应在
/// `instantiate_generic_callables` 里换绑成真实实现。能走到这里只剩前端仍判为
/// 开放的 witness，因此这里只允许按已冻结的 `(method declaration, self 类型)`
/// 精确匹配唯一实例。不得按名字、候选顺序或「第一个参数看起来一样」猜实现——
/// 猜错会把 `Hash::hash` 绑到 `i64.infix -` 这种签名不同的算子上。
fn resolve_witness_to_item(program: &CanonicalProgram, witness: &nyar_types::CanonicalWitness) -> Result<ItemInstanceId> {
    let Some(self_type) = witness.self_type
    else {
        return Err(format!(
            "trait witness self_type 仍开放泛型，无法在 target_execution_plan 消去: trait={:?} method={:?}",
            witness.trait_declaration, witness.method_declaration
        ));
    };
    let mut matching = program
        .linked
        .item_instances
        .iter()
        .filter(|(_, record)| record.declaration == witness.method_declaration)
        .filter(|(_, record)| record.parameter_types.first() == Some(&self_type));
    let Some((instance, _)) = matching.next()
    else {
        return Err(format!(
            "trait witness 缺少 declaration={:?} self_type={self_type:?} 的实例: trait={:?}",
            witness.method_declaration, witness.trait_declaration
        ));
    };
    if matching.next().is_some() {
        return Err(format!(
            "trait witness declaration={:?} self_type={self_type:?} 对应多个实例；实例身份必须由前端冻结",
            witness.method_declaration
        ));
    }
    Ok(*instance)
}

fn lower_operation(
    program: &CanonicalProgram,
    function: &nyar_types::CanonicalFunction,
    instruction: &nyar_types::CanonicalInstruction,
    callees: &mut Vec<ItemInstanceId>,
) -> Result<InstructionKind> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    let layout_for_type = |type_id: nyar_types::TypeId, operation: &str| {
        program
            .linked
            .aggregate_layout_by_type
            .get(&type_id)
            .copied()
            .ok_or_else(|| format!("{operation} 缺少编译器绑定的布局身份: {type_id:?}"))
    };
    let value_type = |value: nyar_types::MirValueId| {
        function.value_types.get(&value).copied().ok_or_else(|| format!("聚合值 `{value:?}` 缺少 Semantic MIR 类型事实"))
    };
    Ok(match &instruction.operation {
        CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), arguments } => {
            if !program.linked.item_instances.contains_key(instance) {
                return Err(format!("调用 identity `{instance:?}` 未解析"));
            }
            callees.push(*instance);
            InstructionKind::Call { callee: Operand::Item(*instance), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        CanonicalOperation::Invoke { callee: CanonicalCallee::Witness(witness), arguments } => {
            let instance = resolve_witness_to_item(program, &witness)?;
            callees.push(instance);
            InstructionKind::Call { callee: Operand::Item(instance), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        CanonicalOperation::Invoke { callee: CanonicalCallee::Value(callee), arguments } => {
            InstructionKind::Call { callee: value(*callee), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        // 具名函数作值：实例身份由前端冻结，目标层只校验链接事实并保留身份。
        // 被作值引用的函数不出现在任何直接调用边上，可达性必须由本指令登记，
        // 否则单态化后的实现函数体会被可执行闭包漏掉。
        CanonicalOperation::LoadFunctionRef { instance } => {
            if !program.linked.item_instances.contains_key(instance) {
                return Err(format!("函数值 identity `{instance:?}` 未解析"));
            }
            callees.push(*instance);
            InstructionKind::LoadFunctionRef { instance: *instance }
        }
        CanonicalOperation::Copy { source } => InstructionKind::Copy { source: value(*source) },
        CanonicalOperation::StoreVar { name, value: stored, ty } => {
            InstructionKind::StoreVar { name: name.clone(), value: value(*stored), ty: ty.map(|id| lower_type(program, id)).transpose()? }
        }
        CanonicalOperation::AggregateCopy { source, destination } => {
            let source_type = value_type(*source)?;
            let destination_type = value_type(*destination)?;
            if source_type != destination_type {
                return Err(format!("AggregateCopy 源与目标类型不一致: {source_type:?} != {destination_type:?}"));
            }
            InstructionKind::AggregateCopy {
                layout_id: layout_for_type(source_type, "AggregateCopy")?,
                source: value(*source),
                dest: value(*destination),
            }
        }
        // 文本常量的编码身份由常量变体自身决定（`Utf8` / `Utf16`），不靠名字或源码回推；
        // canonical 层不保存可被字符串重建的类型，因此在这里从编码恢复。
        CanonicalOperation::LoadConstant { constant } => {
            InstructionKind::LoadConstant { constant: lower_constant(constant), ty: constant_encoding_type(constant) }
        }
        // 结构操作已由 frontend 依据自身内建结构操作身份判定完毕，事实所有权在此结束；
        // backend-private 计划只做载体映射，wasm backend 负责编码成 wasm GC 指令。
        // 在此重新拒绝会把已确认的结构语义降级成字符串错误，属禁止的向上推断。
        CanonicalOperation::ArrayGet { array, index } => InstructionKind::ArrayGet { array: value(*array), index: value(*index) },
        CanonicalOperation::ArraySet { array, index, value: stored } => {
            InstructionKind::ArraySet { array: value(*array), index: value(*index), value: value(*stored) }
        }
        CanonicalOperation::ArrayLength { array } => InstructionKind::ArrayLength { array: value(*array) },
        CanonicalOperation::NullIsNull { value: operand } => InstructionKind::NullIsNull { value: value(*operand) },
        CanonicalOperation::NullUnwrap { value: operand } => InstructionKind::NullUnwrap { value: value(*operand) },
        CanonicalOperation::ArrayNew { array_type, length, initialization } => InstructionKind::ArrayNew {
            array_type: lower_type(program, *array_type)?,
            length: value(*length),
            initialization: match initialization {
                CanonicalArrayInitialization::Default => nyar_types::ArrayInitialization::Default,
                CanonicalArrayInitialization::Fill(fill) => nyar_types::ArrayInitialization::Fill(value(*fill)),
            },
        },
        CanonicalOperation::ArrayFromElements { array_type, elements } => {
            // 只有 FixedArray 才需要 aggregate layout；普通 Array 由 Wasm GC array.new_fixed 处理。
            let need_layout = matches!(
                program.linked.types.get(array_type).map(|row| &row.kind),
                Some(nyar_types::CanonicalTypeKind::Array { length: Some(_), .. })
            );
            let layout_id = if need_layout {
                layout_for_type(*array_type, "ArrayFromElements")?
            }
            else {
                // 普通 Array 不需要 aggregate layout；用 dummy 0 占位，wasm backend 会跳过。
                0u32
            };
            InstructionKind::ArrayFromElements {
                layout_id,
                array_type: lower_type(program, *array_type)?,
                elements: elements.iter().map(|id| value(*id)).collect(),
            }
        }
        CanonicalOperation::TupleNew { fields, element_types } => {
            let result = instruction.results.first().copied().ok_or_else(|| format!("TupleNew 缺少结果值"))?;
            let result_type = value_type(result)?;
            if !matches!(program.linked.types.get(&result_type).map(|row| &row.kind), Some(nyar_types::CanonicalTypeKind::Tuple(_))) {
                return Err(format!("TupleNew 结果不是 tuple 类型: {result_type:?}"));
            }
            if fields.len() != element_types.len() {
                return Err(format!("TupleNew 元素合同长度不一致"));
            }
            InstructionKind::TupleNew {
                layout_id: layout_for_type(result_type, "TupleNew")?,
                fields: fields.iter().map(|id| value(*id)).collect(),
            }
        }
        CanonicalOperation::StructNew { nominal, fields } => {
            InstructionKind::StructNew { nominal: *nominal, fields: fields.iter().map(|(field, id)| (*field, value(*id))).collect() }
        }
        CanonicalOperation::FieldGet { object, field } => InstructionKind::FieldGet { object: value(*object), field: *field },
        CanonicalOperation::FieldSet { object, field, value: stored } => {
            InstructionKind::FieldSet { object: value(*object), field: *field, value: value(*stored) }
        }
        CanonicalOperation::SumNew { nominal, variant, payload } => {
            InstructionKind::SumNew { nominal: *nominal, variant: *variant, payload: payload.map(value) }
        }
        CanonicalOperation::SumPayloadGet { nominal, variant, object } => {
            InstructionKind::SumPayloadGet { nominal: *nominal, variant: *variant, object: value(*object) }
        }
        CanonicalOperation::SumVariantIs { nominal, variant, object } => {
            InstructionKind::SumVariantIs { nominal: *nominal, variant: *variant, object: value(*object) }
        }
        // 语言层显式 `as`：目标类型是后端必须编码的事实，不能丢成 Copy。
        CanonicalOperation::Cast { source, ty } => InstructionKind::Cast { source: value(*source), ty: lower_type(program, *ty)? },
        unsupported => return Err(format!("canonical 操作尚无目标私有合同: {unsupported:?}")),
    })
}

/// 文本常量的编码身份由常量变体自身决定；整数/空值没有在这里声称的编码身份。
fn constant_encoding_type(constant: &CanonicalConstant) -> Option<NyarType> {
    match constant {
        CanonicalConstant::Utf8(_) => Some(NyarType::Utf8),
        CanonicalConstant::Utf16(_) => Some(NyarType::Utf16),
        _ => None,
    }
}

fn lower_constant(constant: &CanonicalConstant) -> Constant {
    match constant {
        CanonicalConstant::Int(v) => Constant::Int(*v),
        CanonicalConstant::Bool(v) => Constant::Bool(*v),
        CanonicalConstant::Utf8(v) => Constant::Utf8(v.clone()),
        CanonicalConstant::Utf16(v) => Constant::Utf16(v.clone()),
        CanonicalConstant::Unit => Constant::Unit,
        CanonicalConstant::Null => Constant::Null,
    }
}

/// 函数声明的签名（参数与返回类型）是否仍含未闭合的 Generic。
///
/// 签名含 Generic 说明该实例是声明模板而非单态化后的闭合实例；目标层无法编码这种
/// 签名，因此它只能作为失败原因，不能作为「可以跳过」的判据。
fn signature_contains_generic(program: &CanonicalProgram, function: &nyar_types::CanonicalFunction) -> bool {
    function.parameters.iter().any(|(_, ty)| type_contains_generic_deep(program, *ty))
        || type_contains_generic_deep(program, function.return_type)
}

/// 递归检查 canonical 类型（含嵌套）是否包含 Generic。
fn type_contains_generic_deep(program: &CanonicalProgram, id: nyar_types::TypeId) -> bool {
    let Some(row) = program.linked.types.get(&id)
    else {
        return false;
    };
    match &row.kind {
        nyar_types::CanonicalTypeKind::Generic { .. } => true,
        nyar_types::CanonicalTypeKind::Nominal { arguments, .. } => arguments.iter().any(|a| type_contains_generic_deep(program, *a)),
        nyar_types::CanonicalTypeKind::Tuple(items)
        | nyar_types::CanonicalTypeKind::Union(items)
        | nyar_types::CanonicalTypeKind::Intersection(items) => items.iter().any(|a| type_contains_generic_deep(program, *a)),
        nyar_types::CanonicalTypeKind::Array { element, .. } => type_contains_generic_deep(program, *element),
        nyar_types::CanonicalTypeKind::Nullable(element) => type_contains_generic_deep(program, *element),
        nyar_types::CanonicalTypeKind::Function { parameters, return_type } => {
            parameters.iter().any(|a| type_contains_generic_deep(program, *a)) || type_contains_generic_deep(program, *return_type)
        }
        _ => false,
    }
}

fn lower_type(program: &CanonicalProgram, id: nyar_types::TypeId) -> Result<NyarType> {
    let kind = &program.linked.types.get(&id).ok_or_else(|| format!("canonical 类型事实缺失"))?.kind;
    match kind {
        CanonicalTypeKind::Primitive(kind) => match kind {
            nyar_types::CanonicalPrimitiveType::Void => Ok(NyarType::Bottom),
            nyar_types::CanonicalPrimitiveType::Unit => Ok(NyarType::Unit),
            nyar_types::CanonicalPrimitiveType::Bool => Ok(NyarType::Boolean),
            // 窄整数保留宽度与符号，不能在计划层提前抹成 i32：表示转换的扩展/截断
            // 配方由两端语义类型决定，抹平宽度后无法区分 `i32.and 0xFF` 与 `i32.extend8_s`。
            nyar_types::CanonicalPrimitiveType::Integer { bits: 8, signed } => Ok(NyarType::Integer8 { signed: *signed }),
            nyar_types::CanonicalPrimitiveType::Integer { bits: 16, signed } => Ok(NyarType::Integer16 { signed: *signed }),
            nyar_types::CanonicalPrimitiveType::Integer { bits: 32, signed } => Ok(NyarType::Integer32 { signed: *signed }),
            nyar_types::CanonicalPrimitiveType::Integer { bits: 64, signed } => Ok(NyarType::Integer64 { signed: *signed }),
            nyar_types::CanonicalPrimitiveType::Character => Ok(NyarType::Character),
            nyar_types::CanonicalPrimitiveType::Utf8 => Ok(NyarType::Utf8),
            nyar_types::CanonicalPrimitiveType::Utf16 => Ok(NyarType::Utf16),
            unsupported => Err(format!("primitive 尚无目标合同: {unsupported:?}")),
        },
        CanonicalTypeKind::Tuple(items) => Ok(NyarType::Tuple(items.iter().map(|id| lower_type(program, *id)).collect::<Result<_>>()?)),
        CanonicalTypeKind::Array { element, length: None } => Ok(NyarType::Array(Box::new(lower_type(program, *element)?))),
        CanonicalTypeKind::Array { element, length: Some(length) } => Ok(NyarType::FixedArray {
            element: Box::new(lower_type(program, *element)?),
            length: usize::try_from(*length).map_err(|_| format!("固定数组长度溢出"))?,
        }),
        CanonicalTypeKind::Nullable(element) => Ok(NyarType::Nullable(Box::new(lower_type(program, *element)?))),
        CanonicalTypeKind::Nominal { declaration, arguments } => {
            if !arguments.is_empty() {
                return Ok(NyarType::Apply(
                    Box::new(lower_type(program, *declaration)?),
                    arguments.iter().map(|argument| lower_type(program, *argument)).collect::<Result<Vec<_>>>()?,
                ));
            }
            let spelling = program
                .linked
                .type_surface_spellings
                .get(if *declaration == id { &id } else { declaration })
                .or_else(|| program.linked.type_surface_spellings.get(&id));
            if let Some(name) = spelling {
                if let Some(primitive) = NyarType::from_surface_primitive(name.as_str()) {
                    return Ok(primitive);
                }
                if program.linked.aggregate_layout_by_type.contains_key(&id)
                    || program.linked.nominal_instances.values().any(|record| record.ty == id)
                {
                    return Ok(NyarType::Named(name.clone()));
                }
                return Err(format!("nominal type `{name}` 缺少目标 layout 或 carrier 合同"));
            }
            if *declaration != id {
                return lower_type(program, *declaration);
            }
            Err(format!("canonical 类型尚无目标合同: Nominal {{ declaration: {declaration:?}, arguments: [] }}"))
        }
        CanonicalTypeKind::Function { parameters, return_type } => Ok(NyarType::Function(Box::new(NyarFunctionType {
            params: parameters.iter().map(|parameter| lower_type(program, *parameter)).collect::<Result<Vec<_>>>()?,
            return_type: lower_type(program, *return_type)?,
        }))),
        CanonicalTypeKind::Union(items) => {
            Ok(NyarType::Union(items.iter().map(|item| lower_type(program, *item)).collect::<Result<Vec<_>>>()?))
        }
        // `Intersection` 必须在进入平台表示计划前由 frontend 擦除（见
        // `frontend_contract::nyar_type` 的 `ConcretizeError`）。把它静默降成 `Union`
        // 会改变语义（`A & B` 不等于 `A | B`），属于错误成功路径，因此失败闭合。
        CanonicalTypeKind::Intersection(items) => {
            Err(format!("canonical 类型 Intersection 尚未在 frontend 擦除，不能进入平台表示计划: {items:?}"))
        }
        unsupported => {
            // Generic 类型只能出现在泛型函数的签名里——如果某个泛型函数被错误地留在了
            // 可达集并尝试编码，这里就会触发。打印引用 Generic 的闭合函数帮助定位。
            let canonical = program;
            let mut refs_debug = String::new();
            for (inst, func) in &program.mir.functions {
                // 跳过泛型声明函数
                let has_generic_sig = func.parameters.iter().any(|(_, tid)| {
                    canonical.linked.types.get(tid).is_some_and(|row| matches!(row.kind, nyar_types::CanonicalTypeKind::Generic { .. }))
                }) || canonical
                    .linked
                    .types
                    .get(&func.return_type)
                    .is_some_and(|row| matches!(row.kind, nyar_types::CanonicalTypeKind::Generic { .. }));
                if has_generic_sig {
                    continue;
                }
                // 递归检查 value_types 里有没有嵌套 Generic
                let mut found = false;
                for (_, tid) in &func.value_types {
                    if type_contains_generic_deep(canonical, *tid) {
                        found = true;
                        break;
                    }
                }
                if found {
                    let name = program.linked.callable_names.get(inst).cloned().unwrap_or_default();
                    refs_debug.push_str(&format!("function `{name}` instance {inst:?}; "));
                }
            }
            let mut type_debug = String::new();
            for (tid, row) in &program.linked.types {
                if matches!(row.kind, nyar_types::CanonicalTypeKind::Generic { .. }) {
                    type_debug.push_str(&format!("TypeId({:?})={:?}; ", tid, row.kind));
                }
            }
            return Err(format!(
                "canonical 类型尚无目标合同: {unsupported:?} (module={}, refs=[{}], generic_types=[{}])",
                program.mir.module_name, refs_debug, type_debug
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_types::{CanonicalBlockId, CanonicalFunction, CanonicalPrimitiveType, ItemId, TypeRecord};

    fn type_id(index: u32) -> nyar_types::TypeId {
        nyar_types::TypeId::from_index(index).expect("测试类型身份")
    }

    fn program_with_types(rows: Vec<(nyar_types::TypeId, CanonicalTypeKind)>) -> CanonicalProgram {
        let mut program = CanonicalProgram::default();
        for (id, kind) in rows {
            program.linked.types.insert(id, TypeRecord { declaration: id, kind });
        }
        program
    }

    fn function_with_signature(parameters: Vec<nyar_types::TypeId>, return_type: nyar_types::TypeId) -> nyar_types::CanonicalFunction {
        CanonicalFunction {
            instance: ItemInstanceId::from_index(0).expect("测试实例身份"),
            parameters: parameters
                .into_iter()
                .enumerate()
                .map(|(index, ty)| (nyar_types::MirValueId::from_index(u32::try_from(index).expect("测试值下标")).expect("测试值身份"), ty))
                .collect(),
            parameter_names: Vec::new(),
            return_type,
            value_types: BTreeMap::new(),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::new(),
        }
    }

    /// 签名含 Generic 的实例只能是失败原因，不能是「可以跳过」的判据。
    #[test]
    fn generic_signature_is_detected_in_parameter_and_nested_and_return() {
        let generic = CanonicalTypeKind::Generic { owner: ItemId::from_index(0).expect("测试声明身份"), ordinal: 0 };
        let program = program_with_types(vec![
            (type_id(0), generic.clone()),
            (type_id(1), CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true })),
            (type_id(2), CanonicalTypeKind::Tuple(vec![type_id(1), type_id(0)])),
        ]);
        assert!(signature_contains_generic(&program, &function_with_signature(vec![type_id(0)], type_id(1))), "直接泛型形参必须失败");
        assert!(signature_contains_generic(&program, &function_with_signature(vec![type_id(2)], type_id(1))), "嵌套泛型形参必须失败");
        assert!(signature_contains_generic(&program, &function_with_signature(vec![type_id(1)], type_id(0))), "泛型返回类型必须失败");
        assert!(!signature_contains_generic(&program, &function_with_signature(vec![type_id(1)], type_id(1))), "闭合标量签名不是泛型模板");
    }

    /// 窄整数与 `Character` 必须保留宽度与符号性：抹成 i32 后无法再区分掩码与符号扩展。
    #[test]
    fn narrow_integer_and_character_primitives_keep_their_identity() {
        let program = program_with_types(vec![
            (type_id(0), CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 8, signed: false })),
            (type_id(1), CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 16, signed: true })),
            (type_id(2), CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Character)),
        ]);
        assert_eq!(lower_type(&program, type_id(0)).expect("u8"), NyarType::Integer8 { signed: false });
        assert_eq!(lower_type(&program, type_id(1)).expect("i16"), NyarType::Integer16 { signed: true });
        assert_eq!(lower_type(&program, type_id(2)).expect("Character"), NyarType::Character);
    }

    /// `Intersection` 必须在 frontend 擦除；平台 lowering 静默降成 `Union` 会改变语义，
    /// 属错误成功路径，因此必须结构化失败闭合。
    #[test]
    fn intersection_type_fails_closed_instead_of_widening_to_union() {
        let program = program_with_types(vec![
            (type_id(0), CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true })),
            (type_id(1), CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool)),
            (type_id(2), CanonicalTypeKind::Intersection(vec![type_id(0), type_id(1)])),
        ]);
        let error = lower_type(&program, type_id(2)).expect_err("Intersection 不得被降成 Union");
        assert!(error.contains("Intersection"), "诊断必须指明未擦除的 Intersection，实际：{error}");
        assert!(error.contains("frontend"), "诊断必须指向 frontend 擦除责任，实际：{error}");
    }

    /// 文本常量进入 executable 计划时必须携带由编码变体决定的类型身份；
    /// canonical 层不保存名称，不能在这里回落 `None` 让后端再猜。
    #[test]
    fn text_constants_carry_their_encoding_identity() {
        assert_eq!(constant_encoding_type(&CanonicalConstant::Utf8("文本".to_string())), Some(NyarType::Utf8));
        assert_eq!(constant_encoding_type(&CanonicalConstant::Utf16("文本".to_string())), Some(NyarType::Utf16));
        assert_eq!(constant_encoding_type(&CanonicalConstant::Int(1)), None, "整数宽度不由常量变体单独声称");
        assert_eq!(constant_encoding_type(&CanonicalConstant::Unit), None, "Unit 不声称文本编码");
    }
}

fn lower_terminator(terminator: CanonicalTerminator) -> Result<Terminator> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    Ok(match terminator {
        CanonicalTerminator::Return { value: result } => Terminator::Return { value: result.map(value) },
        CanonicalTerminator::Jump { target, arguments } => {
            Terminator::Jump { target: BlockRef(target.0), arguments: arguments.into_iter().map(value).collect() }
        }
        CanonicalTerminator::Branch { condition, then_target, else_target } => {
            Terminator::Branch { condition: value(condition), then_target: BlockRef(then_target.0), else_target: BlockRef(else_target.0) }
        }
        CanonicalTerminator::Unreachable => Terminator::Unreachable,
        unsupported => {
            return Err(format!("canonical effect terminator 尚无目标合同: {unsupported:?}"));
        }
    })
}
