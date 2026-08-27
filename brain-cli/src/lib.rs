use std::path::PathBuf;

use anyhow::{Context, Result};
use brain_core::error::BrainError;
use brain_core::index::{self, Hit, Index};
use brain_core::note::{self, Note, NoteKind, NoteMeta};
use brain_core::retrieve;
use brain_core::store::Store;
use chrono::Local;

pub mod cli;

pub use cli::{Cli, Command, FindArgs, SnapArgs};

/// Resolve config + store, falling back to defaults.
pub fn load_store(cfg_path: Option<&PathBuf>) -> Result<Store> {
    Store::open(cfg_path.map(|p| p.as_path())).context("failed to initialize note store")
}

fn load_index(store: &Store) -> Result<Index> {
    let idx_dir = store.index_dir()?;
    index::load_index(&idx_dir)?
        .ok_or_else(|| anyhow::anyhow!("no index found; run `brain index` first"))
}

pub struct NoteResult {
    pub path: PathBuf,
    pub body: String,
}

/// Core `snap` command: capture a note of a given kind.
pub fn snap(
    store: &Store,
    kind: NoteKind,
    title: Option<String>,
    tags: Vec<String>,
    body_text: Option<String>,
    extra: std::collections::BTreeMap<String, String>,
) -> Result<NoteResult> {
    let now = Local::now();
    let meta = NoteMeta {
        kind,
        created: now,
        tags,
        title: title.clone(),
        extra,
    };

    // For Link kind, default title handling happens in caller; keep as-is.

    let dir = store.kind_dir(kind);
    let fname = note::filename_for(kind, &now, title.as_deref());

    // Journal: merge into the day's journal file if it already exists.
    if kind == NoteKind::Journal {
        let path = dir.join(fname);
        let existing_body = if path.exists() {
            let n = note::read_note(&path)?;
            n.body
        } else {
            String::new()
        };
        let body = body_text.unwrap_or_default();
        let mut combined = existing_body;
        if combined.trim().is_empty() {
            combined = body;
        } else {
            combined.push_str(&format!("\n\n## {}\n\n{}", now.format("%H:%M"), body));
        }
        let note = Note {
            path: path.clone(),
            meta,
            body: combined,
        };
        note::write_note(&note)?;
        return Ok(NoteResult {
            path: note.path.clone(),
            body: note.body.clone(),
        });
    }

    let path = dir.join(fname);
    let note = Note {
        path,
        meta,
        body: body_text.unwrap_or_default(),
    };
    note::write_note(&note)?;
    Ok(NoteResult {
        path: note.path,
        body: note.body,
    })
}

/// Open a note in the configured editor; create it first if needed.
pub fn edit(store: &Store, path: &PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(anyhow::anyhow!("note not found: {}", path.display()));
    }
    let editor = store.cfg.editor();
    let status = std::process::Command::new(&editor)
        .arg(path)
        .status()
        .context(format!("failed to launch editor {editor}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!("editor exited with failure"));
    }
    Ok(())
}

/// Run the keyword search over the index.
pub fn find(store: &Store, query: &str) -> Result<Vec<Hit>> {
    let idx = load_index(store)?;
    index::search(&idx, query).context("search failed")
}

/// Nexus-style curated retrieval.
pub fn ask(store: &Store, query: &str) -> Result<retrieve::Answer> {
    let idx = load_index(store)?;
    retrieve::ask(store, &idx, query).map_err(|e| match e {
        BrainError::Config(m) => anyhow::anyhow!(m),
        other => anyhow::anyhow!(other),
    })
}

/// Rebuild the keyword index from the corpus.
pub fn index_cmd(store: &Store) -> Result<()> {
    let idx = index::build_index(store)?;
    let count = idx.meta.note_count;
    index::save_index(&idx, &store.index_dir()?)?;
    println!("indexed {count} note(s)");
    Ok(())
}

/// Print the daily view.
pub fn daily(store: &Store) -> Result<String> {
    let idx_dir = store.index_dir().unwrap_or_default();
    let idx = index::load_index(&idx_dir)?;
    let today = Local::now().format("%Y-%m-%d").to_string();

    let mut out = String::new();
    out.push_str(&format!("{}·{}\n\n", "brain", today));

    // Today's notes: filter docs whose path contains today's date, or whose path is today's journal.
    let today_notes = idx
        .as_ref()
        .map(|i| {
            i.docs
                .iter()
                .filter(|d| {
                    d.path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default()
                        .contains(&today)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if today_notes.is_empty() {
        out.push_str("no notes yet today — `brain snap \"...\"` to start\n");
    } else {
        out.push_str("TODAY:\n");
        for d in today_notes {
            out.push_str(&format!("  {} ({})\n", d.title, d.path.display()));
        }
    }

    // Recent captures: last 5 by created desc (top of list since walk order is arbitrary, sort)
    out.push_str("\nRECENT:\n");
    let mut recent = idx.as_ref().map(|i| i.docs.clone()).unwrap_or_default();
    recent.sort_by(|a, b| b.created.cmp(&a.created));
    for d in recent.iter().take(5) {
        out.push_str(&format!("  {} — {}\n", d.created, d.title));
    }
    Ok(out)
}

/// Capture session/context text (appended from coding sessions) into a session note.
pub fn session(store: &Store, text: &str) -> Result<PathBuf> {
    let now = Local::now();
    let dir = store.kind_dir(NoteKind::Session);
    let fname = note::filename_for(NoteKind::Session, &now, None);
    let path = dir.join(fname);
    let meta = NoteMeta {
        kind: NoteKind::Session,
        created: now,
        tags: vec!["session".into()],
        title: None,
        extra: Default::default(),
    };
    // append to existing session file if it already exists (same minute unlikely)
    let body = if path.exists() {
        let n = note::read_note(&path)?;
        format!("{}\n\n{}", n.body, text)
    } else {
        text.to_string()
    };
    let note = Note { path, meta, body };
    note::write_note(&note)?;
    Ok(note.path)
}
