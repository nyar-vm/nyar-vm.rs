# Trait 系統 (Trait System)

## 概述

Valkyrie 的 Trait 系統提供了強大的抽象機制，支持接口定義、默認實現、多重繼承和匿名 trait。Trait 系統是 Valkyrie 面向對象編程的核心組成部分。

## 基本 Trait 定義

Valkyrie 使用 `trait` 關鍵字定義接口。

### 簡單 Trait

```valkyrie
trait Display {
    micro fmt(self) -> string
}

trait Clone {
    micro clone(self) -> Self
}

trait Debug {
    micro debug_fmt(self) -> string {
        # 默認實現
        ↯format("{}↯{:p}", self.type_name(), &self)
    }
}
```

### 帶關聯類型的 Trait

```valkyrie
trait Iterator {
    type Item
    
    micro next(mut self) -> Self::Item?
    
    micro collect⟨C: FromIterator⟨Self::Item⟩⟩(self) -> C {
        C::from_iter(self)
    }
}

trait FromIterator⟨T⟩ {
    micro from_iter⟨I: Iterator⟨Item = T⟩⟩(iter: I) -> Self
}
```

### 帶約束的 Trait

```valkyrie
trait PartialEq⟨Rhs = Self⟩ {
    micro eq(self, other: &Rhs) -> bool
    
    micro ne(self, other: &Rhs) -> bool {
        !self.eq(other)
    }
}

trait Ord: PartialEq + PartialOrd {
    micro cmp(self, other: &Self) -> Ordering
}
```

## Trait 實現 (imply)

Valkyrie 使用 `imply` 關鍵字為特定類型實現 Trait。

### 基本實現

```valkyrie
class Point {
    x: f64,
    y: f64,
}

imply Point: Display {
    micro fmt(self, f: mut Formatter) -> Result {
        f.write(↯format("({}, {})", self.x, self.y))
    }
}

imply Point: Clone {
    micro clone(self) -> Self {
        Point { x: self.x, y: self.y }
    }
}
```

### 泛型實現

泛型使用數學角括號 `⟨ ⟩`。

```valkyrie
imply⟨T: Display⟩ [T]: Display {
    micro fmt(self) -> string {
        let items = self.iter()
            .map { $item.fmt() }
            .collect::⟨[string]⟩()
            .join(", ")
        ↯format("[{}]", items)
    }
}

imply⟨T: Clone⟩ [T]: Clone {
    micro clone(self) -> Self {
        self.iter().map { $item.clone() }.collect()
    }
}
```

### 條件實現

```valkyrie
imply⟨T: PartialEq⟩ [T]: PartialEq {
    micro eq(self, other: Self) -> bool {
        self.len() == other.len() && 
        self.iter().zip(other.iter()).all { $a.eq($b) }
    }
}
```

## 匿名 Trait

Valkyrie 支持匿名 trait，可以在函數參數中直接定義：

```valkyrie
# 匿名 trait 作為參數
micro process_drawable(drawable: ftrait {
    micro draw(self)
    micro get_bounds(self) -> Rectangle
}) {
    let bounds = drawable.get_bounds()
    print("Drawing object with bounds: {}", bounds)
    drawable.draw()
}

# 使用匿名 trait
let circle = class {
    radius: f64,
    
    micro draw(self) {
        print("Drawing circle with radius {}", self.radius)
    }
    
    micro get_bounds(self) -> Rectangle {
        Rectangle::new(-self.radius, -self.radius, 
                      self.radius * 2, self.radius * 2)
    }
}

process_drawable(circle { radius: 5.0 })
```

### 匿名 Trait 繼承

```valkyrie
# 繼承現有 trait 的匿名 trait
micro handle_serializable(obj: ftrait(Display, Clone) {
    micro serialize(self) -> string
}) {
    print("Object: {}", obj.fmt())
    let cloned = obj.clone()
    let serialized = obj.serialize()
    print("Serialized: {}", serialized)
}
```

## Trait 對象

### 動態分發

```valkyrie
trait Animal {
    micro make_sound(self)
    micro name(self) -> string
}

class Dog {
    name: string,
}

imply Dog: Animal {
    micro make_sound(self) {
        print("Woof!")
    }
    
    micro name(self) -> string {
        self.name.clone()
    }
}

class Cat {
    name: string,
}

imply Cat: Animal {
    micro make_sound(self) {
        print("Meow!")
    }
    
    micro name(self) -> string {
        self.name.clone()
    }
}

# 使用 trait 對象
let animals: [Animal] = [
    Dog { name: "Buddy" },
    Cat { name: "Whiskers" },
]

for animal in animals {
    print("{} says:", animal.name())
    animal.make_sound()
}
```

# Trait 對象安全

```valkyrie
# 對象安全的 trait
trait Draw {
    micro draw(self)  # 接收 self，對象安全
}

# 非對象安全的 trait
trait Clone {
    micro clone(self) -> Self  # 返回 Self，非對象安全
}

# 使用 where 子句限制
trait Container {
    type Item
    
    micro get(self, index: usize) -> Self::Item?
    
    # 只有當 Self::Item 實現了 Display 時才能調用
    micro display_item(self, index: usize) 
    where Self::Item: Display {
        if let item = self.get(index)? {
            print("{}", item.fmt())
        }
    }
}
```

## 高級特性

### 關聯常量

```valkyrie
trait MathConstants {
    const PI: f64 = 3.14159265359
    const E: f64 = 2.71828182846
    
    micro circle_area(radius: f64) -> f64 {
        Self::PI * radius * radius
    }
}

class Calculator {}

imply Calculator: MathConstants {}

let area = Calculator::circle_area(5.0)
```

### 高階 Trait 邊界

```valkyrie
# 高階 trait 邊界
micro map_closure⟨F, T, U⟩(items: [T], f: F) -> [U]
where
    F: for⟨'a⟩ Fn(T) -> U,
{
    items.iter().map(f).collect()
}

# 使用示例
let numbers = [1, 2, 3, 4, 5]
let doubled = map_closure(numbers, micro(x) { x * 2 })
```

### Trait 別名

```valkyrie
# 定義 trait 別名
trait Printable = Display + Debug + Clone

# 使用 trait 別名
micro print_info⟨T: Printable⟩(item: T) {
    print("Display: {}", item.fmt())
    print("Debug: {}", item.debug_fmt())
    let cloned = item.clone()
    print("Cloned: {}", cloned.fmt())
}
```

## 派生宏

Valkyrie 提供了自動派生常用 trait 的宏：

```valkyrie
@derive(Debug, Clone, PartialEq, Eq, Hash)
class User {
    id: u64,
    name: string,
    email: string,
}

@derive(Display)
class Point {
    x: f64,
    y: f64,
}

# 自定義派生行為
@derive(Debug, Clone)
@derive_display(format = "User({})", field = "name")
class SimpleUser {
    name: string,
    internal_id: u64,  # 不會在 Display 中顯示
}
```

## 最佳實踐

### 1. Trait 設計原則

```valkyrie
# 好的設計：單一職責
trait Readable {
    micro read(mut self, buffer: mut [u8]) -> Result⟨usize, Error⟩
}

trait Writable {
    micro write(self, data: [u8]) -> Result⟨usize, Error⟩
}

# 組合使用
trait ReadWrite: Readable + Writable {}
```

### 2. 使用關聯類型 vs 泛型參數

```valkyrie
# 使用關聯類型：每個類型只有一個實現
trait Iterator {
    type Item
    micro next(mut self) -> Self::Item?
}

# 使用泛型參數：可以有多個實現
trait From⟨T⟩ {
    micro from(value: T) -> Self
}

# string 可以從多種類型轉換
imply string: From⟨string⟩ { ... }
imply string: From⟨char⟩ { ... }
imply string: From⟨[char]⟩ { ... }
```

### 3. 錯誤處理

```valkyrie
trait TryFrom⟨T⟩ {
    type Error
    
    micro try_from(value: T) -> Result⟨Self, Self::Error⟩
}

trait TryInto⟨T⟩ {
    type Error
    
    micro try_into(self) -> Result⟨T, Self::Error⟩
}

# 自動實現
imply⟨T, U⟩ T: TryInto⟨U⟩ 
where U: TryFrom⟨T⟩ {
    type Error = U::Error
    
    micro try_into(self) -> Result⟨U, Self::Error⟩ {
        U::try_from(self)
    }
}
```

## 總結

Valkyrie 的 trait 系統提供了：

1. **靈活的抽象**：通過 trait 定義行為接口
2. **代碼復用**：通過默認實現和泛型
3. **類型安全**：編譯時檢查 trait 邊界
4. **動態分發**：通過 trait 對象支持運行時多態
5. **匿名 trait**：支持臨時的行為定義
6. **組合能力**：通過 trait 邊界組合多個能力
