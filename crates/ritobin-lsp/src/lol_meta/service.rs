use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::{BufReader, Write as _},
    num::ParseIntError,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::Context;
use dashmap::RwLock;
use futures::StreamExt as _;
use itertools::Itertools;
use serde::Deserialize;
use tokio::task::JoinError;

use crate::lol_meta::schema::{Class, DumpFile, Property, U32Hash};

#[derive(Debug, Default)]
pub struct Classes(HashMap<U32Hash, Class>);
impl Classes {
    pub fn get(&self, hash: impl Into<U32Hash>) -> Option<&Class> {
        self.0.get(&hash.into())
    }
    pub fn find_property(
        &self,
        class: impl Into<U32Hash>,
        property: impl Into<U32Hash>,
    ) -> Option<&Property> {
        let mut search = self.get(class);
        let property = property.into();
        while let Some(class) = search {
            if let Some(prop) = class.properties.get(&property) {
                return Some(prop);
            }

            search = class.base.and_then(|base| self.get(base));
        }
        None
    }
}

static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Debug, Clone, Default)]
pub struct MetaService {
    pub loaded: Arc<AtomicBool>,
    pub version: Arc<RwLock<Option<VersionTriple>>>,
    pub classes: Arc<RwLock<Classes>>,
}

impl MetaService {
    pub fn new() -> Self {
        Self::default()
    }

    fn load_file_inner(
        self,
        path: impl AsRef<Path>,
        version: Option<VersionTriple>,
    ) -> anyhow::Result<()> {
        let mut file = BufReader::new(std::fs::File::open(path)?);
        let dump: DumpFile = serde_json::from_reader(&mut file)?;
        let count = dump.classes.len();
        let version = version.or_else(|| dump.version.parse().ok());
        *self.version.write() = version;
        *self.classes.write() = Classes(dump.classes);
        self.loaded
            .store(true, std::sync::atomic::Ordering::Relaxed);

        match version {
            Some(version) => tracing::info!("Loaded {count} meta classes (v{version})"),
            None => tracing::info!("Loaded {count} meta classes (unknown version)"),
        }
        Ok(())
    }

    pub async fn load(&self, dir: impl AsRef<Path>) -> anyhow::Result<()> {
        let s = self.clone();
        let dir = dir.as_ref();
        let version: Option<VersionTriple> = tokio::fs::read_to_string(dir.join("version"))
            .await
            .ok()
            .and_then(|v| v.parse().ok());

        let file = dir.join("dump.json");
        tokio::task::spawn_blocking(move || {
            s.load_file_inner(file, version)
                .context("Error loading meta dump")
        })
        .await??;
        Ok(())
    }
    pub async fn load_file(&self, path: PathBuf) -> Result<(), JoinError> {
        let s = self.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = s.load_file_inner(path, None) {
                panic!("could not load meta dump: {e}");
            }
        })
        .await
    }

    pub async fn fetch_latest(&self, dir: impl AsRef<Path>) -> anyhow::Result<Option<PathBuf>> {
        let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;

        let res = client
            .get("https://api.github.com/repos/LeagueToolkit/lol-meta-classes/releases/latest")
            .send()
            .await?
            .text()
            .await
            .context("Reading Github API Response")?;

        let res: GhReleases = serde_json::from_str(&res)
            .with_context(|| format!("Error decoding Github API response: \n{res}"))?;

        let version: VersionTriple = res
            .tag_name
            .parse()
            .context("Could not determine release version!")?;

        if let Some(existing) = self.version.read().as_ref() {
            match existing.cmp(&version) {
                Ordering::Equal => {
                    tracing::info!("Meta up to date.");
                    return Ok(None);
                }
                Ordering::Greater => {
                    tracing::warn!("Local meta is newer than latest release?");
                    return Ok(None);
                }
                Ordering::Less => {}
            }
        }

        let asset = res
            .assets
            .into_iter()
            .find(|asset| asset.content_type == "application/json")
            .context("Could not find dump file in latest release!")?;

        let file_res = client
            .get(&asset.browser_download_url)
            .send()
            .await
            .context("Error downloading meta dump")?;

        let total_size = file_res.content_length().with_context(|| {
            format!(
                "Failed to get content length from '{}'",
                asset.browser_download_url
            )
        })?;

        tracing::info!("Downloading meta v{version}...");

        let dir = dir.as_ref();

        std::fs::create_dir_all(dir)?;

        let path = dir.join("dump.new.json");
        let mut file =
            File::create(&path).with_context(|| format!("Failed to create file '{path:?}'"))?;
        let mut downloaded: u64 = 0;
        let mut stream = file_res.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.context("Error while downloading file")?;
            file.write_all(&chunk)
                .context("Error while writing to file")?;
            let new = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            downloaded = new;
        }

        tracing::info!("Meta v{version} downloaded.");

        let final_path = path.with_file_name("dump.json");
        std::fs::rename(&path, &final_path).context("Error renaming downloaded dump")?;

        std::fs::write(path.with_file_name("version"), version.to_string())?;

        Ok(Some(final_path))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct VersionTriple {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Ord for VersionTriple {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        self.patch.cmp(&other.patch)
    }
}

impl PartialOrd for VersionTriple {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for VersionTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VersionParseError {
    #[error("Not enough parts (need [major].[minor].[patch]")]
    NotEnoughParts,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
}

impl FromStr for VersionTriple {
    type Err = VersionParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().trim_start_matches('v');
        let mut s = s.splitn(3, '.');

        Ok(Self {
            major: s
                .next()
                .map(|n| u32::from_str(n))
                .ok_or(VersionParseError::NotEnoughParts)??,
            minor: s
                .next()
                .map(|n| u32::from_str(n))
                .ok_or(VersionParseError::NotEnoughParts)??,
            patch: s
                .next()
                .map(|n| u32::from_str(n))
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

#[derive(Deserialize)]
struct GhReleaseAsset {
    pub name: String,
    pub content_type: String,
    pub browser_download_url: String,
}

#[derive(Deserialize)]
struct GhReleases {
    tag_name: String,
    assets: Vec<GhReleaseAsset>,
}
