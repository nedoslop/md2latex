//! Inline node rendering: text, code, math, emphasis, links, footnote
//! references, superscripts, and the `~sub~` / `~~strike~~` extensions.

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeValue};
use regex::Regex;

use super::images::render_image_inline;
use super::state::RenderState;
use super::util::escape_latex;

pub(crate) fn render_inline<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState<'_>,
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
            NodeValue::Strong(_) => {
                out.push_str("\\textbf{");
                render_inline(child, out, cfg, state)?;
                out.push('}');
            }
            NodeValue::Emph(_) => {
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
            NodeValue::Image(link) => render_image_inline(&link, out, cfg, state),
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

fn normalize_dashes(text: &str) -> String {
    text.replace(" --- ", " — ").replace(" -- ", " — ")
}

fn escape_url(url: &str) -> String {
    url.replace('%', "\\%").replace('#', "\\#")
}
