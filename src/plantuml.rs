//! PlantUML support: скачивание JAR, проверка Java, рендеринг и кеширование.
//!
//! Блоки кода с языком `plantuml` превращаются в PNG-изображения,
//! которые затем вставляются в документ как обычные иллюстрации (`figure`).
//!
//! JAR скачивается автоматически с GitHub (последний стабильный релиз)
//! в `<output_dir>/plantuml/plantuml.jar`, если его там нет.
//!
//! Кеш: `<output_dir>/plantuml_cache/<sha256>.png`. Ключ — SHA-256 от
//! исходного кода PlantUML. При повторном использовании того же кода
//! рендеринг не выполняется.

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

pub struct PlantUml {
    jar_path: PathBuf,
    java_cmd: String,
    cache_dir: PathBuf,
}

impl PlantUml {
    /// Подготавливает PlantUML: проверяет Java, при необходимости
    /// скачивает JAR, создаёт каталог кеша.
    ///
    /// Возвращает `None`, если PlantUML недоступен (нет Java, не удалось
    /// скачать JAR). В этом случае вызывающий код должен пропустить
    /// рендеринг и вывести предупреждение.
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

        // Проверка Java
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

        // JAR: либо из конфига, либо скачиваем в out_dir/plantuml/
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
        }))
    }
    /// Рендерит PlantUML-код в PNG. Возвращает путь к файлу кеша.
    ///
    /// Если рендеринг не удался — возвращает `Ok(None)` (предупреждение
    /// уже выведено).
    pub fn render(&self, code: &str) -> Result<Option<PathBuf>> {
        let hash = sha256_hex(code);
        let png_path = self.cache_dir.join(format!("{}.png", hash));

        if png_path.is_file() {
            return Ok(Some(png_path));
        }

        // Пишем .puml прямо в cache_dir — тогда PlantUML создаст `<hash>.png`
        // рядом с ним, без всяких `-o` и tempdir (которые на Windows
        // иногда не срабатывают для абсолютных путей).
        let puml_path = self.cache_dir.join(format!("{}.puml", hash));
        fs::write(&puml_path, code)
            .with_context(|| format!("failed to write {}", puml_path.display()))?;

        let output = Command::new(&self.java_cmd)
            .arg("-jar")
            .arg(&self.jar_path)
            .arg("-tpng")
            .arg(&puml_path)
            .output()
            .with_context(|| format!("failed to spawn `{}`", self.java_cmd));

        // .puml больше не нужен в любом случае.
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

        Ok(Some(png_path))
    }
}

/// Скачивает последний стабильный `plantuml.jar` с GitHub в
/// `<out_dir>/plantuml/plantuml.jar`.
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

    // Последний стабильный релиз (GitHub сам отфильтровывает pre-release).
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

    // Ищем JAR, исключая javadoc/sources.
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

/// SHA-256 в hex.
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
