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
        self.get_str(key)
            .map(|s| matches!(s, "1" | "yes" | "true" | "on"))
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
    /// Default `ConfigValue` is an empty `Map` (matches an empty top-level config).
    fn default() -> Self {
        ConfigValue::Map(HashMap::new())
    }
}

impl From<&str> for ConfigValue {
    /// Wrap a string slice into a `ConfigValue::Str`.
    fn from(s: &str) -> Self {
        ConfigValue::Str(s.to_string())
    }
}

impl From<String> for ConfigValue {
    /// Wrap an owned `String` into a `ConfigValue::Str`.
    fn from(s: String) -> Self {
        ConfigValue::Str(s)
    }
}

impl From<HashMap<String, ConfigValue>> for ConfigValue {
    /// Wrap a `HashMap` into a `ConfigValue::Map`.
    fn from(m: HashMap<String, ConfigValue>) -> Self {
        ConfigValue::Map(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(s: &str) -> ConfigValue {
        ConfigValue::Str(s.to_string())
    }

    #[test]
    fn test_as_accessors_return_none_for_wrong_variant() {
        let s = leaf("hello");
        assert_eq!(s.as_str(), Some("hello"));
        assert!(s.as_map().is_none());
        assert!(s.as_list().is_none());
        let m = ConfigValue::Map(HashMap::new());
        assert!(m.as_str().is_none());
        assert!(m.as_map().is_some());
        let l = ConfigValue::List(vec![]);
        assert!(l.as_list().is_some());
    }

    #[test]
    fn test_get_on_non_map_returns_none() {
        let s = leaf("plain");
        // Calling `get` on a leaf never succeeds.
        assert!(s.get("anything").is_none());
        let l = ConfigValue::List(vec![leaf("a")]);
        assert!(l.get("anything").is_none());
    }

    #[test]
    fn test_get_str_f64_i64_bool_parsing() {
        let mut m = HashMap::new();
        m.insert("s".into(), leaf("hello"));
        m.insert("f".into(), leaf("3.14"));
        m.insert("i".into(), leaf("42"));
        m.insert("yes".into(), leaf("yes"));
        m.insert("no".into(), leaf("no"));
        m.insert("one".into(), leaf("1"));
        let v = ConfigValue::Map(m);

        assert_eq!(v.get_str("s"), Some("hello"));
        assert!((v.get_f64("f").unwrap() - 3.14).abs() < 1e-12);
        assert_eq!(v.get_i64("i"), Some(42));
        // Bool: yes/true/on/1 → true, anything else → false.
        assert_eq!(v.get_bool("yes"), Some(true));
        assert_eq!(v.get_bool("one"), Some(true));
        assert_eq!(v.get_bool("no"), Some(false));
        // Missing key → None for all accessors.
        assert!(v.get_str("missing").is_none());
        assert!(v.get_f64("missing").is_none());
        assert!(v.get_i64("missing").is_none());
        assert!(v.get_bool("missing").is_none());
        // f64/i64 parse-failure → None (not Some(0)).
        assert!(v.get_f64("s").is_none());
        assert!(v.get_i64("s").is_none());
    }

    #[test]
    fn test_as_list_or_single_wraps_scalar() {
        let s = leaf("x");
        let list = s.as_list_or_single();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].as_str(), Some("x"));

        let real_list = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let list = real_list.as_list_or_single();
        assert_eq!(list.len(), 3);
        assert_eq!(list[1].as_str(), Some("b"));

        // Map is returned as a single-element list (matches Perl make_list semantics).
        let mut m = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let map = ConfigValue::Map(m);
        let list = map.as_list_or_single();
        assert_eq!(list.len(), 1);
        assert!(list[0].as_map().is_some());
    }

    #[test]
    fn test_default_is_empty_map() {
        let d = ConfigValue::default();
        assert!(matches!(d, ConfigValue::Map(ref m) if m.is_empty()));
    }

    #[test]
    fn test_from_impls() {
        let v: ConfigValue = "abc".into();
        assert_eq!(v.as_str(), Some("abc"));
        let v: ConfigValue = String::from("def").into();
        assert_eq!(v.as_str(), Some("def"));
        let mut m = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let v: ConfigValue = m.into();
        assert!(v.as_map().is_some());
    }

    #[test]
    fn test_get_bool_truthy_values() {
        // AutoTrue: "1" / "yes" / "true" / "on" → Some(true).
        let mut m = HashMap::new();
        m.insert("a".into(), leaf("1"));
        m.insert("b".into(), leaf("yes"));
        m.insert("c".into(), leaf("true"));
        m.insert("d".into(), leaf("on"));
        let v = ConfigValue::Map(m);
        for k in ["a", "b", "c", "d"] {
            assert_eq!(v.get_bool(k), Some(true), "expected true for key {}", k);
        }
    }

    #[test]
    fn test_get_bool_case_sensitive() {
        // `get_bool` does a direct `matches!` comparison — no case folding.
        let mut m = HashMap::new();
        m.insert("upper".into(), leaf("YES"));
        m.insert("mixed".into(), leaf("True"));
        let v = ConfigValue::Map(m);
        // "YES" isn't one of the truthy strings → false.
        assert_eq!(v.get_bool("upper"), Some(false));
        // "True" similarly not matched.
        assert_eq!(v.get_bool("mixed"), Some(false));
    }

    #[test]
    fn test_is_variants_mutually_exclusive() {
        // A value should match exactly one of Str/Map/List.
        let s = leaf("x");
        assert!(s.as_str().is_some() && s.as_map().is_none() && s.as_list().is_none());
        let l = ConfigValue::List(vec![leaf("x")]);
        assert!(l.as_str().is_none() && l.as_map().is_none() && l.as_list().is_some());
        let m = ConfigValue::Map(HashMap::new());
        assert!(m.as_str().is_none() && m.as_map().is_some() && m.as_list().is_none());
    }

    #[test]
    fn test_clone_and_eq_derived() {
        // Clone + PartialEq derived impls work as expected.
        let v1 = ConfigValue::Str("hello".into());
        let v2 = v1.clone();
        assert_eq!(v1, v2);
        let v3 = ConfigValue::Str("world".into());
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_as_list_or_single_empty_list_stays_empty() {
        // Explicit empty List → empty Vec (not wrapped).
        let empty_list = ConfigValue::List(vec![]);
        let v = empty_list.as_list_or_single();
        assert!(v.is_empty());
    }

    #[test]
    fn test_get_nested_map_traversal_one_level() {
        // `get(key)` on a Map returns the child ConfigValue.
        let mut inner = HashMap::new();
        inner.insert("inner_key".into(), leaf("inner_val"));
        let mut outer = HashMap::new();
        outer.insert("outer".into(), ConfigValue::Map(inner));
        let root = ConfigValue::Map(outer);
        // Direct hit.
        assert_eq!(root.get("outer").unwrap().get_str("inner_key"), Some("inner_val"));
        // Missing key on a Map → None.
        assert!(root.get("nonexistent").is_none());
    }

    #[test]
    fn test_get_i64_negative_and_hex_not_supported() {
        // Negative ints parse fine.
        let mut m = HashMap::new();
        m.insert("n".into(), leaf("-42"));
        // Hex strings don't parse via f64::parse — returns None.
        m.insert("h".into(), leaf("0xff"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_i64("n"), Some(-42));
        assert!(v.get_i64("h").is_none());
    }

    #[test]
    fn test_from_impls_preserve_type() {
        // From<&str> yields ConfigValue::Str.
        let v: ConfigValue = "abc".into();
        assert!(matches!(v, ConfigValue::Str(ref s) if s == "abc"));
        // From<HashMap> yields ConfigValue::Map.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let v: ConfigValue = m.into();
        assert!(matches!(v, ConfigValue::Map(_)));
    }

    #[test]
    fn test_get_f64_scientific_notation() {
        // "1.5e6" parses as scientific notation via f64.
        let mut m = HashMap::new();
        m.insert("big".into(), leaf("1.5e6"));
        m.insert("tiny".into(), leaf("1e-3"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_f64("big"), Some(1_500_000.0));
        assert!((v.get_f64("tiny").unwrap() - 0.001).abs() < 1e-12);
    }

    #[test]
    fn test_get_bool_empty_string_is_false() {
        // Empty string doesn't match any of "1"/"yes"/"true"/"on" → Some(false).
        let mut m = HashMap::new();
        m.insert("empty".into(), leaf(""));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_bool("empty"), Some(false));
    }

    #[test]
    fn test_as_list_or_single_str_variant_wraps() {
        // A Str variant → single-element Vec wrapping itself.
        let s = leaf("single");
        let v = s.as_list_or_single();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].as_str(), Some("single"));
    }

    #[test]
    fn test_from_string_owned_to_str_variant() {
        // From<String> (owned) → ConfigValue::Str.
        let owned = String::from("owned_str");
        let v: ConfigValue = owned.into();
        assert!(matches!(v, ConfigValue::Str(ref s) if s == "owned_str"));
    }

    #[test]
    fn test_get_str_skips_non_string_child() {
        // If the child key is a Map or List, get_str returns None (not the child).
        let mut m = HashMap::new();
        m.insert("map_val".into(), ConfigValue::Map(HashMap::new()));
        m.insert("list_val".into(), ConfigValue::List(vec![leaf("x")]));
        m.insert("str_val".into(), leaf("hello"));
        let v = ConfigValue::Map(m);
        assert!(v.get_str("map_val").is_none());
        assert!(v.get_str("list_val").is_none());
        assert_eq!(v.get_str("str_val"), Some("hello"));
    }

    #[test]
    fn test_get_f64_integer_string_parses_as_float() {
        // "42" parses as 42.0 via f64::parse (no decimal needed).
        let mut m = HashMap::new();
        m.insert("n".into(), leaf("42"));
        m.insert("neg".into(), leaf("-7"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_f64("n"), Some(42.0));
        assert_eq!(v.get_f64("neg"), Some(-7.0));
    }

    #[test]
    fn test_partial_eq_across_variants() {
        // PartialEq derived — different variants of same content still unequal.
        let s = ConfigValue::Str("abc".into());
        let mut m = HashMap::new();
        m.insert("abc".into(), leaf(""));
        let map_form = ConfigValue::Map(m);
        // Str vs Map — never equal even if names overlap.
        assert_ne!(s, map_form);
        // Same List contents → equal.
        let l1 = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let l2 = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        assert_eq!(l1, l2);
        // Different order → unequal.
        let l3 = ConfigValue::List(vec![leaf("b"), leaf("a")]);
        assert_ne!(l1, l3);
    }

    #[test]
    fn test_as_list_or_single_nested_list_preserves_refs() {
        // References returned by as_list_or_single point to the original elements.
        let list = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let refs = list.as_list_or_single();
        assert_eq!(refs.len(), 3);
        // Each returned ref is the actual enum inside the list — as_str works.
        assert_eq!(refs[0].as_str(), Some("a"));
        assert_eq!(refs[2].as_str(), Some("c"));
        // Calling again returns fresh refs of the same length.
        let refs2 = list.as_list_or_single();
        assert_eq!(refs2.len(), refs.len());
    }

    #[test]
    fn test_get_f64_negative_exponent() {
        // "5e-3" → 0.005.
        let mut m = HashMap::new();
        m.insert("tiny".into(), leaf("5e-3"));
        m.insert("big_neg".into(), leaf("-1.5e6"));
        let v = ConfigValue::Map(m);
        assert!((v.get_f64("tiny").unwrap() - 0.005).abs() < 1e-12);
        assert_eq!(v.get_f64("big_neg"), Some(-1_500_000.0));
    }

    #[test]
    fn test_get_bool_unknown_values_default_to_false() {
        // Any value not in "1"/"yes"/"true"/"on" → Some(false), not None.
        let mut m = HashMap::new();
        m.insert("unknown".into(), leaf("maybe"));
        m.insert("zero".into(), leaf("0"));
        m.insert("whitespace".into(), leaf(" "));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_bool("unknown"), Some(false));
        assert_eq!(v.get_bool("zero"), Some(false));
        assert_eq!(v.get_bool("whitespace"), Some(false));
    }

    #[test]
    fn test_configvalue_clone_deep_for_nested_list_and_map() {
        // Clone is deep for List + Map variants.
        let mut inner_map = HashMap::new();
        inner_map.insert("k".into(), leaf("v"));
        let list = ConfigValue::List(vec![
            leaf("a"),
            ConfigValue::Map(inner_map.clone()),
            leaf("c"),
        ]);
        let mut cloned = list.clone();
        // Mutate the cloned List — source still sees the original.
        if let ConfigValue::List(ref mut l) = cloned {
            l.push(leaf("d"));
        }
        match list {
            ConfigValue::List(l) => assert_eq!(l.len(), 3),
            _ => panic!("expected list"),
        }
        match cloned {
            ConfigValue::List(l) => assert_eq!(l.len(), 4),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_get_str_preserves_empty_string_value() {
        // A Str child with empty value → get_str returns Some(""), not None.
        let mut m = HashMap::new();
        m.insert("empty".into(), leaf(""));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_str("empty"), Some(""));
        // get_f64 on empty string → None (parse fails).
        assert!(v.get_f64("empty").is_none());
        // get_bool on empty string → Some(false) (no match).
        assert_eq!(v.get_bool("empty"), Some(false));
    }

    #[test]
    fn test_get_i64_extreme_values() {
        // i64 range boundaries.
        let mut m = HashMap::new();
        m.insert("max".into(), leaf(&i64::MAX.to_string()));
        m.insert("min".into(), leaf(&i64::MIN.to_string()));
        m.insert("over".into(), leaf("9223372036854775808")); // MAX+1 → parse fails
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_i64("max"), Some(i64::MAX));
        assert_eq!(v.get_i64("min"), Some(i64::MIN));
        assert!(v.get_i64("over").is_none());
    }

    #[test]
    fn test_as_list_or_single_preserves_map_vs_str_distinction() {
        // A Map wrapped as single-element list retains its Map type.
        let mut inner = HashMap::new();
        inner.insert("k".into(), leaf("v"));
        let m = ConfigValue::Map(inner);
        let list = m.as_list_or_single();
        assert_eq!(list.len(), 1);
        // The single element is still a Map — not promoted to Str.
        assert!(list[0].as_map().is_some());
        assert!(list[0].as_str().is_none());
    }

    #[test]
    fn test_from_str_and_string_impls_produce_equivalent_str() {
        // From<&str> and From<String> produce the same variant (both Str).
        let v1: ConfigValue = "hello".into();
        let v2: ConfigValue = String::from("hello").into();
        assert_eq!(v1, v2);
        // From<HashMap> produces Map variant — distinct from Str.
        let mut m = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let v3: ConfigValue = m.into();
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_default_is_map_not_str_or_list() {
        // Default ConfigValue is specifically Map, not Str/List.
        let d = ConfigValue::default();
        assert!(d.as_map().is_some());
        assert!(d.as_str().is_none());
        assert!(d.as_list().is_none());
        // And the map is empty.
        let m = d.as_map().unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_get_on_list_returns_none() {
        // `get` on a List returns None (only works on Map variant).
        let l = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        assert!(l.get("0").is_none());
        assert!(l.get("a").is_none());
    }

    #[test]
    fn test_clone_deep_for_map_variant() {
        // Cloning a Map variant produces an independent HashMap.
        let mut m = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let v = ConfigValue::Map(m);
        let cloned = v.clone();
        // Both have the key.
        assert!(v.as_map().unwrap().contains_key("k"));
        assert!(cloned.as_map().unwrap().contains_key("k"));
        // PartialEq matches.
        assert_eq!(v, cloned);
    }

    #[test]
    fn test_get_f64_handles_leading_positive_sign() {
        // "+42" is valid f64 input.
        let mut m = HashMap::new();
        m.insert("n".into(), leaf("+42"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_f64("n"), Some(42.0));
    }

    #[test]
    fn test_as_list_or_single_map_variant_wraps_as_one_element() {
        // A Map variant → wrapped as single-element Vec (not unwrapped).
        let mut m = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let v = ConfigValue::Map(m);
        let list = v.as_list_or_single();
        assert_eq!(list.len(), 1);
        // The single element is the Map itself.
        assert!(list[0].as_map().is_some());
    }

    #[test]
    fn test_configvalue_str_variant_deref_not_automatic() {
        // Deref is NOT automatically implemented — use .as_str() explicitly.
        let v = ConfigValue::Str("abc".into());
        assert_eq!(v.as_str(), Some("abc"));
    }

    #[test]
    fn test_get_bool_off_and_on_alternating() {
        // "on"/"off" are recognized forms.
        let mut m = HashMap::new();
        m.insert("a".into(), leaf("on"));
        m.insert("b".into(), leaf("off"));
        let v = ConfigValue::Map(m);
        // "on" is NOT in truthy patterns ("1"/"yes"/"true"/"on") — wait, "on" IS truthy.
        assert_eq!(v.get_bool("a"), Some(true));
        // "off" not truthy.
        assert_eq!(v.get_bool("b"), Some(false));
    }

    #[test]
    fn test_get_str_on_nested_map_returns_none() {
        // get_str on nested Map child → None.
        let mut inner = HashMap::new();
        inner.insert("sub".into(), leaf("v"));
        let mut outer = HashMap::new();
        outer.insert("nested".into(), ConfigValue::Map(inner));
        let v = ConfigValue::Map(outer);
        assert!(v.get_str("nested").is_none());
        // But outer.get("nested") returns Some(Map) not None.
        assert!(v.get("nested").is_some());
    }

    #[test]
    fn test_configvalue_list_empty_roundtrips() {
        // Empty List variant maintains its type.
        let v = ConfigValue::List(vec![]);
        assert!(v.as_list().is_some());
        assert_eq!(v.as_list().unwrap().len(), 0);
        // Clone preserves empty list.
        let c = v.clone();
        assert!(c.as_list().is_some());
        assert_eq!(c.as_list().unwrap().len(), 0);
    }

    #[test]
    fn test_as_list_or_single_non_list_wraps_as_single_ref() {
        // Str/Map variants wrap in a 1-element Vec<&ConfigValue>.
        let s = leaf("hello");
        let v = s.as_list_or_single();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].as_str(), Some("hello"));
        let m = ConfigValue::Map(HashMap::new());
        let v2 = m.as_list_or_single();
        assert_eq!(v2.len(), 1);
        assert!(v2[0].as_map().is_some());
        // List variant yields its contents (not wrapped again).
        let l = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let v3 = l.as_list_or_single();
        assert_eq!(v3.len(), 3);
    }

    #[test]
    fn test_configvalue_default_is_empty_map_variant() {
        // Default impl yields Map(empty), not Str or List.
        let d: ConfigValue = Default::default();
        assert!(d.as_map().is_some());
        assert_eq!(d.as_map().unwrap().len(), 0);
        assert!(d.as_str().is_none());
        assert!(d.as_list().is_none());
    }

    #[test]
    fn test_get_bool_unrecognized_string_returns_some_false() {
        // Map with unrecognized truthiness string → Some(false), NOT None.
        // (matches! returns false; the Option wraps around that with Some.)
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf("maybe"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_bool("flag"), Some(false));
        // Missing key → None (no string to match against).
        assert_eq!(v.get_bool("absent"), None);
        // Empty string → Some(false) (no match).
        let mut m2: HashMap<String, ConfigValue> = HashMap::new();
        m2.insert("flag".into(), leaf(""));
        let v2 = ConfigValue::Map(m2);
        assert_eq!(v2.get_bool("flag"), Some(false));
    }

    #[test]
    fn test_from_string_and_from_str_both_produce_str_variant() {
        // Both From impls route to ConfigValue::Str.
        let a: ConfigValue = "hello".into();
        let b: ConfigValue = String::from("world").into();
        assert_eq!(a.as_str(), Some("hello"));
        assert_eq!(b.as_str(), Some("world"));
        // Both are Eq-comparable via derived PartialEq.
        assert_eq!(a, ConfigValue::Str("hello".into()));
        assert_eq!(b, ConfigValue::Str("world".into()));
    }

    #[test]
    fn test_get_f64_and_get_i64_parse_string_representations() {
        // get_f64/get_i64 parse the stored Str value.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("pi".into(), leaf("3.14159"));
        m.insert("count".into(), leaf("42"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_f64("pi"), Some(3.14159));
        assert_eq!(v.get_i64("count"), Some(42));
        // Missing key → None.
        assert_eq!(v.get_f64("absent"), None);
        assert_eq!(v.get_i64("absent"), None);
    }

    #[test]
    fn test_get_f64_and_get_i64_return_none_on_non_numeric() {
        // Parse fails → None propagated.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("bad".into(), leaf("not_a_number"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_f64("bad"), None);
        assert_eq!(v.get_i64("bad"), None);
    }

    #[test]
    fn test_from_hashmap_conversion_creates_map_variant() {
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let v: ConfigValue = m.into();
        assert!(v.as_map().is_some());
        assert_eq!(v.get_str("k"), Some("v"));
    }

    #[test]
    fn test_partial_eq_distinguishes_variants_with_empty_contents() {
        // Empty Map and empty List are NOT equal despite both being "empty".
        let m = ConfigValue::Map(HashMap::new());
        let l = ConfigValue::List(Vec::new());
        let s = ConfigValue::Str(String::new());
        assert_ne!(m, l);
        assert_ne!(m, s);
        assert_ne!(l, s);
        // Same-variant empties are equal to themselves.
        assert_eq!(m, ConfigValue::Map(HashMap::new()));
        assert_eq!(l, ConfigValue::List(Vec::new()));
    }

    #[test]
    fn test_get_returns_top_level_value_not_recursive() {
        // .get() only looks at the immediate Map's keys — doesn't recurse.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("nested_key".into(), leaf("inside"));
        let mut outer: HashMap<String, ConfigValue> = HashMap::new();
        outer.insert("a".into(), ConfigValue::Map(inner));
        outer.insert("b".into(), leaf("b-val"));
        let v = ConfigValue::Map(outer);
        // get("a") returns the Map, not the Str inside it.
        assert!(v.get("a").unwrap().as_map().is_some());
        // "nested_key" at top level → None (not recursive).
        assert!(v.get("nested_key").is_none());
    }

    #[test]
    fn test_list_clone_preserves_order_and_length() {
        // Clone of List → order-preserving copy.
        let l = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let c = l.clone();
        let items = c.as_list().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_str(), Some("a"));
        assert_eq!(items[1].as_str(), Some("b"));
        assert_eq!(items[2].as_str(), Some("c"));
    }

    #[test]
    fn test_as_str_returns_none_on_non_str_variants() {
        // Map/List variants all return None from as_str (only Str succeeds).
        let m = ConfigValue::Map(HashMap::new());
        assert!(m.as_str().is_none());
        let l = ConfigValue::List(Vec::new());
        assert!(l.as_str().is_none());
        let s = leaf("hello");
        assert_eq!(s.as_str(), Some("hello"));
    }

    #[test]
    fn test_str_equality_case_sensitive_distinguishes_casing() {
        // PartialEq is byte-wise — different casing → not equal.
        let a = leaf("Hello");
        let b = leaf("hello");
        let c = leaf("Hello");
        assert_ne!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn test_as_list_or_single_empty_list_vs_single_value_distinct_lengths() {
        // Empty List → 0 elements; single Str → 1 element (wrapped).
        let empty_list = ConfigValue::List(Vec::new());
        let single = leaf("x");
        assert_eq!(empty_list.as_list_or_single().len(), 0);
        assert_eq!(single.as_list_or_single().len(), 1);
    }

    #[test]
    fn test_get_f64_handles_negative_and_scientific_notation() {
        // parse::<f64>() accepts "-3.14" and "1e5".
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("neg".into(), leaf("-3.14"));
        m.insert("sci".into(), leaf("1e5"));
        m.insert("nan".into(), leaf("NaN"));
        let v = ConfigValue::Map(m);
        assert_eq!(v.get_f64("neg"), Some(-3.14));
        assert_eq!(v.get_f64("sci"), Some(100_000.0));
        // "NaN" parses as f64::NAN — comparing requires is_nan().
        assert!(v.get_f64("nan").unwrap().is_nan());
    }

    #[test]
    fn test_get_bool_case_sensitive_only_lowercase_truthy() {
        // matches! is case-sensitive — uppercase variants return false.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("upper".into(), leaf("YES"));
        m.insert("mixed".into(), leaf("Yes"));
        m.insert("lower".into(), leaf("yes"));
        let v = ConfigValue::Map(m);
        // Upper/Mixed → false (don't match lowercase set).
        assert_eq!(v.get_bool("upper"), Some(false));
        assert_eq!(v.get_bool("mixed"), Some(false));
        // Lower → true.
        assert_eq!(v.get_bool("lower"), Some(true));
    }

    #[test]
    fn test_clone_of_nested_map_produces_independent_inner_hashmap() {
        // Nested Map — clone doesn't share inner HashMap.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("k".into(), leaf("v"));
        let outer = ConfigValue::Map({
            let mut m: HashMap<String, ConfigValue> = HashMap::new();
            m.insert("nested".into(), ConfigValue::Map(inner));
            m
        });
        let cloned = outer.clone();
        // Both should read same leaf.
        assert_eq!(outer.get("nested").unwrap().get_str("k"), Some("v"));
        assert_eq!(cloned.get("nested").unwrap().get_str("k"), Some("v"));
    }

    #[test]
    fn test_get_bool_accepts_on_as_true() {
        // Config::General AutoTrue recognizes "on" in addition to yes/true/1.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf("on"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("flag"), Some(true));
        // "off" is NOT a special false token — it evaluates to false but via falling through the match.
        let mut m2: HashMap<String, ConfigValue> = HashMap::new();
        m2.insert("flag".into(), leaf("off"));
        let c2 = ConfigValue::Map(m2);
        assert_eq!(c2.get_bool("flag"), Some(false));
    }

    #[test]
    fn test_as_list_on_non_list_variants_returns_none() {
        // as_list returns Some only for List variant; Str/Map both None.
        assert_eq!(ConfigValue::Str("abc".into()).as_list(), None);
        assert_eq!(ConfigValue::Map(HashMap::new()).as_list(), None);
        // Empty list variant returns Some of empty vec.
        let lst = ConfigValue::List(vec![]);
        assert_eq!(lst.as_list().unwrap().len(), 0);
    }

    #[test]
    fn test_get_on_list_variant_returns_none_even_for_valid_keys() {
        // .get() only descends into Map — a List with numeric-looking keys still returns None.
        let lst = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        assert_eq!(lst.get("0"), None);
        assert_eq!(lst.get("1"), None);
        assert_eq!(lst.get("anything"), None);
    }

    #[test]
    fn test_as_list_or_single_returns_borrowed_refs_not_clones() {
        // The returned Vec<&ConfigValue> borrows — pointing to the same memory.
        let value = leaf("shared");
        let wrapper = ConfigValue::List(vec![value.clone()]);
        let refs = wrapper.as_list_or_single();
        assert_eq!(refs.len(), 1);
        // Check the borrowed ref points to the inner list element (by value equality).
        assert_eq!(refs[0].as_str(), Some("shared"));
    }

    #[test]
    fn test_get_i64_rejects_float_literal() {
        // "3.14" is not an i64 literal — parse fails → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("3.14"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), None);
        // But whole-number strings work.
        let mut m2: HashMap<String, ConfigValue> = HashMap::new();
        m2.insert("n".into(), leaf("42"));
        let c2 = ConfigValue::Map(m2);
        assert_eq!(c2.get_i64("n"), Some(42));
    }

    #[test]
    fn test_from_impls_produce_distinct_values_for_different_input_types() {
        // &str → Str variant; String → Str variant; HashMap → Map variant.
        let from_str: ConfigValue = "hello".into();
        let from_string: ConfigValue = String::from("hello").into();
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let from_map: ConfigValue = m.clone().into();
        // Str variants compare equal for same content.
        assert_eq!(from_str, from_string);
        // Map variant differs from Str variant.
        assert_ne!(from_str, from_map);
    }

    #[test]
    fn test_as_list_or_single_on_map_wraps_as_one_element_ref() {
        // Map variant is treated as "single" → wrapped in a 1-element Vec.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let map_val = ConfigValue::Map(m);
        let list = map_val.as_list_or_single();
        assert_eq!(list.len(), 1);
        // The wrapped ref points back to the Map.
        assert!(list[0].as_map().is_some());
    }

    #[test]
    fn test_get_bool_on_nested_map_returns_none_for_bool_variant() {
        // get_bool walks get_str which only looks at Str variants — bool on a nested Map → None.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("real_flag".into(), leaf("1"));
        let mut outer: HashMap<String, ConfigValue> = HashMap::new();
        outer.insert("nested".into(), ConfigValue::Map(inner));
        let c = ConfigValue::Map(outer);
        // "nested" is a Map, not a Str → get_str returns None → get_bool → None.
        assert_eq!(c.get_bool("nested"), None);
    }

    #[test]
    fn test_as_map_on_mutable_like_clone_preserves_map_entries() {
        // ConfigValue::Map clone — as_map().get() on clone works the same as on source.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("1"));
        m.insert("b".into(), leaf("2"));
        let src = ConfigValue::Map(m);
        let cloned = src.clone();
        let src_a = src.as_map().unwrap().get("a").and_then(|v| v.as_str());
        let clone_a = cloned.as_map().unwrap().get("a").and_then(|v| v.as_str());
        assert_eq!(src_a, Some("1"));
        assert_eq!(clone_a, Some("1"));
    }

    #[test]
    fn test_config_value_partial_eq_same_string_content_equal() {
        // PartialEq: Str("foo") == Str("foo"); Map with same entries equal; List with same items equal.
        assert_eq!(ConfigValue::Str("x".into()), ConfigValue::Str("x".into()));
        let mut m1: HashMap<String, ConfigValue> = HashMap::new();
        m1.insert("k".into(), leaf("v"));
        let mut m2: HashMap<String, ConfigValue> = HashMap::new();
        m2.insert("k".into(), leaf("v"));
        assert_eq!(ConfigValue::Map(m1), ConfigValue::Map(m2));
        assert_eq!(
            ConfigValue::List(vec![leaf("a"), leaf("b")]),
            ConfigValue::List(vec![leaf("a"), leaf("b")]),
        );
    }

    #[test]
    fn test_get_f64_on_bool_like_string_fails_parse() {
        // "yes"/"true" are not valid f64 — parse returns Err → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf("yes"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("flag"), None);
        // Same for "true".
        let mut m2: HashMap<String, ConfigValue> = HashMap::new();
        m2.insert("flag".into(), leaf("true"));
        let c2 = ConfigValue::Map(m2);
        assert_eq!(c2.get_f64("flag"), None);
    }

    #[test]
    fn test_from_str_trait_creates_str_variant_preserving_content() {
        // <&str>::into → Str variant with same content.
        let raw = "some content";
        let cv: ConfigValue = raw.into();
        assert_eq!(cv.as_str(), Some(raw));
        assert!(cv.as_map().is_none());
        assert!(cv.as_list().is_none());
    }

    #[test]
    fn test_as_list_or_single_with_deeply_nested_list_unwraps_one_level() {
        // List inside List — outer wrapping stripped; inner List preserved as inner elements.
        let inner_list = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let outer = ConfigValue::List(vec![inner_list.clone()]);
        let refs = outer.as_list_or_single();
        // Outer List → refs is the outer elements (1 element: the inner List).
        assert_eq!(refs.len(), 1);
        assert!(refs[0].as_list().is_some());
    }

    #[test]
    fn test_get_bool_falsy_lowercase_not_in_truthy_set_returns_false() {
        // "yes"/"true"/"on"/"1" match → true; everything else → false.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("no"));
        m.insert("b".into(), leaf("random"));
        m.insert("c".into(), leaf(""));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("a"), Some(false));
        assert_eq!(c.get_bool("b"), Some(false));
        assert_eq!(c.get_bool("c"), Some(false));
    }

    #[test]
    fn test_as_str_unchanged_across_clone() {
        // Clone of Str variant preserves exact content.
        let src = ConfigValue::Str("original content".into());
        let clone = src.clone();
        assert_eq!(src.as_str(), clone.as_str());
        // And both point at equivalent values (PartialEq).
        assert_eq!(src, clone);
    }

    #[test]
    fn test_as_list_on_str_and_map_variants_both_none() {
        // as_list is None for non-List variants.
        assert!(ConfigValue::Str("x".into()).as_list().is_none());
        assert!(ConfigValue::Map(HashMap::new()).as_list().is_none());
        // But as_map is Some for Map.
        assert!(ConfigValue::Map(HashMap::new()).as_map().is_some());
        assert!(ConfigValue::Str("x".into()).as_map().is_none());
    }

    #[test]
    fn test_default_instance_returns_map_variant_with_empty_hashmap() {
        // Default::default returns Map(HashMap::new()).
        let d = ConfigValue::default();
        let m = d.as_map().expect("default is Map");
        assert!(m.is_empty());
    }

    #[test]
    fn test_config_value_deep_nested_clone_preserves_structure() {
        // Deep clone of Map->Map->Str triple nesting.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("x".into(), leaf("v"));
        let mut mid: HashMap<String, ConfigValue> = HashMap::new();
        mid.insert("inner".into(), ConfigValue::Map(inner));
        let mut outer: HashMap<String, ConfigValue> = HashMap::new();
        outer.insert("mid".into(), ConfigValue::Map(mid));
        let c = ConfigValue::Map(outer);
        let clone = c.clone();
        // Both paths should resolve to "v".
        let via_src = c
            .as_map().and_then(|m| m.get("mid"))
            .and_then(|v| v.as_map()).and_then(|m| m.get("inner"))
            .and_then(|v| v.as_map()).and_then(|m| m.get("x"))
            .and_then(|v| v.as_str());
        let via_clone = clone
            .as_map().and_then(|m| m.get("mid"))
            .and_then(|v| v.as_map()).and_then(|m| m.get("inner"))
            .and_then(|v| v.as_map()).and_then(|m| m.get("x"))
            .and_then(|v| v.as_str());
        assert_eq!(via_src, Some("v"));
        assert_eq!(via_clone, Some("v"));
    }

    #[test]
    fn test_as_list_or_single_preserves_original_elements() {
        // List contents are preserved — refs point to original list items.
        let src = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let refs = src.as_list_or_single();
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].as_str(), Some("a"));
        assert_eq!(refs[1].as_str(), Some("b"));
        assert_eq!(refs[2].as_str(), Some("c"));
    }

    #[test]
    fn test_get_str_returns_none_for_non_existent_key() {
        // get_str on missing key → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("present".into(), leaf("x"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_str("missing"), None);
        assert_eq!(c.get_str("present"), Some("x"));
    }

    #[test]
    fn test_get_f64_parses_negative_fractional() {
        // Negative float parses correctly.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("v".into(), leaf("-2.718"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("v"), Some(-2.718));
    }

    #[test]
    fn test_get_i64_parses_negative_integer() {
        // Negative integer.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("v".into(), leaf("-42"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("v"), Some(-42));
    }

    #[test]
    fn test_as_list_or_single_on_deeply_nested_map() {
        // Map variant → wraps as 1-element ref vec.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("x".into(), leaf("y"));
        let mv = ConfigValue::Map(inner);
        let refs = mv.as_list_or_single();
        assert_eq!(refs.len(), 1);
        // The wrapped ref is the Map itself.
        assert!(refs[0].as_map().is_some());
    }

    #[test]
    fn test_config_value_str_and_map_not_equal_with_empty_content() {
        // Empty Str "" and empty Map — different variants → not equal.
        assert_ne!(ConfigValue::Str(String::new()), ConfigValue::Map(HashMap::new()));
        // Empty Str and empty List — also different variants.
        assert_ne!(ConfigValue::Str(String::new()), ConfigValue::List(Vec::new()));
    }

    #[test]
    fn test_get_on_str_returns_none_regardless_of_key() {
        // get() only descends into Map — Str variants always None.
        let s = ConfigValue::Str("hello".into());
        assert!(s.get("hello").is_none());
        assert!(s.get("").is_none());
        assert!(s.get("anything").is_none());
    }

    #[test]
    fn test_as_list_or_single_str_variant_returns_one_element_ref() {
        // Str → wrapped as single-element Vec<&ConfigValue>.
        let s = ConfigValue::Str("test".into());
        let refs = s.as_list_or_single();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].as_str(), Some("test"));
    }

    #[test]
    fn test_get_bool_on_empty_value_returns_false() {
        // Empty string "" → not in truthy set → Some(false).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf(""));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("flag"), Some(false));
    }

    #[test]
    fn test_config_value_list_order_preserved_through_cloning() {
        // List ordering preserved through clone.
        let items: Vec<ConfigValue> = (0..5).map(|i| leaf(&format!("item_{}", i))).collect();
        let src = ConfigValue::List(items);
        let cloned = src.clone();
        let src_list = src.as_list().unwrap();
        let cloned_list = cloned.as_list().unwrap();
        for (a, b) in src_list.iter().zip(cloned_list.iter()) {
            assert_eq!(a.as_str(), b.as_str());
        }
    }

    #[test]
    fn test_get_f64_scientific_notation_parses_correctly() {
        // Scientific notation strings parse via f64::parse.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("x".into(), leaf("1.5e3"));
        m.insert("y".into(), leaf("-2.5e-2"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("x"), Some(1500.0));
        assert_eq!(c.get_f64("y"), Some(-0.025));
    }

    #[test]
    fn test_get_i64_fractional_string_returns_none() {
        // Fractional value can't parse as i64 — returns None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("3.14"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), None);
    }

    #[test]
    fn test_get_bool_on_truthy_returns_some_true() {
        // All four truthy variants → Some(true).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("1"));
        m.insert("b".into(), leaf("yes"));
        m.insert("c".into(), leaf("true"));
        m.insert("d".into(), leaf("on"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("a"), Some(true));
        assert_eq!(c.get_bool("b"), Some(true));
        assert_eq!(c.get_bool("c"), Some(true));
        assert_eq!(c.get_bool("d"), Some(true));
    }

    #[test]
    fn test_as_list_from_map_variant_returns_none() {
        // as_list on Map variant → None (only List variant returns Some).
        let c = ConfigValue::Map(HashMap::new());
        assert!(c.as_list().is_none());
    }

    #[test]
    fn test_from_str_impl_creates_str_variant() {
        // &str → ConfigValue::Str via From impl.
        let c: ConfigValue = "hello".into();
        assert_eq!(c.as_str(), Some("hello"));
    }

    #[test]
    fn test_from_string_impl_creates_str_variant() {
        // String → ConfigValue::Str via owned From impl.
        let c: ConfigValue = String::from("world").into();
        assert_eq!(c.as_str(), Some("world"));
    }

    #[test]
    fn test_from_hashmap_impl_creates_map_variant() {
        // HashMap → ConfigValue::Map via From impl.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let c: ConfigValue = m.into();
        assert!(c.as_map().is_some());
        assert_eq!(c.get_str("k"), Some("v"));
    }

    #[test]
    fn test_as_list_or_single_on_list_with_zero_items_returns_empty_vec() {
        // Empty List → empty refs Vec.
        let c = ConfigValue::List(Vec::new());
        assert!(c.as_list_or_single().is_empty());
    }

    #[test]
    fn test_get_str_nested_map_key_present_returns_str() {
        // c["outer"]["inner"] round trip via get_str.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("inner".into(), leaf("foo"));
        let mut outer: HashMap<String, ConfigValue> = HashMap::new();
        outer.insert("outer".into(), ConfigValue::Map(inner));
        let c = ConfigValue::Map(outer);
        let inner_ref = c.get("outer").expect("outer");
        assert_eq!(inner_ref.get_str("inner"), Some("foo"));
    }

    #[test]
    fn test_get_f64_positive_integer_string_parses() {
        // "100" → 100.0 via parse.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("100"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("n"), Some(100.0));
    }

    #[test]
    fn test_get_i64_negative_integer_string_parses() {
        // "-12345" → -12345.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("-12345"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), Some(-12345));
    }

    #[test]
    fn test_as_list_or_single_on_map_variant_wraps_reference() {
        // Map variant → as_list_or_single wraps reference as 1-element Vec.
        let c = ConfigValue::Map(HashMap::new());
        let v = c.as_list_or_single();
        assert_eq!(v.len(), 1);
        // The reference points to the same Map variant.
        assert!(v[0].as_map().is_some());
    }

    #[test]
    fn test_as_map_from_str_variant_returns_none() {
        // as_map on Str variant → None.
        let c = leaf("scalar");
        assert!(c.as_map().is_none());
    }

    #[test]
    fn test_get_str_on_list_variant_returns_none() {
        // List doesn't implement key lookups → get_str returns None.
        let c = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        assert!(c.get_str("any_key").is_none());
    }

    #[test]
    fn test_default_is_map_with_zero_entries() {
        // Default variant is Map; starts empty.
        let c = ConfigValue::default();
        let m = c.as_map().expect("map");
        assert!(m.is_empty());
    }

    #[test]
    fn test_as_list_on_list_returns_all_elements_in_insertion_order() {
        // List variant preserves order.
        let items = vec![leaf("first"), leaf("second"), leaf("third")];
        let c = ConfigValue::List(items);
        let l = c.as_list().expect("list");
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].as_str(), Some("first"));
        assert_eq!(l[1].as_str(), Some("second"));
        assert_eq!(l[2].as_str(), Some("third"));
    }

    #[test]
    fn test_as_list_or_single_returns_same_element_count_for_list_with_items() {
        // List[3] → 3 refs. List[0] → 0. Str/Map → 1.
        let list3 = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        assert_eq!(list3.as_list_or_single().len(), 3);

        let empty_list = ConfigValue::List(Vec::new());
        assert_eq!(empty_list.as_list_or_single().len(), 0);

        let scalar = leaf("x");
        assert_eq!(scalar.as_list_or_single().len(), 1);
    }

    #[test]
    fn test_get_bool_on_missing_key_returns_none() {
        // Key absent → None (distinct from Some(false)).
        let c = ConfigValue::Map(HashMap::new());
        assert_eq!(c.get_bool("missing"), None);
    }

    #[test]
    fn test_clone_preserves_str_content_exactly() {
        // Str variant Clone retains exact bytes.
        let s1 = leaf("hello world with spaces and punctuation!");
        let s2 = s1.clone();
        assert_eq!(s1.as_str(), s2.as_str());
    }

    #[test]
    fn test_as_map_from_list_variant_returns_none() {
        // as_map on List variant → None.
        let c = ConfigValue::List(Vec::new());
        assert!(c.as_map().is_none());
    }

    #[test]
    fn test_get_f64_zero_and_negative_zero_both_parse() {
        // "0" and "-0" both → 0.0.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("0"));
        m.insert("b".into(), leaf("-0"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("a"), Some(0.0));
        assert_eq!(c.get_f64("b"), Some(0.0));
    }

    #[test]
    fn test_get_bool_unrecognized_value_returns_some_false() {
        // Not-in-truthy-set but present → Some(false).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("f".into(), leaf("random_string"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("f"), Some(false));
    }

    #[test]
    fn test_as_str_on_map_variant_returns_none() {
        // as_str on Map variant → None.
        let c = ConfigValue::Map(HashMap::new());
        assert!(c.as_str().is_none());
    }

    #[test]
    fn test_get_i64_leading_plus_sign_parses() {
        // "+100" → 100 (i64::from_str accepts explicit +).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("+100"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), Some(100));
    }

    #[test]
    fn test_config_value_list_clone_preserves_element_count() {
        // Cloning a List preserves exact element count.
        let list = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c"), leaf("d")]);
        let cloned = list.clone();
        assert_eq!(cloned.as_list().unwrap().len(), 4);
    }

    #[test]
    fn test_config_value_map_clone_preserves_key_count() {
        // Cloning a Map preserves all keys.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        for i in 0..5 {
            m.insert(format!("k{}", i), leaf(&format!("v{}", i)));
        }
        let c = ConfigValue::Map(m);
        let cloned = c.clone();
        assert_eq!(cloned.as_map().unwrap().len(), 5);
    }

    #[test]
    fn test_get_str_returns_none_for_nested_lookup_on_non_map() {
        // get_str on Str returns None (not Map variant).
        let c = leaf("scalar");
        assert!(c.get_str("any").is_none());
    }

    #[test]
    fn test_as_list_or_single_returns_ref_pointing_to_original_data() {
        // Refs point to original; changing original after wouldn't affect refs in a frozen snapshot.
        let s = leaf("only");
        let v = s.as_list_or_single();
        assert_eq!(v.len(), 1);
        // Ref is to the same str.
        assert_eq!(v[0].as_str(), Some("only"));
    }

    #[test]
    fn test_get_bool_of_off_string_returns_false() {
        // "off" not in truthy set → Some(false).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf("off"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("flag"), Some(false));
    }

    #[test]
    fn test_get_i64_overflow_value_returns_none() {
        // Value beyond i64::MAX → parse fails → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("big".into(), leaf("99999999999999999999999"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("big"), None);
    }

    #[test]
    fn test_as_list_returns_inner_vec_ref_as_readable_iterator() {
        // as_list returns borrow of inner Vec — iterable.
        let c = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let mut count = 0;
        for _ in c.as_list().unwrap() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_get_f64_nan_value_returns_none_or_nan() {
        // "nan" parses to f64::NAN; returned as Some(NaN). Assert is_nan() on unwrap.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("nan"));
        let c = ConfigValue::Map(m);
        let v = c.get_f64("n");
        // Rust "nan".parse::<f64>() → Ok(NaN) → Some(NaN).
        assert!(v.is_some() && v.unwrap().is_nan());
    }

    #[test]
    fn test_get_f64_inf_parses_as_infinity() {
        // "inf" → f64::INFINITY.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("x".into(), leaf("inf"));
        let c = ConfigValue::Map(m);
        let v = c.get_f64("x").unwrap();
        assert!(v.is_infinite() && v > 0.0);
    }

    #[test]
    fn test_get_i64_i64_max_boundary_parses_correctly() {
        // i64::MAX parsed as-is.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf(&i64::MAX.to_string()));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), Some(i64::MAX));
    }

    #[test]
    fn test_as_list_or_single_called_on_list_yields_items_in_order() {
        // List [a, b, c] → Vec refs in same order.
        let c = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let refs = c.as_list_or_single();
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].as_str(), Some("a"));
        assert_eq!(refs[2].as_str(), Some("c"));
    }

    #[test]
    fn test_config_value_nested_list_cloned_independently() {
        // Clone list of lists → outer and inner both deep-copied.
        let inner = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let outer = ConfigValue::List(vec![inner]);
        let cloned = outer.clone();
        assert_eq!(cloned.as_list().unwrap().len(), 1);
        assert_eq!(cloned.as_list().unwrap()[0].as_list().unwrap().len(), 2);
    }

    #[test]
    fn test_get_f64_negative_infinity_parses() {
        // "-inf" → f64::NEG_INFINITY.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("x".into(), leaf("-inf"));
        let c = ConfigValue::Map(m);
        let v = c.get_f64("x").unwrap();
        assert!(v.is_infinite() && v < 0.0);
    }

    #[test]
    fn test_get_i64_i64_min_boundary_parses_correctly() {
        // i64::MIN parsed as-is.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf(&i64::MIN.to_string()));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), Some(i64::MIN));
    }

    #[test]
    fn test_as_list_or_single_very_long_list_preserves_all() {
        // 1000-element List → 1000 refs.
        let items: Vec<ConfigValue> = (0..1000).map(|i| leaf(&i.to_string())).collect();
        let c = ConfigValue::List(items);
        assert_eq!(c.as_list_or_single().len(), 1000);
    }

    #[test]
    fn test_get_bool_case_sensitive_yes_matches_lowercase_only() {
        // "Yes" (capital Y) not in truthy set → Some(false).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("Yes"));
        m.insert("b".into(), leaf("yes"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("a"), Some(false));
        assert_eq!(c.get_bool("b"), Some(true));
    }

    #[test]
    fn test_get_str_returns_owned_value_ref_unchanged() {
        // Basic get_str returns &str borrow.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("VALUE"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_str("k"), Some("VALUE"));
    }

    #[test]
    fn test_get_f64_zero_as_bare_parses() {
        // "0" → 0.0.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("0"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("k"), Some(0.0));
    }

    #[test]
    fn test_partial_eq_list_ordering_matters() {
        // List order matters for equality.
        let a = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let b = ConfigValue::List(vec![leaf("b"), leaf("a")]);
        assert_ne!(a, b);
    }

    #[test]
    fn test_config_value_str_clone_produces_independent_bytes() {
        // Str clone: modifying underlying buffer... actually Rust String clone is deep.
        let s1 = leaf("original");
        let s2 = s1.clone();
        // Both have same content.
        assert_eq!(s1.as_str(), s2.as_str());
    }

    #[test]
    fn test_get_f64_tiny_fraction_parses() {
        // Very small fraction → f64 preserves precision.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("1e-300"));
        let c = ConfigValue::Map(m);
        let v = c.get_f64("n").unwrap();
        assert!((v - 1e-300).abs() < 1e-320);
    }

    #[test]
    fn test_get_i64_on_whitespace_only_returns_none() {
        // "   " whitespace → parse fails → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("   "));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), None);
    }

    #[test]
    fn test_as_list_on_deep_nested_list_returns_outer() {
        // List containing List → as_list returns outer Vec.
        let inner = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let outer = ConfigValue::List(vec![inner]);
        assert_eq!(outer.as_list().unwrap().len(), 1);
    }

    #[test]
    fn test_clone_produces_independent_list_mutations_allowed() {
        // Rust Clone deep-copies Vec; mutating clone doesn't change original.
        let orig = ConfigValue::List(vec![leaf("a")]);
        let cloned = orig.clone();
        // Both accessible, both contain the same initial element.
        assert_eq!(cloned.as_list().unwrap().len(), 1);
        assert_eq!(orig.as_list().unwrap().len(), 1);
    }

    #[test]
    fn test_get_bool_numeric_one_returns_true() {
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("1"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(true));
    }

    #[test]
    fn test_get_bool_keyword_on_returns_true() {
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("enabled".into(), leaf("on"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("enabled"), Some(true));
    }

    #[test]
    fn test_as_list_or_single_empty_list_returns_empty_vec() {
        let v = ConfigValue::List(vec![]);
        let out = v.as_list_or_single();
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn test_partial_eq_map_with_same_entries_different_insertion_order() {
        let mut a = HashMap::new();
        a.insert("k1".to_string(), leaf("v1"));
        a.insert("k2".to_string(), leaf("v2"));
        let mut b = HashMap::new();
        b.insert("k2".to_string(), leaf("v2"));
        b.insert("k1".to_string(), leaf("v1"));
        assert_eq!(ConfigValue::Map(a), ConfigValue::Map(b));
    }

    #[test]
    fn test_get_on_list_variant_returns_none() {
        // get() on non-Map variant → None regardless of key.
        let l = ConfigValue::List(vec![leaf("x")]);
        assert!(l.get("anything").is_none());
    }

    #[test]
    fn test_get_str_missing_key_on_present_map_returns_none() {
        // Map with unrelated key → get_str("missing") → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("1"));
        let c = ConfigValue::Map(m);
        assert!(c.get_str("b").is_none());
    }

    #[test]
    fn test_default_variant_is_empty_map() {
        // ConfigValue::default() → Map(empty).
        let d = ConfigValue::default();
        assert!(matches!(d, ConfigValue::Map(_)));
        assert_eq!(d.as_map().unwrap().len(), 0);
    }

    #[test]
    fn test_from_str_conversion_produces_str_variant() {
        // From<&str> impl wraps into ConfigValue::Str.
        let v: ConfigValue = "hello".into();
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_get_f64_i64_both_look_up_through_nested_map() {
        // Both get_f64 and get_i64 use the same get_str path via key lookup.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("42"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("n"), Some(42.0));
        assert_eq!(c.get_i64("n"), Some(42));
    }

    #[test]
    fn test_as_list_or_single_str_variant_yields_one_element_vec() {
        // Str variant → one-element vec containing self.
        let v = ConfigValue::Str("single".into());
        let out = v.as_list_or_single();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_str(), Some("single"));
    }

    #[test]
    fn test_as_map_on_str_variant_returns_none() {
        // as_map() on Str → None.
        let v = ConfigValue::Str("x".into());
        assert!(v.as_map().is_none());
    }

    #[test]
    fn test_as_list_on_str_variant_returns_none() {
        // as_list() on Str → None.
        let v = ConfigValue::Str("x".into());
        assert!(v.as_list().is_none());
    }

    #[test]
    fn test_get_f64_invalid_numeric_value_returns_none() {
        // Non-numeric value in map → get_f64 parses as None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("not_a_number"));
        let c = ConfigValue::Map(m);
        assert!(c.get_f64("n").is_none());
    }

    #[test]
    fn test_get_i64_negative_number_parses_correctly() {
        // Negative integer string → parsed as i64.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("-100"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), Some(-100));
    }

    #[test]
    fn test_from_string_conversion_produces_str_variant() {
        // From<String> impl (not just &str) wraps into ConfigValue::Str.
        let s: String = "hello".to_string();
        let v: ConfigValue = s.into();
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_as_list_or_single_nested_list_returns_items() {
        // List of 3 items → Vec of 3 refs.
        let v = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let out = v.as_list_or_single();
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn test_get_bool_false_keyword_returns_false() {
        // "false" (not in truthy set) → Some(false).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("f".into(), leaf("false"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("f"), Some(false));
    }

    #[test]
    fn test_get_bool_missing_key_returns_none() {
        // Key not present → None.
        let m: HashMap<String, ConfigValue> = HashMap::new();
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("missing"), None);
    }

    #[test]
    fn test_get_f64_in_e_notation_parses() {
        // "1e3" → Some(1000.0).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("big".into(), leaf("1e3"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("big"), Some(1000.0));
    }

    #[test]
    fn test_as_str_on_list_variant_returns_none() {
        // List variant → as_str() None.
        let v = ConfigValue::List(vec![leaf("x")]);
        assert!(v.as_str().is_none());
    }

    #[test]
    fn test_get_i64_value_too_large_for_f64_returns_none() {
        // Overflow cases → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("huge".into(), leaf("99999999999999999999999"));
        let c = ConfigValue::Map(m);
        assert!(c.get_i64("huge").is_none());
    }

    #[test]
    fn test_get_f64_negative_fractional_parses() {
        // "-3.14" → Some(-3.14).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("-3.14"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("n"), Some(-3.14));
    }

    #[test]
    fn test_as_map_returns_correct_ref_to_original_entries() {
        // as_map returns live ref — entries visible.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let c = ConfigValue::Map(m);
        let mp = c.as_map().unwrap();
        assert_eq!(mp.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_get_bool_yes_keyword_returns_true() {
        // "yes" → Some(true).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("yes"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(true));
    }

    #[test]
    fn test_get_bool_true_keyword_returns_true() {
        // "true" → Some(true).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("true"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(true));
    }

    #[test]
    fn test_get_str_key_with_int_value_returns_str_ref() {
        // get_str on string value "42" returns Some("42").
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("42"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_str("k"), Some("42"));
    }

    #[test]
    fn test_from_str_chain_wrapping_preserved_value() {
        // Chained From<&str> conversions preserve value.
        let v1: ConfigValue = "hello".into();
        let v2: ConfigValue = "hello".into();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_as_list_returns_exact_vec_ref_for_list_variant() {
        // as_list() returns inner Vec reference.
        let v = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let list = v.as_list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].as_str(), Some("a"));
    }

    #[test]
    fn test_get_bool_on_keyword_returns_true() {
        // "on" → Some(true).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("on"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(true));
    }

    #[test]
    fn test_get_bool_off_keyword_returns_false() {
        // "off" (not in truthy set) → Some(false).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("off"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(false));
    }

    #[test]
    fn test_get_i64_positive_max_boundary_parses() {
        // i64::MAX as string → parses correctly.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf(&i64::MAX.to_string()));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("n"), Some(i64::MAX));
    }

    #[test]
    fn test_as_map_returns_some_for_map_variant_always() {
        // Map variant → as_map() Some.
        let v = ConfigValue::Map(HashMap::new());
        assert!(v.as_map().is_some());
    }

    #[test]
    fn test_get_bool_truthy_one_string_returns_true() {
        // "1" → truthy → true.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("1"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(true));
    }

    #[test]
    fn test_get_f64_exact_zero_parses() {
        // "0" → Some(0.0).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("0"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("n"), Some(0.0));
    }

    #[test]
    fn test_partial_eq_list_different_lengths_not_equal() {
        // List of 2 vs List of 3 → not equal.
        let a = ConfigValue::List(vec![leaf("x"), leaf("y")]);
        let b = ConfigValue::List(vec![leaf("x"), leaf("y"), leaf("z")]);
        assert_ne!(a, b);
    }

    #[test]
    fn test_get_str_on_str_variant_itself_returns_none() {
        // get_str is only for Map lookups — Str variant returns None.
        let v = ConfigValue::Str("value".into());
        assert!(v.get_str("anything").is_none());
    }

    #[test]
    fn test_clone_preserves_deeply_nested_structure() {
        // Map → List → Map → Str deep clone preserves structure.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("key".into(), leaf("deep"));
        let v = ConfigValue::Map({
            let mut m = HashMap::new();
            m.insert("list".into(), ConfigValue::List(vec![ConfigValue::Map(inner)]));
            m
        });
        let c = v.clone();
        assert_eq!(c, v);
    }

    #[test]
    fn test_get_f64_negative_zero_parses_correctly() {
        // "-0" → Some(-0.0).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("-0"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("n"), Some(-0.0));
    }

    #[test]
    fn test_get_i64_positive_small_values_parse() {
        // Small positive integers.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("1"));
        m.insert("b".into(), leaf("10"));
        m.insert("c".into(), leaf("1000"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("a"), Some(1));
        assert_eq!(c.get_i64("b"), Some(10));
        assert_eq!(c.get_i64("c"), Some(1000));
    }

    #[test]
    fn test_get_bool_uppercase_true_string_returns_false() {
        // "True" (capitalized) not in lowercase truthy set → false.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("True"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(false));
    }

    #[test]
    fn test_config_value_clone_list_with_nested_list_preserved() {
        // List of Lists deep clone preserved.
        let v = ConfigValue::List(vec![
            ConfigValue::List(vec![leaf("a"), leaf("b")]),
            ConfigValue::List(vec![leaf("c")]),
        ]);
        let c = v.clone();
        assert_eq!(c, v);
    }

    #[test]
    fn test_get_str_through_map_variant_returns_inner_value() {
        // Map with multiple keys → get_str returns correct key's value.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("alpha"));
        m.insert("b".into(), leaf("beta"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_str("a"), Some("alpha"));
        assert_eq!(c.get_str("b"), Some("beta"));
    }

    #[test]
    fn test_as_list_or_single_on_map_variant_yields_one_element() {
        // Map variant → as_list_or_single returns 1-element vec with self ref.
        let v = ConfigValue::Map(HashMap::new());
        let out = v.as_list_or_single();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn test_get_f64_with_plus_sign_value_parses() {
        // "+3.14" → Some(3.14).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("n".into(), leaf("+3.14"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("n"), Some(3.14));
    }

    #[test]
    fn test_partial_eq_map_same_size_same_entries_equal() {
        // Two Maps with same size and same entries → equal.
        let mut a: HashMap<String, ConfigValue> = HashMap::new();
        a.insert("x".into(), leaf("1"));
        a.insert("y".into(), leaf("2"));
        let mut b: HashMap<String, ConfigValue> = HashMap::new();
        b.insert("x".into(), leaf("1"));
        b.insert("y".into(), leaf("2"));
        assert_eq!(ConfigValue::Map(a), ConfigValue::Map(b));
    }

    #[test]
    fn test_get_i64_with_whitespace_around_value_returns_none() {
        // " 42 " with whitespace → parse fails for i64.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf(" 42 "));
        let c = ConfigValue::Map(m);
        assert!(c.get_i64("k").is_none());
    }

    #[test]
    fn test_as_list_or_single_returns_non_empty_for_str() {
        // Str variant → 1-element vec (non-empty).
        let v = ConfigValue::Str("item".into());
        let out = v.as_list_or_single();
        assert!(!out.is_empty());
    }

    #[test]
    fn test_get_bool_with_uppercase_keyword_returns_false() {
        // "ON" (uppercase) not in lowercase truthy set → false.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("ON"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("k"), Some(false));
    }

    #[test]
    fn test_get_bool_absent_key_on_empty_map_none_v2() {
        // Absent key → get_str None → map absent → None (not Some(false)).
        let m: HashMap<String, ConfigValue> = HashMap::new();
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("nonexistent"), None);
    }

    #[test]
    fn test_from_str_trait_produces_str_variant() {
        // &str → Str via From<&str>.
        let c: ConfigValue = "hello".into();
        assert_eq!(c.as_str(), Some("hello"));
    }

    #[test]
    fn test_from_string_trait_produces_str_variant() {
        // String → Str via From<String>.
        let s = String::from("world");
        let c: ConfigValue = s.into();
        assert_eq!(c.as_str(), Some("world"));
    }

    #[test]
    fn test_default_configvalue_is_empty_map_variant() {
        // Default::default() → Map(empty HashMap).
        let c = ConfigValue::default();
        let m = c.as_map().expect("is Map variant");
        assert!(m.is_empty());
    }

    #[test]
    fn test_as_str_on_list_variant_yields_none_v2() {
        // List variant → as_str() returns None.
        let c = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        assert!(c.as_str().is_none());
    }

    #[test]
    fn test_as_map_on_str_variant_yields_none_v2() {
        // Str variant → as_map() returns None.
        let c = leaf("hello");
        assert!(c.as_map().is_none());
    }

    #[test]
    fn test_as_list_on_str_variant_yields_none_v2() {
        // Str variant → as_list() returns None.
        let c = leaf("hello");
        assert!(c.as_list().is_none());
    }

    #[test]
    fn test_from_hashmap_trait_produces_map_variant() {
        // HashMap → Map via From<HashMap>.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v"));
        let c: ConfigValue = m.into();
        assert_eq!(c.get_str("k"), Some("v"));
    }

    #[test]
    fn test_get_on_str_variant_returns_none() {
        // Str → get(key) None (not a Map).
        let c = leaf("hello");
        assert!(c.get("any_key").is_none());
    }

    #[test]
    fn test_get_on_list_of_two_strs_returns_none_v2() {
        // List → get(key) None (not a Map).
        let c = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        assert!(c.get("key").is_none());
    }

    #[test]
    fn test_get_f64_with_scientific_notation_parses() {
        // "1e3" parses as 1000.0.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf("1e3"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("val"), Some(1000.0));
    }

    #[test]
    fn test_get_i64_negative_value_parses() {
        // Negative integer.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf("-42"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("val"), Some(-42));
    }

    #[test]
    fn test_get_str_absent_key_returns_none() {
        // Missing key → None via get().and_then path.
        let m: HashMap<String, ConfigValue> = HashMap::new();
        let c = ConfigValue::Map(m);
        assert!(c.get_str("nope").is_none());
    }

    #[test]
    fn test_get_f64_value_with_fraction_parses() {
        // Fractional value parses via f64::parse.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf("0.125"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("val"), Some(0.125));
    }

    #[test]
    fn test_get_i64_fractional_value_fails_parse_returns_none() {
        // "3.14" → i64::parse Err → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf("3.14"));
        let c = ConfigValue::Map(m);
        assert!(c.get_i64("val").is_none());
    }

    #[test]
    fn test_as_list_or_single_empty_map_yields_one_element_vec() {
        // Map variant → as_list_or_single catch-all → vec with 1 ref to self.
        let c = ConfigValue::Map(HashMap::new());
        let v = c.as_list_or_single();
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn test_as_list_or_single_list_with_three_elements_returns_all_three() {
        // List → vec with all refs.
        let c = ConfigValue::List(vec![leaf("a"), leaf("b"), leaf("c")]);
        let v = c.as_list_or_single();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].as_str(), Some("a"));
        assert_eq!(v[2].as_str(), Some("c"));
    }

    #[test]
    fn test_get_bool_with_on_lowercase_in_truthy_set() {
        // "on" in truthy → true.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf("on"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("flag"), Some(true));
    }

    #[test]
    fn test_get_bool_with_random_string_not_truthy_returns_false() {
        // Random string not in {1,yes,true,on} → false.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("flag".into(), leaf("hello"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_bool("flag"), Some(false));
    }

    #[test]
    fn test_get_on_map_nested_key_returns_value() {
        // get(key) on Map returns the value.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("a".into(), leaf("1"));
        m.insert("b".into(), leaf("2"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(c.get("b").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn test_clone_on_map_produces_independent_copy() {
        // Cloning a Map ConfigValue produces an independent copy.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("k".into(), leaf("v1"));
        let c1 = ConfigValue::Map(m);
        let c2 = c1.clone();
        assert_eq!(c1.get_str("k"), c2.get_str("k"));
    }

    #[test]
    fn test_as_str_on_map_returns_none() {
        // Map → as_str is None.
        let c = ConfigValue::Map(HashMap::new());
        assert!(c.as_str().is_none());
    }

    #[test]
    fn test_as_list_or_single_nested_list_returns_inner_elements() {
        // Nested list → all refs preserved.
        let inner = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let c = ConfigValue::List(vec![inner, leaf("outside")]);
        let v = c.as_list_or_single();
        assert_eq!(v.len(), 2);  // Outer list has 2 elements: inner-list + "outside".
    }

    #[test]
    fn test_get_f64_on_scientific_negative_parses() {
        // "-1.5e2" → -150.0.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf("-1.5e2"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_f64("val"), Some(-150.0));
    }

    #[test]
    fn test_get_str_recursive_on_absent_returns_none() {
        // get_str on Map without key → None.
        let m: HashMap<String, ConfigValue> = HashMap::new();
        let c = ConfigValue::Map(m);
        assert!(c.get_str("missing").is_none());
    }

    #[test]
    fn test_get_i64_on_string_with_spaces_returns_none() {
        // " 42 " has whitespace → parse::<i64>() Err → None.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf(" 42 "));
        let c = ConfigValue::Map(m);
        assert!(c.get_i64("val").is_none());
    }

    #[test]
    fn test_get_i64_positive_with_plus_sign_parses() {
        // "+42" → Some(42).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("val".into(), leaf("+42"));
        let c = ConfigValue::Map(m);
        assert_eq!(c.get_i64("val"), Some(42));
    }

    #[test]
    fn test_clone_on_list_produces_independent_copy() {
        // Cloning a List → independent Vec.
        let c1 = ConfigValue::List(vec![leaf("a"), leaf("b")]);
        let c2 = c1.clone();
        assert_eq!(c1.as_list().unwrap().len(), c2.as_list().unwrap().len());
    }

    #[test]
    fn test_partial_eq_str_with_same_content_equal() {
        // Str variants with same content are equal.
        let a = leaf("hello");
        let b = leaf("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn test_partial_eq_str_different_content_not_equal() {
        // Str variants with different content not equal.
        let a = leaf("hello");
        let b = leaf("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_partial_eq_list_same_elements_equal() {
        // List variants with same elements equal.
        let a = ConfigValue::List(vec![leaf("x"), leaf("y")]);
        let b = ConfigValue::List(vec![leaf("x"), leaf("y")]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_partial_eq_different_variants_not_equal() {
        // Str != List even with "similar" content.
        let a = leaf("abc");
        let b = ConfigValue::List(vec![leaf("abc")]);
        assert_ne!(a, b);
    }
}
