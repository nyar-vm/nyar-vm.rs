# rusty-java (Mini Java)

A Java language frontend for the Nyar VM.

## Overview

`rusty-java` (also known as Mini Java) is a compiler frontend that allows Java source code to be executed on the Nyar VM. It provides a managed, object-oriented runtime for Java applications, mapping the JVM-style execution model to Nyar's native engine and advanced garbage collector.

## Features

- **Object-Oriented Excellence**: Maps Java's class-based object model, inheritance, and interfaces to Nyar's native object system.
- **Managed Runtime**: Benefits from `nyar-gc` for automatic memory management and `nyar-jit` for multi-tier optimization.
- **Static Typing**: Maintains Java's strong, static type system during the lowering process to Gaia IR.
- **Concurrency Support**: Maps Java's threading and synchronization primitives to Nyar's native task system.
- **AOT & JIT Support**: Supports both ahead-of-time compilation and just-in-time execution.

## Supported Constructs

- **Core Syntax**: `class`, `interface`, `extends`, `implements`, `public`, `private`.
- **Methods & Fields**: Full support for instance and static members.
- **Control Flow**: `if`, `switch`, `for`, `while`, `try-catch-finally`.
- **Standard Library**: Initial support for core `java.lang` and `java.util` classes.
- **Generics**: Basic support for Java generics via witness tables.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run Main.java
```

### Usage as a Library

```rust
use rusty_java::MiniJavaFrontend;
use nyar_types::NyarFrontend;

let frontend = MiniJavaFrontend::default();
let ast = frontend.parse("public class Hello { public static void main(String[] args) { System.out.println(\"Hello from Java!\"); } }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
