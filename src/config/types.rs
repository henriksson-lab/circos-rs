use std::collections::HashMap;

/// A configuration value - can be a string, a map of key-value pairs, or a list of values.
/// This mirrors the Config::General behavior where duplicate keys become lists
/// and `<block>` sections become nested maps.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Str(String),
    Map(HashMap<String, ConfigValue>),
    List(Vec<ConfigValue>),
}

impl ConfigValue {
    /// Get as string, returning None if not a Str variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Get as map, returning None if not a Map variant.
    pub fn as_map(&self) -> Option<&HashMap<String, ConfigValue>> {
        match self {
            ConfigValue::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Get as list, returning None if not a List variant.
    pub fn as_list(&self) -> Option<&Vec<ConfigValue>> {
        match self {
            ConfigValue::List(l) => Some(l),
            _ => None,
        }
    }

    /// Get a nested value by key path (dot-separated or individual key).
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        match self {
            ConfigValue::Map(m) => m.get(key),
            _ => None,
        }
    }

    /// Get a string value from a nested map, with a default.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    /// Get an f64 value from a nested map.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get_str(key).and_then(|s| s.parse().ok())
    }

    /// Get an i64 value from a nested map.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get_str(key).and_then(|s| s.parse().ok())
    }

    /// Get a boolean value (Config::General AutoTrue: yes/true/on/1 -> true).
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get_str(key).map(|s| matches!(s, "1" | "yes" | "true" | "on"))
    }

    /// Treat as a list: if it's a List return the items; if it's a single value
    /// return a one-element vec; if it's missing return empty.
    pub fn as_list_or_single(&self) -> Vec<&ConfigValue> {
        match self {
            ConfigValue::List(l) => l.iter().collect(),
            other => vec![other],
        }
    }
}

impl Default for ConfigValue {
    fn default() -> Self {
        ConfigValue::Map(HashMap::new())
    }
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        ConfigValue::Str(s.to_string())
    }
}

impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        ConfigValue::Str(s)
    }
}

impl From<HashMap<String, ConfigValue>> for ConfigValue {
    fn from(m: HashMap<String, ConfigValue>) -> Self {
        ConfigValue::Map(m)
    }
}
