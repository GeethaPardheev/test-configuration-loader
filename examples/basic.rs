use configuration_loader::Config;

fn main() {
    println!("=== Unified Configuration Loader — Basic Example ===\n");
    println!("Tip: set APP_DATABASE_URL, APP_PORT, APP_LOG_LEVEL, etc. to override defaults.");
    println!("     Or point APP_CONFIG_FILE at a .toml / .yaml file.\n");

    match Config::load() {
        Ok(cfg) => {
            println!("✅ Configuration loaded successfully:");
            println!("   database_url    = {}", cfg.database_url);
            println!("   port            = {}", cfg.port);
            println!("   log_level       = {}", cfg.log_level);
            println!("   max_connections = {}", cfg.max_connections);
            println!("   timeout_secs    = {}", cfg.timeout_secs);
        }
        Err(e) => {
            eprintln!("❌ Failed to load configuration:\n   {e}");
            std::process::exit(1);
        }
    }
}
