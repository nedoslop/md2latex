//! PlantUML support: JAR downloading, Java check, rendering and caching.
//!
//! Code blocks with language `plantuml` are converted to PNG images,
//! which are then inserted into the document as regular illustrations (`figure`).
//!
//! The JAR is automatically downloaded from GitHub (latest stable release)
//! to `<output_dir>/plantuml/plantuml.jar` if it's not already there.
//!
//! Cache: `<output_dir>/plantuml_cache/<sha256>.png`. The key is the SHA-256
//! of the PlantUML source code. Reusing the same code skips rendering.

use crate::config::Config;
use crate::log;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// PNG plus the density it was rendered at — the caller needs both.
pub struct Rendered {
    pub png_path: PathBuf,
    pub dpi: u32,
}

pub struct PlantUml {
    jar_path: PathBuf,
    java_cmd: String,
    cache_dir: PathBuf,
    default_dpi: u32,
}

impl PlantUml {
    /// Prepares PlantUML: checks Java, downloads the JAR if necessary,
    /// creates the cache directory.
    ///
    /// Returns `None` if PlantUML is unavailable (no Java, failed to download JAR).
    /// In this case, the caller should skip rendering and emit a warning.
    pub fn new(cfg: &Config, out_dir: &Path) -> Result<Option<Self>> {
        if !cfg.plantuml.enabled {
            log::info("PlantUML disabled in config");
            return Ok(None);
        }

        let java_cmd = cfg
            .plantuml
            .java_path
            .clone()
            .unwrap_or_else(|| "java".into());

        // Check Java
        let java_ok = Command::new(&java_cmd)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !java_ok {
            log::warn(&format!(
                "Java not found (`{} -version` failed). \
                PlantUML rendering will be skipped. \
                Install a JRE (Java 8+) or set `plantuml.java_path` in the profile.",
                java_cmd
            ));
            return Ok(None);
        }

        // JAR: either from config, or download to out_dir/plantuml/
        let jar_path = if let Some(p) = &cfg.plantuml.jar_path {
            let p = PathBuf::from(p);
            if !p.is_file() {
                log::warn(&format!(
                    "plantuml.jar not found at configured path: {}; \
                    falling back to auto-download",
                    p.display()
                ));
                download_jar_to_default_location(out_dir)?
            } else {
                p
            }
        } else {
            let default = out_dir.join("plantuml").join("plantuml.jar");
            if default.is_file() {
                default
            } else {
                download_jar_to_default_location(out_dir)?
            }
        };

        let cache_dir = out_dir.join("plantuml_cache");
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("failed to create {}", cache_dir.display()))?;

        log::info(&format!(
            "PlantUML ready: jar={}, cache={}",
            jar_path.display(),
            cache_dir.display()
        ));
        Ok(Some(Self {
            jar_path,
            java_cmd,
            cache_dir,
            default_dpi: cfg.plantuml.dpi.max(1),
        }))
    }

    /// Renders PlantUML code to PNG. Returns the path to the cache file.
    ///
    /// If rendering fails, returns `Ok(None)` (warning already emitted).
    pub fn render(&self, code: &str) -> Result<Option<Rendered>> {
        let dpi = effective_dpi(code, self.default_dpi);
        // DPI is part of the render parameters → part of the cache key.
        let hash = sha256_hex(&format!("dpi={}\u{0}{}", dpi, code));
        let png_path = self.cache_dir.join(format!("{}.png", hash));
        if png_path.is_file() {
            return Ok(Some(Rendered { png_path, dpi }));
        }
        let puml_path = self.cache_dir.join(format!("{}.puml", hash));
        fs::write(&puml_path, code)?;

        let output = Command::new(&self.java_cmd)
            .arg("-DPLANTUML_LIMIT_SIZE=16384") // JVM arg: MUST precede -jar
            .arg("-jar")
            .arg(&self.jar_path)
            .arg("-charset")
            .arg("UTF-8")
            .arg("-tpng")
            .arg(format!("-Sdpi={}", dpi))
            .arg(&puml_path)
            .output()
            .with_context(|| format!("failed to spawn `{}`", self.java_cmd));
        let _ = fs::remove_file(&puml_path);
        let output = output?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn(&format!(
                "PlantUML exited with {}: {}",
                output.status,
                stderr.trim()
            ));
            return Ok(None);
        }

        if !png_path.is_file() {
            log::warn(&format!(
                "PlantUML finished successfully but produced no file at {}",
                png_path.display()
            ));
            return Ok(None);
        }
        Ok(Some(Rendered { png_path, dpi }))
    }
}

/// Last `skinparam dpi N` wins (bare or inside a `skinparam [type] { ... }` block).
fn effective_dpi(code: &str, fallback: u32) -> u32 {
    let block_comments = regex::Regex::new(r"(?s)/'.*?'/").unwrap();
    let code = block_comments.replace_all(code, "");
    let num = |s: &str| {
        s.split_whitespace()
            .next()
            .and_then(|v| v.parse::<u32>().ok())
    };

    let mut dpi = fallback;
    let mut in_block = false;
    for raw in code.lines() {
        let line = raw.trim();
        if line.starts_with('\'') {
            continue;
        } // full-line comment only
        let lower = line.to_lowercase();
        if let Some(rest) = lower.strip_prefix("skinparam") {
            if let Some(v) = rest.trim().strip_prefix("dpi").and_then(num) {
                dpi = v;
            }
            if lower.contains('{') {
                in_block = true;
            }
            if lower.contains('}') {
                in_block = false;
            }
        } else if in_block {
            if let Some(v) = lower.strip_prefix("dpi").and_then(num) {
                dpi = v;
            }
            if lower.contains('}') {
                in_block = false;
            }
        }
    }
    dpi.max(1)
}

/// Downloads the latest stable `plantuml.jar` from GitHub to `<out_dir>/plantuml/plantuml.jar`.
fn download_jar_to_default_location(out_dir: &Path) -> Result<PathBuf> {
    let dir = out_dir.join("plantuml");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let jar_path = dir.join("plantuml.jar");
    if jar_path.is_file() {
        return Ok(jar_path);
    }

    log::info("Downloading latest PlantUML JAR from GitHub...");
    let client = Client::builder()
        .user_agent("md2latex-gost/1.0 (PlantUML auto-downloader)")
        .build()
        .context("failed to build HTTP client")?;

    // Latest stable release (GitHub filters out pre-releases).
    let release_url = "https://api.github.com/repos/plantuml/plantuml/releases/latest";
    let release: Value = client
        .get(release_url)
        .send()
        .context("failed to query GitHub releases API")?
        .error_for_status()
        .context("GitHub API returned an error")?
        .json()
        .context("failed to parse GitHub release JSON")?;

    let assets = release["assets"]
        .as_array()
        .context("release has no `assets` array")?;

    // Look for the JAR, excluding javadoc/sources.
    let jar_asset = assets
        .iter()
        .find(|a| {
            a["name"].as_str().map_or(false, |n| {
                n.ends_with(".jar") && !n.contains("javadoc") && !n.contains("sources")
            })
        })
        .context("no suitable `*.jar` asset found in the latest release")?;

    let download_url = jar_asset["browser_download_url"]
        .as_str()
        .context("JAR asset has no `browser_download_url`")?;

    let mut response = client
        .get(download_url)
        .send()
        .context("failed to start JAR download")?
        .error_for_status()
        .context("JAR download returned an error status")?;

    let mut file = fs::File::create(&jar_path)
        .with_context(|| format!("failed to create {}", jar_path.display()))?;
    response
        .copy_to(&mut file)
        .with_context(|| format!("failed to write {}", jar_path.display()))?;

    log::success(&format!("PlantUML JAR downloaded: {}", jar_path.display()));
    Ok(jar_path)
}

/// SHA-256 in hex.
fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    hex
}
