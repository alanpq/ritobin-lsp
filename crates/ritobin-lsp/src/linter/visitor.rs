use ltk_hash::BinHash;
use ltk_meta::PropertyValueEnum;
use ltk_ritobin::ast::{
    AstProperty, AstStruct,
    visitor::{Visit, Visitor},
};

use crate::linter::Lint;
use meta_wiki::{
    schema::{EqExt, U32Hash},
    service::Classes,
};

pub struct Linter<'a> {
    class_meta: &'a Classes,
    class_scopes: Vec<BinHash>,
    pub lints: Vec<Lint>,
}

impl<'a> Linter<'a> {
    pub fn new(class_meta: &'a Classes) -> Self {
        Self {
            class_meta,
            class_scopes: vec![],
            lints: vec![],
        }
    }
}

impl Visitor for Linter<'_> {
    fn enter_struct(&mut self, s: &AstStruct) -> Visit {
        self.class_scopes.push(s.class_hash.value);
        Visit::Continue
    }

    fn exit_struct(&mut self, _s: &AstStruct) -> Visit {
        self.class_scopes.pop();
        Visit::Continue
    }

    fn enter_property(&mut self, property: &AstProperty) -> Visit {
        let Some(&class_hash) = self.class_scopes.last() else {
            return Visit::Continue;
        };
        let Some(class) = self.class_meta.get(class_hash) else {
            return Visit::Continue;
        };

        let key_hash = property.name.value;
        match self.class_meta.find_property(class_hash, key_hash) {
            Some(meta_prop) => {
                let expected = meta_prop.rito_type();
                let got = property.value.rito_type();
                if expected != got {
                    self.lints.push(Lint::MismatchedMetaTypeArg {
                        key: property.name.span,
                        type_expr: property.type_span.unwrap_or(property.value.span()),
                        expected,
                        got,
                    });
                }
                if let Some(default) = class
                    .defaults
                    .as_ref()
                    .and_then(|d| d.get(&U32Hash::from(key_hash)))
                    && let Ok(value) = PropertyValueEnum::try_from(property.value.clone())
                    && EqExt::eq(default, &value)
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
                    span: property.name.span,
                });
            }
        }

        Visit::Continue
    }
}

#[cfg(test)]
mod tests;
