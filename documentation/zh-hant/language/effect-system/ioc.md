# 控制反轉 (Inversion of Control)

控制反轉（IoC）是一種設計原則，通過將對象的創建和依賴關係的管理從對象本身轉移到外部容器或框架中，實現了鬆耦合的設計。在 Valkyrie 中，IoC 通過 Effect 系統實現，提供了強大的依賴注入和服務定位能力。

## 基本概念

### 依賴注入 (Dependency Injection)
依賴注入是 IoC 的一種實現方式，通過外部注入依賴對象，而不是在對象內部創建依賴。

### 服務定位 (Service Locator)
服務定位器是一個中央註冊表，用於查找和獲取服務實例。

### 生命周期管理
容器負責管理對象的生命周期，包括創建、初始化、銷毀等。

## Effect 系統中的 IoC

### 定義依賴注入 Effect

```valkyrie
# 定義依賴注入 Effect
effect DependencyInjection {
    resolve⟨T⟩(service_type: Type⟨T⟩) -> T
    resolve_named⟨T⟩(service_type: Type⟨T⟩, name: string) -> T
    register⟨T⟩(service_type: Type⟨T⟩, instance: T): Unit
    register_factory⟨T⟩(service_type: Type⟨T⟩, factory: { -> T }): Unit
    register_singleton⟨T⟩(service_type: Type⟨T⟩, factory: { -> T }): Unit
}

# 定義服務生命週期 Effect
effect ServiceLifecycle {
    create⟨T⟩(service_type: Type⟨T⟩) -> T
    initialize⟨T⟩(instance: T): T
    dispose⟨T⟩(instance: T): Unit
}
```

### 實現 IoC 容器

```valkyrie
# IoC 容器實現
class IoCContainer {
    private mut services: {Type: Any} = {}
    private mut factories: {Type: { -> Any }} = {}
    private mut singletons: {Type: Any} = {}
    private mut named_services: {string: {Type: Any}} = {}
    
    micro run_with_container(self, task: { -> Unit }) {
        try {
            task()
        }
        .catch {
            case DependencyInjection::resolve⟨T⟩(service_type):
                # 首先檢查單例
                match self.singletons.get(service_type) {
                    case instance: resume(instance as T)
                    case null: {}
                }
                
                # 檢查工廠方法
                match self.factories.get(service_type) {
                    case factory: resume(factory() as T)
                    case null: {}
                }
                
                # 檢查已註冊的實例
                match self.services.get(service_type) {
                    case instance: resume(instance as T)
                    case null: {}
                }
                
                # 嘗試自動裝配
                resume(self.auto_wire(service_type))
            
            case DependencyInjection::resolve_named⟨T⟩(service_type, name):
                match self.named_services.get(name) {
                    case named_map:
                        match named_map.get(service_type) {
                            case instance: resume(instance as T)
                            case null: {}
                        }
                    case null: {}
                }
                raise ServiceNotFoundError { service_type, name }
            
            case DependencyInjection::register⟨T⟩(service_type, instance):
                self.services[service_type] = instance
                resume()
            
            case DependencyInjection::register_factory⟨T⟩(service_type, factory):
                self.factories[service_type] = factory
                resume()
            
            case DependencyInjection::register_singleton⟨T⟩(service_type, factory):
                let instance = factory()
                self.singletons[service_type] = instance
                resume()
                
            case ServiceLifecycle::create⟨T⟩(service_type):
                # 使用反射或編譯時資訊建立實例
                let constructor = service_type.get_constructor()
                let dependencies = constructor.get_parameters().map({ 
                    raise DependencyInjection::resolve($param.type)
                })
                resume(constructor.invoke(dependencies))
            
            case ServiceLifecycle::initialize⟨T⟩(instance):
                # 執行初始化邏輯
                if instance implements Initializable {
                    instance.initialize()
                }
                resume(instance)
            
            case ServiceLifecycle::dispose⟨T⟩(instance):
                # 執行清理邏輯
                if instance implements Disposable {
                    instance.dispose()
                }
                resume()
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

### 使用依賴注入註解

```valkyrie
# 定義服務接口
trait UserRepository {
    micro find_by_id(self, id: string) -> User?
    micro save(self, user: User) -> Unit
    micro delete(self, id: string) -> Unit
}

trait EmailService {
    micro send_email(self, to: string, subject: string, body: string) -> Unit
}

# 實現服務
class DatabaseUserRepository {
    private connection: DatabaseConnection
    
    # 建構子注入
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
            # SMTP 發送邏輯
            let smtp_client = SmtpClient::new(self.config)
            smtp_client.send(to, subject, body)
        }
    }
}
```

### 服務類使用依賴注入

```valkyrie
# 用戶服務類
class UserService {
    private user_repository: UserRepository
    private email_service: EmailService
    
    # 建构子注入
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
        
        # 發送歡迎郵件
        self.email_service.send_email(
            user.email,
            "Welcome!",
            "Welcome {user.name}!"
        )
        
        user
    }
    
    micro get_user(self, id: string) -> User? {
        self.user_repository.find_by_id(id)
    }
    
    @authorized("admin")
    micro delete_user(self, id: string) -> Unit {
        if let user = self.user_repository.find_by_id(id) {
            self.user_repository.delete(id)
            
            # 發送帳戶刪除通知
            self.email_service.send_email(
                user.email,
                "Account Deleted",
                "Your account has been deleted."
            )
        }
    }
}
```

### 配置和啟動

```valkyrie
# 應用程序配置
class ApplicationConfig {
    micro configure_services(self, container: IoCContainer) {
        # 註冊配置
        container.register(EmailConfig, EmailConfig {
            smtp_host: "smtp.example.com",
            smtp_port: 587,
            username: "app↯example.com",
            password: "password"
        })
        
        # 註冊數據庫連接
        container.register_singleton(DatabaseConnection, {
            DatabaseConnection::new("postgresql://localhost/myapp")
        })
        
        # 註冊服務實現
        container.register_factory(UserRepository, {
            let connection = raise DependencyInjection::resolve(DatabaseConnection)
            DatabaseUserRepository::new(connection)
        })
        
        container.register_factory(EmailService, {
            let config = raise DependencyInjection::resolve(EmailConfig)
            SmtpEmailService::new(config)
        })
        
        # 註冊應用服務
        container.register_factory(UserService, {
            let user_repo = raise DependencyInjection::resolve(UserRepository)
            let email_service = raise DependencyInjection::resolve(EmailService)
            UserService::new(user_repo, email_service)
        })
    }
}

# 應用程序啟動
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
            
            # 使用服務
            let user = user_service.create_user("Alice", "alice↯example.com")
            print("Created user: {user.id}")
            
            let found_user = user_service.get_user(user.id)
            print("Found user: {found_user}")
        }
    }
}
```

### 作用域和生命周期

```valkyrie
# 定義服務作用域
union ServiceScope {
    Singleton,
    Scoped,
    Transient,
}

# 作用域管理器
class ScopeManager {
    private mut scoped_services: {string: {Type: Any}} = {}
    private mut current_scope: string? = null
    
    micro begin_scope(mut self, scope_id: string) {
        self.current_scope = scope_id
        self.scoped_services[scope_id] = {}
    }
    
    micro end_scope(mut self, scope_id: string) {
        if let services = self.scoped_services.remove(scope_id) {
            # 清理作用域內的服务
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

### 條件註冊

```valkyrie
# 條件註冊
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

# 使用示例
let conditional = ConditionalRegistration {}
let container = IoCContainer {}

# 根據環境註冊不同實現
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

### 裝飾器模式

```valkyrie
# 服務裝飾器
class ServiceDecorator⟨T⟩ {
    private inner: T
    
    new(inner: T) {
        self.inner = inner
    }
    
    micro get_inner(self) -> T {
        self.inner
    }
}

# 緩存裝飾器
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

# 註冊裝飾器
container.register_factory(UserRepository, {
    let base_repo = DatabaseUserRepository::new(
        raise DependencyInjection::resolve(DatabaseConnection)
    )
    CachedUserRepository::new(base_repo)
})
```

## 最佳實踐

### 1. 接口隔離
定義小而專注的接口，避免大而全的接口。

### 2. 單一職責
每個服務應該只有一個職責，避免服務過於複雜。

### 3. 生命周期管理
合理選擇服務的生命周期，避免內存洩漏和資源浪費。

### 4. 循環依賴檢測
在容器中實現循環依賴檢測，避免無限遞歸。

```valkyrie
# 完整示例：Web 應用程序
class WebApplication {
    private container: IoCContainer
    private scope_manager: ScopeManager
    
    new() {
        self.container = IoCContainer {}
        self.scope_manager = ScopeManager {}
        self.configure_services()
    }
    
    private micro configure_services(self) {
        # 基礎設施服務
        self.container.register_singleton(DatabaseConnection, {
            DatabaseConnection::new(get_connection_string())
        })
        
        # 倉儲層
        self.container.register_factory(UserRepository, {
            let conn = raise DependencyInjection::resolve(DatabaseConnection)
            let base_repo = DatabaseUserRepository::new(conn)
            CachedUserRepository::new(base_repo)
        })
        
        # 應用服務層
        self.container.register_scoped(UserService, {
            let user_repo = raise DependencyInjection::resolve(UserRepository)
            let email_service = raise DependencyInjection::resolve(EmailService)
            UserService::new(user_repo, email_service)
        })
        
        # 控制器層
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

通過 Effect 系統實現的 IoC 提供了類型安全、靈活配置、易於測試的依賴注入能力，使得應用程序的架構更加清晰和可維護。
