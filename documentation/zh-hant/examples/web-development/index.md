# 网页开发

Valkyrie 提供了现代化的网页开发框架，支持服务端渲染、客户端应用、WebAssembly 集成、实时通信等功能，为构建高性能 Web 应用提供完整的解决方案。

## Web 服务器框架

### HTTP 服务器

```valkyrie
# HTTP 服务器核心
class WebServer {
    router: Router,
    middleware_stack: Vector<Box<dyn Middleware>>,
    config: ServerConfig,
    thread_pool: ThreadPool,
}

class ServerConfig {
    host: string,
    port: u16,
    max_connections: usize,
    request_timeout: Duration,
    static_files_dir: string?,
    enable_compression: bool,
}

imply WebServer {
    micro new(config: ServerConfig) -> Self {
        WebServer {
            router: Router::new(),
            middleware_stack: [],
            config,
            thread_pool: ThreadPool::new(num_cpus::get()),
        }
    }
    
    micro route(&mut self, method: HttpMethod, path: string, handler: imply Handler) {
        self.router.add_route(method, path, Box::new(handler))
    }
    
    micro get(&mut self, path: string, handler: imply Handler) {
        self.route(HttpMethod::GET, path, handler)
    }
    
    micro post(&mut self, path: string, handler: imply Handler) {
        self.route(HttpMethod::POST, path, handler)
    }
    
    micro put(&mut self, path: string, handler: imply Handler) {
        self.route(HttpMethod::PUT, path, handler)
    }
    
    micro delete(&mut self, path: string, handler: imply Handler) {
        self.route(HttpMethod::DELETE, path, handler)
    }
    
    micro use_middleware(&mut self, middleware: imply Middleware) {
        self.middleware_stack.push(Box::new(middleware))
    }
    
    micro serve_static(&mut self, path: string, dir: string) {
        let static_handler = StaticFileHandler::new(dir)
        self.get(&format!("{}/*", path), static_handler)
    }
    
    async micro listen(&self) -> Result<(), ServerError> {
        let addr = format!("{}:{}", self.config.host, self.config.port)
        let listener = TcpListener::bind(&addr).await?
        
        print("Server listening on http://{}", addr)
        
        loop {
            let (stream, _) = listener.accept().await?
            let router = self.router.clone()
            let middleware = self.middleware_stack.clone()
            
            self.thread_pool.spawn(async move {
                if let Err(e) = handle_connection(stream, router, middleware).await {
                    print("Error handling connection: {}", e)
                }
            })
        }
    }
}

# 请求处理
async micro handle_connection(
    mut stream: TcpStream,
    router: Router,
    middleware: Vector<Box<dyn Middleware>>
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0; 1024]
    stream.read(&mut buffer).await?
    
    let request = HttpRequest::parse(&buffer)?
    let mut context = RequestContext::new(request)
    
    # 执行中间件链
    for middleware in &middleware {
        middleware.process(&mut context).await?
        if context.response.is_some() {
            break
        }
    }
    
    # 如果中间件没有生成响应，则路由到处理器
    if context.response.is_none() {
        if let Some(handler) = router.find_handler(&context.request) {
            let response = handler.handle(&context.request).await?
            context.response = Some(response)
        } else {
            context.response = Some(HttpResponse::not_found())
        }
    }
    
    # 发送响应
    if let Some(response) = context.response {
        let response_bytes = response.to_bytes()
        stream.write_all(&response_bytes).await?
    }
    
    Ok(())
}
```

### 路由系统

```valkyrie
# 路由器
class Router {
    routes: Vector<Route>,
    groups: Vector<RouteGroup>,
}

class Route {
    method: HttpMethod,
    pattern: PathPattern,
    handler: Box<dyn Handler>,
    middleware: Vector<Box<dyn Middleware>>,
}

class RouteGroup {
    prefix: string,
    routes: Vector<Route>,
    middleware: Vector<Box<dyn Middleware>>,
}

enums PathPattern {
    Static(string),
    Dynamic(string, Vector<string>),  # 模式和参数名
    Wildcard(string),
}

imply Router {
    micro new() -> Self {
        Router {
            routes: [],
            groups: [],
        }
    }
    
    micro add_route(&mut self, method: HttpMethod, path: string, handler: Box<dyn Handler>) {
        let pattern = PathPattern::parse(path)
        let route = Route {
            method,
            pattern,
            handler,
            middleware: [],
        }
        self.routes.push(route)
    }
    
    micro group(&mut self, prefix: string) -> RouteGroupBuilder {
        RouteGroupBuilder::new(prefix, self)
    }
    
    micro find_handler(&self, request: &HttpRequest) -> (imply Handler)? {
        # 首先检查路由组
        for group in &self.groups {
            if request.path.starts_with(&group.prefix) {
                let sub_path = &request.path[group.prefix.len()..]
                for route in &group.routes {
                    if route.method == request.method && route.pattern.matches(sub_path) {
                        return Some(route.handler.as_ref())
                    }
                }
            }
        }
        
        # 然后检查全局路由
        for route in &self.routes {
            if route.method == request.method && route.pattern.matches(&request.path) {
                return Some(route.handler.as_ref())
            }
        }
        
        None
    }
}

imply PathPattern {
    micro parse(path: string) -> Self {
        if path.contains(':') {
            let mut params = []
            let pattern = path.split('/').map(|segment| {
                if segment.starts_with(':') {
                    params.push(segment[1..].to_string())
                    "([^/]+)".to_string()
                } else {
                    regex::escape(segment)
                }
            }).collect::<Vector<_>>().join("/")
            
            PathPattern::Dynamic(pattern, params)
        } else if path.ends_with("*") {
            PathPattern::Wildcard(path[..path.len()-1].to_string())
        } else {
            PathPattern::Static(path.to_string())
        }
    }
    
    micro matches(&self, path: string) -> bool {
        match self {
            PathPattern::Static(pattern) => pattern == path,
            PathPattern::Dynamic(pattern, _) => {
                let regex = Regex::new(pattern).unwrap()
                regex.is_match(path)
            },
            PathPattern::Wildcard(prefix) => path.starts_with(prefix),
        }
    }
    
    micro extract_params(&self, path: string) -> HashMap<string, string> {
        let mut params = HashMap::new()
        
        if let PathPattern::Dynamic(pattern, param_names) = self {
            let regex = Regex::new(pattern).unwrap()
            if let Some(captures) = regex.captures(path) {
                for (i, name) in param_names.iter().enumerate() {
                    if let Some(value) = captures.get(i + 1) {
                        params.insert(name.clone(), value.as_str().to_string())
                    }
                }
            }
        }
        
        params
    }
}
```

### 请求和响应处理

```valkyrie
# HTTP 请求
class HttpRequest {
    method: HttpMethod
    path: string
    query_params: HashMap<string, string>
    headers: HashMap<string, string>
    body: Vector<u8>
    params: HashMap<string, string>  # 路由参数
}

#[derive(Clone, PartialEq)]
union HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

imply HttpRequest {
    micro parse(buffer: &[u8]) -> Result<Self, ParseError> {
        let request_str = String::from_utf8_lossy(buffer)
        let lines: Vector<string> = request_str.lines().collect()
        
        if lines.is_empty() {
            return Fail { error: ParseError::InvalidRequest }
        }
        
        # 解析请求行
        let request_line_parts: Vector<string> = lines[0].split_whitespace().collect()
        if request_line_parts.len() < 3 {
            return Fail { error: ParseError::InvalidRequestLine }
        }
        
        let method = HttpMethod::from_str(request_line_parts[0])?
        let url_parts: Vector<string> = request_line_parts[1].splitn(2, '?').collect()
        let path = url_parts[0].to_string()
        
        # 解析查询参数
        let query_params = if url_parts.len() > 1 {
            parse_query_string(url_parts[1])
        } else {
            HashMap::new()
        }
        
        # 解析头部
        let mut headers = HashMap::new()
        let mut body_start = 1
        
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.is_empty() {
                body_start = i + 1
                break
            }
            
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_lowercase()
                let value = line[colon_pos + 1..].trim().to_string()
                headers.insert(key, value)
            }
        }
        
        # 解析请求体
        let body = if body_start < lines.len() {
            lines[body_start..].join("\n").into_bytes()
        } else {
            []
        }
        
        Ok(HttpRequest {
            method,
            path,
            query_params,
            headers,
            body,
            params: HashMap::new(),
        })
    }
    
    micro get_header(&self, name: string) -> string? {
        self.headers.get(&name.to_lowercase())
    }
    
    micro get_param(&self, name: string) -> string? {
        self.params.get(name)
    }
    
    micro get_query(&self, name: string) -> string? {
        self.query_params.get(name)
    }
    
    micro json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
    
    micro form_data(&self) -> HashMap<string, string> {
        if let Some(content_type) = self.get_header("content-type") {
            if content_type.contains("application/x-www-form-urlencoded") {
                let body_str = String::from_utf8_lossy(&self.body)
                return parse_query_string(&body_str)
            }
        }
        HashMap::new()
    }
}

# HTTP 响应
class HttpResponse {
    status_code: u16
    status_text: string
    headers: HashMap<string, string>
    body: Vector<u8>
}

imply HttpResponse {
    micro new(status_code: u16) -> Self {
        let status_text = match status_code {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        }.to_string()
        
        HttpResponse {
            status_code,
            status_text,
            headers: HashMap::new(),
            body: [],
        }
    }
    
    micro ok() -> Self {
        Self::new(200)
    }
    
    micro created() -> Self {
        Self::new(201)
    }
    
    micro bad_request() -> Self {
        Self::new(400)
    }
    
    micro not_found() -> Self {
        Self::new(404)
    }
    
    micro internal_error() -> Self {
        Self::new(500)
    }
    
    micro with_header(mut self, name: string, value: string) -> Self {
        self.headers.insert(name.to_string(), value.to_string())
        self
    }
    
    micro with_json<T: Serialize>(mut self, data: &T) -> Result<Self, serde_json::Error> {
        let json_str = serde_json::to_string(data)?
        self.body = json_str.into_bytes()
        self.headers.insert("Content-Type".to_string(), "application/json".to_string())
        Ok(self)
    }
    
    micro with_html(mut self, html: string) -> Self {
        self.body = html.as_bytes().to_vec()
        self.headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string())
        self
    }
    
    micro with_text(mut self, text: string) -> Self {
        self.body = text.as_bytes().to_vec()
        self.headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string())
        self
    }
    
    micro redirect(location: string) -> Self {
        Self::new(302).with_header("Location", location)
    }
    
    micro to_bytes(&self) -> Vector<u8> {
        let mut response = format(
            "HTTP/1.1 {} {}\r\n",
            self.status_code,
            self.status_text
        )
        
        # 添加头部
        for (key, value) in &self.headers {
            response.push_str(&format("{}:{}\r\n", key, value))
        }
        
        # 添加 Content-Length
        response.push_str(&format("Content-Length: {}\r\n", self.body.len()))
        response.push_str("\r\n")
        
        let mut bytes = response.into_bytes()
        bytes.extend_from_slice(&self.body)
        bytes
    }
}
```

## 中间件系统

### 中间件接口

```valkyrie
# 中间件特征
trait Middleware: Send + Sync {
    async micro process(&self, context: &mut RequestContext) -> Result<(), MiddlewareError>
}

class RequestContext {
    request: HttpRequest
    response: HttpResponse?
    data: HashMap<string, Box<dyn Any + Send + Sync>>
}

imply RequestContext {
    micro new(request: HttpRequest) -> Self {
        RequestContext {
            request,
            response: None,
            data: HashMap::new(),
        }
    }
    
    micro set_data<T: Any + Send + Sync>(&mut self, key: string, value: T) {
        self.data.insert(key.to_string(), Box::new(value))
    }
    
    micro get_data<T: Any + Send + Sync>(&self, key: string) -> T? {
        self.data.get(key)?.downcast_ref::<T>()
    }
}

# 日志中间件
class LoggingMiddleware {
    format: string
}

imply LoggingMiddleware {
    micro new() -> Self {
        LoggingMiddleware {
            format: "{method} {path} - {status} ({duration}ms)".to_string(),
        }
    }
}

imply LoggingMiddleware: Middleware {
    async micro process(&self, context: &mut RequestContext) -> Result<(), MiddlewareError> {
        let start_time = std::time::Instant::now()
        
        # 记录请求开始
        print("[{}] {} {}", 
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            context.request.method.as_str(),
            context.request.path
        )
        
        # 在响应数据中存储开始时间
        context.set_data("start_time", start_time)
        
        Ok(())
    }
}

# CORS 中间件
class CorsMiddleware {
    allowed_origins: Vector<string>
    allowed_methods: Vector<HttpMethod>
    allowed_headers: Vector<string>
    max_age: u32
}

imply CorsMiddleware {
    micro new() -> Self {
        CorsMiddleware {
            allowed_origins: ["*".to_string()],
            allowed_methods: [HttpMethod::GET, HttpMethod::POST, HttpMethod::PUT, HttpMethod::DELETE],
            allowed_headers: ["Content-Type".to_string(), "Authorization".to_string()],
            max_age: 86400,
        }
    }
    
    micro allow_origin(mut self, origin: string) -> Self {
        self.allowed_origins = [origin.to_string()]
        self
    }
    
    micro allow_methods(mut self, methods: Vector<HttpMethod>) -> Self {
        self.allowed_methods = methods
        self
    }
}

imply CorsMiddleware: Middleware {
    async micro process(&self, context: &mut RequestContext) -> Result<(), MiddlewareError> {
        # 处理预检请求
        if context.request.method == HttpMethod::OPTIONS {
            let response = HttpResponse::ok()
                .with_header("Access-Control-Allow-Origin", &self.allowed_origins.join(", "))
                .with_header("Access-Control-Allow-Methods", 
                    &self.allowed_methods.iter().map(|m| m.as_str()).collect::<Vector<_>>().join(", "))
                .with_header("Access-Control-Allow-Headers", &self.allowed_headers.join(", "))
                .with_header("Access-Control-Max-Age", &self.max_age.to_string())
            
            context.response = Some(response)
            return Ok(())
        }
        
        # 为其他请求添加 CORS 头
        if let Some(ref mut response) = context.response {
            response.headers.insert("Access-Control-Allow-Origin".to_string(), 
                self.allowed_origins.join(", "))
        }
        
        Ok(())
    }
}

# 认证中间件
class AuthMiddleware {
    secret_key: string,
    protected_paths: Vector<string>,
}

imply AuthMiddleware {
    micro new(secret_key: string) -> Self {
        AuthMiddleware {
            secret_key: secret_key.to_string(),
            protected_paths: [],
        }
    }
    
    micro protect_path(mut self, path: string) -> Self {
        self.protected_paths.push(path.to_string())
        self
    }
    
    micro verify_token(&self, token: string) -> Result<Claims, AuthError> {
        # JWT 令牌验证逻辑
        jwt::decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret_key.as_ref()),
            &Validation::default()
        ).map(|data| data.claims)
        .map_err(|_| AuthError::InvalidToken)
    }
}

imply AuthMiddleware: Middleware {
    async micro process(&self, context: &mut RequestContext) -> Result<(), MiddlewareError> {
        # 检查路径是否需要保护
        let needs_auth = self.protected_paths.iter()
            .any(|path| context.request.path.starts_with(path))
        
        if !needs_auth {
            return Ok(())
        }
        
        # 提取认证令牌
        let token = context.request.get_header("authorization")
            .and_then(|auth| auth.strip_prefix("Bearer "))
            .ok_or(MiddlewareError::Unauthorized)?;
        
        # 验证令牌
        match self.verify_token(token) {
            Ok(claims) => {
                context.set_data("user_claims", claims)
                Ok(())
            },
            Err(_) => {
                context.response = Some(HttpResponse::new(401).with_text("Unauthorized"))
                Ok(())
            }
        }
    }
}
```

## 模板引擎

### HTML 模板系统

```valkyrie
# 模板引擎
class TemplateEngine {
    templates: HashMap<string, Template>,
    template_dir: string,
    cache_enabled: bool,
}

class Template {
    name: string,
    content: string,
    compiled: CompiledTemplate,
}

class CompiledTemplate {
    blocks: Vector<TemplateBlock>,
}

enums TemplateBlock {
    Text(string),
    Variable(string),
    Loop {
        variable: string,
        items: string,
        body: Vector<TemplateBlock>,
    },
    Condition {
        expression: string,
        then_body: Vector<TemplateBlock>,
        else_body: Vector<TemplateBlock>?,
    },
    Include(string),
}

imply TemplateEngine {
    micro new(template_dir: string) -> Self {
        TemplateEngine {
            templates: HashMap::new(),
            template_dir: template_dir.to_string(),
            cache_enabled: true,
        }
    }
    
    micro load_template(&mut self, name: string) -> Result<(), TemplateError> {
        let path = format("{}/{}.html", self.template_dir, name)
        let content = std::fs::read_to_string(&path)
            .map_err(|_| TemplateError::TemplateNotFound(name.to_string()))?;
        
        let compiled = self.compile_template(&content)?;
        
        let template = Template {
            name: name.to_string(),
            content,
            compiled,
        }
        
        self.templates.insert(name.to_string(), template)
        Ok(())
    }
    
    micro render(&mut self, name: string, context: &TemplateContext) -> Result<string, TemplateError> {
        if !self.templates.contains_key(name) {
            self.load_template(name)?
        }
        
        let template = self.templates.get(name).unwrap()
        self.render_blocks(&template.compiled.blocks, context)
    }
    
    micro compile_template(&self, content: string) -> Result<CompiledTemplate, TemplateError> {
        let mut blocks = []
        let mut chars = content.chars().peekable()
        let mut current_text = String::new()
        
        while let Some(ch) = chars.next() {
            if ch == '{' && chars.peek() == Some(&'{') {
                chars.next()  # 消费第二个 '{'
                
                # 保存当前文本块
                if !current_text.is_empty() {
                    blocks.push(TemplateBlock::Text(current_text.clone()))
                    current_text.clear()
                }
                
                # 解析模板表达式
                let mut expression = String::new()
                let mut brace_count = 0
                
                while let Some(ch) = chars.next() {
                    if ch == '}' && chars.peek() == Some(&'}') && brace_count == 0 {
                        chars.next()  # 消费第二个 '}'
                        break
                    }
                    
                    if ch == '{' { brace_count += 1 }
                    if ch == '}' { brace_count -= 1 }
                    
                    expression.push(ch)
                }
                
                # 解析表达式类型
                let block = self.parse_expression(&expression.trim())?;
                blocks.push(block)
            } else {
                current_text.push(ch)
            }
        }
        
        # 添加最后的文本块
        if !current_text.is_empty() {
            blocks.push(TemplateBlock::Text(current_text))
        }
        
        Ok(CompiledTemplate { blocks })
    }
    
    micro parse_expression(&self, expr: string) -> Result<TemplateBlock, TemplateError> {
        if expr.starts_with("for ") {
            # 解析循环: for item in items
            let parts: Vector<string> = expr.split_whitespace().collect()
            if parts.len() >= 4 && parts[2] == "in" {
                return Ok(TemplateBlock::Loop {
                    variable: parts[1].to_string(),
                    items: parts[3].to_string(),
                    body: [],  # 需要进一步解析
                })
            }
        } else if expr.starts_with("if ") {
            # 解析条件: if condition
            let condition = expr[3..].trim()
            return Ok(TemplateBlock::Condition {
                expression: condition.to_string(),
                then_body: [],
                else_body: None,
            })
        } else if expr.starts_with("include ") {
            # 解析包含: include "template_name"
            let template_name = expr[8..].trim().trim_matches('"')
            return Ok(TemplateBlock::Include(template_name.to_string()))
        } else {
            # 变量替换
            return Ok(TemplateBlock::Variable(expr.to_string()))
        }
        
        Fail { error: TemplateError::InvalidExpression(expr.to_string()) }
    }
    
    micro render_blocks(&self, blocks: &[TemplateBlock], context: &TemplateContext) -> Result<string, TemplateError> {
        let mut result = String::new()
        
        for block in blocks {
            match block {
                TemplateBlock::Text(text) => {
                    result.push_str(text)
                },
                TemplateBlock::Variable(var_name) => {
                    if let Some(value) = context.get(var_name) {
                        result.push_str(&value.to_string())
                    }
                },
                TemplateBlock::Loop { variable, items, body } => {
                    if let Some(TemplateValue::Array(arr)) = context.get(items) {
                        for item in arr {
                            let mut loop_context = context.clone()
                            loop_context.set(variable, item.clone())
                            result.push_str(&self.render_blocks(body, &loop_context)?)
                        }
                    }
                },
                TemplateBlock::Condition { expression, then_body, else_body } => {
                    if self.evaluate_condition(expression, context)? {
                        result.push_str(&self.render_blocks(then_body, context)?)
                    } else if let Some(else_body) = else_body {
                        result.push_str(&self.render_blocks(else_body, context)?)
                    }
                },
                TemplateBlock::Include(template_name) => {
                    # 递归渲染包含的模板
                    let included = self.render(template_name, context)?;
                    result.push_str(&included)
                },
            }
        }
        
        Ok(result)
    }
}

# 模板上下文
#[derive(Clone)]
class TemplateContext {
    variables: HashMap<string, TemplateValue>,
}

#[derive(Clone)]
enums TemplateValue {
    String(string),
    Number(f64),
    Boolean(bool),
    Array(Vector<TemplateValue>),
    Object(HashMap<string, TemplateValue>),
}

imply TemplateContext {
    micro new() -> Self {
        TemplateContext {
            variables: HashMap::new(),
        }
    }
    
    micro set(&mut self, key: string, value: TemplateValue) {
        self.variables.insert(key.to_string(), value)
    }
    
    micro get(&self, key: string) -> TemplateValue? {
        self.variables.get(key)
    }
    
    micro from_json(json: serde_json::Value) -> Self {
        let mut context = TemplateContext::new()
        context.set("data", Self::json_to_template_value(json))
        context
    }
    
    micro json_to_template_value(json: serde_json::Value) -> TemplateValue {
        match json {
            serde_json::Value::String(s) => TemplateValue::String(s),
            serde_json::Value::Number(n) => TemplateValue::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::Bool(b) => TemplateValue::Boolean(b),
            serde_json::Value::Array(arr) => {
                let values = arr.into_iter().map(Self::json_to_template_value).collect()
                TemplateValue::Array(values)
            },
            serde_json::Value::Object(obj) => {
                let mut map = HashMap::new()
                for (k, v) in obj {
                    map.insert(k, Self::json_to_template_value(v))
                }
                TemplateValue::Object(map)
            },
            serde_json::Value::Null => TemplateValue::String(String::new()),
        }
    }
}
```

## WebAssembly 集成

### WASM 模块加载

```valkyrie
# WebAssembly 运行时
class WasmRuntime {
    modules: HashMap<string, WasmModule>,
    instance_pool: Vector<WasmInstance>,
}

class WasmModule {
    name: string,
    bytecode: Vector<u8>,
    exports: Vector<WasmExport>,
    imports: Vector<WasmImport>,
}

class WasmInstance {
    module_name: string,
    instance: wasmtime::Instance,
    store: wasmtime::Store<()>,
}

class WasmExport {
    name: string,
    function_type: WasmFunctionType,
}

class WasmImport {
    module: string,
    name: string,
    function_type: WasmFunctionType,
}

class WasmFunctionType {
    params: Vector<WasmValueType>,
    results: Vector<WasmValueType>,
}

enums WasmValueType {
    I32,
    I64,
    F32,
    F64,
}

imply WasmRuntime {
    micro new() -> Self {
        WasmRuntime {
            modules: HashMap::new(),
            instance_pool: [],
        }
    }
    
    micro load_module(&mut self, name: string, wasm_bytes: Vector<u8>) -> Result<(), WasmError> {
        let engine = wasmtime::Engine::default()
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))?;
        
        # 分析模块导出和导入
        let exports = self.analyze_exports(&module)?
        let imports = self.analyze_imports(&module)?
        
        let wasm_module = WasmModule {
            name: name.to_string(),
            bytecode: wasm_bytes,
            exports,
            imports,
        }
        
        self.modules.insert(name.to_string(), wasm_module)
        Ok(())
    }
    
    micro create_instance(&mut self, module_name: string) -> Result<usize, WasmError> {
        let module = self.modules.get(module_name)
            .ok_or_else(|| WasmError::ModuleNotFound(module_name.to_string()))?;
        
        let engine = wasmtime::Engine::default()
        let wasm_module = wasmtime::Module::new(&engine, &module.bytecode)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))?;
        
        let mut store = wasmtime::Store::new(&engine, ())
        
        # 创建导入对象
        let mut imports = []
        for import in &module.imports {
            let func = self.create_host_function(&import, &mut store)?
            imports.push(func.into())
        }
        
        let instance = wasmtime::Instance::new(&mut store, &wasm_module, &imports)
            .map_err(|e| WasmError::InstantiationFailed(e.to_string()))?;
        
        let wasm_instance = WasmInstance {
            module_name: module_name.to_string(),
            instance,
            store,
        }
        
        self.instance_pool.push(wasm_instance)
        Ok(self.instance_pool.len() - 1)
    }
    
    micro call_function(
        &mut self,
        instance_id: usize,
        function_name: string,
        args: Vector<WasmValue>
    ) -> Result<Vector<WasmValue>, WasmError> {
        let instance = self.instance_pool.get_mut(instance_id)
            .ok_or(WasmError::InvalidInstance)?;
        
        let func = instance.instance
            .get_typed_func::<(i32, i32), i32>(&mut instance.store, function_name)
            .map_err(|e| WasmError::FunctionNotFound(e.to_string()))?;
        
        # 转换参数
        let wasm_args: Vector<wasmtime::Val> = args.into_iter().map(|arg| {
            match arg {
                WasmValue::I32(v) => wasmtime::Val::I32(v),
                WasmValue::I64(v) => wasmtime::Val::I64(v),
                WasmValue::F32(v) => wasmtime::Val::F32(v.to_bits()),
                WasmValue::F64(v) => wasmtime::Val::F64(v.to_bits()),
            }
        }).collect()
        
        # 调用函数
        let mut results = [wasmtime::Val::I32(0); 1]  # 假设返回一个 i32
        func.call(&mut instance.store, &wasm_args, &mut results)
            .map_err(|e| WasmError::ExecutionFailed(e.to_string()))?;
        
        # 转换结果
        let wasm_results: Vector<WasmValue> = results.into_iter().map(|val| {
            match val {
                wasmtime::Val::I32(v) => WasmValue::I32(v),
                wasmtime::Val::I64(v) => WasmValue::I64(v),
                wasmtime::Val::F32(v) => WasmValue::F32(f32::from_bits(v)),
                wasmtime::Val::F64(v) => WasmValue::F64(f64::from_bits(v)),
                _ => WasmValue::I32(0),
            }
        }).collect()
        
        Ok(wasm_results)
    }
}

#[derive(Clone)]
enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}
```

## 实时通信

### WebSocket 支持

```valkyrie
# WebSocket 服务器
class WebSocketServer {
    connections: HashMap<ConnectionId, WebSocketConnection>,
    message_handlers: HashMap<String, Box<dyn MessageHandler>>,
    next_connection_id: ConnectionId,
}

class WebSocketConnection {
    id: ConnectionId,
    socket: WebSocket,
    user_id: Option<String>,
    rooms: HashSet<String>,
    last_ping: Instant,
}

trait MessageHandler: Send + Sync {
    async micro handle(&self, connection_id: ConnectionId, message: WebSocketMessage) -> Result<(), WebSocketError>
}

#[derive(Clone)]
enum WebSocketMessage {
    Text(String),
    Binary(Vector<u8>),
    Ping(Vector<u8>),
    Pong(Vector<u8>),
    Close(Option<CloseFrame>),
}

class CloseFrame {
    code: u16,
    reason: String,
}

impl WebSocketServer {
    micro new() -> Self {
        WebSocketServer {
            connections: HashMap::new(),
            message_handlers: HashMap::new(),
            next_connection_id: 0,
        }
    }
    
    micro on_message<H: MessageHandler + 'static>(&mut self, message_type: &str, handler: H) {
        self.message_handlers.insert(message_type.to_string(), Box::new(handler))
    }
    
    async micro handle_connection(&mut self, socket: WebSocket) -> Result<(), WebSocketError> {
        let connection_id = self.next_connection_id
        self.next_connection_id += 1
        
        let connection = WebSocketConnection {
            id: connection_id,
            socket,
            user_id: None,
            rooms: HashSet::new(),
            last_ping: Instant::now(),
        }
        
        self.connections.insert(connection_id, connection)
        
        # 启动消息处理循环
        self.message_loop(connection_id).await
    }
    
    async micro message_loop(&mut self, connection_id: ConnectionId) -> Result<(), WebSocketError> {
        loop {
            let connection = self.connections.get_mut(&connection_id)
                .ok_or(WebSocketError::ConnectionNotFound)?;
            
            # 接收消息
            match connection.socket.next().await {
                Some(Ok(msg)) => {
                    let ws_message = match msg {
                        tungstenite::Message::Text(text) => WebSocketMessage::Text(text),
                        tungstenite::Message::Binary(data) => WebSocketMessage::Binary(data),
                        tungstenite::Message::Ping(data) => WebSocketMessage::Ping(data),
                        tungstenite::Message::Pong(data) => WebSocketMessage::Pong(data),
                        tungstenite::Message::Close(frame) => {
                            let close_frame = frame.map(|f| CloseFrame {
                                code: f.code.into(),
                                reason: f.reason.to_string(),
                            })
                            WebSocketMessage::Close(close_frame)
                        },
                    }
                    
                    # 处理消息
                    self.handle_message(connection_id, ws_message).await?
                },
                Some(Err(e)) => {
                    print("WebSocket error: {}", e)
                    break
                },
                None => {
                    # 连接关闭
                    break
                }
            }
        }
        
        # 清理连接
        self.connections.remove(&connection_id)
        Ok(())
    }
    
    async micro handle_message(
        &mut self,
        connection_id: ConnectionId,
        message: WebSocketMessage
    ) -> Result<(), WebSocketError> {
        match &message {
            WebSocketMessage::Text(text) => {
                # 尝试解析为 JSON 消息
                if let Ok(json_msg) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(msg_type) = json_msg.get("type").and_then(|t| t.as_str()) {
                        if let Some(handler) = self.message_handlers.get(msg_type) {
                            handler.handle(connection_id, message).await?
                        }
                    }
                }
            },
            WebSocketMessage::Ping(data) => {
                # 响应 ping
                self.send_to_connection(connection_id, WebSocketMessage::Pong(data.clone())).await?
            },
            WebSocketMessage::Close(_) => {
                # 处理连接关闭
                self.connections.remove(&connection_id)
            },
            _ => {}
        }
        
        Ok(())
    }
    
    async micro send_to_connection(
        &mut self,
        connection_id: ConnectionId,
        message: WebSocketMessage
    ) -> Result<(), WebSocketError> {
        let connection = self.connections.get_mut(&connection_id)
            .ok_or(WebSocketError::ConnectionNotFound)?;
        
        let tungstenite_msg = match message {
            WebSocketMessage::Text(text) => tungstenite::Message::Text(text),
            WebSocketMessage::Binary(data) => tungstenite::Message::Binary(data),
            WebSocketMessage::Ping(data) => tungstenite::Message::Ping(data),
            WebSocketMessage::Pong(data) => tungstenite::Message::Pong(data),
            WebSocketMessage::Close(frame) => {
                let close_frame = frame.map(|f| tungstenite::protocol::CloseFrame {
                    code: tungstenite::protocol::frame::coding::CloseCode::from(f.code),
                    reason: f.reason.into(),
                })
                tungstenite::Message::Close(close_frame)
            },
        }
        
        connection.socket.send(tungstenite_msg).await
            .map_err(|e| WebSocketError::SendFailed(e.to_string()))
    }
    
    async micro broadcast_to_room(&mut self, room: &str, message: WebSocketMessage) -> Result<(), WebSocketError> {
        let connection_ids: Vector<ConnectionId> = self.connections
            .iter()
            .filter(|(_, conn)| conn.rooms.contains(room))
            .map(|(id, _)| *id)
            .collect()
        
        for connection_id in connection_ids {
            self.send_to_connection(connection_id, message.clone()).await?
        }
        
        Ok(())
    }
    
    micro join_room(&mut self, connection_id: ConnectionId, room: &str) -> Result<(), WebSocketError> {
        let connection = self.connections.get_mut(&connection_id)
            .ok_or(WebSocketError::ConnectionNotFound)?;
        
        connection.rooms.insert(room.to_string())
        Ok(())
    }
    
    micro leave_room(&mut self, connection_id: ConnectionId, room: &str) -> Result<(), WebSocketError> {
        let connection = self.connections.get_mut(&connection_id)
            .ok_or(WebSocketError::ConnectionNotFound)?;
        
        connection.rooms.remove(room)
        Ok(())
    }
}
```

## 完整的 Web 应用示例

### 博客应用

```valkyrie
# 博客应用主函数
async micro main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        max_connections: 1000,
        request_timeout: Duration::from_secs(30),
        static_files_dir: Some("./static".to_string()),
        enable_compression: true,
    }
    
    let mut server = WebServer::new(config)
    
    # 添加中间件
    server.use_middleware(LoggingMiddleware::new())
    server.use_middleware(CorsMiddleware::new().allow_origin("*"))
    
    # 静态文件服务
    server.serve_static("/static", "./static")
    
    # 博客路由
    server.get("/", blog_index)
    server.get("/posts/:id", blog_post)
    server.get("/api/posts", api_posts)
    server.post("/api/posts", api_create_post)
    server.put("/api/posts/:id", api_update_post)
    server.delete("/api/posts/:id", api_delete_post)
    
    # 用户认证路由
    server.post("/api/auth/login", auth_login)
    server.post("/api/auth/register", auth_register)
    server.get("/api/auth/profile", auth_profile)
    
    # 启动服务器
    print("Starting blog server on http://127.0.0.1:8080")
    server.listen().await
}

# 博客首页
async micro blog_index(request: &HttpRequest) -> Result<HttpResponse, HandlerError> {
    let mut template_engine = TemplateEngine::new("./templates")
    
    # 获取最新文章
    let posts = get_recent_posts(10).await?
    
    let mut context = TemplateContext::new()
    context.set("title", TemplateValue::String("我的博客".to_string()))
    context.set("posts", posts_to_template_value(posts))
    
    let html = template_engine.render("index", &context)?
    
    Ok(HttpResponse::ok().with_html(&html))
}

# 文章详情页
async micro blog_post(request: &HttpRequest) -> Result<HttpResponse, HandlerError> {
    let post_id = request.get_param("id")
        .ok_or(HandlerError::BadRequest("Missing post ID".to_string()))?;
    
    let post = get_post_by_id(post_id).await?
        .ok_or(HandlerError::NotFound)?;
    
    let mut template_engine = TemplateEngine::new("./templates")
    let mut context = TemplateContext::new()
    context.set("title", TemplateValue::String(post.title.clone()))
    context.set("post", post_to_template_value(post))
    
    let html = template_engine.render("post", &context)?
    
    Ok(HttpResponse::ok().with_html(&html))
}

# API: 获取文章列表
async micro api_posts(request: &HttpRequest) -> Result<HttpResponse, HandlerError> {
    let page = request.get_query("page")
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1)
    
    let limit = request.get_query("limit")
        .and_then(|l| l.parse::<u32>().ok())
        .unwrap_or(10)
    
    let posts = get_posts_paginated(page, limit).await?
    
    let response_data = serde_json::json!({
        "posts": posts,
        "page": page,
        "limit": limit
    })
    
    HttpResponse::ok().with_json(&response_data)
        .map_err(|e| HandlerError::InternalError(e.to_string()))
}

# API: 创建文章
async micro api_create_post(request: &HttpRequest) -> Result<HttpResponse, HandlerError> {
    let post_data: CreatePostRequest = request.json()
        .map_err(|e| HandlerError::BadRequest(e.to_string()))?;
    
    # 验证数据
    if post_data.title.is_empty() || post_data.content.is_empty() {
        return Err(HandlerError::BadRequest("Title and content are required".to_string()))
    }
    
    let post = create_post(post_data).await?
    
    HttpResponse::created().with_json(&post)
        .map_err(|e| HandlerError::InternalError(e.to_string()))
}

# 数据模型
#[derive(Serialize, Deserialize)]
class Post {
    id: u32
    title: String
    content: String
    author: String
    created_at: chrono::DateTime<chrono::Utc>
    updated_at: chrono::DateTime<chrono::Utc>
}

#[derive(Deserialize)]
class CreatePostRequest {
    title: String
    content: String
    author: String
}

# 数据库操作
async micro get_recent_posts(limit: u32) -> Result<Vector<Post>, DatabaseError> {
    # 数据库查询逻辑
    Ok(vec![])
}

async micro get_post_by_id(id: &str) -> Result<Option<Post>, DatabaseError> {
    # 数据库查询逻辑
    Ok(None)
}

async micro create_post(data: CreatePostRequest) -> Result<Post, DatabaseError> {
    # 数据库插入逻辑
    Ok(Post {
        id: 1,
        title: data.title,
        content: data.content,
        author: data.author,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}
```

Valkyrie 网页开发框架提供了完整的现代 Web 开发工具链，支持高性能服务器、灵活的路由系统、强大的模板引擎、WebAssembly 集成和实时通信功能，为构建各种规模的 Web 应用提供了坚实的基础。