//! Configuration loading and data model.
//!
//! Configuration is **layered**:
//!
//! 1. Per-document metadata lives in **YAML frontmatter** of the input
//!    Markdown file (between `---` delimiters at the top). It selects a
//!    **profile** — a directory containing `profile.toml` plus all LaTeX
//!    templates for one institution — and supplies title-page fields.
//! 2. `profile.toml` supplies institution-wide defaults: fonts, page
//!    geometry, structural headings, localized labels, the university
//!    name, and a `[doc_types]` map from document-type names to title-page
//!    template paths.
//! 3. Any field from the frontmatter is deep-merged on top of the profile
//!    defaults.
//!
//! Three pieces come from the **CLI**, not from config: input path, output
//! directory, and whether to run `xelatex` afterwards. They are injected
//! into `[project]` after the merge.
//!
//! ## Private values
//!
//! Any string value in the frontmatter may contain `${VAR}` or
//! `${VAR:-default}` references. They are resolved from environment
//! variables (`.env` in the working directory is auto-loaded). Missing
//! vars without a default abort the build with a clear message — never
//! print a placeholder where a real name should be.
//!
//! Template selection is **path-based**, unchanged from before.

mod env;
mod frontmatter;
mod merge;
mod model;
mod profile;
mod yaml;

pub use model::*;

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// CLI-side knobs that override the merged config after frontmatter + profile
/// have been merged. These used to live in `md2latex.toml` — now they come
/// from clap args.
pub struct CliOverrides {
    pub output_dir: PathBuf,
    pub compile_pdf: bool,
    pub profiles_dir: Option<PathBuf>,
}

impl Config {
    /// Load the config from a Markdown document's YAML frontmatter,
    /// resolving the referenced profile, deep-merging frontmatter on top
    /// of the profile, and injecting CLI paths.
    ///
    /// Returns the config plus the Markdown body with the frontmatter
    /// stripped — that body is what downstream modules expect.
    pub fn from_markdown(
        md_path: &Path,
        md_content: &str,
        cli: CliOverrides,
    ) -> Result<(Self, String)> {
        let (fm_str, body) = frontmatter::split_frontmatter(md_content);
        let fm_yaml: serde_yaml_ng::Value = match fm_str {
            Some(s) if !s.trim().is_empty() => serde_yaml_ng::from_str(s).with_context(|| {
                format!(
                    "invalid YAML frontmatter in {} \
                    (frontmatter must be delimited by `---` lines at the top of the file)",
                    md_path.display()
                )
            })?,
            _ => serde_yaml_ng::Value::Mapping(Default::default()),
        };
        let fm_yaml = env::substitute_env_in_yaml(fm_yaml)
            .with_context(|| format!("resolving $VAR references in {}", md_path.display()))?;
        let fm_toml =
            yaml::yaml_to_toml(fm_yaml).context("converting frontmatter YAML to TOML value")?;

        // ── 1. Locate the profile directory ─────────────────────────────
        let selector = fm_toml
            .get("project")
            .and_then(|p| p.get("profile"))
            .and_then(|v| v.as_str())
            .context(
                "missing `project.profile` in the Markdown frontmatter \
                (e.g. `project: { profile: sibfu }`)",
            )?;
        let selector = selector.to_string();
        let md_dir = md_path.parent().unwrap_or_else(|| Path::new("."));
        let profile_dir =
            profile::resolve_profile_dir(&selector, md_dir, cli.profiles_dir.as_deref())
                .with_context(|| format!("could not resolve profile '{}'", selector))?;

        // ── 2. Load profile.toml ────────────────────────────────────────
        let profile_path = profile_dir.join("profile.toml");
        let profile_raw = fs::read_to_string(&profile_path)
            .with_context(|| format!("failed to read profile: {}", profile_path.display()))?;
        let mut merged: toml::Value = toml::from_str(&profile_raw)
            .with_context(|| format!("invalid TOML in {}", profile_path.display()))?;

        // ── 3. Deep-merge frontmatter over profile ──────────────────────
        merge::merge_toml(&mut merged, fm_toml);

        // ── 4. Inject CLI values into [project] ─────────────────────────
        let md_path_str = md_path.to_string_lossy().into_owned();
        let out_str = cli.output_dir.to_string_lossy().into_owned();
        let project = merged
            .as_table_mut()
            .context("profile.toml must be a TOML table")?
            .entry("project".to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .context("[project] must be a table")?;
        project.insert("input".into(), toml::Value::String(md_path_str));
        project.insert("output_dir".into(), toml::Value::String(out_str));
        project.insert("compile_pdf".into(), toml::Value::Boolean(cli.compile_pdf));
        project.insert("profile".into(), toml::Value::String(selector.clone()));

        let mut cfg: Config = merged
            .try_into()
            .context("invalid merged configuration (profile + frontmatter)")?;

        // ── 5. Fill the university from the profile if the user didn't ──
        if cfg.title.university.trim().is_empty() {
            cfg.title.university = cfg.profile.university.clone();
        }

        // ── 6. Resolve `templates.dir` against the profile directory ────
        if cfg.templates.dir.is_empty() || cfg.templates.dir == "." {
            cfg.templates.dir = profile_dir.to_string_lossy().into_owned();
        } else if !Path::new(&cfg.templates.dir).is_absolute() {
            cfg.templates.dir = profile_dir
                .join(&cfg.templates.dir)
                .to_string_lossy()
                .into_owned();
        }

        // ── 7. Resolve title template from [doc_types] ──────────────────
        if let Some(rel) = cfg.doc_types.get(&cfg.project.doc_type).cloned() {
            cfg.templates.title = rel;
        }

        Ok((cfg, body.to_string()))
    }
}
