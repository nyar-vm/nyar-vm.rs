# rusty-mojo

An experimental Mojo language frontend for the Nyar VM.

## Overview

`rusty-mojo` is an experimental frontend for Mojo, aiming to combine AI- and systems-programming-oriented Mojo with Nyar multi-tier JIT and runtime optimization.

## Features

- **Performance-oriented**: Mojo hardware-aware features mapped to Nyar's efficient execution model.
- **AI integration**: dedicated syntax for AI model development and execution (planned).
- **Nyar integration**: compiles to Gaia IR, optimized via `nyar-jit` and `nyar-aot`.
- **Interop**: interaction with other Nyar-supported languages such as Python and C++ (planned).

## Project status

**Current: experimental / placeholder**

- **Parser**: in development (currently uses `PlaceholderLanguage`).
- **Lowering**: basic module structure generation is available.
- **Execution**: initial integration with `nyar-vm` in progress.

## Roadmap

1. Complete Mojo lexical and syntactic analysis.
2. Map Mojo structs/traits to the Nyar object model and witness tables.
3. Integrate Nyar platform SIMD and hardware acceleration primitives.

## Getting started

### Via Nyar CLI

```bash
nyar run example.mojo
```

## License

Licensed under MIT OR Apache-2.0.
