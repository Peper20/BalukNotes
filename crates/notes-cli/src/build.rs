//! `notes build`: статический сайт — страница на заметку, оглавление,
//! стили, темы и шрифты рядом. Работает без сервера (любой веб-сервер или
//! просто файлы).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use notes_core::figures::FigureOptions;
use notes_core::fonts::WebVariant;
use notes_core::notes::{encode, static_path};
use notes_core::{NoteKind, Notes};

const ASSETS: &[&str] = &["konspekt.css", "static.js"];

pub fn build(notes: &Notes, out: &Path, opts: FigureOptions) -> Result<()> {
    fs::create_dir_all(out).with_context(|| format!("создать {}", out.display()))?;
    let assets = out.join("assets");
    fs::create_dir_all(&assets)?;
    for name in ASSETS {
        let data = notes_server::web_asset(name).with_context(|| format!("нет {name} в сборке клиента app/dist"))?;
        fs::write(assets.join(name), data)?;
    }
    fs::write(assets.join("themes.css"), notes.themes().css())?;
    write_fonts(notes, &assets)?;

    let themes: Vec<_> = notes.themes().themes().iter().map(|t| (t.name.clone(), t.dark)).collect();
    let themes_json = serde_json::to_string(&themes)?;
    let mut failed = 0;
    let mut index = String::new();
    let entries = notes.entries()?;
    for entry in &entries {
        let page = notes.page_static(entry, opts);
        for e in &page.errors {
            eprintln!("{}: {e}", entry.id);
        }
        let Some(rendered) = &page.rendered else {
            failed += 1;
            continue;
        };
        let rel = static_path(&entry.id);
        let up = "../".repeat(entry.id.as_str().matches('/').count());
        let kind = match entry.kind {
            NoteKind::Note => "note",
            NoteKind::Book => "book",
        };
        let title = rendered.title.clone().unwrap_or_else(|| entry.id.name().to_owned());
        let html = page_html(&up, kind, &title, &rendered.styles, &rendered.body, &themes_json);
        let dst = out.join(&rel);
        fs::create_dir_all(dst.parent().expect("у страницы есть каталог"))?;
        fs::write(&dst, html)?;
        let _ =
            write!(index, r#"<li><a href="{}">{}</a></li>"#, encode(&rel.to_string_lossy()), escape(entry.id.as_str()));
    }
    let body = format!(r#"<header class="k-title"><h1>Заметки</h1></header><ul class="k-index">{index}</ul>"#);
    fs::write(out.join("index.html"), page_html("", "index", "Заметки", "", &body, &themes_json))?;

    println!("собрано: {} из {} → {}", entries.len() - failed, entries.len(), out.display());
    if failed > 0 {
        bail!("не собралось заметок: {failed}");
    }
    Ok(())
}

fn write_fonts(notes: &Notes, assets: &Path) -> Result<()> {
    let mut css = String::new();
    for family in notes_server::web_fonts() {
        for v in WebVariant::ALL {
            let Some(font) = notes.fonts().web_font(family, v) else { continue };
            let ext = if font.mime == "font/otf" { "otf" } else { "ttf" };
            let file = format!("{}-{}.{ext}", family.replace(' ', "-"), v.slug());
            fs::create_dir_all(assets.join("fonts"))?;
            fs::write(assets.join("fonts").join(&file), font.data.as_slice())?;
            let _ = writeln!(
                css,
                "@font-face {{ font-family: \"{family}\"; src: url(\"fonts/{file}\"); font-style: {}; font-weight: {}; font-display: swap; }}",
                if v.italic { "italic" } else { "normal" },
                if v.bold { 700 } else { 400 },
            );
        }
    }
    fs::write(assets.join("fonts.css"), css)?;
    Ok(())
}

fn page_html(up: &str, kind: &str, title: &str, styles: &str, body: &str, themes: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html lang="ru" data-kind="{kind}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title}</title><link rel="stylesheet" href="{up}assets/fonts.css"><link rel="stylesheet" href="{up}assets/konspekt.css"><link rel="stylesheet" href="{up}assets/themes.css">{styles}<script>window.K_THEMES = {themes};</script><script src="{up}assets/static.js"></script></head><body class="k-static"><nav class="k-toolbar"><a href="{up}index.html">все заметки</a><button id="k-theme" type="button"></button></nav><main class="k-note">{body}</main></body></html>"#,
        title = escape(title),
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
