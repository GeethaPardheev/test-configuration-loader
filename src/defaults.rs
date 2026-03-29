use crate::partial::{LogLevel, PartialConfig};

/// Returns a [`PartialConfig`] populated with every hard-coded default value.
///
/// Fields that have no sensible default (e.g. `database_url`) are left as
/// `None` so that validation can later produce a clear, actionable error
/// instead of silently using an invalid placeholder.
pub fn defaults() -> PartialConfig {
    PartialConfig {
        database_url: None, // required — must be supplied by env or file
        port: Some(8080),
        log_level: Some(LogLevel::Info),
        max_connections: Some(10),
        timeout_secs: Some(30),
        config_file: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_expected_values() {
        let d = defaults();
        assert_eq!(d.port, Some(8080));
        assert_eq!(d.log_level, Some(LogLevel::Info));
        assert_eq!(d.max_connections, Some(10));
        assert_eq!(d.timeout_secs, Some(30));
        // database_url intentionally has no default
        assert!(d.database_url.is_none());
    }
}
