# rusty-mojo

A Mojo language frontend for the Nyar VM.

## Overview

`rusty-mojo` is an experimental compiler frontend for the Mojo programming language, targeting the Nyar VM. It aims to combine Mojo's high-performance AI and systems programming capabilities with Nyar's advanced runtime optimizations and multi-tier JIT compilation.

## Features

- **Performance Oriented**: Designed to map Mojo's hardware-aware features to Nyar's efficient execution model.
- **AI Integration**: Aims to support Mojo's specialized syntax for AI model development and execution.
- **Nyar Integration**: Seamlessly compiles to Gaia IR, enabling optimization via `nyar-jit` and `nyar-aot`.
- **Interoperability**: Built to allow Mojo code to interact with other Nyar-supported languages like Python and C++.

## Project Status

**Current Status: Experimental / Placeholder**

The `rusty-mojo` frontend is currently in the early stages of development.
- **Parser**: Under development (utilizing `PlaceholderLanguage` for now).
- **Lowering**: Basic module structure generation is implemented.
- **Execution**: Initial integration with `nyar-vm` is in progress.

## Roadmap

1. Implement full Mojo lexical and syntactic analysis.
2. Map Mojo's struct and trait system to Nyar's object model and witness tables.
3. Integrate with SIMD and hardware acceleration primitives provided by the Nyar platform.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run example.mojo
```

## License

Licensed under MIT OR Apache-2.0.
