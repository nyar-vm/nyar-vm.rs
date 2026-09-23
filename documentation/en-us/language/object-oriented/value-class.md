# Value Classes and Immutability

Value classes are a modeling approach centered around the "immutable representation of values". It emphasizes that once an object is created, its observable state no longer changes; any "modification" is expressed by creating a new instance. Value classes help improve concurrency safety, reasonability, and test-friendliness.

## Why Choose Immutability

- Simplify Concurrency: Immutable objects can be safely shared between threads/coroutines without complex synchronization.
- Easier to Reason: State won't be accidentally changed by external factors, making behavior more predictable.
- Facilitate Caching and Reuse: Same input yields same output, allowing for safe caching or structural sharing.
- Test-Friendly: No need to construct various intermediate states; focus only on input and output.

## Basic Modeling Patterns

- Expose only read-only views (read-only properties/accessors).
- Set internal state once through constructors or factory methods.
- Produce new instances via "copy and modify" (the `with` pattern), rather than in-place modification.

## Example: Geometric Point (Read-only properties + Derived new values)

```valkyrie
class Point {
    x: f64,
    y: f64,
    
    micro new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    
    # Read-only accessors (can also be represented with property syntax)
    micro get_x(self) -> f64 { self.x }
    micro get_y(self) -> f64 { self.y }

    # Produce a new value: translate without modifying the original object
    micro translate(self, dx: f64, dy: f64) -> Self {
        Point { x: self.x + dx, y: self.y + dy }
    }
}

let p1 = Point::new(1.0, 2.0)
let p2 = p1.translate(3.0, 4.0)  # p1 remains unchanged, p2 is a new value
```

## Example: Copy-Update for Named Fields (with pattern)

```valkyrie
class Person {
    name: string,
    age: i32,

    micro new(name: string, age: i32) -> Self {
        Person { name, age }
    }

    # Copy and modify fields as needed
    micro with(self, name?: string, age?: i32) -> Self {
        Person {
            name: name ?? self.name,
            age: age ?? self.age,
        }
    }
}

let alice = Person::new("Alice", 20)
let older = alice.with(age: 21)   # Only age changes, name is reused
```

## Construction and Validation

Value classes often complete integrity validation during construction to ensure the object is always in a valid state:

```valkyrie
class Email {
    address: string,

    micro new(address: string) -> Self {
        @require(is_valid_email(address), "Invalid email format")
        Email { address }
    }
}
```

## Equality and Hashing

Value classes emphasize "content-based" equality. When implementing equality/hashing, it should only be based on observable fields:

```valkyrie
class RGB {
    r: u8, g: u8, b: u8,

    micro equals(self, other: &RGB) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b
    }

    micro hash(self) -> i64 {
        ((self.r as i64) << 16) | ((self.g as i64) << 8) | (self.b as i64)
    }
}
```

## Interaction with Properties
