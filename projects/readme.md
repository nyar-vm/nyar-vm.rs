# Projects layout

This directory groups Rust crates and host bindings by responsibility. It describes **only the public structure of this
repository**.

## Directories

| Path         | Role                                                                     |
|--------------|--------------------------------------------------------------------------|
| `compilers/` | Nyar compiler stack: `nyar-language`, `nyar-emitter`, `nyar-analyzer`, … |
| `formats/`   | On-disk bytecode contract (`odin-bytecode`)                              |
| `runtimes/`  | VM, GC, JIT (`odin-vm`, `nyar-gc`, `nyar-jit`)                           |
| `hosts/`     | Node-API / Wasm host bindings (`nyar-napi`, `nyar-wasm`)                 |
| `packages/`  | npm platform collect packages (`nyar-win32-x64`, …)                      |

## Dependency boundaries (summary)

- **Text and binary formats** — Oak owns text parsing and recovery; Acorn owns external binary format models and codecs.
- **Frontend host** — `nyar-language` provides language frontend adapters and their frontend contracts; it may use Oak
  parsers and hand closed semantic facts to the Nyar platform.
- **Platform lowering** — `nyar` / `nyar-emitter` own language-independent planning, target lowering, and artifact
  preparation after the frontend contract. Nyar runtime packages consume verified artifacts.
- **Host-script frontends** (bash / lua / tcl / powershell / c): the corresponding Oak parser owns text syntax; the
  language adapter owns script semantics. No `std-data` mixed parser/codec package is a required compile stage.

## Optional integration tests

Some tests can point at an external Valkyrie language checkout via environment variable (**not required for release**):

```bash
export VALKYRIE_V_ROOT=/path/to/valkyrie-language-checkout
cargo test -p nyar-language
```

When unset, related cases are skipped and default CI is unaffected.

Per-crate documentation lives in each crate's `readme.md` (for example [
`odin-vm/readme.md`](./runtimes/odin-vm/readme.md)).
