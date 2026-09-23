//! Wasm MIR contract tests. Included bodies construct [`ExecutableFunction`] directly.

pub use nyar::{Identifier, QualifiedName};
pub use nyar_emitter::FragmentSubmission;

pub mod contracts {
    pub use nyar_emitter::contracts::*;
}

pub mod executable_provider {
    pub use nyar_emitter::executable_provider::*;
}

pub mod testing {
    pub use nyar_emitter::testing::*;
}

pub fn lower_fragment_mir_to_wasm_module(
    submission: &FragmentSubmission,
    export_name: &str,
) -> nyar_emitter::nyar_backend_wasi::WasmBinaryModule {
    testing::lower_fragment_to_wasm_mir_module(submission, export_name)
}

/// Returns the number of stable Node host imports injected for a module with no
/// source-declared imports. The list is part of the JS-glue ABI contract.
pub fn import_count_for_main() -> u8 {
    12
}

pub use testing::WASM_GC_ANYREF;

mod tests {
    use super::*;
    include!("../src/lowering/backends/wasm/mir/tests.rs");
}
