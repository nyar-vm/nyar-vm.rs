# `legion spy wasm`

`legion spy wasm` 提供 `WASM` / WASI component 二进制的反汇编与分析能力。

## 支持的能力

- 解析任意 `WASM` 二进制与 `.wasi` component（自动识别 component version）。
- Component：列出段、嵌套 core exports，并尝试通过 `wasm-tools component wit` 打印 WIT。
- Component：`--func` 反汇编第一个嵌套 core module 中的函数。
- 列出所有段。
- 列出所有函数。
- 反汇编指定函数。
- 查看 `imports` / `exports`。
- 按绝对偏移定位错误上下文。
- 结构化解析 `Type` 段（`--types`），列出每个 type 条目的索引 / 种类 / 内容，
  并标记 `arraytype(0x61)` 等 `Node.js v24 V8` 不支持的条目。
- 以 `JSON` 形式输出。
- dump 函数体原始字节。
- `--glue-audit`：审计 Node JS-glue 契约导入/导出。
