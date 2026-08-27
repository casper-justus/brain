mod tui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// `studio` — the interactive TUI for your brain.
#[derive(Parser)]
#[command(
    name = "studio",
    about = "Interactive TUI browser for your brain notes"
)]
struct Cli {
    /// Path to a config file.
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = brain_core::store::Store::open(cli.config.as_deref())?;
    tui::run(store).map_err(anyhow::Error::from)
}
