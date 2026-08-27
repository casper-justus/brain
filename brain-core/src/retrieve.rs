use std::collections::HashSet;
use std::path::PathBuf;

use crate::error::{BrainError, Result};
use crate::index::{self, Index};
use crate::note::read_note;
use crate::store::Store;
use crate::Chunk;

/// A curated retrieval result with file:line provenance.
#[derive(Debug, Clone)]
pub struct Retrieved {
    pub citation: String,
    pub source: PathBuf,
    pub line: usize,
    pub excerpt: String,
}

/// A synthesized answer composed of cited chunks.
#[derive(Debug, Clone)]
pub struct Answer {
    pub query: String,
    pub chunks: Vec<Retrieved>,
    /// Short synthesis text (deterministic, template-driven for now).
    pub synthesis: String,
}

/// Nexus-style curation: segment the whole corpus into retrievable chunks.
pub fn curate(store: &Store) -> Result<Vec<Chunk>> {
    let files = store.walk()?;
    let mut chunks = Vec::new();
    for path in &files {
        if let Ok(note) = read_note(path) {
            chunks.extend(index::chunk_note(&note));
        }
    }
    Ok(chunks)
}

/// Rank chunks against a query using token overlap (TF-IDF-inspired).
/// Produces deterministic, provenance-anchored results without an LLM.
pub fn ask(store: &Store, index: &Index, query: &str) -> Result<Answer> {
    let _ = index; // index currently advisory; curate re-walks the corpus each call
    let chunks = curate(store)?;
    if chunks.is_empty() {
        return Err(BrainError::Config(
            "no notes found to search; capture something first".into(),
        ));
    }

    let q_terms = tokenize(query);
    if q_terms.is_empty() {
        return Err(BrainError::Config("empty query".into()));
    }

    // term frequency across the corpus for idf
    let total = chunks.len() as f32;
    let mut df: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut chunk_vec: Vec<Vec<String>> = Vec::new();
    for c in &chunks {
        let terms = tokenize(&c.text);
        let uniq: HashSet<String> = terms.iter().cloned().collect();
        for t in &uniq {
            *df.entry(t.clone()).or_insert(0.0) += 1.0;
        }
        chunk_vec.push(terms);
    }

    let mut scored: Vec<(usize, f32)> = Vec::new();
    for (i, terms) in chunk_vec.iter().enumerate() {
        let mut score = 0.0f32;
        for t in &q_terms {
            let idf = (total / df.get(t).copied().unwrap_or(1.0)).ln().max(0.0);
            let tf = terms.iter().filter(|x| *x == t).count() as f32;
            score += tf * idf;
        }
        if score > 0.0 {
            scored.push((i, score));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top: Vec<Chunk> = scored
        .into_iter()
        .take(5)
        .map(|(i, _)| chunks[i].clone())
        .collect();

    let retrieved: Vec<Retrieved> = top
        .iter()
        .map(|c| {
            let line = c.line.max(1);
            let citation = format!("{}:{}", c.source.display(), line);
            Retrieved {
                citation,
                source: c.source.clone(),
                line,
                excerpt: c.text.clone(),
            }
        })
        .collect();

    let synthesis = synthesize(query, &retrieved);

    Ok(Answer {
        query: query.to_string(),
        chunks: retrieved,
        synthesis,
    })
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

fn synthesize(query: &str, retrieved: &[Retrieved]) -> String {
    if retrieved.is_empty() {
        return format!(
            "No notes matched “{query}”. Try broader terms, or capture more to the corpus."
        );
    }
    let mut cited: Vec<String> = retrieved
        .iter()
        .map(|r| r.citation.clone())
        .collect();
    cited.dedup();
    format!(
        "Found {} relevant passage(s) for “{query}”: {}. {}",
        retrieved.len(),
        cited.join(", "),
        "These are the highest-scoring chunks in the corpus by keyword relevance; "
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use crate::store::{Config, Store};

    #[test]
    fn curate_finds_chunks() {
        let td = tempfile::tempdir().unwrap();
        fs::create_dir(td.path().join("dev")).unwrap();
        let note_path = td.path().join("dev/rust.md");
        fs::write(
            &note_path,
            "---\nkind: dev\ntitle: \"rust\"\n---\nlearn about rust ownership\n\nmore content\n",
        )
        .unwrap();
        let cfg = Config {
            notes_dir: Some(td.path().to_path_buf()),
            ..Default::default()
        };
        let store = Store::new(cfg).unwrap();
        let idx = index::build_index(&store).unwrap();
        let ans = ask(&store, &idx, "rust ownership").unwrap();
        assert!(!ans.chunks.is_empty());
        assert!(ans.synthesis.contains(".md:"));
    }
}
