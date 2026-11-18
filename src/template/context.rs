//! Template context helpers.

use crate::config::{Config, Student};
use crate::markdown::escape_latex;
use tera::Context as TeraContext;

pub(crate) fn collect_author(t: &crate::config::Title) -> String {
    let raw = if !t.student.trim().is_empty() {
        t.student.trim().to_string()
    } else {
        t.students
            .iter()
            .map(|s| s.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    };
    escape_latex(&raw)
}

pub(crate) fn fill_title_context(ctx: &mut TeraContext, cfg: &Config) {
    let t = &cfg.title;
    let fill = cfg.style.title_page_fill_empty;
    ctx.insert("university", &escape_latex(&t.university));
    ctx.insert("institute", &escape_latex(&t.institute));
    ctx.insert("department", &escape_latex(&t.department));
    ctx.insert("doc_kind", &escape_latex(&t.doc_kind));
    ctx.insert("direction_code", &escape_latex(&t.direction_code));
    ctx.insert("direction_name", &escape_latex(&t.direction_name));
    ctx.insert("topic", &escape_latex(&t.topic));

    // Legacy single-student fields.
    ctx.insert("student", &fill_or_blank(&t.student, fill));
    ctx.insert("student_id", &escape_latex(&t.student_id));
    ctx.insert("group", &fill_or_blank(&t.group, fill));

    // Normalised students list (always at least one entry).
    let students_ctx: Vec<Student> = if !t.students.is_empty() {
        t.students
            .iter()
            .map(|s| Student {
                name: fill_or_blank(&s.name, fill),
                group: escape_latex(&s.group),
                student_id: escape_latex(&s.student_id),
            })
            .collect()
    } else if !t.student.trim().is_empty()
        || !t.group.trim().is_empty()
        || !t.student_id.trim().is_empty()
    {
        vec![Student {
            name: fill_or_blank(&t.student, fill),
            group: escape_latex(&t.group),
            student_id: escape_latex(&t.student_id),
        }]
    } else {
        vec![Student {
            name: if fill {
                "________".into()
            } else {
                String::new()
            },
            group: String::new(),
            student_id: String::new(),
        }]
    };
    ctx.insert("students", &students_ctx);
    ctx.insert("supervisor", &fill_or_blank(&t.supervisor, fill));
    ctx.insert(
        "supervisor_title",
        &fill_or_blank(&t.supervisor_title, fill),
    );
    ctx.insert("reviewer", &fill_or_blank(&t.reviewer, fill));
    ctx.insert("year", &escape_latex(&t.year));
    ctx.insert("city", &escape_latex(&t.city));
}

/// Escapes `value` (unless it is empty and `fill` is true — then a
/// signature rule is emitted instead).
fn fill_or_blank(value: &str, fill: bool) -> String {
    let v = value.trim();
    if v.is_empty() && fill {
        "\\rule{3.5cm}{0.4pt}".to_string()
    } else {
        escape_latex(v)
    }
}

pub(crate) fn compute_baseline(size: &str) -> String {
    let s = size.trim();
    if let Some(num) = s
        .strip_suffix("pt")
        .and_then(|n| n.trim().parse::<f32>().ok())
    {
        format!("{}pt", (num * 1.2 * 10.0).round() / 10.0)
    } else {
        "16.8pt".to_string()
    }
}
