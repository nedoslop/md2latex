//! Image preprocessing: copy (and optionally convert) every image referenced
//! from Markdown into `<out_dir>/images/`, rewriting the Markdown links
//! accordingly.
//!
//! **`.puml` files are skipped** — they're PlantUML sources, handled by the
//! `markdown` module (rendered via `plantuml::PlantUml`).
//!
//! A simple on-disk cache (`.cache` in the images directory) records each
//! image's size / mtime and the active conversion settings (format, DPI).
//! On subsequent runs, files whose entry still matches are skipped — so
//! editing the document body no longer re-copies unchanged images, and only
//! new or genuinely modified sources (or a changed `[images]` config) trigger
//! work.

mod cache;
mod convert;

use crate::log;
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
struct ImageStats {
    processed: usize,
    cached: usize,
    missing: usize,
    urls: usize,
    skipped_plantuml: usize,
}

/// Walks through `md`, copies referenced images into `out_dir/images/`,
/// optionally converts them to `convert_to`, and returns the rewritten
/// Markdown where every image link points to the new location.
///
/// Remote URLs are left untouched; missing local files produce a warning.
/// `.puml` references are left untouched — they're handled later by the
/// markdown converter (rendered via PlantUML). Repeated calls reuse
/// `<out_dir>/images/.cache` to skip unchanged files.
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
    let re = Regex::new(r#"!\[([^\]]*)\]\(([^)\s]+)(\s+[^)]*)?\)"#)
        .context("failed to compile image regex")?;
    let mut used_names: HashSet<String> = Default::default();
    let mut src_to_dest: HashMap<PathBuf, String> = Default::default();
    let mut stats = ImageStats::default();

    let result = re.replace_all(md, |caps: &regex::Captures| {
        let alt = &caps[1];
        let src = &caps[2];
        let rest = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        // PlantUML source files are handled by `markdown.rs`; leave them as-is.
        if src.to_lowercase().ends_with(".puml") {
            stats.skipped_plantuml += 1;
            return caps[0].to_string();
        }
        if src.starts_with("http://") || src.starts_with("https://") {
            log::warn(&format!("skipping URL image: {}", src));
            stats.urls += 1;
            return caps[0].to_string();
        }

        let full_src: PathBuf = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            md_dir.join(src)
        };
        if !full_src.exists() {
            log::warn(&format!("file not found: {}", full_src.display()));
            stats.missing += 1;
            return caps[0].to_string();
        }
        if let Some(existing) = src_to_dest.get(&full_src) {
            return format!("![{}](images/{}{})", alt, existing, rest);
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

        let dest = images_out.join(&final_name);
        let src_fp = match convert::src_fingerprint(&full_src) {
            Ok(fp) => fp,
            Err(e) => {
                log::warn(&format!("cannot fingerprint {}: {}", full_src.display(), e));
                return caps[0].to_string();
            }
        };
        let cache_key = format!("{}:{}", src_fp, settings);
        let up_to_date = dest.exists()
            && cache
                .get(&final_name)
                .map(|s| s == cache_key)
                .unwrap_or(false);

        if up_to_date {
            used_names.insert(final_name.clone());
            src_to_dest.insert(full_src.clone(), final_name.clone());
            stats.cached += 1;
            return format!("![{}](images/{}{})", alt, final_name, rest);
        }

        let processed_name = match convert_to {
            Some(_) => {
                let target_ext = final_name
                    .rsplit_once('.')
                    .map(|(_, e)| e.to_string())
                    .unwrap_or_default();
                match convert::convert_image(&full_src, &dest, &target_ext, dpi) {
                    Ok(()) => final_name.clone(),
                    Err(e) => {
                        log::warn(&format!("failed to convert {}: {}", full_src.display(), e));
                        let mut fb = 1;
                        let mut fallback_base = format!("{}{}", convert::sanitize(&stem), ext);
                        while used_names.contains(&fallback_base) {
                            fallback_base = format!("{}_{}{}", convert::sanitize(&stem), fb, ext);
                            fb += 1;
                        }
                        let fallback = images_out.join(&fallback_base);
                        if let Err(e2) = fs::copy(&full_src, &fallback) {
                            log::warn(&format!("failed to copy {}: {}", full_src.display(), e2));
                            return caps[0].to_string();
                        }
                        fallback_base
                    }
                }
            }
            None => {
                if let Err(e) = fs::copy(&full_src, &dest) {
                    log::warn(&format!("failed to copy {}: {}", full_src.display(), e));
                    return caps[0].to_string();
                }
                final_name.clone()
            }
        };

        cache.set(processed_name.clone(), cache_key);
        used_names.insert(processed_name.clone());
        src_to_dest.insert(full_src.clone(), processed_name.clone());
        stats.processed += 1;
        format!("![{}](images/{}{})", alt, processed_name, rest)
    });

    cache.prune_missing(&images_out);
    if let Err(e) = cache.save() {
        log::warn(&format!("failed to save image cache: {}", e));
    }
    log::info(&format!(
        "images: {} processed, {} cached, {} missing, {} url(s), {} plantuml (skipped)",
        stats.processed, stats.cached, stats.missing, stats.urls, stats.skipped_plantuml
    ));
    Ok(result.to_string())
}
