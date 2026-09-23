# Valkyrie 语言文档项目

该项目是 **Valkyrie 编程语言** 的官方文档库，基于 [VitePress](https://vitepress.dev/) 构建。

## 项目简介

Valkyrie 是一门现代的函数式编程语言，专注于提供强大的**代数效应（Algebraic Effects）**系统和**强类型安全**。本项目包含了 Valkyrie 语言的所有官方学习资源、参考指南和示例代码。

## 文档结构

- **[入门指南](./guide/)**: 快速上手 Valkyrie，了解环境配置和基本语法。
- **[语言特性](./language/)**: 深入了解代数效应、模式匹配、元编程和模块系统等核心功能。
- **[实战示例](./examples/)**: 涵盖从 Web 开发到深度学习的多种应用场景示例。
- **[常见问题](./faq.md)**: 汇总开发过程中可能遇到的各类问题。

## 开发与预览

本项目使用 VitePress 驱动，支持实时预览和静态站点生成。

### 安装依赖

在 `projects/valkyrie-document` 目录下，使用 pnpm 安装：

```bash
pnpm install
```

### 启动开发服务器

```bash
pnpm dev
```

启动后可在浏览器访问 `http://localhost:5173` 进行实时预览。

### 构建静态站点

```bash
pnpm build
```

构建产物将保存在 `.vitepress/dist` 目录中。

## 参与贡献

我们欢迎并感谢任何形式的贡献，包括修复错别字、完善文档内容或翻译。

1. Fork 本仓库
2. 创建特性分支
3. 提交更改
4. 发起 Pull Request

---

**Valkyrie Team** - 致力于构建下一代函数式编程语言。
