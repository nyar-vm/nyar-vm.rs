# Functional Programming

Valkyrie is a language deeply influenced by functional programming concepts, providing a rich set of functional features that make code more concise, composable, and testable.

## Core Concepts

### First-Class Functions

In Valkyrie, functions are first-class citizens. This means functions can be:
- Assigned to variables
- Passed as arguments to other functions
- Returned as values from other functions

### Immutability

Valkyrie encourages the use of immutable data. While mutability is supported (via the `mut` keyword), variables are immutable by default, which helps reduce side effects and improves concurrency safety.

### Pure Functions

While Valkyrie allows side effects, it is recommended to write pure functions. A pure function's output depends only on its inputs and produces no observable side effects.

## Main Features

### [Anonymous Functions and Closures](./anonymous-functions.md)

Anonymous functions are functions without a name, and closures are anonymous functions that can capture variables from their defining environment. Valkyrie provides concise closure syntax and automatic parameter inference.

### [Pattern Matching](./pattern-match.md)

The powerful pattern matching system allows you to branch based on the structure of data, supporting the destructuring of tuples, arrays, objects, and custom union types.

### Higher-Order Functions

Higher-order functions are functions that take other functions as arguments or return them as results. Common built-in higher-order functions include `map`, `filter`, `fold`, etc.

```valkyrie
let numbers = [1, 2, 3, 4, 5]

# Using higher-order functions for composition
let result = numbers
    .filter { $x % 2 == 0 }
    .map { $x * $x }
    .fold(0) { $acc + $item }
```

### Function Composition

By combining simple functions into complex ones, you can build highly modular systems.

## Benefits

- **Conciseness**: Reduces boilerplate code and makes logic clearer.
- **Composability**: Build complex functionality by combining small, single-responsibility functions.
- **Testability**: Pure functions are easy to unit test as they don't depend on external state.
- **Safety**: Reduces complexity and potential errors associated with state management.
