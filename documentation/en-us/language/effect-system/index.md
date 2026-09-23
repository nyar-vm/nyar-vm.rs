# Effect System

Valkyrie's effect system provides powerful side-effect management and control flow mechanisms, including exception handling, coroutines, generators, reactive programming, aspect-oriented programming, and dependency injection.

## Effect System Components

- **[Exception Handling](./error-handler.md)** - Flexible error handling and exception propagation mechanisms.
- **[Coroutines](./coroutine.md)** - Cooperative multitasking and asynchronous programming.
- **[Generators](./generator.md)** - Lazy evaluation and value sequence generation.
- **[Reactive Programming](./reactive.md)** - Programming paradigm for data flows and the propagation of change.
- **[Aspect-Oriented Programming](./aop.md)** - Separation and management of cross-cutting concerns.
- **[Dependency Injection](./ioc.md)** - Inversion of Control and dependency management.

---

# Exception Handling System

In Valkyrie, any object can be thrown and caught as an exception. This provides a flexible error handling mechanism that allows programs to handle various exceptional situations in a structured way.

## Basic Exception Handling

### Throwing Exceptions

```valkyrie
# Any object can be thrown as an exception
raise "Something went wrong"
raise 404
raise { code: 500, message: "Internal Server Error" }

# Throwing custom objects
class NetworkError {
    message: string
    code: i32
}

raise NetworkError {
    message: "Connection timeout",
    code: -1
}
```

### Catching Exceptions

```valkyrie
# Basic exception catching
try {
    risky_operation()
}
.catch {
    case _: print("Caught error: ${error}")
}

# Type-specific exception catching
try {
    network_request()
}
.catch {
    case NetworkError:
            print("Network error: ${error.message}")
            retry_connection()
        case string: print("String error: ${error}")
    else: print("Unknown error: ${error}")
}
```

## Exception Types and Patterns

### String Exceptions

```valkyrie
# Simple string exceptions
micro validate_age(age: i32) {
    if age < 0 {
        raise "Age cannot be negative"
    }
    if age > 150 {
        raise "Age seems unrealistic"
    }
}

try {
    validate_age(-5)
}
.catch {
    case message: string: print("Validation error: ${message}")
}
```

### Numeric Exceptions

```valkyrie
# Using numbers as error codes
micro http_request(url: string) {
    if !is_valid_url(url) {
        raise 400  # Bad Request
    }
    if !is_authorized() {
        raise 401  # Unauthorized
    }
    if !resource_exists(url) {
        raise 404  # Not Found
    }
}

try {
    http_request("http://example.com")
}
.catch {
    case 400: print("Bad Request")
    case 401: print("Unauthorized")
    case 404: print("Not Found")
    case code: i32: print("HTTP error code: ${code}")
}
```

### Custom Exception Classes

```valkyrie
# Use classes to carry structured error data
class DatabaseError {
    query: string
    error_message: string
}

micro execute_query(query: string) {
    if query.is_empty() {
        raise DatabaseError {
            query: query,
            error_message: "Query cannot be empty"
        }
    }
    # Execute query...
}

try {
    execute_query("")
}
.catch {
    case err: DatabaseError:
        print("Database error in query: ${err.query}")
        print("Error details: ${err.error_message}")
}
```

## Exception Propagation

### Automatic Propagation

```valkyrie
# Exceptions propagate upwards automatically
micro level3() {
    raise "Error from level 3"
}

micro level2() {
    level3()  # Exception propagates here
}

micro level1() {
    level2()  # Exception continues propagating
}

try {
    level1()
}
.catch {
    case _: print("Caught at top level: ${error}")
}
```

### Exception Conversion

```valkyrie
# Catch and convert exceptions
micro parse_config(content: string) -> Config {
    try {
        json.parse(content)
    }
    .catch {
        case parse_error: raise ConfigError {
            message: "Failed to parse configuration",
            cause: parse_error,
            content_preview: content.substring(0, 100)
        }
    }
}

# Exception chaining
class ConfigError {
    message: string
    cause: Any
    content_preview: string
}
```

## Resource Management

### Automatic Cleanup

```valkyrie
# Use 'using' to ensure resource cleanup
micro process_file(filename: string) {
    using file = File.open(filename) {
        let content = file.read_all()
        process_content(content)
    }  # file is automatically closed
}

# Use 'defer' for delayed execution
micro database_transaction() {
    let transaction = db.begin_transaction()
    defer transaction.rollback()  # Default rollback
    
    try {
        # Execute database operations
        db.insert(data1)
        db.update(data2)
        db.delete(data3)
        
        transaction.commit()
        defer.cancel()  # Cancel rollback
    }
    .catch {
        case error {
            # Automatic rollback on exception
            raise error
        }
    }
}
```

### Resource Wrapping

```valkyrie
# Wrapper for automatic resource management
class ManagedResource⟨T⟩ {
    resource: T
    cleanup: () -> ()
    
    micro new(resource: T, cleanup: () -> ()) -> Self {
        Self { resource, cleanup }
    }
    
    micro use⟨R⟩(block: (T) -> R) -> R {
        let result = block(self.resource)
        self.cleanup()
        result
    }
}

# Usage example
let managed_file = ManagedResource.new(
    File.open("data.txt"),
    { => file.close() }
)

managed_file.use({ $file =>
    let content = file.read_all()
    process_content(content)
})  # file is automatically closed
```

## Exception Handling Patterns

### Retry Pattern

```valkyrie
micro retry⟨T⟩(max_attempts: i32, operation: () -> T) -> T {
    let mut attempts = 0
    loop {
        try {
            return operation()
        }
        .catch {
            case error {
                attempts += 1
            if attempts >= max_attempts {
                raise RetryExhausted {
                    attempts: attempts,
                    last_error: error
                }
            }
            sleep(Duration.seconds(attempts))  # Exponential backoff
        }
    }
}

# Using retry
try {
    let result = retry(3, { =>
        unreliable_network_call()
    })
    print("Success: ${result}")
}
.catch {
    case RetryExhausted: print("Failed after ${error.attempts} attempts: ${error.last_error}")
}
```

### Circuit Breaker Pattern

```valkyrie
class CircuitBreaker {
    failure_count: i32
    failure_threshold: i32
    state: CircuitState
    last_failure_time: DateTime
    
    micro call⟨T⟩(operation: () -> T) -> T {
        match self.state {
            CircuitState.Closed => {
                try {
                    let result = operation()
                    self.reset()
                    result
                }
                .catch {
                    case error {
                        self.record_failure()
                        raise error
                    }
                }
            }
            CircuitState.Open => {
                if self.should_attempt_reset() {
                    self.state = CircuitState.HalfOpen
                    self.call(operation)
                } else {
                    raise CircuitBreakerOpen {
                        message: "Circuit breaker is open"
                    }
                }
            }
            CircuitState.HalfOpen => {
                try {
                    let result = operation()
                    self.reset()
                    result
                }
                .catch {
                    case error {
                        self.state = CircuitState.Open
                        raise error
                    }
                }
            }
        }
    }
}
```

### Exception Aggregation

```valkyrie
# Collect multiple exceptions
class AggregateException {
    exceptions: [Any]
    
    micro add(exception: Any) {
        self.exceptions.push(exception)
    }
    
    micro has_errors() -> bool {
        !self.exceptions.is_empty()
    }
}

micro process_batch(items: [Item]) {
    let errors = AggregateException { exceptions: [] }
    
    for item in items {
        try {
            process_item(item)
        }
        .catch {
            case _: errors.add(error)
        }
    }
    
    if errors.has_errors() {
        raise errors
    }
}
```

## Best Practices

### 1. Exception Type Design

```valkyrie
# Use meaningful exception types
class UserNotFoundError {
    user_id: string
    search_criteria: Map⟨string, string⟩
}

class PermissionDeniedError {
    user: string
    resource: string
    required_permission: string
}

# Avoid generic strings
# raise "User not found"  # Not recommended
```

### 2. Exception Information

```valkyrie
# Provide sufficient context information
class FileProcessingError {
    filename: string
    line_number: i32
    column: i32
    error_type: string
    suggestion: string
}

micro parse_csv(filename: string) {
    try {
        # Parsing logic
    }
    .catch {
        case _: raise FileProcessingError {
            filename: filename,
            line_number: current_line,
            column: current_column,
            error_type: "Invalid CSV format",
            suggestion: "Check for missing quotes or commas"
        }
    }
}
```

### 3. Exception Handling Strategy

```valkyrie
# Handle exceptions at appropriate levels
micro application_main() {
    try {
        run_application()
    }
    .catch {
        case ConfigurationError:
            print("Configuration error: ${error.message}")
            print("Please check your configuration file")
            exit(1)
        case NetworkError:
            print("Network error: ${error.message}")
            print("Please check your internet connection")
            exit(2)
        else:
            print("Unexpected error: ${error}")
            log_error(error)
            exit(99)
    }
}

# Avoid over-catching exceptions
micro bad_example() {
    try {
        some_operation()
    }
    .catch {
        case error {
            # Doing nothing hides errors
        }
    }
}
```

### 4. Testing Exception Handling

```valkyrie
#[test]
micro test_validation_error() {
    let invalid_user = User { email: "" }
    
    try {
        save_user(invalid_user)
        assert(false, "Expected ValidationError")
    }
    .catch {
        case ValidationError:
            @assert_equal(error.field, "email")
            @assert_equal(error.constraint, "Email cannot be empty")
    }
}

#[test]
micro test_retry_exhausted() {
    let mut call_count = 0
    
    try {
        retry(3, { =>
            call_count += 1
            raise "Always fails"
        })
        assert(false, "Expected RetryExhausted")
    }
    .catch {
        case RetryExhausted:
            @assert_equal(error.attempts, 3)
            @assert_equal(call_count, 3)
    }
}
```