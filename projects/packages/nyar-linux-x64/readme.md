# @nyar-vm/nyar-linux-x64

Prebuilt **native host sidecar** for Nyar VM on Linux x64. **Not a public import target.**

This package is pulled automatically when installing `@nyar-vm/nyar` on a matching host.

## Example

Application code imports the main package only:

```typescript
import { createNyarHost } from "@nyar-vm/nyar";

const host = await createNyarHost();
await host.runBytecode(bytes);
```

Build and publish pipelines are documented for maintainers in the repository; they are not repeated here.
