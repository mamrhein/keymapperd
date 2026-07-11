// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{thread, time::Duration};

use evdev::{Device, EventType, Key};
use parking_lot::RwLock;

use crate::{mapping_cache::NativeAction, state::Lookup};

/// Async-signal-safe handler that flips the shutdown flag.
extern "C" fn signal_handler(_sig: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Release);
}

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Scan `/dev/input/event*` for the first real keyboard device.
///
/// Filters out virtual (uinput) devices and devices that lack keyboard
/// capabilities.
fn find_keyboard_device() -> Result<Device, Box<dyn std::error::Error>> {
    let entries = std::fs::read_dir("/dev/input")?;
    let mut candidates: Vec<(std::path::PathBuf, String, Device)> = Vec::new();

    for entry in entries.filter_map(|e| e.ok()) {
        let name_str = entry.file_name().to_string_lossy().to_string();
        if !name_str.starts_with("event") {
            continue;
        }

        let path = entry.path();
        let Ok(device) = Device::open(&path) else {
            continue;
        };

        // Must support keyboard events.
        let supported = device.supported_events();
        if !supported.get(EventType::KEY).is_empty() {
            let dev_name = device.name().to_string();
            // Skip virtual/uinput devices — we don't want to intercept
            // our own synthetic events.
            let lower = dev_name.to_lowercase();
            if !lower.contains("virtual") && !lower.contains("uinput") {
                candidates.push((path, dev_name, device));
            }
        }
    }

    if candidates.is_empty() {
        return Err(
            "No keyboard device found in /dev/input/. \
             Ensure you have read permission on /dev/input/event*"
                .into(),
        );
    }

    // Prefer devices whose name contains common keyboard indicators.
    let prefer_keyword = |name: &str| {
        let l = name.to_lowercase();
        l.contains("keyboard") || l.contains("kbd") || l.contains("at set") || l.contains("apple")
    };

    if let Some((path, dev_name, device)) = candidates
        .iter()
        .find(|(_, name, _)| prefer_keyword(name))
        .map(|(p, n, d)| (p.clone(), n.clone(), d.clone()))
    {
        println!(
            "Linux: using keyboard device '{}' ({})",
            dev_name,
            path.display()
        );
        Ok(device)
    } else {
        // No named keyboard — take the first candidate.
        let (path, dev_name, device) = &candidates[0];
        println!(
            "Linux: no named keyboard found, falling back to '{}' ({})",
            dev_name,
            path.display()
        );
        if candidates.len() > 1 {
            println!(
                "Linux: other candidates: {}",
                candidates[1..]
                    .iter()
                    .map(|(p, n, _)| format!("{} ({})", n, p.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(device.clone())
    }
}

pub(crate) fn start_mapping(
    lookup: Arc<RwLock<dyn Lookup>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut raw_device = find_keyboard_device()?;
    raw_device.grab()?;

    let mut virtual_device = uinput::default()?
        .name("CrossPlatform_Virtual_Keyboard")?
        .event(uinput::event::Keyboard::All)?
        .create()?;

    thread::sleep(Duration::from_millis(200));
    println!("Linux uinput loop virtual keyboard ready.");

    // Register signal handlers for graceful shutdown.
    let handler_ptr = signal_handler as *const () as usize;
    unsafe {
        libc::signal(libc::SIGINT, handler_ptr);
        libc::signal(libc::SIGTERM, handler_ptr);
    }

    // Poll loop with shutdown check.
    while !SHUTDOWN_REQUESTED.load(Ordering::Acquire) {
        match raw_device.fetch_events() {
            Ok(events) => {
                for event in events {
                    if event.event_type() == EventType::KEY {
                        // event.code() returns u16 — matches NativeKey directly.
                        let code = event.code();
                        let value = event.value(); // 1 = Down, 0 = Up, 2 = Repeat

                        let guard = lookup.read();
                        let current_app = guard.active_app().to_lowercase();
                        let active_action = guard
                            .for_app(&current_app, code)
                            .or_else(|| guard.global(code));

                        if let Some(action) = active_action {
                            match action {
                                NativeAction::RemapTo(target) => {
                                    let key = Key::new(*target);
                                    if value == 1 {
                                        virtual_device.press(&key)?;
                                    } else if value == 0 {
                                        virtual_device.release(&key)?;
                                    }
                                    virtual_device.synchronize()?;
                                }
                                NativeAction::Shortcut(targets) => {
                                    if value == 1 {
                                        for t in targets.iter() {
                                            let key = Key::new(*t);
                                            virtual_device.press(&key)?;
                                        }
                                    } else if value == 0 {
                                        for t in targets.iter().rev() {
                                            let key = Key::new(*t);
                                            virtual_device.release(&key)?;
                                        }
                                    }
                                    virtual_device.synchronize()?;
                                }
                            }
                        } else {
                            // Passthrough
                            let key = Key::new(code);
                            if value == 1 {
                                virtual_device.press(&key)?;
                            } else if value == 0 {
                                virtual_device.release(&key)?;
                            }
                            virtual_device.synchronize()?;
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No events available — sleep and retry.
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                eprintln!("Linux: error reading events: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    println!("Shutdown signal received. Cleaning up...");
    Ok(())
}
