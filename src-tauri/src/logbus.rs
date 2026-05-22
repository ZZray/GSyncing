use crate::error::AppResult;
use crate::paths;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

const RING_CAPACITY: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub ts: i64,
    pub level: String,
    pub scope: String,
    pub message: String,
}

pub struct LogBus {
    ring: Mutex<VecDeque<LogEntry>>,
    handle: AppHandle,
}

impl LogBus {
    pub fn new(handle: AppHandle) -> Self {
        Self {
            ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
            handle,
        }
    }

    pub fn push(&self, level: &str, scope: &str, message: impl Into<String>) {
        let entry = LogEntry {
            ts: Utc::now().timestamp_millis(),
            level: level.to_string(),
            scope: scope.to_string(),
            message: message.into(),
        };
        // also mirror to tracing so console + log file get it
        match level {
            "warn" => tracing::warn!(scope = %scope, "{}", entry.message),
            "error" => tracing::error!(scope = %scope, "{}", entry.message),
            "debug" => tracing::debug!(scope = %scope, "{}", entry.message),
            _ => tracing::info!(scope = %scope, "{}", entry.message),
        }
        {
            let mut ring = self.ring.lock().unwrap();
            if ring.len() == RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(entry.clone());
        }
        let _ = self.handle.emit("log-entry", &entry);
    }

    pub fn info(&self, scope: &str, msg: impl Into<String>) {
        self.push("info", scope, msg);
    }
    pub fn warn(&self, scope: &str, msg: impl Into<String>) {
        self.push("warn", scope, msg);
    }
    pub fn error(&self, scope: &str, msg: impl Into<String>) {
        self.push("error", scope, msg);
    }
    pub fn debug(&self, scope: &str, msg: impl Into<String>) {
        self.push("debug", scope, msg);
    }

    pub fn snapshot(&self, limit: usize) -> Vec<LogEntry> {
        let ring = self.ring.lock().unwrap();
        ring.iter().rev().take(limit).rev().cloned().collect()
    }

    pub fn persist_to_disk(&self) -> AppResult<()> {
        let path = paths::log_file()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let ring = self.ring.lock().unwrap();
        let mut buf = String::new();
        for e in ring.iter() {
            buf.push_str(&format!(
                "{} [{}] {}: {}\n",
                chrono::DateTime::from_timestamp_millis(e.ts)
                    .map(|d| d.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
                    .unwrap_or_default(),
                e.level.to_uppercase(),
                e.scope,
                e.message
            ));
        }
        std::fs::write(path, buf)?;
        Ok(())
    }
}
