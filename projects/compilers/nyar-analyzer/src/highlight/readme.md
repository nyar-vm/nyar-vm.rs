# highlight

前端无关的语法高亮契约层（对齐 JetBrains IDE platform + C# `Nyar.Analyzer.Highlight`）。

## 层级

| 层 | 位置 | 职责 |
|:---|:---|:---|
| Platform | `nyar-analyzer::highlight` | `HighlightKind` / `HighlightSpan` / `Highlighter` trait / `HighlighterKind` / `Registry` / `hl-*` HTML |
| Language plugin | `nyar-language::{lang}::highlight` | 同一语言多个 pass：`Lexical`（词法）+ `Semantic`（语义） |

## 多 pass

同一 `language_id` 可注册多种高亮器：

- **Lexical**：仅需源文本，同步、便宜（文档 SSG / 输入过程）
- **Semantic**：依赖 `AnalysisContext`，可把标识符升级为类型 / 函数等（IDE / LSP）

`HighlighterRegistry::highlight_merged` 先词法后语义 overlay。

具体语言**不得**在 analyzer 内实现；analyzer 也不依赖 `std-data` 词法器。
