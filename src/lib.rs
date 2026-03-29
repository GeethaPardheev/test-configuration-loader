//! # configuration-loader
//!
//! A robust, strongly-typed unified configuration loader.
//!
//! Configuration values are loaded from three sources in ascending priority
//! order:
//!
//! 1. **Hard-coded defaults** — sensible baseline values defined in code.
//! 2. **Config file** — a `config.toml` or `config.yaml` file, either
//!    auto-discovered or pointed to via `APP_CONFIG_FILE`.
//! 3. **Environment variables** — prefixed with `APP_`; always win.
//!
//! ## Quick start
//!
//! ```no_run
//! use configuration_loader::Config;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Config::load()?;
//!     println!("Listening on port {}", config.port);
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod defaults;
pub mod env;
pub mod error;
pub mod file;
pub mod hot_reload;
pub mod merge;
pub mod partial;
pub mod validate;

// Re-export the most commonly used items at the crate root for convenience.
pub use error::ConfigError;
pub use partial::{LogLevel, PartialConfig};
pub use validate::Config;

#[cfg(test)]
mod integration_tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use crate::partial::{LogLevel, PartialConfig};
    use crate::validate::Config;
    use crate::{defaults, file, merge, validate, ConfigError};

    // ── helpers ──────────────────────────────────────────────────────────────

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

    fn write_toml(contents: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".toml")
            .tempfile()
            .expect("create temp toml");
        f.write_all(contents.as_bytes()).expect("write temp toml");
        f
    }

    fn write_yaml(contents: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".yaml")
            .tempfile()
            .expect("create temp yaml");
        f.write_all(contents.as_bytes()).expect("write temp yaml");
        f
    }

    use std::sync::Mutex;

    /// Serialises all env-mutating tests so parallel test threads cannot
    /// interfere with each other's `APP_*` variable state.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Set an env var for the duration of `f`, then restore previous value.
    /// Holds `ENV_LOCK` so the mutation is exclusive across the test suite.
    fn with_env<F: FnOnce()>(key: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let prev = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn from_partial_happy_path() {
        let cfg = Config::from_partial(valid_partial()).expect("valid partial should succeed");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.log_level, LogLevel::Info);
        assert_eq!(cfg.database_url, "postgres://localhost/test");
        assert_eq!(cfg.max_connections, 5);
        assert_eq!(cfg.timeout_secs, 10);
    }

    #[test]
    fn defaults_alone_fail_missing_database_url() {
        let d = defaults::defaults();
        let err = validate::validate(d).expect_err("defaults alone should fail (no database_url)");
        assert!(
            matches!(err, ConfigError::MissingRequired { ref key, .. } if key == "database_url")
        );
    }

    #[test]
    fn file_overrides_defaults() {
        let f = write_toml(
            r#"
database_url = "postgres://file-host/db"
port = 5432
"#,
        );
        let file_partial = file::from_file(Some(f.path())).expect("parse file");
        let merged = merge::merge(defaults::defaults(), file_partial);
        let cfg = validate::validate(merged).expect("should validate");
        assert_eq!(cfg.port, 5432); // file wins over default 8080
        assert_eq!(cfg.database_url, "postgres://file-host/db");
        assert_eq!(cfg.max_connections, 10); // default survives
    }

    #[test]
    fn env_overrides_file_and_defaults() {
        let f = write_toml("database_url = \"postgres://file/db\"\nport = 5432\n");
        with_env("APP_PORT", "9999", || {
            let cfg = Config::load_from(f.path()).expect("should load");
            assert_eq!(cfg.port, 9999); // env beats file's 5432
        });
    }

    #[test]
    fn env_overrides_defaults_directly() {
        with_env("APP_PORT", "7777", || {
            let f = write_toml("database_url = \"postgres://localhost/db\"\n");
            let cfg = Config::load_from(f.path()).expect("should load");
            assert_eq!(cfg.port, 7777);
        });
    }

    #[test]
    fn missing_database_url_everywhere_gives_clear_error() {
        // No db url in defaults, file, or env.
        let f = write_toml("port = 3000\n");
        let err = Config::load_from(f.path()).expect_err("should fail");
        match err {
            ConfigError::MissingRequired {
                ref key,
                ref env_hint,
            } => {
                assert_eq!(key, "database_url");
                assert!(env_hint.contains("APP_DATABASE_URL"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn partial_config_no_silent_fallback() {
        // File only supplies `port`; `database_url` is absent from all
        // sources.  The loader must fail loudly, not silently use an invalid
        // default.
        let f = write_toml("port = 4000\n");
        let err = Config::load_from(f.path()).expect_err("should fail");
        assert!(matches!(err, ConfigError::MissingRequired { .. }));
    }

    #[test]
    fn invalid_env_var_produces_typed_error() {
        with_env("APP_PORT", "not_a_port", || {
            let f = write_toml("database_url = \"postgres://localhost/db\"\n");
            let err = Config::load_from(f.path()).expect_err("should fail");
            assert!(matches!(err, ConfigError::InvalidEnvVar { .. }));
        });
    }

    #[test]
    fn port_zero_in_env_produces_validation_error() {
        with_env("APP_PORT", "0", || {
            let err = Config::from_partial(PartialConfig {
                database_url: Some("postgres://localhost/db".into()),
                port: Some(0),
                log_level: Some(LogLevel::Info),
                max_connections: Some(5),
                timeout_secs: Some(10),
                config_file: None,
            })
            .expect_err("port 0 should fail validation");
            assert!(matches!(err, ConfigError::ValidationError { ref key, .. } if key == "port"));
        });
    }

    #[test]
    fn yaml_file_loaded_correctly() {
        let f =
            write_yaml("database_url: \"postgres://yaml-host/db\"\nport: 5555\nlog_level: warn\n");
        let cfg = Config::load_from(f.path()).expect("yaml should load");
        assert_eq!(cfg.port, 5555);
        assert_eq!(cfg.log_level, LogLevel::Warn);
        assert_eq!(cfg.database_url, "postgres://yaml-host/db");
    }

    #[test]
    fn log_level_all_variants_parse() {
        for (s, expected) in &[
            ("trace", LogLevel::Trace),
            ("debug", LogLevel::Debug),
            ("info", LogLevel::Info),
            ("warn", LogLevel::Warn),
            ("warning", LogLevel::Warn),
            ("error", LogLevel::Error),
        ] {
            let parsed: LogLevel = s.parse().expect("should parse");
            assert_eq!(&parsed, expected, "failed for `{s}`");
        }
    }

    #[test]
    fn unknown_log_level_returns_error() {
        let result = "verbose".parse::<LogLevel>();
        assert!(result.is_err());
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let f = write_toml("this is not toml !!!\n");
        let err = Config::load_from(f.path()).expect_err("should fail");
        assert!(matches!(err, ConfigError::ParseError { .. }));
    }

    #[test]
    fn nonexistent_explicit_file_returns_file_not_found() {
        let err =
            Config::load_from(std::path::Path::new("/no/such/file.toml")).expect_err("should fail");
        assert!(matches!(err, ConfigError::FileNotFound { .. }));
    }

    #[test]
    fn three_layer_merge_end_to_end() {
        // defaults: port=8080, log_level=info, max_connections=10, timeout=30
        // file:     port=5000, database_url=file-db
        // env:      port=9999
        // expected: port=9999 (env), database_url=file-db (file),
        //           log_level=info (default), max_connections=10 (default)

        let f = write_toml("database_url = \"postgres://file/db\"\nport = 5000\n");
        with_env("APP_PORT", "9999", || {
            let cfg = Config::load_from(f.path()).expect("should succeed");
            assert_eq!(cfg.port, 9999);
            assert_eq!(cfg.database_url, "postgres://file/db");
            assert_eq!(cfg.log_level, LogLevel::Info);
            assert_eq!(cfg.max_connections, 10);
            assert_eq!(cfg.timeout_secs, 30);
        });
    }
}
