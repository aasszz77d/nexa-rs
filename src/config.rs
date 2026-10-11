//! Configuration model, aligned with original nexa UCI proxy config.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub config: ConfigSection,
    pub procd: ProcdSection,
    pub proxy: ProxySection,
    #[serde(default)]
    pub router_access_controls: Vec<RouterAccessControl>,
    #[serde(default)]
    pub lan_access_controls: Vec<LanAccessControl>,
    pub routing: RoutingSection,
    pub log: LogSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSection {
    pub enabled: bool,
    pub profile: String,
    pub run_binary: String,
    pub run_args: String,
    #[serde(default)]
    pub binary_list: Option<Vec<String>>,
    #[serde(default)]
    pub args_list: Option<Vec<String>>,
    #[serde(default)]
    pub run_dir: String,
    #[serde(default)]
    pub start_delay: i32,
    #[serde(default)]
    pub scheduled_restart: bool,
    #[serde(default)]
    pub scheduled_restart_cron: String,
}

impl Default for ConfigSection {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: "config.yaml".into(),
            run_binary: "sing-box".into(),
            run_args: "run -c {profile}".into(),
            binary_list: Some(vec!["sing-box".into(), "mihomo".into(), "xray".into()]),
            args_list: Some(vec!["run -c {profile}".into(), "-d {rundir} -f {profile}".into()]),
            run_dir: String::new(),
            start_delay: 0,
            scheduled_restart: false,
            scheduled_restart_cron: "0 4 * * *".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcdSection {
    pub fast_reload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySection {
    pub enabled: bool,
    pub ipv4_dns_hijack: bool,
    pub ipv6_dns_hijack: bool,
    pub ipv4_proxy: bool,
    pub ipv6_proxy: bool,
    pub fake_ip_ping_hijack: bool,
    pub tcp_mode: String,
    pub udp_mode: String,
    pub router_proxy: bool,
    pub lan_proxy: bool,
    #[serde(default)]
    pub lan_inbound_interface: Vec<String>,
    pub dns_port: String,
    pub redir_port: String,
    pub tproxy_port: String,
    pub tun_device: String,
    pub ui_port: String,
    pub ui_path: String,
    pub fake_ip_range: String,
    pub fake_ip6_range: String,
    #[serde(default)]
    pub reserved_ip: Vec<String>,
    #[serde(default)]
    pub reserved_ip6: Vec<String>,
    pub bypass_china_mainland_ip: bool,
    pub bypass_china_mainland_ip6: bool,
    pub proxy_tcp_dport: String,
    pub proxy_udp_dport: String,
    #[serde(default)]
    pub bypass_dscp: Vec<String>,
    #[serde(default)]
    pub bypass_fwmark: Vec<String>,
    pub bypass_cgroup: bool,
    pub bypass_gid: bool,
    pub bypass_mark: bool,
    #[serde(default)]
    pub bypass_mark_values: Vec<String>,
    pub tun_timeout: i32,
    pub tun_interval: i32,
}

impl Default for ProxySection {
    fn default() -> Self {
        Self {
            enabled: true,
            ipv4_dns_hijack: true,
            ipv6_dns_hijack: false,
            ipv4_proxy: true,
            ipv6_proxy: false,
            fake_ip_ping_hijack: true,
            tcp_mode: "tproxy".into(),
            udp_mode: "tproxy".into(),
            router_proxy: true,
            lan_proxy: true,
            lan_inbound_interface: vec!["br-lan".into()],
            dns_port: "1053".into(),
            redir_port: "7892".into(),
            tproxy_port: "7893".into(),
            tun_device: "nexa".into(),
            ui_port: "9090".into(),
            ui_path: "/".into(),
            fake_ip_range: "198.18.0.0/15".into(),
            fake_ip6_range: "fc00::/18".into(),
            reserved_ip: vec![],
            reserved_ip6: vec![],
            bypass_china_mainland_ip: true,
            bypass_china_mainland_ip6: false,
            proxy_tcp_dport: "1:65535".into(),
            proxy_udp_dport: "1:65535".into(),
            bypass_dscp: vec![],
            bypass_fwmark: vec![],
            bypass_cgroup: false,
            bypass_gid: false,
            bypass_mark: true,
            bypass_mark_values: vec!["0xff".into()],
            tun_timeout: 5,
            tun_interval: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterAccessControl {
    pub id: String,
    pub enabled: bool,
    #[serde(default)]
    pub user: Vec<String>,
    #[serde(default)]
    pub group: Vec<String>,
    #[serde(default)]
    pub cgroup: Vec<String>,
    pub dns: bool,
    pub proxy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanAccessControl {
    pub id: String,
    pub enabled: bool,
    #[serde(default)]
    pub ip: Vec<String>,
    #[serde(default)]
    pub ip6: Vec<String>,
    #[serde(default)]
    pub mac: Vec<String>,
    pub dns: bool,
    pub proxy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingSection {
    pub tproxy_fw_mark: String,
    pub tproxy_fw_mask: String,
    pub tun_fw_mark: String,
    pub tun_fw_mask: String,
    pub tproxy_rule_pref: i32,
    pub tun_rule_pref: i32,
    pub tproxy_route_table: String,
    pub tun_route_table: String,
    pub cgroup_id: String,
    pub cgroup_name: String,
    pub dummy_device: String,
}

impl Default for RoutingSection {
    fn default() -> Self {
        Self {
            tproxy_fw_mark: "0x1".into(),
            tproxy_fw_mask: "0x1".into(),
            tun_fw_mark: "0x2".into(),
            tun_fw_mask: "0x2".into(),
            tproxy_rule_pref: 9000,
            tun_rule_pref: 9001,
            tproxy_route_table: "100".into(),
            tun_route_table: "101".into(),
            cgroup_id: "0x1234".into(),
            cgroup_name: "nexa".into(),
            dummy_device: "nexa-dummy".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSection {
    pub level: String,
    pub max_size_mb: i32,
    #[serde(default)]
    pub scheduled_clear: bool,
    #[serde(default)]
    pub scheduled_clear_cron: String,
}

impl Default for LogSection {
    fn default() -> Self {
        Self {
            level: "info".into(),
            max_size_mb: 10,
            scheduled_clear: false,
            scheduled_clear_cron: "0 0 * * *".into(),
        }
    }
}
