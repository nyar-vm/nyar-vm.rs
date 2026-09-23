//! Language-neutral PE residualization sink (Futamura 1st projection target).
//!
//! Guest specialize paths residualize interpreter match arms into this sink.
//! The host (`legacy-vm`) implements the **product** sink as a **native-bound**
//! residual (`NativeResidualSink` → x64 / Windows PE). Guests never depend on
//! the VM crate. A stack→`.nyar` sink may exist as a temporary probe only —
//! it is **not** the host-script PE destination.

/// Target for residual operations produced by specializing an interpreter w.r.t. a program.
pub trait ResidualSink {
    /// Push an integer constant.
    fn push_i64(&mut self, value: i64);
    /// Push a boolean constant.
    fn push_bool(&mut self, value: bool);
    /// Push a string constant.
    fn push_string(&mut self, value: &str);
    /// Push `nil` / null.
    fn push_nil(&mut self);
    /// Load a named local onto the stack.
    fn load(&mut self, name: &str);
    /// Store stack top into a named local.
    fn store(&mut self, name: &str);
    /// Pop stack top.
    fn pop(&mut self);
    /// Binary `+` (numeric; string `+` is not used — prefer [`Self::concat`]).
    fn add(&mut self);
    /// Binary `-`.
    fn sub(&mut self);
    /// Binary `*`.
    fn mul(&mut self);
    /// Binary `/` (integer truncating when both ints).
    fn div(&mut self);
    /// Binary `%`.
    fn rem(&mut self);
    /// Binary `==`.
    fn eq(&mut self);
    /// Binary `~=` / `!=`.
    fn ne(&mut self);
    /// Binary `<`.
    fn lt(&mut self);
    /// Binary `<=`.
    fn le(&mut self);
    /// Binary `>`.
    fn gt(&mut self);
    /// Binary `>=`.
    fn ge(&mut self);
    /// String concatenation (`..`).
    fn concat(&mut self);
    /// Logical not (consumes one value, pushes bool).
    fn not(&mut self);
    /// `print` with `argc` arguments already on the stack (left-to-right, top = last).
    fn print(&mut self, argc: usize);
    /// Bind a label at the current code position.
    fn label(&mut self, name: &str);
    /// Unconditional jump to label.
    fn jump(&mut self, label: &str);
    /// Jump when stack top is falsey (Lua: only `nil`/`false`); pops condition.
    fn jump_if_false(&mut self, label: &str);
    /// Jump when stack top is truthy; pops condition.
    fn jump_if_true(&mut self, label: &str);
    /// Begin a residual function.
    fn begin_function(&mut self, name: &str);
    /// Declare a parameter (must follow [`Self::begin_function`]).
    fn add_parameter(&mut self, name: &str);
    /// End the current function.
    fn end_function(&mut self);
    /// Call a residual function by name with `argc` args on the stack.
    fn call(&mut self, name: &str, argc: usize);
    /// Return stack top (or nil if empty — sink may push nil).
    fn ret(&mut self);
    /// Report an unsupported construct (specialization aborts).
    fn unsupported(&mut self, what: &str) -> Result<(), String> {
        Err(format!("PE residualization unsupported: {what}"))
    }
}
