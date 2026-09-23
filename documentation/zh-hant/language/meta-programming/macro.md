# 宏系統 (Macro System)

## 概述

Valkyrie 提供了強大的宏系統，支持編譯時代碼生成和元編程。宏系統分為兩個主要部分：

- **Macro (`↯`)**: 編譯時函數調用，不捕捉後續參數
- **Annotation (`@`)**: 編譯時註解，會捕捉並作用於後續的 `class`、`micro` 等聲明

## Macro vs Annotation

### Macro (`↯`)

Macro 使用 `↯` 前綴，是編譯時的函數調用，不會捕捉後續的代碼元素：

```valkyrie
# 編譯時常量計算
let FIBONACCI_10: i32 = ↯evaluate(fibonacci(10))
let LOOKUP_TABLE: [i32; 256] = ↯evaluate(generate_lookup_table())

# 環境變量獲取
let database_url: string = ↯env("DATABASE_URL")

# 字符串格式化
let message: string = ↯format("Hello, {}!", name)

# 向量創建
let numbers = [1, 2, 3, 4, 5]
let zeros = [0; 10]

# SQL 查詢
let query = ↯sql(
    "SELECT id, name FROM users WHERE active = $1",
    true
)
```

### Annotation (`@`)

Annotation 使用 `@` 前綴，會捕捉並作用於後續的聲明：

```valkyrie
# 測試註解
@test
micro test_addition() {
    @assert_eq(2 + 2, 4)
}

# 序列化註解
@derive(Serialize, Deserialize)
class User {
    name: string
    email: string
}

# 性能測試註解
@benchmark
micro fibonacci_benchmark() {
    fibonacci(30)
}

# 條件編譯註解
@cfg(feature = "debug")
micro debug_function() {
    print("Debug mode enabled")
}
```

## 常用 Macro

### 編譯時計算

```valkyrie
# 編譯時常量計算
let PI_SQUARED: f64 = ↯evaluate(3.14159 * 3.14159)

# 編譯時文件讀取
let config_content: string = ↯compile_time_read_file("config.toml")

# 編譯時環境配置
↯compile_time_env {
    memory_limit: "256MB",
    execution_timeout: "30s",
}
```


### 代碼生成

```valkyrie
# 模板定義
↯template {
    name: "crud_operations",
    params: [Entity: Type, Key: Type],
    body: {
        micro create(entity: Entity) -> Result⟨Key, Any⟩ {
            # 創建實體的通用邏輯
        }
        
        micro read(key: Key) -> Result⟨Entity, Any⟩ {
            # 讀取實體的通用邏輯
        }
        
        micro update(key: Key, entity: Entity) -> Result⟨unit, Any⟩ {
            # 更新實體的通用邏輯
        }
        
        micro delete(key: Key) -> Result⟨unit, Any⟩ {
            # 刪除實體的通用邏輯
        }
    }
}

# 模板實例化
↯generate_code {
    crud_operations⟨User, UserId⟩
    crud_operations⟨Product, ProductId⟩
}
```

### 宏展開控制

```valkyrie
# 宏展開策略控制
↯macro_expansion(strategy: "eager", max_depth: 100)
macro recursive_macro {
    # 遞歸宏定義
}
```

## 常用 Annotation

### 測試相關

```valkyrie
@test
micro test_user_creation() {
    let user = User("Alice", "alice↯example.com")
    @assert_true(user.is_valid())
    @assert_eq(user.name, "Alice")
}

@test
@should_panic
micro test_invalid_email() {
    User("Bob", "invalid-email")
}
```

### 派生註解

```valkyrie
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64,
    y: f64,
}

@derive(Serialize, Deserialize)
class Config {
    database_url: string
    port: u16
}
```

### 條件編譯

```valkyrie
@cfg(target_os = "windows")
micro windows_specific_function() {
    # Windows 特定實現
}

@cfg(feature = "async")
class AsyncHandler {
    # 異步處理器實現
}
```

## 自定義宏

### 聲明式宏

```valkyrie
macro vec_of {
    ($elem:expr; $n:expr) => {
        {
            let mut v = []
            for _ in 0..$n {
                v.push($elem)
            }
            v
        }
    }
    (#(#x:expr),+ #(,)?) => {
        ↯vec(#(#x),+)
    }
}
```

### 過程宏

過程宏是更強大的元編程工具，允許直接操作 AST 或 TokenStream。詳細內容請參考 [過程宏開發指南](./procedural-macros.md)。
