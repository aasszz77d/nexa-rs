//! Authentication: JWT + bcrypt password.

use crate::paths;
use crate::store::Store;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use parking_lot::RwLock;

const JWT_SECRET: &[u8] = b"nexa-rs-default-secret-change-me-in-prod";
const DEFAULT_USER: &str = "admin";
const DEFAULT_PASS: &str = "admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthFile {
    username: String,
    password_hash: String,
    no_auth: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub struct AuthState {
    store: Arc<Store>,
    inner: RwLock<AuthFile>,
}

impl AuthState {
    pub fn new(store: Arc<Store>) -> anyhow::Result<Self> {
        let path = paths::auth_path();
        let auth = if path.exists() {
            let data = fs::read_to_string(&path)?;
            serde_json::from_str(&data)?
        } else {
            let hash = bcrypt::hash(DEFAULT_PASS, 10)?;
            let a = AuthFile {
                username: DEFAULT_USER.into(),
                password_hash: hash,
                no_auth: false,
            };
            fs::write(&path, serde_json::to_string_pretty(&a)?)?;
            a
        };
        Ok(Self {
            store,
            inner: RwLock::new(auth),
        })
    }

    fn persist(&self) -> anyhow::Result<()> {
        let a = self.inner.read().clone();
        fs::write(paths::auth_path(), serde_json::to_string_pretty(&a)?)?;
        Ok(())
    }

    pub fn login(&self, username: &str, password: &str) -> anyhow::Result<String> {
        let a = self.inner.read();
        if a.username != username {
            anyhow::bail!("invalid credentials");
        }
        if !bcrypt::verify(password, &a.password_hash)? {
            anyhow::bail!("invalid credentials");
        }
        drop(a);
        self.issue_token(username)
    }

    pub fn issue_token(&self, username: &str) -> anyhow::Result<String> {
        let exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
            + 7 * 24 * 3600;
        let claims = Claims {
            sub: username.to_string(),
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(JWT_SECRET),
        )?;
        Ok(token)
    }

    pub fn verify_token(&self, token: &str) -> anyhow::Result<Claims> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(JWT_SECRET),
            &Validation::default(),
        )?;
        Ok(data.claims)
    }

    pub fn is_no_auth(&self) -> bool {
        self.inner.read().no_auth
    }

    pub fn set_no_auth(&self, v: bool) -> anyhow::Result<()> {
        self.inner.write().no_auth = v;
        self.persist()
    }

    pub fn username(&self) -> String {
        self.inner.read().username.clone()
    }

    pub fn change_password(&self, old: &str, new_pass: &str) -> anyhow::Result<()> {
        {
            let a = self.inner.read();
            if !bcrypt::verify(old, &a.password_hash)? {
                anyhow::bail!("old password incorrect");
            }
        }
        let hash = bcrypt::hash(new_pass, 10)?;
        self.inner.write().password_hash = hash;
        self.persist()
    }

    #[allow(dead_code)]
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
}
