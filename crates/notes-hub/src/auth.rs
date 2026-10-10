//! Sign-in by login and password, and the session check. It does not know the
//! hub: `notes-hub` and `notes serve --auth` both mount it with
//! [`Auth::protect`] over their own routes.
//!
//! | Path                | What                                                   |
//! |---------------------|--------------------------------------------------------|
//! | `POST /api/login`   | `{ login, password }` -> `{ login, token }` and the cookie |
//! | `POST /api/logout`  | ends the session of the request, clears the cookie (204) |
//! | `GET /api/session`  | `{ login }` of the session, or 401                     |
//!
//! A session is a token the browser keeps in the cookie `notes_session`
//! (`HttpOnly`, `SameSite=Strict`, `Secure` when the proxy said `X-Forwarded-Proto:
//! https`) and a device sends as `Authorization: Bearer <token>`. A token is
//! never accepted in a URL. The login is answered with the same 401 for an
//! unknown login and a wrong password; [`Throttle`] makes the wait after
//! wrong passwords longer (429 with `Retry-After`), and the attempt is counted
//! before the password is checked, so parallel guesses do not skip the wait.
//!
//! [`Auth::protect`] adds these routes to the host's router and puts every
//! request through one middleware:
//!
//! 1. a state-changing request (not GET, HEAD, OPTIONS) with an `Origin` whose
//!    host differs from the `X-Forwarded-Host` / `Host` one is answered 403
//!    (the cookie is `SameSite=Strict` already; this is a second wall);
//! 2. the login and logout paths, and what the host's predicate calls public,
//!    pass;
//! 3. any other request needs a valid session, else 401 `{ error, errors }`;
//!    the account goes to the request extensions as [`Login`].
//!
//! Every check extends the session ([`Sessions::check`]); it writes the file at
//! most once an hour, so it is called in the middleware without a blocking
//! thread. The files of users and sessions are re-read when `notes users`
//! changes them, so a new account signs in and a changed password ends the
//! sessions without a restart.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Extension, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use notes_store::accounts::Accounts;
use notes_store::config::{sessions_file, users_file};
use notes_store::sessions::{self, Sessions, Throttle};

use crate::api::{COOKIE, LoginRequest, LoginResponse, SessionResponse};
use crate::error::{ApiError, ApiResult, Result, blocking};

/// The default lifetime of a session without requests.
pub const DEFAULT_LIFETIME: Duration = Duration::from_hours(24 * 30);

/// The largest login request in bytes.
const MAX_LOGIN_BODY: usize = 8 * 1024;

/// Which requests the host lets through without a session: the method and the
/// path. The login and logout paths are always public.
pub type Public = fn(&Method, &str) -> bool;

/// The account of the request's session; [`Auth::protect`] puts it into the
/// request extensions (`Extension<Login>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login(pub String);

/// Users, sessions and the throttle of wrong passwords.
#[derive(Debug)]
pub struct Auth {
    accounts: Accounts,
    sessions: Sessions,
    throttle: Throttle,
    lifetime: Duration,
}

impl Auth {
    /// Opens `users.json` and `sessions.json` of the data directory (they may
    /// not exist yet). A session lives `lifetime` after its last use.
    ///
    /// # Errors
    /// A file exists but is broken.
    pub fn open(data: &Path, lifetime: Duration) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            accounts: Accounts::open(users_file(data))?,
            sessions: Sessions::open(sessions_file(data), lifetime)?,
            throttle: Throttle::new(),
            lifetime,
        }))
    }

    #[must_use]
    pub fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// The account of a token, if the session is valid (and it counts as used).
    #[must_use]
    pub fn check(&self, token: &str) -> Option<Login> {
        self.sessions.check(token, sessions::now()).map(Login)
    }

    /// Adds the sign-in routes to `router` and puts all of it behind the
    /// session check (see the module docs). `public` is asked about every
    /// other request.
    pub fn protect<S: Clone + Send + Sync + 'static>(self: &Arc<Self>, router: Router<S>, public: Public) -> Router<S> {
        let guard = Guard { auth: self.clone(), public };
        router.merge(self.routes()).layer(middleware::from_fn_with_state(guard, guard_requests))
    }

    fn routes<S: Clone + Send + Sync + 'static>(self: &Arc<Self>) -> Router<S> {
        Router::new()
            .route("/api/login", post(login).layer(DefaultBodyLimit::max(MAX_LOGIN_BODY)))
            .route("/api/logout", post(logout))
            .route("/api/session", get(session))
            .with_state(self.clone())
    }
}

#[derive(Clone)]
struct Guard {
    auth: Arc<Auth>,
    public: Public,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard").finish_non_exhaustive()
    }
}

async fn guard_requests(State(guard): State<Guard>, mut req: Request, next: Next) -> Response {
    if !same_origin(&req) {
        return ApiError(StatusCode::FORBIDDEN, "cross-site request refused".into()).into_response();
    }
    let path = req.uri().path();
    if matches!(path, "/api/login" | "/api/logout") || (guard.public)(req.method(), path) {
        return next.run(req).await;
    }
    let login = presented(req.headers()).find_map(|token| guard.auth.check(token));
    match login {
        Some(login) => {
            req.extensions_mut().insert(login);
            next.run(req).await
        }
        None => sign_in_required(),
    }
}

fn sign_in_required() -> Response {
    ApiError(StatusCode::UNAUTHORIZED, "sign-in required".into()).into_response()
}

/// The tokens a request carries: the bearer one, then the cookie ones.
fn presented(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let cookies = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='));
    bearer.into_iter().chain(cookies).filter(|t| !t.is_empty())
}

/// A state-changing request from a page of another site is refused: its
/// `Origin` names another host than the one asked. Requests without `Origin`
/// (programs) pass.
fn same_origin(req: &Request) -> bool {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    let Some(origin) = req.headers().get(header::ORIGIN) else { return true };
    let origin_host =
        origin.to_str().ok().and_then(|o| o.split_once("://")).and_then(|(_, rest)| rest.split('/').next());
    let headers = req.headers();
    let asked = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .or_else(|| headers.get(header::HOST).and_then(|v| v.to_str().ok()))
        .or_else(|| req.uri().authority().map(axum::http::uri::Authority::as_str))
        .map(str::trim);
    matches!((origin_host, asked), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
}

/// Did the proxy get the request over HTTPS.
fn forwarded_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("https"))
}

/// The `Set-Cookie` header value; `token` is base64url or empty.
fn set_cookie(token: &str, max_age: u64, secure: bool) -> Option<HeaderValue> {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"))
        .ok()
}

fn with_cookie(mut response: Response, cookie: Option<HeaderValue>) -> Response {
    if let Some(cookie) = cookie {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    response
}

async fn login(
    State(auth): State<Arc<Auth>>,
    headers: HeaderMap,
    payload: std::result::Result<Json<LoginRequest>, JsonRejection>,
) -> ApiResult<Response> {
    let Json(LoginRequest { login, password }) = payload.map_err(|e| match e.status() {
        StatusCode::PAYLOAD_TOO_LARGE => ApiError(StatusCode::PAYLOAD_TOO_LARGE, "request too large".into()),
        _ => ApiError(StatusCode::BAD_REQUEST, "expected {\"login\": ..., \"password\": ...}".into()),
    })?;
    if let Err(wait) = auth.throttle.check(&login, Instant::now()) {
        let secs = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
        let mut response =
            ApiError(StatusCode::TOO_MANY_REQUESTS, format!("too many attempts: wait {secs} s")).into_response();
        response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(secs));
        return Ok(response);
    }
    // The attempt counts as wrong until the password is verified: a flood of
    // parallel requests would otherwise all pass the check above.
    auth.throttle.failed(&login, Instant::now());
    let checked = auth.clone();
    let signed_in = blocking(move || -> Result<Option<LoginResponse>> {
        if !checked.accounts.verify(&login, &password) {
            return Ok(None);
        }
        checked.throttle.succeeded(&login);
        // Verified: the login is valid, so lowercase is its stored form.
        let login = login.to_lowercase();
        let token = checked.sessions.create(&login, sessions::now())?;
        Ok(Some(LoginResponse { login, token }))
    })
    .await?;
    let Some(signed_in) = signed_in else {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "wrong login or password".into()));
    };
    tracing::info!(login = signed_in.login, "signed in");
    let cookie = set_cookie(&signed_in.token, auth.lifetime.as_secs(), forwarded_https(&headers));
    Ok(with_cookie(Json(signed_in).into_response(), cookie))
}

async fn logout(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> ApiResult<Response> {
    let tokens: Vec<String> = presented(&headers).map(str::to_owned).collect();
    blocking(move || -> Result<()> {
        for token in &tokens {
            auth.sessions.revoke(token)?;
        }
        Ok(())
    })
    .await?;
    Ok(with_cookie(StatusCode::NO_CONTENT.into_response(), set_cookie("", 0, forwarded_https(&headers))))
}

async fn session(Extension(Login(login)): Extension<Login>) -> Json<SessionResponse> {
    Json(SessionResponse { login })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    fn request(method: Method, headers: &[(&str, &str)]) -> Request {
        let mut builder = Request::builder().method(method).uri("/api/x");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn tokens_from_header_and_cookie() {
        let req = request(Method::GET, &[("authorization", "Bearer abc"), ("cookie", "a=1; notes_session=def; b=2")]);
        assert_eq!(presented(req.headers()).collect::<Vec<_>>(), ["abc", "def"]);
        let req = request(Method::GET, &[("cookie", "notes_session_x=1; xnotes_session=2; notes_session=")]);
        assert_eq!(presented(req.headers()).count(), 0);
        let req = request(Method::GET, &[("authorization", "Basic abc")]);
        assert_eq!(presented(req.headers()).count(), 0);
    }

    #[test]
    fn origin_rules() {
        let post = |headers: &[(&str, &str)]| same_origin(&request(Method::POST, headers));
        assert!(post(&[]), "no Origin: a program");
        assert!(post(&[("origin", "https://hub.example"), ("host", "hub.example")]));
        assert!(post(&[("origin", "https://HUB.example:8443"), ("host", "hub.example:8443")]));
        assert!(!post(&[("origin", "https://evil.example"), ("host", "hub.example")]));
        assert!(!post(&[("origin", "http://hub.example:81"), ("host", "hub.example")]));
        assert!(!post(&[("origin", "null"), ("host", "hub.example")]));
        assert!(!post(&[("origin", "https://hub.example")]), "no host to compare");
        // Behind a proxy the forwarded host is the one the browser used.
        let proxied =
            [("origin", "https://hub.example"), ("host", "127.0.0.1:8422"), ("x-forwarded-host", "hub.example")];
        assert!(post(&proxied));
        assert!(same_origin(&request(Method::GET, &[("origin", "https://evil.example"), ("host", "hub.example")])));
    }
}
