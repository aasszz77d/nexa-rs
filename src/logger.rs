//! Application logger — writes to data/logs/app.log

use crate::paths;
use parking_lot::Mutex;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Arc;

pub struct Logger {
    inner: Mutex<()>,
}

impl Logger {
    pub fn new() -> anyhow::Result<Arc<Self>> {
        if let Some(p) = paths::app_log_path().parent() {
            fs::create_dir_all(p)?;
        }
        Ok(Arc::new(Self {
            inner: Mutex::new(()),
        }))
    }

    pub fn app(&self, tag: &str, msg: &str) {
        let _g = self.inner.lock();
        let line = format!(
            "[{}] [{}] {}\n",
            chrono_like_now(),
            tag,
            msg
        );
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths::app_log_path())
        {
            let _ = f.write_all(line.as_bytes());
        }
        tracing::info!(target: "nexa", "[{}] {}", tag, msg);
    }

    pub fn read_app(&self) -> String {
        fs::read_to_string(paths::app_log_path()).unwrap_or_else(|_| String::new())
    }

    pub fn read_core(&self) -> String {
        fs::read_to_string(paths::core_log_path()).unwrap_or_else(|_| String::new())
    }

    pub fn clear_app(&self) -> anyhow::Result<()> {
        let _g = self.inner.lock();
        fs::write(paths::app_log_path(), "")?;
        Ok(())
    }

    pub fn clear_core(&self) -> anyhow::Result<()> {
        fs::write(paths::core_log_path(), "")?;
        Ok(())
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, m, s)
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
