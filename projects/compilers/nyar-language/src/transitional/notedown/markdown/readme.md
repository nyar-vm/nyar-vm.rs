# Markdown

Markdown 前端：预处理 GFM 扩展后委托 Notedown 解析器，并提供 Notedown IR 双向转换（对齐 C# `Std.Data.Text.Markdown`）。

- `parse(source)` — Markdown → Notedown IR
- `to_notedown` / `from_notedown` — IR 桥接
- HTML 渲染见 `nyar_language::text::notedown`
