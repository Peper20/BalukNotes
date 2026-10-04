//! Reference snapshots of rendering: every note of `tests/vault` -> a text file
//! in `tests/snapshots/`. Any change of the library, the core or Typst that
//! changes the HTML shows in the snapshot diff.
//!
//!   cargo test -p notes-core --test it snapshots::                       # compare
//!   UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it snapshots::    # update
//!
//! The contents of SVG (paths, glyphs) and data URLs are replaced by size and
//! hash: otherwise snapshots would weigh megabytes and the diff would be
//! unreadable. A change of a figure still shows, by the hash.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use notes_core::NotePage;
use notes_core::figures::FigureOptions;

use crate::common::{NOTES, repo};

#[test]
fn rendering_matches_snapshots() {
    let notes = &*NOTES;
    let dir = repo().join("tests/snapshots");
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();

    let mut expected: Vec<PathBuf> = Vec::new();
    let mut failed = Vec::new();
    for entry in notes.entries().unwrap() {
        let page = notes.page(&entry.id, FigureOptions::default()).unwrap();
        let path = dir.join(format!("{}.snap", entry.id));
        let actual = snapshot(&page);
        expected.push(path.clone());
        let old = std::fs::read_to_string(&path).ok();
        if old.as_deref() == Some(actual.as_str()) {
            continue;
        }
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &actual).unwrap();
        } else {
            let first = first_difference(old.as_deref().unwrap_or(""), &actual);
            failed.push(format!("{}: {first}", entry.id));
        }
    }

    // Snapshots of notes that no longer exist.
    let stale: Vec<_> = walk(&dir).into_iter().filter(|p| !expected.contains(p)).collect();
    if update {
        for p in &stale {
            std::fs::remove_file(p).unwrap();
        }
    } else if !stale.is_empty() {
        failed.push(format!("stale snapshots: {stale:?}"));
    }

    assert!(
        failed.is_empty(),
        "rendering changed: check the diff and update the snapshots: \
         UPDATE_SNAPSHOTS=1 cargo test -p notes-core --test it snapshots::\n{}",
        failed.join("\n")
    );
}

/// The snapshot text: note info, then the page body.
fn snapshot(page: &NotePage) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "id: {}\nkind: {:?}", page.id, page.kind);
    for d in page.errors.iter().chain(&page.warnings) {
        let at = d.file.as_deref().map(|f| format!("{f}:{}: ", d.line.unwrap_or(0))).unwrap_or_default();
        let _ = writeln!(s, "{:?}: {at}{}", d.severity, d.message);
    }
    let Some(r) = &page.rendered else {
        s.push_str("(no rendering)\n");
        return s;
    };
    let _ = writeln!(s, "title: {}", r.title.as_deref().unwrap_or("—"));
    let _ = writeln!(s, "tags: {}", r.tags.join(", "));
    s.push_str("headings:\n");
    for h in &r.headings {
        let _ = writeln!(s, "  {} #{} [{}] {}", h.level, h.id, h.anchor, h.text);
        if let Some(html) = &h.html {
            let _ = writeln!(s, "    html: {html}");
        }
    }
    s.push_str("links:\n");
    for l in &r.links {
        let _ = writeln!(s, "  {}{}", l.target, l.anchor.as_deref().map(|a| format!(" / {a}")).unwrap_or_default());
    }
    let _ = writeln!(s, "styles: {}", summary(&r.styles));
    s.push_str("---\n");
    s.push_str(&break_blocks(&collapse(&r.body)));
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// A block element starts a new line, so the diff shows the changed paragraph,
/// not the whole page as one line.
fn break_blocks(html: &str) -> String {
    const BLOCKS: &str = "article header section nav aside div p h1 h2 h3 h4 h5 h6 ul ol li figure figcaption \
                          table thead tbody tr pre blockquote details svg img";
    let mut out = String::with_capacity(html.len() + html.len() / 16);
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let name: String = rest[1..].chars().take_while(char::is_ascii_alphanumeric).collect();
        if BLOCKS.split_whitespace().any(|b| b == name) && !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('<');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

/// Replaces the contents of `<svg>...</svg>` and `data:` URLs with size and hash.
fn collapse(body: &str) -> String {
    let mut out = String::with_capacity(body.len() / 4);
    let mut rest = body;
    while let Some(start) = rest.find("<svg") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let open_end = tail.find('>').map_or(tail.len(), |i| i + 1);
        let close = tail.find("</svg>").map_or(tail.len(), |i| i + "</svg>".len());
        out.push_str(&tail[..open_end]);
        let _ = write!(out, "…{}…</svg>", summary(&tail[open_end..close]));
        rest = &tail[close..];
    }
    out.push_str(rest);
    collapse_data_urls(&out)
}

fn collapse_data_urls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("\"data:") {
        out.push_str(&rest[..=start]);
        let tail = &rest[start + 1..];
        let end = tail.find('"').unwrap_or(tail.len());
        let kind = tail[..end].split([';', ',']).next().unwrap_or("");
        let _ = write!(out, "{kind};{}", summary(&tail[..end]));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// `12345 bytes, fnv 0123abcd...`: a hash stable across Rust versions.
fn summary(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{} bytes, fnv {h:016x}", s.len())
}

fn first_difference(old: &str, new: &str) -> String {
    if old.is_empty() {
        return "no snapshot".into();
    }
    let line =
        old.lines().zip(new.lines()).position(|(a, b)| a != b).unwrap_or(old.lines().count().min(new.lines().count()));
    format!(
        "line {}: was \"{}\", now \"{}\"",
        line + 1,
        old.lines().nth(line).unwrap_or("").chars().take(120).collect::<String>(),
        new.lines().nth(line).unwrap_or("").chars().take(120).collect::<String>()
    )
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else { return out };
    for e in read.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "snap") {
            out.push(p);
        }
    }
    out
}
