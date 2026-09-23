# Reactive Programming

Reactive programming is a programming paradigm based on data streams and the propagation of change. In Valkyrie, reactive programming provides powerful data stream processing capabilities through abstractions such as Observable, Signal, and Reactive.

## Core Concepts

### Observable

Observable is the foundation of reactive programming, representing an observable stream of data:

```valkyrie
# Observable trait definition
trait Observable⟨T⟩ {
    micro subscribe⟨F⟩(self, observer: F) -> Subscription where F: micro(T) -> unit
    micro map⟨U, F⟩(self, f: F) -> Observable⟨U⟩ where F: micro(T) -> U
    micro filter⟨F⟩(self, predicate: F) -> Observable⟨T⟩ where F: micro(T) -> bool
    micro merge(self, other: Observable⟨T⟩) -> Observable⟨T⟩
    micro take(self, count: usize) -> Observable⟨T⟩
}

# Create Observable
let numbers = Observable.from([1, 2, 3, 4, 5])
let mouse_clicks = Observable.from_events("click")
let timer = Observable.interval(1000)  # Trigger once per second
```

### Signal

Signal is a reactive state with a current value:

```valkyrie
# Signal definition
class Signal⟨T⟩ {
    private value: T
    private observers: [micro(T) -> unit]
    
    micro Signal(initial: T) -> Signal⟨T⟩
    micro get(self) -> T
    micro set(mut self, new_value: T)
    micro update⟨F⟩(mut self, updater: F) where F: micro(T) -> T
    micro subscribe⟨F⟩(self, observer: F) -> Subscription where F: micro(T) -> unit
}

# Use Signal
let counter = Signal(0)
let doubled = counter.map { $x * 2 }

# Subscribe to changes
counter.subscribe {
    print("Counter value: ${$value}")
}

# Update value
counter.set(5)  # Output: Counter value: 5
counter.update { $x + 1 }  # Output: Counter value: 6
```

## Basic Operators

### Transformation Operators

```valkyrie
# map - transform each value
let numbers = Observable.from([1, 2, 3, 4, 5])
let squares = numbers.map { $x * $x }

# flat_map - flatten mapping
let words = Observable.from(["hello", "world"])
let characters = words.flat_map {
    Observable.from($word.chars())
}

# scan - accumulation operation
let numbers = Observable.from([1, 2, 3, 4, 5])
let running_sum = numbers.scan(0) { $acc, $x -> $acc + $x }
# Output: 1, 3, 6, 10, 15
```

### Filtering Operators

```valkyrie
# filter - filter values
let numbers = Observable.from([1, 2, 3, 4, 5, 6])
let evens = numbers.filter { $x % 2 == 0 }

# take - take the first N values
let first_three = numbers.take(3)

# skip - skip the first N values
let after_two = numbers.skip(2)

# distinct - remove duplicates
let unique = Observable.from([1, 1, 2, 2, 3, 3]).distinct()
```

### Combination Operators

```valkyrie
# merge - merge multiple streams
let stream1 = Observable.from([1, 3, 5])
let stream2 = Observable.from([2, 4, 6])
let merged = stream1.merge(stream2)

# zip - pair combination
let names = Observable.from(["Alice", "Bob", "Charlie"])
let ages = Observable.from([25, 30, 35])
let people = names.zip(ages).map { ($name, $age) ->
    Person { name: $name, age: $age }
}

# combine_latest - combine latest values
let temperature = Signal(20.0)
let humidity = Signal(60.0)
let comfort_index = temperature.combine_latest(humidity).map { ($temp, $hum) ->
    calculate_comfort($temp, $hum)
}
```

## Practical Application Examples

### Reactive UI Updates

```valkyrie
# Reactive UI Component
class CounterComponent {
    private count: Signal⟨i32⟩
    private increment_clicks: Observable⟨unit⟩
    private decrement_clicks: Observable⟨unit⟩
    
    micro CounterComponent() -> CounterComponent {
        let count = Signal(0)
        let increment_clicks = Observable.from_events("increment")
        let decrement_clicks = Observable.from_events("decrement")
        
        # Respond to click events
        increment_clicks.subscribe {
            count.update { $x + 1 }
        }
        
        decrement_clicks.subscribe {
            count.update { $x - 1 }
        }
        
        CounterComponent {
            count,
            increment_clicks,
            decrement_clicks
        }
    }
    
    micro render(self) -> Widget {
        let count_text = self.count.map { "Count: ${$value}" }
        
        VStack {
            Text(count_text)
            HStack {
                Button("Increment").on_click(self.increment_clicks)
                Button("Decrement").on_click(self.decrement_clicks)
            }
        }
    }
}
```

### Data Stream Processing

```valkyrie
# Real-time data processing pipeline
class DataProcessor {
    micro process_sensor_data(sensor_stream: Observable⟨SensorReading⟩) -> Observable⟨ProcessedData⟩ {
        sensor_stream
            .filter { $reading.is_valid() }  # Filter invalid data
            .map { $reading.normalize() }    # Normalize data
            .buffer(Duration.seconds(5))          # 5-second buffer window
            .map { $batch -> self.analyze_batch($batch) } # Batch analysis
            .filter { $result.confidence > 0.8 } # Filter low confidence results
    }
    
    private micro analyze_batch(batch: [SensorReading]) -> ProcessedData {
        let average = batch.iter().map { $r.value }.sum() / batch.len()
        let variance = calculate_variance(batch)
        
        ProcessedData {
            timestamp: now(),
            average,
            variance,
            confidence: calculate_confidence(variance)
        }
    }
}

# Use data processor
let processor = DataProcessor()
let sensor_stream = Observable.from_websocket("ws://sensor.example.com")
let processed_stream = processor.process_sensor_data(sensor_stream)

processed_stream.subscribe { $data ->
    print("Processing result: average=${$data.average}, confidence=${$data.confidence}")
    
    if $data.confidence > 0.95 {
        alert_system.notify("High confidence data: ${$data}")
    }
}
```

### Asynchronous Operation Composition

```valkyrie
# Reactive HTTP Client
class ReactiveHttpClient {
    micro get⟨T⟩(url: string) -> Observable⟨Result⟨T, HttpError⟩⟩ {
        Observable.create { $observer ->
            async {
                try {
                    let response = http_get(url).await?
                    let data = response.json⟨T⟩().await?
                    $observer.next(Fine { value: data })
                    $observer.complete()
                }
                .catch {
                    case $e:
                        $observer.error($e)
                }
            }
        }
    }
    
    micro retry⟨T⟩(observable: Observable⟨Result⟨T, HttpError⟩⟩, max_retries: usize) -> Observable⟨Result⟨T, HttpError⟩⟩ {
        observable.catch_error { $error ->
            if max_retries > 0 {
                print("Retrying request, remaining attempts: ${max_retries}")
                Observable.timer(Duration.seconds(1))
                    .flat_map { self.retry(observable, max_retries - 1) }
            } else {
                Observable.error($error)
            }
        }
    }
}

# Usage example
let client = ReactiveHttpClient()
let user_data = client.get⟨User⟩("https://api.example.com/user/123")
    .retry(3)  # Retry up to 3 times
    .timeout(Duration.seconds(10))  # 10-second timeout

user_data.subscribe { $result ->
    match $result {
        case Fine { value: $user }:
            print("User info: ${$user.name}")
        case Fail { error: $error }:
            print("Failed to get user info: ${$error}")
    }
}
```

## Error Handling

### Error Recovery Strategies

```valkyrie
# Error handling operators
trait ObservableErrorHandling⟨T⟩ {
    # Catch error and provide a default value
    micro catch_error⟨F⟩(self, handler: F) -> Observable⟨T⟩ where F: micro(Error) -> Observable⟨T⟩
    
    # Retry operation
    micro retry(self, count: usize) -> Observable⟨T⟩
    
    # Timeout handling
    micro timeout(self, duration: Duration) -> Observable⟨T⟩
}

# Practical use
let unreliable_stream = fetch_data_stream()
    .catch_error { $error ->
        print("Error occurred: ${$error}, using cached data")
        Observable.from(cached_data)
    }
    .retry(3)
    .timeout(Duration.seconds(30))

unreliable_stream.subscribe { $data ->
    process_data($data)
}
```

### Error Propagation Control

```valkyrie
# Partial error handling
let mixed_stream = Observable.from([1, 2, 3, 4, 5])
    .map { $x ->
        if $x == 3 {
            raise ValueError { error: "Invalid value: 3" }
        }
        $x * 2
    }
    .on_error_resume_next { $error ->
        print("Skipping error: ${$error}")
        Observable.empty()  # Skip error items
    }

mixed_stream.subscribe { $value ->
    print("Processed value: ${$value}")
}  # Output: 2, 4, 8, 10 (3 is skipped)
```

## Resource Management

### Subscription Lifecycle

```valkyrie
# Subscription management
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

# CompositeSubscription used for managing multiple subscriptions
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

# Usage example
let composite = CompositeSubscription()

let sub1 = timer_stream.subscribe { print("Timer triggered") }
let sub2 = click_stream.subscribe { print("Click event") }

composite.add(sub1)
composite.add(sub2)

# Cleanup all subscriptions when the component is destroyed
composite.dispose_all()
```

### Backpressure Handling

```valkyrie
# Backpressure strategies
struct BackpressureStrategy {
    micro Buffer(capacity: usize)     # Buffer strategy
    micro Drop                        # Drop strategy
    micro Latest                      # Keep latest strategy
    micro Error                       # Error strategy
}

# Apply backpressure control
let fast_producer = Observable.interval(Duration.milliseconds(1))  # Produce data every millisecond
let slow_consumer = fast_producer
    .observe_on(Scheduler.computation())  # Process in computation thread pool
    .buffer(100)  # Buffer 100 elements
    .sample(Duration.seconds(1))  # Sample once per second

slow_consumer.subscribe { $batch ->
    print("Processing batch, size: ${$batch.len()}")
    # Slow processing logic
    Thread.sleep(Duration.milliseconds(100))
}
```

## Schedulers

### Thread Scheduling

```valkyrie
# Scheduler types
struct Scheduler {
    micro CurrentThread    # Current thread
    micro Computation      # Computation thread pool
    micro IO               # I/O thread pool
    micro NewThread        # New thread
    micro Trampoline       # Trampoline scheduler
}

# Specify schedulers
let data_stream = Observable.from_file("large_file.txt")
    .subscribe_on(Scheduler.IO())        # Read file in I/O thread
    .observe_on(Scheduler.Computation()) # Process data in computation thread
    .map { $line -> expensive_computation($line) }
    .observe_on(Scheduler.CurrentThread()) # Update UI in main thread

data_stream.subscribe { $result ->
    update_ui($result)  # UI updates must be on the main thread
}
```

## Testing Support

### Test Scheduler

```valkyrie
# Virtual time scheduler for testing
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

# Testing example
#[test]
micro test_timer_observable() {
    let scheduler = TestScheduler()
    let timer = Observable.timer(Duration.seconds(5), scheduler)
    let mut received_values = []
    
    timer.subscribe { $value ->
        received_values.push($value)
    }
    
    # Advance virtual time
    scheduler.advance_time_by(Duration::seconds(3))
    assert_eq!(received_values.len(), 0)  # Not yet triggered
    
    scheduler.advance_time_by(Duration::seconds(3))
    assert_eq!(received_values.len(), 1)  # Timer triggered
}
```

## Best Practices

### 1. Avoid Memory Leaks

```valkyrie
# Proper subscription management
class Component {
    private subscriptions: CompositeSubscription
    
    micro Component() -> Component {
        let subscriptions = CompositeSubscription()
        
        # Subscribe to data stream
        let sub = data_stream.subscribe { $data ->
            self.handle_data($data)
        }
        
        subscriptions.add(sub)
        
        Component { subscriptions }
    }
    
    micro destroy(mut self) {
        # Cleanup subscriptions when component is destroyed
        self.subscriptions.dispose_all()
    }
}
```

### 2. Reasonable Use of Operators

```valkyrie
# Optimized operator chain
let optimized_stream = source_stream
    .filter { $x.is_valid() }     # Filter early
    .take(1000)                   # Limit count
    .map { $x.transform() }       # Transform data
    .distinct()                   # Remove duplicates
    .buffer(Duration.seconds(1)) # Batch processing

# Avoid overly long operator chains
let intermediate = source_stream
    .filter { $x.is_valid() }
    .map { $x.normalize() }

let final_stream = intermediate
    .group_by { $x.category }
    .flat_map { $group -> $group.buffer(10) }
```

### 3. Error Boundaries

```valkyrie
# Set error boundaries to prevent the entire stream from crashing
let resilient_stream = risky_stream
    .map { $item ->
        try {
            process_item($item)
        }
        .catch {
            case ProcessingError { error: $e }:
                log_error("Processing failed: ${$e}")
                default_value()  # Provide default value
            case _:
                raise  # Re-throw critical errors
        }
    }
    .filter { $result? }
```

Through these reactive programming patterns, Valkyrie provides powerful and flexible data stream processing capabilities, enabling developers to build responsive and maintainable applications.
