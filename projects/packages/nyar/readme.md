# @nyar-vm/nyar

Cross-platform npm facade for the Nyar VM CLI and host APIs. Native and Wasm sidecars are published as separate platform packages under `projects/packages/`.

## Example

```typescript
import { createNyarHost } from "@nyar-vm/nyar";

const host = await createNyarHost();
await host.runBytecode(bytes);
```

Platform-specific binaries (`nyar-win32-x64`, `nyar-linux-x64`, `nyar-darwin-*`, `nyar-wasm32-wasi`) are resolved automatically when this package is installed on a matching host.

Build collect steps (`pnpm build:napi`, `pnpm build:wasm`) live in repository maintainer docs, not on this registry page.

## License

MIT
