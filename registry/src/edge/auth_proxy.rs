use worker::{
    Headers, Method, Request, RequestInit, Response, Result, RouteContext, wasm_bindgen::JsValue,
};

use super::{body::read_body, origin, problem};
use crate::policy::{MAX_AUTH_BYTES, MAX_HEADER_BYTES};

const FORWARDED_HEADERS: [&str; 6] = [
    "Content-Type",
    "Cookie",
    "Authorization",
    "Origin",
    "CF-Connecting-IP",
    "User-Agent",
];

/// Forwards a bounded Better Auth request to the private auth service.
pub(super) async fn forward(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !matches!(req.method(), Method::Get | Method::Post) {
        return problem(405, "method_not_allowed");
    }
    let Some(body) = read_body(&mut req, MAX_AUTH_BYTES).await? else {
        return problem(413, "body_too_large");
    };
    let Some(headers) = forwarded_headers(&req)? else {
        return problem(431, "headers_too_large");
    };
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

/// The allowlisted request headers, or None when one is oversized.
fn forwarded_headers(req: &Request) -> Result<Option<Headers>> {
    let headers = Headers::new();
    for name in FORWARDED_HEADERS {
        if let Some(value) = req.headers().get(name)? {
            if value.len() > MAX_HEADER_BYTES {
                return Ok(None);
            }
            headers.set(name, &value)?;
        }
    }
    Ok(Some(headers))
}
