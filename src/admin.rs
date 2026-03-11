//! Web-based administration panel
//!
//! Provides a simple web interface for:
//! - Viewing server status
//! - Managing shares
//! - Viewing configuration
//! - Basic authentication

use crate::config::{AdminConfig, Config};
use crate::templates;
use crate::vfs::SharedVfs;
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, Response, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use minijinja::context;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Admin server
pub struct AdminServer {
    config: AdminConfig,
    app_config: Arc<RwLock<Config>>,
    running: AtomicBool,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

/// Shared state for admin handlers
#[derive(Clone)]
struct AdminState {
    config: Arc<RwLock<Config>>,
    admin_config: AdminConfig,
}

impl AdminServer {
    pub fn new(admin_config: AdminConfig, app_config: Arc<RwLock<Config>>, _vfs: SharedVfs) -> Arc<Self> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        Arc::new(Self {
            config: admin_config,
            app_config,
            running: AtomicBool::new(false),
            shutdown_tx,
            shutdown_rx,
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("Admin panel is disabled in configuration");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        tracing::info!("Starting admin panel on {}", addr);

        let state = AdminState {
            config: self.app_config.clone(),
            admin_config: self.config.clone(),
        };

        let router = Router::new()
            .route("/", get(admin_dashboard))
            .route("/shares", get(admin_shares))
            .route("/config", get(admin_config_view))
            .route("/api/status", get(api_status))
            .route("/api/shares", get(api_shares))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                basic_auth_middleware,
            ))
            .with_state(state);

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

        tracing::info!("Admin panel listening on http://{}", addr);

        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.shutdown_tx.send(true)?;
        Ok(())
    }
}

/// Basic authentication middleware
async fn basic_auth_middleware(
    State(state): State<AdminState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Basic ") {
                if let Ok(decoded) = base64_decode(&auth_str[6..]) {
                    if let Some((user, pass)) = decoded.split_once(':') {
                        if user == state.admin_config.username
                            && pass == state.admin_config.password
                        {
                            return next.run(request).await;
                        }
                    }
                }
            }
        }
    }

    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::WWW_AUTHENTICATE, "Basic realm=\"Depot Admin\"")
        .body(Body::from("Unauthorized"))
        .unwrap()
}

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

/// Dashboard page using template
async fn admin_dashboard(State(state): State<AdminState>) -> impl IntoResponse {
    let config = state.config.read().unwrap();
    
    let shares: Vec<_> = config.shares.iter()
        .map(|(name, s)| context! {
            name => name,
            path => s.path.display().to_string(),
            virtual_path => s.virtual_path,
            read_only => s.read_only,
            enabled => s.enabled,
        })
        .collect();

    let ctx = context! {
        server_name => config.server_name,
        ftp => context! {
            enabled => config.protocols.ftp.enabled,
            bind_address => config.protocols.ftp.bind_address.to_string(),
            port => config.protocols.ftp.port,
        },
        http => context! {
            enabled => config.protocols.http.enabled,
            bind_address => config.protocols.http.bind_address.to_string(),
            port => config.protocols.http.port,
        },
        shares => shares,
    };

    let html = templates::render("admin/dashboard.html", ctx)
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(html)
}

/// Shares management page using template
async fn admin_shares(State(state): State<AdminState>) -> impl IntoResponse {
    let config = state.config.read().unwrap();
    
    let shares: Vec<_> = config.shares.iter()
        .map(|(name, s)| context! {
            name => name,
            path => s.path.display().to_string(),
            virtual_path => s.virtual_path,
            read_only => s.read_only,
            enabled => s.enabled,
        })
        .collect();

    let ctx = context! {
        shares => shares,
    };

    let html = templates::render("admin/shares.html", ctx)
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(html)
}

/// Configuration view page using template
async fn admin_config_view(State(state): State<AdminState>) -> impl IntoResponse {
    let config = state.config.read().unwrap();
    let config_json = serde_json::to_string_pretty(&*config).unwrap_or_default();

    let ctx = context! {
        config_json => config_json,
    };

    let html = templates::render("admin/config.html", ctx)
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(html)
}

/// API: Get server status
#[derive(Serialize)]
struct StatusResponse {
    server_name: String,
    version: String,
    ftp_enabled: bool,
    http_enabled: bool,
    share_count: usize,
}

async fn api_status(State(state): State<AdminState>) -> impl IntoResponse {
    let config = state.config.read().unwrap();
    
    Json(StatusResponse {
        server_name: config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ftp_enabled: config.protocols.ftp.enabled,
        http_enabled: config.protocols.http.enabled,
        share_count: config.shares.iter().filter(|(_, s)| s.enabled).count(),
    })
}

/// API: Get shares list
async fn api_shares(State(state): State<AdminState>) -> impl IntoResponse {
    let config = state.config.read().unwrap();
    
    #[derive(Serialize)]
    struct ShareInfo {
        name: String,
        virtual_path: String,
        read_only: bool,
        enabled: bool,
    }
    
    let shares: Vec<ShareInfo> = config.shares.iter()
        .map(|(name, s)| ShareInfo {
            name: name.clone(),
            virtual_path: s.virtual_path.clone(),
            read_only: s.read_only,
            enabled: s.enabled,
        })
        .collect();
    
    Json(shares)
}
