//! Web panel API handlers
use axum::{
    extract::{Path, State, Request},
    http::{header, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::fs;
use crate::AppState;

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    if path == "/api/auth/login" || !path.starts_with("/api/") {
        return Ok(next.run(req).await);
    }
    if state.auth.is_no_auth() {
        return Ok(next.run(req).await);
    }
    let auth_header = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("");
    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        if state.auth.verify_token(token).is_ok() {
            return Ok(next.run(req).await);
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

pub async fn serve_index() -> Html<&'static str> { Html(INDEX_HTML) }
pub async fn serve_style() -> impl IntoResponse { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE_CSS) }
pub async fn serve_app_js() -> impl IntoResponse { ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], APP_JS) }
pub async fn serve_favicon() -> impl IntoResponse { ([(header::CONTENT_TYPE, "image/svg+xml")], FAVICON_SVG) }

#[derive(Deserialize)]
pub struct LoginReq { username: String, password: String }

pub async fn handle_login(State(state): State<AppState>, Json(body): Json<LoginReq>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.auth.login(&body.username, &body.password) {
        Ok(token) => Ok(Json(json!({ "token": token, "username": body.username }))),
        Err(_) => Err((StatusCode::UNAUTHORIZED, Json(json!({ "error": "用户名或密码错误" })))),
    }
}

pub async fn handle_me(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "username": state.auth.username(), "no_auth": state.auth.is_no_auth() }))
}

#[derive(Deserialize)]
pub struct ChangePassReq { old_password: String, new_password: String }

pub async fn handle_change_password(State(state): State<AppState>, Json(body): Json<ChangePassReq>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.auth.change_password(&body.old_password, &body.new_password) {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() })))),
    }
}

pub async fn handle_get_no_auth(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "no_auth": state.auth.is_no_auth() }))
}

#[derive(Deserialize)]
pub struct NoAuthReq { no_auth: bool }

pub async fn handle_set_no_auth(State(state): State<AppState>, Json(body): Json<NoAuthReq>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.auth.set_no_auth(body.no_auth).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true, "no_auth": body.no_auth })))
}

pub async fn handle_version(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "version": state.version, "runtime": "rust" }))
}

pub async fn handle_paths() -> Json<serde_json::Value> {
    Json(json!({
        "data_dir": crate::paths::data_dir().to_string_lossy(),
        "profiles_dir": crate::paths::profiles_dir().to_string_lossy(),
        "run_dir": crate::paths::run_dir().to_string_lossy(),
        "config_path": crate::paths::config_path().to_string_lossy(),
    }))
}

pub async fn handle_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cfg = state.store.load().unwrap_or_default();
    Json(json!({
        "enabled": cfg.config.enabled,
        "proxy_enabled": cfg.proxy.enabled,
        "core": state.core.status_json(),
        "profile": cfg.config.profile,
        "run_binary": cfg.config.run_binary,
        "tcp_mode": cfg.proxy.tcp_mode,
        "udp_mode": cfg.proxy.udp_mode,
    }))
}

pub async fn handle_get_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.store.load() {
        Ok(c) => Ok(Json(serde_json::to_value(c).unwrap_or(json!({})))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))),
    }
}

pub async fn handle_put_config(State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg: crate::config::Config = serde_json::from_value(body).map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("invalid config: {}", e) }))))?;
    state.store.save(&cfg).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    state.sched.reload(&cfg);
    state.log.app("API", "配置已保存");
    Ok(Json(json!({ "ok": true })))
}

pub async fn handle_apply_config(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state.store.load().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    state.sched.reload(&cfg);
    if cfg.config.enabled {
        state.net.cleanup(&cfg);
        match state.core.restart(&cfg).await {
            Ok(()) => {
                if let Err(e) = state.net.apply(&cfg) {
                    return Ok(Json(json!({ "ok": false, "message": format!("核心已重启，网络规则失败: {}", e) })));
                }
                Ok(Json(json!({ "ok": true, "message": "已应用并重启核心 + 网络规则" })))
            }
            Err(e) => Ok(Json(json!({ "ok": false, "message": format!("核心启动失败: {}", e) }))),
        }
    } else {
        let _ = state.core.stop().await;
        state.net.cleanup(&cfg);
        Ok(Json(json!({ "ok": true, "message": "已禁用并停止核心、清理网络规则" })))
    }
}

pub async fn handle_start(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut cfg = state.store.load().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    cfg.config.enabled = true;
    state.store.save(&cfg).ok();
    state.net.cleanup(&cfg);
    match state.core.start(&cfg).await {
        Ok(()) => {
            let msg = match state.net.apply(&cfg) {
                Ok(()) => "网络规则已应用".into(),
                Err(e) => format!("网络规则警告: {}", e),
            };
            Ok(Json(json!({ "ok": true, "message": msg })))
        }
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn handle_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mut cfg = state.store.load().unwrap_or_default();
    cfg.config.enabled = false;
    let _ = state.store.save(&cfg);
    let _ = state.core.stop().await;
    state.net.cleanup(&cfg);
    Json(json!({ "ok": true }))
}

pub async fn handle_restart(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state.store.load().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    state.net.cleanup(&cfg);
    match state.core.restart(&cfg).await {
        Ok(()) => { let _ = state.net.apply(&cfg); Ok(Json(json!({ "ok": true }))) }
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn handle_restart_core(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    handle_restart(State(state)).await
}

pub async fn handle_reload(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    handle_apply_config(State(state)).await
}

pub async fn handle_net_on(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state.store.load().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    match state.net.apply(&cfg) {
        Ok(()) => Ok(Json(json!({ "ok": true, "message": "网络规则已应用" }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn handle_net_off(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cfg = state.store.load().unwrap_or_default();
    state.net.cleanup(&cfg);
    Json(json!({ "ok": true, "message": "网络规则已清理" }))
}

pub async fn handle_list_profiles(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "profiles": state.store.list_profiles().unwrap_or_default() }))
}

pub async fn handle_upload_profile(State(state): State<AppState>, mut multipart: axum::extract::Multipart) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut name = String::new();
    let mut content = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let fname = field.name().unwrap_or("").to_string();
        if fname == "name" { name = field.text().await.unwrap_or_default(); }
        else if fname == "file" || fname == "content" {
            if let Ok(data) = field.bytes().await { content = String::from_utf8_lossy(&data).to_string(); }
        }
    }
    if name.is_empty() { name = "config.yaml".into(); }
    state.store.write_profile(&name, &content).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true, "name": name })))
}

pub async fn handle_get_profile(State(state): State<AppState>, Path(name): Path<String>) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    match state.store.read_profile(&name) {
        Ok(c) => Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], c)),
        Err(_) => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "profile not found" })))),
    }
}

#[derive(Deserialize)]
pub struct WriteProfileReq { content: String }

pub async fn handle_write_profile(State(state): State<AppState>, Path(name): Path<String>, Json(body): Json<WriteProfileReq>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.store.write_profile(&name, &body.content).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn handle_delete_profile(State(state): State<AppState>, Path(name): Path<String>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.store.delete_profile(&name).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct RenameReq { new_name: String }

pub async fn handle_rename_profile(State(state): State<AppState>, Path(name): Path<String>, Json(body): Json<RenameReq>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.store.rename_profile(&name, &body.new_name).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn handle_app_log(State(state): State<AppState>) -> impl IntoResponse {
    let content = state.log.read_app();
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], if content.is_empty() { "(empty)".into() } else { content })
}

pub async fn handle_core_log(State(state): State<AppState>) -> impl IntoResponse {
    let content = state.log.read_core();
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], if content.is_empty() { "(empty)".into() } else { content })
}

pub async fn handle_clear_app_log(State(state): State<AppState>) -> Json<serde_json::Value> {
    let _ = state.log.clear_app();
    Json(json!({ "ok": true }))
}

pub async fn handle_clear_core_log(State(state): State<AppState>) -> Json<serde_json::Value> {
    let _ = state.log.clear_core();
    Json(json!({ "ok": true }))
}

pub async fn handle_hosts() -> Json<serde_json::Value> {
    Json(serde_json::to_value(crate::hosts::collect()).unwrap_or(json!({})))
}

pub async fn handle_identifiers() -> Json<serde_json::Value> {
    Json(json!({
        "hostname": fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).unwrap_or_else(|_| "unknown".into()),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}

#[derive(Deserialize)]
pub struct DebugReq { #[serde(default)] cmd: String }

pub async fn handle_debug(State(state): State<AppState>, Json(body): Json<DebugReq>) -> Json<serde_json::Value> {
    state.log.app("Debug", &format!("cmd={}", body.cmd));
    let output = match body.cmd.as_str() {
        "ip_rule" => shell_out("ip rule show"),
        "ip_route" => shell_out("ip route show table all"),
        "nft" => shell_out("nft list table inet nexa 2>&1 || echo '(no nexa table)'"),
        "ps" => shell_out("ps aux 2>/dev/null | head -50 || ps"),
        _ => format!("unknown cmd: {} (try: ip_rule, ip_route, nft, ps)", body.cmd),
    };
    Json(json!({ "ok": true, "output": output }))
}

fn shell_out(cmd: &str) -> String {
    std::process::Command::new("sh").arg("-c").arg(cmd).output()
        .map(|o| { let mut s = String::from_utf8_lossy(&o.stdout).to_string(); s.push_str(&String::from_utf8_lossy(&o.stderr)); s })
        .unwrap_or_else(|e| e.to_string())
}
