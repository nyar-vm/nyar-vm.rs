//! Struct, parent, and field definitions for HIR.

use super::{HirDocumentation, HirFunction, HirProperty, HirVisibility, ValkyrieType};
use crate::{Identifier, NamePath};

/// A parent class with optional alias for renamed inheritance.
///
/// This structure represents a parent class in an inheritance relationship,
/// supporting renamed inheritance where multiple parents can be disambiguated
/// using aliases.
///
/// # Example
///
/// ```v
/// class TeachingAssistant
///     primary: Teacher
///     secondary: Student
/// {
///     // Access via alias: self.primary.method()
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirParent {
    /// Parent class name path.
    pub name: NamePath,

    /// Optional alias for disambiguation (e.g., "primary" in "primary: Parent1").
    ///
    /// When multiple parents have conflicting members, the alias provides
    /// a way to explicitly specify which parent's member to access.
    pub alias: Option<Identifier>,

    /// Generic arguments for the parent class.
    pub generics: Vec<ValkyrieType>,

    /// Runtime offset for accessing parent fields in the object layout.
    ///
    /// This field is populated during code generation and indicates the
    /// byte offset from the start of the object to where the parent's
    /// fields begin. This is essential for multiple inheritance where
    /// each parent's fields are laid out at different offsets.
    ///
    /// A value of `None` indicates the offset has not yet been computed.
    pub offset: Option<usize>,
}

impl HirParent {
    /// Creates a new parent reference with the given name.
    pub fn new(name: NamePath) -> Self {
        Self { name, alias: None, generics: Vec::new(), offset: None }
    }

    /// Creates a parent reference with an alias for renamed inheritance.
    pub fn with_alias(name: NamePath, alias: Identifier) -> Self {
        Self { name, alias: Some(alias), generics: Vec::new(), offset: None }
    }

    /// Creates a parent reference with generic arguments.
    pub fn with_generics(name: NamePath, generics: Vec<ValkyrieType>) -> Self {
        Self { name, alias: None, generics, offset: None }
    }

    /// Creates a fully specified parent reference.
    pub fn full(name: NamePath, alias: Option<Identifier>, generics: Vec<ValkyrieType>) -> Self {
        Self { name, alias, generics, offset: None }
    }

    /// Sets the runtime offset for this parent.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Returns true if this parent has an alias.
    pub fn has_alias(&self) -> bool {
        self.alias.is_some()
    }

    /// Returns the alias or the parent name as a fallback.
    pub fn alias_or_name(&self) -> &str {
        self.alias.as_ref().map(|a| a.as_str()).unwrap_or_else(|| self.name.parts().first().map(|id| id.as_str()).unwrap_or(""))
    }

    /// Returns the storage slot name used to access this parent.
    ///
    /// Non-virtual inheritance always occupies a named slot. An explicit
    /// alias keeps that slot name; otherwise the simple parent type name
    /// is normalized to `snake_case`.
    pub fn slot_name(&self) -> Identifier {
        self.alias.clone().unwrap_or_else(|| {
            let base_name = self.name.parts().last().map(|id| id.as_str()).unwrap_or("");
            Identifier::new(&to_snake_case(base_name))
        })
    }
}

fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    let mut prev_is_lower_or_digit = false;

    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit && !result.is_empty() {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
        }
        else {
            result.push(ch);
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }

    result
}

/// A struct in HIR.
///
/// Structs are the primary data structure in Valkyrie, supporting both
/// value types (declared with `structure`) and reference types (declared with `class`).
///
/// # Derive Macros
///
/// Structs can use `@derive` attributes to automatically implement common traits:
///
/// ```v
/// @derive(Hash, Eq, Show)
/// structure Point {
///     x: i32
///     y: i32
/// }
/// ```
///
/// The derive macro system will generate the appropriate trait implementations
/// based on the struct's fields and the requested traits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirStruct {
    /// The name of the struct.
    pub name: Identifier,
    /// 所属命名空间路径（如 `["core", "text"]`）。
    ///
    /// 由 `namespace foo;` 声明设置，用于在 `TypeDef` 表中区分不同命名空间下的同名类型。
    /// 空向量表示全局命名空间。
    pub namespace: Vec<Identifier>,
    /// Documentation for the struct.
    pub doc: HirDocumentation,
    /// Generic parameters for the struct.
    pub generics: Vec<super::GenericType>,
    /// Parent classes this struct inherits from.
    pub parents: Vec<HirParent>,
    /// Fields of the struct.
    pub fields: Vec<HirField>,
    /// Methods defined directly on the struct.
    pub methods: Vec<HirFunction>,
    /// Properties (computed fields with getter/setter).
    pub properties: Vec<HirProperty>,
    /// Visibility of the struct.
    pub visibility: HirVisibility,
    /// Whether this is a value type (immutable, copied on assignment).
    ///
    /// Value types are declared with `structure` keyword and have copy semantics,
    /// while reference types are declared with `class` keyword and have reference semantics.
    pub is_value_type: bool,
    /// Whether this struct is abstract (cannot be instantiated directly).
    ///
    /// Abstract classes can declare abstract methods that must be implemented
    /// by concrete subclasses. They serve as base classes for inheritance.
    pub is_abstract: bool,
    /// Whether this struct is sealed (restricted inheritance).
    ///
    /// Sealed classes restrict which classes can inherit from them.
    /// All subclasses must be declared in the same file as the sealed class.
    /// This enables exhaustive pattern matching in match expressions.
    pub is_sealed: bool,
    /// Whether this struct is final (cannot be inherited).
    ///
    /// Final classes cannot be inherited by any other class.
    /// This is stronger than sealed, which allows same-file inheritance.
    pub is_final: bool,
    /// Whether this struct is open (can be inherited across modules).
    ///
    /// Open classes can be inherited by classes in other modules.
    /// This is the opposite of sealed/final - it explicitly allows
    /// cross-module inheritance.
    pub is_open: bool,
    /// List of abstract method names that concrete subclasses must implement.
    ///
    /// These are method identifiers declared as abstract in this class or
    /// inherited from abstract parent classes that haven't been implemented yet.
    pub abstract_methods: Vec<Identifier>,
    /// List of abstract property names that concrete subclasses must implement.
    ///
    /// These are property identifiers declared as abstract in this class or
    /// inherited from abstract parent classes that haven't been implemented yet.
    /// Each entry contains the property name and whether getter/setter is required.
    pub abstract_properties: Vec<AbstractPropertyRequirement>,
    /// Traits to derive via the derive macro system.
    ///
    /// This list is populated from `@derive(Trait1, Trait2)` attributes
    /// during the HIR lowering phase. The actual trait implementations
    /// are generated during compilation.
    pub derives: Vec<NamePath>,
}

impl HirStruct {
    /// Creates a new struct with the given name.
    pub fn new(name: Identifier) -> Self {
        Self { name, ..Default::default() }
    }

    /// Returns true if this struct is closed (cannot be inherited).
    ///
    /// A class is closed when it is neither open, sealed, nor abstract.
    /// This is the default behavior for classes without inheritance modifiers.
    ///
    /// # Inheritance Permission Matrix
    ///
    /// | Modifier   | Cross-module | Same-module | None |
    /// |------------|--------------|-------------|------|
    /// | open       | ✓            | ✓           | ✓    |
    /// | sealed     | ✗            | ✓           | ✗    |
    /// | abstract   | ✓            | ✓           | ✓    |
    /// | (none)     | ✗            | ✗           | ✗    |
    /// | final      | ✗            | ✗           | ✗    |
    pub fn is_closed(&self) -> bool {
        !self.is_open && !self.is_sealed && !self.is_abstract
    }

    /// Returns true if this struct allows inheritance from the given module.
    ///
    /// # Arguments
    ///
    /// * `child_module` - The module path of the child class attempting to inherit.
    /// * `parent_module` - The module path of this parent class.
    ///
    /// # Returns
    ///
    /// Returns `true` if inheritance is allowed, `false` otherwise.
    pub fn allows_inheritance_from(&self, child_module: &str, parent_module: &str) -> bool {
        if self.is_open || self.is_abstract {
            return true;
        }
        if self.is_sealed {
            return child_module == parent_module;
        }
        false
    }

    /// Returns the inheritance permission level for this struct.
    ///
    /// This method provides a clear indication of what kind of inheritance
    /// is permitted for this class.
    pub fn inheritance_permission(&self) -> InheritancePermission {
        if self.is_open || self.is_abstract {
            InheritancePermission::Allowed
        }
        else if self.is_sealed {
            InheritancePermission::SameModuleOnly
        }
        else {
            InheritancePermission::Blocked
        }
    }

    /// Returns the parent slot names in declaration order.
    pub fn parent_slot_names(&self) -> Vec<Identifier> {
        self.parents.iter().map(HirParent::slot_name).collect()
    }

    /// Returns duplicated parent slot names, if any.
    pub fn duplicate_parent_slots(&self) -> Vec<Identifier> {
        let mut counts = std::collections::BTreeMap::new();
        for slot in self.parent_slot_names() {
            *counts.entry(slot).or_insert(0usize) += 1;
        }
        counts.into_iter().filter_map(|(slot, count)| (count > 1).then_some(slot)).collect()
    }

    /// Returns parent slot names that conflict with owned field names.
    pub fn parent_slot_field_conflicts(&self) -> Vec<Identifier> {
        let field_names = self.fields.iter().map(|field| field.name.clone()).collect::<std::collections::BTreeSet<_>>();
        self.parent_slot_names().into_iter().filter(|slot| field_names.contains(slot)).collect()
    }
}

/// Inheritance permission levels for classes.
///
/// This enum represents the different levels of inheritance permission
/// that a class can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InheritancePermission {
    /// Inheritance is allowed from any module.
    ///
    /// This applies to `open` and `abstract` classes.
    Allowed,
    /// Inheritance is only allowed within the same module.
    ///
    /// This applies to `sealed` classes.
    SameModuleOnly,
    /// Inheritance is blocked.
    ///
    /// This is the default for classes without modifiers,
    /// and for `final` classes (deprecated).
    Blocked,
}

/// Requirement for abstract property implementation.
///
/// Describes what a concrete subclass must implement for an abstract property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AbstractPropertyRequirement {
    /// The name of the abstract property.
    pub name: Identifier,
    /// Whether a getter implementation is required.
    pub requires_getter: bool,
    /// Whether a setter implementation is required.
    pub requires_setter: bool,
    /// The type of the property.
    pub ty: ValkyrieType,
    /// The parent class that declared this abstract property.
    pub parent_class: Option<Identifier>,
}

impl AbstractPropertyRequirement {
    /// Creates a new abstract property requirement.
    pub fn new(name: Identifier, requires_getter: bool, requires_setter: bool, ty: ValkyrieType) -> Self {
        Self { name, requires_getter, requires_setter, ty, parent_class: None }
    }

    /// Creates a requirement for a read-only property (getter only).
    pub fn readonly(name: Identifier, ty: ValkyrieType) -> Self {
        Self::new(name, true, false, ty)
    }

    /// Creates a requirement for a read-write property (getter and setter).
    pub fn readwrite(name: Identifier, ty: ValkyrieType) -> Self {
        Self::new(name, true, true, ty)
    }

    /// Sets the parent class that declared this requirement.
    pub fn with_parent(mut self, parent: Identifier) -> Self {
        self.parent_class = Some(parent);
        self
    }
}

/// A field in a struct.
///
/// Fields represent the data members of a struct or class.
///
/// # Mutability
///
/// Fields are immutable by default. Mark a field with the `mut` modifier to
/// indicate it may be written from `mut self` methods:
///
/// ```v
/// singleton Counter {
///     mut total: i64 = 0
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirField {
    /// The name of the field.
    pub name: Identifier,
    /// Documentation for the field.
    pub doc: HirDocumentation,
    /// The type of the field.
    pub ty: ValkyrieType,
    /// Visibility of the field.
    pub visibility: HirVisibility,
    /// Whether this field is explicitly marked `mut` and may be written from `mut self` methods.
    pub is_mutable: bool,
}
