use super::{ModuleId, TraitId, TypeId};

/// Error type for cross-module witness table operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrossModuleError {
    /// The module ID is invalid for the operation.
    InvalidModuleId,
    /// The module was not found.
    ModuleNotFound {
        /// The module ID that was not found.
        module_id: ModuleId,
    },
    /// The trait was not found in the module.
    TraitNotFound {
        /// The module ID.
        module_id: ModuleId,
        /// The trait ID that was not found.
        trait_id: TraitId,
    },
    /// The type does not implement the trait in the module.
    ImplementationNotFound {
        /// The module ID.
        module_id: ModuleId,
        /// The type ID.
        type_id: TypeId,
        /// The trait ID.
        trait_id: TraitId,
    },
    /// Circular dependency detected during cross-module resolution.
    CircularDependency {
        /// The dependency chain.
        chain: Vec<ModuleId>,
    },
    /// Version mismatch between modules.
    VersionMismatch {
        /// Expected version.
        expected: u16,
        /// Found version.
        found: u16,
    },
}

impl std::fmt::Display for CrossModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidModuleId => write!(f, "Invalid module ID for cross-module operation"),
            Self::ModuleNotFound { module_id } => {
                write!(f, "Module {} not found", module_id.as_u32())
            }
            Self::TraitNotFound { module_id, trait_id } => {
                write!(f, "Trait {} not found in module {}", trait_id.as_u32(), module_id.as_u32())
            }
            Self::ImplementationNotFound { module_id, type_id, trait_id } => {
                write!(f, "Type {} does not implement trait {} in module {}", type_id.as_u32(), trait_id.as_u32(), module_id.as_u32())
            }
            Self::CircularDependency { chain } => {
                write!(f, "Circular dependency detected: ")?;
                for (i, module_id) in chain.iter().enumerate() {
                    if i > 0 {
                        write!(f, " -> ")?;
                    }
                    write!(f, "{}", module_id.as_u32())?;
                }
                Ok(())
            }
            Self::VersionMismatch { expected, found } => {
                write!(f, "Version mismatch: expected {}, found {}", expected, found)
            }
        }
    }
}

impl std::error::Error for CrossModuleError {}

/// Error type for witness table decoding operations.
///
/// This error is returned when binary deserialization of a witness table fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessDecodeError {
    /// The magic number in the header is invalid.
    InvalidMagic {
        /// The expected magic number.
        expected: [u8; 4],
        /// The actual magic number found.
        found: [u8; 4],
    },
    /// The version is not supported.
    UnsupportedVersion {
        /// The supported version.
        supported: u16,
        /// The version found in the data.
        found: u16,
    },
    /// Unexpected end of data while reading.
    UnexpectedEndOfData {
        /// Description of what was being read.
        context: String,
    },
    /// Invalid UTF-8 string encountered.
    InvalidUtf8 {
        /// The UTF-8 error message.
        message: String,
    },
    /// Invalid data length encountered.
    InvalidLength {
        /// Description of the field.
        field: String,
        /// The expected length or range.
        expected: String,
        /// The actual length.
        found: usize,
    },
    /// IO error during read/write operations.
    IoError {
        /// Description of the IO operation.
        context: String,
    },
    /// Duplicate entry found during deserialization.
    DuplicateEntry {
        /// Description of the duplicate entry.
        entry_type: String,
    },
}

impl std::fmt::Display for WitnessDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic { expected, found } => {
                write!(f, "Invalid magic number: expected {:?}, found {:?}", std::str::from_utf8(expected), std::str::from_utf8(found))
            }
            Self::UnsupportedVersion { supported, found } => {
                write!(f, "Unsupported version: expected {}, found {}", supported, found)
            }
            Self::UnexpectedEndOfData { context } => {
                write!(f, "Unexpected end of data while reading: {}", context)
            }
            Self::InvalidUtf8 { message } => {
                write!(f, "Invalid UTF-8 string: {}", message)
            }
            Self::InvalidLength { field, expected, found } => {
                write!(f, "Invalid length for '{}': expected {}, found {}", field, expected, found)
            }
            Self::IoError { context } => {
                write!(f, "IO error: {}", context)
            }
            Self::DuplicateEntry { entry_type } => {
                write!(f, "Duplicate entry found: {}", entry_type)
            }
        }
    }
}

impl std::error::Error for WitnessDecodeError {}
