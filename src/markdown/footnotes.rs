//! Footnote definition collection (rendered lazily on first reference).

use crate::config::Config;
use crate::log;
use anyhow::Result;
use comrak::nodes::{AstNode, NodeValue};
use std::collections::HashMap;
use std::path::PathBuf;

use super::inline::render_inline;
use super::state::RenderState;

pub(crate) fn collect_footnotes<'a>(
    root: &'a AstNode<'a>,
    cfg: &Config,
) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for node in root.descendants() {
        if let NodeValue::FootnoteDefinition(def) = &node.data.borrow().value {
            let name = def.name.clone();
            let content = render_footnote_content(node, cfg)?;
            map.insert(name, content);
        }
    }
    Ok(map)
}

fn render_footnote_content<'a>(node: &'a AstNode<'a>, cfg: &Config) -> Result<String> {
    let mut out = String::new();
    let mut local = RenderState::new(HashMap::new(), PathBuf::new(), None);
    for child in node.children() {
        let v = child.data.borrow().value.clone();
        match v {
            NodeValue::Paragraph => {
                if !out.is_empty() {
                    out.push(' ');
                }
                let mut p = String::new();
                render_inline(child, &mut p, cfg, &mut local)?;
                out.push_str(p.trim());
            }
            _ => {
                log::warn("footnote with non-paragraph content: only inline content is rendered");
            }
        }
    }
    Ok(out.trim().to_string())
}
