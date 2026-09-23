# Rusty TypeScript

Rusty TypeScript 展示了如何在 Nyar VM 中处理带有复杂静态类型系统的动态语言。

## 核心特性

- **结构化类型**: 接口（Interfaces）与类型别名的静态检查。
- **泛型**: 支持泛型函数与类的定义与类型推导。
- **模块化**: 完整的 ES Modules (import/export) 支持。
- **类型安全性**: 编译时进行深度类型分析，运行时进行类型擦除。

## 编译与 Lowering 流程

Rusty TypeScript 的编译侧重于类型检查后的代码简化：

1. **解析**: 使用 `oak-typescript` 生成 AST。
2. **类型检查**: 在 `type_system` 模块中进行符号分析与类型推导。
3. **Lowering 转换**:
   - **类型擦除**: 移除所有类型注解，仅保留运行时逻辑。
   - **模块转换**: 将 `import`/`export` 转换为 `ImportInfo` 和 `ExportInfo`。
4. **字节码生成**:
   - 使用 `NyarTranslator` 将简化后的 UIR 转换为 Nyar 操作码。
   - 匿名函数映射为 `Lambda` 指令。

## 对象模型处理

- **接口与类**: 接口在运行时被擦除，类被映射为带有构造函数的原型结构。
- **原型链**: 模拟 JavaScript 的原型继承模型。
- **属性访问**: 静态确定的属性访问被优化为索引访问，动态属性使用名称访问。

## Builtin 与 FFI 实现

- **宿主环境**: 
   - 模拟浏览器或 Node.js 的全局对象（如 `console`, `process`）。
   - 通过 `ImportInfo` 链接到宿主提供的原生符号。
- **FFI 实现**: 
   - 使用 `declare` 关键字定义外部符号。
   - 运行时通过 Nyar VM 的模块加载器自动解析并绑定到外部函数。
- **类型擦除验证**: 确保 TypeScript 的静态类型不影响生成的字节码执行效率，仅作为开发期的辅助。
