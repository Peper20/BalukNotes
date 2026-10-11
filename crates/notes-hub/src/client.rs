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
//! [`Outcome`] with a [`Conflict`], not an error. Timeouts: 10 s to connect,
//! 60 s to read, which is longer than the longest long-poll wait.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use notes_store::sync::path::check;
use notes_store::sync::{Base, Changes, Conflict, Entry, Error, MAX_FILE_SIZE, Outcome, Remote, Result};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use ureq::{Agent, AgentBuilder, Request, Response};

use crate::api::{ErrorResponse, HEADER_BASE, LoginRequest, LoginResponse, MAX_WAIT_SECS, VaultInfo};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// The largest answer that is not a file (a list of changes).
const MAX_JSON: u64 = 64 * 1024 * 1024;

/// What stays unencoded in a URL path segment: letters, digits and `-._~`.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn segment(text: &str) -> String {
    utf8_percent_encode(text, SEGMENT).to_string()
}

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

/// A hub address and, after sign-in, a token.
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

    fn request(&self, method: &str, path: &str) -> Request {
        let request = self.agent.request(method, &format!("{}{path}", self.base));
        match &self.token {
            Some(token) => request.set("Authorization", &format!("Bearer {token}")),
            None => request,
        }
    }

    /// Sends the request; the answer of any status is `Ok`.
    fn send(request: Request, body: Option<&[u8]>) -> Result<Response> {
        let result = match body {
            Some(body) => request.send_bytes(body),
            None => request.call(),
        };
        match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
            Err(ureq::Error::Transport(e)) => Err(Error::Network(e.to_string())),
        }
    }

    fn vault_path(vault: &str) -> String {
        format!("/api/sync/vaults/{}", segment(vault))
    }
}

/// The error for an answer that is not the expected one.
fn failure(response: Response) -> Error {
    let status = response.status();
    if status == 401 {
        return Error::Unauthorized;
    }
    let retry = response.header("Retry-After").map(str::to_owned);
    let message = read_json::<ErrorResponse>(response).map_or_else(|_| String::new(), |e| e.error);
    let message = match (message.is_empty(), retry) {
        (false, _) => message,
        (true, Some(secs)) => format!("retry after {secs} s"),
        (true, None) => "no details".into(),
    };
    Error::Other(format!("the hub answered {status}: {message}"))
}

fn read_bytes(response: Response, limit: u64) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    response.into_reader().take(limit + 1).read_to_end(&mut data).map_err(|e| Error::Network(e.to_string()))?;
    if data.len() as u64 > limit {
        return Err(Error::Network(format!("the answer is over {limit} bytes")));
    }
    Ok(data)
}

fn read_json<T: DeserializeOwned>(response: Response) -> Result<T> {
    let data = read_bytes(response, MAX_JSON)?;
    serde_json::from_slice(&data).map_err(|e| Error::Network(format!("not an answer of the hub: {e}")))
}

/// Signs in; the token for [`HttpRemote::new`] and the other functions.
///
/// # Errors
/// [`Error::Unauthorized`] for a wrong login or password.
pub fn login(base_url: &str, login: &str, password: &str) -> Result<String> {
    let api = Api::new(base_url, None);
    let body = serde_json::to_vec(&LoginRequest { login: login.to_owned(), password: password.to_owned() })
        .map_err(|e| Error::Other(e.to_string()))?;
    let request = api.request("POST", "/api/login").set("Content-Type", "application/json");
    let response = Api::send(request, Some(&body))?;
    if response.status() != 200 {
        return Err(failure(response));
    }
    Ok(read_json::<LoginResponse>(response)?.token)
}

/// Ends the session of the token.
///
/// # Errors
/// The hub cannot be reached.
pub fn logout(base_url: &str, token: &str) -> Result<()> {
    let api = Api::new(base_url, Some(token));
    let response = Api::send(api.request("POST", "/api/logout"), Some(&[]))?;
    if response.status() == 204 { Ok(()) } else { Err(failure(response)) }
}

/// The vaults of the account.
///
/// # Errors
/// [`Error::Unauthorized`], or the hub cannot be reached.
pub fn vaults(base_url: &str, token: &str) -> Result<Vec<VaultInfo>> {
    let api = Api::new(base_url, Some(token));
    let response = Api::send(api.request("GET", "/api/sync/vaults"), None)?;
    if response.status() != 200 {
        return Err(failure(response));
    }
    read_json(response)
}

/// Creates an empty vault; one that exists is not an error.
///
/// # Errors
/// An invalid name, [`Error::Unauthorized`], or the hub cannot be reached.
pub fn create_vault(base_url: &str, token: &str, name: &str) -> Result<VaultInfo> {
    let api = Api::new(base_url, Some(token));
    let response = Api::send(api.request("PUT", &Api::vault_path(name)), Some(&[]))?;
    if !matches!(response.status(), 200 | 201) {
        return Err(failure(response));
    }
    read_json(response)
}

/// One vault of the hub as a [`Remote`].
#[derive(Debug, Clone)]
pub struct HttpRemote {
    api: Api,
    vault: String,
}

impl HttpRemote {
    /// `base_url` is the address of the hub (`https://hub.example`).
    #[must_use]
    pub fn new(base_url: &str, token: &str, vault: &str) -> Self {
        Self { api: Api::new(base_url, Some(token)), vault: vault.to_owned() }
    }

    fn files_path(&self, path: &str) -> Result<String> {
        check(path)?;
        let encoded: Vec<String> = path.split('/').map(segment).collect();
        Ok(format!("{}/files/{}", Api::vault_path(&self.vault), encoded.join("/")))
    }

    /// As [`Remote::changes`], but with no entries after `after` it waits up
    /// to `wait` (cut to the hub's 30 s) for a write.
    ///
    /// # Errors
    /// [`Error::Unauthorized`], [`Error::NotFound`] for an unknown vault, or
    /// the hub cannot be reached.
    pub fn wait_changes(&self, after: u64, wait: Duration) -> Result<Changes> {
        let secs = wait.as_secs().min(MAX_WAIT_SECS);
        let path = format!("{}/changes?after={after}&wait={secs}", Api::vault_path(&self.vault));
        let response = Api::send(self.api.request("GET", &path), None)?;
        match response.status() {
            200 => read_json(response),
            404 => Err(Error::NotFound(format!("vault \"{}\"", self.vault))),
            _ => Err(failure(response)),
        }
    }

    fn write(&self, method: &str, path: &str, base: &Base, data: Option<&[u8]>) -> Result<Outcome> {
        let url = self.files_path(path)?;
        let mut request = self.api.request(method, &url).set(HEADER_BASE, &base.to_string());
        if data.is_some() {
            request = request.set("Content-Type", "application/octet-stream");
        }
        let response = Api::send(request, Some(data.unwrap_or_default()))?;
        match response.status() {
            200 => Ok(Ok(read_json::<Entry>(response)?)),
            409 => Ok(Err(read_json::<Conflict>(response)?)),
            413 => Err(Error::TooLarge {
                path: path.to_owned(),
                size: data.map_or(0, |d| d.len() as u64),
                limit: MAX_FILE_SIZE,
            }),
            404 => Err(Error::NotFound(format!("vault \"{}\"", self.vault))),
            _ => Err(failure(response)),
        }
    }
}

impl Remote for HttpRemote {
    fn changes(&self, after: u64) -> Result<Changes> {
        self.wait_changes(after, Duration::ZERO)
    }

    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let response = Api::send(self.api.request("GET", &self.files_path(path)?), None)?;
        match response.status() {
            200 => read_bytes(response, MAX_FILE_SIZE),
            404 => Err(Error::NotFound(path.to_owned())),
            _ => Err(failure(response)),
        }
    }

    fn put(&self, path: &str, base: &Base, data: &[u8]) -> Result<Outcome> {
        self.write("PUT", path, base, Some(data))
    }

    fn delete(&self, path: &str, base: &Base) -> Result<Outcome> {
        self.write("DELETE", path, base, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_are_encoded() {
        assert_eq!(segment("Сеть"), "%D0%A1%D0%B5%D1%82%D1%8C");
        assert_eq!(segment("a b#?%/"), "a%20b%23%3F%25%2F");
        assert_eq!(segment("a-b_c.d~"), "a-b_c.d~");
    }

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
    fn file_urls() {
        let remote = HttpRemote::new("http://h:1/", "t", "My vault");
        assert_eq!(remote.api.base, "http://h:1");
        assert_eq!(
            remote.files_path("a b/Сеть.typ").unwrap(),
            "/api/sync/vaults/My%20vault/files/a%20b/%D0%A1%D0%B5%D1%82%D1%8C.typ"
        );
        assert!(matches!(remote.files_path("a/../b"), Err(Error::InvalidPath { .. })));
        assert!(matches!(remote.files_path("/abs"), Err(Error::InvalidPath { .. })));
    }
}
