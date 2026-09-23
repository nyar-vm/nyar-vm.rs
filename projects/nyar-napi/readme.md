# nyar-napi

Nyar VM — **Node-API 绑定层（纯库）**。

- 仅 `cdylib` / `rlib`，**无** `[[bin]]`、**无** `main`。
- 用户 CLI 在 `packages/nyar`（组装层）。
- 平台 collect：`pnpm build:napi` → `packages/nyar-*-x64/`。

```bash
cargo build -p nyar-napi --release
pnpm build:napi
```
