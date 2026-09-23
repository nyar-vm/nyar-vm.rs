//! TextVM 编译器：字面量模式解析、静态分析与 `.tvm` 产物生成。

pub mod dfa_builder;
pub mod parser;
pub mod static_analyzer;
pub mod static_query;

pub use parser::{ParseError, ParsedPattern};
pub use static_analyzer::{Complexity, analyze};
pub use static_query::{CompileError, CompiledQuery, compile};
