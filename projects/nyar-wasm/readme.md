# nyar-wasm

Nyar VM — **Wasm GC 绑定层（纯库）**。

- 仅 `cdylib` / `rlib`，**无** `[[bin]]`、**无** `main`。
- `packages/nyar-wasm32-wasi` collect：`pnpm build:wasm`。
- 用户 CLI：`packages/nyar`。

```bash
cargo build -p nyar-wasm --release
pnpm build:wasm
```
