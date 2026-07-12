// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use std::{path::Path, sync::Arc};

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use parking_lot::RwLock;

use crate::{mapping_cache::RuntimeLookupCache, state::Lookup};

pub fn start_config_watcher<P: AsRef<Path>>(
    config_path: P,
    state: Arc<RwLock<dyn Lookup>>,
) -> Result<RecommendedWatcher, notify::Error> {
    let path_to_watch = config_path.as_ref().to_owned();

    // Create a cross-platform watcher infrastructure.
    // The closure triggers whenever an OS file system signal fires.
    let mut watcher = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| match result {
            Ok(event) => {
                // We only care about file modifications (e.g., user hits save
                // in text editor)
                if let EventKind::Modify(_) = event.kind {
                    println!(
                        "Configuration file modification detected. \
                         Reloading..."
                    );

                    // Reparse and recompile via the shared loader
                    match RuntimeLookupCache::compile_from_path(&path_to_watch)
                    {
                        Ok(new_cache) => {
                            // Safely acquire a write lock and swap out the
                            // cache via the trait interface
                            let mut write_guard = state.write();
                            write_guard.set_lookup_cache(new_cache);
                            println!(
                                "Configuration hot-swapped successfully!"
                            );
                        }
                        Err(err) => {
                            eprintln!(
                                "Failed to hot-reload configuration: {}",
                                err
                            );
                            eprintln!(
                                "Keeping previous configuration rules safe \
                                 to prevent crashes."
                            );
                        }
                    }
                }
            }
            Err(e) => eprintln!("File system watcher error: {:?}", e),
        },
        Config::default(),
    )?;

    watcher.watch(config_path.as_ref(), RecursiveMode::NonRecursive)?;

    Ok(watcher)
}
