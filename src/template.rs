use crate::config::Config;
use anyhow::{Context, Result};
use tera::{Context as TeraContext, Tera};

pub fn render_document(cfg: &Config, body: &str) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("base.tex.tera", include_str!("../templates/base.tex.tera"))?;
    tera.add_raw_template(
        "title_bachelor.tex.tera",
        include_str!("../templates/title_bachelor.tex.tera"),
    )?;
    tera.add_raw_template(
        "title_master.tex.tera",
        include_str!("../templates/title_master.tex.tera"),
    )?;
    tera.add_raw_template(
        "title_course.tex.tera",
        include_str!("../templates/title_course.tex.tera"),
    )?;

    let title_template = match cfg.project.doc_type.as_str() {
        "vkr_master" => "title_master.tex.tera",
        "course_work" => "title_course.tex.tera",
        _ => "title_bachelor.tex.tera",
    };

    let mut ctx = TeraContext::new();
    fill_title_context(&mut ctx, cfg);
    let title_rendered = tera.render(title_template, &ctx)?;

    // Общие данные для базового шаблона
    ctx.insert("title_page", &title_rendered);
    ctx.insert("body", body);

    ctx.insert("font", &cfg.style.font);
    ctx.insert("font_fallback", &cfg.style.font_fallback);
    ctx.insert("font_size", &cfg.style.font_size);
    ctx.insert("margin_left", &cfg.style.margin_left);
    ctx.insert("margin_right", &cfg.style.margin_right);
    ctx.insert("margin_top", &cfg.style.margin_top);
    ctx.insert("margin_bottom", &cfg.style.margin_bottom);
    ctx.insert("paragraph_indent", &cfg.style.paragraph_indent);
    ctx.insert("line_spacing", &cfg.style.line_spacing);
    ctx.insert("numbering", &cfg.style.numbering);
    ctx.insert("section_bold", &cfg.style.section_bold);
    ctx.insert("figure_caption_below", &cfg.style.figure_caption_below);
    ctx.insert("table_caption_above", &cfg.style.table_caption_above);

    ctx.insert("use_biblatex", &cfg.bibliography.use_biblatex);
    ctx.insert(
        "bib_file",
        &cfg.project
            .bib_file
            .clone()
            .unwrap_or_else(|| "refs.bib".into()),
    );

    // Метаданные PDF
    ctx.insert("topic", &cfg.title.topic);
    ctx.insert(
        "student_author",
        &fill_or_blank(&cfg.title.student, cfg.style.title_page_fill_empty),
    );

    tera.render("base.tex.tera", &ctx)
        .context("Ошибка рендеринга base.tex.tera")
}

fn fill_title_context(ctx: &mut TeraContext, cfg: &Config) {
    let t = &cfg.title;
    let fill = cfg.style.title_page_fill_empty;
    ctx.insert("university", &t.university);
    ctx.insert("institute", &t.institute);
    ctx.insert("department", &t.department);
    ctx.insert("doc_kind", &t.doc_kind);
    ctx.insert("direction_code", &t.direction_code);
    ctx.insert("direction_name", &t.direction_name);
    ctx.insert("topic", &t.topic);
    ctx.insert("student", &fill_or_blank(&t.student, fill));
    ctx.insert("student_id", &t.student_id); // ← добавить
    ctx.insert("group", &fill_or_blank(&t.group, fill));
    ctx.insert("supervisor", &fill_or_blank(&t.supervisor, fill));
    ctx.insert(
        "supervisor_title",
        &fill_or_blank(&t.supervisor_title, fill),
    );
    ctx.insert("reviewer", &fill_or_blank(&t.reviewer, fill));
    ctx.insert("year", &t.year);
    ctx.insert("city", &t.city);
}

fn fill_or_blank(value: &str, fill: bool) -> String {
    if value.trim().is_empty() && fill {
        "________".to_string()
    } else {
        value.to_string()
    }
}
