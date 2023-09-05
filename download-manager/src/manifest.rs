//! Defines the serialization and deserialization format for the manifest.

use camino::Utf8Path;
use eyre::Result;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) downloads: Vec<ManifestEntry>,
}

impl Manifest {
    pub(crate) async fn load(file: &Utf8Path) -> Result<Self> {
        // We use the fs_err crate here for better error messages.
        let contents = fs_err::tokio::read_to_string(file).await?;
        let manifest = toml::from_str(&contents)?;
        Ok(manifest)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ManifestEntry {
    pub(crate) url: Url,
    #[serde(default)]
    pub(crate) file_name: Option<String>,
    // Other options can go here
}
