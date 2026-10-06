# nyar-bytecode

Authoritative on-disk contract for Nyar **storage bytecode** (`.nyar` / `.legion`).

## Overview

This crate owns wire layout, section kinds, opcode head codes, encode/decode, and the pure data structures shared by compilers and the VM loader. It defines **what bytes mean on disk**, not how the interpreter executes them. Runtimes, GC, and JIT must not redefine opcodes or section layout elsewhere.

## Format

| Constant | Meaning |
|:---|:---|
| `NYAR_MAGIC` | File magic `NYAR` (big-endian `0x4E594152`) |
| `NYAR_VERSION` | Current format version (`2`) |
| `BYTECODE_FORMAT_VERSION` | Provenance / cache key alias, kept in sync with `NYAR_VERSION` |
| `OBSOLETE_CALL_NATIVE` | Retired v1 opcode (`0xD1`); v2 loaders must reject modules containing it |

Modules are built from `NyarModuleData`: functions, constants, imports/exports, globals, layouts, witness dispatch tables, and raw code bytes. Sections are described by `NyarSectionKind` with fixed header sizes (`HEADER_SIZE`, `SECTION_HEADER_SIZE`).

## Instruction set (head codes)

Stack operations (`Const`, `LoadLocal`, `StoreGlobal`, …), control flow (`Jump`, `Call`, `Return`), objects (`ObjectNew`, `FieldGet`, `FieldSet`), host calls (`CallImport`, `CallIntrinsic`), async (`Yield`, `Resume`), and algebraic effects (`PerformEffect`). Emit helpers include `emit_plain`, `emit_imm1`, and `emit_imm2`; decode uses `decode_at`.

## Public API

- `encode_module` / `decode_module` — round-trip a full module.
- `NyarFunction`, `NyarImport`, `NyarExport`, `NyarLayout` — structured metadata.
- `NyarDecodeError` — validation failures (magic, version, unknown sections, obsolete opcodes).

## Usage

```rust
use nyar_bytecode::{NyarModuleData, encode_module, decode_module};

let bytes = encode_module(&data);
let restored = decode_module(&bytes)?;
```

## Boundaries

- No interpreter, stack frames, or GC tracing.
- No language-specific lowering; emitters in compiler crates produce `NyarModuleData`.
