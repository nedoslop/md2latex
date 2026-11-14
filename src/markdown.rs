//! Markdown -> LaTeX conversion.
//!
//! Structural headings, appendix prefix, bibliography heading, and all
//! caption labels are read from the config; this module contains no
//! natural-language strings.
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
//! ## Table column alignment
//!
//! GFM column alignment (`:---`, `:---:`, `---:`) is honoured via the
//! `L`, `C`, `R` column types defined in `base.tex.tera`. Merged cells
//! (`\multicolumn`) inherit the alignment of their starting column.

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeCodeBlock, NodeLink, NodeValue, TableAlignment};
use comrak::options::Options;
use comrak::{parse_document, Arena};
use regex::Regex;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    Ordinary,
    StructuralNumbered,
    StructuralUnnumbered,
    Appendix,
}

struct PendingTableCaption {
    label: Option<String>,
    caption: String,
    breakable: bool,
    new_page: bool,
}

/// One logical table cell after `<<`/`^^` folding.
#[derive(Clone)]
struct Cell {
    colspan: usize,
    rowspan: usize,
    content: String,
    /// True for `^^` continuation cells: emit an empty cell.
    phantom: bool,
    /// Alignment inherited from the starting physical column.
    align: TableAlignment,
}

type Row = Vec<Cell>;

/// Mutable state carried through the rendering pass.
struct RenderState {
    ctx: Ctx,
    pending_table: Option<PendingTableCaption>,
    pending_equation_label: Option<String>,
    skip_next_section_pagebreak: bool,
    in_bibliography: bool,
    /// Rendered content of each footnote definition, keyed by name.
    footnotes: HashMap<String, String>,
    /// First-use LaTeX number for each footnote name.
    footnote_numbers: HashMap<String, usize>,
    footnote_counter: usize,
}

impl RenderState {
    fn new(footnotes: HashMap<String, String>) -> Self {
        Self {
            ctx: Ctx::Ordinary,
            pending_table: None,
            pending_equation_label: None,
            skip_next_section_pagebreak: false,
            in_bibliography: false,
            footnotes,
            footnote_numbers: HashMap::new(),
            footnote_counter: 0,
        }
    }
}

pub fn markdown_to_latex(md: &str, cfg: &Config) -> Result<String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = false;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    options.extension.superscript = true;

    let root = parse_document(&arena, md, &options);

    let footnotes = collect_footnotes(root, cfg)?;
    let mut state = RenderState::new(footnotes);
    let mut out = String::new();

    for node in root.children() {
        render_node(node, &mut out, cfg, &mut state)?;
    }
    if let Some(pt) = state.pending_table.take() {
        out.push_str(&escape_latex(&pt.caption));
        out.push_str("\n\n");
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────
// Footnotes
// ─────────────────────────────────────────────────────────────────────────

fn collect_footnotes<'a>(root: &'a AstNode<'a>, cfg: &Config) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for node in root.descendants() {
        if let NodeValue::FootnoteDefinition(def) = &node.data.borrow().value {
            let name = def.name.clone();
            let content = render_footnote_content(node, cfg)?;
            map.insert(name, content);
        }
    }
    Ok(map)
}

/// Renders the content of a footnote definition into inline LaTeX.
/// Only paragraph children are supported; complex block content is
/// skipped with a warning.
fn render_footnote_content<'a>(node: &'a AstNode<'a>, cfg: &Config) -> Result<String> {
    let mut out = String::new();
    let mut local = RenderState::new(HashMap::new());
    for child in node.children() {
        let v = child.data.borrow().value.clone();
        match v {
            NodeValue::Paragraph => {
                if !out.is_empty() {
                    out.push(' ');
                }
                let mut p = String::new();
                render_inline(child, &mut p, cfg, &mut local)?;
                out.push_str(p.trim());
            }
            _ => {
                log::warn("footnote with non-paragraph content: only inline content is rendered");
            }
        }
    }
    Ok(out.trim().to_string())
}

// ─────────────────────────────────────────────────────────────────────────
// AST walker
// ─────────────────────────────────────────────────────────────────────────

fn render_node<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState,
) -> Result<()> {
    let value = node.data.borrow().value.clone();

    // Any non-table, non-footnote-def block flushes a pending table caption.
    if !matches!(
        value,
        NodeValue::Table(_) | NodeValue::FootnoteDefinition(_)
    ) {
        if let Some(pt) = state.pending_table.take() {
            out.push_str(&escape_latex(&pt.caption));
            out.push_str("\n\n");
        }
    }

    match value {
        NodeValue::Heading(h) => {
            render_heading(node, h.level, out, cfg, state)?;
        }
        NodeValue::Paragraph => {
            render_paragraph(node, out, cfg, state)?;
        }
        NodeValue::CodeBlock(code) => {
            render_code_block(&code, out, cfg);
        }
        NodeValue::List(_) => {
            render_list(node, out, cfg, state)?;
        }
        NodeValue::Table(_) => {
            let meta = state.pending_table.take();
            render_table(node, out, meta, cfg, state)?;
        }
        NodeValue::Image(link) => render_image_block(node, &link, out, cfg)?,
        NodeValue::BlockQuote => {
            out.push_str("\\begin{quote}\n");
            for child in node.children() {
                render_node(child, out, cfg, state)?;
            }
            out.push_str("\\end{quote}\n\n");
        }
        NodeValue::FootnoteDefinition(_) => {
            // Consumed by `collect_footnotes`.
        }
        _ => {
            for child in node.children() {
                render_node(child, out, cfg, state)?;
            }
        }
    }
    Ok(())
}

fn render_heading<'a>(
    node: &'a AstNode<'a>,
    level: u8,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState,
) -> Result<()> {
    let raw_text = extract_text(node);
    let clean = strip_heading_number(&raw_text)
        .trim_end_matches('.')
        .to_string();

    if level == 1 {
        if is_structural(&clean, cfg) {
            if cfg.style.structural_headings_newpage {
                out.push_str("\\clearpage\n");
            }
            let upper = clean.trim().to_uppercase();
            state.in_bibliography = upper == cfg.style.bibliography_heading.trim().to_uppercase();
            state.skip_next_section_pagebreak =
                upper == cfg.style.main_part_heading.trim().to_uppercase();

            let unnumbered = cfg.style.is_unnumbered_subsection(&clean);
            state.ctx = if unnumbered {
                Ctx::StructuralUnnumbered
            } else {
                Ctx::StructuralNumbered
            };
            if is_appendix_heading(&clean, cfg) {
                state.ctx = Ctx::Appendix;
                render_appendix_heading(&clean, out, cfg)?;
            } else {
                render_structural_heading(&clean, out, cfg)?;
            }
        } else {
            log::warn(&format!(
                "H1 heading not in structural list: '{}' - rendering as \\section",
                clean
            ));
            state.in_bibliography = false;
            state.ctx = Ctx::StructuralNumbered;
            if state.skip_next_section_pagebreak {
                state.skip_next_section_pagebreak = false;
            } else {
                out.push_str("\\clearpage\n");
            }
            out.push_str(&format!(
                "\\section{{{}}}\n\n",
                escape_latex(&to_title_case_if_upper(&clean))
            ));
        }
    } else {
        state.in_bibliography = false;
        let is_numbered_section = level == 2
            && raw_text
                .trim()
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit());
        if is_numbered_section {
            state.ctx = Ctx::StructuralNumbered;
        }

        if state.ctx == Ctx::StructuralUnnumbered {
            out.push_str(&format!(
                "\\textbf{{{}}}\\par\n\n",
                escape_latex(&to_title_case_if_upper(&clean))
            ));
            return Ok(());
        }

        let (cmd, numbered) = match (state.ctx, level) {
            (Ctx::StructuralNumbered, 2) => ("section", true),
            (Ctx::StructuralNumbered, 3) => ("subsection", true),
            (Ctx::StructuralNumbered, _) => ("subsubsection", true),
            (Ctx::Appendix, 2) => ("subsection", true),
            (Ctx::Appendix, 3) => ("subsubsection", true),
            (Ctx::Appendix, _) => ("paragraph", true),
            (Ctx::Ordinary, 2) => ("subsection", true),
            (Ctx::Ordinary, 3) => ("subsubsection", true),
            (Ctx::Ordinary, _) => ("paragraph", true),
            _ => ("section", true),
        };
        let display = to_title_case_if_upper(&clean);

        if cmd == "section" {
            if state.skip_next_section_pagebreak {
                state.skip_next_section_pagebreak = false;
            } else {
                out.push_str("\\clearpage\n");
            }
        }

        if !numbered {
            let toc_cmd = cmd.trim_end_matches('*');
            out.push_str(&format!(
                "\\phantomsection\n\\addcontentsline{{toc}}{{{}}}{{{}}}\n\\{}{{{}}}\n\n",
                toc_cmd,
                escape_latex(&display),
                cmd,
                escape_latex(&display)
            ));
        } else {
            out.push_str(&format!("\\{}{{{}}}\n\n", cmd, escape_latex(&display)));
        }
    }
    Ok(())
}

fn render_paragraph<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState,
) -> Result<()> {
    let raw_text = extract_text(node).trim().to_string();

    // `#tbl:name Caption ...` — pending table caption.
    if let Some(after) = raw_text.strip_prefix("#tbl:") {
        let (label, caption, breakable, new_page) = parse_table_meta(after);
        state.pending_table = Some(PendingTableCaption {
            label,
            caption,
            breakable,
            new_page,
        });
        return Ok(());
    }

    // `#eq:name` — pending equation label.
    if let Some(after) = raw_text.strip_prefix("#eq:") {
        let name = after.split_whitespace().next().unwrap_or("").trim();
        if name.is_empty() {
            log::warn("empty `#eq:` label — ignoring");
        } else {
            state.pending_equation_label = Some(format!("eq:{}", name));
        }
        return Ok(());
    }

    // Display math paragraph?
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

    // Non-math paragraph — drop any dangling equation label.
    if let Some(lbl) = state.pending_equation_label.take() {
        log::warn(&format!(
            "equation label '{}' was not followed by display math",
            lbl
        ));
    }

    // Image-only paragraph — render as a figure block.
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
                render_image_block(child, link, out, cfg)?;
            }
        }
    } else {
        let mut p = String::new();
        render_inline(node, &mut p, cfg, state)?;
        if !p.trim().is_empty() {
            out.push_str(p.trim());
            out.push_str("\n\n");
        }
    }
    Ok(())
}

/// Splits `#tbl:` info string into (label, caption, breakable, new_page).
fn parse_table_meta(after: &str) -> (Option<String>, String, bool, bool) {
    let mut label = None;
    let mut caption_tokens = Vec::new();
    let mut breakable = true;
    let mut new_page = false;

    for token in after.split_whitespace() {
        if label.is_none() && !token.contains('=') && token != "breakable" && token != "new_page" {
            label = Some(format!("tbl:{}", token));
        } else if token == "breakable=false" || token == "breakable=0" {
            breakable = false;
        } else if token == "breakable=true" || token == "breakable=1" || token == "breakable" {
            breakable = true;
        } else if token == "new_page=true" || token == "new_page=1" || token == "new_page" {
            new_page = true;
        } else if token == "new_page=false" || token == "new_page=0" {
            new_page = false;
        } else {
            caption_tokens.push(token);
        }
    }
    (label, caption_tokens.join(" "), breakable, new_page)
}

// ─────────────────────────────────────────────────────────────────────────
// Code blocks
// ─────────────────────────────────────────────────────────────────────────

/// Parses a code-block info string.
///
/// Grammar: `<lang> [#lst:name] [caption...]`
/// The language token is optional when `#lst:` is the first token.
fn parse_code_info(info: &str) -> (&str, Option<String>, Option<String>) {
    let tokens: Vec<&str> = info.split_whitespace().collect();
    let mut lang = "";
    let mut idx = 0;

    if !tokens.is_empty() && !tokens[0].starts_with('#') && !tokens[0].contains('=') {
        lang = tokens[0];
        idx = 1;
    }

    let mut label = None;
    let mut caption_tokens = Vec::new();
    for tok in &tokens[idx..] {
        if let Some(l) = tok.strip_prefix("#lst:") {
            label = Some(format!("lst:{}", l));
        } else if tok.starts_with('#') && !tok.starts_with("#!") {
            log::warn(&format!("unknown code-block tag '{}' — ignored", tok));
        } else {
            caption_tokens.push(*tok);
        }
    }
    let caption = if caption_tokens.is_empty() {
        None
    } else {
        Some(caption_tokens.join(" "))
    };
    (lang, label, caption)
}

fn render_code_block(code: &NodeCodeBlock, out: &mut String, cfg: &Config) {
    let literal = code.literal.trim_end();
    let (lang, label, caption) = parse_code_info(&code.info);

    if !cfg.code.enabled {
        if label.is_some() || caption.is_some() {
            log::warn("code block has label/caption but code rendering is disabled");
        }
        out.push_str(&format!(
            "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n",
            literal
        ));
        return;
    }

    if cfg.code.engine.eq_ignore_ascii_case("minted") {
        let m = map_minted_lang(lang);
        let mut opts: Vec<String> = Vec::new();
        if let Some(c) = &caption {
            opts.push(format!("caption={{{}}}", escape_latex(c)));
        }
        if let Some(l) = &label {
            opts.push(format!("label={{{}}}", l));
        }
        if opts.is_empty() {
            out.push_str(&format!(
                "\\begin{{minted}}{{{}}}\n{}\n\\end{{minted}}\n\n",
                m, literal
            ));
        } else {
            out.push_str(&format!(
                "\\begin{{minted}}[{}]{{{}}}\n{}\n\\end{{minted}}\n\n",
                opts.join(", "),
                m,
                literal
            ));
        }
    } else {
        let l = map_listings_lang(lang);
        let mut opts: Vec<String> = Vec::new();
        if !l.is_empty() {
            opts.push(format!("language={}", l));
        }
        if let Some(c) = &caption {
            opts.push(format!("caption={{{}}}", escape_latex(c)));
        }
        if let Some(lbl) = &label {
            opts.push(format!("label={{{}}}", lbl));
        }
        if opts.is_empty() {
            out.push_str(&format!(
                "\\begin{{lstlisting}}\n{}\n\\end{{lstlisting}}\n\n",
                literal
            ));
        } else {
            out.push_str(&format!(
                "\\begin{{lstlisting}}[{}]\n{}\n\\end{{lstlisting}}\n\n",
                opts.join(", "),
                literal
            ));
        }
    }
}

fn map_listings_lang(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "c" => "C".into(),
        "cpp" | "c++" | "cxx" | "cc" | "hpp" | "hxx" => "C++".into(),
        "rust" | "rs" => "Rust".into(),
        "python" | "py" => "Python".into(),
        "java" => "Java".into(),
        "js" | "javascript" => "JavaScript".into(),
        "ts" | "typescript" => "JavaScript".into(),
        "html" | "htm" => "HTML".into(),
        "xml" => "XML".into(),
        "bash" | "sh" | "shell" | "zsh" => "bash".into(),
        "sql" => "SQL".into(),
        "php" => "PHP".into(),
        "ruby" | "rb" => "Ruby".into(),
        "perl" => "Perl".into(),
        "r" => "R".into(),
        "matlab" => "Matlab".into(),
        "go" | "golang" => "Go".into(),
        _ => String::new(),
    }
}

fn map_minted_lang(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "" => "text".into(),
        "c++" | "cxx" | "cc" => "cpp".into(),
        "rs" => "rust".into(),
        "py" => "python".into(),
        "js" => "javascript".into(),
        "ts" => "typescript".into(),
        "sh" | "shell" => "bash".into(),
        "yml" => "yaml".into(),
        "htm" => "html".into(),
        other => other.into(),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Lists
// ─────────────────────────────────────────────────────────────────────────

fn render_list<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState,
) -> Result<()> {
    let list_data = match &node.data.borrow().value {
        NodeValue::List(l) => l.clone(),
        _ => return Ok(()),
    };
    let is_ordered = list_data.list_type == comrak::nodes::ListType::Ordered;
    let env = if is_ordered && state.in_bibliography {
        "stubiblist"
    } else if is_ordered {
        "enumerate"
    } else {
        "itemize"
    };

    out.push_str(&format!("\\begin{{{}}}\n", env));

    for item_node in node.children() {
        if !matches!(&item_node.data.borrow().value, NodeValue::Item(_)) {
            continue;
        }

        out.push_str("  \\item ");

        let mut first_block = true;
        for block in item_node.children() {
            let bv = block.data.borrow().value.clone();
            match &bv {
                NodeValue::Paragraph => {
                    if !first_block {
                        out.push('\n');
                    }
                    let mut p = String::new();
                    render_inline(block, &mut p, cfg, state)?;
                    out.push_str(p.trim());
                    first_block = false;
                }
                NodeValue::List(_) => {
                    out.push('\n');
                    render_list(block, out, cfg, state)?;
                    first_block = false;
                }
                _ => {
                    if !first_block {
                        out.push('\n');
                    }
                    let mut p = String::new();
                    render_inline(block, &mut p, cfg, state)?;
                    out.push_str(p.trim());
                    first_block = false;
                }
            }
        }
        out.push('\n');
    }
    out.push_str(&format!("\\end{{{}}}\n\n", env));
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Headings helpers
// ─────────────────────────────────────────────────────────────────────────

fn is_structural(text: &str, cfg: &Config) -> bool {
    let t = text.trim().to_uppercase();
    let appendix_upper = cfg.style.appendix_prefix.trim().to_uppercase();
    cfg.style.structural_list().iter().any(|h| {
        let up = h.trim().to_uppercase();
        t == up || (up == appendix_upper && t.starts_with(&format!("{} ", appendix_upper)))
    })
}

fn normalize_dashes(text: &str) -> String {
    text.replace(" --- ", " — ").replace(" -- ", " — ")
}

fn is_appendix_heading(text: &str, cfg: &Config) -> bool {
    let t = text.trim().to_uppercase();
    let prefix = cfg.style.appendix_prefix.trim().to_uppercase();
    t == prefix || t.starts_with(&format!("{} ", prefix))
}

fn render_structural_heading(text: &str, out: &mut String, cfg: &Config) -> Result<()> {
    let display = if cfg.style.structural_heading_uppercase {
        text.trim().to_uppercase()
    } else {
        to_title_case_if_upper(text)
    };

    if cfg.style.main_part_silent
        && display.trim().to_uppercase() == cfg.style.main_part_heading.trim().to_uppercase()
    {
        return Ok(());
    }

    out.push_str(&format!(
        "\\phantomsection\n\
         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
         {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n\
         \\vspace{{\\baselineskip}}\n\n",
        escape_latex(&display),
        escape_latex(&display)
    ));
    Ok(())
}

fn render_appendix_heading(text: &str, out: &mut String, cfg: &Config) -> Result<()> {
    let trimmed = text.trim();
    let prefix_upper = cfg.style.appendix_prefix.trim().to_uppercase();

    let rest = if trimmed.len() >= prefix_upper.len()
        && trimmed[..prefix_upper.len()].to_uppercase() == prefix_upper
    {
        trimmed[prefix_upper.len()..].trim()
    } else {
        trimmed
    };

    let mut parts = rest.splitn(2, char::is_whitespace);
    let letter = match parts.next() {
        Some(s) if !s.is_empty() => s.to_uppercase(),
        _ => cfg.style.appendix_default_letter.to_uppercase(),
    };
    let title = parts.next().unwrap_or("").trim();

    let app_label = format!("{} {}", cfg.style.appendix_prefix, letter);
    let toc_entry = if title.is_empty() {
        app_label.clone()
    } else {
        format!("{} {}", app_label, to_title_case_if_upper(title))
    };

    // Reset counters and rebind \the<cnt> to the appendix letter.
    // Includes subsections so sub-headings are numbered А.1, А.2, ...
    out.push_str(&format!(
        "\\clearpage\n\
         \\phantomsection\n\
         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
         \\setcounter{{figure}}{{0}}\n\
         \\setcounter{{table}}{{0}}\n\
         \\setcounter{{equation}}{{0}}\n\
         \\setcounter{{subsection}}{{0}}\n\
         \\setcounter{{subsubsection}}{{0}}\n\
         \\renewcommand{{\\thefigure}}{{{}.\\arabic{{figure}}}}\n\
         \\renewcommand{{\\thetable}}{{{}.\\arabic{{table}}}}\n\
         \\renewcommand{{\\theequation}}{{{}.\\arabic{{equation}}}}\n\
         \\renewcommand{{\\thesubsection}}{{{}.\\arabic{{subsection}}}}\n\
         \\renewcommand{{\\thesubsubsection}}{{{}.\\arabic{{subsection}}.\\arabic{{subsubsection}}}}\n\
         {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n",
        escape_latex(&toc_entry),
        letter, letter, letter, letter, letter,
        escape_latex(&app_label)
    ));

    if !title.is_empty() {
        out.push_str(&format!(
            "\\vspace{{0.5\\baselineskip}}\n\
             {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n",
            escape_latex(&to_title_case_if_upper(title))
        ));
    }
    out.push_str("\\vspace{\\baselineskip}\n\n");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Inline rendering
// ─────────────────────────────────────────────────────────────────────────

fn render_inline<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState,
) -> Result<()> {
    for child in node.children() {
        let v = child.data.borrow().value.clone();
        match v {
            NodeValue::Text(t) => out.push_str(&render_text_node(&t)),
            NodeValue::Code(c) => {
                if c.literal.starts_with('\\') {
                    out.push_str(&c.literal);
                } else {
                    out.push_str(&format!("\\texttt{{{}}}", escape_latex(&c.literal)));
                }
            }
            NodeValue::Math(m) => {
                if m.display_math {
                    out.push_str(&format!("\\[{}\\]", m.literal));
                } else {
                    out.push_str(&format!("${}$", m.literal));
                }
            }
            NodeValue::Strong => {
                out.push_str("\\textbf{");
                render_inline(child, out, cfg, state)?;
                out.push('}');
            }
            NodeValue::Emph => {
                out.push_str("\\textit{");
                render_inline(child, out, cfg, state)?;
                out.push('}');
            }
            NodeValue::Link(link) => {
                let is_external = link.url.starts_with("http://")
                    || link.url.starts_with("https://")
                    || link.url.starts_with("ftp://")
                    || link.url.starts_with("mailto:");
                if is_external && cfg.links.colored {
                    out.push_str(&format!(
                        "\\href{{{}}}{{\\textcolor{{{}}}{{",
                        escape_url(&link.url),
                        cfg.links.url_color
                    ));
                    render_inline(child, out, cfg, state)?;
                    out.push_str("}}");
                } else {
                    out.push_str(&format!("\\href{{{}}}{{", escape_url(&link.url)));
                    render_inline(child, out, cfg, state)?;
                    out.push('}');
                }
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            NodeValue::Image(link) => render_image_inline(&link, out, cfg),
            NodeValue::Superscript => {
                out.push_str("\\textsuperscript{");
                render_inline(child, out, cfg, state)?;
                out.push('}');
            }
            NodeValue::FootnoteReference(r) => {
                let name = r.name.clone();
                let num = state.footnote_numbers.get(&name).copied();
                let content = state.footnotes.get(&name).cloned();
                match (num, content) {
                    (Some(n), _) => {
                        out.push_str(&format!("\\footnotemark[{}]", n));
                    }
                    (None, Some(c)) => {
                        state.footnote_counter += 1;
                        let n = state.footnote_counter;
                        state.footnote_numbers.insert(name, n);
                        out.push_str(&format!("\\footnote{{{}}}", c));
                    }
                    (None, None) => {
                        log::warn(&format!("undefined footnote reference: '{}'", name));
                    }
                }
            }
            _ => render_inline(child, out, cfg, state)?,
        }
    }
    Ok(())
}

fn render_text_node(text: &str) -> String {
    let normalized = normalize_dashes(text);
    let re = Regex::new(r"https?://[^\s\)\]\}]+").unwrap();
    let mut out = String::new();
    let mut last = 0;

    for m in re.find_iter(&normalized) {
        out.push_str(&parse_tildes(&normalized[last..m.start()]));
        out.push_str(&format!("\\url{{{}}}", m.as_str()));
        last = m.end();
    }
    out.push_str(&parse_tildes(&normalized[last..]));
    out
}

fn parse_tildes(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;

    while let Some(start) = remaining.find('~') {
        result.push_str(&escape_latex(&remaining[..start]));

        let after = &remaining[start..];

        if after.starts_with("~~") {
            if let Some(end) = after[2..].find("~~") {
                let content = &after[2..2 + end];
                if content.is_empty() {
                    result.push_str("\\textasciitilde{}\\textasciitilde{}");
                    remaining = &after[2..];
                } else {
                    result.push_str("\\sout{");
                    result.push_str(&escape_latex(content));
                    result.push('}');
                    remaining = &after[2 + end + 2..];
                }
            } else {
                result.push_str("\\textasciitilde{}\\textasciitilde{}");
                remaining = &after[2..];
            }
        } else if let Some(end) = after[1..].find('~') {
            let content = &after[1..1 + end];
            if content.is_empty() {
                result.push_str("\\textasciitilde{}");
                remaining = &after[1..];
            } else {
                result.push_str("\\textsubscript{");
                result.push_str(&escape_latex(content));
                result.push('}');
                remaining = &after[1 + end + 1..];
            }
        } else {
            result.push_str("\\textasciitilde{}");
            remaining = &after[1..];
        }
    }
    result.push_str(&escape_latex(remaining));
    result
}

// ─────────────────────────────────────────────────────────────────────────
// Images
// ─────────────────────────────────────────────────────────────────────────

fn render_image_block<'a>(
    node: &'a AstNode<'a>,
    link: &NodeLink,
    out: &mut String,
    cfg: &Config,
) -> Result<()> {
    let src = &link.url;
    let title_raw = link.title.clone();
    let (label, width, title_caption) =
        parse_image_title(&title_raw, &cfg.style.default_figure_width);

    let alt_text = {
        let mut s = String::new();
        for c in node.children() {
            if let NodeValue::Text(t) = &c.data.borrow().value {
                s.push_str(t);
            }
        }
        s.trim().to_string()
    };
    let final_caption = if !alt_text.is_empty() {
        Some(alt_text.trim_end_matches('.').to_string())
    } else {
        title_caption.map(|c| c.trim_end_matches('.').to_string())
    };

    let width_spec =
        if width.contains("\\textwidth") || width.contains("\\linewidth") || width.contains('%') {
            width.clone()
        } else {
            format!("{}\\textwidth", width)
        };
    let interpolate_opt = if cfg.images.interpolate {
        ",interpolate=true"
    } else {
        ""
    };

    out.push_str("\\begin{figure}[H]\n  \\centering\n");
    if !cfg.style.figure_caption_below {
        if let Some(ref c) = final_caption {
            out.push_str(&format!("  \\caption{{{}}}\n", escape_latex(c)));
        }
        if let Some(ref l) = label {
            out.push_str(&format!("  \\label{{{}}}\n", l));
        }
    }
    out.push_str(&format!(
        "  \\includegraphics[width={}{}]{{{}}}\n",
        width_spec, interpolate_opt, src
    ));
    if cfg.style.figure_caption_below {
        if let Some(ref c) = final_caption {
            out.push_str(&format!("  \\caption{{{}}}\n", escape_latex(c)));
        }
        if let Some(ref l) = label {
            out.push_str(&format!("  \\label{{{}}}\n", l));
        }
    }
    out.push_str("\\end{figure}\n\n");
    Ok(())
}

fn render_image_inline(link: &NodeLink, out: &mut String, cfg: &Config) {
    let src = &link.url;
    let (_, width, _) = parse_image_title(&link.title, "\\linewidth");

    let width_spec =
        if width.contains("\\textwidth") || width.contains("\\linewidth") || width.contains('%') {
            width
        } else {
            format!("{}\\linewidth", width)
        };
    let interpolate_opt = if cfg.images.interpolate {
        ",interpolate=true"
    } else {
        ""
    };
    out.push_str(&format!(
        "\\includegraphics[width={}{}]{{{}}}",
        width_spec, interpolate_opt, src
    ));
}

fn parse_image_title(title: &str, default_width: &str) -> (Option<String>, String, Option<String>) {
    let mut rest = title.trim().to_string();
    let mut label = None;
    let mut width = default_width.to_string();
    if rest.starts_with('#') {
        let after = &rest[1..];
        if let Some(space) = after.find(char::is_whitespace) {
            label = Some(after[..space].to_string());
            rest = after[space..].trim().to_string();
        } else {
            label = Some(after.to_string());
            rest = String::new();
        }
    }
    let mut caption_tokens: Vec<String> = Vec::new();
    for token in rest.split_whitespace() {
        if let Some(w) = token.strip_prefix("width=") {
            width = w.to_string();
        } else {
            caption_tokens.push(token.to_string());
        }
    }
    let caption = if caption_tokens.is_empty() {
        None
    } else {
        Some(caption_tokens.join(" "))
    };
    (label, width, caption)
}

// ─────────────────────────────────────────────────────────────────────────
// Table rendering with `<<` / `^^` folding and column alignment.
// ─────────────────────────────────────────────────────────────────────────

/// Maps a `TableAlignment` to the LaTeX alignment command used inside
/// `\multicolumn` specs.
fn alignment_cmd(align: TableAlignment) -> &'static str {
    match align {
        TableAlignment::Left => "\\raggedright",
        TableAlignment::Right => "\\raggedleft",
        _ => "\\centering",
    }
}

/// Folds `<<`-column markers into their left neighbour.
fn merge_columns(raw: &[String], alignments: &[TableAlignment]) -> Row {
    let mut result: Row = Vec::with_capacity(raw.len());
    for (i, cell) in raw.iter().enumerate() {
        let align = alignments.get(i).copied().unwrap_or(TableAlignment::Center);
        let s = cell.trim();
        if s == "<<" {
            if let Some(last) = result.last_mut() {
                last.colspan += 1;
            } else {
                log::warn("'<<' in first cell of a table row has nothing to merge with");
                result.push(Cell {
                    colspan: 1,
                    rowspan: 1,
                    content: String::new(),
                    phantom: false,
                    align,
                });
            }
        } else {
            result.push(Cell {
                colspan: 1,
                rowspan: 1,
                content: cell.clone(),
                phantom: false,
                align,
            });
        }
    }
    result
}

/// Folds `^^`-row markers into the cell above (same physical column
/// range). Requires matching `colspan` on the continuation cell.
fn fold_rowspans(rows: &mut Vec<Row>) {
    let mut active: HashMap<usize, (usize, usize)> = HashMap::new();
    // active[physical_col] = (start_row, start_cell_idx)

    for i in 0..rows.len() {
        let mut col = 0usize;
        let row_len = rows[i].len();
        for j in 0..row_len {
            let cspan = rows[i][j].colspan;
            let is_marker = !rows[i][j].phantom && rows[i][j].content.trim() == "^^";

            if is_marker {
                let mut target: Option<(usize, usize)> = None;
                let mut consistent = true;
                for c in col..(col + cspan) {
                    match active.get(&c) {
                        Some(&(sr, sc)) => {
                            if let Some(t) = target {
                                if t != (sr, sc) {
                                    consistent = false;
                                    break;
                                }
                            } else {
                                target = Some((sr, sc));
                            }
                        }
                        None => {
                            consistent = false;
                            break;
                        }
                    }
                }

                if consistent && target.is_some() {
                    let (sr, sc) = target.unwrap();
                    let target_cell = &rows[sr][sc];

                    // Check for a conflict: attaching `^^` to a horizontally
                    // merged cell while the current row has content in those
                    // columns.
                    let mut conflict = false;
                    if target_cell.colspan > 1 {
                        let target_start: usize = rows[sr].iter().take(sc).map(|c| c.colspan).sum();
                        let target_end = target_start + target_cell.colspan;

                        let mut cur_c = 0;
                        for (k, cell) in rows[i].iter().enumerate() {
                            if k != j && cur_c < target_end && cur_c + cell.colspan > target_start {
                                conflict = true;
                                break;
                            }
                            cur_c += cell.colspan;
                        }
                    }

                    if conflict {
                        log::warn(&format!(
                            "'^^' in table row {} (col {}) cannot merge with a cell spanning \
                             multiple columns because the current row has content in those \
                             columns; treating as empty cell",
                            i, col
                        ));
                        rows[i][j].content.clear();
                        for c in col..(col + cspan) {
                            active.insert(c, (i, j));
                        }
                    } else {
                        rows[sr][sc].rowspan += 1;
                        rows[i][j].phantom = true;
                        rows[i][j].content.clear();
                    }
                } else {
                    log::warn(&format!(
                        "'^^' in table row {} (col {}) cannot be resolved; \
                         treating as empty cell",
                        i, col
                    ));
                    rows[i][j].content.clear();
                    for c in col..(col + cspan) {
                        active.insert(c, (i, j));
                    }
                }
            } else {
                for c in col..(col + cspan) {
                    active.insert(c, (i, j));
                }
            }
            col += cspan;
        }
    }
}

/// Physical columns whose multirow would be cut by the `\hline` *after*
/// row `i`. Those columns are replaced by `\cline` ranges in the output.
fn active_columns_at_hline(rows: &[Row], i: usize, ncols: usize) -> Vec<usize> {
    let mut active = vec![false; ncols];
    for (s, row) in rows.iter().enumerate() {
        if s > i {
            break;
        }
        let mut c = 0usize;
        for cell in row {
            if !cell.phantom && cell.rowspan >= 2 && s + cell.rowspan - 1 > i {
                for k in c..(c + cell.colspan).min(ncols) {
                    active[k] = true;
                }
            }
            c += cell.colspan;
        }
    }
    active
        .iter()
        .enumerate()
        .filter(|(_, a)| **a)
        .map(|(k, _)| k)
        .collect()
}

/// Builds a `\multicolumn` column spec for `tabularx`/`xltabular` that
/// makes a cell spanning `n` columns occupy exactly the horizontal space
/// of those `n` `X`-columns:
///
///   total = n·\hsize + (n−1)·2\tabcolsep + (n−1)·\arrayrulewidth
fn multicolumn_spec(n: usize, align_cmd: &str) -> String {
    let ts = (n - 1) * 2;
    let rw = n - 1;
    format!(
        ">{{\\hsize=\\dimexpr {n}\\hsize + {ts}\\tabcolsep + {rw}\\arrayrulewidth\\relax\
          {algn}\\arraybackslash\\leavevmode}}X",
        n = n,
        ts = ts,
        rw = rw,
        algn = align_cmd,
    )
}

/// Emits one logical cell (may be colspan/rowspan/phantom).
fn emit_cell(cell: &Cell, is_first: bool) -> String {
    if cell.phantom && cell.colspan == 1 {
        return String::new();
    }

    let inner = if cell.phantom {
        String::new()
    } else if cell.rowspan > 1 {
        // `*` form centres the content naturally and respects
        // `\multirowsetup`.
        format!("\\multirow{{{}}}{{*}}{{{}}}", cell.rowspan, cell.content)
    } else {
        cell.content.clone()
    };

    if cell.colspan > 1 {
        let colspec = multicolumn_spec(cell.colspan, alignment_cmd(cell.align));
        let spec = if is_first {
            format!("|{}|", colspec)
        } else {
            format!("{}|", colspec)
        };
        format!("\\multicolumn{{{}}}{{{}}}{{{}}}", cell.colspan, spec, inner)
    } else {
        inner
    }
}

/// Emits a whole logical row as `a & b & c`.
fn emit_row(row: &Row) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(row.len());
    for (i, cell) in row.iter().enumerate() {
        parts.push(emit_cell(cell, i == 0));
    }
    parts.join(" & ")
}

/// Same as `emit_row` but wraps each logical cell's content in `\textbf`.
fn emit_header_row(row: &Row) -> String {
    let bold: Row = row
        .iter()
        .map(|c| Cell {
            colspan: c.colspan,
            rowspan: c.rowspan,
            content: if c.phantom {
                String::new()
            } else {
                format!("\\textbf{{{}}}", c.content)
            },
            phantom: c.phantom,
            align: c.align,
        })
        .collect();
    emit_row(&bold)
}

/// Emits the trailing rule for row `i`:
/// * full `\hline` if no multirow crosses it;
/// * one or more `\cline{a-b}` covering the non-crossed ranges otherwise.
fn emit_trailing_rule(out: &mut String, processed: &[Row], row_idx: usize, ncols: usize) {
    let skip = active_columns_at_hline(processed, row_idx, ncols);
    if skip.is_empty() {
        out.push_str(" \\\\\n  \\hline\n");
        return;
    }

    let skip_set: HashSet<usize> = skip.iter().copied().collect();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut k = 0usize;
    while k < ncols {
        if skip_set.contains(&k) {
            k += 1;
        } else {
            let start = k;
            while k < ncols && !skip_set.contains(&k) {
                k += 1;
            }
            ranges.push((start, k - 1));
        }
    }

    out.push_str(" \\\\\n");
    for (s, e) in ranges {
        out.push_str(&format!("  \\cline{{{}-{}}}\n", s + 1, e + 1));
    }
}

fn render_table<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    meta: Option<PendingTableCaption>,
    cfg: &Config,
    state: &mut RenderState,
) -> Result<()> {
    // Read column alignments from the table AST.
    let alignments: Vec<TableAlignment> = match &node.data.borrow().value {
        NodeValue::Table(t) => t.alignments.clone(),
        _ => Vec::new(),
    };

    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    for row_node in node.children() {
        if let NodeValue::TableRow(_) = &row_node.data.borrow().value {
            let mut cells = Vec::new();
            for cell_node in row_node.children() {
                if let NodeValue::TableCell = &cell_node.data.borrow().value {
                    // Extract the raw text first so that `^^` and `<<`
                    // aren't escaped into `\textasciicircum{}` etc.
                    let raw_text = extract_text(cell_node).trim().to_string();
                    if raw_text == "<<" || raw_text == "^^" {
                        cells.push(raw_text);
                    } else {
                        let mut cell_latex = String::new();
                        render_inline(cell_node, &mut cell_latex, cfg, state)?;
                        cells.push(cell_latex);
                    }
                }
            }
            raw_rows.push(cells);
        }
    }
    if raw_rows.is_empty() {
        return Ok(());
    }
    let ncols = raw_rows[0].len().max(1);
    let baseline = compute_baseline(&cfg.style.table_font_size);
    let double_rule = format!("\\hhline{{|{}|}}", "=".repeat(ncols));

    let (caption_latex, label_latex, breakable, new_page) = match meta {
        Some(m) => (
            escape_latex(m.caption.trim_end_matches('.')),
            m.label.unwrap_or_default(),
            m.breakable,
            m.new_page,
        ),
        None => (String::new(), String::new(), true, false),
    };

    // Fold `<<` then `^^`.
    let mut processed: Vec<Row> = raw_rows
        .iter()
        .map(|r| merge_columns(r, &alignments))
        .collect();
    fold_rowspans(&mut processed);

    if processed
        .first()
        .map_or(false, |r| r.iter().any(|c| c.rowspan > 1))
    {
        log::warn(
            "multirow in table header is not supported; \
             header will be emitted without rowspan",
        );
        for c in processed[0].iter_mut() {
            c.rowspan = 1;
        }
    }

    // Column type string: `L` / `C` / `R` per GFM alignment.
    let col_spec: String = (0..ncols)
        .map(|i| match alignments.get(i) {
            Some(TableAlignment::Left) => "L",
            Some(TableAlignment::Right) => "R",
            _ => "C",
        })
        .collect::<Vec<_>>()
        .join("|");

    if breakable {
        if new_page {
            out.push_str("\\clearpage\n");
        }
        out.push_str("  \\stepcounter{ltid}\n");
        out.push_str(&format!(
            "  \\ltSetLabels{{{}}}{{{}}}\n",
            escape_latex(&cfg.labels.table_continued),
            escape_latex(&cfg.labels.table_ended)
        ));
        out.push_str(&format!(
            "  {{\\fontsize{{{}}}{{{}}}\\selectfont\n",
            cfg.style.table_font_size, baseline
        ));
        out.push_str("  \\begin{xltabular}{\\textwidth}{|");
        out.push_str(&col_spec);
        out.push_str("|}\n");

        let mut cap_line = String::new();
        if !caption_latex.is_empty() {
            cap_line.push_str(&format!("\\caption{{{}}}", caption_latex));
        }
        if !label_latex.is_empty() {
            if !cap_line.is_empty() {
                cap_line.push(' ');
            }
            cap_line.push_str(&format!("\\label{{{}}}", label_latex));
        }
        if !cap_line.is_empty() {
            out.push_str(&format!("  {} \\\\\n", cap_line));
        }

        let header = emit_header_row(&processed[0]);

        out.push_str("  \\hline\n  ");
        out.push_str(&header);
        out.push_str(&format!(" \\\\\n  {}\n  \\endfirsthead\n", double_rule));

        out.push_str("  \\hline\n  ");
        out.push_str(&header);
        out.push_str(&format!(" \\\\\n  {}\n  \\endhead\n", double_rule));

        out.push_str("  \\endfoot\n");
        out.push_str("  \\endlastfoot\n");

        let total_data = processed.len().saturating_sub(1);
        for (data_idx, row) in processed.iter().skip(1).enumerate() {
            let source_row = data_idx + 1;
            let mut row_data = row.clone();
            if data_idx + 1 == total_data {
                if let Some(last_cell) = row_data.last_mut() {
                    if !last_cell.phantom {
                        last_cell.content.push_str("\\label{ltend:\\arabic{ltid}}");
                    }
                }
            }
            out.push_str("  ");
            out.push_str(&emit_row(&row_data));
            emit_trailing_rule(out, &processed, source_row, ncols);
        }

        out.push_str("  \\end{xltabular}\n  }\n\n");
    } else {
        out.push_str("\\begin{table}[H]\n  \\centering\n");
        if cfg.style.table_caption_above {
            if !caption_latex.is_empty() {
                out.push_str(&format!("  \\caption{{{}}}\n", caption_latex));
            }
            if !label_latex.is_empty() {
                out.push_str(&format!("  \\label{{{}}}\n", label_latex));
            }
        }
        out.push_str(&format!(
            "  {{\\fontsize{{{}}}{{{}}}\\selectfont\n",
            cfg.style.table_font_size, baseline
        ));
        out.push_str("  \\begin{tabularx}{\\textwidth}{|");
        out.push_str(&col_spec);
        out.push_str("|}\n  \\hline\n");
        out.push_str("    ");
        out.push_str(&emit_header_row(&processed[0]));
        out.push_str(&format!(" \\\\\n  {}\n", double_rule));
        for (data_idx, row) in processed.iter().skip(1).enumerate() {
            let source_row = data_idx + 1;
            out.push_str("    ");
            out.push_str(&emit_row(row));
            emit_trailing_rule(out, &processed, source_row, ncols);
        }
        out.push_str("  \\end{tabularx}\n  }\n");
        if !cfg.style.table_caption_above {
            if !caption_latex.is_empty() {
                out.push_str(&format!("  \\caption{{{}}}\n", caption_latex));
            }
            if !label_latex.is_empty() {
                out.push_str(&format!("  \\label{{{}}}\n", label_latex));
            }
        }
        out.push_str("\\end{table}\n\n");
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Text extraction & escaping helpers
// ─────────────────────────────────────────────────────────────────────────

fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    for child in node.children() {
        let v = child.data.borrow().value.clone();
        match v {
            NodeValue::Text(t) => s.push_str(&t),
            NodeValue::Code(c) => s.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => s.push(' '),
            _ => s.push_str(&extract_text(child)),
        }
    }
    s
}

fn strip_heading_number(text: &str) -> String {
    let re = Regex::new(r"^\s*\d+(\.\d+)*\.?\s+").unwrap();
    re.replace(text, "").trim().to_string()
}

fn to_title_case_if_upper(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let has_letters = trimmed.chars().any(|c| c.is_alphabetic());
    let all_upper = trimmed
        .chars()
        .filter(|c| c.is_alphabetic())
        .all(|c| c.is_uppercase());
    if !has_letters || !all_upper {
        return trimmed.to_string();
    }
    let lower = trimmed.to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => {
            let mut result = String::new();
            result.extend(first.to_uppercase());
            result.push_str(chars.as_str());
            result
        }
        None => String::new(),
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

fn escape_url(url: &str) -> String {
    url.replace('%', "\\%").replace('#', "\\#")
}

/// Escapes LaTeX special characters in plain text.
pub fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '$' => out.push_str("\\$"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\\' => out.push_str("\\textbackslash{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            _ => out.push(ch),
        }
    }
    out
}
