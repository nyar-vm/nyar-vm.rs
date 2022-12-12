//! Property definitions for HIR.

use super::{HirDocumentation, HirFunction, HirVisibility, ValkyrieType};
use crate::Identifier;

/// A property in a struct (computed field with getter/setter).
///
/// Properties provide controlled access to computed values, with
/// optional getter and setter methods. They appear as fields to
/// external code but are implemented as method calls.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirProperty {
    /// The name of the property.
    pub name: Identifier,
    /// Documentation for the property.
    pub doc: HirDocumentation,
    /// The type of the property.
    pub ty: ValkyrieType,
    /// The getter function for the property.
    pub getter: Option<HirFunction>,
    /// The setter function for the property.
    pub setter: Option<HirFunction>,
    /// Whether this property is read-only (no setter).
    pub is_readonly: bool,
    /// Visibility of the property.
    pub visibility: HirVisibility,
    /// Whether this property is abstract (has no body implementation).
    ///
    /// Abstract properties are declared without a body in abstract classes
    /// and must be implemented by concrete subclasses.
    pub is_abstract: bool,
    /// Whether this property is final (cannot be overridden).
    ///
    /// Final properties cannot be overridden by subclasses.
    pub is_final: bool,
    /// Whether this property is static (belongs to the class, not instances).
    ///
    /// Static properties are accessed via `ClassName.property_name` syntax
    /// and do not have access to `self`.
    pub is_static: bool,
    /// Whether this property is virtual (can be overridden by subclasses).
    ///
    /// Virtual properties use dynamic dispatch through the witness table,
    /// allowing subclasses to provide their own implementation.
    pub is_virtual: bool,
    /// Whether this property overrides a parent class property.
    ///
    /// Override properties must match the signature of the parent property
    /// and are verified during type checking.
    pub is_override: bool,
    /// Whether this property uses lazy initialization.
    ///
    /// Lazy properties cache their computed value after first access.
    /// The getter is only called once, and subsequent accesses return
    /// the cached value.
    pub is_lazy: bool,
    /// The name of the backing field for lazy properties.
    ///
    /// For lazy properties, this field stores the cached value.
    /// The field is initialized to a sentinel value indicating "not computed".
    pub lazy_backing_field: Option<Identifier>,
}

impl HirProperty {
    /// Creates a new property with the given name and type.
    pub fn new(name: Identifier, ty: ValkyrieType) -> Self {
        Self {
            name,
            doc: HirDocumentation::default(),
            ty,
            getter: None,
            setter: None,
            is_readonly: true,
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: false,
            is_static: false,
            is_virtual: false,
            is_override: false,
            is_lazy: false,
            lazy_backing_field: None,
        }
    }

    /// Creates a static property.
    pub fn static_property(name: Identifier, ty: ValkyrieType) -> Self {
        Self::new(name, ty).with_static(true)
    }

    /// Creates an abstract property.
    pub fn abstract_property(name: Identifier, ty: ValkyrieType) -> Self {
        Self::new(name, ty).with_abstract(true)
    }

    /// Creates a virtual property.
    pub fn virtual_property(name: Identifier, ty: ValkyrieType) -> Self {
        Self::new(name, ty).with_virtual(true)
    }

    /// Creates a lazy property.
    pub fn lazy_property(name: Identifier, ty: ValkyrieType) -> Self {
        Self::new(name, ty).with_lazy(true)
    }

    /// Sets the getter function.
    pub fn with_getter(mut self, getter: HirFunction) -> Self {
        self.getter = Some(getter);
        self
    }

    /// Sets the setter function.
    pub fn with_setter(mut self, setter: HirFunction) -> Self {
        self.setter = Some(setter);
        self.is_readonly = false;
        self
    }

    /// Sets the static flag.
    pub fn with_static(mut self, is_static: bool) -> Self {
        self.is_static = is_static;
        self
    }

    /// Sets the abstract flag.
    pub fn with_abstract(mut self, is_abstract: bool) -> Self {
        self.is_abstract = is_abstract;
        self
    }

    /// Sets the virtual flag.
    pub fn with_virtual(mut self, is_virtual: bool) -> Self {
        self.is_virtual = is_virtual;
        self
    }

    /// Sets the override flag.
    pub fn with_override(mut self, is_override: bool) -> Self {
        self.is_override = is_override;
        self
    }

    /// Sets the lazy flag.
    pub fn with_lazy(mut self, is_lazy: bool) -> Self {
        self.is_lazy = is_lazy;
        if is_lazy && self.lazy_backing_field.is_none() {
            self.lazy_backing_field = Some(Identifier::new(&format!("_lazy_{}", self.name)));
        }
        self
    }

    /// Sets the lazy backing field name.
    pub fn with_lazy_backing_field(mut self, field_name: Identifier) -> Self {
        self.lazy_backing_field = Some(field_name);
        self
    }

    /// Sets the final flag.
    pub fn with_final(mut self, is_final: bool) -> Self {
        self.is_final = is_final;
        self
    }

    /// Sets the visibility.
    pub fn with_visibility(mut self, visibility: HirVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Returns true if this property has a getter.
    pub fn has_getter(&self) -> bool {
        self.getter.is_some()
    }

    /// Returns true if this property has a setter.
    pub fn has_setter(&self) -> bool {
        self.setter.is_some()
    }

    /// Returns true if this property is an instance property (not static).
    pub fn is_instance(&self) -> bool {
        !self.is_static
    }

    /// Returns true if this property needs dynamic dispatch.
    pub fn needs_dynamic_dispatch(&self) -> bool {
        self.is_virtual && !self.is_static
    }

    /// Returns true if this property can be overridden.
    pub fn can_be_overridden(&self) -> bool {
        !self.is_final && !self.is_static && (self.is_virtual || self.is_abstract)
    }

    /// Returns true if this property has abstract getter.
    pub fn has_abstract_getter(&self) -> bool {
        self.is_abstract && self.getter.is_none()
    }

    /// Returns true if this property has abstract setter.
    pub fn has_abstract_setter(&self) -> bool {
        self.is_abstract && self.setter.is_none() && !self.is_readonly
    }
}
