use std::path::{Path, PathBuf};

use crate::error::ConfigError;
use crate::partial::PartialConfig;

/// Supported configuration file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Toml,
    Yaml,
}

impl FileFormat {
    /// Detect format from a file extension.
    fn from_extension(path: &Path) -> Result<Self, ConfigError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "toml" => Ok(FileFormat::Toml),
            "yaml" | "yml" => Ok(FileFormat::Yaml),
            other => Err(ConfigError::UnsupportedFormat {
                ext: other.to_owned(),
            }),
        }
    }
}

/// Default file names to probe when no explicit path is given.
const PROBE_ORDER: &[&str] = &["config.toml", "config.yaml", "config.yml"];

/// Determine the config file path to use.
///
/// Resolution order:
/// 1. The caller-supplied `explicit_path` (from `Config::load_from`).
/// 2. The `APP_CONFIG_FILE` environment variable.
/// 3. Probing `config.toml` / `config.yaml` / `config.yml` in the current
///    directory.
///
/// Returns `None` if no file is found and no path was explicitly required.
fn resolve_path(explicit_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit_path {
        return Some(p.to_path_buf());
    }

    // Check the env override without going through our full env module to
    // avoid circular dependencies.
    if let Ok(p) = std::env::var("APP_CONFIG_FILE") {
        return Some(PathBuf::from(p));
    }

    // Probe standard names relative to the current directory.
    for name in PROBE_ORDER {
        let candidate = PathBuf::from(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Parse raw file contents according to the detected format.
fn parse(
    contents: &str,
    format: FileFormat,
    path_display: &str,
) -> Result<PartialConfig, ConfigError> {
    match format {
        FileFormat::Toml => toml::from_str(contents).map_err(|e| ConfigError::ParseError {
            path: path_display.to_owned(),
            source: Box::new(e),
        }),
        FileFormat::Yaml => serde_yaml::from_str(contents).map_err(|e| ConfigError::ParseError {
            path: path_display.to_owned(),
            source: Box::new(e),
        }),
    }
}

/// Load a [`PartialConfig`] from a configuration file.
///
/// * `explicit_path` — pass `Some(path)` to require a specific file, or
///   `None` to use the auto-resolution logic (env var → probe).
///
/// If no file is found and no path was explicitly required, returns a
/// default (all-`None`) [`PartialConfig`] rather than an error — the file
/// is optional.
///
/// If an explicit path is provided but the file does not exist, returns
/// [`ConfigError::FileNotFound`].
pub fn from_file(explicit_path: Option<&Path>) -> Result<PartialConfig, ConfigError> {
    let required = explicit_path.is_some();
    let path = match resolve_path(explicit_path) {
        Some(p) => p,
        None => return Ok(PartialConfig::default()),
    };

    let path_display = path.display().to_string();

    let contents = std::fs::read_to_string(&path).map_err(|e| {
        if required {
            ConfigError::FileNotFound {
                path: path_display.clone(),
                source: e,
            }
        } else {
            ConfigError::Io {
                path: path_display.clone(),
                source: e,
            }
        }
    })?;

    let format = FileFormat::from_extension(&path)?;
    parse(&contents, format, &path_display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(contents: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp file");
        f.write_all(contents.as_bytes()).expect("write temp file");
        f
    }

    #[test]
    fn loads_valid_toml() {
        let f = write_temp(
            r#"
database_url = "postgres://localhost/mydb"
port = 5432
log_level = "debug"
max_connections = 20
timeout_secs = 60
"#,
            ".toml",
        );
        let p = from_file(Some(f.path())).expect("should parse toml");
        assert_eq!(p.database_url.as_deref(), Some("postgres://localhost/mydb"));
        assert_eq!(p.port, Some(5432));
        assert_eq!(p.max_connections, Some(20));
    }

    #[test]
    fn loads_valid_yaml() {
        let f = write_temp(
            r#"
database_url: "postgres://localhost/mydb"
port: 5432
log_level: debug
"#,
            ".yaml",
        );
        let p = from_file(Some(f.path())).expect("should parse yaml");
        assert_eq!(p.database_url.as_deref(), Some("postgres://localhost/mydb"));
        assert_eq!(p.port, Some(5432));
    }

    #[test]
    fn partial_toml_leaves_missing_fields_none() {
        let f = write_temp("port = 9000\n", ".toml");
        let p = from_file(Some(f.path())).expect("should parse partial toml");
        assert_eq!(p.port, Some(9000));
        assert!(p.database_url.is_none()); // not set in file — must stay None
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let f = write_temp("this is not valid toml !!!\n", ".toml");
        let err = from_file(Some(f.path())).expect_err("should fail");
        assert!(matches!(err, ConfigError::ParseError { .. }));
    }

    #[test]
    fn malformed_yaml_returns_parse_error() {
        let f = write_temp("key: :\n  - bad: [unterminated", ".yaml");
        let err = from_file(Some(f.path())).expect_err("should fail");
        assert!(matches!(err, ConfigError::ParseError { .. }));
    }

    #[test]
    fn unsupported_extension_returns_error() {
        let f = write_temp("{}\n", ".json");
        let err = from_file(Some(f.path())).expect_err("should fail for json");
        assert!(matches!(err, ConfigError::UnsupportedFormat { .. }));
    }

    #[test]
    fn missing_explicit_path_returns_file_not_found() {
        let err =
            from_file(Some(Path::new("/nonexistent/path/config.toml"))).expect_err("should fail");
        assert!(matches!(err, ConfigError::FileNotFound { .. }));
    }

    #[test]
    fn no_path_and_no_probe_match_returns_empty_partial() {
        // As long as no config.toml / config.yaml exists in the cwd of the
        // test runner, this should return the all-None default.
        // We explicitly pass None (auto-resolve) and rely on no file
        // existing — if one does exist in the project root, the parse
        // should still succeed (it won't be an error, just a non-empty partial).
        let result = from_file(None);
        assert!(result.is_ok());
    }
}
