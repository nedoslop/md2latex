//! On-disk cache for processed images.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const CACHE_FILE: &str = ".cache";

pub(crate) struct ImageCache {
    path: PathBuf,
    entries: BTreeMap<String, String>,
}

impl ImageCache {
    pub(crate) fn load(dir: &Path) -> Self {
        let path = dir.join(CACHE_FILE);
        let mut entries = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('\t') {
                    if !k.is_empty() {
                        entries.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        Self { path, entries }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|s| s.as_str())
    }

    pub(crate) fn set(&mut self, name: String, value: String) {
        self.entries.insert(name, value);
    }

    pub(crate) fn prune_missing(&mut self, dir: &Path) {
        self.entries.retain(|name, _| dir.join(name).exists());
    }

    pub(crate) fn save(&self) -> Result<()> {
        let mut text = String::new();
        for (k, v) in &self.entries {
            text.push_str(k);
            text.push('\t');
            text.push_str(v);
            text.push('\n');
        }
        fs::write(&self.path, text)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }
}
