//! Profile resolution.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Resolves the `profile:` selector from frontmatter to a directory.
///
/// - Absolute path → used verbatim.
/// - Contains a path separator, or starts with `./`/`../` → resolved
///   relative to the Markdown file's directory.
/// - Plain name (`sibfu`) → searched in:
///     1. `--profiles-dir` (CLI)
///     2. `$MD2LATEX_PROFILES_DIR`
///     3. `<md_dir>/profiles/<name>`
///     4. `<cwd>/profiles/<name>`
pub(crate) fn resolve_profile_dir(
    selector: &str,
    md_dir: &Path,
    cli_profiles_dir: Option<&Path>,
) -> Result<PathBuf> {
    let p = Path::new(selector);
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    let looks_like_path = selector.contains('/')
        || selector.contains('\\')
        || selector == "."
        || selector == ".."
        || selector.starts_with("./")
        || selector.starts_with("../");
    if looks_like_path {
        return Ok(md_dir.join(p));
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = cli_profiles_dir {
        candidates.push(d.join(selector));
    }
    if let Ok(d) = std::env::var("MD2LATEX_PROFILES_DIR") {
        candidates.push(PathBuf::from(d).join(selector));
    }
    candidates.push(md_dir.join("profiles").join(selector));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("profiles").join(selector));
    }
    for c in &candidates {
        if c.join("profile.toml").is_file() {
            return Ok(c.clone());
        }
    }
    let tried = candidates
        .iter()
        .map(|c| format!("  - {}", c.display()))
        .collect::<Vec<_>>()
        .join("\n");
    anyhow::bail!("profile '{}' not found. Tried:\n{}", selector, tried)
}
