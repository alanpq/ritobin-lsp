use std::fmt::Write as _;

use itertools::Itertools;
use lsp_types::{Hover, MarkupContent, MarkupKind, WorkDoneProgressParams};

use ltk_mimir_cache::Table;
use ltk_ritobin::ast::{
    diagnostics::RitoTypeOrVirtual,
    node::{NodeExt as _, SubNodeRef},
    query::{AstObjectDetail, AstPropertyDetail, AstRootEntryDetail},
};

use crate::{lsp::ext::PositionOrRange, wiki, worker::Worker};
use meta_wiki::{client::types::GetDocsNameOrHash, schema::U32Hash};

impl Worker {
    pub(super) async fn hover(
        &self,
        position: PositionOrRange,
        _work_done_progress_params: WorkDoneProgressParams,
    ) -> anyhow::Result<Option<Hover>> {
        let pos = position.start();
        let doc = &self.document;
        let Some(ast) = self.ast.as_ref() else {
            return Ok(None);
        };

        let offset = doc.line_numbers.from_position(pos);
        let mut path = ast.fine_path_to(offset).collect_vec();

        // tracing::info!("######");
        // for n in &fine_path {
        //     tracing::info!("- {:?} / {:?}", n.kind(), n.detail());
        // }

        let Some(located) = path.pop() else {
            return Ok(None);
        };
        // the nearest enclosing class - what hover needs to resolve a property name or render a
        // class' doc link
        let Some(scope) = path.iter().rev().find_map(|n| n.class_hash()) else {
            return Ok(None);
        };

        let class_hash = scope.value;
        let class_name = &doc.text[scope.span()];

        let class = {
            let classes = self.server.meta.classes.read().unwrap();
            classes.get(class_hash).cloned()
        };

        let markup = MarkupContent {
            kind: MarkupKind::Markdown,
            value: match located {
                SubNodeRef::Property(
                    prop,
                    AstPropertyDetail::Name | AstPropertyDetail::Trivia | AstPropertyDetail::Node,
                ) => {
                    let txt = &doc.text[prop.name.span()];
                    let hash = prop.name.value;
                    let prop_meta = {
                        let classes = self.server.meta.classes.read().unwrap();
                        classes.find_property(class_hash, hash).cloned()
                    };
                    match prop_meta {
                        Some(prop_meta) => {
                            let name = GetDocsNameOrHash::try_from(class_name).unwrap();
                            let rito_type = prop_meta.rito_type();

                            let mut str = format!(
                                r#"### [{class_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/)

`{txt}`: `{}`

"#,
                                class_name.to_ascii_lowercase(),
                                rito_type,
                            );
                            let body = match wiki::fetch_class_docs(&self.server.wiki, &name).await
                            {
                                Ok(docs) => wiki::describe(docs.properties.get(txt)).to_owned(),
                                Err(msg) => msg,
                            };
                            writeln!(str, "{body}").unwrap();
                            writeln!(str, "\n`0x{hash:>08x}`").unwrap();
                            str
                        }
                        None => format!("{txt}: ??"),
                    }
                }
                SubNodeRef::Property(_, AstPropertyDetail::TypeExpr) => {
                    return Ok(None);
                }
                SubNodeRef::RootEntry(
                    _,
                    AstRootEntryDetail::PathHash | AstRootEntryDetail::Node,
                )
                | SubNodeRef::Object(_, AstObjectDetail::ClassHash | AstObjectDetail::Node) => {
                    match class {
                        Some(class) => {
                            let mut txt = format!(
                                "### [{class_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/) (`0x{:>08x}`)\n\n",
                                class_name.to_ascii_lowercase(),
                                class_hash,
                            );

                            let mut base = Some((U32Hash::from(class_hash), class));
                            let mut d = 0;
                            let bin_types = self
                                .server
                                .hashes
                                .as_ref()
                                .and_then(|hashes| hashes.table(Table::BinTypes));

                            {
                                let classes = self.server.meta.classes.read().unwrap();
                                while let Some((hash, class)) = base {
                                    if d > 0 {
                                        let base_name = bin_types
                                            .as_ref()
                                            .and_then(|h| h.get((*hash).into()))
                                            .unwrap_or_else(|| hash.to_string().into());
                                        writeln!(
                                            txt,
                                            "{}└─ [{base_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/)\n",
                                            "\u{00A0}".repeat(d - 1),
                                            base_name.to_ascii_lowercase()
                                        )?;
                                    }
                                    d += 1;
                                    base = class
                                        .base
                                        .and_then(|b| Some((b, classes.get(b).cloned()?)));
                                }
                            }

                            let name = GetDocsNameOrHash::try_from(class_name).unwrap();
                            let body = match wiki::fetch_class_docs(&self.server.wiki, &name).await
                            {
                                Ok(docs) => wiki::describe(docs.class.as_ref()).to_owned(),
                                Err(msg) => msg,
                            };
                            writeln!(txt, "{body}").unwrap();

                            txt
                        }
                        None => format!("*Unknown class `{class_name}`*"),
                    }
                }
                SubNodeRef::Value(value) => {
                    format!(
                        "**{}**\n\nvalue: `{value}`",
                        RitoTypeOrVirtual::from(value.rito_type())
                    )
                }
                _ => return Ok(None),
            },
        };

        Ok(Some(Hover {
            contents: lsp_types::HoverContents::Markup(markup),
            range: None,
        }))
    }
}
