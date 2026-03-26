use std::{
    collections::HashMap,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use dashmap::RwLock;

use crate::lol_meta::schema::{Class, DumpFile};

#[derive(Debug, Clone, Default)]
pub struct MetaService {
    pub loaded: Arc<AtomicBool>,
    pub version: Arc<RwLock<String>>,
    pub classes: Arc<RwLock<HashMap<String, Class>>>,
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
        *self.classes.write() = dump.classes;
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
