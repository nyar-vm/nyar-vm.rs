
## 面向物件程式設計

Valkyrie 支援基於類別的面向物件程式設計，提供類別定義、建構函式、方法和繼承等特性。

### 特殊類別型別

- [神經網路型別 (Neural)](./neural.md) - 用於機器學習的特殊類別型別
- [界面組件型別 (Widget)](./widget.md) - 用於 UI 開發的特殊類別型別

### 欄位定義

```valkyrie
# 基本欄位定義
name: string
age: i32
is_active: bool = true  # 預設值

# 存取控制
public username: string
private password: string
protected internal_id: i64

# 唯讀欄位
readonly created_at: DateTime
```

### 類別定義 (class)

使用 `class` 關鍵字定義結構化資料型別。

```valkyrie
class Person {
    name: string
    age: i32
    
    micro constructor(self, name: string, age: i32) {
        self.name = name
        self.age = age
    }

    micro greet(self) {
        print("Hello, I'm ${self.name}")
    }
}
```

### 方法定義

```valkyrie
imply Person {
    # 實例方法
    micro say_hello(self) {
        print("Hello, I'm ${self.name}")
    }

    # 可變方法
    micro set_age(mut self, new_age: i32) {
        self.age = new_age
    }

    # 靜態方法
    micro static create_anonymous() -> Person {
        Person { name: "Anonymous", age: 0 }
    }

    # 帶回傳值的方法
    micro get_info(self) -> string {
        "${self.name} is ${self.age} years old"
    }
}
```

### 繼承 (Inheritance)

類別可以透過在類別名稱後加括號來繼承一個或多個類別。

```valkyrie
class Student(Person) {
    student_id: string
}
```


## 標誌型別 (flags)

### 基本標誌型別

```valkyrie
# 簡單標誌
flags FilePermissions {
    READ = 1,
    WRITE = 2,
    EXECUTE = 4
}

# 使用標誌
let perms = FilePermissions::READ | FilePermissions::WRITE
if perms.contains(FilePermissions::READ) {
    print("可讀")
}

# 複雜標誌
flags WindowStyle {
    RESIZABLE = 0x01,
    MINIMIZABLE = 0x02,
    MAXIMIZABLE = 0x04,
    CLOSABLE = 0x08,
    TITLEBAR = 0x10,
    BORDER = 0x20,
    
    # 組合標誌
    DEFAULT = RESIZABLE | MINIMIZABLE | MAXIMIZABLE | CLOSABLE | TITLEBAR | BORDER,
    DIALOG = CLOSABLE | TITLEBAR | BORDER
}
```

### 標誌操作

```valkyrie
flags Permissions {
    READ = 1,
    write = 2,
    execute = 4,
    
    # 方法
    micro has_read(self) -> bool {
        self.contains(Permissions::read)
    }
    
    micro add_write(mut self) {
        self |= Permissions::write
    }
    
    micro remove_execute(mut self) {
        self &= !Permissions::execute
    }
}
```

## 特徵定義 (trait)

### 基本特徵

```valkyrie
# 簡單特徵
trait Display {
    micro to_string(self) -> string
}

# 帶預設實作的特徵
trait Debug {
    micro debug(self) -> string
    
    # 預設實作
    micro print_debug(self) {
        print(self.debug())
    }
}

# 泛型特徵
trait Iterator<T> {
    micro next(mut self) -> T?
    
    # 預設方法
    micro collect(mut self) -> [T] {
        let mut result = []
        while let item = self.next()? {
            result.push(item)
        }
        result
    }
    
    micro map<U>(self, f: micro(T) -> U) -> MapIterator<T, U> {
        MapIterator::new(self, f)
    }
}
```

### 特徵實作

```valkyrie
# 為型別實作特徵
imply Person: Display {
    micro to_string(self) -> string {
        "${self.name} (${self.age} years old)"
    }
}

imply Person: Debug {
    micro debug(self) -> string {
        "Person { name: \"${self.name}\", age: ${self.age} }"
    }
}

# 條件實作
imply<T> T?: Display where T: Display {
    micro to_string(self) -> string {
        match self {
            value? => "Some(${value.to_string()})",
            _ => "None"
        }
    }
}
```

### 特徵約束

```valkyrie
# 函式中的特徵約束
micro print_items<T>(items: [T]) where T: Display {
    for item in items {
        print(item.to_string())
    }
}

# 多重約束
micro process<T>(value: T) -> string 
where 
    T: Display + Debug + Clone 
{
    let cloned = value.clone()
    "Display: ${value.to_string()}, Debug: ${cloned.debug()}"
}

# 關聯型別
trait Collect<T> {
    type Output
    
    micro collect(self) -> Self::Output
}
```

## 型別別名

```valkyrie
# 簡單型別別名
type UserId = i64
type UserName = string
type Coordinates = (f64, f64)

# 泛型型別別名
type Result<T> = Result<T, string>
type HashMap<K, V> = std::collections::HashMap<K, V>

# 函式型別別名
type Handler = micro(Request) -> Response
type Predicate<T> = micro(T) -> bool
```

## 常數定義

```valkyrie
# 基本常數
const PI: f64 = 3.14159265359
const MAX_USERS: i32 = 1000
const APP_NAME: string = "MyApp"

# 複雜常數
const DEFAULT_CONFIG: Config = Config {
    timeout: 30,
    retries: 3,
    debug: false
}

# 計算常數
const BUFFER_SIZE: usize = 1024 * 1024  # 1MB
const HALF_PI: f64 = PI / 2.0
```

## 模組定義

```valkyrie
# 模組宣告
mod utils {
    public micro helper_function() {
        # 實作
    }
    
    public class UtilityClass {
        # 實作
    }
}

# 使用模組
using utils::helper_function
using utils::UtilityClass

# 重匯出
public using utils::*
```

## 泛型定義

```valkyrie
# 泛型函式
micro swap<T>(mut a: T, mut b: T) {
    let temp = a
    a = b
    b = temp
}

# 泛型類別
class Container<T> {
    value: T
    
    new(value: T) {
        Self { value }
    }
    
    get(self) -> T {
        self.value
    }
    
    set(mut self, new_value: T) {
        self.value = new_value
    }
}

# 約束泛型
class SortedList<T> where T: Ord {
    items: [T]
    
    insert(mut self, item: T) {
        # 保持排序插入
        let pos = self.items.binary_search(item).default { $e }
        self.items.insert(pos, item)
    }
}
```


## 屬性和裝飾器

```valkyrie
# 屬性裝飾器
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64
    y: f64
}

@test
micro test_addition() {
    ↯assert_equal(2 + 2, 4)
}

@deprecated("Use new_function instead")
micro old_function() {
    # 已廢棄的函式
}

@inline
micro fast_calculation(x: i32) -> i32 {
    x * x + 2 * x + 1
}
```
