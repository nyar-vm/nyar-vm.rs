//! Guest host script AST 与 parser（过渡层，待 Oak 前端 + 解释器重写）。

/// Bash script model and parser（已自 vcc-data 迁入）。
pub mod bash;

/// C script model and parser（已自 vcc-data 迁入）。
pub mod c;

/// Lua script model and parser（已自 vcc-data 迁入）。
pub mod lua;

/// PowerShell script model and parser（已自 vcc-data 迁入）。
pub mod powershell;

/// Tcl script model and parser（已自 vcc-data 迁入）。
pub mod tcl;
