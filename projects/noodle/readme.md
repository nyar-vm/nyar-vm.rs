# noodle

`noodle` 面向 Node.js。形态**借鉴** Vite+（统一入口 + 自带 fmt / lint / check），**不是**逐命令对齐 Vite+ / npm / pnpm。

## Stack

- CLI：**clap**
- fmt / lint / check：`nyar-language::javascript`（分析框架插件；不 shell 到 Biome / Prettier / ESLint）
- 包管理：`nyar-package-manager::PackageManager::open_with_manifest_layout`（产品层翻译 `package.json`；布局由 `noodle::project_layout()`）
- build / run / test / exec：读 `package.json` `scripts.*`，经 `ScriptRunner` 执行（用户脚本可以调任意命令；工具链本身不套 npm/pnpm/vite）
- 兼容行为：读 `package.json` 参数（`noodle.compat` / `packageManager`），**不按 lockfile 猜工具**

禁止把 Node / npm / pnpm 概念写进 `nyar-package-manager`。禁止 `Command::new("pnpm")` / `npm` 做 install/fmt。

**Guard：** `src/architecture_guards.rs`（`cargo test -p noodle --lib`）扫描 `src`，防止外国工具链 `Command::new` 回潮。

## Commands

| Command | Behavior |
| --- | --- |
| `noodle create <name>` | 脚手架：`package.json` + `src/` + `.gitignore`；`--compat npm\|pnpm\|…` |
| `noodle install` | 安装 dependencies + devDependencies；`--prod` 跳过 dev；按 compat 同步 `node_modules` |
| `noodle add` / `remove` / `update` | PM 安装/移除/更新；`package.json` 由 noodle 落盘；同步 `node_modules` |
| `noodle fmt` / `lint` / `check` | 内置 JS/TS/JSON 格式化与 lint |
| `noodle build` / `test` / `run` / `exec` | `scripts.*` 或任意命令字符串 |

## Install layout

PM 仍写入 `vendors/{registry}/{name}@{version}`。Noodle 产品层按 **compat** 物化 `node_modules`：

| Compat | Layout |
| --- | --- |
| `npm` / `yarn` / `bun` (default) | **Flat** — `node_modules/{name}` → vendors |
| `pnpm` | **A-lite pnpm-like** — `node_modules/.pnpm/{id}/node_modules/{name}` + top-level links |

`packageManager: "pnpm@…"` 或 `noodle.compat: "pnpm"` 选择 pnpm-like。**自研 adapter ≠ pnpm CLI**（不调用 pnpm，不做完整依赖隔离 / content-addressable store）。

```
my-app/                          # npm-compat
  vendors/npm/…
  node_modules/left-pad → …

my-app/                          # pnpm-compat
  vendors/npm/…
  node_modules/
    .pnpm/left-pad@1.3.0/node_modules/left-pad → vendors/…
    left-pad → .pnpm/left-pad@1.3.0/node_modules/left-pad
```

### Lockfile policy

| File | Policy |
| --- | --- |
| `noodle-lock.von` | **Source of truth** for installs |
| `package-lock.json` / `pnpm-lock.yaml` / `yarn.lock` / `bun.lock*` | **Ignored** (install may print a note). Never used for compat selection |

Compat: `noodle.compat` → `noodle.packageManager` → `packageManager` → default npm.

### Gaps (honest)

- pnpm-like is **A-lite**: no nested dep isolation inside `.pnpm/*/node_modules`, no peer injection, no real pnpm store hash ids
- No npm-style hoisting / peer dedupe parity on flat layout
- No automatic import of foreign lockfiles into `noodle-lock.von`
- Bin shims (`node_modules/.bin`) not generated yet
- Symlink may fall back to copy on locked-down Windows

## Notes

- 注册表 id（如 `"npm"`）由 **noodle 产品**选择，对 PM 是不透明字符串。
- 产品布局：`open_with_manifest_layout(..., noodle::project_layout())` → lockfile `noodle-lock.von`、home `NOODLE_HOME` / `.noodle`。
- `add` / `update` 用 PM `PackageManifest::{add_dependency,add_dev_dependency}` 与 bucket-preserving update；`package.json` 仍由 noodle 落盘（`write_manifest=false`）。
