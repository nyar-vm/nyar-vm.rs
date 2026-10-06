//! 二进制格式模型（过渡 `vcc-data` → `acorn-*`）。
//!
//! JVM `class` / `JAR` 已迁入 `acorn-jvm`；`PE` / `COFF` / `ELF` 仍经 `vcc-data`。

pub use acorn_jvm::{class, jar};
pub use vcc_data::binary::{coff, elf, pe};
