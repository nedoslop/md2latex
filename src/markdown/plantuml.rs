//! PlantUML rendering: fenced `plantuml` blocks and external `.puml`
//! files, emitted as figures. The actual Java/JAR handling lives in
//! `crate::plantuml`.

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::NodeCodeBlock;
use std::fs;
use std::path::{Path, PathBuf};

use super::images::{normalize_height_value, normalize_width_value, parse_image_title, ImageSize};
use super::state::RenderState;
use super::util::format_caption;

pub(crate) fn parse_plantuml_info(
    info: &str,
    default_width: &str,
) -> (bool, Option<String>, ImageSize, Option<String>) {
    let mut render = false;
    let mut label = None;
    let mut size = ImageSize::from_defaults(default_width);
    let mut caption_tokens = Vec::new();
    for tok in info.split_whitespace() {
        if let Some(v) = tok.strip_prefix("render=") {
            render = matches!(v.to_lowercase().as_str(), "true" | "1" | "yes" | "on");
        } else if tok == "render" {
            render = true;
        } else if let Some(w) = tok.strip_prefix("width=") {
            size.width = normalize_width_value(w);
        } else if let Some(h) = tok.strip_prefix("height=") {
            size.height = normalize_height_value(h);
            // Если явно задали только высоту, а width не трогали — сбрасываем дефолтную ширину,
            // чтобы сохранить пропорции по высоте (если width не передан отдельно).
            if !info.contains("width=") {
                size.width = None;
            }
        } else if let Some(l) = tok.strip_prefix("#fig:") {
            label = Some(format!("fig:{}", l));
        } else if let Some(l) = tok.strip_prefix("#lst:") {
            label = Some(format!("lst:{}", l));
        } else if tok.starts_with('#') && !tok.starts_with("#!") {
            log::warn(&format!("unknown plantuml info tag '{}' — ignored", tok));
        } else {
            caption_tokens.push(tok);
        }
    }
    let caption = if caption_tokens.is_empty() {
        None
    } else {
        Some(caption_tokens.join(" "))
    };
    (render, label, size, caption)
}

pub(crate) fn render_plantuml_block(
    code: &NodeCodeBlock,
    out: &mut String,
    cfg: &Config,
    state: &RenderState<'_>,
    label: Option<String>,
    size: &ImageSize,
    caption: Option<String>,
) {
    let Some(plantuml) = state.plantuml else {
        log::warn("PlantUML is not available; code block omitted");
        out.push_str(&format!(
            "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n",
            code.literal.trim_end()
        ));
        return;
    };
    match plantuml.render(&code.literal) {
        Ok(Some(png_path)) => {
            let rel_str = relative_png_path(&png_path, cfg);
            emit_plantuml_figure(
                out,
                &rel_str,
                size,
                caption.as_deref(),
                label.as_deref(),
                cfg,
            );
        }
        Ok(None) => {
            log::warn("PlantUML rendering failed; block omitted");
            out.push_str(&format!(
                "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n",
                code.literal.trim_end()
            ));
        }
        Err(e) => {
            log::warn(&format!("PlantUML error: {}", e));
            out.push_str(&format!(
                "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n",
                code.literal.trim_end()
            ));
        }
    }
}

pub(crate) fn render_plantuml_file(
    alt_text: &str,
    url: &str,
    title_raw: &str,
    out: &mut String,
    cfg: &Config,
    state: &RenderState<'_>,
) -> Result<()> {
    let (label, size, title_caption) =
        parse_image_title(title_raw, &cfg.style.default_figure_width);
    let final_caption = if !alt_text.is_empty() {
        Some(alt_text.trim_end_matches('.').to_string())
    } else {
        title_caption.map(|c| c.trim_end_matches('.').to_string())
    };
    let full_path: PathBuf = if Path::new(url).is_absolute() {
        PathBuf::from(url)
    } else {
        state.md_dir.join(url)
    };
    if !full_path.is_file() {
        log::warn(&format!(
            "PlantUML file not found: {} (link was '{}')",
            full_path.display(),
            url
        ));
        return Ok(());
    }
    let code = match fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn(&format!("failed to read {}: {}", full_path.display(), e));
            return Ok(());
        }
    };
    let Some(plantuml) = state.plantuml else {
        log::warn("PlantUML is not available; `.puml` reference skipped");
        return Ok(());
    };
    match plantuml.render(&code) {
        Ok(Some(png_path)) => {
            let rel_str = relative_png_path(&png_path, cfg);
            emit_plantuml_figure(
                out,
                &rel_str,
                &size,
                final_caption.as_deref(),
                label.as_deref(),
                cfg,
            );
        }
        Ok(None) => {
            log::warn(&format!(
                "PlantUML rendering failed for {}; skipped",
                full_path.display()
            ));
        }
        Err(e) => {
            log::warn(&format!(
                "PlantUML error for {}: {}",
                full_path.display(),
                e
            ));
        }
    }
    Ok(())
}

fn emit_plantuml_figure(
    out: &mut String,
    png_rel_path: &str,
    size: &ImageSize,
    caption: Option<&str>,
    label: Option<&str>,
    cfg: &Config,
) {
    let opts = size.to_includegraphics_opts(true, false);
    out.push_str("\\begin{figure}[H]\n\\centering\n");
    if !cfg.style.figure_caption_below {
        if let Some(c) = caption {
            out.push_str(&format!("  \\caption{{{}}}\n", format_caption(c)));
        }
        if let Some(l) = label {
            out.push_str(&format!("  \\label{{{}}}\n", l));
        }
    }
    if opts.is_empty() {
        out.push_str(&format!("  \\includegraphics{{{}}}\n", png_rel_path));
    } else {
        out.push_str(&format!(
            "  \\includegraphics[{}]{{{}}}\n",
            opts, png_rel_path
        ));
    }
    if cfg.style.figure_caption_below {
        if let Some(c) = caption {
            out.push_str(&format!("  \\caption{{{}}}\n", format_caption(c)));
        }
        if let Some(l) = label {
            out.push_str(&format!("  \\label{{{}}}\n", l));
        }
    }
    out.push_str("\\end{figure}\n");
}

pub(crate) fn is_plantuml_file(url: &str) -> bool {
    url.to_lowercase().ends_with(".puml")
}

fn relative_png_path(png_path: &Path, cfg: &Config) -> String {
    let out_dir = PathBuf::from(&cfg.project.output_dir);
    png_path
        .strip_prefix(&out_dir)
        .unwrap_or(png_path)
        .to_string_lossy()
        .replace('\\', "/")
}
