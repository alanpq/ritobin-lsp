use ltk_hash::BinHash;
use ltk_ritobin::ast::{
    Ast, Object, Property, RootEntry, Value,
    visitor::{Continue, Descend, EnterFlow, ExitFlow, Visitor},
};

use crate::{
    linter::Lint,
    server::HashesSnapshot,
    worker::unhash::{self, Unhasher},
};
use meta_wiki::{
    schema::{EqExt, U32Hash},
    service::Classes,
};

mod shadowed;

pub struct Linter<'a> {
    inner: LintVisitor<'a>,
}

struct LintMap;
impl<'a> unhash::Map<unhash::Report<'a>, Lint> for LintMap {
    fn map(&self, report: unhash::Report) -> Lint {
        Lint::KnownHash {
            hash: report.hash,
            table: report.table,
            value: report.unhash.to_string(),
        }
    }
}

pub struct LintVisitor<'a> {
    class_meta: &'a Classes,
    class_scopes: Vec<BinHash>,
    pub lints: Vec<Lint>,

    unhasher: Option<Unhasher<'a, LintMap, Lint>>,
}

impl<'a> Linter<'a> {
    pub fn new(class_meta: &'a Classes, hashes: Option<&'a HashesSnapshot>) -> Self {
        Self {
            inner: LintVisitor {
                class_meta,
                class_scopes: vec![],
                lints: vec![],
                unhasher: hashes.map(|hashes| Unhasher::new(hashes, LintMap)),
            },
        }
    }
    pub fn run(mut self, ast: &Ast) -> Vec<Lint> {
        shadowed::check_root_objects(&mut self.inner.lints, ast);
        ast.walk(&mut self.inner);
        self.inner.lints
    }
}

impl Visitor for LintVisitor<'_> {
    fn enter_object(&mut self, object: &Object) -> EnterFlow {
        self.class_scopes.push(object.class_hash.value);

        shadowed::check_object(&mut self.lints, object);

        if let Some(lint) = self.unhasher.as_ref().and_then(|u| u.unhash_object(object)) {
            self.lints.push(lint);
        }

        EnterFlow::Continue(Descend::Children)
    }

    fn exit_object(&mut self, _: &Object) -> ExitFlow {
        self.class_scopes.pop();
        ExitFlow::Continue(Continue::Siblings)
    }

    fn enter_root_entry(&mut self, entry: &RootEntry) -> ltk_ritobin::ast::visitor::EnterFlow {
        if let Some(lint) = self
            .unhasher
            .as_ref()
            .and_then(|u| u.unhash_root_entry(entry))
        {
            self.lints.push(lint);
        }

        EnterFlow::Continue(Descend::Children)
    }

    fn enter_value(&mut self, value: &Value) -> EnterFlow {
        if let Some(lint) = self.unhasher.as_ref().and_then(|u| u.unhash_value(value)) {
            self.lints.push(lint);
        }
        if let Value::Map { entries, .. } = value {
            shadowed::check_map_entries(&mut self.lints, entries.iter().map(|e| &e.0));
        }
        EnterFlow::Continue(Descend::Children)
    }

    fn enter_property(&mut self, property: &Property) -> EnterFlow {
        if let Some(lint) = self
            .unhasher
            .as_ref()
            .and_then(|u| u.unhash_property(property))
        {
            self.lints.push(lint);
        }

        let Some(&class_hash) = self.class_scopes.last() else {
            return EnterFlow::Continue(Descend::Children);
        };
        let Some(class) = self.class_meta.get(class_hash) else {
            return EnterFlow::Continue(Descend::Children);
        };

        let key_hash = property.name.value;
        match self.class_meta.find_property(class_hash, key_hash) {
            Some(meta_prop) => {
                let expected = meta_prop.rito_type();
                if let Some(value) = property.value.as_ref() {
                    let got = value.rito_type();
                    if got.is_some_and(|got| got != expected) {
                        self.lints.push(Lint::MismatchedMetaTypeArg {
                            key: property.name.span(),
                            type_expr: if property.type_expr.value.is_some() {
                                property.type_expr.span
                            } else {
                                value.span()
                            },
                            expected,
                            got: got.into(),
                        });
                    }
                }
                if let Some(default) = class
                    .defaults
                    .as_ref()
                    .and_then(|d| d.get(&U32Hash::from(key_hash)))
                    && property
                        .value
                        .as_ref()
                        .and_then(|v| v.to_bin_value())
                        .is_some_and(|v| EqExt::eq(default, &v))
                {
                    self.lints.push(Lint::DefaultValue {
                        entry: property.span(),
                        span: property.span(),
                    });
                }
            }
            None => {
                self.lints.push(Lint::UnknownField {
                    entry: property.span(),
                    span: property.name.span(),
                });
            }
        }

        EnterFlow::Continue(Descend::Children)
    }
}

#[cfg(test)]
mod tests;
