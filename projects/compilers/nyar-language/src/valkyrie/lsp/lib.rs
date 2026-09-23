#![doc = include_str!("readme.md")]
#![warn(missing_docs)]
#![feature(new_range_api)]

use oak_lsp::LspServer;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

pub mod backend;
mod capabilities;
mod diagnostics;
mod errors;
mod handlers;
mod legion;
mod state;
pub mod types;

pub use backend::ValkyrieLanguageService;

/// 日志级别配置
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogLevel {
    /// 关闭日志
    Off,
    /// 错误级别
    Error,
    /// 警告级别
    Warn,
    /// 信息级别（默认）
    #[default]
    Info,
    /// 调试级别
    Debug,
    /// 跟踪级别
    Trace,
}

impl LogLevel {
    /// 转换为 tracing::Level
    pub fn to_tracing_level(self) -> Option<Level> {
        match self {
            LogLevel::Off => None,
            LogLevel::Error => Some(Level::ERROR),
            LogLevel::Warn => Some(Level::WARN),
            LogLevel::Info => Some(Level::INFO),
            LogLevel::Debug => Some(Level::DEBUG),
            LogLevel::Trace => Some(Level::TRACE)
        }
    }

    /// 从字符串解析日志级别
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "off" => Some(LogLevel::Off),
            "error" => Some(LogLevel::Error),
            "warn" | "warning" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Off => write!(f, "off"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Trace => write!(f, "trace"),
        }
    }
}

/// LSP 服务器启动配置
#[derive(Debug, Clone, Default)]
pub struct LspOptions {
    /// 是否使用标准输入输出模式
    pub stdio: bool,
    /// TCP 模式下的端口（如果提供则启用 TCP 模式）
    pub tcp_port: Option<u16>,
    /// 日志级别配置
    pub log_level: LogLevel,
    /// 日志输出文件路径（可选）
    pub log_file: Option<std::path::PathBuf>,
}

/// 初始化日志订阅器
fn init_logging(options: &LspOptions) -> Result<(), Box<dyn std::error::Error>> {
    if options.log_level == LogLevel::Off {
        return Ok(());
    }

    let level = options.log_level.to_tracing_level();

    if let Some(lvl) = level {
        if let Some(ref log_file) = options.log_file {
            let file = std::fs::File::create(log_file)?;
            let subscriber = FmtSubscriber::builder()
                .with_max_level(lvl)
                .with_writer(file)
                .with_ansi(false)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| format!("Failed to set tracing subscriber: {}", e))?;
        } else {
            let subscriber = FmtSubscriber::builder()
                .with_max_level(lvl)
                .with_writer(std::io::stderr)
                .with_ansi(true)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| format!("Failed to set tracing subscriber: {}", e))?;
        }
    }

    Ok(())
}

/// 启动 LSP 服务器
pub async fn start_lsp_server(options: LspOptions) -> Result<(), Box<dyn std::error::Error>> {
    init_logging(&options)?;

    info!("Starting Valkyrie LSP Server (log level: {})", options.log_level);

    if let Some(port) = options.tcp_port {
        start_tcp_server(port).await
    } else {
        start_stdio_server().await
    }
}

/// 启动基于标准输入输出的 LSP 服务器
pub async fn start_stdio_server() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let backend = Arc::new(ValkyrieLanguageService::new());
    let server = LspServer::new(backend);

    info!("Valkyrie LSP Server started on stdio");
    server.run(stdin, stdout).await?;

    Ok(())
}

/// 启动基于 TCP 的 LSP 服务器（用于调试）
pub async fn start_tcp_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    info!("Valkyrie LSP Server listening on 127.0.0.1:{}", port);

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("New connection from {}", addr);

        tokio::spawn(handle_connection(stream));
    }
}

/// 处理单个 TCP 连接
async fn handle_connection(stream: TcpStream) {
    let (read, write) = tokio::io::split(stream);

    let backend = Arc::new(ValkyrieLanguageService::new());
    let server = LspServer::new(backend);

    if let Err(e) = server.run(read, write).await {
        warn!("Error handling connection: {}", e);
    }
}
