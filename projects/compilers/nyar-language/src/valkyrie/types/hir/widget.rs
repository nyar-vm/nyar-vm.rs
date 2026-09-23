//! Widget, widget lifecycle, and singleton definitions for HIR.

use super::{GenericType, HirDocumentation, HirExpr, HirField, HirFunction, HirParent, HirProperty, HirVisibility};
use crate::{Identifier, NamePath};

/// A widget in HIR.
///
/// Widgets are UI components that manage their own state and render
/// themselves to virtual DOM elements. Each widget must implement
/// a `render` method that returns an `Element`.
///
/// # State Management
///
/// Widget fields can be marked as state fields using the `@state` attribute
/// or by naming convention (prefix with `_` or `state_`). State fields
/// trigger re-rendering when modified.
///
/// # Lifecycle
///
/// Widgets support lifecycle methods:
/// - `on_mount`: Called when the widget is first created
/// - `on_unmount`: Called when the widget is destroyed
/// - `on_update`: Called when state changes
///
/// # Example
///
/// ```v
/// widget Counter {
///     _count: i32 = 0
///
///     fn render(self) -> Element {
///         Element("div")
///             .text(self._count.to_string())
///             .on("click", self.increment)
///     }
///
///     fn increment(mut self) {
///         self._count = self._count + 1
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirWidget {
    /// The name of the widget.
    pub name: Identifier,
    /// Documentation for the widget.
    pub doc: HirDocumentation,
    /// Generic parameters for the widget.
    pub generics: Vec<GenericType>,
    /// Fields of the widget (both state and non-state fields).
    pub fields: Vec<HirField>,
    /// Methods defined on the widget.
    pub methods: Vec<HirFunction>,
    /// Visibility of the widget.
    pub visibility: HirVisibility,
    /// Names of fields that are state fields.
    ///
    /// State fields trigger re-rendering when modified.
    /// This list is populated during semantic analysis.
    pub state_fields: Vec<Identifier>,
    /// Initial values for state fields.
    ///
    /// Maps field names to their default expressions.
    pub initial_state: Vec<(Identifier, HirExpr)>,
    /// Lifecycle hooks configuration.
    pub lifecycle: HirWidgetLifecycle,
}

/// Widget lifecycle configuration.
///
/// Defines the lifecycle methods that should be called
/// at various stages of the widget's existence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirWidgetLifecycle {
    /// Whether the widget has an `on_mount` lifecycle method.
    pub has_on_mount: bool,
    /// Whether the widget has an `on_unmount` lifecycle method.
    pub has_on_unmount: bool,
    /// Whether the widget has an `on_update` lifecycle method.
    pub has_on_update: bool,
    /// Whether the widget has a `before_update` lifecycle method.
    pub has_before_update: bool,
    /// Whether the widget has an `after_update` lifecycle method.
    pub has_after_update: bool,
}

/// A singleton in HIR.
///
/// Singletons are classes that have exactly one global instance.
/// They are useful for managing global state, configuration, or resources.
///
/// # Semantics
///
/// - A singleton has exactly one global instance.
/// - Generic singletons are forbidden: a singleton type parameterized by type
///   arguments cannot express "exactly one instance" because each instantiation
///   would need its own global slot. The `generics` field is retained for AST
///   fidelity only; semantic analysis rejects non-empty generics.
/// - An eager singleton (`is_lazy == false`) is initialized at module / type
///   static constructor time and cannot be unloaded; its lifetime equals the
///   application lifetime.
/// - A lazy singleton (`is_lazy == true`) defers initialization until first
///   access and supports `unload`: the finalizer (if any) is invoked, the
///   global slot is cleared, and the next access re-runs the constructor to
///   reactivate a fresh instance.
/// - Singleton members are accessed through the singleton name directly.
/// - Singletons cannot be instantiated by user-level construct expressions;
///   the unique instance is created only by the backend-generated
///   initialization path, which calls the user-defined constructor
///   (`init`) if present to carry extra initialization logic.
///
/// # Constructor and Finalizer
///
/// A singleton may define at most one `init` method (the constructor) and at
/// most one `finalize` method (the finalizer). The constructor runs when the
/// unique instance is created; the finalizer runs when a lazy singleton is
/// unloaded. Eager singletons never unload, so their finalizer (if defined)
/// is never invoked by the runtime.
///
/// # Example
///
/// ```v
/// singleton GlobalConfig {
///     host: String = "localhost"
///     port: i32 = 8080
///
///     micro get_url(self) -> String {
///         f"{self.host}:{self.port}"
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirSingleton {
    /// The name of the singleton.
    pub name: Identifier,
    /// 所属命名空间路径。
    pub namespace: Vec<Identifier>,
    /// Documentation for the singleton.
    pub doc: HirDocumentation,
    /// Generic parameters declared on the singleton.
    ///
    /// Retained for AST fidelity but rejected by semantic analysis: singleton
    /// semantics are incompatible with type parameterization because the
    /// single global instance slot cannot be keyed by type arguments.
    pub generics: Vec<GenericType>,
    /// Parent traits this singleton implements.
    pub parents: Vec<HirParent>,
    /// Fields of the singleton.
    pub fields: Vec<HirField>,
    /// Ordinary methods defined on the singleton (excludes constructor and
    /// finalizer, which are stored in `constructor` / `finalizer`).
    pub methods: Vec<HirFunction>,
    /// Properties (computed fields with getter/setter).
    pub properties: Vec<HirProperty>,
    /// Visibility of the singleton.
    pub visibility: HirVisibility,
    /// Traits to derive via the derive macro system.
    pub derives: Vec<NamePath>,
    /// When `true`, initialization is deferred until first access and the
    /// singleton supports `unload` / reactivate cycles.
    pub is_lazy: bool,
    /// The unique instance variable name for code generation.
    ///
    /// This is typically `INSTANCE`, used to store the global singleton instance.
    pub instance_name: Identifier,
    /// User-defined constructor (`init` method) that carries extra
    /// initialization logic for the unique instance.
    ///
    /// Invoked by the backend after allocating the instance and before storing
    /// it into the global slot. When `None`, the instance is initialized from
    /// field default values only.
    pub constructor: Option<Box<HirFunction>>,
    /// User-defined finalizer (`finalize` method) that releases resources
    /// held by the unique instance.
    ///
    /// Only invoked for lazy singletons during `unload`. Eager singletons
    /// never unload, so their finalizer is never called by the runtime.
    /// When `None`, unloading simply clears the global slot.
    pub finalizer: Option<Box<HirFunction>>,
}
