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
//! ```markdown
//! | H1 | H2 | H3 |
//! |----|----|----|
//! | a  | << | c  |
//! | ^^ | << | d  |
//! | e  | f  | ^^ |
//! ```
//!
//! LaTeX side:
//!
//! * merged **columns** → `\multicolumn{N}{<spec>}{...}` where `<spec>`
//!   is a tabularx-compatible `X` column with `\hsize` inflated so the
//!   merged cell occupies the same total width as the columns it
//!   replaced;
//! * merged **rows** → `\multirow{N}{*}{...}` (the `*` form uses the
//!   content's natural width and respects `\multirowsetup`);
//! * horizontal rules that would cut through a multirow are emitted as
//!   `\cline{a-b}` instead of `\hline`, skipping the affected columns.
//!
//! Requires `\usepackage{multirow}` (and preferably
//! `\renewcommand{\multirowsetup}{\centering}`) in the preamble.
//!
//! Header rowspans are not supported — a warning is emitted if the user
//! tries; use body rows to simulate multi-row headers.

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeCodeBlock, NodeLink, NodeValue};
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
    /// True for `^^` continuation cells: emit an empty cell, don't touch
    /// content.
    phantom: bool,
}

type Row = Vec<Cell>;

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
    let mut out = String::new();
    let mut ctx = Ctx::Ordinary;
    let mut pending: Option<PendingTableCaption> = None;
    let mut skip_next_section_pagebreak = false;
    let mut in_bibliography = false;

    for node in root.children() {
        render_node(
            node,
            &mut out,
            cfg,
            &mut ctx,
            &mut pending,
            &mut skip_next_section_pagebreak,
            &mut in_bibliography,
        )?;
    }
    if let Some(pt) = pending.take() {
        out.push_str(&escape_latex(&pt.caption));
        out.push_str("\n\n");
    }
    Ok(out)
}

fn render_node<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    ctx: &mut Ctx,
    pending: &mut Option<PendingTableCaption>,
    skip_next_section_pagebreak: &mut bool,
    in_bibliography: &mut bool,
) -> Result<()> {
    let value = node.data.borrow().value.clone();

    if !matches!(value, NodeValue::Table(_)) {
        if let Some(pt) = pending.take() {
            out.push_str(&escape_latex(&pt.caption));
            out.push_str("\n\n");
        }
    }

    match value {
        NodeValue::Heading(h) => {
            let raw_text = extract_text(node);
            let clean = strip_heading_number(&raw_text)
                .trim_end_matches('.')
                .to_string();

            if h.level == 1 {
                if is_structural(&clean, cfg) {
                    if cfg.style.structural_headings_newpage {
                        out.push_str("\\clearpage\n");
                    }
                    let upper = clean.trim().to_uppercase();
                    *in_bibliography =
                        upper == cfg.style.bibliography_heading.trim().to_uppercase();
                    *skip_next_section_pagebreak =
                        upper == cfg.style.main_part_heading.trim().to_uppercase();

                    let unnumbered = cfg.style.is_unnumbered_subsection(&clean);
                    *ctx = if unnumbered {
                        Ctx::StructuralUnnumbered
                    } else {
                        Ctx::StructuralNumbered
                    };
                    if is_appendix_heading(&clean, cfg) {
                        *ctx = Ctx::Appendix;
                        render_appendix_heading(&clean, out, cfg)?;
                    } else {
                        render_structural_heading(&clean, out, cfg)?;
                    }
                } else {
                    log::warn(&format!(
                        "H1 heading not in structural list: '{}' - rendering as \\section",
                        clean
                    ));
                    *in_bibliography = false;
                    *ctx = Ctx::StructuralNumbered;
                    if *skip_next_section_pagebreak {
                        *skip_next_section_pagebreak = false;
                    } else {
                        out.push_str("\\clearpage\n");
                    }
                    out.push_str(&format!(
                        "\\section{{{}}}\n\n",
                        escape_latex(&to_title_case_if_upper(&clean))
                    ));
                }
            } else {
                *in_bibliography = false;
                let is_numbered_section = h.level == 2
                    && raw_text
                        .trim()
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_ascii_digit());
                if is_numbered_section {
                    *ctx = Ctx::StructuralNumbered;
                }

                if *ctx == Ctx::StructuralUnnumbered {
                    out.push_str(&format!(
                        "\\textbf{{{}}}\\par\n\n",
                        escape_latex(&to_title_case_if_upper(&clean))
                    ));
                } else {
                    let (cmd, numbered) = match (*ctx, h.level) {
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
                        if *skip_next_section_pagebreak {
                            *skip_next_section_pagebreak = false;
                        } else {
                            out.push_str("\\clearpage\n");
                        }
                    }

                    if !numbered {
                        let toc_cmd = cmd.trim_end_matches('*');
                        out.push_str(&format!(
                            "\\phantomsection\n\\addcontentsline{{toc}}{{{}}}{{{}}}\n\\{}{{{}}}\n\n",
                            toc_cmd, escape_latex(&display), cmd, escape_latex(&display)
                        ));
                    } else {
                        out.push_str(&format!("\\{}{{{}}}\n\n", cmd, escape_latex(&display)));
                    }
                }
            }
        }
        NodeValue::Paragraph => {
            let raw_text = extract_text(node).trim().to_string();
            if raw_text.starts_with("#tbl:") {
                let after = &raw_text[5..];
                let mut label = None;
                let mut caption_tokens = Vec::new();
                let mut breakable = true;
                let mut new_page = false;

                for token in after.split_whitespace() {
                    if label.is_none()
                        && !token.contains('=')
                        && token != "breakable"
                        && token != "new_page"
                    {
                        label = Some(format!("tbl:{}", token));
                    } else if token == "breakable=false" || token == "breakable=0" {
                        breakable = false;
                    } else if token == "breakable=true"
                        || token == "breakable=1"
                        || token == "breakable"
                    {
                        breakable = true;
                    } else if token == "new_page=true"
                        || token == "new_page=1"
                        || token == "new_page"
                    {
                        new_page = true;
                    } else if token == "new_page=false" || token == "new_page=0" {
                        new_page = false;
                    } else {
                        caption_tokens.push(token);
                    }
                }
                let caption = caption_tokens.join(" ");
                *pending = Some(PendingTableCaption {
                    label,
                    caption,
                    breakable,
                    new_page,
                });
                return Ok(());
            }

            let children: Vec<_> = node.children().collect();
            if children.len() == 1 {
                if let NodeValue::Math(m) = &children[0].data.borrow().value {
                    if m.display_math {
                        out.push_str(&format!(
                            "\\begin{{equation}}\n{}\n\\end{{equation}}\n\n",
                            m.literal.trim()
                        ));
                        return Ok(());
                    }
                }
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
                        render_image_block(child, link, out, cfg)?;
                    }
                }
            } else {
                let mut p = String::new();
                render_inline(node, &mut p, cfg)?;
                if !p.trim().is_empty() {
                    out.push_str(p.trim());
                    out.push_str("\n\n");
                }
            }
        }
        NodeValue::CodeBlock(code) => {
            render_code_block(&code, out, cfg);
        }
        NodeValue::List(_) => {
            render_list(node, out, cfg, *in_bibliography)?;
        }
        NodeValue::Table(_) => {
            let meta = pending.take();
            render_table(node, out, meta, cfg)?;
        }
        NodeValue::Image(link) => render_image_block(node, &link, out, cfg)?,
        NodeValue::BlockQuote => {
            out.push_str("\\begin{quote}\n");
            for child in node.children() {
                render_node(
                    child,
                    out,
                    cfg,
                    ctx,
                    pending,
                    skip_next_section_pagebreak,
                    in_bibliography,
                )?;
            }
            out.push_str("\\end{quote}\n\n");
        }
        _ => {
            for child in node.children() {
                render_node(
                    child,
                    out,
                    cfg,
                    ctx,
                    pending,
                    skip_next_section_pagebreak,
                    in_bibliography,
                )?;
            }
        }
    }
    Ok(())
}

fn render_code_block(code: &NodeCodeBlock, out: &mut String, cfg: &Config) {
    let literal = code.literal.trim_end();
    let lang = code.info.split_whitespace().next().unwrap_or("").trim();

    if !cfg.code.enabled {
        out.push_str(&format!(
            "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n",
            literal
        ));
        return;
    }

    if cfg.code.engine.eq_ignore_ascii_case("minted") {
        let m = map_minted_lang(lang);
        out.push_str(&format!(
            "\\begin{{minted}}{{{}}}\n{}\n\\end{{minted}}\n\n",
            m, literal
        ));
    } else {
        let l = map_listings_lang(lang);
        if l.is_empty() {
            out.push_str(&format!(
                "\\begin{{lstlisting}}\n{}\n\\end{{lstlisting}}\n\n",
                literal
            ));
        } else {
            out.push_str(&format!(
                "\\begin{{lstlisting}}[language={}]\n{}\n\\end{{lstlisting}}\n\n",
                l, literal
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

fn render_list<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    in_bibliography: bool,
) -> Result<()> {
    let list_data = match &node.data.borrow().value {
        NodeValue::List(l) => l.clone(),
        _ => return Ok(()),
    };
    let is_ordered = list_data.list_type == comrak::nodes::ListType::Ordered;
    let env = if is_ordered && in_bibliography {
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
                    render_inline(block, &mut p, cfg)?;
                    out.push_str(p.trim());
                    first_block = false;
                }
                NodeValue::List(_) => {
                    out.push('\n');
                    render_list(block, out, cfg, in_bibliography)?;
                    first_block = false;
                }
                _ => {
                    if !first_block {
                        out.push('\n');
                    }
                    let mut p = String::new();
                    render_inline(block, &mut p, cfg)?;
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

    out.push_str(&format!(
        "\\clearpage\n\
         \\phantomsection\n\
         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
         \\setcounter{{figure}}{{0}}\n\
         \\setcounter{{table}}{{0}}\n\
         \\setcounter{{equation}}{{0}}\n\
         \\renewcommand{{\\thefigure}}{{{}.\\arabic{{figure}}}}\n\
         \\renewcommand{{\\thetable}}{{{}.\\arabic{{table}}}}\n\
         \\renewcommand{{\\theequation}}{{{}.\\arabic{{equation}}}}\n\
         {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n",
        escape_latex(&toc_entry),
        letter,
        letter,
        letter,
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

fn render_inline<'a>(node: &'a AstNode<'a>, out: &mut String, cfg: &Config) -> Result<()> {
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
                render_inline(child, out, cfg)?;
                out.push('}');
            }
            NodeValue::Emph => {
                out.push_str("\\textit{");
                render_inline(child, out, cfg)?;
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
                    render_inline(child, out, cfg)?;
                    out.push_str("}}");
                } else {
                    out.push_str(&format!("\\href{{{}}}{{", escape_url(&link.url)));
                    render_inline(child, out, cfg)?;
                    out.push('}');
                }
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            NodeValue::Image(link) => render_image_inline(&link, out, cfg),
            NodeValue::Superscript => {
                out.push_str("\\textsuperscript{");
                render_inline(child, out, cfg)?;
                out.push('}');
            }
            _ => render_inline(child, out, cfg)?,
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
// Table rendering with `<<` / `^^` folding.
// ─────────────────────────────────────────────────────────────────────────

/// Folds `<<`-column markers into their left neighbour.
fn merge_columns(raw: &[String]) -> Row {
    let mut result: Row = Vec::with_capacity(raw.len());
    for cell in raw {
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
                });
            }
        } else {
            result.push(Cell {
                colspan: 1,
                rowspan: 1,
                content: cell.clone(),
                phantom: false,
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

                    // Проверяем, не пытаемся ли мы присоединить `^^` к ячейке,
                    // которая уже объединена по горизонтали (`<<`), при этом
                    // справа от `^^` в текущей строке есть контент.
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
                            "'^^' in table row {} (col {}) cannot merge with a cell spanning multiple columns because the current row has content in those columns; treating as empty cell",
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
/// of those `n` `Y`-columns (`Y = >{\centering\arraybackslash}X`):
///
///   total = n·\hsize + (n−1)·2\tabcolsep + (n−1)·\arrayrulewidth
fn multicolumn_spec(n: usize) -> String {
    let ts = (n - 1) * 2;
    let rw = n - 1;
    // Исправлено: убран лишний пробел после '>' (иначе tabularx ломается),
    // и добавлен \leavevmode для корректного начала абзаца.
    format!(
        ">{{\\hsize=\\dimexpr {n}\\hsize + {ts}\\tabcolsep + {rw}\\arrayrulewidth\\relax\\centering\\arraybackslash\\leavevmode}}X",
        n = n,
        ts = ts,
        rw = rw,
    )
}

/// Emits one logical cell (may be colspan/rowspan/phantom).
fn emit_cell(cell: &Cell, is_first: bool) -> String {
    // Phantom cell with colspan 1 — just an empty cell.
    if cell.phantom && cell.colspan == 1 {
        return String::new();
    }

    let inner = if cell.phantom {
        String::new()
    } else if cell.rowspan > 1 {
        // Используем `*` вместо `=`, чтобы текст центрировался естественно.
        format!("\\multirow{{{}}}{{*}}{{{}}}", cell.rowspan, cell.content)
    } else {
        cell.content.clone()
    };

    if cell.colspan > 1 {
        let colspec = multicolumn_spec(cell.colspan);
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
) -> Result<()> {
    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    for row_node in node.children() {
        if let NodeValue::TableRow(_) = &row_node.data.borrow().value {
            let mut cells = Vec::new();
            for cell_node in row_node.children() {
                if let NodeValue::TableCell = &cell_node.data.borrow().value {
                    // Сначала извлекаем "сырой" текст, чтобы не дать
                    // escape_latex превратить `^^` в `\textasciicircum{}`.
                    let raw_text = extract_text(cell_node).trim().to_string();
                    if raw_text == "<<" || raw_text == "^^" {
                        cells.push(raw_text);
                    } else {
                        let mut cell_latex = String::new();
                        render_inline(cell_node, &mut cell_latex, cfg)?;
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
    let mut processed: Vec<Row> = raw_rows.iter().map(|r| merge_columns(r)).collect();
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
        out.push_str(&vec!["Y"; ncols].join("|"));
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
        out.push_str(&vec!["Y"; ncols].join("|"));
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

fn escape_latex(s: &str) -> String {
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