# Nexa (Rust)

透明代理管理面板的 Rust 重写版本。

独立守护进程，`./nexa` 即可运行，默认监听 `:9990`。  
提供 HTTP API + 内嵌 Web 面板，不依赖 luci / rpcd / ubus / UCI。

## 特性

- 单二进制，内嵌完整 Web 管理面板（`src/web.rs` 一个文件包含全部 HTML/CSS/JS + API）
- JWT 认证 + bcrypt 密码
- 配置文件 / 配置文件管理（profiles）
- 代理核心进程管理（sing-box / mihomo / xray 等）
- 日志查看与清理
- 子命令 `on` / `off`（网络规则占位，可扩展 nftables/iptables）

## 快速开始

```bash
# 编译
cargo build --release

# 运行（数据目录默认 ./data，Linux 下若存在 /etc/nexa 则用之）
./target/release/nexa -p 9990

# 指定数据目录
./target/release/nexa -d /etc/nexa -p 9990

# 仅应用/清理网络规则（当前为 stub）
./target/release/nexa on
./target/release/nexa off
```

默认账号：`admin` / `admin`

浏览器访问：`http://<host>:9990`

## 项目结构

```
src/
  main.rs      # 入口、路由
  web.rs       # ★ 网页面板 + 全部 API 处理器（单文件）
  config.rs    # 配置模型
  auth.rs      # 认证
  store.rs     # 配置持久化
  core.rs      # 代理核心进程管理
  paths.rs     # 路径管理
```

## 与原版 Go nexa 的对应关系

| 原版 | Rust 版 |
|------|---------|
| chi + embed web | axum + 单文件嵌入 |
| bbolt 存储 | JSON 文件 |
| JWT auth | jsonwebtoken + bcrypt |
| netmanager (nft/iptables) | stub（可扩展） |
| core 进程管理 | std::process::Command |

## License

MIT
