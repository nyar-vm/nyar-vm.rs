pub use nyar_emitter::{
    FragmentSubmission, nyar_backend_clr,
    testing::{augment_msil_with_suspend, dispatch_case_keys, lower_fragment_to_clr_msil},
};

pub mod lowering {
    pub mod clr {
        pub use nyar_emitter::testing::lower_fragment_to_clr_msil as lower_fragment_to_msil;
    }
}

mod tests {
    use super::*;
    include!("../src/lowering/backends/clr/suspend/tests.rs");
}
