use std::path::PathBuf;

use brain_core::note::NoteKind;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "brain",
    version,
    about = "Local-first second brain with Nexus-style retrieval"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to a config file.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Capture a note quickly.
    Snap(SnapArgs),
    /// Open a note in your editor.
    Edit { path: PathBuf },
    /// Add session/context text from a coding session.
    Session { text: String },
    /// Keyword search the corpus.
    Find(FindArgs),
    /// Nexus-style curated retrieval with provenance.
    Ask { query: String },
    /// Rebuild the keyword index.
    Index,
    /// Print today's view.
    Daily,
    /// Open the TUI (studio).
    Studio,
}

#[derive(Args)]
pub struct SnapArgs {
    /// Kind of note: capture, journal, dev, link, session.
    #[arg(short, long, default_value = "capture")]
    pub kind: String,
    /// Title for the note.
    #[arg(short = 'T', long)]
    pub title: Option<String>,
    /// Tags, comma separated.
    #[arg(short = 't', long)]
    pub tags: Option<String>,
    /// For link kind: the URL to save.
    #[arg(long)]
    pub url: Option<String>,
    /// Body text. If omitted and stdin is not a TTY, read from stdin.
    pub body: Vec<String>,
}

#[derive(Args)]
pub struct FindArgs {
    pub query: String,
    /// Show only top N results.
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
}

impl SnapArgs {
    pub fn kind(&self) -> anyhow::Result<NoteKind> {
        Ok(NoteKind::parse(&self.kind)?)
    }

    pub fn body_text(&self) -> Option<String> {
        let joined = self.body.join(" ");
        if !joined.trim().is_empty() {
            Some(joined)
        } else {
            None
        }
    }

    pub fn tags_vec(&self) -> Vec<String> {
        self.tags
            .as_deref()
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}
