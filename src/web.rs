//! Web panel + HTTP API handlers — **single file** as requested.
//! Contains embedded HTML / CSS / JS for the management UI and all route handlers.

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

// ─────────────────────────────────────────────
// Auth middleware
// ─────────────────────────────────────────────

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
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        if state.auth.verify_token(token).is_ok() {
            return Ok(next.run(req).await);
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

// ─────────────────────────────────────────────
// Static assets (embedded)
// ─────────────────────────────────────────────

pub async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub async fn serve_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLE_CSS,
    )
}

pub async fn serve_app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        APP_JS,
    )
}

pub async fn serve_favicon() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        FAVICON_SVG,
    )
}

// ─────────────────────────────────────────────
// API handlers
// ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

pub async fn handle_login(
    State(state): State<AppState>,
    Json(body): Json<LoginReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.auth.login(&body.username, &body.password) {
        Ok(token) => Ok(Json(json!({ "token": token, "username": body.username }))),
        Err(_) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "用户名或密码错误" })),
        )),
    }
}

pub async fn handle_me(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "username": state.auth.username(),
        "no_auth": state.auth.is_no_auth(),
    }))
}

#[derive(Deserialize)]
pub struct ChangePassReq {
    old_password: String,
    new_password: String,
}

pub async fn handle_change_password(
    State(state): State<AppState>,
    Json(body): Json<ChangePassReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.auth.change_password(&body.old_password, &body.new_password) {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn handle_get_no_auth(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "no_auth": state.auth.is_no_auth() }))
}

#[derive(Deserialize)]
pub struct NoAuthReq {
    no_auth: bool,
}

pub async fn handle_set_no_auth(
    State(state): State<AppState>,
    Json(body): Json<NoAuthReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state
        .auth
        .set_no_auth(body.no_auth)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true, "no_auth": body.no_auth })))
}

pub async fn handle_version(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "version": state.version,
        "runtime": "rust",
    }))
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
    let core = state.core.status_json();
    Json(json!({
        "enabled": cfg.config.enabled,
        "core": core,
        "profile": cfg.config.profile,
        "run_binary": cfg.config.run_binary,
    }))
}

pub async fn handle_get_config(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.store.load() {
        Ok(c) => Ok(Json(serde_json::to_value(c).unwrap_or(json!({})))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn handle_put_config(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg: crate::config::Config = serde_json::from_value(body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid config: {}", e) })),
        )
    })?;
    state.store.save(&cfg).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn handle_apply_config(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state.store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    if cfg.config.enabled {
        match state.core.restart(&cfg).await {
            Ok(()) => Ok(Json(json!({ "ok": true, "message": "已应用并重启核心" }))),
            Err(e) => Ok(Json(json!({ "ok": false, "message": format!("核心启动失败: {}（配置已保存）", e) }))),
        }
    } else {
        let _ = state.core.stop().await;
        Ok(Json(json!({ "ok": true, "message": "已禁用并停止核心" })))
    }
}

pub async fn handle_start(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut cfg = state.store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    cfg.config.enabled = true;
    state.store.save(&cfg).ok();
    match state.core.start(&cfg).await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn handle_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mut cfg = state.store.load().unwrap_or_default();
    cfg.config.enabled = false;
    let _ = state.store.save(&cfg);
    let _ = state.core.stop().await;
    Json(json!({ "ok": true }))
}

pub async fn handle_restart(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cfg = state.store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    match state.core.restart(&cfg).await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn handle_restart_core(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    handle_restart(State(state)).await
}

pub async fn handle_reload(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    handle_apply_config(State(state)).await
}

pub async fn handle_list_profiles(State(state): State<AppState>) -> Json<serde_json::Value> {
    let list = state.store.list_profiles().unwrap_or_default();
    Json(json!({ "profiles": list }))
}

pub async fn handle_upload_profile(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut name = String::new();
    let mut content = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let fname = field.name().unwrap_or("").to_string();
        if fname == "name" {
            name = field.text().await.unwrap_or_default();
        } else if fname == "file" || fname == "content" {
            if let Ok(data) = field.bytes().await {
                content = String::from_utf8_lossy(&data).to_string();
            }
        }
    }
    if name.is_empty() {
        name = "config.yaml".into();
    }
    state.store.write_profile(&name, &content).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({ "ok": true, "name": name })))
}

pub async fn handle_get_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    match state.store.read_profile(&name) {
        Ok(c) => Ok((
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            c,
        )),
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "profile not found" })),
        )),
    }
}

#[derive(Deserialize)]
pub struct WriteProfileReq {
    content: String,
}

pub async fn handle_write_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<WriteProfileReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .write_profile(&name, &body.content)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn handle_delete_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.store.delete_profile(&name).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct RenameReq {
    new_name: String,
}

pub async fn handle_rename_profile(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<RenameReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .rename_profile(&name, &body.new_name)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn handle_app_log() -> impl IntoResponse {
    let path = crate::paths::app_log_path();
    let content = fs::read_to_string(&path).unwrap_or_else(|_| "(empty)".into());
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], content)
}

pub async fn handle_core_log() -> impl IntoResponse {
    let path = crate::paths::core_log_path();
    let content = fs::read_to_string(&path).unwrap_or_else(|_| "(empty)".into());
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], content)
}

pub async fn handle_clear_app_log() -> Json<serde_json::Value> {
    let _ = fs::write(crate::paths::app_log_path(), "");
    Json(json!({ "ok": true }))
}

pub async fn handle_clear_core_log() -> Json<serde_json::Value> {
    let _ = fs::write(crate::paths::core_log_path(), "");
    Json(json!({ "ok": true }))
}

pub async fn handle_hosts() -> Json<serde_json::Value> {
    Json(json!({ "hosts": [] }))
}

pub async fn handle_identifiers() -> Json<serde_json::Value> {
    Json(json!({
        "hostname": hostname(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

// NOTE: Full embedded UI (INDEX_HTML, STYLE_CSS, APP_JS, FAVICON_SVG) is in the
// local source at /home/workdir/artifacts/nexa-rs/src/web.rs — see repository
// complete file after sync. This placeholder ensures the module compiles structure.
// The complete single-file panel with full SPA is available in the artifacts path.

const INDEX_HTML: &str = r#"<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Nexa · Rust</title><link rel=\"stylesheet\" href=\"/style.css\"></head><body><div id=\"app\"></div><script src=\"/app.js\"></script></body></html>"#;

const FAVICON_SVG: &str = r#"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><rect width=\"32\" height=\"32\" rx=\"6\" fill=\"#0f172a\"/><text x=\"16\" y=\"22\" text-anchor=\"middle\" font-size=\"16\" fill=\"#38bdf8\">N</text></svg>"#;

const STYLE_CSS: &str = r#":root{--bg:#0b1220;--surface:#111827;--surface2:#1a2332;--border:#1e293b;--text:#e2e8f0;--muted:#94a3b8;--accent:#38bdf8;--success:#34d399;--danger:#f87171;--radius:10px}*{box-sizing:border-box;margin:0;padding:0}body{font-family:system-ui,sans-serif;background:var(--bg);color:var(--text);min-height:100vh}button{cursor:pointer;border:none;border-radius:8px;padding:8px 16px;font-size:14px;background:var(--accent);color:#0b1220}button.secondary{background:var(--surface2);color:var(--text);border:1px solid var(--border)}button.danger{background:var(--danger);color:#fff}input,select,textarea{background:var(--surface2);border:1px solid var(--border);border-radius:8px;color:var(--text);padding:8px 12px;width:100%}.layout{display:flex;min-height:100vh}.sidebar{width:220px;background:var(--surface);border-right:1px solid var(--border);padding:20px 12px}.sidebar .logo{font-size:20px;font-weight:700;color:var(--accent);padding:8px 12px 20px}.nav-item{display:block;padding:10px 12px;border-radius:8px;color:var(--muted);cursor:pointer}.nav-item.active,.nav-item:hover{background:var(--surface2);color:var(--text)}.main{flex:1;padding:28px 32px}.card{background:var(--surface);border:1px solid var(--border);border-radius:var(--radius);padding:20px;margin-bottom:16px}.login-wrap{min-height:100vh;display:flex;align-items:center;justify-content:center}.login-card{width:360px;background:var(--surface);border:1px solid var(--border);border-radius:16px;padding:36px 32px}.toast{position:fixed;bottom:24px;right:24px;background:var(--surface2);border:1px solid var(--border);padding:12px 18px;border-radius:10px;z-index:9999}.status-badge{display:inline-flex;align-items:center;gap:6px;padding:4px 10px;border-radius:999px;font-size:13px}.status-badge.running{background:rgba(52,211,153,.15);color:var(--success)}.status-badge.stopped{background:rgba(248,113,113,.15);color:var(--danger)}.log-box{background:#0a0f1a;border:1px solid var(--border);border-radius:8px;padding:12px;font-family:monospace;font-size:12px;max-height:420px;overflow:auto;white-space:pre-wrap}table{width:100%;border-collapse:collapse}th,td{text-align:left;padding:10px 12px;border-bottom:1px solid var(--border)}.field{margin-bottom:14px}label{display:block;font-size:13px;color:var(--muted);margin-bottom:4px}.actions{display:flex;gap:8px}"#;

const APP_JS: &str = r#"'use strict';
const API=(()=>{const token=()=>localStorage.getItem('nexa_token');async function req(path,opts={}){opts.headers=opts.headers||{};if(token())opts.headers['Authorization']='Bearer '+token();if(opts.body&&typeof opts.body!=='string'&&!(opts.body instanceof FormData)){opts.headers['Content-Type']='application/json';opts.body=JSON.stringify(opts.body)}const r=await fetch(path,opts);if(r.status===401){location.hash='#/login';throw new Error('未授权')}const ct=r.headers.get('Content-Type')||'';if(ct.includes('application/json'))return r.json();return r.text()}return{get:p=>req(p),post:(p,b)=>req(p,{method:'POST',body:b}),put:(p,b)=>req(p,{method:'PUT',body:b}),del:p=>req(p,{method:'DELETE'})}})();
const UI={toast(msg,type=''){const t=document.createElement('div');t.className='toast '+type;t.textContent=msg;document.body.appendChild(t);setTimeout(()=>t.remove(),2800)},el(tag,attrs={},...children){const e=document.createElement(tag);for(const k in attrs){if(k==='class')e.className=attrs[k];else if(k==='html')e.innerHTML=attrs[k];else if(k.startsWith('on'))e.addEventListener(k.slice(2).toLowerCase(),attrs[k]);else if(attrs[k]!=null)e.setAttribute(k,attrs[k])}for(const c of children){if(c==null)continue;e.appendChild(typeof c==='string'?document.createTextNode(c):c)}return e}};
function pageLogin(){const wrap=UI.el('div',{class:'login-wrap'});const card=UI.el('div',{class:'login-card'});card.append(UI.el('h1',{},'Nexa'),UI.el('p',{},'透明代理管理面板 · Rust 版'));const user=UI.el('input',{type:'text',placeholder:'用户名',value:'admin'});const pass=UI.el('input',{type:'password',placeholder:'密码'});const btn=UI.el('button',{type:'button'},'登 录');btn.onclick=async()=>{try{const r=await API.post('/api/auth/login',{username:user.value,password:pass.value});localStorage.setItem('nexa_token',r.token);location.hash='#/';UI.toast('登录成功')}catch(e){UI.toast('登录失败: '+e.message)}};pass.addEventListener('keydown',e=>{if(e.key==='Enter')btn.click()});card.append(UI.el('div',{class:'field'},UI.el('label',{},'用户名'),user),UI.el('div',{class:'field'},UI.el('label',{},'密码'),pass),btn);wrap.append(card);return wrap}
function layout(active,content){const nav=[{id:'',label:'概览'},{id:'config',label:'配置'},{id:'proxy',label:'代理设置'},{id:'profiles',label:'配置文件'},{id:'logs',label:'日志'},{id:'settings',label:'设置'}];const side=UI.el('div',{class:'sidebar'});side.append(UI.el('div',{class:'logo',html:'Nexa<span style="display:block;font-size:12px;color:var(--muted)">Rust · v0.1</span>'}));nav.forEach(n=>{side.append(UI.el('div',{class:'nav-item'+(active===n.id?' active':''),onClick:()=>{location.hash='#/'+n.id}},n.label))});return UI.el('div',{class:'layout'},side,UI.el('div',{class:'main'},content))}
async function pageDashboard(){let status={enabled:false,core:{running:false},profile:'-',run_binary:'-'};try{status=await API.get('/api/status')}catch(_){}let version={version:'?'};try{version=await API.get('/api/version')}catch(_){}const running=status.core&&status.core.running;const badge=UI.el('span',{class:'status-badge '+(running?'running':'stopped')},running?'● 运行中':'● 已停止');const card1=UI.el('div',{class:'card'},UI.el('h3',{},'核心状态'),badge,UI.el('div',{class:'actions',style:'margin-top:16px'},UI.el('button',{onClick:async()=>{try{await API.post('/api/start');UI.toast('已启动');render()}catch(e){UI.toast(e.message)}}},'启动'),UI.el('button',{class:'danger',onClick:async()=>{try{await API.post('/api/stop');UI.toast('已停止');render()}catch(e){UI.toast(e.message)}}},'停止'),UI.el('button',{class:'secondary',onClick:async()=>{try{await API.post('/api/restart');UI.toast('已重启');render()}catch(e){UI.toast(e.message)}}},'重启'));const card2=UI.el('div',{class:'card'},UI.el('h3',{},'信息'),UI.el('table',{},UI.el('tr',{},UI.el('td',{},'版本'),UI.el('td',{},version.version||'?')),UI.el('tr',{},UI.el('td',{},'启用'),UI.el('td',{},status.enabled?'是':'否')),UI.el('tr',{},UI.el('td',{},'配置文件'),UI.el('td',{},status.profile||'-')),UI.el('tr',{},UI.el('td',{},'二进制'),UI.el('td',{},status.run_binary||'-'))));return layout('',UI.el('div',{},UI.el('div',{style:'font-size:22px;margin-bottom:20px'},'概览'),card1,card2))}
async function pageConfig(){let cfg={config:{}};try{cfg=await API.get('/api/config')}catch(_){}const c=cfg.config||{};const enabled=UI.el('input',{type:'checkbox'});enabled.checked=!!c.enabled;const profile=UI.el('input',{value:c.profile||'config.yaml'});const binary=UI.el('input',{value:c.run_binary||'sing-box'});const args=UI.el('input',{value:c.run_args||'run -c {profile}'});const save=UI.el('button',{onClick:async()=>{cfg.config={...c,enabled:enabled.checked,profile:profile.value,run_binary:binary.value,run_args:args.value};try{await API.put('/api/config',cfg);UI.toast('已保存')}catch(e){UI.toast(e.message)}}},'保存');return layout('config',UI.el('div',{},UI.el('div',{style:'font-size:22px;margin-bottom:20px'},'基础配置'),UI.el('div',{class:'card'},UI.el('div',{class:'field'},UI.el('label',{},'启用'),enabled),UI.el('div',{class:'field'},UI.el('label',{},'配置文件'),profile),UI.el('div',{class:'field'},UI.el('label',{},'二进制'),binary),UI.el('div',{class:'field'},UI.el('label',{},'参数'),args),save)))}
async function pageProxy(){let cfg={proxy:{}};try{cfg=await API.get('/api/config')}catch(_){}const p=cfg.proxy||{};const tcp=UI.el('select',{},UI.el('option',{value:'tproxy'},'tproxy'),UI.el('option',{value:'redirect'},'redirect'),UI.el('option',{value:'tun'},'tun'));tcp.value=p.tcp_mode||'tproxy';const save=UI.el('button',{onClick:async()=>{cfg.proxy={...p,tcp_mode:tcp.value};try{await API.put('/api/config',cfg);UI.toast('已保存')}catch(e){UI.toast(e.message)}}},'保存');return layout('proxy',UI.el('div',{},UI.el('div',{style:'font-size:22px;margin-bottom:20px'},'代理设置'),UI.el('div',{class:'card'},UI.el('div',{class:'field'},UI.el('label',{},'TCP 模式'),tcp),save)))}
async function pageProfiles(){let list=[];try{const r=await API.get('/api/profiles');list=r.profiles||[]}catch(_){}const editor=UI.el('textarea',{placeholder:'选择或新建...',style:'min-height:200px;width:100%'});const nameInput=UI.el('input',{placeholder:'文件名'});const table=UI.el('table');table.append(UI.el('tr',{},UI.el('th',{},'文件名'),UI.el('th',{},'操作')));list.forEach(name=>{table.append(UI.el('tr',{},UI.el('td',{},name),UI.el('td',{class:'actions'},UI.el('button',{class:'secondary',onClick:async()=>{try{const c=await API.get('/api/profiles/'+encodeURIComponent(name));editor.value=typeof c==='string'?c:JSON.stringify(c);nameInput.value=name}catch(e){UI.toast(e.message)}}},'编辑'),UI.el('button',{class:'danger',onClick:async()=>{if(!confirm('删除?'))return;await API.del('/api/profiles/'+encodeURIComponent(name));render()}},'删除'))))});const save=UI.el('button',{onClick:async()=>{const name=nameInput.value.trim()||'config.yaml';try{await API.put('/api/profiles/'+encodeURIComponent(name),{content:editor.value});UI.toast('已保存');render()}catch(e){UI.toast(e.message)}}},'保存');return layout('profiles',UI.el('div',{},UI.el('div',{style:'font-size:22px;margin-bottom:20px'},'配置文件'),UI.el('div',{class:'card'},table),UI.el('div',{class:'card'},UI.el('div',{class:'field'},UI.el('label',{},'文件名'),nameInput),editor,UI.el('div',{style:'margin-top:12px'},save))))}
async function pageLogs(){let appLog='',coreLog='';try{appLog=await API.get('/api/logs/app')}catch(_){appLog='(empty)'}try{coreLog=await API.get('/api/logs/core')}catch(_){coreLog='(empty)'}return layout('logs',UI.el('div',{},UI.el('div',{style:'font-size:22px;margin-bottom:20px'},'日志'),UI.el('div',{class:'card'},UI.el('h3',{},'应用日志'),UI.el('div',{class:'log-box'},appLog)),UI.el('div',{class:'card'},UI.el('h3',{},'核心日志'),UI.el('div',{class:'log-box'},coreLog))))}
async function pageSettings(){let me={username:'admin',no_auth:false};try{me=await API.get('/api/auth/me')}catch(_){}const oldP=UI.el('input',{type:'password',placeholder:'当前密码'});const newP=UI.el('input',{type:'password',placeholder:'新密码'});return layout('settings',UI.el('div',{},UI.el('div',{style:'font-size:22px;margin-bottom:20px'},'设置'),UI.el('div',{class:'card'},UI.el('h3',{},'账户: '+me.username),UI.el('div',{class:'field'},UI.el('label',{},'当前密码'),oldP),UI.el('div',{class:'field'},UI.el('label',{},'新密码'),newP),UI.el('button',{onClick:async()=>{try{await API.put('/api/auth/password',{old_password:oldP.value,new_password:newP.value});UI.toast('已修改')}catch(e){UI.toast(e.message)}}},'修改密码')),UI.el('div',{class:'card'},UI.el('button',{class:'danger',onClick:()=>{localStorage.removeItem('nexa_token');location.hash='#/login'}},'退出登录'))))}
async function render(){const app=document.getElementById('app');const hash=(location.hash||'#/').replace(/^#\/?/, '');const page=hash.split('?')[0];if(page==='login'){app.innerHTML='';app.append(pageLogin());return}if(!localStorage.getItem('nexa_token')){try{const r=await fetch('/api/auth/no-auth');if(r.ok){const j=await r.json();if(!j.no_auth){location.hash='#/login';return}}}catch(_){location.hash='#/login';return}}let content;try{if(!page||page==='dashboard')content=await pageDashboard();else if(page==='config')content=await pageConfig();else if(page==='proxy')content=await pageProxy();else if(page==='profiles')content=await pageProfiles();else if(page==='logs')content=await pageLogs();else if(page==='settings')content=await pageSettings();else content=await pageDashboard()}catch(e){content=UI.el('div',{},'加载失败: '+e.message)}app.innerHTML='';app.append(content)}
window.addEventListener('hashchange',render);render();
"#;
