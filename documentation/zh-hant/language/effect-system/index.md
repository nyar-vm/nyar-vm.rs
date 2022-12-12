# 效果系統

Valkyrie 的效果系統提供了強大的副作用管理和控制流機制，包括異常處理、協程、生成器、反應式編程、面向切面編程和依賴注入等功能。

## 效果系統組件

- **[異常處理](./error-handler.md)** - 靈活的錯誤處理和異常傳播機制
- **[協程](./coroutine.md)** - 協作式多任務處理和非同步編程
- **[生成器](./generator.md)** - 惰性計算和值序列生成
- **[反應式編程](./reactive.md)** - 數據流和變化傳播的編程範式
- **[面向切面編程](./aop.md)** - 橫切關注點的分離和管理
- **[依賴注入](./ioc.md)** - 控制反轉和依賴管理

---

# 異常處理系統

Valkyrie 中任何對象都可以作為異常被拋出和捕獲。這提供了一種靈活的錯誤處理機制，允許程式以結構化的方式處理各種異常情況。

## 基本異常處理

### 拋出異常

```valkyrie
# 任何對象都可以作為異常拋出
raise "Something went wrong"
raise 404
raise { code: 500, message: "Internal Server Error" }

# 拋出自定義對象
class NetworkError {
    message: string
    code: i32
}

raise NetworkError {
    message: "Connection timeout",
    code: -1
}
```

### 捕獲異常

```valkyrie
# 基本異常捕獲
try {
    risky_operation()
}
.catch {
    case _: print("Caught error: ${error}")
}

# 類型特定的異常捕獲
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

## 異常類型和模式

### 字符串異常

```valkyrie
# 簡單的字符串異常
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

### 數值異常

```valkyrie
# 使用數值作為錯誤碼
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

### 自定義異常類

```valkyrie
# 使用類來攜帶結構化錯誤數據
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
    # 執行查詢...
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

## 異常傳播

### 自動傳播

```valkyrie
# 異常會自動向傳播
micro level3() {
    raise "Error from level 3"
}

micro level2() {
    level3()  # 異常會傳播到這裡
}

micro level1() {
    level2()  # 異常繼續傳播
}

try {
    level1()
}
.catch {
    case _: print("Caught at top level: ${error}")
}
```

### 異常轉換

```valkyrie
# 捕獲並轉換異常
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

# 異常鏈
class ConfigError {
    message: string
    cause: Any
    content_preview: string
}
```

## 資源管理

### 自動清理

```valkyrie
# 使用 using 確保資源清理
micro process_file(filename: string) {
    using file = File.open(filename) {
        let content = file.read_all()
        process_content(content)
    }  # file會自動關閉
}

# 使用 defer 延遲執行
micro database_transaction() {
    let transaction = db.begin_transaction()
    defer transaction.rollback()  # 默認回滾
    
    try {
        # 執行數據庫操作
        db.insert(data1)
        db.update(data2)
        db.delete(data3)
        
        transaction.commit()
        defer.cancel()  # 取消回滾
    }
    .catch {
        case error {
            # 異常時自動回滾
            raise error
        }
    }
}
```

### 資源包裝

```valkyrie
# 自動資源管理的包裝器
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

# 使用示例
let managed_file = ManagedResource.new(
    File.open("data.txt"),
    { => file.close() }
)

managed_file.use({ $file =>
    let content = file.read_all()
    process_content(content)
})  # 文件自動關閉
```

## 異常處理模式

### 重試模式

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
            sleep(Duration.seconds(attempts))  # 指數退避
        }
    }
}

# 使用重試
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

### 斷路器模式

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

### 異常聚合

```valkyrie
# 收集多個異常
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

## 最佳實踐

### 1. 異常類型設計

```valkyrie
# 使用有意義的異常類型
class UserNotFoundError {
    user_id: string
    search_criteria: Map⟨string, string⟩
}

class PermissionDeniedError {
    user: string
    resource: string
    required_permission: string
}

# 而不是通用的字符串
# raise "User not found"  # 不推薦
```

### 2. 異常信息

```valkyrie
# 提供足夠的上下文信息
class FileProcessingError {
    filename: string
    line_number: i32
    column: i32
    error_type: string
    suggestion: string
}

micro parse_csv(filename: string) {
    try {
        # 解析邏輯
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

### 3. 異常處理策略

```valkyrie
# 在適當的層級處理異常
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

# 不要過度捕獲異常
micro bad_example() {
    try {
        some_operation()
    }
    .catch {
        case error {
            # 什麼都不做，隱藏了錯誤
        }
    }
}
```

### 4. 測試異常處理

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
            ↯assert_equal(error.field, "email")
            ↯assert_equal(error.constraint, "Email cannot be empty")
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
            ↯assert_equal(error.attempts, 3)
            ↯assert_equal(call_count, 3)
    }
}
```