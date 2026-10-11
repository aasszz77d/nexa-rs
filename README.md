# Nexa (Rust)

透明代理管理面板的 Rust 重写版本。

独立守护进程，`./nexa` 即可运行，默认监听 `:9990`。  
提供 HTTP API + 内嵌 Web 面板，不依赖 luci / rpcd / ubus / UCI。

## 特性

- **单文件网页面板**：`src/web.rs` 包含全部 HTML/CSS/JS + API 处理器
- JWT 认证 + bcrypt 密码（默认 `admin` / `admin`）
- 配置持久化（JSON）+ profiles 管理
- 代理核心进程管理（sing-box / mihomo / xray 等）
- **网络规则管理**：tproxy / tun 路由、nftables DNS 劫持（`nexa on` / `nexa off`）
- **定时调度**：定时重启核心、定时清空日志（内置 cron）
- 局域网主机发现（DHCP 租约 + ARP/邻居表）
- 日志查看与清理、调试诊断（ip rule / nft / ps）

## 快速开始

```bash
cargo build --release

# 运行
./target/release/nexa -p 9990

# 指定数据目录
./target/release/nexa -d /etc/nexa -p 9990

# 仅应用 / 清理网络规则
./target/release/nexa on
./target/release/nexa off
```

浏览器访问：`http://<host>:9990`

## 项目结构

```
src/
  main.rs         # 入口、路由、Boot 流程
  web.rs          # ★ 网页面板 + 全部 API（单文件）
  config.rs       # 配置模型（对齐原 UCI）
  auth.rs         # JWT + bcrypt
  store.rs        # 配置 / profiles 持久化
  core.rs         # 代理核心进程管理
  netmanager.rs   # 网络规则（ip rule / nftables）
  scheduler.rs    # 定时重启 / 清日志
  hosts.rs        # 局域网主机发现
  logger.rs       # 应用日志
  paths.rs        # 路径管理
```

## API 概览

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /api/auth/login | 登录 |
| GET/PUT | /api/config | 读写配置 |
| POST | /api/config/apply | 应用配置（重启核心 + 网络） |
| POST | /api/start \| stop \| restart | 启停核心 |
| POST | /api/net/on \| off | 单独应用/清理网络规则 |
| GET/PUT/DELETE | /api/profiles/:name | 配置文件管理 |
| GET | /api/logs/app \| core | 日志 |
| GET | /api/hosts | 局域网主机 |
| POST | /api/debug | 诊断（ip_rule / nft / ps） |

## License

MIT
