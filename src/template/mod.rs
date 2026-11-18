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
//!
//! User-visible title fields (`topic`, `university`, student names, …) are
//! escaped before insertion so that stray `&`, `%`, `#`, `_` etc. cannot
//! break the build. Use `[extra]` when you need raw LaTeX in the title page.

mod context;

use crate::config::Config;
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
    context::fill_title_context(&mut ctx, cfg);
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
        &context::compute_baseline(&cfg.style.caption_font_size),
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

    // Biblatex. Empty `bib_file` (common when unset) falls back to `refs.bib`.
    let bib_file = cfg
        .project
        .bib_file
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("refs.bib");
    ctx.insert("use_biblatex", &cfg.bibliography.use_biblatex);
    ctx.insert("bib_file", &bib_file);

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

    // Code highlighting
    ctx.insert("code_enabled", &cfg.code.enabled);
    ctx.insert("code_engine", &cfg.code.engine);
    ctx.insert("code_font_size", &cfg.code.font_size);
    ctx.insert(
        "code_baseline",
        &context::compute_baseline(&cfg.code.font_size),
    );
    ctx.insert("code_line_numbers", &cfg.code.line_numbers);
    ctx.insert("code_frame", &cfg.code.frame);

    // Author for PDF metadata (already escaped).
    ctx.insert("student_author", &context::collect_author(&cfg.title));

    tera.render("base.tex.tera", &ctx)
        .context("failed to render base.tex.tera")
}
