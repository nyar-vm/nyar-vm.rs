# 屬性系統 (Property System)

## 概述

Valkyrie 的屬性系統提供了一種優雅的方式來封裝字段訪問，允許開發者定義 getter 和 setter 方法，同時保持簡潔的訪問語法。屬性系統遵循統一訪問原則，使得字段訪問和方法調用在語法上保持一致。

## 基本屬性定義

### Getter 屬性

```valkyrie
class Rectangle {
    width: f64,
    height: f64,
    
    # 計算屬性 - 面積
    get area(self) -> f64 {
        self.width * self.height
    }
    
    # 計算屬性 - 周長
    get perimeter(self) -> f64 {
        2.0 * (self.width + self.height)
    }
}

# 使用方式
let rect = Rectangle { width: 10.0, height: 5.0 }
let area = rect.area        # 調用 getter，返回 50.0
let perimeter = rect.perimeter  # 調用 getter，返回 30.0
```

### Setter 屬性

```valkyrie
class Temperature {
    celsius: f64,
    
    # Getter - 獲取華氏溫度
    get fahrenheit(self) -> f64 {
        self.celsius * 9.0 / 5.0 + 32.0
    }
    
    # Setter - 設置華氏溫度
    set fahrenheit(mut self, value: f64) {
        self.celsius = (value - 32.0) * 5.0 / 9.0
    }
    
    # Getter - 獲取開爾文溫度
    get kelvin(self) -> f64 {
        self.celsius + 273.15
    }
    
    # Setter - 設置開爾文溫度
    set kelvin(mut self, value: f64) {
        self.celsius = value - 273.15
    }
}

# 使用方式
let mut temp = Temperature { celsius: 25.0 }
print("攝氏度: ${temp.celsius}")     # 25.0
print("華氏度: ${temp.fahrenheit}")   # 77.0
print("開爾文: ${temp.kelvin}")      # 298.15

# 通過 setter 修改溫度
temp.fahrenheit = 86.0  # 設置華氏溫度
print("攝氏度: ${temp.celsius}")     # 30.0

temp.kelvin = 300.0     # 設置開爾文溫度
print("攝氏度: ${temp.celsius}")     # 26.85
```

## 只讀和只寫屬性

### 只讀屬性

```valkyrie
class Person {
    first_name: string,
    last_name: string,
    birth_year: u32,
    
    # 只讀屬性 - 全名
    get full_name(self) -> string {
        "${self.first_name} ${self.last_name}"
    }
    
    # 只讀屬性 - 年齡（基於當前年份）
    get age(self) -> u32 {
        2024 - self.birth_year  # 簡化示例
    }
}

let person = Person {
    first_name: "張",
    last_name: "三",
    birth_year: 1990
}

print(person.full_name)  # "張 三"
print(person.age)        # 34
# person.full_name = "李四"  # 編譯錯誤：沒有 setter
```

### 只寫屬性

```valkyrie
class Logger {
    messages: [string],
    
    # 只寫屬性 - 添加日誌消息
    set message(mut self, msg: string) {
        let timestamp = get_current_timestamp()
        self.messages.push("[${timestamp}] ${msg}")
    }
    
    # 獲取所有消息的方法
    micro get_messages(self) -> &[string] {
        &self.messages
    }
}

let mut logger = Logger { messages: [] }
logger.message = "系統啟動"  # 使用 setter
logger.message = "用戶登錄"  # 使用 setter
# let msg = logger.message   # 編譯錯誤：沒有 getter
```

## 屬性驗證

```valkyrie
class BankAccount {
    balance: f64,
    min_balance: f64,
    
    # 帶驗證的 setter
    set balance(mut self, value: f64) {
        if value < self.min_balance {
            panic("餘額不能低於最低限額: ${self.min_balance}")
        }
        self.balance = value
    }
    
    get balance(self) -> f64 {
        self.balance
    }
}

let mut account = BankAccount {
    balance: 1000.0,
    min_balance: 100.0
}

account.balance = 500.0   # 正常設置
# account.balance = 50.0  # 運行時 panic
```

## 懶加載屬性

```valkyrie
class DataProcessor {
    raw_data: [string],
    processed_data: [ProcessedItem]?,
    
    # 懶加載的計算屬性
    get processed(mut self) -> &[ProcessedItem] {
        if self.processed_data.is_none() {
            let processed = self.raw_data
                .iter()
                .map { $item -> self.process_item($item) }
                .collect()
            self.processed_data = Some(processed)
        }
        
        if let Some(ref data) = self.processed_data {
            data
        } else {
            unreachable!()
        }
    }
    
    micro process_item(self, item: &string) -> ProcessedItem {
        # 複雜的處理邏輯
        ProcessedItem::from(item)
    }
}
```

## 屬性鏈式調用

```valkyrie
class Builder {
    name: string?,
    age: u32?,
    email: string?,
    
    # 鏈式 setter
    set name(mut self, value: string) -> Self {
        self.name = Some(value)
        self
    }
    
    set age(mut self, value: u32) -> Self {
        self.age = Some(value)
        self
    }
    
    set email(mut self, value: string) -> Self {
        self.email = Some(value)
        self
    }
    
    micro build(self) -> Person {
        Person {
            name: self.name.unwrap_or("Unknown"),
            age: self.age.unwrap_or(0),
            email: self.email.unwrap_or("unknown↯example.com")
        }
    }
}

# 鏈式調用
let person = Builder::new()
    .name("Alice")
    .age(30)
    .email("alice↯example.com")
    .build()
```

## 靜態屬性

```valkyrie
class MathConstants {
    # 靜態只讀屬性
    static get pi() -> f64 {
        3.14159265359
    }
    
    static get e() -> f64 {
        2.71828182846
    }
    
    # 靜態可變屬性
    static mut counter: u32 = 0
    
    static get next_id() -> u32 {
        Self::counter += 1
        Self::counter
    }
}

# 使用靜態屬性
let pi_value = MathConstants::pi
let id1 = MathConstants::next_id  # 1
let id2 = MathConstants::next_id  # 2
```

## 屬性重寫

```valkyrie
class Shape {
    # 虛擬屬性
    virtual get area(self) -> f64 {
        0.0
    }
}

class Circle: Shape {
    radius: f64,
    
    # 重寫父類屬性
    override get area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
}

class Square: Shape {
    side: f64,
    
    # 重寫父類屬性
    override get area(self) -> f64 {
        self.side * self.side
    }
}
```

## 最佳實踐

### 1. 使用屬性進行數據封裝

```valkyrie
# 好的實踐：使用屬性封裝內部狀態
class Counter {
    value: u32,
    
    get count(self) -> u32 {
        self.value
    }
    
    set count(mut self, new_value: u32) {
        if new_value > 1000 {
            panic("計數器值不能超過 1000")
        }
        self.value = new_value
    }
    
    micro increment(mut self) {
        self.count = self.count + 1
    }
}
```

### 2. 避免副作用過大的 Getter

```valkyrie
# 避免：getter 中有複雜的副作用
class BadExample {
    get data(mut self) -> [string] {
        # 不好：每次訪問都重新計算
        expensive_computation()
    }
}

# 推薦：使用懶加載或緩存
class GoodExample {
    cached_data: [string]?,
    
    get data(mut self) -> &[string] {
        if self.cached_data.is_none() {
            self.cached_data = Some(expensive_computation())
        }
        self.cached_data.as_ref().unwrap()
    }
}
```

### 3. 保持屬性語義的一致性

```valkyrie
class Rectangle {
    width: f64,
    height: f64,
    
    # 好的實踐：getter 和 setter 操作相同的概念
    get area(self) -> f64 {
        self.width * self.height
    }
    
    # 如果提供 area setter，應該合理地更新 width 和 height
    set area(mut self, new_area: f64) {
        let ratio = (new_area / self.area).sqrt()
        self.width *= ratio
        self.height *= ratio
    }
}
```

## 總結

Valkyrie 的屬性系統提供了：

1. **統一訪問原則**：字段和計算屬性使用相同的訪問語法
2. **數據封裝**：通過 getter/setter 控制數據訪問
3. **計算屬性**：支持動態計算的屬性值
4. **驗證機制**：在 setter 中添加數據驗證邏輯
5. **懶加載**：支持延遲計算和緩存
6. **鏈式調用**：支持流暢的 API 設計

正確使用屬性系統可以提高代碼的封裝性、可維護性和用戶體驗。
