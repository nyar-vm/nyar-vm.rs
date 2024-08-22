# 语言前端示例

本目录记录面向 **Nyar VM 后端** 的语言前端示例。Nyar 项目可以有多个前端和多个并列后端；这些示例选择 Nyar VM 时，才需要把语言语义降低到该后端的输入 contract，再进入 `nyar-ir`。语言自己的语法、类型、控制流和对象模型仍由前端拥有；JVM、CLR、Wasm、Native 等后端不因这些示例而共享 `nyar-ir`。

## Nyar VM 后端边界

Nyar VM 的核心设计目标是“语义降级而非模拟”。各前端先闭合自己的语言语义，再按 Nyar VM 后端 contract 生成 `nyar-ir` 所需事实。这里的 `nyar-ir` 是 VM 的后端 IR，不是 Nyar 项目所有语言或后端的共同语义层：

1. **按需控制流分析**：需要 CFG、SSA、状态机或 continuation 的 frontend 接入相应模块，不强迫其他语言采用相同表示。
2. **显式对象模型接入**：运行时可以支持 **名义对象**、**动态对象**和其他对象协议，但每种语言的对象语义由其 frontend contract 定义。
3. **VM 运行时集成**：进入 Nyar VM 后端的前端按 VM contract 接入 [Nyar GC](../maintenance/nyar-gc.md) 等运行时组件；其他后端使用自己的执行环境。

## 对象模型处理机制

为了处理选入 Nyar VM 后端的语言对象模型，Nyar VM 采用了以下机制：

- **静态对象 (ValueTag::Object)**：用于 Java、C# 等静态语言，基于偏移量访问字段。
- **动态对象 (ValueTag::DynObject)**：用于 Python、Lua 等动态语言，基于哈希表管理属性。
- **特征对象 (ValueTag::TraitObject)**：用于 Rust、Go 等语言，通过“胖指针”和见证表实现动态分发。

这种“多模型运行时”设计使得 Java 调用的 Python 函数可以透明地操作 `DynObject`，反之亦然。

## 独特特性验证 (Hardcore Feature Validation)

除了跨语言互操作，Nyar VM 还需要证明其指令集能够原生且高效地支持各种语言中最“硬核”的独特机制：

1. **作用域清理与栈展开 (Zig `defer` / `errdefer`)**：
    - **挑战**：验证 VM 是否能在 `return` 或异常抛出时，自动、逆序执行已注册的清理块。
    - **目标**：证明 Nyar VM 对 Scope Guard 和自动资源管理的底层支持。
2. **异常截断与恢复 (Go `panic` / `recover`)**：
    - **挑战**：在 `defer` 块中捕获并中止当前的异常传播。
    - **目标**：验证 VM 的 `ExceptionHandler` 栈是否足够灵活，支持动态改变控制流。
3. **一级延续 (Scheme `call/cc`)**：
    - **挑战**：捕获当前执行上下文并将其作为可重复调用的对象。
    - **目标**：压测 Nyar VM `Continuation` 类型的抽象极限，这是支持所有异步/并发模型的基础。
4. **动态拦截 (Lua `metatable`)**：
    - **挑战**：在 `GetElement` 失败时回退到元方法拦截逻辑。
    - **目标**：验证 VM 的动态分发（Fallback）机制。
5. **结构化匹配 (Rust `match` + `enum`)**：
    - **挑战**：将代数数据类型（ADT）的分支跳转优化为 O (1) 的 `Switch` 操作。
    - **目标**：验证 VM 对复杂决策树的执行效率。
6. **自托管运行 (Zig `comptime`)**：
    - **挑战**：在编译期启动 VM 实例执行逻辑并回填结果。
    - **目标**：验证 VM 的轻量化、可重入性以及自托管能力。

## 示例列表

目前 Nyar VM 正在积极适配多种主流语言的前端实现，展示如何利用 `ProjectChomsky` 编译器框架进行映射。

### 核心演示语言

- [Rusty C](rusty-c.md): 经典的过程序编程语言。
- [Rusty C#](rusty-csharp.md): 现代的面向对象语言。
- [Rusty Java](rusty-java.md): 企业级面向对象语言。
- [Rusty Kotlin](rusty-kotlin.md): 简洁的现代多范式语言。
- [Rusty Lua](rusty-lua.md): 嵌入式动态脚本语言。
- [Rusty Python](rusty-python.md): 广泛使用的动态语言。
- [Rusty TypeScript](rusty-typescript.md): 带类型的 JavaScript 超集。

### 实验性/开发中语言

- [Rusty Rust](rusty-rust.md): 现代系统编程语言。
- [Rusty Go](rusty-go.md): 云原生并发语言。
- [Rusty Ruby](rusty-ruby.md): 灵动的面向对象语言。
- [Rusty Scheme](rusty-scheme.md): 一级延续与函数式范式。
- [Rusty Swift](rusty-swift.md): 安全高效的系统语言。
- [Rusty PHP](rusty-php.md): Web 开发脚本语言。
- [Rusty Zig](rusty-zig.md): 现代 C 替代者。
- [其他语言](others.md): 包含 Elixir, Dart, Mojo, Julia, Cobol, Tcl, Prolog, Nim, Nix 等。

## 职责说明

每个前端项目应当遵循 Nyar frontend SPI 及所选择模块的接口规范，主要包含：

1. **Parser**: 将源代码解析为 typed AST。必须使用 `oak-core` 框架。
2. **Lowering**: 将 AST 转换为该语言自己的语义 IR，或转换为明确声明输入 contract 的 Nyar 模块。
3. **Nyar module integration**: 按语言和 target 选择需要的分析、优化、表示或执行模块；不要求所有 frontend 共享同一 IR。

## 接入新语言指南

为了保持架构的一致性并减少重复代码，接入新语言时请遵循以下步骤：

### 1. 项目结构规范

建议的前端项目结构如下：

```text
rusty-lang/
├── bin/                # 命令行工具入口 (如 langc, lang-run)
├── src/
│   ├── codegen/        # 转换为 IKunTree 的逻辑
│   ├── lib.rs          # 实现 NyarFrontend trait
│   └── main.rs         # 使用 NyarDriver 的默认运行工具
├── tests/              # 所有的测试必须放在根目录的 tests/ 下
└── Cargo.toml
```

### 2. 实现 NyarFrontend Trait

在 `lib.rs` 中定义前端结构体并实现 `NyarFrontend` trait：

```rust
use nyar_error::NyarError;
use nyar_frontend::NyarFrontend;
use nyar_types::IKunTree;
use oak_mylang::{MyLanguage, MyRoot};

pub struct MyLanguageFrontend;

impl NyarFrontend for MyLanguageFrontend {
    type Language = MyLanguage;

    fn parse(&self, source: &str) -> Result<MyRoot, NyarError> {
        // 使用 oak-core 构建 AST
    }

    fn lower(&self, ast: &MyRoot) -> Result<IKunTree, NyarError> {
        // 将 AST 转换为 IKunTree
    }
}
```

### 3. 使用 NyarDriver

在 `main.rs` 或 `bin/` 工具中，使用 `NyarDriver` 来驱动执行：

```rust
fn main() {
    let frontend = MyLanguageFrontend::new();
    let driver = NyarDriver::new();
    let input_file = Path::new("test.mylang");
    
    // 直接运行源代码
    driver.run_source(&frontend, input_file).unwrap();
}
```

### 4. 避免过度拷贝

- **严禁跨语言拷贝代码时不修改类型名**：确保 AST 根节点类型与语言对应（如 `JavaRoot`, `RustRoot`）。
- **统一错误处理**：所有的错误必须统一为 `NyarError`。
- **减少 dyn 使用**：`NyarDriver` 使用静态泛型，避免在热点路径使用 trait objects。
