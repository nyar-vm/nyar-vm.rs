# Coroutines

Valkyrie provides powerful coroutine support, implementing cooperative multitasking through the `yield` keyword. Coroutines allow functions to pause and resume execution, making them ideal for handling asynchronous operations and state machines. The primary difference between coroutines and generators is that coroutines focus more on pausing and resuming control flow, rather than just producing a sequence of values.

## Coroutine State Management

### Coroutine Lifecycle

```valkyrie
# Coroutine State Union
union CoroutineState {
    Created,     # Created but not yet started
    Running,     # Currently executing
    Suspended,   # Paused (via yield)
    Completed,   # Finished execution
    Error { error: Any } # An error occurred
}

# Checking Coroutine State
micro example_coroutine() {
    print("Execution started")
    yield "First value"
    print("Execution resumed")
    yield "Second value"
    print("Execution completed")
}

let coro = example_coroutine()
print(coro.state())  # Created

let first = coro.next()
print(coro.state())  # Suspended
print(first)         # "First value"

let second = coro.next()
print(coro.state())  # Suspended
print(second)        # "Second value"

coro.next()          # Execution finished
print(coro.state())  # Completed
```

### Coroutine Control

```valkyrie
# Manually controlling coroutine execution
micro controlled_coroutine() {
    let mut state = "idle"
    loop {
        let command = yield state
        command.match {
            case "start":
                state = "running"
            case "pause":
                state = "paused"
            case "stop":
                state = "stopped"
                break
            case _:
                state = "unknown_command"
        }
    }
}

let coro = controlled_coroutine()
print(coro.next())           # "idle"
print(coro.send("start"))    # "running"
print(coro.send("pause"))    # "paused"
print(coro.send("stop"))     # "stopped"
```

## Async Coroutines

### Async Operations

```valkyrie
# Async Coroutine
async micro fetch_data(url: string) -> string {
    print("Starting request: ${ url }")
    let response = http_get(url).await?
    yield "Request sent"  # yield can be used in async functions
    
    if response.status == 200 {
        yield "Request successful"
        response.body
    } else {
        raise "Request failed: ${ response.status }"
    }
}

# Using Async Coroutines
async micro main() {
    let fetcher = fetch_data("https://api.example.com/data")
    
    # Handling intermediate states
    for status in fetcher {
        print("Status: ${ status }")
    }
    
    # Getting final result
    try {
        let data = fetcher.await?
        print("Data: ${ data }")
    }
    .catch {
        case _:
            print("Error: ${ error }")
    }
}
```

### Concurrent Coroutines

```valkyrie
# Executing multiple coroutines concurrently
async micro concurrent_processing(items: [string]) {
    let promises = items.map(async micro(item) {
        let result = process_item(item)
        yield "Processed: ${ item }"
        result
    })
    
    # Wait for all Promises to complete
    let results = Promise::all(promises).await?
    yield "All tasks completed"
    results
}

# Usage
async micro run_concurrent() {
    let processor = concurrent_processing(["item1", "item2", "item3"])
    
    for update in processor {
        print(update)
    }
    
    let final_results = processor.await?
    print("Final results: ${ final_results }")
}
```

## Advanced Coroutine Patterns

### State Machine Coroutines

```valkyrie
# State Machine Implementation
union State {
    Idle,
    Processing,
    Waiting,
    Complete
}

micro state_machine() {
    let mut state = State::Idle
    let mut data = null
    
    loop {
        state.match {
            case State::Idle: {
                yield "Waiting for input"
                data = yield_receive()  # Wait for external input
                state = State::Processing
            }
            case State::Processing: {
                yield "Processing..."
                let result = process_data(data)
                if result.is_ok() {
                    state = State::Complete
                } else {
                    state = State::Waiting
                }
            }
            case State::Waiting: {
                yield "Waiting for retry"
                sleep(1000)  # Wait for 1 second
                state = State::Processing
            }
            case State::Complete: {
                yield "Processing complete"
                break
            }
        }
    }
}
```

### Coroutine Pool

```valkyrie
# Coroutine Pool Management
class CoroutinePool {
    coroutines: [Coroutine],
    max_size: i32,
    active_count: i32
    
    micro new(max_size: i32) -> Self {
        CoroutinePool {
            coroutines: [],
            max_size: max_size,
            active_count: 0
        }
    }
    
    micro spawn(task: micro() -> Any) -> bool {
        if self.active_count < self.max_size {
            let coro = Coroutine::new(task)
            self.coroutines.push(coro)
            self.active_count += 1
            true
        } else {
            false  # Pool is full
        }
    }
    
    micro run_all() {
        while self.active_count > 0 {
            for coro in self.coroutines {
                if coro.state() == CoroutineState::Suspended {
                    let result = coro.resume()
                    yield "Coroutine progress: ${ result }"
                    
                    if coro.state() == CoroutineState::Completed {
                        self.active_count -= 1
                    }
                }
            }
        }
        yield "All coroutines completed"
    }
}
```

## Error Handling

### Coroutine Exception Handling

```valkyrie
# Exception handling within coroutines
micro error_prone_generator() {
    try {
        yield "Starting process"
        
        let risky_operation = perform_risky_task()
        yield "Risky operation completed"
        
        if risky_operation.is_error() {
            raise "Operation failed"
        }
        
        yield "Process successful"
    }
    .catch {
        case _:
            yield "An error occurred: ${ error }"
            raise error  # Re-throw the exception
    }
}

# Using coroutines with error handling
let gen = error_prone_generator()
try {
    for status in gen {
        print(status)
    }
}
.catch {
    case _:
        print("Coroutine exception: ${ error }")
}
```

## Best Practices

### 1. Coroutine Design Principles

```valkyrie
# Keep coroutines simple and focused
micro good_generator(data: [String]) {
    for item in data {
        if item.is_valid() {
            yield item.process()  # Do only one thing
        }
    }
}

# Avoid complex state management within coroutines
# Bad example:
micro bad_generator() {
    let mut complex_state = ComplexState::new()
    # ... complex state logic
}
```

### 2. Resource Management

```valkyrie
# Ensure proper resource release
micro file_processor(filename: string) {
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
        yield item  # Yield for every element
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
        yield batch  # Process remaining items
    }
}
```

### 4. Testing Coroutines

```valkyrie
# Coroutine testing strategy
micro test_generator() {
    let gen = count_up(3)
    
    # Test generated values
    @assert_equal(gen.next(), 0)
    @assert_equal(gen.next(), 1)
    @assert_equal(gen.next(), 2)
    @assert_equal(gen.next(), null)
    
    # Test state
    @assert_equal(gen.state(), CoroutineState::Completed)
}

# Async coroutine testing
async micro test_async_generator() {
    let gen = async_data_processor()
    
    let first_result = gen.next().await?
    assert!(first_result != null)
    
    let final_result = gen.collect_all().await?
    @assert_equal(final_result.len(), 5)
}
```
