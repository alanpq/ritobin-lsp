use std::sync::OnceLock;

use paths::AbsPathBuf;
use semver::Version;

use crate::lsp::capabilities::ClientCapabilities;

#[derive(Clone, Debug)]
struct ClientInfo {
    name: String,
    version: Option<Version>,
}
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
