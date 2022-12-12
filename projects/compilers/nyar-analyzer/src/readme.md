# nyar-analyzer

`nyar-analyzer` 提供面向 `nyar` 平台的前端无关分析契约。

## 职责
- 承接下游前端已经闭合好的程序事实。
- 表达入口、导入导出、运行时需求、能力标签等中性分析结果。
- 为 `nyar` 的分区、lane 选择和 backend 规划提供统一输入。
- 提供语法高亮平台层（`highlight`：`HighlightKind` / `Highlighter` trait），类似 JetBrains platform。

## 禁止
- 不依赖具体前端包。
- 不定义语言专属 `AST / HIR / MIR / LIR`。
- 不实现具体语言的词法高亮器（由 `nyar-language` 等 language plugin 实现）。
- 不承担目标容器编码和产物打包职责。
