use mvpn_core::config::Config;
use mvpn_providers::create_provider;

pub fn run(config: &Config) {
    for entry in &config.autoconnect.connections {
        let provider = create_provider(entry.provider);
        if !provider.is_available() {
            eprintln!(
                "autoconnect: {} provider not available, skipping {}",
                entry.provider, entry.id
            );
            continue;
        }
        match provider.connect(&entry.id) {
            Ok(()) => eprintln!("autoconnect: connected {} {}", entry.provider, entry.id),
            Err(e) => eprintln!("autoconnect: failed {} {}: {e}", entry.provider, entry.id),
        }
    }
}
