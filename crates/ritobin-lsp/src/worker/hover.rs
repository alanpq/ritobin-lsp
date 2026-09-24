use std::fmt::{self, Write as _};

use itertools::Itertools;
use lsp_types::{Hover, MarkupContent, MarkupKind, WorkDoneProgressParams};

use ltk_hash::BinHash;
use ltk_mimir_cache::Table;
use ltk_ritobin::ast::{
    diagnostics::RitoTypeOrVirtual,
    node::{NodeExt as _, SubNodeRef},
    query::{AstObjectDetail, AstPropertyDetail, AstRootEntryDetail},
};

use crate::{lsp::ext::PositionOrRange, wiki, worker::Worker};
use meta_wiki::{client::types::GetDocsNameOrHash, schema::U32Hash};

use arc_swap::access::Access;

#[derive(Debug, thiserror::Error)]
enum HoverError {
    #[error(transparent)]
    Fmt(#[from] fmt::Error),
    #[error("failed to resolve class name/hash: {0}")]
    Meta(#[from] meta_wiki::client::types::error::ConversionError),
}

impl Worker {
    async fn hover_inner(
        &self,
        Scope {
            node,
            class_hash,
            class_name,
        }: Scope<'_>,
    ) -> Result<Option<String>, HoverError> {
        let doc = &self.document;
        Ok(Some(match node {
            SubNodeRef::Property(
                prop,
                AstPropertyDetail::Name | AstPropertyDetail::Trivia | AstPropertyDetail::Node,
            ) => {
                let txt = &doc.text[prop.name.span()];
                let hash = prop.name.value;
                let prop_meta = {
                    self.server
                        .meta
                        .classes()
                        .load()
                        .find_property(class_hash, hash)
                        .copied()
                };
                match prop_meta {
                    Some(prop_meta) => {
                        let name = GetDocsNameOrHash::try_from(class_name)?;
                        let rito_type = prop_meta.rito_type();

                        let mut str = format!(
                            r#"### [{class_name}](https://meta-wiki.leaguetoolkit.dev/classes/{}/)

`{txt}`: `{}`

"#,
                            class_name.to_ascii_lowercase(),
                            rito_type,
                        );
                        let body = match wiki::fetch_class_docs(&self.server.wiki, &name).await {
                            Ok(docs) => wiki::describe(docs.properties.get(txt)).to_owned(),
                            Err(msg) => msg,
                        };
                        writeln!(str, "{body}")?;
                        writeln!(str, "\n`0x{hash:>08x}`")?;
                        str
                    }
                    None => format!("{txt}: ??"),
                }
            }
            SubNodeRef::Property(_, AstPropertyDetail::TypeExpr) => {
                return Ok(None);
            }
            SubNodeRef::RootEntry(_, AstRootEntryDetail::PathHash | AstRootEntryDetail::Node)
            | SubNodeRef::Object(_, AstObjectDetail::ClassHash | AstObjectDetail::Node) => {
                let classes = &self.server.meta.load_full().classes;
                match classes.get(class_hash) {
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
                                base = class.base.and_then(|b| Some((b, classes.get(b)?)));
                            }
                        }

                        let name = GetDocsNameOrHash::try_from(class_name)?;
                        let body = match wiki::fetch_class_docs(&self.server.wiki, &name).await {
                            Ok(docs) => wiki::describe(docs.class.as_ref()).to_owned(),
                            Err(msg) => msg,
                        };
                        writeln!(txt, "{body}")?;

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
        }))
    }
    pub(super) async fn hover(
        &self,
        position: PositionOrRange,
        _work_done_progress_params: WorkDoneProgressParams,
    ) -> anyhow::Result<Option<Hover>> {
        let pos = position.start();
        let doc = &self.document;
        let ast = self.ast()?;
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
        let class_name = doc.text[scope.span()].trim();

        let markup = MarkupContent {
            kind: MarkupKind::Markdown,
            value: match self
                .hover_inner(Scope {
                    node: located,
                    class_hash,
                    class_name,
                })
                .await
            {
                Ok(s) => s.unwrap_or_default(),
                Err(e) => format!("Error resolving hover - {e}"),
            },
        };

        Ok(Some(Hover {
            contents: lsp_types::HoverContents::Markup(markup),
            range: None,
        }))
    }
}

struct Scope<'a> {
    node: SubNodeRef<'a>,
    class_hash: BinHash,
    class_name: &'a str,
}
