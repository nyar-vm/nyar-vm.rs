pub use nyar_emitter::{FragmentSubmission, nyar_backend_wasi};

pub mod lowering {
    pub mod wasm {
        pub use nyar_emitter::testing::{lower_fragment_to_wasm_module, suspend_run_loop_with_witness_wasm_bytes};
    }
}

mod tests {
    include!("../src/lowering/backends/wasm/tests.rs");
}
