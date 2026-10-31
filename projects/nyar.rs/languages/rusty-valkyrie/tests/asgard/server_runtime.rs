//! 单后端运行时合同骨架：路由、请求作用域、取消/deadline、错误与优雅关闭。
//!
//! 这些测试只验证运行时合同，不涉及作者面 `script server` 语法或真实 HTTP 监听。

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rusty_valkyrie::asgard::server_runtime::{HttpMethod, RouteTable, ServerError, ServerRequest, ServerResponse, SingleBackendServer};

fn table(routes: Vec<(HttpMethod, &str)>) -> RouteTable {
    RouteTable::new(routes.into_iter().map(|(method, path)| (method, path.to_string())).collect()).expect("路由表应合法")
}

#[test]
fn registered_route_runs_handler_and_returns_response() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    let request = ServerRequest::new(HttpMethod::Get, "/health");
    let response = server.handle(&request, None, |route, _scope| {
        assert_eq!(route.path, "/health", "handler 必须收到命中的路由");
        Ok(ServerResponse::text(200, "ok"))
    });
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}

#[test]
fn unknown_path_returns_404_without_running_handler() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    let mut called = false;
    let response = server.handle(&ServerRequest::new(HttpMethod::Get, "/missing"), None, |_, _| {
        called = true;
        Ok(ServerResponse::empty(200))
    });
    assert_eq!(response.status, 404);
    assert!(!called, "非命中路径不得调用处理函数");
}

#[test]
fn method_mismatch_returns_405_with_allow_header() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/items"), (HttpMethod::Post, "/items")]));
    let response = server.handle(&ServerRequest::new(HttpMethod::Delete, "/items"), None, |_, _| Ok(ServerResponse::empty(200)));
    assert_eq!(response.status, 405);
    let allow = response.headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("Allow")).map(|(_, value)| value.as_str());
    assert_eq!(allow, Some("GET, POST"), "405 必须携带允许的方法");
}

#[test]
fn handler_errors_are_closed_without_leaking_internal_details() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/private")]));
    let request = ServerRequest::new(HttpMethod::Get, "/private");
    let unauthorized = server.handle(&request, None, |_, _| Err(ServerError::Unauthorized));
    assert_eq!(unauthorized.status, 401);

    let internal = server.handle(&request, None, |_, _| Err(ServerError::Internal("db password leaked".to_string())));
    assert_eq!(internal.status, 500);
    assert!(internal.body.is_empty(), "内部错误细节不得写入客户端响应体");
}

#[test]
fn shutdown_rejects_new_requests() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    server.shutdown();
    assert!(server.is_shutting_down());
    let response = server.handle(&ServerRequest::new(HttpMethod::Get, "/health"), None, |_, _| Ok(ServerResponse::empty(200)));
    assert_eq!(response.status, 503, "关闭中的新请求必须结构化拒绝");
}

#[test]
fn shutdown_cancels_active_request_scopes() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    let first = server.begin_request(None).expect("启动第一个请求");
    let second = server.begin_request(None).expect("启动第二个请求");
    server.shutdown();
    assert_eq!(first.scope().ensure_active(Instant::now()), Err(ServerError::Cancelled));
    assert_eq!(second.scope().ensure_active(Instant::now()), Err(ServerError::Cancelled), "关闭必须取消全部活动作用域");
}

#[test]
fn cancelling_one_scope_does_not_touch_another() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    let first = server.begin_request(None).expect("启动第一个请求");
    let second = server.begin_request(None).expect("启动第二个请求");
    first.scope().cancellation().cancel();
    assert_eq!(first.scope().ensure_active(Instant::now()), Err(ServerError::Cancelled));
    assert_eq!(second.scope().ensure_active(Instant::now()), Ok(()), "请求作用域必须相互隔离");
}

#[test]
fn expired_deadline_returns_timeout() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    let deadline = Instant::now().checked_sub(Duration::from_secs(5));
    let session = server.begin_request(deadline).expect("启动请求");
    assert_eq!(session.scope().ensure_active(Instant::now()), Err(ServerError::Timeout));
}

#[test]
fn request_scope_cleanup_runs_on_drop() {
    let server = SingleBackendServer::new(table(vec![(HttpMethod::Get, "/health")]));
    let released = Arc::new(AtomicBool::new(false));
    let mut session = server.begin_request(None).expect("启动请求");
    let flag = Arc::clone(&released);
    session.scope_mut().register_cleanup(move || flag.store(true, Ordering::SeqCst));
    drop(session);
    assert!(released.load(Ordering::SeqCst), "作用域销毁必须释放注册的 handle/资源");
}

#[test]
fn route_table_rejects_invalid_or_duplicate_routes() {
    let no_slash = RouteTable::new(vec![(HttpMethod::Get, "health".to_string())]).expect_err("非 `/` 开头路径必须拒绝");
    assert!(no_slash.to_string().contains("以 `/` 开头"), "{no_slash}");

    let with_query = RouteTable::new(vec![(HttpMethod::Get, "/health?x=1".to_string())]).expect_err("查询串不得进入路由路径");
    assert!(with_query.to_string().contains("查询串"), "{with_query}");

    let duplicate = RouteTable::new(vec![(HttpMethod::Get, "/health".to_string()), (HttpMethod::Get, "/health".to_string())])
        .expect_err("同方法同路径必须拒绝");
    assert!(duplicate.to_string().contains("重复路由"), "{duplicate}");
}

#[test]
fn request_header_lookup_is_case_insensitive() {
    let request = ServerRequest {
        method: HttpMethod::Get,
        path: "/health".to_string(),
        headers: vec![("X-Trace".to_string(), "abc".to_string())],
        body: Vec::new(),
    };
    assert_eq!(request.header("x-trace"), Some("abc"));
    assert_eq!(request.header("missing"), None);
}
