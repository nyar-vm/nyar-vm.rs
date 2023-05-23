#![warn(missing_docs)]

//! Optional JIT acceleration for the Nyar bytecode runtime.
//!
//! Language-agnostic: consumes verified bytecode views only. Must not import
//! `nyar-language`, `nyar-vm`, or concrete language frontends.

mod artifact;
mod assumption;
mod baseline_scalar;
mod compiler;
mod deopt;
mod error;
mod machine_code;
mod request;
mod stack_map;

pub use artifact::JitCompiledArtifact;
pub use assumption::{JitAssumption, baseline_scalar_assumptions};
pub use baseline_scalar::{BaselineScalarJit, match_scalar_program};
pub use compiler::{DisabledJit, JitCompiler, StackMapJit};
pub use deopt::{
    DeoptFrame, DeoptMap, DeoptMapEntry, DeoptRestoreError, InlineFrameSpec, RestoredInterpreterFrame, RestoredLocal,
    build_baseline_deopt_map, build_inline_deopt_map, materialize_interpreter_frames,
};
pub use error::JitError;
pub use machine_code::{
    I32Binop, I32Cmp, IMM_ON_LEFT, IMM_ON_RIGHT, MACHINE_CODE_MAGIC, MachineCodeError, ScalarProgram,
    decode_scalar_program, encode_ret_const_i32, encode_ret_i32_add_locals, encode_ret_i32_binop_imm_local,
    encode_ret_i32_binop_locals, encode_ret_i32_cmp_imm_local, encode_ret_i32_cmp_locals,
    encode_ret_i32_select_cmp_consts, encode_ret_i32_select_cmp_locals, encode_ret_local,
};
pub use request::{JitCompileRequest, JitFunctionSpec};
pub use stack_map::{FunctionStackMaps, StackMapEntry, build_conservative_stack_maps};
