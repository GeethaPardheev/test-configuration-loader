use crate::partial::PartialConfig;

/// Merge two [`PartialConfig`] values, with `overlay` taking precedence over
/// `base`.
///
/// Each field uses `Option::or`: if `overlay` has `Some(value)`, that value
/// wins; otherwise the `base` value is kept.  A `None` in `overlay` never
/// clears a `Some` in `base`.
///
/// This function is intentionally pure (no side-effects, no I/O) so that it
/// can be tested in complete isolation.
pub fn merge(base: PartialConfig, overlay: PartialConfig) -> PartialConfig {
    PartialConfig {
        database_url: overlay.database_url.or(base.database_url),
        port: overlay.port.or(base.port),
        log_level: overlay.log_level.or(base.log_level),
        max_connections: overlay.max_connections.or(base.max_connections),
        timeout_secs: overlay.timeout_secs.or(base.timeout_secs),
        // config_file is only meaningful as a transport; it is not merged
        // into the final Config.
        config_file: overlay.config_file.or(base.config_file),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partial::LogLevel;

    #[test]
    fn overlay_wins_over_base() {
        let base = PartialConfig {
            port: Some(8080),
            ..PartialConfig::default()
        };
        let overlay = PartialConfig {
            port: Some(9090),
            ..PartialConfig::default()
        };
        let result = merge(base, overlay);
        assert_eq!(result.port, Some(9090));
    }

    #[test]
    fn none_in_overlay_does_not_erase_base() {
        let base = PartialConfig {
            port: Some(8080),
            ..PartialConfig::default()
        };
        let overlay = PartialConfig::default(); // port is None
        let result = merge(base, overlay);
        assert_eq!(result.port, Some(8080));
    }

    #[test]
    fn both_none_stays_none() {
        let base = PartialConfig::default();
        let overlay = PartialConfig::default();
        let result = merge(base, overlay);
        assert!(result.port.is_none());
    }

    #[test]
    fn full_three_layer_merge_respects_precedence() {
        // Simulate: defaults → file → env
        let defaults = PartialConfig {
            port: Some(8080),
            log_level: Some(LogLevel::Info),
            max_connections: Some(10),
            timeout_secs: Some(30),
            ..PartialConfig::default()
        };
        let file = PartialConfig {
            port: Some(5000), // overrides default
            database_url: Some("postgres://file-db".into()),
            ..PartialConfig::default()
        };
        let env = PartialConfig {
            port: Some(9999), // highest priority — wins
            ..PartialConfig::default()
        };

        let merged = merge(merge(defaults, file), env);

        assert_eq!(merged.port, Some(9999)); // env wins
        assert_eq!(merged.database_url.as_deref(), Some("postgres://file-db")); // from file
        assert_eq!(merged.log_level, Some(LogLevel::Info)); // from defaults
        assert_eq!(merged.max_connections, Some(10)); // from defaults
    }
}
