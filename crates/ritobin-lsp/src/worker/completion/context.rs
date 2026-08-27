use ltk_hash::BinHash;
use ltk_ritobin::{
    ast::{
        Ast, Property, Value,
        node::{NodeExt as _, SubNodeRef},
        query::{AstObjectDetail, AstPropertyDetail, AstRootEntryDetail},
    },
    parse::Span,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorContext {
    PropertyKey { class: BinHash },
    PropertyType { class: BinHash, property: BinHash },
    PropertyValue { class: BinHash, property: BinHash },
    ContainerItem { class: BinHash, property: BinHash },
}

pub struct CompletionContext {
    pub context: CursorContext,
    /// The span that should be replaced
    pub replace: Span,
}

impl CompletionContext {
    /// What a completion at `offset` should offer, and what it should replace.
    pub fn resolve(ast: &Ast, text: &str, offset: u32) -> Option<Self> {
        let path: Vec<SubNodeRef> = ast.fine_path_to(offset).collect();
        classify(&path, text, offset)
    }
}

fn classify(path: &[SubNodeRef], text: &str, offset: u32) -> Option<CompletionContext> {
    tracing::info!("{:?}", path.last());
    match *path.last()? {
        // object trivia means we're in the body of it's properties,
        // which means we want to recommend property keys
        SubNodeRef::Object(s, AstObjectDetail::Node | AstObjectDetail::Trivia) => {
            Some(CompletionContext {
                context: CursorContext::PropertyKey {
                    class: s.class_hash.value,
                },
                // ast properties don't exist when it's just a naked key (for now), so we
                // need to check for partially typed keys here
                replace: word_at(text, offset),
            })
        }
        // On an existing key of a property, a completion includes the entire
        // `key: type = ` prefix, so we can replace the old one entirely.
        SubNodeRef::Property(p, AstPropertyDetail::Name) => Some(CompletionContext {
            context: CursorContext::PropertyKey {
                class: enclosing_class(path, path.len() - 1)?,
            },
            replace: key_prefix(p),
        }),
        SubNodeRef::Property(p, AstPropertyDetail::TypeExpr) => Some(CompletionContext {
            context: CursorContext::PropertyType {
                class: enclosing_class(path, path.len() - 1)?,
                property: p.name.value,
            },
            replace: filter_empty(p.type_expr.span, text).unwrap_or(Span::empty(offset)),
        }),

        SubNodeRef::Value(_)
        | SubNodeRef::Object(_, AstObjectDetail::ClassHash)
        | SubNodeRef::RootEntry(_, AstRootEntryDetail::PathHash) => {
            classify_value(path, text, offset)
        }

        // object/property trivia is dead space we don't care about, Node shouldn't show up in the
        // final noderef in the path
        SubNodeRef::RootEntry(_, AstRootEntryDetail::Trivia | AstRootEntryDetail::Node)
        | SubNodeRef::Property(_, AstPropertyDetail::Trivia | AstPropertyDetail::Node) => None,
    }
}

fn classify_value(path: &[SubNodeRef], text: &str, offset: u32) -> Option<CompletionContext> {
    // The deepest value on the path is the one the cursor is actually in.
    let (vi, value) = path.iter().enumerate().rev().find_map(|(i, n)| match n {
        SubNodeRef::Value(v) => Some((i, *v)),
        _ => None,
    })?;

    // A container item / property value always belongs to some owning property.
    let (pi, property) = path.iter().enumerate().rev().find_map(|(i, n)| match n {
        SubNodeRef::Property(p, _) => Some((i, *p)),
        _ => None,
    })?;
    let class = enclosing_class(path, pi)?;
    let property = property.name.value;

    let context = match *path.get(vi.checked_sub(1)?)? {
        // Descended into a map entry: keys aren't completable, only values.
        SubNodeRef::Value(Value::Map { entries, .. }) => {
            if entries.iter().any(|(key, _)| std::ptr::eq(key, value)) {
                return None;
            }
            CursorContext::ContainerItem { class, property }
        }
        SubNodeRef::Value(
            Value::Container { .. } | Value::UnorderedContainer { .. } | Value::Optional { .. },
        ) => CursorContext::ContainerItem { class, property },
        // The value belongs directly to the property. A container value offers
        // its items (once past the opening brace); anything else is the value.
        SubNodeRef::Property(..) if value.is_containerlike() => {
            if offset <= value.span().start {
                return None;
            }
            CursorContext::ContainerItem { class, property }
        }
        SubNodeRef::Property(..) => CursorContext::PropertyValue { class, property },
        _ => return None,
    };

    Some(CompletionContext {
        context,
        replace: if value.is_containerlike() {
            // we still need to worry about partially typed keys as before
            word_at(text, offset)
        } else {
            filter_empty(value.span(), text).unwrap_or(Span::empty(offset))
        },
    })
}

/// The span that covers `[key: type = ]value`
fn key_prefix(p: &Property) -> Span {
    let end = match &p.value {
        Some(value) => value.span().start,
        None if p.type_expr.value.is_some() => p.type_expr.span.end,
        None => p.name.span().end,
    };
    Span::new(p.name.span().start, end)
}

fn enclosing_class(path: &[SubNodeRef], before: usize) -> Option<BinHash> {
    path[..before]
        .iter()
        .rev()
        .find_map(|n| n.class_hash())
        .map(|h| h.value)
}

/// Return the span iff it contains some non-whitespace characters
fn filter_empty(span: Span, text: &str) -> Option<Span> {
    text[span]
        .chars()
        .any(|c| !c.is_whitespace())
        .then_some(span)
}

/// Find the largest continuous span of ASCII alphanumeric characters that covers this offset.
/// Returns an empty span at the given offset if no alphanumeric characters are found.
fn word_at(text: &str, offset: u32) -> Span {
    let bytes = text.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    let mut start = (offset as usize).min(bytes.len());
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = (offset as usize).min(bytes.len());
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }
    Span::new(start as u32, end as u32)
}
