//! Data model for configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Case-conversion and heading-numbering enums ─────────────────────────

/// Режим приведения регистра для строки (заголовка, подписи и т.п.).
///
/// Задаётся в конфиге строкой в `snake_case`:
/// `preserve`, `upper`, `title`, `sentence`, `auto_title`.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaseMode {
    /// Оставить как есть.
    #[default]
    Preserve,
    /// Всё в ВЕРХНИЙ РЕГИСТР.
    Upper,
    /// Каждое слово с Заглавной.
    Title,
    /// Первая буква заглавная, остальное — как в источнике.
    Sentence,
    /// Если строка целиком в верхнем регистре — привести к Title,
    /// иначе оставить как есть. Историческое поведение.
    AutoTitle,
}

impl CaseMode {
    /// Приводит регистр строки согласно режиму.
    pub fn apply(self, s: &str) -> String {
        match self {
            CaseMode::Preserve => s.to_string(),
            CaseMode::Upper => s.to_uppercase(),
            CaseMode::Title => to_title_case(s),
            CaseMode::Sentence => to_sentence_case(s),
            CaseMode::AutoTitle => to_title_case_if_upper(s),
        }
    }
}

/// Как обрабатывать номера в заголовках Markdown (`# 1. Введение`).
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HeadingNumbering {
    /// Снять номер из источника и позволить LaTeX нумеровать (по умолчанию).
    #[default]
    Auto,
    /// Использовать номера из источника, синхронизируя счётчики LaTeX
    /// через `\setcounter{...}{N-1}`.
    Source,
    /// Снять номера и печатать заголовки без нумерации (`\section*{}` +
    /// запись в TOC).
    None,
}

// ── Config ──────────────────────────────────────────────────────────────

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
    /// Упрощённый синтаксис ссылок (`{tbl:x}`, `@fig:y`, `\ref{z}`).
    #[serde(default)]
    pub references: References,
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

    // ── Регистр и нумерация заголовков (все поля опциональны) ────────────
    #[serde(default)]
    pub heading_case: Option<CaseMode>,
    #[serde(default)]
    pub structural_heading_case: Option<CaseMode>,
    #[serde(default)]
    pub subsection_case: Option<CaseMode>,
    #[serde(default)]
    pub heading_numbering: HeadingNumbering,
}

impl Style {
    pub fn heading_case_mode(&self) -> CaseMode {
        if let Some(m) = self.heading_case {
            return m;
        }
        if self.section_uppercase {
            CaseMode::Upper
        } else {
            CaseMode::AutoTitle
        }
    }

    pub fn structural_heading_case_mode(&self) -> CaseMode {
        self.structural_heading_case
            .unwrap_or(if self.structural_heading_uppercase {
                CaseMode::Upper
            } else {
                CaseMode::AutoTitle
            })
    }

    pub fn subsection_case_mode(&self) -> CaseMode {
        self.subsection_case.unwrap_or(CaseMode::AutoTitle)
    }
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

// ── References (упрощённый синтаксис ссылок) ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct References {
    /// Включить упрощённый синтаксис.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// `braces` — `{tbl:x}`; `at` — `@tbl:x`; `both` — оба.
    #[serde(default = "default_reference_syntax")]
    pub syntax: String,
    /// LaTeX-команда по умолчанию (когда для префикса не задана своя).
    #[serde(default = "default_reference_command")]
    pub default_command: String,
    /// Префиксы, которые распознаются. Всё прочее (например `{myCustom:x}`)
    /// считается обычным LaTeX и не трогается.
    #[serde(default = "default_reference_prefixes")]
    pub prefixes: Vec<String>,
    /// Переопределение команды для конкретного префикса.
    #[serde(default)]
    pub commands: HashMap<String, String>,
}

fn default_reference_syntax() -> String {
    "braces".into()
}
fn default_reference_command() -> String {
    "\\ref".into()
}
fn default_reference_prefixes() -> Vec<String> {
    vec![
        "eq".into(),
        "fig".into(),
        "tbl".into(),
        "lst".into(),
        "sec".into(),
        "app".into(),
    ]
}

impl Default for References {
    fn default() -> Self {
        Self {
            enabled: true,
            syntax: default_reference_syntax(),
            default_command: default_reference_command(),
            prefixes: default_reference_prefixes(),
            commands: HashMap::new(),
        }
    }
}

// ── PlantUML ────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct PlantUmlConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Render density passed to PlantUML as `-Sdpi=N`.
    /// Overridden by `skinparam dpi` inside the diagram.
    #[serde(default = "default_plantuml_dpi")]
    pub dpi: u32,
    #[serde(default)]
    pub jar_path: Option<String>,
    #[serde(default)]
    pub java_path: Option<String>,
}

fn default_plantuml_dpi() -> u32 {
    96
}

impl Default for PlantUmlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dpi: default_plantuml_dpi(),
            jar_path: None,
            java_path: None,
        }
    }
}

// ── Remaining structures ────────────────────────────────────────────────
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
    /// Автоматически добавлять скобки в ссылках на формулы:
    /// `\ref{eq:foo}` → `\eqref{eq:foo}`. Если `false`, наоборот,
    /// `\eqref{eq:foo}` → `\ref{eq:foo}`.
    #[serde(default = "default_true")]
    pub equation_ref_parens: bool,
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
            equation_ref_parens: true,
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

// ── Helpers for case conversion ─────────────────────────────────────────

fn to_title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn to_sentence_case(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut first_alpha_done = false;
    for c in lower.chars() {
        if !first_alpha_done && c.is_alphabetic() {
            result.extend(c.to_uppercase());
            first_alpha_done = true;
        } else {
            result.push(c);
        }
    }
    result
}

fn to_title_case_if_upper(s: &str) -> String {
    if s.chars().all(|c| !c.is_lowercase()) {
        to_title_case(s)
    } else {
        s.to_string()
    }
}
