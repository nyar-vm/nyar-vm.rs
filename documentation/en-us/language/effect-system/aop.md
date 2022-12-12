# Aspect-Oriented Programming (AOP)

Aspect-Oriented Programming (AOP) is a programming paradigm that increases modularity by allowing the separation of cross-cutting concerns (such as logging, security, transaction management, etc.) from the core business logic. In Valkyrie, AOP is implemented through the Effect system, providing powerful and flexible aspect-oriented programming capabilities.

## Basic Concepts

### Aspect
An Aspect is a modular unit of a cross-cutting concern, containing code to be executed at specific join points.

### Join Point
A Join Point is a point during the execution of a program where aspect code can be inserted, such as method calls or field accesses.

### Advice
Advice is the code executed at a join point, including before advice, after advice, around advice, etc.

## AOP in the Effect System

### Defining Aspect Effects

```valkyrie
# Define Logging Aspect Effect
effect LogAspect {
    before_method(class_name: string, method_name: string, args: [Any]): Unit
    after_method(class_name: string, method_name: string, result: Any): Unit
    on_error(class_name: string, method_name: string, error: Any): Unit
}

# Define Performance Monitoring Aspect Effect
effect MetricsAspect {
    start_timing(operation: string): TimingContext
    end_timing(context: TimingContext): Duration
    record_metric(name: string, value: f64): Unit
}
```

### Implementing Aspect Handlers

```valkyrie
# Logging Aspect Handler
class ConsoleLogHandler {
    handle LogAspect {
        before_method(class_name, method_name, args) {
            print(f"→ {class_name}.{method_name}({args})")
        }
        
        after_method(class_name, method_name, result) {
            print(f"← {class_name}.{method_name} = {result}")
        }
        
        on_error(class_name, method_name, error) {
            print(f"× {class_name}.{method_name} throws {error}")
        }
    }
}

# Performance Monitoring Handler
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

### Using Aspect Annotations

```valkyrie
class PaymentService {
    # Using multiple aspects
    @around(LogAspect, MetricsAspect)
    micro process_payment(self, order_id: String, amount: Decimal) -> Receipt {
        # Before advice executes automatically
        raise LogAspect::before_method("PaymentService", "process_payment", [order_id, amount])
        let timing_ctx = raise MetricsAspect::start_timing("payment_processing")
        
        try {
            # Core business logic
            let receipt = self.do_payment(order_id, amount)
            
            # After advice
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
    
    private micro do_payment(self, order_id: String, amount: Decimal) -> Receipt {
        # Actual payment logic
        Receipt { id: generate_id(), order_id, amount, timestamp: now() }
    }
}
```

### Combining Aspects

```valkyrie
# Define Composite Aspect
effect AuditAspect {
    log_access(user_id: string, resource: string, action: string): Unit
    log_data_change(table: string, record_id: string, old_value: Any, new_value: Any): Unit
}

# Security Audit Service
class SecurityAuditService {
    @around(LogAspect, AuditAspect)
    micro update_user_profile(self, user_id: string, profile: UserProfile) -> Unit {
        raise AuditAspect::log_access(user_id, "user_profile", "update")
        
        let old_profile = self.get_user_profile(user_id)
        
        # Update logic
        self.save_user_profile(user_id, profile)
        
        raise AuditAspect::log_data_change("users", user_id, old_profile, profile)
    }
}
```

### Conditional Aspects

```valkyrie
# Define Conditional Aspect Effect
effect ConditionalAspect {
    should_apply(context: AspectContext) -> bool
    apply_advice(context: AspectContext): Unit
}

class DebugAspect {
    handle ConditionalAspect {
        should_apply(context) -> bool {
            # Only apply in debug mode
            config.debug_mode && context.method_name.starts_with("debug_")
        }
        
        apply_advice(context) {
            print(f"Debug: {context.class_name}.{context.method_name}")
        }
    }
}
```

### Aspect Composers

```valkyrie
# Aspect Composer
class AspectComposer {
    static micro compose_aspects⟨T⟩(aspects: [Effect]) -> Effect {
        effect ComposedAspect {
            execute(context: AspectContext): T
        }
        
        handle ComposedAspect {
            execute(context) -> T {
                # Execute before advice of all aspects in order
                for aspect in aspects {
                    raise aspect.before(context)
                }
                
                try {
                    let result = context.proceed()
                    
                    # Execute after advice of all aspects in reverse order
                    for aspect in aspects.reverse() {
                        raise aspect.after(context, result)
                    }
                    
                    result
                }
                .catch {
                    case _:
                        # Execute error advice
                        for aspect in aspects.reverse() {
                            raise aspect.on_error(context, error)
                        }
                        raise error
                }
            }
        }
    }
}
```

## Advanced Features

### Dynamic Aspects

```valkyrie
# Dynamic Aspect Manager
class DynamicAspectManager {
    private mut aspects: [Effect] = []
    
    micro add_aspect(self, aspect: Effect) {
        self.aspects.push(aspect)
    }
    
    micro remove_aspect(self, aspect: Effect) {
        self.aspects.retain { $a != aspect }
    }
    
    @around(DynamicAspect)
    micro execute_with_aspects⟨T⟩(self, operation: { -> T }) -> T {
        let context = AspectContext::new()
        
        # Dynamically apply all registered aspects
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

### Aspect Chains

```valkyrie
# Aspect Chain Processing
class AspectChain {
    private aspects: [Effect]
    private mut current_index: usize = 0
    
    micro proceed<T>(mut self, context: AspectContext) -> T {
        if self.current_index >= self.aspects.len() {
            # Execute original method
            context.target_method()
        } else {
            let aspect = self.aspects[self.current_index]
            self.current_index += 1
            
            # Execute current aspect
            raise aspect::around(context, || { self.proceed(context) })
        }
    }
}
```

## Best Practices

### 1. Single Responsibility for Aspects
Each aspect should focus on a single cross-cutting concern, avoiding mixing multiple functionalities within a single aspect.

### 2. Rational Use of Aspect Order
When multiple aspects act on the same join point, consider their execution order.

### 3. Avoid Strong Coupling Between Aspects
Aspects should remain independent, avoiding mutual dependencies.

### 4. Performance Considerations
Aspects increase the overhead of method calls; use them cautiously in performance-sensitive scenarios.

```valkyrie
# Example: Complete AOP Application
class OrderService {
    @around(LogAspect, MetricsAspect, SecurityAspect, TransactionAspect)
    micro create_order(self, user_id: string, items: [OrderItem]) -> Order {
        # Before advice for all aspects executes automatically
        # - Log method call
        # - Start performance timing
        # - Security check
        # - Start transaction
        
        let order = Order {
            id: generate_order_id(),
            user_id,
            items,
            status: OrderStatus.Pending,
            created_at: now()
        }
        
        self.save_order(order)
        
        # After advice for all aspects executes automatically
        # - Commit transaction
        # - Record security log
        # - Record performance metrics
        # - Record method return value
        
        order
    }
}

# Usage Example
let order_service = OrderService {}
let log_handler = ConsoleLogHandler {}
let metrics_handler = MetricsHandler {}

with log_handler, metrics_handler {
    let order = order_service.create_order("user123", [
        OrderItem { product_id: "p1", quantity: 2 },
        OrderItem { product_id: "p2", quantity: 1 }
    ])
    print(f"Created order: {order.id}")
}
```

AOP implemented through the Effect system provides type-safe, highly composable, and easy-to-test aspect programming capabilities, making the management of cross-cutting concerns more elegant and powerful.
