#![allow(missing_docs)]

use crate::types::{
    Identifier, NamePath, SourceID, SourceSpan,
    hir::{
        HirAssociatedTypeImpl, HirAttribute, HirBlock, HirDocumentation, HirFunction, HirImpl, HirModule, HirStruct, HirVisibility,
        ValkyrieType,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    FieldMissingTrait { field: Identifier, field_type: String, required_trait: String },
    Conflict { target: Identifier, trait_name: String, location: String },
    UnknownTrait { trait_name: String, available: Vec<String> },
    AbstractType { target: Identifier, trait_name: String },
}

impl DeriveError {
    pub fn field_missing_trait(field: Identifier, field_type: &str, required_trait: &str) -> Self {
        Self::FieldMissingTrait { field, field_type: field_type.to_string(), required_trait: required_trait.to_string() }
    }

    pub fn conflict(target: Identifier, trait_name: &str, location: &str) -> Self {
        Self::Conflict { target, trait_name: trait_name.to_string(), location: location.to_string() }
    }

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
            DeriveError::AbstractType { target, trait_name } => write!(f, "抽象类型 `{target}` 不能自动派生 `{trait_name}`"),
        }
    }
}

impl std::error::Error for DeriveError {}

#[derive(Debug, Clone, Default)]
pub struct DeriveResult {
    pub impls: Vec<HirImpl>,
    pub errors: Vec<DeriveError>,
}

impl DeriveResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn add_error(&mut self, error: DeriveError) {
        self.errors.push(error);
    }
}

#[derive(Debug, Clone, Default)]
pub struct InjectionStats {
    pub structs_processed: usize,
    pub structs_skipped: usize,
    pub impls_generated: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InjectionResult {
    pub impls: Vec<HirImpl>,
    pub errors: Vec<DeriveError>,
    pub stats: InjectionStats,
}

impl InjectionResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_impls(&self) -> bool {
        !self.impls.is_empty()
    }

    pub fn merge(&mut self, other: InjectionResult) {
        self.impls.extend(other.impls);
        self.errors.extend(other.errors);
        self.stats.structs_processed += other.stats.structs_processed;
        self.stats.structs_skipped += other.stats.structs_skipped;
        self.stats.impls_generated += other.stats.impls_generated;
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeriveUsageAnalysis {
    struct_count: usize,
    total_derive_requests: usize,
}

impl DeriveUsageAnalysis {
    pub fn struct_count(&self) -> usize {
        self.struct_count
    }

    pub fn total_derive_requests(&self) -> usize {
        self.total_derive_requests
    }
}

pub trait BuiltinDerive {
    fn name(&self) -> &'static str;

    fn can_derive(&self, target: &HirStruct) -> Result<(), DeriveError> {
        if target.is_abstract {
            return Err(DeriveError::AbstractType { target: target.name.clone(), trait_name: self.name().to_string() });
        }
        Ok(())
    }

    fn derive(&self, target: &HirStruct) -> Result<Vec<HirImpl>, DeriveError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CloneDerive;

impl CloneDerive {
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

#[derive(Debug, Clone, Copy, Default)]
pub struct DebugDerive;

impl DebugDerive {
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

#[derive(Debug, Clone, Copy, Default)]
pub struct HashDerive;

impl HashDerive {
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

#[derive(Debug, Clone, Copy, Default)]
pub struct EqDerive;

impl EqDerive {
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

pub fn generate_eq_impl(target: &HirStruct) -> HirImpl {
    make_impl(target, "Eq", vec![])
}

#[derive(Debug, Clone, Default)]
pub struct DeriveRegistry;

pub fn create_builtin_registry() -> DeriveRegistry {
    DeriveRegistry
}

impl DeriveRegistry {
    pub fn find(&self, name: &str) -> Option<&'static str> {
        match name {
            "Clone" => Some("Clone"),
            "Debug" => Some("Debug"),
            "Eq" => Some("Eq"),
            "Hash" => Some("Hash"),
            _ => None,
        }
    }

    pub fn available_derives(&self) -> Vec<&'static str> {
        vec!["Debug", "Clone", "Eq", "Hash"]
    }

    pub fn can_derive(&self, target: &HirStruct, name: &str) -> Result<(), DeriveError> {
        let derive = self.require(name)?;
        derive.can_derive(target)
    }

    pub fn derive_trait(&self, target: &HirStruct, trait_path: &NamePath, existing_impls: &[HirImpl]) -> Result<Vec<HirImpl>, DeriveError> {
        let trait_name = trait_path.parts().last().map(|part| part.as_str()).unwrap_or("");

        if existing_impls.iter().any(|impl_block| impl_block.trait_path.as_ref() == Some(trait_path)) {
            return Err(DeriveError::conflict(target.name.clone(), trait_name, "existing impl"));
        }

        self.require(trait_name)?.derive(target)
    }

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

#[derive(Debug, Clone, Default)]
pub struct DeriveInjector {
    registry: DeriveRegistry,
    errors: Vec<DeriveError>,
}

impl DeriveInjector {
    pub fn new() -> Self {
        Self { registry: create_builtin_registry(), errors: Vec::new() }
    }

    pub fn available_derives(&self) -> Vec<&'static str> {
        self.registry.available_derives()
    }

    pub fn errors(&self) -> &[DeriveError] {
        &self.errors
    }

    pub fn can_derive(&self, target: &HirStruct, name: &str, existing_impls: &[HirImpl]) -> Result<(), DeriveError> {
        if existing_impls.iter().any(|impl_block| impl_block.trait_path.as_ref().is_some_and(|path| path.to_string() == name)) {
            return Err(DeriveError::conflict(target.name.clone(), name, "existing impl"));
        }

        self.registry.can_derive(target, name)
    }

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
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::<HirAttribute>::new(),
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
