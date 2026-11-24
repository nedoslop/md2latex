//! Markdown -> LaTeX conversion.
//!
//! Structural headings, appendix prefix, bibliography heading, and all
//! caption labels are read from the config; this module contains no
//! natural-language strings.
//!
//! ## Module map
//!
//! * `state`     — rendering context shared by all submodules
//! * `util`      — LaTeX escaping / caption formatting / text helpers
//! * `inline`    — inline nodes (text, code, math, links, footnote refs)
//! * `headings`  — structural headings, sections, appendices
//! * `lists`     — itemize / enumerate / bibliography lists
//! * `footnotes` — footnote definition collection
//! * `code`      — fenced code blocks (listings / minted)
//! * `images`    — image sizing, figure and inline images
//! * `plantuml`  — PlantUML rendering (inline blocks and `.puml` files)
//! * `tables`    — GFM tables with merged cells
//!
//! ## Table cell merging
//!
//! Two GFM-table cell markers are recognised after trimming:
//!
//! * `<<` — **column** continuation: extends the horizontal span
//!   (`colspan`) of the cell to its left by one. Chains.
//! * `^^` — **row** continuation: extends the vertical span (`rowspan`)
//!   of the cell above by one. Chains. Works in any row (header or
//!   data). The `^^` cell itself becomes empty in the output; the
//!   content lives in the *starting* cell of the multirow.
//!
//! ## Footnotes
//!
//! Standard GFM footnotes (`[^name]` + `[^name]: text`) are supported.
//! On first reference we emit `\footnote{...}`; later references to the
//! same footnote emit `\footnotemark[N]` pointing at the number LaTeX
//! assigned to the first occurrence.
//!
//! ## Equation labels
//!
//! A standalone paragraph `#eq:name` immediately before a `$$...$$`
//! display equation attaches `\label{eq:name}` to that equation. The
//! label is dropped (with a warning) if the next block isn't display
//! math.
//!
//! ## Code block labels and captions
//!
//! Info-string syntax: ```` ```<lang> #lst:name Caption text ````.
//! `#lst:name` becomes `label=lst:name`; the remaining tokens (if any)
//! become the caption. Works for both `listings` and `minted` engines.
//!
//! ## PlantUML
//!
//! Two ways to include a diagram:
//!
//! 1. **Inline code block** — a fenced block with language `plantuml`.
//!    By default it is rendered as a **code listing** (just like any
//!    other language). Add `render=true` to turn it into a rendered
//!    figure:
//!
//!    ````markdown
//!    ```plantuml render=true #fig:my-diagram Пример диаграммы
//!    @startuml
//!    A -> B
//!    @enduml
//!    ```
//!    ````
//!
//!    `render=false` (or omitting `render`) keeps the block as a
//!    listing. Both `#lst:` and `#fig:` labels are accepted; use
//!    `#lst:` for listings and `#fig:` for rendered figures — a
//!    mismatch is not an error but is semantically odd.
//!
//! 2. **External file** — an image reference to a `.puml` file. Files
//!    with a `.puml` extension are **always** rendered as figures
//!    (the extension itself is the "render me" signal):
//!
//!    ```markdown
//!    ![Пример диаграммы](diagrams/seq.puml "#fig:seq width=0.8")
//!    ```
//!
//! Rendered figures honour `style.figure_caption_below` — with СТО СФУ
//! profiles the caption goes **below** the picture.
//!
//! ## References inside captions
//!
//! Captions (of listings, figures, tables) support the same "`` `\cmd` ``
//! = raw LaTeX" convention that inline code uses in body text. E.g.
//! `Код примера \`\ref{fig:x}\`` produces `Код примера \ref{fig:x}`.
//!
//! ## Table column alignment
//!
//! GFM column alignment (`:---`, `:---:`, `---:`) is honoured via the
//! `L`, `C`, `R` column types defined in `base.tex.tera`. Merged cells
//! (`\multicolumn`) inherit the alignment of their starting column.

mod code;
mod footnotes;
mod headings;
mod images;
mod inline;
mod lists;
mod plantuml;
mod state;
mod tables;
mod util;

pub use util::{escape_latex, format_caption};

use crate::config::Config;
use crate::log;
use crate::plantuml::PlantUml;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeValue};
use comrak::options::Options;
use comrak::{parse_document, Arena};
use std::fs;
use std::path::{Path, PathBuf};

use code::render_code_block;
use footnotes::collect_footnotes;
use headings::render_heading;
use images::render_image_block;
use inline::render_inline;
use lists::render_list;
use plantuml::{is_plantuml_file, render_plantuml_file};
use state::{PendingTableCaption, RenderState};
use tables::{parse_table_meta, render_table};
use util::extract_text;

/// Parses the document once; the same AST is used for the PlantUML
/// pre-pass and for rendering.
pub fn parse<'a>(arena: &'a Arena<'a>, md: &str) -> &'a AstNode<'a> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = false;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    options.extension.superscript = true;
    parse_document(arena, md, &options)
}

/// Every PlantUML source the renderer *may* ask for. Over-collecting is
/// harmless (an unused PNG in the cache); under-collecting only costs a
/// lazy render, never correctness.
pub fn collect_plantuml_sources<'a>(
    root: &'a AstNode<'a>,
    cfg: &Config,
    md_dir: &Path,
) -> Vec<String> {
    if !cfg.plantuml.enabled {
        return Vec::new();
    }
    let mut out = Vec::new();
    for node in root.descendants() {
        match &node.data.borrow().value {
            NodeValue::CodeBlock(cb) => {
                let first = cb.info.split_whitespace().next().unwrap_or("");
                if first.eq_ignore_ascii_case("plantuml") {
                    let rest = cb
                        .info
                        .split_once(char::is_whitespace)
                        .map(|(_, r)| r.trim())
                        .unwrap_or("");
                    if plantuml::info_requests_render(rest) {
                        out.push(cb.literal.clone());
                    }
                }
            }
            NodeValue::Image(link) => {
                if is_plantuml_file(&link.url) {
                    let p: PathBuf = if Path::new(&link.url).is_absolute() {
                        PathBuf::from(&link.url)
                    } else {
                        md_dir.join(&link.url)
                    };
                    if let Ok(code) = fs::read_to_string(&p) {
                        out.push(code);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn render<'a>(
    root: &'a AstNode<'a>,
    cfg: &Config,
    md_dir: &Path,
    plantuml: Option<&PlantUml>,
) -> Result<String> {
    let footnotes = collect_footnotes(root, cfg)?;
    let mut state = RenderState::new(footnotes, md_dir.to_path_buf(), plantuml);
    let mut out = String::new();
    for node in root.children() {
        render_node(node, &mut out, cfg, &mut state)?;
    }
    if let Some(pt) = state.pending_table.take() {
        out.push_str(&format_caption(&pt.caption));
        out.push_str("\n\n");
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────
// AST walker
// ─────────────────────────────────────────────────────────────────────────

fn render_node<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState<'_>,
) -> Result<()> {
    let data = node.data.borrow();
    let value = &data.value;

    if !matches!(
        value,
        NodeValue::Table(_) | NodeValue::FootnoteDefinition(_)
    ) {
        if let Some(pt) = state.pending_table.take() {
            out.push_str(&format_caption(&pt.caption));
            out.push_str("\n\n");
        }
    }

    match value {
        NodeValue::Heading(h) => render_heading(node, h.level, out, cfg, state)?,
        NodeValue::Paragraph => render_paragraph(node, out, cfg, state)?,
        NodeValue::CodeBlock(code) => render_code_block(code, out, cfg, state),
        NodeValue::List(_) => render_list(node, out, cfg, state)?,
        NodeValue::Table(_) => {
            let meta = state.pending_table.take();
            render_table(node, out, meta, cfg, state)?;
        }
        NodeValue::Image(link) => render_image_block(node, link, out, cfg, state)?,
        NodeValue::BlockQuote => {
            out.push_str("\\begin{quote}\n");
            for child in node.children() {
                render_node(child, out, cfg, state)?;
            }
            out.push_str("\\end{quote}\n\n");
        }
        NodeValue::FootnoteDefinition(_) => {}
        _ => {
            for child in node.children() {
                render_node(child, out, cfg, state)?;
            }
        }
    }
    Ok(())
}

fn render_paragraph<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState<'_>,
) -> Result<()> {
    let raw_text = extract_text(node).trim().to_string();
    if let Some(after) = raw_text.strip_prefix("#tbl:") {
        let (label, caption, breakable, new_page, widths) = parse_table_meta(after);
        state.pending_table = Some(PendingTableCaption {
            label,
            caption,
            breakable,
            new_page,
            widths,
        });
        return Ok(());
    }
    if let Some(after) = raw_text.strip_prefix("#eq:") {
        let name = after.split_whitespace().next().unwrap_or("").trim();
        if name.is_empty() {
            log::warn("empty `#eq:` label — ignoring");
        } else {
            state.pending_equation_label = Some(format!("eq:{}", name));
        }
        return Ok(());
    }
    let children: Vec<_> = node.children().collect();
    if children.len() == 1 {
        if let NodeValue::Math(m) = &children[0].data.borrow().value {
            if m.display_math {
                let mut eq = String::from("\\begin{equation}\n");
                eq.push_str(m.literal.trim());
                if let Some(lbl) = state.pending_equation_label.take() {
                    eq.push_str(&format!("\n\\label{{{}}}", lbl));
                }
                eq.push_str("\n\\end{equation}\n\n");
                out.push_str(&eq);
                return Ok(());
            }
        }
    }
    if let Some(lbl) = state.pending_equation_label.take() {
        log::warn(&format!(
            "equation label '{}' was not followed by display math",
            lbl
        ));
    }
    let mut has_image = false;
    let mut has_text = false;
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Image(_) => has_image = true,
            NodeValue::Text(t) if !t.trim().is_empty() => has_text = true,
            NodeValue::SoftBreak | NodeValue::LineBreak => {}
            _ => has_text = true,
        }
    }
    if has_image && !has_text {
        for child in node.children() {
            if let NodeValue::Image(link) = &child.data.borrow().value {
                render_image_block(child, link, out, cfg, state)?;
            }
        }
        return Ok(());
    }
    let puml_images: Vec<(String, String, String)> = node
        .children()
        .filter_map(|c| {
            if let NodeValue::Image(link) = &c.data.borrow().value {
                if is_plantuml_file(&link.url) {
                    let alt = extract_text(c).trim().to_string();
                    return Some((alt, link.url.clone(), link.title.clone()));
                }
            }
            None
        })
        .collect();
    state.skip_puml_images = true;
    let mut p = String::new();
    let inline_result = render_inline(node, &mut p, cfg, state);
    state.skip_puml_images = false;
    inline_result?;
    if !p.trim().is_empty() {
        out.push_str(p.trim());
        out.push_str("\n\n");
    }
    for (alt, url, title) in puml_images {
        render_plantuml_file(&alt, &url, &title, out, cfg, state)?;
    }
    Ok(())
}
