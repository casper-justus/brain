use anyhow::Result;
use brain_core::note::NoteKind;
use brain_cli::{load_store, Cli, Command, SnapArgs};
use clap::Parser;
use std::collections::BTreeMap;
use std::process::Command as ProcessCommand;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Snap(args) => cmd_snap(&args, cli.config.as_ref()),
        Command::Edit { path } => {
            let store = load_store(cli.config.as_ref())?;
            brain_cli::edit(&store, &path)
        }
        Command::Session { text } => {
            let store = load_store(cli.config.as_ref())?;
            let path = brain_cli::session(&store, &text)?;
            println!("{}", path.display());
            Ok(())
        }
        Command::Find(args) => {
            let store = load_store(cli.config.as_ref())?;
            let hits = brain_cli::find(&store, &args.query)?;
            for hit in hits.iter().take(args.limit) {
                println!(
                    "{} (score {:.2})\n  {:?}\n",
                    hit.entry.path.display(),
                    hit.score,
                    hit.matched_terms,
                );
                println!("  {}", hit.snippet);
                println!();
            }
            if hits.is_empty() {
                println!("no results for {:?}", args.query);
            }
            Ok(())
        }
        Command::Ask { query } => {
            let store = load_store(cli.config.as_ref())?;
            let answer = brain_cli::ask(&store, &query)?;
            println!("{}", answer.synthesis);
            println!();
            for r in answer.chunks {
                println!("[{}]", r.citation);
                println!("  {}", r.excerpt);
                println!();
            }
            Ok(())
        }
        Command::Index => {
            let store = load_store(cli.config.as_ref())?;
            brain_cli::index_cmd(&store)
        }
        Command::Daily => {
            let store = load_store(cli.config.as_ref())?;
            println!("{}", brain_cli::daily(&store)?);
            Ok(())
        }
        Command::Studio => {
            let mut cmd = ProcessCommand::new("studio");
            if let Some(cfg) = cli.config.as_ref() {
                cmd.arg("--config").arg(cfg);
            }
            let status = cmd.status().map_err(|e| {
                anyhow::anyhow!("failed to launch studio TUI (is brain-tui installed?): {e}")
            })?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
}

fn cmd_snap(args: &SnapArgs, cfg_path: Option<&std::path::PathBuf>) -> Result<()> {
    let store = load_store(cfg_path)?;
    let kind = args.kind()?;

    let mut extra = BTreeMap::new();
    if let Some(url) = args.url.as_deref() {
        extra.insert("url".to_string(), url.to_string());
    }

    let body_text = if kind == NoteKind::Link {
        args.body_text().or_else(|| args.url.clone())
    } else {
        args.body_text()
    };

    let note = brain_cli::snap(
        &store,
        kind,
        args.title.clone(),
        args.tags_vec(),
        body_text,
        extra,
    )?;
    println!("{}", note.path.display());
    Ok(())
}
