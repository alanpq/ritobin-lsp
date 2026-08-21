pub mod context;
pub mod items;

use lsp_types::{CompletionItem, CompletionResponse};
use ltk_mimir_cache::Table;

use crate::worker::{
    CompletionRequest, Worker,
    completion::{
        context::CursorContext,
        items::{property_items, type_item, value_items, value_slot},
    },
};

use meta_wiki::schema::U32Hash;

impl Worker {
    pub fn complete(&self, req: CompletionRequest) -> anyhow::Result<Option<CompletionResponse>> {
        let doc = &self.document;
        let Some(cst) = self.cst.as_ref() else {
            return Ok(None);
        };

        let offset = doc.line_numbers.from_position(&req.position);
        let Some(context) = context::resolve(cst, &doc.text, offset) else {
            return Ok(None);
        };

        let guard = self.server.meta.classes.read().unwrap();
        let classes = &*guard;

        let table = |table| self.server.hashes.as_ref().and_then(|h| h.table(table));
        let (fields, types) = (table(Table::BinFields), table(Table::BinTypes));
        let field_name = |hash: U32Hash| fields.as_ref().and_then(|db| db.get(hash.0.into()));
        let type_name = |hash: U32Hash| types.as_ref().and_then(|db| db.get(hash.0.into()));

        let snippets = self.server.config.caps().completion_snippet();
        let values = |class, property, element| {
            value_slot(classes, class, property, element)
                .map(|slot| value_items(classes, slot, type_name, snippets))
                .unwrap_or_default()
        };

        let items: Vec<CompletionItem> = match context {
            CursorContext::PropertyKey { class } => property_items(classes, class, field_name),
            CursorContext::PropertyType { class, property } => {
                type_item(classes, class, property).into_iter().collect()
            }
            CursorContext::PropertyValue { class, property } => values(class, property, false),
            CursorContext::ContainerItem { class, property } => values(class, property, true),
        };

        Ok(Some(CompletionResponse::Array(items)))
    }
}
