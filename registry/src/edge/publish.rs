use std::collections::HashMap;

use ejm_harness_package_spec::{Manifest, archive_sha256, validate_archive};
use worker::{Conditional, Env, Request, Response, Result, RouteContext};

use super::{
    account::{identity, mutation_allowed, owns},
    body::read_body,
    limits::{global_upload_allowed, rate},
    origin, package_parameters, parameter, problem,
    versions::{
        VersionRow, archive_key, archive_matches, public_version, publication_value,
        reserved_version,
    },
};
use crate::policy::{Identity, MAX_METADATA_BYTES, Publication, valid_version};

/// Reserves an exact version. Retrying the identical reservation returns it unchanged.
pub(super) async fn reserve(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !mutation_allowed(&req, &ctx.env)? {
        return problem(403, "origin_or_credentials_required");
    }
    let Some(identity) = identity(&req, &ctx.env).await? else {
        return problem(401, "sign_in_required");
    };
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(400, "invalid_package");
    };
    if !owns(&ctx.env, &identity, publisher).await? {
        return problem(403, "publisher_unavailable");
    }
    if !rate(&ctx.env, &identity, "reserve", 3600, 20, 0, 0).await? {
        return problem(429, "rate_limited");
    }
    let Some(bytes) = read_body(&mut req, MAX_METADATA_BYTES).await? else {
        return problem(413, "body_too_large");
    };
    let Some(input) = serde_json::from_slice::<Publication>(&bytes)
        .ok()
        .filter(|input| input.valid_for(publisher, name))
    else {
        return problem(400, "invalid_publication");
    };
    let manifest_json = serde_json::to_string(&input.manifest)?;
    let version = &input.manifest.version;
    if let Some(row) = reserved_version(&ctx.env, publisher, name, version).await? {
        return reservation_response(&ctx.env, &row, &manifest_json, &input);
    }
    let inserted = ctx
        .env
        .d1("DB")?
        .prepare("INSERT INTO versions(publisher,name,version,manifest_json,description,sha256,size) VALUES (?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(&[
            publisher.into(),
            name.into(),
            version.as_str().into(),
            manifest_json.as_str().into(),
            input.manifest.description.as_str().into(),
            input.sha256.as_str().into(),
            input.size.into(),
        ])?
        .run()
        .await;
    if inserted.is_err() {
        return problem(409, "publication_unavailable_or_quota_reached");
    }
    let Some(row) = reserved_version(&ctx.env, publisher, name, version).await? else {
        return problem(409, "publication_unavailable");
    };
    reservation_response(&ctx.env, &row, &manifest_json, &input)
}

/// Verifies and stores the reserved archive, then marks the version ready.
pub(super) async fn upload(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !mutation_allowed(&req, &ctx.env)? {
        return problem(403, "origin_or_credentials_required");
    }
    let Some(identity) = identity(&req, &ctx.env).await? else {
        return problem(401, "sign_in_required");
    };
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(400, "invalid_package");
    };
    let version = parameter(&ctx, "version");
    if !valid_version(version) {
        return problem(400, "invalid_version");
    }
    if !owns(&ctx.env, &identity, publisher).await? {
        return problem(403, "publisher_unavailable");
    }
    let Some(row) = reserved_version(&ctx.env, publisher, name, version).await? else {
        return problem(404, "reservation_required");
    };
    if row.quarantined != 0 {
        return problem(403, "version_unavailable");
    }
    if let Some(rejection) = reject_over_budget(&ctx.env, &identity, row.size).await? {
        return Ok(rejection);
    }
    let Some(bytes) = read_body(&mut req, row.size as usize).await? else {
        return problem(413, "body_too_large");
    };
    if let Some(rejection) = reject_invalid_archive(&bytes, &row)? {
        return Ok(rejection);
    }
    let key = archive_key(publisher, name, version);
    if let Some(rejection) = store_archive(&ctx.env, &row, &key, bytes).await? {
        return Ok(rejection);
    }
    ctx.env
        .d1("DB")?
        .prepare("UPDATE versions SET ready=1,published_at=coalesce(published_at,unixepoch()) WHERE publisher=? AND name=? AND version=? AND quarantined=0 AND EXISTS (SELECT 1 FROM publishers p JOIN registry_accounts a ON a.id=p.user_id WHERE p.handle=versions.publisher AND p.user_id=? AND p.suspended=0 AND a.suspended=0)")
        .bind(&[publisher.into(), name.into(), version.into(), identity.id.into()])?
        .run()
        .await?;
    let Some(row) = public_version(&ctx.env, publisher, name, version).await? else {
        return problem(403, "publication_unavailable");
    };
    Response::from_json(&publication_value(&row, &origin(&ctx.env)?)?)
}

/// Versions are immutable: a matching, unquarantined row is the reservation, any other row conflicts.
fn reservation_response(
    env: &Env,
    row: &VersionRow,
    manifest_json: &str,
    input: &Publication,
) -> Result<Response> {
    if row.manifest_json != manifest_json
        || row.sha256 != input.sha256
        || row.size != input.size
        || row.quarantined != 0
    {
        return problem(409, "version_immutable");
    }
    Response::from_json(&publication_value(row, &origin(env)?)?)
}

/// Charges the account and registry upload budgets. Some(response) when either is exhausted.
async fn reject_over_budget(env: &Env, identity: &Identity, size: u32) -> Result<Option<Response>> {
    if !rate(env, identity, "upload", 3600, 40, 0, 0).await?
        || !rate(
            env,
            identity,
            "upload-bytes",
            86400,
            40,
            size,
            64 * 1024 * 1024,
        )
        .await?
    {
        return problem(429, "rate_limited").map(Some);
    }
    if !global_upload_allowed(env, size).await? {
        return problem(429, "registry_upload_budget_reached").map(Some);
    }
    Ok(None)
}

/// Some(response) unless the bytes are the reserved archive with the reserved manifest.
fn reject_invalid_archive(bytes: &[u8], row: &VersionRow) -> Result<Option<Response>> {
    if bytes.len() != row.size as usize || archive_sha256(bytes) != row.sha256 {
        return problem(400, "archive_integrity_mismatch").map(Some);
    }
    let Ok(manifest) = validate_archive(bytes) else {
        return problem(400, "invalid_archive").map(Some);
    };
    if manifest != serde_json::from_str::<Manifest>(&row.manifest_json)? {
        return problem(400, "archive_manifest_mismatch").map(Some);
    }
    Ok(None)
}

/// Creates the object once. A retry may repair an interrupted finalization only when the
/// existing object already matches the reservation. It never replaces another archive.
async fn store_archive(
    env: &Env,
    row: &VersionRow,
    key: &str,
    bytes: Vec<u8>,
) -> Result<Option<Response>> {
    let bucket = env.bucket("PACKAGES")?;
    let metadata = HashMap::from([("sha256".to_owned(), row.sha256.clone())]);
    let created = bucket
        .put(key, bytes)
        .custom_metadata(metadata)
        .only_if(Conditional {
            etag_does_not_match: Some("*".into()),
            ..Conditional::default()
        })
        .execute()
        .await?;
    if created.is_some() {
        return Ok(None);
    }
    let Some(object) = bucket.head(key).await? else {
        return problem(503, "archive_unavailable").map(Some);
    };
    if archive_matches(&object, row)? {
        Ok(None)
    } else {
        problem(409, "version_immutable").map(Some)
    }
}
