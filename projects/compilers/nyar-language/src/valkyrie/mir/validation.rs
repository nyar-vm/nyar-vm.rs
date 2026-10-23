//! Semantic MIR 与 MIR 的合同校验。
//!
//! 两个事实域各自独立：
//!
//! - `semantic_contract` / `symbols` / `module_validation` / `function_validation` 负责
//!   Semantic MIR 的 stable-ID、聚合、名义结构与静态调用合同。
//! - `control_flow` / `effect_validation` / `type_inference` / `diagnostics` 负责 MIR 的
//!   可达块、effect 恢复与静态类型校验。
//!
//! 本模块只拒绝不合法 IR，不创建、不修正 callable、layout 或类型等语义事实；
//! 根文件只声明子模块并重导出公共项。

mod control_flow;
mod diagnostics;
mod effect_validation;
mod errors;
mod function_validation;
mod module_validation;
mod semantic_contract;
mod symbols;
mod type_inference;

pub use errors::MirValidationError;
pub use module_validation::validate_module;
pub use semantic_contract::{SemanticMirContractError, semantic_observation, validate_semantic_module};
pub use symbols::is_language_builtin_symbol;
