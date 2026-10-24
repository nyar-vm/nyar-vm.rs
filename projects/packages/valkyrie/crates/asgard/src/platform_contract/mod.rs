//! 跨平台 UiHost / ASGARD 硬门禁（防平台 drift）。

mod checks;
mod gates;

pub use checks::{
    GateFailure, check_android_compose_bootstrap, check_android_compose_vendor, check_android_dist, check_ios_dist, check_no_legacy_magic,
    expected_magics, format_gate_failures, is_vendor_compose_dex, validate_android_apk_dex, validate_android_dist_bytes,
    validate_android_dist_dir, validate_android_native_so, validate_ios_dist_bytes, validate_runtime_source_cip, validate_terminal_dist_dir,
};
pub use gates::GateId;
