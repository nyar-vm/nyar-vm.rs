# 錯誤處理

Valkyrie 使用 `try` 語句和 `catch` 機制來處理錯誤。`try` 是一個獨立的語句，可以與類型系統結合使用。

## Try 語句

### 基本 Try 語法

```valkyrie
# try 是獨立語句，返回 Result 類型
let result = try Result⟨string⟩ {
    read_file("config.txt")?
}

# 帶錯誤類型的 try
let data = try Result⟨Data, ParseError⟩ {
    let content = read_file("data.json")?
    parse_json(content)?
}

# 簡化形式
let value = try {
    risky_operation()?
}
```

### Try 與可選類型

```valkyrie
# 處理可能失敗的操作
let maybe_value = try i32? {
    let input = get_user_input()?
    parse_number(input)?
}

# 鏈式操作
let result = try User? {
    let id = extract_user_id(request)?
    let user = find_user_by_id(id)?
    validate_user(user)?
}
```

## Catch 處理

### 非主幹控制流 Catch

```valkyrie
# 使用 .catch 處理錯誤
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

# 命名 catch
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

### Match 風格的 Catch

```valkyrie
# catch 和 match 是對偶的，能力一模一樣
let user_data = try Result⟨UserData⟩ {
    let raw = fetch_user_data(user_id)?
    validate_and_parse(raw)?
}
.catch {
    case ValidationError { field, message }:
        show_field_error(field, message)
        UserData::guest()
    case NetworkError { code, .. } if code >= 500:
        # 服務器錯誤，稍後重試
        schedule_retry()
        UserData::cached(user_id)
    case NetworkError { code, .. } if code >= 400:
        # 客戶端錯誤
        UserData::error(code)
    else: UserData::unknown_error()
}
```

## 錯誤傳播

### 問號操作符

```valkyrie
# ? 操作符用於錯誤傳播
micro process_file(path: string) -> Result⟨string, FileError⟩ {
    let content = read_file(path)?  # 如果失敗，直接返回錯誤
    let processed = transform_content(content)?
    validate_result(processed)?
}

# 在 try 塊中使用
let final_result = try Result⟨ProcessedData⟩ {
    let raw = fetch_data()?
    let cleaned = clean_data(raw)?
    let validated = validate_data(cleaned)?
    process_final(validated)?
}
```

### 錯誤轉換

```valkyrie
# 自動錯誤轉換
micro read_and_parse(path: string) -> Result⟨Config, AppError⟩ {
    try Result⟨Config, AppError⟩ {
        let content = read_file(path)?  # FileError -> AppError
        let config = parse_json(content)?  # ParseError -> AppError
        validate_config(config)?  # ValidationError -> AppError
    }
}

# 手動錯誤轉換
let result = try Result⟨Data⟩ {
    fetch_data().map_err { $e -> AppError::Network($e) }?
}
```

## 自定義錯誤類型

```valkyrie
# 定義錯誤類型
union AppError {
    Network(NetworkError),
    Parse(ParseError),
    Validation { field: string, message: string },
    IO(IOError)
}

# 實現錯誤轉換
imply AppError: From⟨NetworkError⟩ {
    micro from(err: NetworkError) -> AppError {
        AppError::Network(err)
    }
}

# 使用自定義錯誤
micro load_user_config(user_id: string) -> Result⟨UserConfig, AppError⟩ {
    try Result⟨UserConfig, AppError⟩ {
        let path = get_config_path(user_id)?
        let content = read_file(path)?
        let config = parse_config(content)?
        validate_user_config(config)?
    }
}
```

## 錯誤恢復模式

### 回退策略

```valkyrie
# 多級回退
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

# 重試機制
let data = try Result⟨Data⟩ {
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

### 部分恢復

```valkyrie
# 處理部分失敗
let results = try Result⟨[ProcessedItem]⟩ {
    items.map { $item ->
        try ProcessedItem? {
            process_item($item)?
        }
        .catch {
            case ProcessingError(msg):
                log_warning("Skipping item: ${msg}")
                null  # 跳過失敗的項目
            else: null
        }
    }).filter_map { $x }.collect()
}
```

## 最佳實踐

### 1. 錯誤類型設計

```valkyrie
# 自定義驗證錯誤
union ValidationError {
    InvalidFormat { field: string, expected: string },
    ValueOutOfRange { field: string, min: i32, max: i32 },
    RequiredFieldMissing { field: string }
}

# 驗證效果定義
effect Validation {
    micro validate_user(user: User) -> Result⟨(), [ValidationError]⟩
}

# 實現驗證邏輯
micro check_user(user: User) -> Result⟨(), [ValidationError]⟩ {
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
        Fine(())
    } else {
        Fail(errors)
    }
}

# 上下文信息
class ContextualError {
    operation: string,
    context: Map⟨string, string⟩,
    source: Error
}
```

### 2. 錯誤處理策略

```valkyrie
# 就近處理
micro validate_user_input(input: UserInput) -> Result⟨ValidatedInput, ValidationError⟩ {
    try Result⟨ValidatedInput⟩ {
        let email = validate_email(input.email)?
        let age = validate_age(input.age)?
        let name = validate_name(input.name)?
        class: ValidatedInput { email, age, name }
    }
}

# 統一錯誤處理
micro main() {
    let result = try Result⟨()⟩ {
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

### 3. 資源管理

```valkyrie
# 使用 RAII 模式
class FileHandle {
    path: string,
    handle: File
}

impl Drop for FileHandle {
    micro drop() {
        self.handle.close()
    }
}

# 安全的資源使用
micro process_file_safely(path: string) -> Result⟨string, FileError⟩ {
    try Result⟨string⟩ {
        let file = FileHandle::open(path)?
        let content = file.read_all()?
        process_content(content)?
    }  # file 自動關閉
}
```

通過 Effect 系統實現的錯誤處理提供了類型安全、結構化且易於組合的錯誤管理能力，使得應用程序的健壯性得到了顯著提升。