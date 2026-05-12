//! Per-arbiter YAML persistence layer.
//!
//! Each arbiter's state is stored in a separate YAML file at
//! `{data_dir}/{arbiter_did}.yaml`.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use arbiter_core::PersistentArbiter;

// ---------------------------------------------------------------------------
// Persister
// ---------------------------------------------------------------------------

/// Manages reading and writing arbiter state to YAML files on disk.
#[derive(Debug, Clone)]
pub struct Persister {
    data_dir: PathBuf,
}

impl Persister {
    /// Create a new persister that stores files in `data_dir`.
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Load all arbiter states from disk.
    ///
    /// Returns a map of arbiter DID to their persistent state.
    pub fn load_all(&self) -> HashMap<String, PersistentArbiter> {
        let mut result = HashMap::new();

        if !self.data_dir.exists() {
            tracing::info!("Data directory does not exist, creating: {:?}", self.data_dir);
            if let Err(e) = fs::create_dir_all(&self.data_dir) {
                tracing::error!("Failed to create data directory: {e}");
                return result;
            }
            return result;
        }

        let entries = match fs::read_dir(&self.data_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::error!("Failed to read data directory: {e}");
                return result;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |ext| ext != "yaml") {
                continue;
            }

            // Extract DID from filename: "{did}.yaml"
            let did = match path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            match self.load_single(&path) {
                Ok(state) => {
                    tracing::info!("Loaded arbiter state: {did}");
                    result.insert(did, state);
                }
                Err(e) => {
                    tracing::error!("Failed to load arbiter state from {:?}: {e}", path);
                }
            }
        }

        result
    }

    /// Write a single arbiter's state to its YAML file.
    pub fn persist(&self, arbiter_did: &str, state: &PersistentArbiter) -> io::Result<()> {
        fs::create_dir_all(&self.data_dir)?;

        let path = self.file_path(arbiter_did);
        let yaml = serde_yaml::to_string(state)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Atomically write to a temp file, then rename
        let tmp_path = path.with_extension("yaml.tmp");
        fs::write(&tmp_path, &yaml)?;
        fs::rename(&tmp_path, &path)?;

        tracing::debug!("Persisted arbiter state: {arbiter_did}");
        Ok(())
    }

    /// Check if a persisted file exists for the given arbiter DID.
    pub fn exists(&self, arbiter_did: &str) -> bool {
        self.file_path(arbiter_did).exists()
    }

    /// Delete the persisted file for an arbiter.
    pub fn delete(&self, arbiter_did: &str) -> io::Result<()> {
        let path = self.file_path(arbiter_did);
        if path.exists() {
            fs::remove_file(&path)?;
            tracing::debug!("Deleted arbiter state file: {arbiter_did}");
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn file_path(&self, arbiter_did: &str) -> PathBuf {
        // Sanitize DID for use as a filename (replace special chars)
        let safe_name = arbiter_did.replace(':', "_");
        self.data_dir.join(format!("{safe_name}.yaml"))
    }

    fn load_single(&self, path: &Path) -> Result<PersistentArbiter, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let state: PersistentArbiter = serde_yaml::from_str(&contents)?;
        Ok(state)
    }
}
