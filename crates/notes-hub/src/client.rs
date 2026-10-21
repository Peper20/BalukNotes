//! The device side of the hub API: [`HttpRemote`] is a
//! [`notes_store::sync::Remote`] over HTTP (blocking, `ureq`), so
//! [`notes_store::sync::sync`] works with a hub as it does with a local
//! [`HubVault`](notes_store::sync::HubVault). Call it from a blocking thread,
//! not from an async task.
//!
//! The functions [`login`], [`logout`], [`vaults`] and [`create_vault`] are the
//! account side. The token is the one `POST /api/login` gives; it is sent as
//! `Authorization: Bearer`, never in a URL.
//!
//! Errors are those of the sync module: a refused session (401) is
//! [`Error::Unauthorized`], a network failure (no connection, a timeout, a
//! broken answer) is [`Error::Network`], a version conflict (409) is an
//! [`Outcome`](notes_store::sync::Outcome) with a
//! [`Conflict`](notes_store::sync::Conflict), not an error. The URLs and the
//! status mapping of the sync calls are [`notes_store::sync::wire`], shared
//! with the browser's client; this module is the `ureq` transport. Timeouts:
//! 10 s to connect, 60 s to read, which is longer than the longest long-poll
//! wait.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use notes_store::sync::wire::{self, Answer, MAX_JSON, Transport, WireRemote};
use notes_store::sync::{Base, Changes, Error, Outcome, Remote, Result};
use serde::de::DeserializeOwned;
use ureq::{Agent, AgentBuilder, Response};

use crate::api::{LoginRequest, LoginResponse, MAX_WAIT_SECS, VaultInfo};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// The HTTP agent with TLS. `ureq` built with `native-tls` still has no TLS
/// until it is given a connector: without one every `https://` request fails.
fn agent() -> Agent {
    let builder = AgentBuilder::new().timeout_connect(CONNECT_TIMEOUT).timeout_read(READ_TIMEOUT);
    match native_tls::TlsConnector::new() {
        Ok(tls) => builder.tls_connector(Arc::new(tls)).build(),
        Err(e) => {
            tracing::warn!("no TLS for the hub client, https will not work: {e}");
            builder.build()
        }
    }
}

/// A hub address and, after sign-in, a token: the [`Transport`] of the
/// native client.
#[derive(Debug, Clone)]
struct Api {
    base: String,
    token: Option<String>,
    agent: Agent,
}

impl Api {
    fn new(base_url: &str, token: Option<&str>) -> Self {
        Self { base: base_url.trim_end_matches('/').to_owned(), token: token.map(str::to_owned), agent: agent() }
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        let request = self.agent.request(method, &format!("{}{path}", self.base));
        match &self.token {
            Some(token) => request.set("Authorization", &format!("Bearer {token}")),
            None => request,
        }
    }

    /// Sends the request; the answer of any status is `Ok`, its body read up
    /// to `limit` bytes (an error answer whose body cannot be read has none).
    fn send(request: ureq::Request, body: Option<&[u8]>, limit: u64) -> Result<Answer> {
        let result = match body {
            Some(body) => request.send_bytes(body),
            None => request.call(),
        };
        let response = match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(e)) => return Err(Error::Network(e.to_string())),
        };
        let status = response.status();
        let retry_after = response.header("Retry-After").map(str::to_owned);
        let body = match read_bytes(response, limit) {
            Ok(body) => body,
            Err(_) if status != 200 => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Answer { status, retry_after, body })
    }

    /// A call of the account side: the answer if its status is one of `ok`,
    /// otherwise the error.
    fn call(&self, method: &str, path: &str, json: Option<&[u8]>, ok: &[u16]) -> Result<Answer> {
        let request = self.request(method, path);
        let answer = match json {
            Some(body) => Self::send(request.set("Content-Type", "application/json"), Some(body), MAX_JSON)?,
            None if method == "GET" => Self::send(request, None, MAX_JSON)?,
            None => Self::send(request, Some(&[]), MAX_JSON)?,
        };
        if ok.contains(&answer.status) { Ok(answer) } else { Err(wire::failure(&answer)) }
    }

    fn call_json<T: DeserializeOwned>(&self, method: &str, path: &str, json: Option<&[u8]>, ok: &[u16]) -> Result<T> {
        wire::json(&self.call(method, path, json, ok)?)
    }
}

impl Transport for Api {
    fn send(&self, request: &wire::Request<'_>) -> Result<Answer> {
        let mut http = self.request(request.method, &request.url);
        if let Some(base) = &request.base {
            http = http.set(wire::HEADER_BASE, base);
        }
        if let Some(content_type) = request.content_type {
            http = http.set("Content-Type", content_type);
        }
        Self::send(http, request.body, request.limit)
    }
}

fn read_bytes(response: Response, limit: u64) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    response.into_reader().take(limit + 1).read_to_end(&mut data).map_err(|e| Error::Network(e.to_string()))?;
    if data.len() as u64 > limit {
        return Err(Error::Network(format!("the answer is over {limit} bytes")));
    }
    Ok(data)
}

/// Signs in; the token for [`HttpRemote::new`] and the other functions.
///
/// # Errors
/// [`Error::Unauthorized`] for a wrong login or password.
pub fn login(base_url: &str, login: &str, password: &str) -> Result<String> {
    let api = Api::new(base_url, None);
    let body = serde_json::to_vec(&LoginRequest { login: login.to_owned(), password: password.to_owned() })
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(api.call_json::<LoginResponse>("POST", "/api/login", Some(&body), &[200])?.token)
}

/// Ends the session of the token.
///
/// # Errors
/// The hub cannot be reached.
pub fn logout(base_url: &str, token: &str) -> Result<()> {
    Api::new(base_url, Some(token)).call("POST", "/api/logout", None, &[204]).map(drop)
}

/// The vaults of the account.
///
/// # Errors
/// [`Error::Unauthorized`], or the hub cannot be reached.
pub fn vaults(base_url: &str, token: &str) -> Result<Vec<VaultInfo>> {
    Api::new(base_url, Some(token)).call_json("GET", "/api/sync/vaults", None, &[200])
}

/// Creates an empty vault; one that exists is not an error.
///
/// # Errors
/// An invalid name, [`Error::Unauthorized`], or the hub cannot be reached.
pub fn create_vault(base_url: &str, token: &str, name: &str) -> Result<VaultInfo> {
    Api::new(base_url, Some(token)).call_json("PUT", &wire::vault_url(name), None, &[200, 201])
}

/// One vault of the hub as a [`Remote`].
#[derive(Debug, Clone)]
pub struct HttpRemote(WireRemote<Api>);

impl HttpRemote {
    /// `base_url` is the address of the hub (`https://hub.example`).
    #[must_use]
    pub fn new(base_url: &str, token: &str, vault: &str) -> Self {
        Self(WireRemote::new(Api::new(base_url, Some(token)), vault))
    }

    /// As [`Remote::changes`], but with no entries after `after` it waits up
    /// to `wait` (cut to the hub's 30 s) for a write.
    ///
    /// # Errors
    /// [`Error::Unauthorized`], [`Error::NotFound`] for an unknown vault, or
    /// the hub cannot be reached.
    pub fn wait_changes(&self, after: u64, wait: Duration) -> Result<Changes> {
        self.0.wait_changes(after, wait.as_secs().min(MAX_WAIT_SECS))
    }
}

impl Remote for HttpRemote {
    fn changes(&self, after: u64) -> Result<Changes> {
        self.0.changes(after)
    }

    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.0.get(path)
    }

    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        self.0.put(path, base, data)
    }

    fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        self.0.delete(path, base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client speaks TLS: an `https://` request reaches the handshake (which a
    /// plain TCP listener fails), it is not refused for a missing TLS backend.
    #[test]
    fn https_has_a_tls_backend() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || drop(listener.accept()));
        let error = login(&format!("https://127.0.0.1:{port}"), "a", "b").unwrap_err().to_string();
        server.join().unwrap();
        assert!(!error.contains("no TLS backend"), "{error}");
    }

    #[test]
    fn base_url() {
        let remote = HttpRemote::new("http://h:1/", "t", "My vault");
        assert_eq!(remote.0.transport().base, "http://h:1");
    }
}
