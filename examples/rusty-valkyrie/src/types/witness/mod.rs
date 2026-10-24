#![doc = include_str!("readme.md")]

mod error;
mod ids;
mod metadata;
mod method;
mod registry;
mod table;

pub use error::{CrossModuleError, WitnessDecodeError};
pub use ids::{MethodId, ModuleId, TraitId, TypeId, WITNESS_MAGIC, WITNESS_VERSION};
pub use metadata::{TypeKind, TypeMetadata};
pub use method::{MethodEntry, MethodPath};
pub use registry::{WitnessRegistry, WitnessTableBuilder};
pub use table::{TraitObject, WitnessTable};
