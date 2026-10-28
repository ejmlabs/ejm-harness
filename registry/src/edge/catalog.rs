use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;
use worker::{Request, Response, Result, RouteContext};

use super::{
    origin, package_parameters, problem, version_parameters,
    versions::{ACTIVE, VersionRow, archive_key, archive_matches, public_version, version_value},
};
use crate::policy::valid_version;

#[derive(Deserialize)]
struct CatalogRow {
    publisher: String,
    name: String,
    description: String,
    latest_version: String,
}

/// Packages whose latest ready version is public, paged by `publisher/name`.
pub(super) async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let query = query(&req)?;
    let search = query.get("q").map_or("", String::as_str);
    let Some((cursor, limit)) = page(&query, 20, 50) else {
        return problem(400, "invalid_limit");
    };
    if search.len() > 128 || cursor.len() > 100 || search.chars().any(char::is_control) {
        return problem(400, "invalid_query");
    }
    let mut rows = ctx
        .env
        .d1("DB")?
        .prepare(format!("SELECT v.publisher,v.name,v.description,v.version AS latest_version FROM versions v JOIN publishers p ON p.handle=v.publisher JOIN registry_accounts a ON a.id=p.user_id WHERE {ACTIVE} AND v.publisher || '/' || v.name > ? AND instr(lower(v.publisher || '/' || v.name || ' ' || v.description),lower(?)) > 0 AND v.version = (SELECT x.version FROM versions x WHERE x.publisher=v.publisher AND x.name=v.name AND x.ready=1 AND x.quarantined=0 ORDER BY x.published_at DESC,x.version DESC LIMIT 1) ORDER BY v.publisher,v.name LIMIT ?"))
        .bind(&[cursor.into(), search.into(), (limit + 1).into()])?
        .all()
        .await?
        .results::<CatalogRow>()?;
    let next_cursor = if rows.len() > limit as usize {
        rows.pop();
        rows.last()
            .map(|row| format!("{}/{}", row.publisher, row.name))
    } else {
        None
    };
    let packages: Vec<_> = rows
        .into_iter()
        .map(|row| {
            json!({
                "name":format!("@{}/{}",row.publisher,row.name),
                "description":row.description,
                "latest_version":row.latest_version,
            })
        })
        .collect();
    Response::from_json(&json!({"packages":packages,"next_cursor":next_cursor}))
}

/// Public versions of one package, paged by exact version string.
pub(super) async fn package(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((publisher, name)) = package_parameters(&ctx) else {
        return problem(404, "not_found");
    };
    let query = query(&req)?;
    let Some((cursor, limit)) = page(&query, 3, 3) else {
        return problem(400, "invalid_limit");
    };
    if !cursor.is_empty() && !valid_version(cursor) {
        return problem(400, "invalid_cursor");
    }
    let mut rows = ctx
        .env
        .d1("DB")?
        .prepare(format!("SELECT v.* FROM versions v JOIN publishers p ON p.handle=v.publisher JOIN registry_accounts a ON a.id=p.user_id WHERE {ACTIVE} AND v.publisher=? AND v.name=? AND v.version > ? ORDER BY v.version LIMIT ?"))
        .bind(&[publisher.into(), name.into(), cursor.into(), (limit + 1).into()])?
        .all()
        .await?
        .results::<VersionRow>()?;
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

pub(super) async fn version(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((publisher, name, version)) = version_parameters(&ctx) else {
        return problem(404, "not_found");
    };
    let Some(row) = public_version(&ctx.env, publisher, name, version).await? else {
        return problem(404, "not_found");
    };
    Response::from_json(&version_value(&row, &origin(&ctx.env)?)?)
}

/// Streams the stored archive only if it still matches the version's size and SHA-256.
pub(super) async fn download(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((publisher, name, version)) = version_parameters(&ctx) else {
        return problem(404, "not_found");
    };
    let Some(row) = public_version(&ctx.env, publisher, name, version).await? else {
        return problem(404, "not_found");
    };
    let bucket = ctx.env.bucket("PACKAGES")?;
    let object = match bucket
        .get(archive_key(publisher, name, version))
        .execute()
        .await?
    {
        Some(object) if archive_matches(&object, &row)? => object,
        _ => return problem(503, "archive_unavailable"),
    };
    let Some(body) = object.body() else {
        return problem(503, "archive_unavailable");
    };
    let mut response = Response::from_stream(body.stream()?)?;
    let headers = response.headers_mut();
    headers.set("Content-Type", "application/gzip")?;
    headers.set("Content-Length", &row.size.to_string())?;
    headers.set(
        "Content-Disposition",
        &format!("attachment; filename=\"{publisher}-{name}-{version}.tar.gz\""),
    )?;
    Ok(response)
}

fn query(req: &Request) -> Result<HashMap<String, String>> {
    Ok(req.url()?.query_pairs().into_owned().collect())
}

/// The cursor and a limit within `1..=maximum`, or None when the limit is invalid.
fn page(
    query: &HashMap<String, String>,
    default_limit: u32,
    maximum_limit: u32,
) -> Option<(&str, u32)> {
    let limit = query
        .get("limit")
        .map_or(Some(default_limit), |value| value.parse::<u32>().ok())
        .filter(|limit| (1..=maximum_limit).contains(limit))?;
    Some((query.get("cursor").map_or("", String::as_str), limit))
}
