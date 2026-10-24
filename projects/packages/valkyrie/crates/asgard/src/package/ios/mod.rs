//! iOS 二进制交付（`AsgardHost` 唯一制品；无 `host.bin` / `ui.bin`）。

mod project;

pub use project::{IosPackageOutput, package_ios_project};
