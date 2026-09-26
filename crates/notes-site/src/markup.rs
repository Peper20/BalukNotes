//! Разметка сайта, которую сборка вшивает в страницы: оглавление, «Ссылаются
//! сюда», список заметок главной и страница тегов. Все — списки ссылок, их
//! пункты пишет одна функция [`link_list`]. Теги в шапке заметки становятся
//! ссылками на страницу тегов ([`link_tags`]).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use notes_core::NoteId;
use notes_core::graph::Backlink;
use notes_core::notes::{encode, static_path};
use notes_core::render::{Heading, slug};

/// Уровней оглавления от верхнего (как `panels.toc_depth` по умолчанию в клиенте).
pub(crate) const TOC_DEPTH: u8 = 2;

/// Пункт списка ссылок.
#[derive(Debug)]
pub(crate) struct Item {
    /// Адрес — уже закодированный.
    pub href: String,
    /// Текст ссылки (экранируется при записи).
    pub text: String,
    /// Готовый HTML ссылки вместо текста (заголовок с формулой).
    pub html: Option<String>,
    /// Уровень пункта оглавления (`data-depth`).
    pub depth: Option<u8>,
    /// Готовая разметка после ссылки (папка, раздел, «книга»).
    pub after: String,
}

impl Item {
    fn new(href: String, text: &str) -> Self {
        Self { href, text: text.to_owned(), html: None, depth: None, after: String::new() }
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
        let text = item.html.unwrap_or_else(|| escape(&item.text));
        let _ = write!(out, r#"><a href="{}">{text}</a>{}</li>"#, item.href, item.after);
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
    let items = shown.into_iter().map(|h| Item {
        depth: Some(h.level - top),
        html: h.html.clone(),
        ..Item::new(format!("#{}", encode(&h.id)), &h.text)
    });
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

/// Заметка на странице тегов.
#[derive(Debug)]
pub(crate) struct TagNote {
    /// Адрес от корня сайта — уже закодированный.
    pub href: String,
    pub title: String,
    /// Папка заметки (`""` — корень).
    pub folder: String,
}

/// `id` раздела тега на странице тегов (`tags.html#…`).
pub(crate) fn tag_anchor(tag: &str) -> String {
    slug(tag)
}

/// Страница тегов: облако (тег и число заметок; частые — первыми), затем
/// раздел на тег со списком заметок. Без скрипта.
pub(crate) fn tags_html(tags: &BTreeMap<String, Vec<TagNote>>) -> String {
    let mut out = String::from(r#"<header class="k-title"><h1>Теги</h1></header>"#);
    if tags.is_empty() {
        out.push_str(r#"<p class="k-site-tags-empty">Тегов пока нет: они задаются в шаблоне заметки — <code>tags: ("сеть", "linux")</code>.</p>"#);
        return out;
    }
    let mut by_count: Vec<_> = tags.iter().collect();
    by_count.sort_by(|(a, x), (b, y)| y.len().cmp(&x.len()).then_with(|| a.cmp(b)));
    out.push_str(r#"<p class="k-site-tag-cloud">"#);
    for (tag, notes) in &by_count {
        let _ = write!(
            out,
            r##"<a class="k-site-tag" href="#{}">#{} <small>{}</small></a>"##,
            encode(&tag_anchor(tag)),
            escape(tag),
            notes.len()
        );
    }
    out.push_str("</p>");
    for (tag, notes) in tags {
        let _ = write!(out, r#"<section class="k-site-tag-notes" id="{}"><h2>#{}</h2>"#, escape(&tag_anchor(tag)), escape(tag));
        let items = notes.iter().map(|n| Item {
            after: if n.folder.is_empty() { String::new() } else { format!("<span> · {}</span>", escape(&n.folder)) },
            ..Item::new(n.href.clone(), &n.title)
        });
        link_list(&mut out, "ul", items);
        out.push_str("</section>");
    }
    out
}

/// Теги в шапке заметки (`<ul class="k-tags"><li>тег</li>…`) — ссылками на
/// их раздел страницы тегов.
pub(crate) fn link_tags(body: &str, up: &str) -> String {
    const OPEN: &str = r#"<ul class="k-tags">"#;
    let Some(start) = body.find(OPEN) else { return body.to_owned() };
    let from = start + OPEN.len();
    let Some(len) = body[from..].find("</ul>") else { return body.to_owned() };
    let list = &body[from..from + len];
    let mut out = String::with_capacity(body.len() + list.len() * 2);
    out.push_str(&body[..from]);
    for item in list.split("<li>").filter(|s| !s.trim().is_empty()) {
        let text = item.trim_end().strip_suffix("</li>").unwrap_or(item).trim();
        let tag = unescape(text);
        let _ = write!(out, r#"<li><a href="{up}tags.html#{}">{text}</a></li>"#, encode(&tag_anchor(&tag)));
    }
    out.push_str(&body[from + len..]);
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading(level: u8, id: &str, text: &str) -> Heading {
        Heading { level, id: id.into(), anchor: id.into(), text: text.into(), html: None }
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
        // Заголовок с формулой — его HTML из ядра.
        let mut math = heading(2, "c", "ℝ");
        math.html = Some("<math><mi>ℝ</mi></math>".into());
        let toc = toc_html(&[heading(2, "a", "Раз"), math]);
        assert!(toc.contains(r##"<a href="#c"><math><mi>ℝ</mi></math></a>"##));
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
    fn tags_page_and_links() {
        let mut tags = BTreeMap::new();
        let note = |href: &str, folder: &str| TagNote { href: href.into(), title: "З".into(), folder: folder.into() };
        tags.insert("редкий".to_owned(), vec![note("a.html", "")]);
        tags.insert("с пробелом".to_owned(), vec![note("a.html", ""), note("p/b.html", "p")]);
        let html = tags_html(&tags);
        // Частые — первыми; раздел на тег, id — слаг.
        let cloud = html.find("#с пробелом <small>2").unwrap();
        assert!(cloud < html.find("#редкий <small>1").unwrap());
        assert!(html.contains(r##"<a class="k-site-tag" href="#с-пробелом">"##));
        assert!(html.contains(r#"<section class="k-site-tag-notes" id="с-пробелом"><h2>#с пробелом</h2><ul><li><a href="a.html">З</a></li><li><a href="p/b.html">З</a><span> · p</span></li></ul>"#));
        assert!(tags_html(&BTreeMap::new()).contains("Тегов пока нет"));

        let body = r#"<header><ul class="k-tags"><li>сеть</li><li>a &amp; b</li></ul></header><ul><li>x</li></ul>"#;
        assert_eq!(
            link_tags(body, "../"),
            r#"<header><ul class="k-tags"><li><a href="../tags.html#сеть">сеть</a></li><li><a href="../tags.html#a-and-b">a &amp; b</a></li></ul></header><ul><li>x</li></ul>"#
        );
        assert_eq!(link_tags("<p>без тегов</p>", ""), "<p>без тегов</p>");
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
