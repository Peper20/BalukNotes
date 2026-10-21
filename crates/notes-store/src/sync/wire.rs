//! The wire rules of the hub's sync API on the device side, without a
//! transport: the URLs, the `x-base` header, how an answer maps to a result.
//! [`WireRemote`] is a [`Remote`] over any [`Transport`]: the native client
//! (`notes_hub::client::HttpRemote`, `ureq`) and the browser one
//! (`notes-wasm`, synchronous `XMLHttpRequest`) differ only in how they send
//! a request.
//!
//! The mapping: 200 is the result, a 409 of a write is an [`Outcome`] with a
//! [`Conflict`], 413 is [`Error::TooLarge`], 404 is [`Error::NotFound`] (the
//! vault for a listing or a write, the file for a read), 401 is
//! [`Error::Unauthorized`], the rest is [`Error::Other`] with the hub's
//! message. A body that is not the expected JSON is [`Error::Network`].

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::engine::Remote;
use super::error::{Error, Result};
use super::hub::MAX_FILE_SIZE;
use super::path::check;
use super::protocol::{Base, Changes, Conflict, Entry, Outcome};

/// The header of a write: the version it replaces ([`Base`] in its text form).
pub const HEADER_BASE: &str = "x-base";

/// The largest answer that is not a file (a list of changes, an error).
pub const MAX_JSON: u64 = 64 * 1024 * 1024;

/// What stays unencoded in a URL path segment: letters, digits and `-._~`.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// A URL path segment, percent-encoded.
pub fn segment(text: &str) -> String {
    utf8_percent_encode(text, SEGMENT).to_string()
}

/// `/api/sync/vaults/{vault}`.
pub fn vault_url(vault: &str) -> String {
    format!("/api/sync/vaults/{}", segment(vault))
}

/// The changes after `after`, waiting up to `wait_secs` for one.
pub fn changes_url(vault: &str, after: u64, wait_secs: u64) -> String {
    format!("{}/changes?after={after}&wait={wait_secs}", vault_url(vault))
}

/// The file `path` of `vault`, each segment encoded.
///
/// # Errors
/// [`Error::InvalidPath`] for a path that is not synced.
pub fn file_url(vault: &str, path: &str) -> Result<String> {
    check(path)?;
    let encoded: Vec<String> = path.split('/').map(segment).collect();
    Ok(format!("{}/files/{}", vault_url(vault), encoded.join("/")))
}

/// One request of the sync API, as a [`Transport`] sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request<'a> {
    pub method: &'static str,
    /// The path and query, to be put after the hub's address.
    pub url: String,
    /// The [`HEADER_BASE`] value of a write.
    pub base: Option<String>,
    /// The `Content-Type` of the body.
    pub content_type: Option<&'static str>,
    /// The body; `None` for a request that sends none (GET).
    pub body: Option<&'a [u8]>,
    /// The largest body of the answer to accept.
    pub limit: u64,
}

/// The answer of any status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    /// The `Retry-After` header.
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

/// How a client sends a request to the hub.
pub trait Transport {
    /// Sends the request and returns the answer of any status.
    ///
    /// # Errors
    /// [`Error::Network`]: no answer (no connection, a timeout, an answer
    /// over [`Request::limit`]).
    fn send(&self, request: &Request<'_>) -> Result<Answer>;
}

/// The body of an error answer of the hub (`{ error, errors }`).
#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// The error for an answer that is not the expected one.
pub fn failure(answer: &Answer) -> Error {
    let status = answer.status;
    if status == 401 {
        return Error::Unauthorized;
    }
    let message = serde_json::from_slice::<ErrorBody>(&answer.body).map(|e| e.error).unwrap_or_default();
    let message = match (message.is_empty(), &answer.retry_after) {
        (false, _) => message,
        (true, Some(secs)) => format!("retry after {secs} s"),
        (true, None) => "no details".into(),
    };
    Error::Other(format!("the hub answered {status}: {message}"))
}

/// The JSON body of an answer.
///
/// # Errors
/// [`Error::Network`]: the body is not a `T`.
pub fn json<T: DeserializeOwned>(answer: &Answer) -> Result<T> {
    serde_json::from_slice(&answer.body).map_err(|e| Error::Network(format!("not an answer of the hub: {e}")))
}

/// One vault of the hub as a [`Remote`] over a [`Transport`].
#[derive(Debug, Clone)]
pub struct WireRemote<T> {
    transport: T,
    vault: String,
}

impl<T: Transport> WireRemote<T> {
    pub fn new(transport: T, vault: &str) -> Self {
        Self { transport, vault: vault.to_owned() }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn vault_missing(&self) -> Error {
        Error::NotFound(format!("vault \"{}\"", self.vault))
    }

    /// As [`Remote::changes`], but with no entries after `after` the hub
    /// waits up to `wait_secs` for a write.
    ///
    /// # Errors
    /// [`Error::Unauthorized`], [`Error::NotFound`] for an unknown vault, or
    /// the hub cannot be reached.
    pub fn wait_changes(&self, after: u64, wait_secs: u64) -> Result<Changes> {
        let url = changes_url(&self.vault, after, wait_secs);
        let answer = self.transport.send(&Request {
            method: "GET",
            url,
            base: None,
            content_type: None,
            body: None,
            limit: MAX_JSON,
        })?;
        match answer.status {
            200 => json(&answer),
            404 => Err(self.vault_missing()),
            _ => Err(failure(&answer)),
        }
    }

    fn write(&self, method: &'static str, path: &str, base: &Base, data: Option<&[u8]>) -> Result<Outcome> {
        let answer = self.transport.send(&Request {
            method,
            url: file_url(&self.vault, path)?,
            base: Some(base.to_string()),
            content_type: data.map(|_| "application/octet-stream"),
            body: Some(data.unwrap_or_default()),
            limit: MAX_JSON,
        })?;
        match answer.status {
            200 => Ok(Ok(json::<Entry>(&answer)?)),
            409 => Ok(Err(json::<Conflict>(&answer)?)),
            413 => Err(Error::TooLarge {
                path: path.to_owned(),
                size: data.map_or(0, |d| d.len() as u64),
                limit: MAX_FILE_SIZE,
            }),
            404 => Err(self.vault_missing()),
            _ => Err(failure(&answer)),
        }
    }
}

impl<T: Transport> Remote for WireRemote<T> {
    fn changes(&self, after: u64) -> Result<Changes> {
        self.wait_changes(after, 0)
    }

    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let answer = self.transport.send(&Request {
            method: "GET",
            url: file_url(&self.vault, path)?,
            base: None,
            content_type: None,
            body: None,
            limit: MAX_FILE_SIZE,
        })?;
        match answer.status {
            200 => Ok(answer.body),
            404 => Err(Error::NotFound(path.to_owned())),
            _ => Err(failure(&answer)),
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
    use std::cell::RefCell;

    use super::*;
    use crate::sync::hash_hex;

    #[test]
    fn urls() {
        assert_eq!(segment("Сеть"), "%D0%A1%D0%B5%D1%82%D1%8C");
        assert_eq!(segment("a b#?%/"), "a%20b%23%3F%25%2F");
        assert_eq!(segment("a-b_c.d~"), "a-b_c.d~");
        assert_eq!(vault_url("My vault"), "/api/sync/vaults/My%20vault");
        assert_eq!(
            changes_url("Учёба", 7, 30),
            "/api/sync/vaults/%D0%A3%D1%87%D1%91%D0%B1%D0%B0/changes?after=7&wait=30"
        );
        assert_eq!(
            file_url("My vault", "a b/Сеть.typ").unwrap(),
            "/api/sync/vaults/My%20vault/files/a%20b/%D0%A1%D0%B5%D1%82%D1%8C.typ"
        );
        assert_eq!(file_url("v", "x#1?.typ").unwrap(), "/api/sync/vaults/v/files/x%231%3F.typ");
        assert!(matches!(file_url("v", "a/../b"), Err(Error::InvalidPath { .. })));
        assert!(matches!(file_url("v", "/abs"), Err(Error::InvalidPath { .. })));
    }

    /// A transport that records the requests and gives one fixed answer.
    struct Fake {
        answer: Answer,
        sent: RefCell<Vec<Sent>>,
    }

    /// The method, the URL, the base, the body, the content type, the limit.
    type Sent = (String, String, Option<String>, Option<Vec<u8>>, Option<&'static str>, u64);

    impl Fake {
        fn new(status: u16, body: &str) -> WireRemote<Self> {
            let answer = Answer { status, retry_after: None, body: body.as_bytes().to_vec() };
            WireRemote::new(Self { answer, sent: RefCell::default() }, "v")
        }
    }

    impl Transport for Fake {
        fn send(&self, r: &Request<'_>) -> Result<Answer> {
            self.sent.borrow_mut().push((
                r.method.to_owned(),
                r.url.clone(),
                r.base.clone(),
                r.body.map(<[u8]>::to_vec),
                r.content_type,
                r.limit,
            ));
            Ok(self.answer.clone())
        }
    }

    const ENTRY: &str = r#"{"path":"a.typ","hash":null,"size":0,"seq":3}"#;

    /// What each call sends: the method, the URL, the base header, the body.
    #[test]
    fn requests() {
        let remote = Fake::new(200, r#"{"seq":0,"entries":[]}"#);
        remote.changes(5).unwrap();
        assert_eq!(
            remote.transport().sent.borrow()[0],
            ("GET".into(), "/api/sync/vaults/v/changes?after=5&wait=0".into(), None, None, None, MAX_JSON)
        );
        let remote = Fake::new(200, "bytes");
        assert_eq!(remote.get("a.typ").unwrap(), b"bytes");
        assert_eq!(
            remote.transport().sent.borrow()[0],
            ("GET".into(), "/api/sync/vaults/v/files/a.typ".into(), None, None, None, MAX_FILE_SIZE)
        );
        let remote = Fake::new(200, ENTRY);
        let hash = hash_hex(b"x");
        remote.put("a.typ", &Base::Absent, b"new").unwrap().unwrap();
        remote.delete("a.typ", &Base::Hash(hash.clone())).unwrap().unwrap();
        let sent = remote.transport().sent.borrow();
        assert_eq!(
            sent[0],
            (
                "PUT".into(),
                "/api/sync/vaults/v/files/a.typ".into(),
                Some("absent".into()),
                Some(b"new".to_vec()),
                Some("application/octet-stream"),
                MAX_JSON
            )
        );
        assert_eq!(
            sent[1],
            ("DELETE".into(), "/api/sync/vaults/v/files/a.typ".into(), Some(hash), Some(Vec::new()), None, MAX_JSON)
        );
        assert!(remote.put("../a", &Base::Any, b"").is_err(), "an invalid path is not sent");
        assert_eq!(remote.transport().sent.borrow().len(), 2);
    }

    /// The status of an answer and the result it makes.
    #[test]
    fn statuses() {
        let changes = Fake::new(200, r#"{"seq":4,"entries":[]}"#).changes(0).unwrap();
        assert_eq!(changes, Changes { seq: 4, entries: Vec::new() });
        assert!(matches!(Fake::new(404, "").changes(0), Err(Error::NotFound(v)) if v == "vault \"v\""));
        assert!(matches!(Fake::new(401, "").changes(0), Err(Error::Unauthorized)));
        assert!(matches!(Fake::new(200, "<html>").changes(0), Err(Error::Network(_))));

        assert!(matches!(Fake::new(404, "").get("a.typ"), Err(Error::NotFound(p)) if p == "a.typ"));
        let error = Fake::new(500, r#"{"error":"disk","errors":[]}"#).get("a.typ").unwrap_err();
        assert_eq!(error.to_string(), "the hub answered 500: disk");

        let entry: Entry = serde_json::from_str(ENTRY).unwrap();
        assert_eq!(Fake::new(200, ENTRY).put("a.typ", &Base::Any, b"x").unwrap(), Ok(entry.clone()));
        let conflict = format!(r#"{{"current":{ENTRY}}}"#);
        assert_eq!(
            Fake::new(409, &conflict).delete("a.typ", &Base::Any).unwrap(),
            Err(Conflict { current: Some(entry) })
        );
        assert!(matches!(
            Fake::new(413, "").put("a.typ", &Base::Any, b"xy"),
            Err(Error::TooLarge { size: 2, limit: MAX_FILE_SIZE, .. })
        ));
        assert!(
            matches!(Fake::new(404, "").delete("a.typ", &Base::Any), Err(Error::NotFound(v)) if v == "vault \"v\"")
        );

        let busy = Answer { status: 503, retry_after: Some("5".into()), body: Vec::new() };
        assert_eq!(failure(&busy).to_string(), "the hub answered 503: retry after 5 s");
        assert_eq!(
            failure(&Answer { status: 502, ..Answer::default() }).to_string(),
            "the hub answered 502: no details"
        );
    }
}
