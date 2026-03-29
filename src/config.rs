use std::path::Path;

use crate::defaults;
use crate::env;
use crate::error::ConfigError;
use crate::file;
use crate::merge;
use crate::partial::PartialConfig;
use crate::validate::{self, Config};

impl Config {
    /// Load configuration using the default resolution strategy:
    ///
    /// 1. Hard-coded defaults (lowest priority)
    /// 2. Config file (`config.toml` / `config.yaml` in cwd, or the path in
    ///    `APP_CONFIG_FILE`)
    /// 3. Environment variables (highest priority)
    ///
    /// Returns a fully validated [`Config`] or a descriptive [`ConfigError`].
    pub fn load() -> Result<Self, ConfigError> {
        let base = defaults::defaults();
        let file_partial = file::from_file(None)?;
        let env_partial = env::from_env()?;

        let merged = merge::merge(merge::merge(base, file_partial), env_partial);
        validate::validate(merged)
    }

    /// Load configuration from an explicit config file path, then overlay
    /// environment variables on top.
    ///
    /// Useful in tests that need a specific file without affecting the
    /// process environment.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let base = defaults::defaults();
        let file_partial = file::from_file(Some(path))?;
        let env_partial = env::from_env()?;

        let merged = merge::merge(merge::merge(base, file_partial), env_partial);
        validate::validate(merged)
    }

    /// Build a [`Config`] directly from a [`PartialConfig`].
    ///
    /// This is the primary entry point for unit tests — it lets tests
    /// construct arbitrary configurations without touching the filesystem
    /// or the process environment.
    pub fn from_partial(partial: PartialConfig) -> Result<Self, ConfigError> {
        validate::validate(partial)
    }
}
