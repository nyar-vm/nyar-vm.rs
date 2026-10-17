# @nyar/nyar-unknown-wasm32

Prebuilt **Wasm GC host sidecar** for Nyar VM. **Not a public import target.**

This package is pulled automatically when installing `@nyar/nyar` on Wasm-capable hosts.

## Example

Application code imports the main package only:

```typescript
import { createNyarHost } from "@nyar/nyar";

const host = await createNyarHost();
await host.runBytecode(bytes);
```

Build and publish pipelines are documented for maintainers in the repository; they are not repeated here.
