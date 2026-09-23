# Valkyrie LSP

基于 Nyar 编译器基础设施的 Valkyrie 语言服务器协议实现。

## 概述

Valkyrie LSP 是 Valkyrie 编程语言的语言服务器实现，提供了完整的 IDE 支持功能。它基于 Nyar 编译器基础设施构建，利用统一的查询体系为开发者提供优秀的编程体验。

### 设计理念

**重要说明**：Nyar 作为编译器本身不提供 LSP 服务，而是为宿主语言提供统一的 LSP 查询体系。这种设计带来了以下优势：

- **模块化架构**：编译器核心与 LSP 服务完全分离
- **基础设施复用**：多个宿主语言可以共享相同的编译器基础设施
- **定制化扩展**：每个宿主语言可以实现自己特定的 LSP 功能
- **统一查询接口**：所有语言服务都基于相同的查询引擎

## 功能特性

### 核心 LSP 功能

- ✅ **实时诊断**：语法错误、类型错误、语义警告
- ✅ **智能补全**：上下文感知的代码补全建议
- ✅ **符号导航**：定义跳转、引用查找、符号搜索
- ✅ **悬停信息**：类型信息、文档显示
- ✅ **代码操作**：快速修复、重构建议
- ✅ **语义高亮**：基于语义分析的语法高亮
- ✅ **文档符号**：大纲视图、符号树
- ✅ **代码格式化**：自动格式化、范围格式化
- ✅ **重命名**：智能重命名符号

### Valkyrie 特定扩展

- 🔧 **AST 查看**：`valkyrie/getAst` - 获取抽象语法树
- 🔧 **HIR 查看**：`valkyrie/getHir` - 获取高级中间表示
- 🔧 **符号查询**：`valkyrie/querySymbol` - 深度符号信息查询

## 安装与使用

### 作为独立服务器

```bash
# 构建项目
cargo build --release

# 标准输入输出模式（推荐用于 IDE 集成）
./target/release/valkyrie-lsp --stdio

# TCP 模式（用于调试和测试）
./target/release/valkyrie-lsp --tcp 9257
```

### IDE 集成

#### VS Code

在 VS Code 设置中添加：

```json
{
  "valkyrie.lsp.serverPath": "/path/to/valkyrie-lsp",
  "valkyrie.lsp.serverArgs": ["--stdio"]
}
```

#### Neovim (使用 nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')

lspconfig.valkyrie_lsp = {
  default_config = {
    cmd = { '/path/to/valkyrie-lsp', '--stdio' },
    filetypes = { 'valkyrie' },
    root_dir = lspconfig.util.root_pattern('.git', 'Cargo.toml'),
    settings = {},
  },
}

lspconfig.valkyrie_lsp.setup{}
```

#### Emacs (使用 lsp-mode)

```elisp
(use-package lsp-mode
  :config
  (add-to-list 'lsp-language-id-configuration '(valkyrie-mode . "valkyrie"))
  (lsp-register-client
   (make-lsp-client :new-connection (lsp-stdio-connection '("/path/to/valkyrie-lsp" "--stdio"))
                    :major-modes '(valkyrie-mode)
                    :server-id 'valkyrie-lsp)))
```

### 作为库使用

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
valkyrie-lsp = { path = "../valkyrie-lsp" }
tokio = { version = "1.0", features = ["full"] }
oak-lsp = { path = "../../../oaks/projects/oak-lsp" }
```

使用示例：

```rust
use oak_lsp::LspServer;
use valkyrie_lsp::ValkyrieBackend;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = Arc::new(ValkyrieBackend::new());
    let server = LspServer::new(backend);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    server.run(stdin, stdout).await?;
    Ok(())
}
```

## 架构设计

### 整体架构

```text
┌─────────────────────────────────────────────────────────────┐
│                    IDE Client                               │
│  (VS Code, IntelliJ, Vim, Emacs, etc.)                    │
└─────────────────────┬───────────────────────────────────────┘
                      │ LSP Protocol (JSON-RPC)
┌─────────────────────▼───────────────────────────────────────┐
│                 Valkyrie LSP Server                        │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐   │
│  │   Backend   │ │  Handlers   │ │    Diagnostics      │   │
│  │             │ │             │ │                     │   │
│  │ - Protocol  │ │ - Symbols   │ │ - Error Conversion  │   │
│  │ - Routing   │ │ - Actions   │ │ - Filtering         │   │
│  │ - State     │ │ - Format    │ │ - Publishing        │   │
│  └─────────────┘ └─────────────┘ └─────────────────────┘   │
└─────────────────────┬───────────────────────────────────────┘
                      │ Nyar Compiler API
┌─────────────────────▼───────────────────────────────────────┐
│              Nyar Compiler Infrastructure                   │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐   │
│  │  AST/HIR    │ │ Query Engine│ │   Error System      │   │
│  │             │ │             │ │                     │   │
│  │ - Parsing   │ │ - Symbols   │ │ - Diagnostics       │   │
│  │ - Analysis  │ │ - References│ │ - Source Maps       │   │
│  │ - Types     │ │ - Completion│ │ - Miette Integration│   │
│  └─────────────┘ └─────────────┘ └─────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 模块组织

- **`backend.rs`** - LSP 协议实现和请求路由
- **`state.rs`** - 服务器状态管理和文档缓存
- **`handlers.rs`** - 各种 LSP 请求的具体处理逻辑
- **`diagnostics.rs`** - 诊断信息转换和管理
- **`capabilities.rs`** - 服务器能力声明

## 开发指南

### 构建要求

- Rust 1.70+
- Cargo
- 依赖的 Nyar 编译器组件

### 开发环境设置

```bash
# 克隆项目
git clone <repository-url>
cd nyar-vm/projects/valkyrie-lsp

# 安装依赖
cargo build

# 运行测试
cargo test

# 启动开发服务器（TCP 模式便于调试）
cargo run -- --tcp 9257
```

### 调试技巧

1. **使用 TCP 模式**：便于使用调试工具连接
2. **启用日志**：设置 `RUST_LOG=debug` 环境变量
3. **LSP 客户端调试**：大多数编辑器都有 LSP 日志功能

### 添加新功能

1. 在 `capabilities.rs` 中声明新能力
2. 在 `backend.rs` 中添加路由
3. 在 `handlers.rs` 中实现处理逻辑
4. 更新测试和文档

## 配置选项

### 服务器配置

```rust
use valkyrie_lsp::{ServerConfig, ServerMode, DiagnosticFilterConfig};

let config = ServerConfig {
    mode: ServerMode::Stdio,
    port: None,
    log_level: tracing::Level::INFO,
    diagnostic_filter: DiagnosticFilterConfig {
        enabled_severities: vec![
            DiagnosticSeverity::ERROR,
            DiagnosticSeverity::WARNING,
        ],
        ignored_codes: vec!["unused_variable".to_string()],
        max_diagnostics_per_file: Some(100),
    },
    enable_custom_methods: true,
};
```

### 环境变量

- `RUST_LOG` - 日志级别控制
- `VALKYRIE_LSP_PORT` - 默认 TCP 端口
- `VALKYRIE_LSP_MAX_DIAGNOSTICS` - 每文件最大诊断数量

## 性能优化

### 缓存策略

- **文档缓存**：编译结果和 AST/HIR 缓存
- **符号缓存**：符号表和引用关系缓存
- **增量更新**：只重新分析变更的文档

### 内存管理

- 使用 `DashMap` 进行并发安全的缓存
- 定期清理未使用的文档状态
- 限制缓存大小防止内存泄漏

## 故障排除

### 常见问题

1. **LSP 服务器无法启动**
   - 检查可执行文件路径
   - 验证依赖项是否正确安装
   - 查看错误日志

2. **补全功能不工作**
   - 确认文档已正确解析
   - 检查符号表是否构建成功
   - 验证客户端是否支持补全功能

3. **诊断信息不显示**
   - 检查编译是否成功
   - 验证诊断过滤配置
   - 确认客户端订阅了诊断事件

### 日志分析

启用详细日志：

```bash
RUST_LOG=valkyrie_lsp=debug,nyar_core=info ./valkyrie-lsp --stdio
```

## 贡献指南

1. Fork 项目
2. 创建功能分支
3. 编写测试
4. 提交 Pull Request

### 代码规范

- 遵循 Rust 官方代码风格
- 添加适当的文档注释
- 编写单元测试和集成测试
- 使用 `cargo fmt` 和 `cargo clippy`

## 许可证

MIT License - 详见 LICENSE 文件

## 相关项目

- [Nyar Compiler](../nyar-core/) - 核心编译器基础设施
- [Nyar Query](../nyar-query/) - 统一查询引擎
- [Nyar Error](../nyar-error/) - 错误处理系统