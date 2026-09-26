//! Разметка сайта, которую сборка вшивает в страницы: оглавление, «Ссылаются
//! сюда» и список заметок главной. Все три — списки ссылок, их пункты пишет
//! одна функция [`link_list`].

use std::collections::BTreeMap;
use std::fmt::Write as _;

use notes_core::NoteId;
use notes_core::graph::Backlink;
use notes_core::notes::{encode, static_path};
use notes_core::render::Heading;

/// Уровней оглавления от верхнего (как `panels.toc_depth` по умолчанию в клиенте).
pub(crate) const TOC_DEPTH: u8 = 2;

/// Пункт списка ссылок.
#[derive(Debug)]
pub(crate) struct Item {
    /// Адрес — уже закодированный.
    pub href: String,
    /// Текст ссылки (экранируется при записи).
    pub text: String,
    /// Уровень пункта оглавления (`data-depth`).
    pub depth: Option<u8>,
    /// Готовая разметка после ссылки (папка, раздел, «книга»).
    pub after: String,
}

impl Item {
    fn new(href: String, text: &str) -> Self {
        Self { href, text: text.to_owned(), depth: None, after: String::new() }
    }
}

/// Список ссылок `<{tag}><li><a href="…">…</a>…</li>…</{tag}>` (`tag` — `ol` или `ul`).
pub(crate) fn link_list(out: &mut String, tag: &str, items: impl IntoIterator<Item = Item>) {
    let _ = write!(out, "<{tag}>");
    for item in items {
        out.push_str("<li");
        if let Some(depth) = item.depth {
            let _ = write!(out, r#" data-depth="{depth}""#);
        }
        let _ = write!(out, r#"><a href="{}">{}</a>{}</li>"#, item.href, escape(&item.text), item.after);
    }
    let _ = write!(out, "</{tag}>");
}

/// Оглавление: `TOC_DEPTH` уровней от верхнего; меньше двух пунктов — нет.
/// Свёрнуто: скрипт сайта раскрывает его на широком экране.
pub(crate) fn toc_html(headings: &[Heading]) -> String {
    let Some(top) = headings.iter().map(|h| h.level).min() else { return String::new() };
    let shown: Vec<_> = headings.iter().filter(|h| h.level - top < TOC_DEPTH).collect();
    if shown.len() < 2 {
        return String::new();
    }
    let mut out = String::from(r#"<details class="k-static-toc"><summary>Содержание</summary>"#);
    let items = shown
        .into_iter()
        .map(|h| Item { depth: Some(h.level - top), ..Item::new(format!("#{}", encode(&h.id)), &h.text) });
    link_list(&mut out, "ol", items);
    out.push_str("</details>");
    out
}

/// «Ссылаются сюда»: кто ссылается на заметку и на какой раздел.
/// `exists` — есть ли такая страница (ссылки из черновиков не ведут никуда).
pub(crate) fn backlinks_html(up: &str, backlinks: &[Backlink], exists: impl Fn(&NoteId) -> bool) -> String {
    let shown: Vec<_> = backlinks.iter().filter(|b| exists(&b.from)).collect();
    if shown.is_empty() {
        return String::new();
    }
    let mut out =
        format!(r#"<section class="k-static-backlinks" id="backlinks"><h2>Ссылаются сюда · {}</h2>"#, shown.len());
    let items = shown.into_iter().map(|b| {
        let (folder, name) = b.from.as_str().rsplit_once('/').unwrap_or(("", b.from.as_str()));
        let mut item = Item::new(format!("{up}{}", encode(&static_path(&b.from).to_string_lossy())), name);
        if !folder.is_empty() {
            let _ = write!(item.after, "<span> · {}</span>", escape(folder));
        }
        if let Some(anchor) = &b.anchor {
            let _ = write!(item.after, "<span> → «{}»</span>", escape(anchor));
        }
        item
    });
    link_list(&mut out, "ul", items);
    out.push_str("</section>");
    out
}

/// Заметка в списке главной.
#[derive(Debug)]
pub(crate) struct IndexEntry {
    pub href: String,
    pub title: String,
    pub book: bool,
}

/// Список заметок главной: сначала заметки из корня, затем папки.
pub(crate) fn index_html(index: &BTreeMap<String, Vec<IndexEntry>>) -> String {
    let mut out = String::from(r#"<div class="k-index">"#);
    for (folder, notes) in index {
        out.push_str("<section>");
        if !folder.is_empty() {
            let _ = write!(out, "<h2>{}</h2>", escape(folder));
        }
        let items = notes.iter().map(|n| Item {
            after: if n.book { r#" <span class="k-index-book">книга</span>"#.into() } else { String::new() },
            ..Item::new(n.href.clone(), &n.title)
        });
        link_list(&mut out, "ul", items);
        out.push_str("</section>");
    }
    out.push_str("</div>");
    out
}

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(toc.starts_with(r#"<details class="k-static-toc"><summary>Содержание</summary><ol>"#));
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

    #[test]
    fn index_folders_and_book_badge() {
        let mut index = BTreeMap::new();
        index.insert(String::new(), vec![IndexEntry { href: "a.html".into(), title: "А & Б".into(), book: false }]);
        index.insert("Папка".into(), vec![IndexEntry { href: "p/k.html".into(), title: "К".into(), book: true }]);
        assert_eq!(
            index_html(&index),
            r#"<div class="k-index"><section><ul><li><a href="a.html">А &amp; Б</a></li></ul></section><section><h2>Папка</h2><ul><li><a href="p/k.html">К</a> <span class="k-index-book">книга</span></li></ul></section></div>"#
        );
    }
}
