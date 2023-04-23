#![warn(missing_docs)]

//! Nyar bytecode VM — language-agnostic `.nyar` / Nyar IR runtime.
//!
//! # Layering
//!
//! - This crate executes **decoded Nyar IR modules** only. It must not import concrete
//!   language ASTs or compilers.
//! - Managed heap / GC live in **`nyar-gc`**; optional JIT lives in **`nyar-jit`**.
//! - This crate owns the interpreter loop.
//! - Concrete languages lower through `emitter` into `.nyar` bytes before this VM sees them.
//!
//! # Naming
//!
//! Crate package: `nyar-vm`. Rust import path: `nyar_vm`. CLI binary: `nyar-vm`.

pub mod array_runtime;
pub mod deopt_value;
pub mod error;
pub mod executable;
pub mod executor;
pub mod frame;
pub mod host;
pub mod jit;
pub mod json_bridge;
pub mod module;
pub mod ops;
pub mod stack;
pub mod value;
pub mod verify;
pub mod vm;
pub mod workload_json;

pub use deopt_value::{decode_value_from_deopt, encode_value_for_deopt};
pub use error::NyarRuntimeError;
pub use executable::{ExecOp, ExecutableFunction, InstructionIndex};
pub use module::ModuleGlobals;
pub use nyar_gc::{
    ConcurrentMarkController, ConcurrentMarkError, ConcurrentMarkEvent, ConcurrentMarkState, ConcurrentMarkTicker, GarbageCollector,
    GcMode, GcPolicy, GcRoots, Generation, HostRoots, IntentError, IntentSource, LayoutDescriptor, LayoutId, ObjectHeap,
    ObjectPayload, ObjectLifetimeHint, PromotionFailure, RelocateMap, RootHandle, StrategyController, StrategyDecision,
    WorkloadHints, WorkloadIntent, WriteBarrier,
};
pub use value::{CoroutineState, ObjectId, Value};
pub use vm::NyarVm;
