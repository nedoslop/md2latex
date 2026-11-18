//! List rendering: itemize, enumerate, and the bibliography list.

use crate::config::Config;
use anyhow::Result;
use comrak::nodes::{AstNode, ListType, NodeValue};

use super::inline::render_inline;
use super::state::RenderState;

pub(crate) fn render_list<'a>(
    node: &'a AstNode<'a>,
    out: &mut String,
    cfg: &Config,
    state: &mut RenderState<'_>,
) -> Result<()> {
    let list_data = match &node.data.borrow().value {
        NodeValue::List(l) => l.clone(),
        _ => return Ok(()),
    };
    let is_ordered = list_data.list_type == ListType::Ordered;
    let env = if is_ordered && state.in_bibliography {
        "stubiblist"
    } else if is_ordered {
        "enumerate"
    } else {
        "itemize"
    };
    out.push_str(&format!("\\begin{{{}}}\n", env));
    for item_node in node.children() {
        if !matches!(&item_node.data.borrow().value, NodeValue::Item(_)) {
            continue;
        }
        out.push_str("  \\item ");
        let mut first_block = true;
        for block in item_node.children() {
            let bv = block.data.borrow().value.clone();
            match &bv {
                NodeValue::Paragraph => {
                    if !first_block {
                        out.push('\n');
                    }
                    let mut p = String::new();
                    render_inline(block, &mut p, cfg, state)?;
                    out.push_str(p.trim());
                    first_block = false;
                }
                NodeValue::List(_) => {
                    out.push('\n');
                    render_list(block, out, cfg, state)?;
                    first_block = false;
                }
                _ => {
                    if !first_block {
                        out.push('\n');
                    }
                    let mut p = String::new();
                    render_inline(block, &mut p, cfg, state)?;
                    out.push_str(p.trim());
                    first_block = false;
                }
            }
        }
        out.push('\n');
    }
    out.push_str(&format!("\\end{{{}}}\n", env));
    Ok(())
}