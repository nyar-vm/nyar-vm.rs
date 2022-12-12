# Valkyrie 元編程架構

Valkyrie 提供了強大的元編程支持，允許在編譯時進行代碼生成、變換和分析。通過集成在編譯器中的元編程系統，Valkyrie 能夠實現宏系統、編譯時計算、類型級編程等高級能力。

## 元編程架構概覽

### 元編程在編譯器中的定位

Valkyrie 的元編程系統深度集成在多層 IR 架構中，在不同層次提供相應的能力：

```
源代碼 + 元編程指令
         ↓
    AST + 宏展開
         ↓
    HIR + 編譯時計算
         ↓
    MIR + 代碼優化
         ↓
    LIR + 平台特化
         ↓
目標代碼 + 運行時支持
```

## 核心元編程特性

### [編譯時計算](./compile-time-computation.md)

**常量表達式求值**:
```valkyrie
// 編譯時常量計算
let FIBONACCI_10: i32 = ↯evaluate(fibonacci(10))
let LOOKUP_TABLE: [i32; 256] = ↯evaluate(generate_lookup_table())

// 編譯時字符串處理
let CONFIG_KEY: string = ↯evaluate(↯format("app.{}.version", ↯env("BUILD_TARGET")))
```

**編譯時函數執行**:
```valkyrie
// 標記為編譯時函數
@const_fn
micro fibonacci(n: i32) -> i32 {
    n.match {
        case 0 | 1: n
        case _: fibonacci(n-1) + fibonacci(n-2)
    }
}

// 編譯時數據結構操作
@const_fn
micro build_state_machine() -> StateMachine {
    let mut sm = StateMachine()
    sm.add_state("start")
    sm.add_state("processing")
    sm.add_state("end")
    sm.add_transition("start", "process", "processing")
    sm.add_transition("processing", "finish", "end")
    sm
}
```

### [宏系統](./macro-system.md)

**聲明式宏**:
```valkyrie
// 模式匹配宏
macro vec_of {
    (#elem:expr; #n:expr) => {
        {
            let mut v = []
            for _ in 0..<#n {
                v.push(#elem)
            }
            v
        }
    }
    (#(#x:expr),+ #(,)?) => {
        ↯vec(#(#x),+)
    }
}

// 使用示例
let zeros = ↯vec_of(0; 10)
let numbers = ↯vec_of(1, 2, 3, 4, 5)
```

**過程宏**:
```valkyrie
// 自定義派生宏
@derive(Serialize, Deserialize, Debug)
class User {
    id: u64,
    name: string,
    email: string,
}

// 屬性宏
@api_endpoint(method: "GET", path: "/users/{id}")
micro get_user(id: u64) -> Result⟨User, ApiError⟩ {
    // 自動生成路由註冊和參數驗證代碼
    database::find_user(id)
}

// 函數式宏
let sql_query = ↯sql(
    "SELECT id, name, email FROM users WHERE active = $1",
    true
)
```

### [代碼生成](./code-generation.md)

**基於模板的代碼生成**:
```valkyrie
// 模板定義
↯template {
    name: "crud_operations",
    params: [Entity: Type, Key: Type],
    body: {
        impl CrudOperations⟨{{Key}}⟩ for {{Entity}} {
            micro create(data: {{Entity}}) -> Result⟨{{Key}}, Any⟩ {
                // 生成創建邏輯
            }
            
            micro read(id: {{Key}}) -> Result⟨{{Entity}}, Any⟩ {
                // 生成讀取邏輯
            }
            
            micro update(id: {{Key}}, data: {{Entity}}) -> Result⟨unit, Any⟩ {
                // 生成更新邏輯
            }
            
            micro delete(id: {{Key}}) -> Result⟨unit, Any⟩ {
                // 生成刪除邏輯
            }
        }
    }
}

// 模板實例化
↯generate_code {
    crud_operations⟨User, UserId⟩
    crud_operations⟨Product, ProductId⟩
    crud_operations⟨Order, OrderId⟩
}
```

**反射驅動的代碼生成**:
```valkyrie
// 自動生成序列化代碼
@auto_serialize
class Config {
    database_url: string,
    port: u16,
    debug: bool,
}

// 編譯時生成的代碼
impl Serialize for Config {
    micro serialize(self) -> SerializedData {
        let mut data = SerializedData()
        data.insert("database_url", self.database_url)
        data.insert("port", self.port)
        data.insert("debug", self.debug)
        data
    }
}
```

### [類型級編程](./type-level-programming.md)

**類型級函數**:
```valkyrie
// 類型級計算
type Add(a: Nat, b: Nat) -> Nat {
    Add(Zero, b) = b,
    Add(Succ(a), b) = Succ(Add(a, b))
}

// 類型級列表操作
type Length(list: [T]) -> Nat {
    Length(Nil) = Zero,
    Length(Cons(_, tail)) = Succ(Length(tail))
}

// 編譯時類型驗證
micro safe_array_access⟨const N: usize, const I: usize⟩(arr: [i32; N]) -> i32 
where
    Assert⟨LessThan⟨I, N⟩⟩: True
{
    arr[I]  // 編譯時保證索引安全
}
```

**依賴類型支持**:
```valkyrie
// 長度依賴的向量類型
class Vector⟨T, const N: usize⟩ {
    data: [T; N],
}

impl⟨T, const N: usize⟩ Vector⟨T, N⟩ {
    micro push⟨const M: usize⟩(self, item: T) -> Vector⟨T, {N + 1}⟩ {
        // 類型級別保證長度正確性
    }
    
    micro concat⟨const M: usize⟩(self, other: Vector⟨T, M⟩) -> Vector⟨T, {N + M}⟩ {
        // 編譯時計算結果長度
    }
}
```
