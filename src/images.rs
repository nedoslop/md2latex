use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// Копирует все изображения, на которые ссылается markdown, в `<out_dir>/images/`
/// и переписывает пути в md на `images/<sanitized_name>`.
pub fn prepare_images(md: &str, md_path: &Path, out_dir: &Path) -> Result<String> {
    let images_out = out_dir.join("images");
    fs::create_dir_all(&images_out)
        .with_context(|| format!("Не удалось создать {}", images_out.display()))?;

    let md_dir = md_path.parent().unwrap_or_else(|| Path::new("."));

    // ![alt](src "title") или ![alt](src)
    let re = Regex::new(r#"!\[([^\]]*)\]\(([^)\s]+)(\s+[^)]*)?\)"#)
        .context("Не удалось скомпилировать регулярное выражение")?;

    let mut used_names: std::collections::HashSet<String> = Default::default();

    let result = re.replace_all(md, |caps: &regex::Captures| {
        let alt = &caps[1];
        let src = &caps[2];
        let rest = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        if src.starts_with("http://") || src.starts_with("https://") {
            eprintln!("⚠️  Пропускаю URL-изображение: {}", src);
            return caps[0].to_string();
        }

        let full_src: PathBuf = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            md_dir.join(src)
        };

        if !full_src.exists() {
            eprintln!("⚠️  Не найден файл: {}", full_src.display());
            return caps[0].to_string();
        }

        let base_name = full_src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());

        // Уникализируем имя при коллизии
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

        let dest = images_out.join(&sanitized);
        if let Err(e) = fs::copy(&full_src, &dest) {
            eprintln!("⚠️  Не удалось скопировать {}: {}", full_src.display(), e);
            return caps[0].to_string();
        }

        format!("![{}](images/{}{})", alt, sanitized, rest)
    });

    Ok(result.to_string())
}

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