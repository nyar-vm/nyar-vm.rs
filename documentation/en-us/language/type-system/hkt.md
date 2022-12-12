# Higher-Kinded Types (HKT)

Higher-Kinded Types (HKT) are an advanced feature of the Valkyrie type system that allows for abstraction over type constructors, enabling more powerful generic programming patterns.

## Basic Concepts

### Kinds of Types

In Valkyrie, types have different "kinds":

```valkyrie
# Kind *: Concrete types
let x: i32        # The kind of i32 is *
let y: string     # The kind of string is *

# Kind * -> *: Unary type constructors
type []           # The kind of [] is * -> *
type ?            # The kind of ? is * -> *

# Kind * -> * -> *: Binary type constructors
type Result⟨T, E⟩ # The kind of Result is * -> * -> *
type {K: V}       # The kind of {K: V} is * -> * -> *

# Kind (* -> *) -> *: Higher-kinded type constructors
type Monad⟨M⟩     # The kind of M is * -> *
```

### Abstraction over Type Constructors

```valkyrie
# Define a higher-kinded trait
trait Functor where Self: * -> * {
    micro map⟨A, B⟩(self: Self⟨A⟩, f: micro(A) -> B) -> Self⟨B⟩
}

# Implementation for concrete types
imply ?: Functor {
    micro map⟨A, B⟩(self: A?, f: micro(A) -> B) -> B? {
        match self {
            case value: f(value)
            case None: None
        }
    }
}

imply []: Functor {
    micro map⟨A, B⟩(self: [A], f: micro(A) -> B) -> [B] {
        [ f(item) for item in self ]
    }
}
```

## Monad Pattern

### Monad Trait Definition

```valkyrie
# Monad trait
trait Monad where Self: * -> * {
    # Wrap a value into the monad
    micro pure⟨A⟩(value: A) -> Self⟨A⟩
    
    # Monadic bind operation
    micro bind⟨A, B⟩(self: Self⟨A⟩, f: micro(A) -> Self⟨B⟩) -> Self⟨B⟩
    
    # Convenience method: map can be implemented via bind and pure
    micro map⟨A, B⟩(self: Self⟨A⟩, f: micro(A) -> B) -> Self⟨B⟩ {
        self.bind({ $x; Self::pure(f($x)) })
    }
}

# ? Monad implementation
imply ?: Monad {
    micro pure⟨A⟩(value: A) -> A? {
        value
    }
    
    micro bind⟨A, B⟩(self: A?, f: micro(A) -> B?) -> B? {
        match self {
            case value: f(value)
            case None: None
        }
    }
}

# Result Monad implementation
imply⟨E⟩ Result⟨_, E⟩: Monad {
    micro pure⟨A⟩(value: A) -> Result⟨A, E⟩ {
        Fine(value)
    }
    
    micro bind⟨A, B⟩(self: Result⟨A, E⟩, f: micro(A) -> Result⟨B, E⟩) -> Result⟨B, E⟩ {
        match self {
            case Fine(value): f(value)
            case Fail(error): Fail(error)
        }
    }
}
```
