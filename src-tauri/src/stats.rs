//! Lightweight sync statistics. Append-only JSONL at
//! `<data_dir>/stats.jsonl`. One line per finished sync. Designed for ~years
//! of usage to stay under a few MB — each entry is < 200 bytes.

use crate::error::AppResult;
use crate::paths;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatEntry {
    pub ts: i64,
    #[serde(rename = "gameId")]
    pub game_id: String,
    pub direction: String,
    pub success: bool,
    #[serde(rename = "uploadedFiles", default)]
    pub uploaded_files: usize,
    #[serde(rename = "downloadedFiles", default)]
    pub downloaded_files: usize,
    #[serde(rename = "totalBytes", default)]
    pub total_bytes: u64,
    #[serde(rename = "durationMs", default)]
    pub duration_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct StatStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl StatStore {
    pub fn new() -> AppResult<Self> {
        let path = paths::data_dir()?.join("stats.jsonl");
        Ok(Self {
            path,
            write_lock: Mutex::new(()),
        })
    }

    /// Hard cap on persisted entries. When exceeded we drop the oldest
    /// `TRIM_CHUNK` so the file never grows unbounded over months of use.
    pub const MAX_ENTRIES: usize = 5_000;
    pub const TRIM_CHUNK: usize = 1_000;

    pub async fn append(&self, mut entry: StatEntry) -> AppResult<()> {
        if entry.ts == 0 {
            entry.ts = Utc::now().timestamp_millis();
        }
        let line = serde_json::to_string(&entry)? + "\n";
        let _guard = self.write_lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            use std::io::Write;
            // Probe line count without holding the file open for read+write
            // simultaneously. Cheap because the file is small (~200 B/line).
            let line_count = if path.exists() {
                std::fs::read(&path)
                    .map(|b| b.iter().filter(|c| **c == b'\n').count())
                    .unwrap_or(0)
            } else {
                0
            };
            if line_count >= Self::MAX_ENTRIES {
                // Truncate: read all, drop first TRIM_CHUNK lines, atomic write back.
                if let Ok(bytes) = std::fs::read(&path) {
                    let mut lines: Vec<&[u8]> = bytes.split(|c| *c == b'\n').collect();
                    // Last entry after split('\n') is usually empty; preserve it.
                    let trailing_empty = lines.last().map(|l| l.is_empty()).unwrap_or(false);
                    if trailing_empty {
                        lines.pop();
                    }
                    if lines.len() > Self::TRIM_CHUNK {
                        let keep = &lines[Self::TRIM_CHUNK..];
                        let tmp = path.with_extension("jsonl.tmp");
                        let mut out: Vec<u8> = Vec::new();
                        for l in keep {
                            out.extend_from_slice(l);
                            out.push(b'\n');
                        }
                        std::fs::write(&tmp, &out)?;
                        std::fs::rename(&tmp, &path)?;
                    }
                }
            }
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            f.write_all(line.as_bytes())?;
            Ok(())
        })
        .await
        .map_err(|e| crate::error::AppError::other(format!("stats join: {e}")))??;
        Ok(())
    }

    pub async fn read_all(&self) -> AppResult<Vec<StatEntry>> {
        let path = self.path.clone();
        let bytes = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            if !path.exists() {
                return Ok(Vec::new());
            }
            std::fs::read(path)
        })
        .await
        .map_err(|e| crate::error::AppError::other(format!("stats read join: {e}")))?
        .map_err(crate::error::AppError::Io)?;
        let mut out = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_slice::<StatEntry>(line) {
                out.push(entry);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_round_trips_json() {
        let e = StatEntry {
            ts: 1700000000000,
            game_id: "g1".into(),
            direction: "auto".into(),
            success: true,
            uploaded_files: 3,
            downloaded_files: 0,
            total_bytes: 1234,
            duration_ms: 980,
            error: None,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"gameId\""));
        assert!(json.contains("\"uploadedFiles\""));
        let back: StatEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.game_id, "g1");
        assert_eq!(back.uploaded_files, 3);
    }
}
