//! Необязательный токен доступа (`notes serve --token`, `NOTES_TOKEN`).
//!
//! Нужен встроенному серверу Tauri: он слушает `127.0.0.1` на случайном
//! порту, и без токена к API могла бы обратиться любая программа или
//! страница браузера на этой машине. Токен принимается:
//!
//! - заголовком `Authorization: Bearer <токен>` — для запросов API из кода;
//! - параметром адреса `?token=<токен>` — окно приложения открывает так
//!   первую страницу; в ответ сервер ставит cookie;
//! - cookie `notes_token` (`HttpOnly`, `SameSite=Strict`) — её браузер сам
//!   шлёт со всеми запросами страницы: API, файлы клиента, шрифты, PDF.
//!
//! Без токена (по умолчанию) проверки нет — как раньше.

use axum::extract::{Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;

/// Имя cookie с токеном.
pub(crate) const COOKIE: &str = "notes_token";

#[derive(Debug, serde::Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// Промежуточный слой: пропускает запрос с верным токеном, иначе — 401.
pub(crate) async fn require_token(State(token): State<std::sync::Arc<str>>, req: Request, next: Next) -> Response {
    let from_query = Query::<TokenQuery>::try_from_uri(req.uri()).ok().and_then(|q| q.0.token);
    let presented = from_query.as_deref().is_some_and(|t| same(t, &token));
    let valid = |t: Option<&str>| t.is_some_and(|t| same(t, &token));
    if !(presented || valid(bearer(&req)) || valid(cookie(&req))) {
        return ApiError(StatusCode::UNAUTHORIZED, "нужен токен доступа".into()).into_response();
    }
    let mut res = next.run(req).await;
    if presented {
        // Токен из адреса — запомнить в cookie: дальше браузер шлёт его сам.
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

/// Сравнение без раннего выхода: время не выдаёт, сколько знаков совпало.
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
