# Project 模式

## 概述

Project 模式是 Valkyrie 语言的单项目管理模式，通过 `legion.json` 文件定义项目配置。当目录中存在 `legion.json` 文件且没有 `legions.json` 文件时，该目录被识别为独立项目模式。

## 文件结构

```
project-root/
├── legion.json           # 项目配置文件
├── library/
│   └── _.vk             # 建议：命名空间根目录定义（也支持 _.valkyrie）
├── binary/
│   ├── main.vk          # 注册为指令 `main`
│   ├── tool1.vk         # 注册为指令 `tool1`
│   └── tool2/
│       └── _.vk         # 注册为指令 `tool2`
├── tests/               # 测试文件
├── docs/                # 文档目录
├── examples/            # 示例代码
└── target/              # 构建输出
```

## legion.json 配置

`legion.json` 是 JSON5 格式的配置文件，支持注释和更灵活的语法：

### 基础配置

```json5
{
    // 项目基本信息
    "name": "my-valkyrie-project",
    "version": "1.0.0",
    "description": "A Valkyrie language project",
    "type": "application", // 或 "library", "plugin", "tool"
    
    // 作者信息
    "authors": [
        "Developer Name <email↯example.com>"
    ],
    
    // 许可证
    "license": "MIT",
    
    // 仓库信息
    "repository": "https://github.com/user/my-valkyrie-project",
    "homepage": "https://my-project.dev",
    "documentation": "https://docs.my-project.dev",
    
    // 发布配置
    "publish": true,
    "private": false,
    
    // 语言版本
    "edition": "2024",
    
    // 入口文件
    "main": "binary/main.vk",
    "lib": "library/_.vk"
}
```

### 依赖管理

```json5
{
    // 运行时依赖
    "dependencies": {
        "valkyrie-std": "^1.0.0",
        "serde": {
            "version": "^1.0",
            "features": ["derive"]
        },
        "local-lib": {
            "path": "../local-lib"
        },
        "git-dep": {
            "git": "https://github.com/user/repo.git",
            "branch": "main"
        }
    },
    
    // 构建时依赖
    "build-dependencies": {
        "build-script": "^0.1.0"
    },
    
    // 开发依赖
    "dev-dependencies": {
        "test-framework": "^2.0.0",
        "benchmark": "^1.5.0"
    },
    
    // 可选依赖
    "optional-dependencies": {
        "feature-x": "^1.0.0",
        "feature-y": "^2.0.0"
    }
}
```

### 功能特性

```json5
{
    // 功能特性定义
    "features": {
        "default": ["std", "serde"],
        "std": [],
        "serde": ["dep:serde"],
        "async": ["dep:tokio"],
        "full": ["std", "serde", "async"]
    }
}
```

### 构建配置

```json5
{
    // 构建设置
    "build": {
        "target": "native", // 或 "wasm", "js", "llvm"
        "optimization": "release", // 或 "debug", "size", "speed"
        "output": "target/",
        "incremental": true,
        "parallel": true,
        "cache": true
    },
    
    // 编译器选项
    "compiler": {
        "warnings": "deny",
        "errors": "abort",
        "debug-info": true,
        "strip-symbols": false
    },
    
    // 链接器选项
    "linker": {
        "lto": true,
        "strip": false,
        "static": false
    }
}
```

### 脚本命令

```json5
{
    // 自定义脚本
    "scripts": {
        "build": "valkyrie build --release",
        "test": "valkyrie test",
        "run": "valkyrie run",
        "clean": "valkyrie clean",
        "fmt": "valkyrie fmt",
        "lint": "valkyrie lint",
        "doc": "valkyrie doc --open",
        "publish": "valkyrie publish",
        "install": "valkyrie install",
        "dev": "valkyrie run --watch",
        "benchmark": "valkyrie bench"
    }
}
```

## 项目类型

### 1. 应用程序 (Application)

```json5
{
    "type": "application",
    "main": "binary/main.vk",
    "build": {
        "target": "native",
        "executable": "my-app"
    }
}
```

### 2. 库 (Library)

```json5
{
    "type": "library",
    "lib": "library/_.vk",
    "build": {
        "target": "library",
        "crate-type": ["lib", "dylib"]
    }
}
```

### 3. 插件 (Plugin)

```json5
{
    "type": "plugin",
    "plugin": {
        "interface": "valkyrie-plugin-api",
        "version": "1.0.0"
    }
}
```

### 4. 工具 (Tool)

```json5
{
    "type": "tool",
    "bin": [
        {
            "name": "my-tool",
            "path": "src/bin/tool.vk"
        }
    ]
}
```

## 语义特性

### 1. 依赖解析

- **版本约束**：支持语义化版本约束
- **路径依赖**：支持本地路径依赖
- **Git 依赖**：支持 Git 仓库依赖
- **条件依赖**：基于特性的条件依赖

### 2. 特性系统

- **默认特性**：自动启用的特性集合
- **可选特性**：按需启用的功能特性
- **特性组合**：特性之间的依赖关系
- **特性传播**：依赖项特性的传播机制

### 3. 构建系统

- **增量编译**：只编译变更的部分
- **并行构建**：多核并行编译
- **缓存机制**：构建结果缓存
- **交叉编译**：支持多目标平台编译

### 4. 包管理

- **版本管理**：自动版本号管理
- **发布流程**：标准化的发布流程
- **依赖锁定**：确保构建的可重现性
- **安全检查**：依赖安全性检查

## 开发工作流

### 1. 项目初始化

```bash
# 创建新项目
valkyrie new my-project
cd my-project

# 或者初始化现有目录
valkyrie init
```

### 2. 依赖管理

```bash
# 添加依赖
valkyrie add serde↯^1.0

# 添加开发依赖
valkyrie add --dev test-framework

# 更新依赖
valkyrie update

# 移除依赖
valkyrie remove old-dep
```

### 3. 构建和测试

```bash
# 构建项目
valkyrie build

# 运行项目
valkyrie run

# 运行测试
valkyrie test

# 生成文档
valkyrie doc

# 运行二进制程序
v main                     # 运行 binary/main.vk
v tool1                    # 运行 binary/tool1.vk
v tool2                    # 运行 binary/tool2/_.vk
```

### 4. 发布流程

```bash
# 检查项目
valkyrie check

# 运行完整测试
valkyrie test --all-features

# 发布到仓库
valkyrie publish
```

## 配置示例

### Web 应用项目

```json5
{
    "name": "web-app",
    "version": "1.0.0",
    "type": "application",
    "main": "src/main.vk",
    
    "dependencies": {
        "valkyrie-web": "^2.0.0",
        "valkyrie-router": "^1.5.0",
        "valkyrie-templates": "^1.0.0"
    },
    
    "features": {
        "default": ["server"],
        "server": ["dep:valkyrie-web"],
        "client": ["dep:valkyrie-wasm"]
    },
    
    "build": {
        "target": "wasm",
        "optimization": "size"
    },
    
    "scripts": {
        "dev": "valkyrie run --watch --features server",
        "build-client": "valkyrie build --target wasm --features client",
        "serve": "valkyrie run --release"
    }
}
```

### 库项目

```json5
{
    "name": "data-structures",
    "version": "2.1.0",
    "type": "library",
    "lib": "src/lib.vk",
    "description": "High-performance data structures for Valkyrie",
    
    "authors": ["Library Team <team↯example.com>"],
    "license": "Apache-2.0",
    "repository": "https://github.com/team/data-structures",
    
    "dependencies": {
        "valkyrie-std": "^1.0.0"
    },
    
    "dev-dependencies": {
        "criterion": "^0.5.0",
        "proptest": "^1.0.0"
    },
    
    "features": {
        "default": ["std"],
        "std": [],
        "no-std": [],
        "serde": ["dep:serde"],
        "parallel": ["dep:rayon"]
    },
    
    "build": {
        "optimization": "speed",
        "lto": true
    }
}
```

## 最佳实践

### 1. 项目结构

- **清晰分层**：合理组织源代码结构
- **模块化设计**：使用模块系统组织代码
- **测试覆盖**：为所有公共 API 编写测试
- **文档完整**：提供完整的 API 文档

### 2. 依赖管理

- **最小依赖**：只添加必要的依赖
- **版本锁定**：使用精确的版本约束
- **定期更新**：定期更新依赖版本
- **安全审计**：定期进行安全审计

### 3. 构建优化

- **增量构建**：利用增量编译特性
- **并行构建**：启用并行编译
- **缓存利用**：充分利用构建缓存
- **目标优化**：针对目标平台优化

### 4. 版本管理

- **语义化版本**：遵循语义化版本规范
- **变更日志**：维护详细的变更日志
- **向后兼容**：保持 API 的向后兼容性
- **弃用策略**：合理的 API 弃用策略

## 工具集成

### IDE 支持

- **项目识别**：IDE 自动识别项目配置
- **依赖管理**：图形化的依赖管理界面
- **构建集成**：集成的构建和运行功能
- **调试支持**：完整的调试功能支持

### CI/CD 集成

- **自动构建**：基于配置的自动构建
- **测试执行**：自动化测试执行
- **质量检查**：代码质量和安全检查
- **自动发布**：基于标签的自动发布

## 迁移和兼容性

### 配置迁移

- **版本升级**：配置文件版本升级
- **格式转换**：从其他格式转换
- **向后兼容**：保持向后兼容性
- **迁移工具**：提供自动迁移工具

### 生态系统兼容

- **标准遵循**：遵循社区标准
- **工具链集成**：与现有工具链集成
- **平台支持**：多平台支持
- **互操作性**：与其他语言的互操作