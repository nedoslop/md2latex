//! Build helpers: bibliography copying and timings table.

use crate::config;
use crate::log;
use anyhow::{Context, Result};
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, CellAlignment, Color, Table};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Copies the biblatex `.bib` file into `out_dir`, preserving relative
/// subpaths so the `\addbibresource{...}` path still resolves.
pub(crate) fn copy_bibliography(
    cfg: &config::Config,
    md_path: &Path,
    out_dir: &Path,
) -> Result<()> {
    if !cfg.bibliography.use_biblatex {
        return Ok(());
    }
    let rel = cfg
        .project
        .bib_file
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("refs.bib");
    let bib_src = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        md_path.parent().unwrap_or_else(|| Path::new(".")).join(rel)
    };
    if !bib_src.is_file() {
        log::warn(&format!(
            "bibliography file not found: {} (biber will fail)",
            bib_src.display()
        ));
        return Ok(());
    }
    let bib_dst = out_dir.join(rel);
    if let Some(parent) = bib_dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&bib_src, &bib_dst).with_context(|| {
        format!(
            "failed to copy {} -> {}",
            bib_src.display(),
            bib_dst.display()
        )
    })?;
    log::info(&format!(
        "bibliography: {} -> {}",
        bib_src.display(),
        bib_dst.display()
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn print_timings(
    cfg: &config::Config,
    config_time: Duration,
    images_time: Duration,
    md_time: Duration,
    render_time: Duration,
    write_time: Duration,
    pdf_time: Option<Duration>,
    total: Duration,
) {
    let mut rows: Vec<(String, Duration)> = vec![
        ("Config".to_string(), config_time),
        ("Images".to_string(), images_time),
        ("Markdown → LaTeX".to_string(), md_time),
        ("Render .tex".to_string(), render_time),
        ("Write .tex".to_string(), write_time),
    ];
    if let Some(t) = pdf_time {
        let engine = if cfg.code.enabled && cfg.code.engine.eq_ignore_ascii_case("minted") {
            "xelatex + biber + shell-escape"
        } else {
            "xelatex + biber"
        };
        rows.push((format!("Compile PDF ({})", engine), t));
    }

    let mut table = Table::new();
    table
        .load_style(UTF8_FULL.with_rounded_corners())
        .set_header(vec![
            Cell::new("Stage").fg(Color::Cyan),
            Cell::new("Time")
                .fg(Color::Cyan)
                .set_alignment(CellAlignment::Right),
        ]);
    for (label, dur) in &rows {
        table.add_row(vec![
            Cell::new(label),
            Cell::new(fmt_duration(*dur))
                .fg(Color::Green)
                .set_alignment(CellAlignment::Right),
        ]);
    }
    table.add_row(vec![
        Cell::new("Total").add_attribute(Attribute::Bold),
        Cell::new(fmt_duration(total))
            .fg(Color::Green)
            .add_attribute(Attribute::Bold)
            .set_alignment(CellAlignment::Right),
    ]);

    log::header("Timings");
    println!("{table}");
    println!();
}

pub(crate) fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 1.0 {
        format!("{:.3} s", secs)
    } else if secs >= 0.001 {
        format!("{:.1} ms", secs * 1000.0)
    } else {
        format!("{:.0} µs", secs * 1_000_000.0)
    }
}
