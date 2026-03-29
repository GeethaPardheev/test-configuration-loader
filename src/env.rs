use std::str::FromStr;

use crate::error::ConfigError;
use crate::partial::{LogLevel, PartialConfig};

/// The environment variable prefix used for all config keys.
const PREFIX: &str = "APP_";

/// Helper: read an optional env var string.
#[must_use]
fn read_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Helper: parse an env var into type `T`, producing a typed error on failure.
fn parse_var<T>(key: &str, raw: &str) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    raw.parse::<T>().map_err(|e| ConfigError::InvalidEnvVar {
        key: key.to_owned(),
        value: raw.to_owned(),
        reason: e.to_string(),
    })
}

/// Load a [`PartialConfig`] from the process environment.
///
/// Environment variable mapping:
///
/// | Env var              | Field            | Type      |
/// |----------------------|------------------|-----------|
/// | `APP_DATABASE_URL`   | `database_url`   | `String`  |
/// | `APP_PORT`           | `port`           | `u16`     |
/// | `APP_LOG_LEVEL`      | `log_level`      | `LogLevel`|
/// | `APP_MAX_CONNECTIONS`| `max_connections`| `u32`     |
/// | `APP_TIMEOUT_SECS`   | `timeout_secs`   | `u64`     |
/// | `APP_CONFIG_FILE`    | `config_file`    | `String`  |
///
/// Missing variables are silently `None`; present-but-invalid variables
/// return [`ConfigError::InvalidEnvVar`].
///
/// # Errors
/// Returns a [`ConfigError::InvalidEnvVar`] if any environment variable
/// is present but fails to parse into its expected type.
pub fn from_env() -> Result<PartialConfig, ConfigError> {
    let key_db = format!("{PREFIX}DATABASE_URL");
    let key_port = format!("{PREFIX}PORT");
    let key_log = format!("{PREFIX}LOG_LEVEL");
    let key_max = format!("{PREFIX}MAX_CONNECTIONS");
    let key_timeout = format!("{PREFIX}TIMEOUT_SECS");
    let key_file = format!("{PREFIX}CONFIG_FILE");

    let database_url = read_var(&key_db);

    let port = read_var(&key_port)
        .map(|v| parse_var::<u16>(&key_port, &v))
        .transpose()?;

    let log_level = read_var(&key_log)
        .map(|v| parse_var::<LogLevel>(&key_log, &v))
        .transpose()?;

    let max_connections = read_var(&key_max)
        .map(|v| parse_var::<u32>(&key_max, &v))
        .transpose()?;

    let timeout_secs = read_var(&key_timeout)
        .map(|v| parse_var::<u64>(&key_timeout, &v))
        .transpose()?;

    let config_file = read_var(&key_file);

    Ok(PartialConfig {
        database_url,
        port,
        log_level,
        max_connections,
        timeout_secs,
        config_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A process-wide lock that all env-modifying tests must hold.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<F: FnOnce()>(key: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let prev = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }
        f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn reads_port_from_env() {
        with_env("APP_PORT", "9090", || {
            let p = from_env().expect("from_env should succeed");
            assert_eq!(p.port, Some(9090_u16));
        });
    }

    #[test]
    fn invalid_port_returns_error() {
        with_env("APP_PORT", "not_a_number", || {
            let err = from_env().expect_err("should fail with invalid port");
            assert!(matches!(err, ConfigError::InvalidEnvVar { .. }));
        });
    }

    #[test]
    fn reads_log_level_case_insensitive() {
        with_env("APP_LOG_LEVEL", "WARN", || {
            let p = from_env().expect("from_env should succeed");
            assert_eq!(p.log_level, Some(LogLevel::Warn));
        });
    }

    #[test]
    fn invalid_log_level_returns_error() {
        with_env("APP_LOG_LEVEL", "verbose", || {
            let err = from_env().expect_err("should fail with invalid log level");
            assert!(matches!(err, ConfigError::InvalidEnvVar { .. }));
        });
    }

    #[test]
    fn missing_env_vars_produce_none_fields() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        for key in &[
            "APP_DATABASE_URL",
            "APP_PORT",
            "APP_LOG_LEVEL",
            "APP_MAX_CONNECTIONS",
            "APP_TIMEOUT_SECS",
            "APP_CONFIG_FILE",
        ] {
            unsafe {
                std::env::remove_var(key);
            }
        }
        let p = from_env().expect("from_env should succeed with no vars set");
        assert!(p.database_url.is_none());
        assert!(p.port.is_none());
        assert!(p.log_level.is_none());
    }
}
