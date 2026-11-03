//! Human-only HTTPS delivery. Trusted facts are minted by this TLS accept loop;
//! no router, forwarded-header adapter or verifier override is publicly exposed.
use super::*;
use axum::{
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use sha2::Digest;
use std::{collections::BTreeMap, sync::Mutex};
mod assets;
mod budget;
mod context;
mod events;
mod routes;
mod server;
#[cfg(test)]
mod tests;
pub use assets::BrowserAssets;
use context::*;
pub use server::HumanBrowserServer;

fn origin(value: &str) -> Result<url::Url, AuthError> {
    let u = url::Url::parse(value).map_err(|_| AuthError::InvalidInput)?;
    if u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.path() != "/"
        || u.query().is_some()
        || u.fragment().is_some()
        || u.origin().ascii_serialization() != value
    {
        return Err(AuthError::InvalidInput);
    }
    Ok(u)
}
pub(in crate::controller_keys::human_auth) fn validated_operational_origin(
    value: &str,
) -> Result<(), AuthError> {
    origin(value).map(|_| ())
}

fn check_headers(r: &Request<Body>, expected: &str) -> Result<(), AuthError> {
    let h = r.headers();
    let authority = expected.strip_prefix("https://").ok_or(AuthError::Denied)?;
    let single = |key: &str| -> Option<&str> {
        let mut all = h.get_all(key).iter();
        let v = all.next()?.to_str().ok()?;
        if all.next().is_some() { None } else { Some(v) }
    };
    if single("host") != Some(authority)
        || single("sec-fetch-site") != Some("same-origin")
        || h.keys()
            .any(|k| k.as_str() == "forwarded" || k.as_str().starts_with("x-forwarded-"))
        || h.contains_key(header::AUTHORIZATION)
        || h.contains_key("sec-purpose")
        || h.contains_key("purpose")
    {
        return Err(AuthError::Denied);
    }
    if r.method() != axum::http::Method::GET && r.method() != axum::http::Method::POST {
        return Err(AuthError::InvalidInput);
    }
    if r.method() == axum::http::Method::POST && single("origin") != Some(expected)
        || h.contains_key("origin") && single("origin") != Some(expected)
    {
        return Err(AuthError::Denied);
    }
    Ok(())
}

struct SecretBody(Zeroizing<Vec<u8>>);
impl AsRef<[u8]> for SecretBody {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
fn json(status: StatusCode, value: &serde_json::Value) -> Response<Body> {
    let bytes = serde_json::to_vec(value).expect("bounded JSON values serialize");
    let mut r = Response::new(Body::from(axum::body::Bytes::from_owner(SecretBody(
        Zeroizing::new(bytes),
    ))));
    *r.status_mut() = status;
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    r
}
fn failure(e: AuthError) -> Response<Body> {
    let (status, code) = match e {
        AuthError::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
        AuthError::Denied => (StatusCode::UNAUTHORIZED, "authentication_denied"),
        AuthError::Busy => (StatusCode::TOO_MANY_REQUESTS, "temporarily_limited"),
        AuthError::Conflict | AuthError::StaleVersion => (StatusCode::CONFLICT, "stale_state"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
    };
    json(status, &serde_json::json!({"schema":1,"error":code}))
}
