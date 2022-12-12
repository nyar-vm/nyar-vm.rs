# Type Function

Type functions are a powerful feature in the Valkyrie language used for calculations at the type level. Defined using the `mezzo` keyword, type functions allow for the manipulation and transformation of types at compile time.

## Basic Syntax

```valkyrie
mezzo FunctionName(param: Type) -> Type {
    $ Type function body
}
```

## Examples

### Check Even Type

```valkyrie
mezzo IsEven(z: Type) -> bool {
    $ Check if type z represents an even number
    match z {
        i32 if z % 2 == 0 => true,
        _ => false
    }
}
```

### Type Mapping

```valkyrie
mezzo MapType⟨T⟩(input: T) -> Type {
    $ Map and transform the input type
    match input {
        i32 => i64,
        f32 => f64,
        _ => input
    }
}
```

### Conditional Type Selection

```valkyrie
mezzo ConditionalType⟨T, U⟩(condition: bool) -> Type {
    $ Select a type based on a condition
    if condition {
        T
    } else {
        U
    }
}
```

## Features

- **Compile-time Execution**: Type functions are executed at compile time, incurring no runtime overhead.
- **Type Safety**: All type operations are verified by the compiler.
- **Recursion Support**: Supports recursive type function definitions.
- **Pattern Matching**: Types can be pattern-matched.

## Use Cases

1. **Type Validation**: Verify if a type meets specific conditions at compile time.
2. **Type Conversion**: Automatically derive and convert related types.
3. **Generic Constraints**: Add complex type constraints to generic parameters.
4. **Metaprogramming**: Implement advanced compile-time code generation.

## Considerations

- Type functions must be pure functions with no side effects.
- All branches must return a valid type.
- Recursion depth is limited to prevent infinite recursion.
