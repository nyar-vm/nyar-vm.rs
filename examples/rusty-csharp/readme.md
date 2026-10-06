# rusty-csharp

A C# language frontend for the Nyar VM.

## Overview

`rusty-csharp` executes C# source on the Nyar VM, mapping .NET-style OOP onto Nyar's object system and GC.

## Features

- **Modern C# subset**: contemporary language features where implemented.
- **Managed object model**: classes, interfaces, structs on Nyar objects.
- **Algebraic effects**: `async`/`await` and Task patterns on Nyar effects.
- **Static typing**: preserved through lowering to unified IR.
- **JIT**: hot paths accelerated by `nyar-jit`.

## Supported constructs

- **OOP**: classes, inheritance, interfaces, properties, events.
- **Generics**: type and method generics via witness tables.
- **LINQ**: basic query expressions.
- **Async/await**: first-class async lowering.
- **Standard library**: initial `System` namespace coverage.

## Getting started

### Via Nyar CLI

```bash
nyar run Program.cs
```

### As a library

```rust
use rusty_csharp::RustyCSharpFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyCSharpFrontend::new();
let ast = frontend.parse("class Program { static void Main() { System.Console.WriteLine(\"Hello from C#!\"); } }").unwrap();
```

## License

Licensed under MIT OR Apache-2.0.
