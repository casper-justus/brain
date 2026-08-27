use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod error;
pub mod index;
pub mod note;
pub mod retrieve;
pub mod store;

pub use error::BrainError;
pub use note::{Note, NoteKind, NoteMeta};
pub use store::{Config, Store};

/// A single searchable / retrievable unit of content, produced by the curator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub source: PathBuf,
    pub line: usize,
    pub text: String,
    pub title: Option<String>,
}
