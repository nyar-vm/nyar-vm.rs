# Type System

Valkyrie provides a powerful and flexible type system, supporting static type checking, type inference, and advanced type features.

## Basic Types

### Primitive Types

```valkyrie
# Integer types
let a: i32 = 42
let b: u64 = 100
let c: isize = -1

# Floating-point types
let x: f32 = 3.14
let y: f64 = 2.718281828

# Boolean type
let flag: bool = true

# Characters and Strings
let ch: char = 'A'
let text: string = "Hello, World!"
```

### Composite Types

```valkyrie
# Array types
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let dynamic: [string] = ["a", "b", "c"]

# Tuple types
let point: (f64, f64) = (3.0, 4.0)
let mixed: (string, i32, bool) = ("test", 42, true)

# Optional types
let maybe_value: i32? = 42
let empty: string? = None
```

## Composite Type Definitions

### Record Types

```valkyrie
# Basic record type
type Point = {
    x: f64,
    y: f64,
}

# Generic record type
type Container⟨T⟩ = {
    value: T,
    metadata: string,
}

# Nested record type
type Person = {
    name: string,
    age: i32,
    address: {
        street: string,
        city: string,
    },
}
```

### Unity Types (Unions)

```valkyrie
# Basic unity type
unity Result⟨T, E⟩ {
    Fine(T),
    Fail(E)
}

# Complex unity type
unity Expression {
    Literal(i32),
    Variable(string),
    Binary {
        left: Expression,
        operator: string,
        right: Expression,
    }
}
```

## Traits and Implementations

### Trait Definition

```valkyrie
# Basic trait
trait Display {
    micro fmt(self) -> string
}
```
