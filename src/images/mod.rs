//! Image preprocessing: copy (and optionally convert) every image referenced
//! from Markdown into `<out_dir>/images/`, rewriting the Markdown links.
//!
//! Three phases:
//!   1. plan   — sequential, cheap: resolve paths, allocate destination names
//!               in document order, consult the cache. No I/O beyond stat().
//!   2. work   — parallel (rayon): copy / decode+encode / raster→PDF.
//!   3. commit — sequential: fallbacks for failures, cache update, rewrite.
//!
//! `.puml` files and remote URLs are left untouched, as before.

mod cache;
mod convert;

use crate::log;
use anyhow::{Context, Result};
use rayon::prelude::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"!\[([^\]]*)\]\(([^)\s]+)(\s+[^)]*)?\)"#).expect("image regex is valid")
});

#[derive(Default, Debug)]
struct ImageStats {
    processed: usize,
    cached: usize,
    missing: usize,
    urls: usize,
    skipped_plantuml: usize,
}

enum Target {
    Ready(String),
    Pending(usize),
}

enum Action {
    Keep,
    Ready(String),
    Pending(usize),
}

struct Slot {
    span: (usize, usize),
    alt: (usize, usize),
    rest: Option<(usize, usize)>,
    action: Action,
}

struct Job {
    src: PathBuf,
    dest: PathBuf,
    name: String,
    stem: String,
    ext: String,
    cache_key: String,
    convert: bool,
}

pub fn prepare_images(
    md: &str,
    md_path: &Path,
    out_dir: &Path,
    convert_to: Option<&str>,
    dpi: u32,
) -> Result<String> {
    let images_out = out_dir.join("images");
    fs::create_dir_all(&images_out)
        .with_context(|| format!("failed to create {}", images_out.display()))?;

    let mut cache = cache::ImageCache::load(&images_out);
    let settings = convert::settings_fingerprint(convert_to, dpi);
    let md_dir = md_path.parent().unwrap_or_else(|| Path::new("."));

    let mut used_names: HashSet<String> = HashSet::new();
    let mut src_to_target: HashMap<PathBuf, Target> = HashMap::new();
    let mut stats = ImageStats::default();
    let mut jobs: Vec<Job> = Vec::new();
    let mut slots: Vec<Slot> = Vec::new();

    // ── Phase 1: plan (document order → deterministic names) ────────────
    for caps in IMAGE_RE.captures_iter(md) {
        let full = caps.get(0).unwrap();
        let alt = caps.get(1).unwrap();
        let mut slot = Slot {
            span: (full.start(), full.end()),
            alt: (alt.start(), alt.end()),
            rest: caps.get(3).map(|m| (m.start(), m.end())),
            action: Action::Keep,
        };
        let src = &caps[2];

        if src.to_lowercase().ends_with(".puml") {
            stats.skipped_plantuml += 1;
            slots.push(slot);
            continue;
        }
        if src.starts_with("http://") || src.starts_with("https://") {
            log::warn(&format!("skipping URL image: {}", src));
            stats.urls += 1;
            slots.push(slot);
            continue;
        }

        let full_src: PathBuf = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            md_dir.join(src)
        };
        if !full_src.exists() {
            log::warn(&format!("file not found: {}", full_src.display()));
            stats.missing += 1;
            slots.push(slot);
            continue;
        }
        if let Some(t) = src_to_target.get(&full_src) {
            slot.action = match t {
                Target::Ready(n) => Action::Ready(n.clone()),
                Target::Pending(i) => Action::Pending(*i),
            };
            slots.push(slot);
            continue;
        }

        let base_name = full_src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());
        let stem = Path::new(&base_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());
        let ext = Path::new(&base_name)
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();

        let mut counter = 1;
        let mut candidate = format!("{}{}", convert::sanitize(&stem), ext);
        let mut final_name = convert::final_dest_name(&candidate, convert_to);
        while used_names.contains(&final_name) {
            candidate = format!("{}_{}{}", convert::sanitize(&stem), counter, ext);
            final_name = convert::final_dest_name(&candidate, convert_to);
            counter += 1;
        }

        let src_fp = match convert::src_fingerprint(&full_src) {
            Ok(fp) => fp,
            Err(e) => {
                log::warn(&format!("cannot fingerprint {}: {}", full_src.display(), e));
                slots.push(slot);
                continue;
            }
        };
        let cache_key = format!("{}:{}", src_fp, settings);
        let dest = images_out.join(&final_name);
        let up_to_date = dest.exists()
            && cache
                .get(&final_name)
                .map(|s| s == cache_key)
                .unwrap_or(false);

        used_names.insert(final_name.clone());
        if up_to_date {
            stats.cached += 1;
            src_to_target.insert(full_src, Target::Ready(final_name.clone()));
            slot.action = Action::Ready(final_name);
        } else {
            let idx = jobs.len();
            jobs.push(Job {
                src: full_src.clone(),
                dest,
                name: final_name,
                stem,
                ext,
                cache_key,
                convert: convert_to.is_some(),
            });
            src_to_target.insert(full_src, Target::Pending(idx));
            slot.action = Action::Pending(idx);
        }
        slots.push(slot);
    }

    // ── Phase 2: parallel work ──────────────────────────────────────────
    let outcomes: Vec<std::result::Result<(), String>> = jobs
        .par_iter()
        .map(|job| {
            if job.convert {
                let target_ext = job
                    .name
                    .rsplit_once('.')
                    .map(|(_, e)| e.to_string())
                    .unwrap_or_default();
                convert::convert_image(&job.src, &job.dest, &target_ext, dpi)
                    .map_err(|e| format!("failed to convert {}: {}", job.src.display(), e))
            } else {
                fs::copy(&job.src, &job.dest)
                    .map(|_| ())
                    .map_err(|e| format!("failed to copy {}: {}", job.src.display(), e))
            }
        })
        .collect();

    // ── Phase 3: commit, in order (fallback naming stays deterministic) ──
    let mut produced: Vec<Option<String>> = Vec::with_capacity(jobs.len());
    for (job, outcome) in jobs.iter().zip(outcomes) {
        match outcome {
            Ok(()) => {
                cache.set(job.name.clone(), job.cache_key.clone());
                stats.processed += 1;
                produced.push(Some(job.name.clone()));
            }
            Err(msg) => {
                log::warn(&msg);
                // The reserved name was never written — release it, exactly
                // like the sequential version did, then pick a fallback.
                used_names.remove(&job.name);
                if !job.convert {
                    produced.push(None);
                    continue;
                }
                let mut fb = 1;
                let mut fallback = format!("{}{}", convert::sanitize(&job.stem), job.ext);
                while used_names.contains(&fallback) {
                    fallback = format!("{}_{}{}", convert::sanitize(&job.stem), fb, job.ext);
                    fb += 1;
                }
                match fs::copy(&job.src, images_out.join(&fallback)) {
                    Ok(_) => {
                        used_names.insert(fallback.clone());
                        cache.set(fallback.clone(), job.cache_key.clone());
                        stats.processed += 1;
                        produced.push(Some(fallback));
                    }
                    Err(e) => {
                        log::warn(&format!("failed to copy {}: {}", job.src.display(), e));
                        produced.push(None);
                    }
                }
            }
        }
    }

    // ── Phase 4: single-pass rewrite ────────────────────────────────────
    let mut out = String::with_capacity(md.len());
    let mut pos = 0usize;
    for slot in &slots {
        out.push_str(&md[pos..slot.span.0]);
        let name = match &slot.action {
            Action::Keep => None,
            Action::Ready(n) => Some(n.as_str()),
            Action::Pending(i) => produced[*i].as_deref(),
        };
        match name {
            Some(n) => {
                out.push_str("![");
                out.push_str(&md[slot.alt.0..slot.alt.1]);
                out.push_str("](images/");
                out.push_str(n);
                if let Some((s, e)) = slot.rest {
                    out.push_str(&md[s..e]);
                }
                out.push(')');
            }
            None => out.push_str(&md[slot.span.0..slot.span.1]),
        }
        pos = slot.span.1;
    }
    out.push_str(&md[pos..]);

    cache.prune_missing(&images_out);
    if let Err(e) = cache.save() {
        log::warn(&format!("failed to save image cache: {}", e));
    }
    log::info(&format!(
        "images: {} processed, {} cached, {} missing, {} url(s), {} plantuml (skipped)",
        stats.processed, stats.cached, stats.missing, stats.urls, stats.skipped_plantuml
    ));
    Ok(out)
}
