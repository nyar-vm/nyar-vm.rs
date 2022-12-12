# Macro System

## Overview

Valkyrie provides a powerful macro system that supports compile-time code generation and metaprogramming. The macro system is divided into two main parts:

- **Macro (`@`)**: Compile-time function calls that do not capture subsequent parameters.
- **Annotation (`@`)**: Compile-time annotations that capture and act on subsequent declarations like `class`, `micro`, etc.

## Macro vs Annotation

### Macro (`@`)

Macros use the `@` prefix and are compile-time function calls that do not capture subsequent code elements:

```valkyrie
# Compile-time constant computation
let FIBONACCI_10: i32 = @evaluate(fibonacci(10))
let LOOKUP_TABLE: [i32; 256] = @evaluate(generate_lookup_table())

# Environment variable access
let database_url: string = @env("DATABASE_URL")

# String formatting
let message: string = @format("Hello, {}!", name)

# Vector creation
let numbers = @vec(1, 2, 3, 4, 5)
let zeros = @vec(0; 10)

# SQL query
let query = @sql(
    "SELECT id, name FROM users WHERE active = $1",
    true
)
```

### Annotation (`@`)

Annotations use the `@` prefix and capture and act on subsequent declarations:

```valkyrie
# Test annotation
@test
micro test_addition() {
    @assert_eq(2 + 2, 4)
}

# Serialization annotation
@derive(Serialize, Deserialize)
class User {
    name: string
    email: string
}

# Benchmark annotation
@benchmark
micro fibonacci_benchmark() {
    fibonacci(30)
}

# Conditional compilation annotation
@cfg(feature = "debug")
micro debug_function() {
    print("Debug mode enabled")
}
```

## Common Macros

### Compile-time Computation

```valkyrie
# Compile-time constant computation
let PI_SQUARED: f64 = ↯evaluate(3.14159 * 3.14159)

# Compile-time file reading
let config_content: string = ↯compile_time_read_file("config.toml")

# Compile-time environment configuration
↯compile_time_env {
    memory_limit: "256MB",
    execution_timeout: "30s",
}
```

### Code Generation

```valkyrie
# Template definition
↯template {
    name: "crud_operations",
    params: [Entity: Type, Key: Type],
    body: {
        micro create(entity: Entity) -> Result⟨Key, Any⟩ {
            # Generic logic for creating an entity
        }
        
        micro read(key: Key) -> Result⟨Entity, Any⟩ {
            # Generic logic for reading an entity
        }
        
        micro update(key: Key, entity: Entity) -> Result⟨unit, Any⟩ {
            # Generic logic for updating an entity
        }
        
        micro delete(key: Key) -> Result⟨unit, Any⟩ {
            # Generic logic for deleting an entity
        }
    }
}

# Template instantiation
↯generate_code {
    crud_operations⟨User, UserId⟩
    crud_operations⟨Product, ProductId⟩
}
```

### Macro Expansion Control

```valkyrie
# Macro expansion strategy control
↯macro_expansion(strategy: "eager", max_depth: 100)
macro recursive_macro {
    # Recursive macro definition
}
```

## Common Annotations

### Testing

```valkyrie
@test
micro test_user_creation() {
    let user = User("Alice", "alice↯example.com")
    @assert_true(user.is_valid())
    @assert_eq(user.name, "Alice")
}

@test
@should_panic
micro test_invalid_email() {
    User("Bob", "invalid-email")
}
```

### Derivation

```valkyrie
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64,
    y: f64,
}

@derive(Serialize, Deserialize)
class Config {
    database_url: string
    port: u16
}
```

### Conditional Compilation

```valkyrie
@cfg(target_os = "windows")
micro windows_specific_function() {
    # Windows-specific implementation
}

@cfg(feature = "async")
class AsyncHandler {
    # Async handler implementation
}
```

## Custom Macros

### Declarative Macros

```valkyrie
macro vec_of {
    (#elem:expr; #n:expr) => {
        {
            let mut v = []
            for _ in 0..#n {
                v.push(#elem)
            }
            v
        }
    }
    (#(#x:expr),+ #(,)?) => {
        @vec(#(#x),+)
    }
}
```

### Procedural Macros

Procedural macros are more powerful metaprogramming tools that allow direct manipulation of AST or TokenStream. For more details, please refer to the [Procedural Macro Development Guide](./procedural-macros.md).
