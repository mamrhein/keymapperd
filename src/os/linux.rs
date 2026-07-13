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

use evdev::{Device, EventType, Key as EvdevKey};
use parking_lot::RwLock;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{key_names, mapping_cache::NativeKey, state::Lookup};

// ---------------------------------------------------------------------------
// Platform-specific Key enum — discriminants ARE the evdev KEY_* codes
// ---------------------------------------------------------------------------

/// Linux evdev keycode for a physical key on a US ANSI keyboard.
///
/// Discriminant values come from `<linux/input-event-codes.h>` (`KEY_*` constants).
/// `key as u16` yields the native evdev code — no translation needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Key {
    // --- Modifiers ---
    LeftControl = 29,   // KEY_LEFTCTRL
    RightControl = 97,  // KEY_RIGHTCTRL
    LeftShift = 42,     // KEY_LEFTSHIFT
    RightShift = 54,    // KEY_RIGHTSHIFT
    LeftAlt = 56,       // KEY_LEFTALT
    RightAlt = 100,     // KEY_RIGHTALT
    LeftCommand = 125,  // KEY_LEFTMETA
    RightCommand = 126, // KEY_RIGHTMETA
    CapsLock = 58,      // KEY_CAPSLOCK
    // --- Editor / misc ---
    Tab = 15,       // KEY_TAB
    Space = 57,     // KEY_SPACE
    Return = 28,    // KEY_ENTER
    Backspace = 14, // KEY_BACKSPACE
    Delete = 111,   // KEY_DELETE
    Escape = 1,     // KEY_ESC
    // --- Navigation ---
    UpArrow = 103,    // KEY_UP
    DownArrow = 108,  // KEY_DOWN
    LeftArrow = 105,  // KEY_LEFT
    RightArrow = 106, // KEY_RIGHT
    PageUp = 104,     // KEY_PAGEUP
    PageDown = 109,   // KEY_PAGEDOWN
    Home = 102,       // KEY_HOME
    End = 107,        // KEY_END
    // --- Function keys ---
    F1 = 59,  // KEY_F1
    F2 = 60,  // KEY_F2
    F3 = 61,  // KEY_F3
    F4 = 62,  // KEY_F4
    F5 = 63,  // KEY_F5
    F6 = 64,  // KEY_F6
    F7 = 65,  // KEY_F7
    F8 = 66,  // KEY_F8
    F9 = 67,  // KEY_F9
    F10 = 68, // KEY_F10
    F11 = 87, // KEY_F11
    F12 = 88, // KEY_F12
    // --- Letters ---
    A = 30, // KEY_A
    B = 48, // KEY_B
    C = 46, // KEY_C
    D = 32, // KEY_D
    E = 18, // KEY_E
    F = 33, // KEY_F
    G = 34, // KEY_G
    H = 35, // KEY_H
    I = 23, // KEY_I
    J = 36, // KEY_J
    K = 37, // KEY_K
    L = 38, // KEY_L
    M = 50, // KEY_M
    N = 49, // KEY_N
    O = 24, // KEY_O
    P = 25, // KEY_P
    Q = 16, // KEY_Q
    R = 19, // KEY_R
    S = 31, // KEY_S
    T = 20, // KEY_T
    U = 22, // KEY_U
    V = 47, // KEY_V
    W = 17, // KEY_W
    X = 45, // KEY_X
    Y = 21, // KEY_Y
    Z = 44, // KEY_Z
    // --- Numbers ---
    Number1 = 2,  // KEY_1
    Number2 = 3,  // KEY_2
    Number3 = 4,  // KEY_3
    Number4 = 5,  // KEY_4
    Number5 = 6,  // KEY_5
    Number6 = 7,  // KEY_6
    Number7 = 8,  // KEY_7
    Number8 = 9,  // KEY_8
    Number9 = 10, // KEY_9
    Number0 = 11, // KEY_0
}

impl Key {
    /// Convert to the native evdev code.  Zero-cost — the discriminant IS the code.
    pub const fn as_native(self) -> u16 {
        self as u16
    }

    /// Return the modifier bit **position** (0–7) for this key.
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
        let canonical = key_names::resolve_alias(&l).unwrap_or(&l);
        Self::from_canonical(canonical).ok_or_else(|| {
            serde::de::Error::custom(key_names::unknown_key_error(&s))
        })
    }
}

// ---------------------------------------------------------------------------
// Modifier handling — specific key bits (evdev can distinguish left/right)
// ---------------------------------------------------------------------------

/// Map a raw evdev keycode to its modifier bit position.  Returns `None`
/// for non-modifier keys.
fn keycode_to_modifier_bit(code: u16) -> Option<u8> {
    match code {
        29 => Some(0),   // KEY_LEFTCTRL
        97 => Some(1),   // KEY_RIGHTCTRL
        42 => Some(2),   // KEY_LEFTSHIFT
        54 => Some(3),   // KEY_RIGHTSHIFT
        56 => Some(4),   // KEY_LEFTALT
        100 => Some(5),  // KEY_RIGHTALT
        125 => Some(6),  // KEY_LEFTMETA
        126 => Some(7),  // KEY_RIGHTMETA
        _ => None,
    }
}

/// Map a modifier bit position back to the native evdev code for emission.
fn modifier_bit_to_code(bit: u8) -> Option<u16> {
    match bit {
        0 => Some(29),   // KEY_LEFTCTRL
        1 => Some(97),   // KEY_RIGHTCTRL
        2 => Some(42),   // KEY_LEFTSHIFT
        3 => Some(54),   // KEY_RIGHTSHIFT
        4 => Some(56),   // KEY_LEFTALT
        5 => Some(100),  // KEY_RIGHTALT
        6 => Some(125),  // KEY_LEFTMETA
        7 => Some(126),  // KEY_RIGHTMETA
        _ => None,
    }
}

/// Emit a single `NativeKey` as a chord: hold modifiers, press base,
/// release base, release modifiers in reverse order.
fn emit_key_event(
    device: &mut uinput::VirtualDevice,
    native_key: &NativeKey,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut pressed_modifiers: Vec<u16> = Vec::new();

    // Press modifiers in ascending bit order.
    for bit in 0..8 {
        if (native_key.modifiers >> bit) & 1 == 1 {
            if let Some(code) = modifier_bit_to_code(bit) {
                device.press(&EvdevKey::new(code))?;
                pressed_modifiers.push(code);
                device.synchronize()?;
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    // Press base key.
    device.press(&EvdevKey::new(native_key.base))?;
    device.synchronize()?;
    thread::sleep(Duration::from_millis(1));

    // Release base key.
    device.release(&EvdevKey::new(native_key.base))?;
    device.synchronize()?;
    thread::sleep(Duration::from_millis(1));

    // Release modifiers in reverse order.
    for code in pressed_modifiers.into_iter().rev() {
        device.release(&EvdevKey::new(code))?;
        device.synchronize()?;
        thread::sleep(Duration::from_millis(1));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// evdev event loop
// ---------------------------------------------------------------------------

/// Async-signal-safe handler that flips the shutdown flag.
extern "C" fn signal_handler(_sig: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Release);
}

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Discover the first suitable keyboard device on Linux.
///
/// Scans `/dev/input/event*` devices and returns the first one that
/// supports keyboard events (EV_KEY).  If no suitable device is found,
/// returns an error.
fn find_keyboard_device() -> Result<Device, Box<dyn std::error::Error>> {
    use std::fs;
    use std::path::Path;

    let input_path = Path::new("/dev/input");
    if !input_path.exists() {
        return Err("No /dev/input directory found. \
                    Do you have permission to access input devices?"
            .into());
    }

    let mut devices: Vec<Device> = Vec::new();

    for entry in fs::read_dir(input_path)? {
        let entry = entry?;
        let path = entry.path();

        if !path.to_string_lossy().starts_with("/dev/input/event") {
            continue;
        }

        if let Ok(device) = Device::open(&path) {
            devices.push(device);
        }
    }

    if devices.is_empty() {
        return Err("No keyboard device found. \
                    Try adding your user to the 'input' group: \
                    sudo usermod -aG input $USER"
            .into());
    }

    // Return the first device that supports EV_KEY.
    for device in devices {
        if device.supported_events().contains(EventType::KEY) {
            return Ok(device);
        }
    }

    Err("No keyboard device found that supports EV_KEY".into())
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

    // Track modifier state so chord rules can match against active modifiers.
    // Uses specific bits (not groups) — evdev can distinguish left/right.
    let mut active_modifiers: u8 = 0;

    while !SHUTDOWN_REQUESTED.load(Ordering::Acquire) {
        match raw_device.fetch_events() {
            Ok(events) => {
                for event in events {
                    if event.event_type() == EventType::KEY {
                        let code = event.code();
                        let value = event.value(); // 1 = Down, 0 = Up, 2 = Repeat

                        let maybe_mod_bit = keycode_to_modifier_bit(code);

                        // Update modifier tracking.
                        if let Some(bit) = maybe_mod_bit {
                            if value == 1 {
                                active_modifiers |= 1 << bit;
                            } else if value == 0 {
                                active_modifiers &= !(1 << bit);
                            }
                        }

                        let guard = lookup.read();
                        let current_app = guard.active_app().to_lowercase();
                        let active_outputs = guard
                            .for_app(&current_app, code, active_modifiers)
                            .or_else(|| guard.global(code, active_modifiers))
                            .map(|v| v.to_vec());
                        drop(guard);

                        if let Some(outputs) = active_outputs {
                            if value == 1 {
                                // Key down: emit all output key events as chords.
                                for native_key in &outputs {
                                    if let Err(e) =
                                        emit_key_event(&mut virtual_device, native_key)
                                    {
                                        eprintln!("emit error: {}", e);
                                    }
                                }
                            }
                            // Suppress the original event for remapped keys.
                            continue;
                        }

                        // Passthrough: forward the event through uinput.
                        let key = EvdevKey::new(code);
                        if value == 1 {
                            virtual_device.press(&key)?;
                        } else if value == 0 {
                            virtual_device.release(&key)?;
                        } else {
                            // value == 2 (repeat): fire press+release to
                            // preserve autorepeat through the virtual device.
                            virtual_device.press(&key)?;
                            virtual_device.release(&key)?;
                        }
                        virtual_device.synchronize()?;
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
