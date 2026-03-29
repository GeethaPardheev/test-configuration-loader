//! # configuration-loader
//!
//! A robust, strongly-typed unified configuration loader that merges values
//! from three sources in ascending priority order:
//!
//! 1. **Hard-coded defaults** — baseline values baked into the binary.
//! 2. **Config file** — a `config.toml` or `config.yaml` file discovered
//!    automatically or pointed to via `APP_CONFIG_FILE`.
//! 3. **Environment variables** — prefixed `APP_*`, always win.
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

pub use error::ConfigError;
pub use partial::{LogLevel, PartialConfig};
pub use validate::Config;

#[cfg(test)]
mod integration_tests {
    use std::io::Write;
    use std::sync::Mutex;

    use tempfile::NamedTempFile;

    use crate::partial::{LogLevel, PartialConfig};
    use crate::validate::Config;
    use crate::{defaults, file, merge, validate, ConfigError};

    // A single process-wide lock so that tests which temporarily mutate the
    // environment do not interfere with one another when run in parallel.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn write_toml(contents: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".toml")
            .tempfile()
            .expect("create temp file");
        f.write_all(contents.as_bytes()).expect("write temp file");
        f
    }

    fn write_yaml(contents: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".yaml")
            .tempfile()
            .expect("create temp file");
        f.write_all(contents.as_bytes()).expect("write temp file");
        f
    }

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

    /// Sets `key=value`, runs `f`, then restores the previous value.
    /// Holds `ENV_LOCK` so the mutation is exclusive across the test binary.
    fn with_env<F: FnOnce()>(key: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let prev = std::env::var(key).ok();
        // SAFETY: we hold ENV_LOCK, ensuring no other thread reads or writes
        // this variable concurrently.
        unsafe { std::env::set_var(key, value) };
        f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    // ── layer interaction ────────────────────────────────────────────────────

    #[test]
    fn defaults_alone_fail_because_database_url_is_required() {
        let err =
            validate::validate(defaults::defaults()).expect_err("no source supplies database_url");
        assert!(
            matches!(err, ConfigError::MissingRequired { ref key, .. } if key == "database_url")
        );
    }

    #[test]
    fn file_overrides_defaults() {
        let f = write_toml(
            r#"
database_url = "postgres://db-host/orders"
port = 5432
"#,
        );
        let merged = merge::merge(
            defaults::defaults(),
            file::from_file(Some(f.path())).expect("parse"),
        );
        let cfg = validate::validate(merged).expect("validate");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.database_url, "postgres://db-host/orders");
        assert_eq!(cfg.max_connections, 10); // default intact
    }

    #[test]
    fn env_overrides_file_port() {
        let f = write_toml("database_url = \"postgres://file/db\"\nport = 5432\n");
        with_env("APP_PORT", "9000", || {
            let cfg = Config::load_from(f.path()).expect("load");
            assert_eq!(cfg.port, 9000);
        });
    }

    #[test]
    fn env_overrides_default_port() {
        with_env("APP_PORT", "7000", || {
            let f = write_toml("database_url = \"postgres://localhost/myapp\"\n");
            assert_eq!(Config::load_from(f.path()).expect("load").port, 7000);
        });
    }

    #[test]
    fn three_layer_merge_full_precedence() {
        // defaults: port=8080, log_level=info, max_connections=10, timeout=30
        // file:     port=5000, database_url set
        // env:      port=9999   <- wins
        let f = write_toml("database_url = \"postgres://prod/main\"\nport = 5000\n");
        with_env("APP_PORT", "9999", || {
            let cfg = Config::load_from(f.path()).expect("load");
            assert_eq!(cfg.port, 9999);
            assert_eq!(cfg.database_url, "postgres://prod/main");
            assert_eq!(cfg.log_level, LogLevel::Info);
            assert_eq!(cfg.max_connections, 10);
            assert_eq!(cfg.timeout_secs, 30);
        });
    }

    // ── validation ───────────────────────────────────────────────────────────

    #[test]
    fn from_partial_produces_correct_config() {
        let cfg = Config::from_partial(valid_partial()).expect("valid");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.database_url, "postgres://localhost/test");
        assert_eq!(cfg.log_level, LogLevel::Info);
        assert_eq!(cfg.max_connections, 5);
        assert_eq!(cfg.timeout_secs, 10);
    }

    #[test]
    fn missing_database_url_gives_actionable_error() {
        let f = write_toml("port = 3000\n");
        match Config::load_from(f.path()).expect_err("missing db url") {
            ConfigError::MissingRequired { key, env_hint } => {
                assert_eq!(key, "database_url");
                assert!(
                    env_hint.contains("APP_DATABASE_URL"),
                    "hint was: {env_hint}"
                );
            }
            other => panic!("wrong variant: {other}"),
        }
    }

    #[test]
    fn partial_file_does_not_silently_supply_missing_required_field() {
        // Only port in the file — database_url absent from every source.
        let f = write_toml("port = 4000\n");
        assert!(matches!(
            Config::load_from(f.path()).expect_err("should fail"),
            ConfigError::MissingRequired { .. }
        ));
    }

    // ── error types ──────────────────────────────────────────────────────────

    #[test]
    fn bad_env_var_value_gives_typed_error() {
        with_env("APP_PORT", "not_a_port", || {
            let f = write_toml("database_url = \"postgres://localhost/myapp\"\n");
            assert!(matches!(
                Config::load_from(f.path()).expect_err("bad port"),
                ConfigError::InvalidEnvVar { .. }
            ));
        });
    }

    #[test]
    fn malformed_toml_gives_parse_error() {
        let f = write_toml("this is not toml !!!\n");
        assert!(matches!(
            Config::load_from(f.path()).expect_err("malformed"),
            ConfigError::ParseError { .. }
        ));
    }

    #[test]
    fn nonexistent_explicit_path_gives_file_not_found() {
        assert!(matches!(
            Config::load_from(std::path::Path::new("/no/such/file.toml"))
                .expect_err("missing file"),
            ConfigError::FileNotFound { .. }
        ));
    }

    #[test]
    fn port_zero_rejected_by_validate() {
        let mut p = valid_partial();
        p.port = Some(0);
        let err = Config::from_partial(p).expect_err("port 0");
        assert!(matches!(err, ConfigError::ValidationError { ref key, .. } if key == "port"));
    }

    // ── format support ───────────────────────────────────────────────────────

    #[test]
    fn yaml_file_parsed_correctly() {
        let f = write_yaml(
            "database_url: \"postgres://yaml-host/analytics\"\nport: 5555\nlog_level: warn\n",
        );
        let cfg = Config::load_from(f.path()).expect("yaml");
        assert_eq!(cfg.port, 5555);
        assert_eq!(cfg.log_level, LogLevel::Warn);
        assert_eq!(cfg.database_url, "postgres://yaml-host/analytics");
    }

    // ── LogLevel parsing ─────────────────────────────────────────────────────

    #[test]
    fn log_level_parses_all_variants() {
        let cases: &[(&str, LogLevel)] = &[
            ("trace", LogLevel::Trace),
            ("debug", LogLevel::Debug),
            ("info", LogLevel::Info),
            ("warn", LogLevel::Warn),
            ("warning", LogLevel::Warn),
            ("error", LogLevel::Error),
        ];
        for (s, expected) in cases {
            let got: LogLevel = s.parse().unwrap_or_else(|_| panic!("parse `{s}`"));
            assert_eq!(got, *expected);
        }
    }

    #[test]
    fn unknown_log_level_string_is_rejected() {
        assert!("verbose".parse::<LogLevel>().is_err());
    }
}
