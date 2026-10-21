//! The hub's sync API from the worker: a synchronous `XMLHttpRequest` (allowed
//! in workers) as the [`Transport`] of [`WireRemote`], so a round runs as one
//! blocking call like natively. The wire rules are those of every device
//! ([`notes_store::sync::wire`]). The session is the page's cookie, sent by the
//! browser itself: no token here.

use js_sys::Uint8Array;
use notes_store::sync::wire::{Answer, HEADER_BASE, Request, Transport, WireRemote};
use notes_store::sync::{Error, Result};
use wasm_bindgen::JsValue;
use web_sys::{XmlHttpRequest, XmlHttpRequestResponseType};

/// The address of the hub: `""` is the page's server.
#[derive(Debug, Clone)]
pub struct Xhr {
    base: String,
}

/// One vault of the hub over [`Xhr`].
pub type XhrRemote = WireRemote<Xhr>;

impl Xhr {
    pub fn remote(base: &str, vault: &str) -> XhrRemote {
        WireRemote::new(Self { base: base.trim_end_matches('/').to_owned() }, vault)
    }
}

fn network(e: &JsValue) -> Error {
    Error::Network(e.as_string().or_else(|| js_sys::JSON::stringify(e).ok().map(String::from)).unwrap_or_default())
}

impl Transport for Xhr {
    fn send(&self, request: &Request<'_>) -> Result<Answer> {
        let xhr = XmlHttpRequest::new().map_err(|e| network(&e))?;
        xhr.open_with_async(request.method, &format!("{}{}", self.base, request.url), false)
            .map_err(|e| network(&e))?;
        xhr.set_response_type(XmlHttpRequestResponseType::Arraybuffer);
        if let Some(base) = &request.base {
            xhr.set_request_header(HEADER_BASE, base).map_err(|e| network(&e))?;
        }
        if let Some(content_type) = request.content_type {
            xhr.set_request_header("Content-Type", content_type).map_err(|e| network(&e))?;
        }
        xhr.send_with_opt_u8_array(request.body).map_err(|e| network(&e))?;
        let status = xhr.status().map_err(|e| network(&e))?;
        if status == 0 {
            return Err(Error::Network("no answer".into()));
        }
        let body = match xhr.response() {
            Ok(value) if value.is_object() => Uint8Array::new(&value).to_vec(),
            _ => Vec::new(),
        };
        if body.len() as u64 > request.limit {
            return Err(Error::Network(format!("the answer is over {} bytes", request.limit)));
        }
        let retry_after = xhr.get_response_header("Retry-After").ok().flatten();
        Ok(Answer { status, retry_after, body })
    }
}
