//! Image preprocessing: copy (and optionally convert) every image referenced
//! from Markdown into `<out_dir>/images/`, rewriting the Markdown links
//! accordingly.

use anyhow::{Context, Result};
use image::ImageFormat;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// Walks through `md`, copies referenced images into `out_dir/images/`,
/// optionally converts them to `convert_to`, and returns the rewritten
/// Markdown where every image link points to the new location.
///
/// Remote URLs are left untouched; missing local files produce a warning.
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

    let md_dir = md_path.parent().unwrap_or_else(|| Path::new("."));

    let re = Regex::new(r#"!\[([^\]]*)\]\(([^)\s]+)(\s+[^)]*)?\)"#)
        .context("failed to compile image regex")?;

    let mut used_names: std::collections::HashSet<String> = Default::default();

    let result = re.replace_all(md, |caps: &regex::Captures| {
        let alt = &caps[1];
        let src = &caps[2];
        let rest = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        if src.starts_with("http://") || src.starts_with("https://") {
            eprintln!("warning: skipping URL image: {}", src);
            return caps[0].to_string();
        }

        let full_src: PathBuf = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            md_dir.join(src)
        };

        if !full_src.exists() {
            eprintln!("warning: file not found: {}", full_src.display());
            return caps[0].to_string();
        }

        let base_name = full_src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());

        let mut sanitized = sanitize(&base_name);
        let mut n = 1;
        while used_names.contains(&sanitized) && images_out.join(&sanitized) != full_src {
            let stem = Path::new(&base_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "image".into());
            let ext = Path::new(&base_name)
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();
            sanitized = format!("{}_{}{}", sanitize(&stem), n, ext);
            n += 1;
        }
        used_names.insert(sanitized.clone());

        let mut dest = images_out.join(&sanitized);

        if let Some(fmt) = convert_to {
            let target_ext = fmt.trim_start_matches('.').to_lowercase();
            let target_name = dest.with_extension(&target_ext);
            match convert_image(&full_src, &target_name, &target_ext, dpi) {
                Ok(()) => {
                    dest = target_name;
                    sanitized = dest.file_name().unwrap().to_string_lossy().to_string();
                }
                Err(e) => eprintln!("warning: failed to convert {}: {}", full_src.display(), e),
            }
        }

        // Always copy (overwrites), so updated sources reach `out/images/`
        // on repeated builds.
        if let Err(e) = fs::copy(&full_src, &dest) {
            eprintln!("warning: failed to copy {}: {}", full_src.display(), e);
            return caps[0].to_string();
        }

        format!("![{}](images/{}{})", alt, sanitized, rest)
    });

    Ok(result.to_string())
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
///
/// Uses the `printpdf 0.12` API: `RawImage::from_dynamic_image` converts
/// an `image::DynamicImage` into `printpdf`'s internal representation,
/// `doc.add_image` registers it as an XObject, and `Op::UseXobject`
/// places it on the page at its natural size (one pixel = one dot at 300 dpi
/// by default[reference:2]).
fn raster_to_pdf(src: &Path, dst: &Path, dpi: u32) -> Result<()> {
    use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage, XObjectTransform};

    let img = image::open(src).with_context(|| format!("failed to open {}", src.display()))?;
    let (w, h) = (img.width(), img.height());
    let dpi_f = dpi.max(1) as f32;
    let width_mm = (w as f32) * 25.4 / dpi_f;
    let height_mm = (h as f32) * 25.4 / dpi_f;

    // Convert `image::DynamicImage` into `printpdf::RawImage`.
    // This requires the `images` feature (or a format-specific feature
    // such as `png` / `jpeg`) to be enabled on `printpdf`.
    let raw_image = RawImage::from_dynamic_image(img)
        .map_err(|e| anyhow::anyhow!("failed to convert image to RawImage: {}", e))?;

    let mut doc = PdfDocument::new("image");

    // Register the image as an XObject in the document and obtain its ID.
    let image_xobject_id = doc.add_image(&raw_image);

    // Place the image at the origin. Scaling can be adjusted via
    // `XObjectTransform`; the default keeps the natural size.
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
