//! TextVM 运行时：字面量 / DFA / 回溯执行器与查询入口。

pub mod code_point;
pub mod encoding;
pub mod engine;
pub mod executors;
pub mod inst;
pub mod tvm;

pub use code_point::CodePointIter;
pub use encoding::TextEncoding;
pub use engine::{CompiledUnit, EngineLoader, Match};
pub use inst::{Inst, InstKind};
