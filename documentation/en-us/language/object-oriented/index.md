
## Object-Oriented Programming

Valkyrie supports class-based object-oriented programming, providing features such as class definitions, constructors, methods, and inheritance.

### Special Class Types

- [Neural Network Type (Neural)](./neural.md) - Special class type for machine learning
- [Widget Component Type (Widget)](./widget.md) - Special class type for UI development

### Field Definitions

```valkyrie
# Basic field definitions
name: string
age: i32
is_active: bool = true  # Default value

# Access control
public username: string
private password: string
protected internal_id: i64

# Read-only fields
readonly created_at: DateTime
```

### Class Definitions (class)

Use the `class` keyword to define structured data types.

```valkyrie
class Person {
    name: string
    age: i32
    
    micro constructor(self, name: string, age: i32) {
        self.name = name
        self.age = age
    }

    micro greet(self) {
        print("Hello, I'm ${self.name}")
    }
}
```

### Method Definitions

```valkyrie
imply Person {
    # Instance method
    micro say_hello(self) {
        print("Hello, I'm ${self.name}")
    }

    # Mutable method
    micro set_age(mut self, new_age: i32) {
        self.age = new_age
    }

    # Static method
    micro static create_anonymous() -> Person {
        Person { name: "Anonymous", age: 0 }
    }

    # Method with return value
    micro get_info(self) -> string {
        "${self.name} is ${self.age} years old"
    }
}
```

### Inheritance

Classes can inherit from one or more classes by adding parentheses after the class name.

```valkyrie
class Student(Person) {
    student_id: string
}
```


## Flags (flags)

### Basic Flags

```valkyrie
# Simple flags
flags FilePermissions {
    READ = 1,
    WRITE = 2,
    EXECUTE = 4
}

# Using flags
let perms = FilePermissions::READ | FilePermissions::WRITE
if perms.contains(FilePermissions::READ) {
    print("Readable")
}

# Complex flags
flags WindowStyle {
    RESIZABLE = 0x01,
    MINIMIZABLE = 0x02,
    MAXIMIZABLE = 0x04,
    CLOSABLE = 0x08,
    TITLEBAR = 0x10,
    BORDER = 0x20,
    
    # Combined flags
    DEFAULT = RESIZABLE | MINIMIZABLE | MAXIMIZABLE | CLOSABLE | TITLEBAR | BORDER,
    DIALOG = CLOSABLE | TITLEBAR | BORDER
}
```

### Flag Operations

```valkyrie
flags Permissions {
    READ = 1,
    write = 2,
    execute = 4,
    
    # Methods
    micro has_read(self) -> bool {
        self.contains(Permissions::read)
    }
    
    micro add_write(mut self) {
        self |= Permissions::write
    }
    
    micro remove_execute(mut self) {
        self &= !Permissions::execute
    }
}
```

## Traits (trait)

### Basic Traits

```valkyrie
# Simple trait
trait Display {
    micro to_string(self) -> string
}

# Trait with default implementation
trait Debug {
    micro debug(self) -> string
    
    # Default implementation
    micro print_debug(self) {
        print(self.debug())
    }
}

# Generic trait
trait Iterator<T> {
    micro next(mut self) -> T?
    
    # Default methods
    micro collect(mut self) -> [T] {
        let mut result = []
        while let item = self.next()? {
            result.push(item)
        }
        result
    }
    
    micro map<U>(self, f: micro(T) -> U) -> MapIterator<T, U> {
        MapIterator::new(self, f)
    }
}
```

### Trait Implementation

```valkyrie
# Implementing a trait for a type
imply Person: Display {
    micro to_string(self) -> string {
        "${self.name} (${self.age} years old)"
    }
}

imply Person: Debug {
    micro debug(self) -> string {
        "Person { name: \"${self.name}\", age: ${self.age} }"
    }
}

# Conditional implementation
imply<T> T?: Display where T: Display {
    micro to_string(self) -> string {
        match self {
            value? => "Some(${value.to_string()})",
            _ => "None"
        }
    }
}
```

### Trait Constraints

```valkyrie
# Trait constraints in functions
micro print_items<T>(items: [T]) where T: Display {
    for item in items {
        print(item.to_string())
    }
}

# Multiple constraints
micro process<T>(value: T) -> string 
where 
    T: Display + Debug + Clone 
{
    let cloned = value.clone()
    "Display: ${value.to_string()}, Debug: ${cloned.debug()}"
}

# Associated types
trait Collect<T> {
    type Output
    
    micro collect(self) -> Self::Output
}
```

## Type Aliases

```valkyrie
# Simple type aliases
type UserId = i64
type UserName = string
type Coordinates = (f64, f64)

# Generic type aliases
type Result<T> = Result<T, string>
type HashMap<K, V> = std::collections::HashMap<K, V>

# Function type aliases
type Handler = micro(Request) -> Response
type Predicate<T> = micro(T) -> bool
```

## Constants

```valkyrie
# Basic constants
const PI: f64 = 3.14159265359
const MAX_USERS: i32 = 1000
const APP_NAME: string = "MyApp"

# Complex constants
const DEFAULT_CONFIG: Config = Config {
    timeout: 30,
    retries: 3,
    debug: false
}

# Computed constants
const BUFFER_SIZE: usize = 1024 * 1024  # 1MB
const HALF_PI: f64 = PI / 2.0
```

## Modules

```valkyrie
# Module declaration
mod utils {
    public micro helper_function() {
        # Implementation
    }
    
    public class UtilityClass {
        # Implementation
    }
}

# Using modules
using utils::helper_function
using utils::UtilityClass

# Re-exporting
public using utils::*
```

## Generics

```valkyrie
# Generic function
micro swap<T>(mut a: T, mut b: T) {
    let temp = a
    a = b
    b = temp
}

# Generic class
class Container<T> {
    value: T
    
    new(value: T) {
        Self { value }
    }
    
    get(self) -> T {
        self.value
    }
    
    set(mut self, new_value: T) {
        self.value = new_value
    }
}

# Constrained generics
class SortedList<T> where T: Ord {
    items: [T]
    
    insert(mut self, item: T) {
        # Maintain sorted insertion
        let pos = self.items.binary_search(item).default { $e }
        self.items.insert(pos, item)
    }
}
```


## Attributes and Decorators

```valkyrie
# Attribute decorators
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64
    y: f64
}

@test
micro test_addition() {
    ↯assert_equal(2 + 2, 4)
}

@deprecated("Use new_function instead")
micro old_function() {
    # Deprecated function
}

@inline
micro fast_calculation(x: i32) -> i32 {
    x * x + 2 * x + 1
}
```
