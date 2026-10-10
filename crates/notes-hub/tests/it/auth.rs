//! Sign-in, sessions and the session check.

use axum::Router;
use axum::extract::Extension;
use axum::http::{Method, StatusCode};
use axum::routing::{get, post};
use notes_hub::api::{LoginResponse, SessionResponse};
use notes_hub::auth::{Auth, Login};
use notes_store::config::sessions_file;
use notes_store::sessions::Sessions;

use crate::common::{Harness, PASSWORD, send};

const JSON: (&str, &str) = ("content-type", "application/json");

fn credentials(login: &str, password: &str) -> Vec<u8> {
    format!("{{\"login\":\"{login}\",\"password\":\"{password}\"}}").into_bytes()
}

#[tokio::test]
async fn login_session_and_cookie() {
    let h = Harness::new(&["ivan"]);
    let reply = h.send("POST", "/api/login", &[JSON], credentials("Ivan", PASSWORD)).await;
    assert_eq!(reply.status, StatusCode::OK);
    let LoginResponse { login, token } = reply.json();
    assert_eq!(login, "ivan", "the login is lowercase, however it was typed");
    assert_eq!(token.len(), 43);
    let cookie = reply.header("set-cookie");
    assert_eq!(cookie, format!("notes_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000"));

    // Behind the proxy: Secure only for HTTPS.
    let body = credentials("ivan", PASSWORD);
    let https = h.send("POST", "/api/login", &[JSON, ("x-forwarded-proto", "https")], body.clone()).await;
    assert!(https.header("set-cookie").ends_with("; Secure"), "{}", https.header("set-cookie"));
    let plain = h.send("POST", "/api/login", &[JSON, ("x-forwarded-proto", "http")], body).await;
    assert!(!plain.header("set-cookie").contains("Secure"));

    // The cookie and the bearer token both open the session; nothing else does.
    let by_cookie = h.send("GET", "/api/session", &[("cookie", &format!("a=1; notes_session={token}"))], vec![]).await;
    assert_eq!(by_cookie.status, StatusCode::OK);
    assert_eq!(by_cookie.json::<SessionResponse>().login, "ivan");
    let by_bearer = h.send("GET", "/api/session", &[("authorization", &format!("Bearer {token}"))], vec![]).await;
    assert_eq!(by_bearer.json::<SessionResponse>().login, "ivan");
    let in_url = h.send("GET", &format!("/api/session?token={token}"), &[], vec![]).await;
    assert_eq!(in_url.status, StatusCode::UNAUTHORIZED, "a token in a URL is never accepted");
    for headers in [vec![], vec![("authorization", "Bearer nope")], vec![("cookie", "notes_session=nope")]] {
        let reply = h.send("GET", "/api/session", &headers, vec![]).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.error(), "sign-in required");
    }
}

#[tokio::test]
async fn wrong_login_and_password_look_alike() {
    let h = Harness::new(&["ivan"]);
    let unknown = h.send("POST", "/api/login", &[JSON], credentials("nobody", PASSWORD)).await;
    let wrong = h.send("POST", "/api/login", &[JSON], credentials("ivan", "wrong password")).await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.body, wrong.body);
    assert_eq!(wrong.error(), "wrong login or password");
    assert!(wrong.headers.get("set-cookie").is_none());
    let broken = h.send("POST", "/api/login", &[JSON], b"{\"login\":1}".to_vec()).await;
    assert_eq!(broken.status, StatusCode::BAD_REQUEST);
    broken.error();
    let big = h.send("POST", "/api/login", &[JSON], credentials("ivan", &"x".repeat(20_000))).await;
    assert_eq!(big.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn logout_ends_the_session() {
    let h = Harness::new(&["ivan"]);
    let token = h.login("ivan").await;
    let other = h.login("ivan").await;
    let cookie = format!("notes_session={token}");
    let reply = h.send("POST", "/api/logout", &[("cookie", &cookie)], vec![]).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(reply.header("set-cookie"), "notes_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    let after = h.send("GET", "/api/session", &[("cookie", &cookie)], vec![]).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    let kept = h.send("GET", "/api/session", &[("authorization", &format!("Bearer {other}"))], vec![]).await;
    assert_eq!(kept.status, StatusCode::OK, "only the presented session ends");
    // Without a session it is fine, too.
    let none = h.send("POST", "/api/logout", &[], vec![]).await;
    assert_eq!(none.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn wrong_passwords_are_throttled() {
    let h = Harness::new(&["ivan", "anna"]);
    let first = h.send("POST", "/api/login", &[JSON], credentials("ivan", "wrong password")).await;
    assert_eq!(first.status, StatusCode::UNAUTHORIZED);
    // The pause is on: even the right password waits, and is not verified.
    let right = h.send("POST", "/api/login", &[JSON], credentials("ivan", PASSWORD)).await;
    assert_eq!(right.status, StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = right.header("retry-after").parse().unwrap();
    assert!((1..=60).contains(&retry), "{retry}");
    assert!(right.error().contains("too many attempts"));
    // Another login is not slowed down.
    assert_eq!(h.send("POST", "/api/login", &[JSON], credentials("anna", PASSWORD)).await.status, StatusCode::OK);
}

#[tokio::test]
async fn cross_site_changes_are_refused() {
    let h = Harness::new(&["ivan"]);
    let token = h.login("ivan").await;
    let cookie = format!("notes_session={token}");
    let evil = [("origin", "https://evil.example"), ("host", "hub.example"), ("cookie", cookie.as_str())];
    assert_eq!(h.send("POST", "/api/logout", &evil, vec![]).await.status, StatusCode::FORBIDDEN);
    let login = h.send("POST", "/api/login", &evil[..2], credentials("ivan", PASSWORD)).await;
    assert_eq!(login.status, StatusCode::FORBIDDEN);
    login.error();
    let put = h.send("PUT", "/api/sync/vaults/notes", &evil, vec![]).await;
    assert_eq!(put.status, StatusCode::FORBIDDEN);
    assert_eq!(h.send("GET", "/api/session", &evil, vec![]).await.status, StatusCode::OK, "reading is not blocked");
    // The same site, also behind a proxy that rewrites Host.
    let own = [("origin", "https://hub.example"), ("host", "hub.example"), ("cookie", cookie.as_str())];
    assert_eq!(h.send("PUT", "/api/sync/vaults/notes", &own, vec![]).await.status, StatusCode::CREATED);
    let proxied = [
        ("origin", "https://hub.example"),
        ("host", "127.0.0.1:8422"),
        ("x-forwarded-host", "hub.example"),
        ("cookie", cookie.as_str()),
    ];
    assert_eq!(h.send("PUT", "/api/sync/vaults/notes", &proxied, vec![]).await.status, StatusCode::OK);
}

#[tokio::test]
async fn changes_of_other_processes_apply_without_restart() {
    let h = Harness::new(&["ivan"]);
    let token = h.login("ivan").await;
    let bearer = format!("Bearer {token}");
    // `notes users add anna` in another process: she can sign in.
    notes_store::accounts::Accounts::open(notes_store::config::users_file(h.dir.path()))
        .unwrap()
        .add("anna", PASSWORD)
        .unwrap();
    assert_eq!(h.send("POST", "/api/login", &[JSON], credentials("anna", PASSWORD)).await.status, StatusCode::OK);
    // `notes users passwd ivan`: the old sessions end.
    let ended = Sessions::open(sessions_file(h.dir.path()), std::time::Duration::from_secs(u64::MAX))
        .unwrap()
        .revoke_user("ivan")
        .unwrap();
    assert_eq!(ended, 1);
    let reply = h.send("GET", "/api/session", &[("authorization", &bearer)], vec![]).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_host_decides_which_paths_are_public() {
    let dir = tempfile::tempdir().unwrap();
    let auth = Auth::open(dir.path(), notes_hub::auth::DEFAULT_LIFETIME).unwrap();
    auth.accounts().add("ivan", PASSWORD).unwrap();
    let app: Router = Router::new()
        .route("/index.html", get(|| async { "client" }))
        .route("/api/notes", get(|| async { "notes" }))
        .route("/api/notes", post(|| async { "created" }))
        .route("/api/whoami", get(|Extension(Login(login)): Extension<Login>| async move { login }));
    let app = auth.protect(app, |method, path| *method == Method::GET && !path.starts_with("/api/"));
    let status = |method: &'static str, uri: &'static str, token: Option<&str>| {
        let app = app.clone();
        let auth_header = token.map(|t| format!("Bearer {t}"));
        async move {
            let headers: Vec<(&str, &str)> = auth_header.iter().map(|a| ("authorization", a.as_str())).collect();
            send(&app, method, uri, &headers, vec![]).await
        }
    };
    assert_eq!(status("GET", "/index.html", None).await.status, StatusCode::OK);
    assert_eq!(status("GET", "/api/notes", None).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(status("POST", "/api/notes", None).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(status("GET", "/nothing", None).await.status, StatusCode::NOT_FOUND, "a public unknown path");
    let token = auth.sessions().create("ivan", notes_store::sessions::now()).unwrap();
    assert_eq!(status("GET", "/api/notes", Some(&token)).await.text(), "notes");
    assert_eq!(status("GET", "/api/whoami", Some(&token)).await.text(), "ivan");
    let login = send(&app, "POST", "/api/login", &[JSON], credentials("ivan", PASSWORD)).await;
    assert_eq!(login.status, StatusCode::OK, "the sign-in routes are always public");
}
