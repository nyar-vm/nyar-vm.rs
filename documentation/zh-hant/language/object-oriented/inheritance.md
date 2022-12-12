# 多重繼承系統 (Multiple Inheritance)

## 概述

Valkyrie 支援多重繼承，允許一個類別同時繼承多個父類別。多重繼承使用 C3 線性化演算法來解決方法解析順序 (Method Resolution Order, MRO) 問題，確保繼承的一致性和可預測性。

## 基本多重繼承語法

### 簡單多重繼承

```valkyrie
class A {
    micro method_a(self) {
        print("Method from A")
    }
    
    micro common_method(self) {
        print("A's common method")
    }
}

class B {
    micro method_b(self) {
        print("Method from B")
    }
    
    micro common_method(self) {
        print("B's common method")
    }
}

class C {
    micro method_c(self) {
        print("Method from C")
    }
    
    micro common_method(self) {
        print("C's common method")
    }
}

# 多重繼承語法：class 子類別(父類別1, 父類別2, ...)
class MultiChild(A, B, C) {
    micro own_method(self) {
        print("MultiChild's own method")
    }
}
```

### 重新命名繼承

當多個父類別有同名方法時，可以使用重新命名語法來避免衝突：

```valkyrie
class Display {
    micro show(self) {
        print("Display show")
    }
}

class Printer {
    micro show(self) {
        print("Printer show")
    }
    
    micro print(self) {
        print("Printing...")
    }
}

# 重新命名繼承語法：class 子類別(rename: 父類別, 其他父類別)
class Document(rename: Display, Printer) {
    micro display_document(self) {
        # 透過重新命名存取 Display 的方法
        self.rename.show()  # 呼叫 Display::show
        self.print()        # 呼叫 Printer::print
        self.show()         # 呼叫 Printer::show (C3 線性化的第一個匹配)
    }
}
```

### 複雜重新命名場景

```valkyrie
class FileReader {
    micro read(self) -> string {
        "Reading from file"
    }
    
    micro close(self) {
        print("Closing file")
    }
}

class NetworkReader {
    micro read(self) -> string {
        "Reading from network"
    }
    
    micro close(self) {
        print("Closing connection")
    }
}

class Logger {
    micro log(self, message: string) {
        print("Log: {}", message)
    }
}

# 多重重新命名
class HybridReader(file_reader: FileReader, net_reader: NetworkReader, Logger) {
    micro read_from_file(self) -> string {
        let content = self.file_reader.read()
        self.log(↯format("Read from file: {}", content))
        content
    }
    
    micro read_from_network(self) -> string {
        let content = self.net_reader.read()
        self.log(↯format("Read from network: {}", content))
        content
    }
    
    micro cleanup(self) {
        self.file_reader.close()
        self.net_reader.close()
    }
}
```

## C3 線性化演算法

Valkyrie 使用 C3 線性化演算法來確定方法解析順序：

```valkyrie
class A {
    micro method(self) { print("A") }
}

class B(A) {
    micro method(self) { print("B") }
}

class C(A) {
    micro method(self) { print("C") }
}

class D(B, C) {
    # 沒有重寫 method
}

# C3 線性化順序：D -> B -> C -> A
# 呼叫 d.method() 會呼叫 B::method
let d = D {}
d.method()  # 輸出："B"
```

### 線性化順序範例

```valkyrie
class Base {
    micro base_method(self) { print("Base") }
}

class Left(Base) {
    micro left_method(self) { print("Left") }
    micro common_method(self) { print("Left common") }
}

class Right(Base) {
    micro right_method(self) { print("Right") }
    micro common_method(self) { print("Right common") }
}

class Middle(Left, Right) {
    micro middle_method(self) { print("Middle") }
}

class Final(Middle, Right) {
    # C3 線性化：Final -> Middle -> Left -> Right -> Base
    micro test_resolution(self) {
        self.common_method()  # 呼叫 Left::common_method
        self.left_method()    # 呼叫 Left::left_method
        self.right_method()   # 呼叫 Right::right_method
        self.base_method()    # 呼叫 Base::base_method
    }
}
```

## 方法存取模式

### 直接存取

```valkyrie
class Child(A, B, C) {
    micro test_access(self) {
        # 直接呼叫，使用 C3 線性化順序
        self.common_method()  # 呼叫第一個匹配的方法
        
        # 透過重新命名存取特定父類別的方法
        # 注意：沒有 super 關鍵字
    }
}
```

### 重新命名存取

```valkyrie
class AdvancedChild(primary: A, secondary: B, tertiary: C) {
    micro demonstrate_access(self) {
        # 透過重新命名存取特定父類別
        self.primary.common_method()    # 呼叫 A::common_method
        self.secondary.common_method()  # 呼叫 B::common_method
        self.tertiary.common_method()   # 呼叫 C::common_method
        
        # 直接存取使用 C3 線性化
        self.common_method()  # 呼叫 A::common_method (第一個)
    }
}
```

## 匿名類別繼承

Valkyrie 支援匿名類別的繼承：

```valkyrie
# 匿名類別繼承
micro process_shape(shape: class(Drawable, Movable) {
    micro area(self) -> f64
}) {
    shape.draw()
    shape.move_to(10.0, 20.0)
    print("Area: {}", shape.area())
}

# 使用匿名類別
let circle = class(Drawable, Movable) {
    radius: f64,
    x: f64,
    y: f64,
    
    micro area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
}

process_shape(circle { radius: 5.0, x: 0.0, y: 0.0 })
```

### 匿名類別重新命名繼承

```valkyrie
# 匿名類別的重新命名繼承
micro create_hybrid_processor() -> class(reader: FileReader, writer: FileWriter) {
    micro process(self, filename: string) {
        let content = self.reader.read_file(filename)
        let processed = content.to_uppercase()
        self.writer.write_file(filename + ".processed", processed)
    }
}

let processor = create_hybrid_processor() {
    # 匿名類別實作
}

## 建構函式和初始化

```valkyrie
class Parent1 {
    value1: i32,
    
    micro new(v1: i32) -> Self {
        Self { value1: v1 }
    }
}

class Parent2 {
    value2: string,
    
    micro new(v2: string) -> Self {
        Self { value2: v2 }
    }
}

class MultiInherit(Parent1, Parent2) {
    own_value: f64,
    
    # 多重繼承的建構函式
    micro new(v1: i32, v2: string, own: f64) -> Self {
        Self {
            # 父類別欄位初始化
            value1: v1,
            value2: v2,
            # 自己的欄位
            own_value: own,
        }
    }
}
```

## 抽象類別和界面

```valkyrie
# 抽象基類
abstract class Shape {
    abstract micro area(self) -> f64
    abstract micro perimeter(self) -> f64
    
    # 具體方法
    micro describe(self) {
        print("Area: {}, Perimeter: {}", self.area(), self.perimeter())
    }
}

# 界面定義
trait Drawable {
    micro draw(self)
    micro set_color(self, color: Color)
}

# 多重繼承：抽象類別 + 界面
class Rectangle(Shape): Drawable {
    width: f64,
    height: f64,
    color: Color,
    
    # 實作抽象方法
    micro area(self) -> f64 {
        self.width * self.height
    }
    
    micro perimeter(self) -> f64 {
        2.0 * (self.width + self.height)
    }
    
    # 實作界面方法
    micro draw(self) {
        print("Drawing rectangle {}x{}", self.width, self.height)
    }
    
    micro set_color(self, color: Color) {
        self.color = color
    }
}
```

## 鑽石問題解決

```valkyrie
class GrandParent {
    micro method(self) { print("GrandParent") }
}

class Parent1(GrandParent) {
    micro method(self) { print("Parent1") }
}

class Parent2(GrandParent) {
    micro method(self) { print("Parent2") }
}

# 鑽石繼承
class Child(Parent1, Parent2) {
    # C3 線性化自動解決鑽石問題
    # 線性化順序：Child -> Parent1 -> Parent2 -> GrandParent
    
    micro test_diamond(self) {
        self.method()  # 呼叫 Parent1::method
    }
    
    # 如果需要呼叫特定父類別的方法，使用重新命名
}

# 使用重新命名解決鑽石問題
class ResolvedChild(p1: Parent1, p2: Parent2) {
    micro test_resolved(self) {
        self.p1.method()  # 明確呼叫 Parent1::method
        self.p2.method()  # 明確呼叫 Parent2::method
    }
}
```

## 最佳實踐

### 1. 優先使用組合而非繼承

```valkyrie
# 好的設計：組合
class Document {
    reader: FileReader,
    writer: FileWriter,
    logger: Logger,
    
    micro process(self) {
        let content = self.reader.read()
        let processed = self.transform(content)
        self.writer.write(processed)
        self.logger.log("Document processed")
    }
}

# 避免：過度繼承
# class Document(FileReader, FileWriter, Logger) { ... }
```

### 2. 使用重新命名避免方法衝突

```valkyrie
# 清晰的重新命名
class MediaPlayer(audio: AudioPlayer, video: VideoPlayer) {
    micro play_audio(self, file: String) {
        self.audio.play(file)
    }
    
    micro play_video(self, file: String) {
        self.video.play(file)
    }
}
```

### 3. 文件化繼承關係

```valkyrie
# 清晰的繼承文件
@doc("""
MultiProcessor 繼承關係：
- DataProcessor: 提供資料處理能力
- NetworkHandler: 提供網路通訊能力
- Logger: 提供日誌記錄能力

C3 線性化順序：MultiProcessor -> DataProcessor -> NetworkHandler -> Logger
""")
class MultiProcessor(DataProcessor, NetworkHandler, Logger) {
    # 實作
}
```

## 總結

Valkyrie 的多重繼承系統特點：

1. **C3 線性化**：確保方法解析的一致性
2. **重新命名機制**：解決方法名衝突
3. **無 super 關鍵字**：透過重新命名明確存取父類別方法
4. **匿名類別支援**：支援臨時的多重繼承類別定義
5. **型別安全**：編譯時檢查繼承關係的合法性

正確使用多重繼承可以實現靈活的程式碼複用，但應該謹慎使用，優先考慮組合和界面設計。
