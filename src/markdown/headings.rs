//! Heading rendering: structural H1s, numbered sections, appendices.
//!
//! ## Регистр
//!
//! Регистр заголовков управляется тремя конфигами (все — опциональные,
//! значения по умолчанию сохраняют прежнее поведение):
//!
//! * `style.structural_heading_case` — для структурных заголовков
//!   (ВВЕДЕНИЕ, ЗАКЛЮЧЕНИЕ, …). Fallback: `structural_heading_uppercase`.
//! * `style.heading_case` — для `\section`-заголовков.
//!   Fallback: `section_uppercase`.
//! * `style.subsection_case` — для подразделов.
//!   Default: `auto_title`.
//!
//! Режимы: `preserve`, `upper`, `title`, `sentence`, `auto_title`.
//!
//! ## Нумерация
//!
//! `style.heading_numbering`:
//!
//! * `auto` (по умолчанию) — номер из Markdown отбрасывается, LaTeX
//!   нумерует автоматически.
//! * `source` — номер из Markdown сохраняется; перед `\section`/`\subsection`
//!   выставляется `\setcounter{...}{N-1}`. Предупреждение, если число
//!   компонентов не совпадает с глубиной команды.
//! * `none` — номер отбрасывается, заголовок печатается без нумерации
//!   (`\section*{}` + `\addcontentsline`).
//!
//! Структурные заголовки и заголовки приложений обрабатываются отдельной
//! ветвью и не подчиняются `heading_numbering` — у них своя схема
//! (ВВЕДЕНИЕ/ЗАКЛЮЧЕНИЕ без номера; приложения — буквами A, B, …).

use crate::config::{CaseMode, Config, HeadingNumbering};
use crate::log;
use anyhow::Result;
use comrak::nodes::AstNode;
use regex::Regex;

use super::state::{Ctx, RenderState};
use super::util::{escape_latex, extract_text};

pub(crate) fn render_heading<'a>(
    node: &'a AstNode<'a>,
    level: u8,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState<'_>,
) -> Result<()> {
    let raw_text = extract_text(node);
    let (numbers, clean) = split_heading_prefix(&raw_text);
    let clean = clean.trim_end_matches('.').trim().to_string();

    // ── H1: структурный или обычный раздел ──────────────────────────────
    if level == 1 {
        if is_structural(&clean, cfg) {
            if cfg.style.structural_headings_newpage {
                out.push_str("\\clearpage\n");
            }
            let upper = clean.trim().to_uppercase();
            state.in_bibliography = upper == cfg.style.bibliography_heading.trim().to_uppercase();
            state.skip_next_section_pagebreak =
                upper == cfg.style.main_part_heading.trim().to_uppercase();
            let unnumbered = cfg.style.is_unnumbered_subsection(&clean);
            state.ctx = if unnumbered {
                Ctx::StructuralUnnumbered
            } else {
                Ctx::StructuralNumbered
            };
            if is_appendix_heading(&clean, cfg) {
                state.ctx = Ctx::Appendix;
                render_appendix_heading(&clean, out, cfg)?;
            } else {
                render_structural_heading(&clean, out, cfg)?;
            }
        } else {
            log::warn(&format!(
                "H1 heading not in structural list: '{}' - rendering as \\section",
                clean
            ));
            state.in_bibliography = false;
            state.ctx = Ctx::StructuralNumbered;
            if state.skip_next_section_pagebreak {
                state.skip_next_section_pagebreak = false;
            } else {
                out.push_str("\\clearpage\n");
            }
            let display = cfg.style.heading_case_mode().apply(&clean);
            let numbered = cfg.style.heading_numbering != HeadingNumbering::None;
            if cfg.style.heading_numbering == HeadingNumbering::Source {
                sync_counter("section", numbers.as_deref(), out);
            }
            emit_section_like("section", &display, numbered, out);
        }
        return Ok(());
    }

    // ── H2+: подразделы ─────────────────────────────────────────────────
    state.in_bibliography = false;
    let is_numbered_section = level == 2
        && raw_text
            .trim()
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_digit());
    if is_numbered_section {
        state.ctx = Ctx::StructuralNumbered;
    }

    if state.ctx == Ctx::StructuralUnnumbered {
        // Подраздел внутри структурного заголовка — печатаем жирным
        // без нумерации.
        let display = cfg.style.subsection_case_mode().apply(&clean);
        out.push_str(&format!("\\textbf{{{}}}\\par\n\n", escape_latex(&display)));
        return Ok(());
    }

    let (cmd, numbered) = match (state.ctx, level) {
        (Ctx::StructuralNumbered, 2) => ("section", true),
        (Ctx::StructuralNumbered, 3) => ("subsection", true),
        (Ctx::StructuralNumbered, _) => ("subsubsection", true),
        (Ctx::Appendix, 2) => ("subsection", true),
        (Ctx::Appendix, 3) => ("subsubsection", true),
        (Ctx::Appendix, _) => ("paragraph", true),
        (Ctx::Ordinary, 2) => ("subsection", true),
        (Ctx::Ordinary, 3) => ("subsubsection", true),
        (Ctx::Ordinary, _) => ("paragraph", true),
        _ => ("section", true),
    };

    // Режим регистра выбираем по типу команды.
    let case_mode = if cmd == "section" {
        cfg.style.heading_case_mode()
    } else {
        cfg.style.subsection_case_mode()
    };
    let display = case_mode.apply(&clean);

    if cmd == "section" {
        if state.skip_next_section_pagebreak {
            state.skip_next_section_pagebreak = false;
        } else {
            out.push_str("\\clearpage\n");
        }
    }

    // Синхронизация счётчика в режиме `source`.
    if cfg.style.heading_numbering == HeadingNumbering::Source {
        sync_counter(cmd, numbers.as_deref(), out);
    }

    // В режиме `none` печатаем без номера; в остальных — как обычно.
    let actually_numbered = numbered && cfg.style.heading_numbering != HeadingNumbering::None;
    emit_section_like(cmd, &display, actually_numbered, out);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Structural headings
// ─────────────────────────────────────────────────────────────────────────

fn is_structural(text: &str, cfg: &Config) -> bool {
    let t = text.trim().to_uppercase();
    let appendix_upper = cfg.style.appendix_prefix.trim().to_uppercase();
    cfg.style.structural_list().iter().any(|h| {
        let up = h.trim().to_uppercase();
        t == up || (up == appendix_upper && t.starts_with(&format!("{} ", appendix_upper)))
    })
}

fn is_appendix_heading(text: &str, cfg: &Config) -> bool {
    let t = text.trim().to_uppercase();
    let prefix = cfg.style.appendix_prefix.trim().to_uppercase();
    t == prefix || t.starts_with(&format!("{} ", prefix))
}

fn render_structural_heading(text: &str, out: &mut String, cfg: &Config) -> Result<()> {
    let display = cfg.style.structural_heading_case_mode().apply(text.trim());
    if cfg.style.main_part_silent
        && display.trim().to_uppercase() == cfg.style.main_part_heading.trim().to_uppercase()
    {
        return Ok(());
    }
    out.push_str(&format!(
        "\\phantomsection\n\
         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
         {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n\
         \\vspace{{\\baselineskip}}\n\n",
        escape_latex(&display),
        escape_latex(&display)
    ));
    Ok(())
}

fn render_appendix_heading(text: &str, out: &mut String, cfg: &Config) -> Result<()> {
    let trimmed = text.trim();
    let prefix_upper = cfg.style.appendix_prefix.trim().to_uppercase();
    let rest = if trimmed.len() >= prefix_upper.len()
        && trimmed[..prefix_upper.len()].to_uppercase() == prefix_upper
    {
        trimmed[prefix_upper.len()..].trim()
    } else {
        trimmed
    };
    let mut parts = rest.splitn(2, char::is_whitespace);
    let letter = match parts.next() {
        Some(s) if !s.is_empty() => s.to_uppercase(),
        _ => cfg.style.appendix_default_letter.to_uppercase(),
    };
    let title = parts.next().unwrap_or("").trim();
    let app_label = format!("{} {}", cfg.style.appendix_prefix, letter);
    let toc_entry = if title.is_empty() {
        app_label.clone()
    } else {
        let t = cfg.style.structural_heading_case_mode().apply(title);
        format!("{} {}", app_label, t)
    };
    let listing_reset = if cfg.code.enabled && !cfg.code.engine.eq_ignore_ascii_case("minted") {
        format!(
            "\\setcounter{{lstlisting}}{{0}}\n\
             \\renewcommand{{\\thelstlisting}}{{{}.\\arabic{{lstlisting}}}}\n",
            letter
        )
    } else {
        String::new()
    };
    out.push_str(&format!(
        "\\clearpage\n\
         \\phantomsection\n\
         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
         \\setcounter{{figure}}{{0}}\n\
         \\setcounter{{table}}{{0}}\n\
         \\setcounter{{equation}}{{0}}\n\
         \\setcounter{{subsection}}{{0}}\n\
         \\setcounter{{subsubsection}}{{0}}\n\
         \\renewcommand{{\\thefigure}}{{{}.\\arabic{{figure}}}}\n\
         \\renewcommand{{\\thetable}}{{{}.\\arabic{{table}}}}\n\
         \\renewcommand{{\\theequation}}{{{}.\\arabic{{equation}}}}\n\
         \\renewcommand{{\\thesubsection}}{{{}.\\arabic{{subsection}}}}\n\
         \\renewcommand{{\\thesubsubsection}}{{{}.\\arabic{{subsection}}.\\arabic{{subsubsection}}}}\n\
         {}\
         {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n",
        escape_latex(&toc_entry),
        letter,
        letter,
        letter,
        letter,
        letter,
        listing_reset,
        escape_latex(&app_label)
    ));
    if !title.is_empty() {
        let t = cfg.style.subsection_case_mode().apply(title);
        out.push_str(&format!(
            "\\vspace{{0.5\\baselineskip}}\n\
             {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n",
            escape_latex(&t)
        ));
    }
    out.push_str("\\vspace{\\baselineskip}\n\n");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Numbering helpers
// ─────────────────────────────────────────────────────────────────────────

/// Отделяет числовой префикс (`1`, `1.2`, `1.2.3`) от текста заголовка.
///
/// Возвращает `(числа, текст-без-префикса)`.
fn split_heading_prefix(text: &str) -> (Option<Vec<u32>>, String) {
    let re = Regex::new(r"^\s*(\d+(?:\.\d+)*)\.?\s+").unwrap();
    if let Some(c) = re.captures(text) {
        let nums: Vec<u32> = c[1].split('.').filter_map(|s| s.parse().ok()).collect();
        (Some(nums), re.replace(text, "").to_string())
    } else {
        (None, text.to_string())
    }
}

/// Синхронизирует LaTeX-счётчик перед `\section`/`\subsection`/…
/// в режиме `HeadingNumbering::Source`.
fn sync_counter(cmd: &str, numbers: Option<&[u32]>, out: &mut String) {
    let (counter, expected_len) = match cmd {
        "section" => ("section", 1usize),
        "subsection" => ("subsection", 2usize),
        "subsubsection" => ("subsubsection", 3usize),
        _ => return,
    };
    let Some(nums) = numbers else { return };
    if nums.len() != expected_len {
        log::warn(&format!(
            "heading number {:?} has {} component(s), expected {} for \\{}; counter not synced",
            nums,
            nums.len(),
            expected_len,
            cmd
        ));
        return;
    }
    let n = nums[expected_len - 1];
    if n > 0 {
        out.push_str(&format!("\\setcounter{{{}}}{{{}}}\n", counter, n - 1));
    }
}

/// Печатает `\section{...}` (numbered = true) либо
/// `\section*{...}` + TOC-entry (numbered = false).
fn emit_section_like(cmd: &str, display: &str, numbered: bool, out: &mut String) {
    if numbered {
        out.push_str(&format!("\\{}{{{}}}\n\n", cmd, escape_latex(display)));
    } else {
        out.push_str(&format!(
            "\\phantomsection\n\
             \\addcontentsline{{toc}}{{{}}}{{{}}}\n\
             \\{}*{{{}}}\n\n",
            cmd,
            escape_latex(display),
            cmd,
            escape_latex(display)
        ));
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Legacy helpers kept for API compatibility
// ─────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn strip_heading_number(s: &str) -> String {
    let re = Regex::new(r"^\s*\d+(\.\d+)*\.?\s*").unwrap();
    re.replace(s, "").to_string()
}

#[allow(dead_code)]
fn to_title_case_if_upper(s: &str) -> String {
    CaseMode::AutoTitle.apply(s)
}
