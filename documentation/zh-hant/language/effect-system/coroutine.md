# 協程

Valkyrie 提供了強大的協程支持，通過 `yield` 關鍵字實現協作式多任務處理。協程允許函數在執行過程中暫停和恢復，非常適合處理非同步操作和狀態機。協程與生成器的主要區別在於協程更注重控制流的暫停和恢復，而不僅僅是產生值序列。

## 協程狀態管理

### 協程生命週期

```valkyrie
# 協程狀態枚舉
union CoroutineState {
    Created,     # 已創建但未開始
    Running,     # 正在執行
    Suspended,   # 已暫停（yield）
    Completed,   # 已完成
    Error { error: Any } # 發生錯誤
}

# 檢查協程狀態
micro example_coroutine() {
    print("開始執行")
    yield "第一個值"
    print("繼續執行")
    yield "第二個值"
    print("執行完成")
}

let coro = example_coroutine()
print(coro.state())  # Created

let first = coro.next()
print(coro.state())  # Suspended
print(first)         # "第一個值"

let second = coro.next()
print(coro.state())  # Suspended
print(second)        # "第二個值"

coro.next()          # 完成執行
print(coro.state())  # Completed
```

### 協程控制

```valkyrie
# 手動控制協程執行
micro controlled_coroutine() {
    let mut state = "idle"
    loop {
        let command = yield state
        command.match {
            case "start":
                state = "running"
            case "pause":
                state = "paused"
            case "stop":
                state = "stopped"
                break
            case _:
                state = "unknown_command"
        }
    }
}

let coro = controlled_coroutine()
print(coro.next())           # "idle"
print(coro.send("start"))    # "running"
print(coro.send("pause"))    # "paused"
print(coro.send("stop"))     # "stopped"
```

## 非同步協程

### 非同步操作

```valkyrie
# 非同步協程
async micro fetch_data(url: string) -> string {
    print("開始請求: ${ url }")
    let response = http_get(url).await?
    yield "請求已發送"  # 可以在非同步函數中使用 yield
    
    if response.status == 200 {
        yield "請求成功"
        response.body
    } else {
        raise "請求失敗: ${ response.status }"
    }
}

# 使用非同步協程
async micro main() {
    let fetcher = fetch_data("https://api.example.com/data")
    
    # 處理中間狀態
    for status in fetcher {
        print("狀態: ${ status }")
    }
    
    # 獲取最終結果
    try {
        let data = fetcher.await?
        print("數據: ${ data }")
    }
    .catch {
        case _:
            print("錯誤: ${ error }")
    }
}
```

### 並發協程

```valkyrie
# 並發執行多個協程
async micro concurrent_processing(items: [string]) {
    let promises = items.map(async micro(item) {
        let result = process_item(item)
        yield "處理完成: ${ item }"
        result
    })
    
    # 等待所有 Promise 完成
    let results = Promise::all(promises).await?
    yield "所有任務完成"
    results
}

# 使用
async micro run_concurrent() {
    let processor = concurrent_processing(["item1", "item2", "item3"])
    
    for update in processor {
        print(update)
    }
    
    let final_results = processor.await?
    print("最終結果: ${ final_results }")
}
```

## 高級協程模式

### 狀態機協程

```valkyrie
# 狀態機實現
union State {
    Idle,
    Processing,
    Waiting,
    Complete
}

micro state_machine() {
    let mut state = State::Idle
    let mut data = null
    
    loop {
        state.match {
            case State::Idle: {
                yield "等待輸入"
                data = yield_receive()  # 等待外部輸入
                state = State::Processing
            }
            case State::Processing: {
                yield "處理中..."
                let result = process_data(data)
                if result.is_ok() {
                    state = State::Complete
                } else {
                    state = State::Waiting
                }
            }
            case State::Waiting: {
                yield "等待重試"
                sleep(1000)  # 等待1秒
                state = State::Processing
            }
            case State::Complete: {
                yield "處理完成"
                break
            }
        }
    }
}
```

### 協程池

```valkyrie
# 協程池管理
class CoroutinePool {
    coroutines: [Coroutine],
    max_size: i32,
    active_count: i32
    
    micro new(max_size: i32) -> Self {
        CoroutinePool {
            coroutines: [],
            max_size: max_size,
            active_count: 0
        }
    }
    
    micro spawn(task: micro() -> Any) -> bool {
        if self.active_count < self.max_size {
            let coro = Coroutine::new(task)
            self.coroutines.push(coro)
            self.active_count += 1
            true
        } else {
            false  # 池已滿
        }
    }
    
    micro run_all() {
        while self.active_count > 0 {
            for coro in self.coroutines {
                if coro.state() == CoroutineState::Suspended {
                    let result = coro.resume()
                    yield "協程進度: ${ result }"
                    
                    if coro.state() == CoroutineState::Completed {
                        self.active_count -= 1
                    }
                }
            }
        }
        yield "所有協程完成"
    }
}
```

## 錯誤處理

### 協程異常處理

```valkyrie
# 協程中的異常處理
micro error_prone_generator() {
    try {
        yield "開始處理"
        
        let risky_operation = perform_risky_task()
        yield "風險操作完成"
        
        if risky_operation.is_error() {
            raise "操作失敗"
        }
        
        yield "處理成功"
    }
    .catch {
        case _:
            yield "發生錯誤: ${ error }"
            raise error  # 重新拋出異常
    }
}

# 使用帶錯誤處理的協程
let gen = error_prone_generator()
try {
    for status in gen {
        print(status)
    }
}
.catch {
    case _:
        print("協程異常: ${ error }")
}
```

## 最佳實踐

### 1. 協程設計原則

```valkyrie
# 保持協程簡單和專注
micro good_generator(data: [string]) {
    for item in data {
        if item.is_valid() {
            yield item.process()  # 只做一件事
        }
    }
}

# 避免在協程中進行複雜的狀態管理
# 不好的例子：
micro bad_generator() {
    let mut complex_state = ComplexState::new()
    # ... 複雜的狀態邏輯
}
```

### 2. 資源管理

```valkyrie
# 確保資源正確釋放
micro file_processor(filename: string) {
    using file = open_file(filename) {
        while !file.eof() {
            let line = file.read_line()
            yield process_line(line)
        }
    }  # 文件自動關閉
}
```

### 3. 性能考慮

```valkyrie
# 避免頻繁的小yield
# 不好的例子：
micro inefficient_generator(data: [i32]) {
    for item in data {
        yield item  # 每個元素都yield
    }
}

# 好的例子：
micro efficient_generator(data: [i32]) {
    let mut batch = []
    for item in data {
        batch.push(item)
        if batch.len() >= 100 {
            yield batch  # 批量yield
            batch = []
        }
    }
    if !batch.is_empty() {
        yield batch  # 處理剩餘項目
    }
}
```

### 4. 測試協程

```valkyrie
# 協程測試策略
micro test_generator() {
    let gen = count_up(3)
    
    # 測試生成的值
    ↯assert_equal(gen.next(), 0)
    ↯assert_equal(gen.next(), 1)
    ↯assert_equal(gen.next(), 2)
    ↯assert_equal(gen.next(), null)
    
    # 測試狀態
    ↯assert_equal(gen.state(), CoroutineState::Completed)
}

# 非同步協程測試
async micro test_async_generator() {
    let gen = async_data_processor()
    
    let first_result = gen.next().await?
    assert!(first_result != null)
    
    let final_result = gen.collect_all().await?
    ↯assert_equal(final_result.len(), 5)
}
```
