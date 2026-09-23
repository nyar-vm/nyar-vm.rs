# Control Flow

Valkyrie provides a rich set of control flow statements to manage the execution flow of programs.

## Conditional Statements

### if Statement

```valkyrie
# Basic if statement
if condition {
    # execute code
}

# if-else statement
if x > 0 {
    print("positive")
}
else {
    print("non-positive")
}

# if-else if-else chain
if score >= 90 {
    grade = "A"
}
else if score >= 80 {
    grade = "B"
}
else if score >= 70 {
    grade = "C"
}
else {
    grade = "F"
}

# if expression (returning a value)
let result = if x > 0 { "positive" } else { "non-positive" }

# Multi-line if expression
let message = if user.is_admin {
    "Admin User"
}
else if user.is_premium {
    "Premium User"
}
else {
    "Regular User"
}
```

### Conditional Expressions

```valkyrie
# Ternary operator style
let max = if a > b { a } else { b }

# Chained conditions
let status = if online { "Online" } else if busy { "Busy" } else { "Offline" }
```

## Loop Statements

### loop Statement (Infinite Loop)

```valkyrie
# Basic infinite loop
loop {
    # code to execute infinitely
    if should_break {
        break
    }
}

# Labeled loop
'outer: loop {
    'inner: loop {
        if condition1 {
            break 'outer  # break out of the outer loop
        }
        if condition2 {
            break 'inner  # break out of the inner loop
        }
    }
}

# Loop with return value
let result = loop {
    let input = get_input()
    if input.is_valid() {
        break input.value()  # return value
    }
}
```

### while Statement

```valkyrie
# Basic while loop
while condition {
    # execute code when condition is true
    update_condition()
}

# Complex condition
while x > 0 && y < 100 {
    x -= 1
    y += 2
}

# while let pattern matching
while let Some { value: item } = iterator.next() {
    process(item)
}

# Labeled while loop
'search: while has_more_data() {
    let data = get_next_data()
    if data.is_target() {
        break 'search
    }
}
```

### until Statement

```valkyrie
# until loop (executes when condition is false)
until condition {
    # execute code when condition is false
    update_condition()
}

# equivalent to while !condition
until x <= 0 {
    x -= 1
}

# until let pattern matching
until let None = optional_value {
    process(optional_value.unwrap())
    optional_value = get_next_optional()
}
```

### for Statement

```valkyrie
# Range iteration
for i in 0..<10 {
    print(i)  # prints 0 to 9
}

# Range with inclusive end
for i in 0..=10 {
    print(i)  # prints 0 to 10
}

# Array iteration
let numbers = [1, 2, 3, 4, 5]
for num in numbers {
    print(num)
}

# Iteration with index
for (index, value) in numbers.enumerate() {
    print(f"Index ${index}: Value ${value}")
}

# String iteration
for char in "hello".chars() {
    print(char)
}

# Object entry iteration
for (key, value) in object.entries() {
    print(f"${key}: ${value}")
}

# for loop with condition
for item in collection where item.is_valid() {
    process(item)
}

# Nested loops
for i in 0..<3 {
    for j in 0..<3 {
        print(f"(${i}, ${j})")
    }
}
```

## Pattern Matching

### match Statement

Valkyrie's `match` statement provides powerful structured pattern matching capabilities.

```valkyrie
match value {
    # Literal matching
    case 1: print("One")
    case "hello": print("Greeting")
    
    # Variable binding
    case x: print("Got ${x}")
    
    # Type matching
    case is i32: print("It's an integer")
    
    # Structural destructuring
    case Point { x, y }: print("Point at ${x}, ${y}")
    
    # List matching
    case [first, ..rest]: print("First: ${first}, Rest: ${rest}")
    
    # Guard conditions
    case x if x > 100: print("Large number: ${x}")
    
    # Wildcard
    case _: print("Something else")
}
```

### Pattern Destructuring

```valkyrie
unity Option⟨T⟩ {
    Some { value: T },
    None
}

let result = Some { value: 42 }

match result {
    case Some { value }: print("Value: ${value}")
    case None: print("Empty value")
}
```

### Destructuring Assignment

```valkyrie
# Array destructuring
let [first, second, ..rest] = array  # destructure array to kvs
let [a, _, c] = [1, 2, 3]  # ignore second element

# Tuple destructuring
let (x, y, z) = (1, 2, 3)
let (name, ..) = ("Alice", 25, "Engineer")  # only take first

# Object destructuring
let { name, age } = person
let { x: new_x, y: new_y } = point  # rename
let { name, ..rest } = user  # destructure dict to kvs
```

## Exception Handling

### catch Statement (Exception Handler)

```valkyrie
# Basic exception handling
catch {
    risky_operation()
} handle error {
    print(f"Error occurred: ${error}")
}

# Multiple exception type handling
catch {
    complex_operation()
} handle NetworkError(msg) {
    print(f"Network error: ${msg}")
} handle ValidationError(field) {
    print(f"Validation error: ${field}")
} handle error {
    print(f"Unknown error: ${error}")
}

# Exception handling with resource management
using resource = acquire_resource() {
    catch {
        file_operation()
    } handle IOError(msg) {
        print("IO Error: ${msg}")
    }
}  # resource will be automatically cleaned up

# Exception handling expression
let result = catch {
    parse_number(input)
} handle ParseError(_) {
    0  # default value
}

# Nested exception handling
catch {
    catch {
        inner_operation()
    } handle InnerError(e) {
        handle_inner_error(e)
    }
    outer_operation()
} handle OuterError(e) {
    handle_outer_error(e)
}
```

### Exception Propagation

```valkyrie
# Use ? operator to propagate exceptions
micro process_file(path: string) -> Result⟨string, IOError⟩ {
    let content = read_file(path)?  # returns error early if failed
    let processed = transform(content)?
    Fine { value: processed }
}

# Throw exception manually
micro validate_age(age: i32) -> Result⟨unit, ValidationError⟩ {
    if age < 0 {
        throw ValidationError("Age cannot be negative")
    }
    if age > 150 {
        throw ValidationError("Age cannot exceed 150")
    }
    Fine { value: () }
}
```

## Control Flow Keywords

### break and continue

```valkyrie
# break exits loop
for i in 0..<10 {
    if i == 5 {
        break  # exit loop
    }
    print(i)
}

# continue skips current iteration
for i in 0..<10 {
    if i % 2 == 0 {
        continue  # skip even numbers
    }
    print(i)  # only print odd numbers
}

# Labeled break and continue
'outer: for i in 0..<3 {
'inner: for j in 0..<3 {
        if i == 1 && j == 1 {
            break 'outer  # exit outer loop
        }
        if j == 2 {
            continue 'outer  # continue next iteration of outer loop
        }
        print(f"(${i}, ${j})")
    }
}

# break with return value
let found = loop {
    let item = get_next_item()
    if item.is_target() {
        break Some { value: item }  # return found item
    }
    if no_more_items() {
        break None  # return empty value
    }
}
```

### return Statement

```valkyrie
# Function return
micro calculate(x: i32, y: i32) -> i32 {
    if x < 0 || y < 0 {
        return -1  # early return
    }
    x + y  # implicit return
}

# Empty return
micro log_message(msg: string) {
    if msg.is_empty() {
        return  # early return with no value
    }
    print(msg)
}
```
