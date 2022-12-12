# 语言前端示例

Nyar VM 旨在建立一个真正的**统一编程语言运行时**。虽然不同编程语言在语法、控制流和对象模型上存在巨大差异，但 Nyar 通过多层抽象将这些差异在运行时进行了语义对齐。

## 统一运行时哲学

Nyar VM 的核心设计目标是“语义降级而非模拟”。我们并不是在一种语言中模拟另一种语言，而是将所有语言的意图降级到同一套高度抽象且灵活的原始操作上：

1.  **控制流统一**：通过 SSA 变换和 CFG 规范化，将复杂的控制流（如异常、协程、生成器）降级为统一的跳转和状态机。
2.  **对象模型对齐**：运行时原生支持**名义对象**（Java/C#）、**动态对象**（Python/JS）和**特征对象**（Rust/Go），确保不同范式的语言能以最优性能运行。
3.  **内存管理集成**：所有前端共享同一套高性能的 [Nyar GC](../maintenance/nyar-gc.md)，实现跨语言的对象透明传递。

## 对象模型处理机制

为了处理 C、Python、Java、Rust 等语言迥异的对象模型，Nyar VM 采用了以下机制：

- **静态对象 (ValueTag::Object)**：用于 Java、C# 等静态语言，基于偏移量访问字段。
- **动态对象 (ValueTag::DynObject)**：用于 Python、Lua 等动态语言，基于哈希表管理属性。
- **特征对象 (ValueTag::TraitObject)**：用于 Rust、Go 等语言，通过“胖指针”和见证表实现动态分发。

这种“多模型运行时”设计使得 Java 调用的 Python 函数可以透明地操作 `DynObject`，反之亦然。

## 独特特性验证 (Hardcore Feature Validation)

除了跨语言互操作，Nyar VM 还需要证明其指令集能够原生且高效地支持各种语言中最“硬核”的独特机制：

1.  **作用域清理与栈展开 (Zig `defer` / `errdefer`)**：
    - **挑战**：验证 VM 是否能在 `return` 或异常抛出时，自动、逆序执行已注册的清理块。
    - **目标**：证明 Nyar VM 对 Scope Guard 和自动资源管理的底层支持。
2.  **异常截断与恢复 (Go `panic` / `recover`)**：
    - **挑战**：在 `defer` 块中捕获并中止当前的异常传播。
    - **目标**：验证 VM 的 `ExceptionHandler` 栈是否足够灵活，支持动态改变控制流。
3.  **一级延续 (Scheme `call/cc`)**：
    - **挑战**：捕获当前执行上下文并将其作为可重复调用的对象。
    - **目标**：压测 Nyar VM `Continuation` 类型的抽象极限，这是支持所有异步/并发模型的基础。
4.  **动态拦截 (Lua `metatable`)**：
    - **挑战**：在 `GetElement` 失败时回退到元方法拦截逻辑。
    - **目标**：验证 VM 的动态分发（Fallback）机制。
5.  **结构化匹配 (Rust `match` + `enum`)**：
    - **挑战**：将代数数据类型（ADT）的分支跳转优化为 O(1) 的 `Switch` 操作。
    - **目标**：验证 VM 对复杂决策树的执行效率。
6.  **自托管运行 (Zig `comptime`)**：
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

每个前端项目应当遵循统一的接口规范，主要包含：

1. **Parser**: 将源代码解析为 typed AST。必须使用 `oak-core` 框架。
2. **Lowering**: 将 AST 转换为通用中间表示 `IKunTree`。
3. **NyarDriver Integration**: 所有的前端应当接入 `nyar-vm` 提供的 `NyarDriver`，以实现统一的 JIT 运行与 AOT 编译体验。

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
