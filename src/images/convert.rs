//! Image conversion and fingerprinting.

use anyhow::{Context, Result};
use image::ImageFormat;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

pub(crate) fn final_dest_name(sanitized: &str, convert_to: Option<&str>) -> String {
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

pub(crate) fn settings_fingerprint(convert_to: Option<&str>, dpi: u32) -> String {
    format!("{}:{}", convert_to.unwrap_or(""), dpi)
}

pub(crate) fn src_fingerprint(path: &Path) -> std::io::Result<String> {
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

pub(crate) fn convert_image(src: &Path, dst: &Path, ext: &str, dpi: u32) -> Result<()> {
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

pub(crate) fn sanitize(name: &str) -> String {
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
