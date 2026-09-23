//! Platform type model for the Nyar cross-language optimizer.
//!
//! `NyarType` is language-neutral executable/layout type information consumed by
//! planners and backends. Frontend-specific constructs (e.g. `auto`, uninstantiated
//! generics, associated types) must be erased or rejected before values reach this
//! layer — they do not belong in this enum.

use crate::Identifier;
use std::fmt;

/// Function type `fn(params...) -> ret` at the platform layer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NyarFunctionType {
    /// Parameter types in declaration order.
    pub params: Vec<NyarType>,
    /// Return type.
    pub return_type: NyarType,
}

/// Dynamic trait-object / witness-table type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WitnessObject {
    /// Trait identity (simple name aligned with current MIR witness lowering).
    pub trait_path: Identifier,
    /// Instantiated type arguments for the trait, if any.
    pub type_arguments: Vec<NyarType>,
}

/// Platform type carried by executable IR and backend lowering.
///
/// This is intentionally a *concrete* subset: values that survive frontend
/// lowering and can be mapped to WASM / CLR / JVM / native representations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NyarType {
    /// Bottom type in Nyar type System, never-returning.
    Bottom,
    /// Unit / void-as-value.
    Unit,
    /// Boolean.
    Boolean,
    /// 8-bit integer.
    Integer8 {
        /// Signedness.
        signed: bool,
    },
    /// 16-bit integer.
    Integer16 {
        /// Signedness.
        signed: bool,
    },
    /// 32-bit integer.
    Integer32 {
        /// Signedness.
        signed: bool,
    },
    /// 64-bit integer.
    Integer64 {
        /// Signedness.
        signed: bool,
    },
    /// 128-bit integer (backends may narrow).
    Integer128 {
        /// Signedness.
        signed: bool,
    },
    /// 32-bit float.
    Float32,
    /// 64-bit float.
    Float64,
    /// Unicode scalar / character.
    Character,
    /// UTF-8 string.
    Utf8,
    /// UTF-16 string.
    Utf16,
    /// Nominal type; layout resolved via aggregate / class side tables.
    Named(Identifier),
    /// Instantiated application `Base<Args...>`.
    Apply(Box<NyarType>, Vec<NyarType>),
    /// Function type.
    Function(Box<NyarFunctionType>),
    /// Product / tuple.
    Tuple(Vec<NyarType>),
    /// Heap array `[T]`.
    Array(Box<NyarType>),
    /// Fixed-length array `[T; N]` on stack.
    FixedArray {
        /// Element type.
        element: Box<NyarType>,
        /// Element count.
        length: usize,
    },
    /// Dynamic dispatch via witness tables.
    TraitObject(WitnessObject),
    /// Explicit nullable value `T?`. This is distinct from an anonymous union
    /// and cannot be recovered from a nominal `null` spelling.
    Nullable(Box<NyarType>),
    /// Anonymous runtime-distinguishable union.
    Union(Vec<NyarType>),
}

impl Default for NyarType {
    fn default() -> Self {
        NyarType::Unit
    }
}

impl NyarType {
    /// `Void` or `Unit`.
    pub fn is_unitish(&self) -> bool {
        matches!(self, NyarType::Bottom | NyarType::Unit)
    }

    /// `Float32` or `Float64`.
    pub fn is_float(&self) -> bool {
        matches!(self, NyarType::Float32 | NyarType::Float64)
    }

    /// Integer types that backends typically widen to 64-bit slots.
    pub fn is_i64_width(&self) -> bool {
        matches!(self, NyarType::Integer64 { .. } | NyarType::Integer128 { .. })
    }
}

impl fmt::Display for NyarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NyarType::Bottom => f.write_str("void"),
            NyarType::Unit => f.write_str("unit"),
            NyarType::Boolean => f.write_str("bool"),
            NyarType::Integer8 { signed: true } => f.write_str("i8"),
            NyarType::Integer8 { signed: false } => f.write_str("u8"),
            NyarType::Integer16 { signed: true } => f.write_str("i16"),
            NyarType::Integer16 { signed: false } => f.write_str("u16"),
            NyarType::Integer32 { signed: true } => f.write_str("i32"),
            NyarType::Integer32 { signed: false } => f.write_str("u32"),
            NyarType::Integer64 { signed: true } => f.write_str("i64"),
            NyarType::Integer64 { signed: false } => f.write_str("u64"),
            NyarType::Integer128 { signed: true } => f.write_str("i128"),
            NyarType::Integer128 { signed: false } => f.write_str("u128"),
            NyarType::Float32 => f.write_str("f32"),
            NyarType::Float64 => f.write_str("f64"),
            NyarType::Character => f.write_str("char"),
            NyarType::Utf8 => f.write_str("utf8"),
            NyarType::Utf16 => f.write_str("utf16"),
            NyarType::Named(name) => write!(f, "{name}"),
            NyarType::Apply(base, args) => {
                write!(f, "{base}<")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(">")
            }
            NyarType::Function(func) => {
                f.write_str("fn(")?;
                for (i, param) in func.params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ") -> {}", func.return_type)
            }
            NyarType::Tuple(elems) => {
                f.write_str("(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                f.write_str(")")
            }
            NyarType::Array(elem) => write!(f, "[{elem}]"),
            NyarType::FixedArray { element, length } => write!(f, "[{element}; {length}]"),
            NyarType::TraitObject(object) => {
                write!(f, "dyn {}", object.trait_path)?;
                if object.type_arguments.is_empty() {
                    return Ok(());
                }
                f.write_str("<")?;
                for (i, arg) in object.type_arguments.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(">")
            }
            NyarType::Nullable(payload) => write!(f, "{payload}?"),
            NyarType::Union(arms) => {
                for (i, arm) in arms.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{arm}")?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_scalars_and_helpers() {
        assert_eq!(NyarType::Unit.to_string(), "unit");
        assert!(NyarType::Bottom.is_unitish());
        assert!(NyarType::Float64.is_float());
        assert!(NyarType::Integer64 { signed: true }.is_i64_width());
        assert_eq!(NyarType::Array(Box::new(NyarType::Boolean)).to_string(), "[bool]");
    }

    #[test]
    fn named_and_apply_display() {
        let named = NyarType::Named(Identifier::new("Point"));
        assert_eq!(named.to_string(), "Point");
        let applied = NyarType::Apply(Box::new(named), vec![NyarType::Integer32 { signed: true }]);
        assert_eq!(applied.to_string(), "Point<i32>");
    }
}
