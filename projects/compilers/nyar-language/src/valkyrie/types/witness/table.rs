use std::{
    collections::HashMap,
    io::{Read, Write},
};

use crate::Identifier;

use super::{
    MethodEntry, MethodId, ModuleId, TraitId, TypeId, TypeMetadata, WITNESS_MAGIC, WITNESS_VERSION, WitnessDecodeError,
    ids::{ByteReader, ByteWriter},
};
/// Witness table for dynamic trait dispatch.
///
/// A witness table is the runtime representation of a type's implementation
/// of a trait. It contains mappings from trait methods to their concrete
/// implementations and associated type bindings.
///
/// In the Valkyrie OOP system, witness tables enable:
/// - Dynamic dispatch for trait methods
/// - Runtime type information for reflection
/// - Associated type resolution
/// - Cross-module trait implementation linking
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WitnessTable {
    /// The module where this witness table is defined.
    pub module_id: ModuleId,
    /// The trait being implemented.
    pub trait_id: TraitId,
    /// The type implementing the trait.
    pub type_id: TypeId,
    /// Method implementations for the trait.
    pub method_entries: Vec<MethodEntry>,
    /// Associated type bindings for the trait.
    pub associated_types: HashMap<Identifier, TypeId>,
    /// Metadata about the implementing type.
    pub type_metadata: TypeMetadata,
}

impl WitnessTable {
    /// Creates a new witness table for a type-trait pair in a specific module.
    pub fn new_in_module(module_id: ModuleId, trait_id: TraitId, type_id: TypeId, type_metadata: TypeMetadata) -> Self {
        Self { module_id, trait_id, type_id, method_entries: Vec::new(), associated_types: HashMap::new(), type_metadata }
    }

    /// Creates a new witness table for a type-trait pair (local module).
    pub fn new(trait_id: TraitId, type_id: TypeId, type_metadata: TypeMetadata) -> Self {
        Self::new_in_module(ModuleId::LOCAL, trait_id, type_id, type_metadata)
    }

    /// Adds a method entry to the witness table.
    pub fn add_method(&mut self, entry: MethodEntry) {
        self.method_entries.push(entry);
    }

    /// Adds an associated type binding to the witness table.
    pub fn add_associated_type(&mut self, name: Identifier, type_id: TypeId) {
        self.associated_types.insert(name, type_id);
    }

    /// Looks up a method by its identifier.
    pub fn find_method(&self, method_id: MethodId) -> Option<&MethodEntry> {
        self.method_entries.iter().find(|e| e.method_id == method_id)
    }

    /// Looks up a method by its name.
    pub fn find_method_by_name(&self, name: &Identifier) -> Option<&MethodEntry> {
        self.method_entries.iter().find(|e| &e.name == name)
    }

    /// Looks up an associated type by its name.
    pub fn get_associated_type(&self, name: &Identifier) -> Option<TypeId> {
        self.associated_types.get(name).copied()
    }

    /// Returns the number of method entries.
    pub fn method_count(&self) -> usize {
        self.method_entries.len()
    }

    /// Returns the number of associated types.
    pub fn associated_type_count(&self) -> usize {
        self.associated_types.len()
    }

    /// Serializes the witness table to bytes.
    ///
    /// The binary format is:
    /// - Magic number (4 bytes): "VWIT"
    /// - Version (2 bytes): 0x0002
    /// - Module ID (4 bytes)
    /// - Trait ID (4 bytes)
    /// - Type ID (4 bytes)
    /// - Method count (8 bytes)
    /// - Method entries (variable)
    /// - Associated type count (8 bytes)
    /// - Associated type bindings (variable)
    /// - Type metadata (variable)
    ///
    /// # Returns
    /// A vector of bytes representing the serialized witness table.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write_to(&mut buf).expect("Vec write should never fail");
        buf
    }

    /// Deserializes a witness table from bytes.
    ///
    /// # Arguments
    /// * `data` - The byte slice to deserialize from.
    ///
    /// # Returns
    /// The deserialized witness table, or an error if deserialization fails.
    pub fn from_bytes(data: &[u8]) -> Result<Self, WitnessDecodeError> {
        let mut cursor = std::io::Cursor::new(data);
        Self::read_from(&mut cursor)
    }

    pub(super) fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(WITNESS_MAGIC)?;
        writer.write_u16(WITNESS_VERSION)?;
        writer.write_u32(self.module_id.as_u32())?;
        writer.write_u32(self.trait_id.as_u32())?;
        writer.write_u32(self.type_id.as_u32())?;
        writer.write_usize(self.method_entries.len())?;
        for entry in &self.method_entries {
            entry.write_to(writer)?;
        }
        writer.write_usize(self.associated_types.len())?;
        for (name, type_id) in &self.associated_types {
            writer.write_identifier(name)?;
            writer.write_u32(type_id.as_u32())?;
        }
        self.type_metadata.write_to(writer)
    }

    pub(super) fn read_from<R: Read>(reader: &mut R) -> Result<Self, WitnessDecodeError> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(|_| WitnessDecodeError::UnexpectedEndOfData { context: "magic number".to_string() })?;
        if &magic != WITNESS_MAGIC {
            return Err(WitnessDecodeError::InvalidMagic { expected: *WITNESS_MAGIC, found: magic });
        }

        let version = reader.read_u16()?;
        if version != WITNESS_VERSION {
            return Err(WitnessDecodeError::UnsupportedVersion { supported: WITNESS_VERSION, found: version });
        }

        let module_id = ModuleId::new(reader.read_u32()?);
        let trait_id = TraitId::new(reader.read_u32()?);
        let type_id = TypeId::new(reader.read_u32()?);

        let method_count = reader.read_usize()?;
        if method_count > 65536 {
            return Err(WitnessDecodeError::InvalidLength {
                field: "method entries".to_string(),
                expected: "<= 65536".to_string(),
                found: method_count,
            });
        }
        let mut method_entries = Vec::with_capacity(method_count);
        for _ in 0..method_count {
            method_entries.push(MethodEntry::read_from(reader)?);
        }

        let associated_type_count = reader.read_usize()?;
        if associated_type_count > 1024 {
            return Err(WitnessDecodeError::InvalidLength {
                field: "associated types".to_string(),
                expected: "<= 1024".to_string(),
                found: associated_type_count,
            });
        }
        let mut associated_types = HashMap::with_capacity(associated_type_count);
        for _ in 0..associated_type_count {
            let name = reader.read_identifier()?;
            let type_id_val = TypeId::new(reader.read_u32()?);
            associated_types.insert(name, type_id_val);
        }

        let type_metadata = TypeMetadata::read_from(reader)?;

        Ok(Self { module_id, trait_id, type_id, method_entries, associated_types, type_metadata })
    }
}

/// Runtime representation of a trait object (fat pointer).
///
/// A trait object consists of two pointers:
/// - data: Pointer to the actual object data
/// - witness: Pointer to the witness table for dynamic dispatch
///
/// This enables dynamic method dispatch through the witness table
/// while maintaining type safety.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraitObject {
    /// The module where the witness table is defined.
    pub module_id: ModuleId,
    /// The type ID of the concrete type.
    pub concrete_type: TypeId,
    /// The trait being implemented.
    pub trait_id: TraitId,
    /// Pointer to the witness table (index into registry).
    pub witness_table_index: usize,
}

impl TraitObject {
    /// Creates a new trait object (local module).
    pub fn new(concrete_type: TypeId, trait_id: TraitId, witness_table_index: usize) -> Self {
        Self::new_in_module(ModuleId::LOCAL, concrete_type, trait_id, witness_table_index)
    }

    /// Creates a new trait object with explicit module ID.
    pub fn new_in_module(module_id: ModuleId, concrete_type: TypeId, trait_id: TraitId, witness_table_index: usize) -> Self {
        Self { module_id, concrete_type, trait_id, witness_table_index }
    }

    /// Checks if this trait object references a foreign module.
    pub fn is_cross_module(&self) -> bool {
        self.module_id != ModuleId::LOCAL
    }
}
