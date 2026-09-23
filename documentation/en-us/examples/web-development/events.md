# 事件 (Events)

在 Valkyrie 中，事件是实现组件间解耦和响应式编程的关键机制。Valkyrie 提供了两种类型的事件声明，以区分其预期行为和消费者数量：单点事件 (`event`) 和广播事件 (`events`)。

## 1. 单点事件 (Single-Point Event)

单点事件是指那些**预期只有一个或少数特定订阅者**的事件。这类事件通常用于回调、特定状态的唯一响应或生命周期事件。

### 声明语法

使用 `event` 关键字声明单点事件：

```valkyrie
event event_name(parameter1: Type1, parameter2: Type2, ...)
```

### 示例

```valkyrie
class AsyncOperation {
    // 定义一个单点事件，用于通知操作完成
    event on_completed(result: Result<String, Error>)

    micro start(mut self) {
        print("异步操作开始...")
        // 模拟异步操作完成并触发事件
        // 假设这里有一个内部机制来调用 on_completed
        // 例如：self.on_completed(Fine { value: "操作成功" })
    }
}

class MyProcessor {
    micro process(self) {
        let op = AsyncOperation::new()
        // 订阅单点事件
        op.on_completed = micro(result) {
            match result {
                case Fine { value }: print("操作完成: ${value}")
                case Fail { error }: print("操作失败: ${error}")
            }
        }
        op.start()
    }
}
```

### 改善

*   **明确的意图**: 开发者一眼就能看出 `on_completed` 是一个单点事件，不应该有多个消费者。
*   **编译时/运行时检查**: 语言可以强制执行“单点”的约束，防止意外地附加多个处理器。
*   **简化 API**: 订阅和取消订阅的 API 可能更简单，例如直接赋值即可替换旧的处理器。

## 2. 广播事件 (Broadcast Event)

广播事件是指那些**预期可以有零个、一个或多个订阅者**的事件。这类事件通常用于 UI 事件、系统级通知或领域事件。

### 声明语法

使用 `events` 关键字声明广播事件：

```valkyrie
events event_name(parameter1: Type1, parameter2: Type2, ...)
```

### 示例

```valkyrie
class Button {
    // 定义一个广播事件，用于通知点击
    events clicked(sender: &Button, args: &ClickEventArgs)

    micro simulate_click(mut self) {
        print("按钮被点击了！")
        // 触发所有订阅者
        // 假设语言提供内置的触发机制，例如：
        self.clicked.trigger(self, ClickEventArgs::new())
    }
}

class Logger {
    micro log_click(self, sender: &Button, args: &ClickEventArgs) {
        print("日志：按钮 '${sender.id}' 被点击。")
    }
}

class Analytics {
    micro track_click(self, sender: &Button, args: &ClickEventArgs) {
        print("分析：记录按钮 '${sender.id}' 的点击事件。")
    }
}

micro main() {
    let mut my_button = Button { id: "submit_btn" }
    let logger = Logger::new()
    let analytics = Analytics::new()

    // 订阅广播事件
    my_button.clicked.add(micro(s, a) { logger.log_click(s, a) })
    my_button.clicked.add(micro(s, a) { analytics.track_click(s, a) })

    my_button.simulate_click()
}
```

### 改善

*   **明确的意图**: 清楚地表明这是一个可以被多个消费者订阅的事件。
*   **内置的订阅管理**: 语言自动处理订阅者列表的添加 (`add`)、移除 (`remove`) 和遍历调用。
*   **标准化的 API**: 提供统一的订阅/取消订阅接口，提高代码的一致性和可读性。

## 总结

在 Valkyrie 中引入 `event` 和 `events` 关键词来区分单点事件和广播事件，将带来以下显著改善：

1.  **提高代码清晰度**: 开发者通过关键词就能立即理解事件的预期行为和消费者数量。
2.  **增强类型安全和约束**: 语言可以在编译时或运行时强制执行事件的“单点”或“多点”约束。
3.  **简化开发**: 语言内置的事件管理机制将大大减少样板代码。
4.  **优化性能**: 针对单点事件，语言可以进行更激进的优化。
5.  **促进良好设计**: 鼓励开发者在设计事件时就考虑其传播范围和消费者数量。