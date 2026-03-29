/// All error variants that can occur while loading or validating configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A config file was explicitly requested but could not be found.
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
    #[error("required configuration key `{key}` is missing — set it via environment variable `{env_hint}` or in the config file")]
    MissingRequired { key: String, env_hint: String },

    /// A value was present but failed a domain validation rule.
    #[error("validation failed for `{key}`: {reason}")]
    ValidationError { key: String, reason: String },

    /// The config file has an extension that is not supported.
    #[error("unsupported config file format `{ext}` — supported formats are: toml, yaml, yml")]
    UnsupportedFormat { ext: String },

    /// An I/O error that is not a simple "file not found".
    #[error("I/O error while reading config file `{path}`: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    /// A error from the hot-reload file watcher.
    #[error("hot-reload watcher error: {0}")]
    WatcherError(String),
}
