//! Inline node rendering: text, code, math, emphasis, links, footnote
//! references, superscripts, and the `~sub~` / `~~strike~~` extensions.
//!
//! ## Упрощённый синтаксис ссылок на метки
//!
//! В inline-тексте (но не в code-span) распознаются и корректно
//! превращаются в LaTeX:
//!
//! * `\ref{tbl:x}`, `\ref{fig:x}`, `\ref{sec:x}`, `\ref{app:x}`,
//!   `\ref{lst:x}`, `\ref{eq:x}`;
//! * `\eqref{eq:x}`, `\autoref{x}`, `\cref{x}`, `\pageref{x}`;
//! * `{tbl:x}`, `{fig:x}`, `{eq:x}`, `{sec:x}`, … — если
//!   `references.enabled = true` и `references.syntax` включает `braces`;
//! * `@tbl:x`, `@fig:x`, `@eq:x`, … — если `references.syntax` включает `at`.
//!
//! Какой командой печатать конкретный префикс, задаётся в
//! `[references.commands]`; по умолчанию — `references.default_command`.
//!
//! ```toml
//! [references]
//! enabled         = true
//! syntax          = "braces"        # braces | at | both
//! default_command = "\\ref"
//! prefixes        = ["eq", "fig", "tbl", "lst", "sec", "app"]
//!
//! [references.commands]
//! eq  = "\\eqref"
//! fig = "\\ref"
//! tbl = "\\ref"
//! lst = "\\ref"
//! ```
//!
//! `labels.equation_ref_parens = true` (по умолчанию) **принудительно**
//! выводит `\eqref{...}` для меток `eq:`, даже если `references.commands.eq`
//! задан иначе — то есть «скобки вокруг номера формулы» включаются
//! автоматически. При `false` — наоборот, `\ref{...}` без скобок.
//!
//! В **code-span** (`...`) новый синтаксис `{label}` / `@label` не
//! применяется — там ожидается «сырой LaTeX». Но `\ref{eq:x}` ⇄
//! `\eqref{eq:x}` внутри code-span работает согласно
//! `equation_ref_parens` (историческое поведение, см. `process_raw_latex_refs`).

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
            NodeValue::Text(t) => out.push_str(&render_text_node(&t, cfg)),
            NodeValue::Code(c) => {
                if c.literal.starts_with('\\') {
                    out.push_str(&process_raw_latex_refs(&c.literal, cfg));
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

// ─────────────────────────────────────────────────────────────────────────
// Упрощённые ссылки на метки
// ─────────────────────────────────────────────────────────────────────────

enum Piece {
    Text(String),
    Latex(String),
}

fn render_text_node(text: &str, cfg: &Config) -> String {
    let normalized = normalize_dashes(text);
    let pieces = split_references(&normalized, cfg);
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(s) => out.push_str(&render_plain_text(&s)),
            Piece::Latex(s) => out.push_str(&s),
        }
    }
    out
}

fn split_references(text: &str, cfg: &Config) -> Vec<Piece> {
    // Собираем интервалы, которые нужно заменить готовым LaTeX-фрагментом.
    let mut matches: Vec<(usize, usize, String)> = Vec::new();

    // 1. Полные LaTeX-ссылки: \ref{...}, \eqref{...}, \autoref{...},
    //    \cref{...}, \pageref{...}. Их нельзя escape-ить — иначе вывод
    //    превратится в \textbackslash{}ref\{...\}.
    let latex_ref_re = Regex::new(r"\\(eqref|ref|autoref|cref|pageref)\{([a-zA-Z0-9_\-:]+)\}")
        .expect("latex-reference regex is valid");
    for caps in latex_ref_re.captures_iter(text) {
        let full = caps.get(0).unwrap();
        let cmd = &caps[1];
        let label = &caps[2];
        let prefix = label.split(':').next().unwrap_or("");

        // `equation_ref_parens` переопределяет команду только для eq-меток
        // и только для \ref / \eqref.
        let latex = if prefix == "eq" && (cmd == "ref" || cmd == "eqref") {
            if cfg.labels.equation_ref_parens {
                format!("\\eqref{{{}}}", label)
            } else {
                format!("\\ref{{{}}}", label)
            }
        } else {
            format!("\\{}{{{}}}", cmd, label)
        };
        matches.push((full.start(), full.end(), latex));
    }

    if cfg.references.enabled {
        // 2. {label}
        let use_braces = matches!(cfg.references.syntax.as_str(), "braces" | "both");
        if use_braces {
            let re = Regex::new(r"\{([a-zA-Z][a-zA-Z0-9_\-]*:[a-zA-Z0-9_\-:]+)\}")
                .expect("brace-reference regex is valid");
            for caps in re.captures_iter(text) {
                let full = caps.get(0).unwrap();
                if overlaps(&matches, full.start(), full.end()) {
                    continue;
                }
                if let Some(latex) = label_to_latex(&caps[1], cfg) {
                    matches.push((full.start(), full.end(), latex));
                }
            }
        }
        // 3. @label
        let use_at = matches!(cfg.references.syntax.as_str(), "at" | "both");
        if use_at {
            let re = Regex::new(r"@([a-zA-Z][a-zA-Z0-9_\-]*:[a-zA-Z0-9_\-:]+)")
                .expect("at-reference regex is valid");
            for caps in re.captures_iter(text) {
                let full = caps.get(0).unwrap();
                if overlaps(&matches, full.start(), full.end()) {
                    continue;
                }
                if let Some(latex) = label_to_latex(&caps[1], cfg) {
                    matches.push((full.start(), full.end(), latex));
                }
            }
        }
    }

    // Сортируем и убираем пересечения (оставляем первый — он уже
    // проверен на overlaps, но сортировка нужна для корректной склейки).
    matches.sort_by_key(|(s, _, _)| *s);
    let mut filtered: Vec<(usize, usize, String)> = Vec::new();
    let mut last_end = 0;
    for (s, e, l) in matches {
        if s >= last_end {
            filtered.push((s, e, l));
            last_end = e;
        }
    }

    let mut pieces = Vec::new();
    let mut pos = 0;
    for (s, e, latex) in filtered {
        if s > pos {
            pieces.push(Piece::Text(text[pos..s].to_string()));
        }
        pieces.push(Piece::Latex(latex));
        pos = e;
    }
    if pos < text.len() {
        pieces.push(Piece::Text(text[pos..].to_string()));
    }
    pieces
}

fn overlaps(matches: &[(usize, usize, String)], s: usize, e: usize) -> bool {
    matches.iter().any(|(ms, me, _)| s < *me && e > *ms)
}

fn label_to_latex(label: &str, cfg: &Config) -> Option<String> {
    let prefix = label.split(':').next()?;
    if !cfg.references.prefixes.iter().any(|p| p == prefix) {
        return None;
    }
    // Автоматические скобки вокруг номера формулы.
    if prefix == "eq" && cfg.labels.equation_ref_parens {
        return Some(format!("\\eqref{{{}}}", label));
    }
    let cmd = cfg
        .references
        .commands
        .get(prefix)
        .cloned()
        .unwrap_or_else(|| cfg.references.default_command.clone());
    Some(format!("{}{{{}}}", cmd, label))
}

// ─────────────────────────────────────────────────────────────────────────
// Code-span: \ref{eq:x} ⇄ \eqref{eq:x}
// ─────────────────────────────────────────────────────────────────────────

fn process_raw_latex_refs(literal: &str, cfg: &Config) -> String {
    if cfg.labels.equation_ref_parens {
        let re = Regex::new(r"\\ref\{eq:([^}]+)\}").unwrap();
        re.replace_all(literal, |caps: &regex::Captures| {
            format!("\\eqref{{eq:{}}}", &caps[1])
        })
        .into_owned()
    } else {
        let re = Regex::new(r"\\eqref\{eq:([^}]+)\}").unwrap();
        re.replace_all(literal, |caps: &regex::Captures| {
            format!("\\ref{{eq:{}}}", &caps[1])
        })
        .into_owned()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Plain text helpers
// ─────────────────────────────────────────────────────────────────────────

fn render_plain_text(text: &str) -> String {
    let re = Regex::new(r"https?://[^\s\)\]\}]+").unwrap();
    let mut out = String::new();
    let mut last = 0;
    for m in re.find_iter(text) {
        out.push_str(&parse_tildes(&text[last..m.start()]));
        out.push_str(&format!("\\url{{{}}}", m.as_str()));
        last = m.end();
    }
    out.push_str(&parse_tildes(&text[last..]));
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
