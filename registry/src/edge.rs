mod account;
mod auth_proxy;
mod body;
mod catalog;
mod limits;
mod moderation;
mod publish;
mod versions;

use serde_json::json;
use worker::{Context, Env, Method, Request, Response, Result, RouteContext, Router, event};

use crate::policy::{valid_component, valid_handle, valid_version};

const SECURITY_HEADERS: [(&str, &str); 7] = [
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
];

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let mut response = serve(req, env)
        .await
        .or_else(|_| problem(503, "service_unavailable"))?;
    let headers = response.headers().clone();
    response = response.with_headers(headers);
    for (name, value) in SECURITY_HEADERS {
        response.headers_mut().set(name, value)?;
    }
    Ok(response)
}

async fn serve(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    if url.as_str().len() > 2048 {
        return problem(414, "url_too_long");
    }
    let api = url.path().starts_with("/v1/") || url.path().starts_with("/api/auth/");
    if api && !within_edge_limit(&req, &env).await? {
        return problem(429, "rate_limited");
    }
    Router::new()
        .get_async("/", assets)
        .get_async("/v1/packages", catalog::list)
        .get_async("/v1/packages/:publisher/:name", catalog::package)
        .get_async(
            "/v1/packages/:publisher/:name/versions/:version",
            catalog::version,
        )
        .get_async(
            "/v1/packages/:publisher/:name/versions/:version/download",
            catalog::download,
        )
        .get_async("/v1/me", account::me)
        .post_async("/v1/publishers", account::claim_publisher)
        .post_async("/v1/packages/:publisher/:name/versions", publish::reserve)
        .put_async(
            "/v1/packages/:publisher/:name/versions/:version/archive",
            publish::upload,
        )
        .post_async("/v1/operator/:kind/:target", moderation::moderate)
        .post_async(
            "/v1/operator/versions/:publisher/:name/:version",
            moderation::moderate_version,
        )
        .on_async("/api/auth/*path", auth_proxy::forward)
        .or_else_any_method_async("/*path", assets)
        .run(req, env)
        .await
}

/// Anonymous traffic is limited per client IP at the edge.
async fn within_edge_limit(req: &Request, env: &Env) -> Result<bool> {
    let ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .unwrap_or_else(|| "unknown".into());
    Ok(env
        .rate_limiter("EDGE_LIMIT")?
        .limit(format!("api:{ip}"))
        .await?
        .success)
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

fn version_parameters(ctx: &RouteContext<()>) -> Option<(&str, &str, &str)> {
    let (publisher, name) = package_parameters(ctx)?;
    let version = parameter(ctx, "version");
    valid_version(version).then_some((publisher, name, version))
}

fn origin(env: &Env) -> Result<String> {
    Ok(env.var("REGISTRY_ORIGIN")?.to_string())
}
