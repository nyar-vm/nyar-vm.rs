# Error Handling

Valkyrie uses `try` statements and the `catch` mechanism to handle errors. `try` is an independent statement that can be integrated with the type system.

## Try Statement

### Basic Try Syntax

```valkyrie
# try is an independent statement returning a Result type
let result = try Result⟨string⟩ {
    read_file("config.txt")?
}

# try with a specified error type
let data = try Result⟨Data, ParseError⟩ {
    let content = read_file("data.json")?
    parse_json(content)?
}

# Simplified form
let value = try {
    risky_operation()?
}
```

### Try with Optional Types

```valkyrie
# Handling operations that might fail
let maybe_value = try i32? {
    let input = get_user_input()?
    parse_number(input)?
}

# Chained operations
let result = try User? {
    let id = extract_user_id(request)?
    let user = find_user_by_id(id)?
    validate_user(user)?
}
```

## Catch Handling

### Non-mainline Control Flow Catch

```valkyrie
# Using .catch to handle errors
let config = try Result⟨Config⟩ {
    read_config_file()?
}
.catch {
    case FileNotFound(path): create_default_config(path)
    case ParseError(msg):
        log_error(msg)
        Config::default()
    case error:
        print("Unexpected error: {error}")
        Config::empty()
}

# Named catch
let data = try Result⟨Data⟩ {
    fetch_remote_data()?
}
.catch {
    case TimeoutError: retry_with_backoff()
    case ConnectionError(msg): use_cached_data()
    case error:
        log_error(error)
        Data::empty()
}
```

### Match-style Catch

```valkyrie
# catch and match are duals, possessing identical capabilities
let user_data = try Result⟨UserData⟩ {
    let raw = fetch_user_data(user_id)?
    validate_and_parse(raw)?
}
.catch {
    case ValidationError { field, message }:
        show_field_error(field, message)
        UserData::guest()
    case NetworkError { code, .. } if code >= 500:
        # Server error, retry later
        schedule_retry()
        UserData::cached(user_id)
    case NetworkError { code, .. } if code >= 400:
        # Client error
        UserData::error(code)
    else: UserData::unknown_error()
}
```

## Error Propagation

### Question Mark Operator

```valkyrie
# The ? operator is used for error propagation
micro process_file(path: string) -> Result⟨string, FileError⟩ {
    let content = read_file(path)?  # If it fails, return the error immediately
    let processed = transform_content(content)?
    validate_result(processed)?
}

# Using ? within a try block
let final_result = try Result⟨ProcessedData⟩ {
    let raw = fetch_data()?
    let cleaned = clean_data(raw)?
    let validated = validate_data(cleaned)?
    process_final(validated)?
}
```

### Error Conversion

```valkyrie
# Automatic error conversion
micro read_and_parse(path: string) -> Result⟨Config, AppError⟩ {
    try Result⟨Config, AppError⟩ {
        let content = read_file(path)?  # FileError -> AppError
        let config = parse_json(content)?  # ParseError -> AppError
        validate_config(config)?  # ValidationError -> AppError
    }
}

# Manual error conversion
let result = try Result⟨Data⟩ {
    fetch_data().map_err { $e -> AppError::Network($e) }?
}
```

## Custom Error Types

```valkyrie
# Define error types
union AppError {
    Network(NetworkError),
    Parse(ParseError),
    Validation { field: string, message: string },
    IO(IOError)
}

# Implement error conversion
imply AppError: From⟨NetworkError⟩ {
    micro from(err: NetworkError) -> AppError {
        AppError::Network(err)
    }
}

# Using custom errors
micro load_user_config(user_id: string) -> Result⟨UserConfig, AppError⟩ {
    try Result⟨UserConfig, AppError⟩ {
        let path = get_config_path(user_id)?
        let content = read_file(path)?
        let config = parse_config(content)?
        validate_user_config(config)?
    }
}
```

## Error Recovery Patterns

### Fallback Strategies

```valkyrie
# Multi-level fallback
let avatar = try Image? {
    load_from_cdn(user_id)?
}
.catch {
    case NetworkError: try Image? {
        load_from_cache(user_id)?
    }
    .catch {
        case CacheError: default_avatar()
        else: null
    }
    else: default_avatar()
}

# Retry mechanism
let data = try Result<Data> {
    fetch_with_retry(url, max_retries = 3)?
}
.catch {
    case RetryExhausted(attempts):
        log_error("Failed after ${attempts} attempts")
        use_fallback_data()
    case error:
        log_error("Unexpected error: ${error}")
        Data::empty()
}
```

### Partial Recovery

```valkyrie
# Handling partial failures
let results = try Result⟨[ProcessedItem]⟩ {
    items.map { $item ->
        try ProcessedItem? {
            process_item($item)?
        }
        .catch {
            case ProcessingError(msg):
                log_warning("Skipping item: ${msg}")
                null  # Skip the failed item
            else: null
        }
    }).filter_map { $x }.collect()
}
```

## Best Practices

### 1. Error Type Design

```valkyrie
# Custom validation errors
union ValidationError {
    InvalidFormat { field: String, expected: String },
    ValueOutOfRange { field: String, min: i32, max: i32 },
    RequiredFieldMissing { field: String }
}

# Validation effect definition
effect Validation {
    micro validate_user(user: User) -> Result<(), [ValidationError]>
}

# Implement validation logic
micro check_user(user: User) -> Result<(), [ValidationError]> {
    let mut errors = []
    
    if user.name.is_empty() {
        errors.push(ValidationError::RequiredFieldMissing { field: "name" })
    }
    
    if user.age < 0 || user.age > 150 {
        errors.push(ValidationError::ValueOutOfRange { 
            field: "age", 
            min: 0, 
            max: 150 
        })
    }
    
    if errors.is_empty() {
        Fine { value: () }
    } else {
        Fail { error: errors }
    }
}

# Contextual information
class ContextualError {
    operation: String,
    context: Map<String, String>,
    source: Box<dyn Error>
}
```

### 2. Error Handling Strategies

```valkyrie
# Handle errors locally
micro validate_user_input(input: UserInput) -> Result<ValidatedInput, ValidationError> {
    try Result<ValidatedInput> {
        let email = validate_email(input.email)?
        let age = validate_age(input.age)?
        let name = validate_name(input.name)?
        class: ValidatedInput { email, age, name }
    }
}

# Unified error handling
micro main() {
    let result = try Result<()> {
        run_application()?
    }
    .catch {
        case ConfigError(msg):
            print("Configuration error: ${msg}")
            exit(1)
        case NetworkError(msg):
            print("Network error: ${msg}")
            exit(2)
        case error:
            print("Unexpected error: ${error}")
            exit(99)
    }
}
```

### 3. Resource Management

```valkyrie
# Use the RAII pattern
class FileHandle {
    path: string,
    handle: File
}

impl Drop for FileHandle {
    micro drop() {
        self.handle.close()
    }
}

# Safe resource usage
micro process_file_safely(path: string) -> Result⟨string, FileError⟩ {
    try Result⟨string⟩ {
        let file = FileHandle::open(path)?
        let content = file.read_all()?
        process_content(content)?
    }  # file is closed automatically
}
```

Error handling implemented through the Effect system provides type-safe, structured, and easily composable error management capabilities, significantly enhancing the robustness of applications.