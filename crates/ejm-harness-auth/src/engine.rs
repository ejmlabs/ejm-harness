mod accounts;
mod attempt;
mod callback;
mod complete;
mod discovery;
mod http;
mod logout;
mod renewal;
mod start;
mod token;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, ensure};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex as AsyncMutex;

use crate::{AuthConfig, CredentialStore, identity::IdentityValidator};
use attempt::Attempt;

pub(crate) use discovery::Discovery;
pub(crate) use http::Http;

pub(crate) const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const DYNAMIC_CLIENT: &str = "dynamic_agent_client";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

#[derive(Clone)]
pub struct AuthEngine {
    inner: Arc<Inner>,
}

struct Inner {
    config: AuthConfig,
    store: Arc<dyn CredentialStore>,
    http: Arc<Http>,
    identity: IdentityValidator,
    discovery: AsyncMutex<Option<Discovery>>,
    attempts: Mutex<HashMap<String, Arc<Attempt>>>,
}

impl AuthEngine {
    /// Create a `ChatGPT` public-client engine. The host supplies private credential storage.
    ///
    /// # Errors
    /// Rejects an empty app name or an unavailable HTTP transport.
    pub fn new(config: AuthConfig, store: Arc<dyn CredentialStore>) -> Result<Self> {
        ensure!(
            !config.agent_name.trim().is_empty() && config.agent_name.len() <= 128,
            "invalid agent name"
        );
        let http = Arc::new(Http::new()?);
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                store,
                identity: IdentityValidator::new(http.clone()),
                http,
                discovery: AsyncMutex::new(None),
                attempts: Mutex::new(HashMap::new()),
            }),
        })
    }
}

/// Lock a mutex, recovering its data if a panicking holder poisoned it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn equal(left: &str, right: &str) -> bool {
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

pub(crate) fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
