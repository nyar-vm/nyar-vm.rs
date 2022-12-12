//! Module and documentation definitions for HIR.

use super::{
    HirEnum, HirFlags, HirFunction, HirImpl, HirSingleton, HirStatement, HirStruct, HirTrait, HirWidget, ValkyrieType,
    type_family::{HirTypeFamily, HirTypeFunction},
};
use crate::{Identifier, NamePath, SourceSpan};

/// A module in HIR.
///
/// Modules are the top-level organizational unit in Valkyrie,
/// containing functions, types, and other definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirModule {
    /// The fully qualified name path of the module.
    pub name: NamePath,
    /// Documentation for the module.
    pub doc: HirDocumentation,
    /// Import statements in this module.
    pub imports: Vec<HirImport>,
    /// Non-fatal compile warnings collected during lowering.
    #[cfg_attr(feature = "serde", serde(skip, default = "Vec::new"))]
    pub warnings: Vec<HirCompileWarning>,
    /// Nested submodules.
    pub submodules: Vec<HirModule>,
    /// Functions defined in this module.
    pub functions: Vec<HirFunction>,
    /// Structs defined in this module.
    pub structs: Vec<HirStruct>,
    /// Enums defined in this module.
    pub enums: Vec<HirEnum>,
    /// Nominal sum definitions exported by resolved dependencies.  This is
    /// legacy migration storage. Formal compilation must use
    /// `imported_nominal_exports`, which retains exporter provenance.
    #[cfg_attr(feature = "serde", serde(default))]
    pub imported_enums: Vec<HirEnum>,
    /// Complete nominal metadata explicitly exported by resolved dependencies.
    /// Consumers may use an imported variant only when its exporter and layout
    /// are both available through this structured boundary.
    #[cfg_attr(feature = "serde", serde(default))]
    pub imported_semantic_exports: Vec<HirDependencySemanticExport>,
    /// Flags types defined in this module.
    pub flags: Vec<HirFlags>,
    /// Traits defined in this module.
    pub traits: Vec<HirTrait>,
    /// Impl blocks defined in this module.
    pub impls: Vec<HirImpl>,
    /// Type functions defined in this module.
    pub type_functions: Vec<HirTypeFunction>,
    /// Type families defined in this module.
    pub type_families: Vec<HirTypeFamily>,
    /// Widgets defined in this module.
    pub widgets: Vec<HirWidget>,
    /// Singletons defined in this module.
    pub singletons: Vec<HirSingleton>,
    /// Top-level statements in this module.
    pub statements: Vec<HirStatement>,
    /// `type Alias = Target` declarations in this module.
    pub type_aliases: Vec<HirTypeAlias>,
}

/// A resolved `using` import in HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirImport {
    /// Imported module path.
    pub path: NamePath,
    /// Optional module alias (`using path as Alias`).
    pub alias: Option<Identifier>,
    /// Selective import bindings.
    pub bindings: Vec<HirImportBinding>,
    /// Whether this is a glob import.
    pub glob: bool,
}

/// One selective import binding inside `using path.{ ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirImportBinding {
    /// Imported symbol name.
    pub name: Identifier,
    /// Optional local alias.
    pub alias: Option<Identifier>,
}

/// A non-fatal compile warning attached to a module.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirCompileWarning {
    /// Stable warning code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Source location.
    pub span: SourceSpan,
}

/// A module-scoped type alias in HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirTypeAlias {
    /// Alias name.
    pub name: Identifier,
    /// Generic parameter names (e.g. `T` in `type Alias<T> = Result<T, E>`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub generics: Vec<Identifier>,
    /// Resolved target type (may still contain the alias's own type parameters).
    pub target: ValkyrieType,
    /// Source span for error reporting.
    pub span: SourceSpan,
}

impl Default for HirModule {
    fn default() -> Self {
        Self {
            name: NamePath::new(vec![Identifier::new("main")]),
            doc: HirDocumentation::default(),
            imports: Vec::new(),
            warnings: Vec::new(),
            submodules: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            imported_enums: Vec::new(),
            imported_semantic_exports: Vec::new(),
            flags: Vec::new(),
            traits: Vec::new(),
            impls: Vec::new(),
            type_functions: Vec::new(),
            type_families: Vec::new(),
            widgets: Vec::new(),
            singletons: Vec::new(),
            statements: Vec::new(),
            type_aliases: Vec::new(),
        }
    }
}

/// Structured semantic metadata exported by one resolved dependency module.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirDependencySemanticExport {
    /// Fully qualified module that owns these definitions.
    pub module: NamePath,
    /// Complete exported function signatures and call contracts.
    pub functions: Vec<HirFunction>,
    /// Complete aggregate declarations, including storage and field layout.
    pub structs: Vec<HirStruct>,
    /// Complete nominal sum definitions, including generic and variant layout.
    pub enums: Vec<HirEnum>,
    /// Exported trait contracts and witness requirements.
    pub traits: Vec<HirTrait>,
    /// Exported type aliases with resolved generic targets.
    pub type_aliases: Vec<HirTypeAlias>,
    /// Exported `imply` / trait-impl method contracts (signatures for overload).
    ///
    /// Bodies stay owned by the exporting module's MIR; consumers resolve calls
    /// through these contracts, then Stage1 links reachable MIR bodies.
    pub impls: Vec<HirImpl>,
}

impl HirModule {
    /// Iterates nominal sums available through the formal dependency-export
    /// boundary. The legacy `imported_enums` field is intentionally excluded.
    pub fn imported_nominal_enums(&self) -> impl Iterator<Item = &HirEnum> {
        self.imported_semantic_exports.iter().flat_map(|export| export.enums.iter())
    }
}

/// Documentation for HIR items.
///
/// Documentation is stored as a collection of lines, typically
/// extracted from doc comments (`///` or `/** */`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirDocumentation {
    /// The documentation lines.
    pub lines: Vec<String>,
}

impl HirDocumentation {
    /// Creates documentation from multiple lines.
    pub fn from_lines(lines: Vec<String>) -> Self {
        Self { lines }
    }

    /// Creates documentation from a single line.
    pub fn from_single(line: impl Into<String>) -> Self {
        Self { lines: vec![line.into()] }
    }

    /// Returns true if the documentation is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
