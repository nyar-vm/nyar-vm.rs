# Notedown

Valkyrie 文档 IR（对齐 C# `Std.Data.Text.Notedown` / pandoc AST）。

- **解析**：`NotedownDocument::parse(source)`
- **格式化**：`notedown::formatter::format(document)`
- 块级词法 + 行内解析；支持 `$...$` / `$$...$$` 数学、GFM 表格、脚注、div fence 等。

HTML 渲染在 `nyar_language::text::notedown`。
