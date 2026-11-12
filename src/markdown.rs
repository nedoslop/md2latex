use crate::config::Config;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeLink, NodeValue};
use comrak::{parse_document, Arena, ComrakOptions};
use regex::Regex;

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    Ordinary,
    StructuralNumbered,
    StructuralUnnumbered,
}

struct PendingTableCaption {
    label: Option<String>,
    caption: String,
}

pub fn markdown_to_latex(md: &str, cfg: &Config) -> Result<String> {
    let arena = Arena::new();
    let mut options = ComrakOptions::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;

    let root = parse_document(&arena, md, &options);
    let mut out = String::new();
    let mut ctx = Ctx::Ordinary;
    let mut pending: Option<PendingTableCaption> = None;

    for node in root.children() {
        render_node(node, &mut out, cfg, &mut ctx, &mut pending)?;
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
) -> Result<()> {
    let value = node.data.borrow().value.clone();

    // Если была подпись таблицы, а сейчас не таблица — сбрасываем как обычный текст
    if !matches!(value, NodeValue::Table(_)) {
        if let Some(pt) = pending.take() {
            out.push_str(&escape_latex(&pt.caption));
            out.push_str("\n\n");
        }
    }

    match value {
        NodeValue::Heading(h) => {
            let clean = strip_heading_number(&extract_text(node));
            if h.level == 1 {
                if is_structural(&clean, cfg) {
                    if cfg.style.structural_headings_newpage {
                        out.push_str("\\clearpage\n");
                    }
                    let unnumbered = cfg.style.is_unnumbered_subsection(&clean);
                    *ctx = if unnumbered {
                        Ctx::StructuralUnnumbered
                    } else {
                        Ctx::StructuralNumbered
                    };
                    let upper = clean.trim().to_uppercase();
                    // Печатаем ВСЕ структурные заголовки, включая ОСНОВНАЯ ЧАСТЬ
                    out.push_str(&format!(
                        "\\phantomsection\n\
                         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
                         \\begin{{center}}\\textbf{{{}}}\\end{{center}}\n\
                         \\vspace{{12pt}}\n\n",
                        escape_latex(&upper),
                        escape_latex(&upper)
                    ));
                } else {
                    *ctx = Ctx::Ordinary;
                    out.push_str("\\clearpage\n");
                    let upper = clean.trim().to_uppercase();
                    out.push_str(&format!("\\section{{{}}}\n\n", escape_latex(&upper)));
                }
            } else {
                let (cmd_star, numbered, is_top_section) = match (*ctx, h.level) {
                    (Ctx::StructuralUnnumbered, 2) => ("section*", false, false),
                    (Ctx::StructuralUnnumbered, 3) => ("subsection*", false, false),
                    (Ctx::StructuralUnnumbered, _) => ("subsubsection*", false, false),
                    (Ctx::StructuralNumbered, 2) => ("section", true, true),
                    (Ctx::StructuralNumbered, 3) => ("subsection", true, false),
                    (Ctx::StructuralNumbered, _) => ("subsubsection", true, false),
                    (Ctx::Ordinary, 2) => ("subsection", true, false),
                    (Ctx::Ordinary, 3) => ("subsubsection", true, false),
                    (Ctx::Ordinary, _) => ("paragraph", true, false),
                };

                if is_top_section {
                    out.push_str("\\clearpage\n");
                    let upper = clean.trim().to_uppercase();
                    out.push_str(&format!("\\section{{{}}}\n\n", escape_latex(&upper)));
                } else {
                    let display = to_title_case_if_upper(&clean);
                    if !numbered {
                        let toc_cmd = cmd_star.trim_end_matches('*');
                        out.push_str(&format!(
                            "\\phantomsection\n\
                             \\addcontentsline{{toc}}{{{}}}{{{}}}\n\
                             \\{}{{{}}}\n\n",
                            toc_cmd,
                            escape_latex(&display),
                            cmd_star,
                            escape_latex(&display)
                        ));
                    } else {
                        out.push_str(&format!("\\{}{{{}}}\n\n", cmd_star, escape_latex(&display)));
                    }
                }
            }
        }
        NodeValue::Paragraph => {
            let raw_text = extract_text(node).trim().to_string();
            if raw_text.starts_with("#tbl:") {
                let after = &raw_text[5..];
                let (label, caption) = if let Some(sp) = after.find(char::is_whitespace) {
                    (
                        Some(format!("tbl:{}", &after[..sp])),
                        after[sp..].trim().to_string(),
                    )
                } else {
                    (Some(format!("tbl:{}", after)), String::new())
                };
                *pending = Some(PendingTableCaption { label, caption });
                return Ok(());
            }

            // Блочная формула $$...$$, стоящая отдельным абзацем
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
                        render_image(child, link, out, cfg)?;
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
            out.push_str(&format!(
                "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n",
                code.literal.trim_end()
            ));
        }
        NodeValue::List(list) => {
            let env = if list.list_type == comrak::nodes::ListType::Ordered {
                "enumerate"
            } else {
                "itemize"
            };
            out.push_str(&format!("\\begin{{{}}}\n", env));
            for child in node.children() {
                if let NodeValue::Item(_) = &child.data.borrow().value {
                    let mut item = String::new();
                    for (idx, block) in child.children().enumerate() {
                        if idx > 0 {
                            item.push(' ');
                        }
                        render_inline(block, &mut item, cfg)?;
                    }
                    out.push_str(&format!("  \\item {}\n", item.trim()));
                }
            }
            out.push_str(&format!("\\end{{{}}}\n\n", env));
        }
        NodeValue::Table(_) => {
            let meta = pending.take();
            render_table(node, out, meta, cfg)?;
        }
        NodeValue::Image(link) => render_image(node, &link, out, cfg)?,
        NodeValue::BlockQuote => {
            out.push_str("\\begin{quote}\n");
            for child in node.children() {
                render_node(child, out, cfg, ctx, pending)?;
            }
            out.push_str("\\end{quote}\n\n");
        }
        _ => {
            for child in node.children() {
                render_node(child, out, cfg, ctx, pending)?;
            }
        }
    }
    Ok(())
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

fn render_inline<'a>(node: &'a AstNode<'a>, out: &mut String, cfg: &Config) -> Result<()> {
    for child in node.children() {
        let v = child.data.borrow().value.clone();
        match v {
            NodeValue::Text(t) => out.push_str(&escape_latex(&t)),
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
                out.push_str(&format!("\\href{{{}}}{{", escape_url(&link.url)));
                render_inline(child, out, cfg)?;
                out.push('}');
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            NodeValue::Image(link) => render_image(child, &link, out, cfg)?,
            _ => render_inline(child, out, cfg)?,
        }
    }
    Ok(())
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

fn render_image<'a>(
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
        Some(alt_text)
    } else {
        title_caption
    };

    let width_spec =
        if width.contains("\\textwidth") || width.contains("\\linewidth") || width.contains('%') {
            width.clone()
        } else {
            format!("{}\\textwidth", width)
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
        "  \\includegraphics[width={}]{{{}}}\n",
        width_spec, src
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

fn render_table<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    meta: Option<PendingTableCaption>,
    cfg: &Config,
) -> Result<()> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row_node in node.children() {
        if let NodeValue::TableRow(_) = &row_node.data.borrow().value {
            let mut cells = Vec::new();
            for cell_node in row_node.children() {
                if let NodeValue::TableCell = &cell_node.data.borrow().value {
                    let mut cell_latex = String::new();
                    render_inline(cell_node, &mut cell_latex, cfg)?;
                    cells.push(cell_latex);
                }
            }
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return Ok(());
    }
    let ncols = rows[0].len().max(1);

    out.push_str("\\begin{table}[H]\n  \\centering\n");
    if cfg.style.table_caption_above {
        if let Some(ref m) = meta {
            if !m.caption.is_empty() {
                out.push_str(&format!("  \\caption{{{}}}\n", escape_latex(&m.caption)));
            }
            if let Some(ref l) = m.label {
                out.push_str(&format!("  \\label{{{}}}\n", l));
            }
        }
    }
    out.push_str("  {\\small\n");
    out.push_str("  \\begin{tabularx}{\\textwidth}{|");
    out.push_str(&vec!["Y"; ncols].join("|"));
    out.push_str("|}\n  \\hline\n");
    for (i, row) in rows.iter().enumerate() {
        if i == 0 {
            let bold: Vec<String> = row.iter().map(|c| format!("\\textbf{{{}}}", c)).collect();
            out.push_str("    ");
            out.push_str(&bold.join(" & "));
            out.push_str(" \\\\\n  \\hline\\hline\n");
        } else {
            out.push_str("    ");
            out.push_str(&row.join(" & "));
            out.push_str(" \\\\\n  \\hline\n");
        }
    }
    out.push_str("  \\end{tabularx}\n  }\n");
    if !cfg.style.table_caption_above {
        if let Some(ref m) = meta {
            if !m.caption.is_empty() {
                out.push_str(&format!("  \\caption{{{}}}\n", escape_latex(&m.caption)));
            }
            if let Some(ref l) = m.label {
                out.push_str(&format!("  \\label{{{}}}\n", l));
            }
        }
    }
    out.push_str("\\end{table}\n\n");
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

fn is_structural(text: &str, cfg: &Config) -> bool {
    let t = text.trim().to_uppercase();
    for h in cfg.style.structural_list() {
        let up = h.trim().to_uppercase();
        if t == up {
            return true;
        }
        if up == "ПРИЛОЖЕНИЕ" && t.starts_with("ПРИЛОЖЕНИЕ ") {
            return true;
        }
    }
    false
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
            _ => out.push(ch),
        }
    }
    out
}
