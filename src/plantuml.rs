//! PlantUML support: JAR downloading, Java check, batched/parallel rendering.
//!
//! Rendering is a two-step affair:
//!   * `prepare()` collects every diagram source up front, dedups it, groups
//!     it by effective DPI and renders it in batched, parallel JVM calls;
//!   * `render()` is then a pure cache lookup during the (sequential) AST walk.
//!
//! Nothing is initialised at all when the document contains no diagrams:
//! no `java -version` probe, no GitHub round-trip.
//!
//! Cache: `<output_dir>/plantuml_cache/<sha256(dpi\0source)>.png`.

use crate::config::Config;
use crate::log;
use anyhow::{Context, Result};
use rayon::prelude::*;
use reqwest::blocking::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

/// Keeps the command line safely below the OS argument-size limit.
const MAX_FILES_PER_JVM: usize = 48;
/// JVM start-up dominates for small diagrams: don't spin up a worker for
/// fewer than this many diagrams.
const MIN_DIAGRAMS_PER_JVM: usize = 4;

static BLOCK_COMMENT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)/'.*?'/").unwrap());

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

fn cache_input(dpi: u32, code: &str) -> String {
    format!("dpi={}\u{0}{}", dpi, code)
}

impl PlantUml {
    /// Initialises PlantUML **only if there is something to render**, then
    /// pre-renders every source in batched parallel JVM calls.
    pub fn prepare(cfg: &Config, out_dir: &Path, sources: &[String]) -> Result<Option<Self>> {
        if !cfg.plantuml.enabled {
            if !sources.is_empty() {
                log::info("PlantUML disabled in config");
            }
            return Ok(None);
        }
        if sources.is_empty() {
            return Ok(None); // no Java probe, no download
        }
        let Some(p) = Self::init(cfg, out_dir)? else {
            return Ok(None);
        };
        p.prerender(sources);
        Ok(Some(p))
    }

    fn init(cfg: &Config, out_dir: &Path) -> Result<Option<Self>> {
        if !cfg.plantuml.enabled {
            log::info("PlantUML disabled in config");
            return Ok(None);
        }
        let java_cmd = cfg
            .plantuml
            .java_path
            .clone()
            .unwrap_or_else(|| "java".into());

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

        let jar_path = if let Some(p) = &cfg.plantuml.jar_path {
            let p = PathBuf::from(p);
            if !p.is_file() {
                log::warn(&format!(
                    "plantuml.jar not found at configured path: {}; falling back to auto-download",
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

    /// Batched + parallel warm-up of the cache. Failures are retried
    /// one-by-one so the user still gets a precise per-diagram message.
    pub fn prerender(&self, sources: &[String]) {
        let mut groups: BTreeMap<u32, Vec<(String, String)>> = BTreeMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut cached = 0usize;

        for code in sources {
            let dpi = effective_dpi(code, self.default_dpi);
            let hash = sha256_hex(&cache_input(dpi, code));
            if !seen.insert(hash.clone()) {
                continue;
            }
            if self.cache_dir.join(format!("{}.png", &hash)).is_file() {
                cached += 1;
                continue;
            }
            groups.entry(dpi).or_default().push((hash, code.clone()));
        }

        let pending: usize = groups.values().map(Vec::len).sum();
        if pending == 0 {
            if cached > 0 {
                log::info(&format!("PlantUML: {} diagram(s) from cache", cached));
            }
            return;
        }

        let workers = rayon::current_num_threads().max(1);
        let mut batches: Vec<(u32, Vec<(String, String)>)> = Vec::new();
        for (dpi, items) in groups {
            let target =
                ((items.len() + MIN_DIAGRAMS_PER_JVM - 1) / MIN_DIAGRAMS_PER_JVM).clamp(1, workers);
            let chunk = ((items.len() + target - 1) / target).clamp(1, MAX_FILES_PER_JVM);
            for part in items.chunks(chunk) {
                batches.push((dpi, part.to_vec()));
            }
        }

        log::info(&format!(
            "PlantUML: {} to render, {} cached, {} JVM batch(es)",
            pending,
            cached,
            batches.len()
        ));

        let missing: Vec<(u32, String, String)> = batches
            .par_iter()
            .flat_map_iter(|(dpi, items)| self.run_batch(*dpi, items).into_iter())
            .collect();

        for (dpi, hash, code) in missing {
            self.render_single(dpi, &hash, &code);
        }
    }

    /// One JVM, many input files. Returns the items whose PNG is missing.
    fn run_batch(&self, dpi: u32, items: &[(String, String)]) -> Vec<(u32, String, String)> {
        for (hash, code) in items {
            let p = self.cache_dir.join(format!("{}.puml", hash));
            if let Err(e) = fs::write(&p, code) {
                log::warn(&format!("failed to write {}: {}", p.display(), e));
            }
        }

        let mut cmd = Command::new(&self.java_cmd);
        cmd.arg("-DPLANTUML_LIMIT_SIZE=16384") // JVM arg: MUST precede -jar
            .arg("-jar")
            .arg(&self.jar_path)
            .arg("-charset")
            .arg("UTF-8")
            .arg("-tpng")
            .arg(format!("-Sdpi={}", dpi));
        for (hash, _) in items {
            cmd.arg(self.cache_dir.join(format!("{}.puml", hash)));
        }
        let result = cmd.output();

        for (hash, _) in items {
            let _ = fs::remove_file(self.cache_dir.join(format!("{}.puml", hash)));
        }

        match result {
            Ok(o) if o.status.success() => {}
            Ok(o) => log::warn(&format!(
                "PlantUML exited with {}: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => log::warn(&format!("failed to spawn `{}`: {}", self.java_cmd, e)),
        }

        items
            .iter()
            .filter(|(h, _)| !self.cache_dir.join(format!("{}.png", h)).is_file())
            .map(|(h, c)| (dpi, h.clone(), c.clone()))
            .collect()
    }

    fn render_single(&self, dpi: u32, hash: &str, code: &str) {
        let puml_path = self.cache_dir.join(format!("{}.puml", hash));
        if let Err(e) = fs::write(&puml_path, code) {
            log::warn(&format!("failed to write {}: {}", puml_path.display(), e));
            return;
        }
        let output = Command::new(&self.java_cmd)
            .arg("-DPLANTUML_LIMIT_SIZE=16384")
            .arg("-jar")
            .arg(&self.jar_path)
            .arg("-charset")
            .arg("UTF-8")
            .arg("-tpng")
            .arg(format!("-Sdpi={}", dpi))
            .arg(&puml_path)
            .output();
        let _ = fs::remove_file(&puml_path);

        match output {
            Ok(o) if o.status.success() => {}
            Ok(o) => log::warn(&format!(
                "PlantUML exited with {}: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => log::warn(&format!("failed to spawn `{}`: {}", self.java_cmd, e)),
        }
    }

    /// Cache lookup; renders on demand if the pre-pass missed this source.
    pub fn render(&self, code: &str) -> Result<Option<Rendered>> {
        let dpi = effective_dpi(code, self.default_dpi);
        let hash = sha256_hex(&cache_input(dpi, code));
        let png_path = self.cache_dir.join(format!("{}.png", hash));
        if png_path.is_file() {
            return Ok(Some(Rendered { png_path, dpi }));
        }
        self.render_single(dpi, &hash, code);
        if png_path.is_file() {
            Ok(Some(Rendered { png_path, dpi }))
        } else {
            log::warn(&format!(
                "PlantUML finished but produced no file at {}",
                png_path.display()
            ));
            Ok(None)
        }
    }
}

/// Last `skinparam dpi N` wins (bare or inside a `skinparam [type] { … }` block).
fn effective_dpi(code: &str, fallback: u32) -> u32 {
    let stripped;
    let code = if code.contains("/'") {
        stripped = BLOCK_COMMENT_RE.replace_all(code, "");
        stripped.as_ref()
    } else {
        code
    };
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
        }
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
