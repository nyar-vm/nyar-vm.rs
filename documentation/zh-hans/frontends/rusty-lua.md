# Rusty Lua

这是一个类 Lua 语言的编译器前端实现，旨在验证 Nyar VM 对动态语言语法的支持。

## 核心特性

- **动态类型**: 运行时处理数值、字符串、布尔和 `nil`。
- **表 (Table)**: Lua 的灵魂，支持关联数组与数组模式的混合存储。
- **闭包**: 完整的 lexical scoping 支持，支持上值（Upvalues）捕获。
- **元表系统 (Metatables)**: 利用 Nyar VM 的动态拦截（Fallback）机制实现 `__index`, `__newindex` 等逻辑。
- **闭包捕获**: 原生支持 Upvalue，映射到 Nyar VM 的词法作用域。
- **轻量级协程**: 映射到 Nyar VM 的 `Continuation` 或轻量级线程。
- **环境隔离**: 每个模块拥有独立的 `_ENV`，映射到 VM 的模块化命名空间。

## 实现进度

- [x] 基础语法解析
- [ ] Table 与 Metatable 语义 (动态拦截测试)
- [ ] Upvalue 捕获支持
- [ ] 协程（Coroutine）库

## 编译与 Lowering 流程

Rusty Lua 采用双层 IR 转换策略以实现深度优化：

1. **解析**: 使用 `oak-lua` 将源码解析为 UIR。
2. **Gaia IR 生成 (Lowering)**:
   - 使用 `GaiaTranslator` 将 `IKunTree` 转换为 `Gaia` 指令。
   - 局部变量被映射到 `GaiaFunction` 的寄存器槽位。
   - 字符串常量被提取到 `GaiaModule` 的常量池中。
3. **块与跳转**:
   - 将 Lua 的控制流映射为 `GaiaBlock`。
   - 通过 `GaiaTerminator` 实现循环与分支跳转。

## 对象模型处理

- **表实现**: 在 Nyar VM 中，Lua Table 映射为高度优化的哈希表。
- **索引操作**: `a.b` 或 `a[b]` 映射为 `GetField` 或 `InvokeMethod`（当涉及元方法时）。
- **环境 (Env)**: 全局变量存储在特殊的 `_G` 表中，通过词法环境层层查找。

## Builtin 与 FFI 实现

- **标准库**: 提供 `print`, `type`, `pairs`, `ipairs` 等核心函数的内建实现。
- **C API 模拟**: 
   - 允许通过 Nyar VM 的栈操作接口与 Rust/C 函数交互。
   - 支持动态加载 `.so`/`.dll` 并将其中的函数注册到 Lua 环境。
- **闭包实现**: 
   - 使用 `ManagedInstruction` 处理闭包创建。
   - VM 自动管理捕获变量的生命周期。
