# types src

Shared type definitions and compile-time data structures for Valkyrie.

## Responsibilities

- Maintain `SourceSpan`, error types, HIR structures, and witness-related foundations.
- Act as the shared contract between parser, compiler, and interpreter paths.

## Forbidden

- No concrete compile pipelines in this layer.
- Do not expand shared types into a god module of behavior and control flow.
