//! Sign-in, identity validation, and serialized credential renewal for local hosts.
//!
//! Hosts own browser interaction and callback listeners. Credential storage is injected.

mod engine;
mod identity;
mod store;
mod types;

pub use engine::AuthEngine;
pub use store::{CredentialStore, CredentialTransaction, FileStore};
pub use types::{
    AuthConfig, CredentialSource, LoginAttempt, LogoutResult, RegistrationStatus, RetryRequired,
    SecretString, StaticCredential, StoredAuth, StoredCredentials, StoredRegistration,
};
