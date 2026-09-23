# 反應式編程

反應式編程是一種基於數據流和變化傳播的編程範式。在 Valkyrie 中，反應式編程通過 Observable、Signal 和 Reactive 等抽象提供了強大的數據流處理能力。

## 核心概念

### Observable

Observable 是反應式編程的基礎，表示一個可觀察的數據流：

```valkyrie
# Observable 特徵定義
trait Observable⟨T⟩ {
    micro subscribe⟨F⟩(self, observer: F) -> Subscription where F: micro(T) -> unit
    micro map⟨U, F⟩(self, f: F) -> Observable⟨U⟩ where F: micro(T) -> U
    micro filter⟨F⟩(self, predicate: F) -> Observable⟨T⟩ where F: micro(T) -> bool
    micro merge(self, other: Observable⟨T⟩) -> Observable⟨T⟩
    micro take(self, count: usize) -> Observable⟨T⟩
}

# 創建 Observable
let numbers = Observable.from([1, 2, 3, 4, 5])
let mouse_clicks = Observable.from_events("click")
let timer = Observable.interval(1000)  # 每秒觸發一次
```

### Signal

Signal 是具有當前值的反應式狀態：

```valkyrie
# Signal 定義
class Signal⟨T⟩ {
    private value: T
    private observers: [micro(T) -> unit]
    
    micro Signal(initial: T) -> Signal⟨T⟩
    micro get(self) -> T
    micro set(mut self, new_value: T)
    micro update⟨F⟩(mut self, updater: F) where F: micro(T) -> T
    micro subscribe⟨F⟩(self, observer: F) -> Subscription where F: micro(T) -> unit
}

# 使用 Signal
let counter = Signal(0)
let doubled = counter.map { it * 2 }

# 訂閱變化
counter.subscribe {
    print("計數器值: ${it}")
}

# 更新值
counter.set(5)  # 輸出: 計數器值: 5
counter.update { it + 1 }  # 輸出: 計數器值: 6
```

## 基本運算子

### 轉換運算子

```valkyrie
# map - 轉換每個值
let numbers = Observable.from([1, 2, 3, 4, 5])
let squares = numbers.map { it * it }

# flat_map - 扁平化映射
let words = Observable.from(["hello", "world"])
let characters = words.flat_map {
    Observable.from(it.chars())
}

# scan - 累積操作
let numbers = Observable.from([1, 2, 3, 4, 5])
let running_sum = numbers.scan(0) { acc, x -> acc + x }
# 輸出: 1, 3, 6, 10, 15
```

### 過濾運算子

```valkyrie
# filter - 過濾值
let numbers = Observable.from([1, 2, 3, 4, 5, 6])
let evens = numbers.filter { it % 2 == 0 }

# take - 取前 N 個值
let first_three = numbers.take(3)

# skip - 跳過前 N 個值
let after_two = numbers.skip(2)

# distinct - 去重
let unique = Observable.from([1, 1, 2, 2, 3, 3]).distinct()
```

### 組合運算子

```valkyrie
# merge - 合併多個流
let stream1 = Observable.from([1, 3, 5])
let stream2 = Observable.from([2, 4, 6])
let merged = stream1.merge(stream2)

# zip - 配對組合
let names = Observable.from(["Alice", "Bob", "Charlie"])
let ages = Observable.from([25, 30, 35])
let people = names.zip(ages).map { (name, age) ->
    Person { name, age }
}

# combine_latest - 最新值組合
let temperature = Signal(20.0)
let humidity = Signal(60.0)
let comfort_index = temperature.combine_latest(humidity).map { (temp, hum) ->
    calculate_comfort(temp, hum)
}
```

## 實際應用示例

### 用戶界面反應式更新

```valkyrie
# 反應式 UI 組件
class CounterComponent {
    private count: Signal⟨i32⟩
    private increment_clicks: Observable⟨unit⟩
    private decrement_clicks: Observable⟨unit⟩
    
    micro CounterComponent() -> CounterComponent {
        let count = Signal(0)
        let increment_clicks = Observable.from_events("increment")
        let decrement_clicks = Observable.from_events("decrement")
        
        # 響應點擊事件
        increment_clicks.subscribe {
            count.update { it + 1 }
        }
        
        decrement_clicks.subscribe {
            count.update { it - 1 }
        }
        
        CounterComponent {
            count,
            increment_clicks,
            decrement_clicks
        }
    }
    
    micro render(self) -> Widget {
        let count_text = self.count.map { "計數: ${it}" }
        
        VStack {
            Text(count_text)
            HStack {
                Button("增加").on_click(self.increment_clicks)
                Button("減少").on_click(self.decrement_clicks)
            }
        }
    }
}
```

### 數據流處理

```valkyrie
# 即時數據處理管道
class DataProcessor {
    micro process_sensor_data(sensor_stream: Observable⟨SensorReading⟩) -> Observable⟨ProcessedData⟩ {
        sensor_stream
            .filter { it.is_valid() }  # 過濾無效數據
            .map { it.normalize() }    # 標準化數據
            .buffer(Duration.seconds(5))          # 5秒緩衝窗口
            .map { batch -> self.analyze_batch(batch) } # 批量分析
            .filter { it.confidence > 0.8 } # 過濾低置信度結果
    }
    
    private micro analyze_batch(batch: [SensorReading]) -> ProcessedData {
        let average = batch.iter().map { it.value }.sum() / batch.len()
        let variance = calculate_variance(batch)
        
        ProcessedData {
            timestamp: now(),
            average,
            variance,
            confidence: calculate_confidence(variance)
        }
    }
}

# 使用數據處理器
let processor = DataProcessor()
let sensor_stream = Observable.from_websocket("ws://sensor.example.com")
let processed_stream = processor.process_sensor_data(sensor_stream)

processed_stream.subscribe { data ->
    print("處理結果: 平均值=${data.average}, 置信度=${data.confidence}")
    
    if data.confidence > 0.95 {
        alert_system.notify("高置信度數據: ${data}")
    }
}
```

### 非同步操作組合

```valkyrie
# 反應式 HTTP 客戶端
class ReactiveHttpClient {
    micro get⟨T⟩(url: string) -> Observable⟨Result⟨T, HttpError⟩⟩ {
        Observable.create { observer ->
            async {
                try {
                    let response = http_get(url).await?
                    let data = response.json⟨T⟩().await?
                    observer.next(Fine { value: data })
                    observer.complete()
                }
                .catch {
                    case e:
                        observer.error(e)
                }
            }
        }
    }
    
    micro retry⟨T⟩(observable: Observable⟨Result⟨T, HttpError⟩⟩, max_retries: usize) -> Observable⟨Result⟨T, HttpError⟩⟩ {
        observable.catch_error { error ->
            if max_retries > 0 {
                print("重試請求，剩餘次數: ${max_retries}")
                Observable.timer(Duration.seconds(1))
                    .flat_map { self.retry(observable, max_retries - 1) }
            } else {
                Observable.error(error)
            }
        }
    }
}

# 使用示例
let client = ReactiveHttpClient()
let user_data = client.get⟨User⟩("https://api.example.com/user/123")
    .retry(3)  # 最多重試3次
    .timeout(Duration.seconds(10))  # 10秒超時

user_data.subscribe { result ->
    match result {
        case Fine { value: user }:
            print("用戶信息: ${user.name}")
        case Fail { error: error }:
            print("獲取用戶信息失敗: ${error}")
    }
}
```

## 錯誤處理

### 錯誤恢復策略

```valkyrie
# 錯誤處理運算子
trait ObservableErrorHandling⟨T⟩ {
    # 捕獲錯誤並提供默認值
    micro catch_error⟨F⟩(self, handler: F) -> Observable⟨T⟩ where F: micro(Any) -> Observable⟨T⟩
    
    # 重試操作
    micro retry(self, count: usize) -> Observable⟨T⟩
    
    # 超時處理
    micro timeout(self, duration: Duration) -> Observable⟨T⟩
}

# 實際使用
let unreliable_stream = fetch_data_stream()
    .catch_error { error ->
        print("發生錯誤: ${error}，使用緩存數據")
        Observable.from(cached_data)
    }
    .retry(3)
    .timeout(Duration.seconds(30))

unreliable_stream.subscribe { data ->
    process_data(data)
}
```

### 錯誤傳播控制

```valkyrie
# 部分錯誤處理
let mixed_stream = Observable.from([1, 2, 3, 4, 5])
    .map { x ->
        if x == 3 {
            raise ValueError { error: "無效值: 3" }
        }
        x * 2
    }
    .on_error_resume_next { error ->
        print("跳過錯誤: ${error}")
        Observable.empty()  # 跳過錯誤項
    }

mixed_stream.subscribe { value ->
    print("處理值: ${value}")
}  # 輸出: 2, 4, 8, 10 (跳過了3)
```

## 資源管理

### 訂閱生命週期

```valkyrie
# Subscription 管理
class Subscription {
    private is_disposed: bool
    private cleanup: micro() -> unit
    
    micro dispose(mut self) {
        if !self.is_disposed {
            self.cleanup()
            self.is_disposed = true
        }
    }
    
    micro is_disposed(self) -> bool {
        self.is_disposed
    }
}

# CompositeSubscription 用於管理多個訂閱
class CompositeSubscription {
    private subscriptions: [Subscription]
    
    micro add(mut self, subscription: Subscription) {
        self.subscriptions.push(subscription)
    }
    
    micro dispose_all(mut self) {
        for subscription in self.subscriptions {
            subscription.dispose()
        }
        self.subscriptions.clear()
    }
}

# 使用示例
let composite = CompositeSubscription()

let sub1 = timer_stream.subscribe { print("定時器觸發") }
let sub2 = click_stream.subscribe { print("點擊事件") }

composite.add(sub1)
composite.add(sub2)

# 在組件銷毀時清理所有訂閱
composite.dispose_all()
```

### 背壓處理

```valkyrie
# 背壓策略
struct BackpressureStrategy {
    micro Buffer(capacity: usize)     # 緩衝策略
    micro Drop                        # 丟棄策略
    micro Latest                      # 保留最新策略
    micro Error                       # 錯誤策略
}

# 應用背壓控制
let fast_producer = Observable.interval(Duration.milliseconds(1))  # 每毫秒產生數據
let slow_consumer = fast_producer
    .observe_on(Scheduler.computation())  # 在計算執行緒池處理
    .buffer(100)  # 緩衝100個元素
    .sample(Duration.seconds(1))  # 每秒採樣一次

slow_consumer.subscribe { batch ->
    print("處理批次，大小: ${batch.len()}")
    # 慢速處理邏輯
    Thread.sleep(Duration.milliseconds(100))
}
```

## 調度器

### 執行緒調度

```valkyrie
# 調度器類型
struct Scheduler {
    micro CurrentThread    # 當前執行緒
    micro Computation      # 計算執行緒池
    micro IO               # I/O 執行緒池
    micro NewThread        # 新執行緒
    micro Trampoline       # 蹦床調度器
}

# 指定調度器
let data_stream = Observable.from_file("large_file.txt")
    .subscribe_on(Scheduler.IO())        # 在 I/O 執行緒讀取文件
    .observe_on(Scheduler.Computation()) # 在計算執行緒處理數據
    .map { line -> expensive_computation(line) }
    .observe_on(Scheduler.CurrentThread()) # 在主執行緒更新 UI

data_stream.subscribe { result ->
    update_ui(result)  # UI 更新必須在主執行緒
}
```

## 測試支持

### 測試調度器

```valkyrie
# 測試用的虛擬時間調度器
class TestScheduler {
    private virtual_time: Duration
    private scheduled_actions: [(Duration, micro() -> unit)]
    
    micro advance_time_by(mut self, duration: Duration) {
        let target_time = self.virtual_time + duration
        
        while let (time, action)? = self.scheduled_actions.first() {
            if time <= target_time {
                self.virtual_time = time
                action()
                self.scheduled_actions.remove(0)
            } else {
                break
            }
        }
        
        self.virtual_time = target_time
    }
}

# 測試示例
#[test]
micro test_timer_observable() {
    let scheduler = TestScheduler()
    let timer = Observable.timer(Duration.seconds(5), scheduler)
    let mut received_values = []
    
    timer.subscribe { value ->
        received_values.push(value)
    }
    
    # 推進虛擬時間
    scheduler.advance_time_by(Duration.seconds(3))
    assert_eq!(received_values.len(), 0)  # 還沒到時間
    
    scheduler.advance_time_by(Duration.seconds(3))
    assert_eq!(received_values.len(), 1)  # 定時器觸發
}
```

## 最佳實踐

### 1. 避免記憶體洩漏

```valkyrie
# 正確的訂閱管理
class Component {
    private subscriptions: CompositeSubscription
    
    micro Component() -> Component {
        let subscriptions = CompositeSubscription()
        
        # 訂閱數據流
        let sub = data_stream.subscribe { data ->
            self.handle_data(data)
        }
        
        subscriptions.add(sub)
        
        Component { subscriptions }
    }
    
    micro destroy(mut self) {
        # 組件銷毀時清理訂閱
        self.subscriptions.dispose_all()
    }
}
```

### 2. 合理使用運算子

```valkyrie
# 優化運算子鏈
let optimized_stream = source_stream
    .filter { it.is_valid() }     # 儘早過濾
    .take(1000)                   # 限制數量
    .map { it.transform() }       # 轉換數據
    .distinct()                   # 去重
    .buffer(Duration.seconds(1)) # 批處理

# 避免過長的運算子鏈
let intermediate = source_stream
    .filter { it.is_valid() }
    .map { it.normalize() }

let final_stream = intermediate
    .group_by { it.category }
    .flat_map { group -> group.buffer(10) }
```

### 3. 錯誤邊界

```valkyrie
# 設置錯誤邊界防止整個流崩潰
let resilient_stream = risky_stream
    .map { item ->
        try {
            process_item(item)
        }
        .catch {
            case ProcessingError { error: e }:
                log_error("處理失敗: ${e}")
                default_value()  # 提供默認值
            case _:
                raise  # 重新拋出嚴重錯誤
        }
    }
    .filter { it? }
```

通過這些反應式編程模式，Valkyrie 提供了強大而靈活的數據流處理能力，使開發者能夠構建響應式、可維護的應用程序。
