use serde::Deserialize;
use serde_json::{Value, json};
use worker::{Env, Error, Headers, Method, Request, RequestInit, Response, Result, RouteContext};

use super::{
    body::{read_body, read_stream},
    limits::rate,
    origin, problem,
};
use crate::policy::{Identity, MAX_HEADER_BYTES, PublisherInput, mutation_credentials_allowed};

#[derive(Deserialize)]
struct PublisherRow {
    handle: String,
    display_name: String,
    suspended: u8,
}

#[derive(Deserialize)]
struct SuspendedRow {
    suspended: u8,
}

/// The caller's identity from the private auth service, or None when not signed in.
pub(super) async fn identity(req: &Request, env: &Env) -> Result<Option<Identity>> {
    let Some(headers) = credential_headers(req)? else {
        return Ok(None);
    };
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
    if !identity.valid() {
        return Err(Error::RustError("invalid identity service response".into()));
    }
    Ok(Some(identity))
}

/// A valid bearer, else a bounded cookie. Never both, and never a cookie after a bad bearer.
fn credential_headers(req: &Request) -> Result<Option<Headers>> {
    let headers = Headers::new();
    if let Some(value) = req.headers().get("Authorization")? {
        if !mutation_credentials_allowed(Some(&value), None, None, "") {
            return Ok(None);
        }
        headers.set("Authorization", &value)?;
    } else if let Some(value) = req.headers().get("Cookie")? {
        if value.len() > MAX_HEADER_BYTES {
            return Ok(None);
        }
        headers.set("Cookie", &value)?;
    } else {
        return Ok(None);
    }
    Ok(Some(headers))
}

pub(super) fn mutation_allowed(req: &Request, env: &Env) -> Result<bool> {
    Ok(mutation_credentials_allowed(
        req.headers().get("Authorization")?.as_deref(),
        req.headers().get("Cookie")?.as_deref(),
        req.headers().get("Origin")?.as_deref(),
        &origin(env)?,
    ))
}

/// Registers the account on first sight, then reports whether it is not suspended.
pub(super) async fn active_account(env: &Env, identity: &Identity) -> Result<bool> {
    let db = env.d1("DB")?;
    db.prepare("INSERT INTO registry_accounts(id) VALUES (?) ON CONFLICT DO NOTHING")
        .bind(&[identity.id.clone().into()])?
        .run()
        .await?;
    let account = db
        .prepare("SELECT suspended FROM registry_accounts WHERE id = ?")
        .bind(&[identity.id.clone().into()])?
        .first::<SuspendedRow>(None)
        .await?;
    Ok(account.is_some_and(|account| account.suspended == 0))
}

/// True when the account owns the publisher and neither is suspended.
pub(super) async fn owns(env: &Env, identity: &Identity, publisher: &str) -> Result<bool> {
    let row = env
        .d1("DB")?
        .prepare("SELECT p.suspended FROM publishers p JOIN registry_accounts a ON a.id = p.user_id WHERE p.handle = ? AND p.user_id = ? AND p.suspended = 0 AND a.suspended = 0")
        .bind(&[publisher.into(), identity.id.clone().into()])?
        .first::<SuspendedRow>(None)
        .await?;
    Ok(row.is_some())
}

pub(super) async fn me(req: Request, ctx: RouteContext<()>) -> Result<Response> {
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
    Response::from_json(&json!({
        "user":{"id":identity.id,"display_name":identity.display_name},
        "publisher":publisher.as_ref().map(publisher_value),
        "suspended":!active,
    }))
}

pub(super) async fn claim_publisher(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
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
    let Some(input) = serde_json::from_slice::<PublisherInput>(&bytes)
        .ok()
        .filter(PublisherInput::valid)
    else {
        return problem(400, "invalid_publisher");
    };
    let db = ctx.env.d1("DB")?;
    db.prepare(
        "INSERT INTO publishers(handle,user_id,display_name) VALUES (?,?,?) ON CONFLICT DO NOTHING",
    )
    .bind(&[
        input.handle.clone().into(),
        identity.id.clone().into(),
        input.display_name.into(),
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
    Response::from_json(&publisher_value(&row))
}

fn publisher_value(row: &PublisherRow) -> Value {
    json!({"handle":row.handle,"display_name":row.display_name,"suspended":row.suspended!=0})
}
