//! Conversion between League `.bin` files and ritobin text, backing the
//! `ritobin-lsp/deserializeBin` / `ritobin-lsp/serializeBin` requests.
//!
//! Both functions do blocking file IO and are meant to run inside
//! `tokio::task::spawn_blocking`. Their error enums render (via `Display`) to
//! the user-facing string the dispatcher forwards as an LSP response error.

use std::{
    fs,
    io::{BufReader, Cursor},
    path::Path,
};

use ltk_ritobin::{
    Cst,
    ast::{PartialBin, diagnostics::Diagnostic},
    print::{Print as _, PrintConfig, PrintError},
};

use crate::{fs_ext, lsp::ext::DeserializeBinResult, server::BinHashProvider};

#[derive(Debug, thiserror::Error)]
pub enum DeserializeError {
    #[error("Failed to open '{path}': {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("'{path}' is not a League of Legends .bin file.")]
    InvalidSignature { path: String },

    #[error("Failed to read '{path}': {source}")]
    Read {
        path: String,
        #[source]
        source: ltk_meta::Error,
    },

    #[error("Failed to print '{path}' as ritobin: {source}")]
    Print {
        path: String,
        #[source]
        source: PrintError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SerializeError {
    #[error("Could not save bin. Fix the errors shown in the editor and try again.")]
    InvalidSource,

    #[error("This is a PTCH override bin, which cannot be written back yet.")]
    Override,

    #[error("Failed to serialize bin: {source}")]
    Serialize {
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write '{path}': {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub fn deserialize_bin(
    bin_path: &Path,
    hashes: BinHashProvider,
) -> Result<DeserializeBinResult, DeserializeError> {
    let path = bin_path.display().to_string();

    let file = fs::File::open(bin_path).map_err(|source| DeserializeError::Open {
        path: path.clone(),
        source,
    })?;
    let bin =
        ltk_meta::Bin::from_reader(&mut BufReader::new(file)).map_err(|source| match source {
            ltk_meta::Error::InvalidFileSignature => {
                DeserializeError::InvalidSignature { path: path.clone() }
            }
            source => DeserializeError::Read {
                path: path.clone(),
                source,
            },
        })?;

    // Default print config renders all hashes as hex; hashtable-backed naming
    // can be layered in later via a PrintConfig hash provider.
    let text = bin
        .print_with_config(PrintConfig::default().with_hashes(hashes))
        .map_err(|source| DeserializeError::Print { path, source })?;

    Ok(DeserializeBinResult {
        text,
        is_override: bin.is_override,
    })
}

pub fn serialize_bin(bin_path: &Path, text: &str) -> Result<(), SerializeError> {
    let cst = Cst::parse(text);
    if !cst.errors.is_empty() {
        return Err(SerializeError::InvalidSource);
    }

    let PartialBin { bin, diagnostics } = cst.build_bin(text);
    let has_errors = diagnostics
        .iter()
        .any(|d| !matches!(d.diagnostic, Diagnostic::ShadowedEntry { .. }));
    if has_errors {
        return Err(SerializeError::InvalidSource);
    }
    if bin.is_override {
        return Err(SerializeError::Override);
    }

    let mut buf = Cursor::new(Vec::new());
    bin.to_writer(&mut buf)
        .map_err(|source| SerializeError::Serialize { source })?;

    fs_ext::write_atomic(bin_path, buf.get_ref()).map_err(|source| SerializeError::Write {
        path: bin_path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("ritobin-lsp-tests");
        fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{}-{name}", std::process::id()))
    }

    #[test]
    fn round_trip() {
        let path = temp_path("round-trip.bin");
        let bin = ltk_meta::Bin::builder().dependency("base.bin").build();
        let mut buf = Cursor::new(Vec::new());
        bin.to_writer(&mut buf).unwrap();
        fs::write(&path, buf.get_ref()).unwrap();

        let deserialized = deserialize_bin(&path, BinHashProvider::default()).unwrap();
        assert!(!deserialized.is_override);
        serialize_bin(&path, &deserialized.text).unwrap();

        let reread =
            ltk_meta::Bin::from_reader(&mut BufReader::new(fs::File::open(&path).unwrap()))
                .unwrap();
        assert_eq!(reread, bin);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let path = temp_path("bad-magic.bin");
        fs::write(&path, b"JUNKJUNKJUNKJUNK").unwrap();

        let err = deserialize_bin(&path, BinHashProvider::default()).unwrap_err();
        assert!(
            matches!(err, DeserializeError::InvalidSignature { .. }),
            "{err}"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parse_errors_refuse_save() {
        let path = temp_path("parse-err.bin");
        let err = serialize_bin(&path, "entry: {{{{").unwrap_err();
        assert!(matches!(err, SerializeError::InvalidSource), "{err}");
        assert!(!path.exists());
    }
}
