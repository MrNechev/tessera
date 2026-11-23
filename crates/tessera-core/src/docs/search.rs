//! Full-text search across documents and "where is this file used".

use serde::Serialize;
use serde_json::Value;

use super::{DocError, MediaRef, Store};

/// One search result inside a document.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Hit {
    #[serde(rename = "docId")]
    pub doc_id: String,
    #[serde(rename = "docTitle")]
    pub doc_title: String,
    #[serde(rename = "sectionId", skip_serializing_if = "String::is_empty")]
    pub section_id: String,
    /// Index, -1 = the document itself (title/tags).
    pub section: i64,
    /// title | tag | section | text | caption | notes
    pub field: String,
    pub snippet: String,
}

/// A place where a media file is referenced.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Use {
    #[serde(rename = "docId")]
    pub doc_id: String,
    #[serde(rename = "docTitle")]
    pub doc_title: String,
    #[serde(rename = "sectionId", skip_serializing_if = "String::is_empty")]
    pub section_id: String,
    pub section: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// section | cover | music
    #[serde(rename = "where")]
    pub where_: String,
}

/// Flattens TipTap JSON to plain text.
pub fn plain_text(raw: Option<&Value>) -> String {
    let Some(v) = raw else { return String::new() };
    let mut b = String::new();
    fn walk(v: &Value, b: &mut String) {
        let Some(m) = v.as_object() else { return };
        if let Some(Value::String(t)) = m.get("text") {
            b.push_str(t);
        }
        if let Some(Value::Array(c)) = m.get("content") {
            for ch in c {
                walk(ch, b);
            }
            let typ = m.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if matches!(typ, "paragraph" | "heading" | "listItem" | "codeBlock") {
                b.push('\n');
            }
        }
    }
    walk(v, &mut b);
    b
}

fn rune_start(b: u8) -> bool {
    b & 0xC0 != 0x80
}

/// Context around the first match of `lower_q` in `text` ("" if none).
pub(crate) fn snippet(text: &str, lower_q: &str) -> String {
    let lt = text.to_lowercase();
    let Some(i) = lt.find(lower_q) else {
        return String::new();
    };
    let tb = text.as_bytes();
    if tb.is_empty() {
        return String::new();
    }
    // lowercasing can change byte lengths for some scripts; clamp to rune boundaries
    let mut start = i.saturating_sub(50);
    while start > 0 && !rune_start(tb[start.min(tb.len() - 1)]) {
        start -= 1;
    }
    let mut end = (i + lower_q.len() + 70).min(tb.len());
    while end < tb.len() && !rune_start(tb[end]) {
        end += 1;
    }
    let s0 = start.min(tb.len());
    if s0 > end {
        return String::new();
    }
    let part = String::from_utf8_lossy(&tb[s0..end]);
    let mut s = part.split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 {
        s = format!("…{s}");
    }
    if end < tb.len() {
        s.push('…');
    }
    s
}

impl Store {
    /// Finds a phrase in every document: titles, tags, section titles, text, captions and
    /// speaker notes (case-insensitive). At most `limit` hits.
    pub fn search(&self, q: &str, limit: usize) -> Result<Vec<Hit>, DocError> {
        let q = q.trim().to_lowercase();
        let mut out = Vec::new();
        if q.is_empty() {
            return Ok(out);
        }
        let metas = self.list()?;
        for m in metas {
            let Ok(d) = self.get(&m.id) else { continue };
            let add = |out: &mut Vec<Hit>, h: Hit| {
                out.push(h);
                out.len() >= limit
            };
            let sn = snippet(&d.title, &q);
            if !sn.is_empty()
                && add(
                    &mut out,
                    Hit {
                        doc_id: d.id.clone(),
                        doc_title: d.title.clone(),
                        section_id: String::new(),
                        section: -1,
                        field: "title".into(),
                        snippet: sn,
                    },
                )
            {
                return Ok(out);
            }
            for t in &d.tags {
                if t.to_lowercase().contains(&q)
                    && add(
                        &mut out,
                        Hit {
                            doc_id: d.id.clone(),
                            doc_title: d.title.clone(),
                            section_id: String::new(),
                            section: -1,
                            field: "tag".into(),
                            snippet: t.clone(),
                        },
                    )
                {
                    return Ok(out);
                }
            }
            for (i, sec) in d.sections.iter().enumerate() {
                let try_field = |out: &mut Vec<Hit>, field: &str, text: &str| -> bool {
                    let sn = snippet(text, &q);
                    if sn.is_empty() {
                        return false;
                    }
                    add(
                        out,
                        Hit {
                            doc_id: d.id.clone(),
                            doc_title: d.title.clone(),
                            section_id: sec.id.clone(),
                            section: i as i64,
                            field: field.into(),
                            snippet: sn,
                        },
                    )
                };
                if try_field(&mut out, "section", &sec.title)
                    || try_field(&mut out, "text", &plain_text(sec.text.as_ref()))
                    || try_field(&mut out, "notes", &sec.notes)
                {
                    return Ok(out);
                }
                for mr in &sec.media {
                    if try_field(&mut out, "caption", &mr.caption) {
                        return Ok(out);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Where a library file (or, with `dir`, anything below a folder) is used.
    pub fn usage(&self, lib: &str, p: &str, dir: bool) -> Result<Vec<Use>, DocError> {
        let matches = |r: Option<&MediaRef>| -> bool {
            let Some(r) = r else { return false };
            if r.library_id != lib {
                return false;
            }
            if dir {
                return p.is_empty() || r.path.starts_with(&format!("{p}/"));
            }
            r.path == p
        };
        let mut out = Vec::new();
        for m in self.list()? {
            let Ok(d) = self.get(&m.id) else { continue };
            let base = |section_id: String, section: i64, title: String, where_: &str| Use {
                doc_id: d.id.clone(),
                doc_title: d.title.clone(),
                section_id,
                section,
                title,
                where_: where_.into(),
            };
            if matches(d.cover.as_ref()) {
                out.push(base(String::new(), -1, String::new(), "cover"));
            }
            if let Some(st) = &d.settings {
                if let Some(raw) = st.extra.get("music") {
                    if let Ok(mr) = serde_json::from_value::<MediaRef>(raw.clone()) {
                        if matches(Some(&mr)) {
                            out.push(base(String::new(), -1, String::new(), "music"));
                        }
                    }
                }
            }
            for (i, sec) in d.sections.iter().enumerate() {
                if sec.media.iter().any(|mr| matches(Some(mr))) {
                    out.push(base(sec.id.clone(), i as i64, sec.title.clone(), "section"));
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_snippets() {
        let v: Value = serde_json::from_str(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello "},{"type":"text","text":"world"}]},{"type":"heading","content":[{"type":"text","text":"Next"}]}]}"#).unwrap();
        assert_eq!(plain_text(Some(&v)), "Hello world\nNext\n");
        assert_eq!(snippet("A short   title", "short"), "A short title");
        let long = format!("{}needle{}", "x ".repeat(60), " y".repeat(60));
        let s = snippet(&long, "needle");
        assert!(
            s.starts_with('…') && s.ends_with('…') && s.contains("needle"),
            "{s}"
        );
        assert_eq!(snippet("Смешни котки", "котки"), "Смешни котки");
        assert_eq!(snippet("abc", "zzz"), "");
    }
}
