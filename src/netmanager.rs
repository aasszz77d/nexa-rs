//! Network / firewall manager — applies routing rules & nftables via shell.
//! Aligns with original nexa netmanager (tproxy / tun / cleanup).

use crate::config::Config;
use crate::logger::Logger;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct NetManager {
    log: Arc<Logger>,
}

impl NetManager {
    pub fn new(log: Arc<Logger>) -> Self {
        Self { log }
    }

    pub fn apply(&self, cfg: &Config) -> anyhow::Result<()> {
        let p = &cfg.proxy;
        if !p.enabled {
            self.log.app("代理", "已禁用，跳过防火墙设置。");
            return Ok(());
        }
        self.log.app("代理", "已启用，配置防火墙规则。");

        let tproxy = p.tcp_mode == "tproxy" || p.udp_mode == "tproxy";
        let tun = p.tcp_mode == "tun" || p.udp_mode == "tun";
        let r = &cfg.routing;

        if tproxy && has_bridge() && module_loaded("br_netfilter") {
            if p.ipv4_proxy && sysctl_get("net.bridge.bridge-nf-call-iptables") == "1" {
                let _ = sysctl_set("net.bridge.bridge-nf-call-iptables", "0");
                self.log.app("代理", "已关闭 bridge-nf-call-iptables");
            }
            if p.ipv6_proxy && sysctl_get("net.bridge.bridge-nf-call-ip6tables") == "1" {
                let _ = sysctl_set("net.bridge.bridge-nf-call-ip6tables", "0");
                self.log.app("代理", "已关闭 bridge-nf-call-ip6tables");
            }
        }

        if tun {
            self.log.app("代理", &format!("等待 TUN 设备 {} 上线…", p.tun_device));
            if !wait_for_tun(&p.tun_device, p.tun_timeout, p.tun_interval) {
                self.log.app("代理", "超时，TUN 设备未上线。");
                anyhow::bail!("tun device {} not up", p.tun_device);
            }
            self.log.app("代理", "TUN 设备已上线。");
        }

        if tproxy {
            if p.ipv4_proxy {
                run(&["ip", "-4", "route", "replace", "local", "default", "dev", "lo", "table", &r.tproxy_route_table]);
                let fwmark = format!("{}/{}", r.tproxy_fw_mark, r.tproxy_fw_mask);
                run(&["ip", "-4", "rule", "add", "pref", &r.tproxy_rule_pref.to_string(), "fwmark", &fwmark, "table", &r.tproxy_route_table]);
            }
            if p.ipv6_proxy {
                run(&["ip", "-6", "route", "replace", "local", "default", "dev", "lo", "table", &r.tproxy_route_table]);
                let fwmark = format!("{}/{}", r.tproxy_fw_mark, r.tproxy_fw_mask);
                run(&["ip", "-6", "rule", "add", "pref", &r.tproxy_rule_pref.to_string(), "fwmark", &fwmark, "table", &r.tproxy_route_table]);
            }
            self.log.app("代理", "已添加 tproxy 路由规则。");
        }

        if tun {
            if p.ipv4_proxy {
                run(&["ip", "-4", "route", "replace", "default", "dev", &p.tun_device, "table", &r.tun_route_table]);
                let fwmark = format!("{}/{}", r.tun_fw_mark, r.tun_fw_mask);
                run(&["ip", "-4", "rule", "add", "pref", &r.tun_rule_pref.to_string(), "fwmark", &fwmark, "table", &r.tun_route_table]);
            }
            if p.ipv6_proxy {
                run(&["ip", "-6", "route", "replace", "default", "dev", &p.tun_device, "table", &r.tun_route_table]);
                let fwmark = format!("{}/{}", r.tun_fw_mark, r.tun_fw_mask);
                run(&["ip", "-6", "rule", "add", "pref", &r.tun_rule_pref.to_string(), "fwmark", &fwmark, "table", &r.tun_route_table]);
            }
            self.log.app("代理", "已添加 tun 路由规则。");
        }

        if let Err(e) = self.apply_nft(cfg) {
            self.log.app("代理", &format!("nft 规则应用警告: {}", e));
        }

        self.log.app("代理", "网络规则配置完成。");
        Ok(())
    }

    pub fn cleanup(&self, cfg: &Config) {
        self.log.app("代理", "清理网络规则…");
        let r = &cfg.routing;

        for pref in [r.tproxy_rule_pref.to_string(), r.tun_rule_pref.to_string()] {
            run(&["ip", "-4", "rule", "del", "pref", &pref]);
            run(&["ip", "-6", "rule", "del", "pref", &pref]);
        }

        for table in [&r.tproxy_route_table, &r.tun_route_table] {
            run(&["ip", "-4", "route", "flush", "table", table]);
            run(&["ip", "-6", "route", "flush", "table", table]);
        }

        run(&["nft", "delete", "table", "inet", "nexa"]);

        if sysctl_get("net.bridge.bridge-nf-call-iptables") == "0" {
            let _ = sysctl_set("net.bridge.bridge-nf-call-iptables", "1");
        }
        if sysctl_get("net.bridge.bridge-nf-call-ip6tables") == "0" {
            let _ = sysctl_set("net.bridge.bridge-nf-call-ip6tables", "1");
        }

        if !r.dummy_device.is_empty() {
            run(&["ip", "link", "del", &r.dummy_device]);
        }

        self.log.app("代理", "网络规则已清理。");
    }

    fn apply_nft(&self, cfg: &Config) -> anyhow::Result<()> {
        let p = &cfg.proxy;
        let r = &cfg.routing;

        let mut rules = String::from(
            "table inet nexa {\n  chain prerouting {\n    type filter hook prerouting priority mangle; policy accept;\n",
        );

        if p.ipv4_dns_hijack {
            rules.push_str(&format!(
                "    meta l4proto {{ tcp, udp }} th dport 53 redirect to :{}\n",
                p.dns_port
            ));
        }

        if p.tcp_mode == "tproxy" && p.ipv4_proxy {
            rules.push_str(&format!(
                "    meta l4proto tcp tproxy ip to :{} meta mark set {}\n",
                p.tproxy_port, r.tproxy_fw_mark
            ));
        }
        if p.udp_mode == "tproxy" && p.ipv4_proxy {
            rules.push_str(&format!(
                "    meta l4proto udp tproxy ip to :{} meta mark set {}\n",
                p.tproxy_port, r.tproxy_fw_mark
            ));
        }

        rules.push_str("  }\n}\n");

        let path = crate::paths::data_dir().join("nexa.nft");
        std::fs::write(&path, &rules)?;
        run(&["nft", "delete", "table", "inet", "nexa"]);
        let status = Command::new("nft")
            .args(["-f", path.to_str().unwrap_or("")])
            .status();
        match status {
            Ok(s) if s.success() => {
                self.log.app("代理", "nftables 规则已应用。");
                Ok(())
            }
            Ok(s) => anyhow::bail!("nft exit code {:?}", s.code()),
            Err(e) => anyhow::bail!("nft not available: {}", e),
        }
    }
}

fn run(args: &[&str]) {
    let _ = Command::new(args[0]).args(&args[1..]).output();
}

fn sysctl_get(key: &str) -> String {
    let path = format!("/proc/sys/{}", key.replace('.', "/"));
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn sysctl_set(key: &str, val: &str) -> std::io::Result<()> {
    let path = format!("/proc/sys/{}", key.replace('.', "/"));
    std::fs::write(path, val)
}

fn has_bridge() -> bool {
    std::path::Path::new("/sys/class/net")
        .read_dir()
        .ok()
        .map(|d| d.filter_map(|e| e.ok()).any(|e| e.path().join("bridge").exists()))
        .unwrap_or(false)
}

fn module_loaded(name: &str) -> bool {
    std::fs::read_to_string("/proc/modules")
        .map(|s| s.lines().any(|l| l.starts_with(name)))
        .unwrap_or(false)
}

fn wait_for_tun(device: &str, timeout_sec: i32, interval_sec: i32) -> bool {
    let timeout = if timeout_sec <= 0 { 5 } else { timeout_sec };
    let interval = if interval_sec <= 0 { 1 } else { interval_sec };
    let path = format!("/sys/class/net/{}", device);
    for _ in 0..(timeout / interval).max(1) {
        if std::path::Path::new(&path).exists() {
            return true;
        }
        thread::sleep(Duration::from_secs(interval as u64));
    }
    std::path::Path::new(&path).exists()
}
