use anyhow::{bail, Context, Result};
use byroredux_bsa::{Ba2Archive, BsaArchive};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::normalize_asset_path;

enum AssetSource {
    Loose { files: HashMap<String, PathBuf> },
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl AssetSource {
    fn open(path: &Path) -> Result<Self> {
        if path.is_dir() {
            let mut files = HashMap::new();
            for entry in WalkDir::new(path).follow_links(false) {
                let entry = entry.with_context(|| format!("walk source {}", path.display()))?;
                if !entry.file_type().is_file() {
                    continue;
                }
                let relative = entry
                    .path()
                    .strip_prefix(path)
                    .with_context(|| format!("relativize {}", entry.path().display()))?;
                files.insert(
                    normalize_asset_path(&relative.to_string_lossy()),
                    entry.path().to_path_buf(),
                );
            }
            return Ok(Self::Loose { files });
        }

        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("bsa") => {
                Ok(Self::Bsa(BsaArchive::open(path).with_context(|| {
                    format!("open BSA source {}", path.display())
                })?))
            }
            Some("ba2") => {
                Ok(Self::Ba2(Ba2Archive::open(path).with_context(|| {
                    format!("open BA2 source {}", path.display())
                })?))
            }
            _ => bail!(
                "source {} must be a directory, .bsa, or .ba2 archive",
                path.display()
            ),
        }
    }

    fn list_files(&self) -> Vec<String> {
        match self {
            Self::Loose { files } => files.keys().cloned().collect(),
            Self::Bsa(archive) => archive
                .list_files()
                .into_iter()
                .map(normalize_asset_path)
                .collect(),
            Self::Ba2(archive) => archive
                .list_files()
                .into_iter()
                .map(normalize_asset_path)
                .collect(),
        }
    }

    fn extract(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let path = normalize_asset_path(path);
        match self {
            Self::Loose { files } => files
                .get(&path)
                .map(|actual| {
                    fs::read(actual)
                        .with_context(|| format!("read loose texture {}", actual.display()))
                })
                .transpose(),
            Self::Bsa(archive) => {
                if archive.contains(&path) {
                    Ok(Some(
                        archive
                            .extract(&path)
                            .with_context(|| format!("extract {path} from BSA"))?,
                    ))
                } else {
                    Ok(None)
                }
            }
            Self::Ba2(archive) => {
                if archive.contains(&path) {
                    Ok(Some(
                        archive
                            .extract(&path)
                            .with_context(|| format!("extract {path} from BA2"))?,
                    ))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

/// Ordered loose/archive sources. Later sources override earlier sources, like
/// a game/mod load order.
pub struct SourceStack {
    sources: Vec<AssetSource>,
}

impl SourceStack {
    pub fn open(paths: &[PathBuf]) -> Result<Self> {
        if paths.is_empty() {
            bail!("at least one --source is required");
        }
        let sources = paths
            .iter()
            .map(|path| AssetSource::open(path))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { sources })
    }

    pub fn list_files(&self) -> Vec<String> {
        let mut visible = BTreeMap::<String, usize>::new();
        for (source_index, source) in self.sources.iter().enumerate() {
            for path in source.list_files() {
                visible.insert(normalize_asset_path(&path), source_index);
            }
        }
        visible.into_keys().collect()
    }

    pub fn extract(&self, path: &str) -> Result<Vec<u8>> {
        for source in self.sources.iter().rev() {
            if let Some(bytes) = source.extract(path)? {
                return Ok(bytes);
            }
        }
        bail!("texture {:?} was not found in any source", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_loose_source_overrides_earlier_source() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::create_dir_all(first.path().join("textures")).unwrap();
        fs::create_dir_all(second.path().join("textures")).unwrap();
        fs::write(first.path().join("textures/a.dds"), b"first").unwrap();
        fs::write(second.path().join("textures/a.dds"), b"second").unwrap();

        let sources =
            SourceStack::open(&[first.path().to_path_buf(), second.path().to_path_buf()]).unwrap();
        assert_eq!(sources.extract("textures\\a.dds").unwrap(), b"second");
        assert_eq!(sources.list_files(), vec!["textures/a.dds"]);
    }
}
