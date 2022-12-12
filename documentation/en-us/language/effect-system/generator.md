# Generators

Valkyrie's generators are special functions that can produce a series of values using the `yield` keyword. Generators provide a way of lazy computation, calculating the next value only when needed, which is ideal for handling large datasets or infinite sequences.

## Basic Generator Syntax

### Simple Generator

```valkyrie
# Basic generator function
micro count_up(max: i32) {
    let mut i = 0
    while i < max {
        yield i
        i += 1
    }
}

# Using the generator
let counter = count_up(5)
for value in counter {
    print(value)  # Output: 0, 1, 2, 3, 4
}
```

### Infinite Generator

```valkyrie
# Fibonacci sequence generator
micro fibonacci() {
    let mut a = 0
    let mut b = 1
    loop {
        yield a
        let temp = a + b
        a = b
        b = temp
    }
}

# Get the first 10 Fibonacci numbers
let fib = fibonacci()
for i in 0..<10 {
    print(fib.next())  # 0, 1, 1, 2, 3, 5, 8, 13, 21, 34
}
```

### Generator with Return Value

```valkyrie
# Generators can have a final return value
micro process_items(items: [string]) -> i32 {
    let mut count = 0
    for item in items {
        if item.is_valid() {
            yield item.process()
            count += 1
        }
    }
    count  # Finally returns the number of processed items
}

# Usage
let processor = process_items(["item1", "item2", "item3"])
for result in processor {
    print("Processed: ${ result }")
}
let total_count = processor.return_value()  # Get the final return value
```

## Generator State Management

### Sequence Environments

In addition to defining an entire `micro` function as a generator, Valkyrie supports using `sequence` environments to define generators locally within regular functions. This allows you to produce a lazy sequence without changing the nature of the entire function.

#### Local Generators

Use a `sequence` block to create an anonymous generator object:

```valkyrie
micro process_data(data: [i32]) {
    # Define a local generator within a regular function
    let gen = sequence {
        for item in data {
            if item > 0 {
                yield item * 2
            }
        }
    }
    
    # Use the local generator
    for val in gen {
        print(val)
    }
}
```

#### Explicit Type Declaration

You can also explicitly specify the element type produced by the `sequence` environment:

```valkyrie
let gen = sequence Iterator<string> {
    yield "Hello"
    yield "World"
}
```

#### Expression Usage

The `sequence` environment is an expression and can be passed as an argument or returned directly:

```valkyrie
micro get_numbers() {
    return sequence {
        yield 1
        yield 2
        yield 3
    }
}
```

## Generator State Management

### Generator Lifecycle

```valkyrie
# Generator state union
union GeneratorState {
    Created,     # Created but not yet started
    Running,     # Currently executing
    Suspended,   # Paused (via yield)
    Completed,   # Finished execution
    Error { error: Any } # An error occurred
}

# Check generator state
micro example_generator() {
    print("Execution started")
    yield "First value"
    print("Execution resumed")
    yield "Second value"
    print("Execution completed")
}

let gen = example_generator()
print(gen.state())  # Created

let first = gen.next()
print(gen.state())  # Suspended
print(first)        # "First value"

let second = gen.next()
print(gen.state())  # Suspended
print(second)       # "Second value"

gen.next()          # Completion
print(gen.state())  # Completed
```

### Generator Control

```valkyrie
# Manually control generator execution
micro controlled_generator() {
    let mut value = 0
    loop {
        let input = yield value
        if input != null {
            value = input  # Receive external input
        } else {
            value += 1     # Default increment
        }
    }
}

let gen = controlled_generator()
print(gen.next())        # 0
print(gen.send(10))      # 10 (Send value to generator)
print(gen.next())        # 11
print(gen.send(100))     # 100
```

## Generator Pipelines

### Pipeline Processing

```valkyrie
# Generator pipeline processing
micro pipeline_stage1(input: Iterator<i32>) {
    for value in input {
        yield value * 2  # Stage 1: Multiply by 2
    }
}

micro pipeline_stage2(input: Iterator<i32>) {
    for value in input {
        if value % 4 == 0 {
            yield value  # Stage 2: Filter multiples of 4
        }
    }
}

micro pipeline_stage3(input: Iterator<i32>) {
    for value in input {
        yield "Result: ${ value }"  # Stage 3: Formatting
    }
}

# Construct the pipeline
let numbers = [1, 2, 3, 4, 5, 6, 7, 8]
let stage1 = pipeline_stage1(numbers.iter())
let stage2 = pipeline_stage2(stage1)
let stage3 = pipeline_stage3(stage2)

for result in stage3 {
    print(result)  # "Result: 4", "Result: 8", "Result: 12", "Result: 16"
}
```

### Combining Generators

```valkyrie
# Combine multiple generators
micro combine_generators(gen1: Generator⟨i32⟩, gen2: Generator⟨i32⟩) {
    # Alternately produce values from two generators
    loop {
        let val1 = gen1.next()
        let val2 = gen2.next()
        
        if val1 != null {
            yield val1
        }
        if val2 != null {
            yield val2
        }
        
        if val1 == null && val2 == null {
            break
        }
    }
}

let gen1 = count_up(3)  # 0, 1, 2
let gen2 = count_up(2)  # 0, 1
let combined = combine_generators(gen1, gen2)

for value in combined {
    print(value)  # 0, 0, 1, 1, 2
}
```

## Advanced Generator Patterns

### Lazy Computation

```valkyrie
# Lazy computation of prime numbers
micro prime_generator() {
    let mut candidates = 2..
    let mut primes = []
    
    for candidate in candidates {
        let is_prime = primes.all(|p| candidate % p != 0)
        if is_prime {
            primes.push(candidate)
            yield candidate
        }
    }
}

# Get the first 10 prime numbers
let primes = prime_generator()
for i in 0..<10 {
    print(primes.next())  # 2, 3, 5, 7, 11, 13, 17, 19, 23, 29
}
```

### File Processing Generator

```valkyrie
# Read file line by line
micro read_lines(filename: String) {
    let file = open_file(filename)
    try {
        while !file.eof() {
            let line = file.read_line()
            if !line.is_empty() {
                yield line.trim()
            }
        }
    } finally {
        file.close()
    }
}

# Usage
for line in read_lines("data.txt") {
    print("Line: ${ line }")
}
```

### Data Transformation Generator

```valkyrie
# Data transformation pipeline
micro transform_data(data: Iterator⟨string⟩) {
    for item in data {
        # Parse JSON
        let parsed = json_parse(item)
        if parsed.is_ok() {
            let obj = parsed.unwrap()
            
            # Validate data
            if obj.has_field("id") && obj.has_field("name") {
                # Transform format
                let transformed = {
                    id: obj.id,
                    name: obj.name.to_uppercase(),
                    timestamp: current_time()
                }
                yield transformed
            }
        }
    }
}
```

## Error Handling

### Generator Exception Handling

```valkyrie
# Exception handling within generators
micro error_prone_generator() {
    try {
        yield "Processing started"
        
        let risky_operation = perform_risky_task()
        yield "Risky operation completed"
        
        if risky_operation.is_error() {
            raise "Operation failed"
        }
        
        yield "Processing successful"
    }
    .catch {
        case _:
            yield "Error occurred: ${ error }"
            raise error  # Re-throw the exception
    }
}

# Using a generator with error handling
let gen = error_prone_generator()
try {
    for status in gen {
        print(status)
    }
}
.catch {
    case _:
        print("Generator exception: ${ error }")
}
```

## Best Practices

### 1. Generator Design Principles

```valkyrie
# Keep generators simple and focused
micro good_generator(data: [String]) {
    for item in data {
        if item.is_valid() {
            yield item.process()  # Do one thing only
        }
    }
}

# Avoid complex state management within generators
# Bad example:
micro bad_generator() {
    let mut complex_state = ComplexState::new()
    # ... complex state logic
}
```

### 2. Resource Management

```valkyrie
# Ensure resources are correctly released
micro file_processor(filename: String) {
    using file = open_file(filename) {
        while !file.eof() {
            let line = file.read_line()
            yield process_line(line)
        }
    }  # File automatically closed
}
```

### 3. Performance Considerations

```valkyrie
# Avoid frequent small yields
# Bad example:
micro inefficient_generator(data: [i32]) {
    for item in data {
        yield item  # yield every element
    }
}

# Good example:
micro efficient_generator(data: [i32]) {
    let mut batch = []
    for item in data {
        batch.push(item)
        if batch.len() >= 100 {
            yield batch  # Batch yield
            batch = []
        }
    }
    if !batch.is_empty() {
        yield batch  # Handle remaining items
    }
}
```

### 4. Testing Generators

```valkyrie
# Generator testing strategy
micro test_generator() {
    let gen = count_up(3)
    
    # Test generated values
    @assert_equal(gen.next(), 0)
    @assert_equal(gen.next(), 1)
    @assert_equal(gen.next(), 2)
    @assert_equal(gen.next(), null)
    
    # Test state
    @assert_equal(gen.state(), GeneratorState::Completed)
}

# Generator integration testing
micro test_pipeline() {
    let input = [1, 2, 3, 4]
    let pipeline = pipeline_stage1(input.iter())
    let results = pipeline.collect()
    
    @assert_equal(results, [2, 4, 6, 8])
}
```

### 5. Return Value Restrictions

```valkyrie
# Generator return values cannot be anonymous classes
# Error example:
micro bad_generator() -> class { x: i32 } {  # Compilation error
    yield 1
    class { x: 42 }  # Anonymous class as return value makes type inference difficult
}

# Correct example:
class Result {
    x: i32
}

micro good_generator() -> Result {
    yield 1
    Result { x: 42 }  # Use a named type
}

# Or use a type alias
type GeneratorResult = class { x: i32 }

micro another_good_generator() -> GeneratorResult {
    yield 1
    GeneratorResult { x: 42 }
}
```
