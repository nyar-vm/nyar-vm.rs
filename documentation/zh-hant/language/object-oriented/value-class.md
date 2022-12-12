# 值类（Value Class）与数据不变性

值类是一种以“值的不可变表示”为核心的建模方式。它强调：对象一旦创建，其可观察状态不再改变；任何“修改”都通过创建一个新实例来表达。值类有助于提升并发安全性、可推理性与测试友好性。

## 为什么选择不变性

- 简化并发：不可变对象可安全地在多线程/协程之间共享，无需复杂的同步。
- 更易推理：状态不会被外部意外改变，行为更可预测。
- 便于缓存与复用：相同输入得到相同输出，可安全地缓存或做结构共享。
- 便于测试：无需构造各种中间状态，只关注输入输出。

## 基本建模模式

- 对外只暴露只读视图（只读属性/访问器）。
- 通过构造函数或工厂方法一次性设置内部状态。
- 通过“复制并修改”的方式（`with` 方法）产生新实例，而非原位修改。

## 示例：几何点 Point（只读属性 + 派生新值）

```valkyrie
class Point {
    x: f64,
    y: f64,
    
    micro new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    
    # 只读访问器（也可用属性语法表示）
    micro get_x(self) -> f64 { self.x }
    micro get_y(self) -> f64 { self.y }

    # 产生新值：平移而不修改原对象
    micro translate(self, dx: f64, dy: f64) -> Self {
        Point { x: self.x + dx, y: self.y + dy }
    }
}

let p1 = Point::new(1.0, 2.0)
let p2 = p1.translate(3.0, 4.0)  # p1 不变，p2 为新值
```

## 示例：具名字段的复制更新（with 模式）

```valkyrie
class Person {
    name: string,
    age: i32,

    micro new(name: string, age: i32) -> Self {
        Person { name, age }
    }

    # 複製並按需改動部分欄位
    micro with(self, name?: string, age?: i32) -> Self {
        Person {
            name: name ?? self.name,
            age: age ?? self.age,
        }
    }
}

let alice = Person::new("Alice", 20)
let older = alice.with(age: 21)   # 仅变更 age，name 复用
```

## 构造与校验

值类常在构造阶段完成完整性校验，保证后续对象始终处于合法状态：

```valkyrie
class Email {
    address: string,

    micro new(address: string) -> Self {
        @require(is_valid_email(address), "郵箱格式不合法")
        Email { address }
    }
}
```

## 相等性与哈希

值类强调“基于内容”的相等性。实现相等/哈希时，应只基于可观察字段：

```valkyrie
class RGB {
    r: u8, g: u8, b: u8,

    micro equals(self, other: &RGB) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b
    }

    micro hash(self) -> i64 {
        ((self.r as i64) << 16) | ((self.g as i64) << 8) | (self.b as i64)
    }
}
```

## 与属性（property）配合

- 建议对外仅提供 getter，不提供 setter，或仅在内部可写、对外只读。
- 需要派生值时，使用方法/计算属性返回新对象，而不是修改原对象。

## 与协程/并发的关系

- 不变对象可安全跨协程传递；无需担心竞态与锁的开销。
- 可结合持久化数据结构实现“逻辑更新、结构共享”。

## 最佳实践

- 值类应保持小而精，聚焦业务含义清晰的“值”。
- 避免隐藏可变全局状态或外部依赖，确保可纯粹地被复用与测试。
- 提供清晰的构造与校验，保证对象一经创建即合法。
- 提供 `with`/复制更新等便捷 API，鼓励不可变风格。