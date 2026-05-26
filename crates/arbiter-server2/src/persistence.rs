//! JSON file persistence for arbiter state.
//!
//! Persists the full server snapshot to a JSON file on disk.
//! Periodically flushed by the background persistence loop.

use std::path::PathBuf;

use anyhow::Context;
use arbiter_core3::ServerSnapshot;

/// Persists arbiter state to disk as a single JSON file.
#[derive(Clone)]
pub struct Persister {
    path: PathBuf,
}

impl Persister {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { path: data_dir }
    }

    fn state_path(&self) -> PathBuf {
        let mut p = self.path.clone();
        p.push("state.json");
        p
    }

    /// Save the full server snapshot to disk.
    pub fn save_all(&self, snapshot: &ServerSnapshot) -> anyhow::Result<()> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create data dir: {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(&path, &json)
            .with_context(|| format!("Failed to write state to {}", path.display()))?;
        Ok(())
    }

    /// Load the server snapshot from disk.
    pub fn load_all(&self) -> anyhow::Result<ServerSnapshot> {
        let path = self.state_path();
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read state from {}", path.display()))?;
        let snapshot: ServerSnapshot = serde_json::from_str(&json)?;
        Ok(snapshot)
    }
}
