use nyar_language::types::{Identifier, witness::*};

fn create_test_identifier(s: &str) -> Identifier {
    Identifier::new(s)
}

fn create_test_method_path() -> MethodPath {
    MethodPath::new(
        vec![create_test_identifier("std"), create_test_identifier("collections")],
        create_test_identifier("HashMap"),
        create_test_identifier("insert"),
    )
}

fn create_test_type_metadata() -> TypeMetadata {
    TypeMetadata::new(TypeId::new(1), create_test_identifier("MyType"), TypeKind::Struct).with_layout(64, 8).as_value_type()
}

fn create_test_method_entry() -> MethodEntry {
    MethodEntry::new(MethodId::new(1), create_test_identifier("test_method"), TypeId::new(1), create_test_method_path(), false)
}

fn create_test_witness_table() -> WitnessTable {
    let mut table = WitnessTable::new(TraitId::new(1), TypeId::new(1), create_test_type_metadata());
    table.add_method(create_test_method_entry());
    table.add_associated_type(create_test_identifier("Item"), TypeId::new(2));
    table
}

#[test]
fn test_method_path_serialization() {
    let path = create_test_method_path();
    let bytes = path.to_bytes();
    let decoded = MethodPath::from_bytes(&bytes).unwrap();
    assert_eq!(path, decoded);
}

#[test]
fn test_method_entry_serialization() {
    let entry = create_test_method_entry();
    let bytes = entry.to_bytes();
    let decoded = MethodEntry::from_bytes(&bytes).unwrap();
    assert_eq!(entry, decoded);
}

#[test]
fn test_type_metadata_serialization() {
    let metadata = create_test_type_metadata();
    let bytes = metadata.to_bytes();
    let decoded = TypeMetadata::from_bytes(&bytes).unwrap();
    assert_eq!(metadata, decoded);
}

#[test]
fn test_type_kind_roundtrip() {
    let kinds = [
        TypeKind::Primitive,
        TypeKind::Struct,
        TypeKind::Enum,
        TypeKind::Trait,
        TypeKind::Function,
        TypeKind::Tuple,
        TypeKind::Array,
        TypeKind::Generic,
        TypeKind::Alias,
        TypeKind::Foreign,
    ];
    for kind in kinds {
        let byte = kind.to_byte();
        let decoded = TypeKind::from_byte(byte).unwrap();
        assert_eq!(kind, decoded);
    }
}

#[test]
fn test_witness_table_serialization() {
    let table = create_test_witness_table();
    let bytes = table.to_bytes();
    let decoded = WitnessTable::from_bytes(&bytes).unwrap();
    assert_eq!(table, decoded);
}

#[test]
fn test_witness_registry_serialization() {
    let mut registry = WitnessRegistry::new();
    registry.register(create_test_witness_table());

    let table2 = WitnessTable::new(TraitId::new(2), TypeId::new(1), create_test_type_metadata());
    registry.register(table2);

    let bytes = registry.serialize_all();
    let mut decoded_registry = WitnessRegistry::new();
    decoded_registry.deserialize_into(&bytes).unwrap();

    assert_eq!(registry.table_count(), decoded_registry.table_count());
}

#[test]
fn test_invalid_magic() {
    let data = b"XXXX\x01\x00\x01\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    let result = WitnessTable::from_bytes(data);
    assert!(matches!(result, Err(WitnessDecodeError::InvalidMagic { .. })));
}

#[test]
fn test_unsupported_version() {
    let mut data = Vec::new();
    data.extend_from_slice(b"VWIT");
    data.extend_from_slice(&0x0001u16.to_le_bytes());
    let result = WitnessTable::from_bytes(&data);
    assert!(matches!(result, Err(WitnessDecodeError::UnsupportedVersion { .. })));
}

#[test]
fn test_empty_witness_table() {
    let table = WitnessTable::new(
        TraitId::new(1),
        TypeId::new(1),
        TypeMetadata::new(TypeId::new(1), create_test_identifier("EmptyType"), TypeKind::Struct),
    );
    let bytes = table.to_bytes();
    let decoded = WitnessTable::from_bytes(&bytes).unwrap();
    assert_eq!(table, decoded);
    assert_eq!(decoded.method_count(), 0);
    assert_eq!(decoded.associated_type_count(), 0);
}

#[test]
fn test_witness_decode_error_display() {
    let err = WitnessDecodeError::InvalidMagic { expected: *b"VWIT", found: *b"XXXX" };
    assert!(err.to_string().contains("Invalid magic number"));

    let err = WitnessDecodeError::UnsupportedVersion { supported: 1, found: 2 };
    assert!(err.to_string().contains("Unsupported version"));

    let err = WitnessDecodeError::UnexpectedEndOfData { context: "test".to_string() };
    assert!(err.to_string().contains("Unexpected end of data"));

    let err = WitnessDecodeError::InvalidUtf8 { message: "bad utf8".to_string() };
    assert!(err.to_string().contains("Invalid UTF-8"));

    let err = WitnessDecodeError::InvalidLength { field: "test".to_string(), expected: "10".to_string(), found: 5 };
    assert!(err.to_string().contains("Invalid length"));
}

#[test]
fn test_cross_module_witness_table() {
    let foreign_module = ModuleId::new(42);
    let mut table = WitnessTable::new_in_module(foreign_module, TraitId::new(1), TypeId::new(1), create_test_type_metadata());
    table.add_method(create_test_method_entry());

    let bytes = table.to_bytes();
    let decoded = WitnessTable::from_bytes(&bytes).unwrap();
    assert_eq!(table, decoded);
    assert_eq!(decoded.module_id, foreign_module);
    assert!(decoded.module_id != ModuleId::LOCAL);
}

#[test]
fn test_cross_module_registry() {
    let mut registry = WitnessRegistry::new();

    let local_table = create_test_witness_table();
    registry.register(local_table);

    let foreign_module = ModuleId::new(100);
    let foreign_table = WitnessTable::new_in_module(foreign_module, TraitId::new(2), TypeId::new(2), create_test_type_metadata());
    registry.register_foreign(foreign_table).unwrap();

    assert!(registry.get(TypeId::new(1), TraitId::new(1)).is_some());
    assert!(registry.get_in_module(foreign_module, TypeId::new(2), TraitId::new(2)).is_some());

    let implementors = registry.get_implementors(foreign_module, TraitId::new(2)).unwrap();
    assert_eq!(implementors.len(), 1);
    assert_eq!(implementors[0], TypeId::new(2));
}

#[test]
fn test_module_unregister() {
    let mut registry = WitnessRegistry::new();

    let module_a = ModuleId::new(10);
    let module_b = ModuleId::new(20);

    let table_a = WitnessTable::new_in_module(module_a, TraitId::new(1), TypeId::new(1), create_test_type_metadata());
    registry.register(table_a);

    let table_b = WitnessTable::new_in_module(module_b, TraitId::new(1), TypeId::new(1), create_test_type_metadata());
    registry.register(table_b);

    assert_eq!(registry.table_count(), 2);

    registry.unregister_module(module_a);
    assert_eq!(registry.table_count(), 1);
    assert!(registry.get_in_module(module_a, TypeId::new(1), TraitId::new(1)).is_none());
    assert!(registry.get_in_module(module_b, TypeId::new(1), TraitId::new(1)).is_some());
}

#[test]
fn test_cross_module_error_display() {
    let err = CrossModuleError::ModuleNotFound { module_id: ModuleId::new(42) };
    assert!(err.to_string().contains("Module 42 not found"));

    let err = CrossModuleError::TraitNotFound { module_id: ModuleId::new(1), trait_id: TraitId::new(2) };
    assert!(err.to_string().contains("Trait 2 not found in module 1"));

    let err = CrossModuleError::ImplementationNotFound { module_id: ModuleId::new(1), type_id: TypeId::new(2), trait_id: TraitId::new(3) };
    assert!(err.to_string().contains("Type 2 does not implement trait 3"));

    let err = CrossModuleError::CircularDependency { chain: vec![ModuleId::new(1), ModuleId::new(2), ModuleId::new(1)] };
    assert!(err.to_string().contains("Circular dependency"));

    let err = CrossModuleError::VersionMismatch { expected: 1, found: 2 };
    assert!(err.to_string().contains("Version mismatch"));
}

#[test]
fn test_trait_object_cross_module() {
    let foreign_module = ModuleId::new(50);
    let trait_obj = TraitObject::new_in_module(foreign_module, TypeId::new(1), TraitId::new(2), 0);
    assert!(trait_obj.is_cross_module());

    let local_obj = TraitObject::new(TypeId::new(1), TraitId::new(2), 0);
    assert!(!local_obj.is_cross_module());
}

#[test]
fn test_method_path_cross_module() {
    let foreign_module = ModuleId::new(100);
    let path = MethodPath::new_with_module(
        foreign_module,
        vec![create_test_identifier("std")],
        create_test_identifier("Container"),
        create_test_identifier("method"),
    );
    assert!(path.is_cross_module());

    let local_path =
        MethodPath::new(vec![create_test_identifier("local")], create_test_identifier("Container"), create_test_identifier("method"));
    assert!(!local_path.is_cross_module());

    let bytes = path.to_bytes();
    let decoded = MethodPath::from_bytes(&bytes).unwrap();
    assert_eq!(path, decoded);
    assert_eq!(decoded.module_id, foreign_module);
}

#[test]
fn test_register_foreign_rejects_local() {
    let mut registry = WitnessRegistry::new();
    let local_table = create_test_witness_table();
    let result = registry.register_foreign(local_table);
    assert!(matches!(result, Err(CrossModuleError::InvalidModuleId)));
}
