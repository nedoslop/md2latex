//! Image sizing helpers and figure / inline image rendering.

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeLink, NodeValue};

use super::plantuml::{is_plantuml_file, render_plantuml_file};
use super::state::RenderState;
use super::util::{extract_text, format_caption};

#[derive(Clone, Debug)]
pub(crate) struct ImageSize {
    /// None означает `orig` (не передавать width в \includegraphics)
    pub(crate) width: Option<String>,
    pub(crate) height: Option<String>,
}

impl ImageSize {
    pub(crate) fn from_defaults(default_width: &str) -> Self {
        Self {
            width: normalize_width_value(default_width),
            height: None,
        }
    }

    /// Формирует список опций для `\includegraphics[...]`
    pub(crate) fn to_includegraphics_opts(&self, block_mode: bool, interpolate: bool) -> String {
        let mut opts = Vec::new();
        let unit_ref = if block_mode {
            "\\textwidth"
        } else {
            "\\linewidth"
        };
        let height_ref = if block_mode {
            "\\textheight"
        } else {
            "\\linewidth"
        };
        if let Some(ref w) = self.width {
            opts.push(format!("width={}", format_dim(w, unit_ref)));
        }
        if let Some(ref h) = self.height {
            opts.push(format!("height={}", format_dim(h, height_ref)));
        }
        if self.width.is_some() && self.height.is_some() {
            opts.push("keepaspectratio=false".to_string());
        }
        if interpolate {
            opts.push("interpolate=true".to_string());
        }
        opts.join(",")
    }
}

pub(crate) fn normalize_width_value(val: &str) -> Option<String> {
    let v = val.trim();
    if v.is_empty()
        || v.eq_ignore_ascii_case("orig")
        || v.eq_ignore_ascii_case("original")
        || v.eq_ignore_ascii_case("auto")
    {
        None
    } else {
        Some(v.to_string())
    }
}

pub(crate) fn normalize_height_value(val: &str) -> Option<String> {
    let v = val.trim();
    if v.is_empty()
        || v.eq_ignore_ascii_case("orig")
        || v.eq_ignore_ascii_case("original")
        || v.eq_ignore_ascii_case("auto")
    {
        None
    } else {
        Some(v.to_string())
    }
}

/// Если передано голое число (например `0.5`), добавляет `\textwidth` / `\textheight` / `\linewidth`.
/// Если уже есть единицы (`5cm`, `100mm`, `0.5\textwidth` и т.д.), оставляет как есть.
fn format_dim(val: &str, default_unit: &str) -> String {
    if val.parse::<f64>().is_ok() {
        format!("{}{}", val, default_unit)
    } else {
        val.to_string()
    }
}

pub(crate) fn render_image_block<'a>(
    node: &'a AstNode<'a>,
    link: &NodeLink,
    out: &mut String,
    cfg: &Config,
    state: &RenderState<'_>,
) -> Result<()> {
    if is_plantuml_file(&link.url) {
        let alt = extract_text(node).trim().to_string();
        return render_plantuml_file(&alt, &link.url, &link.title, out, cfg, state);
    }
    let src = &link.url;
    let title_raw = link.title.clone();
    let (label, size, title_caption) =
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
    let opts = size.to_includegraphics_opts(true, cfg.images.interpolate);
    out.push_str("\\begin{figure}[H]\n\\centering\n");
    if !cfg.style.figure_caption_below {
        if let Some(ref c) = final_caption {
            out.push_str(&format!("  \\caption{{{}}}\n", format_caption(c)));
        }
        if let Some(ref l) = label {
            out.push_str(&format!("  \\label{{{}}}\n", l));
        }
    }
    if opts.is_empty() {
        out.push_str(&format!("  \\includegraphics{{{}}}\n", src));
    } else {
        out.push_str(&format!(
            "  \\includegraphics[{}]{{{}}}\n",
            opts, src
        ));
    }
    if cfg.style.figure_caption_below {
        if let Some(ref c) = final_caption {
            out.push_str(&format!("  \\caption{{{}}}\n", format_caption(c)));
        }
        if let Some(ref l) = label {
            out.push_str(&format!("  \\label{{{}}}\n", l));
        }
    }
    out.push_str("\\end{figure}\n");
    Ok(())
}

pub(crate) fn render_image_inline(
    link: &NodeLink,
    out: &mut String,
    cfg: &Config,
    state: &RenderState<'_>,
) {
    if is_plantuml_file(&link.url) {
        if state.skip_puml_images {
            return;
        }
        log::warn(&format!(
            "PlantUML diagram '{}' in inline context — move it to its own \
             paragraph to render as a figure",
            link.url
        ));
        return;
    }
    let src = &link.url;
    let (_, size, _) = parse_image_title(&link.title, "1.0");
    let opts = size.to_includegraphics_opts(false, cfg.images.interpolate);
    if opts.is_empty() {
        out.push_str(&format!("\\includegraphics{{{}}}", src));
    } else {
        out.push_str(&format!("\\includegraphics[{}]{{{}}}", opts, src));
    }
}

pub(crate) fn parse_image_title(
    title: &str,
    default_width: &str,
) -> (Option<String>, ImageSize, Option<String>) {
    let mut rest = title.trim().to_string();
    let mut label = None;
    let mut size = ImageSize::from_defaults(default_width);
    let mut has_explicit_width = false;
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
            size.width = normalize_width_value(w);
            has_explicit_width = true;
        } else if let Some(h) = token.strip_prefix("height=") {
            size.height = normalize_height_value(h);
            if !has_explicit_width {
                size.width = None;
            }
        } else {
            caption_tokens.push(token.to_string());
        }
    }
    let caption = if caption_tokens.is_empty() {
        None
    } else {
        Some(caption_tokens.join(" "))
    };
    (label, size, caption)
}