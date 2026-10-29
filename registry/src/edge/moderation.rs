use serde_json::json;
use worker::{Env, Request, Response, Result, RouteContext};

use super::{
    account::{active_account, identity, mutation_allowed},
    body::read_body,
    limits::rate,
    parameter, problem, version_parameters,
};
use crate::policy::{Identity, ModerationInput, valid_handle};

/// Suspends or restores an account or publisher, and logs the decision in the same batch.
pub(super) async fn moderate(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
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
        db.prepare(sql).bind(&[
            u32::from(input.disabled).into(),
            input.reason.clone().into(),
            target.into(),
        ])?,
        db.prepare("INSERT INTO moderation_log(operator_id,target_type,target,disabled,reason) VALUES (?,?,?,?,?)")
            .bind(&[
                identity.id.into(),
                kind.into(),
                target.into(),
                u32::from(input.disabled).into(),
                input.reason.into(),
            ])?,
    ])
    .await?;
    Response::from_json(&json!({"ok":true}))
}

/// Quarantines or restores a version, and logs the decision in the same batch.
pub(super) async fn moderate_version(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Some((identity, input)) = operator_input(&mut req, &ctx.env).await? else {
        return problem(403, "operator_required");
    };
    let Some((publisher, name, version)) = version_parameters(&ctx) else {
        return problem(400, "invalid_target");
    };
    let db = ctx.env.d1("DB")?;
    db.batch(vec![
        db.prepare("UPDATE versions SET quarantined=?,quarantine_reason=? WHERE publisher=? AND name=? AND version=?")
            .bind(&[
                u32::from(input.disabled).into(),
                input.reason.clone().into(),
                publisher.into(),
                name.into(),
                version.into(),
            ])?,
        db.prepare("INSERT INTO moderation_log(operator_id,target_type,target,disabled,reason) VALUES (?,'versions',?,?,?)")
            .bind(&[
                identity.id.into(),
                format!("{publisher}/{name}/{version}").into(),
                u32::from(input.disabled).into(),
                input.reason.into(),
            ])?,
    ])
    .await?;
    Response::from_json(&json!({"ok":true}))
}

/// The operator and their decision, or None unless a live, active, rate-limited operator
/// sent a valid same-origin request.
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
    Ok(serde_json::from_slice::<ModerationInput>(&bytes)
        .ok()
        .filter(ModerationInput::valid)
        .map(|input| (identity, input)))
}
