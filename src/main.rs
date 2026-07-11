// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

mod config;
mod hot_reload;
mod mapping_cache;
mod os;
mod os_bridge;

use std::{sync::Arc, thread, time::Duration};

use parking_lot::RwLock;

use crate::{config::AppConfig, mapping_cache::RuntimeLookupCache};

pub struct RuntimeState {
    pub lookup_cache: RuntimeLookupCache,
    pub active_app: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = "config.toml";

    // Create a fallback config file if it does not exist
    if !std::path::Path::new(config_path).exists() {
        std::fs::write(
            config_path,
            "[[rules]]\ntrigger = \"CapsLock\"\naction = { RemapTo = \
             \"LeftControl\" }\napplications = []",
        )?;
    }

    let initial_content = std::fs::read_to_string(config_path)?;
    let parsed_config = AppConfig::load_from_str(&initial_content)?;
    let initial_cache =
        RuntimeLookupCache::compile_from_config(&parsed_config);

    let state = Arc::new(RwLock::new(RuntimeState {
        lookup_cache: initial_cache,
        active_app: String::from("unknown"),
    }));

    // Start hot-reloader thread
    let _watcher =
        hot_reload::start_config_watcher(config_path, Arc::clone(&state))?;

    // Start tracking foreground windows natively
    let tracker_state = Arc::clone(&state);
    thread::spawn(move || {
        println!("Native window tracking thread active.");
        loop {
            let current_focused_app =
                match active_win_pos_rs::get_active_window() {
                    Ok(window) => window.app_name,
                    Err(_) => String::from("unknown"),
                };

            // Acquire a read lock first to check whether the value actually
            // changed. This avoids evicting concurrent readers on every poll
            // cycle (10 Hz) when the foreground app is stable.
            if !tracker_state.read().active_app.eq(&current_focused_app) {
                let mut write_guard = tracker_state.write();
                // Re-check behind the write lock in case another writer
                // already updated between the read-check and now.
                if !write_guard.active_app.eq(&current_focused_app) {
                    write_guard.active_app = current_focused_app;
                }
            }

            thread::sleep(Duration::from_millis(100));
        }
    });

    println!("Cross-platform runtime engines fully synchronized.");
    let input_state = Arc::clone(&state);

    crate::os::start_mapping(input_state)
}
