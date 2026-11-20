//! Configuration loading and data model.
//!
//! Configuration is **layered** (later layers win):
//!
//! 1. `profile.toml` — institution-wide defaults: fonts, page geometry,
//!    structural headings, localized labels, the university name, and a
//!    `[doc_types]` map from document-type names to title-page templates.
//!    A profile is a directory with `profile.toml` plus all LaTeX templates
//!    for one institution.
//! 2. **Shared `.toml` configs** listed under the frontmatter key
//!    `include:` (a string or a list of paths). They hold everything that
//!    repeats between documents — institute, department, supervisor,
//!    student/group, city, year, `project.profile`, `project.doc_type`, …
//!    so a new practice report only carries its own topic. An included
//!    file may include further files (`include = [...]`); each file is
//!    merged once, nested includes before the file itself.
//! 3. Per-document **YAML frontmatter** of the input Markdown file
//!    (between `---` delimiters at the top).
//! 4. The **CLI**: input path, output directory, and whether to run
//!    `xelatex` afterwards. They are injected into `[project]` last.
//!
//! ## Private values
//!
//! Any string value in the frontmatter *or in an included `.toml`* may
//! contain `${VAR}` or `${VAR:-default}` references. They are resolved from
//! environment variables (`.env` in the working directory is auto-loaded).
//! Missing vars without a default abort the build with a clear message —
//! never print a placeholder where a real name should be.
//!
//! Template selection is **path-based**, unchanged from before.

mod env;
mod frontmatter;
mod include;
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
    /// resolving the referenced profile, merging included `.toml` files
    /// and the frontmatter on top of it, and injecting CLI paths.
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
        let mut fm_toml =
            yaml::yaml_to_toml(fm_yaml).context("converting frontmatter YAML to TOML value")?;

        let md_dir = md_path.parent().unwrap_or_else(|| Path::new("."));

        // ── 1. Shared configs pulled in with `include:` ──────────────────
        let selectors = include::take_includes(&mut fm_toml)
            .with_context(|| format!("invalid `include` in {}", md_path.display()))?;
        let mut loader = include::Loader::new(md_dir);
        let included = loader
            .load(&selectors, md_dir)
            .with_context(|| format!("loading included config(s) of {}", md_path.display()))?;
        if !loader.loaded().is_empty() {
            let names: Vec<String> = loader
                .loaded()
                .iter()
                .map(|p| p.strip_prefix(md_dir).unwrap_or(p).display().to_string())
                .collect();
            crate::log::info(&format!("Included config: {}", names.join(", ")));
        }

        // ── 2. Locate the profile directory ─────────────────────────────
        // `project.profile` may come from the frontmatter or from an
        // included .toml — a shared file usually pins it for all documents.
        let selector = fm_toml
            .get("project")
            .and_then(|p| p.get("profile"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                included
                    .get("project")
                    .and_then(|p| p.get("profile"))
                    .and_then(|v| v.as_str())
            })
            .context(
                "missing `project.profile` in the Markdown frontmatter or in an \
                 included .toml (e.g. `project: { profile: sibfu }`)",
            )?;
        let selector = selector.to_string();
        let profile_dir =
            profile::resolve_profile_dir(&selector, md_dir, cli.profiles_dir.as_deref())
                .with_context(|| format!("could not resolve profile '{}'", selector))?;

        // ── 3. Load profile.toml ────────────────────────────────────────
        let profile_path = profile_dir.join("profile.toml");
        let profile_raw = fs::read_to_string(&profile_path)
            .with_context(|| format!("failed to read profile: {}", profile_path.display()))?;
        let mut merged: toml::Value = toml::from_str(&profile_raw)
            .with_context(|| format!("invalid TOML in {}", profile_path.display()))?;
        if merged.get("include").is_some() {
            crate::log::warn(&format!(
                "`include` in {} is ignored — list shared configs in the Markdown frontmatter",
                profile_path.display()
            ));
        }

        // ── 4. Deep-merge: included configs, then frontmatter ───────────
        merge::merge_toml(&mut merged, included);
        merge::merge_toml(&mut merged, fm_toml);

        // ── 5. Inject CLI values into [project] ─────────────────────────
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
            .context("invalid merged configuration (profile + includes + frontmatter)")?;

        // ── 6. Fill the university from the profile if the user didn't ──
        if cfg.title.university.trim().is_empty() {
            cfg.title.university = cfg.profile.university.clone();
        }

        // ── 7. Resolve `templates.dir` against the profile directory ────
        if cfg.templates.dir.is_empty() || cfg.templates.dir == "." {
            cfg.templates.dir = profile_dir.to_string_lossy().into_owned();
        } else if !Path::new(&cfg.templates.dir).is_absolute() {
            cfg.templates.dir = profile_dir
                .join(&cfg.templates.dir)
                .to_string_lossy()
                .into_owned();
        }

        // ── 8. Resolve title template from [doc_types] ──────────────────
        if let Some(rel) = cfg.doc_types.get(&cfg.project.doc_type).cloned() {
            cfg.templates.title = rel;
        }

        Ok((cfg, body.to_string()))
    }
}
