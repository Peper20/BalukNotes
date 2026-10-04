//! An optional access token (`notes serve --token`, `NOTES_TOKEN`).
//!
//! Closes the `notes serve` API to other programs and pages until there is a
//! login with sessions (architecture §9). The Tauri window does not need it:
//! it reaches the core through its own URL scheme, with no port
//! (docs/research/E7.md). The token is accepted as:
//!
//! - the `Authorization: Bearer <token>` header, for API requests from code;
//! - the `?token=<token>` URL parameter, which opens the first page; the
//!   server answers with a cookie;
//! - the `notes_token` cookie (`HttpOnly`, `SameSite=Strict`), which the
//!   browser sends with every request of the page: API, client files, fonts, PDF.
//!
//! Without a token (the default) nothing is checked.

use axum::extract::{Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;

/// The name of the token cookie.
pub(crate) const COOKIE: &str = "notes_token";

#[derive(Debug, serde::Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// Middleware: passes a request with the right token, otherwise answers 401.
pub(crate) async fn require_token(State(token): State<std::sync::Arc<str>>, req: Request, next: Next) -> Response {
    let from_query = Query::<TokenQuery>::try_from_uri(req.uri()).ok().and_then(|q| q.0.token);
    let presented = from_query.as_deref().is_some_and(|t| same(t, &token));
    let valid = |t: Option<&str>| t.is_some_and(|t| same(t, &token));
    if !(presented || valid(bearer(&req)) || valid(cookie(&req))) {
        return ApiError(StatusCode::UNAUTHORIZED, "access token required".into()).into_response();
    }
    let mut res = next.run(req).await;
    if presented {
        // A token from the URL goes to a cookie: from now on the browser sends it itself.
        let value = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict");
        if let Ok(value) = HeaderValue::from_str(&value) {
            res.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    res
}

fn bearer(req: &Request) -> Option<&str> {
    req.headers().get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")
}

fn cookie(req: &Request) -> Option<&str> {
    req.headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='))
}

/// Compares without an early exit: the time does not reveal how many characters matched.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_whole_tokens() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
        assert!(!same("", "a"));
    }

    #[test]
    fn finds_token_cookie_among_others() {
        let req = Request::builder()
            .header(header::COOKIE, "a=1; notes_token=s3cret; b=2")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(cookie(&req), Some("s3cret"));
        let other =
            Request::builder().header(header::COOKIE, "notes_token_x=1").body(axum::body::Body::empty()).unwrap();
        assert_eq!(cookie(&other), None);
    }
}
