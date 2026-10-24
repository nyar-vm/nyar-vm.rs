# legion tests

这里放 `legion` 的集成测试与真实运行冒烟测试。

## 职责

- 覆盖前端源码到构建产物再到实际运行的完整链路。
- 持有真实语言 fixture，例如 `tests/fixtures` 下的 `control_flow`、`oop` 与 `text/valkyrie` 文本快照样例。
- 作为 `legion build` 与 `legion run` 的上层回归护栏。
- 复用 `nyar::testing` 提供的通用 fixture 基线能力，只在这里保留 `legion` 自己的执行与环境变量包装。

## 文本快照 fixture

Valkyrie lexer / parse / highlight 文本 sidecar 由 `legion/tests/fixtures/text/valkyrie/` 统一持有；`std-data` 与
`nyar-language` 的 runner 只读该目录。详见 [`tests/fixtures/text/valkyrie/readme.md`](fixtures/text/valkyrie/readme.md)。

## 基线更新

- `runtime_smoke` fixture 首次运行缺少 `.yaml` 时会自动生成。
- `cmds_run` 的多平台 `legion run --target` 回归已改为 fixture 驱动，先校验结构化 `run_contract`，再执行目标 runtime
  并记录程序本身的 `exit_code`、`stdout`、`stderr`，基线记录在 `tests/fixtures/cmds_run/*.valkyrie.yaml`。
- 强制重生成支持 `VALKYRIE_TEST_REGENERATE=1`、`LEGION_TEST_REGENERATE=1`，兼容 `NYAR_TEST_REGENERATE=1`。

## 运行时前置条件与跳过条件

部分集成测试会在构建成功后尝试真实执行产物；缺少对应运行时工具链时，测试会跳过运行时断言而不是失败。

| 目标                      | 所需命令        | 跳过条件                                                                                             |
|---------------------------|-----------------|------------------------------------------------------------------------------------------------------|
| CLR (`.exe` / `.dll`)     | `dotnet`        | `cmds_build` 中 CLR witness / suspend 测试在 `dotnet` 不在 `PATH` 时跳过执行断言                     |
| JVM (`.jar`)              | `java`          | `builds_jvm_witness_dispatch`、`builds_jvm_suspend_trait_combo` 在 `java` 不在 `PATH` 时跳过执行断言 |
| Node (`.mjs` / `.wasm`)   | `node`          | `runtime_smoke` / `cmds_run` fixture 在缺少 `node` 时整组跳过                                        |
| WASI (`.wasi`)            | `wasmtime`      | witness / `runtime_smoke` / `cmds_run` 在缺少 `wasmtime` 时跳过或 panic（见具体测试）                |
| Native Windows (`.exe`)   | 无（直接执行）  | 仅在 Windows 上运行；`builds_native_msvc_*` 直接 `Command::new(exe)`                                 |
| Native Linux (`main` ELF) | 无（直接 exec） | Unix 直接执行；Windows 需要 WSL（`wsl` + `wslpath`），不可用时 panic                                 |

`runtime_smoke` 与 `cmds_run` fixture 会通过 `can_run_all_required_commands` 检查 fixture 声明目标所需的全部命令；任一缺失则打印
`skip runtime fixture: missing <label>` 并跳过该 fixture 组。

`run-contracts.txt` 可能列出多个 `physical_entry`；测试辅助代码会按契约顺序查找第一个已存在的产物，而不是只使用第一条契约或任意扩展名匹配。

Node suspend 走 WASM glue（`emitter` 的 `wasm.rs`），无独立 Node suspend lowering；`runtime_smoke` 的 node target 覆盖该路径。
