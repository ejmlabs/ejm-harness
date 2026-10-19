use ejm_harness_package_spec::{Manifest, archive_sha256, validate_archive};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use worker::{
    ByteStream, Conditional, Context, Env, Error, Headers, Method, Request, RequestInit, Response,
    Result, RouteContext, Router, event, wasm_bindgen::JsValue,
};

use crate::policy::{
    Identity, MAX_ARCHIVE_BYTES, MAX_AUTH_BYTES, MAX_METADATA_BYTES, ModerationInput, Publication,
    PublisherInput, mutation_credentials_allowed, valid_component, valid_digest, valid_handle,
    valid_version,
};

const ACTIVE: &str = "v.ready = 1 AND v.quarantined = 0 AND p.suspended = 0 AND a.suspended = 0";

#[derive(Deserialize)]
struct VersionRow {
    publisher: String,
    name: String,
    version: String,
    manifest_json: String,
    sha256: String,
    size: u32,
    ready: u8,
    quarantined: u8,
}

#[derive(Deserialize)]
struct PublisherRow {
    handle: String,
    display_name: String,
    suspended: u8,
}

#[derive(Deserialize)]
struct AccountRow {
    suspended: u8,
}

#[derive(Deserialize)]
struct CatalogRow {
    publisher: String,
    name: String,
    description: String,
    latest_version: String,
}

#[derive(Deserialize)]
struct Counter {
    count: u32,
}

#[derive(Deserialize)]
struct UploadLimits {
    daily_upload_bytes: u32,
    daily_upload_count: u32,
}

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let result = serve(req, env).await;
    let mut response = match result {
        Ok(response) => response,
        Err(_) => problem(503, "service_unavailable")?,
    };
    let headers = response.headers().clone();
    response = response.with_headers(headers);
    for (name, value) in [
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Frame-Options", "DENY"),
        (
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=()",
        ),
        (
            "Content-Security-Policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
        ("Strict-Transport-Security", "max-age=31536000"),
    ] {
        response.headers_mut().set(name, value)?;
    }
    Ok(response)
}

async fn serve(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    if url.as_str().len() > 2048 {
        return problem(414, "url_too_long");
    }
    if url.path().starts_with("/v1/") || url.path().starts_with("/api/auth/") {
        let ip = req
            .headers()
            .get("CF-Connecting-IP")?
            .unwrap_or_else(|| "unknown".into());
        if !env
            .rate_limiter("EDGE_LIMIT")?
            .limit(format!("api:{ip}"))
            .await?
            .success
        {
            return problem(429, "rate_limited");
        }
    }
    Router::new()
        .get_async("/", assets)
        .get_async("/v1/packages", catalog)
        .get_async("/v1/packages/:publisher/:name", package)
        .get_async("/v1/packages/:publisher/:name/versions/:version", version)
        .get_async(
            "/v1/packages/:publisher/:name/versions/:version/download",
            download,
        )
        .get_async("/v1/me", me)
        .post_async("/v1/publishers", claim_publisher)
        .post_async("/v1/packages/:publisher/:name/versions", reserve)
        .put_async(
            "/v1/packages/:publisher/:name/versions/:version/archive",
            upload,
        )
        .post_async("/v1/operator/:kind/:target", moderate)
        .post_async(
            "/v1/operator/versions/:publisher/:name/:version",
            moderate_version,
        )
        .on_async("/api/auth/*path", proxy_auth)
        .or_else_any_method_async("/*path", assets)
        .run(req, env)
        .await
}

fn problem(status: u16, code: &str) -> Result<Response> {
    Ok(Response::from_json(&json!({"error":{"code":code}}))?.with_status(status))
}

fn parameter<'a>(ctx: &'a RouteContext<()>, name: &str) -> &'a str {
    ctx.param(name).map_or("", String::as_str)
}

fn package_parameters(ctx: &RouteContext<()>) -> Option<(&str, &str)> {
    let publisher = parameter(ctx, "publisher");
    let name = parameter(ctx, "name");
    (valid_handle(publisher) && valid_component(name)).then_some((publisher, name))
}

fn origin(env: &Env) -> Result<String> {
    Ok(env.var("REGISTRY_ORIGIN")?.to_string())
}

async fn read_body(req: &mut Request, maximum: usize) -> Result<Option<Vec<u8>>> {
    if req.headers().get("Content-Length")?.is_some_and(|length| {
        length
            .parse::<usize>()
            .map_or(true, |length| length > maximum)
    }) {
        return Ok(None);
    }
    let Ok(stream) = req.stream() else {
        return Ok(Some(Vec::new()));
    };
    read_stream(stream, maximum).await
}

async fn read_stream(mut stream: ByteStream, maximum: usize) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}

async fn identity(req: &Request, env: &Env) -> Result<Option<Identity>> {
    let headers = Headers::new();
    if let Some(value) = req.headers().get("Authorization")? {
        if !mutation_credentials_allowed(Some(&value), None, None, "") {
            return Ok(None);
        }
        headers.set("Authorization", &value)?;
    } else if let Some(value) = req.headers().get("Cookie")? {
        if value.len() > 8192 {
            return Ok(None);
        }
        headers.set("Cookie", &value)?;
    } else {
        return Ok(None);
    }
    let mut response = env
        .service("AUTH")?
        .fetch(
            "https://auth.internal/internal/session",
            Some(RequestInit {
                method: Method::Get,
                headers,
                ..RequestInit::default()
            }),
        )
        .await?;
    if response.status_code() == 401 {
        return Ok(None);
    }
    if response.status_code() != 200 {
        return Err(Error::RustError("identity service unavailable".into()));
    }
    let Some(bytes) = read_stream(response.stream()?, 4096).await? else {
        return Err(Error::RustError(
            "identity service response exceeds limit".into(),
        ));
    };
    let identity: Identity = serde_json::from_slice(&bytes)?;
    if identity.id.is_empty() || identity.id.len() > 128 || identity.display_name.len() > 512 {
        return Err(Error::RustError("invalid identity service response".into()));
    }
    Ok(Some(identity))
}

fn mutation_allowed(req: &Request, env: &Env) -> Result<bool> {
    Ok(mutation_credentials_allowed(
        req.headers().get("Authorization")?.as_deref(),
        req.headers().get("Cookie")?.as_deref(),
        req.headers().get("Origin")?.as_deref(),
        &origin(env)?,
    ))
}

async fn active_account(env: &Env, identity: &Identity) -> Result<bool> {
    let db = env.d1("DB")?;
    db.prepare("INSERT INTO registry_accounts(id) VALUES (?) ON CONFLICT DO NOTHING")
        .bind(&[identity.id.clone().into()])?
        .run()
        .await?;
    let account = db
        .prepare("SELECT suspended FROM registry_accounts WHERE id = ?")
        .bind(&[identity.id.clone().into()])?
        .first::<AccountRow>(None)
        .await?;
    Ok(account.is_some_and(|account| account.suspended == 0))
}

async fn owns(env: &Env, identity: &Identity, publisher: &str) -> Result<bool> {
    let row = env.d1("DB")?.prepare(
        "SELECT p.suspended FROM publishers p JOIN registry_accounts a ON a.id = p.user_id WHERE p.handle = ? AND p.user_id = ? AND p.suspended = 0 AND a.suspended = 0",
    ).bind(&[publisher.into(), identity.id.clone().into()])?.first::<AccountRow>(None).await?;
    Ok(row.is_some())
}

async fn rate(
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
        .run().await?;
    let result = db.prepare(
        "INSERT INTO rate_limits(key,count,bytes,expires_at) VALUES (?,1,?,unixepoch()+?) ON CONFLICT(key) DO UPDATE SET count = CASE WHEN expires_at <= unixepoch() THEN 1 ELSE count + 1 END, bytes = CASE WHEN expires_at <= unixepoch() THEN excluded.bytes ELSE bytes + excluded.bytes END, expires_at = CASE WHEN expires_at <= unixepoch() THEN excluded.expires_at ELSE expires_at END WHERE expires_at <= unixepoch() OR (count < ? AND bytes + excluded.bytes <= ?) RETURNING count",
    ).bind(&[key.into(), bytes.into(), seconds.into(), limit.into(), maximum_bytes.into()])?
        .first::<Counter>(None).await?;
    Ok(result.is_some_and(|counter| counter.count > 0))
}

async fn global_upload_allowed(env: &Env, bytes: u32) -> Result<bool> {
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

fn version_value(row: &VersionRow, registry_origin: &str) -> Result<Value> {
    let manifest: Manifest = serde_json::from_str(&row.manifest_json)?;
    Ok(json!({
        "manifest":manifest,"sha256":row.sha256,"size":row.size,
        "download_url":format!("{registry_origin}/v1/packages/{}/{}/versions/{}/download",row.publisher,row.name,row.version),
    }))
}

fn publication_value(row: &VersionRow, registry_origin: &str) -> Result<Value> {
    let mut value = version_value(row, registry_origin)?;
    value["status"] = json!(if row.ready == 1 { "ready" } else { "pending" });
    value["upload_url"] = json!(format!(
        "{registry_origin}/v1/packages/{}/{}/versions/{}/archive",
        row.publisher, row.name, row.version
    ));
    Ok(value)
}

async fn version_row(
    env: &Env,
    publisher: &str,
    name: &str,
    version: &str,
    public: bool,
) -> Result<Option<VersionRow>> {
    let condition = if public { ACTIVE } else { "1 = 1" };
    env.d1("DB")?.prepare(format!(
        "SELECT v.* FROM versions v JOIN publishers p ON p.handle = v.publisher JOIN registry_accounts a ON a.id = p.user_id WHERE v.publisher = ? AND v.name = ? AND v.version = ? AND {condition}",
    )).bind(&[publisher.into(), name.into(), version.into()])?.first::<VersionRow>(None).await
}

async fn catalog(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let search = query.get("q").map_or("", String::as_str);
    let cursor = query.get("cursor").map_or("", String::as_str);
    let limit = query
        .get("limit")
        .map_or(Some(20), |value| value.parse::<u32>().ok());
    let Some(limit) = limit.filter(|limit| (1..=50).contains(limit)) else {
        return problem(400, "invalid_limit");
    };
    if search.len() > 128 || cursor.len() > 100 || search.chars().any(char::is_control) {
        return problem(400, "invalid_query");
    }
    let mut rows = ctx.env.d1("DB")?.prepare(format!(
        "SELECT v.publisher,v.name,v.description,v.version AS latest_version FROM versions v JOIN publishers p ON p.handle=v.publisher JOIN registry_accounts a ON a.id=p.user_id WHERE {ACTIVE} AND v.publisher || '/' || v.name > ? AND instr(lower(v.publisher || '/' || v.name || ' ' || v.description),lower(?)) > 0 AND v.version = (SELECT x.version FROM versions x WHERE x.publisher=v.publisher AND x.name=v.name AND x.ready=1 AND x.quarantined=0 ORDER BY x.published_at DESC,x.version DESC LIMIT 1) ORDER BY v.publisher,v.name LIMIT ?",
    )).bind(&[cursor.into(),search.into(),(limit+1).into()])?.all().await?.results::<CatalogRow>()?;
    let next_cursor = if rows.len() > limit as usize {
        rows.pop();
        rows.last()
            .map(|row| format!("{}/{}", row.publisher, row.name))
    } else {
        None
    };
    let packages: Vec<_> = rows.into_iter().map(|row| json!({"name":format!("@{}/{}",row.publisher,row.name),"description":row.description,"latest_version":row.latest_version})).collect();
    Response::from_json(&json!({"packages":packages,"next_cursor":next_cursor}))
}

async fn package(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(404, "not_found");
    };
    let url = req.url()?;
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let cursor = query.get("cursor").map_or("", String::as_str);
    let limit = query
        .get("limit")
        .map_or(Some(3), |value| value.parse::<u32>().ok());
    let Some(limit) = limit.filter(|limit| (1..=3).contains(limit)) else {
        return problem(400, "invalid_limit");
    };
    if !cursor.is_empty() && !valid_version(cursor) {
        return problem(400, "invalid_cursor");
    }
    let mut rows = ctx.env.d1("DB")?.prepare(format!(
        "SELECT v.* FROM versions v JOIN publishers p ON p.handle=v.publisher JOIN registry_accounts a ON a.id=p.user_id WHERE {ACTIVE} AND v.publisher=? AND v.name=? AND v.version > ? ORDER BY v.version LIMIT ?",
    )).bind(&[publisher.into(),name.into(),cursor.into(),(limit+1).into()])?.all().await?.results::<VersionRow>()?;
    if rows.is_empty() && cursor.is_empty() {
        return problem(404, "not_found");
    }
    let next_cursor = if rows.len() > limit as usize {
        rows.pop();
        rows.last().map(|row| row.version.clone())
    } else {
        None
    };
    let registry_origin = origin(&ctx.env)?;
    let versions = rows
        .iter()
        .map(|row| version_value(row, &registry_origin))
        .collect::<Result<Vec<_>>>()?;
    Response::from_json(
        &json!({"name":format!("@{publisher}/{name}"),"versions":versions,"next_cursor":next_cursor}),
    )
}

async fn version(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(404, "not_found");
    };
    let version = parameter(&ctx, "version");
    if !valid_version(version) {
        return problem(404, "not_found");
    }
    let Some(row) = version_row(&ctx.env, publisher, name, version, true).await? else {
        return problem(404, "not_found");
    };
    Response::from_json(&version_value(&row, &origin(&ctx.env)?)?)
}

async fn download(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(404, "not_found");
    };
    let version = parameter(&ctx, "version");
    if !valid_version(version) {
        return problem(404, "not_found");
    }
    let Some(row) = version_row(&ctx.env, publisher, name, version, true).await? else {
        return problem(404, "not_found");
    };
    let bucket = ctx.env.bucket("PACKAGES")?;
    let Some(object) = bucket
        .get(format!("{publisher}/{name}/{version}.tar.gz"))
        .execute()
        .await?
    else {
        return problem(503, "archive_unavailable");
    };
    if object.size() != u64::from(row.size)
        || object.custom_metadata()?.get("sha256") != Some(&row.sha256)
    {
        return problem(503, "archive_unavailable");
    }
    let Some(body) = object.body() else {
        return problem(503, "archive_unavailable");
    };
    let mut response = Response::from_stream(body.stream()?)?;
    response
        .headers_mut()
        .set("Content-Type", "application/gzip")?;
    response
        .headers_mut()
        .set("Content-Length", &row.size.to_string())?;
    response.headers_mut().set(
        "Content-Disposition",
        &format!("attachment; filename=\"{publisher}-{name}-{version}.tar.gz\""),
    )?;
    Ok(response)
}

async fn me(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some(identity) = identity(&req, &ctx.env).await? else {
        return problem(401, "sign_in_required");
    };
    let active = active_account(&ctx.env, &identity).await?;
    let publisher = ctx
        .env
        .d1("DB")?
        .prepare("SELECT handle,display_name,suspended FROM publishers WHERE user_id=?")
        .bind(&[identity.id.clone().into()])?
        .first::<PublisherRow>(None)
        .await?;
    let publisher = publisher.map(|row| json!({"handle":row.handle,"display_name":row.display_name,"suspended":row.suspended!=0}));
    Response::from_json(
        &json!({"user":{"id":identity.id,"display_name":identity.display_name},"publisher":publisher,"suspended":!active}),
    )
}

async fn claim_publisher(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !mutation_allowed(&req, &ctx.env)? {
        return problem(403, "origin_or_credentials_required");
    }
    let Some(identity) = identity(&req, &ctx.env).await? else {
        return problem(401, "sign_in_required");
    };
    if !active_account(&ctx.env, &identity).await? {
        return problem(403, "account_unavailable");
    }
    if !rate(&ctx.env, &identity, "publisher", 3600, 5, 0, 0).await? {
        return problem(429, "rate_limited");
    }
    let Some(bytes) = read_body(&mut req, 1024).await? else {
        return problem(413, "body_too_large");
    };
    let Ok(input) = serde_json::from_slice::<PublisherInput>(&bytes) else {
        return problem(400, "invalid_publisher");
    };
    if !valid_handle(&input.handle)
        || input.display_name.trim().is_empty()
        || input.display_name.len() > 80
        || input.display_name.chars().any(char::is_control)
    {
        return problem(400, "invalid_publisher");
    }
    let db = ctx.env.d1("DB")?;
    db.prepare(
        "INSERT INTO publishers(handle,user_id,display_name) VALUES (?,?,?) ON CONFLICT DO NOTHING",
    )
    .bind(&[
        input.handle.clone().into(),
        identity.id.clone().into(),
        input.display_name.clone().into(),
    ])?
    .run()
    .await?;
    let Some(row) = db
        .prepare(
            "SELECT handle,display_name,suspended FROM publishers WHERE handle=? AND user_id=?",
        )
        .bind(&[input.handle.into(), identity.id.into()])?
        .first::<PublisherRow>(None)
        .await?
    else {
        return problem(409, "publisher_unavailable");
    };
    Response::from_json(
        &json!({"handle":row.handle,"display_name":row.display_name,"suspended":row.suspended!=0}),
    )
}

async fn reserve(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
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
    let Ok(input) = serde_json::from_slice::<Publication>(&bytes) else {
        return problem(400, "invalid_publication");
    };
    if input.manifest.validate().is_err()
        || !valid_version(&input.manifest.version)
        || input.manifest.name != format!("@{publisher}/{name}")
        || !valid_digest(&input.sha256)
        || input.size == 0
        || u64::from(input.size) > MAX_ARCHIVE_BYTES as u64
    {
        return problem(400, "invalid_publication");
    }
    let manifest_json = serde_json::to_string(&input.manifest)?;
    if let Some(row) =
        version_row(&ctx.env, publisher, name, &input.manifest.version, false).await?
    {
        if row.manifest_json != manifest_json
            || row.sha256 != input.sha256
            || row.size != input.size
            || row.quarantined != 0
        {
            return problem(409, "version_immutable");
        }
        return Response::from_json(&publication_value(&row, &origin(&ctx.env)?)?);
    }
    let db = ctx.env.d1("DB")?;
    let result=db.prepare("INSERT INTO versions(publisher,name,version,manifest_json,description,sha256,size) VALUES (?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(&[publisher.into(),name.into(),input.manifest.version.clone().into(),manifest_json.clone().into(),input.manifest.description.into(),input.sha256.clone().into(),input.size.into()])?.run().await;
    if result.is_err() {
        return problem(409, "publication_unavailable_or_quota_reached");
    }
    let Some(row) = version_row(&ctx.env, publisher, name, &input.manifest.version, false).await?
    else {
        return problem(409, "publication_unavailable");
    };
    if row.manifest_json != manifest_json
        || row.sha256 != input.sha256
        || row.size != input.size
        || row.quarantined != 0
    {
        return problem(409, "version_immutable");
    }
    Response::from_json(&publication_value(&row, &origin(&ctx.env)?)?)
}

async fn upload(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
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
    let Some(row) = version_row(&ctx.env, publisher, name, version, false).await? else {
        return problem(404, "reservation_required");
    };
    if row.quarantined != 0 {
        return problem(403, "version_unavailable");
    }
    if !rate(&ctx.env, &identity, "upload", 3600, 40, 0, 0).await?
        || !rate(
            &ctx.env,
            &identity,
            "upload-bytes",
            86400,
            40,
            row.size,
            64 * 1024 * 1024,
        )
        .await?
    {
        return problem(429, "rate_limited");
    }
    if !global_upload_allowed(&ctx.env, row.size).await? {
        return problem(429, "registry_upload_budget_reached");
    }
    let Some(bytes) = read_body(&mut req, row.size as usize).await? else {
        return problem(413, "body_too_large");
    };
    if bytes.len() != row.size as usize || archive_sha256(&bytes) != row.sha256 {
        return problem(400, "archive_integrity_mismatch");
    }
    let Ok(manifest) = validate_archive(&bytes) else {
        return problem(400, "invalid_archive");
    };
    if manifest != serde_json::from_str::<Manifest>(&row.manifest_json)? {
        return problem(400, "archive_manifest_mismatch");
    }
    let bucket = ctx.env.bucket("PACKAGES")?;
    let key = format!("{publisher}/{name}/{version}.tar.gz");
    let metadata = std::collections::HashMap::from([("sha256".to_owned(), row.sha256.clone())]);
    if bucket
        .put(key.clone(), bytes)
        .custom_metadata(metadata)
        .only_if(Conditional {
            etag_does_not_match: Some("*".into()),
            ..Conditional::default()
        })
        .execute()
        .await?
        .is_none()
    {
        let Some(object) = bucket.head(key).await? else {
            return problem(503, "archive_unavailable");
        };
        if object.size() != u64::from(row.size)
            || object.custom_metadata()?.get("sha256") != Some(&row.sha256)
        {
            return problem(409, "version_immutable");
        }
    }
    ctx.env.d1("DB")?.prepare("UPDATE versions SET ready=1,published_at=coalesce(published_at,unixepoch()) WHERE publisher=? AND name=? AND version=? AND quarantined=0 AND EXISTS (SELECT 1 FROM publishers p JOIN registry_accounts a ON a.id=p.user_id WHERE p.handle=versions.publisher AND p.user_id=? AND p.suspended=0 AND a.suspended=0)")
        .bind(&[publisher.into(),name.into(),version.into(),identity.id.into()])?.run().await?;
    let Some(row) = version_row(&ctx.env, publisher, name, version, true).await? else {
        return problem(403, "publication_unavailable");
    };
    Response::from_json(&publication_value(&row, &origin(&ctx.env)?)?)
}

async fn operator_input(
    req: &mut Request,
    env: &Env,
) -> Result<Option<(Identity, ModerationInput)>> {
    if !mutation_allowed(req, env)? {
        return Ok(None);
    }
    let Some(identity) = identity(req, env).await? else {
        return Ok(None);
    };
    if !identity.is_operator || !active_account(env, &identity).await? {
        return Ok(None);
    }
    if !rate(env, &identity, "moderate", 3600, 100, 0, 0).await? {
        return Ok(None);
    }
    let Some(bytes) = read_body(req, 1024).await? else {
        return Ok(None);
    };
    let Ok(input) = serde_json::from_slice::<ModerationInput>(&bytes) else {
        return Ok(None);
    };
    if input.reason.trim().is_empty()
        || input.reason.len() > 512
        || input.reason.chars().any(char::is_control)
    {
        return Ok(None);
    }
    Ok(Some((identity, input)))
}

async fn moderate(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((identity, input)) = operator_input(&mut req, &ctx.env).await? else {
        return problem(403, "operator_required");
    };
    let kind = parameter(&ctx, "kind");
    let target = parameter(&ctx, "target");
    let sql = match kind {
        "accounts" if !target.is_empty() && target.len() <= 128 => {
            "UPDATE registry_accounts SET suspended=?,suspension_reason=? WHERE id=?"
        }
        "publishers" if valid_handle(target) => {
            "UPDATE publishers SET suspended=?,suspension_reason=? WHERE handle=?"
        }
        _ => return problem(400, "invalid_target"),
    };
    let db = ctx.env.d1("DB")?;
    db.batch(vec![
        db.prepare(sql).bind(&[u32::from(input.disabled).into(),input.reason.clone().into(),target.into()])?,
        db.prepare("INSERT INTO moderation_log(operator_id,target_type,target,disabled,reason) VALUES (?,?,?,?,?)")
            .bind(&[identity.id.into(),kind.into(),target.into(),u32::from(input.disabled).into(),input.reason.into()])?,
    ]).await?;
    Response::from_json(&json!({"ok":true}))
}

async fn moderate_version(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((identity, input)) = operator_input(&mut req, &ctx.env).await? else {
        return problem(403, "operator_required");
    };
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(400, "invalid_target");
    };
    let version = parameter(&ctx, "version");
    if !valid_version(version) {
        return problem(400, "invalid_target");
    }
    let db = ctx.env.d1("DB")?;
    db.batch(vec![
        db.prepare("UPDATE versions SET quarantined=?,quarantine_reason=? WHERE publisher=? AND name=? AND version=?")
            .bind(&[u32::from(input.disabled).into(),input.reason.clone().into(),publisher.into(),name.into(),version.into()])?,
        db.prepare("INSERT INTO moderation_log(operator_id,target_type,target,disabled,reason) VALUES (?,'versions',?,?,?)")
            .bind(&[identity.id.into(),format!("{publisher}/{name}/{version}").into(),u32::from(input.disabled).into(),input.reason.into()])?,
    ]).await?;
    Response::from_json(&json!({"ok":true}))
}

async fn proxy_auth(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !matches!(req.method(), Method::Get | Method::Post) {
        return problem(405, "method_not_allowed");
    }
    let Some(body) = read_body(&mut req, MAX_AUTH_BYTES).await? else {
        return problem(413, "body_too_large");
    };
    let headers = Headers::new();
    for name in [
        "Content-Type",
        "Cookie",
        "Authorization",
        "Origin",
        "CF-Connecting-IP",
        "User-Agent",
    ] {
        if let Some(value) = req.headers().get(name)? {
            if value.len() > 8192 {
                return problem(431, "headers_too_large");
            }
            headers.set(name, &value)?;
        }
    }
    let url = req.url()?;
    let target = format!(
        "{}{}{}",
        origin(&ctx.env)?,
        url.path(),
        url.query()
            .map_or(String::new(), |query| format!("?{query}"))
    );
    ctx.env
        .service("AUTH")?
        .fetch(
            &target,
            Some(RequestInit {
                method: req.method(),
                headers,
                body: (!body.is_empty())
                    .then(|| JsValue::from(js_sys::Uint8Array::from(body.as_slice()))),
                ..RequestInit::default()
            }),
        )
        .await
}

async fn assets(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let path = req.path();
    if path.starts_with("/v1/")
        || path.starts_with("/api/")
        || !matches!(req.method(), Method::Get | Method::Head)
    {
        return problem(404, "not_found");
    }
    ctx.env.assets("ASSETS")?.fetch_request(req).await
}
