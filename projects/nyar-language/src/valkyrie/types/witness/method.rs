use std::io::{Read, Write};

use crate::Identifier;

use super::{
    MethodId, ModuleId, TypeId, WitnessDecodeError,
    ids::{ByteReader, ByteWriter},
};

/// Path to a method implementation in the module system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MethodPath {
    /// The module where this method is defined.
    pub module_id: ModuleId,
    /// The module path components.
    pub module: std::sync::Arc<[Identifier]>,
    /// The type or trait name containing the method.
    pub container: Identifier,
    /// The method name.
    pub method: Identifier,
}

impl MethodPath {
    /// Creates a new method path with explicit module ID.
    pub fn new_with_module(module_id: ModuleId, module: Vec<Identifier>, container: Identifier, method: Identifier) -> Self {
        Self { module_id, module: module.into(), container, method }
    }

    /// Creates a new method path (local module).
    pub fn new(module: Vec<Identifier>, container: Identifier, method: Identifier) -> Self {
        Self { module_id: ModuleId::LOCAL, module: module.into(), container, method }
    }

    /// Returns the module path as a slice of identifiers.
    pub fn module_path(&self) -> &[Identifier] {
        &self.module
    }

    /// Checks if this method is defined in a foreign module.
    pub fn is_cross_module(&self) -> bool {
        self.module_id != ModuleId::LOCAL
    }

    /// Serializes the method path to bytes.
    ///
    /// # Returns
    /// A vector of bytes representing the serialized method path.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write_to(&mut buf).expect("Vec write should never fail");
        buf
    }

    /// Deserializes a method path from bytes.
    ///
    /// # Arguments
    /// * `data` - The byte slice to deserialize from.
    ///
    /// # Returns
    /// The deserialized method path, or an error if deserialization fails.
    pub fn from_bytes(data: &[u8]) -> Result<Self, WitnessDecodeError> {
        let mut cursor = std::io::Cursor::new(data);
        Self::read_from(&mut cursor)
    }

    pub(super) fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_u32(self.module_id.as_u32())?;
        writer.write_usize(self.module.len())?;
        for id in self.module.iter() {
            writer.write_identifier(id)?;
        }
        writer.write_identifier(&self.container)?;
        writer.write_identifier(&self.method)
    }

    pub(super) fn read_from<R: Read>(reader: &mut R) -> Result<Self, WitnessDecodeError> {
        let module_id = ModuleId::new(reader.read_u32()?);
        let module_len = reader.read_usize()?;
        if module_len > 256 {
            return Err(WitnessDecodeError::InvalidLength {
                field: "module path".to_string(),
                expected: "<= 256".to_string(),
                found: module_len,
            });
        }
        let mut module = Vec::with_capacity(module_len);
        for _ in 0..module_len {
            module.push(reader.read_identifier()?);
        }
        let container = reader.read_identifier()?;
        let method = reader.read_identifier()?;
        Ok(Self { module_id, module: module.into(), container, method })
    }
}

/// Entry in a witness table representing a method implementation.
///
/// Each method entry maps a trait method signature to its concrete implementation
/// for a specific type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MethodEntry {
    /// The identifier of the method in the trait definition.
    pub method_id: MethodId,
    /// The name of the method.
    pub name: Identifier,
    /// The type identifier where the method is implemented.
    pub implementing_type: TypeId,
    /// The fully qualified path to the method implementation.
    pub implementation_path: MethodPath,
    /// Whether this method is a default implementation from the trait.
    pub is_default: bool,
}

impl MethodEntry {
    /// Creates a new method entry.
    pub fn new(method_id: MethodId, name: Identifier, implementing_type: TypeId, implementation_path: MethodPath, is_default: bool) -> Self {
        Self { method_id, name, implementing_type, implementation_path, is_default }
    }

    /// Serializes the method entry to bytes.
    ///
    /// # Returns
    /// A vector of bytes representing the serialized method entry.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write_to(&mut buf).expect("Vec write should never fail");
        buf
    }

    /// Deserializes a method entry from bytes.
    ///
    /// # Arguments
    /// * `data` - The byte slice to deserialize from.
    ///
    /// # Returns
    /// The deserialized method entry, or an error if deserialization fails.
    pub fn from_bytes(data: &[u8]) -> Result<Self, WitnessDecodeError> {
        let mut cursor = std::io::Cursor::new(data);
        Self::read_from(&mut cursor)
    }

    pub(super) fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_u32(self.method_id.as_u32())?;
        writer.write_identifier(&self.name)?;
        writer.write_u32(self.implementing_type.as_u32())?;
        self.implementation_path.write_to(writer)?;
        writer.write_u8(if self.is_default { 1 } else { 0 })
    }

    pub(super) fn read_from<R: Read>(reader: &mut R) -> Result<Self, WitnessDecodeError> {
        let method_id = MethodId::new(reader.read_u32()?);
        let name = reader.read_identifier()?;
        let implementing_type = TypeId::new(reader.read_u32()?);
        let implementation_path = MethodPath::read_from(reader)?;
        let is_default = reader.read_u8()? != 0;
        Ok(Self { method_id, name, implementing_type, implementation_path, is_default })
    }
}
