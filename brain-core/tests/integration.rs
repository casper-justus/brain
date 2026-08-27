//! End-to-end integration tests exercising the real store, index, and retrieval
//! against a temporary notes directory — no mocks.

use brain_core::index::{self, Index};
use brain_core::note::{self, NoteKind, NoteMeta};
use brain_core::retrieve;
use brain_core::store::{Config, Store};
use std::path::PathBuf;

fn setup_store() -> (tempfile::TempDir, Store, PathBuf) {
    let td = tempfile::tempdir().unwrap();
    let root = td.path().to_path_buf();
    let cfg = Config {
        notes_dir: Some(root.clone()),
        ..Default::default()
    };
    let store = Store::new(cfg).unwrap();
    (td, store, root)
}

fn write_note(
    dir: &std::path::Path,
    fname: &str,
    kind: NoteKind,
    title: &str,
    body: &str,
) -> PathBuf {
    let path = dir.join(dir_of(kind)).join(fname);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let meta = NoteMeta {
        kind,
        title: Some(title.to_string()),
        created: chrono::Local::now(),
        ..Default::default()
    };
    let note = note::Note {
        path,
        meta,
        body: body.to_string(),
    };
    note::write_note(&note).unwrap();
    note.path.clone()
}

fn dir_of(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Capture => "capture",
        NoteKind::Dev => "dev",
        NoteKind::Link => "link",
        NoteKind::Session => "session",
        NoteKind::Journal => "",
    }
}

#[test]
fn full_pipeline_capture_index_search_curate() {
    let (_td, store, root) = setup_store();

    write_note(
        &root,
        "capture-1.md",
        NoteKind::Capture,
        "rosemary",
        "remember to water the rosemary plant",
    );
    write_note(
        &root,
        "dev-1.md",
        NoteKind::Dev,
        "rust notes",
        "learn about rust ownership and borrowing",
    );
    write_note(
        &root,
        "link-1.md",
        NoteKind::Link,
        "rust book",
        "https://doc.rust-lang.org/book",
    );
    write_note(
        &root,
        "session-1.md",
        NoteKind::Session,
        "deployment",
        "decided to use Cloudflare Workers for deployment",
    );

    // index the whole corpus
    let idx: Index = index::build_index(&store).unwrap();
    assert_eq!(idx.meta.note_count, 4, "all four notes indexed");

    // persist + reload
    let index_dir = store.index_dir().unwrap();
    index::save_index(&idx, &index_dir).unwrap();
    let loaded = index::load_index(&index_dir)
        .unwrap()
        .expect("index reloads");
    assert_eq!(loaded.meta.note_count, 4);

    // keyword search finds the dev note
    let hits = index::search(&loaded, "ownership").unwrap();
    let dev = hits.iter().find(|h| h.entry.title == "rust notes");
    assert!(dev.is_some(), "dev note found by keyword search");
    assert!(dev.unwrap().snippet.contains("ownership"));

    // curated retrieval returns a cited chunk
    let ans = retrieve::ask(&store, &loaded, "cloudflare deployment").unwrap();
    assert!(!ans.chunks.is_empty(), "ask returns chunks");
    let session_cited = ans
        .chunks
        .iter()
        .any(|c| c.excerpt.contains("Cloudflare Workers"));
    assert!(session_cited, "ask surfaces the session note");
    assert!(
        ans.synthesis.contains(".md:"),
        "synthesis carries file:line citation"
    );
}

#[test]
fn note_round_trips_via_write_and_read() {
    let (_td, _store, root) = setup_store();
    let journal_dir = root.join(dir_of(NoteKind::Journal));
    let path = journal_dir.join("journal-2026-08-27.md");

    let meta = NoteMeta {
        kind: NoteKind::Journal,
        ..Default::default()
    };
    note::write_note(&note::Note {
        path: path.clone(),
        meta,
        body: "my daily entry body".to_string(),
    })
    .unwrap();

    let back = note::read_note(&path).unwrap();
    assert_eq!(back.meta.kind, NoteKind::Journal);
    assert_eq!(back.body.trim(), "my daily entry body");
}

#[test]
fn search_no_hits_returns_empty() {
    let (_td, store, root) = setup_store();
    write_note(
        &root,
        "a.md",
        NoteKind::Capture,
        "alpha",
        "something about photosynthesis",
    );
    let idx = index::build_index(&store).unwrap();
    let hits = index::search(&idx, "zzzznothing").unwrap();
    assert!(hits.is_empty());
}

#[test]
fn empty_corpus_curate_reports_no_notes() {
    let (_td, store, _root) = setup_store();
    let idx = index::build_index(&store).unwrap();
    assert_eq!(idx.meta.note_count, 0);
    let err = retrieve::ask(&store, &idx, "anything").err();
    assert!(err.is_some(), "ask on empty corpus errors gracefully");
}
