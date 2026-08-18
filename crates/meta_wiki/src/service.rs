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
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use tokio::task::JoinError;

use crate::schema::{Class, DumpFile, Property, U32Hash};

#[derive(Debug, Default)]
pub struct Classes {
    classes: HashMap<U32Hash, Class>,
    children: FxHashMap<U32Hash, Vec<U32Hash>>,
}
impl Classes {
    pub fn new(classes: HashMap<U32Hash, Class>) -> Self {
        let mut children: FxHashMap<U32Hash, Vec<U32Hash>> = FxHashMap::default();
        for (hash, class) in &classes {
            if let Some(base) = class.base {
                children.entry(base).or_default().push(*hash);
            }
        }
        Self { classes, children }
    }

    /// Every class assignable to `root` — `root` itself plus its transitive subclasses — paired with
    /// its distance from `root`. Interfaces are descended through but never yielded; they cannot be
    /// written as a value.
    pub fn concrete_descendants(&self, root: impl Into<U32Hash>) -> Vec<(u32, U32Hash)> {
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        let mut stack = vec![(0, root.into())];

        while let Some((depth, hash)) = stack.pop() {
            let Some(class) = self.get(hash).filter(|_| seen.insert(hash)) else {
                continue;
            };
            if !class.flags.interface {
                out.push((depth, hash));
            }
            stack.extend(
                self.children
                    .get(&hash)
                    .into_iter()
                    .flatten()
                    .map(|&child| (depth + 1, child)),
            );
        }

        out
    }

    pub fn get(&self, hash: impl Into<U32Hash>) -> Option<&Class> {
        self.classes.get(&hash.into())
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
        *self.classes.write() = Classes::new(dump.classes);
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
                .map(u32::from_str)
                .ok_or(VersionParseError::NotEnoughParts)??,
            minor: s
                .next()
                .map(u32::from_str)
                .ok_or(VersionParseError::NotEnoughParts)??,
            patch: s.next().map(u32::from_str).transpose()?.unwrap_or_default(),
        })
    }
}

#[derive(Deserialize)]
struct GhReleaseAsset {
    pub content_type: String,
    pub browser_download_url: String,
}

#[derive(Deserialize)]
struct GhReleases {
    pub tag_name: String,
    pub assets: Vec<GhReleaseAsset>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ClassFlags;

    fn class(base: Option<u32>, interface: bool) -> Class {
        Class {
            base: base.map(U32Hash),
            flags: ClassFlags {
                interface,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn classes(entries: impl IntoIterator<Item = (u32, Class)>) -> Classes {
        Classes::new(
            entries
                .into_iter()
                .map(|(hash, class)| (U32Hash(hash), class))
                .collect(),
        )
    }

    fn descendants(classes: &Classes, root: u32) -> Vec<(u32, u32)> {
        let mut out: Vec<_> = classes
            .concrete_descendants(U32Hash(root))
            .into_iter()
            .map(|(depth, hash)| (depth, hash.0))
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn a_leaf_class_yields_only_itself() {
        let classes = classes([(1, class(None, false))]);
        assert_eq!(descendants(&classes, 1), vec![(0, 1)]);
    }

    #[test]
    fn subclasses_are_yielded_with_their_distance_from_the_root() {
        let classes = classes([
            (1, class(None, false)),
            (2, class(Some(1), false)),
            (3, class(Some(2), false)),
        ]);
        assert_eq!(descendants(&classes, 1), vec![(0, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn interfaces_are_descended_through_but_never_yielded() {
        let classes = classes([
            (1, class(None, true)),
            (2, class(Some(1), true)),
            (3, class(Some(2), false)),
        ]);
        assert_eq!(descendants(&classes, 1), vec![(2, 3)]);
    }

    #[test]
    fn unrelated_classes_are_excluded() {
        let classes = classes([
            (1, class(None, false)),
            (2, class(Some(1), false)),
            (9, class(None, false)),
        ]);
        assert_eq!(descendants(&classes, 1), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn a_base_cycle_terminates() {
        let classes = classes([(1, class(Some(2), false)), (2, class(Some(1), false))]);
        assert_eq!(descendants(&classes, 1), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn an_unknown_root_yields_nothing() {
        let classes = classes([(1, class(None, false))]);
        assert_eq!(descendants(&classes, 7), vec![]);
    }
}
