// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use std::{fs, path::Path};

use indexmap::IndexMap;

use crate::config::{AppConfig, ChordTrigger, Key};

/// A compiled chord: specific modifier requirement + base key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NativeChord {
    /// Bitmask of required modifier groups.  A rule matches when
    /// `(pressed_modifiers & self.modifiers) == self.modifiers`.
    pub modifiers: u8,
    /// The non-modifier base key's native code.
    pub base: u16,
}

/// Platform-native structural action layout.
#[derive(Debug, Clone)]
pub enum NativeAction {
    RemapTo(u16),
    Shortcut(Vec<u16>),
}

/// Compiled key-mapping cache optimised for fast runtime lookups.
///
/// Single-key rules are stored in `IndexMap`s for O(1) lookup while
/// preserving insertion order.  Chord rules are stored as small `Vec`s
/// and scanned linearly -- the rule count is small enough that O(n)
/// is negligible compared to FFI overhead.
pub struct RuntimeLookupCache {
    // Single-key rules (bare key, no modifiers required).
    process_map: IndexMap<String, IndexMap<u16, NativeAction>>,
    global_map: IndexMap<u16, NativeAction>,
    // Chord rules (modifiers + base key).
    process_chords: IndexMap<String, Vec<(NativeChord, NativeAction)>>,
    global_chords: Vec<(NativeChord, NativeAction)>,
}

impl RuntimeLookupCache {
    pub(crate) fn process_map(
        &self,
    ) -> &IndexMap<String, IndexMap<u16, NativeAction>> {
        &self.process_map
    }

    pub(crate) fn global_map(&self) -> &IndexMap<u16, NativeAction> {
        &self.global_map
    }

    pub(crate) fn process_chords(
        &self,
    ) -> &IndexMap<String, Vec<(NativeChord, NativeAction)>> {
        &self.process_chords
    }

    pub(crate) fn global_chords(&self) -> &Vec<(NativeChord, NativeAction)> {
        &self.global_chords
    }
}

impl RuntimeLookupCache {
    /// Load a YAML config file, parse it, and compile the lookup cache
    /// in one step.  Used by both initialisation and hot-reload.
    pub fn compile_from_path<P: AsRef<Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let parsed = AppConfig::load_from_str(&content)?;
        Ok(Self::compile_from_config(&parsed))
    }

    pub fn compile_from_config(app_config: &AppConfig) -> Self {
        let mut process_map: IndexMap<String, IndexMap<u16, NativeAction>> =
            IndexMap::new();
        let mut global_map: IndexMap<u16, NativeAction> = IndexMap::new();
        let mut process_chords: IndexMap<
            String,
            Vec<(NativeChord, NativeAction)>,
        > = IndexMap::new();
        let mut global_chords: Vec<(NativeChord, NativeAction)> = Vec::new();

        // Iterate groups in definition order.  First-match-wins is
        // guaranteed by the IndexMap/Vec preserving insertion order.
        for group in &app_config.groups {
            // Skip empty groups (no mappings defined).
            if group.mappings.is_empty() {
                continue;
            }

            let apps: Vec<String> = if group.apps.is_empty() {
                Vec::new()
            } else {
                group.apps.iter().map(|a| a.to_lowercase()).collect()
            };

            for (trigger, outputs) in group.mappings.iter() {
                let native_action = compile_action(outputs);
                let (chord, is_single_key) = compile_trigger(trigger);

                if is_single_key {
                    // Single-key rule -- goes into the IndexMap.
                    let native = chord.base;
                    if apps.is_empty() {
                        global_map.insert(native, native_action);
                    } else {
                        for app in &apps {
                            process_map
                                .entry(app.clone())
                                .or_default()
                                .insert(native, native_action.clone());
                        }
                    }
                } else {
                    // Chord rule -- goes into the linear scan list.
                    let entry = (chord, native_action);

                    if apps.is_empty() {
                        global_chords.push(entry);
                    } else {
                        for app in &apps {
                            process_chords
                                .entry(app.clone())
                                .or_default()
                                .push(entry.clone());
                        }
                    }
                }
            }
        }

        RuntimeLookupCache {
            process_map,
            global_map,
            process_chords,
            global_chords,
        }
    }
}

/// Compile a list of output chords into a native action.
fn compile_action(outputs: &[ChordTrigger]) -> NativeAction {
    if outputs.len() == 1 {
        // Single output -- remap to that key.
        let target = chord_to_native_key(&outputs[0]);
        NativeAction::RemapTo(target)
    } else {
        // Multiple outputs -- simulate as a shortcut sequence.
        let codes: Vec<u16> =
            outputs.iter().map(chord_to_native_key).collect();
        NativeAction::Shortcut(codes)
    }
}

/// Extract the native key code from a chord trigger.  For chords, this
/// extracts the base key's native code; modifiers are only meaningful
/// for input triggers, not outputs.
fn chord_to_native_key(chord: &ChordTrigger) -> u16 {
    match chord {
        ChordTrigger::Key(key) => key.as_native(),
        ChordTrigger::Chord { base, .. } => base.as_native(),
    }
}

/// Compile a trigger chord into its native representation.
/// Returns the compiled `NativeChord` and whether it's a single-key trigger
/// (no modifiers).
fn compile_trigger(trigger: &ChordTrigger) -> (NativeChord, bool) {
    match trigger {
        ChordTrigger::Key(key) => (
            NativeChord {
                modifiers: 0,
                base: key.as_native(),
            },
            true,
        ),
        ChordTrigger::Chord { base, modifiers } => {
            let chord_mods = compile_modifier_bits(modifiers.iter().copied());
            (
                NativeChord {
                    modifiers: chord_mods,
                    base: base.as_native(),
                },
                false,
            )
        }
    }
}

/// Compile a list of modifier keys into a combined group bitmask.
///
/// Each key contributes its _group_ bits so that specifying "ctrl" (which
/// resolves to LeftControl via the alias layer) still matches right-control.
fn compile_modifier_bits<'a>(keys: impl Iterator<Item = Key> + 'a) -> u8 {
    let mut bits: u8 = 0;
    for key in keys {
        bits |= modifier_group_bits(key);
    }
    bits
}

/// Return the group mask for a modifier key.
///
/// Left/right pairs share the same mask so that specifying either side
/// matches both.  Non-modifier keys contribute nothing.
fn modifier_group_bits(key: Key) -> u8 {
    match key {
        // Control group: bits 0 | 1
        Key::LeftControl | Key::RightControl => (1 << 0) | (1 << 1),
        // Shift group: bits 2 | 3
        Key::LeftShift | Key::RightShift => (1 << 2) | (1 << 3),
        // Alt/Option group: bits 4 | 5
        Key::LeftAlt | Key::RightAlt => (1 << 4) | (1 << 5),
        // Command/Meta/Win group: bits 6 | 7
        Key::LeftCommand | Key::RightCommand => (1 << 6) | (1 << 7),
        // Non-modifier keys contribute nothing.
        _ => 0,
    }
}
