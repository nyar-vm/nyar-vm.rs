pub use nyar_emitter::{FragmentSubmission, nyar_backend_jvm, testing::dispatch_case_keys};

pub mod lowering {
    pub mod jvm {
        pub use nyar_emitter::testing::lower_fragment_to_jvm_class;
    }
}

mod tests {
    use super::*;
    include!("../src/lowering/backends/jvm/suspend/tests.rs");
}
