/// Function metadata required to compile one bytecode region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitFunctionSpec {
    /// Code section byte offset.
    pub code_offset: i32,
    /// Code section byte length.
    pub code_length: i32,
    /// Local slot count.
    pub local_count: i32,
    /// Parameter arity.
    pub arity: i32,
    /// 内码 safepoint 指令下标（与 `ExecutableFunction.safepoints` 对齐）。
    pub safepoint_indices: Vec<u32>,
}

/// Language-agnostic JIT input: verified bytecode bytes plus one function slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitCompileRequest {
    /// Module format version.
    pub module_version: u32,
    /// Module name (diagnostics only).
    pub module_name: String,
    /// Flat code section bytes.
    pub code_bytes: Vec<u8>,
    /// 常量池中的 `Integer32`（其它种类为 `None`，下标与外码一致）。
    pub constant_i32: Vec<Option<i32>>,
    /// Index into the module function table.
    pub function_index: usize,
    /// Selected function metadata.
    pub function: JitFunctionSpec,
}

impl JitCompileRequest {
    /// Returns the bytecode slice for the selected function.
    pub fn function_code(&self) -> Result<&[u8], crate::JitError> {
        let offset =
            usize::try_from(self.function.code_offset).map_err(|_| crate::JitError::InvalidBytecode("negative code_offset".to_string()))?;
        let length =
            usize::try_from(self.function.code_length).map_err(|_| crate::JitError::InvalidBytecode("negative code_length".to_string()))?;
        let end = offset + length;
        if end > self.code_bytes.len() {
            return Err(crate::JitError::InvalidBytecode(format!(
                "function code range [{offset}, {end}) exceeds code section length {len}",
                len = self.code_bytes.len()
            )));
        }
        Ok(&self.code_bytes[offset..end])
    }
}
