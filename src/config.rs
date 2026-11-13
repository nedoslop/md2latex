use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    pub labels: Labels, // <-- ДОБАВЛЕНО
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

    #[serde(default = "default_appendix_prefix")]
    pub appendix_prefix: String,
    #[serde(default = "default_main_part_heading")]
    pub main_part_heading: String,
    #[serde(default = "default_bibliography_heading")]
    pub bibliography_heading: String,
}

// ── Локализация (подписи и названия) ─────────────────────────────────────
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
fn default_table_continued() -> String {
    "Продолжение таблицы".into()
}
fn default_table_ended() -> String {
    "Окончание таблицы".into()
}
fn default_figure_name() -> String {
    "Рисунок".into()
}
fn default_table_name() -> String {
    "Таблица".into()
}
fn default_contents_name() -> String {
    "СОДЕРЖАНИЕ".into()
}
fn default_bibliography_name() -> String {
    "СПИСОК ИСПОЛЬЗОВАННЫХ ИСТОЧНИКОВ".into()
}
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
fn default_appendix_prefix() -> String {
    "ПРИЛОЖЕНИЕ".into()
}
fn default_main_part_heading() -> String {
    "ОСНОВНАЯ ЧАСТЬ".into()
}
fn default_bibliography_heading() -> String {
    "СПИСОК ИСПОЛЬЗОВАННЫХ ИСТОЧНИКОВ".into()
}

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
            self.bibliography_heading.clone(),
            "СПИСОК СОКРАЩЕНИЙ".into(),
            self.appendix_prefix.clone(),
            self.main_part_heading.clone(),
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Student {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub student_id: String,
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
    #[serde(default = "default_title_course")]
    pub title_course: String,
    #[serde(default = "default_title_bachelor")]
    pub title_bachelor: String,
    #[serde(default = "default_title_master")]
    pub title_master: String,
}
fn default_templates_dir() -> String {
    "templates".into()
}
fn default_base_template() -> String {
    "base.tex.tera".into()
}
fn default_title_course() -> String {
    "title_course.tex.tera".into()
}
fn default_title_bachelor() -> String {
    "title_bachelor.tex.tera".into()
}
fn default_title_master() -> String {
    "title_master.tex.tera".into()
}
impl Default for Templates {
    fn default() -> Self {
        Self {
            dir: default_templates_dir(),
            base: default_base_template(),
            title_course: default_title_course(),
            title_bachelor: default_title_bachelor(),
            title_master: default_title_master(),
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
    pub fn title_path_for(&self, doc_type: &str) -> PathBuf {
        let name = match doc_type {
            "vkr_master" => &self.title_master,
            "course_work" => &self.title_course,
            _ => &self.title_bachelor,
        };
        self.dir_path().join(name)
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

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Не удалось прочитать {}", path.display()))?;
        let mut cfg: Config = toml::from_str(&raw).with_context(|| "Некорректный TOML-конфиг")?;

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
