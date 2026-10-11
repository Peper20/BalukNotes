//! A hub on a temporary data directory and a way to call its router.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt as _;
use notes_hub::Hub;
use notes_hub::api::{LoginResponse, VaultInfo};
use serde::de::DeserializeOwned;
use tower::ServiceExt as _;

pub const PASSWORD: &str = "correct horse";

/// The hub with accounts, on its own data directory.
pub struct Harness {
    pub dir: tempfile::TempDir,
    pub hub: Hub,
    pub app: Router,
}

impl Harness {
    /// Accounts with the password [`PASSWORD`].
    pub fn new(logins: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::open(dir.path(), notes_hub::auth::DEFAULT_LIFETIME).unwrap();
        for login in logins {
            hub.auth().accounts().add(login, PASSWORD).unwrap();
        }
        let app = hub.router();
        Self { dir, hub, app }
    }

    pub async fn send(&self, method: &str, uri: &str, headers: &[(&str, &str)], body: Vec<u8>) -> Reply {
        send(&self.app, method, uri, headers, body).await
    }

    pub async fn login(&self, login: &str) -> String {
        let body = format!("{{\"login\":\"{login}\",\"password\":\"{PASSWORD}\"}}").into_bytes();
        let reply = self.send("POST", "/api/login", &[("content-type", "application/json")], body).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
        reply.json::<LoginResponse>().token
    }

    /// A signed-in account: calls with its bearer token.
    pub async fn account(&self, login: &str) -> Account<'_> {
        Account { hub: self, auth: format!("Bearer {}", self.login(login).await) }
    }
}

pub struct Account<'a> {
    hub: &'a Harness,
    auth: String,
}

impl Account<'_> {
    pub async fn call(&self, method: &str, uri: &str, extra: &[(&str, &str)], body: &[u8]) -> Reply {
        let mut headers = vec![("authorization", self.auth.as_str())];
        headers.extend_from_slice(extra);
        self.hub.send(method, uri, &headers, body.to_vec()).await
    }

    pub async fn get(&self, uri: &str) -> Reply {
        self.call("GET", uri, &[], b"").await
    }

    pub async fn create_vault(&self, name: &str) -> Reply {
        self.call("PUT", &format!("/api/sync/vaults/{name}"), &[], b"").await
    }

    pub async fn vaults(&self) -> Vec<VaultInfo> {
        self.get("/api/sync/vaults").await.json()
    }

    pub async fn put(&self, vault: &str, path: &str, base: &str, body: &[u8]) -> Reply {
        let uri = format!("/api/sync/vaults/{vault}/files/{path}");
        self.call("PUT", &uri, &[("x-base", base)], body).await
    }

    pub async fn delete(&self, vault: &str, path: &str, base: &str) -> Reply {
        let uri = format!("/api/sync/vaults/{vault}/files/{path}");
        self.call("DELETE", &uri, &[("x-base", base)], b"").await
    }
}

#[derive(Debug)]
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> &str {
        self.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or_default()
    }

    /// The `error` of an error body; its shape is `{ error, errors: [] }`.
    pub fn error(&self) -> String {
        let value: serde_json::Value = self.json();
        assert_eq!(value["errors"], serde_json::json!([]), "{value}");
        value["error"].as_str().unwrap().to_owned()
    }
}

pub async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)], body: Vec<u8>) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
    let (parts, body) = response.into_parts();
    Reply { status: parts.status, headers: parts.headers, body: body.collect().await.unwrap().to_bytes().to_vec() }
}
