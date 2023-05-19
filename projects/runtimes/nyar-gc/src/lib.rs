#![warn(missing_docs)]

//! Managed object heap and collector for Nyar VM.
//!
//! 语言无关：不依赖 `nyar-language` 或外码前端。外码布局由加载器转换为
//! [`LayoutDescriptor`]；策略提示见 [`GcPolicy`] / [`WorkloadHints`]。
//! 宿主持久根见 [`HostRoots`] / [`RootHandle`]；分代标签见 [`Generation`]。
//! 工作负载意图见 [`WorkloadIntent`] / [`StrategyController`]。
//! 并发标记协议骨架见 [`ConcurrentMarkController`]；SATB 缓冲挂在 [`WriteBarrier`]。

mod barrier;
mod collector;
mod concurrent;
mod concurrent_ticker;
mod controller;
mod generation;
mod heap;
mod intent;
mod layout;
mod policy;
mod promotion;
mod relocate;
mod roots;
mod trace;
mod value;

pub use barrier::{WriteBarrier, write_value_slot};
pub use collector::{GcRoots, GarbageCollector, TracePollReport};
pub use concurrent::{ConcurrentMarkController, ConcurrentMarkError, ConcurrentMarkEvent, ConcurrentMarkState};
pub use concurrent_ticker::ConcurrentMarkTicker;
pub use controller::{StrategyController, StrategyDecision};
pub use generation::Generation;
pub use heap::{ObjectHeap, ObjectPayload};
pub use intent::{IntentError, IntentSource, ObjectLifetimeHint, WorkloadIntent};
pub use layout::{LayoutDescriptor, LayoutId};
pub use policy::{GcMode, GcPolicy, WorkloadHints};
pub use promotion::PromotionFailure;
pub use relocate::RelocateMap;
pub use roots::{HostRoots, RootHandle};
pub use value::{CoroutineState, ObjectId, Value};
