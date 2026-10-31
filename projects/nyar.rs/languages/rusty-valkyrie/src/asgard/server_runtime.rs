//! 单后端运行时合同骨架。
//!
//! 首轮后端只支持**单实例**：一个 server 进程在一个部署 generation 内处理
//! HTTP 请求，不引入集群、多租户或分布式 session。本模块只拥有运行时合同：
//! 请求作用域、路由/方法匹配、结构化错误、取消/deadline 与优雅关闭。
//!
//! 作者面 `script server`、路由 DSL 与跨区调用 grammar 尚未冻结，本模块不提供
//! 任何作者语法或自动 RPC；后续编译器按正式 grammar 生成路由表并调用这里的
//! 运行时入口。

use std::{
    fmt,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use crate::asgard::error::{AsgardError, AsgardResult};

/// HTTP 方法；只列出单后端首轮消费的方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HttpMethod {
    /// `GET`。
    Get,
    /// `POST`。
    Post,
    /// `PUT`。
    Put,
    /// `PATCH`。
    Patch,
    /// `DELETE`。
    Delete,
    /// `HEAD`。
    Head,
    /// `OPTIONS`。
    Options,
}

impl HttpMethod {
    /// 大写方法名，用于 `Allow` 响应头与诊断。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// 一个已进入运行时的 HTTP 请求。
///
/// `path` 只含路径部分；查询串与原始 URL 由上游解析后写入 `query`/`headers`，
/// 运行时不重新拼接或猜测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRequest {
    /// HTTP 方法。
    pub method: HttpMethod,
    /// 请求路径（以 `/` 开头，不含查询串）。
    pub path: String,
    /// 请求头；名称保持上游给的大小写。
    pub headers: Vec<(String, String)>,
    /// 请求体字节。
    pub body: Vec<u8>,
}

impl ServerRequest {
    /// 构造一个无头、无体的最小请求。
    pub fn new(method: HttpMethod, path: impl Into<String>) -> Self {
        Self { method, path: path.into(), headers: Vec::new(), body: Vec::new() }
    }

    /// 按不区分大小写查找请求头。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

/// 运行时要写回的 HTTP 响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerResponse {
    /// 状态码。
    pub status: u16,
    /// 响应头。
    pub headers: Vec<(String, String)>,
    /// 响应体。
    pub body: Vec<u8>,
}

impl ServerResponse {
    /// 空体响应。
    pub fn empty(status: u16) -> Self {
        Self { status, headers: Vec::new(), body: Vec::new() }
    }

    /// 文本响应；不在此处设置 `Content-Type`，由 adapter 按目标补齐。
    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self { status, headers: Vec::new(), body: body.into().into_bytes() }
    }

    /// 追加一个响应头。
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

/// 单后端运行时的结构化错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerError {
    /// 路径没有注册路由。
    NotFound,
    /// 路径存在路由，但方法不匹配。
    MethodNotAllowed {
        /// 该路径允许的方法（按路由声明顺序）。
        allowed: Vec<HttpMethod>,
    },
    /// 请求未通过认证/授权。
    Unauthorized,
    /// 请求超过 deadline。
    Timeout,
    /// 请求或服务在完成前被取消。
    Cancelled,
    /// 服务正在优雅关闭，拒绝新请求。
    ShuttingDown,
    /// 服务端内部错误；文本只用于诊断，不进入客户端产物。
    Internal(String),
}

impl ServerError {
    /// 该错误对应的 HTTP 状态码。
    pub fn status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::MethodNotAllowed { .. } => 405,
            Self::Unauthorized => 401,
            Self::Timeout => 408,
            Self::Cancelled => 499,
            Self::ShuttingDown => 503,
            Self::Internal(_) => 500,
        }
    }

    /// 转为响应；`MethodNotAllowed` 携带 `Allow` 头，内部错误不泄漏细节。
    pub fn into_response(self) -> ServerResponse {
        match self {
            Self::NotFound => ServerResponse::empty(404),
            Self::MethodNotAllowed { allowed } => {
                let allow = allowed.iter().map(|method| method.as_str()).collect::<Vec<_>>().join(", ");
                ServerResponse::empty(405).with_header("Allow", allow)
            }
            Self::Unauthorized => ServerResponse::empty(401),
            Self::Timeout => ServerResponse::empty(408),
            Self::Cancelled => ServerResponse::empty(499),
            Self::ShuttingDown => ServerResponse::empty(503),
            // 诊断文本留在进程内，不写入客户端响应体。
            Self::Internal(_detail) => ServerResponse::empty(500),
        }
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("route not found"),
            Self::MethodNotAllowed { allowed } => {
                write!(f, "method not allowed; allowed: {}", allowed.iter().map(|method| method.as_str()).collect::<Vec<_>>().join(", "))
            }
            Self::Unauthorized => f.write_str("unauthorized"),
            Self::Timeout => f.write_str("request deadline exceeded"),
            Self::Cancelled => f.write_str("request cancelled"),
            Self::ShuttingDown => f.write_str("server is shutting down"),
            Self::Internal(detail) => write!(f, "internal server error: {detail}"),
        }
    }
}

impl std::error::Error for ServerError {}

/// 一条已注册路由：方法 + 精确路径。
///
/// 首轮只做精确路径匹配；路径参数、通配与自动路由不在未冻结的作者面上发明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredRoute {
    /// 允许的方法。
    pub method: HttpMethod,
    /// 精确请求路径（以 `/` 开头）。
    pub path: String,
}

/// 路由解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRoute {
    /// 路由在表内的稳定下标。
    pub index: usize,
    /// 命中的方法。
    pub method: HttpMethod,
    /// 命中的路径。
    pub path: String,
}

/// 单实例路由表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteTable {
    routes: Vec<RegisteredRoute>,
}

impl RouteTable {
    /// 校验并注册路由；空路径、非 `/` 开头或同方法同路径重复声明都拒绝。
    pub fn new(routes: Vec<(HttpMethod, String)>) -> AsgardResult<Self> {
        let mut table = Vec::with_capacity(routes.len());
        for (method, path) in routes {
            if path.is_empty() || !path.starts_with('/') {
                return Err(AsgardError::msg(report!("路由路径必须以 `/` 开头且非空: `{path}`")));
            }
            if path.contains('?') || path.contains('#') {
                return Err(AsgardError::msg(report!("路由路径只接受路径部分，不能包含查询串或片段: `{path}`")));
            }
            if table.iter().any(|route: &RegisteredRoute| route.method == method && route.path == path) {
                return Err(AsgardError::msg(report!("重复路由声明: {} {path}", method.as_str())));
            }
            table.push(RegisteredRoute { method, path });
        }
        Ok(Self { routes: table })
    }

    /// 已注册路由，按声明顺序。
    pub fn routes(&self) -> &[RegisteredRoute] {
        &self.routes
    }

    /// 解析请求方法 + 路径；未知路径为 `NotFound`，路径存在但方法不匹配为
    /// `MethodNotAllowed` 并携带该路径的全部允许方法。
    pub fn resolve(&self, method: HttpMethod, path: &str) -> Result<ResolvedRoute, ServerError> {
        let mut allowed = Vec::new();
        for (index, route) in self.routes.iter().enumerate() {
            if route.path != path {
                continue;
            }
            if route.method == method {
                return Ok(ResolvedRoute { index, method, path: route.path.clone() });
            }
            allowed.push(route.method);
        }
        if allowed.is_empty() { Err(ServerError::NotFound) } else { Err(ServerError::MethodNotAllowed { allowed }) }
    }
}

/// 取消句柄：跨线程停止一个请求作用域，不触碰其它作用域。
#[derive(Debug, Clone)]
pub struct CancellationHandle(Arc<AtomicBool>);

impl CancellationHandle {
    /// 标记取消。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// 是否已被取消。
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 单个请求的作用域：deadline、取消标记与释放回调。
///
/// 作用域销毁时按注册逆序执行 cleanups，取消未完成工作并释放 handle；
/// 不使用进程级全局状态承载请求数据。
pub struct RequestScope {
    cancellation: CancellationHandle,
    deadline: Option<Instant>,
    cleanups: Vec<Box<dyn FnOnce() + Send>>,
}

impl RequestScope {
    /// 取消句柄；可在其它线程调用以取消本作用域。
    pub fn cancellation(&self) -> CancellationHandle {
        self.cancellation.clone()
    }

    /// 注册释放回调；作用域销毁时执行，注册顺序无关，按逆序执行。
    pub fn register_cleanup(&mut self, cleanup: impl FnOnce() + Send + 'static) {
        self.cleanups.push(Box::new(cleanup));
    }

    /// 在 `now` 时刻校验作用域仍可继续工作。
    pub fn ensure_active(&self, now: Instant) -> Result<(), ServerError> {
        if self.cancellation.is_cancelled() {
            return Err(ServerError::Cancelled);
        }
        if let Some(deadline) = self.deadline
            && now >= deadline
        {
            return Err(ServerError::Timeout);
        }
        Ok(())
    }

    /// 作用域 deadline；未设置时返回 `None`。
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

impl Drop for RequestScope {
    fn drop(&mut self) {
        while let Some(cleanup) = self.cleanups.pop() {
            cleanup();
        }
    }
}

/// 一个请求会话：拥有请求作用域，销毁即释放资源。
pub struct RequestSession {
    scope: RequestScope,
}

impl RequestSession {
    /// 请求作用域。
    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }

    /// 可变的请求作用域；用于注册 cleanup。
    pub fn scope_mut(&mut self) -> &mut RequestScope {
        &mut self.scope
    }
}

/// 单实例 server 运行时。
///
/// 只做单进程内的路由解析、请求作用域与关闭状态；不内置 HTTP 监听或集群。
/// 适配层（HTTP 库/宿主）负责把连接转换为 [`ServerRequest`] 并写回
/// [`ServerResponse`]。
pub struct SingleBackendServer {
    routes: RouteTable,
    shutting_down: Arc<AtomicBool>,
    active: Mutex<Vec<Weak<AtomicBool>>>,
}

impl SingleBackendServer {
    /// 用一张路由表构造单实例运行时。
    pub fn new(routes: RouteTable) -> Self {
        Self { routes, shutting_down: Arc::new(AtomicBool::new(false)), active: Mutex::new(Vec::new()) }
    }

    /// 路由表。
    pub fn routes(&self) -> &RouteTable {
        &self.routes
    }

    /// 是否处于优雅关闭状态。
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// 开始一个请求会话；关闭中拒绝新请求。
    ///
    /// 会话注册自己的取消标记，`shutdown` 会取消全部活动会话。
    pub fn begin_request(&self, deadline: Option<Instant>) -> Result<RequestSession, ServerError> {
        if self.is_shutting_down() {
            return Err(ServerError::ShuttingDown);
        }
        let token = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active.lock().expect("active request registry poisoned");
            active.retain(|weak| weak.strong_count() > 0);
            active.push(Arc::downgrade(&token));
        }
        Ok(RequestSession { scope: RequestScope { cancellation: CancellationHandle(token), deadline, cleanups: Vec::new() } })
    }

    /// 处理一个请求：先解析路由，再在该请求作用域内调用处理函数。
    ///
    /// 404/405/503 由运行时直接闭合；处理函数返回的 [`ServerError`] 转为响应。
    pub fn handle<H>(&self, request: &ServerRequest, deadline: Option<Instant>, handler: H) -> ServerResponse
    where
        H: FnOnce(&ResolvedRoute, &mut RequestScope) -> Result<ServerResponse, ServerError>,
    {
        let route = match self.routes.resolve(request.method, &request.path) {
            Ok(route) => route,
            Err(error) => return error.into_response(),
        };
        let mut session = match self.begin_request(deadline) {
            Ok(session) => session,
            Err(error) => return error.into_response(),
        };
        match handler(&route, session.scope_mut()) {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    }

    /// 进入优雅关闭：拒绝新请求，并取消全部活动请求作用域。
    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let active = self.active.lock().expect("active request registry poisoned");
        for weak in active.iter() {
            if let Some(token) = weak.upgrade() {
                token.store(true, Ordering::SeqCst);
            }
        }
    }
}
