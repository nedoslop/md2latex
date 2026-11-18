//! GFM table rendering with `<<` (colspan) / `^^` (rowspan) merging,
//! breakable `xltabular` and fixed `tabularx` variants.

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeValue, TableAlignment};
use std::collections::{HashMap, HashSet};

use super::inline::render_inline;
use super::state::{PendingTableCaption, RenderState};
use super::util::{compute_baseline, escape_latex, extract_text, format_caption};

#[derive(Clone)]
struct Cell {
    colspan: usize,
    rowspan: usize,
    content: String,
    phantom: bool,
    align: TableAlignment,
}

type Row = Vec<Cell>;

pub(crate) fn parse_table_meta(after: &str) -> (Option<String>, String, bool, bool) {
    let mut label = None;
    let mut caption_tokens = Vec::new();
    let mut breakable = true;
    let mut new_page = false;
    for token in after.split_whitespace() {
        if label.is_none() && !token.contains('=') && token != "breakable" && token != "new_page" {
            label = Some(format!("tbl:{}", token));
        } else if token == "breakable=false" || token == "breakable=0" {
            breakable = false;
        } else if token == "breakable=true" || token == "breakable=1" || token == "breakable" {
            breakable = true;
        } else if token == "new_page=true" || token == "new_page=1" || token == "new_page" {
            new_page = true;
        } else if token == "new_page=false" || token == "new_page=0" {
            new_page = false;
        } else {
            caption_tokens.push(token);
        }
    }
    (label, caption_tokens.join(" "), breakable, new_page)
}

fn alignment_cmd(align: TableAlignment) -> &'static str {
    match align {
        TableAlignment::Left => "\\raggedright",
        TableAlignment::Right => "\\raggedleft",
        _ => "\\centering",
    }
}

fn merge_columns(raw: &[String], alignments: &[TableAlignment]) -> Row {
    let mut result: Row = Vec::with_capacity(raw.len());
    for (i, cell) in raw.iter().enumerate() {
        let align = alignments.get(i).copied().unwrap_or(TableAlignment::Center);
        let s = cell.trim();
        if s == "<<" {
            if let Some(last) = result.last_mut() {
                last.colspan += 1;
            } else {
                log::warn("'<<' in first cell of a table row has nothing to merge with");
                result.push(Cell {
                    colspan: 1,
                    rowspan: 1,
                    content: String::new(),
                    phantom: false,
                    align,
                });
            }
        } else {
            result.push(Cell {
                colspan: 1,
                rowspan: 1,
                content: cell.clone(),
                phantom: false,
                align,
            });
        }
    }
    result
}

fn fold_rowspans(rows: &mut Vec<Row>) {
    let mut active: HashMap<usize, (usize, usize)> = HashMap::new();
    for i in 0..rows.len() {
        let mut col = 0usize;
        let row_len = rows[i].len();
        for j in 0..row_len {
            let cspan = rows[i][j].colspan;
            let is_marker = !rows[i][j].phantom && rows[i][j].content.trim() == "^^";
            if is_marker {
                let mut target: Option<(usize, usize)> = None;
                let mut consistent = true;
                for c in col..(col + cspan) {
                    match active.get(&c) {
                        Some(&(sr, sc)) => {
                            if let Some(t) = target {
                                if t != (sr, sc) {
                                    consistent = false;
                                    break;
                                }
                            } else {
                                target = Some((sr, sc));
                            }
                        }
                        None => {
                            consistent = false;
                            break;
                        }
                    }
                }
                if consistent && target.is_some() {
                    let (sr, sc) = target.unwrap();
                    let target_cell = &rows[sr][sc];
                    let mut conflict = false;
                    if target_cell.colspan > 1 {
                        let target_start: usize = rows[sr].iter().take(sc).map(|c| c.colspan).sum();
                        let target_end = target_start + target_cell.colspan;
                        let mut cur_c = 0;
                        for (k, cell) in rows[i].iter().enumerate() {
                            if k != j && cur_c < target_end && cur_c + cell.colspan > target_start {
                                conflict = true;
                                break;
                            }
                            cur_c += cell.colspan;
                        }
                    }
                    if conflict {
                        log::warn(&format!(
                            "'^^' in table row {} (col {}) cannot merge with a cell spanning \
                             multiple columns because the current row has content in those \
                             columns; treating as empty cell",
                            i, col
                        ));
                        rows[i][j].content.clear();
                        for c in col..(col + cspan) {
                            active.insert(c, (i, j));
                        }
                    } else {
                        rows[sr][sc].rowspan += 1;
                        rows[i][j].phantom = true;
                        rows[i][j].content.clear();
                    }
                } else {
                    log::warn(&format!(
                        "'^^' in table row {} (col {}) cannot be resolved; \
                         treating as empty cell",
                        i, col
                    ));
                    rows[i][j].content.clear();
                    for c in col..(col + cspan) {
                        active.insert(c, (i, j));
                    }
                }
            } else {
                for c in col..(col + cspan) {
                    active.insert(c, (i, j));
                }
            }
            col += cspan;
        }
    }
}

fn active_columns_at_hline(rows: &[Row], i: usize, ncols: usize) -> Vec<usize> {
    let mut active = vec![false; ncols];
    for (s, row) in rows.iter().enumerate() {
        if s > i {
            break;
        }
        let mut c = 0usize;
        for cell in row {
            if !cell.phantom && cell.rowspan >= 2 && s + cell.rowspan - 1 > i {
                for k in c..(c + cell.colspan).min(ncols) {
                    active[k] = true;
                }
            }
            c += cell.colspan;
        }
    }
    active
        .iter()
        .enumerate()
        .filter(|(_, a)| **a)
        .map(|(k, _)| k)
        .collect()
}

fn multicolumn_spec(n: usize, align_cmd: &str) -> String {
    let ts = (n - 1) * 2;
    let rw = n - 1;
    format!(
        ">{{\\hsize=\\dimexpr {n}\\hsize + {ts}\\tabcolsep + {rw}\\arrayrulewidth\\relax\
         {algn}\\arraybackslash\\leavevmode}}X",
        n = n,
        ts = ts,
        rw = rw,
        algn = align_cmd,
    )
}

fn emit_cell(cell: &Cell, is_first: bool) -> String {
    if cell.phantom && cell.colspan == 1 {
        return String::new();
    }
    let inner = if cell.phantom {
        String::new()
    } else if cell.rowspan > 1 {
        format!("\\multirow{{{}}}{{*}}{{{}}}", cell.rowspan, cell.content)
    } else {
        cell.content.clone()
    };
    if cell.colspan > 1 {
        let colspec = multicolumn_spec(cell.colspan, alignment_cmd(cell.align));
        let spec = if is_first {
            format!("|{}|", colspec)
        } else {
            format!("{}|", colspec)
        };
        format!("\\multicolumn{{{}}}{{{}}}{{{}}}", cell.colspan, spec, inner)
    } else {
        inner
    }
}

fn emit_row(row: &Row) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(row.len());
    for (i, cell) in row.iter().enumerate() {
        parts.push(emit_cell(cell, i == 0));
    }
    parts.join(" & ")
}

fn emit_header_row(row: &Row) -> String {
    let bold: Row = row
        .iter()
        .map(|c| Cell {
            colspan: c.colspan,
            rowspan: c.rowspan,
            content: if c.phantom {
                String::new()
            } else {
                format!("\\textbf{{{}}}", c.content)
            },
            phantom: c.phantom,
            align: c.align,
        })
        .collect();
    emit_row(&bold)
}

fn emit_trailing_rule(out: &mut String, processed: &[Row], row_idx: usize, ncols: usize) {
    let skip = active_columns_at_hline(processed, row_idx, ncols);
    if skip.is_empty() {
        out.push_str(" \\\\\n\\hline\n");
        return;
    }
    let skip_set: HashSet<usize> = skip.iter().copied().collect();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut k = 0usize;
    while k < ncols {
        if skip_set.contains(&k) {
            k += 1;
        } else {
            let start = k;
            while k < ncols && !skip_set.contains(&k) {
                k += 1;
            }
            ranges.push((start, k - 1));
        }
    }
    out.push_str(" \\\\\n");
    for (s, e) in ranges {
        out.push_str(&format!("  \\cline{{{}-{}}}\n", s + 1, e + 1));
    }
}

pub(crate) fn render_table<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    meta: Option<PendingTableCaption>,
    cfg: &Config,
    state: &mut RenderState<'_>,
) -> Result<()> {
    let alignments: Vec<TableAlignment> = match &node.data.borrow().value {
        NodeValue::Table(t) => t.alignments.clone(),
        _ => Vec::new(),
    };
    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    for row_node in node.children() {
        if let NodeValue::TableRow(_) = &row_node.data.borrow().value {
            let mut cells = Vec::new();
            for cell_node in row_node.children() {
                if let NodeValue::TableCell = &cell_node.data.borrow().value {
                    let raw_text = extract_text(cell_node).trim().to_string();
                    if raw_text == "<<" || raw_text == "^^" {
                        cells.push(raw_text);
                    } else {
                        let mut cell_latex = String::new();
                        render_inline(cell_node, &mut cell_latex, cfg, state)?;
                        cells.push(cell_latex);
                    }
                }
            }
            raw_rows.push(cells);
        }
    }
    if raw_rows.is_empty() {
        return Ok(());
    }
    let ncols = raw_rows[0].len().max(1);
    let baseline = compute_baseline(&cfg.style.table_font_size);
    let double_rule = format!("\\hhline{{|{}|}}", "=".repeat(ncols));
    let (caption_latex, label_latex, breakable, new_page) = match meta {
        Some(m) => (
            format_caption(m.caption.trim_end_matches('.')),
            m.label.unwrap_or_default(),
            m.breakable,
            m.new_page,
        ),
        None => (String::new(), String::new(), true, false),
    };
    let mut processed: Vec<Row> = raw_rows
        .iter()
        .map(|r| merge_columns(r, &alignments))
        .collect();
    fold_rowspans(&mut processed);
    if processed
        .first()
        .map_or(false, |r| r.iter().any(|c| c.rowspan > 1))
    {
        log::warn(
            "multirow in table header is not supported; \
             header will be emitted without rowspan",
        );
        for c in processed[0].iter_mut() {
            c.rowspan = 1;
        }
    }
    let col_spec: String = (0..ncols)
        .map(|i| match alignments.get(i) {
            Some(TableAlignment::Left) => "L",
            Some(TableAlignment::Right) => "R",
            _ => "C",
        })
        .collect::<Vec<_>>()
        .join("|");
    if breakable {
        if new_page {
            out.push_str("\\clearpage\n");
        }
        out.push_str("  \\stepcounter{ltid}\n");
        out.push_str(&format!(
            "  \\ltSetLabels{{{}}}{{{}}}\n",
            escape_latex(&cfg.labels.table_continued),
            escape_latex(&cfg.labels.table_ended)
        ));
        out.push_str(&format!(
            "  {{\\fontsize{{{}}}{{{}}}\\selectfont\n",
            cfg.style.table_font_size, baseline
        ));
        out.push_str("  \\begin{xltabular}{\\textwidth}{|");
        out.push_str(&col_spec);
        out.push_str("|}\n");
        let mut cap_line = String::new();
        if !caption_latex.is_empty() {
            cap_line.push_str(&format!("\\caption{{{}}}", caption_latex));
        }
        if !label_latex.is_empty() {
            if !cap_line.is_empty() {
                cap_line.push(' ');
            }
            cap_line.push_str(&format!("\\label{{{}}}", label_latex));
        }
        if !cap_line.is_empty() {
            out.push_str(&format!("  {} \\\\\n", cap_line));
        }
        let header = emit_header_row(&processed[0]);
        out.push_str("  \\hline\n");
        out.push_str(&header);
        out.push_str(&format!(" \\\\\n{}\n\\endfirsthead\n", double_rule));
        out.push_str("  \\hline\n");
        out.push_str(&header);
        out.push_str(&format!(" \\\\\n{}\n\\endhead\n", double_rule));
        out.push_str("  \\endfoot\n");
        out.push_str("  \\endlastfoot\n");
        let total_data = processed.len().saturating_sub(1);
        for (data_idx, row) in processed.iter().skip(1).enumerate() {
            let source_row = data_idx + 1;
            let mut row_data = row.clone();
            if data_idx + 1 == total_data {
                if let Some(last_cell) = row_data.last_mut() {
                    if !last_cell.phantom {
                        last_cell.content.push_str("\\label{ltend:\\arabic{ltid}}");
                    }
                }
            }
            out.push_str("  ");
            out.push_str(&emit_row(&row_data));
            emit_trailing_rule(out, &processed, source_row, ncols);
        }
        out.push_str("  \\end{xltabular}\n}\n");
    } else {
        out.push_str("\\begin{table}[H]\n\\centering\n");
        if cfg.style.table_caption_above {
            if !caption_latex.is_empty() {
                out.push_str(&format!("  \\caption{{{}}}\n", caption_latex));
            }
            if !label_latex.is_empty() {
                out.push_str(&format!("  \\label{{{}}}\n", label_latex));
            }
        }
        out.push_str(&format!(
            "  {{\\fontsize{{{}}}{{{}}}\\selectfont\n",
            cfg.style.table_font_size, baseline
        ));
        out.push_str("  \\begin{tabularx}{\\textwidth}{|");
        out.push_str(&col_spec);
        out.push_str("|}\n\\hline\n");
        out.push_str(&emit_header_row(&processed[0]));
        out.push_str(" \\\\\n\\hline\n");
        for row in processed.iter().skip(1) {
            out.push_str("  ");
            out.push_str(&emit_row(row));
            out.push_str(" \\\\\n\\hline\n");
        }
        out.push_str("  \\end{tabularx}\n}\n");
        if !cfg.style.table_caption_above {
            if !caption_latex.is_empty() {
                out.push_str(&format!("  \\caption{{{}}}\n", caption_latex));
            }
            if !label_latex.is_empty() {
                out.push_str(&format!("  \\label{{{}}}\n", label_latex));
            }
        }
        out.push_str("\\end{table}\n");
    }
    Ok(())
}