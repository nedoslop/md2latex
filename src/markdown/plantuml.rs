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

fn render_flag(tok: &str) -> Option<bool> {
    if tok == "render" {
        return Some(true);
    }
    tok.strip_prefix("render=")
        .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes" | "on"))
}

/// Cheap, warning-free probe used by the collection pre-pass.
pub(crate) fn info_requests_render(info: &str) -> bool {
    let mut render = false;
    for tok in info.split_whitespace() {
        if let Some(v) = render_flag(tok) {
            render = v;
        }
    }
    render
}

pub(crate) fn parse_plantuml_info(
    info: &str,
    default_width: &str,
) -> (bool, Option<String>, ImageSize, Option<String>) {
    let mut render = false;
    let mut label = None;
    let mut size = ImageSize::from_defaults(default_width);
    let mut caption_tokens = Vec::new();
    for tok in info.split_whitespace() {
        if let Some(v) = render_flag(tok) {
            render = v;
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
            "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n",
            code.literal.trim_end()
        ));
        return;
    };
    match plantuml.render(&code.literal) {
        Ok(Some(rendered)) => {
            emit_plantuml_figure(
                out,
                &rendered.png_path,
                rendered.dpi,
                size,
                caption.as_deref(),
                label.as_deref(),
                cfg,
            );
        }
        Ok(None) => {
            log::warn("PlantUML rendering failed; block omitted");
            out.push_str(&format!(
                "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n",
                code.literal.trim_end()
            ));
        }
        Err(e) => {
            log::warn(&format!("PlantUML error: {}", e));
            out.push_str(&format!(
                "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n",
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
        Ok(Some(rendered)) => {
            emit_plantuml_figure(
                out,
                &rendered.png_path,
                rendered.dpi,
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

/// Emits a rendered diagram as a `figure`.
///
/// `dpi` is the density the PNG was **actually rendered at**: it comes
/// from `PlantUml::render`, which passes `-Sdpi=N` on the command line
/// (`plantuml.dpi` from config, overridden by `skinparam dpi` in the
/// source). Nothing is guessed from PNG metadata.
fn emit_plantuml_figure(
    out: &mut String,
    png_path: &Path,
    dpi: u32,
    size: &ImageSize,
    caption: Option<&str>,
    label: Option<&str>,
    cfg: &Config,
) {
    let png_rel_path = relative_png_path(png_path, cfg);

    // `width=orig` (no width and no height): pin the natural physical
    // size explicitly. XeTeX assumes 72 dpi for PNGs, which would
    // inflate the image by dpi/72 — 1.33× at 96 dpi, 4.17× at 300.
    let mut size = size.clone();
    if size.width.is_none() && size.height.is_none() {
        if let Some(w) = natural_width_bp(png_path, dpi) {
            size.width = Some(format!("{:.2}bp", w));
        } else {
            log::warn(&format!(
                "cannot determine natural size of {}; using engine default",
                png_path.display()
            ));
        }
    }

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
    out.push_str("\\end{figure}\n\n");
}

/// Natural physical width in big points (1bp = 1/72 in), from the pixel
/// width and the known render density. Exact — no `pHYs` round-trip.
fn natural_width_bp(png_path: &Path, dpi: u32) -> Option<f64> {
    let (w_px, _) = image::image_dimensions(png_path).ok()?;
    if w_px == 0 {
        return None;
    }
    Some(w_px as f64 * 72.0 / dpi.max(1) as f64)
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
