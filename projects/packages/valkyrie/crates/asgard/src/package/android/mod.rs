//! Android 二进制交付（`classes.dex` 尾段嵌入；无侧车 `.bin` / `.so`）。

mod project;

pub use project::{AndroidPackageOutput, package_android_project};
