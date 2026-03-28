use std::{
    collections::HashMap,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use dashmap::RwLock;

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

#[derive(Debug, Clone, Default)]
pub struct MetaService {
    pub loaded: Arc<AtomicBool>,
    pub version: Arc<RwLock<String>>,
    pub classes: Arc<RwLock<Classes>>,
}

impl MetaService {
    pub fn new() -> Self {
        Self::default()
    }

    fn load_file_inner(self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let mut file = BufReader::new(std::fs::File::open(path)?);
        let dump: DumpFile = serde_json::from_reader(&mut file)?;
        let count = dump.classes.len();
        *self.version.write() = dump.version;
        *self.classes.write() = Classes(dump.classes);
        self.loaded
            .store(true, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("Loaded {count} meta classes");
        Ok(())
    }

    pub fn load_file(&self, path: PathBuf) {
        let s = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = s.load_file_inner(path) {
                panic!("could not load meta dump: {e}");
            }
        });
    }
}
