pub use nyar_emitter::{
    FragmentSubmission,
    testing::{
        NATIVE_VALUE_AREA_BASE, SUSPEND_SPILL_RSP_OFFSET, emit_native_witness_tables as emit_witness_tables,
        lower_fragment_to_native_executable, lower_mir_functions_to_native_msvc, lower_mir_functions_to_native_sysv,
        lower_suspend_witness_calls_linux, lower_suspend_witness_calls_windows, native_value_area_size,
    },
};

mod tests {
    use super::*;
    include!("../src/lowering/backends/native/tests.rs");
}
