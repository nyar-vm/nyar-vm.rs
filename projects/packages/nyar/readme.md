# @nyar/nyar

Cross-platform Nyar host CLI and TypeScript facade. **CLI orchestration** uses [`@vmz/commander`](https://www.npmjs.com/package/@vmz/commander); **orthogonal native capabilities** are exported from `nyar-napi` via `src/native.ts`.

## CLI

```bash
nyar version
nyar host-info
nyar runtime list
nyar runtime prepare --family wasi --artifact ./app.wasm --entry main
```

## Programmatic native surface

```typescript
import { getHostInfo, listRuntimeFamilies, prepareRuntimeCommand, runHostScript } from '@nyar/nyar';

const info = getHostInfo();
const families = listRuntimeFamilies();
const cmd = prepareRuntimeCommand({
    family: 'wasi',
    artifact: '/path/to/app.wasm',
    entry: 'main',
});

const result = runHostScript({language: 'lua', source: 'print("hi")'});
console.log(result.value);
```

`runHostScript` executes host-script source through the `legend-vm` substrate in `nyar-napi`; the simulation packages (`@nyar/bash`, `@nyar/lua`, `@nyar/tcl`) use it for their `bin` entries.

Platform collects (`nyar-win32-x64`, `nyar-linux-x64`, `nyar-darwin-*`, `nyar-unknown-wasm32`) are all optional dependencies: the host picks the collect matching the current OS/arch, falls back to the Wasm collect when available, and reports a clear error when none is installed.

## License

MIT
