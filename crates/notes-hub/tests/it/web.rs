//! The browser version: `Hub::with_web` serves a built folder next to the API.

use std::path::Path;

use axum::Router;
use axum::http::StatusCode;

use crate::common::{Harness, send};

const INDEX: &str = "<!doctype html><title>page</title>";

/// A bundle in a folder: the page, a hashed and a plain asset, the module,
/// the stylesheets and a font part.
fn bundle(dir: &Path) {
    for (path, data) in [
        ("index.html", INDEX.as_bytes()),
        ("assets/index-AbCd1234.js", b"console.log(1)".as_slice()),
        ("assets/config.js", b"globalThis.__NOTES_API__ = {};"),
        ("wasm/notes_wasm_bg.wasm", b"\0asm\x01\0\0\0"),
        ("wasm/fonts.json", br#"{"fonts":["fonts/font-0a1b2c3d.bin"]}"#),
        ("wasm/fonts/font-0a1b2c3d.bin", b"\0\x01\0\0font"),
        ("api/fonts.css", b"@font-face{}"),
        ("api/themes.css", b":root{}"),
        ("fonts/Some Font/regular/a.woff2", b"wOF2"),
    ] {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }
}

/// The hub of `h` with the folder `web`.
fn with_web(h: &Harness, web: &Path) -> Router {
    h.hub.clone().with_web(web).unwrap().router()
}

#[tokio::test]
async fn files_with_type_cache_and_csp() {
    let h = Harness::new(&["ivan"]);
    let web = tempfile::tempdir().unwrap();
    bundle(web.path());
    let app = with_web(&h, web.path());

    let wasm = send(&app, "GET", "/wasm/notes_wasm_bg.wasm", &[], vec![]).await;
    assert_eq!(wasm.status, StatusCode::OK);
    assert_eq!(wasm.header("content-type"), "application/wasm");
    assert_eq!(wasm.header("cache-control"), "no-cache");
    assert_eq!(wasm.body, b"\0asm\x01\0\0\0");
    assert!(wasm.header("content-security-policy").contains("script-src 'self' 'wasm-unsafe-eval';"));

    let hashed = send(&app, "GET", "/assets/index-AbCd1234.js", &[], vec![]).await;
    assert_eq!(hashed.header("content-type"), "text/javascript; charset=utf-8");
    assert_eq!(hashed.header("cache-control"), "public, max-age=31536000, immutable");
    let config = send(&app, "GET", "/assets/config.js", &[], vec![]).await;
    assert_eq!(config.header("cache-control"), "no-cache");

    // The fonts of the core module are named by a hash of their content: immutable; their list is not.
    let font = send(&app, "GET", "/wasm/fonts/font-0a1b2c3d.bin", &[], vec![]).await;
    assert_eq!((font.status, font.header("content-type")), (StatusCode::OK, "application/octet-stream"));
    assert_eq!(font.header("cache-control"), "public, max-age=31536000, immutable");
    assert_eq!(font.body, b"\0\x01\0\0font");
    let list = send(&app, "GET", "/wasm/fonts.json", &[], vec![]).await;
    assert_eq!((list.status, list.header("content-type")), (StatusCode::OK, "application/json"));
    assert_eq!(list.header("cache-control"), "no-cache");

    // Nobody is signed in: the stylesheets and the fonts are public, a name is percent-decoded.
    for uri in ["/api/fonts.css", "/api/themes.css"] {
        let css = send(&app, "GET", uri, &[], vec![]).await;
        assert_eq!((css.status, css.header("content-type")), (StatusCode::OK, "text/css; charset=utf-8"), "{uri}");
    }
    let font = send(&app, "GET", "/fonts/Some%20Font/regular/a.woff2", &[], vec![]).await;
    assert_eq!((font.status, font.header("content-type")), (StatusCode::OK, "font/woff2"));
}

#[tokio::test]
async fn a_large_file_is_served_intact_with_its_length() {
    let h = Harness::new(&["ivan"]);
    let web = tempfile::tempdir().unwrap();
    bundle(web.path());
    // Several MB that are not a repeat of one block, so a lost or doubled chunk shows.
    let big: Vec<u8> = (0..5_000_000_u32).map(|i| u8::try_from(i.wrapping_mul(2_654_435_761) >> 24).unwrap()).collect();
    std::fs::write(web.path().join("wasm/big.wasm"), &big).unwrap();
    let app = with_web(&h, web.path());

    let reply = send(&app, "GET", "/wasm/big.wasm", &[], vec![]).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header("content-length"), big.len().to_string());
    assert_eq!(reply.header("content-type"), "application/wasm");
    assert!(reply.body == big, "the body differs from the file");
}

#[tokio::test]
async fn client_routes_answer_the_page() {
    let h = Harness::new(&["ivan"]);
    let web = tempfile::tempdir().unwrap();
    bundle(web.path());
    let app = with_web(&h, web.path());

    for uri in ["/", "/v/x/n/y", "/v/x/graph", "/n/a%20b", "/index.html"] {
        let page = send(&app, "GET", uri, &[], vec![]).await;
        assert_eq!(page.status, StatusCode::OK, "{uri}");
        assert_eq!(page.text(), INDEX, "{uri}");
        assert_eq!(page.header("content-type"), "text/html; charset=utf-8");
        assert_eq!(page.header("cache-control"), "no-cache");
        assert!(page.header("content-security-policy").contains("object-src 'none'"));
    }
    // A folder of files only: a missing file is a 404, not the page.
    for uri in ["/assets/nope.js", "/wasm/x.wasm", "/fonts/a/b/c.woff2", "/assets"] {
        let missing = send(&app, "GET", uri, &[], vec![]).await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND, "{uri}");
    }
    // The rest of `/api/` is behind the session.
    let api = send(&app, "GET", "/api/nope.css", &[], vec![]).await;
    assert_eq!(api.status, StatusCode::UNAUTHORIZED);
    let post = send(&app, "POST", "/v/x/n/y", &[], vec![]).await;
    assert_eq!(post.status, StatusCode::UNAUTHORIZED, "only GET is public");
}

#[tokio::test]
async fn the_hubs_own_routes_win() {
    let h = Harness::new(&["ivan"]);
    let web = tempfile::tempdir().unwrap();
    bundle(web.path());
    // Files that would shadow the API, were the files first.
    std::fs::write(web.path().join("api/session"), "file").unwrap();
    std::fs::create_dir_all(web.path().join("api/sync")).unwrap();
    std::fs::write(web.path().join("api/sync/vaults"), "file").unwrap();
    let app = with_web(&h, web.path());

    let session = send(&app, "GET", "/api/session", &[], vec![]).await;
    assert_eq!(session.status, StatusCode::UNAUTHORIZED);
    assert_eq!(session.error(), "sign-in required");
    let token = h.login("ivan").await;
    let bearer = format!("Bearer {token}");
    let session = send(&app, "GET", "/api/session", &[("authorization", &bearer)], vec![]).await;
    assert_eq!(session.json::<notes_hub::api::SessionResponse>().login, "ivan");
    let vaults = send(&app, "GET", "/api/sync/vaults", &[("authorization", &bearer)], vec![]).await;
    assert_eq!(vaults.text(), "[]");
    let anonymous = send(&app, "GET", "/api/sync/vaults", &[], vec![]).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn paths_stay_inside_the_folder() {
    let h = Harness::new(&["ivan"]);
    let outer = tempfile::tempdir().unwrap();
    let web = outer.path().join("web");
    bundle(&web);
    std::fs::write(outer.path().join("secret.txt"), "secret").unwrap();
    std::os::unix::fs::symlink(outer.path().join("secret.txt"), web.join("assets/leak.txt")).unwrap();
    std::os::unix::fs::symlink(outer.path(), web.join("wasm/up")).unwrap();
    let app = with_web(&h, &web);

    for uri in ["/../secret.txt", "/assets/../../secret.txt", "/assets/%2e%2e/%2e%2e/secret.txt", "/%2e%2e/", "/.."] {
        let reply = send(&app, "GET", uri, &[], vec![]).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{uri}");
        assert!(!reply.text().contains("secret"), "{uri}");
    }
    let bad_bytes = send(&app, "GET", "/%ff", &[], vec![]).await;
    assert_eq!(bad_bytes.status, StatusCode::BAD_REQUEST);
    for uri in ["/assets/leak.txt", "/wasm/up/secret.txt"] {
        let reply = send(&app, "GET", uri, &[], vec![]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(!reply.text().contains("secret"), "{uri}");
    }
}

#[test]
fn a_folder_without_a_page_is_refused() {
    let h = Harness::new(&[]);
    let empty = tempfile::tempdir().unwrap();
    let e = h.hub.clone().with_web(empty.path()).unwrap_err().to_string();
    assert!(e.contains("no index.html") && e.contains("tools/web-bundle.sh"), "{e}");
    assert!(h.hub.clone().with_web(&empty.path().join("nope")).is_err());
}

#[tokio::test]
async fn without_web_nothing_changes() {
    let h = Harness::new(&["ivan"]);
    for uri in ["/", "/v/x/n/y", "/assets/index.js", "/api/themes.css"] {
        let reply = h.send("GET", uri, &[], vec![]).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri}: behind sign-in, no page");
    }
}
