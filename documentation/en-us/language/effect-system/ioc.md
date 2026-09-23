# Inversion of Control (IoC)

Inversion of Control (IoC) is a design principle that achieves loose coupling by transferring the management of object creation and dependencies from the objects themselves to an external container or framework. In Valkyrie, IoC is implemented through the Effect system, providing powerful dependency injection and service location capabilities.

## Basic Concepts

### Dependency Injection (DI)
Dependency Injection is an implementation of IoC where dependency objects are injected externally rather than being created inside the object.

### Service Locator
A service locator is a central registry used to find and obtain service instances.

### Lifecycle Management
The container is responsible for managing the lifecycle of objects, including creation, initialization, and disposal.

## IoC in the Effect System

### Defining Dependency Injection Effects

```valkyrie
# Define Dependency Injection Effect
effect DependencyInjection {
    resolve⟨T⟩(service_type: Type⟨T⟩) -> T
    resolve_named⟨T⟩(service_type: Type⟨T⟩, name: string) -> T
    register⟨T⟩(service_type: Type⟨T⟩, instance: T): Unit
    register_factory⟨T⟩(service_type: Type⟨T⟩, factory: { -> T }): Unit
    register_singleton⟨T⟩(service_type: Type⟨T⟩, factory: { -> T }): Unit
}

# Define Service Lifecycle Effect
effect ServiceLifecycle {
    create⟨T⟩(service_type: Type⟨T⟩) -> T
    initialize⟨T⟩(instance: T): T
    dispose⟨T⟩(instance: T): Unit
}
```

### Implementing an IoC Container

```valkyrie
# IoC Container implementation
class IoCContainer {
    private mut services: {Type: Any} = {}
    private mut factories: {Type: { -> Any }} = {}
    private mut singletons: {Type: Any} = {}
    private mut named_services: {string: {Type: Any}} = {}
    
    handle DependencyInjection {
        resolve⟨T⟩(service_type) -> T {
            # First check singletons
            match self.singletons.get(service_type) {
                case instance: return instance as T
                case null: {}
            }
            
            # Check factories
            match self.factories.get(service_type) {
                case factory: return factory() as T
                case null: {}
            }
            
            # Check registered instances
            match self.services.get(service_type) {
                case instance: return instance as T
                case null: {}
            }
            
            # Attempt auto-wiring
            self.auto_wire(service_type)
        }
        
        resolve_named⟨T⟩(service_type, name) -> T {
            match self.named_services.get(name) {
                case named_map:
                    match named_map.get(service_type) {
                        case instance: return instance as T
                        case null: {}
                    }
                case null: {}
            }
            
            raise ServiceNotFoundError { service_type, name }
        }
        
        register⟨T⟩(service_type, instance) {
            self.services[service_type] = instance
        }
        
        register_factory⟨T⟩(service_type, factory) {
            self.factories[service_type] = factory
        }
        
        register_singleton⟨T⟩(service_type, factory) {
            let instance = factory()
            self.singletons[service_type] = instance
        }
    }
    
    handle ServiceLifecycle {
        create⟨T⟩(service_type) -> T {
            # Use reflection or compile-time info to create instances
            let constructor = service_type.get_constructor()
            let dependencies = constructor.get_parameters().map({ 
                raise DependencyInjection::resolve($param.type)
            })
            constructor.invoke(dependencies)
        }
        
        initialize⟨T⟩(instance) -> T {
            # Execute initialization logic
            if instance implements Initializable {
                instance.initialize()
            }
            instance
        }
        
        dispose⟨T⟩(instance) {
            # Execute cleanup logic
            if instance implements Disposable {
                instance.dispose()
            }
        }
    }
    
    private micro auto_wire⟨T⟩(self, service_type: Type⟨T⟩) -> T {
        let instance = raise ServiceLifecycle::create(service_type)
        let initialized = raise ServiceLifecycle::initialize(instance)
        self.services[service_type] = initialized
        initialized
    }
}
```

### Using Dependency Injection Annotations

```valkyrie
# Define service interfaces
trait UserRepository {
    micro find_by_id(self, id: string) -> User?
    micro save(self, user: User) -> unit
    micro delete(self, id: string) -> unit
}

trait EmailService {
    micro send_email(self, to: string, subject: string, body: string) -> unit
}

# Implement services
class DatabaseUserRepository {
    private connection: DatabaseConnection
    
    # Constructor injection
    new(@inject connection: DatabaseConnection) {
        self.connection = connection
    }
    
    impl UserRepository {
        micro find_by_id(self, id) -> User? {
            self.connection.query("SELECT * FROM users WHERE id = ?", [id])
                .map({ User::from_row($row) })
        }
        
        micro save(self, user) {
            self.connection.execute(
                "INSERT INTO users (id, name, email) VALUES (?, ?, ?)",
                [user.id, user.name, user.email]
            )
        }
        
        micro delete(self, id) {
            self.connection.execute("DELETE FROM users WHERE id = ?", [id])
        }
    }
}

class SmtpEmailService {
    private config: EmailConfig
    
    new(@inject config: EmailConfig) {
        self.config = config
    }
    
    impl EmailService {
        micro send_email(self, to, subject, body) {
            # SMTP sending logic
            let smtp_client = SmtpClient::new(self.config)
            smtp_client.send(to, subject, body)
        }
    }
}
```

### Service Class Using Dependency Injection

```valkyrie
# User service class
class UserService {
    private user_repository: UserRepository
    private email_service: EmailService
    
    # Constructor injection
    new(
        @inject user_repository: UserRepository,
        @inject email_service: EmailService
    ) {
        self.user_repository = user_repository
        self.email_service = email_service
    }
    
    @transactional
    micro create_user(self, name: string, email: string) -> User {
        let user = User {
            id: generate_id(),
            name,
            email,
            created_at: now()
        }
        
        self.user_repository.save(user)
        
        # Send welcome email
        self.email_service.send_email(
            user.email,
            "Welcome!",
            f"Welcome {user.name}!"
        )
        
        user
    }
    
    micro get_user(self, id: string) -> User? {
        self.user_repository.find_by_id(id)
    }
    
    @authorized("admin")
    micro delete_user(self, id: string) -> unit {
        if let user = self.user_repository.find_by_id(id) {
            self.user_repository.delete(id)
            
            # Send account deletion notification
            self.email_service.send_email(
                user.email,
                "Account Deleted",
                "Your account has been deleted."
            )
        }
    }
}
```

### Configuration and Startup

```valkyrie
# Application configuration
class ApplicationConfig {
    micro configure_services(self, container: IoCContainer) {
        # Register configuration
        container.register(EmailConfig, EmailConfig {
            smtp_host: "smtp.example.com",
            smtp_port: 587,
            username: "app@example.com",
            password: "password"
        })
        
        # Register database connection
        container.register_singleton(DatabaseConnection, {
            DatabaseConnection::new("postgresql://localhost/myapp")
        })
        
        # Register service implementations
        container.register_factory(UserRepository, {
            let connection = raise DependencyInjection::resolve(DatabaseConnection)
            DatabaseUserRepository::new(connection)
        })
        
        container.register_factory(EmailService, {
            let config = raise DependencyInjection::resolve(EmailConfig)
            SmtpEmailService::new(config)
        })
        
        # Register application services
        container.register_factory(UserService, {
            let user_repo = raise DependencyInjection::resolve(UserRepository)
            let email_service = raise DependencyInjection::resolve(EmailService)
            UserService::new(user_repo, email_service)
        })
    }
}

# Application startup
class Application {
    private container: IoCContainer
    
    new() {
        self.container = IoCContainer {}
        let config = ApplicationConfig {}
        config.configure_services(self.container)
    }
    
    micro run(self) {
        with self.container {
            let user_service = raise DependencyInjection::resolve(UserService)
            
            # Use the service
            let user = user_service.create_user("Alice", "alice@example.com")
            print("Created user: {user.id}")
            
            let found_user = user_service.get_user(user.id)
            print("Found user: {found_user}")
        }
    }
}
```

### Scopes and Lifecycles

```valkyrie
# Define service scopes
union ServiceScope {
    Singleton,
    Scoped,
    Transient,
}

# Scope manager
class ScopeManager {
    private mut scoped_services: {string: {Type: Any}} = {}
    private mut current_scope: string? = null
    
    micro begin_scope(mut self, scope_id: string) {
        self.current_scope = scope_id
        self.scoped_services[scope_id] = {}
    }
    
    micro end_scope(mut self, scope_id: string) {
        if let services = self.scoped_services.remove(scope_id) {
            # Cleanup services within the scope
            for (_, service) in services {
                raise ServiceLifecycle.dispose(service)
            }
        }
        
        if self.current_scope == scope_id {
            self.current_scope = null
        }
    }
    
    micro get_scoped_service⟨T⟩(self, service_type: Type⟨T⟩) -> T? {
        if let scope_id = self.current_scope {
            if let services = self.scoped_services.get(scope_id) {
                services.get(service_type).map { $service as T }
            } else {
                null
            }
        } else {
            null
        }
    }
    
    micro set_scoped_service⟨T⟩(mut self, service_type: Type⟨T⟩, instance: T) {
        if let scope_id = self.current_scope {
            self.scoped_services.get_mut(scope_id)[service_type] = instance
        }
    }
}
```

### Conditional Registration

```valkyrie
# Conditional registration
class ConditionalRegistration {
    micro register_if⟨T⟩(
        self,
        container: IoCContainer,
        service_type: Type⟨T⟩,
        factory: { -> T },
        condition: { -> bool }
    ) {
        if condition() {
            container.register_factory(service_type, factory)
        }
    }
    
    micro register_profile⟨T⟩(
        self,
        container: IoCContainer,
        service_type: Type⟨T⟩,
        implementations: {string: { -> T }},
        active_profile: string
    ) {
        if let factory = implementations.get(active_profile) {
            container.register_factory(service_type, factory)
        }
    }
}

# Usage example
let conditional = ConditionalRegistration {}
let container = IoCContainer {}

# Register different implementations based on environment
conditional.register_profile(
    container,
    EmailService,
    {
        "development": { MockEmailService {} },
        "production": { SmtpEmailService::new(email_config) },
        "testing": { InMemoryEmailService {} }
    },
    get_active_profile()
)
```

### Decorator Pattern

```valkyrie
# Service decorator
class ServiceDecorator⟨T⟩ {
    private inner: T
    
    new(inner: T) {
        self.inner = inner
    }
    
    micro get_inner(self) -> T {
        self.inner
    }
}

# Cache decorator
class CachedUserRepository {
    private inner: UserRepository
    private mut cache: {string: User} = {}
    
    new(@inject inner: UserRepository) {
        self.inner = inner
    }
    
    impl UserRepository {
        micro find_by_id(self, id) -> User? {
            if let cached = self.cache.get(id) {
                return cached
            }
            
            let user = self.inner.find_by_id(id)
            if let u = user {
                self.cache[id] = u
            }
            user
        }
        
        micro save(self, user) {
            self.inner.save(user)
            self.cache[user.id] = user
        }
        
        micro delete(self, id) {
            self.inner.delete(id)
            self.cache.remove(id)
        }
    }
}

# Register decorator
container.register_factory(UserRepository, {
    let base_repo = DatabaseUserRepository::new(
        raise DependencyInjection::resolve(DatabaseConnection)
    )
    CachedUserRepository::new(base_repo)
})
```

## Best Practices

### 1. Interface Segregation
Define small, focused interfaces rather than large, all-encompassing ones.

### 2. Single Responsibility
Each service should have a single responsibility to avoid over-complexity.

### 3. Lifecycle Management
Choose appropriate lifecycles for services to avoid memory leaks and resource waste.

### 4. Circular Dependency Detection
Implement circular dependency detection in the container to avoid infinite recursion.

```valkyrie
# Full example: Web Application
class WebApplication {
    private container: IoCContainer
    private scope_manager: ScopeManager
    
    new() {
        self.container = IoCContainer {}
        self.scope_manager = ScopeManager {}
        self.configure_services()
    }
    
    private micro configure_services(self) {
        # Infrastructure services
        self.container.register_singleton(DatabaseConnection, {
            DatabaseConnection::new(get_connection_string())
        })
        
        # Repository layer
        self.container.register_factory(UserRepository, {
            let conn = raise DependencyInjection::resolve(DatabaseConnection)
            let base_repo = DatabaseUserRepository::new(conn)
            CachedUserRepository::new(base_repo)
        })
        
        # Application service layer
        self.container.register_scoped(UserService, {
            let user_repo = raise DependencyInjection::resolve(UserRepository)
            let email_service = raise DependencyInjection::resolve(EmailService)
            UserService::new(user_repo, email_service)
        })
        
        # Controller layer
        self.container.register_scoped(UserController, {
            let user_service = raise DependencyInjection::resolve(UserService)
            UserController::new(user_service)
        })
    }
    
    micro handle_request(self, request: HttpRequest) -> HttpResponse {
        let request_id = generate_request_id()
        
        self.scope_manager.begin_scope(request_id)
        
        try {
            with self.container, self.scope_manager {
                let controller = raise DependencyInjection::resolve(UserController)
                controller.handle(request)
            }
        } finally {
            self.scope_manager.end_scope(request_id)
        }
    }
}
```

IoC implemented through the Effect system provides type-safe, flexibly configured, and easily testable dependency injection, resulting in a cleaner and more maintainable application architecture.
