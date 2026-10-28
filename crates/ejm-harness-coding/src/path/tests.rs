use cap_std::ambient_authority;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    edit::{Edit, edit},
    list::list_files,
    read::{read_file, sha256_hex},
    search::search,
};

#[test]
fn reads_cannot_traverse_parent_or_symlink_outside_workspace() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("private"), "secret")?;
    let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
    assert!(read_file(&root, &json!({"path":"../private"})).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape"))?;
        assert!(read_file(&root, &json!({"path":"escape/private"})).is_err());
        let cancel = CancellationToken::new();
        assert!(
            edit(
                &root,
                &json!({"path":"escape/new","content":"bad"}),
                Edit::Write,
                &cancel
            )
            .is_err()
        );
        assert!(!outside.path().join("new").exists());
    }
    Ok(())
}

#[test]
fn reads_listings_and_search_reject_protected_names_and_case_variants() -> Result<()> {
    for name in [".git", ".GIT", ".ejm-harness", ".EjM-HaRnEsS"] {
        let temp = tempfile::tempdir()?;
        std::fs::create_dir(temp.path().join(name))?;
        std::fs::write(temp.path().join(name).join("state.txt"), "fixture needle\n")?;
        std::fs::write(temp.path().join("visible.txt"), "ordinary needle\n")?;
        let cwd = temp.path().canonicalize()?;
        let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
        let cancel = CancellationToken::new();
        let file = format!("{name}/state.txt");
        assert!(
            read_file(&root, &json!({"path":file})).is_err(),
            "allowed {file}"
        );
        assert!(
            list_files(&root, &json!({"path":name})).is_err(),
            "listed {name}"
        );
        assert!(
            search(
                &root,
                &cwd,
                &json!({"path":name,"pattern":"needle"}),
                &cancel
            )
            .is_err(),
            "searched {name}"
        );
        let listing = list_files(&root, &json!({}))?;
        assert_eq!(listing.content, "visible.txt");
        assert!(
            read_file(&root, &json!({"path":"visible.txt"}))?
                .content
                .contains("ordinary")
        );
        let output = search(&root, &cwd, &json!({"pattern":"needle"}), &cancel)?;
        assert!(output.content.contains("ordinary needle"));
        assert!(!output.content.contains("fixture needle"));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn reads_listings_and_search_reject_protected_aliases_but_allow_workspace_aliases() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let cwd = temp.path().canonicalize()?;
    for (directory, alias) in [(".git", "metadata"), (".ejm-harness", "state")] {
        std::fs::create_dir(cwd.join(directory))?;
        std::fs::write(cwd.join(directory).join("record.txt"), "private fixture\n")?;
        std::os::unix::fs::symlink(directory, cwd.join(alias))?;
        std::os::unix::fs::symlink(
            format!("{directory}/record.txt"),
            cwd.join(format!("{alias}-file")),
        )?;
    }
    std::fs::create_dir(cwd.join("ordinary"))?;
    std::fs::write(cwd.join("ordinary/visible.txt"), "ordinary needle\n")?;
    std::os::unix::fs::symlink("ordinary", cwd.join("allowed"))?;
    let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
    let cancel = CancellationToken::new();
    for alias in ["metadata", "state"] {
        for path in [format!("{alias}/record.txt"), format!("{alias}-file")] {
            assert!(
                read_file(&root, &json!({"path":path})).is_err(),
                "read {path}"
            );
            assert!(
                search(
                    &root,
                    &cwd,
                    &json!({"path":path,"pattern":"fixture"}),
                    &cancel
                )
                .is_err(),
                "searched {path}"
            );
        }
        assert!(
            list_files(&root, &json!({"path":alias})).is_err(),
            "listed {alias}"
        );
        assert!(
            search(
                &root,
                &cwd,
                &json!({"path":alias,"pattern":"fixture"}),
                &cancel
            )
            .is_err(),
            "searched {alias}"
        );
    }
    assert!(
        read_file(&root, &json!({"path":"allowed/visible.txt"}))?
            .content
            .contains("ordinary needle")
    );
    assert_eq!(
        list_files(&root, &json!({"path":"allowed"}))?.content,
        "visible.txt"
    );
    let output = search(
        &root,
        &cwd,
        &json!({"path":"allowed","pattern":"needle"}),
        &cancel,
    )?;
    assert!(output.content.contains("ordinary needle"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn writes_reject_metadata_aliases_and_case_variants() -> Result<()> {
    let temp = tempfile::tempdir()?;
    std::fs::create_dir(temp.path().join(".git"))?;
    std::fs::write(temp.path().join(".git/config"), "original\n")?;
    std::os::unix::fs::symlink(".git", temp.path().join("meta"))?;
    std::os::unix::fs::symlink(".git/config", temp.path().join("alias"))?;
    let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
    let cancel = CancellationToken::new();
    for path in [
        "meta/config",
        "meta/new",
        "alias",
        ".GIT/config",
        ".EjM-HaRnEsS/state",
    ] {
        assert!(
            edit(
                &root,
                &json!({"path":path,"content":"changed\n","expected_hash":sha256_hex(b"original\n")}),
                Edit::Write,
                &cancel
            )
            .is_err(),
            "allowed {path}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(temp.path().join(".git/config"))?,
        "original\n"
    );
    assert!(!temp.path().join(".git/new").exists());
    Ok(())
}
