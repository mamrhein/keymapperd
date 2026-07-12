// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

// Re-export the platform-specific Key type so that downstream modules
// (mapping_cache, state, hot_reload) import it from this module.
pub(crate) use crate::os::Key;

/// A key press, optionally combined with modifier keys.
///
/// Accepts compact `+`-separated strings in YAML:
/// - `"CapsLock"` -- single key, no modifiers
/// - `"ctrl+a"` -- chord: ctrl held while pressing a
/// - `"cmd+shift+t"` -- chord: cmd+shift held while pressing t
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChordTrigger {
    /// Single key with no modifier requirement (e.g. CapsLock alone).
    Key(Key),
    /// Base key combined with specific modifiers (e.g. Ctrl+A).
    Chord { base: Key, modifiers: Vec<Key> },
}

impl<'de> Deserialize<'de> for ChordTrigger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(de::Error::custom)
    }
}

impl Serialize for ChordTrigger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Key(key) => serializer.serialize_str(key.as_str()),
            Self::Chord { base, modifiers } => {
                let parts: Vec<String> = modifiers
                    .iter()
                    .map(|k| k.as_str().to_string())
                    .chain(std::iter::once(base.as_str().to_string()))
                    .collect();
                serializer.serialize_str(&parts.join("+"))
            }
        }
    }
}

impl ChordTrigger {
    /// Parse a `+`-separated string into a trigger.
    fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').collect();

        if parts.is_empty() || (parts.len() == 1 && parts[0].trim().is_empty())
        {
            return Err("empty trigger string".to_string());
        }

        if parts.len() == 1 {
            // Single key: "CapsLock", "a", "f1"
            let key = parse_key(parts[0])?;
            Ok(Self::Key(key))
        } else {
            // Chord: "ctrl+a", "cmd+shift+t"
            // Last token is the base key; all preceding tokens are modifiers.
            let base = parse_key(parts[parts.len() - 1])?;
            let modifiers: Result<Vec<Key>, _> = parts[..parts.len() - 1]
                .iter()
                .map(|p| parse_key(p))
                .collect();
            Ok(Self::Chord {
                base,
                modifiers: modifiers?,
            })
        }
    }
}

/// Parse a single token from the config string into a `Key`.
fn parse_key(token: &str) -> Result<Key, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("empty key token in trigger".to_string());
    }

    // Strip underscores so that "left_control" and "leftcontrol" match.
    let lower = trimmed.replace('_', "").to_lowercase();
    let canonical = crate::key_names::resolve_alias(&lower).unwrap_or(&lower);

    Key::from_canonical(canonical)
        .ok_or_else(|| crate::key_names::unknown_key_error(trimmed))
}

// ---------------------------------------------------------------------------
// MappingTable -- ordered chord -> chords mapping
// ---------------------------------------------------------------------------

/// An ordered collection of mappings from a trigger chord to output chords.
///
/// Stored as an `IndexMap` to guarantee first-match-wins semantics when the
/// cache is compiled.  Deserialized from a YAML mapping where keys are chord
/// strings and values are either a single chord string or a list of chord
/// strings.
#[derive(Debug, Clone)]
pub struct MappingTable(IndexMap<ChordTrigger, Vec<ChordTrigger>>);

impl Default for MappingTable {
    fn default() -> Self {
        Self(IndexMap::new())
    }
}

impl MappingTable {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over (trigger, outputs) pairs in definition order.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&ChordTrigger, &[ChordTrigger])> {
        self.0.iter().map(|(k, v)| (k, v.as_slice()))
    }
}

/// Custom visitor that deserializes a YAML mapping into an ordered
/// `MappingTable`, preserving document order.
struct MappingTableVisitor;

impl<'de> de::Visitor<'de> for MappingTableVisitor {
    type Value = MappingTable;

    fn expecting(
        &self,
        formatter: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        formatter.write_str("a mapping from chord strings to chord(s)")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        let mut map = IndexMap::new();

        while let Some((key_str, value)) =
            access.next_entry::<String, serde_yaml::Value>()?
        {
            let trigger =
                ChordTrigger::parse(&key_str).map_err(de::Error::custom)?;

            let outputs = match value {
                serde_yaml::Value::String(s) => {
                    let chord =
                        ChordTrigger::parse(&s).map_err(de::Error::custom)?;
                    vec![chord]
                }
                serde_yaml::Value::Sequence(seq) => seq
                    .into_iter()
                    .map(|v| match v {
                        serde_yaml::Value::String(s) => {
                            ChordTrigger::parse(&s).map_err(de::Error::custom)
                        }
                        _ => Err(de::Error::custom(
                            "expected a chord string in output sequence",
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(de::Error::custom(
                        "expected a chord string or list of chord strings",
                    ));
                }
            };

            map.insert(trigger, outputs);
        }

        Ok(MappingTable(map))
    }
}

impl<'de> Deserialize<'de> for MappingTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(MappingTableVisitor)
    }
}

impl Serialize for MappingTable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (trigger, outputs) in &self.0 {
            let key = match trigger {
                ChordTrigger::Key(k) => k.as_str().to_string(),
                ChordTrigger::Chord { base, modifiers } => {
                    let parts: Vec<String> = modifiers
                        .iter()
                        .map(|k| k.as_str().to_string())
                        .chain(std::iter::once(base.as_str().to_string()))
                        .collect();
                    parts.join("+")
                }
            };

            if outputs.len() == 1 {
                map.serialize_entry(&key, &outputs[0])?;
            } else {
                map.serialize_entry(&key, outputs.as_slice())?;
            }
        }
        map.end()
    }
}

// ---------------------------------------------------------------------------
// RuleGroup -- app-scoped collection of mappings
// ---------------------------------------------------------------------------

/// A named group of key mappings, optionally scoped to specific applications.
///
/// When `apps` is empty the group applies globally (all applications).
/// Groups are evaluated in definition order; the first group whose app
/// scope matches is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleGroup {
    /// Human-readable name for documentation/debugging.  Not required.
    #[serde(default)]
    pub name: Option<String>,

    /// Target applications (process names or bundle IDs).  An empty list
    /// means the group applies globally.
    #[serde(default)]
    pub apps: Vec<String>,

    /// Ordered chord-to-chords mappings.  The first matching rule wins.
    #[serde(default)]
    pub mappings: MappingTable,
}

// ---------------------------------------------------------------------------
// AppConfig -- root configuration document
// ---------------------------------------------------------------------------

/// The root configuration layout representing the YAML file structure.
///
/// The document is a sequence of rule groups.  Groups are evaluated in
/// definition order; within each group, mappings are evaluated in
/// definition order.  The first matching rule wins.
#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub groups: Vec<RuleGroup>,
}

/// Deserializes AppConfig from either:
/// - A bare YAML sequence of rule groups (the primary format)
/// - A YAML mapping with a "groups" key (for programmatic use)
struct AppConfigVisitor;

impl<'de> de::Visitor<'de> for AppConfigVisitor {
    type Value = AppConfig;

    fn expecting(
        &self,
        formatter: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        formatter.write_str(
            "a sequence of rule groups or a mapping with a 'groups' key",
        )
    }

    fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut groups = Vec::new();
        while let Some(group) = access.next_element::<RuleGroup>()? {
            groups.push(group);
        }
        Ok(AppConfig { groups })
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        let mut groups = Vec::<RuleGroup>::new();
        while let Some(key) = access.next_key::<String>()? {
            if key == "groups" {
                groups = access.next_value()?;
            } else {
                return Err(de::Error::unknown_field(&key, &["groups"]));
            }
        }
        Ok(AppConfig { groups })
    }
}

impl<'de> Deserialize<'de> for AppConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(AppConfigVisitor)
    }
}

impl Serialize for AppConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.groups.serialize(serializer)
    }
}

impl AppConfig {
    pub fn load_from_str(yaml_str: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml_str)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config() {
        let config = AppConfig::load_from_str("groups: []").unwrap();
        assert!(config.groups.is_empty());
    }

    #[test]
    fn parse_global_group() {
        let yaml = r#"
- mappings:
    capslock: left_control
"#;
        let config = AppConfig::load_from_str(yaml).unwrap();
        assert_eq!(config.groups.len(), 1);

        let group = &config.groups[0];
        assert!(group.apps.is_empty()); // global

        let mut mappings = group.mappings.iter();
        let (trigger, outputs) = mappings.next().unwrap();
        assert!(matches!(trigger, ChordTrigger::Key(Key::CapsLock)));
        assert_eq!(outputs.len(), 1);
        assert!(matches!(outputs[0], ChordTrigger::Key(Key::LeftControl)));
        assert!(mappings.next().is_none());
    }

    #[test]
    fn parse_app_scoped_group() {
        let yaml = r#"
- name: "iterm nav"
  apps: [iTerm2]
  mappings:
    ctrl+h: left
    ctrl+l: right
"#;
        let config = AppConfig::load_from_str(yaml).unwrap();
        assert_eq!(config.groups.len(), 1);

        let group = &config.groups[0];
        assert_eq!(group.name.as_deref(), Some("iterm nav"));
        assert_eq!(group.apps, vec!["iTerm2".to_string()]);

        let mut mappings = group.mappings.iter();

        // ctrl+h -> left
        let (trigger, outputs) = mappings.next().unwrap();
        match trigger {
            ChordTrigger::Chord { base, modifiers } => {
                assert!(matches!(base, Key::H));
                assert_eq!(modifiers.len(), 1);
                assert!(matches!(modifiers[0], Key::LeftControl));
            }
            _ => panic!("expected chord trigger"),
        }
        assert_eq!(outputs.len(), 1);
        assert!(matches!(outputs[0], ChordTrigger::Key(Key::LeftArrow)));

        // ctrl+l -> right
        let (trigger, outputs) = mappings.next().unwrap();
        match trigger {
            ChordTrigger::Chord { base, modifiers } => {
                assert!(matches!(base, Key::L));
                assert_eq!(modifiers.len(), 1);
            }
            _ => panic!("expected chord trigger"),
        }
        assert!(matches!(outputs[0], ChordTrigger::Key(Key::RightArrow)));
    }

    #[test]
    fn parse_multi_output() {
        let yaml = r#"
- mappings:
    capslock: [left_control, capslock]
"#;
        let config = AppConfig::load_from_str(yaml).unwrap();
        let group = &config.groups[0];

        let mut mappings = group.mappings.iter();
        let (_trigger, outputs) = mappings.next().unwrap();
        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn parse_multiple_groups() {
        let yaml = r#"
- mappings:
    capslock: left_control

- name: "iterm nav"
  apps: [iTerm2]
  mappings:
    ctrl+h: left
"#;
        let config = AppConfig::load_from_str(yaml).unwrap();
        assert_eq!(config.groups.len(), 2);
        assert!(config.groups[0].apps.is_empty());
        assert_eq!(config.groups[1].apps.len(), 1);
    }

    #[test]
    fn parse_group_without_mappings() {
        let yaml = r#"
- name: "placeholder"
"#;
        let config = AppConfig::load_from_str(yaml).unwrap();
        assert_eq!(config.groups.len(), 1);
        assert!(config.groups[0].mappings.is_empty());
    }

    #[test]
    fn parse_complex_config() {
        // The real example config structure.
        let yaml = r#"
- mappings:
    capslock: left_control
    left_control: [left_control, capslock]

- name: "iterm nav"
  apps: [iTerm2]
  mappings:
    ctrl+h: left
    ctrl+j: down
    ctrl+k: up
    ctrl+l: right

- name: "global shortcuts"
  mappings:
    ctrl+shift+left: [cmd, left]
    ctrl+shift+right: [cmd, right]
"#;
        let config = AppConfig::load_from_str(yaml).unwrap();
        assert_eq!(config.groups.len(), 3);

        // Global group: capslock swap
        assert!(config.groups[0].apps.is_empty());
        assert_eq!(config.groups[0].mappings.iter().count(), 2);

        // iTerm group
        assert_eq!(config.groups[1].apps, vec!["iTerm2".to_string()]);
        assert_eq!(config.groups[1].mappings.iter().count(), 4);

        // Global shortcuts
        assert!(config.groups[2].apps.is_empty());
        assert_eq!(config.groups[2].mappings.iter().count(), 2);
    }
}
