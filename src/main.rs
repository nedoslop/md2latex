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
#[command(about = "Конвертер Markdown -> LaTeX с оформлением по ГОСТ / СТУ")]
struct Cli {
    #[arg(short, long, default_value = "md2latex.toml")]
    config: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::from_file(&cli.config)
        .context("Не удалось загрузить конфигурацию")?;

    let md_path = PathBuf::from(&cfg.project.input);
    let md_content = fs::read_to_string(&md_path)
        .with_context(|| format!("Не удалось прочитать {}", md_path.display()))?;

    let out_dir = PathBuf::from(&cfg.project.output_dir);
    fs::create_dir_all(&out_dir)?;

    let md_prepared = images::prepare_images(
        &md_content,
        &md_path,
        &out_dir,
        cfg.images.convert_to.as_deref(),
        cfg.images.dpi,
    )
    .context("Ошибка при подготовке изображений")?;

    let body = markdown::markdown_to_latex(&md_prepared, &cfg)
        .context("Ошибка при преобразовании Markdown")?;

    let full_latex = template::render_document(&cfg, &body)
        .context("Ошибка при рендеринге LaTeX-шаблона")?;

    let tex_path = out_dir.join("main.tex");
    fs::write(&tex_path, &full_latex)
        .with_context(|| format!("Не удалось записать {}", tex_path.display()))?;
    println!("✅ LaTeX сохранён: {}", tex_path.display());

    if cfg.project.compile_pdf {
        compile_pdf(&tex_path)?;
    }

    Ok(())
}

fn compile_pdf(tex_path: &Path) -> Result<()> {
    let dir = tex_path.parent().unwrap();
    println!("🔨 Запуск xelatex...");

    let args: &[&str] = &["-interaction=nonstopmode", "-halt-on-error", "main.tex"];
    run_latex(dir, args)?;

    if tex_path.with_extension("bcf").exists() {
        println!("📚 Запуск biber...");
        Command::new("biber")
            .arg("main")
            .current_dir(dir)
            .status()
            .context("biber не найден или завершился с ошибкой")?;
    }

    run_latex(dir, args)?;
    // добавлено: третий прогон — стабилизация номеров страниц в оглавлении
    // и перекрёстных ссылок \ref{} при длинном ToC
    run_latex(dir, args)?;

    println!("✅ PDF готов: {}", dir.join("main.pdf").display());
    Ok(())
}

fn run_latex(dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("xelatex")
        .args(args)
        .current_dir(dir)
        .status()
        .context("xelatex не найден. Установите TeX Live / MiKTeX.")?;
    if !status.success() {
        anyhow::bail!("xelatex завершился с ошибкой");
    }
    Ok(())
}