//! JVM MIR contract tests. Included bodies construct [`ExecutableFunction`] directly
//! (no `nyar-language` MIR), so this crate root re-exports driver modules as `crate::*`.

pub use nyar::{Identifier, QualifiedName};
pub use nyar_emitter::{
    FragmentSubmission,
    nyar_backend_jvm::{JvmInstruction, JvmMethodDescriptor, JvmTypeDescriptor},
};
pub use ordered_float;

pub mod contracts {
    pub use nyar_emitter::contracts::*;
}

pub mod executable_provider {
    pub use nyar_emitter::executable_provider::*;
}

pub mod testing {
    pub use nyar_emitter::testing::*;
}

pub use nyar_emitter::testing::{JvmLocalKind, jvm_local_slot_conflicts};

pub fn lower_mir_function_to_jvm(
    submission: &FragmentSubmission,
    operation: &nyar::QualifiedName,
    mir_function: &contracts::ExecutableFunction,
) -> nyar_emitter::nyar_backend_jvm::JvmMethodSignature {
    testing::lower_mir_to_jvm_method(submission, operation, mir_function)
}

mod tests {
    use super::*;
    include!("../src/lowering/backends/jvm/mir/tests.rs");
}
