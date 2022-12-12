
## 异步原语与类型系统

### Future：底层异步原语

Valkyrie 的异步系统基于 `Future` 作为底层原语。所有异步操作最终都会产生 `Future` 实例：

- `Promise` - Future 的具体实现，用于异步任务执行、值传递和组合
- `async { ... }` 块 - 创建 Promise 实例的语法糖

```valkyrie
# 所有这些都是 Promise 实例（实现了 Future 接口）
let promise1: Promise⟨string⟩ = async { "hello" }
let promise2: Promise⟨i32⟩ = Promise.resolve(42)
let composed: Promise⟨string⟩ = async { promise1.await + promise2.await.to_string() }
```

## 异步块：async { }

在异步函数之外或之内，都可以使用 `async { ... }` 创建一个可执行的异步 Promise 对象。该块内可以使用 `await` 等待其它异步结果。

```valkyrie
# 创建一个异步 Promise（不会立即阻塞当前线程）
let promise = async {
    let user = fetch_user(42).await?
    let posts = fetch_posts(user.id).await?
    (user, posts)
}

# Promise 可被组合
let composed = async {
    let (u, p) = promise.await?
    render(u, p)
}
```

特点：
- `async { ... }` 是表达式，返回一个 Promise 句柄，可被存入变量、作为参数传递或进一步组合。
- Promise 不会自动阻塞当前线程，如何"运行"由下节的 run.* 与 `awake` 控制。

## 运行控制：run.await / run.block / run.awake / awake

为统一控制异步 Promise 的执行与结果获取，约定 Promise 句柄提供 `run` 控制器：

- `promise.await`：在异步上下文中挂起当前协程，直至 Promise 完成并返回结果。
- `promise.block`：在同步上下文中阻塞当前线程直至 Promise 完成，返回结果（适合 CLI、测试入口等）。
- `promise.awake`：将 Promise 调度到执行器上异步启动，但不等待结果，返回轻量句柄或 unit。
- `awake <expr>`：语法糖，等价于对 `<expr>` 产生的 Promise 执行"fire-and-forget"，即触发后忽略结果与错误（可用于日志、遥测等非关键路径）。

### 使用示例

```valkyrie
# 同步入口中（阻塞等待）
micro main() {
    let promise = async {
        compute_heavy()  # 假设是计算密集操作
    }
    let result = promise.block?
    print("结果: ${ result }")
}
```

```valkyrie
# 异步上下文中（协作式等待）
async micro handle_request(id: i64) -> string {
    let promise = async {
        let data = fetch_by_id(id).await?
        transform(data)
    }
    let out = promise.await?
    out
}
```

```valkyrie
# 调度但不关心结果（fire-and-forget）
let promise = async {
    audit("user_login")
}

let bg_promise = async { refresh_cache() }
_bg = bg_promise.awake   # 触发后台刷新并忽略结果
```

### 异步方法调用规则

#### 执行控制语义

对于返回 Future 的方法调用（Promise 等 Future 实例）：

1. **自动执行规则**：
   - `obj.call_fut()` 本身就相当于 `obj.call_fut.await()`，会自动执行并等待结果
   - 括号可以省略：`obj.call_fut` 等价于 `obj.call_fut()`

2. **显式控制语义**：
   - `obj.call_fut.await` - 显式等待（与自动执行等价）
   - `obj.call_fut.awake` - fire-and-forget 语义，不等待结果
   - `obj.call_fut.block` - 阻塞等待（同步上下文中使用）

3. **函数绑定**：
   - `let f = obj.call_fut` - 不会自动执行，而是把返回 future 的函数绑定到 f
   - 静态方法遵循同样的规则

4. **错误处理**：
   - `?` 操作符用于 Result 类型的错误传播，与 await 无关
   - `promise.run.await` 用于等待 Promise 完成
   - `promise.run.block` 用于阻塞等待 Promise 完成
   - 如果需要错误传播，在整个表达式后使用：`promise.run.await?`

### Promise 高级用法

#### 1. 封装回调函数

Promise 可以用来封装传统的回调式 API，将其转换为异步/await 模式：

```valkyrie
# 封装回调式 API
micro wrap_callback_api(url: string) -> Promise⟨string⟩ {
    Promise(micro(resolve, reject) {
        # 调用传统的回调式 API
        http_request_with_callback(url, micro(result) {
            if result.is_success() {
                resolve(result.data)
            } else {
                reject(result.error)
            }
        })
    })
}

# 使用封装后的 Promise
async micro fetch_data() {
    try {
        let data = wrap_callback_api("https://api.example.com").await?
        print("获取数据: ${ data }")
    }
    .catch {
        case _:
            print("请求失败: ${ error }")
    }
}
```

#### 2. Promise 取消功能

Promise 支持取消操作，这是 Future 基础接口所不具备的功能：

```valkyrie
# 创建可取消的 Promise
let (promise, cancellation_token) = Promise.cancellable(micro(resolve, reject, is_cancelled) {
    let mut count = 0
    loop {
        if is_cancelled() {
            reject("操作已取消")
            break
        }
        
        count += 1
        Thread.sleep(Duration.milliseconds(1000))  # 模拟长时间操作
        
        if count >= 10 {
            resolve("操作完成")
            break
        }
    }
})

# 在另一个地方取消操作
Timer.timeout(5000) {
    cancellation_token.cancel()
    print("已请求取消操作")
}

# 等待结果或取消
try {
    let result = promise.await?
    print("结果: ${ result }")
}
.catch {
    case _:
        print("操作被取消或失败: ${ error }")
}
```

**注意**：Future 作为底层原语不提供 cancel 功能，只有 Promise 等具体实现才支持取消操作。

### Future 系统的统一性

由于 Promise 是 Future 的具体实现，所有异步操作都通过 Promise 提供统一的执行控制接口：

```valkyrie
# 所有异步操作都返回 Promise
let promise1 = async { compute() }
let promise2 = Promise.resolve(42)

# 统一的执行控制
promise1.await    # 等待 Promise 完成
promise2.await    # 等待 Promise 完成
promise1.awake    # fire-and-forget Promise
promise2.awake    # fire-and-forget Promise
```

Promise 作为 Future 的唯一实现，提供了完整的异步功能，包括取消操作等高级特性。

### 与现有 await 语法的关系

- 在异步函数内，Promise 方法调用通常会自动 await，不需要手动写 .await
- 在同步函数内，若需要等待 Promise 结果，使用 `.block`；不等待则使用 `.awake`
- `awake` 的语义为 "fire then ignore"，适合非关键路径、可重试或可丢弃的任务
- 所有 Promise 实例都遵循相同的执行语义

## 异步流：Stream

### Stream 概念

当协程和生成器结合异步操作时，需要一种特殊的 `Stream` 类型来处理异步迭代。Stream 是异步版本的迭代器，能够处理异步产生的值序列。

```valkyrie
# Stream 特征定义
trait Stream⟨T⟩ {
    async micro next(mut self) -> T?
    async micro collect(self) -> [T]
    async micro for_each⟨F⟩(self, f: F) where F: async micro(T) -> unit
}
```

### 协程 Stream 化

协程可以转换为 Stream，提供异步迭代能力：

```valkyrie
# 协程转 Stream
async micro* fetch_pages(base_url: string) -> Stream⟨string⟩ {
    let mut page = 1
    loop {
        let url = "${ base_url }?page=${ page }"
        let response = http_get(url).await?
        
        if response.is_empty() {
            break
        }
        
        yield response  # 异步产生值
        page += 1
    }
}

# 使用 Stream
async micro process_all_pages() {
    let page_stream = fetch_pages("https://api.example.com/data")
    
    # 异步迭代
    async for page in page_stream {
        try {
            process_page(page).await?
        }
        .catch {
            case NetworkError(e):
                print("网络错误，跳过: ${ e }")
                continue
            case _:
                break  # 其他错误则停止处理
        }
    }
}
```

### Future Iterator vs Iterator Future

#### Future Iterator（推荐模式）

每次迭代返回一个 Future，适合处理独立的异步操作：

```valkyrie
# Future Iterator: Iterator⟨Promise⟨T⟩⟩
class FutureIterator⟨T⟩ {
    micro next(mut self) -> Promise⟨T⟩?
}

# 使用示例
micro process_urls(urls: [string]) -> FutureIterator⟨string⟩ {
    urls.into_iter().map(async micro(url) {
        http_get(url).await?
    })
}

# 并发处理
async micro handle_concurrent() {
    let futures = process_urls(["url1", "url2", "url3"])
    let results = Promise.all(futures.collect()).await?
    
    for result in results {
        print("结果: ${ result }")
    }
}
```

#### Iterator Future（特殊场景）

整个迭代过程是异步的，适合有序依赖的场景：

```valkyrie
# Iterator Future: Promise⟨Iterator⟨T⟩⟩
class IteratorFuture⟨T⟩ {
    async micro resolve(self) -> Iterator⟨T⟩
}

# 使用示例：需要认证后才能获取迭代器
async micro authenticated_data() -> IteratorFuture⟨UserData⟩ {
    let token = authenticate().await?
    let data_iter = fetch_user_data(token).await?
    IteratorFuture(data_iter)
}
```

### Stream 错误处理策略

#### 1. 错误传播（Fail Fast）

```valkyrie
# 遇到错误立即停止
async micro strict_processing() {
    let stream = fetch_pages("https://api.example.com")
    
    async for page in stream {
        let processed = process_page(page).await?  # 错误会立即传播
        save_result(processed).await?
    }
}
```

#### 2. 错误跳过（Continue on Error）

```valkyrie
# 跳过错误项，继续处理
async micro resilient_processing() {
    let stream = fetch_pages("https://api.example.com")
    
    async for page_result in stream {
        try {
            let page = page_result?  # 解包 Result
            let processed = process_page(page).await?
            save_result(processed).await?
        }
        .catch {
            case ProcessingError(e):
                log_error("处理失败，跳过: ${ e }")
                continue
            case _:
                break  # 严重错误则停止
        }
    }
}
```

#### 3. 错误收集（Collect Errors）

```valkyrie
# 收集所有错误和成功结果
async micro collect_all_results() {
    let stream = fetch_pages("https://api.example.com")
    let mut results = []
    let mut errors = []
    
    async for page_result in stream {
        match page_result {
            case Fine { value: page }:
                try {
                    let processed = process_page(page).await?
                    results.push(processed)
                }
                .catch {
                    case e:
                        errors.push(e)
                }
            case Fail { error: e }:
                errors.push(e)
        }
    }
    
    (results, errors)
}
```

### Stream 组合操作

```valkyrie
# Stream 的函数式操作
async micro stream_operations() {
    let stream = fetch_pages("https://api.example.com")
    
    let processed_stream = stream
        .filter(async micro(page) { !page.is_empty() })  # 过滤空页面
        .map(async micro(page) { parse_json(page).await? })  # 解析 JSON
        .take(10)  # 只取前10个
        .buffer(3)  # 缓冲3个并发请求
    
    let results = processed_stream.collect().await?
    print("处理完成: ${ results.len() } 个结果")
}
```

### 背压控制（Backpressure）

```valkyrie
# 控制 Stream 的生产速度
class BackpressureStream⟨T⟩ {
    private buffer_size: usize
    private current_buffer: [T]
    
    async micro next_batch(mut self, batch_size: usize) -> [T] {
        # 实现背压控制逻辑
        while self.current_buffer.len() < batch_size {
            if let item? = self.source.next().await {
                self.current_buffer.push(item)
            } else {
                break
            }
        }
        
        self.current_buffer.drain(..batch_size.min(self.current_buffer.len()))
    }
}
```

通过 Stream 抽象，协程和生成器能够优雅地处理异步迭代场景，提供灵活的错误处理策略和高效的资源管理。

