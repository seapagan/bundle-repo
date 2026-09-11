mod metadata;

use std::path::{Path, PathBuf};

use config::{Config, File, FileFormat};
use dirs_next::home_dir;

use metadata::parse_metadata_source;
#[cfg(test)]
pub(crate) use metadata::{CandidateValue, MetadataCandidate};
pub(crate) use metadata::{
    ConfigSourceIdentity, MetadataError, MetadataSource,
    validate_and_merge_metadata,
};

use crate::structs::Params;

pub(crate) struct LoadedConfig {
    pub(crate) params: Params,
    pub(crate) metadata_sources: Vec<MetadataSource>,
    pub(crate) legacy_error: Option<String>,
}

struct ConfigSource {
    path: PathBuf,
    identity: ConfigSourceIdentity,
}

pub(crate) fn load_config() -> Result<LoadedConfig, MetadataError> {
    let global =
        home_dir().map(|home| home.join(".config/bundlerepo/config.toml"));
    load_config_from_paths(global.as_deref(), Path::new(".bundlerepo.toml"))
}

pub(crate) fn load_config_from_paths(
    global: Option<&Path>,
    local: &Path,
) -> Result<LoadedConfig, MetadataError> {
    let sources = config_sources(global, local);
    let metadata_sources = sources
        .iter()
        .filter_map(|source| parse_metadata_source(source).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    let mut builder = Config::builder();

    for source in &sources {
        builder = builder.add_source(
            File::from(source.path.clone()).format(FileFormat::Toml),
        );
    }

    let (params, legacy_error) = match builder.build() {
        Ok(config) => (config.into(), None),
        Err(error) => (Params::default(), Some(error.to_string())),
    };

    Ok(LoadedConfig {
        params,
        metadata_sources,
        legacy_error,
    })
}

fn config_sources(global: Option<&Path>, local: &Path) -> Vec<ConfigSource> {
    let mut sources = Vec::with_capacity(2);
    if let Some(path) = global.filter(|path| path.exists()) {
        sources.push(ConfigSource {
            path: path.to_path_buf(),
            identity: ConfigSourceIdentity::Global,
        });
    }
    if local.exists() {
        sources.push(ConfigSource {
            path: local.to_path_buf(),
            identity: ConfigSourceIdentity::RepositoryLocal,
        });
    }
    sources
}

#[cfg(test)]
#[path = "../tests/crate/configuration.rs"]
mod tests;
