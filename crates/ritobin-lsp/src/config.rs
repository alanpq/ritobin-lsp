use std::{
    fmt, iter,
    sync::{Arc, OnceLock},
};

use itertools::Itertools as _;
use paths::{AbsPathBuf, Utf8PathBuf};
use semver::Version;
use serde::de::DeserializeOwned;

use crate::lsp::capabilities::ClientCapabilities;

#[derive(Clone, Debug)]
struct ClientInfo {
    name: String,
    version: Option<Version>,
}

#[derive(Debug)]
pub enum ConfigErrorInner {
    Json {
        config_key: String,
        error: serde_json::Error,
    },
    // Toml {
    //     config_key: String,
    //     error: toml::de::Error,
    // },
    ParseError {
        reason: String,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ConfigErrors(Vec<Arc<ConfigErrorInner>>);

impl ConfigErrors {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let errors = self.0.iter().format_with("\n", |inner, f| {
            match &**inner {
                ConfigErrorInner::Json {
                    config_key: key,
                    error: e,
                } => {
                    f(key)?;
                    f(&": ")?;
                    f(e)
                }
                ConfigErrorInner::ParseError { reason } => f(reason),
            }?;
            f(&";")
        });
        write!(
            f,
            "invalid config value{}:\n{}",
            if self.0.len() == 1 { "" } else { "s" },
            errors
        )
    }
}

impl std::error::Error for ConfigErrors {}

#[derive(Clone)]
pub struct Config {
    caps: ClientCapabilities,
    root_path: AbsPathBuf,
    /// The workspace roots as registered by the LSP client
    workspace_roots: Vec<AbsPathBuf>,
    // snippets: Vec<Snippet>,
    client_info: Option<ClientInfo>,
}

impl Config {
    pub fn new(
        root_path: AbsPathBuf,
        caps: lsp_types::ClientCapabilities,
        workspace_roots: Vec<AbsPathBuf>,
        client_info: Option<lsp_types::ClientInfo>,
    ) -> Self {
        // static DEFAULT_CONFIG_DATA: OnceLock<&'static DefaultConfigData> = OnceLock::new();

        Config {
            caps: ClientCapabilities::new(caps),
            root_path,
            // snippets: Default::default(),
            workspace_roots,
            client_info: client_info.map(|it| ClientInfo {
                name: it.name,
                version: it
                    .version
                    .as_deref()
                    .map(Version::parse)
                    .and_then(Result::ok),
            }),
            // client_config: (FullConfigInput::default(), ConfigErrors(vec![])),
            // default_config: DEFAULT_CONFIG_DATA.get_or_init(|| Box::leak(Box::default())),
            // user_config: None,
            // detached_files: Default::default(),
            // validation_errors: Default::default(),
            // ratoml_file: Default::default(),
        }
    }

    pub fn root_path(&self) -> &AbsPathBuf {
        &self.root_path
    }

    pub fn caps(&self) -> &ClientCapabilities {
        &self.caps
    }

    // VSCode is our reference implementation, so we allow ourselves to work around issues by
    // special casing certain versions
    pub fn visual_studio_code_version(&self) -> Option<&Version> {
        self.client_info
            .as_ref()
            .filter(|it| it.name.starts_with("Visual Studio Code"))
            .and_then(|it| it.version.as_ref())
    }
    pub fn client_is_neovim(&self) -> bool {
        self.client_info
            .as_ref()
            .map(|it| it.name == "Neovim")
            .unwrap_or_default()
    }
}

#[derive(Default, Debug)]
pub struct ConfigChange {
    user_config_change: Option<Arc<str>>,
    client_config_change: Option<serde_json::Value>,
    // source_map_change: Option<Arc<FxHashMap<SourceRootId, SourceRootId>>>,
}

impl ConfigChange {
    pub fn change_user_config(&mut self, content: Option<Arc<str>>) {
        assert!(self.user_config_change.is_none()); // Otherwise it is a double write.
        self.user_config_change = content;
    }

    pub fn change_client_config(&mut self, change: serde_json::Value) {
        self.client_config_change = Some(change);
    }

    // pub fn change_source_root_parent_map(
    //     &mut self,
    //     source_root_map: Arc<FxHashMap<SourceRootId, SourceRootId>>,
    // ) {
    //     assert!(self.source_map_change.is_none());
    //     self.source_map_change = Some(source_root_map);
    // }
}

fn get_field_json<T: DeserializeOwned>(
    json: &mut serde_json::Value,
    error_sink: &mut Vec<(String, serde_json::Error)>,
    field: &'static str,
    alias: Option<&'static str>,
) -> Option<T> {
    // XXX: check alias first, to work around the VS Code where it pre-fills the
    // defaults instead of sending an empty object.
    alias
        .into_iter()
        .chain(iter::once(field))
        .filter_map(move |field| {
            let mut pointer = field.replace('_', "/");
            pointer.insert(0, '/');
            json.pointer_mut(&pointer)
                .map(|it| serde_json::from_value(it.take()).map_err(|e| (e, pointer)))
        })
        .flat_map(|res| match res {
            Ok(it) => Some(it),
            Err((e, pointer)) => {
                tracing::warn!("Failed to deserialize config field at {}: {:?}", pointer, e);
                error_sink.push((pointer, e));
                None
            }
        })
        .next()
}
