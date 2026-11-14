//! Command-line entry point: loads config from the Markdown frontmatter,
//! prepares images, converts Markdown to LaTeX, renders the final `.tex`,
//! and optionally compiles it with `xelatex`.
//!
//! External tools (`xelatex`, `biber`) run **quietly by default**: their
//! stdout/stderr is captured and dumped to the console only if they fail.
//! Pass `--verbose` to stream the full log live.

mod config;
mod images;
mod log;
mod markdown;
mod template;

use anyhow::{Context, Result};
use clap::Parser;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, CellAlignment, Color, Table};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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
    // Load .env (if any) so frontmatter `${VAR}` references can resolve.
    // Shell-exported vars win — dotenvy only fills what isn't already set.
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

    // Copy the .bib file next to the generated .tex so biber can find it.
    copy_bibliography(&cfg, &md_path, &out_dir).context("bibliography copy failed")?;

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
    log::success(&format!("LaTeX saved:   {}", tex_path.display()));

    let pdf_time = if cfg.project.compile_pdf {
        let shell_escape = cfg.code.enabled && cfg.code.engine.eq_ignore_ascii_case("minted");
        let t = Instant::now();
        compile_pdf(&tex_path, shell_escape, cli.verbose)?;
        Some(t.elapsed())
    } else {
        None
    };

    print_timings(
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

/// Copies the biblatex `.bib` file into `out_dir`, preserving relative
/// subpaths so the `\addbibresource{...}` path still resolves.
fn copy_bibliography(cfg: &config::Config, md_path: &Path, out_dir: &Path) -> Result<()> {
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
fn print_timings(
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
fn compile_pdf(tex_path: &Path, shell_escape: bool, verbose: bool) -> Result<()> {
    let dir = tex_path.parent().unwrap();

    let mut args: Vec<&str> = vec!["-interaction=nonstopmode", "-halt-on-error"];
    if shell_escape {
        args.push("-shell-escape");
    }
    args.push("main.tex");

    log::info("xelatex [1/3]...");
    run_tool("xelatex", &args, dir, verbose)
        .context("xelatex not found. Please install TeX Live / MiKTeX.")?;

    if tex_path.with_extension("bcf").exists() {
        log::info("biber...");
        run_tool("biber", &["main"], dir, verbose).context("biber not found or failed")?;
    }

    log::info("xelatex [2/3]...");
    run_tool("xelatex", &args, dir, verbose)
        .context("xelatex not found. Please install TeX Live / MiKTeX.")?;

    log::info("xelatex [3/3]...");
    run_tool("xelatex", &args, dir, verbose)
        .context("xelatex not found. Please install TeX Live / MiKTeX.")?;

    log::success(&format!(
        "PDF ready:     {}",
        dir.join("main.pdf").display()
    ));
    Ok(())
}

fn run_tool(name: &str, args: &[&str], dir: &Path, verbose: bool) -> Result<()> {
    if verbose {
        let status = Command::new(name)
            .args(args)
            .current_dir(dir)
            .status()
            .with_context(|| format!("failed to spawn {}", name))?;
        if !status.success() {
            anyhow::bail!("{} exited with {}", name, status);
        }
        return Ok(());
    }

    let output = Command::new(name)
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("failed to spawn {}", name))?;

    if output.status.success() {
        return Ok(());
    }

    let mut wrote = false;
    if !output.stdout.is_empty() {
        log::error(&format!("─── {} stdout ───", name));
        let _ = std::io::stderr().write_all(&output.stdout);
        wrote = true;
    }
    if !output.stderr.is_empty() {
        if wrote {
            eprintln!();
        }
        log::error(&format!("─── {} stderr ───", name));
        let _ = std::io::stderr().write_all(&output.stderr);
        wrote = true;
    }
    if !wrote {
        log::error(&format!("({} produced no output)", name));
    }

    anyhow::bail!(
        "{} exited with {} — full log above; \
         rerun with --verbose to stream it live",
        name,
        output.status
    )
}
