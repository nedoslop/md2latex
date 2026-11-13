//! LaTeX template rendering via Tera.
//!
//! All strings that reach the final document are either user content, the
//! template itself, or pulled from the configuration. The Rust side never
//! decides what a title page looks like: it only exposes a generic context
//! (`title.*`, `style.*`, `labels.*`, and the free-form `extra.*` namespace)
//! and lets the template choose what to render.
//!
//! Institution-specific boilerplate (ministry line, organisation lines,
//! signature labels) is defined **inside** the title template with
//! `{% set x = extra.x | default(value="…") %}`. This means:
//!
//! * the template file is the single source of truth for the layout;
//! * any value can be overridden from the config via `[extra]`;
//! * swapping to another institution is a matter of pointing `templates.title`
//!   at a different file — no code changes.

use crate::config::{Config, Student};
use anyhow::{Context, Result};
use std::fs;
use tera::{Context as TeraContext, Tera};

/// Renders the base template with the given `body` and returns the final
/// LaTeX source.
pub fn render_document(cfg: &Config, body: &str) -> Result<String> {
    let base_path = cfg.templates.base_path();
    let title_path = cfg.templates.title_path();

    let base_content = fs::read_to_string(&base_path)
        .with_context(|| format!("failed to read template: {}", base_path.display()))?;
    let title_content = fs::read_to_string(&title_path)
        .with_context(|| format!("failed to read template: {}", title_path.display()))?;

    let mut tera = Tera::default();
    tera.add_raw_template("base.tex.tera", &base_content)?;
    tera.add_raw_template("title.tex.tera", &title_content)?;

    let mut ctx = TeraContext::new();
    fill_title_context(&mut ctx, cfg);
    // `extra` is passed through verbatim: whatever the user puts in the
    // `[extra]` table of the TOML config is available as `extra.*` inside
    // every template.
    ctx.insert("extra", &cfg.extra);
    let title_rendered = tera.render("title.tex.tera", &ctx)?;

    ctx.insert("title_page", &title_rendered);
    ctx.insert("body", body);

    // Style
    ctx.insert("font", &cfg.style.font);
    ctx.insert("font_fallback", &cfg.style.font_fallback);
    ctx.insert("font_size", &cfg.style.font_size);
    ctx.insert("main_font", &cfg.style.main_font);
    ctx.insert("mono_font", &cfg.style.mono_font);
    ctx.insert("margin_left", &cfg.style.margin_left);
    ctx.insert("margin_right", &cfg.style.margin_right);
    ctx.insert("margin_top", &cfg.style.margin_top);
    ctx.insert("margin_bottom", &cfg.style.margin_bottom);
    ctx.insert("paragraph_indent", &cfg.style.paragraph_indent);
    ctx.insert("line_spacing", &cfg.style.line_spacing);
    ctx.insert("numbering", &cfg.style.numbering);
    ctx.insert("section_bold", &cfg.style.section_bold);
    ctx.insert("section_uppercase", &cfg.style.section_uppercase);
    ctx.insert("figure_caption_below", &cfg.style.figure_caption_below);
    ctx.insert("table_caption_above", &cfg.style.table_caption_above);
    ctx.insert("caption_font_size", &cfg.style.caption_font_size);
    ctx.insert(
        "caption_baseline",
        &compute_baseline(&cfg.style.caption_font_size),
    );
    ctx.insert("table_font_size", &cfg.style.table_font_size);
    ctx.insert("section_align", &cfg.style.section_align);
    ctx.insert("section_before_skip", &cfg.style.section_before_skip);
    ctx.insert("section_after_skip", &cfg.style.section_after_skip);
    ctx.insert("subsection_before_skip", &cfg.style.subsection_before_skip);
    ctx.insert("subsection_after_skip", &cfg.style.subsection_after_skip);

    // Language
    ctx.insert("language", &cfg.style.language);
    ctx.insert("other_language", &cfg.style.other_language);

    // Links
    ctx.insert("links_colored", &cfg.links.colored);
    ctx.insert("links_link_color", &cfg.links.link_color);
    ctx.insert("links_cite_color", &cfg.links.cite_color);
    ctx.insert("links_url_color", &cfg.links.url_color);
    ctx.insert("links_boxed", &cfg.links.boxed);

    ctx.insert("images_interpolate", &cfg.images.interpolate);

    // Biblatex
    ctx.insert("use_biblatex", &cfg.bibliography.use_biblatex);
    ctx.insert(
        "bib_file",
        &cfg.project
            .bib_file
            .clone()
            .unwrap_or_else(|| "refs.bib".into()),
    );

    // Lists
    ctx.insert("itemize_label", &cfg.lists.itemize_label);
    ctx.insert("enumerate_label", &cfg.lists.enumerate_label);
    ctx.insert("bibliography_label", &cfg.lists.bibliography_label);

    // TOC
    ctx.insert("toc_clickable_dots", &cfg.toc.clickable_dots);

    // Line breaking
    ctx.insert("lb_sloppy", &cfg.line_breaking.sloppy);
    ctx.insert("lb_hyphen_penalty", &cfg.line_breaking.hyphen_penalty);
    ctx.insert("lb_exhyphen_penalty", &cfg.line_breaking.exhyphen_penalty);
    ctx.insert("lb_emergencystretch", &cfg.line_breaking.emergencystretch);
    ctx.insert("lb_tolerance", &cfg.line_breaking.tolerance);
    ctx.insert("lb_hbadness", &cfg.line_breaking.hbadness);

    // Labels
    ctx.insert("labels", &cfg.labels);

    // Title meta (used by base template for PDF metadata and by the title
    // page itself).
    ctx.insert("topic", &cfg.title.topic);
    ctx.insert("student_author", &collect_author(&cfg.title));

    tera.render("base.tex.tera", &ctx)
        .context("failed to render base.tex.tera")
}

/// "Author(s)" string for PDF metadata: either a single student or the
/// comma-separated list of all students' names.
fn collect_author(t: &crate::config::Title) -> String {
    if !t.student.trim().is_empty() {
        return t.student.clone();
    }
    let names: Vec<String> = t
        .students
        .iter()
        .map(|s| s.name.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    names.join(", ")
}

/// Fills the title-page context with the typed domain fields.
///
/// Nothing institution-specific is inserted here — those live in the
/// template and can be overridden via `extra.*`.
fn fill_title_context(ctx: &mut TeraContext, cfg: &Config) {
    let t = &cfg.title;
    let fill = cfg.style.title_page_fill_empty;

    ctx.insert("university", &t.university);
    ctx.insert("institute", &t.institute);
    ctx.insert("department", &t.department);
    ctx.insert("doc_kind", &t.doc_kind);
    ctx.insert("direction_code", &t.direction_code);
    ctx.insert("direction_name", &t.direction_name);
    ctx.insert("topic", &t.topic);

    // Legacy single-student fields.
    ctx.insert("student", &fill_or_blank(&t.student, fill));
    ctx.insert("student_id", &t.student_id);
    ctx.insert("group", &fill_or_blank(&t.group, fill));

    // Normalized students list (always at least one entry).
    let students_ctx: Vec<Student> = if !t.students.is_empty() {
        t.students
            .iter()
            .map(|s| Student {
                name: fill_or_blank(&s.name, fill),
                group: s.group.clone(),
                student_id: s.student_id.clone(),
            })
            .collect()
    } else if !t.student.trim().is_empty()
        || !t.group.trim().is_empty()
        || !t.student_id.trim().is_empty()
    {
        vec![Student {
            name: fill_or_blank(&t.student, fill),
            group: t.group.clone(),
            student_id: t.student_id.clone(),
        }]
    } else {
        vec![Student {
            name: if fill {
                "________".into()
            } else {
                String::new()
            },
            group: String::new(),
            student_id: String::new(),
        }]
    };
    ctx.insert("students", &students_ctx);

    ctx.insert("supervisor", &fill_or_blank(&t.supervisor, fill));
    ctx.insert(
        "supervisor_title",
        &fill_or_blank(&t.supervisor_title, fill),
    );
    ctx.insert("reviewer", &fill_or_blank(&t.reviewer, fill));
    ctx.insert("year", &t.year);
    ctx.insert("city", &t.city);
}

/// Replaces an empty string with a neutral placeholder rule when
/// `title_page_fill_empty` is enabled.
fn fill_or_blank(value: &str, fill: bool) -> String {
    if value.trim().is_empty() && fill {
        "\\rule{3.5cm}{0.4pt}".to_string()
    } else {
        value.to_string()
    }
}

fn compute_baseline(size: &str) -> String {
    let s = size.trim();
    if let Some(num) = s
        .strip_suffix("pt")
        .and_then(|n| n.trim().parse::<f32>().ok())
    {
        format!("{}pt", (num * 1.2 * 10.0).round() / 10.0)
    } else {
        "16.8pt".to_string()
    }
}
