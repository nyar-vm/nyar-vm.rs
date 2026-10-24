//! `.nyar` / `.legion` 模块的数据模型。
//!
//! 本模块只定义常量池、函数、导入导出、全局槽、witness 分派与布局描述等
//! 纯格式结构；不包含 section 的编解码行为。

/// Constant pool entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarConstantKind {
    /// 空值常量。
    Null = 0x00,
    /// 布尔常量。
    Boolean = 0x01,
    /// UTF-8 字符串常量。
    String = 0x05,
    /// 大整数常量（字节序列）。
    BigInt = 0x06,
    /// 32 位有符号整数常量。
    Integer32 = 0x11,
    /// 64 位浮点常量。
    Float64 = 0x22,
}

/// Constant pool entry.
#[derive(Debug, Clone, PartialEq)]
pub enum NyarConstant {
    /// 空值。
    Null,
    /// 布尔值。
    Boolean(bool),
    /// 32 位有符号整数。
    Integer32(i32),
    /// 64 位浮点数。
    Float64(f64),
    /// UTF-8 字符串。
    String(String),
    /// 大整数字节表示。
    BigInt(Vec<u8>),
}

/// Function metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarFunction {
    /// 函数名称。
    pub name: String,
    /// 形参个数。
    pub arity: i32,
    /// 局部变量槽位数。
    pub local_count: i32,
    /// 函数体在 code section 中的字节偏移。
    pub code_offset: i32,
    /// 函数体字节长度。
    pub code_length: i32,
}

/// Import entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarImportKind {
    /// 导入函数符号。
    Function = 0,
}

/// Import table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarImport {
    /// 导入项种类。
    pub kind: NyarImportKind,
    /// 来源模块名。
    pub module_name: String,
    /// 导入符号名。
    pub symbol_name: String,
}

/// Export entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarExportKind {
    /// 导出函数。
    Function = 0,
    /// 导出全局槽。
    Global = 1,
}

/// Export table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarExport {
    /// 导出项种类。
    pub kind: NyarExportKind,
    /// 对外可见符号名。
    pub symbol_name: String,
    /// Function index when `kind == Function`; global index when `kind == Global`.
    pub function_index: i32,
}

/// Module-level global slot (singleton instance storage, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarGlobal {
    /// Slot name, e.g. `Counter.INSTANCE`.
    pub name: String,
    /// Aggregate type name used for allocation.
    pub type_name: String,
}

/// Witness dispatch entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarWitnessDispatchEntry {
    /// 方法语义标识。
    pub method_id: i32,
    /// 具体类型标识。
    pub type_id: i32,
    /// 方法名称。
    pub method_name: String,
    /// 实现函数在 functions 表中的索引。
    pub function_index: i32,
    /// 接口语义标识。
    pub interface_id: i32,
    /// 方法在接口 vtable 中的槽位索引。
    pub interface_method_index: i32,
}

/// Section kind in a `.legion` module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarSectionKind {
    /// 常量池段。
    Constants = 0x01,
    /// 函数元数据段。
    Functions = 0x02,
    /// 指令字节码段。
    Code = 0x03,
    /// 导入表段。
    Imports = 0x04,
    /// 导出表段。
    Exports = 0x05,
    /// witness 分派表段。
    WitnessEntries = 0x06,
    /// 模块全局槽段。
    Globals = 0x07,
    /// 模块初始化函数索引段。
    InitFunctions = 0x08,
    /// 稠密布局描述表；`ObjectNew` 的 layout_id 索引此段。
    Layouts = 0x09,
}

/// 语言中立的布局描述：仅槽位数，不含字段名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NyarLayout {
    /// 字段槽数量（`FieldGet` / `FieldSet` 的 field_slot 上界）。
    pub field_count: i32,
}

/// Decoded Nyar module binary view.
#[derive(Debug, Clone, PartialEq)]
pub struct NyarModuleData {
    /// 模块格式版本号。
    pub version: u32,
    /// 模块名称。
    pub name: String,
    /// 常量池条目。
    pub constants: Vec<NyarConstant>,
    /// 函数元数据表。
    pub functions: Vec<NyarFunction>,
    /// 导入表。
    pub imports: Vec<NyarImport>,
    /// 导出表。
    pub exports: Vec<NyarExport>,
    /// witness 分派条目。
    pub witness_entries: Vec<NyarWitnessDispatchEntry>,
    /// 指令字节码。
    pub code_bytes: Vec<u8>,
    /// Module-level global slots.
    pub globals: Vec<NyarGlobal>,
    /// Function indices to run for eager singleton / module initialization.
    pub init_function_indices: Vec<i32>,
    /// Layout descriptors；`layout_id` 为稠密下标。
    pub layouts: Vec<NyarLayout>,
}
