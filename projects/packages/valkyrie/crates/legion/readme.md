# legion

Valkyrie 工作区 CLI：清单解析、依赖解析、构建与目标选择。

## 职责

- `legion` 命令行入口
- 工作区与 `legion.von` / `legions.von` 解析
- 本地依赖覆盖：`.config/legion/legions.von`（类似 Rust `.cargo/config.toml`）
- 构建计划与目标编排

编译器语义与后端降低在 OVM（`odin-vm` / `odin-bytecode`）与 Acorn / Oak 的 git 依赖中实现；本 crate 负责装配与调度。

## 构建

`legion` crate 仅提供库 API（`legion::cli::run_with_argv`）。**禁止** `[[bin]]` / `vcc.exe` / `legion.exe`。
原生 CLI 宿主由 `vcc-napi` 产出 platform `.node` collect；npm 用户入口是 `@valkyrie-language/legion`（经 VCC N-API / wasm collect）。

```bash
cargo test -p legion
node scripts/build.mjs napi        # 生成 platform `vcc.<triple>.node` collect
```

npm 装配见 [`projects/packages/legion`](../../packages/legion/README.md)。

## 本地 path 覆盖（`.config/legion/legions.von`）

项目 `legion.von` 可固定 **git 依赖**（CI 友好）。本机开发时在仓库根或上级目录放置
`.config/legion/legions.von`，用 **path** 覆盖同名依赖，无需改题目 manifest：

```von
{
    name: "local-overrides",
    dependencies: {
        "std": {
            source: "path",
            path: "../valkyrie.v/projects/std"
        },
        "core": {
            source: "path",
            path: "../valkyrie.v/projects/core"
        }
    }
}
```

`path` 相对包含 `.config/` 的目录解析。更近层的配置文件优先级更高；可选全局
`~/.config/legion/legions.von` 作为最低优先级默认值。建议将本机覆盖文件加入 `.gitignore`。
