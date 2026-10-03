//! Запрос страницы окна (схема `notes://`) - ядру через сокет Unix, ответ -
//! целиком (схема Tauri потоков не умеет, docs/research/E7.md). Ядро не
//! отвечает - 502 с ошибкой, как у сервера: интерфейс покажет "нет связи".

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
                "{method} {uri} {} {} Б {:?}",
                response.status().as_u16(),
                response.body().len(),
                started.elapsed()
            );
            response
        }
        Err(e) => {
            tracing::warn!("ядро не ответило: {e}");
            let body = serde_json::json!({ "error": format!("ядро не отвечает: {e}"), "errors": [] });
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.to_string().into_bytes())
                .expect("ответ")
        }
    }
}

async fn send(socket: &Path, request: Request<Vec<u8>>) -> Result<Response<Vec<u8>>, Error> {
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    let (mut parts, body) = request.into_parts();
    // Ядру - только путь: `notes://localhost/x?y` -> `/x?y`.
    parts.uri = parts.uri.path_and_query().map_or("/", http::uri::PathAndQuery::as_str).parse()?;
    parts.headers.insert(header::HOST, HeaderValue::from_static("localhost"));
    // Сжатый ответ схема отдала бы как есть; внутри машины сжатие не нужно.
    parts.headers.remove(header::ACCEPT_ENCODING);
    let response = sender.send_request(Request::from_parts(parts, Full::new(Bytes::from(body)))).await?;
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, body.collect().await?.to_bytes().to_vec()))
}
