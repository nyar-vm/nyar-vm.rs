//! Concretize Valkyrie HIR types into platform [`NyarType`].

use std::{collections::BTreeMap, fmt};

use nyar::{NyarFunctionType, NyarType, WitnessObject};

use crate::{MirFunction, MirValueRef, types::hir::ValkyrieType};

/// Failure while turning a frontend type into a platform type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcretizeError {
    /// Human-readable reason.
    pub message: String,
}

impl ConcretizeError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for ConcretizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConcretizeError {}

/// Expand language primitive aliases (`usize`, `i32`, …) and text carriers
/// (`Utf8Text`/`utf8`, `Utf16Text`/`utf16`) to concrete platform shapes.
///
/// Struct field layouts previously kept `Named("usize")` while integer literals lower to
/// `Integer32`, which failed SMIR010 StructNew field-type contracts (TextSpan.start/stop).
/// `type utf16 = Utf16Text` leaves `return self` as `Named(Utf16Text)` while the declared
/// return is `ValkyrieType::Utf16` → SMIR007 unless both concretize to `NyarType::Utf16`.
fn concretize_primitive_named_alias(name: &str) -> Option<NyarType> {
    let simple = name.rsplit([':', '.']).next().unwrap_or(name);
    Some(match simple {
        "bool" => NyarType::Boolean,
        "byte" | "u8" => NyarType::Integer8 { signed: false },
        "sbyte" | "i8" => NyarType::Integer8 { signed: true },
        "u16" => NyarType::Integer16 { signed: false },
        "i16" => NyarType::Integer16 { signed: true },
        // Platform-width and unannotated integer literals share i32 in this seed profile.
        "usize" | "u32" | "i32" => NyarType::Integer32 { signed: true },
        "isize" => NyarType::Integer32 { signed: true },
        "u64" | "i64" => NyarType::Integer64 { signed: true },
        "u128" | "i128" => NyarType::Integer128 { signed: true },
        "f32" => NyarType::Float32,
        "f64" | "f128" => NyarType::Float64,
        "utf8" | "Utf8Text" => NyarType::Utf8,
        "utf16" | "Utf16Text" => NyarType::Utf16,
        _ => return None,
    })
}

/// Strict concretize: rejects frontend-only constructs that must not reach backends.
pub fn concretize_type(ty: &ValkyrieType) -> Result<NyarType, ConcretizeError> {
    match ty {
        ValkyrieType::Void => Ok(NyarType::Bottom),
        ValkyrieType::Unit => Ok(NyarType::Unit),
        ValkyrieType::Boolean => Ok(NyarType::Boolean),
        ValkyrieType::Integer8 { signed } => Ok(NyarType::Integer8 { signed: *signed }),
        ValkyrieType::Integer16 { signed } => Ok(NyarType::Integer16 { signed: *signed }),
        ValkyrieType::Integer32 { signed } => Ok(NyarType::Integer32 { signed: *signed }),
        ValkyrieType::Integer64 { signed } => Ok(NyarType::Integer64 { signed: *signed }),
        ValkyrieType::Integer128 { signed } => Ok(NyarType::Integer128 { signed: *signed }),
        ValkyrieType::Float32 => Ok(NyarType::Float32),
        ValkyrieType::Float64 => Ok(NyarType::Float64),
        ValkyrieType::Character => Ok(NyarType::Character),
        ValkyrieType::Utf8 => Ok(NyarType::Utf8),
        ValkyrieType::Utf16 => Ok(NyarType::Utf16),
        ValkyrieType::Named(name) => Ok(concretize_primitive_named_alias(name.as_str()).unwrap_or_else(|| NyarType::Named(name.clone()))),
        ValkyrieType::Apply(base, args) => {
            Ok(NyarType::Apply(Box::new(concretize_type(base)?), args.iter().map(concretize_type).collect::<Result<Vec<_>, _>>()?))
        }
        ValkyrieType::Function(func) => Ok(NyarType::Function(Box::new(NyarFunctionType {
            params: func.params.iter().map(concretize_type).collect::<Result<Vec<_>, _>>()?,
            return_type: concretize_type(&func.return_type)?,
        }))),
        ValkyrieType::Tuple(elems) => Ok(NyarType::Tuple(elems.iter().map(concretize_type).collect::<Result<Vec<_>, _>>()?)),
        ValkyrieType::Array(elem) => Ok(NyarType::Array(Box::new(concretize_type(elem)?))),
        ValkyrieType::FixedArray { element, length } => {
            Ok(NyarType::FixedArray { element: Box::new(concretize_type(element)?), length: *length })
        }
        ValkyrieType::TraitObject(object) => Ok(NyarType::TraitObject(WitnessObject {
            trait_path: object.trait_path.clone(),
            type_arguments: object.type_arguments.iter().map(concretize_type).collect::<Result<Vec<_>, _>>()?,
        })),
        ValkyrieType::Nullable(payload) => Ok(NyarType::Nullable(Box::new(concretize_type(payload)?))),
        ValkyrieType::Union(arms) => Ok(NyarType::Union(arms.iter().map(concretize_type).collect::<Result<Vec<_>, _>>()?)),
        ValkyrieType::AutoType => Err(ConcretizeError::new("auto type must be resolved before platform lowering")),
        ValkyrieType::SelfType => Err(ConcretizeError::new("Self type must be substituted before platform lowering")),
        ValkyrieType::Generic(generic) => {
            Err(ConcretizeError::new(format!("generic parameter `{}` must be instantiated before platform lowering", generic.name)))
        }
        ValkyrieType::TypeLambda(_) => Err(ConcretizeError::new("type lambda must be erased before platform lowering")),
        ValkyrieType::Associated(_) => Err(ConcretizeError::new("associated type must be resolved before platform lowering")),
        ValkyrieType::Row(_) => Err(ConcretizeError::new("row type must be erased before platform lowering")),
        ValkyrieType::Intersection(_) => Err(ConcretizeError::new("intersection type must be erased before platform lowering")),
    }
}

/// Lossy concretize for migration boundaries: always produces a [`NyarType`].
///
/// Frontend-only constructs are erased to platform-neutral shapes backends already
/// map to object-like ABI (CLR `Object`, JVM `Ljava/lang/Object;`). Uninstantiated
/// generics must **not** become `Named("T")` — that leaks type-parameter names into
/// PE TypeRef owners without an `[assembly]` prefix and fails fail-closed writers.
pub fn concretize_type_lossy(ty: &ValkyrieType) -> NyarType {
    match concretize_type(ty) {
        Ok(nyar) => nyar,
        Err(_) => match ty {
            ValkyrieType::SelfType => NyarType::Named(nyar::Identifier::new("Self")),
            ValkyrieType::AutoType => NyarType::Named(nyar::Identifier::new("__auto")),
            // Type erasure: same CLR/JVM mapping as `NyarType::Apply` / `TraitObject`.
            ValkyrieType::Generic(_) => {
                NyarType::TraitObject(WitnessObject { trait_path: nyar::Identifier::new("__generic"), type_arguments: Vec::new() })
            }
            ValkyrieType::TypeLambda(_) => NyarType::Named(nyar::Identifier::new("__type_lambda")),
            ValkyrieType::Associated(_) => NyarType::Named(nyar::Identifier::new("__associated")),
            ValkyrieType::Row(_) => NyarType::Named(nyar::Identifier::new("__row")),
            ValkyrieType::Intersection(_) => NyarType::Named(nyar::Identifier::new("__intersection")),
            other => {
                // Nested failures (e.g. Apply with Auto arg): recurse lossy.
                match other {
                    ValkyrieType::Apply(base, args) => {
                        NyarType::Apply(Box::new(concretize_type_lossy(base)), args.iter().map(concretize_type_lossy).collect())
                    }
                    ValkyrieType::Function(func) => NyarType::Function(Box::new(NyarFunctionType {
                        params: func.params.iter().map(concretize_type_lossy).collect(),
                        return_type: concretize_type_lossy(&func.return_type),
                    })),
                    ValkyrieType::Tuple(elems) => NyarType::Tuple(elems.iter().map(concretize_type_lossy).collect()),
                    ValkyrieType::Array(elem) => NyarType::Array(Box::new(concretize_type_lossy(elem))),
                    ValkyrieType::FixedArray { element, length } => {
                        NyarType::FixedArray { element: Box::new(concretize_type_lossy(element)), length: *length }
                    }
                    ValkyrieType::TraitObject(object) => NyarType::TraitObject(WitnessObject {
                        trait_path: object.trait_path.clone(),
                        type_arguments: object.type_arguments.iter().map(concretize_type_lossy).collect(),
                    }),
                    ValkyrieType::Nullable(payload) => NyarType::Nullable(Box::new(concretize_type_lossy(payload))),
                    ValkyrieType::Union(arms) => NyarType::Union(arms.iter().map(concretize_type_lossy).collect()),
                    _ => NyarType::Named(nyar::Identifier::new("__opaque")),
                }
            }
        },
    }
}

/// Concretize MIR function type maps into platform types.
pub fn concretize_mir_function_types(
    function: &MirFunction,
) -> Result<(NyarType, Vec<NyarType>, BTreeMap<MirValueRef, NyarType>), ConcretizeError> {
    let return_type = concretize_type(&function.return_type)?;
    let param_types = function.param_types.iter().map(concretize_type).collect::<Result<Vec<_>, _>>()?;
    let mut value_types = BTreeMap::new();
    for (key, ty) in &function.value_types {
        value_types.insert(*key, concretize_type(ty)?);
    }
    Ok((return_type, param_types, value_types))
}

/// Lossy variant of [`concretize_mir_function_types`] for `From` boundaries.
pub fn concretize_mir_function_types_lossy(function: &MirFunction) -> (NyarType, Vec<NyarType>, BTreeMap<MirValueRef, NyarType>) {
    let return_type = concretize_type_lossy(&function.return_type);
    let param_types = function.param_types.iter().map(concretize_type_lossy).collect();
    let value_types = function.value_types.iter().map(|(k, v)| (*k, concretize_type_lossy(v))).collect();
    (return_type, param_types, value_types)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::hir::{FunctionType, GenericType, HirKind};

    #[test]
    fn concretizes_scalars_and_arrays() {
        let ty = ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true }));
        assert_eq!(concretize_type(&ty).unwrap(), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })));
    }

    #[test]
    fn rejects_auto_and_generic() {
        assert!(concretize_type(&ValkyrieType::AutoType).is_err());
        assert!(
            concretize_type(&ValkyrieType::Generic(GenericType { name: nyar::Identifier::new("T"), kind: HirKind::Type, bounds: Vec::new() }))
                .is_err()
        );
    }

    #[test]
    fn lossy_erases_row_and_self() {
        assert_eq!(concretize_type_lossy(&ValkyrieType::SelfType), NyarType::Named(nyar::Identifier::new("Self")));
        assert_eq!(
            concretize_type_lossy(&ValkyrieType::Row(crate::types::hir::RowType { methods: Vec::new() })),
            NyarType::Named(nyar::Identifier::new("__row"))
        );
    }

    #[test]
    fn lossy_erases_generic_param_not_named_t() {
        let erased = concretize_type_lossy(&ValkyrieType::Generic(GenericType {
            name: nyar::Identifier::new("T"),
            kind: HirKind::Type,
            bounds: Vec::new(),
        }));
        assert!(
            matches!(erased, NyarType::TraitObject(ref object) if object.trait_path.as_str() == "__generic"),
            "uninstantiated generics must erase, not become Named(T); got {erased:?}"
        );
    }

    #[test]
    fn concretizes_function_type() {
        let ty = ValkyrieType::Function(Box::new(FunctionType { params: vec![ValkyrieType::Boolean], return_type: ValkyrieType::Unit }));
        assert_eq!(
            concretize_type(&ty).unwrap(),
            NyarType::Function(Box::new(NyarFunctionType { params: vec![NyarType::Boolean], return_type: NyarType::Unit }))
        );
    }
}
