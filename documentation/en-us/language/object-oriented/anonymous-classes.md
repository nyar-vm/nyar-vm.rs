# 匿名类 (Anonymous Classes)

## 概述

Valkyrie 支持匿名类，允许在需要时临时定义类而无需显式声明。匿名类特别适用于回调函数、临时对象创建和函数式编程场景。

## 基本匿名类语法

### 简单匿名类

```valkyrie
# 匿名类语法：class { 字段和方法定义 }
micro create_point() -> class {
    x: f64,
    y: f64,
    
    micro distance_from_origin(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
} {
    class {
        x: 10.0,
        y: 20.0,
        
        micro distance_from_origin(self) -> f64 {
            (self.x * self.x + self.y * self.y).sqrt()
        }
    }
}

# 使用匿名类
let point = create_point()
let distance = point.distance_from_origin()
print("Distance: {}", distance)
```

### 匿名类作为参数

```valkyrie
# 接受匿名类作为参数
micro process_drawable(drawable: class {
    micro draw(self)
    micro get_area(self) -> f64
}) {
    print("Area: {}", drawable.get_area())
    drawable.draw()
}

# 传递匿名类实例
process_drawable(class {
    radius: f64,
    
    micro draw(self) {
        print("Drawing circle with radius {}", self.radius)
    }
    
    micro get_area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
} { radius: 5.0 })
```

### 匿名类与闭包的区别

```valkyrie
# 闭包语法：{ 参数 表达式 }
let closure = { $x $x * 2 }
let result = closure(5)  # 结果：10

# 匿名类语法：class { 字段和方法 }
let anonymous_obj = class {
    multiplier: i32,
    
    micro multiply(self, x: i32) -> i32 {
        x * self.multiplier
    }
} { multiplier: 2 }

let result = anonymous_obj.multiply(5)  # 结果：10
```

## 匿名类继承

### 继承具名类

```valkyrie
class Shape {
    color: string,
    
    micro set_color(mut self, color: string) {
        self.color = color
    }
    
    micro get_color(self) -> string {
        self.color.clone()
    }
}

# Anonymous class inheriting from named class
micro create_circle(radius: f64) -> class(Shape) {
    radius: f64,
    
    micro area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
    
    micro draw(self) {
        print("Drawing {} circle with radius {}", 
                self.get_color(), self.radius)
    }
} {
    class(Shape) {
        color: "red",
        radius: radius,
        
        micro area(self) -> f64 {
            3.14159 * self.radius * self.radius
        }
        
        micro draw(self) {
            print("Drawing {} circle with radius {}", 
                    self.get_color(), self.radius)
        }
    }
}
```

### Multiple Inheritance with Anonymous Classes

```valkyrie
trait Drawable {
    micro draw(self)
}

trait Movable {
    micro move_to(mut self, x: f64, y: f64)
    micro get_position(self) -> (f64, f64)
}

class GameObject {
    id: u32,
    
    micro get_id(self) -> u32 {
        self.id
    }
}

# Anonymous class with multiple inheritance
micro create_sprite() -> class(GameObject): Drawable + Movable {
    x: f64,
    y: f64,
    sprite_name: string,
} {
    class(GameObject): Drawable + Movable {
        id: 1001,
        x: 0.0,
        y: 0.0,
        sprite_name: "player",
        
        micro draw(self) {
            print("Drawing sprite '{}' at ({}, {})", 
                    self.sprite_name, self.x, self.y)
        }
        
        micro move_to(mut self, x: f64, y: f64) {
            self.x = x
            self.y = y
        }
        
        micro get_position(self) -> (f64, f64) {
            (self.x, self.y)
        }
    }
}
```

## Advanced Usage of Anonymous Classes

### Factory Pattern

```valkyrie
# Implementing factory pattern with anonymous classes
micro create_handler(handler_type: string) -> class {
    micro handle(self, request: string) -> string
} {
    match handler_type {
        case "json" => class {
            micro handle(self, request: string) -> string {
                ↯format("{{\"response\": \"{}\"}}", request)
            }
        },
        case "xml" => class {
            micro handle(self, request: string) -> string {
                ↯format("<response>{}</response>", request)
            }
        },
        case _ => class {
            micro handle(self, request: string) -> string {
                ↯format("Plain response: {}", request)
            }
        }
    }
}

let json_handler = create_handler("json")
let response = json_handler.handle("Hello World")
```

### Strategy Pattern

```valkyrie
# Strategy interface
trait SortStrategy {
    micro sort(self, data: &mut [i32])
}

# Implementing different strategies using anonymous classes
micro get_sort_strategy(strategy_name: string) -> class: SortStrategy {
    match strategy_name {
        case "bubble" => class: SortStrategy {
            micro sort(self, data: &mut [i32]) {
                # Bubble sort implementation
                for i in 0..data.len() {
                    for j in 0..(data.len() - 1 - i) {
                        if data[j] > data[j + 1] {
                            data.swap(j, j + 1)
                        }
                    }
                }
            }
        },
        case "quick" => class: SortStrategy {
            micro sort(self, data: &mut [i32]) {
                # Quick sort implementation
                self.quick_sort(data, 0, data.len() as i32 - 1)
            }
            
            micro quick_sort(self, data: &mut [i32], low: i32, high: i32) {
                if low < high {
                    let pi = self.partition(data, low, high)
                    self.quick_sort(data, low, pi - 1)
                    self.quick_sort(data, pi + 1, high)
                }
            }
            
            micro partition(self, data: &mut [i32], low: i32, high: i32) -> i32 {
                # Partition implementation
                let pivot = data[high as usize]
                let mut i = low - 1
                
                for j in low..high {
                    if data[j as usize] <= pivot {
                        i += 1
                        data.swap(i as usize, j as usize)
                    }
                }
                data.swap((i + 1) as usize, high as usize)
                i + 1
            }
        },
        case _ => class: SortStrategy {
            micro sort(self, data: &mut [i32]) {
                data.sort()  # Use default sort
            }
        }
    }
}
```

### Builder Pattern

```valkyrie
# Implementing builder pattern with anonymous classes
micro create_config_builder() -> class {
    host: string?,
    port: u16?,
    timeout: u32?,
    
    micro set_host(mut self, host: string) -> Self {
        self.host = Some(host)
        self
    }
    
    micro set_port(mut self, port: u16) -> Self {
        self.port = Some(port)
        self
    }
    
    micro set_timeout(mut self, timeout: u32) -> Self {
        self.timeout = Some(timeout)
        self
    }
    
    micro build(self) -> Config {
        Config {
            host: self.host.unwrap_or("localhost"),
            port: self.port.unwrap_or(8080),
            timeout: self.timeout.unwrap_or(30),
        }
    }
} {
    class {
        host: None,
        port: None,
        timeout: None,
        
        micro set_host(mut self, host: string) -> Self {
            self.host = Some(host)
            self
        }
        
        micro set_port(mut self, port: u16) -> Self {
            self.port = Some(port)
            self
        }
        
        micro set_timeout(mut self, timeout: u32) -> Self {
            self.timeout = Some(timeout)
            self
        }
        
        micro build(self) -> Config {
            Config {
                host: self.host.unwrap_or("localhost"),
                port: self.port.unwrap_or(8080),
                timeout: self.timeout.unwrap_or(30),
            }
        }
    }
}

# Using the builder
let config = create_config_builder()
    .set_host("example.com")
    .set_port(9000)
    .set_timeout(60)
    .build()
```

## Anonymous Classes and Generics

### Generic Anonymous Classes

```valkyrie
# Generic anonymous class
micro create_container⟨T⟩(value: T) -> class {
    value: T,
    
    micro get(self) -> &T {
        &self.value
    }
    
    micro set(mut self, new_value: T) {
        self.value = new_value
    }
} {
    class {
        value: value,
        
        micro get(self) -> &T {
            &self.value
        }
        
        micro set(mut self, new_value: T) {
            self.value = new_value
        }
    }
}

let string_container = create_container("Hello")
let number_container = create_container(42)
```

### Constrained Generic Anonymous Classes

```valkyrie
# Generic anonymous class with constraints
micro create_comparable_pair⟨T: PartialOrd + Clone⟩(a: T, b: T) -> class {
    first: T,
    second: T,
    
    micro max(self) -> T {
        if self.first > self.second {
            self.first.clone()
        } else {
            self.second.clone()
        }
    }
    
    micro min(self) -> T {
        if self.first < self.second {
            self.first.clone()
        } else {
            self.second.clone()
        }
    }
} {
    class {
        first: a,
        second: b,
        
        micro max(self) -> T {
            if self.first > self.second {
                self.first.clone()
            } else {
                self.second.clone()
            }
        }
        
        micro min(self) -> T {
            if self.first < self.second {
                self.first.clone()
            } else {
                self.second.clone()
            }
        }
    }
}
```

## Lifecycle of Anonymous Classes

### Capturing External Variables

```valkyrie
micro create_counter(initial: i32) -> class {
    count: i32,
    
    micro increment(mut self) -> i32 {
        self.count += 1
        self.count
    }
    
    micro get_count(self) -> i32 {
        self.count
    }
} {
    class {
        count: initial,  # Capture external variable
        
        micro increment(mut self) -> i32 {
            self.count += 1
            self.count
        }
        
        micro get_count(self) -> i32 {
            self.count
        }
    }
}

let counter = create_counter(10)
let value1 = counter.increment()  # 11
let value2 = counter.increment()  # 12
```

## Limitations and Notes

### Return Value Limitations

**Important**: Anonymous classes cannot be used as function return types; this causes type inference errors and compilation issues.

```valkyrie
# ❌ Error: Cannot return anonymous class
micro create_handler() -> ? {
    return new {
        micro handle(data: string) {
            print("Handling: ${ data }")
        }
    }
}

# ✅ Correct: Use named class or trait
trait Handler {
    micro handle(data: string)
}

micro create_handler() -> Handler {
    return new Handler {
        micro handle(data: string) {
            print("Handling: ${ data }")
        }
    }
}
```

### Type Inference Issues

Type information for anonymous classes is difficult to determine at compile time, so:

1. **Avoid as return values**: Causes type system confusion
2. **Limit generic usage**: Use with caution in generic contexts
3. **Interface constraints**: Prefer using traits to constrain anonymous classes

## Best Practices

### Design Principles

1. **Single Responsibility**: An anonymous class should only be responsible for one specific task
2. **Simplicity**: Avoid implementing overly complex logic in anonymous classes
3. **Readability**: Ensure the purpose of the anonymous class is clear
4. **Lifecycle Management**: Pay attention to the lifecycle of anonymous class instances
5. **Type Safety**: Avoid using anonymous classes as return types

### Performance Considerations

1. **Memory Usage**: Anonymous classes create new types; be aware of memory overhead
2. **Compilation Time**: Heavy use of anonymous classes may affect compilation performance
3. **Runtime Performance**: Method calls on anonymous classes are the same as regular classes
4. **Type Checking Overhead**: Type checking for anonymous classes may be more complex

### Recommended Use Cases

- **Local Callbacks**: Create temporary callback objects inside methods
- **Strategy Implementation**: Implement specific strategy interfaces
- **Adapter Pattern**: Adapt different interfaces (but not as return values)
- **Test Stubs**: Create mock objects in tests

### Scenarios to Avoid

- **Function Return Values**: Never use anonymous classes as return values
- **Public API**: Avoid exposing anonymous classes in public interfaces
- **Long-term Storage**: Do not store anonymous class instances long-term
- **Complex Inheritance**: Avoid complex anonymous class inheritance chains

### 1. Appropriate Use of Anonymous Classes

```valkyrie
# Good use: Temporary object
micro process_data(processor: class {
    micro process(self, data: string) -> string
}) -> string {
    processor.process("input data")
}

# Avoid: Complex anonymous classes
# If an anonymous class is too complex, it should be defined as a named class
```

### 2. Keep Anonymous Classes Concise

```valkyrie
# Good design: Concise anonymous class
let validator = class {
    micro validate(self, input: string) -> bool {
        !input.is_empty() && input.len() <= 100
    }
}

# Avoid: Overly complex anonymous class
# class {
#     // Too many fields and methods
# }
```

### 3. Explicit Type Annotations

```valkyrie
# Good practice: Explicit type
micro create_handler() -> class {
    micro handle(self, request: string) -> Result⟨string, Error⟩
} {
    # Implementation
}

# Avoid: Vague types
# micro create_handler() -> class { ... }  # Interface unclear
```

## Summary

Valkyrie's anonymous class features:

1. **Flexibility**: Create objects without pre-defining a class
2. **Inheritance Support**: Support for inheriting from named classes and implementing traits
3. **Generic Support**: Support for generic parameters and constraints
4. **Closure Distinction**: Clearly distinguished from closure syntax
5. **Pattern Support**: Suitable for Factory, Strategy, Builder, and other design patterns

Anonymous classes are particularly suitable for scenarios requiring temporary objects, callback handling, and functional programming.