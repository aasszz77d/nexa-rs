//! Nexa - Transparent proxy management panel (Rust rewrite)
//!
//! Independent daemon, run with `./nexa`, default listen :9990.
//! Provides HTTP API + embedded Web panel. No dependency on luci/rpcd/ubus/UCI.
//!
//! Subcommands:
//!   nexa on   - Apply firewall/routing rules (stub) then exit
//!   nexa off  - Cleanup firewall/routing rules (stub) then exit

mod auth;
mod config;
mod core;
mod paths;
mod store;
mod web;

use axum::{
    routing::{get, post, put, delete},
    Router,
    middleware,
};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::AuthState;
use crate::store::Store;
use crate::core::CoreManager;

#[derive(Parser, Debug)]
#[command(name = "nexa", about = "Nexa transparent proxy management panel")]
struct Cli {
    /// HTTP listen address
    #[arg(long, default_value = "0.0.0.0:9990")]
    addr: String,

    /// Web listen port (overrides --addr port if set)
    #[arg(short = 'p', long)]
    port: Option<u16>,

    /// Data directory (default: /etc/nexa or ./data)
    #[arg(short = 'd', long)]
    data_dir: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Apply network rules (stub) and exit
    On,
    /// Cleanup network rules (stub) and exit
    Off,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub auth: Arc<AuthState>,
    pub core: Arc<CoreManager>,
    pub version: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "nexa=info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    paths::init(cli.data_dir.as_deref());

    match cli.command {
        Some(Commands::On) => {
            tracing::info!("Applying network rules (stub)...");
            println!("nexa on: network rules applied (stub)");
            return Ok(());
        }
        Some(Commands::Off) => {
            tracing::info!("Cleaning network rules (stub)...");
            println!("nexa off: network rules cleaned (stub)");
            return Ok(());
        }
        None => {}
    }

    let store = Arc::new(Store::open()?);
    let auth = Arc::new(AuthState::new(store.clone())?);
    let core = Arc::new(CoreManager::new(store.clone()));

    let state = AppState {
        store: store.clone(),
        auth: auth.clone(),
        core: core.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // Boot: if enabled, try start
    if let Ok(cfg) = store.load() {
        if cfg.config.enabled {
            tracing::info!("Config enabled, starting core...");
            if let Err(e) = core.start(&cfg).await {
                tracing::warn!("Core start failed: {}", e);
            }
        }
    }

    let app = build_router(state);

    let mut addr: SocketAddr = cli.addr.parse()?;
    if let Some(p) = cli.port {
        addr.set_port(p);
    }

    tracing::info!("Nexa listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(state: AppState) -> Router {
    use crate::web;

    let api = Router::new()
        // Auth (no middleware)
        .route("/api/auth/login", post(web::handle_login))
        // Protected
        .route("/api/auth/me", get(web::handle_me))
        .route("/api/auth/password", put(web::handle_change_password))
        .route("/api/auth/no-auth", get(web::handle_get_no_auth).put(web::handle_set_no_auth))
        .route("/api/version", get(web::handle_version))
        .route("/api/paths", get(web::handle_paths))
        .route("/api/status", get(web::handle_status))
        .route("/api/config", get(web::handle_get_config).put(web::handle_put_config))
        .route("/api/config/apply", post(web::handle_apply_config))
        .route("/api/start", post(web::handle_start))
        .route("/api/stop", post(web::handle_stop))
        .route("/api/restart", post(web::handle_restart))
        .route("/api/restart-core", post(web::handle_restart_core))
        .route("/api/reload", post(web::handle_reload))
        .route("/api/profiles", get(web::handle_list_profiles).post(web::handle_upload_profile))
        .route("/api/profiles/:name", get(web::handle_get_profile).put(web::handle_write_profile).delete(web::handle_delete_profile))
        .route("/api/profiles/:name/rename", post(web::handle_rename_profile))
        .route("/api/logs/app", get(web::handle_app_log))
        .route("/api/logs/core", get(web::handle_core_log))
        .route("/api/logs/app/clear", post(web::handle_clear_app_log))
        .route("/api/logs/core/clear", post(web::handle_clear_core_log))
        .route("/api/hosts", get(web::handle_hosts))
        .route("/api/identifiers", get(web::handle_identifiers))
        .layer(middleware::from_fn_with_state(state.clone(), web::auth_middleware));

    Router::new()
        .merge(api)
        // Static / SPA fallback – all from single web.rs
        .route("/", get(web::serve_index))
        .route("/index.html", get(web::serve_index))
        .route("/style.css", get(web::serve_style))
        .route("/app.js", get(web::serve_app_js))
        .route("/favicon.svg", get(web::serve_favicon))
        .fallback(get(web::serve_index))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
