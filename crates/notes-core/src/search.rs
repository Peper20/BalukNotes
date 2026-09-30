//! Поиск по тексту всех заметок — по индексу исходников ([`crate::outline`]),
//! без компиляции.
//!
//! Запрос — слова через пробел; раздел подходит, если каждое слово есть в
//! его тексте, заголовке или в названии/пути заметки (подстрокой, без учёта
//! регистра, «ё» = «е»). Вес: название > заголовок раздела > число вхождений в
//! тексте. Результат — раздел заметки (ссылка ведёт к нему) и фрагмент текста
//! с отмеченными совпадениями.
//!
//! Поиск в одной заметке ([`search_in`], «в этой книге» палитры) — все
//! подходящие разделы по порядку текста: браузерный Ctrl+F видит только
//! показанную главу книги.

use std::collections::HashSet;

use serde::Serialize;

use crate::graph::Snapshot;
use crate::outline::{Outline, Section};
use crate::render::{slug, unique};
use crate::vault::{NoteId, NoteKind};

/// Сколько разделов одной заметки показывать.
const PER_NOTE: usize = 3;
/// Длина фрагмента в символах.
const SNIPPET: usize = 160;
/// Сколько символов показать до первого совпадения.
const BEFORE: usize = 40;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SearchHit {
    pub id: NoteId,
    pub kind: NoteKind,
    /// Название заметки (или имя файла).
    pub title: String,
    /// Раздел, где нашлось; `None` — начало заметки.
    pub heading: Option<String>,
    /// Якорь раздела для ссылки (`#…`).
    pub anchor: Option<String>,
    pub snippet: Vec<Fragment>,
    pub score: u32,
}

/// Кусок фрагмента: совпадение или текст между совпадениями.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Fragment {
    pub text: String,
    pub hit: bool,
}

pub fn search(snap: &Snapshot, query: &str, limit: usize) -> Vec<SearchHit> {
    let mut hits = scoped(snap, query, None);
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);
    hits
}

/// Поиск в одной заметке (книге): все подходящие разделы по порядку текста.
pub fn search_in(snap: &Snapshot, id: &NoteId, query: &str, limit: usize) -> Vec<SearchHit> {
    let mut hits = scoped(snap, query, Some(id));
    hits.truncate(limit);
    hits
}

/// Разделы, где нашлись все слова: у каждой заметки — лучшие [`PER_NOTE`];
/// с `only` — только она, все разделы по порядку.
fn scoped(snap: &Snapshot, query: &str, only: Option<&NoteId>) -> Vec<SearchHit> {
    let words: Vec<Vec<char>> = query.split_whitespace().map(|w| w.chars().map(fold).collect()).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (entry, outline) in snap.outlines() {
        if only.is_some_and(|id| *id != entry.id) {
            continue;
        }
        let title = outline.title.clone().unwrap_or_else(|| entry.id.name().to_owned());
        let head: Vec<char> = format!("{title} {}", entry.id).chars().map(fold).collect();
        let mut own: Vec<SearchHit> = Vec::new();
        for (section, anchor) in outline.sections.iter().zip(section_ids(outline)) {
            if let Some(mut hit) = match_section(entry.id.clone(), entry.kind, &title, &head, section, &words) {
                hit.anchor = anchor;
                own.push(hit);
            }
        }
        // Совпало только название — одна строка на заметку, а не по разделу.
        if own.iter().all(|h| h.snippet.iter().all(|f| !f.hit)) {
            own.truncate(1);
        }
        if only.is_none() {
            own.sort_by_key(|h| std::cmp::Reverse(h.score));
            own.truncate(PER_NOTE);
        }
        hits.extend(own);
    }
    hits
}

fn match_section(
    id: NoteId,
    kind: NoteKind,
    title: &str,
    head: &[char],
    section: &Section,
    words: &[Vec<char>],
) -> Option<SearchHit> {
    let heading: Vec<char> = section.heading.as_deref().unwrap_or("").chars().map(fold).collect();
    let original: Vec<char> = section.text.chars().collect();
    let text: Vec<char> = original.iter().copied().map(fold).collect();
    let mut score = 0u32;
    for w in words {
        let (in_head, in_heading, in_text) = (count(head, w) > 0, count(&heading, w) > 0, count(&text, w));
        if !in_head && !in_heading && in_text == 0 {
            return None;
        }
        score += u32::from(in_head) * 20 + u32::from(in_heading) * 8 + u32::try_from(in_text.min(5)).unwrap_or(5);
    }
    Some(SearchHit {
        id,
        kind,
        title: title.to_owned(),
        anchor: None,
        heading: section.heading.clone(),
        snippet: snippet(&original, &text, words),
        score,
    })
}

/// Превью заметки для подсказки при наведении на ссылку.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Preview {
    pub id: NoteId,
    pub kind: NoteKind,
    pub title: String,
    /// Раздел, на который ведёт ссылка (если якорь нашёлся).
    pub heading: Option<String>,
    /// Начало текста раздела (или заметки), до `PREVIEW` символов.
    pub text: String,
    pub tags: Vec<String>,
}

/// Длина текста превью в символах.
const PREVIEW: usize = 420;

/// Превью заметки `id`; якорь — `id` раздела, его метка или текст заголовка.
pub fn preview(snap: &Snapshot, id: &NoteId, anchor: Option<&str>) -> Option<Preview> {
    let (entry, outline) = snap.outlines().find(|(e, _)| e.id == *id)?;
    let at = anchor.and_then(|a| section_at(outline, a)).map(|(i, _)| i);
    // Без якоря — начало заметки: первый раздел с текстом.
    let section = match at {
        Some(i) => &outline.sections[i],
        None => outline.sections.iter().find(|s| !s.text.is_empty())?,
    };
    let mut text: String = section.text.chars().take(PREVIEW).collect();
    if section.text.chars().count() > PREVIEW {
        if let Some(cut) = text.rfind(' ') {
            text.truncate(cut);
        }
        text.push('…');
    }
    Some(Preview {
        id: entry.id.clone(),
        kind: entry.kind,
        title: outline.title.clone().unwrap_or_else(|| entry.id.name().to_owned()),
        heading: at.and(section.heading.clone()),
        text,
        tags: outline.tags.clone(),
    })
}

/// Глава книги со своими тегами — для списка заметок и страницы тегов.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TaggedChapter {
    pub title: String,
    /// `id` заголовка главы: ссылка `…/n/<книга>#<anchor>` открывает главу.
    pub anchor: String,
    /// Свои теги главы; теги корня книги (`Outline::tags`) она наследует.
    pub tags: Vec<String>,
}

/// Главы со своими тегами (`chapter.with(tags: …)`), по порядку.
pub fn tagged_chapters(outline: &Outline) -> Vec<TaggedChapter> {
    outline
        .sections
        .iter()
        .zip(section_ids(outline))
        .filter(|(s, _)| !s.tags.is_empty())
        .filter_map(|(s, anchor)| {
            Some(TaggedChapter { title: s.heading.clone()?, anchor: anchor?, tags: s.tags.clone() })
        })
        .collect()
}

/// Раздел по якорю ссылки (`#see(…, anchor: …)`): `id` раздела (метка) или
/// текст заголовка — как находит ссылка на странице. Номер раздела и его `id`.
pub fn section_at(outline: &Outline, anchor: &str) -> Option<(usize, String)> {
    let wanted = slug(anchor);
    outline.sections.iter().zip(section_ids(outline)).enumerate().find_map(|(i, (s, sid))| {
        let sid = sid?;
        (sid == anchor || s.heading.as_deref().map(slug).as_deref() == Some(wanted.as_str())).then_some((i, sid))
    })
}

/// `id` разделов — по тем же правилам, что у отрисовки (`render.rs`): метка,
/// иначе слаг текста с `-2`, `-3` у повторов. Слаг одинаковых заголовков
/// («Итоги» в каждой главе) привёл бы к первому из них.
fn section_ids(outline: &Outline) -> Vec<Option<String>> {
    let mut used = HashSet::new();
    outline
        .sections
        .iter()
        .map(|s| {
            let heading = s.heading.as_deref()?;
            Some(s.label.clone().unwrap_or_else(|| unique(&slug(heading), &mut used)))
        })
        .collect()
}

/// Сравнение без регистра и «ё»; один символ → один символ, чтобы позиции
/// в свёрнутом тексте совпадали с исходным.
fn fold(c: char) -> char {
    let l = c.to_lowercase().next().unwrap_or(c);
    if l == 'ё' { 'е' } else { l }
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

fn count(hay: &[char], needle: &[char]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while let Some(at) = find(hay, needle, i) {
        n += 1;
        i = at + needle.len();
    }
    n
}

/// Окно вокруг первого совпадения, по границам слов, с отмеченными словами.
fn snippet(original: &[char], text: &[char], words: &[Vec<char>]) -> Vec<Fragment> {
    let first = words.iter().filter_map(|w| find(text, w, 0)).min();
    let len = original.len();
    let mut start = first.map_or(0, |p| p.saturating_sub(BEFORE));
    if start > 0 {
        start = (start..len).find(|&i| original[i - 1] == ' ').unwrap_or(start);
    }
    let mut end = (start + SNIPPET).min(len);
    if end < len {
        end = (start..end).rev().find(|&i| original[i] == ' ').filter(|&i| i > start).unwrap_or(end);
    }
    // Отметки совпадений в окне: [начало, конец) с объединением пересечений.
    let mut marks: Vec<(usize, usize)> = Vec::new();
    for w in words {
        let mut i = start;
        while let Some(at) = find(&text[..end], w, i) {
            marks.push((at, at + w.len()));
            i = at + w.len();
        }
    }
    marks.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (a, b) in marks {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let mut out = Vec::new();
    let mut push = |text: String, hit: bool| {
        if !text.is_empty() {
            out.push(Fragment { text, hit });
        }
    };
    let mut pos = start;
    let prefix = if start > 0 { "…" } else { "" };
    let mut plain: String = prefix.to_owned();
    for (a, b) in merged {
        plain.extend(&original[pos..a]);
        push(std::mem::take(&mut plain), false);
        push(original[a..b].iter().collect(), true);
        pos = b;
    }
    plain.extend(&original[pos..end]);
    if end < len {
        plain.push('…');
    }
    push(plain, false);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag(s: &[Fragment]) -> String {
        s.iter().map(|f| if f.hit { format!("[{}]", f.text) } else { f.text.clone() }).collect()
    }

    #[test]
    fn snippet_marks_words_case_and_yo_insensitive() {
        let original: Vec<char> = "Ёжик ищет ёлку. Потом ЕЖИК спит.".chars().collect();
        let text: Vec<char> = original.iter().copied().map(fold).collect();
        let words = vec!["ежик".chars().collect::<Vec<_>>()];
        assert_eq!(frag(&snippet(&original, &text, &words)), "[Ёжик] ищет ёлку. Потом [ЕЖИК] спит.");
    }

    #[test]
    fn long_text_is_cut_around_first_hit() {
        let original: Vec<char> = format!("{} искомое {}", "слово ".repeat(40), "хвост ".repeat(60)).chars().collect();
        let text: Vec<char> = original.iter().copied().map(fold).collect();
        let s = frag(&snippet(&original, &text, &[("искомое".chars().collect())]));
        assert!(s.starts_with('…') && s.ends_with('…'), "{s}");
        assert!(s.contains("[искомое]"));
        assert!(s.chars().count() <= SNIPPET + 4, "{}", s.chars().count());
    }
}
