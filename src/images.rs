//! Image preprocessing: copy (and optionally convert) every image referenced
//! from Markdown into `<out_dir>/images/`, rewriting the Markdown links
//! accordingly.
//!
//! A simple on-disk cache (`.cache` in the images directory) records each
//! image's size / mtime and the active conversion settings (format, DPI).
//! On subsequent runs, files whose entry still matches are skipped — so
//! editing the document body no longer re-copies unchanged images, and only
//! new or genuinely modified sources (or a changed `[images]` config) trigger
//! work.

use anyhow::{Context, Result};
use image::ImageFormat;
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const CACHE_FILE: &str = ".cache";

#[derive(Default, Debug)]
struct ImageStats {
    processed: usize,
    cached: usize,
    missing: usize,
    urls: usize,
}

/// Walks through `md`, copies referenced images into `out_dir/images/`,
/// optionally converts them to `convert_to`, and returns the rewritten
/// Markdown where every image link points to the new location.
///
/// Remote URLs are left untouched; missing local files produce a warning.
/// Repeated calls reuse `<out_dir>/images/.cache` to skip unchanged files.
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

    let mut cache = ImageCache::load(&images_out);
    let settings = settings_fingerprint(convert_to, dpi);
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

        if src.starts_with("http://") || src.starts_with("https://") {
            eprintln!("warning: skipping URL image: {}", src);
            stats.urls += 1;
            return caps[0].to_string();
        }

        let full_src: PathBuf = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            md_dir.join(src)
        };

        if !full_src.exists() {
            eprintln!("warning: file not found: {}", full_src.display());
            stats.missing += 1;
            return caps[0].to_string();
        }

        // Same source already handled in this document — reuse its name.
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

        // Resolve a unique destination name.
        let mut counter = 1;
        let mut candidate = format!("{}{}", sanitize(&stem), ext);
        let mut final_name = final_dest_name(&candidate, convert_to);
        while used_names.contains(&final_name) {
            candidate = format!("{}_{}{}", sanitize(&stem), counter, ext);
            final_name = final_dest_name(&candidate, convert_to);
            counter += 1;
        }

        let dest = images_out.join(&final_name);

        // Cache lookup: destination must exist *and* match the current
        // (source, settings) fingerprint.
        let src_fp = match src_fingerprint(&full_src) {
            Ok(fp) => fp,
            Err(e) => {
                eprintln!("warning: cannot fingerprint {}: {}", full_src.display(), e);
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

        // Regenerate.
        let processed_name = match convert_to {
            Some(_) => {
                let target_ext = final_name
                    .rsplit_once('.')
                    .map(|(_, e)| e.to_string())
                    .unwrap_or_default();
                match convert_image(&full_src, &dest, &target_ext, dpi) {
                    Ok(()) => final_name.clone(),
                    Err(e) => {
                        eprintln!(
                            "warning: failed to convert {}: {}",
                            full_src.display(),
                            e
                        );
                        // Fallback: copy with original extension, uniquified.
                        let mut fb = 1;
                        let mut fallback_base = format!("{}{}", sanitize(&stem), ext);
                        while used_names.contains(&fallback_base) {
                            fallback_base =
                                format!("{}_{}{}", sanitize(&stem), fb, ext);
                            fb += 1;
                        }
                        let fallback = images_out.join(&fallback_base);
                        if let Err(e2) = fs::copy(&full_src, &fallback) {
                            eprintln!(
                                "warning: failed to copy {}: {}",
                                full_src.display(),
                                e2
                            );
                            return caps[0].to_string();
                        }
                        fallback_base
                    }
                }
            }
            None => {
                if let Err(e) = fs::copy(&full_src, &dest) {
                    eprintln!("warning: failed to copy {}: {}", full_src.display(), e);
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

    // Drop entries whose output file disappeared, then persist.
    cache.prune_missing(&images_out);
    if let Err(e) = cache.save() {
        eprintln!("warning: failed to save image cache: {}", e);
    }

    eprintln!(
        "images: {} processed, {} cached, {} missing, {} url(s)",
        stats.processed, stats.cached, stats.missing, stats.urls
    );

    Ok(result.to_string())
}

/// Computes the final on-disk name given a sanitized base name and the
/// optional target conversion format (mirrors `Path::with_extension`, which
/// is what the pre-cache code used).
fn final_dest_name(sanitized: &str, convert_to: Option<&str>) -> String {
    match convert_to {
        Some(fmt) => {
            let target_ext = fmt.trim_start_matches('.').to_lowercase();
            Path::new(sanitized)
                .with_extension(&target_ext)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| sanitized.to_string())
        }
        None => sanitized.to_string(),
    }
}

/// Settings fingerprint: any change here invalidates every cache entry.
fn settings_fingerprint(convert_to: Option<&str>, dpi: u32) -> String {
    format!("{}:{}", convert_to.unwrap_or(""), dpi)
}

/// `size:mtime_ns` fingerprint for the source file. Cheap and reliable for
/// "did this file change since last build" — no need to hash the bytes.
fn src_fingerprint(path: &Path) -> std::io::Result<String> {
    let meta = fs::metadata(path)?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(format!("{}:{}", size, mtime))
}

fn convert_image(src: &Path, dst: &Path, ext: &str, dpi: u32) -> Result<()> {
    match ext {
        "pdf" => raster_to_pdf(src, dst, dpi),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tiff" | "webp" => {
            let img =
                image::open(src).with_context(|| format!("failed to open {}", src.display()))?;
            let format = match ext {
                "png" => ImageFormat::Png,
                "jpg" | "jpeg" => ImageFormat::Jpeg,
                "bmp" => ImageFormat::Bmp,
                "gif" => ImageFormat::Gif,
                "tiff" => ImageFormat::Tiff,
                "webp" => ImageFormat::WebP,
                _ => unreachable!(),
            };
            img.save_with_format(dst, format)
                .with_context(|| format!("failed to save {}", dst.display()))?;
            Ok(())
        }
        other => anyhow::bail!("unsupported format: {}", other),
    }
}

/// Rasterises a raster image into a single-page PDF at the given DPI.
fn raster_to_pdf(src: &Path, dst: &Path, dpi: u32) -> Result<()> {
    use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage, XObjectTransform};

    let img = image::open(src).with_context(|| format!("failed to open {}", src.display()))?;
    let (w, h) = (img.width(), img.height());
    let dpi_f = dpi.max(1) as f32;
    let width_mm = (w as f32) * 25.4 / dpi_f;
    let height_mm = (h as f32) * 25.4 / dpi_f;

    let raw_image = RawImage::from_dynamic_image(img)
        .map_err(|e| anyhow::anyhow!("failed to convert image to RawImage: {}", e))?;

    let mut doc = PdfDocument::new("image");
    let image_xobject_id = doc.add_image(&raw_image);

    let page_contents = vec![Op::UseXobject {
        id: image_xobject_id,
        transform: XObjectTransform::default(),
    }];

    let page = PdfPage::new(Mm(width_mm), Mm(height_mm), page_contents);

    let mut warnings = Vec::new();
    let pdf_bytes = doc
        .with_pages(vec![page])
        .save(&PdfSaveOptions::default(), &mut warnings);

    fs::write(dst, &pdf_bytes).with_context(|| format!("failed to write {}", dst.display()))?;
    Ok(())
}

/// Replaces characters that may be unsafe in filenames.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Persistent on-disk cache: one `filename\tfingerprint\n` per line.
/// Kept in TSV rather than TOML to avoid filename → key-escaping weirdness
/// (filenames can contain `.`, `-`, etc. that TOML bare keys reject).
struct ImageCache {
    path: PathBuf,
    entries: BTreeMap<String, String>,
}

impl ImageCache {
    fn load(dir: &Path) -> Self {
        let path = dir.join(CACHE_FILE);
        let mut entries = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('\t') {
                    if !k.is_empty() {
                        entries.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        Self { path, entries }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|s| s.as_str())
    }

    fn set(&mut self, name: String, value: String) {
        self.entries.insert(name, value);
    }

    fn prune_missing(&mut self, dir: &Path) {
        self.entries.retain(|name, _| dir.join(name).exists());
    }

    fn save(&self) -> Result<()> {
        let mut text = String::new();
        for (k, v) in &self.entries {
            text.push_str(k);
            text.push('\t');
            text.push_str(v);
            text.push('\n');
        }
        fs::write(&self.path, text)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }
}