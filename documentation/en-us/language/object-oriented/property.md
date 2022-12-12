# Property System

## Overview

Valkyrie's property system provides an elegant way to encapsulate field access, allowing developers to define getter and setter methods while maintaining concise access syntax. The property system follows the Uniform Access Principle, making field access and method calls syntactically consistent.

## Basic Property Definition

### Getter Properties

```valkyrie
class Rectangle {
    width: f64,
    height: f64,
    
    # Calculated property - area
    get area(self) -> f64 {
        self.width * self.height
    }
    
    # Calculated property - perimeter
    get perimeter(self) -> f64 {
        2.0 * (self.width + self.height)
    }
}

# Usage
let rect = Rectangle { width: 10.0, height: 5.0 }
let area = rect.area        # Calls getter, returns 50.0
let perimeter = rect.perimeter  # Calls getter, returns 30.0
```

### Setter Properties

```valkyrie
class Temperature {
    celsius: f64,
    
    # Getter - Get temperature in Fahrenheit
    get fahrenheit(self) -> f64 {
        self.celsius * 9.0 / 5.0 + 32.0
    }
    
    # Setter - Set temperature in Fahrenheit
    set fahrenheit(mut self, value: f64) {
        self.celsius = (value - 32.0) * 5.0 / 9.0
    }
    
    # Getter - Get temperature in Kelvin
    get kelvin(self) -> f64 {
        self.celsius + 273.15
    }
    
    # Setter - Set temperature in Kelvin
    set kelvin(mut self, value: f64) {
        self.celsius = value - 273.15
    }
}

# Usage
let mut temp = Temperature { celsius: 25.0 }
print("Celsius: ${temp.celsius}")     # 25.0
print("Fahrenheit: ${temp.fahrenheit}")   # 77.0
print("Kelvin: ${temp.kelvin}")      # 298.15

# Modify temperature via setter
temp.fahrenheit = 86.0  # Set Fahrenheit
print("Celsius: ${temp.celsius}")     # 30.0

temp.kelvin = 300.0     # Set Kelvin
print("Celsius: ${temp.celsius}")     # 26.85
```

## Read-Only and Write-Only Properties

### Read-Only Properties

```valkyrie
class Person {
    first_name: string,
    last_name: string,
    birth_year: u32,
    
    # Read-only property - full name
    get full_name(self) -> string {
        "${self.first_name} ${self.last_name}"
    }
    
    # Read-only property - age (based on current year)
    get age(self) -> u32 {
        2024 - self.birth_year  # Simplified example
    }
}

let person = Person {
    first_name: "John",
    last_name: "Doe",
    birth_year: 1990
}

print(person.full_name)  # "John Doe"
print(person.age)        # 34
# person.full_name = "Jane Doe"  # Compile error: no setter
```

### Write-Only Properties

```valkyrie
class Logger {
    messages: [string],
    
    # Write-only property - add log message
    set message(mut self, msg: string) {
        let timestamp = get_current_timestamp()
        self.messages.push("[${timestamp}] ${msg}")
    }
    
    # Method to get all messages
    micro get_messages(self) -> &[string] {
        &self.messages
    }
}

let mut logger = Logger { messages: [] }
logger.message = "System started"  # Use setter
logger.message = "User logged in"  # Use setter
# let msg = logger.message   # Compile error: no getter
```

## Property Validation

```valkyrie
class BankAccount {
    balance: f64,
    min_balance: f64,
    
    # Setter with validation
    set balance(mut self, value: f64) {
        if value < self.min_balance {
            panic("Balance cannot be lower than minimum: ${self.min_balance}")
        }
        self.balance = value
    }
    
    get balance(self) -> f64 {
        self.balance
    }
}

let mut account = BankAccount {
    balance: 1000.0,
    min_balance: 100.0
}

account.balance = 500.0   # Normal assignment
# account.balance = 50.0  # Runtime panic
```

## Lazy-Loaded Properties

```valkyrie
class DataProcessor {
    raw_data: [string],
    processed_data: [ProcessedItem]?,
    
    # Lazy-loaded calculated property
    get processed(mut self) -> &[ProcessedItem] {
        if self.processed_data.is_none() {
            let processed = self.raw_data
                .iter()
                .map { $item -> self.process_item($item) }
                .collect()
            self.processed_data = Some(processed)
        }
        
        if let Some(ref data) = self.processed_data {
            data
        } else {
            unreachable!()
        }
    }
    
    micro process_item(self, item: &string) -> ProcessedItem {
        # Complex processing logic
        ProcessedItem::from(item)
    }
}
```

## Property Chaining

```valkyrie
class Builder {
    name: string?,
    age: u32?,
    email: string?,
    
    # Chained setters
    set name(mut self, value: string) -> Self {
        self.name = Some(value)
        self
    }
    
    set age(mut self, value: u32) -> Self {
        self.age = Some(value)
        self
    }
    
    set email(mut self, value: string) -> Self {
        self.email = Some(value)
        self
    }
    
    micro build(self) -> Person {
        Person {
            name: self.name.unwrap_or("Unknown"),
            age: self.age.unwrap_or(0),
            email: self.email.unwrap_or("unknown↯example.com")
        }
    }
}

# Chained calls
let person = Builder::new()
    .name("Alice")
    .age(30)
    .email("alice↯example.com")
    .build()
```

## Static Properties

```valkyrie
class MathConstants {
    # Static read-only properties
    static get pi() -> f64 {
        3.14159265359
    }
    
    static get e() -> f64 {
        2.71828182846
    }
    
    # Static mutable property
    static mut counter: u32 = 0
    
    static get next_id() -> u32 {
        Self::counter += 1
        Self::counter
    }
}

# Using static properties
let pi_value = MathConstants::pi
let id1 = MathConstants::next_id  # 1
let id2 = MathConstants::next_id  # 2
```

## Property Overriding

```valkyrie
class Shape {
    # Virtual property
    virtual get area(self) -> f64 {
        0.0
    }
}

class Circle: Shape {
    radius: f64,
    
    # Override parent property
    override get area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
}

class Square: Shape {
    side: f64,
    
    # Override parent property
    override get area(self) -> f64 {
        self.side * self.side
    }
}
```

## Best Practices

### 1. Use Properties for Data Encapsulation

```valkyrie
# Good practice: use properties to encapsulate internal state
class Counter {
    value: u32,
    
    get count(self) -> u32 {
        self.value
    }
    
    set count(mut self, new_value: u32) {
        if new_value > 1000 {
            panic("Counter value cannot exceed 1000")
        }
        self.value = new_value
    }
    
    micro increment(mut self) {
        self.count = self.count + 1
    }
}
```

### 2. Avoid Getters with Excessive Side Effects

```valkyrie
# Avoid: complex side effects in getters
class BadExample {
    get data(mut self) -> [string] {
        # Bad: re-calculates on every access
        expensive_computation()
    }
}

# Recommended: use lazy loading or caching
class GoodExample {
    cached_data: [string]?,
    
    get data(mut self) -> &[string] {
        if self.cached_data.is_none() {
            self.cached_data = Some(expensive_computation())
        }
        self.cached_data.as_ref().unwrap()
    }
}
```

### 3. Maintain Property Semantic Consistency

```valkyrie
class Rectangle {
    width: f64,
    height: f64,
    
    # Good practice: getter and setter operate on the same concept
    get area(self) -> f64 {
        self.width * self.height
    }
    
    # If providing an area setter, it should reasonably update width and height
    set area(mut self, new_area: f64) {
        let ratio = (new_area / self.area).sqrt()
        self.width *= ratio
        self.height *= ratio
    }
}
```

## Summary

Valkyrie's property system provides:

1. **Uniform Access Principle**: Fields and calculated properties use the same access syntax.
2. **Data Encapsulation**: Control data access via getters/setters.
3. **Calculated Properties**: Support for dynamically computed values.
4. **Validation Mechanism**: Add data validation logic in setters.
5. **Lazy Loading**: Support for deferred computation and caching.
6. **Chained Calls**: Support for fluent API design.

Correct use of the property system improves code encapsulation, maintainability, and user experience.
