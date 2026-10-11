//! Sign-in for the TCP address (`notes serve --auth`, architecture §9).
//!
//! The sign-in itself is `notes_hub::auth` (login and password, the cookie
//! session, `POST /api/login`, `POST /api/logout`, `GET /api/session`);
//! [`AppState::auth`](crate::AppState::auth) mounts it over the whole router.
//! The Unix socket (the `notes-app` window) never has it: only the user can
//! reach the socket.
//!
//! What the browser needs to draw the sign-in screen is public - [`is_public`];
//! everything else needs a session.

use axum::Router;
use axum::http::{Method, StatusCode};
use axum::routing::get;

use crate::AppState;

/// Paths of the client page (the shell; the routing is in JS).
const PAGES: [&str; 4] = ["/v", "/n", "/graph", "/tags"];

/// The API paths of the sign-in screen: the theme and the fonts.
const OPEN_API: [&str; 3] = ["/api/themes", "/api/themes.css", "/api/fonts.css"];

/// Whether a request needs no session: the client page for every client
/// route, the client files, the fonts and the themes (the sign-in screen is
/// drawn with them). Only reads: any other method needs a session.
pub(crate) fn is_public(method: &Method, path: &str) -> bool {
    if !matches!(*method, Method::GET | Method::HEAD) {
        return false;
    }
    let under = |dir: &str| path.strip_prefix(dir).is_some_and(|rest| rest.starts_with('/'));
    path == "/"
        || PAGES.iter().any(|page| path == *page || under(page))
        || under("/assets")
        || under("/fonts")
        || OPEN_API.contains(&path)
}

/// Without sign-in `GET /api/session` answers 204: the client sees "no
/// sign-in here" (a 404 would show up in the browser console).
pub(crate) fn no_auth_routes() -> Router<AppState> {
    Router::new().route("/api/session", get(|| async { StatusCode::NO_CONTENT }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_sign_in_screen_needs_is_public() {
        for path in [
            "/",
            "/v/Study",
            "/v/Study/",
            "/v/Study/n/Network/SSH",
            "/n/Network/SSH",
            "/graph",
            "/tags/rust",
            "/assets/index-CX38oHQo.js",
            "/api/themes",
            "/api/themes.css",
            "/api/fonts.css",
            "/fonts/Lora/regular/latin.woff2",
        ] {
            assert!(is_public(&Method::GET, path), "{path}");
            assert!(is_public(&Method::HEAD, path), "{path}");
        }
    }

    #[test]
    fn data_and_writes_are_not_public() {
        for path in [
            "/api/vaults",
            "/api/vaults/test/notes",
            "/api/vaults/test/events",
            "/api/vaults/test/pdf/a",
            "/api/settings",
            "/api/session",
            "/api/themesx",
            "/api/themes/x",
            "/assetsx",
            "/graphs",
            "/vault",
            "/other",
        ] {
            assert!(!is_public(&Method::GET, path), "{path}");
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(!is_public(&method, "/"), "{method}");
            assert!(!is_public(&method, "/api/themes"), "{method}");
        }
    }
}
