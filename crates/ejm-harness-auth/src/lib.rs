//! Sign-in, identity validation, and serialized credential renewal for local hosts.
//!
//! Hosts own browser interaction and callback listeners. Credential storage is injected.

mod claude;
mod engine;
mod identity;
mod store;
mod types;

pub use claude::{
    CLAUDE_CODE_ENV_OVERRIDES, ClaudeCodeClient, ClaudeCodeStatus, resolve_claude_code_program,
};
pub use engine::AuthEngine;
pub use store::{
    CredentialStore, CredentialTransaction, FileStore, SecretId, SecretVault, VaultKeyStore,
    VaultTransaction,
};
pub use types::{
    AuthConfig, CredentialSource, LoginAttempt, LogoutResult, RegistrationStatus, RetryRequired,
    SecretString, StaticCredential, StoredAuth, StoredCredentials, StoredRegistration,
};
