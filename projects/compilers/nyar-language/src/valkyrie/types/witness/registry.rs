use std::{
    collections::HashMap,
    io::{Read, Write},
};

use crate::Identifier;

use super::{
    CrossModuleError, MethodEntry, ModuleId, TraitId, TypeId, TypeMetadata, WITNESS_MAGIC, WITNESS_VERSION, WitnessDecodeError, WitnessTable,
    ids::{ByteReader, ByteWriter},
};
/// Registry for managing witness tables across all type-trait implementations.
///
/// The witness registry provides efficient lookup of witness tables by
/// type and trait identifiers, enabling fast dynamic dispatch.
///
/// # Cross-Module Support
///
/// The registry supports witness tables from multiple modules:
/// - Local module tables are registered directly
/// - Foreign module tables are registered with their module ID
/// - Lookup can query across all modules or filter by module
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WitnessRegistry {
    /// Maps (module_id, type_id, trait_id) triples to their witness tables.
    tables: HashMap<(ModuleId, TypeId, TraitId), WitnessTable>,
    /// Maps type_id to all implemented trait ids (per module).
    type_traits: HashMap<(ModuleId, TypeId), Vec<TraitId>>,
    /// Maps module_id to all witness tables in that module.
    module_tables: HashMap<ModuleId, Vec<(TypeId, TraitId)>>,
    /// Maps trait_id to all types implementing that trait (per module).
    trait_implementors: HashMap<(ModuleId, TraitId), Vec<TypeId>>,
}

impl WitnessRegistry {
    /// Creates a new empty witness registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a witness table in the registry.
    pub fn register(&mut self, table: WitnessTable) {
        let module_id = table.module_id;
        let type_id = table.type_id;
        let trait_id = table.trait_id;
        let key = (module_id, type_id, trait_id);

        self.tables.insert(key, table);

        self.type_traits.entry((module_id, type_id)).or_default().push(trait_id);

        self.module_tables.entry(module_id).or_default().push((type_id, trait_id));

        self.trait_implementors.entry((module_id, trait_id)).or_default().push(type_id);
    }

    /// Registers a witness table from a foreign module.
    pub fn register_foreign(&mut self, table: WitnessTable) -> Result<(), CrossModuleError> {
        if table.module_id == ModuleId::LOCAL {
            return Err(CrossModuleError::InvalidModuleId);
        }
        self.register(table);
        Ok(())
    }

    /// Looks up a witness table by type and trait identifiers (local module).
    pub fn get(&self, type_id: TypeId, trait_id: TraitId) -> Option<&WitnessTable> {
        self.tables.get(&(ModuleId::LOCAL, type_id, trait_id))
    }

    /// Looks up a witness table by module, type and trait identifiers.
    pub fn get_in_module(&self, module_id: ModuleId, type_id: TypeId, trait_id: TraitId) -> Option<&WitnessTable> {
        self.tables.get(&(module_id, type_id, trait_id))
    }

    /// Returns all traits implemented by a type in a specific module.
    pub fn get_traits_for_type_in_module(&self, module_id: ModuleId, type_id: TypeId) -> Option<&[TraitId]> {
        self.type_traits.get(&(module_id, type_id)).map(|v| v.as_slice())
    }

    /// Returns all traits implemented by a type (local module).
    pub fn get_traits_for_type(&self, type_id: TypeId) -> Option<&[TraitId]> {
        self.get_traits_for_type_in_module(ModuleId::LOCAL, type_id)
    }

    /// Checks if a type implements a specific trait (local module).
    pub fn implements_trait(&self, type_id: TypeId, trait_id: TraitId) -> bool {
        self.tables.contains_key(&(ModuleId::LOCAL, type_id, trait_id))
    }

    /// Checks if a type implements a specific trait in a specific module.
    pub fn implements_trait_in_module(&self, module_id: ModuleId, type_id: TypeId, trait_id: TraitId) -> bool {
        self.tables.contains_key(&(module_id, type_id, trait_id))
    }

    /// Returns all types implementing a trait in a specific module.
    pub fn get_implementors(&self, module_id: ModuleId, trait_id: TraitId) -> Option<&[TypeId]> {
        self.trait_implementors.get(&(module_id, trait_id)).map(|v| v.as_slice())
    }

    /// Returns all witness tables in a specific module.
    pub fn get_module_tables(&self, module_id: ModuleId) -> Option<Vec<&WitnessTable>> {
        self.module_tables
            .get(&module_id)
            .map(|keys| keys.iter().filter_map(|(type_id, trait_id)| self.tables.get(&(module_id, *type_id, *trait_id))).collect())
    }

    /// Removes all witness tables from a specific module.
    pub fn unregister_module(&mut self, module_id: ModuleId) {
        if let Some(keys) = self.module_tables.remove(&module_id) {
            for (type_id, trait_id) in keys {
                self.tables.remove(&(module_id, type_id, trait_id));
                self.type_traits.remove(&(module_id, type_id));
                self.trait_implementors.remove(&(module_id, trait_id));
            }
        }
    }

    /// Returns the total number of witness tables.
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// Serializes all witness tables in the registry to bytes.
    ///
    /// The binary format is:
    /// - Magic number (4 bytes): "VWIT"
    /// - Version (2 bytes): 0x0002
    /// - Registry marker (4 bytes): "REGI"
    /// - Table count (8 bytes)
    /// - Witness tables (variable)
    ///
    /// # Returns
    /// A vector of bytes representing all serialized witness tables.
    pub fn serialize_all(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.write_all(WITNESS_MAGIC).expect("Vec write should never fail");
        buf.write_u16(WITNESS_VERSION).expect("Vec write should never fail");
        buf.write_all(b"REGI").expect("Vec write should never fail");
        buf.write_usize(self.tables.len()).expect("Vec write should never fail");
        for table in self.tables.values() {
            table.write_to(&mut buf).expect("Vec write should never fail");
        }
        buf
    }

    /// Deserializes witness tables from bytes and registers them.
    ///
    /// # Arguments
    /// * `data` - The byte slice to deserialize from.
    ///
    /// # Returns
    /// Ok(()) if deserialization and registration succeeds, or an error.
    pub fn deserialize_into(&mut self, data: &[u8]) -> Result<(), WitnessDecodeError> {
        let mut cursor = std::io::Cursor::new(data);

        let mut magic = [0u8; 4];
        cursor.read_exact(&mut magic).map_err(|_| WitnessDecodeError::UnexpectedEndOfData { context: "magic number".to_string() })?;
        if &magic != WITNESS_MAGIC {
            return Err(WitnessDecodeError::InvalidMagic { expected: *WITNESS_MAGIC, found: magic });
        }

        let version = cursor.read_u16()?;
        if version != WITNESS_VERSION {
            return Err(WitnessDecodeError::UnsupportedVersion { supported: WITNESS_VERSION, found: version });
        }

        let mut marker = [0u8; 4];
        cursor.read_exact(&mut marker).map_err(|_| WitnessDecodeError::UnexpectedEndOfData { context: "registry marker".to_string() })?;
        if &marker != b"REGI" {
            return Err(WitnessDecodeError::InvalidMagic { expected: *b"REGI", found: marker });
        }

        let table_count = cursor.read_usize()?;
        if table_count > 1048576 {
            return Err(WitnessDecodeError::InvalidLength {
                field: "witness tables".to_string(),
                expected: "<= 1048576".to_string(),
                found: table_count,
            });
        }

        for _ in 0..table_count {
            let table = WitnessTable::read_from(&mut cursor)?;
            self.register(table);
        }

        Ok(())
    }
}

/// Builder for constructing witness tables during compilation.
///
/// This builder is used by the compiler to generate witness tables
/// from `imply` blocks and register them in the witness registry.
#[derive(Debug, Clone)]
pub struct WitnessTableBuilder {
    module_id: ModuleId,
    trait_id: TraitId,
    type_id: TypeId,
    method_entries: Vec<MethodEntry>,
    associated_types: HashMap<Identifier, TypeId>,
    type_metadata: Option<TypeMetadata>,
}

impl WitnessTableBuilder {
    /// Creates a new builder for a type-trait pair (local module).
    pub fn new(trait_id: TraitId, type_id: TypeId) -> Self {
        Self {
            module_id: ModuleId::LOCAL,
            trait_id,
            type_id,
            method_entries: Vec::new(),
            associated_types: HashMap::new(),
            type_metadata: None,
        }
    }

    /// Creates a new builder for a type-trait pair in a specific module.
    pub fn new_in_module(module_id: ModuleId, trait_id: TraitId, type_id: TypeId) -> Self {
        Self { module_id, trait_id, type_id, method_entries: Vec::new(), associated_types: HashMap::new(), type_metadata: None }
    }

    /// Sets the module ID for this witness table.
    pub fn with_module(mut self, module_id: ModuleId) -> Self {
        self.module_id = module_id;
        self
    }

    /// Sets the type metadata.
    pub fn with_metadata(mut self, metadata: TypeMetadata) -> Self {
        self.type_metadata = Some(metadata);
        self
    }

    /// Adds a method entry.
    pub fn add_method(mut self, entry: MethodEntry) -> Self {
        self.method_entries.push(entry);
        self
    }

    /// Adds an associated type binding.
    pub fn add_associated_type(mut self, name: Identifier, type_id: TypeId) -> Self {
        self.associated_types.insert(name, type_id);
        self
    }

    /// Builds the witness table.
    ///
    /// Returns None if type metadata was not provided.
    pub fn build(self) -> Option<WitnessTable> {
        let metadata = self.type_metadata?;
        Some(WitnessTable {
            module_id: self.module_id,
            trait_id: self.trait_id,
            type_id: self.type_id,
            method_entries: self.method_entries,
            associated_types: self.associated_types,
            type_metadata: metadata,
        })
    }
}
