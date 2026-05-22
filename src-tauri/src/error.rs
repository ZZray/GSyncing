use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("backend not found: {0}")]
    BackendNotFound(String),

    #[error("game not found: {0}")]
    GameNotFound(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("path error: {0}")]
    Path(String),

    #[error("conflict on {path}: local and remote both modified")]
    Conflict { path: String },

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn storage<E: std::fmt::Display>(e: E) -> Self {
        Self::Storage(e.to_string())
    }

    pub fn other<E: std::fmt::Display>(e: E) -> Self {
        Self::Other(e.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(format!("{e:#}"))
    }
}

// Serialize as plain string so the frontend gets a readable message via `String(e)`.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
