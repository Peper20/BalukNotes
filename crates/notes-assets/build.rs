//! Релизная сборка встраивает клиент из `app/dist` — без него она бесполезна.
//! Отладочная читает клиент с диска и собирается и без него (тесты API).

fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    assert!(
        !release || dist.join("index.html").is_file(),
        "нет app/dist — сначала соберите клиент: npm --prefix app ci && npm --prefix app run build"
    );
}
