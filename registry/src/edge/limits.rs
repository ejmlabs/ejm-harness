use serde::Deserialize;
use worker::{Env, Result};

use crate::policy::Identity;

#[derive(Deserialize)]
struct Counter {
    count: u32,
}

#[derive(Deserialize)]
struct UploadLimits {
    daily_upload_bytes: u32,
    daily_upload_count: u32,
}

/// Counts one attempt in the account's fixed window. False when the window is exhausted.
pub(super) async fn rate(
    env: &Env,
    identity: &Identity,
    kind: &str,
    seconds: u32,
    limit: u32,
    bytes: u32,
    maximum_bytes: u32,
) -> Result<bool> {
    rate_key(
        env,
        format!("account:{}:{kind}", identity.id),
        seconds,
        limit,
        bytes,
        maximum_bytes,
    )
    .await
}

/// Counts one attempt in the registry-wide upload window. Missing limits deny.
pub(super) async fn global_upload_allowed(env: &Env, bytes: u32) -> Result<bool> {
    let Some(limits) = env
        .d1("DB")?
        .prepare("SELECT daily_upload_bytes,daily_upload_count FROM registry_limits WHERE id=1")
        .first::<UploadLimits>(None)
        .await?
    else {
        return Ok(false);
    };
    rate_key(
        env,
        "global:uploads".into(),
        86400,
        limits.daily_upload_count,
        bytes,
        limits.daily_upload_bytes,
    )
    .await
}

async fn rate_key(
    env: &Env,
    key: String,
    seconds: u32,
    limit: u32,
    bytes: u32,
    maximum_bytes: u32,
) -> Result<bool> {
    if bytes > maximum_bytes || limit == 0 {
        return Ok(false);
    }
    let db = env.d1("DB")?;
    db.prepare("DELETE FROM rate_limits WHERE key IN (SELECT key FROM rate_limits WHERE expires_at < unixepoch() LIMIT 64)")
        .run()
        .await?;
    let result = db
        .prepare("INSERT INTO rate_limits(key,count,bytes,expires_at) VALUES (?,1,?,unixepoch()+?) ON CONFLICT(key) DO UPDATE SET count = CASE WHEN expires_at <= unixepoch() THEN 1 ELSE count + 1 END, bytes = CASE WHEN expires_at <= unixepoch() THEN excluded.bytes ELSE bytes + excluded.bytes END, expires_at = CASE WHEN expires_at <= unixepoch() THEN excluded.expires_at ELSE expires_at END WHERE expires_at <= unixepoch() OR (count < ? AND bytes + excluded.bytes <= ?) RETURNING count")
        .bind(&[key.into(), bytes.into(), seconds.into(), limit.into(), maximum_bytes.into()])?
        .first::<Counter>(None)
        .await?;
    Ok(result.is_some_and(|counter| counter.count > 0))
}
