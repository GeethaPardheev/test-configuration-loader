/// All error variants that can occur while loading or validating configuration.
///
/// The enum is marked `#[non_exhaustive]` so that adding new variants in a
/// future release does not break downstream `match` arms.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// A config file was explicitly requested but could not be found on disk.
    #[error("config file not found at `{path}`: {source}")]
    FileNotFound {
        path: String,
        source: std::io::Error,
    },

    /// A config file was found but its contents could not be parsed.
    #[error("failed to parse config file `{path}`: {source}")]
    ParseError {
        path: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// An environment variable was present but could not be converted to the
    /// expected type.
    #[error("environment variable `{key}` has invalid value `{value}`: {reason}")]
    InvalidEnvVar {
        key: String,
        value: String,
        reason: String,
    },

    /// A required configuration key was absent from every source.
    ///
    /// The `env_hint` field names the environment variable the operator can
    /// set to supply the missing value without editing any files.
    #[error(
        "required configuration key `{key}` is missing \
         — set it via environment variable `{env_hint}` or in the config file"
    )]
    MissingRequired { key: String, env_hint: String },

    /// A value was present but failed a domain-specific validation rule.
    #[error("validation failed for `{key}`: {reason}")]
    ValidationError { key: String, reason: String },

    /// The config file has an extension that no parser supports.
    #[error(
        "unsupported config file format `{ext}` \
         — supported formats are: toml, yaml, yml"
    )]
    UnsupportedFormat { ext: String },

    /// An I/O error that is not a simple "file not found".
    #[error("I/O error reading config file `{path}`: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    /// The hot-reload file watcher encountered an error.
    #[error("hot-reload watcher error: {0}")]
    WatcherError(String),
}
