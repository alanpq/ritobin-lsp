use std::{fmt::Debug, ops::ControlFlow};

use lsp_types::{DocumentSymbol, PartialResultParams, WorkDoneProgressParams};

use ltk_ritobin::{
    ast::{
        Object, Property, Value,
        visitor::{Break, Continue, Descend, EnterFlow, ExitFlow, Visitor, VisitorExt},
    },
    parse::Span,
};
use ritobin_lsp::line_ends::LineNumbers;

use crate::{
    document::Document,
    worker::{Unparsed, Worker},
};

mod kind;
pub use kind::*;

mod limit;
pub use limit::*;

mod text;
pub use text::*;

impl Worker {
    pub(super) fn symbols(
        &self,
        _work_done_progress_params: WorkDoneProgressParams,
        _partial_result_params: PartialResultParams,
    ) -> Result<Symbols<DocumentSymbol>, Unparsed> {
        let doc = &self.document;
        let ast = self.ast()?;
        Ok(SymbolVisitor::new(doc)
            .limit_symbols(100_000)
            .limit_depth(20)
            .walk(ast)
            .finish()
            .into_lsp(&doc.text, &doc.line_numbers))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Symbols<S = Symbol> {
    pub symbols: Vec<S>,
    pub total_limit_reached: Option<(usize, bool)>,
    pub depth_limit_reached: Option<(usize, bool)>,
}

impl Symbols<Symbol> {
    pub fn into_lsp(self, text: &str, lines: &LineNumbers) -> Symbols<DocumentSymbol> {
        Symbols {
            symbols: self
                .symbols
                .into_iter()
                .map(|s| s.into_lsp(text, lines))
                .collect(),
            total_limit_reached: self.total_limit_reached,
            depth_limit_reached: self.depth_limit_reached,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum Action {
    #[default]
    NoOp,
    PushedListObject,
    HitDepthLimit,
}

pub struct SymbolVisitor<'a> {
    document: &'a Document,
    symbols: Vec<Symbol>,
    action_stack: Vec<Action>,
    symbol_count: usize,
    cursor: Vec<usize>,
    total_limit: Limit,
    depth_limit: Limit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: Text<String>,
    pub detail: Option<Text<String>>,
    pub kind: SymbolKind,
    pub range: Span,
    pub selection_range: Span,
    pub children: Option<Vec<Symbol>>,
}
impl Symbol {
    pub fn into_lsp(self, text: &str, lines: &LineNumbers) -> DocumentSymbol {
        let Self {
            name,
            detail,
            kind,
            range,
            selection_range,
            children,
        } = self;
        DocumentSymbol {
            name: name.into_string(text),
            detail: detail.map(|d| d.into_string(text)),
            kind: kind.into(),
            tags: None,
            #[allow(deprecated, reason = "we don't use it")]
            deprecated: None,
            range: lines.from_span(range),
            selection_range: lines.from_span(selection_range),
            children: children.map(|c| c.into_iter().map(|s| s.into_lsp(text, lines)).collect()),
        }
    }
}

impl<'a> SymbolVisitor<'a> {
    pub fn new(document: &'a Document) -> Self {
        Self {
            document,
            symbols: vec![],
            action_stack: vec![],
            symbol_count: 0,
            cursor: vec![],
            total_limit: Limit::default(),
            depth_limit: Limit::default(),
        }
    }

    pub fn limit_symbols(mut self, count: usize) -> Self {
        self.total_limit.max = Some(count);
        self
    }
    pub fn limit_depth(mut self, depth: usize) -> Self {
        self.depth_limit.max = Some(depth);
        self
    }

    fn get_cursor(&mut self) -> Option<&mut Symbol> {
        let mut cursor = self.cursor.iter().copied();
        let mut symbol = self.symbols.get_mut(cursor.next()?)?;
        for i in cursor {
            symbol = symbol.children.as_mut()?.get_mut(i)?;
        }
        Some(symbol)
    }

    fn check_total(&mut self) -> ControlFlow<Break> {
        match self.total_limit.update(self.symbol_count) {
            true => ControlFlow::Break(Break::Stop),
            false => ControlFlow::Continue(()),
        }
    }

    fn too_deep(&mut self) -> bool {
        self.depth_limit.update(self.cursor.len())
    }

    fn commit(&mut self, idx: Option<usize>) -> ControlFlow<Break> {
        self.check_total()?;
        if let Some(idx) = idx {
            self.cursor.push(idx);
        }
        self.symbol_count = self.symbol_count.saturating_add(1);
        ControlFlow::Continue(())
    }

    fn push(&mut self, symbol: Symbol, follow: bool) -> ControlFlow<Break, Action> {
        self.check_total()?;
        if self.too_deep() {
            return ControlFlow::Continue(Action::HitDepthLimit);
        }

        let Some(parent) = self.get_cursor() else {
            let idx = self.symbols.idx_push(symbol);
            self.commit(Some(idx))?;
            return ControlFlow::Continue(Action::NoOp);
        };
        let Some(children) = parent.children.as_mut() else {
            return ControlFlow::Continue(Action::NoOp);
        };

        if symbol.kind.as_simple() == Some(SimpleKind::Object) {
            if parent.kind.holds(SimpleKind::List) {
                let idx = children.idx_push(Symbol {
                    name: format!("[{}]", children.len()).into(),
                    detail: Some(symbol.name),
                    kind: SimpleKind::ListItem.into(),
                    ..symbol
                });
                self.commit(follow.then_some(idx))?;
                return ControlFlow::Continue(Action::PushedListObject);
            }
            return ControlFlow::Continue(Action::NoOp);
        }

        let can_have_children = symbol.children.is_some();
        let idx = children.idx_push(symbol);
        self.commit((follow && can_have_children).then_some(idx))?;
        ControlFlow::Continue(Action::NoOp)
    }

    fn enter_symbol(&mut self, symbol: Symbol) -> EnterFlow {
        match self.push(symbol, true)? {
            Action::HitDepthLimit => Descend::Skip,
            _ => Descend::Children,
        }
        .into()
    }

    fn extend(&mut self, symbols: impl IntoIterator<Item = Symbol>) -> EnterFlow {
        for s in symbols.into_iter() {
            self.push(s, false)?;
        }
        EnterFlow::Continue(Descend::Children)
    }

    fn object(&self, object: &Object) -> Symbol {
        Symbol {
            name: object.class_hash.span().into(),
            detail: None,
            kind: SimpleKind::Object.into(),
            range: object.span,
            selection_range: object.class_hash.span(),
            children: Some(vec![]),
        }
    }

    pub fn property(&self, prop: &Property) -> Symbol {
        Symbol {
            name: prop.name.span().into(),
            detail: prop
                .value
                .as_ref()
                .and_then(|v| Text::value_type(&self.document.text, v)),
            kind: SymbolKind::Property(prop.value.as_ref().and_then(SimpleKind::from_value)),
            range: prop.span(),
            selection_range: prop.name.span(),
            children: prop
                .value
                .as_ref()
                .is_some_and(can_have_children)
                .then(Vec::new),
        }
    }

    fn map_entry(&self, key: &Value, value: &Option<Value>) -> Symbol {
        Symbol {
            name: key.span().into(),
            detail: value
                .as_ref()
                .and_then(|v| Text::value_type(&self.document.text, v)),
            kind: SimpleKind::MapEntry.into(),
            range: match value {
                Some(val) => key.span().cover(val.span()),
                None => key.span(),
            },
            selection_range: key.span(),
            children: match value {
                Some(Value::Struct(obj) | Value::Embedded(obj)) => Some(
                    obj.properties
                        .iter()
                        .map(|prop| self.property(prop))
                        .collect(),
                ),
                _ => None,
            },
        }
    }

    pub fn finish(self) -> Symbols {
        Symbols {
            symbols: self.symbols,
            total_limit_reached: self.total_limit.report(),
            depth_limit_reached: self.depth_limit.report(),
        }
    }
}

fn can_have_children(value: &Value) -> bool {
    matches!(
        value,
        Value::Struct(_)
            | Value::Embedded(_)
            | Value::Container { .. }
            | Value::UnorderedContainer { .. }
            | Value::Map { .. }
    )
}

impl Visitor for SymbolVisitor<'_> {
    fn enter_root_entry(
        &mut self,
        object: &ltk_ritobin::ast::RootEntry,
    ) -> ltk_ritobin::ast::visitor::EnterFlow {
        self.cursor.clear();
        self.enter_symbol(Symbol {
            name: object.path_hash.span().into(),
            detail: Some(object.object.class_hash.span().into()),
            kind: SimpleKind::RootEntry.into(),
            range: object.span(),
            selection_range: object.path_hash.span(),
            children: Some(vec![]),
        })
    }

    fn enter_property(&mut self, prop: &ltk_ritobin::ast::Property) -> EnterFlow {
        let symbol = self.property(prop);
        self.enter_symbol(symbol)
    }
    fn exit_property(&mut self, prop: &ltk_ritobin::ast::Property) -> ExitFlow {
        if prop.value.as_ref().is_some_and(can_have_children) {
            self.cursor.pop();
        }
        Continue::Siblings.into()
    }

    fn enter_value(&mut self, value: &Value) -> EnterFlow {
        match value {
            Value::Struct(obj) | Value::Embedded(obj) => {
                match self.push(self.object(obj), true)? {
                    Action::HitDepthLimit => return EnterFlow::Continue(Descend::Skip),
                    action => {
                        self.action_stack.push(action);
                    }
                }
                Descend::Children.into()
            }
            Value::Map { entries, .. } => {
                let entries: Vec<Symbol> =
                    entries.iter().map(|(k, v)| self.map_entry(k, v)).collect();
                self.action_stack.push(Action::NoOp);
                self.extend(entries)
            }
            _ => {
                self.action_stack.push(Action::NoOp);
                Descend::Children.into()
            }
        }
    }
    fn exit_value(&mut self, _value: &Value) -> ExitFlow {
        if let Some(Action::PushedListObject) = self.action_stack.pop() {
            self.cursor.pop();
        }
        Continue::Siblings.into()
    }
}

trait IdxPush {
    type Value;
    /// Push an item into this container, returning that pushed item's index.
    fn idx_push(&mut self, v: Self::Value) -> usize;
}

impl<T> IdxPush for Vec<T> {
    type Value = T;

    fn idx_push(&mut self, v: Self::Value) -> usize {
        let idx = self.len();
        self.push(v);
        idx
    }
}
