use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::note::{Note, NoteKind};
use crate::store::Store;

pub const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexMeta {
    pub version: u32,
    pub built: String,
    pub note_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub path: PathBuf,
    pub title: String,
    pub kind: String,
    pub created: String,
    pub size: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Index {
    pub meta: IndexMeta,
    /// lowercased term -> (doc idx, term frequency)
    pub terms: HashMap<String, Vec<(usize, u32)>>,
    pub docs: Vec<IndexEntry>,
    /// content store: index -> full lowercased text (for snippet reconstruction)
    pub text: Vec<String>,
}

/// A scored search hit.
#[derive(Debug, Clone)]
pub struct Hit {
    pub entry: IndexEntry,
    pub score: f32,
    pub snippet: String,
    pub matched_terms: Vec<String>,
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

pub fn build_index(store: &Store) -> Result<Index> {
    let files = store.walk()?;
    let mut idx = Index::default();
    idx.meta.version = INDEX_VERSION;
    idx.meta.built = Local::now().to_rfc3339();

    for path in &files {
        let note = match crate::note::read_note(path) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let combined = format!(
            "{} {} {:?} {} {}",
            note.body,
            note.meta.title.as_deref().unwrap_or(""),
            note.meta.tags,
            note.meta.kind.as_str(),
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        );
        let text = combined.to_lowercase();
        let doc_idx = idx.docs.len();
        idx.docs.push(IndexEntry {
            path: path.clone(),
            title: note.meta.title.clone().unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            }),
            kind: note.meta.kind.as_str().to_string(),
            created: note.meta.created.to_rfc3339(),
            size: note.body.len(),
        });
        idx.text.push(text.clone());

        let mut freqs: HashMap<String, u32> = HashMap::new();
        for term in tokenize(&combined) {
            *freqs.entry(term).or_insert(0) += 1;
        }
        for (term, tf) in freqs {
            idx.terms.entry(term).or_default().push((doc_idx, tf));
        }
    }
    idx.meta.note_count = idx.docs.len();
    Ok(idx)
}

pub fn save_index(index: &Index, index_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(index_dir)?;
    let path = index_dir.join("index.json");
    let bytes = serde_json::to_vec_pretty(index)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn load_index(index_dir: &Path) -> Result<Option<Index>> {
    let path = index_dir.join("index.json");
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let idx: Index = serde_json::from_str(&raw)?;
    if idx.meta.version != INDEX_VERSION {
        return Ok(None);
    }
    Ok(Some(idx))
}

/// Search the index with preference for term-frequency, plus filename boosts.
pub fn search(index: &Index, query: &str) -> Result<Vec<Hit>> {
    let terms = tokenize(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    // gather candidate doc indices with scores
    let mut scores: HashMap<usize, f32> = HashMap::new();
    let mut matched: HashMap<usize, Vec<String>> = HashMap::new();
    let doc_len = index.docs.len();
    for term in &terms {
        if let Some(postings) = index.terms.get(term) {
            let df = postings.len().max(1) as f32;
            let idf = ((doc_len as f32) / df).ln().max(0.1);
            for (doc, tf) in postings {
                let boost = if index.docs[*doc]
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_lowercase().contains(term))
                    .unwrap_or(false)
                {
                    2.0
                } else {
                    1.0
                };
                let score = (*tf as f32) * idf * boost;
                *scores.entry(*doc).or_insert(0.0) += score;
                matched.entry(*doc).or_default().push(term.clone());
            }
        }
    }

    let mut scored: Vec<(usize, f32)> = scores.into_iter().collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut hits = Vec::new();
    for (doc, score) in scored {
        let entry = index.docs[doc].clone();
        let matched_terms = {
            let mut v = matched.remove(&doc).unwrap_or_default();
            v.sort();
            v.dedup();
            v
        };
        let snippet = make_snippet(&index.text[doc], &matched_terms);
        hits.push(Hit {
            entry,
            score,
            snippet,
            matched_terms,
        });
    }
    Ok(hits)
}

fn make_snippet(text: &str, terms: &[String]) -> String {
    let lower = text.to_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for term in terms {
        if let Some(pos) = lower.find(term) {
            let start = pos.saturating_sub(40);
            let end = (pos + term.len() + 60).min(lower.len());
            let len = end - start;
            match best {
                Some((_, bl)) if bl >= len => {}
                _ => best = Some((start, len)),
            }
        }
    }
    match best {
        Some((start, len)) => {
            let mut s = text.chars().skip(start).take(len).collect::<String>();
            if start > 0 {
                s.insert(0, '…');
            }
            let end = start + len;
            if end < text.len() {
                s.push('…');
            }
            s
        }
        None => text.chars().take(120).collect(),
    }
}

/// Segment a single note into retrievable chunks split on blank lines/headings.
pub fn chunk_note(note: &Note) -> Vec<crate::Chunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_line = 1usize;
    for (i, line) in note.body.lines().enumerate() {
        let lineno = i + 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            if !current.trim().is_empty() {
                chunks.push(crate::Chunk {
                    source: note.path.clone(),
                    line: current_line,
                    text: current.trim().to_string(),
                    title: note.meta.title.clone(),
                });
            }
            current = String::new();
            current_line = lineno + 1;
            if !line.trim().is_empty() {
                // a heading starts a new chunk including it
                current_line = lineno;
                current.push_str(line);
                current.push('\n');
            }
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        chunks.push(crate::Chunk {
            source: note.path.clone(),
            line: current_line,
            text: current.trim().to_string(),
            title: note.meta.title.clone(),
        });
    }
    if chunks.is_empty() {
        chunks.push(crate::Chunk {
            source: note.path.clone(),
            line: 1,
            text: "".to_string(),
            title: note.meta.title.clone(),
        });
    }
    chunks
}

/// Quick offset-based helper used by retrieval to find a line for a chunk.
pub fn line_of(substring: &str, text: &str) -> Option<usize> {
    text.find(substring)
        .map(|pos| 1 + text[..pos].matches('\n').count())
}

pub fn note_kind(path: &Path) -> NoteKind {
    let parent = path.parent().map(|p| p.to_string_lossy().to_string());
    match parent.as_deref() {
        Some(p) if p.ends_with("dev") => NoteKind::Dev,
        Some(p) if p.ends_with("link") => NoteKind::Link,
        Some(p) if p.ends_with("session") => NoteKind::Session,
        Some(p) if p.ends_with("capture") => NoteKind::Capture,
        _ => NoteKind::Journal,
    }
}

/// Track a set of known file paths so the CLI can tell the user the index may be stale.
pub fn known_paths(index: &Index) -> HashSet<PathBuf> {
    index.docs.iter().map(|d| d.path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_and_searches() {
        let mut idx = Index::default();
        idx.meta.version = INDEX_VERSION;
        idx.docs.push(IndexEntry {
            path: PathBuf::from("/tmp/rust-notes.md"),
            title: "rust".into(),
            kind: "dev".into(),
            created: "".into(),
            size: 0,
        });
        idx.text.push("learn rust ownership and borrow".to_string());
        idx.terms.insert("rust".into(), vec![(0, 2)]);
        idx.terms.insert("borrow".into(), vec![(0, 1)]);
        let hits = search(&idx, "rust borrow").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.path, PathBuf::from("/tmp/rust-notes.md"));
    }

    #[test]
    fn chunks_split_on_blank_and_headings() {
        let body = "line one\nline two\n\n## heading\nline three".to_string();
        let note = Note {
            path: PathBuf::from("/tmp/x.md"),
            meta: Default::default(),
            body,
        };
        let chunks = chunk_note(&note);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn build_and_round_trip_index() {
        use crate::store::{Config, Store};
        use std::fs;

        let td = tempfile::tempdir().unwrap();
        fs::create_dir(td.path().join("dev")).unwrap();
        fs::write(
            td.path().join("dev/note.md"),
            "---\nkind: dev\ntitle: \"alpha\"\ntags: [\"rust\"]\n---\nrust notes about ownership\n",
        )
        .unwrap();
        let cfg = Config {
            notes_dir: Some(td.path().to_path_buf()),
            ..Default::default()
        };
        let store = Store::new(cfg).unwrap();
        let idx = build_index(&store).unwrap();
        assert_eq!(idx.meta.note_count, 1);

        let idx_dir = store.index_dir().unwrap();
        save_index(&idx, &idx_dir).unwrap();
        let loaded = load_index(&idx_dir).unwrap().unwrap();
        assert_eq!(loaded.meta.note_count, 1);

        let hits = search(&loaded, "ownership").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("ownership"));
    }
}
