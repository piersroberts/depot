//! HTTP server implementation using Axum
//!
//! Provides HTTP access with:
//! - Simple HTML directory listings (no JavaScript)
//! - HTTP/1.0 compatible responses
//! - Basic table layout for retro browser support
//! - Direct file downloads
//! - Optional Basic authentication

use crate::config::{HttpConfig, User};
use crate::templates;
use crate::themes;
use crate::vfs::{SharedVfs, VfsDirEntry};
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Request, Response, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
    Router,
};
use minijinja::context;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use urlencoding::encode;

/// URL-encode a path while preserving '/' separators
fn url_encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

use super::ProtocolServer;

/// HTTP server wrapper
pub struct HttpServer {
    config: HttpConfig,
    users: HashMap<String, User>,
    vfs: SharedVfs,
    running: AtomicBool,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

/// Shared state for Axum handlers
#[derive(Clone)]
struct AppState {
    vfs: SharedVfs,
    config: HttpConfig,
    users: HashMap<String, User>,
}

impl HttpServer {
    pub fn new(config: HttpConfig, users: HashMap<String, User>, vfs: SharedVfs) -> Arc<Self> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        Arc::new(Self {
            config,
            users,
            vfs,
            running: AtomicBool::new(false),
            shutdown_tx,
            shutdown_rx,
        })
    }

    fn create_router(&self) -> Router {
        let state = AppState {
            vfs: self.vfs.clone(),
            config: self.config.clone(),
            users: self.users.clone(),
        };

        let router = Router::new()
            .route("/", get(handle_root))
            .route("/*path", get(handle_path));

        // Conditionally add authentication middleware
        if self.config.require_auth {
            router
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    http_auth_middleware,
                ))
                .with_state(state)
        } else {
            router.with_state(state)
        }
    }
}

#[async_trait]
impl ProtocolServer for HttpServer {
    fn name(&self) -> &'static str {
        "HTTP"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("HTTP server is disabled in configuration");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        tracing::info!("Starting HTTP server on {}", addr);

        let router = self.create_router();
        self.running.store(true, Ordering::SeqCst);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.changed().await;
                })
                .await
                .ok();
        });

        tracing::info!("HTTP server listening on http://{}", addr);

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.shutdown_tx.send(true)?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Handle root path
async fn handle_root(State(state): State<AppState>) -> impl IntoResponse {
    handle_directory(&state, "/").await
}

/// Handle any path
async fn handle_path(State(state): State<AppState>, Path(path): Path<String>) -> impl IntoResponse {
    let virtual_path = format!("/{path}");

    // Check if it's a directory or file
    if state.vfs.is_dir(&virtual_path).await {
        handle_directory(&state, &virtual_path).await
    } else {
        handle_file(&state, &virtual_path).await
    }
}

/// Generate directory listing HTML using templates
async fn handle_directory(state: &AppState, path: &str) -> Response<Body> {
    match state.vfs.list_dir(path).await {
        Ok(entries) => {
            let html = generate_directory_html(path, entries, &state.config);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header(header::CONNECTION, "close")
                .body(Body::from(html))
                .unwrap()
        }
        Err(e) => {
            tracing::warn!("Directory listing error for {}: {}", path, e);
            error_response(StatusCode::NOT_FOUND, "Directory not found", &state.config)
        }
    }
}

/// Serve a file
async fn handle_file(state: &AppState, path: &str) -> Response<Body> {
    let physical_path = match state.vfs.resolve_path(path) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("File resolve error for {}: {}", path, e);
            return error_response(StatusCode::NOT_FOUND, "File not found", &state.config);
        }
    };

    let metadata = match tokio::fs::metadata(&physical_path).await {
        Ok(m) => m,
        Err(_) => return error_response(StatusCode::NOT_FOUND, "File not found", &state.config),
    };

    let file = match tokio::fs::File::open(&physical_path).await {
        Ok(f) => f,
        Err(_) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot open file",
                &state.config,
            )
        }
    };

    let content_type = guess_content_type(path);
    let filename = physical_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, metadata.len())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CONNECTION, "close")
        .body(body)
        .unwrap()
}

/// Generate directory listing HTML using minijinja template
fn generate_directory_html(
    path: &str,
    mut entries: Vec<VfsDirEntry>,
    config: &HttpConfig,
) -> String {
    // Sort: directories first, then by name
    entries.sort_by(|a, b| match (a.metadata.is_dir, b.metadata.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    // Convert entries to template-friendly format
    let template_entries: Vec<_> = entries
        .iter()
        .map(|e| {
            let raw_path = if e.virtual_path.starts_with('/') {
                e.virtual_path.clone()
            } else {
                format!("/{}", e.virtual_path)
            };
            let link = url_encode_path(&raw_path);

            let display_name = if e.metadata.is_dir {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            };

            context! {
                name => e.name,
                display_name => display_name,
                link => link,
                is_dir => e.metadata.is_dir,
                size => e.metadata.size,
                modified => templates::systemtime_to_timestamp(e.metadata.modified),
            }
        })
        .collect();

    let parent_path = if path != "/" {
        Some(url_encode_path(&get_parent_path(path)))
    } else {
        None
    };

    let theme = themes::get_theme(&config.theme);

    let ctx = context! {
        path => path,
        entries => template_entries,
        parent_path => parent_path,
        show_file_sizes => config.show_file_sizes,
        show_dates => config.show_dates,
        theme => theme,
        footer_message => config.footer_message,
        custom_css => config.custom_css,
        server_name => "Depot File Server",
    };

    templates::render("http/directory.html", ctx).unwrap_or_else(|e| format!("Template error: {e}"))
}

/// Create an error response using template
fn error_response(status: StatusCode, message: &str, config: &HttpConfig) -> Response<Body> {
    let theme = themes::get_theme(&config.theme);

    let ctx = context! {
        status_code => status.as_u16(),
        status_text => status.canonical_reason().unwrap_or("Error"),
        message => message,
        theme => theme,
        custom_css => config.custom_css,
        server_name => "Depot File Server",
    };

    let html = templates::render("http/error.html", ctx)
        .unwrap_or_else(|e| format!("Template error: {e}"));

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONNECTION, "close")
        .body(Body::from(html))
        .unwrap()
}

/// Get parent directory path
fn get_parent_path(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }

    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => trimmed[..pos].to_string(),
        None => "/".to_string(),
    }
}

/// Basic authentication middleware for HTTP
async fn http_auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(stripped) = auth_str.strip_prefix("Basic ") {
                if let Ok(decoded) = base64_decode(stripped) {
                    if let Some((username, password)) = decoded.split_once(':') {
                        // Check against configured users
                        if let Some(user) = state.users.get(username) {
                            if user.enabled && user.verify_password(password) {
                                return next.run(request).await;
                            }
                        }
                    }
                }
            }
        }
    }

    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"Depot File Server\"",
        )
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("Unauthorized - Please log in"))
        .unwrap()
}

/// Decode base64 string
fn base64_decode(input: &str) -> Result<String, ()> {
    use std::collections::HashMap;

    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let decode_map: HashMap<char, u8> = alphabet
        .chars()
        .enumerate()
        .map(|(i, c)| (c, i as u8))
        .collect();

    let input = input.trim_end_matches('=');
    let mut result = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;

    for c in input.chars() {
        if let Some(&val) = decode_map.get(&c) {
            buffer = (buffer << 6) | val as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                result.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
    }

    String::from_utf8(result).map_err(|_| ())
}

/// Guess content type from file extension
fn guess_content_type(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        // Text
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",

        // Images
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",

        // Audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mod" => "audio/mod",
        "s3m" => "audio/s3m",
        "xm" => "audio/xm",
        "it" => "audio/it",

        // Video
        "mp4" => "video/mp4",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",

        // Archives
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "rar" => "application/x-rar-compressed",
        "7z" => "application/x-7z-compressed",
        "lha" | "lzh" => "application/x-lzh-compressed",
        "dms" => "application/x-dms",
        "adf" => "application/x-amiga-disk-format",

        // Executables and disk images
        "exe" => "application/x-msdownload",
        "iso" => "application/x-iso9660-image",
        "img" => "application/octet-stream",
        "bin" => "application/octet-stream",
        "rom" => "application/octet-stream",

        // Documents
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "rtf" => "application/rtf",

        // Default
        _ => "application/octet-stream",
    }
}
