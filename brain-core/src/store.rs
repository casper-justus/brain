use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{BrainError, Result};

/// Configuration for the brain app, mirroring a TOML config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Root directory holding all notes.
    #[serde(default)]
    pub notes_dir: Option<PathBuf>,
    /// Where the index is written (defaults to `notes_dir/.brain/index`).
    #[serde(default)]
    pub index_dir: Option<PathBuf>,
    /// Editor used by `brain edit` (defaults to `$EDITOR`).
    #[serde(default)]
    pub editor: Option<String>,
    /// Whether to include hidden files when walking the corpus.
    #[serde(default = "default_true")]
    pub include_hidden: bool,
    /// Subdirectories to ignore when walking the corpus.
    #[serde(default)]
    pub ignore: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notes_dir: None,
            index_dir: None,
            editor: None,
            include_hidden: true,
            ignore: vec![".git".into(), ".brain".into()],
        }
    }
}

impl Config {
    pub fn notes_dir(&self) -> Result<PathBuf> {
        self.notes_dir
            .clone()
            .ok_or_else(|| BrainError::Config("no notes_dir configured".into()))
    }

    pub fn index_dir(&self) -> Result<PathBuf> {
        if let Some(d) = &self.index_dir {
            Ok(d.clone())
        } else {
            Ok(self.notes_dir()?.join(".brain").join("index"))
        }
    }

    pub fn editor(&self) -> String {
        self.editor.clone().unwrap_or_else(|| {
            std::env::var("EDITOR")
                .or_else(|_| std::env::var("VISUAL"))
                .unwrap_or_else(|_| "vi".into())
        })
    }

    /// Global config file path (~/.config/brain/config.toml).
    pub fn global_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("brain")
            .join("config.toml")
    }

    /// Load config by merging: defaults <- global file <- explicit path.
    pub fn load(explicit: Option<&Path>) -> Result<Config> {
        let mut cfg = Config::default();
        let mut merger = |p: &Path| -> Result<()> {
            if p.exists() {
                let raw = std::fs::read_to_string(p)?;
                let loaded: Config = toml::from_str(&raw)?;
                cfg.merge(loaded);
            }
            Ok(())
        };

        // explicit wins over global; merge global first so explicit overrides
        match explicit {
            Some(p) => {
                merger(&Self::global_path())?;
                merger(p)?;
            }
            None => {
                merger(&Self::global_path())?;
            }
        }
        Ok(cfg)
    }

    fn merge(&mut self, other: Config) {
        if other.notes_dir.is_some() {
            self.notes_dir = other.notes_dir;
        }
        if other.index_dir.is_some() {
            self.index_dir = other.index_dir;
        }
        if other.editor.is_some() {
            self.editor = other.editor;
        }
        self.include_hidden = other.include_hidden;
        self.ignore = other.ignore;
    }
}

/// High-level store responsible for note CRUD and path layout.
pub struct Store {
    pub cfg: Config,
}

impl Store {
    /// Open the store from config (optional explicit path), defaulting notes_dir to ~/brain.
    pub fn open(explicit: Option<&Path>) -> Result<Self> {
        let mut cfg = Config::load(explicit)?;
        if cfg.notes_dir.is_none() {
            cfg.notes_dir = Some(
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("brain"),
            );
        }
        Store::new(cfg)
    }
    pub fn new(cfg: Config) -> Result<Self> {
        // eager-validation: ensure notes_dir exists or can be created
        let notes = cfg.notes_dir()?;
        std::fs::create_dir_all(&notes)?;
        Ok(Self { cfg })
    }

    pub fn notes_dir(&self) -> &Path {
        // notes_dir always validated on construction
        self.cfg.notes_dir.as_ref().unwrap().as_path()
    }

    pub fn index_dir(&self) -> Result<PathBuf> {
        self.cfg.index_dir()
    }

    /// Directory where a given kind's notes live. Journals go flat at notes root.
    pub fn kind_dir(&self, kind: crate::note::NoteKind) -> PathBuf {
        match kind {
            crate::note::NoteKind::Journal => self.notes_dir().to_path_buf(),
            other => self.notes_dir().join(other.as_str()),
        }
    }

    /// Collect every markdown file in the corpus (respecting ignore + hidden).
    pub fn walk(&self) -> Result<Vec<PathBuf>> {
        let root = self.notes_dir();
        let mut files = Vec::new();
        let mut walker = ignore::WalkBuilder::new(root);
        walker
            .hidden(!self.cfg.include_hidden)
            .standard_filters(false);
        for pat in &self.cfg.ignore {
            walker.add_custom_ignore_filename(pat);
        }
        for result in walker.build() {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    files.push(path.to_path_buf());
                }
            }
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn create_notes_dir_on_new() {
        let td = tempfile::tempdir().unwrap();
        let cfg = Config {
            notes_dir: Some(td.path().to_path_buf()),
            ..Default::default()
        };
        let store = Store::new(cfg).unwrap();
        assert!(store.notes_dir().exists());
    }

    #[test]
    fn walks_markdown_only() {
        let td = tempfile::tempdir().unwrap();
        fs::create_dir(td.path().join("dev")).unwrap();
        fs::write(td.path().join("a.md"), "# hi").unwrap();
        fs::write(td.path().join("dev/b.md"), "# two").unwrap();
        fs::write(td.path().join("c.txt"), "not md").unwrap();
        let cfg = Config {
            notes_dir: Some(td.path().to_path_buf()),
            ..Default::default()
        };
        let store = Store::new(cfg).unwrap();
        let files = store.walk().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.extension().unwrap() == "md"));
    }
}
