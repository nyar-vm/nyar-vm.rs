#![deny(missing_docs)]

//! Nyar `.nyar` / `.legion` 外码格式合同（储存码）。
//!
//! 本 crate 是 opcode、section、encode/decode 与纯格式结构的唯一所有者。
//! 内码、解释器、GC 与 JIT 不得在此定义运行时执行表示。
//!
//! 模块边界：
//!
//! - [`opcode`]：头码、编码形、指令解码与写入 helper。
//! - [`model`]：常量池、函数、导入导出、全局槽、witness 与布局等纯结构。
//! - [`errors`]：解码错误类型。
//! - [`decode`]：模块入口与各 section 解析器。
//! - [`encode`]：模块入口与各 section 序列化器。
//!
//! 根模块只做导出，不承载格式细节。

mod decode;
mod encode;
mod errors;
mod model;
mod opcode;

pub use decode::decode_module;
pub use encode::encode_module;
pub use errors::NyarDecodeError;
pub use model::{
    NyarConstant, NyarConstantKind, NyarExport, NyarExportKind, NyarFunction, NyarGlobal, NyarImport, NyarImportKind, NyarLayout,
    NyarModuleData, NyarSectionKind, NyarWitnessDispatchEntry,
};
pub use opcode::{
    BYTECODE_FORMAT_VERSION, HEADER_SIZE, NYAR_MAGIC, NYAR_VERSION, NyarHeadCode, NyarInstruction, NyarInstructionForm, OBSOLETE_CALL_NATIVE,
    SECTION_HEADER_SIZE, decode_at, emit_imm1, emit_imm2, emit_plain,
};
