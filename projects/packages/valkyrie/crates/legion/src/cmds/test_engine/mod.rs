//! 测试/基准共享引擎。

mod discover;
mod session;
mod targets;

pub use discover::{DiscoveredFunction, discover_project_tests, discover_test_functions, extract_function_name};
pub use session::{bench_project, run_tests_for_project};
pub use targets::{parse_target_label, resolve_test_targets};
