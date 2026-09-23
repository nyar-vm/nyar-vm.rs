# Module System

Valkyrie uses a module system based on namespaces and imports (`using`), providing flexible code organization and dependency management.

## Namespace Declaration (namespace)

Valkyrie uses the `namespace!` keyword at the top of a file to declare a namespace.

```valkyrie
namespace! math.geometry;

class Point {
    x: f64
    y: f64
}

imply Point {
    micro distance(self, other: Point) -> f64 {
        let dx = self.x - other.x
        let dy = self.y - other.y
        (dx * dx + dy * dy).sqrt()
    }
}
```

## Import System (using)

Use the `using` keyword to import members from other namespaces.

```valkyrie
using math.geometry;
using math.geometry.Point;
```

## Visibility Control

### Access Modifiers

```valkyrie
namespace database {
    # Public class
    pub class Connection {
        # Private fields
        host: string,
        port: u16,
        # Public field
        pub timeout: Duration,
    }
    
    # Crate-visible (package-visible)
    class InternalConfig {
        secret_key: string,
    }
    
    # Private function
    micro validate_connection(conn: &Connection) -> bool {
        # Internal validation logic
        true
    }
    
    # Public function
    pub micro connect(host: string, port: u16) -> Result⟨Connection, Error⟩ {
        let conn = Connection(host, port, timeout: Duration::seconds(30))
        if validate_connection(&conn) {
            Fine(conn)
        } else {
            Fail(Error::InvalidConnection)
        }
    }
}
```

### Re-exporting

```valkyrie
namespace api {
    # Re-export types from other modules
    pub using database.{Connection, Error}
    pub using auth.{User, Session}
    
    # Provide a unified API interface
    pub micro create_authenticated_connection(credentials: Credentials) -> Result⟨(Connection, Session), Error⟩ {
        let session = auth::login(credentials)?
        let connection = database::connect("localhost", 5432)?
        Fine { value: (connection, session) }
    }
}
```

## File Path Independent Modules

### Logical Module Organization

Valkyrie's module system does not depend on file paths but is based on logical namespaces:

```valkyrie
# File: src/geometry.val
namespace math.geometry {
    pub class Point { x: f64, y: f64 }
}

# File: src/algebra.val  
namespace math.algebra {
    pub class Matrix(data: [[f64]])
}

# File: src/utils.val
namespace math.geometry {  # Extend an existing namespace
    pub micro origin() -> Point {
        Point(0.0, 0.0)
    }
}
```

### Module Declaration Files

```valkyrie
# File: math.module.val
# Declare the public interface of a module
module math {
    pub namespace geometry {
        pub class Point
        pub micro distance(Point, Point) -> f64
        pub micro origin() -> Point
    }
    
    pub namespace algebra {
        pub class Matrix
        pub micro multiply(Matrix, Matrix) -> Matrix
    }
}
```

## Conditional Compilation and Features

### Feature Gating

```valkyrie
namespace network {
    # Basic network functionality
    pub class TcpStream { /* ... */ }
    
    # Async functionality (requires async feature)
    @cfg(feature = "async")
    pub namespace async {
        pub class AsyncTcpStream { /* ... */ }
        
        pub micro connect_async(addr: SocketAddr) -> Future⟨Result⟨AsyncTcpStream, Error⟩⟩ {
            # Async connection implementation
        }
    }
    
    # TLS support (requires tls feature)
    @cfg(feature = "tls")
    pub namespace tls {
        pub class TlsStream { /* ... */ }
        
        pub micro wrap_tls(stream: TcpStream, config: TlsConfig) -> Result⟨TlsStream, TlsError⟩ {
            # TLS wrapper implementation
        }
    }
}
```

### Platform-Specific Code

```valkyrie
namespace platform {
    # Common interface
    pub trait FileSystem {
        micro read_file(path: string) -> Result⟨string, IoError⟩
        micro write_file(path: string, content: string) -> Result⟨(), IoError⟩
    }
    
    # Windows implementation
    @cfg(target_os = "windows")
    pub namespace windows {
        pub class WindowsFileSystem
        
        imply WindowsFileSystem: FileSystem {
            micro read_file(&self, path: string) -> string {
                # Windows-specific implementation
                "Windows content"
            }
        }
    }
    
    # Unix implementation
    @cfg(any(target_os = "linux", target_os = "macos"))
    pub namespace unix {
        pub class UnixFileSystem
        
        imply UnixFileSystem: FileSystem {
            micro read_file(&self, path: string) -> string {
                # Unix-specific implementation
                "Unix content"
            }
        }
    }
}

## Dependency Management

### External Dependencies

```json5
// Project configuration: legion.json
{
    "dependencies": {
        "serde": "^1.0",
        "tokio": { "version": "^1.0", "features": ["full"] },
        "log": "^0.4"
    },
    "dev-dependencies": {
        "tokio-test": "^0.4"
    }
}
```

Use external dependencies in code:

```valkyrie
using serde.{Serialize, Deserialize}
using tokio.runtime.Runtime
using log.{info, warn, error}

@derive(Serialize, Deserialize)
class Config {
    host: string,
    port: u16,
}

micro main() {
    let rt = Runtime::new().unwrap()
    rt.block_on(async_main())
}

async micro async_main() {
    info("Starting application")
    # Async logic
}
```

### Workspaces

```json5
// Workspace configuration: legions.json
{
    "members": [
        "core",
        "api",
        "cli",
        "web"
    ],
    "dependencies": {
        "serde": "1.0",
        "tokio": "1.0"
    }
}
```

Reference workspace dependencies in subprojects:

```json5
// core/legion.json
{
    "dependencies": {
        "serde": true,
        "tokio": true,
        "api": { "path": "../api" }
    }
}
```

## Module Initialization

### Static Initialization

```valkyrie
namespace config {
    # Static configuration
    pub static DATABASE_URL: string = ↯env("DATABASE_URL")
    pub static MAX_CONNECTIONS: i32 = 100
    
    # Lazy initialization
    pub static LOGGER: Lazy⟨Logger⟩ = Lazy::new({
        Logger::new()
            .with_level(LogLevel::Info)
            .with_output(Output::Stdout)
    })
}
```

### Dynamic Initialization

```valkyrie
namespace database {
    static mut CONNECTION_POOL: ConnectionPool? = None
    
    pub micro initialize(config: DatabaseConfig) -> Result⟨(), Error⟩ {
        unsafe {
            if CONNECTION_POOL != None {
                return Fail(Error::AlreadyInitialized)
            }
            
            let pool = ConnectionPool::new(config)?
            CONNECTION_POOL = pool
            Fine { value: () }
        }
    }
    
    pub micro get_connection() -> Result⟨Connection, Error⟩ {
        unsafe {
            CONNECTION_POOL?
                .get_connection() !! Error::NotInitialized
        }
    }
}
```

## Test Modules

### Unit Tests

```valkyrie
namespace math.geometry {
    pub micro distance(p1: Point, p2: Point) -> f64 {
        let dx = p1.x - p2.x
        let dy = p1.y - p2.y
        (dx * dx + dy * dy).sqrt()
    }
    
    # Test module
    @cfg(test)
    namespace tests {
        using super.*
        
        @test
        micro test_distance_same_point() {
            let p = Point(1.0, 2.0)
            let dist = distance(p, p)
            ↯assert_equal(dist, 0.0)
        }
        
        @test
        micro test_distance_different_points() {
            let p1 = Point(0.0, 0.0)
            let p2 = Point(3.0, 4.0)
            let dist = distance(p1, p2)
            ↯assert_equal(dist, 5.0)
        }
    }
}
```

### Integration Tests

```valkyrie
# File: tests/integration.val
using myapp.api.*
using myapp.database.*

@test
micro test_full_workflow() {
    # Setup test environment
    let config = TestConfig::default()
    initialize_test_database(config)
    
    # Execute test
    let user = create_user("alice", "alice↯example.com")
    @assert_true(user.is_ok())
    
    let found_user = find_user_by_email("alice↯example.com")
    @assert_true(found_user.is_some())
    
    # Cleanup
    cleanup_test_database()
}
```

## Best Practices

### Module Design Principles

1. **Single Responsibility**: Each module should have a clear responsibility.
2. **Low Coupling**: Dependencies between modules should be minimized.
3. **High Cohesion**: Related functionalities should be organized in the same module.
4. **Stable Interface**: Public interfaces should remain stable.

### Naming Conventions

```valkyrie
# Good namespace organization
namespace myapp {
    namespace core {        # Core functionality
        namespace types     # Base types
        namespace traits    # Trait definitions
        namespace utils     # Utility functions
    }
    
    namespace services {    # Business services
        namespace user      # User service
        namespace auth      # Auth service
        namespace payment   # Payment service
    }
    
    namespace adapters {    # Adapter layer
        namespace database  # Database adapter
        namespace http      # HTTP adapter
        namespace cache     # Cache adapter
    }
}
```

### Version Compatibility

```valkyrie
namespace api {
    # Versioned API
    namespace v1 {
        pub class User {
            id: i64,
            name: String,
        }
        
        pub micro get_user(id: i64) -> Option<User> {
            # v1 implementation
        }
    }
    
    namespace v2 {
        pub class User {
            id: i64,
            name: String,
            email: String,  # Added field
        }
        
        pub micro get_user(id: i64) -> Option<User> {
            # v2 implementation
        }
        
        # Backward compatibility
        pub micro get_user_v1(id: i64) -> Option<v1::User> {
            get_user(id).map({ v1::User {
                id: u.id,
                name: u.name,
            })
        }
    }
    
    # Current version alias
    pub using v2.*
}
```
```
