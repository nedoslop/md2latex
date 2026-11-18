//! LaTeX escaping and text helpers shared by the renderer modules.

use comrak::nodes::{AstNode, NodeValue};

pub fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '\\' => out.push_str("\\textbackslash{}"),
            _ => out.push(c),
        }
    }
    out
}

pub fn format_caption(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut buf = String::new();
    let mut in_code = false;
    for c in s.chars() {
        if c == '`' {
            if in_code {
                if buf.starts_with('\\') {
                    out.push_str(&buf);
                } else {
                    out.push('`');
                    out.push_str(&escape_latex(&buf));
                    out.push('`');
                }
                buf.clear();
                in_code = false;
            } else {
                out.push_str(&escape_latex(&buf));
                buf.clear();
                in_code = true;
            }
        } else {
            buf.push(c);
        }
    }
    if in_code {
        out.push('`');
        out.push_str(&escape_latex(&buf));
    } else {
        out.push_str(&escape_latex(&buf));
    }
    out
}

pub(crate) fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    for c in node.descendants() {
        if let NodeValue::Text(t) = &c.data.borrow().value {
            s.push_str(t);
        }
    }
    s
}

pub(crate) fn compute_baseline(font_size: &str) -> String {
    let num: f64 = font_size
        .trim_end_matches("pt")
        .trim_end_matches("PT")
        .parse()
        .unwrap_or(12.0);
    format!("{:.1}pt", num * 1.2)
}