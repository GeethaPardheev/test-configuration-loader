use crate::error::ConfigError;
use crate::partial::{LogLevel, PartialConfig};

/// The fully resolved, strongly-typed application configuration.
///
/// All fields are guaranteed to be valid; this struct is never constructed
/// unless every constraint has passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Database connection URL.
    pub database_url: String,
    /// TCP port the application listens on (1–65535).
    pub port: u16,
    /// Minimum log severity level.
    pub log_level: LogLevel,
    /// Size of the database connection pool (≥ 1).
    pub max_connections: u32,
    /// Request timeout in seconds (≥ 1).
    pub timeout_secs: u64,
}

/// Validate a merged [`PartialConfig`] and produce a [`Config`].
///
/// Validation rules:
/// - `database_url`: required; must be non-empty.
/// - `port`: required; must be in 1–65535 (the `u16` type already enforces
///   the upper bound; we reject 0 explicitly).
/// - `log_level`: required; defaults should always supply it, but we still
///   check to be safe.
/// - `max_connections`: required; must be ≥ 1.
/// - `timeout_secs`: required; must be ≥ 1.
///
/// # Errors
/// Returns a [`ConfigError::MissingRequired`] if any required property is `None`.
/// Returns a [`ConfigError::ValidationError`] if a configured property violates domains invariants.
pub fn validate(partial: PartialConfig) -> Result<Config, ConfigError> {
    // ── database_url ────────────────────────────────────────────────────────
    let database_url = partial
        .database_url
        .ok_or_else(|| ConfigError::MissingRequired {
            key: "database_url".into(),
            env_hint: "APP_DATABASE_URL".into(),
        })?;

    if database_url.trim().is_empty() {
        return Err(ConfigError::ValidationError {
            key: "database_url".into(),
            reason: "must not be empty".into(),
        });
    }

    // ── port ────────────────────────────────────────────────────────────────
    let port = partial.port.ok_or_else(|| ConfigError::MissingRequired {
        key: "port".into(),
        env_hint: "APP_PORT".into(),
    })?;

    if port == 0 {
        return Err(ConfigError::ValidationError {
            key: "port".into(),
            reason: "must be between 1 and 65535 (got 0)".into(),
        });
    }

    // ── log_level ───────────────────────────────────────────────────────────
    let log_level = partial
        .log_level
        .ok_or_else(|| ConfigError::MissingRequired {
            key: "log_level".into(),
            env_hint: "APP_LOG_LEVEL".into(),
        })?;

    // ── max_connections ─────────────────────────────────────────────────────
    let max_connections = partial
        .max_connections
        .ok_or_else(|| ConfigError::MissingRequired {
            key: "max_connections".into(),
            env_hint: "APP_MAX_CONNECTIONS".into(),
        })?;

    if max_connections == 0 {
        return Err(ConfigError::ValidationError {
            key: "max_connections".into(),
            reason: "must be at least 1 (got 0)".into(),
        });
    }

    // ── timeout_secs ────────────────────────────────────────────────────────
    let timeout_secs = partial
        .timeout_secs
        .ok_or_else(|| ConfigError::MissingRequired {
            key: "timeout_secs".into(),
            env_hint: "APP_TIMEOUT_SECS".into(),
        })?;

    if timeout_secs == 0 {
        return Err(ConfigError::ValidationError {
            key: "timeout_secs".into(),
            reason: "must be at least 1 second (got 0)".into(),
        });
    }

    Ok(Config {
        database_url,
        port,
        log_level,
        max_connections,
        timeout_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_partial() -> PartialConfig {
        PartialConfig {
            database_url: Some("postgres://localhost/test".into()),
            port: Some(8080),
            log_level: Some(LogLevel::Info),
            max_connections: Some(5),
            timeout_secs: Some(10),
            config_file: None,
        }
    }

    #[test]
    fn valid_partial_produces_config() {
        let cfg = validate(valid_partial()).expect("should validate");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.log_level, LogLevel::Info);
    }

    #[test]
    fn missing_database_url_returns_missing_required() {
        let mut p = valid_partial();
        p.database_url = None;
        let err = validate(p).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::MissingRequired { ref key, .. } if key == "database_url")
        );
    }

    #[test]
    fn empty_database_url_returns_validation_error() {
        let mut p = valid_partial();
        p.database_url = Some("   ".into());
        let err = validate(p).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::ValidationError { ref key, .. } if key == "database_url")
        );
    }

    #[test]
    fn port_zero_returns_validation_error() {
        let mut p = valid_partial();
        p.port = Some(0);
        let err = validate(p).expect_err("should fail");
        assert!(matches!(err, ConfigError::ValidationError { ref key, .. } if key == "port"));
    }

    #[test]
    fn max_connections_zero_returns_validation_error() {
        let mut p = valid_partial();
        p.max_connections = Some(0);
        let err = validate(p).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::ValidationError { ref key, .. } if key == "max_connections")
        );
    }

    #[test]
    fn timeout_zero_returns_validation_error() {
        let mut p = valid_partial();
        p.timeout_secs = Some(0);
        let err = validate(p).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::ValidationError { ref key, .. } if key == "timeout_secs")
        );
    }

    #[test]
    fn missing_port_returns_missing_required() {
        let mut p = valid_partial();
        p.port = None;
        let err = validate(p).expect_err("should fail");
        assert!(matches!(err, ConfigError::MissingRequired { ref key, .. } if key == "port"));
    }
}
