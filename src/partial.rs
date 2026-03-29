use serde::Deserialize;

/// A string-like representation of the logging level, used across all
/// deserialization paths before conversion into the typed [`LogLevel`].
///
/// Having a dedicated newtype lets us implement `FromStr` cleanly and
/// keeps `PartialConfig` fully `serde`-compatible.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LogLevel::Trace),
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" | "warning" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            other => Err(format!(
                "unknown log level `{other}` — expected one of: trace, debug, info, warn, error"
            )),
        }
    }
}

/// Intermediate representation produced by every configuration source.
///
/// Every field is `Option<T>` so that a source that does not specify a value
/// leaves the field as `None`, which in turn allows a lower-priority source's
/// value to survive the merge step.
///
/// This is the **only** struct that crosses source boundaries.  It is
/// deliberately not `pub` outside of this crate — callers only ever see the
/// final [`Config`](crate::config::Config).
#[derive(Debug, Default, Deserialize)]
pub struct PartialConfig {
    /// The URL used to connect to the database.
    /// Required — no meaningful default exists.
    pub database_url: Option<String>,

    /// TCP port the application listens on.
    /// Default: 8080.
    pub port: Option<u16>,

    /// Minimum severity level for log output.
    /// Default: Info.
    pub log_level: Option<LogLevel>,

    /// Maximum number of database connections in the pool.
    /// Default: 10.
    pub max_connections: Option<u32>,

    /// Request timeout in seconds.
    /// Default: 30.
    pub timeout_secs: Option<u64>,

    /// Path to the config file.  Only meaningful when read from the
    /// environment (`APP_CONFIG_FILE`); ignored when read from a file itself.
    #[serde(skip)]
    pub config_file: Option<String>,
}
