// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use std::{ptr::null_mut, sync::Arc};

use parking_lot::RwLock;
use windows_sys::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

use crate::{mapping_cache::NativeAction, state::Lookup};

static mut HOOK_HANDLE: windows_sys::Win32::UI::WindowsAndFiltering::HHOOK = 0;
// TODO(#8): Replace static mut with a safe alternative (e.g., thread-local
// or hook-proc redesign). Windows LL keyboard hook API does not support
// passing user data, so a global is currently unavoidable.
static mut SHARED_LOOKUP: Option<Arc<RwLock<dyn Lookup>>> = None;

pub(crate) fn start_mapping(
    lookup: Arc<RwLock<dyn Lookup>>,
) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        SHARED_LOOKUP = Some(lookup);
        let h_instance: HINSTANCE = GetModuleHandleW(null_mut());

        HOOK_HANDLE = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            h_instance,
            0,
        );

        if HOOK_HANDLE == 0 {
            return Err("Failed to install global hook".into());
        }
        println!("Windows low-level hook listening...");

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {}

        UnhookWindowsHookEx(HOOK_HANDLE);
    }
    Ok(())
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if code >= 0 {
        if let Some(ref lookup) = SHARED_LOOKUP {
            let kbd_struct = *(l_param as *const KBDLLHOOKSTRUCT);
            let vk_code = kbd_struct.vkCode;

            let is_key_down = w_param as u32 == WM_KEYDOWN
                || w_param as u32 == WM_SYSKEYDOWN;
            let is_key_up =
                w_param as u32 == WM_KEYUP || w_param as u32 == WM_SYSKEYUP;

            let guard = lookup.read();
            let current_app = guard.active_app().to_lowercase();
            let active_action = guard
                .for_app(&current_app, vk_code)
                .or_else(|| guard.global(vk_code));

            if let Some(action) = active_action {
                match action {
                    NativeAction::RemapTo(target_vk) => {
                        simulate_key_event(**target_vk as u8, is_key_up);
                    }
                    NativeAction::Shortcut(target_vks) => {
                        if is_key_down {
                            for vk in *target_vks {
                                simulate_key_event(*vk as u8, false);
                            }
                        } else if is_key_up {
                            for vk in target_vks.iter().rev() {
                                simulate_key_event(*vk as u8, true);
                            }
                        }
                    }
                }
                return 1; // Swallow key
            }
        }
    }
    CallNextHookEx(HOOK_HANDLE, code, w_param, l_param)
}

unsafe fn simulate_key_event(vk_byte: u8, is_key_up: bool) {
    use windows_sys::Win32::UI::WindowsAndFiltering::{
        KEYEVENTF_KEYUP, keybd_event,
    };
    let flags = if is_key_up { KEYEVENTF_KEYUP } else { 0 };
    keybd_event(vk_byte, 0, flags, 0);
}
