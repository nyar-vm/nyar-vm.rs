# Projects layout

This directory groups Rust crates and host bindings by responsibility. It describes **only the public structure of this repository**.

## Directories

| Path | Role |
| --- | --- |
| `compilers/` | Nyar compiler stack: `nyar-language`, `nyar-emitter`, `nyar-analyzer`, … |
| `formats/` | On-disk bytecode contract (`nyar-bytecode`) |
| `runtimes/` | VM, GC, JIT (`nyar-vm`, `nyar-gc`, `nyar-jit`) |
| `hosts/` | Node-API / Wasm host bindings (`nyar-napi`, `nyar-wasm`) |
| `packages/` | npm platform collect packages (`nyar-win32-x64`, …) |

## Dependency boundaries (summary)

- **Text and binary formats** — `std-data` (from [valkyrie.rs](https://github.com/valkyrie-language/valkyrie.rs) `vcc-data`; workspace dependency key remains `std-data`).
- **Compile main chain** — `nyar-language` → `nyar-emitter` → `std-data`; `nyar-language` must not depend on emitter lowering implementation details.
- **Host-script frontends** (bash / lua / tcl / powershell / c): parsing in `std-data`, semantics and `interpret` in `nyar-language::src/<lang>/`.

## Optional integration tests

Some tests can point at an external Valkyrie language checkout via environment variable (**not required for release**):

```bash
export VALKYRIE_V_ROOT=/path/to/valkyrie-language-checkout
cargo test -p nyar-language
```

When unset, related cases are skipped and default CI is unaffected.

Per-crate documentation lives in each crate's `readme.md` (for example [`nyar-vm/readme.md`](./runtimes/nyar-vm/readme.md)).
