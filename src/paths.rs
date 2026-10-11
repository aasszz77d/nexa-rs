//! Path configuration, mirrors original nexa paths package.

use once_cell::sync::OnceCell;
use std::path::PathBuf;

static DATA_DIR: OnceCell<PathBuf> = OnceCell::new();
static RUN_DIR: OnceCell<parking_lot::RwLock<PathBuf>> = OnceCell::new();

pub fn init(data_dir: Option<&str>) {
    let base = if let Some(d) = data_dir {
        PathBuf::from(d)
    } else if cfg!(target_os = "linux") && std::path::Path::new("/etc/nexa").exists() {
        PathBuf::from("/etc/nexa")
    } else {
        PathBuf::from("./data")
    };
    let _ = DATA_DIR.set(base.clone());
    let run = base.join("run");
    let _ = RUN_DIR.set(parking_lot::RwLock::new(run));
    let _ = std::fs::create_dir_all(&base);
    let _ = std::fs::create_dir_all(base.join("profiles"));
    let _ = std::fs::create_dir_all(base.join("run"));
    let _ = std::fs::create_dir_all(base.join("logs"));
}

fn data() -> &'static PathBuf {
    DATA_DIR.get().expect("paths::init not called")
}

pub fn data_dir() -> PathBuf {
    data().clone()
}

pub fn config_path() -> PathBuf {
    data().join("config.json")
}

pub fn auth_path() -> PathBuf {
    data().join("auth.json")
}

pub fn profiles_dir() -> PathBuf {
    data().join("profiles")
}

pub fn run_dir() -> PathBuf {
    RUN_DIR.get().expect("paths::init").read().clone()
}

pub fn set_run_dir(path: &str) -> anyhow::Result<()> {
    if path.is_empty() {
        let default = data().join("run");
        *RUN_DIR.get().unwrap().write() = default;
        return Ok(());
    }
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        anyhow::bail!("run_dir must be absolute path");
    }
    std::fs::create_dir_all(&p)?;
    *RUN_DIR.get().unwrap().write() = p;
    Ok(())
}

pub fn app_log_path() -> PathBuf {
    data().join("logs").join("app.log")
}

pub fn core_log_path() -> PathBuf {
    data().join("logs").join("core.log")
}
