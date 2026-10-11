//! Core process manager (proxy binary lifecycle).

use crate::config::Config;
use crate::paths;
use crate::store::Store;
use parking_lot::Mutex;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tracing::{info, warn};

pub struct CoreManager {
    store: Arc<Store>,
    child: Mutex<Option<Child>>,
}

impl CoreManager {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            child: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.child.lock();
        if let Some(ref mut c) = *guard {
            match c.try_wait() {
                Ok(Some(_)) => {
                    *guard = None;
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    *guard = None;
                    false
                }
            }
        } else {
            false
        }
    }

    pub async fn start(&self, cfg: &Config) -> anyhow::Result<()> {
        if self.is_running() {
            self.stop().await?;
        }

        let _ = paths::set_run_dir(&cfg.config.run_dir);

        let binary = if cfg.config.run_binary.is_empty() {
            "sing-box".to_string()
        } else {
            cfg.config.run_binary.clone()
        };

        let profile_name = if cfg.config.profile.is_empty() {
            "config.yaml".to_string()
        } else {
            cfg.config.profile.clone()
        };

        let src = paths::profiles_dir().join(&profile_name);
        let dst = paths::run_dir().join(&profile_name);
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        }

        let args_tpl = if cfg.config.run_args.is_empty() {
            "run -c {profile}".to_string()
        } else {
            cfg.config.run_args.clone()
        };

        let args_str = args_tpl
            .replace("{profile}", &dst.to_string_lossy())
            .replace("{rundir}", &paths::run_dir().to_string_lossy());

        let args: Vec<&str> = args_str.split_whitespace().collect();

        info!("Starting core: {} {}", binary, args_str);

        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths::core_log_path())?;

        let child = Command::new(&binary)
            .args(&args)
            .current_dir(paths::run_dir())
            .stdout(Stdio::from(log_file.try_clone()?))
            .stderr(Stdio::from(log_file))
            .spawn();

        match child {
            Ok(c) => {
                *self.child.lock() = Some(c);
                info!("Core started");
                Ok(())
            }
            Err(e) => {
                warn!("Failed to start core {}: {}", binary, e);
                Err(anyhow::anyhow!("failed to spawn {}: {}", binary, e))
            }
        }
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        let mut guard = self.child.lock();
        if let Some(mut c) = guard.take() {
            let _ = c.kill();
            let _ = c.wait();
            info!("Core stopped");
        }
        Ok(())
    }

    pub async fn restart(&self, cfg: &Config) -> anyhow::Result<()> {
        self.stop().await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        self.start(cfg).await
    }

    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "running": self.is_running(),
            "pid": self.child.lock().as_ref().map(|c| c.id()),
        })
    }
}
