//! Integration tests that drive the real `brain` binary (via CARGO_BIN_EXE_brain)
//! against a temporary config + notes directory.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_brain")
}

/// Write a temp config pointing notes_dir at a fresh temp dir; return (config, notes_dir).
fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let td = tempfile::tempdir().unwrap();
    let notes_dir = td.path().join("notes");
    let config = td.path().join("config.toml");
    std::fs::write(
        &config,
        format!("notes_dir = {:?}\n", notes_dir.to_string_lossy()),
    )
    .unwrap();
    (td, notes_dir, config)
}

fn run(config: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(["--config"])
        .arg(config)
        .args(args)
        .output()
        .expect("brain binary runs")
}

fn assert_ok(out: &std::process::Output) -> String {
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn snap_capture_writes_markdown_note() {
    let (_td, notes, config) = setup();
    let out = assert_ok(&run(
        &config,
        &["snap", "water the rosemary", "-t", "home,plant"],
    ));
    let path = out.trim();
    assert!(path.ends_with(".md"), "prints note path, got: {path}");

    let body = std::fs::read_to_string(path).unwrap();
    assert!(body.contains("water the rosemary"), "body persisted");
    assert!(body.contains("kind: capture"), "has capture front matter");
    assert!(body.contains("home"), "tags persisted");
    assert!(Path::new(path).starts_with(notes.join("capture")));
}

#[test]
fn snap_dev_and_link_kinds() {
    let (_td, notes, config) = setup();

    let dev = assert_ok(&run(
        &config,
        &[
            "snap",
            "--kind",
            "dev",
            "-T",
            "rust",
            "learn ownership",
            "-t",
            "rust",
        ],
    ));
    assert!(Path::new(dev.trim()).starts_with(notes.join("dev")));

    let link = assert_ok(&run(
        &config,
        &[
            "snap",
            "--kind",
            "link",
            "-T",
            "book",
            "--url",
            "https://doc.rust-lang.org/book",
        ],
    ));
    assert!(Path::new(link.trim()).starts_with(notes.join("link")));
    let link_body = std::fs::read_to_string(link.trim()).unwrap();
    assert!(
        link_body.contains("https://doc.rust-lang.org/book"),
        "link stores url"
    );
}

#[test]
fn index_then_find_and_ask() {
    let (_td, _notes, config) = setup();
    assert_ok(&run(
        &config,
        &[
            "snap",
            "--kind",
            "dev",
            "-T",
            "rust",
            "rust ownership and borrowing",
        ],
    ));
    assert_ok(&run(
        &config,
        &["snap", "cloudflare workers deployment decision"],
    ));

    assert_ok(&run(&config, &["index"]));

    let find = assert_ok(&run(&config, &["find", "ownership"]));
    assert!(find.contains("ownership"), "find returns match: {find}");

    let ask = assert_ok(&run(&config, &["ask", "cloudflare deployment"]));
    assert!(
        ask.contains("cloudflare workers"),
        "ask cites content: {ask}"
    );
    assert!(ask.contains(".md:"), "ask includes file:line citation");
}

#[test]
fn session_then_find() {
    let (_td, notes, config) = setup();
    let out = assert_ok(&run(
        &config,
        &["session", "chat: decided to use Cloudflare Workers"],
    ));
    assert!(Path::new(out.trim()).starts_with(notes.join("session")));

    assert_ok(&run(&config, &["index"]));
    let find = assert_ok(&run(&config, &["find", "cloudflare"]));
    assert!(
        find.contains("session"),
        "session note indexed & searchable: {find}"
    );
}

#[test]
fn snap_missing_term_find_no_results() {
    let (_td, _notes, config) = setup();
    assert_ok(&run(&config, &["snap", "hello world"]));
    assert_ok(&run(&config, &["index"]));
    let find = assert_ok(&run(&config, &["find", "zzzznothing"]));
    assert!(find.contains("no results"), "graceful empty result: {find}");
}

#[test]
fn daily_lists_today() {
    let (_td, _notes, config) = setup();
    assert_ok(&run(&config, &["snap", "todays note"]));
    assert_ok(&run(&config, &["index"]));
    let daily = assert_ok(&run(&config, &["daily"]));
    assert!(daily.contains("TODAY"), "daily view has section: {daily}");
}

#[test]
fn journal_appends_into_single_daily_file() {
    let (_td, notes, config) = setup();
    let first = assert_ok(&run(&config, &["snap", "--kind", "journal", "alpha entry"]));
    let second = assert_ok(&run(&config, &["snap", "--kind", "journal", "beta entry"]));

    assert_eq!(
        first.trim(),
        second.trim(),
        "both entries share one daily journal file"
    );

    let raw = std::fs::read_to_string(first.trim()).unwrap();
    assert!(raw.contains("alpha entry"), "first entry preserved: {raw}");
    assert!(raw.contains("beta entry"), "second entry appended: {raw}");

    // journal lives at the notes root, not a subdir
    assert_eq!(Path::new(first.trim()).parent().unwrap(), notes.as_path());
}
