# vcc-napi

Valkyrie Compiler Collect — **Node-API 绑定层（纯库）**。

- 仅 `cdylib` / `rlib`， **无** `[[bin]]`、 **无** `main`。
- 用户 CLI 在 `packages/legion` / `packages/asgard`（组装层）。
- 平台 collect：`pnpm build:napi` → `packages/vcc-*-x64/`。

```bash
cargo build -p vcc-napi --release
pnpm build:napi
```
