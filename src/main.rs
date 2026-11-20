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
use mimalloc::MiMalloc;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser)]
#[command(name = "md2latex-gost")]
#[command(about = "Markdown -> LaTeX converter with GOST / STU formatting")]
struct Cli {
    input: PathBuf,
    #[arg(short, long, default_value = "out")]
    output_dir: PathBuf,
    #[arg(long)]
    profiles_dir: Option<PathBuf>,
    #[arg(long)]
    compile: bool,
    #[arg(short, long)]
    verbose: bool,
    /// Worker threads for image processing and PlantUML rendering.
    /// 0 = one per logical CPU (or $RAYON_NUM_THREADS).
    #[arg(short = 'j', long, default_value_t = 0)]
    jobs: usize,
}

fn init_thread_pool(jobs: usize) {
    let n = if jobs > 0 {
        Some(jobs)
    } else {
        std::env::var("RAYON_NUM_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
    };
    if let Some(n) = n {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(|i| format!("md2latex-{}", i))
            .build_global();
    }
}

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let total_start = Instant::now();
    let cli = Cli::parse();
    init_thread_pool(cli.jobs);

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

    // ── Parse once; warm the PlantUML cache in parallel ─────────────────
    let t = Instant::now();
    let md_dir = md_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let arena = comrak::Arena::new();
    let root = markdown::parse(&arena, &md_prepared);
    let sources = markdown::collect_plantuml_sources(root, &cfg, &md_dir);
    let plantuml = plantuml::PlantUml::prepare(&cfg, &out_dir, &sources)
        .context("failed to initialise PlantUML")?;
    let puml_time = t.elapsed();

    let t = Instant::now();
    let body = markdown::render(root, &cfg, &md_dir, plantuml.as_ref())
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
        puml_time,
        md_time,
        render_time,
        write_time,
        pdf_time,
        total_start.elapsed(),
    );
    Ok(())
}
