use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub project: Project,
    pub style: Style,
    pub title: Title,
    #[serde(default)]
    pub bibliography: Bibliography,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub doc_type: String,
    pub input: String,
    pub output_dir: String,
    #[serde(default)]
    pub bib_file: Option<String>,
    #[serde(default)]
    pub compile_pdf: bool,
}

#[derive(Debug, Deserialize)]
pub struct Style {
    pub font: String,
    #[serde(default = "default_font_fallback")]
    pub font_fallback: String,
    #[serde(default = "default_font_size")]
    pub font_size: String,

    #[serde(default = "default_margin")]
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
    pub structural_headings_unnumbered_subsections: Vec<String>,
    #[serde(default = "default_true")]
    pub structural_headings_newpage: bool,
    #[serde(default = "default_true")]
    pub section_bold: bool,

    #[serde(default = "default_true")]
    pub figure_caption_below: bool,
    #[serde(default = "default_true")]
    pub table_caption_above: bool,

    #[serde(default = "default_figure_width")]
    pub default_figure_width: String,
}

fn default_font_fallback() -> String { "TeX Gyre Termes".into() }
fn default_font_size() -> String { "14pt".into() }
fn default_margin() -> String { "20mm".into() }
fn default_paragraph_indent() -> String { "12.5mm".into() }
fn default_line_spacing() -> String { "onehalf".into() }
fn default_numbering() -> String { "within_section".into() }
fn default_figure_width() -> String { "0.8".into() }
fn default_true() -> bool { true }

impl Style {
    pub fn structural_list(&self) -> Vec<String> {
        if !self.structural_headings.is_empty() {
            return self.structural_headings.clone();
        }
        vec![
            "ВВЕДЕНИЕ".into(),
            "ЗАКЛЮЧЕНИЕ".into(),
            "СОДЕРЖАНИЕ".into(),
            "РЕФЕРАТ".into(),
            "АННОТАЦИЯ".into(),
            "СПИСОК ИСПОЛЬЗОВАННЫХ ИСТОЧНИКОВ".into(),
            "СПИСОК СОКРАЩЕНИЙ".into(),
            "ПРИЛОЖЕНИЕ".into(),
            "ОСНОВНАЯ ЧАСТЬ".into(),
        ]
    }

    pub fn is_unnumbered_subsection(&self, heading: &str) -> bool {
        let t = heading.trim().to_uppercase();
        if !self.structural_headings_unnumbered_subsections.is_empty() {
            return self
                .structural_headings_unnumbered_subsections
                .iter()
                .any(|h| t == h.trim().to_uppercase());
        }
        matches!(
            t.as_str(),
            "ВВЕДЕНИЕ"
                | "ЗАКЛЮЧЕНИЕ"
                | "РЕФЕРАТ"
                | "АННОТАЦИЯ"
                | "СОДЕРЖАНИЕ"
                | "СПИСОК ИСПОЛЬЗОВАННЫХ ИСТОЧНИКОВ"
                | "СПИСОК СОКРАЩЕНИЙ"
                | "ПРИЛОЖЕНИЕ"
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct Title {
    pub university: String,
    pub institute: String,
    pub department: String,
    pub doc_kind: String,
    pub direction_code: String,
    pub direction_name: String,
    pub topic: String,
    pub student: String,
    #[serde(default)]
    pub student_id: String,
    pub group: String,
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

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Не удалось прочитать {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw).with_context(|| "Некорректный TOML-конфиг")?;
        Ok(cfg)
    }
}