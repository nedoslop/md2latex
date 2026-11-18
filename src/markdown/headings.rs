//! Heading rendering: structural H1s, numbered sections, appendices.

use crate::config::Config;
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
    let clean = strip_heading_number(&raw_text)
        .trim_end_matches('.')
        .to_string();
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
            out.push_str(&format!(
                "\\section{{{}}}\n",
                escape_latex(&to_title_case_if_upper(&clean))
            ));
        }
    } else {
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
            out.push_str(&format!(
                "\\textbf{{{}}}\\par\n",
                escape_latex(&to_title_case_if_upper(&clean))
            ));
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
        let display = to_title_case_if_upper(&clean);
        if cmd == "section" {
            if state.skip_next_section_pagebreak {
                state.skip_next_section_pagebreak = false;
            } else {
                out.push_str("\\clearpage\n");
            }
        }
        if !numbered {
            let toc_cmd = cmd.trim_end_matches('*');
            out.push_str(&format!(
                "\\phantomsection\n\\addcontentsline{{toc}}{{{}}}{{{}}}\n\\{}{{{}}}\n",
                toc_cmd,
                escape_latex(&display),
                cmd,
                escape_latex(&display)
            ));
        } else {
            out.push_str(&format!("\\{}{{{}}}\n", cmd, escape_latex(&display)));
        }
    }
    Ok(())
}

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
    let display = if cfg.style.structural_heading_uppercase {
        text.trim().to_uppercase()
    } else {
        to_title_case_if_upper(text)
    };
    if cfg.style.main_part_silent
        && display.trim().to_uppercase() == cfg.style.main_part_heading.trim().to_uppercase()
    {
        return Ok(());
    }
    out.push_str(&format!(
        "\\phantomsection\n\
         \\addcontentsline{{toc}}{{section}}{{{}}}\n\
         {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n\
         \\vspace{{\\baselineskip}}\n",
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
        format!("{} {}", app_label, to_title_case_if_upper(title))
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
        out.push_str(&format!(
            "\\vspace{{0.5\\baselineskip}}\n\
             {{\\centering\\setlength{{\\parindent}}{{0pt}}\\textbf{{{}}}\\par}}\n",
            escape_latex(&to_title_case_if_upper(title))
        ));
    }
    out.push_str("\\vspace{\\baselineskip}\n");
    Ok(())
}

fn strip_heading_number(s: &str) -> String {
    let re = Regex::new(r"^\s*\d+(\.\d+)*\.?\s*").unwrap();
    re.replace(s, "").to_string()
}

fn to_title_case_if_upper(s: &str) -> String {
    if s.chars().all(|c| !c.is_lowercase()) {
        s.split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        s.to_string()
    }
}
