use std::io::{Read, Write};

use crate::Identifier;

pub const WITNESS_MAGIC: &[u8; 4] = b"VWIT";
pub const WITNESS_VERSION: u16 = 0x0002;

/// Unique identifier for a module in the compilation system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ModuleId(pub u32);

impl ModuleId {
    /// Creates a new module identifier.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw identifier value.
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    /// Reserved identifier for the current module (local reference).
    pub const LOCAL: Self = Self(0);

    /// Reserved identifier for the standard library module.
    pub const STDLIB: Self = Self(1);
}

/// Unique identifier for a trait in the type system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraitId(pub u32);

impl TraitId {
    /// Creates a new trait identifier.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw identifier value.
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Unique identifier for a type in the type system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeId(pub u32);

impl TypeId {
    /// Creates a new type identifier.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw identifier value.
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Unique identifier for a method within a trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MethodId(pub u32);

impl MethodId {
    /// Creates a new method identifier.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw identifier value.
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

pub(super) trait ByteWriter: Write {
    fn write_u8(&mut self, value: u8) -> std::io::Result<()> {
        self.write_all(&[value])
    }

    fn write_u16(&mut self, value: u16) -> std::io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) -> std::io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    fn write_u64(&mut self, value: u64) -> std::io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    fn write_usize(&mut self, value: usize) -> std::io::Result<()> {
        self.write_u64(value as u64)
    }

    fn write_str(&mut self, value: &str) -> std::io::Result<()> {
        let bytes = value.as_bytes();
        self.write_usize(bytes.len())?;
        self.write_all(bytes)
    }

    fn write_identifier(&mut self, id: &Identifier) -> std::io::Result<()> {
        self.write_str(id.as_str())
    }

    fn write_option_u64(&mut self, value: Option<u64>) -> std::io::Result<()> {
        if let Some(v) = value {
            self.write_u8(1)?;
            self.write_u64(v)
        }
        else {
            self.write_u8(0)
        }
    }
}

impl<W: Write> ByteWriter for W {}

pub(super) trait ByteReader: Read {
    fn read_u8(&mut self) -> Result<u8, super::WitnessDecodeError> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf).map_err(|_| super::WitnessDecodeError::UnexpectedEndOfData { context: "u8".to_string() })?;
        Ok(buf[0])
    }

    fn read_u16(&mut self) -> Result<u16, super::WitnessDecodeError> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf).map_err(|_| super::WitnessDecodeError::UnexpectedEndOfData { context: "u16".to_string() })?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_u32(&mut self) -> Result<u32, super::WitnessDecodeError> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf).map_err(|_| super::WitnessDecodeError::UnexpectedEndOfData { context: "u32".to_string() })?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64(&mut self) -> Result<u64, super::WitnessDecodeError> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf).map_err(|_| super::WitnessDecodeError::UnexpectedEndOfData { context: "u64".to_string() })?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_usize(&mut self) -> Result<usize, super::WitnessDecodeError> {
        self.read_u64().map(|v| v as usize)
    }

    fn read_string(&mut self) -> Result<String, super::WitnessDecodeError> {
        let len = self.read_usize()?;
        if len > 1024 * 1024 {
            return Err(super::WitnessDecodeError::InvalidLength { field: "string".to_string(), expected: "<= 1MB".to_string(), found: len });
        }
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf).map_err(|_| super::WitnessDecodeError::UnexpectedEndOfData { context: "string".to_string() })?;
        String::from_utf8(buf).map_err(|e| super::WitnessDecodeError::InvalidUtf8 { message: e.to_string() })
    }

    fn read_identifier(&mut self) -> Result<Identifier, super::WitnessDecodeError> {
        let s = self.read_string()?;
        Ok(Identifier::new(&s))
    }

    fn read_option_u64(&mut self) -> Result<Option<u64>, super::WitnessDecodeError> {
        let flag = self.read_u8()?;
        if flag == 1 { Ok(Some(self.read_u64()?)) } else { Ok(None) }
    }
}

impl<R: Read> ByteReader for R {}
