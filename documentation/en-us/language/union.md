# Union Types

Union types are a powerful feature of Valkyrie's type system for representing multiple possible values. They allow a value to choose one from several different variants, each of which can carry different types of data.

## Basic Union Types

### Simple Union Types

```valkyrie
# Result type - representing operations that may succeed or fail
union Result⟨T, E⟩ {
    Fine { value: T },
    Fail { error: E }
}

# Option type - representing values that may or may not exist
union Option⟨T⟩ {
    Some { value: T },
    None
}

# Boolean wrapper type
union Bool {
    True,
    False
}
```

### Complex Union Types

```valkyrie
# JSON value type
union JsonValue {
    Null,
    Bool { value: bool },
    Number { value: f64 },
    String { value: string },
    Array { items: [JsonValue] },
    Object { fields: {string: JsonValue} }
}

# Expression abstract syntax tree
union Expression {
    Literal { value: i32 },
    Variable { name: string },
    Binary {
        left: Expression,
        operator: string,
        right: Expression
    }
}
```

## Using Union Types

### Pattern Matching

```valkyrie
# Basic pattern matching
let result: Result⟨i32, string⟩ = Fine { value: 42 }
match result {
    case Fine { value }: print("Success: ${value}")
    case Fail { error }: print("Failure: ${error}")
}

# Nested pattern matching
let nested: Result⟨Option⟨i32⟩, string⟩ = Fine { value: Some { value: 42 } }
match nested {
    case Fine { value: Some { value } }: print("Value: ${value}")
    case Fine { value: None }: print("No value")
    case Fail { error }: print("Error: ${error}")
}
```

### if let Expression

```valkyrie
# Simplified pattern matching
if let Fine { value } = result {
    print("Successfully obtained value: ${value}")
}

# With else branch
if let Some { value } = option {
    process(value)
} else {
    print("Option is empty")
}
```

## Union Type Methods

### Associated Methods

```valkyrie
union Result⟨T, E⟩ {
    Fine { value: T },
    Fail { error: E },
    
    # Check if successful
    micro is_fine(self) -> bool {
        match self {
            case Fine { .. }: true
            case Fail { .. }: false
        }
    }
    
    # Get the value (throws an exception or crashes if failure)
    micro unwrap(self) -> T {
        if let Fine { value } = self {
            value
        } else {
            panic("Called unwrap on Fail")
        }
    }
    
    # Safely get the value
    micro unwrap_or(self, default: T) -> T {
        if let Fine { value } = self {
            value
        } else {
            default
        }
    }
    
    # Map success value
    micro map⟨U⟩(self, f: micro(T) -> U) -> Result⟨U, E⟩ {
        if let Fine { value } = self {
            Fine { value: f(value) }
        } else if let Fail { error } = self {
            Fail { error }
        }
    }
    
    # Map error value
    micro map_err⟨F⟩(self, f: micro(E) -> F) -> Result⟨T, F⟩ {
        if let Fine { value } = self {
            Fine { value }
        } else if let Fail { error } = self {
            Fail { error: f(error) }
        }
    }
}

### Option Type Methods

```valkyrie
union Option⟨T⟩ {
    Some { value: T },
    None,
    
    # Check if value exists
    micro is_some(self) -> bool {
        if let Some { .. } = self {
            true
        } else {
            false
        }
    }
    
    # Check if empty
    micro is_none(self) -> bool {
        if let None = self {
            true
        } else {
            false
        }
    }
    
    # Map value
    micro map⟨U⟩(self, f: micro(T) -> U) -> Option⟨U⟩ {
        if let Some { value } = self {
            Some { value: f(value) }
        } else {
            None
        }
    }
    
    # Filter value
    micro filter(self, predicate: micro(T) -> bool) -> Option⟨T⟩ {
        if let Some { value } = self {
            if predicate(value) {
                Some { value }
            } else {
                None
            }
        } else {
            None
        }
    }
}
```

## Advanced Features

### Generic Union Types

```valkyrie
# Multi-parameter generics
union Either⟨L, R⟩ {
    Left { value: L },
    Right { value: R }
}

# Generics with constraints
union Container⟨T⟩ where T: Clone {
    Single { item: T },
    Multiple { items: [T] }
}
```

### Recursive Union Types

```valkyrie
# Linked List
union List⟨T⟩ {
    Empty,
    Node {
        value: T,
        next: List⟨T⟩
    }
}

# Binary Tree
union Tree⟨T⟩ {
    Leaf { value: T },
    Branch {
        left: Tree⟨T⟩,
        right: Tree⟨T⟩
    }
}
```

## Best Practices

### 1. Use Descriptive Variant Names

```valkyrie
# Good naming
union HttpResponse {
    Success { data: string, status: u16 },
    ClientError { message: string, code: u16 },
    ServerError { message: string, code: u16 },
    NetworkError { reason: string }
}

# Avoid over-simplified naming
union Bad {
    A { x: i32 },
    B { y: string }
}
```

### 2. Rational Use of Field Naming

```valkyrie
# When there is only one field, use value
union Option⟨T⟩ {
    Some { value: T },
    None
}

# Use descriptive names for multiple fields
union Person {
    Student { name: string, grade: i32 },
    Teacher { name: string, subject: string }
}
```

### 3. Provide Convenience Methods

```valkyrie
union ValidationResult⟨T⟩ {
    Valid { data: T },
    Invalid { errors: [string] },
    
    # Convenience method
    micro is_valid(self) -> bool {
        match self {
            case Valid { .. }: true
            case Invalid { .. }: false
        }
    }
    
    micro get_errors(self) -> [string] {
        if let Invalid { errors } = self {
            errors
        } else {
            []
        }
    }
}
```
```
