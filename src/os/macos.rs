// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use objc2_core_foundation::{
    CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGKeyCode,
};
use parking_lot::RwLock;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{key_names, mapping_cache::NativeKey, state::Lookup};

// ---------------------------------------------------------------------------
// Platform-specific Key enum — discriminants ARE the CGKeyCode values
// ---------------------------------------------------------------------------

/// macOS virtual keycode for a physical key on a US ANSI keyboard.
///
/// Discriminant values come from `<HIToolbox/Events.h>` (`kVK_*` constants).
/// `key as u16` yields the native CGKeyCode — no translation needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Key {
    // --- Modifiers ---
    LeftControl = 59,  // kVK_Control
    RightControl = 62, // kVK_RightControl
    LeftShift = 56,    // kVK_Shift
    RightShift = 60,   // kVK_RightShift
    LeftAlt = 58,      // kVK_Option
    RightAlt = 61,     // kVK_RightOption
    LeftCommand = 55,  // kVK_Command
    RightCommand = 54, // kVK_RightCommand
    CapsLock = 27,     // kVK_CapsLock
    // --- Editor / misc ---
    Tab = 48,       // kVK_Tab
    Space = 49,     // kVK_Space
    Return = 36,    // kVK_Return
    Backspace = 51, // kVK_Delete
    Delete = 117,   // kVK_ForwardDelete
    Escape = 53,    // kVK_Escape
    // --- Navigation ---
    UpArrow = 126,    // kVK_UpArrow
    DownArrow = 125,  // kVK_DownArrow
    LeftArrow = 123,  // kVK_LeftArrow
    RightArrow = 124, // kVK_RightArrow
    PageUp = 116,     // kVK_PageUp
    PageDown = 121,   // kVK_PageDown
    Home = 115,       // kVK_Home
    End = 119,        // kVK_End
    // --- Function keys ---
    F1 = 122,  // kVK_F1
    F2 = 120,  // kVK_F2
    F3 = 99,   // kVK_F3
    F4 = 118,  // kVK_F4
    F5 = 96,   // kVK_F5
    F6 = 97,   // kVK_F6
    F7 = 98,   // kVK_F7
    F8 = 100,  // kVK_F8
    F9 = 101,  // kVK_F9
    F10 = 109, // kVK_F10
    F11 = 103, // kVK_F11
    F12 = 111, // kVK_F12
    // --- Letters ---
    A = 0,  // kVK_ANSI_A
    B = 11, // kVK_ANSI_B
    C = 8,  // kVK_ANSI_C
    D = 2,  // kVK_ANSI_D
    E = 14, // kVK_ANSI_E
    F = 3,  // kVK_ANSI_F
    G = 5,  // kVK_ANSI_G
    H = 4,  // kVK_ANSI_H
    I = 34, // kVK_ANSI_I
    J = 38, // kVK_ANSI_J
    K = 40, // kVK_ANSI_K
    L = 37, // kVK_ANSI_L
    M = 46, // kVK_ANSI_M
    N = 45, // kVK_ANSI_N
    O = 31, // kVK_ANSI_O
    P = 35, // kVK_ANSI_P
    Q = 12, // kVK_ANSI_Q
    R = 15, // kVK_ANSI_R
    S = 1,  // kVK_ANSI_S
    T = 17, // kVK_ANSI_T
    U = 32, // kVK_ANSI_U
    V = 9,  // kVK_ANSI_V
    W = 13, // kVK_ANSI_W
    X = 7,  // kVK_ANSI_X
    Y = 16, // kVK_ANSI_Y
    Z = 6,  // kVK_ANSI_Z
    // --- Numbers ---
    Number1 = 18, // kVK_ANSI_1
    Number2 = 19, // kVK_ANSI_2
    Number3 = 20, // kVK_ANSI_3
    Number4 = 21, // kVK_ANSI_4
    Number5 = 23, // kVK_ANSI_5
    Number6 = 22, // kVK_ANSI_6
    Number7 = 26, // kVK_ANSI_7
    Number8 = 28, // kVK_ANSI_8
    Number9 = 25, // kVK_ANSI_9
    Number0 = 29, // kVK_ANSI_0
}

impl Key {
    /// Convert to the native CGKeyCode.  Zero-cost — the discriminant IS the
    /// code.
    pub const fn as_native(self) -> u16 {
        self as u16
    }

    /// Return the modifier bit **position** (0–7) for this key.
    ///
    /// Modifier keys return `Some(position)` where position is the index in
    /// the 8-bit modifier mask.  Non-modifier keys return `None`.
    pub const fn as_modifier_bit(self) -> Option<u8> {
        match self {
            Self::LeftControl => Some(0),
            Self::RightControl => Some(1),
            Self::LeftShift => Some(2),
            Self::RightShift => Some(3),
            Self::LeftAlt => Some(4),
            Self::RightAlt => Some(5),
            Self::LeftCommand => Some(6),
            Self::RightCommand => Some(7),
            _ => None,
        }
    }

    /// Return the possible modifier bit positions for this key.
    ///
    /// All modifier keys return both left and right positions, enabling
    /// "either side" matching.  Non-modifier keys return `None`.
    pub fn as_modifier_positions(self) -> Option<Vec<u8>> {
        match self {
            Self::LeftControl | Self::RightControl => Some(vec![0, 1]),
            Self::LeftShift | Self::RightShift => Some(vec![2, 3]),
            Self::LeftAlt | Self::RightAlt => Some(vec![4, 5]),
            Self::LeftCommand | Self::RightCommand => Some(vec![6, 7]),
            _ => None,
        }
    }

    /// Return the canonical config-name for this key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LeftControl => "leftcontrol",
            Self::RightControl => "rightcontrol",
            Self::LeftShift => "leftshift",
            Self::RightShift => "rightshift",
            Self::LeftAlt => "leftalt",
            Self::RightAlt => "rightalt",
            Self::LeftCommand => "leftcommand",
            Self::RightCommand => "rightcommand",
            Self::CapsLock => "capslock",
            Self::Tab => "tab",
            Self::Space => "space",
            Self::Return => "return",
            Self::Backspace => "backspace",
            Self::Delete => "delete",
            Self::Escape => "escape",
            Self::UpArrow => "uparrow",
            Self::DownArrow => "downarrow",
            Self::LeftArrow => "leftarrow",
            Self::RightArrow => "rightarrow",
            Self::PageUp => "pageup",
            Self::PageDown => "pagedown",
            Self::Home => "home",
            Self::End => "end",
            Self::F1 => "f1",
            Self::F2 => "f2",
            Self::F3 => "f3",
            Self::F4 => "f4",
            Self::F5 => "f5",
            Self::F6 => "f6",
            Self::F7 => "f7",
            Self::F8 => "f8",
            Self::F9 => "f9",
            Self::F10 => "f10",
            Self::F11 => "f11",
            Self::F12 => "f12",
            Self::A => "a",
            Self::B => "b",
            Self::C => "c",
            Self::D => "d",
            Self::E => "e",
            Self::F => "f",
            Self::G => "g",
            Self::H => "h",
            Self::I => "i",
            Self::J => "j",
            Self::K => "k",
            Self::L => "l",
            Self::M => "m",
            Self::N => "n",
            Self::O => "o",
            Self::P => "p",
            Self::Q => "q",
            Self::R => "r",
            Self::S => "s",
            Self::T => "t",
            Self::U => "u",
            Self::V => "v",
            Self::W => "w",
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::Number1 => "1",
            Self::Number2 => "2",
            Self::Number3 => "3",
            Self::Number4 => "4",
            Self::Number5 => "5",
            Self::Number6 => "6",
            Self::Number7 => "7",
            Self::Number8 => "8",
            Self::Number9 => "9",
            Self::Number0 => "0",
        }
    }

    /// Parse a canonical name into a Key variant.
    ///
    /// Returns `None` for unrecognised names (caller should error).
    pub fn from_canonical(name: &str) -> Option<Self> {
        match name {
            "leftcontrol" => Some(Self::LeftControl),
            "rightcontrol" => Some(Self::RightControl),
            "leftshift" => Some(Self::LeftShift),
            "rightshift" => Some(Self::RightShift),
            "leftalt" => Some(Self::LeftAlt),
            "rightalt" => Some(Self::RightAlt),
            "leftcommand" => Some(Self::LeftCommand),
            "rightcommand" => Some(Self::RightCommand),
            "capslock" => Some(Self::CapsLock),
            "tab" => Some(Self::Tab),
            "space" => Some(Self::Space),
            "return" => Some(Self::Return),
            "backspace" => Some(Self::Backspace),
            "delete" => Some(Self::Delete),
            "escape" => Some(Self::Escape),
            "uparrow" => Some(Self::UpArrow),
            "downarrow" => Some(Self::DownArrow),
            "leftarrow" => Some(Self::LeftArrow),
            "rightarrow" => Some(Self::RightArrow),
            "pageup" => Some(Self::PageUp),
            "pagedown" => Some(Self::PageDown),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "f1" => Some(Self::F1),
            "f2" => Some(Self::F2),
            "f3" => Some(Self::F3),
            "f4" => Some(Self::F4),
            "f5" => Some(Self::F5),
            "f6" => Some(Self::F6),
            "f7" => Some(Self::F7),
            "f8" => Some(Self::F8),
            "f9" => Some(Self::F9),
            "f10" => Some(Self::F10),
            "f11" => Some(Self::F11),
            "f12" => Some(Self::F12),
            "a" => Some(Self::A),
            "b" => Some(Self::B),
            "c" => Some(Self::C),
            "d" => Some(Self::D),
            "e" => Some(Self::E),
            "f" => Some(Self::F),
            "g" => Some(Self::G),
            "h" => Some(Self::H),
            "i" => Some(Self::I),
            "j" => Some(Self::J),
            "k" => Some(Self::K),
            "l" => Some(Self::L),
            "m" => Some(Self::M),
            "n" => Some(Self::N),
            "o" => Some(Self::O),
            "p" => Some(Self::P),
            "q" => Some(Self::Q),
            "r" => Some(Self::R),
            "s" => Some(Self::S),
            "t" => Some(Self::T),
            "u" => Some(Self::U),
            "v" => Some(Self::V),
            "w" => Some(Self::W),
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            "1" | "number1" => Some(Self::Number1),
            "2" | "number2" => Some(Self::Number2),
            "3" | "number3" => Some(Self::Number3),
            "4" | "number4" => Some(Self::Number4),
            "5" | "number5" => Some(Self::Number5),
            "6" | "number6" => Some(Self::Number6),
            "7" | "number7" => Some(Self::Number7),
            "8" | "number8" => Some(Self::Number8),
            "9" | "number9" => Some(Self::Number9),
            "0" | "number0" => Some(Self::Number0),
            _ => None,
        }
    }
}

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let l = s.to_lowercase();
        // Try alias first, then canonical name.
        let canonical = key_names::resolve_alias(&l).unwrap_or(&l);
        Self::from_canonical(canonical).ok_or_else(|| {
            serde::de::Error::custom(key_names::unknown_key_error(&s))
        })
    }
}

// ---------------------------------------------------------------------------
// Modifier extraction — track specific key state for exact matching
// ---------------------------------------------------------------------------

/// Map a CGKeyCode to its modifier bit position.  Returns `None` for
/// non-modifier keys.
fn keycode_to_modifier_bit(code: CGKeyCode) -> Option<u8> {
    match code {
        59 => Some(0), // kVK_Control (left)
        62 => Some(1), // kVK_RightControl
        56 => Some(2), // kVK_Shift (left)
        60 => Some(3), // kVK_RightShift
        58 => Some(4), // kVK_Option (left)
        61 => Some(5), // kVK_RightOption
        55 => Some(6), // kVK_Command (left)
        54 => Some(7), // kVK_RightCommand
        _ => None,
    }
}

/// Map a modifier bit position back to the native CGKeyCode for emission.
fn modifier_bit_to_code(bit: u8) -> Option<CGKeyCode> {
    match bit {
        0 => Some(59), // kVK_Control (left)
        1 => Some(62), // kVK_RightControl
        2 => Some(56), // kVK_Shift (left)
        3 => Some(60), // kVK_RightShift
        4 => Some(58), // kVK_Option (left)
        5 => Some(61), // kVK_RightOption
        6 => Some(55), // kVK_Command (left)
        7 => Some(54), // kVK_RightCommand
        _ => None,
    }
}

/// Emit a single `NativeKey` as a chord: hold modifiers, press base,
/// release base, release modifiers in reverse order.
fn emit_key_event(source: &CFRetained<CGEventSource>, native_key: &NativeKey) {
    let mut pressed_modifiers: Vec<CGKeyCode> = Vec::new();

    // Press modifiers in ascending bit order.
    for bit in 0..8 {
        if (native_key.modifiers >> bit) & 1 == 1 {
            if let Some(code) = modifier_bit_to_code(bit) {
                if let Some(e) =
                    CGEvent::new_keyboard_event(Some(source), code, true)
                {
                    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
                }
                pressed_modifiers.push(code);
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    // Press base key.
    if let Some(e) = CGEvent::new_keyboard_event(
        Some(source),
        native_key.base as CGKeyCode,
        true,
    ) {
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
    }
    thread::sleep(Duration::from_millis(1));

    // Release base key.
    if let Some(e) = CGEvent::new_keyboard_event(
        Some(source),
        native_key.base as CGKeyCode,
        false,
    ) {
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
    }
    thread::sleep(Duration::from_millis(1));

    // Release modifiers in reverse order.
    for code in pressed_modifiers.into_iter().rev() {
        if let Some(e) = CGEvent::new_keyboard_event(Some(source), code, false)
        {
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
        }
        thread::sleep(Duration::from_millis(1));
    }
}

// ---------------------------------------------------------------------------
// Event tap implementation
// ---------------------------------------------------------------------------

/// Shared mutable state bridged into the C callback via `user_info`.
struct TapContext {
    /// Trait-object lookup: decouples this module from RuntimeState's shape.
    lookup: Arc<RwLock<dyn Lookup>>,
    /// Pre-created event source reused for every synthetic keyboard event.
    /// Avoids a per-keystroke allocation inside the hot callback path.
    source: CFRetained<CGEventSource>,
    /// Bitmask tracking which specific modifier keys are physically pressed.
    /// Updated on every modifier key down/up event.  This enables exact
    /// matching against compiled rules, which use specific bits (not groups).
    modifier_state: u8,
}

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Holds the tap, run-loop-source, and callback context so they stay alive
/// for the lifetime of the event-loop, and are cleanly reclaimed on drop.
struct EventTapHandle {
    tap: CFRetained<CFMachPort>,
    #[allow(dead_code)]
    run_loop_source: CFRetained<CFRunLoopSource>,
    /// Raw pointer to the heap-allocated `TapContext` passed as `user_info`.
    /// Reclaimed in `Drop` to avoid a memory leak.
    context_ptr: *mut TapContext,
}

impl Drop for EventTapHandle {
    fn drop(&mut self) {
        // Disable the tap so no further callbacks fire.
        CGEvent::tap_enable(&self.tap, false);

        // Reclaim and free the leaked `Box<TapContext>`. This also drops
        // the `CFRetained<CGEventSource>`, releasing the CoreFoundation
        // object.
        unsafe {
            drop(Box::from_raw(self.context_ptr));
        }
    }
}

/// Async-signal-safe handler that flips the shutdown flag.
extern "C" fn signal_handler(_sig: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Release);
}

pub(crate) fn start_mapping(
    lookup: Arc<RwLock<dyn Lookup>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mask: u64 =
        (1u64 << CGEventType::KeyDown.0) | (1u64 << CGEventType::KeyUp.0);

    let source =
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .ok_or("Failed to create CGEventSource")?;

    // Allocate the context on the heap and leak the `Box` to get a stable
    // pointer for the FFI callback.  The `EventTapHandle` owns this pointer
    // and reclaims it in `Drop`.
    let context_ptr = Box::into_raw(Box::new(TapContext {
        lookup,
        source,
        modifier_state: 0,
    })) as *mut _;

    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(macos_keyboard_callback_ffi),
            context_ptr as *mut c_void,
        )
    };

    let Some(tap) = tap else {
        // Tap creation failed; reclaim the context to avoid the leak.
        unsafe {
            drop(Box::from_raw(context_ptr));
        }
        return Err("Failed to create macOS CGEventTap. Verify \
                    Accessibility privileges!"
            .into());
    };

    let Some(run_loop_source) =
        CFMachPort::new_run_loop_source(None, Some(&tap), 0)
    else {
        unsafe {
            drop(Box::from_raw(context_ptr));
        }
        return Err("Failed to create CFRunLoopSource from Mach Port.".into());
    };

    let run_loop = CFRunLoop::current().ok_or("No current run loop")?;
    run_loop
        .add_source(Some(&run_loop_source), unsafe { kCFRunLoopCommonModes });

    CGEvent::tap_enable(&tap, true);
    println!("Modern compile-safe macOS Event Tap actively running...");

    // Register signal handlers for graceful shutdown.
    let handler_ptr = signal_handler as *const () as usize;
    unsafe {
        libc::signal(libc::SIGINT, handler_ptr);
        libc::signal(libc::SIGTERM, handler_ptr);
    }

    // Own the tap, run-loop-source, and context pointer. Dropped together
    // when the run-loop exits.
    let handle = EventTapHandle {
        tap,
        run_loop_source,
        context_ptr,
    };

    // Poll the run-loop with a short timeout so we can check the shutdown
    // flag each iteration. This avoids an infinite `CFRunLoop::run()` block
    // and lets us exit cleanly on SIGINT / SIGTERM.
    while !SHUTDOWN_REQUESTED.load(Ordering::Acquire) {
        CFRunLoop::run_in_mode(
            unsafe { kCFRunLoopCommonModes },
            0.5, // 500 ms timeout
            true,
        );
    }

    println!("Shutdown signal received. Cleaning up...");

    // `handle` is dropped here, which:
    // 1. Disables the tap
    // 2. Reclaims and frees the `TapContext` (and its `CGEventSource`)
    drop(handle);

    Ok(())
}

/// FFI callback invoked by the event tap for every matching keyboard event.
///
/// # Safety
/// Called from CoreGraphics on the run-loop thread.  `proxy` and `user_info`
/// are managed by the system / our `TapContext`.
unsafe extern "C-unwind" fn macos_keyboard_callback_ffi(
    _proxy: objc2_core_graphics::CGEventTapProxy,
    _type: CGEventType,
    event: core::ptr::NonNull<objc2_core_graphics::CGEvent>,
    user_info: *mut std::ffi::c_void,
) -> *mut objc2_core_graphics::CGEvent {
    if user_info.is_null() {
        return event.as_ptr();
    }

    let context = unsafe { &mut *(user_info as *mut TapContext) };

    // CGKeyCode is u16 — matches the native key code directly.
    let native_key: CGKeyCode = unsafe {
        CGEvent::integer_value_field(
            Some(event.as_ref()),
            CGEventField::KeyboardEventKeycode,
        )
    } as CGKeyCode;

    let is_down = _type == CGEventType::KeyDown;

    // Track specific modifier key state so we can do exact matching against
    // compiled rules (which use specific bits, not group flags).
    if let Some(bit) = keycode_to_modifier_bit(native_key) {
        if is_down {
            context.modifier_state |= 1 << bit;
        } else {
            context.modifier_state &= !(1 << bit);
        }
        // Modifier-only events are passed through — don't remap them.
        return event.as_ptr();
    }

    // Use the tracked modifier state for lookup — this reflects exactly which
    // physical modifier keys are pressed.
    let pressed_modifiers = context.modifier_state;

    // Resolve the remapping through the trait interface.  Clone the outputs
    // so we can drop the read lock before expensive CGEvent operations.
    let guard = context.lookup.read();
    let current_app = guard.active_app().to_lowercase();
    let active_outputs = guard
        .for_app(&current_app, native_key, pressed_modifiers)
        .or_else(|| guard.global(native_key, pressed_modifiers))
        .map(|v| v.to_vec());
    drop(guard);

    if let Some(outputs) = active_outputs {
        if is_down {
            // Emit all output key events as chords.
            for native_key in &outputs {
                emit_key_event(&context.source, native_key);
            }
        }
        // Suppress the original event.
        return std::ptr::null_mut();
    }

    event.as_ptr()
}
