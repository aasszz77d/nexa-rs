//! Collect LAN hosts from DHCP leases and neighbor table.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Host {
    pub ip: String,
    pub ip6: String,
    pub mac: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Hosts {
    pub ip: Vec<String>,
    pub ip6: Vec<String>,
    pub mac: Vec<String>,
    pub list: Vec<Host>,
}

const DHCP_FILES: &[&str] = &[
    "/tmp/dhcp.leases",
    "/var/lib/misc/dnsmasq.leases",
    "/var/lib/dhcp/dhcpd.leases",
    "/var/db/dhcpd.leases",
];

pub fn collect() -> Hosts {
    let mut by_mac: HashMap<String, Host> = HashMap::new();

    for path in DHCP_FILES {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let mac = parts[1].to_lowercase();
                    let ip = parts[2].to_string();
                    let name = if parts[3] != "*" {
                        parts[3].to_string()
                    } else {
                        String::new()
                    };
                    let entry = by_mac.entry(mac.clone()).or_insert_with(|| Host {
                        mac: mac.clone(),
                        ..Default::default()
                    });
                    if ip.contains(':') {
                        if entry.ip6.is_empty() {
                            entry.ip6 = ip;
                        }
                    } else if entry.ip.is_empty() {
                        entry.ip = ip;
                    }
                    if entry.name.is_empty() && !name.is_empty() {
                        entry.name = name;
                    }
                }
            }
        }
    }

    if let Ok(out) = Command::new("ip").args(["neigh", "show"]).output() {
        parse_neigh(&String::from_utf8_lossy(&out.stdout), &mut by_mac);
    }
    if let Ok(out) = Command::new("ip").args(["-6", "neigh", "show"]).output() {
        parse_neigh(&String::from_utf8_lossy(&out.stdout), &mut by_mac);
    }

    let mut result = Hosts::default();
    for (_, h) in by_mac {
        if !h.ip.is_empty() {
            result.ip.push(h.ip.clone());
        }
        if !h.ip6.is_empty() {
            result.ip6.push(h.ip6.clone());
        }
        if !h.mac.is_empty() {
            result.mac.push(h.mac.clone());
        }
        result.list.push(h);
    }
    result.ip.sort();
    result.ip.dedup();
    result.ip6.sort();
    result.ip6.dedup();
    result.mac.sort();
    result.mac.dedup();
    result.list.sort_by(|a, b| a.ip.cmp(&b.ip));
    result
}

fn parse_neigh(text: &str, by_mac: &mut HashMap<String, Host>) {
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let ip = parts[0];
        let mut mac = String::new();
        for i in 0..parts.len() {
            if parts[i] == "lladdr" && i + 1 < parts.len() {
                mac = parts[i + 1].to_lowercase();
                break;
            }
        }
        if mac.is_empty() || mac == "00:00:00:00:00:00" {
            continue;
        }
        let entry = by_mac.entry(mac.clone()).or_insert_with(|| Host {
            mac: mac.clone(),
            ..Default::default()
        });
        if ip.contains(':') {
            if entry.ip6.is_empty() {
                entry.ip6 = ip.to_string();
            }
        } else if entry.ip.is_empty() {
            entry.ip = ip.to_string();
        }
    }
}
