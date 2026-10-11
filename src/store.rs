//! Persistent config store (JSON file).

use crate::config::Config;
use crate::paths;
use parking_lot::RwLock;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

pub struct Store {
    path: PathBuf,
    cache: RwLock<Option<Config>>,
}

impl Store {
    pub fn open() -> anyhow::Result<Self> {
        let path = paths::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let store = Self {
            path,
            cache: RwLock::new(None),
        };
        if !store.path.exists() {
            store.save(&Config::default())?;
        }
        Ok(store)
    }

    pub fn load(&self) -> anyhow::Result<Config> {
        if let Some(c) = self.cache.read().as_ref() {
            return Ok(c.clone());
        }
        let mut f = fs::File::open(&self.path)?;
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        let cfg: Config = serde_json::from_str(&buf)?;
        *self.cache.write() = Some(cfg.clone());
        Ok(cfg)
    }

    pub fn save(&self, cfg: &Config) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(cfg)?;
        let mut f = fs::File::create(&self.path)?;
        f.write_all(json.as_bytes())?;
        *self.cache.write() = Some(cfg.clone());
        Ok(())
    }

    pub fn list_profiles(&self) -> anyhow::Result<Vec<String>> {
        let dir = paths::profiles_dir();
        let mut names = Vec::new();
        if dir.exists() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    if let Some(n) = entry.file_name().to_str() {
                        names.push(n.to_string());
                    }
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn read_profile(&self, name: &str) -> anyhow::Result<String> {
        let path = paths::profiles_dir().join(sanitize_name(name));
        Ok(fs::read_to_string(path)?)
    }

    pub fn write_profile(&self, name: &str, content: &str) -> anyhow::Result<()> {
        let path = paths::profiles_dir().join(sanitize_name(name));
        fs::write(path, content)?;
        Ok(())
    }

    pub fn delete_profile(&self, name: &str) -> anyhow::Result<()> {
        let path = paths::profiles_dir().join(sanitize_name(name));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn rename_profile(&self, old: &str, new: &str) -> anyhow::Result<()> {
        let from = paths::profiles_dir().join(sanitize_name(old));
        let to = paths::profiles_dir().join(sanitize_name(new));
        fs::rename(from, to)?;
        Ok(())
    }
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_' || *c == '+')
        .collect()
}
