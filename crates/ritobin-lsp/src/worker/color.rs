use lsp_types::{
    Color, ColorInformation, ColorPresentation, PartialResultParams, Range, TextEdit,
    WorkDoneProgressParams,
};
use ltk_ritobin::ast::{
    Value,
    visitor::{Descend, EnterFlow, Visitor, VisitorExt},
};
use ritobin_lsp::line_ends::LineNumbers;

use crate::worker::{Unparsed, Worker};

impl Worker {
    pub(super) fn document_colors(
        &self,
        _work_done_progress_params: WorkDoneProgressParams,
        _partial_result_params: PartialResultParams,
    ) -> Result<Vec<ColorInformation>, Unparsed> {
        let ast = self.ast()?;
        Ok(ColorVisitor {
            lines: &self.document.line_numbers,
            colors: Vec::new(),
        }
        .walk(ast)
        .colors)
    }

    pub(super) fn color_presentations(&self, color: Color, range: Range) -> Vec<ColorPresentation> {
        let map = |component: f32| (component * 255.0).round().clamp(0.0, 255.0) as u8;
        let label = format!(
            "{{ {}, {}, {}, {} }}",
            map(color.red),
            map(color.green),
            map(color.blue),
            map(color.alpha),
        );
        vec![ColorPresentation {
            text_edit: Some(TextEdit {
                range,
                new_text: label.clone(),
            }),
            label,
            additional_text_edits: None,
        }]
    }
}

struct ColorVisitor<'a> {
    lines: &'a LineNumbers,
    colors: Vec<ColorInformation>,
}

impl Visitor for ColorVisitor<'_> {
    fn enter_value(&mut self, value: &Value) -> EnterFlow {
        if let Value::Color(c) = value {
            self.colors.push(ColorInformation {
                range: self.lines.from_span(c.span),
                color: Color {
                    red: c.r as f32 / 255.0,
                    green: c.g as f32 / 255.0,
                    blue: c.b as f32 / 255.0,
                    alpha: c.a as f32 / 255.0,
                },
            });
        }
        Descend::Children.into()
    }
}
