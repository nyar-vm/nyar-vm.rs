# Nyar Language Features Guide

## Overview

Nyar is a modern programming language designed to provide powerful expressiveness and extreme performance. This guide details the core features and advanced functionality of the Nyar language, helping developers deeply understand and utilize it.

## Compiler Architecture

Nyar's compiler adopts a modern design, providing:
- Highly optimized incremental compilation infrastructure
- Multi-layer intermediate representations (AST → HIR → CFG → SSA → LIR)
- Pluggable code generation backends
- A unified error handling framework
- Comprehensive Language Server Protocol (LSP) support

## Core Language Features

### 1. Powerful Type System

#### Basic Types

```nyar
# Primitive types
let integer: i32 = 42
let float: f64 = 3.14159
let boolean: bool = true
let character: char = 'A'
let text: String = "Hello, World!"

# Compound types
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let point: (f64, f64) = (3.0, 4.0)
let maybe_value: Option<i32> = Some(42)
```

#### Generics and Type Parameters

```nyar
# Generic function
micro identity<T>(value: T) -> T {
    value
}

# Generic type
type Container<T> = {
    value: T,
    metadata: String,
}

# Constrained generics
micro compare<T>(a: T, b: T) -> bool
where T: PartialEq
{
    a == b
}
```

#### Higher-Kinded Types (HKT)

```valkyrie
# Type constructor
type Functor<F> = {
    map: micro<A, B>(F<A>, micro(A) -> B) -> F<B>
}

# Monad pattern
type Monad<M> = {
    pure: micro<A>(A) -> M<A>,
    bind: micro<A, B>(M<A>, micro(A) -> M<B>) -> M<B>
}

# Option Monad implementation
impl Monad<Option> {
    pure(value) { Some { value } }
    
    bind(opt, f) {
        match opt {
            with [monad_bind];
            case Some(value): f(value)
            case None: None
        }
    }
}
```

#### Type Functions (mezzo)

```valkyrie
# Compile-time type calculation
mezzo Add<A, B>(a: A, b: B) -> Type {
    # Type-level addition
    match (a, b) {
        (Zero, n) => n,
        (Succ<m>, n) => Succ<Add<m, n>>
    }
}

# Conditional type selection
mezzo If<Condition, Then, Else>(cond: Condition) -> Type {
    if cond {
        Then
    } else {
        Else
    }
}

# Type validation
mezzo IsNumeric<T>(t: T) -> bool {
    match t {
        i8 | i16 | i32 | i64 | i128 => true,
        u8 | u16 | u32 | u64 | u128 => true,
        f32 | f64 => true,
        _ => false
    }
}
```

### 2. Pattern Matching System

#### Basic Pattern Matching

```valkyrie
# Value matching
match value {
    with [basic_match];
    case 0: "zero"
    case 1: "one"
    case 2: "two"
    else: "other"
}

# Range matching
match score {
    with [grade_calculation];
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case 60..=69: "D"
    else: "F"
}

# Multi-value matching
match day {
    with [weekend_check];
    case "Saturday" | "Sunday": "Weekend"
    case "Monday"..="Friday": "Weekday"
    else: "Invalid"
}
```

#### Destructuring Matching

```valkyrie
# Tuple destructuring
match point {
    with [coordinate_analysis];
    case (0, 0): "Origin"
    case (x, 0): f"On X-axis at {x}"
    case (0, y): f"On Y-axis at {y}"
    case (x, y) if x == y: f"Diagonal at {x}"
    case (x, y): f"Point at ({x}, {y})"
}

# Array destructuring
match array {
    with [array_pattern];
    case []: "Empty"
    case [x]: f"Single element: {x}"
    case [first, ..rest]: f"First: {first}, Rest: {rest.len()} items"
}

# Object destructuring
match person {
    with [person_info];
    case { name: "Alice", age }: f"Alice is {age} years old"
    case { name, age: 18..=25 }: f"Young adult: ${name}"
    case { name, age }: f"{name} is {age} years old"
}
```

#### Union Type Matching

```valkyrie
# Result type matching
match result {
    with [result_handling];
    case Fine { value }: f"Success: {value}"
    case Fail { error }: f"Error: {error}"
}

# Complex union type
union Expression {
    Literal { value: i32 },
    Variable { name: String },
    Binary {
        left: Expression,
        operator: String,
        right: Expression,
    },
}

match expr {
    with [expression_evaluation];
    case Literal(value): value
    case Variable(name): lookup_variable(name)
    case Binary { left, operator: "+", right }: {
        evaluate(left) + evaluate(right)
    }
    case Binary { left, operator: "*", right }: {
        evaluate(left) * evaluate(right)
    }
    else: 0
}
```

### 3. Functional Programming Features

#### Higher-Order Functions

```valkyrie
# Function as an argument
micro apply_twice<T>(f: micro(T) -> T, value: T) -> T {
    f(f(value))
}

# Function composition
micro compose<A, B, C>(f: micro(B) -> C, g: micro(A) -> B) -> micro(A) -> C {
    micro(x) { f(g(x)) }
}

# Currying
micro add(x: i32) -> micro(i32) -> i32 {
    micro(y) { x + y }
}

let add_five = add(5)
let result = add_five(10)  # Result is 15
```

#### Closures and Lambda Expressions

```valkyrie
# Basic closure
let square = micro(x) { x * x }
let add = micro(x, y) { x + y }

# Capture external variables
let multiplier = 3
let multiply_by_three = micro(x) { x * multiplier }

# Complex closure
let process_data = micro(data) {
    let cleaned = data.filter { $item.is_valid() }
    let transformed = cleaned.map { $item.transform() }
    transformed.reduce { $acc + $item }
}

# Trailing closure syntax
numbers.map { $x * $x }
    .filter { $x > 10 }
    .reduce { $acc + $x }
```

#### Recursion and Tail Recursion Optimization

```valkyrie
# Normal recursion
micro factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

# Tail recursion optimization
micro factorial_tail(n: i32, acc: i32 = 1) -> i32 {
    if n <= 1 {
        acc
    } else {
        factorial_tail(n - 1, n * acc)
    }
}

# Mutual recursion
micro is_even(n: i32) -> bool {
    if n == 0 {
        true
    } else {
        is_odd(n - 1)
    }
}

micro is_odd(n: i32) -> bool {
    if n == 0 {
        false
    } else {
        is_even(n - 1)
    }
}
```

### 4. Object-Oriented Programming

#### Class Definition and Inheritance

```valkyrie
# Basic class definition
class Animal {
    name: String
    age: i32
    
    new(name: String, age: i32) -> Self {
        Self { name, age }
    }
    
    speak(self) {
        print("${self.name} makes a sound")
    }
    
    get_info(self) -> String {
        "${self.name} is ${self.age} years old"
    }
}

# Inheritance
class Dog extends Animal {
    breed: String
    
    new(name: String, age: i32, breed: String) -> Self {
        Self {
            name,
            age,
            breed
        }
    }
    
    speak(self) {
        print("${self.name} barks: Woof!")
    }
    
    fetch(self, item: String) {
        print("${self.name} fetches the ${item}")
    }
}
```

#### Traits and Implementation

```valkyrie
$ Trait definition
trait Drawable {
    draw(self)
    get_area(self) -> f64
}

trait Comparable<T> {
    compare(self, other: T) -> i32
}

$ Trait implementation
class Circle {
    radius: f64
    
    new(radius: f64) -> Self {
        Self { radius }
    }
}

impl Drawable for Circle {
    draw(self) {
        print("Drawing circle with radius ${self.radius}")
    }
    
    get_area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
}

impl Comparable<Circle> for Circle {
    compare(self, other: Circle) -> i32 {
        if self.radius < other.radius {
            -1
        } else if self.radius > other.radius {
            1
        } else {
            0
        }
    }
}
```

### 5. Module System

#### Namespace Organization

```valkyrie
# Basic namespace
namespace math {
    let PI = 3.14159
    
    micro sin(x: f64) -> f64 {
        # Sine function implementation
        x  # Simplified implementation
    }
    
    micro cos(x: f64) -> f64 {
        # Cosine function implementation
        1.0 - x * x / 2.0  # Simplified implementation
    }
}

# Nested namespace
namespace graphics {
    namespace shapes {
        class Rectangle {
            width: f64
            height: f64
            
            new(width: f64, height: f64) -> Self {
                Self { width, height }
            }
            
            area(self) -> f64 {
                self.width * self.height
            }
        }
    }
    
    namespace colors {
        class RGB {
            r: u8
            g: u8
            b: u8
        }
        
        let RED: RGB = class { r: 255, g: 0, b: 0 }
let GREEN: RGB = class { r: 0, g: 255, b: 0 }
let BLUE: RGB = class { r: 0, g: 0, b: 255 }
    }
}
```

#### Import and Usage

```valkyrie
# Full import
using math.*

micro calculate_circle_area(radius: f64) -> f64 {
    math.PI * radius * radius
}

# Selective import
using math.{PI, sin, cos}
using graphics.shapes.Rectangle
using graphics.colors.{RED, GREEN, BLUE}

# Rename import
using graphics.shapes.Rectangle as Rect
using graphics.colors.RGB as Color

# Using imported content
micro create_colored_rectangle() -> (Rect, Color) {
    let rect = Rect::new(10.0, 20.0)
    let color = RED
    (rect, color)
}
```

### 6. Control Flow

#### Conditional Control

```valkyrie
# Basic condition
if condition {
    # Execute code
} else if other_condition {
    # Other condition
} else {
    # Default case
}

# Conditional expression
let result = if x > 0 {
    "positive"
} else if x < 0 {
    "negative"
} else {
    "zero"
}

# Guard condition
if let Some { value } = optional_value {
    print(f"Got value: {value}")
}
```

#### Loop Control

```valkyrie
# while loop
while condition {
    # Loop body
    if should_break {
        break
    }
    if should_continue {
        continue
    }
}

# for loop
for i in 0..10 {
    print(i)
}

for item in collection {
    process(item)
}

for (index, value) in collection.enumerate() {
    print(f"Index {index}: {value}")
}

# Infinite loop
loop {
    let input = get_input()
    if input == "quit" {
        break
    }
    process(input)
}

# Labeled loops
'outer: loop {
    'inner: for i in 0..10 {
        if should_break_outer {
            break 'outer
        }
        if should_continue_inner {
            continue 'inner
        }
    }
}
```

### 7. Error Handling

#### Exception System

```valkyrie
# Raising exceptions
micro validate_age(age: i32) {
    if age < 0 {
        raise "Age cannot be negative"
    }
    if age > 150 {
        raise "Age seems unrealistic"
    }
}

# Catching exceptions
try {
    validate_age(-5)
    risky_operation()
}
.catch {
    case error: String:
        print(f"String error: {error}")
    case error: NetworkError:
        print(f"Network error: {error.message}")
        retry_connection()
    case error:
        print(f"Unknown error: {error}")
}
```

#### Result Type

```valkyrie
# Using Result type
micro divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err { error: "Division by zero" }
    } else {
        Fine { value: a / b }
    }
}

# Chained error handling
let result = divide(10.0, 2.0)
    .map { $value * 2.0 }
    .and_then { $value -> 
        if $value > 100.0 {
            Err { error: "Value too large" }
        } else {
            Ok { value: $value }
        }
    }

match result {
    with [error_handling];
    case Fine { value }: print(f"Result: {value}")
    case Fail { error }: print(f"Error: {error}")
}
```

### 8. Metaprogramming

#### Macro System

```valkyrie
# Simple macro definition
macro debug_print($expr) {
    @cfg(debug_assertions)
    print("DEBUG: {} = {}", stringify!($expr), $expr)
}

# Using macro
debug_print!(x + y)
# Expands to: print("DEBUG: x + y = {}", x + y)

# Complex macro
macro create_class($name, $($field:$type),*) {
    class $name {
        $($field: $type,)*
        
        new($($field: $type),*) -> Self {
            Self {
                $($field,)*
            }
        }
    }
}

# Using complex macro
create_class!(Person, name: String, age: i32)
```

#### Compile-time Calculation

```valkyrie
# Compile-time constant
@const_eval
micro fibonacci_const(n: i32) -> i32 {
    if n <= 1 {
        n
    } else {
        fibonacci_const(n - 1) + fibonacci_const(n - 2)
    }
}

let fib_10 = fibonacci_const(10)  # Calculated at compile-time

# Compile-time type generation
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64
    y: f64
}
```

## Advanced Features

### 1. Memory Management

```valkyrie
# Garbage Collection
let data = allocate_large_data()  # Automatically managed memory
# Automatically reclaimed when data leaves scope

# Reference Counting
let shared_data = Rc::new(expensive_data())
let reference1 = shared_data.clone()
let reference2 = shared_data.clone()
# Automatically released when all references leave scope
```

### 2. Concurrency and Async

```valkyrie
# Async function
async micro fetch_data(url: String) -> Result<String, Error> {
    let response = http_client.get(url).await?
    Fine { value: response.text().await? }
}

# Concurrent execution
async micro process_urls(urls: [String]) -> [Result<String, Error>] {
    let futures = urls.map { $url -> fetch_data($url) }
    futures.join_all().await
}
```

### 3. Performance Optimization

```valkyrie
# Inline optimization
@inline
micro fast_add(a: i32, b: i32) -> i32 {
    a + b
}

# Specialization optimization
@specialize
micro generic_sort<T>(data: [T]) -> [T]
where T: Ord
{
    # Generate optimized versions for each concrete type
    data.sort()
}

# Zero-cost abstraction
let result = numbers
    .iter()
    .map { $x * $x }
    .filter { $x > 100 }
    .collect()
# Compiled equivalent to a hand-written loop
```

## Summary

The Valkyrie language provides a rich set of features:

1. **Type Safety**: Powerful static type system, catching errors at compile-time
2. **Expressiveness**: Advanced features like pattern matching, higher-kinded types, and type functions
3. **Functional**: Higher-order functions, closures, immutability, and other functional programming traits
4. **Object-Oriented**: Support for classes, inheritance, traits, etc.
5. **Modularity**: Flexible namespace and import system
6. **Metaprogramming**: Macro system and compile-time calculation
7. **Performance**: Zero-cost abstractions and compile-time optimizations
8. **Security**: Memory safety and error handling mechanisms

These features enable Valkyrie to adapt to various programming scenarios, from systems programming to application development, and from functional to object-oriented programming, providing an excellent development experience and runtime performance.
