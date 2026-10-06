//! Guest host script AST 与 parser（过渡 `vcc-data`，待 `oak-bash` 等 Oak 前端 + 解释器重写）。

/// Bash script model and parser.
pub mod bash {
    pub use vcc_data::text::bash::*;
}

/// C script model and parser.
pub mod c {
    pub use vcc_data::text::c::*;
}

/// Lua script model and parser.
pub mod lua {
    pub use vcc_data::text::lua::*;
}

/// PowerShell script model and parser.
pub mod powershell {
    pub use vcc_data::text::powershell::*;
}

/// Tcl script model and parser.
pub mod tcl {
    pub use vcc_data::text::tcl::*;
}
