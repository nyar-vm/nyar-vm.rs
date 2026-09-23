# Host Script 语言分层约定

本文约束 **legend / nyar 多语言 demo** 的分工与落点。目标是证明优化与运行时路径可语言无关，不是追求 ISO C / 完整 shell。

权威范例以仓库现状为准：**bash / lua / powershell / c / tcl** 均已是「语言包 `interpret` + `legacy-vm` 薄适配」。子集表随实现更新，写在各层 `readme.md`，本文不锁定语法细节。

## 架构目标（已拍板）

`legacy-vm` 类比 **GraalVM / Truffle**：PE / Futamura 风格的解释器基板，从解释推导编译。核心是 frames / nodes / specialization / PE — **语言无关**。

| 组件 | 目标角色 |
| --- | --- |
| `legacy-vm` | 语言无关 PE 基板 + `guest` 注册缝 + 注册 / 检测 / 薄 guest 适配；**specialize → native residual → PE** |
| `nyar-language::src/<lang>/` | **具体** guest 语言（`*Module` / `*SemanticBridge` / **interpret** / specialize） |
| Guest ↔ VM | 薄、语言中立的 runtime ABI（`guest::GuestInterpretFn` / `GuestInterpret` + value bridge） |

**禁止**：在 `nyar-language` 再抽共享 `HostScript*` / `host_script` trait 层 — guest 必须保持具体类型。

**现状**：内置 host-script guests（bash / lua / powershell / c / tcl）的 interpret 均在 `nyar-language`；VM `evaluator/<lang>.rs` 只做薄适配。护栏见 `legacy-vm/tests/architecture_guards.rs`（禁止 evaluator 再拥有全量 AST walk）。

## 设计心智模型

`legacy-vm` **不是**「又一个语言运行时」：它不懂各语言语法，只提供 **挂 guest + 解释调度 + 特化出 native**。

```text
                    ┌─ bash / lua / ps / tcl / c / …
                    │  （各自 AST + interpret）
                    ▼
              ┌─────────────┐
 legend ────► │ legacy-vm   │  guest 缝 + 值桥 + 调度
              │  PE 基板    │
              └──────┬──────┘
                     │
          ┌──────────┼──────────┐
          ▼                     ▼
     run = interpret      specialize = PE（第 1 投影）
     （日常 fixture）      → NativeResidual → x64 / Windows PE
```

### 怎么解释「奇形怪状」的语言

靠 **插拔，不是统一 AST**。每一门语言自己的形态关在语言包里：

```text
源码 → std-data（该语言 AST）→ nyar-language interpret（语义真相）
     → legacy-vm 薄适配（LegacyValue ↔ 本地值）→ guest 缝注册 → legend 调 run
```

基板只看见：给定语言 id，调 `GuestInterpretFn`。能塞进各种怪语言，是因为各写各的 **parse + interpret** 并注册薄适配，**不必**改 PE 核心，也 **禁止** 在 VM 里再写全量 tree-walk。

与 Valkyrie 主链对比：Valkyrie 把语言收成统一 IR 再手工多后端；legacy-vm 允许 guest **语义形态各异**，统一的是 **注册 / 调度 ABI + 特化出口**，不是语法。

### 怎么「编译」

**Futamura 第 1 投影**：对「解释器 × 某段程序」做特化 → **native-bound residual** → PE。

```text
interpret（特化对象）──specialize──► NativeResidualModule ──emit──► .exe
                                         └── 不是 nyar-vm / `.nyar`（产品路径）
```

- `legend run` / fixture：仍走 interpret（`targets: [legacy-vm]`）。
- `legend build --target native`（`pe` / `exe`）：specialize 编译路径。
- `.nyar` / `StackResidualSink` / `nyar-vm`：至多探针，**禁止**当作 host-script PE 终点。
- 进 `.nyar` **不等于**吃到 Valkyrie→emitter 的 CLR/JVM/WASM/… 全平台链。

**现状边界**：Lua → native residual 已接线；完整逐 op x64 codegen 仍薄；其它 host-script specialize 可能仍 stub。细节见 [`legacy-vm/readme.md`](legacy-vm/readme.md)。

## 分层职责

| 层 | Crate / 位置 | 做什么 | 禁止做什么 |
| --- | --- | --- | --- |
| 1. 文本语法 | `std-data`（`src/text/<lang>/`） | lexer / parser / AST / 最小文本模型 | 语义分析、interpret、runner、CLI、fixture 装配 |
| 2. 语言包（guest） | `nyar-language`（`src/<lang>/` 各语言自洽） | 具体 `*Module` / `*SemanticBridge`、**interpret（主实现）**、语言侧导出 | 共享 `host_script` trait 层；解析器副本；legend CLI；把逻辑塞进 `valkyrie` IR 主链 |
| 3. PE 基板 + 薄适配 | `legacy-vm` | PE 核心；`guest` 缝；`LegacyVmRunner` 注册 / 检测；`evaluator/<lang>.rs` **薄适配**；**specialize → native residual → PE**；`lang_*` / architecture guards | 永久把具体语言语义烤进 PE 核心；在 legend 写解释器；跳过 `std-data` 手搓 AST；在 VM 内新增全量 interpret；把 host-script PE 接到 `nyar-vm` |
| 4. 产品 CLI | `legend` | 读配置、fixture、`--language` / 短 flag、调用 runner | **任何** 语言解析或求值逻辑 |

```
源码 ──► std-data (parse/AST)
              │
              ▼
        nyar-language guest (concrete bridge + interpret 主实现)
              │
              ▼
        legacy-vm (PE substrate + guest seam + register/detect + 薄 adapter)
              │
              ▼
        legend (CLI / fixtures / flags)
```

### 必须落点 / 禁止落点

- **解析器只进 `std-data`**。不要在 `legend`、`legacy-vm`、`nyar-language` 再写一份 lexer/parser。
- **interpret 主实现在 `nyar-language::…`**。`legacy-vm` 的 `evaluator/<lang>.rs` 应适配/委托，而不是成为第二套语言语义真相源。
- **`legacy-vm` 核心（`algebra` / `compiler` / `value` / `guest`）保持语言无关**。具体语言品牌不得渗入 PE 基板。
- **`legend` 只装配**：读文件、解析 flag、调 `LegacyVmRunner`、维护 `fixtures/`。禁止在 `legend/src` 增加语言分支求值。

## 范围哲学（demo subset）

- 子集只要能跑通 fixture / `lang_*` smoke，证明 **legacy-vm 路径对多语言可插拔**。
- 不实现完整标准、完整 shell、完整动态语言运行时。
- 动态语言分析前端（如 `javascript` / `python` 在 `nyar-language`）≠ legend 的 `nyar-vm` 编译目标；解释路径走 `legacy-vm`。
- **Host-script PE / Futamura 特化终点是 `native`（residual → x64 / PE），不是 `nyar-vm`。** PE residual ≠ nyar-vm bytecode product；`.nyar` 至多是探针。
- 子集清单只写**已支持**的语法，放在各层 `readme.md`；未落地则写「骨架 / TBD」，不要虚构完整表。

## 今日 interpret 落点（对照目标）

| 语言 | `nyar-language` interpret | `legacy-vm` evaluator | 状态 |
| --- | --- | --- | --- |
| **bash** | 有（主实现） | 薄适配 → language | **目标形态** |
| **lua** | 有（主实现） | 薄适配 → language | **目标形态** |
| **powershell** | 有（主实现） | 薄适配 → language | **目标形态** |
| **c** | 有（主实现） | 薄适配 → language | **目标形态** |
| **tcl** | 有（主实现） | 薄适配 → language | **目标形态** |

`javascript` / `python` 在 runner 侧仍可能是 stub（分析前端 ≠ legend 解释 guest），不计入上表。

## 如何新增一门语言（checklist）

对照任一已落地 guest（如 **bash**），把 `<lang>` 换成目标语言名：

| 层 | 范例路径 | 新语言应对齐 |
| --- | --- | --- |
| `std-data` | `src/text/bash/…` | `src/text/<lang>/…` |
| `nyar-language` | `src/bash/{mod,module,semantic_bridge,interpret}.rs` | `src/<lang>/…`（可执行时**必须**有 `interpret`） |
| `legacy-vm` | `evaluator/bash.rs`（薄适配）+ `tests/lang_bash.rs` + runner / `guest` 注册 | **仅**薄适配 + `tests/lang_<lang>.rs`；**禁止**在 VM 内新增全量 interpret（architecture guards 会拦） |
| `legend` | `fixtures/bash/…` + manifest | `fixtures/<lang>/…`；`targets: [legacy-vm]` |

### 1. `std-data`

- [ ] `src/text/<lang>/`：lexer / parser / AST（或最小脚本模型）
- [ ] 包级 `readme.md` 写清当前子集（只写已支持的语法）
- [ ] 解析测试落在 `std-data`

### 2. `nyar-language`

- [ ] `src/<lang>/`：`module` + `semantic_bridge`（**具体** inherent API：`language_id` / `source_path` / `exported_symbols`；不要引入跨语言 trait）
- [ ] 需要可执行时：`interpret`（语言包主实现）
- [ ] `src/<lang>/readme.md`：职责 + 当前子集/状态
- [ ] 在 `lib.rs` / 模块树中挂载（与现有 `bash` / `c` / `lua` 并列，**不要**塞进 `valkyrie/`）

### 3. `legacy-vm`

- [ ] `evaluator/<lang>.rs`：**薄适配**（求值委托 `nyar-language` interpret；经 `guest` 缝注册）
- [ ] `runner.rs`：`register_defaults` 注册规范名与别名
- [ ] 检测：扩展名 / shebang / 必要时 content sniff
- [ ] `tests/lang_<lang>.rs`：至少 1 个 smoke + 1 个扩展名检测
- [ ] 将新适配器加入 `architecture_guards` 的 `THIN_GUEST_ADAPTERS`（必须委托 `evaluate_<lang>_source`，且不得 `std_data::text::`）

### 4. `legend`

- [ ] `fixtures/<lang>/…` + sidecar yaml
- [ ] `fixtures/manifest.yaml`：`language` + `targets: [legacy-vm]`（解释型 host script **不要**标 `nyar-vm`；编译特化走 `native`，不是 `nyar-vm`）
- [ ] 如需：`language_short_flag` 增加短 flag（仅展示/别名）
- [ ] 确认 `tests/host_script_fixtures.rs` 能覆盖该语言（`INTERPRETED_LANGUAGES` 与 manifest 一致）

### Host-script 语言额外核对

- [ ] 与 bash 同构：`std-data` 文本模型 → language 包 bridge + interpret → legacy-vm **薄**注册
- [ ] 子集文档写在各层 `readme.md`，本文不维护逐语言语法表
- [ ] 并行改 bash/ps/tcl 时：**只改本层文件**，避免跨语言大挪移

## 语言检测与短 flag

优先级（实现见 `legacy-vm` `LegacyVmRunner` / `legend`）：

1. **显式**：`legend` `--language` / 短 flag
2. **Shebang**：`detect_language_from_shebang`
3. **扩展名**：`.c` / `.lua` / `.sh` / `.ps1` / `.tcl` 等 → `detect_language_from_path`
4. **内容 sniff**：`detect_language_from_content`（eval 无路径时）；无法判断时默认 `bash`（现状）

Legend fixture 以 **manifest 的 `language` 字段** 为准，不依赖 sniff。

短 flag（仅 CLI 展示/别名，见 `legend` `language_short_flag`）：

| 语言 | 短 flag |
| --- | --- |
| `c` | `--c` |
| `lua` | `--lua` |
| `bash` | `--sh` |
| `powershell` | `--ps1` |
| `tcl` | `--tcl` |

## 测试期望

```text
cargo test -p legacy-vm --test architecture_guards
cargo test -p legacy-vm --test golden_futamura
cargo test -p legacy-vm --test lang_<lang>
cargo test -p nyar-language --test architecture_guards
cargo test -p legend --test host_script_fixtures
```

- `architecture_guards`：VM 核心 / `guest` 语言无关；禁止 HostScript 回潮；evaluator 必须薄委托 `nyar-language`
- `golden_futamura`：Lua 第 1 投影 = specialize → **native residual → PE**（不是 `NyarVm::run`）
- `lang_*`：解释器 smoke + 检测行为
- legend fixtures：manifest → `legacy-vm` → sidecar；再生 sidecar 可用 `LEGEND_TEST_REGENERATE=1` / `NYAR_TEST_REGENERATE=1`
- 解析正确性优先在 `std-data`；不要只在 legend 用字符串断言代替 AST 测试

## 稳定范例 vs 子集表

| 语言 | 文档策略 | 子集表状态 |
| --- | --- | --- |
| **bash** | 薄适配范例 | 已写：`std-data` / `nyar-language` readme；VM 委托 language interpret |
| **lua** | 同构；相对 shell **稍丰富** 的 mid-subset（控制流 + tables） | language 包有 `interpret`；VM 薄适配；子集见各层 `readme.md` |
| **powershell** | 同构 | 已写：`vcc-data`、`nyar-language/src/powershell/readme.md`；VM 薄适配 |
| **C** | 同构 | language 包有 `interpret`；VM 薄适配 |
| **tcl** | 同构 | language 包有 `interpret`；VM 薄适配 |

## 相关入口

- `legacy-vm/readme.md` — PE 基板与 `guest` 缝职责
- `vcc-data` — 格式树边界
- `legend/readme.md` — CLI / fixture 职责摘要
- `nyar-language/tests/architecture_guards.rs` — 禁止共享 `host_script` 模块
- `legacy-vm/tests/architecture_guards.rs` — 基板语言无关 + 薄适配强制
