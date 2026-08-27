use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrainError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("note not found: {0}")]
    NotFound(String),

    #[error("index is stale; run `brain index` to rebuild: {0}")]
    StaleIndex(String),

    #[error("config problem: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, BrainError>;
