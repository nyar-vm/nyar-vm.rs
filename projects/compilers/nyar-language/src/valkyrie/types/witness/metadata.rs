use std::io::{Read, Write};

use crate::Identifier;

use super::{
    TypeId, WitnessDecodeError,
    ids::{ByteReader, ByteWriter},
};

/// Classification of type kinds in the Valkyrie type system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TypeKind {
    /// A primitive type (int, float, bool, etc.).
    Primitive,
    /// A struct or class type.
    Struct,
    /// An enum type.
    Enum,
    /// A trait type.
    Trait,
    /// A function type.
    Function,
    /// A tuple type.
    Tuple,
    /// An array type.
    Array,
    /// A generic type parameter.
    Generic,
    /// A type alias.
    Alias,
    /// An opaque foreign type.
    Foreign,
}

impl TypeKind {
    /// Converts the type kind to a byte representation.
    ///
    /// # Returns
    /// A single byte representing the type kind.
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Primitive => 0,
            Self::Struct => 1,
            Self::Enum => 2,
            Self::Trait => 3,
            Self::Function => 4,
            Self::Tuple => 5,
            Self::Array => 6,
            Self::Generic => 7,
            Self::Alias => 8,
            Self::Foreign => 9,
        }
    }

    /// Creates a type kind from its byte representation.
    ///
    /// # Arguments
    /// * `byte` - The byte to convert.
    ///
    /// # Returns
    /// The type kind, or an error if the byte is invalid.
    pub fn from_byte(byte: u8) -> Result<Self, WitnessDecodeError> {
        match byte {
            0 => Ok(Self::Primitive),
            1 => Ok(Self::Struct),
            2 => Ok(Self::Enum),
            3 => Ok(Self::Trait),
            4 => Ok(Self::Function),
            5 => Ok(Self::Tuple),
            6 => Ok(Self::Array),
            7 => Ok(Self::Generic),
            8 => Ok(Self::Alias),
            9 => Ok(Self::Foreign),
            _ => Err(WitnessDecodeError::InvalidLength { field: "TypeKind".to_string(), expected: "0-9".to_string(), found: byte as usize }),
        }
    }
}

/// Metadata about a type for runtime reflection and dispatch.
///
/// Type metadata contains information needed for dynamic type checking,
/// reflection, and runtime dispatch in the Valkyrie type system.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeMetadata {
    /// The unique identifier for this type.
    pub type_id: TypeId,
    /// The name of the type.
    pub name: Identifier,
    /// The size of the type in bytes, if known at compile time.
    pub size: Option<usize>,
    /// The alignment requirement in bytes, if known at compile time.
    pub alignment: Option<usize>,
    /// Whether this type is a value type (copied on assignment).
    pub is_value_type: bool,
    /// Whether this type is marked as nullable.
    pub is_nullable: bool,
    /// The type kind classification.
    pub kind: TypeKind,
}

impl TypeMetadata {
    /// Creates a new type metadata instance.
    pub fn new(type_id: TypeId, name: Identifier, kind: TypeKind) -> Self {
        Self { type_id, name, size: None, alignment: None, is_value_type: false, is_nullable: false, kind }
    }

    /// Sets the size and alignment of the type.
    pub fn with_layout(mut self, size: usize, alignment: usize) -> Self {
        self.size = Some(size);
        self.alignment = Some(alignment);
        self
    }

    /// Marks this type as a value type.
    pub fn as_value_type(mut self) -> Self {
        self.is_value_type = true;
        self
    }

    /// Marks this type as nullable.
    pub fn as_nullable(mut self) -> Self {
        self.is_nullable = true;
        self
    }

    pub(super) fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_u32(self.type_id.as_u32())?;
        writer.write_identifier(&self.name)?;
        writer.write_option_u64(self.size.map(|s| s as u64))?;
        writer.write_option_u64(self.alignment.map(|a| a as u64))?;
        writer.write_u8(if self.is_value_type { 1 } else { 0 })?;
        writer.write_u8(if self.is_nullable { 1 } else { 0 })?;
        writer.write_u8(self.kind.to_byte())
    }

    pub(super) fn read_from<R: Read>(reader: &mut R) -> Result<Self, WitnessDecodeError> {
        let type_id = TypeId::new(reader.read_u32()?);
        let name = reader.read_identifier()?;
        let size = reader.read_option_u64()?.map(|s| s as usize);
        let alignment = reader.read_option_u64()?.map(|a| a as usize);
        let is_value_type = reader.read_u8()? != 0;
        let is_nullable = reader.read_u8()? != 0;
        let kind = TypeKind::from_byte(reader.read_u8()?)?;
        Ok(Self { type_id, name, size, alignment, is_value_type, is_nullable, kind })
    }

    /// Serializes the type metadata to a byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.write_to(&mut bytes).expect("write to Vec<u8> should not fail");
        bytes
    }

    /// Deserializes type metadata from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, WitnessDecodeError> {
        let mut cursor = std::io::Cursor::new(data);
        Self::read_from(&mut cursor)
    }
}
