//! Command-line entry point: loads config, prepares images, converts
//! Markdown to LaTeX, renders the final `.tex`, and optionally compiles it
//! with `xelatex`.

mod config;
mod images;
mod markdown;
mod template;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "md2latex-gost")]
#[command(about = "Markdown -> LaTeX converter with GOST / STU formatting")]
struct Cli {
    #[arg(short, long, default_value = "md2latex.toml")]
    config: PathBuf,
}

fn main() -> Result<()> {
    let total_start = Instant::now();

    let cli = Cli::parse();

    let t = Instant::now();
    let cfg = config::Config::from_file(&cli.config).context("failed to load configuration")?;
    let config_time = t.elapsed();

    if !cfg.project.profile.is_empty() {
        println!("Profile:       {}", cfg.project.profile);
    }
    if !cfg.project.doc_type.is_empty() {
        println!("Document type: {}", cfg.project.doc_type);
    }

    let t = Instant::now();
    let md_path = PathBuf::from(&cfg.project.input);
    let md_content = fs::read_to_string(&md_path)
        .with_context(|| format!("failed to read {}", md_path.display()))?;
    let read_time = t.elapsed();

    let out_dir = PathBuf::from(&cfg.project.output_dir);
    fs::create_dir_all(&out_dir)?;

    let t = Instant::now();
    let md_prepared = images::prepare_images(
        &md_content,
        &md_path,
        &out_dir,
        cfg.images.convert_to.as_deref(),
        cfg.images.dpi,
    )
    .context("image preparation failed")?;
    let images_time = t.elapsed();

    let t = Instant::now();
    let body =
        markdown::markdown_to_latex(&md_prepared, &cfg).context("Markdown conversion failed")?;
    let md_time = t.elapsed();

    let t = Instant::now();
    let full_latex =
        template::render_document(&cfg, &body).context("LaTeX template rendering failed")?;
    let render_time = t.elapsed();

    let t = Instant::now();
    let tex_path = out_dir.join("main.tex");
    fs::write(&tex_path, &full_latex)
        .with_context(|| format!("failed to write {}", tex_path.display()))?;
    let write_time = t.elapsed();
    println!("LaTeX saved:   {}", tex_path.display());

    let pdf_time = if cfg.project.compile_pdf {
        let shell_escape = cfg.code.enabled && cfg.code.engine.eq_ignore_ascii_case("minted");
        let t = Instant::now();
        compile_pdf(&tex_path, shell_escape)?;
        Some(t.elapsed())
    } else {
        None
    };

    print_timings(
        &cfg,
        config_time,
        read_time,
        images_time,
        md_time,
        render_time,
        write_time,
        pdf_time,
        total_start.elapsed(),
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_timings(
    cfg: &config::Config,
    config_time: Duration,
    read_time: Duration,
    images_time: Duration,
    md_time: Duration,
    render_time: Duration,
    write_time: Duration,
    pdf_time: Option<Duration>,
    total: Duration,
) {
    println!();
    println!("Timings:");
    println!("  config:        {:>10}", fmt_duration(config_time));
    println!("  read markdown: {:>10}", fmt_duration(read_time));
    println!("  images:        {:>10}", fmt_duration(images_time));
    println!("  md → latex:    {:>10}", fmt_duration(md_time));
    println!("  render .tex:   {:>10}", fmt_duration(render_time));
    println!("  write .tex:    {:>10}", fmt_duration(write_time));
    if let Some(t) = pdf_time {
        let engine = if cfg.code.enabled && cfg.code.engine.eq_ignore_ascii_case("minted") {
            "xelatex (+ biber, shell-escape)"
        } else {
            "xelatex (+ biber)"
        };
        println!("  compile pdf:   {:>10}  ({})", fmt_duration(t), engine);
    }
    println!("  ─────────────────────────");
    println!("  total:         {:>10}", fmt_duration(total));
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 1.0 {
        format!("{:.3} s", secs)
    } else if secs >= 0.001 {
        format!("{:.1} ms", secs * 1000.0)
    } else {
        format!("{:.0} µs", secs * 1_000_000.0)
    }
}

/// Runs `xelatex` (and `biber` if needed), then `xelatex` twice more to
/// stabilise the table of contents and cross-references.
fn compile_pdf(tex_path: &Path, shell_escape: bool) -> Result<()> {
    let dir = tex_path.parent().unwrap();
    println!("running xelatex...");

    let mut args: Vec<&str> = vec!["-interaction=nonstopmode", "-halt-on-error"];
    if shell_escape {
        args.push("-shell-escape");
    }
    args.push("main.tex");

    run_latex(dir, &args)?;

    if tex_path.with_extension("bcf").exists() {
        println!("running biber...");
        Command::new("biber")
            .arg("main")
            .current_dir(dir)
            .status()
            .context("biber not found or failed")?;
    }

    run_latex(dir, &args)?;
    // Third pass stabilises page numbers in the ToC and \ref{} when the
    // ToC is long.
    run_latex(dir, &args)?;

    println!("PDF ready:     {}", dir.join("main.pdf").display());
    Ok(())
}

fn run_latex(dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("xelatex")
        .args(args)
        .current_dir(dir)
        .status()
        .context("xelatex not found. Please install TeX Live / MiKTeX.")?;
    if !status.success() {
        anyhow::bail!("xelatex exited with an error");
    }
    Ok(())
}
