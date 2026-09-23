# rusty-csharp

A C# language frontend for the Nyar VM.

## Overview

`rusty-csharp` is a compiler frontend that allows C# source code to be executed on the Nyar VM. It provides a robust, managed environment for C# applications, mapping the .NET-style object model to Nyar's native execution engine and advanced garbage collector.

## Features

- **Modern C# Syntax**: Support for a significant subset of modern C# language features.
- **Managed Object Model**: Maps C# classes, interfaces, and structs to Nyar's native object system.
- **Algebraic Effects Integration**: Maps C# `async`/`await` and task-based patterns to Nyar's native algebraic effects.
- **Strong Typing**: Maintains C#'s strong, static type system during the lowering process to Gaia IR.
- **JIT Optimization**: Leverages `nyar-jit` for high-performance execution of hot code paths.

## Supported Constructs

- **Object-Oriented Features**: Classes, Inheritance, Interfaces, Properties, Events.
- **Generics**: Support for generic types and methods via Nyar's witness tables.
- **LINQ**: Support for basic LINQ expressions and query syntax.
- **Async/Await**: Native support for asynchronous programming.
- **Standard Library**: Initial support for core `System` namespace types.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run Program.cs
```

### Usage as a Library

```rust
use rusty_csharp::RustyCSharpFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyCSharpFrontend::new();
let ast = frontend.parse("class Program { static void Main() { System.Console.WriteLine(\"Hello from C#!\"); } }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
