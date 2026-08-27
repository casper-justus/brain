use chrono::{DateTime, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{BrainError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteKind {
    Capture,
    Journal,
    Dev,
    Link,
    Session,
}

impl NoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NoteKind::Capture => "capture",
            NoteKind::Journal => "journal",
            NoteKind::Dev => "dev",
            NoteKind::Link => "link",
            NoteKind::Session => "session",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "capture" | "snap" => Ok(NoteKind::Capture),
            "journal" | "daily" => Ok(NoteKind::Journal),
            "dev" | "technical" => Ok(NoteKind::Dev),
            "link" | "bookmark" => Ok(NoteKind::Link),
            "session" | "context" => Ok(NoteKind::Session),
            other => Err(BrainError::Config(format!("unknown note kind: {other}"))),
        }
    }
}

/// Front matter + body for every note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMeta {
    pub kind: NoteKind,
    pub created: DateTime<Local>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default, flatten)]
    pub extra: std::collections::BTreeMap<String, String>,
}

impl Default for NoteMeta {
    fn default() -> Self {
        Self {
            kind: NoteKind::Capture,
            created: Local::now(),
            tags: Vec::new(),
            title: None,
            extra: Default::default(),
        }
    }
}

/// A fully materialized note read from disk.
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub meta: NoteMeta,
    pub body: String,
}

/// Render meta into a YAML-ish front matter block (kept dependency-light, hand-rolled).
pub fn render_front_matter(meta: &NoteMeta) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("kind: {}\n", meta.kind.as_str()));
    out.push_str(&format!("created: {}\n", meta.created.to_rfc3339()));
    if !meta.tags.is_empty() {
        out.push_str(&format!(
            "tags: [{}]\n",
            meta.tags
                .iter()
                .map(|t| format!("{t:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(t) = &meta.title {
        out.push_str(&format!("title: {t:?}\n"));
    }
    for (k, v) in &meta.extra {
        out.push_str(&format!("{k}: {v:?}\n"));
    }
    out.push_str("---\n");
    out
}

/// Parse the front matter (if present, delimited by leading `---`) from raw text.
fn parse_front_matter(raw: &str) -> (NoteMeta, String) {
    let mut meta = NoteMeta::default();
    let rest = if let Some(stripped) = raw.strip_prefix("---\n") {
        match stripped.find("\n---") {
            Some(end) => {
                let fm = &stripped[..end];
                let body = &stripped[end + 4..];
                for line in fm.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        let k = k.trim();
                        let v = v.trim().trim_matches('"').trim_matches('"');
                        match k {
                            "kind" => {
                                if let Ok(kind) = NoteKind::parse(v) {
                                    meta.kind = kind;
                                }
                            }
                            "created" => {
                                if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
                                    meta.created = dt.with_timezone(&Local);
                                }
                            }
                            "tags" => {
                                let clean = v.trim_start_matches('[').trim_end_matches(']');
                                meta.tags = clean
                                    .split(',')
                                    .map(|t| {
                                        t.trim().trim_matches('"').trim_matches('"').to_string()
                                    })
                                    .filter(|t| !t.is_empty())
                                    .collect();
                            }
                            "title" => meta.title = Some(v.to_string()),
                            _ => {
                                meta.extra.insert(k.to_string(), v.to_string());
                            }
                        }
                    }
                }
                body.to_string()
            }
            None => stripped.to_string(),
        }
    } else {
        raw.to_string()
    };
    (meta, rest)
}

/// Build a filename for a newly captured note.
pub fn filename_for(kind: NoteKind, created: &DateTime<Local>, title: Option<&str>) -> String {
    let date = created.format("%Y-%m-%d").to_string();
    let base = match (kind, title) {
        (NoteKind::Journal, _) => format!("journal-{date}"),
        (_, Some(t)) => {
            let slug = slugify(t);
            format!("{date}-{slug}")
        }
        (NoteKind::Link, _) => format!(
            "link-{date}-{:02}{:02}{:02}",
            created.hour(),
            created.minute(),
            created.second()
        ),
        (NoteKind::Session, _) => format!(
            "session-{date}-{:02}{:02}{:02}",
            created.hour(),
            created.minute(),
            created.second()
        ),
        _ => format!(
            "{date}-{:02}{:02}{:02}",
            created.hour(),
            created.minute(),
            created.second()
        ),
    };
    format!("{base}.md")
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "note".to_string()
    } else {
        trimmed
    }
}

/// Read a note from a path, parsing its front matter.
pub fn read_note(path: &Path) -> Result<Note> {
    let raw = std::fs::read_to_string(path)?;
    let (meta, body) = parse_front_matter(&raw);
    Ok(Note {
        path: path.to_path_buf(),
        meta,
        body: body.trim_start_matches('\n').to_string(),
    })
}

/// Render a full note to a string (front matter + body) and write it atomically.
pub fn write_note(note: &Note) -> Result<()> {
    if let Some(parent) = note.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!("{}{}\n", render_front_matter(&note.meta), note.body);
    // atomic-ish write
    let tmp = note.path.with_extension("md.tmp");
    std::fs::write(&tmp, &content)?;
    std::fs::rename(&tmp, &note.path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_front_matter_and_body() {
        let meta = NoteMeta {
            kind: NoteKind::Dev,
            title: Some("my note".into()),
            tags: vec!["rust".into(), "brain".into()],
            created: Local::now(),
            ..Default::default()
        };
        let body = "some body text\nline two".to_string();
        let note = Note {
            path: PathBuf::from("/tmp/x.md"),
            meta,
            body,
        };
        let rendered = render_front_matter(&note.meta);
        let full = format!("{rendered}\n{}", note.body);
        let (parsed, parsed_body) = parse_front_matter(&full);
        assert_eq!(parsed.kind, NoteKind::Dev);
        assert_eq!(parsed.title.as_deref(), Some("my note"));
        assert_eq!(parsed.tags, vec!["rust", "brain"]);
        assert_eq!(parsed_body.trim(), "some body text\nline two");
    }

    #[test]
    fn slugifies_titles() {
        assert_eq!(slugify("Hello World Notes"), "hello-world-notes");
        assert_eq!(slugify("!!!"), "note");
    }
}
