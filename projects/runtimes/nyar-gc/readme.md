# nyar-gc

Managed object heap and garbage collector for the Nyar bytecode runtime.

## Overview

`nyar-gc` provides allocation, tracing, collection, and tuning hooks used by `nyar-vm::Executor`. It is **language-agnostic**: it never imports `nyar-language` or guest frontends. Object shapes arrive as `LayoutDescriptor` values registered by the loader; the heap stores dense slot vectors keyed by `layout_id`.

## Features

- **`ObjectHeap`** — free-list allocation, layout table, write barrier, host roots, generation tags, live/total byte accounting.
- **`GarbageCollector`** — mark-sweep baseline with nursery / tenured soft capacities and promotion failure reporting.
- **`LayoutDescriptor` / `LayoutId`** — field-slot layout contract (no field names on the hot path).
- **`Value` / `ObjectPayload`** — NaN-boxed scalars plus `LayoutObject` and suspended `Coroutine` payloads.
- **`GcPolicy` / `WorkloadHints` / `WorkloadIntent`** — policy inputs and workload-driven strategy transitions.
- **`HostRoots` / `RootHandle`** — persistent roots supplied by the embedder.
- **`ConcurrentMarkController` / `WriteBarrier`** — concurrent marking protocol skeleton and SATB barrier hooks.
- **`StrategyController`** — bridges intent JSON into collector decisions.

## Integration with `nyar-vm`

The VM embeds `ObjectHeap` and `GarbageCollector` inside `Executor`. During interpretation, stack slots and global tables hold `Value` references; collection traces from roots through object graphs. Deoptimization paths use `encode_value_for_deopt` helpers exported from `nyar-vm`, not from this crate.

## Usage

```rust
use nyar_gc::{ObjectHeap, GarbageCollector, GcPolicy};

let mut heap = ObjectHeap::new();
let mut gc = GarbageCollector::new();
// Register layouts, allocate objects, trace from roots — see heap/collector APIs.
```

## Boundaries

- Does not load `.nyar` files or decode bytecode.
- Does not define language type systems or witness tables (those live in compilers / `nyar-types`).
