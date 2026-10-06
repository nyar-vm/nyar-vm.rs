# rusty-java (Mini Java)

A Java language frontend for the Nyar VM.

## Overview

`rusty-java` (Mini Java) runs Java source on the Nyar VM, mapping JVM-style OOP onto Nyar's object system and managed runtime.

## Features

- **Object-oriented**: classes, inheritance, and interfaces on Nyar objects.
- **Managed runtime**: `nyar-gc` plus `nyar-jit` optimization tiers.
- **Static typing**: strong static types preserved through lowering to unified IR.
- **Concurrency**: threads and synchronization mapped to Nyar tasks.
- **AOT and JIT**: supports ahead-of-time and just-in-time execution paths.

## Supported constructs

- **Core syntax**: `class`, `interface`, `extends`, `implements`, visibility modifiers.
- **Methods and fields**: instance and static members.
- **Control flow**: `if`, `switch`, `for`, `while`, `try-catch-finally`.
- **Standard library**: initial `java.lang` / `java.util` coverage.
- **Generics**: basic generics via witness tables.

## Getting started

### Via Nyar CLI

```bash
nyar run Main.java
```

### As a library

```rust
use rusty_java::MiniJavaFrontend;
use nyar_types::NyarFrontend;

let frontend = MiniJavaFrontend::default();
let ast = frontend.parse("public class Hello { public static void main(String[] args) { System.out.println(\"Hello from Java!\"); } }").unwrap();
```

## License

Licensed under MIT OR Apache-2.0.
