//! 语言无关的 PE 残化接收端（Futamura 第一投影目标）。
//!
//! guest 语言的 specialize 路径把解释器 match 分支残化为本契约的操作流；
//! 宿主（`legend-vm`）实现产品级接收端（`NativeResidualSink` → x64 / Windows PE）。
//! guest 前端不依赖 VM crate；栈式 `.nyar` 接收端仅可作为临时探针存在，
//! 不是宿主脚本的 PE 目标。

/// 把解释器针对某程序特化后产生的操作推入的目标接收端。
pub trait ResidualSink {
    /// 推入整数常量。
    fn push_i64(&mut self, value: i64);
    /// 推入布尔常量。
    fn push_bool(&mut self, value: bool);
    /// 推入字符串常量。
    fn push_string(&mut self, value: &str);
    /// 推入 `nil` / null。
    fn push_nil(&mut self);
    /// 将命名局部变量加载到栈上。
    fn load(&mut self, name: &str);
    /// 将栈顶存入命名局部变量。
    fn store(&mut self, name: &str);
    /// 弹出栈顶。
    fn pop(&mut self);
    /// 二元 `+`（数值；字符串拼接请用 [`Self::concat`]）。
    fn add(&mut self);
    /// 二元 `-`。
    fn sub(&mut self);
    /// 二元 `*`。
    fn mul(&mut self);
    /// 二元 `/`（两侧均为整数时截断）。
    fn div(&mut self);
    /// 二元 `%`。
    fn rem(&mut self);
    /// 二元 `==`。
    fn eq(&mut self);
    /// 二元 `~=` / `!=`。
    fn ne(&mut self);
    /// 二元 `<`。
    fn lt(&mut self);
    /// 二元 `<=`。
    fn le(&mut self);
    /// 二元 `>`。
    fn gt(&mut self);
    /// 二元 `>=`。
    fn ge(&mut self);
    /// 字符串拼接（`..`）。
    fn concat(&mut self);
    /// 逻辑非（消耗一个值，压入 bool）。
    fn not(&mut self);
    /// `print`，`argc` 个实参已在栈上（从左到右，栈顶为最后一个）。
    fn print(&mut self, argc: usize);
    /// 在当前代码位置绑定标签。
    fn label(&mut self, name: &str);
    /// 无条件跳转到标签。
    fn jump(&mut self, label: &str);
    /// 栈顶为假值时跳转（Lua 仅 `nil`/`false` 为假）；弹出条件。
    fn jump_if_false(&mut self, label: &str);
    /// 栈顶为真值时跳转；弹出条件。
    fn jump_if_true(&mut self, label: &str);
    /// 开始一个残化函数。
    fn begin_function(&mut self, name: &str);
    /// 声明形参（必须紧跟 [`Self::begin_function`]）。
    fn add_parameter(&mut self, name: &str);
    /// 结束当前函数。
    fn end_function(&mut self);
    /// 以栈上的 `argc` 个实参调用残化函数。
    fn call(&mut self, name: &str, argc: usize);
    /// 返回栈顶（栈空时接收端可推入 nil）。
    fn ret(&mut self);
    /// 报告不支持的构造（特化中止）。
    fn unsupported(&mut self, what: &str) -> Result<(), String> {
        Err(format!("PE residualization unsupported: {what}"))
    }
}
