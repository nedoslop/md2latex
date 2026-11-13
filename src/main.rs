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

#[derive(Parser)]
#[command(name = "md2latex-gost")]
#[command(about = "Markdown -> LaTeX converter with GOST / STU formatting")]
struct Cli {
    #[arg(short, long, default_value = "md2latex.toml")]
    config: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::from_file(&cli.config).context("failed to load configuration")?;

    if !cfg.project.doc_type.is_empty() {
        println!("Document type: {}", cfg.project.doc_type);
    }

    let md_path = PathBuf::from(&cfg.project.input);
    let md_content = fs::read_to_string(&md_path)
        .with_context(|| format!("failed to read {}", md_path.display()))?;

    let out_dir = PathBuf::from(&cfg.project.output_dir);
    fs::create_dir_all(&out_dir)?;

    let md_prepared = images::prepare_images(
        &md_content,
        &md_path,
        &out_dir,
        cfg.images.convert_to.as_deref(),
        cfg.images.dpi,
    )
    .context("image preparation failed")?;

    let body =
        markdown::markdown_to_latex(&md_prepared, &cfg).context("Markdown conversion failed")?;

    let full_latex =
        template::render_document(&cfg, &body).context("LaTeX template rendering failed")?;

    let tex_path = out_dir.join("main.tex");
    fs::write(&tex_path, &full_latex)
        .with_context(|| format!("failed to write {}", tex_path.display()))?;
    println!("LaTeX saved: {}", tex_path.display());

    if cfg.project.compile_pdf {
        let shell_escape = cfg.code.enabled && cfg.code.engine.eq_ignore_ascii_case("minted");
        compile_pdf(&tex_path, shell_escape)?;
    }

    Ok(())
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

    println!("PDF ready: {}", dir.join("main.pdf").display());
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
