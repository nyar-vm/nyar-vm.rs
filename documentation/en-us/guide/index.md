# Getting Started with Nyar

Welcome to Nyar! Nyar is a modern functional programming language that provides a powerful type system, a flexible module system, and rich language features.

## What is Nyar?

Nyar is a multi-paradigm programming language that offers:

- 🎯 **Powerful Type System**: Supports generics, higher-kinded types, type inference, and other advanced features.
- 🚀 **Modern Syntax**: Concise and intuitive syntax, supporting pattern matching, closures, and more.
- 🔒 **Memory Safety**: A garbage collector automatically manages memory to prevent leaks.
- ⚡ **High Performance**: Zero-cost abstractions and compile-time optimizations.
- 🔧 **Flexible Module System**: Namespace-based module organization.

## Basic Syntax

### Variable Definitions

```nyar
# Immutable variables
let name = "Alice"
let age = 30
let is_active = true

# Mutable variables
let mut counter = 0
let mut items = []

# Explicit type annotations
let score: i32 = 95
let price: f64 = 29.99
let message: String = "Hello"
```

### Function Definitions

```nyar
# Basic function definition
micro greet() {
    print("Hello, World!")
}

# Function with parameters and return value
micro add(a: i32, b: i32) -> i32 {
    a + b
}

# Function with multiple parameters
micro calculate(x: f64, y: f64, operation: String) -> f64 {
    if operation == "add" {
        x + y
    } else if operation == "multiply" {
        x * y
    } else {
        0.0
    }
}
```

### Basic Data Types

```valkyrie
# Integer types
let a: i32 = 42
let b: u64 = 100

# Floating-point types
let x: f32 = 3.14
let y: f64 = 2.718281828

# Boolean type
let flag: bool = true

# Characters and Strings
let ch: char = 'A'
let text: String = "Hello, World!"

# Array types
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let dynamic: [String] = ["a", "b", "c"]

# Tuple types
let point: (f64, f64) = (3.0, 4.0)
let mixed: (String, i32, bool) = ("test", 42, true)
```

## Control Flow

### Conditional Statements

```valkyrie
# if statement
if x > 0 {
    print("Positive")
} else {
    print("Non-positive")
}

# if expression
let result = if x > 0 { "positive" } else { "non-positive" }

# Multiple conditions
if score >= 90 {
    grade = "A"
} else if score >= 80 {
    grade = "B"
} else {
    grade = "F"
}
```

### Loop Statements

```valkyrie
# while loop
while counter < 10 {
    print(counter)
    counter = counter + 1
}

# for loop
for i in 0..10 {
    print(i)
}

# Iterating over an array
for item in items {
    print(item)
}

# Infinite loop
loop {
    if should_break {
        break
    }
}
```

## Pattern Matching

```valkyrie
# Basic pattern matching
match value {
    case 1: "one"
    case 2: "two"
    case 3: "three"
    case _: "other"
}

# Range matching
match score {
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case _: "F"
}

# Tuple destructuring
match point {
    case (0, 0): "Origin"
    case (x, 0): "On X-axis at ${ x }"
    case (0, y): "On Y-axis at ${ y }"
    case (x, y): "Point at (${ x }, ${ y })"
}
```

## Type Definitions

### Record Types

```valkyrie
# Basic record type
type Point = {
    x: f64,
    y: f64,
}

# Generic record type
type Container<T> = {
    value: T,
    metadata: String,
}
```

### Union Types

```valkyrie
# Basic union type
union Result<T, E> {
    Fine { value: T },
    Fail { error: E }
}

# Using union types
let result: Result<i32, String> = Fine { value: 42 }
match result {
    case Fine { value }: print("Success: ${ value }")
    case Fail { error }: print("Error: ${ error }")
}
```
