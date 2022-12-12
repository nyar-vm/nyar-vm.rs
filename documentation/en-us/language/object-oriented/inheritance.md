# Multiple Inheritance System

## Overview

Valkyrie supports multiple inheritance, allowing a class to inherit from multiple parent classes simultaneously. Multiple inheritance uses the C3 linearization algorithm to resolve Method Resolution Order (MRO) issues, ensuring consistency and predictability in inheritance.

## Basic Multiple Inheritance Syntax

### Simple Multiple Inheritance

```valkyrie
class A {
    micro method_a(self) {
        print("Method from A")
    }
    
    micro common_method(self) {
        print("A's common method")
    }
}

class B {
    micro method_b(self) {
        print("Method from B")
    }
    
    micro common_method(self) {
        print("B's common method")
    }
}

class C {
    micro method_c(self) {
        print("Method from C")
    }
    
    micro common_method(self) {
        print("C's common method")
    }
}

# Multiple inheritance syntax: class Child(Parent1, Parent2, ...)
class MultiChild(A, B, C) {
    micro own_method(self) {
        print("MultiChild's own method")
    }
}
```

### Renaming Inheritance

When multiple parent classes have methods with the same name, you can use renaming syntax to avoid conflicts:

```valkyrie
class Display {
    micro show(self) {
        print("Display show")
    }
}

class Printer {
    micro show(self) {
        print("Printer show")
    }
    
    micro print(self) {
        print("Printing...")
    }
}

# Renaming inheritance syntax: class Child(rename: Parent, OtherParent)
class Document(rename: Display, Printer) {
    micro display_document(self) {
        # Access Display's method via renaming
        self.rename.show()  # Calls Display::show
        self.print()        # Calls Printer::print
        self.show()         # Calls Printer::show (first match in C3 linearization)
    }
}
```

### Complex Renaming Scenarios

```valkyrie
class FileReader {
    micro read(self) -> string {
        "Reading from file"
    }
    
    micro close(self) {
        print("Closing file")
    }
}

class NetworkReader {
    micro read(self) -> string {
        "Reading from network"
    }
    
    micro close(self) {
        print("Closing connection")
    }
}

class Logger {
    micro log(self, message: string) {
        print("Log: {}", message)
    }
}

# Multiple renaming
class HybridReader(file_reader: FileReader, net_reader: NetworkReader, Logger) {
    micro read_from_file(self) -> string {
        let content = self.file_reader.read()
        self.log(↯format("Read from file: {}", content))
        content
    }
    
    micro read_from_network(self) -> string {
        let content = self.net_reader.read()
        self.log(↯format("Read from network: {}", content))
        content
    }
    
    micro cleanup(self) {
        self.file_reader.close()
        self.net_reader.close()
    }
}
```

## C3 Linearization Algorithm

Valkyrie uses the C3 linearization algorithm to determine the Method Resolution Order:

```valkyrie
class A {
    micro method(self) { print("A") }
}

class B(A) {
    micro method(self) { print("B") }
}

class C(A) {
    micro method(self) { print("C") }
}

class D(B, C) {
    # No override for method
}

# C3 linearization order: D -> B -> C -> A
# Calling d.method() will call B::method
let d = D {}
d.method()  # Output: "B"
```

### Linearization Order Example

```valkyrie
class Base {
    micro base_method(self) { print("Base") }
}

class Left(Base) {
    micro left_method(self) { print("Left") }
    micro common_method(self) { print("Left common") }
}

class Right(Base) {
    micro right_method(self) { print("Right") }
    micro common_method(self) { print("Right common") }
}

class Middle(Left, Right) {
    micro middle_method(self) { print("Middle") }
}

class Final(Middle, Right) {
    # C3 linearization: Final -> Middle -> Left -> Right -> Base
    micro test_resolution(self) {
        self.common_method()  # Calls Left::common_method
        self.left_method()    # Calls Left::left_method
        self.right_method()   # Calls Right::right_method
        self.base_method()    # Calls Base::base_method
    }
}
```

## Method Access Patterns

### Direct Access

```valkyrie
class Child(A, B, C) {
    micro test_access(self) {
        # Direct call, uses C3 linearization order
        self.common_method()  # Calls the first matching method
        
        # Access a specific parent's method via renaming
        # Note: No 'super' keyword
    }
}
```

### Renamed Access

```valkyrie
class AdvancedChild(primary: A, secondary: B, tertiary: C) {
    micro demonstrate_access(self) {
        # Access specific parents via renaming
        self.primary.common_method()    # Calls A::common_method
        self.secondary.common_method()  # Calls B::common_method
        self.tertiary.common_method()   # Calls C::common_method
        
        # Direct access uses C3 linearization
        self.common_method()  # Calls A::common_method (first one)
    }
}
```

## Anonymous Class Inheritance

Valkyrie supports inheritance for anonymous classes:

```valkyrie
# Anonymous class inheritance
micro process_shape(shape: class(Drawable, Movable) {
    micro area(self) -> f64
}) {
    shape.draw()
    shape.move_to(10.0, 20.0)
    print("Area: {}", shape.area())
}

# Using an anonymous class
let circle = class(Drawable, Movable) {
    radius: f64,
    x: f64,
    y: f64,
    
    micro area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
}

process_shape(circle { radius: 5.0, x: 0.0, y: 0.0 })
```

### Anonymous Class Renamed Inheritance

```valkyrie
# Renamed inheritance for anonymous classes
micro create_hybrid_processor() -> class(reader: FileReader, writer: FileWriter) {
    micro process(self, filename: String) {
        let content = self.reader.read_file(filename)
        let processed = content.to_uppercase()
        self.writer.write_file(filename + ".processed", processed)
    }
}

let processor = create_hybrid_processor() {
    # Anonymous class implementation
}
```

## Constructors and Initialization

```valkyrie
class Parent1 {
    value1: i32,
    
    micro new(v1: i32) -> Self {
        Self { value1: v1 }
    }
}

class Parent2 {
    value2: String,
    
    micro new(v2: String) -> Self {
        Self { value2: v2 }
    }
}

class MultiInherit(Parent1, Parent2) {
    own_value: f64,
    
    # Constructor for multiple inheritance
    micro new(v1: i32, v2: String, own: f64) -> Self {
        Self {
            # Parent field initialization
            value1: v1,
            value2: v2,
            # Own field
            own_value: own,
        }
    }
}
```

## Abstract Classes and Interfaces

```valkyrie
# Abstract base class
abstract class Shape {
    abstract micro area(self) -> f64
    abstract micro perimeter(self) -> f64
    
    # Concrete method
    micro describe(self) {
        print("Area: {}, Perimeter: {}", self.area(), self.perimeter())
    }
}

# Interface definition
trait Drawable {
    micro draw(self)
    micro set_color(self, color: Color)
}

# Multiple inheritance: Abstract class + Interface
class Rectangle(Shape): Drawable {
    width: f64,
    height: f64,
    color: Color,
    
    # Implementing abstract methods
    micro area(self) -> f64 {
        self.width * self.height
    }
    
    micro perimeter(self) -> f64 {
        2.0 * (self.width + self.height)
    }
    
    # Implementing interface methods
    micro draw(self) {
        print("Drawing rectangle {}x{}", self.width, self.height)
    }
    
    micro set_color(self, color: Color) {
        self.color = color
    }
}
```

## Diamond Problem Resolution

```valkyrie
class GrandParent {
    micro method(self) { print("GrandParent") }
}

class Parent1(GrandParent) {
    micro method(self) { print("Parent1") }
}

class Parent2(GrandParent) {
    micro method(self) { print("Parent2") }
}

# Diamond inheritance
class Child(Parent1, Parent2) {
    # C3 linearization automatically resolves the diamond problem
    # Linearization order: Child -> Parent1 -> Parent2 -> GrandParent
    
    micro test_diamond(self) {
        self.method()  # Calls Parent1::method
    }
    
    # If you need to call a specific parent's method, use renaming
}

# Using renaming to resolve the diamond problem
class ResolvedChild(p1: Parent1, p2: Parent2) {
    micro test_resolved(self) {
        self.p1.method()  # Explicitly call Parent1::method
        self.p2.method()  # Explicitly call Parent2::method
    }
}
```

## Best Practices

### 1. Favor Composition Over Inheritance

```valkyrie
# Good design: Composition
class Document {
    reader: FileReader,
    writer: FileWriter,
    logger: Logger,
    
    micro process(self) {
        let content = self.reader.read()
        let processed = self.transform(content)
        self.writer.write(processed)
        self.logger.log("Document processed")
    }
}

# Avoid: Excessive inheritance
# class Document(FileReader, FileWriter, Logger) { ... }
```

### 2. Use Renaming to Avoid Method Conflicts

```valkyrie
# Clear renaming
class MediaPlayer(audio: AudioPlayer, video: VideoPlayer) {
    micro play_audio(self, file: String) {
        self.audio.play(file)
    }
    
    micro play_video(self, file: String) {
        self.video.play(file)
    }
}
```

### 3. Document Inheritance Relationships

```valkyrie
# Clear inheritance documentation
@doc("""
MultiProcessor inheritance relationship:
- DataProcessor: Provides data processing capabilities
- NetworkHandler: Provides network communication capabilities
- Logger: Provides logging capabilities

C3 linearization order: MultiProcessor -> DataProcessor -> NetworkHandler -> Logger
""")
class MultiProcessor(DataProcessor, NetworkHandler, Logger) {
    # Implementation
}
```

## Summary

Features of Valkyrie's multiple inheritance system:

1. **C3 Linearization**: Ensures consistency in method resolution
2. **Renaming Mechanism**: Resolves method name conflicts
3. **No super keyword**: Access parent methods explicitly via renaming
4. **Anonymous Class Support**: Supports temporary multiple inheritance class definitions
5. **Type Safety**: Compile-time checks for the validity of inheritance relationships

Proper use of multiple inheritance can achieve flexible code reuse, but it should be used with caution, prioritizing composition and interface design.
