# legend

多语言统一运行时 CLI（demo）。证明 host-script **guest** 可经 `legacy-vm`（PE 基板）插拔执行，**不是**各语言完整实现的宿主。

## 职责（只做这些）

- 读源文件 / eval 片段、解析 `--language` 与短 flag
- 调用 `LegacyVmRunner` 执行或列出支持语言
- 维护 `fixtures` + `fixtures/manifest.yaml` 与 sidecar 期望
- 目标别名展示（`legacy-vm` / `native` / `nyar-vm` / …）

## 两条路径（不要混）

| CLI | 走什么 | 说明 |
| --- | --- | --- |
| `run` / `eval` | **interpret**（`LegacyVmRunner::run`） | 日常 demo / fixture；`--target` 在 run 上被忽略 |
| `build --target native`（`pe` / `exe`） | **specialize → native residual → PE** | host-script PE 产品路径 |
| `build --target nyar-vm` | **拒绝** host-script | PE 终点不是 nyar-vm；`.nyar` 至多是其它探针 |

心智模型与分层全文：[`../host-script-languages.md`](../host-script-languages.md)、[`../../runtimes/legend-vm`](../../runtimes/legend-vm/readme.md)。

## 禁止

- 不在 `legend/src` 实现 lexer / parser / AST / interpret
- 不为某语言加「特殊求值分支」；语言逻辑落在 `std-data` → `nyar-language`（guest）→ `legacy-vm`（薄适配）
- 解释型 host script fixture 的 `targets` 使用 `legacy-vm`，不要标成 `nyar-vm`
- 不要把 host-script PE 产品说成「编到 nyar-vm」

## 测试

```text
cargo test -p legend --test host_script_fixtures
```

再生 sidecar：`LEGEND_TEST_REGENERATE=1` 或 `NYAR_TEST_REGENERATE=1`。
