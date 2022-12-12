# format

前端无关的源码格式化 / printer 契约层（对齐 `highlight` 分层）。

## 层级

| 层 | 位置 | 职责 |
|:---|:---|:---|
| Platform | `nyar-analyzer::format` | `FormatOptions` / `FormatError` / **`Document` 布局引擎** / **`syntax` CST** / `SourceMap` / `SourceFormatter` / `Printer` / Registry |
| Language plugin | `nyar-language` | `FormatSyntax`（CST → `Document`）+ `ToDocument`（模型 printer）+ 各语言 `SourceFormatter` |

## 两类写出（必须分离）

| 契约 | 输入 | 输出 | 用途 |
|:---|:---|:---|:---|
| **SourceFormatter** | 源码文本 | `FormattedOutput`（文本 + `SourceMap`） | **正规格式化**；保留 trivia；`legion fmt` / LSP |
| **Printer** | 已解析**数据模型** | 文本 | 序列化 / 调试；**不保证**注释与空白保留 |

## 正规格式化管线

```text
source → lossless lexer → CST → FormatSyntax::format_document → Document::render_with_map
```

## Document 引擎

Wadler 风格文档代数：`text` / `trivia` / `append` / `nest` / `line` / `softline` / `hardline` / `group` / `fill`。

语言侧实现 `FormatSyntax::format_document(&self, options)`（CST）与 `ToDocument::to_document`（模型 printer，orphan 规则下 trait 在 `nyar-language`）。

## 遗留

`FormatBuffer` 已弃用，仅供 V/Awsl 过渡；迁移完成后移除。
