//! LaTeX escaping and text helpers shared by the renderer modules.

use comrak::nodes::{AstNode, NodeValue};

#[inline(always)]
fn special(b: u8) -> Option<&'static str> {
    match b {
        b'&' => Some("\\&"),
        b'%' => Some("\\%"),
        b'$' => Some("\\$"),
        b'#' => Some("\\#"),
        b'_' => Some("\\_"),
        b'{' => Some("\\{"),
        b'}' => Some("\\}"),
        b'~' => Some("\\textasciitilde{}"),
        b'^' => Some("\\textasciicircum{}"),
        b'\\' => Some("\\textbackslash{}"),
        _ => None,
    }
}

/// All special characters are ASCII, so byte indices are char boundaries.
pub fn escape_latex(s: &str) -> String {
    let bytes = s.as_bytes();
    if !bytes.iter().any(|&b| special(b).is_some()) {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len() + 16);
    let mut run = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(rep) = special(b) {
            out.push_str(&s[run..i]);
            out.push_str(rep);
            run = i + 1;
        }
    }
    out.push_str(&s[run..]);
    out
}

pub fn format_caption(s: &str) -> String {
    if !s.contains('`') {
        return escape_latex(s);
    }
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
