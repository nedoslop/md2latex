//! Fenced code blocks → `lstlisting` / `minted`, with language mapping.
//! PlantUML blocks are dispatched to the `plantuml` submodule.

use crate::config::Config;
use crate::log;
use comrak::nodes::NodeCodeBlock;

use super::plantuml::{parse_plantuml_info, render_plantuml_block};
use super::state::RenderState;
use super::util::format_caption;

pub(crate) fn render_code_block(
    code: &NodeCodeBlock,
    out: &mut String,
    cfg: &Config,
    state: &RenderState<'_>,
) {
    let literal = code.literal.trim_end();
    let first_token = code.info.split_whitespace().next().unwrap_or("");
    if first_token.eq_ignore_ascii_case("plantuml") {
        let rest = code
            .info
            .split_once(char::is_whitespace)
            .map(|(_, r)| r.trim())
            .unwrap_or("");
        let (render, label, size, caption) =
            parse_plantuml_info(rest, &cfg.style.default_figure_width);
        if render {
            render_plantuml_block(code, out, cfg, state, label, &size, caption);
        } else {
            emit_listing(
                literal,
                "plantuml",
                label.as_deref(),
                caption.as_deref(),
                out,
                cfg,
            );
        }
        return;
    }
    let (lang, label, caption) = parse_code_info(&code.info);
    emit_listing(
        literal,
        lang,
        label.as_deref(),
        caption.as_deref(),
        out,
        cfg,
    );
}

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

fn emit_listing(
    literal: &str,
    lang: &str,
    label: Option<&str>,
    caption: Option<&str>,
    out: &mut String,
    cfg: &Config,
) {
    if !cfg.code.enabled {
        if label.is_some() || caption.is_some() {
            log::warn("code block has label/caption but code rendering is disabled");
        }
        out.push_str(&format!(
            "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n",
            literal
        ));
        return;
    }
    if cfg.code.engine.eq_ignore_ascii_case("minted") {
        let m = map_minted_lang(lang);
        let mut opts: Vec<String> = Vec::new();
        if let Some(c) = caption {
            opts.push(format!("caption={{{}}}", format_caption(c)));
        }
        if let Some(l) = label {
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
        if let Some(c) = caption {
            opts.push(format!("caption={{{}}}", format_caption(c)));
        }
        if let Some(lbl) = label {
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
        "plantuml" | "puml" => "PlantUML".into(),
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
