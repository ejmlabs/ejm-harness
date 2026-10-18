#[cfg(not(unix))]
use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::StoredAuth;

/// Serializes credential reads and updates across host processes.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Hold exclusive ownership until the returned transaction is dropped.
    ///
    /// # Errors
    /// Returns an error for cancellation, lock timeout, or unsafe storage paths.
    async fn lock(&self, cancel: CancellationToken) -> Result<Box<dyn CredentialTransaction>>;
}

/// A credential transaction retains its process lock through network exchanges.
pub trait CredentialTransaction: Send {
    /// Read the current state, or return empty state when no file exists.
    ///
    /// # Errors
    /// Returns an error for unsafe, corrupt, oversized, or unsupported state.
    fn load(&mut self) -> Result<StoredAuth>;

    /// Atomically replace and synchronize the current state.
    ///
    /// # Errors
    /// Returns an error for invalid state, unsafe paths, or a failed durable save.
    /// A synchronization error after replacement leaves the save outcome uncertain.
    fn save(&mut self, state: &StoredAuth) -> Result<()>;
}

#[cfg(unix)]
pub use native::FileStore;

#[cfg(unix)]
mod native {
    use std::{
        collections::HashSet,
        fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions, TryLockError},
        io::{self, Read, Write},
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };

    use anyhow::{Context, Result, bail, ensure};
    use async_trait::async_trait;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use tokio::time::{Instant, sleep_until};
    use tokio_util::sync::CancellationToken;
    use zeroize::Zeroizing;

    use super::{CredentialStore, CredentialTransaction};
    use crate::{SecretString, StoredAuth, StoredCredentials, StoredRegistration};

    const MAX_STORE_BYTES: usize = 1024 * 1024;
    const MAX_REGISTRATIONS: usize = 64;
    const LOCK_WAIT: Duration = Duration::from_secs(30);
    const LOCK_RETRY: Duration = Duration::from_millis(25);

    /// Private Unix file storage with a stable lock inode and atomic replacements.
    #[derive(Clone)]
    pub struct FileStore {
        location: Arc<Location>,
    }

    struct Location {
        path: PathBuf,
        directory: File,
        lock_anchor: File,
        owner: u32,
    }

    impl FileStore {
        /// Open a private directory containing `state.json` and `.lock`.
        ///
        /// New directories use mode 0700. Existing directories and files must have
        /// the current effective owner and exact private modes. Symlink entries,
        /// hardlinked files, and replacement directory or lock inodes are rejected.
        ///
        /// # Errors
        /// Returns an error when the directory, state file, or lock file is
        /// unsafe or unavailable.
        pub fn open(path: impl AsRef<Path>) -> Result<Self> {
            let owner = rustix::process::geteuid().as_raw();
            let path = path.as_ref();
            match fs::symlink_metadata(path) {
                Ok(metadata) => check_directory(&metadata, owner)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(path)
                        .context("create private credential directory")?;
                }
                Err(error) => return Err(error).context("inspect credential directory"),
            }
            let before = fs::symlink_metadata(path).context("inspect credential directory")?;
            check_directory(&before, owner)?;
            let directory = File::open(path).context("open credential directory")?;
            let opened = directory
                .metadata()
                .context("inspect opened credential directory")?;
            check_directory(&opened, owner)?;
            ensure!(
                same_inode(&before, &opened),
                "credential directory changed during open"
            );
            // Resolve parent aliases such as /tmp while rejecting a symlink at the store itself.
            let path = fs::canonicalize(path).context("resolve credential directory")?;
            let current = fs::symlink_metadata(&path).context("inspect credential directory")?;
            check_directory(&current, owner)?;
            ensure!(
                same_inode(&opened, &current),
                "credential directory changed during open"
            );

            let lock_path = path.join(".lock");
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&lock_path)
            {
                Ok(file) => {
                    check_file(
                        &file.metadata().context("inspect new credential lock")?,
                        owner,
                    )?;
                    file.sync_all().context("synchronize new credential lock")?;
                    directory
                        .sync_all()
                        .context("synchronize credential directory")?;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("create credential lock"),
            }
            let lock_anchor = open_private_file(&lock_path, owner, true)?;
            let location = Arc::new(Location {
                path,
                directory,
                lock_anchor,
                owner,
            });
            location.validate()?;
            location.check_state()?;
            Ok(Self { location })
        }

        async fn lock_with_timeout(
            &self,
            cancel: CancellationToken,
            timeout: Duration,
        ) -> Result<Box<dyn CredentialTransaction>> {
            let deadline = Instant::now() + timeout;
            ensure!(!cancel.is_cancelled(), "credential store lock cancelled");
            self.location.validate()?;
            // A clone would share the same open file description and could defeat exclusivity.
            let lock =
                open_private_file(&self.location.path.join(".lock"), self.location.owner, true)?;
            loop {
                ensure!(!cancel.is_cancelled(), "credential store lock cancelled");
                ensure!(Instant::now() < deadline, "credential store lock timed out");
                self.location.validate()?;
                ensure!(
                    same_inode(
                        &lock.metadata().context("inspect credential lock")?,
                        &self
                            .location
                            .lock_anchor
                            .metadata()
                            .context("inspect credential lock anchor")?
                    ),
                    "credential lock was replaced"
                );
                match lock.try_lock() {
                    Ok(()) => {
                        self.location.validate()?;
                        ensure!(!cancel.is_cancelled(), "credential store lock cancelled");
                        return Ok(Box::new(FileTransaction {
                            location: Arc::clone(&self.location),
                            _lock: lock,
                        }));
                    }
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Error(error)) => {
                        return Err(error).context("acquire credential store lock");
                    }
                }
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => bail!("credential store lock cancelled"),
                    () = sleep_until(deadline.min(Instant::now() + LOCK_RETRY)) => {}
                }
            }
        }
    }

    #[async_trait]
    impl CredentialStore for FileStore {
        async fn lock(&self, cancel: CancellationToken) -> Result<Box<dyn CredentialTransaction>> {
            self.lock_with_timeout(cancel, LOCK_WAIT).await
        }
    }

    impl Location {
        fn validate(&self) -> Result<()> {
            let current =
                fs::symlink_metadata(&self.path).context("inspect credential directory")?;
            check_directory(&current, self.owner)?;
            let opened = self
                .directory
                .metadata()
                .context("inspect credential directory handle")?;
            check_directory(&opened, self.owner)?;
            ensure!(
                same_inode(&current, &opened),
                "credential directory was replaced"
            );
            let lock =
                fs::symlink_metadata(self.path.join(".lock")).context("inspect credential lock")?;
            check_file(&lock, self.owner)?;
            let anchor = self
                .lock_anchor
                .metadata()
                .context("inspect credential lock anchor")?;
            check_file(&anchor, self.owner)?;
            ensure!(same_inode(&lock, &anchor), "credential lock was replaced");
            Ok(())
        }

        fn check_state(&self) -> Result<()> {
            match fs::symlink_metadata(self.path.join("state.json")) {
                Ok(metadata) => {
                    check_file(&metadata, self.owner)?;
                    check_size(&metadata)?;
                    Ok(())
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error).context("inspect credential state"),
            }
        }
    }

    struct FileTransaction {
        location: Arc<Location>,
        _lock: File,
    }

    impl CredentialTransaction for FileTransaction {
        fn load(&mut self) -> Result<StoredAuth> {
            self.location.validate()?;
            self.location.check_state()?;
            let path = self.location.path.join("state.json");
            if !path.try_exists().context("inspect credential state")? {
                return Ok(StoredAuth::default());
            }
            let mut file = open_private_file(&path, self.location.owner, false)?;
            check_size(&file.metadata().context("inspect credential state")?)?;
            let mut encoded = Zeroizing::new(Vec::with_capacity(MAX_STORE_BYTES + 1));
            let limit =
                u64::try_from(MAX_STORE_BYTES + 1).context("credential size limit unavailable")?;
            (&mut file)
                .take(limit)
                .read_to_end(&mut encoded)
                .context("read credential state")?;
            ensure!(
                encoded.len() <= MAX_STORE_BYTES,
                "credential state exceeds 1 MiB"
            );
            let current =
                fs::symlink_metadata(&path).context("inspect credential state after read")?;
            check_file(&current, self.location.owner)?;
            ensure!(
                same_inode(
                    &current,
                    &file.metadata().context("inspect credential state handle")?
                ),
                "credential state changed during read"
            );
            let wire: WireAuth = serde_json::from_slice(&encoded)
                .map_err(|_| anyhow::anyhow!("credential state contains invalid JSON"))?;
            ensure!(wire.version == 1, "unsupported credential state version");
            let state = wire.into_state();
            validate_state(&state)?;
            Ok(state)
        }

        fn save(&mut self, state: &StoredAuth) -> Result<()> {
            validate_state(state)?;
            self.location.validate()?;
            self.location.check_state()?;
            let wire = WireAuth::from_state(state);
            let mut encoded = JsonBuffer {
                bytes: Zeroizing::new(Vec::with_capacity(MAX_STORE_BYTES)),
            };
            serde_json::to_writer(&mut encoded, &wire)
                .map_err(|_| anyhow::anyhow!("credential state exceeds supported JSON size"))?;
            let mut temporary = tempfile::Builder::new()
                .prefix(".state-")
                .tempfile_in(&self.location.path)
                .context("create private credential replacement")?;
            temporary
                .as_file()
                .set_permissions(Permissions::from_mode(0o600))
                .context("set credential replacement permissions")?;
            check_file(
                &temporary
                    .as_file()
                    .metadata()
                    .context("inspect credential replacement")?,
                self.location.owner,
            )?;
            temporary
                .write_all(&encoded.bytes)
                .context("write credential replacement")?;
            temporary
                .as_file()
                .sync_all()
                .context("synchronize credential replacement")?;
            self.location
                .directory
                .sync_all()
                .context("synchronize credential replacement directory")?;
            self.location.validate()?;
            self.location.check_state()?;
            temporary
                .persist(self.location.path.join("state.json"))
                .map_err(|_| anyhow::anyhow!("atomically replace credential state"))?;
            self.location
                .directory
                .sync_all()
                .context("synchronize saved credential state")?;
            self.location.validate()?;
            self.location.check_state()
        }
    }

    fn check_directory(metadata: &Metadata, owner: u32) -> Result<()> {
        ensure!(
            metadata.is_dir(),
            "credential directory must be a real directory"
        );
        ensure!(
            metadata.uid() == owner,
            "credential directory has a different owner"
        );
        ensure!(
            metadata.mode() & 0o7777 == 0o700,
            "credential directory must have mode 0700"
        );
        Ok(())
    }

    fn check_file(metadata: &Metadata, owner: u32) -> Result<()> {
        ensure!(
            metadata.is_file(),
            "credential file must be a regular file without symlinks"
        );
        ensure!(
            metadata.uid() == owner,
            "credential file has a different owner"
        );
        ensure!(
            metadata.mode() & 0o7777 == 0o600,
            "credential file must have mode 0600"
        );
        ensure!(
            metadata.nlink() == 1,
            "credential file must not have hard links"
        );
        Ok(())
    }

    fn check_size(metadata: &Metadata) -> Result<()> {
        ensure!(
            usize::try_from(metadata.len()).is_ok_and(|length| length <= MAX_STORE_BYTES),
            "credential state exceeds 1 MiB"
        );
        Ok(())
    }

    fn same_inode(left: &Metadata, right: &Metadata) -> bool {
        left.dev() == right.dev() && left.ino() == right.ino()
    }

    fn open_private_file(path: &Path, owner: u32, write: bool) -> Result<File> {
        let before = fs::symlink_metadata(path).context("inspect private credential file")?;
        check_file(&before, owner)?;
        let file = OpenOptions::new()
            .read(true)
            .write(write)
            .open(path)
            .context("open private credential file")?;
        let opened = file.metadata().context("inspect opened credential file")?;
        check_file(&opened, owner)?;
        ensure!(
            same_inode(&before, &opened),
            "credential file changed during open"
        );
        let after = fs::symlink_metadata(path).context("inspect credential file after open")?;
        check_file(&after, owner)?;
        ensure!(
            same_inode(&after, &opened),
            "credential file changed during open"
        );
        Ok(file)
    }

    struct JsonBuffer {
        bytes: Zeroizing<Vec<u8>>,
    }

    impl Write for JsonBuffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_STORE_BYTES.saturating_sub(self.bytes.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "credential state exceeds 1 MiB",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn validate_state(state: &StoredAuth) -> Result<()> {
        ensure!(
            !state.host_id.is_empty(),
            "credential host identifier is empty"
        );
        ensure!(
            state.registrations.len() <= MAX_REGISTRATIONS,
            "credential state exceeds 64 registrations"
        );
        let mut identifiers = HashSet::new();
        let mut bytes = state.host_id.len();
        for registration in &state.registrations {
            ensure!(
                [
                    &registration.id,
                    &registration.issuer,
                    &registration.subject,
                    &registration.client_id
                ]
                .iter()
                .all(|value| !value.is_empty()),
                "credential registration has an empty identity field"
            );
            ensure!(
                identifiers.insert(&registration.id),
                "duplicate credential registration identifier"
            );
            for value in [
                &registration.id,
                &registration.issuer,
                &registration.subject,
                &registration.client_id,
            ] {
                bytes = bytes.saturating_add(value.len());
            }
            bytes = bytes.saturating_add(registration.email.as_ref().map_or(0, String::len));
            if let Some(credentials) = &registration.credentials {
                ensure!(
                    !credentials.id_token.expose_secret().is_empty(),
                    "credential identity token is empty"
                );
                for token in [
                    credentials.access_token.as_ref(),
                    credentials.refresh_token.as_ref(),
                    Some(&credentials.id_token),
                ]
                .into_iter()
                .flatten()
                {
                    ensure!(
                        !token.expose_secret().is_empty(),
                        "credential token is empty"
                    );
                    bytes = bytes.saturating_add(token.expose_secret().len());
                }
                ensure!(
                    credentials.scopes.len() <= 256,
                    "credential state has too many scopes"
                );
                for scope in &credentials.scopes {
                    ensure!(!scope.is_empty(), "credential scope is empty");
                    bytes = bytes.saturating_add(scope.len());
                }
            }
            ensure!(bytes <= MAX_STORE_BYTES, "credential state exceeds 1 MiB");
        }
        ensure!(bytes <= MAX_STORE_BYTES, "credential state exceeds 1 MiB");
        Ok(())
    }

    // Only private wire adapters can serialize a secret. Both owned token copies and
    // complete JSON buffers are zeroed on drop, including parse and write failures.
    struct WireSecret(SecretString);

    impl Serialize for WireSecret {
        fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
            serializer.serialize_str(self.0.expose_secret())
        }
    }

    impl<'de> Deserialize<'de> for WireSecret {
        fn deserialize<D: Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            String::deserialize(deserializer).map(|value| Self(SecretString::new(value)))
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct WireAuth {
        version: u8,
        host_id: String,
        registrations: Vec<WireRegistration>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct WireRegistration {
        id: String,
        issuer: String,
        subject: String,
        client_id: String,
        email: Option<String>,
        credentials: Option<WireCredentials>,
        refresh_pending: bool,
        #[serde(default)]
        authorization_revision: u64,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct WireCredentials {
        access_token: Option<WireSecret>,
        refresh_token: Option<WireSecret>,
        id_token: WireSecret,
        scopes: Vec<String>,
        expires_at: Option<u64>,
        earliest_refresh_at: Option<u64>,
    }

    impl WireAuth {
        fn from_state(state: &StoredAuth) -> Self {
            Self {
                version: 1,
                host_id: state.host_id.clone(),
                registrations: state
                    .registrations
                    .iter()
                    .map(|registration| WireRegistration {
                        id: registration.id.clone(),
                        issuer: registration.issuer.clone(),
                        subject: registration.subject.clone(),
                        client_id: registration.client_id.clone(),
                        email: registration.email.clone(),
                        credentials: registration.credentials.as_ref().map(|credentials| {
                            WireCredentials {
                                access_token: credentials.access_token.clone().map(WireSecret),
                                refresh_token: credentials.refresh_token.clone().map(WireSecret),
                                id_token: WireSecret(credentials.id_token.clone()),
                                scopes: credentials.scopes.clone(),
                                expires_at: credentials.expires_at,
                                earliest_refresh_at: credentials.earliest_refresh_at,
                            }
                        }),
                        refresh_pending: registration.refresh_pending,
                        authorization_revision: registration.authorization_revision,
                    })
                    .collect(),
            }
        }

        fn into_state(self) -> StoredAuth {
            StoredAuth {
                host_id: self.host_id,
                registrations: self
                    .registrations
                    .into_iter()
                    .map(|registration| StoredRegistration {
                        id: registration.id,
                        issuer: registration.issuer,
                        subject: registration.subject,
                        client_id: registration.client_id,
                        email: registration.email,
                        credentials: registration.credentials.map(|credentials| {
                            StoredCredentials {
                                access_token: credentials.access_token.map(|token| token.0),
                                refresh_token: credentials.refresh_token.map(|token| token.0),
                                id_token: credentials.id_token.0,
                                scopes: credentials.scopes,
                                expires_at: credentials.expires_at,
                                earliest_refresh_at: credentials.earliest_refresh_at,
                            }
                        }),
                        refresh_pending: registration.refresh_pending,
                        authorization_revision: registration.authorization_revision,
                    })
                    .collect(),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::os::unix::fs::symlink;

        use super::*;

        fn sample() -> StoredAuth {
            StoredAuth {
                host_id: "test-host".into(),
                registrations: vec![StoredRegistration {
                    id: "registration".into(),
                    issuer: "https://issuer.example".into(),
                    subject: "subject".into(),
                    client_id: "client".into(),
                    email: Some("test@example.com".into()),
                    credentials: Some(StoredCredentials {
                        access_token: Some(SecretString::new("secret-access")),
                        refresh_token: Some(SecretString::new("secret-refresh")),
                        id_token: SecretString::new("secret-identity"),
                        scopes: vec!["openid".into()],
                        expires_at: Some(100),
                        earliest_refresh_at: Some(90),
                    }),
                    refresh_pending: false,
                    authorization_revision: 7,
                }],
            }
        }

        fn private_file(path: &Path, contents: &[u8]) -> Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(contents)?;
            Ok(())
        }

        #[tokio::test]
        async fn private_atomic_roundtrip_preserves_lock_inode() -> Result<()> {
            let temporary = tempfile::tempdir()?;
            let path = temporary.path().join("auth");
            let store = FileStore::open(&path)?;
            assert_eq!(fs::metadata(&path)?.mode() & 0o7777, 0o700);
            let lock_inode = fs::metadata(path.join(".lock"))?.ino();
            let mut transaction = store.lock(CancellationToken::new()).await?;
            assert!(transaction.load()?.registrations.is_empty());
            transaction.save(&sample())?;
            assert_eq!(
                fs::metadata(path.join("state.json"))?.mode() & 0o7777,
                0o600
            );
            let restored = transaction.load()?;
            assert_eq!(restored.host_id, "test-host");
            assert_eq!(restored.registrations[0].authorization_revision, 7);
            let credentials = restored.registrations[0]
                .credentials
                .as_ref()
                .context("missing credentials")?;
            assert_eq!(
                credentials
                    .refresh_token
                    .as_ref()
                    .context("missing refresh token")?
                    .expose_secret(),
                "secret-refresh"
            );
            let mut pending = sample();
            pending.registrations[0].refresh_pending = true;
            transaction.save(&pending)?;
            assert!(transaction.load()?.registrations[0].refresh_pending);
            fs::write(
                path.join("state.json"),
                br#"{
                    "version": 1,
                    "host_id": "test-host",
                    "registrations": [{
                        "id": "registration",
                        "issuer": "https://issuer.example",
                        "subject": "subject",
                        "client_id": "client",
                        "email": null,
                        "credentials": null,
                        "refresh_pending": false
                    }]
                }"#,
            )?;
            let migrated = transaction.load()?;
            assert_eq!(migrated.registrations[0].authorization_revision, 0);
            transaction.save(&migrated)?;
            assert_eq!(
                transaction.load()?.registrations[0].authorization_revision,
                0
            );
            assert_eq!(fs::metadata(path.join(".lock"))?.ino(), lock_inode);
            assert_eq!(fs::read_dir(&path)?.count(), 2);
            Ok(())
        }

        #[test]
        fn wrong_modes_and_owners_are_rejected() -> Result<()> {
            let temporary = tempfile::tempdir()?;
            let path = temporary.path().join("auth");
            let store = FileStore::open(&path)?;
            let metadata = fs::metadata(&path)?;
            assert!(check_directory(&metadata, metadata.uid().wrapping_add(1)).is_err());
            let lock = fs::metadata(path.join(".lock"))?;
            assert!(check_file(&lock, lock.uid().wrapping_add(1)).is_err());
            fs::set_permissions(path.join(".lock"), Permissions::from_mode(0o644))?;
            assert!(store.location.validate().is_err());
            assert!(FileStore::open(&path).is_err());
            fs::set_permissions(path.join(".lock"), Permissions::from_mode(0o600))?;
            fs::set_permissions(&path, Permissions::from_mode(0o755))?;
            assert!(FileStore::open(&path).is_err());
            Ok(())
        }

        #[tokio::test]
        async fn symlinks_hardlinks_and_replaced_lock_inodes_fail_closed() -> Result<()> {
            let temporary = tempfile::tempdir()?;
            let real = temporary.path().join("real");
            FileStore::open(&real)?;
            let alias = temporary.path().join("alias");
            symlink(&real, &alias)?;
            assert!(FileStore::open(&alias).is_err());
            let store = FileStore::open(&real)?;
            let target = temporary.path().join("target");
            private_file(&target, b"not credentials")?;
            symlink(&target, real.join("state.json"))?;
            let mut transaction = store.lock(CancellationToken::new()).await?;
            assert!(transaction.load().is_err());
            assert!(transaction.save(&sample()).is_err());
            fs::remove_file(real.join("state.json"))?;
            fs::hard_link(&target, real.join("state.json"))?;
            assert!(transaction.load().is_err());
            fs::remove_file(real.join("state.json"))?;
            drop(transaction);
            fs::rename(real.join(".lock"), real.join("old-lock"))?;
            private_file(&real.join(".lock"), b"")?;
            assert!(store.lock(CancellationToken::new()).await.is_err());
            Ok(())
        }

        #[tokio::test]
        async fn corrupt_oversized_and_invalid_state_never_replace_valid_credentials() -> Result<()>
        {
            let temporary = tempfile::tempdir()?;
            let path = temporary.path().join("auth");
            let store = FileStore::open(&path)?;
            let mut transaction = store.lock(CancellationToken::new()).await?;
            transaction.save(&sample())?;
            let state_path = path.join("state.json");
            let before = fs::read(&state_path)?;
            let mut invalid = sample();
            invalid.registrations.push(invalid.registrations[0].clone());
            assert!(transaction.save(&invalid).is_err());
            invalid.registrations = vec![sample().registrations[0].clone(); 65];
            assert!(transaction.save(&invalid).is_err());
            invalid = sample();
            invalid.host_id = "x".repeat(MAX_STORE_BYTES);
            assert!(transaction.save(&invalid).is_err());
            invalid = sample();
            invalid.registrations[0]
                .credentials
                .as_mut()
                .context("missing test credentials")?
                .access_token = Some(SecretString::new("\"".repeat(MAX_STORE_BYTES / 2)));
            assert!(transaction.save(&invalid).is_err());
            assert_eq!(fs::read(&state_path)?, before);
            fs::write(&state_path, b"{\"secret-value\": \"do not print me\"}")?;
            let error = transaction
                .load()
                .err()
                .context("corrupt state was accepted")?;
            assert!(!format!("{error:#}").contains("do not print me"));
            assert!(!format!("{error:#}").contains("secret-value"));
            fs::write(&state_path, vec![b'x'; MAX_STORE_BYTES + 1])?;
            assert!(transaction.load().is_err());
            assert!(FileStore::open(&path).is_err());
            Ok(())
        }

        #[tokio::test]
        async fn independent_handles_hold_exclusivity_until_drop_and_waits_are_bounded()
        -> Result<()> {
            let temporary = tempfile::tempdir()?;
            let path = temporary.path().join("auth");
            let first = FileStore::open(&path)?;
            let second = FileStore::open(&path)?;
            let transaction = first.lock(CancellationToken::new()).await?;
            let cancel = CancellationToken::new();
            let cancellation = cancel.clone();
            let waiting = second.lock(cancel);
            tokio::pin!(waiting);
            assert!(
                tokio::time::timeout(Duration::from_millis(35), &mut waiting)
                    .await
                    .is_err()
            );
            cancellation.cancel();
            assert!(
                tokio::time::timeout(Duration::from_millis(250), waiting)
                    .await?
                    .is_err()
            );
            let timed_out = second
                .lock_with_timeout(CancellationToken::new(), Duration::from_millis(35))
                .await
                .err()
                .context("contended credential lock did not time out")?;
            assert!(timed_out.to_string().contains("timed out"));
            drop(transaction);
            let _next = tokio::time::timeout(
                Duration::from_millis(250),
                second.lock(CancellationToken::new()),
            )
            .await??;
            Ok(())
        }

        #[tokio::test]
        async fn transaction_lock_is_exclusive_across_processes() -> Result<()> {
            const PROBE_PATH: &str = "EJM_HARNESS_AUTH_STORE_LOCK_PROBE_PATH";
            const PROBE_HELD: &str = "EJM_HARNESS_AUTH_STORE_LOCK_PROBE_HELD";
            if let Some(path) = std::env::var_os(PROBE_PATH) {
                let store = FileStore::open(path)?;
                let lock = open_private_file(
                    &store.location.path.join(".lock"),
                    store.location.owner,
                    true,
                )?;
                let result = lock.try_lock();
                if std::env::var_os(PROBE_HELD).is_some() {
                    ensure!(
                        matches!(result, Err(TryLockError::WouldBlock)),
                        "other process acquired a held credential lock"
                    );
                } else {
                    result.map_err(anyhow::Error::from)?;
                }
                return Ok(());
            }
            let temporary = tempfile::tempdir()?;
            let path = temporary.path().join("auth");
            let store = FileStore::open(&path)?;
            let transaction = store.lock(CancellationToken::new()).await?;
            let probe = |held: bool| {
                let mut command = tokio::process::Command::new(std::env::current_exe()?);
                command
                    .args([
                        "--exact",
                        "store::native::tests::transaction_lock_is_exclusive_across_processes",
                    ])
                    .env(PROBE_PATH, &path)
                    .env_remove(PROBE_HELD)
                    .kill_on_drop(true);
                if held {
                    command.env(PROBE_HELD, "1");
                }
                Ok::<_, anyhow::Error>(command)
            };
            let output =
                tokio::time::timeout(Duration::from_secs(5), probe(true)?.output()).await??;
            ensure!(
                output.status.success(),
                "credential lock subprocess check failed"
            );
            drop(transaction);
            let output =
                tokio::time::timeout(Duration::from_secs(5), probe(false)?.output()).await??;
            ensure!(
                output.status.success(),
                "dropped credential lock remained held in another process"
            );
            Ok(())
        }
    }
}

/// File credentials require Unix owner and permission checks.
#[cfg(not(unix))]
pub struct FileStore;

#[cfg(not(unix))]
impl FileStore {
    /// # Errors
    /// Always returns an error because private file storage is unsupported here.
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        anyhow::bail!("private credential file storage requires Unix ownership checks")
    }
}

#[cfg(not(unix))]
#[async_trait]
impl CredentialStore for FileStore {
    async fn lock(&self, _cancel: CancellationToken) -> Result<Box<dyn CredentialTransaction>> {
        anyhow::bail!("private credential file storage requires Unix ownership checks")
    }
}
