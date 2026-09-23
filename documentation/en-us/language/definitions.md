# Definitions

Valkyrie provides various definition syntaxes for declaring namespaces, variables, functions, types, and other program entities.

## Namespace Definition

Valkyrie uses the `namespace` or `namespace!` keywords to declare the namespace a module belongs to.

```valkyrie
# Explicitly declare a namespace
namespace! package.collection.option;

# Or
namespace package.text;
```

## Variable Definition

### Basic Variable Definition

```valkyrie
# Immutable variable
let name = "Alice"
let age = 30

# Mutable variable
let mut counter = 0

# Explicit type annotation
let score: i32 = 95

# Deferred initialization
let result: i32
if condition {
    result = 42
} else {
    result = 0
}
```

## Function Definition (micro)

Valkyrie uses the `micro` keyword to define functions.

### Basic Function Definition

```valkyrie
# No-argument function
micro greet() {
    print("Hello, World!")
}

# Function with arguments
micro add(a: i32, b: i32) -> i32 {
    a + b
}
```

## Type Definition

Valkyrie distinguishes between structured data (`class`) and algebraic data types (`unity`).

### Class Definition (class)

```valkyrie
class Point {
    x: f64
    y: f64
}
```

### Union Type Definition (unity)

`unity` is used to define algebraic data types similar to Rust's `enum`.

```valkyrie
unity Option⟨V⟩ {
    Some {
        value: V
    }
    None
}
```

## Implementation Definition (imply)

Valkyrie uses the `imply` keyword to implement methods or Traits for types.

```valkyrie
imply Option⸬Some {
    constructor(value: V) {
        this.value = value
    }
}

imply Unicode {
    # Implementation methods
}
```

# Named Parameter Call
let user = create_user(name: "Alice", active: false)
let result = sum(1, 2, 3, 4, 5)

# Reference Parameters
micro modify_array(arr: [i32]) {
    for i in 0..<arr.len() {
        arr[i] *= 2
    }
}

# Generic Parameters
micro identity⟨T⟩(value: T) -> T {
    value
}

micro map⟨T, U⟩(items: [T], transform: micro(T) -> U) -> [U] {
    let mut result = []
    for item in items {
        result.push(transform(item))
    }
    result
}
```

### Higher-Order Functions

```valkyrie
# Function as a parameter
micro apply_operation(x: i32, y: i32, op: micro(i32, i32) -> i32) -> i32 {
    op(x, y)
}

# Returning a function
micro make_adder(n: i32) -> micro(i32) -> i32 {
    micro(x: i32) -> i32 {
        x + n
    }
}

# Closures
let add_five = make_adder(5)
let result = add_five(10)  # 15

# Anonymous functions
let numbers = [1, 2, 3, 4, 5]
let doubled = numbers.map(micro(x) { x * 2 })
let filtered = numbers.filter(micro(x) { x % 2 == 0 })
```
