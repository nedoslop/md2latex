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
    #[serde(default)]
    pub links: Links,
    #[serde(default)]
    pub images: Images,
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
    pub structural_headings_unnumbered_subsections: Vec<String>,
    #[serde(default = "default_true")]
    pub structural_headings_newpage: bool,
    #[serde(default = "default_true")]
    pub section_bold: bool,
    #[serde(default = "default_false")]
    pub section_uppercase: bool,

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

    #[serde(default = "default_true")]
    pub structural_heading_uppercase: bool,

    #[serde(default = "default_section_before_skip")]
    pub section_before_skip: String,
    #[serde(default = "default_section_after_skip")]
    pub section_after_skip: String,
    #[serde(default = "default_subsection_before_skip")]
    pub subsection_before_skip: String,
    #[serde(default = "default_subsection_after_skip")]
    pub subsection_after_skip: String,
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

impl Style {
    pub fn structural_list(&self) -> Vec<String> {
        if !self.structural_headings.is_empty() {
            return self.structural_headings.clone();
        }
        vec![
            "ВВЕДЕНИЕ".into(), "ЗАКЛЮЧЕНИЕ".into(), "СОДЕРЖАНИЕ".into(),
            "РЕФЕРАТ".into(), "АННОТАЦИЯ".into(), "СПИСОК ИСПОЛЬЗОВАННЫХ ИСТОЧНИКОВ".into(),
            "СПИСОК СОКРАЩЕНИЙ".into(), "ПРИЛОЖЕНИЕ".into(), "ОСНОВНАЯ ЧАСТЬ".into(),
        ]
    }

    pub fn is_unnumbered_subsection(&self, heading: &str) -> bool {
        let t = heading.trim().to_uppercase();
        if !self.structural_headings_unnumbered_subsections.is_empty() {
            return self.structural_headings_unnumbered_subsections.iter().any(|h| t == h.trim().to_uppercase());
        }
        matches!(
            t.as_str(),
            "ВВЕДЕНИЕ" | "ЗАКЛЮЧЕНИЕ" | "РЕФЕРАТ" | "АННОТАЦИЯ" | "СОДЕРЖАНИЕ"
                | "СПИСОК ИСПОЛЬЗОВАННЫХ ИСТОЧНИКОВ" | "СПИСОК СОКРАЩЕНИЙ" | "ПРИЛОЖЕНИЕ"
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
        Self { colored: true, link_color: "black".into(), cite_color: "black".into(), url_color: "blue".into(), boxed: false }
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
    fn default() -> Self { Self { interpolate: true, convert_to: None, dpi: 300 } }
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).with_context(|| format!("Не удалось прочитать {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw).with_context(|| "Некорректный TOML-конфиг")?;
        Ok(cfg)
    }
}