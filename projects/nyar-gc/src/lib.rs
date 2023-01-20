#![warn(missing_docs)]

//! Managed object heap and mark-sweep collector for Nyar VM.
//!
//! Language-agnostic: no dependency on `nyar-language` or bytecode frontends.

mod collector;
mod heap;
mod trace;
mod value;

pub use collector::{GcRoots, GarbageCollector};
pub use heap::{ObjectHeap, ObjectPayload};
pub use value::{CoroutineState, ObjectId, Value};
