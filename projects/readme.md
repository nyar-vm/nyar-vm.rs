# Projects 布局

本目录按职责拆分 Rust crate 与宿主绑定，**仅描述本仓库公开结构**。

## 目录

| 路径 | 职责 |
| --- | --- |
| `compilers/` | Nyar 编译器栈：`nyar-language`、`nyar-emitter`、`nyar-analyzer` 等 |
| `hosts/` | Node-API / Wasm 宿主绑定（`nyar-napi`、`nyar-wasm`） |
| `packages/` | npm 平台 collect（`nyar-win32-x64` 等） |
| `nvm/` | Nyar VM 字节码解释器 CLI（`nyar-vm`） |

## 依赖边界（摘要）

- **文本与二进制格式**：`std-data`（自 [valkyrie.rs](https://github.com/valkyrie-language/valkyrie.rs) 的 `vcc-data` crate，依赖键仍为 `std-data`）。
- **编译主链**：`nyar-language` → `nyar-emitter` → `std-data`；`nyar-language` 不得反向依赖 emitter 的 lowering 实现细节。
- **Host script 前端**（bash / lua / tcl / powershell / c）：解析在 `std-data`，语义与 `interpret` 在 `nyar-language::src/<lang>/`。

## 可选集成测试

部分测试可通过环境变量指向外部 Valkyrie 语言源码树（**非发布必需**）：

```bash
export VALKYRIE_V_ROOT=/path/to/valkyrie-language-checkout
cargo test -p nyar-language
```

未设置时，相关用例自动跳过，不影响默认 CI。
