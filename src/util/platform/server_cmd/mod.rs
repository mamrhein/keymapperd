// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

//! Platform-specific helpers for managing the keymapperd daemon process.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

const DAEMON_NAME: &str = "keymapperd";

/// Check whether a keymapperd process is running for the current user.
#[cfg(target_os = "macos")]
pub fn is_running() -> bool {
    macos::is_daemon_running(DAEMON_NAME)
}

#[cfg(target_os = "linux")]
pub fn is_running() -> bool {
    linux::is_daemon_running(DAEMON_NAME)
}

#[cfg(target_os = "windows")]
pub fn is_running() -> bool {
    windows::is_daemon_running(DAEMON_NAME)
}

/// Attempt to start keymapperd as a background process.
///
/// Returns `Ok(())` when the daemon was spawned and verified to be running.
/// Returns `Err` if the executable could not be found, the spawn failed, or
/// the process exited immediately after starting.
#[cfg(target_os = "macos")]
pub fn start() -> Result<(), String> {
    let spawn_result = macos::spawn_daemon(DAEMON_NAME);
    verify_start(spawn_result)
}

#[cfg(target_os = "linux")]
pub fn start() -> Result<(), String> {
    let spawn_result = linux::spawn_daemon(DAEMON_NAME);
    verify_start(spawn_result)
}

#[cfg(target_os = "windows")]
pub fn start() -> Result<(), String> {
    let spawn_result = windows::spawn_daemon(DAEMON_NAME);
    verify_start(spawn_result)
}

/// After a successful spawn, wait briefly and confirm the daemon is still alive.
fn verify_start(spawn_result: Result<(), String>) -> Result<(), String> {
    spawn_result?;

    // Give the daemon time to initialize or fail.
    std::thread::sleep(std::time::Duration::from_millis(500));

    if !is_running() {
        return Err("daemon started but exited immediately".to_string());
    }

    Ok(())
}
