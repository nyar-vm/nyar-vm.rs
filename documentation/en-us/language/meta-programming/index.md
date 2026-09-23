# Valkyrie Metaprogramming Architecture

Valkyrie provides powerful metaprogramming support, allowing for code generation, transformation, and analysis at compile time. Through the integrated metaprogramming system in the compiler, Valkyrie can achieve advanced capabilities such as macro systems, compile-time computation, and type-level programming.

## Metaprogramming Architecture Overview

### Positioning of Metaprogramming in the Compiler

Valkyrie's metaprogramming system is deeply integrated within the multi-layer IR architecture, providing corresponding capabilities at different levels:

```
Source Code + Metaprogramming Instructions
         ↓
    AST + Macro Expansion
         ↓
    HIR + Compile-time Computation
         ↓
    MIR + Code Optimization
         ↓
    LIR + Platform Specialization
         ↓
Target Code + Runtime Support
```

## Core Metaprogramming Features

### [Compile-time Computation](./compile-time-computation.md)

**Constant Expression Evaluation**:
```valkyrie
// Compile-time constant computation
let FIBONACCI_10: i32 = @evaluate(fibonacci(10))
let LOOKUP_TABLE: [i32; 256] = @evaluate(generate_lookup_table())

// Compile-time string processing
let CONFIG_KEY: string = @evaluate(@format("app.{}.version", @env("BUILD_TARGET")))
```

**Compile-time Function Execution**:
```valkyrie
// Marked as a compile-time function
@const_fn
micro fibonacci(n: i32) -> i32 {
    match n {
        case 0 | 1: n
        case _: fibonacci(n - 1) + fibonacci(n - 2)
    }
}

// Compile-time data structure operations
@const_fn
micro build_state_machine() -> StateMachine {
    let mut sm = StateMachine()
    sm.add_state("start")
    sm.add_state("processing")
    sm.add_state("end")
    sm.add_transition("start", "process", "processing")
    sm.add_transition("processing", "finish", "end")
    sm
}
```

### [Macro System](./macro-system.md)

**Declarative Macros**:
```valkyrie
// Pattern matching macro
macro vec_of {
    (#elem:expr; #n:expr) => {
        {
            let mut v = []
            for _ in 0..<#n {
                v.push(#elem)
            }
            v
        }
    }
    (#(#x:expr),+ #(,)?) => {
        @vec(#(#x),+)
    }
}

// Usage example
let zeros = @vec_of(0; 10)
let numbers = @vec_of(1, 2, 3, 4, 5)
```

**Procedural Macros**:
```valkyrie
// Custom derive macro
@derive(Serialize, Deserialize, Debug)
class User {
    id: u64,
    name: string,
    email: string,
}

// Attribute macro
@api_endpoint(method: "GET", path: "/users/{id}")
micro get_user(id: u64) -> Result⟨User, ApiError⟩ {
    # Automatically generate routing registration and parameter validation code
    database::find_user(id)
}

// Functional macro
let sql_query = @sql(
    "SELECT id, name, email FROM users WHERE active = $1",
    true
)
```

### [Code Generation](./code-generation.md)

**Template-based Code Generation**:
```valkyrie
// Template definition
@template {
    name: "crud_operations",
    params: [Entity: Type, Key: Type],
    body: {
        impl CrudOperations⟨{{Key}}⟩ for {{Entity}} {
            micro create(data: {{Entity}}) -> Result⟨{{Key}}, Any⟩ {
                // Generate creation logic
            }
            
            micro read(id: {{Key}}) -> Result⟨{{Entity}}, Any⟩ {
                // Generate reading logic
            }
            
            micro update(id: {{Key}}, data: {{Entity}}) -> Result⟨unit, Any⟩ {
                // Generate update logic
            }
            
            micro delete(id: {{Key}}) -> Result⟨unit, Any⟩ {
                // Generate deletion logic
            }
        }
    }
}

// Template instantiation
@generate_code {
    crud_operations⟨User, UserId⟩
    crud_operations⟨Product, ProductId⟩
    crud_operations⟨Order, OrderId⟩
}
```

**Reflection-driven Code Generation**:
```valkyrie
// Automatically generate serialization code
@auto_serialize
class Config {
    database_url: string,
    port: u16,
    debug: bool,
}

// Compile-time generated code
impl Serialize for Config {
    micro serialize(self) -> SerializedData {
        let mut data = SerializedData()
        data.insert("database_url", self.database_url)
        data.insert("port", self.port)
        data.insert("debug", self.debug)
        data
    }
}
```

### [Type-level Programming](./type-level-programming.md)

**Type-level Functions**:
```valkyrie
// Type-level computation
type Add(a: Nat, b: Nat) -> Nat {
    Add(Zero, b) = b,
    Add(Succ(a), b) = Succ(Add(a, b))
}

// Type-level list operations
type Length(list: [T]) -> Nat {
    Length(Nil) = Zero,
    Length(Cons(_, tail)) = Succ(Length(tail))
}

// Compile-time type validation
micro safe_array_access⟨const N: usize, const I: usize⟩(arr: [i32; N]) -> i32 
where
    Assert⟨LessThan⟨I, N⟩⟩: True
{
    arr[I]  // Guaranteed index safety at compile time
}
```

**Dependent Type Support**:
```valkyrie
// Length-dependent vector type
class Vector⟨T, const N: usize⟩ {
    data: [T; N]
}

impl⟨T, const N: usize⟩ Vector⟨T, N⟩ {
    micro push⟨const M: usize⟩(self, item: T) -> Vector⟨T, {N + 1}⟩ {
        // Type-level guarantee of length correctness
    }
    
    micro concat⟨const M: usize⟩(self, other: Vector⟨T, M⟩) -> Vector⟨T, {N + M}⟩ {
        // Compile-time calculation of result length
    }
}
```
