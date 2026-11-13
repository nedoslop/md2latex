//! Configuration loading and data model.
//!
//! The Rust code itself contains **no** natural-language strings and **no**
//! institution-specific boilerplate. Everything the converter must know
//! about (languages, structural headings, caption labels) is supplied via
//! `[style]` / `[labels]`. Everything that only the title page cares about
//! (ministry name, organisation lines, signature labels, …) lives inside
//! the LaTeX template file — see `templates/title_course.tex.tera`.
//!
//! A free-form `[extra]` table is injected verbatim into the template
//! context under `extra.*`. Use it to override template-side defaults or
//! to pass any custom value without extending the Rust data model.
//!
//! Template selection is **path-based**: the config points directly at the
//! base and title `.tex.tera` files. There is no code-side mapping from a
//! "document type" to a template.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level configuration tree loaded from a TOML file.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub project: Project,
    pub style: Style,
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

    /// Free-form key/value namespace injected verbatim into the template
    /// context under `extra.*`.
    ///
    /// This is the escape hatch for anything the typed schema does not
    /// cover: institution-specific boilerplate (`extra.ministry`,
    /// `extra.org_line1`, …), per-document custom fields, ad-hoc flags,
    /// etc. Templates read them as `{{ extra.my_key | default(value="…") }}`.
    ///
    /// The whole table is passed through serde_json to Tera unchanged, so
    /// any TOML value (string, integer, float, bool, array, nested table)
    /// is available.
    #[serde(default = "default_extra")]
    pub extra: toml::Value,
}

/// Project-level settings: input file, output directory, build options.
#[derive(Debug, Deserialize)]
pub struct Project {
    /// Informational label for the document being built (e.g.
    /// `"course_work"`, `"master_thesis"`, `"report"`).
    ///
    /// It is **not** used to select a template — templates are chosen by
    /// explicit paths in [`Templates`]. This field exists purely so the
    /// configuration can record what kind of document is being built.
    pub doc_type: String,

    pub input: String,
    pub output_dir: String,
    #[serde(default)]
    pub bib_file: Option<String>,
    #[serde(default)]
    pub compile_pdf: bool,
}

/// Document style settings.
///
/// Fields in this struct influence the **converter's** behaviour (parsing,
/// numbering, heading recognition, language setup). Institution-specific
/// presentation is the template's responsibility, not the config's.
#[derive(Debug, Deserialize)]
pub struct Style {
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

    /// Explicit list of structural (top-level) headings. When non-empty,
    /// overrides everything derived from `extra_structural_headings` and
    /// the three special headings below.
    #[serde(default)]
    pub structural_headings: Vec<String>,

    /// Additional structural headings, appended (together with
    /// `bibliography_heading`, `appendix_prefix`, `main_part_heading`) to
    /// form the effective structural-headings list.
    #[serde(default)]
    pub extra_structural_headings: Vec<String>,

    /// Subsection headings (H2+) inside structural sections that should
    /// remain unnumbered (e.g. sub-headings of an introduction).
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

    /// Default appendix designator used when an appendix heading contains
    /// no explicit letter.
    #[serde(default = "default_appendix_default_letter")]
    pub appendix_default_letter: String,

    /// Prefix that identifies an appendix heading (e.g. "APPENDIX").
    #[serde(default = "default_appendix_prefix")]
    pub appendix_prefix: String,

    /// Heading of the "main part" section that is not printed as a title
    /// but switches back to numbered sub-sections.
    #[serde(default = "default_main_part_heading")]
    pub main_part_heading: String,

    /// Heading of the bibliography section.
    #[serde(default = "default_bibliography_heading")]
    pub bibliography_heading: String,

    /// Main language for polyglossia (e.g. "russian", "english").
    #[serde(default = "default_language")]
    pub language: String,

    /// Secondary language for polyglossia (e.g. "english").
    #[serde(default = "default_other_language")]
    pub other_language: String,
}

// ── Localizable labels used by the converter ──────────────────────────────
//
// These are the names the **converter** injects into LaTeX (`\figurename`,
// `\tablename`, `\contentsname`, and the "continued / end of table"
// prefixes). They are language-level, not institution-level, and so belong
// to the config rather than to a template.
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
    #[serde(default = "default_contents_name")]
    pub contents_name: String,
    #[serde(default = "default_bibliography_name")]
    pub bibliography_name: String,
}

fn default_table_continued() -> String { "Continued table".into() }
fn default_table_ended() -> String { "End of table".into() }
fn default_figure_name() -> String { "Figure".into() }
fn default_table_name() -> String { "Table".into() }
fn default_contents_name() -> String { "CONTENTS".into() }
fn default_bibliography_name() -> String { "REFERENCES".into() }

impl Default for Labels {
    fn default() -> Self {
        Self {
            table_continued: default_table_continued(),
            table_ended: default_table_ended(),
            figure_name: default_figure_name(),
            table_name: default_table_name(),
            contents_name: default_contents_name(),
            bibliography_name: default_bibliography_name(),
        }
    }
}

fn default_font_fallback() -> String { "TeX Gyre Termes".into() }
fn default_font_size() -> String { "14pt".into() }
fn default_margin_left() -> String { "30mm".into() }
fn default_margin() -> String { "20mm".into() }
fn default_paragraph_indent() -> String { "12.5mm".into() }
fn default_line_spacing() -> String { "onehalf".into() }
fn default_numbering() -> String { "within_section".into() }
fn default_figure_width() -> String { "0.8".into() }
fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_main_font() -> String { "Times New Roman".into() }
fn default_mono_font() -> String { "Courier New".into() }
fn default_caption_font_size() -> String { "14pt".into() }
fn default_table_font_size() -> String { "12pt".into() }
fn default_section_align() -> String { "center".into() }
fn default_section_before_skip() -> String { "12pt".into() }
fn default_section_after_skip() -> String { "12pt".into() }
fn default_subsection_before_skip() -> String { "12pt".into() }
fn default_subsection_after_skip() -> String { "6pt".into() }
fn default_appendix_default_letter() -> String { "A".into() }
fn default_appendix_prefix() -> String { "APPENDIX".into() }
fn default_main_part_heading() -> String { "MAIN PART".into() }
fn default_bibliography_heading() -> String { "REFERENCES".into() }
fn default_language() -> String { "english".into() }
fn default_other_language() -> String { "russian".into() }

/// Default for the `[extra]` table: an empty TOML table, so `extra.*`
/// resolves to "undefined" in Tera and `| default(...)` works as expected.
fn default_extra() -> toml::Value {
    toml::Value::Table(toml::Table::new())
}

impl Style {
    /// Effective list of structural (top-level) headings.
    ///
    /// * If `structural_headings` is explicitly set, it is returned as-is.
    /// * Otherwise the list is composed from `extra_structural_headings`
    ///   plus the three special headings.
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

    /// Returns `true` if `heading` is an unnumbered subsection of a
    /// structural section (or an appendix heading).
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

/// One student on the title page.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Student {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub student_id: String,
}

/// Domain data shown on the title page.
///
/// Only universal "thesis title page" concepts live here: who, what, where,
/// when. Institution-specific presentation (which ministry line to print,
/// what labels to use, how the layout is structured) is not modelled here —
/// that belongs to the title-page template, optionally overridden via
/// `[extra]`.
#[derive(Debug, Deserialize)]
pub struct Title {
    pub university: String,
    pub institute: String,
    pub department: String,
    pub doc_kind: String,
    pub direction_code: String,
    pub direction_name: String,
    pub topic: String,

    #[serde(default)]
    pub student: String,
    #[serde(default)]
    pub student_id: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub students: Vec<Student>,

    pub supervisor: String,
    pub supervisor_title: String,
    #[serde(default)]
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
fn default_link_color() -> String { "black".into() }
fn default_cite_color() -> String { "black".into() }
fn default_url_color() -> String { "blue".into() }
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
fn default_dpi() -> u32 { 300 }
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
fn default_itemize_label() -> String { "\\textendash".into() }
fn default_enumerate_label() -> String { "\\arabic*)".into() }
fn default_bibliography_label() -> String { "\\arabic*.".into() }
impl Default for Lists {
    fn default() -> Self {
        Self {
            itemize_label: default_itemize_label(),
            enumerate_label: default_enumerate_label(),
            bibliography_label: default_bibliography_label(),
        }
    }
}

/// Template file locations.
///
/// Both `base` and `title` are **file paths**, not document-type keys. A
/// relative path is resolved against `dir`; an absolute path (or one
/// starting with `..`) is used as-is via `Path::join`. The code is
/// completely agnostic of any particular institution's template layout.
#[derive(Debug, Deserialize)]
pub struct Templates {
    #[serde(default = "default_templates_dir")]
    pub dir: String,

    /// Path to the main LaTeX template (relative to `dir` unless absolute).
    #[serde(default = "default_base_template")]
    pub base: String,

    /// Path to the title-page LaTeX template (relative to `dir` unless
    /// absolute). This is the single source of truth for title-page
    /// rendering — there is no fallback mapping by `doc_type`.
    #[serde(default = "default_title_template")]
    pub title: String,
}

fn default_templates_dir() -> String { "templates".into() }
fn default_base_template() -> String { "base.tex.tera".into() }
fn default_title_template() -> String { "title.tex.tera".into() }

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
    /// Full path to the title-page template. Absolute `title` values win
    /// over `dir`, matching `PathBuf::join` semantics.
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
        Self { clickable_dots: true }
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
fn default_hyphen_penalty() -> i32 { 50 }
fn default_exhyphen_penalty() -> i32 { 50 }
fn default_emergencystretch() -> String { "2em".into() }
fn default_tolerance() -> i32 { 9999 }
fn default_hbadness() -> i32 { 10000 }

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

impl Config {
    /// Loads a config from disk and resolves a relative `templates.dir`
    /// against the config file's location.
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut cfg: Config =
            toml::from_str(&raw).with_context(|| "invalid TOML configuration")?;

        let cfg_dir = path.parent().unwrap_or_else(|| Path::new("."));
        if !Path::new(&cfg.templates.dir).is_absolute() {
            cfg.templates.dir = cfg_dir
                .join(&cfg.templates.dir)
                .to_string_lossy()
                .into_owned();
        }
        Ok(cfg)
    }
}