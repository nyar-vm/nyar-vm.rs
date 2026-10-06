//! 二进制格式模型（过渡 `vcc-data` → `acorn-*`）。
//!
//! JVM `class` / `JAR` 已迁入 `acorn-jvm`。
//! `COFF` / `ELF` / `PE`（含托管 `CLR` 写出）已迁入 `acorn-pe`。

pub use acorn_jvm::{class, jar};
pub use acorn_pe::{coff, elf, pe};
