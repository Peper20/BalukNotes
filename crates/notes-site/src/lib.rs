//! Статический сайт из хранилища (`notes build`): страница на заметку,
//! список заметок, страница тегов и граф, стили, темы и шрифты рядом.
//! Работает без сервера (любой веб-сервер или просто файлы). В страницу
//! вшиты оглавление, «Ссылаются сюда» и ссылки тегов ([`markup`]) — они
//! видны и без JS; скрипт сайта (`app/src/static/`) добавляет тему,
//! интерактивные рисунки, ползунок кадров, поиск и граф. Данные поиска и
//! графа — файлы `assets/data/*.js` ([`data`]), собираются здесь же.
//! Файлы клиента — из сборки `app/dist` (`notes-assets`).

mod data;
mod markup;
mod page;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use notes_core::figures::FigureOptions;
use notes_core::fonts::{WebVariant, font_file_name};
use notes_core::notes::{encode, static_path};
use notes_core::vault_graph::GraphFilter;
use notes_core::{NoteKind, Notes};

use crate::markup::{IndexEntry, TagNote, backlinks_html, index_html, link_tags, tags_html, toc_html};
use crate::page::Page;

/// Файлы клиента, нужные сайту, кроме частей скрипта `static*.js` (их
/// сборка берёт все: `app/scripts/build-static.mjs`).
const ASSETS: &[&str] = &["baluk.css"];

/// Собрать сайт в `out` (создаётся; существующие файлы перезаписываются).
/// Заметки, которые не собрались, пропускаются — в конце ошибка с их числом.
pub fn build(notes: &Notes, out: &Path, opts: FigureOptions) -> Result<()> {
    fs::create_dir_all(out).with_context(|| format!("создать {}", out.display()))?;
    let assets = out.join("assets");
    fs::create_dir_all(&assets)?;
    let scripts = notes_assets::asset_names("static");
    if !scripts.iter().any(|n| n == "static.js") {
        bail!("нет static.js в сборке клиента app/dist");
    }
    for name in ASSETS.iter().copied().chain(scripts.iter().map(String::as_str)) {
        let file = notes_assets::asset(name).with_context(|| format!("нет {name} в сборке клиента app/dist"))?;
        fs::write(assets.join(name), file.data)?;
    }
    fs::write(assets.join("themes.css"), notes.themes().css())?;
    write_fonts(notes, &assets)?;

    let themes: Vec<_> = notes.themes().themes().iter().map(|t| (t.name.clone(), t.dark, t.title.clone())).collect();
    let themes_json = serde_json::to_string(&themes)?;
    let mut failed = 0;
    // Главная: заметки по папкам.
    let mut index: BTreeMap<String, Vec<IndexEntry>> = BTreeMap::new();
    // Тег → заметки (для страницы тегов); собранные заметки.
    let mut tags: BTreeMap<String, Vec<TagNote>> = BTreeMap::new();
    let mut built = HashSet::new();
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
        let body = format!(
            "{}{}",
            link_tags(&rendered.body, &up),
            backlinks_html(&up, &links.backlinks(&entry.id), |id| names.contains(id))
        );
        let html = Page { up: &up, kind, title: &title, styles: &rendered.styles, body: &body, themes: &themes_json }
            .html(&toc_html(&rendered.headings));
        let dst = out.join(&rel);
        fs::create_dir_all(dst.parent().expect("у страницы есть каталог"))?;
        fs::write(&dst, html)?;
        let folder = entry.id.as_str().rsplit_once('/').map_or("", |(f, _)| f);
        let href = encode(&rel.to_string_lossy());
        for tag in &rendered.tags {
            tags.entry(tag.clone()).or_default().push(TagNote {
                href: href.clone(),
                title: title.clone(),
                folder: folder.to_owned(),
            });
        }
        built.insert(entry.id.clone());
        index.entry(folder.to_owned()).or_default().push(IndexEntry {
            href,
            title,
            book: entry.kind == NoteKind::Book,
        });
    }
    write_extras(notes, out, &themes_json, &built, &tags)?;
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

/// Страница тегов, страница графа и данные для скрипта сайта: индекс поиска
/// и граф всего хранилища (узел несобравшейся заметки — как ненаписанной:
/// её страницы нет).
fn write_extras(
    notes: &Notes,
    out: &Path,
    themes: &str,
    built: &HashSet<notes_core::NoteId>,
    tags: &BTreeMap<String, Vec<TagNote>>,
) -> Result<()> {
    let page = |kind, title, body: &str| Page { up: "", kind, title, styles: "", body, themes }.html("");
    fs::write(out.join("tags.html"), page("tags", "Теги", &tags_html(tags)))?;
    let graph_body = r#"<div class="k-site-graph" id="k-site-graph"><noscript><p class="k-site-graph-empty">Граф рисует скрипт сайта — включите JavaScript.</p></noscript></div>"#;
    fs::write(out.join("graph.html"), page("graph", "Граф заметок", graph_body))?;

    let dir = out.join("assets/data");
    let docs = notes.search_documents(|id| built.contains(id))?;
    data::write(&dir, "search", &docs)?;
    let mut graph = notes.graph_layout(&GraphFilter::default())?;
    for node in &mut graph.nodes {
        if node.kind.is_some() && !built.iter().any(|id| id.as_str() == node.id) {
            node.kind = None;
        }
    }
    data::write(&dir, "graph", &graph)?;
    Ok(())
}

/// Шрифты оформления: части WOFF2 (по наборам знаков) и `fonts.css` с
/// `unicode-range` — как у сервера. Части сжимаются параллельно.
fn write_fonts(notes: &Notes, assets: &Path) -> Result<()> {
    let dir = assets.join("fonts");
    fs::create_dir_all(&dir)?;
    let families = notes.themes().web_fonts();
    let mut jobs = Vec::new();
    for family in families {
        for v in WebVariant::ALL {
            let Some(face) = notes.fonts().web_face(family, v) else { continue };
            for chunk in &face.chunks {
                jobs.push((face.clone(), font_file_name(family, v, &chunk.name), chunk.name.clone()));
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
    fs::write(assets.join("fonts.css"), notes.fonts().font_faces(families, "fonts/"))?;
    Ok(())
}
