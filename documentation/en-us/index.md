---
layout: home

hero:
  name: "Valkyrie"
  text: "Modern Functional Programming Language"
  tagline: A next-generation programming language merging algebraic effects with a strong type system.
  image:
    src: /logo.svg
    alt: Valkyrie
  actions:
    - theme: brand
      text: Get Started
      link: /guide/getting-started
    - theme: alt
      text: View Examples
      link: /examples/

features:
  - icon: 🎭
    title: Algebraic Effects System
    details: Native support for algebraic effects to gracefully handle side effects like exceptions, asynchrony, and state management, providing more powerful and flexible control flow abstractions than traditional exception handling.
  - icon: 🔒
    title: Strong Type System
    details: An advanced type system supporting generics, type inference, and pattern matching, catching more errors at compile time and providing an exceptional development experience and code safety.
  - icon: 🚀
    title: Multi-target Compilation
    details: Compiles to WebAssembly, JavaScript, and native code. A single codebase runs in browsers, on servers, and in desktop environments, truly achieving "write once, run anywhere."
  - icon: ⚡️
    title: Functional Programming
    details: Features like immutable data structures, higher-order functions, and tail-call optimization, combined with modern syntax design, make code more concise, reliable, and maintainable.
  - icon: 🛠️
    title: Modern Toolchain
    details: Integrated package manager, formatting tools, and language server provide a complete development ecosystem with support for incremental compilation and intelligent code completion.
  - icon: 🌐
    title: Progressive Adoption
    details: Seamlessly integrates with existing JavaScript/TypeScript projects, supporting progressive migration and reducing learning costs and migration risks.
---

## What is Valkyrie?

Valkyrie is a modern functional programming language designed for building reliable, high-performance applications. It combines an algebraic effects system with a strong type system to provide a brand-new programming experience for developers.

### Core Features

- **Algebraic Effects**: Gracefully handle side effects, unifying exceptions, asynchrony, and state management.
- **Strong Type System**: Compile-time error checking with support for type inference and pattern matching.
- **Multi-target Compilation**: Compiles to WebAssembly, JavaScript, and native code.
- **Functional Programming**: Immutable data, higher-order functions, and tail-call optimization.
- **Modern Syntax**: Concise and expressive syntax design.
- **Progressive Adoption**: Seamless integration with existing ecosystems.

### Quick Example

```valkyrie
// Define algebraic effects
effect Http {
    get(url: String): String
    post(url: String, body: String): String
}

// Function using effects
micro fetch_user_data(id: Int) -> User {
    let response = raise Http::get(`/api/users/{id}`)
    parse_json(response)
}

// Effect handler
micro main() {
    try {
        fetch_user_data(42)
    }
    .catch {
        case Http::get(url): resume(http_client.get(url))
        case Http::post(url, body): resume(http_client.post(url, body))
    }
}

// Pattern matching and type safety
match user_result {
    Some(u) if u.age >= 18: print("Adult user: {u.name}"),
    Some(u): print("Minor user: {u.name}"),
    None: print("User does not exist")
}
```

This simple Valkyrie program demonstrates:
- Definition and usage of algebraic effects
- Strong type system and type inference
- Pattern matching and conditional guards
- String interpolation and modern syntax

## Get Started

Ready to experience the power of Valkyrie?

[Quick Start →](/guide/getting-started)

## Why Choose Valkyrie?

### 🎯 **Solving Real Problems**
Traditional programming languages often struggle when dealing with side effects. Valkyrie's algebraic effects system provides an elegant solution, making exception handling, asynchronous programming, and state management simple and powerful.
