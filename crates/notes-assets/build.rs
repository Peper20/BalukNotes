//! A release build embeds the client from `app/dist` and is useless without it.
//! A debug build reads the client from disk and builds without it (API tests).

fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    assert!(
        !release || dist.join("index.html").is_file(),
        "no app/dist: build the client first: npm --prefix app ci && npm --prefix app run build"
    );
}
