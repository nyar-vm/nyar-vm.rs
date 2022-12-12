use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ScriptType {
    /// Basic types like number, string, boolean, etc.
    Atom(AtomType),
    /// Literal types like "hello", 42, true
    Literal(LiteralType),
    /// Array type: T[]
    Array(Box<ScriptType>),
    /// Tuple type: [T, U]
    Tuple(Vec<ScriptType>),
    /// Object type: { a: T, b?: U }
    Object(ObjectType),
    /// Function type: (a: T) => U
    Function(FunctionType),
    /// Union type: T | U
    Union(Vec<ScriptType>),
    /// Intersection type: T & U
    Intersection(Vec<ScriptType>),
    /// Named type reference: MyInterface<T>
    Reference(TypeReference),
    /// Advanced type operators
    Operator(TypeOperator),
    /// Conditional type: T extends U ? X : Y
    Conditional(ConditionalType),
    /// Mapped type: { [K in keyof T]: U }
    Mapped(MappedType),
    /// Template literal type: `head${T}tail`
    TemplateLiteral(Vec<TemplateElement>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateElement {
    String(String),
    Type(Box<ScriptType>),
}

impl ObjectType {
    pub fn get_property_type(&self, name: &str) -> Option<ScriptType> {
        if let Some(prop) = self.properties.iter().find(|p| p.name == name) {
            return Some(prop.ty.clone());
        }
        for idx in &self.index_signatures {
            if idx.key_type == AtomType::String {
                return Some(idx.value_type.clone());
            }
        }
        None
    }
}

impl ScriptType {
    /// Substitute generic type parameters with concrete types
    pub fn substitute(&self, substitution: &HashMap<String, ScriptType>) -> ScriptType {
        match self {
            ScriptType::Reference(ref_ty) => {
                if let Some(ty) = substitution.get(&ref_ty.name) {
                    ty.clone()
                } else {
                    let mut new_args = Vec::new();
                    for arg in &ref_ty.args {
                        new_args.push(arg.substitute(substitution));
                    }
                    ScriptType::Reference(TypeReference {
                        name: ref_ty.name.clone(),
                        args: new_args,
                    })
                }
            }
            ScriptType::Array(inner) => ScriptType::Array(Box::new(inner.substitute(substitution))),
            ScriptType::Tuple(elems) => {
                let mut new_elems = Vec::new();
                for e in elems {
                    new_elems.push(e.substitute(substitution));
                }
                ScriptType::Tuple(new_elems)
            }
            ScriptType::Object(obj) => {
                let mut new_props = Vec::new();
                for p in &obj.properties {
                    new_props.push(Property {
                        name: p.name.clone(),
                        ty: p.ty.substitute(substitution),
                        optional: p.optional,
                        readonly: p.readonly,
                    });
                }
                let mut new_indices = Vec::new();
                for idx in &obj.index_signatures {
                    new_indices.push(IndexSignature {
                        key_name: idx.key_name.clone(),
                        key_type: idx.key_type,
                        value_type: idx.value_type.substitute(substitution),
                    });
                }
                ScriptType::Object(ObjectType {
                    properties: new_props,
                    index_signatures: new_indices,
                })
            }
            ScriptType::Function(func) => {
                let mut new_params = Vec::new();
                for p in &func.params {
                    new_params.push(Parameter {
                        name: p.name.clone(),
                        ty: p.ty.substitute(substitution),
                        optional: p.optional,
                    });
                }
                ScriptType::Function(FunctionType {
                    params: new_params,
                    return_type: Box::new(func.return_type.substitute(substitution)),
                    type_params: func.type_params.clone(),
                })
            }
            ScriptType::Union(variants) => {
                let mut new_variants = Vec::new();
                for v in variants {
                    new_variants.push(v.substitute(substitution));
                }
                ScriptType::Union(new_variants)
            }
            ScriptType::Intersection(variants) => {
                let mut new_variants = Vec::new();
                for v in variants {
                    new_variants.push(v.substitute(substitution));
                }
                ScriptType::Intersection(new_variants)
            }
            ScriptType::Operator(op) => match op {
                TypeOperator::KeyOf(inner) => {
                    ScriptType::Operator(TypeOperator::KeyOf(Box::new(inner.substitute(substitution))))
                }
                TypeOperator::ReadOnly(inner) => {
                    ScriptType::Operator(TypeOperator::ReadOnly(Box::new(inner.substitute(substitution))))
                }
                TypeOperator::Partial(inner) => {
                    ScriptType::Operator(TypeOperator::Partial(Box::new(inner.substitute(substitution))))
                }
                TypeOperator::Required(inner) => {
                    ScriptType::Operator(TypeOperator::Required(Box::new(inner.substitute(substitution))))
                }
                TypeOperator::Pick(inner, keys) => ScriptType::Operator(TypeOperator::Pick(
                    Box::new(inner.substitute(substitution)),
                    keys.clone(),
                )),
                TypeOperator::Omit(inner, keys) => ScriptType::Operator(TypeOperator::Omit(
                    Box::new(inner.substitute(substitution)),
                    keys.clone(),
                )),
                TypeOperator::Record(k, v) => ScriptType::Operator(TypeOperator::Record(
                    Box::new(k.substitute(substitution)),
                    Box::new(v.substitute(substitution)),
                )),
                _ => self.clone(),
            },
            ScriptType::Mapped(mapped) => ScriptType::Mapped(MappedType {
                key_name: mapped.key_name.clone(),
                key_type: Box::new(mapped.key_type.substitute(substitution)),
                value_type: Box::new(mapped.value_type.substitute(substitution)),
                readonly: mapped.readonly,
                optional: mapped.optional,
            }),
            ScriptType::Conditional(cond) => ScriptType::Conditional(ConditionalType {
                check_type: Box::new(cond.check_type.substitute(substitution)),
                extends_type: Box::new(cond.extends_type.substitute(substitution)),
                true_type: Box::new(cond.true_type.substitute(substitution)),
                false_type: Box::new(cond.false_type.substitute(substitution)),
            }),
            _ => self.clone(),
        }
    }

    /// Resolve the type to its concrete representation, expanding aliases and applying operators
    pub fn resolve(&self, registry: &TypeRegistry) -> ScriptType {
        match self {
            ScriptType::Reference(ref_ty) => {
                if let Some(def) = registry.get_type_definition(&ref_ty.name) {
                    match def {
                        TypeDefinition::Alias(alias) => {
                            let mut resolved = alias.ty.clone();
                            if !alias.type_params.is_empty() {
                                let mut substitution = HashMap::new();
                                for (tp, arg) in alias.type_params.iter().zip(ref_ty.args.iter()) {
                                    substitution.insert(tp.name.clone(), arg.clone());
                                }
                                resolved = resolved.substitute(&substitution);
                            }
                            resolved.resolve(registry)
                        }
                        TypeDefinition::Interface(interface) => {
                            let mut obj = interface.body.clone();
                            // Handle interface inheritance
                            for ext in &interface.extends {
                                let ext_resolved = ScriptType::Reference(ext.clone()).resolve(registry);
                                if let ScriptType::Object(ext_obj) = ext_resolved {
                                    obj.properties.extend(ext_obj.properties);
                                    obj.index_signatures.extend(ext_obj.index_signatures);
                                }
                            }
                            if !interface.type_params.is_empty() {
                                let mut substitution = HashMap::new();
                                for (tp, arg) in interface.type_params.iter().zip(ref_ty.args.iter()) {
                                    substitution.insert(tp.name.clone(), arg.clone());
                                }
                                ScriptType::Object(obj).substitute(&substitution).resolve(registry)
                            } else {
                                ScriptType::Object(obj)
                            }
                        }
                        TypeDefinition::Enum(enum_def) => {
                            let variants = enum_def
                                .members
                                .iter()
                                .filter_map(|m| m.value.as_ref().map(|v| ScriptType::Literal(v.clone())))
                                .collect::<Vec<_>>();
                            if variants.is_empty() {
                                ScriptType::Atom(AtomType::Number)
                            } else {
                                ScriptType::Union(variants)
                            }
                        }
                    }
                } else {
                    self.clone()
                }
            }
            ScriptType::Operator(op) => match op {
                TypeOperator::Partial(inner) => {
                    let resolved = inner.resolve(registry);
                    if let ScriptType::Object(mut obj) = resolved {
                        for prop in &mut obj.properties {
                            prop.optional = true;
                        }
                        ScriptType::Object(obj)
                    } else {
                        resolved
                    }
                }
                TypeOperator::Required(inner) => {
                    let resolved = inner.resolve(registry);
                    if let ScriptType::Object(mut obj) = resolved {
                        for prop in &mut obj.properties {
                            prop.optional = false;
                        }
                        ScriptType::Object(obj)
                    } else {
                        resolved
                    }
                }
                TypeOperator::ReadOnly(inner) => {
                    let resolved = inner.resolve(registry);
                    if let ScriptType::Object(mut obj) = resolved {
                        for prop in &mut obj.properties {
                            prop.readonly = true;
                        }
                        ScriptType::Object(obj)
                    } else {
                        resolved
                    }
                }
                TypeOperator::Pick(inner, keys) => {
                    let resolved = inner.resolve(registry);
                    if let ScriptType::Object(mut obj) = resolved {
                        obj.properties.retain(|p| keys.contains(&p.name));
                        ScriptType::Object(obj)
                    } else {
                        resolved
                    }
                }
                TypeOperator::Omit(inner, keys) => {
                    let resolved = inner.resolve(registry);
                    if let ScriptType::Object(mut obj) = resolved {
                        obj.properties.retain(|p| !keys.contains(&p.name));
                        ScriptType::Object(obj)
                    } else {
                        resolved
                    }
                }
                TypeOperator::Record(key_type, value_type) => {
                    let key_res = key_type.resolve(registry);
                    let mut props = Vec::new();
                    if let ScriptType::Union(variants) = key_res {
                        for v in variants {
                            if let ScriptType::Literal(LiteralType::String(name)) = v {
                                props.push(Property {
                                    name,
                                    ty: *value_type.clone(),
                                    optional: false,
                                    readonly: false,
                                });
                            }
                        }
                        ScriptType::Object(ObjectType {
                            properties: props,
                            index_signatures: Vec::new(),
                        })
                    } else {
                        let mut obj = ObjectType {
                            properties: Vec::new(),
                            index_signatures: Vec::new(),
                        };
                        let key_atom = match key_type.resolve(registry) {
                            ScriptType::Atom(a) => a,
                            _ => AtomType::String,
                        };
                        obj.index_signatures.push(IndexSignature {
                            key_name: "key".to_string(),
                            key_type: key_atom,
                            value_type: *value_type.clone(),
                        });
                        ScriptType::Object(obj)
                    }
                }
                TypeOperator::KeyOf(inner) => {
                    let resolved = inner.resolve(registry);
                    match resolved {
                        ScriptType::Object(obj) => {
                            let variants = obj
                                .properties
                                .iter()
                                .map(|p| ScriptType::Literal(LiteralType::String(p.name.clone())))
                                .collect();
                            ScriptType::Union(variants)
                        }
                        ScriptType::Array(_) | ScriptType::Tuple(_) => {
                            ScriptType::Union(vec![
                                ScriptType::Atom(AtomType::Number),
                                ScriptType::Literal(LiteralType::String("length".to_string())),
                                // Add other array methods if needed
                            ])
                        }
                        _ => ScriptType::Union(vec![
                            ScriptType::Atom(AtomType::String),
                            ScriptType::Atom(AtomType::Number),
                            ScriptType::Atom(AtomType::Symbol),
                        ]),
                    }
                }
                _ => self.clone(),
            },
            ScriptType::Mapped(mapped) => {
                let key_type = mapped.key_type.resolve(registry);
                if let ScriptType::Union(variants) = key_type {
                    let mut props = Vec::new();
                    for v in variants {
                        if let ScriptType::Literal(LiteralType::String(name)) = v.clone() {
                            // Substitute the key name into the value type if it's a generic reference
                            // This is a simplified version of T[K]
                            let value_substitution = {
                                let mut sub = HashMap::new();
                                sub.insert(mapped.key_name.clone(), v);
                                mapped.value_type.substitute(&sub)
                            };
                            props.push(Property {
                                name,
                                ty: value_substitution.resolve(registry),
                                optional: mapped.optional.unwrap_or(false),
                                readonly: mapped.readonly.unwrap_or(false),
                            });
                        }
                    }
                    ScriptType::Object(ObjectType {
                        properties: props,
                        index_signatures: Vec::new(),
                    })
                } else {
                    self.clone()
                }
            }
            ScriptType::Conditional(cond) => {
                if cond.check_type.is_assignable_to(&cond.extends_type, registry) {
                    cond.true_type.resolve(registry)
                } else {
                    cond.false_type.resolve(registry)
                }
            }
            _ => self.clone(),
        }
    }

    /// Check if a type is assignable to another
    pub fn is_assignable_to(&self, other: &ScriptType, registry: &TypeRegistry) -> bool {
        if self == other {
            return true;
        }

        let s = self.resolve(registry);
        let o = other.resolve(registry);

        match (&s, &o) {
            // Any can be assigned to anything, and anything can be assigned to Any
            (ScriptType::Atom(AtomType::Any), _) => true,
            (_, ScriptType::Atom(AtomType::Any)) => true,

            // Unknown can only be assigned to Any or Unknown
            (ScriptType::Atom(AtomType::Unknown), ScriptType::Atom(AtomType::Unknown)) => true,
            (ScriptType::Atom(AtomType::Unknown), _) => false,

            // Never can be assigned to anything
            (ScriptType::Atom(AtomType::Never), _) => true,

            // Basic atom types
            (ScriptType::Atom(a), ScriptType::Atom(b)) => a == b,

            // Null safety (strict mode simulation)
            (ScriptType::Atom(AtomType::Null), _) => false,
            (ScriptType::Atom(AtomType::Undefined), _) => false,

            // Literal types can be assigned to their base types
            (ScriptType::Literal(LiteralType::String(_)), ScriptType::Atom(AtomType::String)) => true,
            (ScriptType::Literal(LiteralType::Number(_)), ScriptType::Atom(AtomType::Number)) => true,
            (ScriptType::Literal(LiteralType::Boolean(_)), ScriptType::Atom(AtomType::Boolean)) => true,
            (ScriptType::Literal(LiteralType::BigInt(_)), ScriptType::Atom(AtomType::BigInt)) => true,
            (ScriptType::Literal(a), ScriptType::Literal(b)) => a == b,

            // Array covariance (simplified)
            (ScriptType::Array(inner_a), ScriptType::Array(inner_b)) => {
                inner_a.is_assignable_to(inner_b, registry)
            }

            // Tuple types
            (ScriptType::Tuple(elems_a), ScriptType::Tuple(elems_b)) => {
                if elems_a.len() != elems_b.len() {
                    return false;
                }
                for (a, b) in elems_a.iter().zip(elems_b.iter()) {
                    if !a.is_assignable_to(b, registry) {
                        return false;
                    }
                }
                true
            }

            // Function types (contravariant parameters, covariant return type)
            (ScriptType::Function(func_a), ScriptType::Function(func_b)) => {
                // Return type covariance
                if !func_a.return_type.is_assignable_to(&func_b.return_type, registry) {
                    return false;
                }
                // Parameters contravariance
                // TypeScript allows assigning a function with fewer parameters to one with more
                if func_a.params.len() > func_b.params.len() {
                    return false;
                }
                for (p_a, p_b) in func_a.params.iter().zip(func_b.params.iter()) {
                    // Contravariance: p_b.ty must be assignable to p_a.ty
                    if !p_b.ty.is_assignable_to(&p_a.ty, registry) {
                        return false;
                    }
                }
                true
            }

            // Union types: T can be assigned to A | B if T is assignable to A or T is assignable to B
            (t, ScriptType::Union(variants)) => {
                variants.iter().any(|v| t.is_assignable_to(v, registry))
            }
            // A | B can be assigned to T if both A and B are assignable to T
            (ScriptType::Union(variants), t) => {
                variants.iter().all(|v| v.is_assignable_to(t, registry))
            }

            // Intersection types: T can be assigned to A & B if T is assignable to A and T is assignable to B
            (t, ScriptType::Intersection(variants)) => {
                variants.iter().all(|v| t.is_assignable_to(v, registry))
            }
            // A & B can be assigned to T if A is assignable to T or B is assignable to T
            (ScriptType::Intersection(variants), t) => {
                variants.iter().any(|v| v.is_assignable_to(t, registry))
            }

            // Object types (structural typing)
            (ScriptType::Object(obj_a), ScriptType::Object(obj_b)) => {
                // Check all properties in B are present and compatible in A
                for prop_b in &obj_b.properties {
                    let found = obj_a.properties.iter().find(|p| p.name == prop_b.name);
                    match found {
                        Some(prop_a) => {
                            if !prop_a.ty.is_assignable_to(&prop_b.ty, registry) {
                                return false;
                            }
                        }
                        None => {
                            // If not found in properties, check index signatures
                            let mut compatible_index = false;
                            for idx in &obj_a.index_signatures {
                                 // For now assume key name matches if it's a string index
                                 if idx.key_type == AtomType::String {
                                     if obj_a.get_property_type(&prop_b.name).map_or(false, |ty| ty.is_assignable_to(&prop_b.ty, registry)) {
                                         compatible_index = true;
                                         break;
                                     }
                                 }
                             }
                            if !compatible_index && !prop_b.optional {
                                return false;
                            }
                        }
                    }
                }
                
                // Check index signatures in B are compatible with index signatures in A
                for idx_b in &obj_b.index_signatures {
                    let mut compatible = false;
                    for idx_a in &obj_a.index_signatures {
                        if idx_a.key_type == idx_b.key_type && idx_a.value_type.is_assignable_to(&idx_b.value_type, registry) {
                            compatible = true;
                            break;
                        }
                    }
                    if !compatible {
                        return false;
                    }
                }
                
                true
            }

            // Template literal types (simplified)
            (ScriptType::Literal(LiteralType::String(_s)), ScriptType::TemplateLiteral(_)) => {
                // In a real implementation, we would check if the string matches the template
                // For now, assume string literals are assignable to template literals if they might match
                true
            }
            (ScriptType::TemplateLiteral(_), ScriptType::Atom(AtomType::String)) => true,

            _ => false,
        }
    }

    /// Helper to get the type of a property, including from index signatures
    pub fn get_property_type(&self, name: &str, registry: &TypeRegistry) -> Option<ScriptType> {
        let resolved = self.resolve(registry);
        if let ScriptType::Object(obj) = resolved {
            obj.get_property_type(name)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomType {
    Any,
    Unknown,
    Never,
    Void,
    Null,
    Undefined,
    Number,
    BigInt,
    String,
    Boolean,
    Symbol,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralType {
    String(String),
    Number(f64),
    Boolean(bool),
    BigInt(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectType {
    pub properties: Vec<Property>,
    pub index_signatures: Vec<IndexSignature>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub ty: ScriptType,
    pub optional: bool,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexSignature {
    pub key_name: String,
    pub key_type: AtomType, // usually String or Number
    pub value_type: ScriptType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionType {
    pub params: Vec<Parameter>,
    pub return_type: Box<ScriptType>,
    pub type_params: Vec<TypeParameter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: ScriptType,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParameter {
    pub name: String,
    pub constraint: Option<Box<ScriptType>>,
    pub default: Option<Box<ScriptType>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeReference {
    pub name: String,
    pub args: Vec<ScriptType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeOperator {
    KeyOf(Box<ScriptType>),
    TypeOf(String),
    ReadOnly(Box<ScriptType>),
    Partial(Box<ScriptType>),
    Required(Box<ScriptType>),
    Pick(Box<ScriptType>, Vec<String>),
    Omit(Box<ScriptType>, Vec<String>),
    Record(Box<ScriptType>, Box<ScriptType>),
    Infer(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalType {
    pub check_type: Box<ScriptType>,
    pub extends_type: Box<ScriptType>,
    pub true_type: Box<ScriptType>,
    pub false_type: Box<ScriptType>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MappedType {
    pub key_name: String,
    pub key_type: Box<ScriptType>,
    pub value_type: Box<ScriptType>,
    pub readonly: Option<bool>,
    pub optional: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum TypeDefinition {
    Interface(InterfaceDefinition),
    Alias(AliasDefinition),
    Enum(EnumDefinition),
}

#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    pub interfaces: HashMap<String, InterfaceDefinition>,
    pub aliases: HashMap<String, AliasDefinition>,
    pub enums: HashMap<String, EnumDefinition>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register_builtins();
        registry
    }

    fn register_builtins(&mut self) {
        // Register built-in utility types as aliases or special markers
        // In a real implementation, these would be handled by the type checker
        // as they are often generic and mapped types.
        
        // Example: type Partial<T> = { [P in keyof T]?: T[P] };
        // Here we just ensure the registry is aware of them if needed.
    }

    pub fn register_interface(&mut self, def: InterfaceDefinition) {
        self.interfaces.insert(def.name.clone(), def);
    }

    pub fn register_alias(&mut self, def: AliasDefinition) {
        self.aliases.insert(def.name.clone(), def);
    }

    pub fn register_enum(&mut self, def: EnumDefinition) {
        self.enums.insert(def.name.clone(), def);
    }

    pub fn get_type_definition(&self, name: &str) -> Option<TypeDefinition> {
        if let Some(alias) = self.aliases.get(name) {
            return Some(TypeDefinition::Alias(alias.clone()));
        }
        if let Some(interface) = self.interfaces.get(name) {
            return Some(TypeDefinition::Interface(interface.clone()));
        }
        if let Some(enum_def) = self.enums.get(name) {
            return Some(TypeDefinition::Enum(enum_def.clone()));
        }
        None
    }

    pub fn get_type(&self, name: &str) -> Option<ScriptType> {
        match self.get_type_definition(name) {
            Some(TypeDefinition::Alias(alias)) => Some(alias.ty.clone()),
            Some(TypeDefinition::Interface(interface)) => Some(ScriptType::Object(interface.body.clone())),
            Some(TypeDefinition::Enum(enum_def)) => {
                let variants = enum_def
                    .members
                    .iter()
                    .filter_map(|m| m.value.as_ref().map(|v| ScriptType::Literal(v.clone())))
                    .collect::<Vec<_>>();
                if variants.is_empty() {
                    Some(ScriptType::Atom(AtomType::Number))
                } else {
                    Some(ScriptType::Union(variants))
                }
            }
            None => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterfaceDefinition {
    pub name: String,
    pub type_params: Vec<TypeParameter>,
    pub extends: Vec<TypeReference>,
    pub body: ObjectType,
}

#[derive(Debug, Clone)]
pub struct AliasDefinition {
    pub name: String,
    pub type_params: Vec<TypeParameter>,
    pub ty: ScriptType,
}

#[derive(Debug, Clone)]
pub struct EnumDefinition {
    pub name: String,
    pub members: Vec<EnumMember>,
}

#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: String,
    pub value: Option<LiteralType>,
}
