//! TEMP: diagnostic for os error 32 creating the package `objects` child on Windows. Remove before merge.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::panic, reason = "temporary diagnostic")]

use std::path::Path;

use rightkit_fs::{StateDir, private};

type Setup = fn(&Path) -> Vec<String>;

fn step(label: &str, setup: Setup) -> String {
    let temp = tempfile::tempdir().unwrap();
    let root = std::path::absolute(temp.path().join("packages")).unwrap();
    let directory = match StateDir::create(&root) {
        Ok(directory) => directory,
        Err(error) => return format!("{label}: create {error:?}"),
    };
    let steps = setup(directory.path());
    format!(
        "{label}: setup {steps:?} -> child {:?}",
        directory.child("objects").map(|_| ())
    )
}

fn verify(path: &Path) -> String {
    format!("verify {:?}", private::verify_owner_only(path))
}

fn restrict(path: &Path) -> String {
    format!("restrict {:?}", private::restrict_owner_only(path))
}

fn strict(path: &Path) -> String {
    format!("strict {:?}", private::is_owner_only_strict(path))
}

#[test]
fn objects_child_probe() {
    let results = [
        step("A plain", |_| Vec::new()),
        step("B verify", |p| vec![verify(p)]),
        step("C restrict", |p| vec![restrict(p)]),
        step("D strict", |p| vec![strict(p)]),
        step("E restrict+strict", |p| vec![restrict(p), strict(p)]),
        step("F protect", |p| vec![verify(p), restrict(p), strict(p)]),
    ];
    panic!("probe results:\n{}", results.join("\n"));
}
