//! Deterministic publication and privacy failures at the real permanent-name boundary.

use std::{
    fs::{File, TryLockError},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::sync::{Arc, Mutex};

use rightkit_fs::{FileKey, FsError, handle::link_count, state::testing::on_empty_created};

use super::super::wire;

struct FixtureChild(Child);

impl FixtureChild {
    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "fixture child did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for FixtureChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn identity(file: &File) -> FileKey {
    FileKey::of_file(file, Path::new("startup.lock")).unwrap()
}

#[test]
fn creator_death_preserves_the_first_visible_lock_identity() {
    const CHILD_ROOT: &str = "EJM_HARNESS_NATIVE_LOCK_FIXTURE_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let directory = wire::directory(&root, "creator-death").unwrap();
        let _hook = on_empty_created(move |created| {
            created.try_lock().unwrap();
            std::fs::write(root.join("created"), b"ready").unwrap();
            std::thread::sleep(Duration::from_secs(10));
            Err(FsError::Unavailable("parent did not terminate the fixture"))
        });
        wire::stable_lock(&directory, "startup.lock").unwrap();
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let path = root.path().canonicalize().unwrap();
    let directory = wire::directory(&path, "creator-death").unwrap();
    let mut child = FixtureChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native::tests::stable_locks::creator_death_preserves_the_first_visible_lock_identity",
                "--nocapture",
            ])
            .env(CHILD_ROOT, &path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.join("created").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "creator exited early"
        );
        assert!(
            Instant::now() < deadline,
            "creator did not reach publication"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    let peer = wire::stable_lock(&directory, "startup.lock").unwrap();
    let original = identity(&peer);
    assert_eq!(link_count(&peer).unwrap(), 1);
    assert_eq!(directory.list(8).unwrap(), vec!["startup.lock"]);
    assert!(matches!(peer.try_lock(), Err(TryLockError::WouldBlock)));

    child.0.kill().unwrap();
    assert!(!child.wait().success());
    let recovered = wire::stable_lock(&directory, "startup.lock").unwrap();
    assert_eq!(identity(&recovered), original);
    assert_eq!(link_count(&recovered).unwrap(), 1);
    recovered.try_lock().unwrap();
    assert!(matches!(peer.try_lock(), Err(TryLockError::WouldBlock)));
    recovered.unlock().unwrap();
    peer.try_lock().unwrap();
    eprintln!(
        "permanent-lock creator {} was killed and reaped",
        child.0.id()
    );
}

#[cfg(unix)]
#[test]
fn failed_creation_keeps_the_inode_and_private_reopen_refuses_it() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let directory =
        wire::directory(&root.path().canonicalize().unwrap(), "permission-failure").unwrap();
    let captured = Arc::new(Mutex::new(None));
    let key = Arc::clone(&captured);
    let hook = on_empty_created(move |created| {
        *key.lock().unwrap() = Some(identity(created));
        created
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .unwrap();
        Ok(())
    });
    let error = wire::stable_lock(&directory, "startup.lock").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<FsError>(),
        Some(FsError::TooPermissive(_))
    ));
    drop(hook);
    let original = captured.lock().unwrap().unwrap();
    let error = wire::stable_lock(&directory, "startup.lock").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<FsError>(),
        Some(FsError::TooPermissive(_))
    ));
    let retained = directory
        .open_read("startup.lock", &rightkit_fs::handle::ReadOptions::new())
        .unwrap();
    assert_eq!(identity(&retained), original);
    assert_eq!(
        retained.metadata().unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[cfg(target_os = "macos")]
#[test]
fn failed_acl_creation_keeps_the_inode_and_private_reopen_refuses_it() {
    let root = tempfile::tempdir().unwrap();
    let directory = wire::directory(&root.path().canonicalize().unwrap(), "acl-failure").unwrap();
    let path = directory.path().join("startup.lock");
    let captured = Arc::new(Mutex::new(None));
    let key = Arc::clone(&captured);
    let hook = on_empty_created(move |created| {
        *key.lock().unwrap() = Some(identity(created));
        let mut chmod = FixtureChild(
            Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(chmod.wait().success());
        Ok(())
    });
    let error = wire::stable_lock(&directory, "startup.lock").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<FsError>(),
        Some(FsError::UnsafeAcl(_))
    ));
    drop(hook);
    let original = captured.lock().unwrap().unwrap();
    let error = wire::stable_lock(&directory, "startup.lock").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<FsError>(),
        Some(FsError::UnsafeAcl(_))
    ));
    let retained = directory
        .open_read("startup.lock", &rightkit_fs::handle::ReadOptions::new())
        .unwrap();
    assert_eq!(identity(&retained), original);
    assert_eq!(directory.list(8).unwrap(), vec!["startup.lock"]);
}
