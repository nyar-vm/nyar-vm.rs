# Anonymous Functions and Closures

## Anonymous Functions

Anonymous functions are functions without a name, which can be defined and used directly in expressions.

### Basic Syntax

```valkyrie
# Basic anonymous function
let add = micro(x, y) { x + y }

# Single parameter anonymous function
let square = micro(x) { x * x }

# No parameter anonymous function
let get_random = micro() { random() }
```

## Closures

A closure is a special type of anonymous function that can capture variables from its defining environment.

### Closure Syntax
Closures are defined using curly braces `{}`, and parameters use the `$` prefix:

```valkyrie
# Single parameter closure
let numbers = [1, 2, 3, 4, 5]
let doubled = numbers.map { $x * 2 }

# Multi-parameter closure
let pairs = [(1, 2), (3, 4), (5, 6)]
let sums = pairs.map { $a + $b }

# No parameter closure
let lazy_value = { 42 }
```

### Parameter Auto-Inference

Parameters in a closure are automatically registered to the function signature in the order they first appear:

```valkyrie
# $x is the first parameter, $y is the second parameter
let operation = { $x + $y * 2 }

# Using only one parameter
let increment = { $n + 1 }
```

## Trailing Closures

When the last parameter of a function is a closure, you can use the trailing closure syntax, omitting the parentheses:

```valkyrie
# Traditional call
list.map(micro(x) { x * 2 })

# Trailing closure syntax (exactly equivalent)
list.map { $x * 2 }

# When there are multiple parameters, only the last one can use trailing syntax
list.fold(0, micro(acc, item) { acc + item })
# Equivalent to
list.fold(0) { $acc + $item }
```

### Complex Example

```valkyrie
# Method chaining with trailing closures
let result = numbers
    .filter { $x > 0 }
    .map { $x * $x }
    .fold(0) { $acc + $item }

# Nested closures
let matrix = [[1, 2], [3, 4], [5, 6]]
let flattened = matrix
    .map { $row.map { $x * 2 } }
    .flatten()
```

## Closure Capture

Closures can capture variables from their defining environment:

```valkyrie
let multiplier = 10
let numbers = [1, 2, 3, 4, 5]

# Closure captures external variable multiplier
let scaled = numbers.map { $x * multiplier }

# Capture mutable variable
let mut counter = 0
let increment_counter = {
    counter += 1
    counter
}
```

## Higher-Order Function Example

```valkyrie
# Custom higher-order function
micro apply_twice⟨T⟩(value: T, f: micro(T) -> T) -> T {
    f(f(value))
}

# Using trailing closure
let result = apply_twice(5) { $x * 2 }  # Result: 20

# Function composition
micro compose⟨A, B, C⟩(f: micro(B) -> C, g: micro(A) -> B) -> micro(A) -> C {
    { f(g($x)) }
}

let add_one = micro(x) { x + 1 }
let double = micro(x) { x * 2 }
let add_one_then_double = compose(double, add_one)
```

## Best Practices

1. **Conciseness**: For simple operations, prefer closures over named functions.
2. **Readability**: Complex logic should use named functions to improve readability.
3. **Trailing Closures**: When a closure is the last parameter, use trailing syntax to improve code aesthetics.
4. **Parameter Naming**: Use meaningful parameter names in closures (e.g., `$item`, `$element` instead of `$x`).

```valkyrie
# Good practice
users.filter { $user.is_active }
     .map { $user.name }
     .sort_by { $name.length }

# Avoid excessive nesting
let process_data = micro(data) {
    data.filter { $item.is_valid }
        .transform { $item.normalize() }
        .group_by { $item.category }
}
```