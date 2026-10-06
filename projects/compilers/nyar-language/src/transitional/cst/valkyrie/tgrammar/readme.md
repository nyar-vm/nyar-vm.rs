# T-Grammar / Meta Level（Valkyrie 语言扩展）

T-Grammar 提供 `<% %>` 元计算语句，用于 `t"` 模板字符串、X-Grammar 混写与宏 AST 展开。

- **`<% kw %>`**：以块级关键词开头（`if` / `else if` / `else` / `end` / `loop` / `match` / `case`）时为控制流指令。
- **`<% ... %>`**（非关键词）：视为 Valkyrie 语句，由主编译器解析执行。
- 块级控制流以 `<% end %>` 闭合；`end` 后可跟可选标签（如 `<% end match %>`、`<% end if %>`），与裸 `<% end %>` 等价。
- 插值 `{expr}`；模板内注释 `<# ... #>`。
