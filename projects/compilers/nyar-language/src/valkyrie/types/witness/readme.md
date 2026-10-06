# Witness table

Core witness-table data structures for Valkyrie dynamic dispatch.

## Terminology

| Term | Scope | Notes |
| :--- | :--- | :--- |
| **witness table** | This module / `trait` · `imply` | Valkyrie dynamic dispatch; fat pointer `(data, witness_table)` |
| **COM vtable** | Windows FFI | `[com]` interop only; not part of this module |
| **classic OOP vtable** | External comparison | Documentation contrast only; not Valkyrie's model |

## Overview

A witness table is the runtime representation of a trait implementation used for dynamic method dispatch. Each `impl Trait for Type` generates a witness table.

## Data shape

```text
struct WitnessTable {
    trait_id: Identifier,
    type_id: Identifier,
    methods: Vec<WitnessMethod>,
    associated_types: Vec<AssociatedType>,
}
```

## Dispatch modes

- **Static dispatch** — concrete type known at compile time
- **Dynamic dispatch** — runtime lookup through a witness table
