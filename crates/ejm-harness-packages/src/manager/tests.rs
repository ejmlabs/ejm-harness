use std::fs::TryLockError;

use anyhow::{Context, Result};
use serde_json::json;

use super::{PackageManager, pack_directory};
use crate::PackageGrants;

#[test]
fn operation_lock_releases_with_a_retained_description() -> Result<()> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    let manager = PackageManager::new(store.path().join("packages"))?;
    let archive = |version: &str| -> Result<Vec<u8>> {
        std::fs::write(
            source.path().join(super::MANIFEST_FILE),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "name": "@example/lock",
                "version": version,
                "description": "Registration lock fixture"
            }))?,
        )?;
        pack_directory(source.path())
    };
    manager.install(&archive("1.0.0")?, "local:lock-fixture")?;

    let guard = manager.lock()?;
    let retained = guard.try_clone()?;
    let error = manager
        .enable("@example/lock", PackageGrants::default())
        .err()
        .context("mutation must contend while its operation guard is held")?;
    assert!(matches!(
        error.downcast_ref::<TryLockError>(),
        Some(TryLockError::WouldBlock)
    ));

    drop(guard);
    manager.enable("@example/lock", PackageGrants::default())?;
    assert!(
        manager
            .enable("@example/missing", PackageGrants::default())
            .is_err()
    );
    manager.install(&archive("2.0.0")?, "local:lock-fixture")?;
    drop(retained);
    Ok(())
}
