use anyhow::Result;
use ejm_harness_packages::{PackageGrants, PackageManager, pack_directory};
use serde_json::json;

fn fixture(root: &std::path::Path, version: &str) -> Result<()> {
    std::fs::write(
        root.join("ejm.package.json"),
        serde_json::to_vec(&json!({
            "schema_version":1,"name":"@example/test","version":version,"description":"Installation fixture",
            "runtime":{"kind":"node","entry":"extension.mjs"},
            "exports":{"commands":[{"name":"hello","description":"Hello","handler":"hello"}]}
        }))?,
    )?;
    std::fs::write(
        root.join("extension.mjs"),
        "throw new Error('must not execute on install'); export function hello(){ return 'hello'; }",
    )?;
    Ok(())
}

#[test]
fn install_never_executes_and_update_clears_trust() -> Result<()> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    let manager = PackageManager::new(store.path().join("packages"))?;
    let first = manager.install(&pack_directory(source.path())?, "local:fixture")?;
    assert!(!first.enabled);
    assert!(manager.enabled()?.is_empty());
    let grants = PackageGrants {
        commands: vec!["hello".into()],
        ..PackageGrants::default()
    };
    manager.enable("@example/test", grants)?;
    assert_eq!(manager.enabled()?.len(), 1);
    fixture(source.path(), "2.0.0")?;
    let next = manager.install(&pack_directory(source.path())?, "local:fixture")?;
    assert!(!next.enabled && next.grants.commands.is_empty());
    assert_eq!(manager.list()?.len(), 1);
    assert!(manager.enabled()?.is_empty());
    Ok(())
}

#[test]
fn changed_content_and_undeclared_grants_fail_before_loading() -> Result<()> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    let manager = PackageManager::new(store.path().join("packages"))?;
    let record = manager.install(&pack_directory(source.path())?, "local:fixture")?;
    assert!(
        manager
            .enable(
                "@example/test",
                PackageGrants {
                    commands: vec!["hidden".into()],
                    ..PackageGrants::default()
                }
            )
            .is_err()
    );
    let installation = manager.open(&record)?;
    std::fs::write(installation.root.join("extension.mjs"), "changed")?;
    assert!(manager.open(&record).is_err());
    Ok(())
}

#[test]
fn unexpected_import_files_and_directories_fail_before_loading() -> Result<()> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    std::fs::write(
        source.path().join("extension.mjs"),
        "import './injected.mjs'; export function hello() { return 'hello'; }",
    )?;
    let manager = PackageManager::new(store.path().join("packages"))?;
    let record = manager.install(&pack_directory(source.path())?, "local:fixture")?;
    let installation = manager.open(&record)?;
    let injected = installation.root.join("injected.mjs");
    std::fs::write(&injected, "throw new Error('unverified imported code');")?;
    assert!(manager.open(&record).is_err());
    std::fs::remove_file(injected)?;
    assert!(manager.open(&record).is_ok());
    std::fs::create_dir(installation.root.join("unexpected"))?;
    assert!(manager.open(&record).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn snapshot_and_source_directory_links_are_rejected() -> Result<()> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    for directory in ["actual", "linked"] {
        std::fs::create_dir(source.path().join(directory))?;
        std::fs::write(
            source.path().join(directory).join("helper.mjs"),
            "export const value = 1;",
        )?;
    }
    let manager = PackageManager::new(store.path().join("packages"))?;
    let record = manager.install(&pack_directory(source.path())?, "local:fixture")?;
    let installation = manager.open(&record)?;
    let linked = installation.root.join("linked");
    std::fs::remove_dir_all(&linked)?;
    std::os::unix::fs::symlink(installation.root.join("actual"), &linked)?;
    assert!(manager.open(&record).is_err());
    std::fs::remove_dir_all(source.path().join("linked"))?;
    std::os::unix::fs::symlink(source.path().join("actual"), source.path().join("linked"))?;
    assert!(pack_directory(source.path()).is_err());
    Ok(())
}

#[test]
fn package_packing_rejects_depth_overflow() -> Result<()> {
    let source = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    let mut nested = source.path().to_path_buf();
    for _ in 0..31 {
        nested.push("d");
    }
    std::fs::create_dir_all(&nested)?;
    std::fs::write(nested.join("allowed.mjs"), "export const value = 1;")?;
    assert!(pack_directory(source.path()).is_ok());
    nested.push("d");
    std::fs::create_dir(&nested)?;
    std::fs::write(nested.join("omitted.mjs"), "export const value = 2;")?;
    assert!(pack_directory(source.path()).is_err());
    Ok(())
}

#[test]
fn package_packing_counts_directories_toward_traversal_limit() -> Result<()> {
    let source = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    for index in 0..ejm_harness_package_spec::MAX_ARCHIVE_FILES - 2 {
        std::fs::create_dir(source.path().join(format!("directory-{index}")))?;
    }
    assert!(pack_directory(source.path()).is_ok());
    std::fs::create_dir(source.path().join("one-too-many"))?;
    assert!(pack_directory(source.path()).is_err());
    Ok(())
}

#[test]
fn package_packing_is_independent_of_creation_order() -> Result<()> {
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    fixture(first.path(), "1.0.0")?;
    fixture(second.path(), "1.0.0")?;
    let files = ["z/last.mjs", "a/first.mjs", "middle.mjs"];
    for (root, reverse) in [(first.path(), false), (second.path(), true)] {
        let mut order = files;
        if reverse {
            order.reverse();
        }
        for relative in order {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, relative)?;
        }
    }
    assert_eq!(
        pack_directory(first.path())?,
        pack_directory(second.path())?
    );
    Ok(())
}

#[test]
fn package_snapshot_is_deterministic_and_removal_keeps_active_files() -> Result<()> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    fixture(source.path(), "1.0.0")?;
    let first = pack_directory(source.path())?;
    assert_eq!(first, pack_directory(source.path())?);
    let manager = PackageManager::new(store.path().join("packages"))?;
    let record = manager.install(&first, "local:fixture")?;
    let installation = manager.open(&record)?;
    manager.remove("@example/test")?;
    assert!(manager.list()?.is_empty());
    assert!(installation.root.join("extension.mjs").is_file());
    Ok(())
}
