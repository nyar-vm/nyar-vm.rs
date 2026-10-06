# interpreter src

Runtime dispatch source for `nyar-runner` (rustdoc module doc).

## Responsibilities

- Organize per-target runtime descriptors.
- Provide effects, FFI, registry, and runtime dispatch.
- Maintain host-side target mapping for `legion run`.
- Keep each target runtime isolated; no shared messy host logic.

## Target mapping

| Module | Runs as |
|:---|:---|
| `jvm` | Java |
| `clr` | .NET |
| `wasm` | Node |
| `windows` | native `.exe` |
| `wasi` | Wasmtime |

## Forbidden

- Do not compensate for missing compiler lowering here.
- Do not let one target directory become the cross-platform mega-entry.
