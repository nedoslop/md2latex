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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────────────────────────────────
// Data model
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct Config {
    pub project: Project,

    #[serde(default)]
    pub profile: Profile,

    #[serde(default)]
    pub doc_types: HashMap<String, String>,

    pub style: Style,

    #[serde(default)]
    pub title: Title,

    #[serde(default)]
    pub bibliography: Bibliography,
    #[serde(default)]
    pub links: Links,
    #[serde(default)]
    pub images: Images,
    #[serde(default)]
    pub lists: Lists,
    #[serde(default)]
    pub templates: Templates,
    #[serde(default)]
    pub toc: Toc,
    #[serde(default)]
    pub line_breaking: LineBreaking,
    #[serde(default)]
    pub labels: Labels,
    #[serde(default)]
    pub code: Code,

    /// PlantUML rendering settings.
    #[serde(default)]
    pub plantuml: PlantUmlConfig,

    #[serde(default = "default_extra")]
    pub extra: toml::Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub university: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Project {
    pub doc_type: String,

    /// The selector as written in frontmatter (`sibfu`, `./profiles/x`,
    /// or an absolute path). Kept for logging; the *resolved* profile
    /// directory lives on `templates.dir`.
    pub profile: String,

    /// Filled from the CLI.
    pub input: String,
    /// Filled from the CLI.
    pub output_dir: String,

    pub bib_file: Option<String>,
    /// Filled from the CLI (`--compile`).
    pub compile_pdf: bool,
}

#[derive(Debug, Deserialize)]
pub struct Style {
    #[serde(default = "default_main_font")]
    pub font: String,
    #[serde(default = "default_font_fallback")]
    pub font_fallback: String,
    #[serde(default = "default_font_size")]
    pub font_size: String,

    #[serde(default = "default_margin_left")]
    pub margin_left: String,
    #[serde(default = "default_margin")]
    pub margin_right: String,
    #[serde(default = "default_margin")]
    pub margin_top: String,
    #[serde(default = "default_margin")]
    pub margin_bottom: String,

    #[serde(default = "default_paragraph_indent")]
    pub paragraph_indent: String,
    #[serde(default = "default_line_spacing")]
    pub line_spacing: String,
    #[serde(default = "default_numbering")]
    pub numbering: String,

    #[serde(default = "default_true")]
    pub title_page_fill_empty: bool,

    #[serde(default)]
    pub structural_headings: Vec<String>,

    #[serde(default)]
    pub extra_structural_headings: Vec<String>,

    #[serde(default)]
    pub structural_headings_unnumbered_subsections: Vec<String>,

    #[serde(default = "default_true")]
    pub structural_headings_newpage: bool,
    #[serde(default = "default_true")]
    pub section_bold: bool,
    #[serde(default = "default_false")]
    pub section_uppercase: bool,
    #[serde(default = "default_true")]
    pub structural_heading_uppercase: bool,

    #[serde(default = "default_true")]
    pub figure_caption_below: bool,
    #[serde(default = "default_true")]
    pub table_caption_above: bool,

    #[serde(default = "default_figure_width")]
    pub default_figure_width: String,

    #[serde(default = "default_main_font")]
    pub main_font: String,
    #[serde(default = "default_mono_font")]
    pub mono_font: String,
    #[serde(default = "default_caption_font_size")]
    pub caption_font_size: String,
    #[serde(default = "default_table_font_size")]
    pub table_font_size: String,
    #[serde(default = "default_section_align")]
    pub section_align: String,

    #[serde(default = "default_section_before_skip")]
    pub section_before_skip: String,
    #[serde(default = "default_section_after_skip")]
    pub section_after_skip: String,
    #[serde(default = "default_subsection_before_skip")]
    pub subsection_before_skip: String,
    #[serde(default = "default_subsection_after_skip")]
    pub subsection_after_skip: String,

    #[serde(default = "default_appendix_default_letter")]
    pub appendix_default_letter: String,
    #[serde(default = "default_appendix_prefix")]
    pub appendix_prefix: String,
    #[serde(default = "default_main_part_heading")]
    pub main_part_heading: String,

    #[serde(default = "default_true")]
    pub main_part_silent: bool,

    #[serde(default = "default_bibliography_heading")]
    pub bibliography_heading: String,

    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_other_language")]
    pub other_language: String,
}

#[derive(Debug, Deserialize)]
pub struct Code {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_code_engine")]
    pub engine: String,
    #[serde(default = "default_code_font_size")]
    pub font_size: String,
    #[serde(default = "default_false")]
    pub line_numbers: bool,
    #[serde(default = "default_true")]
    pub frame: bool,
}

fn default_code_engine() -> String {
    "listings".into()
}
fn default_code_font_size() -> String {
    "12pt".into()
}

impl Default for Code {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: default_code_engine(),
            font_size: default_code_font_size(),
            line_numbers: false,
            frame: true,
        }
    }
}

// ── PlantUML ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PlantUmlConfig {
    /// Включить/выключить рендеринг PlantUML.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Путь к готовому `plantuml.jar`. Если не задан — JAR будет скачан
    /// автоматически в `<output_dir>/plantuml/plantuml.jar`.
    #[serde(default)]
    pub jar_path: Option<String>,

    /// Команда запуска Java (по умолчанию `java`).
    #[serde(default)]
    pub java_path: Option<String>,
}

impl Default for PlantUmlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            jar_path: None,
            java_path: None,
        }
    }
}

// ── Остальные структуры без изменений ───────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Labels {
    #[serde(default = "default_table_continued")]
    pub table_continued: String,
    #[serde(default = "default_table_ended")]
    pub table_ended: String,
    #[serde(default = "default_figure_name")]
    pub figure_name: String,
    #[serde(default = "default_table_name")]
    pub table_name: String,
    #[serde(default = "default_listing_name")]
    pub listing_name: String,
    #[serde(default = "default_contents_name")]
    pub contents_name: String,
    #[serde(default = "default_bibliography_name")]
    pub bibliography_name: String,
}

fn default_table_continued() -> String {
    "Continued table".into()
}
fn default_table_ended() -> String {
    "End of table".into()
}
fn default_figure_name() -> String {
    "Figure".into()
}
fn default_table_name() -> String {
    "Table".into()
}
fn default_listing_name() -> String {
    "Listing".into()
}
fn default_contents_name() -> String {
    "CONTENTS".into()
}
fn default_bibliography_name() -> String {
    "REFERENCES".into()
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            table_continued: default_table_continued(),
            table_ended: default_table_ended(),
            figure_name: default_figure_name(),
            table_name: default_table_name(),
            listing_name: default_listing_name(),
            contents_name: default_contents_name(),
            bibliography_name: default_bibliography_name(),
        }
    }
}

fn default_font_fallback() -> String {
    "TeX Gyre Termes".into()
}
fn default_font_size() -> String {
    "14pt".into()
}
fn default_margin_left() -> String {
    "30mm".into()
}
fn default_margin() -> String {
    "20mm".into()
}
fn default_paragraph_indent() -> String {
    "12.5mm".into()
}
fn default_line_spacing() -> String {
    "onehalf".into()
}
fn default_numbering() -> String {
    "within_section".into()
}
fn default_figure_width() -> String {
    "0.8".into()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_main_font() -> String {
    "Times New Roman".into()
}
fn default_mono_font() -> String {
    "Courier New".into()
}
fn default_caption_font_size() -> String {
    "14pt".into()
}
fn default_table_font_size() -> String {
    "12pt".into()
}
fn default_section_align() -> String {
    "center".into()
}
fn default_section_before_skip() -> String {
    "12pt".into()
}
fn default_section_after_skip() -> String {
    "12pt".into()
}
fn default_subsection_before_skip() -> String {
    "12pt".into()
}
fn default_subsection_after_skip() -> String {
    "6pt".into()
}
fn default_appendix_default_letter() -> String {
    "A".into()
}
fn default_appendix_prefix() -> String {
    "APPENDIX".into()
}
fn default_main_part_heading() -> String {
    "MAIN PART".into()
}
fn default_bibliography_heading() -> String {
    "REFERENCES".into()
}
fn default_language() -> String {
    "english".into()
}
fn default_other_language() -> String {
    "russian".into()
}

fn default_extra() -> toml::Value {
    toml::Value::Table(toml::Table::new())
}

impl Style {
    pub fn structural_list(&self) -> Vec<String> {
        if !self.structural_headings.is_empty() {
            return self.structural_headings.clone();
        }
        let mut list = Vec::with_capacity(self.extra_structural_headings.len() + 3);
        list.extend(self.extra_structural_headings.iter().cloned());
        list.push(self.bibliography_heading.clone());
        list.push(self.appendix_prefix.clone());
        list.push(self.main_part_heading.clone());
        list
    }

    pub fn is_unnumbered_subsection(&self, heading: &str) -> bool {
        let t = heading.trim().to_uppercase();

        if !self.structural_headings_unnumbered_subsections.is_empty() {
            return self
                .structural_headings_unnumbered_subsections
                .iter()
                .any(|h| t == h.trim().to_uppercase());
        }

        let appendix_upper = self.appendix_prefix.trim().to_uppercase();
        if t == appendix_upper || t.starts_with(&format!("{} ", appendix_upper)) {
            return true;
        }

        let main_upper = self.main_part_heading.trim().to_uppercase();
        if t == main_upper {
            return false;
        }

        self.structural_list()
            .iter()
            .any(|h| t == h.trim().to_uppercase())
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Student {
    pub name: String,
    pub group: String,
    pub student_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Title {
    pub university: String,
    pub institute: String,
    pub department: String,
    pub doc_kind: String,
    pub direction_code: String,
    pub direction_name: String,
    pub topic: String,

    pub student: String,
    pub student_id: String,
    pub group: String,
    pub students: Vec<Student>,

    pub supervisor: String,
    pub supervisor_title: String,
    pub reviewer: String,
    pub year: String,
    pub city: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Bibliography {
    #[serde(default)]
    pub use_biblatex: bool,
}

#[derive(Debug, Deserialize)]
pub struct Links {
    #[serde(default = "default_true")]
    pub colored: bool,
    #[serde(default = "default_link_color")]
    pub link_color: String,
    #[serde(default = "default_cite_color")]
    pub cite_color: String,
    #[serde(default = "default_url_color")]
    pub url_color: String,
    #[serde(default = "default_false")]
    pub boxed: bool,
}
fn default_link_color() -> String {
    "black".into()
}
fn default_cite_color() -> String {
    "black".into()
}
fn default_url_color() -> String {
    "blue".into()
}
impl Default for Links {
    fn default() -> Self {
        Self {
            colored: true,
            link_color: "black".into(),
            cite_color: "black".into(),
            url_color: "blue".into(),
            boxed: false,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Images {
    #[serde(default = "default_true")]
    pub interpolate: bool,
    #[serde(default)]
    pub convert_to: Option<String>,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
}
fn default_dpi() -> u32 {
    300
}
impl Default for Images {
    fn default() -> Self {
        Self {
            interpolate: true,
            convert_to: None,
            dpi: 300,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Lists {
    #[serde(default = "default_itemize_label")]
    pub itemize_label: String,
    #[serde(default = "default_enumerate_label")]
    pub enumerate_label: String,
    #[serde(default = "default_bibliography_label")]
    pub bibliography_label: String,
}
fn default_itemize_label() -> String {
    "\\textendash".into()
}
fn default_enumerate_label() -> String {
    "\\arabic*)".into()
}
fn default_bibliography_label() -> String {
    "\\arabic*.".into()
}
impl Default for Lists {
    fn default() -> Self {
        Self {
            itemize_label: default_itemize_label(),
            enumerate_label: default_enumerate_label(),
            bibliography_label: default_bibliography_label(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Templates {
    #[serde(default = "default_templates_dir")]
    pub dir: String,
    #[serde(default = "default_base_template")]
    pub base: String,
    #[serde(default = "default_title_template")]
    pub title: String,
}
fn default_templates_dir() -> String {
    String::new()
}
fn default_base_template() -> String {
    "base.tex.tera".into()
}
fn default_title_template() -> String {
    "title.tex.tera".into()
}
impl Default for Templates {
    fn default() -> Self {
        Self {
            dir: default_templates_dir(),
            base: default_base_template(),
            title: default_title_template(),
        }
    }
}
impl Templates {
    pub fn dir_path(&self) -> PathBuf {
        PathBuf::from(&self.dir)
    }
    pub fn base_path(&self) -> PathBuf {
        self.dir_path().join(&self.base)
    }
    pub fn title_path(&self) -> PathBuf {
        self.dir_path().join(&self.title)
    }
}

#[derive(Debug, Deserialize)]
pub struct Toc {
    #[serde(default = "default_true")]
    pub clickable_dots: bool,
}
impl Default for Toc {
    fn default() -> Self {
        Self {
            clickable_dots: true,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LineBreaking {
    #[serde(default = "default_hyphen_penalty")]
    pub hyphen_penalty: i32,
    #[serde(default = "default_exhyphen_penalty")]
    pub exhyphen_penalty: i32,
    #[serde(default = "default_true")]
    pub sloppy: bool,
    #[serde(default = "default_emergencystretch")]
    pub emergencystretch: String,
    #[serde(default = "default_tolerance")]
    pub tolerance: i32,
    #[serde(default = "default_hbadness")]
    pub hbadness: i32,
}
fn default_hyphen_penalty() -> i32 {
    50
}
fn default_exhyphen_penalty() -> i32 {
    50
}
fn default_emergencystretch() -> String {
    "2em".into()
}
fn default_tolerance() -> i32 {
    9999
}
fn default_hbadness() -> i32 {
    10000
}
impl Default for LineBreaking {
    fn default() -> Self {
        Self {
            hyphen_penalty: 50,
            exhyphen_penalty: 50,
            sloppy: true,
            emergencystretch: "2em".into(),
            tolerance: 9999,
            hbadness: 10000,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Loading
// ─────────────────────────────────────────────────────────────────────────

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
        let (fm_str, body) = split_frontmatter(md_content);

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
        let fm_yaml = substitute_env_in_yaml(fm_yaml)
            .with_context(|| format!("resolving $VAR references in {}", md_path.display()))?;
        let fm_toml = yaml_to_toml(fm_yaml).context("converting frontmatter YAML to TOML value")?;

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
        let profile_dir = resolve_profile_dir(&selector, md_dir, cli.profiles_dir.as_deref())
            .with_context(|| format!("could not resolve profile '{}'", selector))?;

        // ── 2. Load profile.toml ────────────────────────────────────────
        let profile_path = profile_dir.join("profile.toml");
        let profile_raw = fs::read_to_string(&profile_path)
            .with_context(|| format!("failed to read profile: {}", profile_path.display()))?;
        let mut merged: toml::Value = toml::from_str(&profile_raw)
            .with_context(|| format!("invalid TOML in {}", profile_path.display()))?;

        // ── 3. Deep-merge frontmatter over profile ──────────────────────
        merge_toml(&mut merged, fm_toml);

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

// ─────────────────────────────────────────────────────────────────────────
// Frontmatter extraction
// ─────────────────────────────────────────────────────────────────────────

/// Splits a Markdown document into `(frontmatter, body)`.
///
/// Frontmatter is recognised as the region between the leading `---` line
/// and the next standalone `---` line. If the file doesn't start with `---`,
/// or the closing delimiter is missing, everything is treated as body.
fn split_frontmatter(md: &str) -> (Option<&str>, &str) {
    let md = md.strip_prefix('\u{feff}').unwrap_or(md);

    // First line must be exactly `---`.
    let first_line_end = md.find('\n').unwrap_or(md.len());
    let first_line = md[..first_line_end].trim_end_matches('\r');
    if first_line != "---" {
        return (None, md);
    }
    let after_first = &md[(first_line_end + 1).min(md.len())..];

    // Look for a closing `---` on its own line.
    let bytes = after_first.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let line_end = after_first[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        let line = after_first[pos..line_end].trim_end_matches('\r');
        if line == "---" {
            let fm = after_first[..pos].trim_end_matches(|c| c == '\r' || c == '\n');
            let body_start = (line_end + 1).min(bytes.len());
            return (Some(fm), &after_first[body_start..]);
        }
        pos = line_end + 1;
    }
    (None, md)
}

// ─────────────────────────────────────────────────────────────────────────
// `${ENV_VAR}` substitution
// ─────────────────────────────────────────────────────────────────────────

/// Replaces `${NAME}` and `${NAME:-default}` in every string in the YAML
/// tree. Missing vars without a default produce an error — we never want
/// to silently drop a student's name from the title page.
fn substitute_env_in_yaml(v: serde_yaml_ng::Value) -> Result<serde_yaml_ng::Value> {
    use serde_yaml_ng::Value as Y;

    Ok(match v {
        Y::String(s) => Y::String(substitute_env_str(&s)?),
        Y::Sequence(seq) => {
            let items: Result<Vec<_>> = seq.into_iter().map(substitute_env_in_yaml).collect();
            Y::Sequence(items?)
        }
        Y::Mapping(m) => {
            let mut out = serde_yaml_ng::Mapping::new();
            for (k, val) in m {
                out.insert(k, substitute_env_in_yaml(val)?);
            }
            Y::Mapping(out)
        }
        Y::Tagged(t) => substitute_env_in_yaml(t.value)?,
        other => other,
    })
}

fn substitute_env_str(s: &str) -> Result<String> {
    // Match `${NAME}` or `${NAME:-default}`. Names use POSIX-ish rules.
    let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}")
        .expect("env-substitution regex is valid");

    let mut missing: Vec<String> = Vec::new();
    let out = re.replace_all(s, |caps: &regex::Captures| {
        let name = &caps[1];
        match std::env::var(name) {
            Ok(v) => v,
            Err(_) => match caps.get(2) {
                Some(def) => def.as_str().to_string(),
                None => {
                    missing.push(name.to_string());
                    String::new()
                }
            },
        }
    });

    if !missing.is_empty() {
        let mut uniq = missing.clone();
        uniq.sort();
        uniq.dedup();
        anyhow::bail!(
            "missing required environment variable(s): {}. \
             Set them in the shell or in a `.env` file.",
            uniq.join(", ")
        );
    }
    Ok(out.into_owned())
}

// ─────────────────────────────────────────────────────────────────────────
// YAML → toml::Value
// ─────────────────────────────────────────────────────────────────────────

fn yaml_to_toml(yaml: serde_yaml_ng::Value) -> Result<toml::Value> {
    use serde_yaml_ng::Value as Y;
    use toml::Value as T;

    Ok(match yaml {
        Y::Null => T::String(String::new()),
        Y::Bool(b) => T::Boolean(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                T::Integer(i)
            } else if let Some(f) = n.as_f64() {
                T::Float(f)
            } else {
                return Err(anyhow::anyhow!("unsupported YAML number: {:?}", n));
            }
        }
        Y::String(s) => T::String(s),
        Y::Sequence(seq) => {
            let items: Result<Vec<_>> = seq.into_iter().map(yaml_to_toml).collect();
            T::Array(items?)
        }
        Y::Mapping(m) => {
            let mut table = toml::map::Map::new();
            for (k, v) in m {
                // Skip explicit nulls — this lets fields fall back to
                // profile / serde defaults instead of becoming "".
                if matches!(v, Y::Null) {
                    continue;
                }
                let key = k
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("YAML frontmatter keys must be strings"))?
                    .to_string();
                table.insert(key, yaml_to_toml(v)?);
            }
            T::Table(table)
        }
        Y::Tagged(t) => yaml_to_toml(t.value)?,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Profile resolution
// ─────────────────────────────────────────────────────────────────────────

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
fn resolve_profile_dir(
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

// ─────────────────────────────────────────────────────────────────────────
// Deep merge
// ─────────────────────────────────────────────────────────────────────────

/// Deep merge `over` into `base`. Tables are merged recursively; every
/// other value from `over` replaces the one in `base`.
fn merge_toml(base: &mut toml::Value, over: toml::Value) {
    if let (toml::Value::Table(bt), toml::Value::Table(ot)) = (base, over) {
        for (k, v) in ot {
            match bt.get_mut(&k) {
                Some(existing) => merge_toml(existing, v),
                None => {
                    bt.insert(k, v);
                }
            }
        }
    }
}
