use ejm_harness_package_spec::Manifest;
use serde::Deserialize;
use serde_json::{Value, json};
use worker::{Env, Object, Result};

/// Visible to the public: ready, not quarantined, and owned by an active publisher and account.
pub(super) const ACTIVE: &str =
    "v.ready = 1 AND v.quarantined = 0 AND p.suspended = 0 AND a.suspended = 0";

#[derive(Deserialize)]
pub(super) struct VersionRow {
    publisher: String,
    name: String,
    pub(super) version: String,
    pub(super) manifest_json: String,
    pub(super) sha256: String,
    pub(super) size: u32,
    ready: u8,
    pub(super) quarantined: u8,
}

/// The version only if the public may see it.
pub(super) async fn public_version(
    env: &Env,
    publisher: &str,
    name: &str,
    version: &str,
) -> Result<Option<VersionRow>> {
    version_row(env, publisher, name, version, ACTIVE).await
}

/// The version in any state, including pending and quarantined.
pub(super) async fn reserved_version(
    env: &Env,
    publisher: &str,
    name: &str,
    version: &str,
) -> Result<Option<VersionRow>> {
    version_row(env, publisher, name, version, "1 = 1").await
}

async fn version_row(
    env: &Env,
    publisher: &str,
    name: &str,
    version: &str,
    condition: &str,
) -> Result<Option<VersionRow>> {
    env.d1("DB")?
        .prepare(format!("SELECT v.* FROM versions v JOIN publishers p ON p.handle = v.publisher JOIN registry_accounts a ON a.id = p.user_id WHERE v.publisher = ? AND v.name = ? AND v.version = ? AND {condition}"))
        .bind(&[publisher.into(), name.into(), version.into()])?
        .first::<VersionRow>(None)
        .await
}

pub(super) fn version_value(row: &VersionRow, registry_origin: &str) -> Result<Value> {
    let manifest: Manifest = serde_json::from_str(&row.manifest_json)?;
    Ok(json!({
        "manifest":manifest,"sha256":row.sha256,"size":row.size,
        "download_url":version_url(registry_origin, row, "download"),
    }))
}

pub(super) fn publication_value(row: &VersionRow, registry_origin: &str) -> Result<Value> {
    let mut value = version_value(row, registry_origin)?;
    value["status"] = json!(if row.ready == 1 { "ready" } else { "pending" });
    value["upload_url"] = json!(version_url(registry_origin, row, "archive"));
    Ok(value)
}

fn version_url(registry_origin: &str, row: &VersionRow, leaf: &str) -> String {
    format!(
        "{registry_origin}/v1/packages/{}/{}/versions/{}/{leaf}",
        row.publisher, row.name, row.version
    )
}

pub(super) fn archive_key(publisher: &str, name: &str, version: &str) -> String {
    format!("{publisher}/{name}/{version}.tar.gz")
}

/// True when the stored object has the reserved size and SHA-256.
pub(super) fn archive_matches(object: &Object, row: &VersionRow) -> Result<bool> {
    Ok(object.size() == u64::from(row.size)
        && object.custom_metadata()?.get("sha256") == Some(&row.sha256))
}
