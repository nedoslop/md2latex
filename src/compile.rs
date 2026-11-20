//! PDF compilation via xelatex/biber.

use crate::log;
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

const XELATEX_HINT: &str = "xelatex not found. Please install TeX Live / MiKTeX.";
const AUX_EXTS: &[&str] = &["aux", "toc", "lof", "lot", "out", "bbl", "bcf"];

fn aux_snapshot(dir: &Path) -> Vec<Option<Vec<u8>>> {
    AUX_EXTS
        .iter()
        .map(|e| fs::read(dir.join("main").with_extension(e)).ok())
        .collect()
}

pub(crate) fn compile_pdf(tex_path: &Path, shell_escape: bool, verbose: bool) -> Result<()> {
    let dir = tex_path.parent().unwrap();
    let mut args: Vec<&str> = vec!["-interaction=nonstopmode", "-halt-on-error"];
    if shell_escape {
        args.push("-shell-escape");
    }
    args.push("main.tex");

    log::info("xelatex [1/3]...");
    run_tool("xelatex", &args, dir, verbose).context(XELATEX_HINT)?;

    if tex_path.with_extension("bcf").exists() {
        log::info("biber...");
        run_tool("biber", &["main"], dir, verbose).context("biber not found or failed")?;
    }

    // Snapshot *after* biber: the .bbl is an input of the next pass.
    let before = aux_snapshot(dir);
    log::info("xelatex [2/3]...");
    run_tool("xelatex", &args, dir, verbose).context(XELATEX_HINT)?;

    if aux_snapshot(dir) == before {
        log::info("xelatex [3/3] skipped — cross-references already converged");
    } else {
        log::info("xelatex [3/3]...");
        run_tool("xelatex", &args, dir, verbose).context(XELATEX_HINT)?;
    }

    log::success(&format!(
        "PDF ready:     {}",
        dir.join("main.pdf").display()
    ));
    Ok(())
}

pub(crate) fn run_tool(name: &str, args: &[&str], dir: &Path, verbose: bool) -> Result<()> {
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
