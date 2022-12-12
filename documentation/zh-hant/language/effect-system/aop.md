# 面向切面編程 (Aspect-Oriented Programming)

面向切面編程（AOP）是一種編程範式，通過將橫切關注點（如日誌、安全、事務管理等）從核心業務邏輯中分離出來，提高代碼的模組化程度。在 Valkyrie 中，AOP 通過 Effect 系統實現，提供了強大而靈活的切面編程能力。

## 基本概念

### 切面 (Aspect)
切面是橫切關注點的模組化單元，包含了在特定連接點執行的代碼。

### 連接點 (Join Point)
連接點是程序執行過程中可以插入切面代碼的點，如方法呼叫、欄位存取等。

### 通知 (Advice)
通知是在連接點執行的代碼，包括前置通知、後置通知、環繞通知等。

## Effect 系統中的 AOP

### 定義切面 Effect

```valkyrie
# 定義日誌切面 Effect
effect LogAspect {
    before_method(class_name: string, method_name: string, args: [Any]): Unit
    after_method(class_name: string, method_name: string, result: Any): Unit
    on_error(class_name: string, method_name: string, error: Any): Unit
}

# 定義性能監控切面 Effect
effect MetricsAspect {
    start_timing(operation: string): TimingContext
    end_timing(context: TimingContext): Duration
    record_metric(name: string, value: f64): Unit
}
```

### 實現切面處理器

```valkyrie
# 切面處理器
class ConsoleLogHandler {
    handle LogAspect {
        before_method(class_name, method_name, args) {
            print("→ {class_name}::{method_name}({args})")
        }
        
        after_method(class_name, method_name, result) {
            print("← {class_name}::{method_name} = {result}")
        }
        
        on_error(class_name, method_name, error) {
            print("× {class_name}::{method_name} throws {error}")
        }
    }
}

# 性能監控處理器
class MetricsHandler {
    private mut timings: {string: DateTime} = {}
    
    handle MetricsAspect {
        start_timing(operation) -> TimingContext {
            let start_time = now()
            self.timings[operation] = start_time
            TimingContext { operation, start_time }
        }
        
        end_timing(context) -> Duration {
            let end_time = now()
            let duration = end_time - context.start_time
            self.timings.remove(context.operation)
            duration
        }
        
        record_metric(name, value) {
            metrics_store.record(name, value)
        }
    }
}
```

### 使用切面註解

```valkyrie
class PaymentService {
    # 使用多個切面
    @around(LogAspect, MetricsAspect)
    micro process_payment(self, order_id: string, amount: Decimal) -> Receipt {
        # 前置通知自動執行
        raise LogAspect::before_method("PaymentService", "process_payment", [order_id, amount])
        let timing_ctx = raise MetricsAspect::start_timing("payment_processing")
        
        try {
            # 核心業務邏輯
            let receipt = self.do_payment(order_id, amount)
            
            # 後置通知
            raise LogAspect::after_method("PaymentService", "process_payment", receipt)
            let duration = raise MetricsAspect::end_timing(timing_ctx)
            raise MetricsAspect::record_metric("payment_duration", duration.as_millis())
            
            receipt
        }
        .catch {
            case error:
                raise LogAspect::on_error("PaymentService", "process_payment", error)
                raise error
        }
    }
    
    private micro do_payment(self, order_id: string, amount: Decimal) -> Receipt {
        # 實際支付邏輯
        Receipt { id: generate_id(), order_id, amount, timestamp: now() }
    }
}
```

### 組合切面

```valkyrie
# 定義複合切面
effect AuditAspect {
    log_access(user_id: string, resource: string, action: string): Unit
    log_data_change(table: string, record_id: string, old_value: Any, new_value: Any): Unit
}

# 安全審計服務
class SecurityAuditService {
    @around(LogAspect, AuditAspect)
    micro update_user_profile(self, user_id: string, profile: UserProfile) -> Unit {
        raise AuditAspect::log_access(user_id, "user_profile", "update")
        
        let old_profile = self.get_user_profile(user_id)
        
        # 更新邏輯
        self.save_user_profile(user_id, profile)
        
        raise AuditAspect::log_data_change("users", user_id, old_profile, profile)
    }
}
```

### 條件切面

```valkyrie
# 定義條件切面 Effect
effect ConditionalAspect {
    should_apply(context: AspectContext) -> bool
    apply_advice(context: AspectContext): Unit
}

class DebugAspect {
    // 處理器實現
    micro apply(context: AspectContext) {
        try {
            if (raise ConditionalAspect::should_apply(context)) {
                raise ConditionalAspect::apply_advice(context)
            }
        }
        .catch {
            case ConditionalAspect::should_apply(ctx): 
                resume(config.debug_mode && ctx.method_name.starts_with("debug_"))
            case ConditionalAspect::apply_advice(ctx):
                print("Debug: {ctx.class_name}.{ctx.method_name}")
                resume()
        }
    }
}
```

### 切面組合器

```valkyrie
# 切面組合器
class AspectComposer {
    static micro compose_aspects⟨T⟩(aspects: [Effect]) -> Effect {
        effect ComposedAspect {
            execute(context: AspectContext): T
        }
        
        try {
            // 執行邏輯
        }
        .catch {
            case ComposedAspect::execute(context):
                # 按順序執行所有切面的前置通知
                for aspect in aspects {
                    raise aspect::before(context)
                }
                
                try {
                    let result = context.proceed()
                    
                    # 按逆序執行所有切面的後置通知
                    for aspect in aspects.reverse() {
                        raise aspect::after(context, result)
                    }
                    
                    resume(result)
                }
                .catch {
                    case error:
                        # 執行異常通知
                        for aspect in aspects.reverse() {
                            raise aspect::on_error(context, error)
                        }
                        raise error
                }
        }
    }
}
```

## 高級特性

### 動態切面

```valkyrie
# 動態切面管理器
class DynamicAspectManager {
    private mut aspects: [Effect] = []
    
    micro add_aspect(self, aspect: Effect) {
        self.aspects.push(aspect)
    }
    
    micro remove_aspect(self, aspect: Effect) {
        self.aspects.retain { $a != aspect }
    }
    
    @around(DynamicAspect)
    micro execute_with_aspects<T>(self, operation: { -> T }) -> T {
        let context = AspectContext::new()
        
        # 動態應用所有註冊的切面
        for aspect in self.aspects {
            raise aspect::before(context)
        }
        
        try {
            let result = operation()
            
            for aspect in self.aspects.reverse() {
                raise aspect::after(context, result)
            }
            
            result
        }
        .catch {
            case error:
                for aspect in self.aspects.reverse() {
                    raise aspect::on_error(context, error)
                }
                raise error
        }
    }
}
```

### 切面鏈

```valkyrie
# 切面鏈處理
class AspectChain {
    private aspects: [Effect]
    private mut current_index: usize = 0
    
    micro proceed<T>(mut self, context: AspectContext) -> T {
        if self.current_index >= self.aspects.len() {
            # 執行原始方法
            context.target_method()
        } else {
            let aspect = self.aspects[self.current_index]
            self.current_index += 1
            
            # 執行當前切面
            raise aspect::around(context, || { self.proceed(context) })
        }
    }
}
```

## 最佳實踐

### 1. 切面職責單一
每個切面應該只關注一個橫切關注點，避免在單個切面中混合多種功能。

### 2. 合理使用切面順序
當多個切面作用於同一個連接點時，要考慮他們的執行順序。

### 3. 避免切面間的強耦合
切面之間應該保持獨立，避免相互依賴。

### 4. 性能考慮
切面會增加方法呼叫的開銷，在性能敏感的場景中要謹慎使用。

```valkyrie
# 示例：完整的 AOP 應用
class OrderService {
    @around(LogAspect, MetricsAspect, SecurityAspect, TransactionAspect)
    micro create_order(self, user_id: string, items: [OrderItem]) -> Order {
        # 所有切面的前置通知會自動執行
        # - 日誌記錄方法呼叫
        # - 開始性能計時
        # - 安全檢查
        # - 開始事務
        
        let order = Order {
            id: generate_order_id(),
            user_id,
            items,
            status: OrderStatus::Pending,
            created_at: now()
        }
        
        self.save_order(order)
        
        # 所有切面的後置通知會自動執行
        # - 提交事務
        # - 記錄安全日誌
        # - 記錄性能指標
        # - 記錄方法返回值
        
        order
    }
}

# 使用示例
let order_service = OrderService {}
let log_handler = ConsoleLogHandler {}
let metrics_handler = MetricsHandler {}

with log_handler, metrics_handler {
    let order = order_service.create_order("user123", [
        OrderItem { product_id: "p1", quantity: 2 },
        OrderItem { product_id: "p2", quantity: 1 }
    ])
    print("Created order: {order.id}")
}
```

通過 Effect 系統實現的 AOP 提供了類型安全、組合性強、易於測試的切面編程能力，使得橫切關注點的管理變得更加優雅和強大。
