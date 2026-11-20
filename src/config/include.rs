//! Shared `.toml` configuration includes.
//!
//! A Markdown document can pull in one or more external TOML files instead
//! of repeating the same metadata in every report:
//!
//! ```yaml
//! ---
//! include:
//!   - ../common/student.toml
//!   - practice_defaults.toml
//! title:
//!   topic: "Сортировки"
//! extra:
//!   work_number: "3"
//! ---
//! ```
//!
//! A `.toml` file may itself include others: `include = ["base.toml"]`.
//!
//! Precedence (lowest → highest): `profile.toml` → included files (nested
//! includes first, then the file itself, in the order listed) → inline
//! frontmatter → CLI.
//!
//! Paths are resolved relative to the file containing the `include` (the
//! Markdown file for frontmatter), then relative to the Markdown directory,
//! then to the working directory. The `.toml` extension may be omitted.
//!
//! Every file is merged **at most once** per document (first occurrence
//! wins its position in the chain); cycles and runaway nesting are errors.

use super::env;
use super::merge::merge_toml;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Frontmatter / TOML key holding the include list.
const KEY: &str = "include";
/// Guard against pathological nesting.
const MAX_DEPTH: usize = 32;

/// Removes `include` from `v` and returns the selectors, in order.
/// Accepts a single string or an array of strings; anything else is an error.
pub(crate) fn take_includes(v: &mut toml::Value) -> Result<Vec<String>> {
    let Some(table) = v.as_table_mut() else {
        return Ok(Vec::new());
    };
    let Some(raw) = table.remove(KEY) else {
        return Ok(Vec::new());
    };
    match raw {
        toml::Value::String(s) => Ok(vec![s]),
        toml::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    toml::Value::String(s) => out.push(s),
                    _ => anyhow::bail!("`{}` entries must be strings (paths to .toml files)", KEY),
                }
            }
            Ok(out)
        }
        _ => anyhow::bail!(
            "`{}` must be a string or an array of strings (paths to .toml files)",
            KEY
        ),
    }
}

pub(crate) struct Loader<'a> {
    md_dir: &'a Path,
    /// Files already merged, in merge order (post-order: nested first).
    loaded: Vec<PathBuf>,
    seen: HashSet<PathBuf>,
    /// Files currently being processed — cycle detection.
    stack: Vec<PathBuf>,
}

impl<'a> Loader<'a> {
    pub(crate) fn new(md_dir: &'a Path) -> Self {
        Self {
            md_dir,
            loaded: Vec::new(),
            seen: HashSet::new(),
            stack: Vec::new(),
        }
    }

    /// Files that were actually merged (for logging).
    pub(crate) fn loaded(&self) -> &[PathBuf] {
        &self.loaded
    }

    /// Merges `selectors` (in order) into a fresh TOML table.
    pub(crate) fn load(&mut self, selectors: &[String], base_dir: &Path) -> Result<toml::Value> {
        let mut acc = toml::Value::Table(toml::Table::new());
        for sel in selectors {
            self.load_one(sel, base_dir, &mut acc)?;
        }
        Ok(acc)
    }

    fn load_one(&mut self, selector: &str, base_dir: &Path, acc: &mut toml::Value) -> Result<()> {
        let sel = selector.trim();
        if sel.is_empty() {
            anyhow::bail!("empty path in `{}`", KEY);
        }
        if self.stack.len() >= MAX_DEPTH {
            anyhow::bail!("`{}` nesting exceeds {} levels", KEY, MAX_DEPTH);
        }

        let path = resolve(sel, base_dir, self.md_dir)?;
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());

        if self.stack.contains(&key) {
            let chain: Vec<String> = self
                .stack
                .iter()
                .chain(std::iter::once(&key))
                .map(|p| p.display().to_string())
                .collect();
            anyhow::bail!("circular `{}`: {}", KEY, chain.join(" -> "));
        }
        if self.seen.contains(&key) {
            return Ok(()); // include-once
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read included config {}", path.display()))?;
        let val: toml::Value =
            toml::from_str(&raw).with_context(|| format!("invalid TOML in {}", path.display()))?;
        let mut val = env::substitute_env_in_toml(val)
            .with_context(|| format!("resolving $VAR references in {}", path.display()))?;
        if !val.is_table() {
            anyhow::bail!("included config {} must be a TOML table", path.display());
        }
        let nested = take_includes(&mut val)?;

        self.seen.insert(key.clone());
        self.stack.push(key.clone());
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let nested_res = self.load(&nested, dir);
        self.stack.pop();
        let nested_val =
            nested_res.with_context(|| format!("resolving `{}` in {}", KEY, path.display()))?;
        self.loaded.push(path);

        // What a file includes has lower precedence than the file itself.
        merge_toml(acc, nested_val);
        merge_toml(acc, val);
        Ok(())
    }
}

/// absolute → verbatim; relative → including file's dir, md dir, cwd.
/// A missing extension is retried with `.toml` appended.
fn resolve(selector: &str, base_dir: &Path, md_dir: &Path) -> Result<PathBuf> {
    let p = Path::new(selector);

    let mut names: Vec<PathBuf> = vec![p.to_path_buf()];
    if p.extension().is_none() {
        names.push(PathBuf::from(format!("{}.toml", selector)));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if p.is_absolute() {
        candidates.extend(names);
    } else {
        let mut dirs: Vec<PathBuf> = vec![base_dir.to_path_buf()];
        if base_dir != md_dir {
            dirs.push(md_dir.to_path_buf());
        }
        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd);
        }
        for d in &dirs {
            for name in &names {
                let c = d.join(name);
                if !candidates.contains(&c) {
                    candidates.push(c);
                }
            }
        }
    }

    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    let tried = candidates
        .iter()
        .map(|c| format!("  - {}", c.display()))
        .collect::<Vec<_>>()
        .join("\n");
    anyhow::bail!(
        "included config '{}' not found. Tried:\n{}",
        selector,
        tried
    )
}
