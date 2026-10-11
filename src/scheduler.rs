//! Built-in cron scheduler for scheduled restart / log clear.

use crate::config::Config;
use crate::core::CoreManager;
use crate::logger::Logger;
use crate::store::Store;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

pub struct Scheduler {
    log: Arc<Logger>,
    core: Arc<CoreManager>,
    store: Arc<Store>,
    stop: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Scheduler {
    pub fn new(log: Arc<Logger>, core: Arc<CoreManager>, store: Arc<Store>) -> Arc<Self> {
        Arc::new(Self {
            log,
            core,
            store,
            stop: Mutex::new(None),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let mut guard = self.stop.lock();
        if guard.is_some() {
            return;
        }
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            this.loop_run().await;
        });
        *guard = Some(handle);
        self.log.app("调度", "调度器已启动。");
    }

    pub fn stop(&self) {
        if let Some(h) = self.stop.lock().take() {
            h.abort();
            self.log.app("调度", "调度器已停止。");
        }
    }

    pub fn reload(&self, cfg: &Config) {
        let _ = cfg;
        self.log.app("调度", "调度任务已刷新。");
    }

    async fn loop_run(&self) {
        loop {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let wait = 60 - (now % 60);
            tokio::time::sleep(Duration::from_secs(wait)).await;

            let cfg = match self.store.load() {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (min, hour, dom, mon, dow) = current_cron_fields();

            if cfg.config.scheduled_restart
                && !cfg.config.scheduled_restart_cron.is_empty()
                && cron_match(&cfg.config.scheduled_restart_cron, min, hour, dom, mon, dow)
            {
                self.log.app("调度", "定时重启核心…");
                if let Err(e) = self.core.restart(&cfg).await {
                    self.log.app("调度", &format!("定时重启失败: {}", e));
                } else {
                    self.log.app("调度", "定时重启完成。");
                }
            }

            if cfg.log.scheduled_clear
                && !cfg.log.scheduled_clear_cron.is_empty()
                && cron_match(&cfg.log.scheduled_clear_cron, min, hour, dom, mon, dow)
            {
                self.log.app("调度", "定时清空日志…");
                let _ = self.log.clear_app();
                let _ = self.log.clear_core();
            }
        }
    }
}

fn current_cron_fields() -> (u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let hour = (rem / 3600) as u32;
    let min = ((rem % 3600) / 60) as u32;
    let dow = ((days + 4) % 7) as u32;
    let (_y, mon, dom) = days_to_ymd(days as i64);
    (min, hour, dom, mon, dow)
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn cron_match(expr: &str, min: u32, hour: u32, dom: u32, mon: u32, dow: u32) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    field_match(parts[0], min)
        && field_match(parts[1], hour)
        && field_match(parts[2], dom)
        && field_match(parts[3], mon)
        && field_match(parts[4], dow)
}

fn field_match(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }
    for part in field.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(lo), Ok(hi)) = (a.parse::<u32>(), b.parse::<u32>()) {
                if value >= lo && value <= hi {
                    return true;
                }
            }
        } else if let Some((a, step)) = part.split_once('/') {
            let base = if a == "*" { 0 } else { a.parse().unwrap_or(0) };
            let step: u32 = step.parse().unwrap_or(1);
            if step > 0 && value >= base && (value - base) % step == 0 {
                return true;
            }
        } else if part.parse::<u32>().ok() == Some(value) {
            return true;
        }
    }
    false
}
