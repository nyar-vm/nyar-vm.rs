# Pattern Matching (Match)

Valkyrie provides powerful pattern matching capabilities, supporting various matching patterns and syntax forms.

## Basic Match Syntax

### Standard Match Statement

```valkyrie
# Basic pattern matching
match value {
    case 1: "one"
    case 2: "two"
    case 3: "three"
    else: "other"
}

# Range matching
match score {
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case 60..=69: "D"
    else: "F"
}

# Multi-value matching
match day {
    case "Saturday" | "Sunday": "Weekend"
    case "Monday"..="Friday": "Weekday"
    else: "Invalid day"
}
```

### Expression Match Syntax

```valkyrie
# Expression form of match
let result = match value {
    case 1: "one"
    case 2: "two"
    case 3: "three"
    else: "other"
}

# Method chaining
let processed = match input.transform() {
    case Fine(value): value * 2
    case Fail(error): 0
}
```

## Destructuring Matching

### Tuple Destructuring

```valkyrie
match point {
    case (0, 0): "Origin"
    case (x, 0): "On X-axis at ${x}"
    case (0, y): "On Y-axis at ${y}"
    case (x, y): "Point at (${x}, ${y})"
}

# Nested tuples
match nested {
    case ((a, b), c): "Nested: ${a}, ${b}, ${c}"
    case (x, (y, z)): "Other nested: ${x}, ${y}, ${z}"
    else: "No match"
}
```

### Array Destructuring

```valkyrie
match array {
    case []: "Empty array"
    case [x]: "Single element: ${x}"
    case [first, second]: "Two elements: ${first}, ${second}"
    case [head, ..tail]: "Head: ${head}, Tail length: ${tail.len()}"
    case [.., last]: "Last element: ${last}"
    case [first, .., last]: "First: ${first}, Last: ${last}"
}

# Fixed-length matching
match coordinates {
    case [x, y]: "2D point: (${x}, ${y})"
    case [x, y, z]: "3D point: (${x}, ${y}, ${z})"
    else: "Unsupported dimension"
}
```

### Object Destructuring

```valkyrie
match person {
    case { name: "Alice", age }: "Alice is ${age} years old"
    case { name, age: 18..=65 }: "${name} is working age"
    case { name, age, city: "Beijing" }: "${name} from Beijing, age ${age}"
}
```

## Guard Conditions

You can use `if` clauses to add additional filtering conditions to pattern matching:

```valkyrie
match point {
    case (x, y) if x == y: "On diagonal"
    case (x, y) if x > y: "Below diagonal"
    case (x, y): "Above diagonal"
}
```

## Type Matching

Pattern matching can also be used to check and convert types:

```valkyrie
match shape {
    case s: Circle: "Circle with radius ${s.radius}"
    case s: Rectangle: "Rectangle ${s.width}x${s.height}"
    case _: "Unknown shape"
}
```
