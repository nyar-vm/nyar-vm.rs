# vcc-wasm

Valkyrie Compiler Collect — **Wasm GC 绑定层（纯库）**。

- 仅 `cdylib` / `rlib`， **无** `[[bin]]`、 **无** `main`。
- `projects/packages/vcc-unknown-wasm32` 由 `legion build --target node` + `node scripts/build.mjs assemble` 装配。
- `projects/packages/vcc-wasm32-wasi`：`pnpm build:wasm`。
- 用户 CLI：`projects/packages/legion`、`projects/packages/asgard`。

```bash
cargo build -p vcc-wasm --release
pnpm build:wasm
```
