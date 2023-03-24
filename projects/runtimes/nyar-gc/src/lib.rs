#![warn(missing_docs)]

//! Managed object heap and collector for Nyar VM.
//!
//! 语言无关：不依赖 `nyar-language` 或外码前端。外码布局由加载器转换为
//! [`LayoutDescriptor`]；策略提示见 [`GcPolicy`] / [`WorkloadHints`]。

mod barrier;
mod collector;
mod heap;
mod layout;
mod policy;
mod trace;
mod value;

pub use barrier::{WriteBarrier, write_value_slot};
pub use collector::{GcRoots, GarbageCollector};
pub use heap::{ObjectHeap, ObjectPayload};
pub use layout::{LayoutDescriptor, LayoutId};
pub use policy::{GcMode, GcPolicy, WorkloadHints};
pub use value::{CoroutineState, ObjectId, Value};
