//! Shared rendering context passed through all renderer modules.

use crate::plantuml::PlantUml;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Ctx {
    Ordinary,
    StructuralNumbered,
    StructuralUnnumbered,
    Appendix,
}

pub(crate) struct PendingTableCaption {
    pub(crate) label: Option<String>,
    pub(crate) caption: String,
    pub(crate) breakable: bool,
    pub(crate) new_page: bool,
}

pub(crate) struct RenderState<'a> {
    pub(crate) ctx: Ctx,
    pub(crate) pending_table: Option<PendingTableCaption>,
    pub(crate) pending_equation_label: Option<String>,
    pub(crate) skip_next_section_pagebreak: bool,
    pub(crate) in_bibliography: bool,
    pub(crate) footnotes: HashMap<String, String>,
    pub(crate) footnote_numbers: HashMap<String, usize>,
    pub(crate) footnote_counter: usize,
    pub(crate) md_dir: PathBuf,
    pub(crate) plantuml: Option<&'a PlantUml>,
    pub(crate) skip_puml_images: bool,
}

impl<'a> RenderState<'a> {
    pub(crate) fn new(
        footnotes: HashMap<String, String>,
        md_dir: PathBuf,
        plantuml: Option<&'a PlantUml>,
    ) -> Self {
        Self {
            ctx: Ctx::Ordinary,
            pending_table: None,
            pending_equation_label: None,
            skip_next_section_pagebreak: false,
            in_bibliography: false,
            footnotes,
            footnote_numbers: HashMap::new(),
            footnote_counter: 0,
            md_dir,
            plantuml,
            skip_puml_images: false,
        }
    }
}