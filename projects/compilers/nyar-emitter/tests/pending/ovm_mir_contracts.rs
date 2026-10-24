pub use nyar_emitter::{FragmentSubmission, testing::lower_fragment_to_nyar_module as lower_fragment_mir_to_nyar_module};

mod tests {
    include!("../src/lowering/backends/ovm/mir/tests.rs");
}
