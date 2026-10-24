//! `asgard dev` 开发服务器：静态托管 + 文件监视 + SSE/WebSocket 风格 HMR 推送。

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
};

use miette::{IntoDiagnostic, Result, WrapErr};

use crate::pipeline::{CompileOptions, compile_voa_project};

static BUILD_GENERATION: AtomicU64 = AtomicU64::new(1);

/// 开发服务器选项。
#[derive(Debug, Clone)]
pub struct DevServerOptions {
    /// 项目目录。
    pub project_dir: PathBuf,
    /// dist 目录。
    pub dist_dir: PathBuf,
    /// 监听地址。
    pub host: String,
    /// 监听端口。
    pub port: u16,
    /// 监视的源码目录（相对 project_dir）。
    pub watch_dirs: Vec<String>,
    /// 忽略的路径片段。
    pub ignore_dirs: Vec<String>,
    /// 防抖毫秒。
    pub debounce_ms: u64,
}

impl DevServerOptions {
    /// 默认开发服务器选项。
    pub fn new(project_dir: PathBuf, dist_dir: PathBuf) -> Self {
        Self {
            project_dir,
            dist_dir,
            host: "127.0.0.1".into(),
            port: 3000,
            watch_dirs: vec!["source".into(), "assets".into()],
            ignore_dirs: vec!["node_modules".into(), "dist".into(), ".git".into()],
            debounce_ms: 100,
        }
    }
}

/// 启动开发服务器（阻塞当前线程）。
pub fn run_dev_server(options: &DevServerOptions, compile: &CompileOptions) -> Result<()> {
    let generation = Arc::new(AtomicU64::new(BUILD_GENERATION.load(Ordering::SeqCst)));
    let rebuilding = Arc::new(Mutex::new(false));
    let compile_opts = compile.clone();
    let project_dir = options.project_dir.clone();
    let dist_dir = options.dist_dir.clone();
    let watch_dirs = options.watch_dirs.clone();
    let ignore_dirs = options.ignore_dirs.clone();
    let debounce = Duration::from_millis(options.debounce_ms);

    spawn_watcher(
        project_dir.clone(),
        watch_dirs,
        ignore_dirs,
        debounce,
        generation.clone(),
        rebuilding.clone(),
        compile_opts,
        dist_dir.clone(),
    );

    let addr = format!("{}:{}", options.host, options.port);
    let listener = TcpListener::bind(&addr).into_diagnostic().wrap_err_with(|| format!("绑定 {addr} 失败"))?;
    eprintln!("asgard dev server: http://{addr}/");
    eprintln!("asgard dev server: HMR SSE /__asgard/hmr/events (legacy JSON /__asgard/hmr)");

    for stream in listener.incoming() {
        let stream = stream.into_diagnostic()?;
        let current_gen = generation.load(Ordering::SeqCst);
        if let Err(error) = handle_connection(stream, &dist_dir, current_gen, generation.clone()) {
            eprintln!("asgard dev: {error}");
        }
    }
    Ok(())
}

fn spawn_watcher(
    project_dir: PathBuf,
    watch_dirs: Vec<String>,
    ignore_dirs: Vec<String>,
    debounce: Duration,
    generation: Arc<AtomicU64>,
    rebuilding: Arc<Mutex<bool>>,
    compile_opts: CompileOptions,
    dist_dir: PathBuf,
) {
    thread::spawn(move || {
        let mut snapshots = collect_mtimes(&project_dir, &watch_dirs, &ignore_dirs);
        loop {
            thread::sleep(debounce);
            let next = collect_mtimes(&project_dir, &watch_dirs, &ignore_dirs);
            if next == snapshots {
                continue;
            }
            snapshots = next;
            let mut guard = rebuilding.lock().expect("rebuild lock");
            if *guard {
                continue;
            }
            *guard = true;
            drop(guard);

            eprintln!("asgard dev: 检测到变更，重新构建…");
            let mut opts = compile_opts.clone();
            opts.output_dir = Some(dist_dir.clone());
            match compile_voa_project(&opts) {
                Ok(report) => {
                    let build_gen = BUILD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                    generation.store(build_gen, Ordering::SeqCst);
                    eprintln!("asgard dev: 重建完成 (gen={build_gen}, components={}, wasm={})", report.component_count, report.wasm_built);
                }
                Err(error) => eprintln!("asgard dev: 重建失败: {error}"),
            }
            *rebuilding.lock().expect("rebuild lock") = false;
        }
    });
}

fn should_ignore(path: &Path, ignore_dirs: &[String]) -> bool {
    path.components().any(|c| if let std::path::Component::Normal(name) = c { ignore_dirs.iter().any(|ig| name == ig.as_str()) } else { false })
}

fn collect_mtimes(project_dir: &Path, watch_dirs: &[String], ignore_dirs: &[String]) -> Vec<(PathBuf, SystemTime)> {
    let mut out = Vec::new();
    for rel in watch_dirs {
        let root = project_dir.join(rel);
        if !root.exists() {
            continue;
        }
        collect_dir_mtimes(&root, ignore_dirs, &mut out);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_dir_mtimes(dir: &Path, ignore_dirs: &[String], out: &mut Vec<(PathBuf, SystemTime)>) {
    if should_ignore(dir, ignore_dirs) {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dir_mtimes(&path, ignore_dirs, out);
            continue;
        }
        if should_ignore(&path, ignore_dirs) {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(mtime) = meta.modified() {
                out.push((path, mtime));
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, dist_dir: &Path, generation: u64, generation_arc: Arc<AtomicU64>) -> Result<()> {
    let mut buffer = [0u8; 4096];
    let size = stream.read(&mut buffer).into_diagnostic()?;
    let request = String::from_utf8_lossy(&buffer[..size]);
    let mut lines = request.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let mut path = parts.next().unwrap_or("/").to_string();
    if path == "/" {
        path = "/index.html".into();
    }

    if method == "GET" && path == "/__asgard/hmr" {
        return write_response(&mut stream, 200, "application/json", &format!(r#"{{"generation":{generation}}}"#));
    }

    if method == "GET" && path == "/__asgard/hmr/events" {
        return handle_sse(&mut stream, generation_arc);
    }

    if method != "GET" && method != "HEAD" {
        return write_response(&mut stream, 405, "text/plain", "Method Not Allowed");
    }

    let file_path = sanitize_path(dist_dir, &path)?;
    if !file_path.starts_with(dist_dir) {
        return write_response(&mut stream, 403, "text/plain", "Forbidden");
    }

    if !file_path.exists() || file_path.is_dir() {
        return write_response(&mut stream, 404, "text/plain", "Not Found");
    }

    let bytes = fs::read(&file_path).into_diagnostic()?;
    let content_type = guess_content_type(&file_path);
    if method == "HEAD" {
        return write_response_headers(&mut stream, 200, content_type, bytes.len());
    }
    write_response_bytes(&mut stream, 200, content_type, &bytes)
}

fn handle_sse(stream: &mut TcpStream, generation: Arc<AtomicU64>) -> Result<()> {
    let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
    stream.write_all(header.as_bytes()).into_diagnostic()?;
    let mut last = generation.load(Ordering::SeqCst);
    let payload = format!("data: {{\"generation\":{last},\"changedModules\":[\"awsl\"]}}\n\n");
    stream.write_all(payload.as_bytes()).into_diagnostic()?;
    stream.flush().into_diagnostic()?;
    for _ in 0..120 {
        thread::sleep(Duration::from_millis(500));
        let current = generation.load(Ordering::SeqCst);
        if current != last {
            last = current;
            let msg = format!("data: {{\"generation\":{last},\"changedModules\":[\"wasm\",\"glue\"]}}\n\n");
            if stream.write_all(msg.as_bytes()).is_err() {
                break;
            }
            let _ = stream.flush();
        }
    }
    Ok(())
}

fn sanitize_path(dist_dir: &Path, url_path: &str) -> Result<PathBuf> {
    let trimmed = url_path.trim_start_matches('/');
    let candidate = dist_dir.join(trimmed);
    Ok(candidate)
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) -> Result<()> {
    write_response_bytes(stream, status, content_type, body.as_bytes())
}

fn write_response_bytes(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> Result<()> {
    let status_text = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).into_diagnostic()?;
    stream.write_all(body).into_diagnostic()?;
    Ok(())
}

fn write_response_headers(stream: &mut TcpStream, status: u16, content_type: &str, len: usize) -> Result<()> {
    let header = format!("HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
    stream.write_all(header.as_bytes()).into_diagnostic()?;
    Ok(())
}

/// 生成注入 `index.html` 的 HMR 客户端脚本（SSE 推送 + WASM 热替换）。
pub fn generate_hmr_client_script() -> String {
    r#"(function () {
  var lastGen = 0;
  var voaState = window.__voa;
  function hotSwap() {
    if (!window.__asgardWasmUrl) { location.reload(); return; }
    var saved = window.__voa;
    fetch(window.__asgardWasmUrl).then(function (r) { return r.arrayBuffer(); }).then(function (buf) {
      return WebAssembly.instantiate(buf, window.__asgardWasmImports || {});
    }).then(function (result) {
      window.__asgardWasmInstance = result.instance;
      window.__voa = saved || voaState;
      document.dispatchEvent(new CustomEvent('asgard:hmr'));
    }).catch(function () { location.reload(); });
  }
  function onPayload(payload) {
    if (!payload || typeof payload.generation !== 'number') return;
    if (lastGen === 0) { lastGen = payload.generation; return; }
    if (payload.generation !== lastGen) {
      lastGen = payload.generation;
      if (payload.changedModules && payload.changedModules.indexOf('wasm') >= 0) {
        hotSwap();
      } else {
        location.reload();
      }
    }
  }
  if (typeof EventSource !== 'undefined') {
    var es = new EventSource('/__asgard/hmr/events');
    es.onmessage = function (ev) {
      try { onPayload(JSON.parse(ev.data)); } catch (e) {}
    };
  } else {
    setInterval(function () {
      fetch('/__asgard/hmr').then(function (r) { return r.json(); }).then(onPayload).catch(function () {});
    }, 500);
  }
})();"#
        .to_string()
}
#[cfg(test)]
#[path = "../tests/path_wired/dev_server.rs"]
mod dev_server_tests;
