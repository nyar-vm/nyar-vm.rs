# Trait System

## Overview

Valkyrie's Trait system provides a powerful abstraction mechanism, supporting interface definitions, default implementations, multiple inheritance, and anonymous traits. The Trait system is a core component of object-oriented programming in Valkyrie.

## Basic Trait Definition

Valkyrie uses the `trait` keyword to define interfaces.

### Simple Trait

```valkyrie
trait Display {
    micro fmt(self) -> string
}

trait Clone {
    micro clone(self) -> Self
}

trait Debug {
    micro debug_fmt(self) -> string {
        # Default implementation
        ↯format("{}↯{:p}", self.type_name(), &self)
    }
}
```

### Trait with Associated Types

```valkyrie
trait Iterator {
    type Item
    
    micro next(mut self) -> Self::Item?
    
    micro collect⟨C: FromIterator⟨Self::Item⟩⟩(self) -> C {
        C::from_iter(self)
    }
}

trait FromIterator⟨T⟩ {
    micro from_iter⟨I: Iterator⟨Item = T⟩⟩(iter: I) -> Self
}
```

### Trait with Constraints

```valkyrie
trait PartialEq⟨Rhs = Self⟩ {
    micro eq(self, other: &Rhs) -> bool
    
    micro ne(self, other: &Rhs) -> bool {
        !self.eq(other)
    }
}

trait Ord: PartialEq + PartialOrd {
    micro cmp(self, other: &Self) -> Ordering
}
```

## Trait Implementation (imply)

Valkyrie uses the `imply` keyword to implement Traits for specific types.

### Basic Implementation

```valkyrie
class Point {
    x: f64,
    y: f64,
}

imply Point: Display {
    micro fmt(self, f: mut Formatter) -> Result {
        f.write(↯format("({}, {})", self.x, self.y))
    }
}

imply Point: Clone {
    micro clone(self) -> Self {
        Point { x: self.x, y: self.y }
    }
}
```

### Generic Implementation

Generics use mathematical angle brackets `⟨ ⟩`.

```valkyrie
imply⟨T: Display⟩ [T]: Display {
    micro fmt(self) -> string {
        let items = self.iter()
            .map { $item.fmt() }
            .collect::⟨[string]⟩()
            .join(", ")
        ↯format("[{}]", items)
    }
}

imply⟨T: Clone⟩ [T]: Clone {
    micro clone(self) -> Self {
        self.iter().map { $item.clone() }.collect()
    }
}
```

### Conditional Implementation

```valkyrie
imply⟨T: PartialEq⟩ [T]: PartialEq {
    micro eq(self, other: Self) -> bool {
        self.len() == other.len() && 
        self.iter().zip(other.iter()).all { $a.eq($b) }
    }
}
```

## Anonymous Trait

Valkyrie supports anonymous traits, which can be defined directly in function parameters:

```valkyrie
# Anonymous trait as a parameter
micro process_drawable(drawable: ftrait {
    micro draw(self)
    micro get_bounds(self) -> Rectangle
}) {
    let bounds = drawable.get_bounds()
    print("Drawing object with bounds: {}", bounds)
    drawable.draw()
}

# Using an anonymous trait
let circle = class {
    radius: f64,
    
    micro draw(self) {
        print("Drawing circle with radius {}", self.radius)
    }
    
    micro get_bounds(self) -> Rectangle {
        Rectangle::new(-self.radius, -self.radius, 
                      self.radius * 2, self.radius * 2)
    }
}

process_drawable(circle { radius: 5.0 })
```

### Anonymous Trait Inheritance

```valkyrie
# Anonymous trait inheriting from existing traits
micro handle_serializable(obj: ftrait(Display, Clone) {
    micro serialize(self) -> string
}) {
    print("Object: {}", obj.fmt())
    let cloned = obj.clone()
    let serialized = obj.serialize()
    print("Serialized: {}", serialized)
}
```

## Trait Objects

### Dynamic Dispatch

```valkyrie
trait Animal {
    micro make_sound(self)
    micro name(self) -> string
}

class Dog {
    name: string,
}

imply Dog: Animal {
    micro make_sound(self) {
        print("Woof!")
    }
    
    micro name(self) -> string {
        self.name.clone()
    }
}

class Cat {
    name: string,
}

imply Cat: Animal {
    micro make_sound(self) {
        print("Meow!")
    }
    
    micro name(self) -> string {
        self.name.clone()
    }
}

# Using trait objects
let animals: [Animal] = [
    Dog { name: "Buddy" },
    Cat { name: "Whiskers" },
]

for animal in animals {
    print("{} says:", animal.name())
    animal.make_sound()
}
```

# Trait Object Safety

```valkyrie
# Object-safe trait
trait Draw {
    micro draw(self)  # Receives self, object-safe
}

# Non-object-safe trait
trait Clone {
    micro clone(self) -> Self  # Returns Self, not object-safe
}

# Using where clause restrictions
trait Container {
    type Item
    
    micro get(self, index: usize) -> Self::Item?
    
    # Only callable when Self::Item implements Display
    micro display_item(self, index: usize) 
    where Self::Item: Display {
        if let item = self.get(index)? {
            print("{}", item.fmt())
        }
    }
}
```

## Advanced Features

### Associated Constants

```valkyrie
trait MathConstants {
    const PI: f64 = 3.14159265359
    const E: f64 = 2.71828182846
    
    micro circle_area(radius: f64) -> f64 {
        Self::PI * radius * radius
    }
}

class Calculator {}

imply Calculator: MathConstants {}

let area = Calculator::circle_area(5.0)
```

### Higher-Rank Trait Bounds

```valkyrie
# Higher-rank trait bounds
micro map_closure⟨F, T, U⟩(items: [T], f: F) -> [U]
where
    F: for⟨'a⟩ Fn(T) -> U,
{
    items.iter().map(f).collect()
}

# Usage example
let numbers = [1, 2, 3, 4, 5]
let doubled = map_closure(numbers, micro(x) { x * 2 })
```

### Trait Alias

```valkyrie
# Define a trait alias
trait Printable = Display + Debug + Clone

# Using a trait alias
micro print_info⟨T: Printable⟩(item: T) {
    print("Display: {}", item.fmt())
    print("Debug: {}", item.debug_fmt())
    let cloned = item.clone()
    print("Cloned: {}", cloned.fmt())
}
```

## Derive Macros

Valkyrie provides macros to automatically derive common traits:

```valkyrie
@derive(Debug, Clone, PartialEq, Eq, Hash)
class User {
    id: u64,
    name: string,
    email: string,
}

@derive(Display)
class Point {
    x: f64,
    y: f64,
}

# Customizing derive behavior
@derive(Debug, Clone)
@derive_display(format = "User({})", field = "name")
class SimpleUser {
    name: string,
    internal_id: u64,  # Won't be shown in Display
}
```

## Best Practices

### 1. Trait Design Principles

```valkyrie
# Good design: Single Responsibility
trait Readable {
    micro read(mut self, buffer: mut [u8]) -> Result⟨usize, Error⟩
}

trait Writable {
    micro write(self, data: [u8]) -> Result⟨usize, Error⟩
}

# Combined usage
trait ReadWrite: Readable + Writable {}
```

### 2. Associated Types vs Generic Parameters

```valkyrie
# Using associated types: only one implementation per type
trait Iterator {
    type Item
    micro next(mut self) -> Self::Item?
}

# Using generic parameters: can have multiple implementations
trait From⟨T⟩ {
    micro from(value: T) -> Self
}

# string can be converted from multiple types
imply string: From⟨string⟩ { ... }
imply string: From⟨char⟩ { ... }
imply string: From⟨[char]⟩ { ... }
```

### 3. Error Handling

```valkyrie
trait TryFrom⟨T⟩ {
    type Error
    
    micro try_from(value: T) -> Result⟨Self, Self::Error⟩
}

trait TryInto⟨T⟩ {
    type Error
    
    micro try_into(self) -> Result⟨T, Self::Error⟩
}

# Automatic implementation
imply⟨T, U⟩ T: TryInto⟨U⟩ 
where U: TryFrom⟨T⟩ {
    type Error = U::Error
    
    micro try_into(self) -> Result⟨U, Self::Error⟩ {
        U::try_from(self)
    }
}
```

## Summary

Valkyrie's trait system provides:

1. **Flexible Abstraction**: Define behavior interfaces through traits
2. **Code Reuse**: Through default implementations and generics
3. **Type Safety**: Compile-time trait bound checks
4. **Dynamic Dispatch**: Runtime polymorphism via trait objects
5. **Anonymous Traits**: Support for ad-hoc behavior definitions
6. **Composition**: Combine multiple capabilities via trait bounds
