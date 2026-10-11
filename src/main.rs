//! Nexa - Transparent proxy management panel (Rust rewrite)
//!
//! Independent daemon, run with `./nexa`, default listen :9990.
//! Provides HTTP API + embedded Web panel. No dependency on luci/rpcd/ubus/UCI.
//!
//! Subcommands:
//!   nexa on   - Apply firewall/routing rules then exit
//!   nexa off  - Cleanup firewall/routing rules then exit

mod auth;
mod config;
mod core;
mod hosts;
mod logger;
mod netmanager;
mod paths;
mod scheduler;
mod store;
mod web;

use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::AuthState;
use crate::core::CoreManager;
use crate::logger::Logger;
use crate::netmanager::NetManager;
use crate::scheduler::Scheduler;
use crate::store::Store;

#[derive(Parser, Debug)]
#[command(name = "nexa", about = "Nexa transparent proxy management panel")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:9990")]
    addr: String,
    #[arg(short = 'p', long)]
    port: Option<u16>,
    #[arg(short = 'd', long)]
    data_dir: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    On,
    Off,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub auth: Arc<AuthState>,
    pub core: Arc<CoreManager>,
    pub net: Arc<NetManager>,
    pub log: Arc<Logger>,
    pub sched: Arc<Scheduler>,
    pub version: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nexa=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    paths::init(cli.data_dir.as_deref());

    let log = Logger::new()?;
    let store = Arc::new(Store::open()?);
    let net = Arc::new(NetManager::new(log.clone()));

    match cli.command {
        Some(Commands::On) => {
            let cfg = store.load().unwrap_or_default();
            log.app("CLI", "执行 nexa on…");
            match net.apply(&cfg) {
                Ok(()) => {
                    println!("nexa on: network rules applied");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("nexa on failed: {}", e);
                    Err(e)
                }
            }
        }
        Some(Commands::Off) => {
            let cfg = store.load().unwrap_or_default();
            log.app("CLI", "执行 nexa off…");
            net.cleanup(&cfg);
            println!("nexa off: network rules cleaned");
            Ok(())
        }
        None => run_server(cli, store, log, net).await,
    }
}

async fn run_server(
    cli: Cli,
    store: Arc<Store>,
    log: Arc<Logger>,
    net: Arc<NetManager>,
) -> anyhow::Result<()> {
    let auth = Arc::new(AuthState::new(store.clone())?);
    let core = Arc::new(CoreManager::new(store.clone(), log.clone()));
    let sched = Scheduler::new(log.clone(), core.clone(), store.clone());

    let state = AppState {
        store: store.clone(),
        auth: auth.clone(),
        core: core.clone(),
        net: net.clone(),
        log: log.clone(),
        sched: sched.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    sched.start();
    if let Ok(cfg) = store.load() {
        sched.reload(&cfg);
        if cfg.config.enabled {
            log.app("App", "配置已启用，启动中…");
            if cfg.config.start_delay > 0 {
                log.app("App", &format!("延迟 {} 秒后启动。", cfg.config.start_delay));
                tokio::time::sleep(std::time::Duration::from_secs(cfg.config.start_delay as u64)).await;
            }
            net.cleanup(&cfg);
            if let Err(e) = core.start(&cfg).await {
                log.app("App", &format!("核心启动失败: {}", e));
            } else if let Err(e) = net.apply(&cfg) {
                log.app("App", &format!("网络规则应用失败: {}", e));
            }
        } else {
            log.app("App", "已禁用，不启动核心。");
        }
    }

    let app = build_router(state);
    let mut addr: SocketAddr = cli.addr.parse()?;
    if let Some(p) = cli.port {
        addr.set_port(p);
    }

    log.app("App", &format!("监听 http://{}", addr));
    tracing::info!("Nexa listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(state: AppState) -> Router {
    use crate::web;

    let api = Router::new()
        .route("/api/auth/login", post(web::handle_login))
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
        .route("/api/net/on", post(web::handle_net_on))
        .route("/api/net/off", post(web::handle_net_off))
        .route("/api/profiles", get(web::handle_list_profiles).post(web::handle_upload_profile))
        .route("/api/profiles/:name", get(web::handle_get_profile).put(web::handle_write_profile).delete(web::handle_delete_profile))
        .route("/api/profiles/:name/rename", post(web::handle_rename_profile))
        .route("/api/logs/app", get(web::handle_app_log))
        .route("/api/logs/core", get(web::handle_core_log))
        .route("/api/logs/app/clear", post(web::handle_clear_app_log))
        .route("/api/logs/core/clear", post(web::handle_clear_core_log))
        .route("/api/hosts", get(web::handle_hosts))
        .route("/api/identifiers", get(web::handle_identifiers))
        .route("/api/debug", post(web::handle_debug))
        .layer(middleware::from_fn_with_state(state.clone(), web::auth_middleware));

    Router::new()
        .merge(api)
        .route("/", get(web::serve_index))
        .route("/index.html", get(web::serve_index))
        .route("/style.css", get(web::serve_style))
        .route("/app.js", get(web::serve_app_js))
        .route("/favicon.svg", get(web::serve_favicon))
        .fallback(get(web::serve_index))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
