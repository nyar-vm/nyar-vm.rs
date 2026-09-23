# 高階型別 (Higher-Kinded Types)

高階型別（HKT）是 Valkyrie 型別系統的高級特性，允許對型別構造器進行抽象，實現更強大的泛型編程模式。

## 基本概念

### 型別的種類 (Kinds)

在 Valkyrie 中，型別有不同的"種類"：

```valkyrie
# 種類 *：具體型別
let x: i32        # i32 的種類是 *
let y: string     # string 的種類是 *

# 種類 * -> *：一元型別構造器
type []           # [] 的種類是 * -> *
type ?            # ? 的種類是 * -> *

# 種類 * -> * -> *：二元型別構造器
type Result⟨T, E⟩ # Result 的種類是 * -> * -> *
type {K: V}       # {K: V} 的種類是 * -> * -> *

# 種類 (* -> *) -> *：高階型別構造器
type Monad⟨M⟩     # M 的種類是 * -> *
```

### 型別構造器抽象

```valkyrie
# 定義高階型別特徵
trait Functor where Self: * -> * {
    micro map⟨A, B⟩(self: Self⟨A⟩, f: micro(A) -> B) -> Self⟨B⟩
}

# 為具體型別實現
imply ?: Functor {
    micro map⟨A, B⟩(self: A?, f: micro(A) -> B) -> B? {
        match self {
            case value: f(value)
            case None: None
        }
    }
}

imply []: Functor {
    micro map⟨A, B⟩(self: [A], f: micro(A) -> B) -> [B] {
        [ f(item) for item in self ]
    }
}
```

## 單子模式 (Monad Pattern)

### 單子特徵定義

```valkyrie
# 單子特徵
trait Monad where Self: * -> * {
    # 將值包裝到單子中
    micro pure⟨A⟩(value: A) -> Self⟨A⟩
    
    # 單子綁定操作
    micro bind⟨A, B⟩(self: Self⟨A⟩, f: micro(A) -> Self⟨B⟩) -> Self⟨B⟩
    
    # 便利方法：map 可以通過 bind 和 pure 實現
    micro map⟨A, B⟩(self: Self⟨A⟩, f: micro(A) -> B) -> Self⟨B⟩ {
        self.bind({ $x; Self::pure(f($x)) })
    }
}

# ? 單子實現
imply ?: Monad {
    micro pure⟨A⟩(value: A) -> A? {
        value
    }
    
    micro bind⟨A, B⟩(self: A?, f: micro(A) -> B?) -> B? {
        match self {
            case value: f(value)
            case None: None
        }
    }
}

# Result 單子實現
imply⟨E⟩ Result⟨_, E⟩: Monad {
    micro pure⟨A⟩(value: A) -> Result⟨A, E⟩ {
        Fine(value)
    }
    
    micro bind⟨A, B⟩(self: Result⟨A, E⟩, f: micro(A) -> Result⟨B, E⟩) -> Result⟨B, E⟩ {
        match self {
            case Fine(value): f(value)
            case Fail(error): Fail(error)
        }
    }
}
```
