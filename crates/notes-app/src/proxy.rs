//! A request of the window's page (`notes://` scheme) goes to the core over a
//! Unix socket; the response comes back whole (a Tauri scheme cannot stream,
//! docs/research/E7.md). If the core does not answer: 502 with an error, as
//! from the server, and the client shows "no connection".

use std::path::Path;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use tauri::http::{self, HeaderValue, Request, Response, StatusCode, header};

type Error = Box<dyn std::error::Error + Send + Sync>;

pub async fn forward(socket: &Path, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let started = std::time::Instant::now();
    let (method, uri) = (request.method().clone(), request.uri().clone());
    match send(socket, request).await {
        Ok(response) => {
            tracing::debug!(
                "{method} {uri} {} {} B {:?}",
                response.status().as_u16(),
                response.body().len(),
                started.elapsed()
            );
            response
        }
        Err(e) => {
            tracing::warn!("core did not answer: {e}");
            let body = serde_json::json!({ "error": format!("core is not answering: {e}"), "errors": [] });
            let mut response = Response::new(body.to_string().into_bytes());
            *response.status_mut() = StatusCode::BAD_GATEWAY;
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response
        }
    }
}

async fn send(socket: &Path, request: Request<Vec<u8>>) -> Result<Response<Vec<u8>>, Error> {
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    let (mut parts, body) = request.into_parts();
    // The core gets only the path: `notes://localhost/x?y` -> `/x?y`.
    parts.uri = parts.uri.path_and_query().map_or("/", http::uri::PathAndQuery::as_str).parse()?;
    parts.headers.insert(header::HOST, HeaderValue::from_static("localhost"));
    // The scheme would pass a compressed response through as is; no compression is needed on one machine.
    parts.headers.remove(header::ACCEPT_ENCODING);
    let response = sender.send_request(Request::from_parts(parts, Full::new(Bytes::from(body)))).await?;
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, body.collect().await?.to_bytes().to_vec()))
}
