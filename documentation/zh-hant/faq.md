# 常见问题 (FAQ)

本页面收集了 Valkyrie 开发过程中的常见问题和解答。

## 语言基础

### Q: Valkyrie 与其他函数式语言有什么区别？

A: Valkyrie 的独特之处在于：
- **代数效应系统**：原生支持代数效应，优雅处理副作用
- **多目标编译**：可编译到 WebAssembly、JavaScript 和原生代码
- **现代语法**：结合函数式特性与现代语法设计
- **渐进式采用**：可与现有 JavaScript/TypeScript 项目无缝集成
- **强类型推导**：先进的类型系统，减少显式类型注解

### Q: 什么是代数效应？为什么重要？

A: 代数效应是一种处理副作用的抽象机制：
- **统一抽象**：将异常、异步、状态管理等副作用统一处理
- **可组合性**：效应可以自由组合和嵌套
- **控制反转**：调用者决定如何处理效应，而不是被调用者
- **类型安全**：效应在类型系统中得到体现

```valkyrie
effect State<T> {
    get(): T
    set(value: T): Unit
}

micro counter() -> Int {
    let current = raise State::get()
    raise State::set(current + 1)
    current + 1
}
```

### Q: Valkyrie 支持哪些数据类型？

A: Valkyrie 支持丰富的类型系统：
- **基础类型**：Int, Float, String, Bool, Unit
- **容器类型**：List&lt;T&gt;, Array&lt;T&gt;, Map&lt;K,V&gt;, Set&lt;T&gt;
- **可选类型**：Option&lt;T&gt; (Some(value) | None)
- **结果类型**：Result&lt;T,E&gt; (Fine(value) | Fail(error))
- **函数类型**：(A, B) -> C
- **代数数据类型**：自定义的 sum 和 product 类型
- **效应类型**：带有效应标注的函数类型

## 语法和特性

### Q: 如何定义和使用代数数据类型？

A: 使用 `type` 关键字定义：

```valkyrie
# Sum 类型（联合类型）
union Result<T, E> {
    Fine { value: T },
    Fail { error: E }
}

# Product 类型（记录类型）
type User = {
    id: Int,
    name: String,
    email: Option<String>
}

# 模式匹配
match result {
    case Fine { value }: print("成功: ${value}")
    case Fail { error }: print("错误: ${error}")
}
```

### Q: 如何處理異步操作？

A: Valkyrie 支持原生的 `async/await` 語法：

```valkyrie
micro fetch_user_data(id: i32) -> User {
    let response = fetch("/api/users/{id}").await;
    parse_json(response)
}

// 頂層也可以使用 await
let user = fetch_user_data(42).await;
print("User: {user.name}");
```

### Q: 如何進行錯誤處理？

A: Valkyrie 提供多種錯誤處理方式：

```valkyrie
# 1. 使用 Result 類型
micro divide(a: Float, b: Float) -> Result<Float, String> {
    if b == 0.0 {
        Fail { error: "除零錯誤" }
    } else {
        Fine { value: a / b }
    }
}

# 2. 使用異常效應
effect Exception {
    throw(message: String): Never
}

micro safe_divide(a: Float, b: Float) -> Float {
    if b == 0.0 {
        Exception::throw { message: "除零錯誤" }
    } else {
        a / b
    }
}
```

## 编译和部署

### Q: Valkyrie 如何编译到不同目标？

A: Valkyrie 支持多目标编译：

```bash
# 编译到 WebAssembly
valkyrie build --target wasm

# 编译到 JavaScript
valkyrie build --target js

# 编译到原生代码
valkyrie build --target native

# 编译到 TypeScript 定义
valkyrie build --target ts-defs
```

### Q: 如何与现有 JavaScript 项目集成？

A: Valkyrie 提供无缝集成：

```valkyrie
# 导入 JavaScript 模块
using "↯std/fetch" { fetch }
using "↯std/console" { console }

# 导出给 JavaScript 使用
export micro greet(name: String) -> String {
    "Hello, ${name}!"
}

# 使用 JavaScript 对象
micro process_data(data: JSObject) -> JSObject {
    # 处理逻辑
    data
}
```

### Q: 性能如何？有哪些优化？

A: Valkyrie 提供多种性能优化：
- **尾调用优化**：自动优化尾递归
- **内联优化**：小函数自动内联
- **死代码消除**：移除未使用的代码
- **效应优化**：编译时优化效应处理
- **内存管理**：智能的垃圾回收和内存复用

## 工具和生态

### Q: 有哪些开发工具支持？

A: Valkyrie 提供完整的工具链：
- **编译器**：`valkyrie` CLI 工具
- **包管理器**：内置的依赖管理
- **格式化工具**：`valkyrie fmt` 代码格式化
- **语言服务器**：VS Code、Vim 等编辑器支持
- **调试器**：源码级调试支持
- **测试框架**：内置单元测试和集成测试

### Q: 如何編寫和運行測試？

A: 使用內置測試框架：

```valkyrie
# 單元測試
@test
micro test_addition() {
    Assert::eq(add(2, 3), 5)
    Assert::eq(add(-1, 1), 0)
}

# 屬性測試
@test
micro test_addition_commutative() {
    Assert::forall(forall a: Int, b: Int {
        add(a, b) == add(b, a)
    })
}

# 效應測試
@test
micro test_state_effect() {
    let result = try {
        counter()
    } .catch {
        case State::get {}:
            resume(0)
        case State::set { value }:
            resume(())
    }
    Assert::eq(result, 1)
}
```

### Q: 如何管理项目依赖？

A: 使用 `valkyrie.toml` 配置文件：

```toml
[package]
name = "my-project"
version = "0.1.0"
authors = ["Your Name <your.email@example.com>"]

[dependencies]
std = "1.0"
http = "0.3"
json = "0.2"

[dev-dependencies]
test-utils = "0.1"

[build]
targets = ["js", "wasm"]
optimization = "release"
```

## 学习和社区

### Q: 如何学习 Valkyrie？

A: 推荐的学习路径：
1. **基础语法**：从函数式编程概念开始
2. **类型系统**：理解代数数据类型和模式匹配
3. **代数效应**：掌握效应的定义和处理
4. **实践项目**：构建小型应用程序
5. **高级特性**：学习性能优化和工具使用

### Q: 有哪些学习资源？

A: 可用的学习资源：
- **官方教程**：[快速开始指南](/guide/getting-started)
- **示例项目**：[代码示例](/examples/)
- **API 文档**：完整的标准库文档
- **社区论坛**：GitHub Discussions
- **视频教程**：YouTube 频道

### Q: 如何贡献到 Valkyrie 项目？

A: 贡献方式：
1. **报告问题**：提交 bug 报告和功能请求
2. **改进文档**：完善文档和示例
3. **编写代码**：实现新功能或修复问题
4. **测试反馈**：使用预发布版本并提供反馈
5. **社区支持**：帮助其他用户解决问题

### Q: Valkyrie 的发展路线图是什么？

A: 主要发展方向：
- **语言特性**：模块系统、宏系统、并发原语
- **工具改进**：更好的错误信息、调试体验、IDE 支持
- **性能优化**：编译速度、运行时性能、内存使用
- **生态建设**：标准库扩展、第三方包、框架支持
- **平台支持**：更多编译目标、移动平台、嵌入式系统

---

如果您的问题没有在这里找到答案，请：
- 查看 [官方文档](/guide/)
- 提交 [GitHub Issue](https://github.com/valkyrie-lang/valkyrie/issues)
- 参与 [社区讨论](https://github.com/valkyrie-lang/valkyrie/discussions)
- 加入 [Discord 社区](https://discord.gg/valkyrie-lang)