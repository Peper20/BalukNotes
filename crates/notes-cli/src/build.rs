//! `notes build`: статический сайт — страница на заметку, список заметок,
//! стили, темы и шрифты рядом. Работает без сервера (любой веб-сервер или
//! просто файлы). В страницу вшиты оглавление и «Ссылаются сюда» — они
//! видны и без JS; скрипт сайта (`app/src/static.ts`) добавляет тему,
//! интерактивные рисунки и ползунок кадров.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use notes_core::figures::FigureOptions;
use notes_core::fonts::WebVariant;
use notes_core::graph::Backlink;
use notes_core::notes::{encode, static_path};
use notes_core::render::Heading;
use notes_core::{NoteKind, Notes};

/// Уровней оглавления от верхнего (как `panels.toc_depth` по умолчанию в клиенте).
const TOC_DEPTH: u8 = 2;

const ASSETS: &[&str] = &["baluk.css", "static.js"];

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

    let themes: Vec<_> = notes.themes().themes().iter().map(|t| (t.name.clone(), t.dark, t.title.clone())).collect();
    let themes_json = serde_json::to_string(&themes)?;
    let mut failed = 0;
    // Главная: заметки по папкам (папка → [(ссылка, заголовок, книга ли)]).
    let mut index: BTreeMap<String, Vec<(String, String, bool)>> = BTreeMap::new();
    let entries = notes.entries()?;
    let links = notes.index()?;
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
        let names: Vec<_> = entries.iter().map(|e| e.id.clone()).collect();
        let body =
            format!("{}{}", rendered.body, backlinks_html(&up, &links.backlinks(&entry.id), |id| names.contains(id)));
        let html = Page { up: &up, kind, title: &title, styles: &rendered.styles, body: &body, themes: &themes_json }
            .html(&toc_html(&rendered.headings));
        let dst = out.join(&rel);
        fs::create_dir_all(dst.parent().expect("у страницы есть каталог"))?;
        fs::write(&dst, html)?;
        let folder = entry.id.as_str().rsplit_once('/').map_or("", |(f, _)| f);
        index.entry(folder.to_owned()).or_default().push((
            encode(&rel.to_string_lossy()),
            title,
            entry.kind == NoteKind::Book,
        ));
    }
    let body = format!(r#"<header class="k-title"><h1>Заметки</h1></header>{}"#, index_html(&index));
    let index_page =
        Page { up: "", kind: "index", title: "Заметки", styles: "", body: &body, themes: &themes_json };
    fs::write(out.join("index.html"), index_page.html(""))?;

    println!("собрано: {} из {} → {}", entries.len() - failed, entries.len(), out.display());
    if failed > 0 {
        bail!("не собралось заметок: {failed}");
    }
    Ok(())
}

/// Шрифты оформления: части WOFF2 (по наборам знаков) и `fonts.css` с
/// `unicode-range` — как у сервера. Части сжимаются параллельно.
fn write_fonts(notes: &Notes, assets: &Path) -> Result<()> {
    let dir = assets.join("fonts");
    fs::create_dir_all(&dir)?;
    let mut jobs = Vec::new();
    for family in notes_server::web_fonts() {
        for v in WebVariant::ALL {
            let Some(face) = notes.fonts().web_face(family, v) else { continue };
            for chunk in &face.chunks {
                jobs.push((face.clone(), notes_server::font_file_name(family, v, &chunk.name), chunk.name.clone()));
            }
        }
    }
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| -> Result<()> {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> Result<()> {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((face, file, chunk)) = jobs.get(i) else { return Ok(()) };
                        let data = face.file(chunk).with_context(|| format!("шрифт {file}: не вышло сжать"))?;
                        fs::write(dir.join(file), &*data)?;
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("поток сжатия шрифтов")?;
        }
        Ok(())
    })?;
    fs::write(assets.join("fonts.css"), notes_server::font_faces(notes, "fonts/"))?;
    Ok(())
}

struct Page<'a> {
    /// Путь к корню сайта (`../` на каждый уровень папок).
    up: &'a str,
    kind: &'a str,
    title: &'a str,
    styles: &'a str,
    body: &'a str,
    themes: &'a str,
}

impl Page<'_> {
    /// Страница целиком; `toc` — оглавление (идёт до `<main>`).
    fn html(&self, toc: &str) -> String {
        let Page { up, kind, styles, body, themes, .. } = self;
        format!(
            r#"<!DOCTYPE html><html lang="ru" data-kind="{kind}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title}</title><link rel="stylesheet" href="{up}assets/fonts.css"><link rel="stylesheet" href="{up}assets/baluk.css"><link rel="stylesheet" href="{up}assets/themes.css">{styles}<script>window.K_THEMES = {themes};</script><script src="{up}assets/static.js"></script></head><body class="k-static"><nav class="k-toolbar"><a class="k-toolbar-home" href="{up}index.html">Все заметки</a><button id="k-theme" type="button" hidden></button></nav>{toc}<main class="k-note">{body}</main></body></html>"#,
            title = escape(self.title),
        )
    }
}

/// Оглавление: `TOC_DEPTH` уровней от верхнего; меньше двух пунктов — нет.
/// Свёрнуто: скрипт сайта раскрывает его на широком экране.
/// Список заметок главной: сначала заметки из корня, затем папки.
fn index_html(index: &BTreeMap<String, Vec<(String, String, bool)>>) -> String {
    let mut out = String::from(r#"<div class="k-index">"#);
    for (folder, notes) in index {
        out.push_str("<section>");
        if !folder.is_empty() {
            let _ = write!(out, "<h2>{}</h2>", escape(folder));
        }
        out.push_str("<ul>");
        for (href, title, book) in notes {
            let badge = if *book { r#" <span class="k-index-book">книга</span>"# } else { "" };
            let _ = write!(out, r#"<li><a href="{href}">{}</a>{badge}</li>"#, escape(title));
        }
        out.push_str("</ul></section>");
    }
    out.push_str("</div>");
    out
}

fn toc_html(headings: &[Heading]) -> String {
    let Some(top) = headings.iter().map(|h| h.level).min() else { return String::new() };
    let shown: Vec<_> = headings.iter().filter(|h| h.level - top < TOC_DEPTH).collect();
    if shown.len() < 2 {
        return String::new();
    }
    let mut out = String::from(r#"<details class="k-static-toc"><summary>Содержание</summary><ol>"#);
    for h in shown {
        let _ = write!(
            out,
            r##"<li data-depth="{}"><a href="#{}">{}</a></li>"##,
            h.level - top,
            encode(&h.id),
            escape(&h.text)
        );
    }
    out.push_str("</ol></details>");
    out
}

/// «Ссылаются сюда»: кто ссылается на заметку и на какой раздел.
/// `exists` — есть ли такая страница (ссылки из черновиков не ведут никуда).
fn backlinks_html(up: &str, backlinks: &[Backlink], exists: impl Fn(&notes_core::NoteId) -> bool) -> String {
    let shown: Vec<_> = backlinks.iter().filter(|b| exists(&b.from)).collect();
    if shown.is_empty() {
        return String::new();
    }
    let mut out =
        format!(r#"<section class="k-static-backlinks" id="backlinks"><h2>Ссылаются сюда · {}</h2><ul>"#, shown.len());
    for b in shown {
        let (folder, name) = b.from.as_str().rsplit_once('/').unwrap_or(("", b.from.as_str()));
        let _ = write!(
            out,
            r#"<li><a href="{up}{}">{}</a>"#,
            encode(&static_path(&b.from).to_string_lossy()),
            escape(name)
        );
        if !folder.is_empty() {
            let _ = write!(out, "<span> · {}</span>", escape(folder));
        }
        if let Some(anchor) = &b.anchor {
            let _ = write!(out, "<span> → «{}»</span>", escape(anchor));
        }
        out.push_str("</li>");
    }
    out.push_str("</ul></section>");
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use notes_core::NoteId;

    fn heading(level: u8, id: &str, text: &str) -> Heading {
        Heading { level, id: id.into(), anchor: id.into(), text: text.into() }
    }

    #[test]
    fn toc_two_levels_from_top() {
        let toc = toc_html(&[
            heading(2, "a", "Раз"),
            heading(3, "a-1", "Раз <и> два"),
            heading(4, "a-1-1", "глубоко"),
            heading(2, "b", "Два"),
        ]);
        assert!(toc.starts_with(r#"<details class="k-static-toc">"#));
        assert!(toc.contains(r##"<li data-depth="1"><a href="#a-1">Раз &lt;и&gt; два</a></li>"##));
        assert!(!toc.contains("глубоко"));
        // один пункт — не оглавление
        assert_eq!(toc_html(&[heading(2, "a", "Раз")]), "");
    }

    #[test]
    fn backlinks_relative_and_only_existing() {
        let b = |from: &str, anchor: Option<&str>| Backlink {
            from: NoteId::new(from).unwrap(),
            anchor: anchor.map(Into::into),
        };
        let html = backlinks_html(
            "../",
            &[b("Сеть/UFW", Some("Смена порта")), b("Черновик", None)],
            |id| id.as_str() != "Черновик",
        );
        assert!(html.contains("Ссылаются сюда · 1"));
        assert!(html.contains(&format!(r#"<a href="../{}">UFW</a>"#, encode("Сеть/UFW.html"))));
        assert!(html.contains("· Сеть") && html.contains("→ «Смена порта»"));
        assert_eq!(backlinks_html("", &[], |_| true), "");
    }
}
