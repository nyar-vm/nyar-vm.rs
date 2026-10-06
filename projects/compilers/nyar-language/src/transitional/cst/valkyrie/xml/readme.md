# X-Grammar / XML（Valkyrie 语言扩展）

X-Grammar 是 **Valkyrie 语言的内联标记扩展**，不是独立方言。在 `.v` / `.vx` 源码中可直接书写 XML 标签与 `{expr}` 插值（默认
TSX 子集）。

- `.vx`：约定使用 X-Grammar 的 Valkyrie 源文件扩展名（类 TSX）， **没有** AWSL 式的 `<template>` / `<script>` 块结构。
- AWSL（`.awsl`）：独立语言，块式 SFC 由 `std-data::text::awsl` + `voa` 负责。

首轮支持：元素、静态文本、花括号插值。属性：静态 `"..."`、动态 `:name="expr"`（KV 上禁止 `{}`）。`<if>` / `<loop>` / `$name`
等后续扩展。
