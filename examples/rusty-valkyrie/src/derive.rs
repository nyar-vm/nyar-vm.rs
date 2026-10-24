//! 内置 derive 语义：为带派生请求的 HIR 结构体生成 `Clone` / `Debug` / `Eq` / `Hash` 等 impl。

use crate::types::{
    Identifier, NamePath, SourceID, SourceSpan,
    hir::{
        HirAssociatedTypeImpl, HirAttribute, HirBlock, HirDocumentation, HirFunction, HirImpl, HirModule, HirStruct, HirVisibility,
        ValkyrieType,
    },
};

/// 派生（derive）过程中的错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    /// 字段类型未实现派生所需的 trait。
    FieldMissingTrait {
        /// 未满足 trait 约束的字段名。
        field: Identifier,
        /// 该字段的类型描述。
        field_type: String,
        /// 该字段需要实现的 trait 名。
        required_trait: String,
    },
    /// 目标类型已手动实现该 trait，派生会与之冲突。
    Conflict {
        /// 发生冲突的目标类型。
        target: Identifier,
        /// 冲突的 trait 名。
        trait_name: String,
        /// 冲突来源描述（如 `existing impl`）。
        location: String,
    },
    /// 请求了不认识的派生 trait。
    UnknownTrait {
        /// 未知的 trait 名。
        trait_name: String,
        /// 当前可用的 trait 名列表。
        available: Vec<String>,
    },
    /// 抽象类型不能自动派生该 trait。
    AbstractType {
        /// 抽象类型名。
        target: Identifier,
        /// 被拒绝派生的 trait 名。
        trait_name: String,
    },
}

impl DeriveError {
    /// 构造「字段缺少 trait 实现」错误。
    pub fn field_missing_trait(field: Identifier, field_type: &str, required_trait: &str) -> Self {
        Self::FieldMissingTrait { field, field_type: field_type.to_string(), required_trait: required_trait.to_string() }
    }

    /// 构造「已有手动实现」冲突错误。
    pub fn conflict(target: Identifier, trait_name: &str, location: &str) -> Self {
        Self::Conflict { target, trait_name: trait_name.to_string(), location: location.to_string() }
    }

    /// 构造「未知派生 trait」错误。
    pub fn unknown_trait(trait_name: &str, available: Vec<String>) -> Self {
        Self::UnknownTrait { trait_name: trait_name.to_string(), available }
    }
}

impl std::fmt::Display for DeriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeriveError::FieldMissingTrait { field, field_type, required_trait } => {
                write!(f, "字段 `{field}: {field_type}` 未实现 `{required_trait}` trait")
            }
            DeriveError::Conflict { target, trait_name, location } => {
                write!(f, "类型 `{target}` 已在{location}手动实现 `{trait_name}`")
            }
            DeriveError::UnknownTrait { trait_name, available } => {
                write!(f, "未知的派生 trait `{trait_name}`，可用 trait: {}", available.join(", "))
            }
            DeriveError::AbstractType { target, trait_name } => {
                write!(f, "抽象类型 `{target}` 不能自动派生 `{trait_name}`")
            }
        }
    }
}

impl std::error::Error for DeriveError {}

/// 一次派生运行的聚合结果。
#[derive(Debug, Clone, Default)]
pub struct DeriveResult {
    /// 生成的 impl 块。
    pub impls: Vec<HirImpl>,
    /// 派生过程中产生的错误。
    pub errors: Vec<DeriveError>,
}

impl DeriveResult {
    /// 创建空结果。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否没有任何错误。
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// 是否产生了错误。
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// 追加一条错误。
    pub fn add_error(&mut self, error: DeriveError) {
        self.errors.push(error);
    }
}

/// 派生注入的统计计数。
#[derive(Debug, Clone, Default)]
pub struct InjectionStats {
    /// 处理过的结构体数量。
    pub structs_processed: usize,
    /// 因无派生需求而跳过的结构体数量。
    pub structs_skipped: usize,
    /// 生成的 impl 块总数。
    pub impls_generated: usize,
}

/// 一次派生注入的结果，包含生成物、错误与统计。
#[derive(Debug, Clone, Default)]
pub struct InjectionResult {
    /// 注入生成的 impl 块。
    pub impls: Vec<HirImpl>,
    /// 注入过程中产生的错误。
    pub errors: Vec<DeriveError>,
    /// 本次注入的统计计数。
    pub stats: InjectionStats,
}

impl InjectionResult {
    /// 创建空结果。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否没有任何错误。
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// 是否产生了错误。
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// 是否生成了 impl 块。
    pub fn has_impls(&self) -> bool {
        !self.impls.is_empty()
    }

    /// 合并另一个结果，累加其 impl、错误与统计。
    pub fn merge(&mut self, other: InjectionResult) {
        self.impls.extend(other.impls);
        self.errors.extend(other.errors);
        self.stats.structs_processed += other.stats.structs_processed;
        self.stats.structs_skipped += other.stats.structs_skipped;
        self.stats.impls_generated += other.stats.impls_generated;
    }
}

/// 模块级派生使用情况统计。
#[derive(Debug, Clone, Default)]
pub struct DeriveUsageAnalysis {
    struct_count: usize,
    total_derive_requests: usize,
}

impl DeriveUsageAnalysis {
    /// 扫描到的结构体数量。
    pub fn struct_count(&self) -> usize {
        self.struct_count
    }

    /// 累计的派生请求数量。
    pub fn total_derive_requests(&self) -> usize {
        self.total_derive_requests
    }
}

/// 内置派生（derive）实现者的统一接口。
pub trait BuiltinDerive {
    /// 该派生对应的 trait 名（如 `Clone`）。
    fn name(&self) -> &'static str;

    /// 判断目标结构体是否可派生，抽象类型默认被拒绝。
    fn can_derive(&self, target: &HirStruct) -> Result<(), DeriveError> {
        if target.is_abstract {
            return Err(DeriveError::AbstractType { target: target.name.clone(), trait_name: self.name().to_string() });
        }
        Ok(())
    }

    /// 为目标结构体生成对应的 impl 块。
    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError>;
}

/// `Clone` 派生实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct CloneDerive;

impl CloneDerive {
    /// 创建一个 `Clone` 派生器。
    pub fn new() -> Self {
        Self
    }
}

impl BuiltinDerive for CloneDerive {
    fn name(&self) -> &'static str {
        "Clone"
    }

    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError> {
        self.can_derive(target)?;
        Ok(vec![make_impl(target, "Clone", vec![make_method("clone")])])
    }
}

/// `Debug` 派生实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct DebugDerive;

impl DebugDerive {
    /// 创建一个 `Debug` 派生器。
    pub fn new() -> Self {
        Self
    }
}

impl BuiltinDerive for DebugDerive {
    fn name(&self) -> &'static str {
        "Debug"
    }

    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError> {
        self.can_derive(target)?;
        Ok(vec![make_impl(target, "Debug", vec![make_method("format")])])
    }
}

/// `Hash` 派生实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct HashDerive;

impl HashDerive {
    /// 创建一个 `Hash` 派生器。
    pub fn new() -> Self {
        Self
    }
}

impl BuiltinDerive for HashDerive {
    fn name(&self) -> &'static str {
        "Hash"
    }

    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError> {
        self.can_derive(target)?;
        Ok(vec![make_impl(target, "Hash", vec![make_method("hash")])])
    }
}

/// `Eq` 派生实现，同时生成 `PartialEq` 与 `Eq`。
#[derive(Debug, Clone, Copy, Default)]
pub struct EqDerive;

impl EqDerive {
    /// 创建一个 `Eq` 派生器。
    pub fn new() -> Self {
        Self
    }
}

impl BuiltinDerive for EqDerive {
    fn name(&self) -> &'static str {
        "Eq"
    }

    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError> {
        self.can_derive(target)?;
        Ok(vec![make_impl(target, "PartialEq", vec![make_method("eq")]), generate_eq_impl(target)])
    }
}

/// 为目标结构体生成空的 `Eq` impl 块。
pub fn generate_eq_impl(target: &HirStruct) -> HirImpl {
    make_impl(target, "Eq", vec![])
}

/// 内置派生 trait 的注册表。
#[derive(Debug, Clone, Default)]
pub struct DeriveRegistry;

/// 创建包含全部内置派生 trait 的注册表。
pub fn create_builtin_registry() -> DeriveRegistry {
    DeriveRegistry
}

impl DeriveRegistry {
    /// 按名查找内置派生 trait，返回其规范名。
    pub fn find(&self, name: &str) -> Option<&'static str> {
        match name {
            "Clone" => Some("Clone"),
            "Debug" => Some("Debug"),
            "Eq" => Some("Eq"),
            "Hash" => Some("Hash"),
            _ => None,
        }
    }

    /// 列出所有可用的内置派生 trait 名。
    pub fn available_derives(&self) -> Vec<&'static str> {
        vec!["Debug", "Clone", "Eq", "Hash"]
    }

    /// 判断目标结构体能否派生具名 trait。
    pub fn can_derive(&self, target: &HirStruct, name: &str) -> Result<(), DeriveError> {
        let derive = self.require(name)?;
        derive.can_derive(target)
    }

    /// 为指定 trait 路径执行派生，已有实现则报冲突。
    pub fn derive_trait(&self, target: &HirStruct, trait_path: &NamePath, existing_impls: &[HirImpl]) -> Result<Vec<HirImpl>, DeriveError> {
        let trait_name = trait_path.parts().last().map(|part| part.as_str()).unwrap_or("");

        if existing_impls.iter().any(|impl_block| impl_block.trait_path.as_ref() == Some(trait_path)) {
            return Err(DeriveError::conflict(target.name.clone(), trait_name, "existing impl"));
        }

        self.require(trait_name)?.derive(target)
    }

    /// 依次派生多个 trait，聚合成功产物与错误。
    pub fn derive_all(&self, target: &HirStruct, traits: &[NamePath], existing_impls: &[HirImpl]) -> DeriveResult {
        let mut result = DeriveResult::new();

        for trait_path in traits {
            match self.derive_trait(target, trait_path, existing_impls) {
                Ok(mut impls) => result.impls.append(&mut impls),
                Err(error) => result.errors.push(error),
            }
        }

        result
    }

    fn require(&self, name: &str) -> Result<DeriveHandle, DeriveError> {
        match name {
            "Clone" => Ok(DeriveHandle::Clone(CloneDerive::new())),
            "Debug" => Ok(DeriveHandle::Debug(DebugDerive::new())),
            "Eq" => Ok(DeriveHandle::Eq(EqDerive::new())),
            "Hash" => Ok(DeriveHandle::Hash(HashDerive::new())),
            _ => Err(DeriveError::unknown_trait(name, self.available_derives().into_iter().map(str::to_string).collect())),
        }
    }
}

enum DeriveHandle {
    Clone(CloneDerive),
    Debug(DebugDerive),
    Eq(EqDerive),
    Hash(HashDerive),
}

impl DeriveHandle {
    fn can_derive(&self, target: &HirStruct) -> Result<(), DeriveError> {
        match self {
            DeriveHandle::Clone(derive) => derive.can_derive(target),
            DeriveHandle::Debug(derive) => derive.can_derive(target),
            DeriveHandle::Eq(derive) => derive.can_derive(target),
            DeriveHandle::Hash(derive) => derive.can_derive(target),
        }
    }

    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError> {
        match self {
            DeriveHandle::Clone(derive) => derive.derive(target),
            DeriveHandle::Debug(derive) => derive.derive(target),
            DeriveHandle::Eq(derive) => derive.derive(target),
            DeriveHandle::Hash(derive) => derive.derive(target),
        }
    }
}

/// 派生注入器：为模块内的结构体自动补全派生 impl。
#[derive(Debug, Clone, Default)]
pub struct DeriveInjector {
    registry: DeriveRegistry,
    errors: Vec<DeriveError>,
}

impl DeriveInjector {
    /// 创建注入器，内部使用内置派生注册表。
    pub fn new() -> Self {
        Self { registry: create_builtin_registry(), errors: Vec::new() }
    }

    /// 列出可用的内置派生 trait 名。
    pub fn available_derives(&self) -> Vec<&'static str> {
        self.registry.available_derives()
    }

    /// 返回注入过程中累计的错误。
    pub fn errors(&self) -> &[DeriveError] {
        &self.errors
    }

    /// 判断结构体能否派生具名 trait，已存在的实现会报冲突。
    pub fn can_derive(&self, target: &HirStruct, name: &str, existing_impls: &[HirImpl]) -> Result<(), DeriveError> {
        if existing_impls.iter().any(|impl_block| impl_block.trait_path.as_ref().is_some_and(|path| path.to_string() == name)) {
            return Err(DeriveError::conflict(target.name.clone(), name, "existing impl"));
        }

        self.registry.can_derive(target, name)
    }

    /// 为单个结构体注入派生 impl，并返回本次结果。
    pub fn inject_for_struct(&mut self, target: &HirStruct, existing_impls: &[HirImpl]) -> InjectionResult {
        let mut result = InjectionResult::new();

        if target.derives.is_empty() {
            result.stats.structs_skipped = 1;
            return result;
        }

        result.stats.structs_processed = 1;

        for trait_path in &target.derives {
            match self.registry.derive_trait(target, trait_path, existing_impls) {
                Ok(mut impls) => {
                    result.stats.impls_generated += impls.len();
                    result.impls.append(&mut impls);
                }
                Err(error) => {
                    self.errors.push(error.clone());
                    result.errors.push(error);
                }
            }
        }

        result
    }

    /// 递归为模块（含子模块）内的结构体注入派生 impl。
    pub fn inject_derives(&mut self, module: &mut HirModule) -> InjectionResult {
        let mut result = InjectionResult::new();

        for strukt in &module.structs {
            if strukt.derives.is_empty() {
                result.stats.structs_skipped += 1;
                continue;
            }

            let struct_result = self.inject_for_struct(strukt, &module.impls);
            result.stats.structs_processed += struct_result.stats.structs_processed;
            result.stats.structs_skipped += struct_result.stats.structs_skipped;
            result.stats.impls_generated += struct_result.stats.impls_generated;
            result.errors.extend(struct_result.errors.clone());
            result.impls.extend(struct_result.impls.clone());
            module.impls.extend(struct_result.impls);
        }

        for submodule in &mut module.submodules {
            let sub_result = self.inject_derives(submodule);
            result.merge(sub_result);
        }

        result
    }

    /// 统计模块（含子模块）内结构体数量与派生请求数量。
    pub fn analyze_derive_usage(&self, module: &HirModule) -> DeriveUsageAnalysis {
        fn visit(module: &HirModule, analysis: &mut DeriveUsageAnalysis) {
            analysis.struct_count += module.structs.len();
            analysis.total_derive_requests += module.structs.iter().map(|item| item.derives.len()).sum::<usize>();
            for submodule in &module.submodules {
                visit(submodule, analysis);
            }
        }

        let mut analysis = DeriveUsageAnalysis::default();
        visit(module, &mut analysis);
        analysis
    }
}

fn make_impl(target: &HirStruct, trait_name: &str, methods: Vec<HirFunction>) -> HirImpl {
    HirImpl {
        trait_declaration: None,
        declaring_namespace: NamePath::default(),
        trait_arguments: Vec::new(),
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(target.name.clone()),
        trait_path: Some(NamePath::new(vec![Identifier::new(trait_name)])),
        methods,
        associated_type_impls: Vec::<HirAssociatedTypeImpl>::new(),
        associated_const_impls: vec![],
    }
}

fn make_method(name: &str) -> HirFunction {
    HirFunction {
        declaration: None,
        instance: None,
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::<HirAttribute>::new(),
        where_constraints: Vec::new(),
        generics: vec![],
        params: vec![],
        return_type: ValkyrieType::Unit,
        body: HirBlock { statements: vec![], expr: None, span: empty_span() },
        span: empty_span(),
        visibility: HirVisibility::public(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

fn empty_span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}
