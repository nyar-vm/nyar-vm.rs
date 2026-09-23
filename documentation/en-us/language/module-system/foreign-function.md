# Foreign Function Interface (FFI)

Valkyrie provides a powerful FFI system that supports interoperation with various languages such as C, C++, Rust, Python, and JavaScript, enabling efficient cross-language calls and data exchange.

## C/C++ Interoperability

### Basic C Function Calls

```valkyrie
# Declare external C functions
@import(c, "libc", "malloc")
micro malloc(size: usize) -> *mut u8

@import(c, "libc", "free")
micro free(ptr: *mut u8)

@import(c, "libc", "strlen")
micro strlen(s: *const i8) -> usize

@import(c, "libc", "printf")
micro printf(format: *const i8, ...) -> i32

@import(c, "libc", "sin")
micro sin(x: f64) -> f64

@import(c, "libc", "cos")
micro cos(x: f64) -> f64

# Use C functions
micro use_c_functions() {
    unsafe {
        let ptr = malloc(1024)
        if !ptr.is_null() {
            # Use memory
            free(ptr)
        }
        
        let result = sin(3.14159 / 2.0)
        print("sin(π/2) = {}", result)
    }
}
```

### C Struct Interoperability

```valkyrie
# C-compatible class
@repr(C)
class Point {
    x: f64
    y: f64
}

@repr(C)
class Rectangle {
    top_left: Point
    bottom_right: Point
}

# Declare C functions using classes
@import(c, "geometry_lib", "calculate_distance")
micro calculate_distance(p1: *const Point, p2: *const Point) -> f64

@import(c, "geometry_lib", "rectangle_area")
micro rectangle_area(rect: *const Rectangle) -> f64

@import(c, "geometry_lib", "create_point")
micro create_point(x: f64, y: f64) -> Point

# Interact with C using classes
micro geometry_calculations() {
    let p1 = Point(0.0, 0.0)
    let p2 = Point(3.0, 4.0)
    
    unsafe {
        let distance = calculate_distance(&p1, &p2)
        print("Distance: {}", distance)
        
        let rect = Rectangle(
            Point(0.0, 10.0),
            Point(5.0, 0.0)
        )
        let area = rectangle_area(&rect)
        print("Area: {}", area)
    }
}
```

### C++ Class Interoperability

```valkyrie
# C wrapper declarations for C++ classes
# C interface for Vector3D class
@import(c, "vector_lib", "vector3d_new")
micro vector3d_new(x: f64, y: f64, z: f64) -> *mut void

@import(c, "vector_lib", "vector3d_delete")
micro vector3d_delete(ptr: *mut void)

@import(c, "vector_lib", "vector3d_magnitude")
micro vector3d_magnitude(ptr: *const void) -> f64

@import(c, "vector_lib", "vector3d_normalize")
micro vector3d_normalize(ptr: *mut void)

@import(c, "vector_lib", "vector3d_dot")
micro vector3d_dot(ptr1: *const void, ptr2: *const void) -> f64

@import(c, "vector_lib", "vector3d_cross")
micro vector3d_cross(ptr1: *const void, ptr2: *const void) -> *mut void

# Valkyrie wrapper
class Vector3D {
    ptr: *mut void
}

imply Vector3D {
    micro constructor(mut self, x: f64, y: f64, z: f64) {
        unsafe {
            self.ptr = vector3d_new(x, y, z)
        }
    }
    
    micro magnitude(self) -> f64 {
        unsafe { vector3d_magnitude(self.ptr) }
    }
    
    micro normalize(mut self) {
        unsafe { vector3d_normalize(self.ptr) }
    }
    
    micro dot(self, other: Vector3D) -> f64 {
        unsafe { vector3d_dot(self.ptr, other.ptr) }
    }
    
    micro cross(self, other: Vector3D) -> Vector3D {
        unsafe {
            Vector3D {
                ptr: vector3d_cross(self.ptr, other.ptr)
            }
        }
    }
}

imply Vector3D: Drop {
    micro drop(mut self) {
        unsafe {
            vector3d_delete(self.ptr)
        }
    }
}
```

## Rust Interoperability

### Calling Rust Libraries

```valkyrie
# Link Rust static library
@link(name: "myrust_lib", kind: "static")
@import(rust, "myrust_lib", "rust_fibonacci")
micro rust_fibonacci(n: u32) -> u64

@import(rust, "myrust_lib", "rust_sort_array")
micro rust_sort_array(arr: *mut i32, len: usize)

@import(rust, "myrust_lib", "rust_json_parse")
micro rust_json_parse(json_str: *const i8) -> *mut void

@import(rust, "myrust_lib", "rust_json_free")
micro rust_json_free(ptr: *mut void)

# Use Rust functions
micro use_rust_library() {
    unsafe {
        let fib_10 = rust_fibonacci(10)
        print("Fibonacci(10) = {}", fib_10)
        
        let mut numbers = [5, 2, 8, 1, 9, 3]
        rust_sort_array(numbers.as_mut_ptr(), numbers.len())
        print("Sorted: {}", numbers)
    }
}
```

### Exporting Functions to Other Languages

```valkyrie
# Export Valkyrie function to C/C++
@export(c, "valkyrie_add")
micro valkyrie_add(a: i32, b: i32) -> i32 {
    a + b
}

@export(c, "valkyrie_process_array")
micro valkyrie_process_array(arr: *mut f64, len: usize) {
    if arr.is_null() { return }
    
    unsafe {
        let slice = std::slice::from_raw_parts_mut(arr, len)
        for item in slice {
            *item = item.sqrt()  # Calculate square root
        }
    }
}

@export(c, "valkyrie_create_string")
micro valkyrie_create_string(s: *const i8) -> *mut i8 {
    if s.is_null() { return std::ptr::null_mut() }
    
    unsafe {
        let c_str = CStr::from_ptr(s)
        let rust_str = c_str.to_str().default("")
        let processed = "Processed: {}".format(rust_str)
        
        let c_string = CString::new(processed).expect("Invalid string")
        c_string.into_raw()
    }
}

@export(c, "valkyrie_free_string")
micro valkyrie_free_string(s: *mut i8) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s)
        }
    }
}
```

## Python Interoperability

Valkyrie provides native interop support with Python, allowing you to import Python modules and call their functions directly.

```valkyrie
using std::python

class PythonInterpreter {
    handle: python.Handle
}

imply PythonInterpreter {
    micro new() -> Self {
        Self { handle: python.init() }
    }
    
    micro run_script(self, script: string) {
        self.handle.execute(script)
    }
}

imply PythonInterpreter: Drop {
    micro drop(mut self) {
        python.finalize(self.handle)
    }
}

micro main() {
    let py = PythonInterpreter::new()
    
    # Import Python module
    let math = python.import("math")
    let result = math.sin(3.14159 / 2.0)
    print("Python sin(π/2) = {}", result)
    
    # Execute complex Python code
    py.run_script(r"
import matplotlib.pyplot as plt
import numpy as np

x = np.linspace(0, 10, 100)
y = np.sin(x)
plt.plot(x, y)
plt.show()
    ")
}
```

### Python Extension Modules

```valkyrie
# Create Python extension module
using pyo3::prelude

@pyfunction
micro fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2)
    }
}

@pyfunction
micro matrix_multiply(a: [[f64]], b: [[f64]]) -> Result<[[f64]], PyError> {
    let rows_a = a.len()
    let cols_a = a[0].len()
    let cols_b = b[0].len()
    
    if cols_a != b.len() {
        return Fail { error: PyError::ValueError("Matrix dimensions don't match") }
    }
    
    let mut result = [[0.0; cols_b]; rows_a]
    
    for i in 0..rows_a {
        for j in 0..cols_b {
            for k in 0..cols_a {
                result[i][j] += a[i][k] * b[k][j]
            }
        }
    }
    
    Ok(result)
}

@pyclass
class Calculator {
    @pyo3(get, set)
    value: f64,
}

@pymethods
imply Calculator {
    @new
    micro new(initial_value: f64) -> Self {
        Calculator { value: initial_value }
    }
    
    micro add(mut self, other: f64) -> f64 {
        self.value += other
        self.value
    }
    
    micro multiply(mut self, other: f64) -> f64 {
        self.value *= other
        self.value
    }
    
    micro reset(mut self) {
        self.value = 0.0
    }
}

@pymodule
micro valkyrie_math(_py: Python, m: PyModule) -> Result<(), PyError> {
    m.add_function(wrap_pyfunction!(fibonacci, m)?)?;
    m.add_function(wrap_pyfunction!(matrix_multiply, m)?)?;
    m.add_class::<Calculator>()?;
    Ok(())
}
```

## JavaScript Interoperability

### WebAssembly Export

```valkyrie
# Compile to WebAssembly
using wasm_bindgen.prelude

@wasm_bindgen
# Import JavaScript functions
@import(js, "console", "log")
micro log(s: string)

@import(js, "Math", "random")
micro random() -> f64

@import(js, "window", "alert")
micro alert(s: string)

# Simplify logging with a macro
macro console_log(args) {
    log("{}".format(args))
}

@wasm_bindgen
class GameEngine {
    width: u32
    height: u32
    entities: [Entity]
}

@wasm_bindgen
imply GameEngine {
    @wasm_bindgen(constructor)
    micro new(width: u32, height: u32) -> Self {
        console_log("Creating game engine {}x{}".format(width, height))
        GameEngine {
            width,
            height,
            entities: [],
        }
    }
    
    @wasm_bindgen
    micro add_entity(mut self, x: f64, y: f64) -> usize {
        let entity = Entity { x, y, vx: 0.0, vy: 0.0 }
        self.entities.push(entity)
        self.entities.len() - 1
    }
    
    @wasm_bindgen
    micro update(mut self, dt: f64) {
        for entity in mut self.entities {
            entity.x += entity.vx * dt
            entity.y += entity.vy * dt
            
            # Boundary checks
            if entity.x < 0.0 || entity.x > self.width as f64 {
                entity.vx = -entity.vx
            }
            if entity.y < 0.0 || entity.y > self.height as f64 {
                entity.vy = -entity.vy
            }
        }
    }
    
    @wasm_bindgen
    micro get_entity_positions(self) -> [f64] {
        let mut positions = []
        for entity in self.entities {
            positions.push(entity.x)
            positions.push(entity.y)
        }
        positions
    }
}

class Entity {
    x: f64
    y: f64
    vx: f64
    vy: f64
}

# Export math functions
@wasm_bindgen
micro fast_inverse_sqrt(x: f32) -> f32 {
    # Quake III Fast Inverse Square Root algorithm
    let i = x.to_bits()
    let i = 0x5f3759df - (i >> 1)
    let y = f32::from_bits(i)
    y * (1.5 - 0.5 * x * y * y)
}

@wasm_bindgen
micro mandelbrot(cx: f64, cy: f64, max_iter: u32) -> u32 {
    let mut x = 0.0
    let mut y = 0.0
    let mut iter = 0
    
    while x * x + y * y <= 4.0 && iter < max_iter {
        let temp = x * x - y * y + cx
        y = 2.0 * x * y + cy
        x = temp
        iter += 1
    }
    
    iter
}
```

### Node.js Native Modules

```valkyrie
# Node.js N-API bindings
using napi.bindgen_prelude

@napi
micro fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2)
    }
}

@napi
micro process_image(data: Buffer, width: u32, height: u32) -> Result<Buffer, Error> {
    let mut pixels = data.to_vector()
    
    # Simple image processing: invert colors
    for i in (0..pixels.len()).step_by(4) {
        pixels[i] = 255 - pixels[i]      # R
        pixels[i + 1] = 255 - pixels[i + 1]  # G
        pixels[i + 2] = 255 - pixels[i + 2]  # B
        # Alpha channel remains unchanged
    }
    
    Ok(Buffer::from(pixels))
}

@napi
class FileProcessor {
    buffer_size: u32
}

@napi
imply FileProcessor {
    @napi(constructor)
    micro new(buffer_size: u32) -> Self {
        FileProcessor { buffer_size }
    }
    
    @napi
    micro process_file(self, path: string) -> Result<string, Error> {
        # File processing logic
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::new(Status::GenericFailure, "Failed to read file: {}".format(e)))?
        
        # Simple processing: count lines and characters
        let lines = content.lines().count()
        let chars = content.chars().count()
        
        Ok("File: {}, Lines: {}, Characters: {}".format(path, lines, chars))
    }
    
    @napi
    async micro process_file_async(self, path: string) -> Result<string, Error> {
        # Async file processing
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| Error::new(Status::GenericFailure, "Failed to read file: {}".format(e)))?
        
        # Simulate time-consuming processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await
        
        let processed = content.to_uppercase()
        Ok(processed)
    }
}
```

## Dynamic Library Loading

### Runtime Dynamic Loading

```valkyrie
using libloading.{Library, Symbol}

# Dynamic Library Manager
class DynamicLibrary {
    lib: Library
}

imply DynamicLibrary {
    micro load(path: string) -> Result<Self, Error> {
        unsafe {
            let lib = Library::new(path)?
            Ok(DynamicLibrary { lib })
        }
    }
    
    micro get_function<T>(self, name: [u8]) -> Result<Symbol<T>, Error> {
        unsafe {
            let symbol = self.lib.get(name)?
            Ok(symbol)
        }
    }
}

# Use dynamic library
micro use_dynamic_library() -> Result<(), Error> {
    let lib = DynamicLibrary::load("./libmath.so")?
    
    # Get function pointers
    let add_func: Symbol<unsafe micro(i32, i32) -> i32> = lib.get_function(b"add")?
    let multiply_func: Symbol<unsafe micro(f64, f64) -> f64> = lib.get_function(b"multiply")?
    
    # Call dynamically loaded functions
    unsafe {
        let sum = add_func(5, 3)
        let product = multiply_func(2.5, 4.0)
        
        print("5 + 3 = {}", sum)
        print("2.5 * 4.0 = {}", product)
    }
    
    Ok(())
}
```

### Plugin System

```valkyrie
# Plugin interface definition
trait Plugin {
    micro name(self) -> string
    micro version(self) -> string
    micro initialize(mut self) -> Result<(), string>
    micro execute(self, input: string) -> Result<string, string>
    micro cleanup(mut self)
}

# Plugin Manager
class PluginManager {
    plugins: HashMap<string, Plugin>
    libraries: [Library]
}

imply PluginManager {
    micro new() -> Self {
        PluginManager {
            plugins: HashMap::new(),
            libraries: [],
        }
    }
    
    micro load_plugin(mut self, path: string) -> Result<(), Error> {
        unsafe {
            let lib = Library::new(path)?
            
            # Get plugin creation function
            let create_plugin: Symbol<unsafe micro() -> *mut Plugin> = 
                lib.get(b"create_plugin")?
            
            let plugin_ptr = create_plugin()
            let plugin = Box::from_raw(plugin_ptr)
            
            let name = plugin.name().to_string()
            self.plugins.insert(name, plugin)
            self.libraries.push(lib)
            
            Ok(())
        }
    }
    
    micro execute_plugin(self, name: string, input: string) -> Result<string, string> {
        match self.plugins.get(name) {
            Some(plugin) => plugin.execute(input),
            None => Fail("Plugin '{}' not found".format(name))
        }
    }
    
    micro list_plugins(self) -> [(string, string)] {
        self.plugins.iter()
            .map(|(name, plugin)| (name.clone(), plugin.version().to_string()))
            .collect()
    }
}

# Plugin export macro
macro export_plugin(plugin_type) {
    @export(c, "create_plugin")
    micro create_plugin() -> *mut Plugin {
        let plugin = Box::new(plugin_type::new())
        Box::into_raw(plugin)
    }
    
    @export(c, "destroy_plugin")
    micro destroy_plugin(plugin: *mut Plugin) {
        if !plugin.is_null() {
            unsafe {
                let _ = Box::from_raw(plugin)
            }
        }
    }
}
```

## Memory Management and Safety

### Safe FFI Wrappers

```valkyrie
# Safe C string handling
class SafeCString {
    ptr: *mut i8
}

imply SafeCString {
    micro new(s: string) -> Result<Self, Error> {
        let c_string = CString::new(s)?
        Ok(SafeCString {
            ptr: c_string.into_raw()
        })
    }
    
    micro as_ptr(self) -> *const i8 {
        self.ptr
    }
    
    micro to_string(self) -> Result<string, Error> {
        unsafe {
            let c_str = CStr::from_ptr(self.ptr)
            Ok(c_str.to_str()?.to_string())
        }
    }
}

imply SafeCString: Drop {
    micro drop(mut self) {
        if !self.ptr.is_null() {
            unsafe {
                let _ = CString::from_raw(self.ptr)
            }
        }
    }
}

# Safe memory management
class ManagedBuffer {
    ptr: *mut u8
    size: usize
    capacity: usize
}

imply ManagedBuffer {
    micro new(capacity: usize) -> Self {
        unsafe {
            let ptr = malloc(capacity)
            if ptr.is_null() {
                panic("Memory allocation failed")
            }
            ManagedBuffer {
                ptr,
                size: 0,
                capacity,
            }
        }
    }
    
    micro as_slice(self) -> [u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr, self.size)
        }
    }
    
    micro as_mut_slice(mut self) -> mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr, self.size)
        }
    }
    
    micro resize(mut self, new_size: usize) -> Result<(), string> {
        if new_size > self.capacity {
            return Fail("Size exceeds capacity")
        }
        self.size = new_size
        Ok(())
    }
}

imply ManagedBuffer: Drop {
    micro drop(mut self) {
        if !self.ptr.is_null() {
            unsafe {
                free(self.ptr)
            }
        }
    }
}
```

### Error Handling

```valkyrie
# FFI Error types
@derive(Debug)
enums FFIError {
    NullPointer
    InvalidUtf8(Error)
    LibraryLoadError(string)
    SymbolNotFound(string)
    FunctionCallFailed(i32)
    MemoryAllocationFailed
}

imply FFIError: Display {
    micro fmt(self, f: Formatter) -> Result {
        match self {
            FFIError::NullPointer => f.write("Null pointer encountered"),
            FFIError::InvalidUtf8(e) => f.write("Invalid UTF-8: {}".format(e)),
            FFIError::LibraryLoadError(msg) => f.write("Library load error: {}".format(msg)),
            FFIError::SymbolNotFound(name) => f.write("Symbol not found: {}".format(name)),
            FFIError::FunctionCallFailed(code) => f.write("Function call failed with code: {}".format(code)),
            FFIError::MemoryAllocationFailed => f.write("Memory allocation failed"),
        }
    }
}

imply FFIError: Error {}

# Safe FFI call wrapper
micro safe_ffi_call<F, R>(f: F) -> Result<R, FFIError>
where
    F: micro() -> R,
{
    # Setup error handling
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|_| FFIError::FunctionCallFailed(-1))
}
```

## Best Practices

### 1. Type Safety

```valkyrie
# Use newtype pattern to enhance type safety
class FileHandle(*mut void)
class DatabaseConnection(*mut void)

# Avoid confusing different types of pointers
imply FileHandle {
    micro new(path: string) -> Result<Self, FFIError> {
        let c_path = CString::new(path).map_err(|_| FFIError::InvalidUtf8)?
        unsafe {
            let handle = fopen(c_path.as_ptr(), b"r\0".as_ptr() as *const i8)
            if handle.is_null() {
                Fail(FFIError::NullPointer)
            } else {
                Ok(FileHandle(handle))
            }
        }
    }
}
```

### 2. Resource Management

```valkyrie
# RAII pattern ensures resource release
class ResourceGuard<T> {
    resource: T?
    cleanup: micro(T)
}

imply ResourceGuard<T> {
    micro new(resource: T, cleanup: micro(T)) -> Self {
        ResourceGuard {
            resource: Some(resource),
            cleanup,
        }
    }
    
    micro take(mut self) -> T? {
        self.resource.take()
    }
}

imply ResourceGuard<T>: Drop {
    micro drop(mut self) {
        if let item = self.resource.take()? {
            (self.cleanup)(item)
        }
    }
}
```

### 3. Version Compatibility

```valkyrie
# Version checking
class LibraryVersion {
    major: u32
    minor: u32
    patch: u32
}

micro check_library_compatibility(required: LibraryVersion, actual: LibraryVersion) -> bool {
    # Major versions must match
    if required.major != actual.major {
        return false
    }
    
    # Minor versions are backward compatible
    if actual.minor < required.minor {
        return false
    }
    
    # Patch versions do not affect compatibility
    true
}
```

Valkyrie's FFI system provides safe and efficient cross-language interop capabilities, supporting integration with mainstream programming languages and runtime environments, providing a powerful infrastructure for building complex multi-language applications.
