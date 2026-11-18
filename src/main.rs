//! Command-line entry point: loads config from the Markdown frontmatter,
//! prepares images, converts Markdown to LaTeX, renders the final `.tex`,
//! and optionally compiles it with `xelatex`.
//!
//! External tools (`xelatex`, `biber`) run **quietly by default**: their
//! stdout/stderr is captured and dumped to the console only if they fail.
//! Pass `--verbose` to stream the full log live.

mod build;
mod compile;
mod config;
mod images;
mod log;
mod markdown;
mod plantuml;
mod template;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "md2latex-gost")]
#[command(about = "Markdown -> LaTeX converter with GOST / STU formatting")]
struct Cli {
    /// Input Markdown file (its YAML frontmatter selects the profile,
    /// document type, title-page fields, etc.)
    input: PathBuf,
    /// Output directory for `main.tex` (and `main.pdf` when `--compile` is set).
    #[arg(short, long, default_value = "out")]
    output_dir: PathBuf,
    /// Directory that contains institution profiles. Overrides
    /// `$MD2LATEX_PROFILES_DIR`. When omitted, the lookup tries
    /// `$MD2LATEX_PROFILES_DIR`, `<md_dir>/profiles`, `<cwd>/profiles`.
    #[arg(long)]
    profiles_dir: Option<PathBuf>,
    /// Compile the generated `.tex` to PDF with xelatex (+ biber).
    #[arg(long)]
    compile: bool,
    /// Stream xelatex/biber output to the console. Without this flag,
    /// external tools run quietly; their captured output is printed
    /// only if they fail.
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let total_start = Instant::now();
    let cli = Cli::parse();

    log::header("md2latex-gost");
    let md_path = cli.input.clone();
    let md_content = fs::read_to_string(&md_path)
        .with_context(|| format!("failed to read {}", md_path.display()))?;

    // ── Config: frontmatter (MD) + profile.toml + CLI ───────────────────
    let t = Instant::now();
    let (cfg, md_body) = config::Config::from_markdown(
        &md_path,
        &md_content,
        config::CliOverrides {
            output_dir: cli.output_dir.clone(),
            compile_pdf: cli.compile,
            profiles_dir: cli.profiles_dir.clone(),
        },
    )
    .context("failed to load configuration")?;
    let config_time = t.elapsed();

    log::field("Profile", &cfg.project.profile);
    log::field("Document type", &cfg.project.doc_type);
    log::field("Input", &md_path.display().to_string());
    log::field("Output", &cfg.project.output_dir);

    let t = Instant::now();
    let out_dir = PathBuf::from(&cfg.project.output_dir);
    fs::create_dir_all(&out_dir)?;

    let md_prepared = images::prepare_images(
        &md_body,
        &md_path,
        &out_dir,
        cfg.images.convert_to.as_deref(),
        cfg.images.dpi,
    )
    .context("image preparation failed")?;
    build::copy_bibliography(&cfg, &md_path, &out_dir).context("bibliography copy failed")?;
    let images_time = t.elapsed();

    // ── PlantUML: подготовка JAR и рендерера ─────────────────────────────
    let plantuml =
        plantuml::PlantUml::new(&cfg, &out_dir).context("failed to initialise PlantUML")?;

    let t = Instant::now();
    let md_dir = md_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let body = markdown::markdown_to_latex(&md_prepared, &cfg, &md_dir, plantuml.as_ref())
        .context("Markdown conversion failed")?;
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

    log::success(&format!("LaTeX saved:   {}", tex_path.display()));

    let pdf_time = if cfg.project.compile_pdf {
        let shell_escape = cfg.code.enabled && cfg.code.engine.eq_ignore_ascii_case("minted");
        let t = Instant::now();
        compile::compile_pdf(&tex_path, shell_escape, cli.verbose)?;
        Some(t.elapsed())
    } else {
        None
    };

    build::print_timings(
        &cfg,
        config_time,
        images_time,
        md_time,
        render_time,
        write_time,
        pdf_time,
        total_start.elapsed(),
    );
    Ok(())
}
