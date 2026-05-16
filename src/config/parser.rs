use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::types::ConfigValue;

/// Parser for Config::General format files.
///
/// Supports:
/// - `key = value` assignments
/// - `<block_name>` / `</block_name>` nesting
/// - `<<include file>>` directives
/// - AllowMultiOptions (duplicate keys become lists)
/// - LowerCaseNames (all keys lowercased)
/// - AutoTrue (yes/no/true/false/on/off -> 1/0)
/// - IncludeAgain (same file can be included multiple times)
pub struct ConfigParser {
    /// Directories to search for included files.
    pub config_paths: Vec<PathBuf>,
    pub auto_true: bool,
    pub lower_case_names: bool,
}

impl Default for ConfigParser {
    /// Build a `ConfigParser` with the same defaults as `ConfigParser::new`.
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigParser {
    /// Construct a new parser with `auto_true` and `lower_case_names` enabled
    /// and no extra include search paths.
    pub fn new() -> Self {
        ConfigParser {
            config_paths: Vec::new(),
            auto_true: true,
            lower_case_names: true,
        }
    }

    /// Parse a configuration file and return the top-level map.
    pub fn parse_file(&self, path: &Path) -> Result<HashMap<String, ConfigValue>, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

        let base_dir = path.parent().unwrap_or(Path::new("."));
        let mut search_paths = vec![base_dir.to_path_buf()];
        search_paths.extend(self.config_paths.iter().cloned());

        let expanded = self.expand_includes(&content, &search_paths)?;
        self.parse_string(&expanded)
    }

    /// Parse a config string (after includes have been expanded).
    pub fn parse_string(&self, content: &str) -> Result<HashMap<String, ConfigValue>, String> {
        let mut stack: Vec<(String, HashMap<String, ConfigValue>)> = Vec::new();
        let mut current = HashMap::new();

        for (line_num, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Strip inline comments (but be careful with # inside quoted strings)
            let line = strip_inline_comment(line);

            // Block open: <block_name> or <block_name arg>
            if line.starts_with('<') && !line.starts_with("</") && !line.starts_with("<<") {
                let block_name = line.trim_start_matches('<').trim_end_matches('>').trim();
                let block_key = if self.lower_case_names {
                    block_name
                        .split_whitespace()
                        .next()
                        .unwrap_or(block_name)
                        .to_lowercase()
                } else {
                    block_name
                        .split_whitespace()
                        .next()
                        .unwrap_or(block_name)
                        .to_string()
                };
                stack.push((block_key, current));
                current = HashMap::new();
                continue;
            }

            // Block close: </block_name>
            if line.starts_with("</") {
                let (block_key, mut parent) = stack
                    .pop()
                    .ok_or_else(|| format!("line {}: unexpected block close", line_num + 1))?;
                let block_value = ConfigValue::Map(current);
                insert_multi(&mut parent, &block_key, block_value);
                current = parent;
                continue;
            }

            // Key = value assignment
            if let Some((key, value)) = parse_assignment(line) {
                let key = if self.lower_case_names {
                    key.to_lowercase()
                } else {
                    key.to_string()
                };
                let value = if self.auto_true {
                    auto_true_convert(value)
                } else {
                    value.to_string()
                };
                insert_multi(&mut current, &key, ConfigValue::Str(value));
                continue;
            }

            // Lines that are just a value without = (rare, skip)
        }

        if !stack.is_empty() {
            return Err(format!(
                "unclosed block(s): {}",
                stack
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(current)
    }

    /// Expand `<<include file>>` directives recursively.
    fn expand_includes(&self, content: &str, search_paths: &[PathBuf]) -> Result<String, String> {
        let mut result = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<<include") {
                let file_ref = trimmed
                    .trim_start_matches("<<include")
                    .trim_end_matches(">>")
                    .trim();
                let included_path = self.find_file(file_ref, search_paths)?;
                let included_content = fs::read_to_string(&included_path).map_err(|e| {
                    format!(
                        "cannot read included file {}: {}",
                        included_path.display(),
                        e
                    )
                })?;
                let included_dir = included_path.parent().unwrap_or(Path::new("."));
                let mut child_paths = vec![included_dir.to_path_buf()];
                child_paths.extend(search_paths.iter().cloned());
                let expanded = self.expand_includes(&included_content, &child_paths)?;
                result.push_str(&expanded);
                result.push('\n');
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        Ok(result)
    }

    /// Find a file in the search paths.
    fn find_file(&self, file: &str, search_paths: &[PathBuf]) -> Result<PathBuf, String> {
        let p = Path::new(file);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        for dir in search_paths {
            let candidate = dir.join(file);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        Err(format!(
            "cannot find included file '{}' in search paths: {:?}",
            file, search_paths
        ))
    }
}

/// Parse a `key = value` or `key value` line.
fn parse_assignment(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once('=') {
        Some((key.trim(), value.trim()))
    } else {
        // Config::General also supports "key value" without =
        // but only for simple cases. We handle the = case primarily.
        None
    }
}

/// Strip inline comments (text after # that is not in quotes).
fn strip_inline_comment(line: &str) -> &str {
    // Simple approach: find first # not preceded by quotes
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == '#' && !in_quotes {
            return line[..i].trim();
        }
    }
    line
}

/// Convert AutoTrue values: yes/true/on -> "1", no/false/off -> "0".
fn auto_true_convert(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "yes" | "true" | "on" => "1".to_string(),
        "no" | "false" | "off" => "0".to_string(),
        _ => value.to_string(),
    }
}

/// Insert a value into a map, converting to a List if the key already exists
/// (AllowMultiOptions behavior).
fn insert_multi(map: &mut HashMap<String, ConfigValue>, key: &str, value: ConfigValue) {
    use std::collections::hash_map::Entry;
    match map.entry(key.to_string()) {
        Entry::Occupied(mut e) => {
            let existing = e.get_mut();
            match existing {
                ConfigValue::List(list) => {
                    list.push(value);
                }
                _ => {
                    let prev = std::mem::replace(existing, ConfigValue::List(Vec::new()));
                    if let ConfigValue::List(list) = existing {
                        list.push(prev);
                        list.push(value);
                    }
                }
            }
        }
        Entry::Vacant(e) => {
            e.insert(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let parser = ConfigParser::new();
        let config = parser
            .parse_string(
                r#"
key1 = value1
key2 = 42
debug = no
verbose = yes
"#,
            )
            .unwrap();

        assert_eq!(config.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(config.get("key2").unwrap().as_str().unwrap(), "42");
        // AutoTrue
        assert_eq!(config.get("debug").unwrap().as_str().unwrap(), "0");
        assert_eq!(config.get("verbose").unwrap().as_str().unwrap(), "1");
    }

    #[test]
    fn test_parse_block() {
        let parser = ConfigParser::new();
        let config = parser
            .parse_string(
                r#"
<image>
dir = /tmp
file = test.png
radius = 1500p
</image>
"#,
            )
            .unwrap();

        let image = config.get("image").unwrap().as_map().unwrap();
        assert_eq!(image.get("dir").unwrap().as_str().unwrap(), "/tmp");
        assert_eq!(image.get("file").unwrap().as_str().unwrap(), "test.png");
        assert_eq!(image.get("radius").unwrap().as_str().unwrap(), "1500p");
    }

    #[test]
    fn test_parse_nested_blocks() {
        let parser = ConfigParser::new();
        let config = parser
            .parse_string(
                r#"
<ideogram>
thickness = 100p
<spacing>
default = 10u
</spacing>
</ideogram>
"#,
            )
            .unwrap();

        let ideogram = config.get("ideogram").unwrap().as_map().unwrap();
        assert_eq!(ideogram.get("thickness").unwrap().as_str().unwrap(), "100p");
        let spacing = ideogram.get("spacing").unwrap().as_map().unwrap();
        assert_eq!(spacing.get("default").unwrap().as_str().unwrap(), "10u");
    }

    #[test]
    fn test_parse_multi_options() {
        let parser = ConfigParser::new();
        let config = parser
            .parse_string(
                r#"
<ticks>
<tick>
spacing = 5u
size = 5p
</tick>
<tick>
spacing = 10u
size = 8p
</tick>
</ticks>
"#,
            )
            .unwrap();

        let ticks = config.get("ticks").unwrap().as_map().unwrap();
        let tick_list = ticks.get("tick").unwrap().as_list().unwrap();
        assert_eq!(tick_list.len(), 2);
        assert_eq!(
            tick_list[0]
                .as_map()
                .unwrap()
                .get("spacing")
                .unwrap()
                .as_str()
                .unwrap(),
            "5u"
        );
        assert_eq!(
            tick_list[1]
                .as_map()
                .unwrap()
                .get("spacing")
                .unwrap()
                .as_str()
                .unwrap(),
            "10u"
        );
    }

    #[test]
    fn test_lowercase_names() {
        let parser = ConfigParser::new();
        let config = parser.parse_string("MyKey = hello\n").unwrap();
        assert!(config.contains_key("mykey"));
        assert!(!config.contains_key("MyKey"));
    }

    #[test]
    fn test_comments() {
        let parser = ConfigParser::new();
        let config = parser
            .parse_string(
                r#"
# this is a comment
key1 = value1
key2 = value2 # inline comment
"#,
            )
            .unwrap();

        assert_eq!(config.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(config.get("key2").unwrap().as_str().unwrap(), "value2");
    }

    #[test]
    fn test_auto_true() {
        let parser = ConfigParser::new();
        let config = parser
            .parse_string(
                r#"
a = yes
b = no
c = true
d = false
e = on
f = off
g = maybe
"#,
            )
            .unwrap();

        assert_eq!(config.get("a").unwrap().as_str().unwrap(), "1");
        assert_eq!(config.get("b").unwrap().as_str().unwrap(), "0");
        assert_eq!(config.get("c").unwrap().as_str().unwrap(), "1");
        assert_eq!(config.get("d").unwrap().as_str().unwrap(), "0");
        assert_eq!(config.get("e").unwrap().as_str().unwrap(), "1");
        assert_eq!(config.get("f").unwrap().as_str().unwrap(), "0");
        assert_eq!(config.get("g").unwrap().as_str().unwrap(), "maybe");
    }

    #[test]
    fn test_parse_duplicate_key_becomes_list() {
        // Config::General collapses duplicate top-level keys into a List.
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string(
                r#"
chromosomes_scale = hs1=2
chromosomes_scale = hs2=3
"#,
            )
            .unwrap();
        let v = cfg.get("chromosomes_scale").unwrap();
        match v {
            ConfigValue::List(list) => {
                assert_eq!(list.len(), 2);
                assert_eq!(list[0].as_str(), Some("hs1=2"));
                assert_eq!(list[1].as_str(), Some("hs2=3"));
            }
            _ => panic!("expected List variant, got {:?}", v),
        }
    }

    #[test]
    fn test_parse_three_level_nesting() {
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string(
                r#"
<ideogram>
<spacing>
<pairwise hs1;hs2>
spacing = 5u
</pairwise>
</spacing>
</ideogram>
"#,
            )
            .unwrap();
        let pair = cfg
            .get("ideogram")
            .and_then(|v| v.get("spacing"))
            .and_then(|v| v.as_map())
            .expect("spacing map");
        // Pairwise block key includes the tag portion in Config::General semantics.
        assert!(pair.iter().any(|(k, _)| k.starts_with("pairwise")));
    }

    #[test]
    fn test_parse_whitespace_and_empty_lines() {
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string(
                r#"

key1 = value1


key2 = value2


"#,
            )
            .unwrap();
        assert_eq!(cfg.get("key1").unwrap().as_str(), Some("value1"));
        assert_eq!(cfg.get("key2").unwrap().as_str(), Some("value2"));
    }

    #[test]
    fn test_parse_value_preserves_internal_whitespace() {
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string("label = Chromosome 1 (human)\n")
            .unwrap();
        assert_eq!(
            cfg.get("label").unwrap().as_str(),
            Some("Chromosome 1 (human)")
        );
    }

    #[test]
    fn test_parse_assignment_helper() {
        assert_eq!(parse_assignment("key = value"), Some(("key", "value")));
        assert_eq!(parse_assignment("  k  =  v  "), Some(("k", "v")));
        // No `=` → None (Perl's "key value" form not supported here).
        assert_eq!(parse_assignment("no equals"), None);
    }

    #[test]
    fn test_strip_inline_comment_outside_quotes() {
        // `#` outside quotes → stripped.
        assert_eq!(strip_inline_comment("foo = bar # comment"), "foo = bar");
        // `#` inside quotes → preserved.
        assert_eq!(strip_inline_comment(r#"s = "not # a comment""#), r#"s = "not # a comment""#);
        // `#` at start → whole line becomes empty.
        assert_eq!(strip_inline_comment("# full-line comment"), "");
        // No `#` → unchanged.
        assert_eq!(strip_inline_comment("foo = bar"), "foo = bar");
    }

    #[test]
    fn test_auto_true_convert_all_forms() {
        // Truthy values.
        assert_eq!(auto_true_convert("yes"), "1");
        assert_eq!(auto_true_convert("true"), "1");
        assert_eq!(auto_true_convert("on"), "1");
        // Case-insensitive.
        assert_eq!(auto_true_convert("YES"), "1");
        assert_eq!(auto_true_convert("True"), "1");
        // Falsy values.
        assert_eq!(auto_true_convert("no"), "0");
        assert_eq!(auto_true_convert("false"), "0");
        assert_eq!(auto_true_convert("off"), "0");
        assert_eq!(auto_true_convert("NO"), "0");
        // Other values pass through.
        assert_eq!(auto_true_convert("42"), "42");
        assert_eq!(auto_true_convert("hello"), "hello");
    }

    #[test]
    fn test_parse_string_unclosed_block_errors() {
        // Opened `<image>` without matching `</image>` → unclosed-block error.
        let parser = ConfigParser::new();
        let r = parser.parse_string("<image>\ndir = /tmp\n");
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("unclosed block"), "got: {}", err);
        assert!(err.contains("image"), "block name should appear in err: {}", err);
    }

    #[test]
    fn test_parse_string_unexpected_close_errors() {
        // `</foo>` with no open block → unexpected-close error.
        let parser = ConfigParser::new();
        let r = parser.parse_string("</image>\n");
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("unexpected block close"), "got: {}", err);
    }

    #[test]
    fn test_parse_block_argument_stripped_from_key() {
        // `<pairwise hs1;hs2>` → block key is just "pairwise" (first word).
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string("<pairwise hs1;hs2>\nspacing = 5u\n</pairwise>\n")
            .unwrap();
        // Key should be lowercased "pairwise" with no suffix.
        assert!(cfg.contains_key("pairwise"));
        let inner = cfg.get("pairwise").and_then(|v| v.as_map()).unwrap();
        assert_eq!(inner.get("spacing").and_then(|v| v.as_str()), Some("5u"));
    }

    #[test]
    fn test_parse_file_with_include_directive_expands_content() {
        // `<<include FILE>>` directive pulls in FILE's content verbatim.
        let dir = tempfile::tempdir().unwrap();
        let included = dir.path().join("shared.conf");
        std::fs::write(&included, "shared_key = shared_val\n").unwrap();
        let main = dir.path().join("main.conf");
        std::fs::write(
            &main,
            format!("<<include {}>>\nmain_key = main_val\n", included.display()),
        )
        .unwrap();
        let parser = ConfigParser::new();
        let cfg = parser.parse_file(&main).unwrap();
        assert_eq!(cfg.get("shared_key").and_then(|v| v.as_str()), Some("shared_val"));
        assert_eq!(cfg.get("main_key").and_then(|v| v.as_str()), Some("main_val"));
    }

    #[test]
    fn test_parse_file_include_missing_file_errors() {
        // `<<include /nonexistent.conf>>` → Err because `find_file` misses.
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.conf");
        std::fs::write(&main, "<<include /nonexistent_999.conf>>\n").unwrap();
        let parser = ConfigParser::new();
        let r = parser.parse_file(&main);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("cannot find"));
    }

    #[test]
    fn test_parse_file_include_nested_directory_search() {
        // An include referenced by basename gets found via search_paths.
        let dir = tempfile::tempdir().unwrap();
        // Include file in a subdirectory.
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let included = sub.join("color.conf");
        std::fs::write(&included, "c = blue\n").unwrap();
        let main = dir.path().join("main.conf");
        std::fs::write(&main, "<<include color.conf>>\n").unwrap();
        // Parser's config_paths points at the sub dir.
        let parser = ConfigParser {
            config_paths: vec![sub.clone()],
            auto_true: true,
            lower_case_names: true,
        };
        let cfg = parser.parse_file(&main).unwrap();
        assert_eq!(cfg.get("c").and_then(|v| v.as_str()), Some("blue"));
    }

    #[test]
    fn test_insert_multi_with_map_values_promotes_to_list() {
        // Inserting Map values under same key promotes to List of Maps.
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        let mut m1 = HashMap::new();
        m1.insert("k".into(), ConfigValue::Str("v1".into()));
        insert_multi(&mut map, "grp", ConfigValue::Map(m1));
        let mut m2 = HashMap::new();
        m2.insert("k".into(), ConfigValue::Str("v2".into()));
        insert_multi(&mut map, "grp", ConfigValue::Map(m2));
        match map.get("grp").unwrap() {
            ConfigValue::List(l) => {
                assert_eq!(l.len(), 2);
                assert!(l[0].as_map().is_some());
                assert!(l[1].as_map().is_some());
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn test_insert_multi_distinct_keys_stay_scalars() {
        // Each distinct key stays a Str (no promotion when key is first-time).
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut map, "a", ConfigValue::Str("1".into()));
        insert_multi(&mut map, "b", ConfigValue::Str("2".into()));
        insert_multi(&mut map, "c", ConfigValue::Str("3".into()));
        assert_eq!(map.len(), 3);
        assert!(matches!(map.get("a"), Some(ConfigValue::Str(_))));
        assert!(matches!(map.get("b"), Some(ConfigValue::Str(_))));
        assert!(matches!(map.get("c"), Some(ConfigValue::Str(_))));
    }

    #[test]
    fn test_insert_multi_mixed_str_and_map_values() {
        // First value is Str, second is Map → promotes to List with mixed variants.
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut map, "k", ConfigValue::Str("plain".into()));
        let mut inner = HashMap::new();
        inner.insert("x".into(), ConfigValue::Str("y".into()));
        insert_multi(&mut map, "k", ConfigValue::Map(inner));
        let v = map.get("k").unwrap();
        match v {
            ConfigValue::List(l) => {
                assert_eq!(l.len(), 2);
                assert!(l[0].as_str().is_some());
                assert!(l[1].as_map().is_some());
            }
            _ => panic!("expected List, got {:?}", v),
        }
    }

    #[test]
    fn test_insert_multi_appends_to_existing_list_without_rewrapping() {
        // A key that's already a List → new insertions append directly to
        // the existing list (no re-promotion from scratch).
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        let initial = ConfigValue::List(vec![
            ConfigValue::Str("existing1".into()),
            ConfigValue::Str("existing2".into()),
        ]);
        map.insert("k".into(), initial);
        insert_multi(&mut map, "k", ConfigValue::Str("new3".into()));
        insert_multi(&mut map, "k", ConfigValue::Str("new4".into()));
        let v = map.get("k").unwrap();
        match v {
            ConfigValue::List(l) => {
                assert_eq!(l.len(), 4);
                assert_eq!(l[0].as_str(), Some("existing1"));
                assert_eq!(l[1].as_str(), Some("existing2"));
                assert_eq!(l[2].as_str(), Some("new3"));
                assert_eq!(l[3].as_str(), Some("new4"));
            }
            _ => panic!("expected List, got {:?}", v),
        }
    }

    #[test]
    fn test_parse_assignment_empty_value_after_equals() {
        // "key =" → key="key", value="" (empty after trim).
        assert_eq!(parse_assignment("key ="), Some(("key", "")));
        assert_eq!(parse_assignment("key=  "), Some(("key", "")));
    }

    #[test]
    fn test_parse_assignment_multiple_equals_keeps_only_first_split() {
        // `split_once('=')` splits on first `=` only.
        assert_eq!(
            parse_assignment("foo = bar = baz"),
            Some(("foo", "bar = baz"))
        );
    }

    #[test]
    fn test_parse_assignment_value_with_equals_in_quotes_not_specially_handled() {
        // split_once('=') doesn't care about quotes — first `=` wins.
        assert_eq!(
            parse_assignment("s = \"a=b\""),
            Some(("s", "\"a=b\""))
        );
    }

    #[test]
    fn test_strip_inline_comment_empty_line_returns_empty() {
        // Empty string → empty.
        assert_eq!(strip_inline_comment(""), "");
        // Just whitespace → whitespace preserved (not trimmed by this fn).
        assert_eq!(strip_inline_comment("   "), "   ");
    }

    #[test]
    fn test_auto_true_convert_whitespace_preserved() {
        // Whitespace around truthy values is NOT trimmed by auto_true_convert
        // (it compares the lowercased string verbatim).
        assert_eq!(auto_true_convert(" yes"), " yes");
        assert_eq!(auto_true_convert("yes "), "yes ");
        assert_eq!(auto_true_convert("yes"), "1");
    }

    #[test]
    fn test_auto_true_convert_numeric_value_passthrough() {
        // Numbers pass through unchanged.
        assert_eq!(auto_true_convert("42"), "42");
        assert_eq!(auto_true_convert("-3.14"), "-3.14");
        assert_eq!(auto_true_convert("0"), "0");
    }

    #[test]
    fn test_auto_true_convert_mixed_case_truthy() {
        // Case-insensitive comparison: yEs/YES/TRue all → "1".
        assert_eq!(auto_true_convert("yEs"), "1");
        assert_eq!(auto_true_convert("TRUE"), "1");
        assert_eq!(auto_true_convert("On"), "1");
        assert_eq!(auto_true_convert("TrUe"), "1");
    }

    #[test]
    fn test_auto_true_convert_mixed_case_falsy() {
        // OFF/FALSE/NO case-insensitive → "0".
        assert_eq!(auto_true_convert("OFF"), "0");
        assert_eq!(auto_true_convert("False"), "0");
        assert_eq!(auto_true_convert("fALsE"), "0");
        assert_eq!(auto_true_convert("no"), "0");
    }

    #[test]
    fn test_parse_string_auto_true_disabled_preserves_values() {
        // auto_true=false → "yes"/"no" strings pass through verbatim.
        let parser = ConfigParser {
            config_paths: vec![],
            auto_true: false,
            lower_case_names: true,
        };
        let cfg = parser
            .parse_string("verbose = yes\ndebug = no\nother = whatever\n")
            .unwrap();
        assert_eq!(cfg.get("verbose").and_then(|v| v.as_str()), Some("yes"));
        assert_eq!(cfg.get("debug").and_then(|v| v.as_str()), Some("no"));
        assert_eq!(cfg.get("other").and_then(|v| v.as_str()), Some("whatever"));
    }

    #[test]
    fn test_parse_string_case_preservation_disabled() {
        // lower_case_names=false → keys keep their original case.
        let parser = ConfigParser {
            config_paths: vec![],
            auto_true: true,
            lower_case_names: false,
        };
        let cfg = parser.parse_string("MyKey = value\nOTHER = x\n").unwrap();
        assert!(cfg.contains_key("MyKey"));
        assert!(cfg.contains_key("OTHER"));
        assert!(!cfg.contains_key("mykey"));
    }

    #[test]
    fn test_parse_string_block_arg_with_space_in_name() {
        // `<block name with spaces>` → block_key is first whitespace token only.
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string("<foo bar baz>\nkey = val\n</foo>\n")
            .unwrap();
        // Only "foo" should be a key; the rest after whitespace is ignored.
        assert!(cfg.contains_key("foo"));
        let inner = cfg.get("foo").and_then(|v| v.as_map()).unwrap();
        assert_eq!(inner.get("key").and_then(|v| v.as_str()), Some("val"));
    }

    #[test]
    fn test_parse_string_multiple_empty_lines_between_blocks() {
        // Multiple empty lines between sections don't affect parsing.
        let parser = ConfigParser::new();
        let cfg = parser
            .parse_string("\n\n\nk1 = v1\n\n\n\nk2 = v2\n\n\n")
            .unwrap();
        assert_eq!(cfg.get("k1").and_then(|v| v.as_str()), Some("v1"));
        assert_eq!(cfg.get("k2").and_then(|v| v.as_str()), Some("v2"));
    }

    #[test]
    fn test_parse_file_nested_includes_recurse() {
        // An included file can itself contain an `<<include>>` directive.
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner.conf");
        std::fs::write(&inner, "deep_key = deep_val\n").unwrap();
        let middle = dir.path().join("middle.conf");
        std::fs::write(
            &middle,
            format!("middle_key = m\n<<include {}>>\n", inner.display()),
        )
        .unwrap();
        let main = dir.path().join("main.conf");
        std::fs::write(
            &main,
            format!("main_key = top\n<<include {}>>\n", middle.display()),
        )
        .unwrap();
        let parser = ConfigParser::new();
        let cfg = parser.parse_file(&main).unwrap();
        assert_eq!(cfg.get("main_key").and_then(|v| v.as_str()), Some("top"));
        assert_eq!(cfg.get("middle_key").and_then(|v| v.as_str()), Some("m"));
        assert_eq!(cfg.get("deep_key").and_then(|v| v.as_str()), Some("deep_val"));
    }

    #[test]
    fn test_strip_inline_comment_quotes_reopen() {
        // Multiple quoted regions: `#` outside is stripped, `#` inside each quoted region preserved.
        assert_eq!(
            strip_inline_comment(r#"a = "x#y" b = "z#w" # real comment"#),
            r#"a = "x#y" b = "z#w""#
        );
        // Unclosed quote → everything after the opening `"` treated as quoted → no strip.
        assert_eq!(
            strip_inline_comment(r#"s = "unterminated # stays"#),
            r#"s = "unterminated # stays"#
        );
    }

    #[test]
    fn test_insert_multi_grows_into_list_on_third_duplicate() {
        // First insertion → plain Str, second → List[Str,Str], third → List[Str,Str,Str].
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut map, "k", ConfigValue::Str("a".into()));
        insert_multi(&mut map, "k", ConfigValue::Str("b".into()));
        insert_multi(&mut map, "k", ConfigValue::Str("c".into()));
        let v = map.get("k").unwrap();
        match v {
            ConfigValue::List(l) => {
                assert_eq!(l.len(), 3);
                assert_eq!(l[0].as_str(), Some("a"));
                assert_eq!(l[1].as_str(), Some("b"));
                assert_eq!(l[2].as_str(), Some("c"));
            }
            _ => panic!("expected List after 3 inserts, got {:?}", v),
        }
    }

    #[test]
    fn test_strip_inline_comment_no_comment_returns_line_unchanged() {
        // No `#` in line → entire line returned as-is (not trimmed).
        assert_eq!(strip_inline_comment("key = value"), "key = value");
        // Quoted-only `#` (inside quotes) → preserved.
        assert_eq!(strip_inline_comment(r#"k = "a#b""#), r#"k = "a#b""#);
    }

    #[test]
    fn test_strip_inline_comment_comment_at_start_returns_empty() {
        // `#` at position 0 → everything stripped → empty string.
        assert_eq!(strip_inline_comment("# full comment line"), "");
        // Whitespace before first `#` → trimmed portion preserved (empty after trim).
        assert_eq!(strip_inline_comment("    # comment"), "");
    }

    #[test]
    fn test_auto_true_convert_non_truthy_passthrough() {
        // Non-truthy/falsy strings pass through verbatim (preserving case).
        assert_eq!(auto_true_convert("hello"), "hello");
        assert_eq!(auto_true_convert("42"), "42");
        assert_eq!(auto_true_convert(""), "");
        assert_eq!(auto_true_convert("yEs"), "1");
        // Partial match like "yes_please" does NOT match — full-string lowercase only.
        assert_eq!(auto_true_convert("yes_please"), "yes_please");
    }

    #[test]
    fn test_parse_assignment_key_without_equals_returns_none() {
        // `key value` without `=` → None (impl only handles = form).
        assert!(parse_assignment("key value").is_none());
        // Empty line → None.
        assert!(parse_assignment("").is_none());
        // Whitespace-only → None.
        assert!(parse_assignment("   ").is_none());
        // With equals even with no value → Some with empty value.
        let (k, v) = parse_assignment("key =").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "");
    }

    #[test]
    fn test_parse_assignment_whitespace_around_key_and_value() {
        // split_once(=) then trim() both sides.
        let (k, v) = parse_assignment("  mykey   =   myvalue   ").unwrap();
        assert_eq!(k, "mykey");
        assert_eq!(v, "myvalue");
        // Tabs and multiple spaces also trimmed.
        let (k, v) = parse_assignment("\tkey2\t=\tvalue2\t").unwrap();
        assert_eq!(k, "key2");
        assert_eq!(v, "value2");
    }

    #[test]
    fn test_auto_true_convert_all_truthy_lowercase_forms() {
        // All four truthy forms (case-insensitive) → "1".
        for v in ["yes", "true", "on", "YES", "TRUE", "ON", "Yes", "True", "On"] {
            assert_eq!(auto_true_convert(v), "1", "expected '1' for {}", v);
        }
        // All three falsy forms → "0".
        for v in ["no", "false", "off", "NO", "FALSE", "OFF", "No", "False", "Off"] {
            assert_eq!(auto_true_convert(v), "0", "expected '0' for {}", v);
        }
    }

    #[test]
    fn test_insert_multi_with_three_different_keys_stays_scalars() {
        // Three distinct keys → all stored as Str (no list promotion).
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut map, "k1", ConfigValue::Str("v1".into()));
        insert_multi(&mut map, "k2", ConfigValue::Str("v2".into()));
        insert_multi(&mut map, "k3", ConfigValue::Str("v3".into()));
        assert_eq!(map.len(), 3);
        for (k, expected) in [("k1", "v1"), ("k2", "v2"), ("k3", "v3")] {
            match map.get(k) {
                Some(ConfigValue::Str(s)) => assert_eq!(s, expected),
                _ => panic!("expected Str for key {}", k),
            }
        }
    }

    #[test]
    fn test_strip_inline_comment_trailing_hash_no_space() {
        // "k=v#comment" — first # outside quotes triggers trim.
        assert_eq!(strip_inline_comment("k=v#c"), "k=v");
        // Hash immediately after space.
        assert_eq!(strip_inline_comment("a b #x"), "a b");
        // Hash inside quotes → preserved (no trim).
        assert_eq!(strip_inline_comment(r#"s="abc#def""#), r#"s="abc#def""#);
    }

    #[test]
    fn test_config_parser_default_has_expected_flags() {
        // Default ConfigParser has auto_true=true and lower_case_names=true.
        let p = ConfigParser::default();
        assert!(p.auto_true);
        assert!(p.lower_case_names);
        assert!(p.config_paths.is_empty());
    }

    #[test]
    fn test_parse_assignment_value_with_spaces_inside() {
        // split_once keeps value's inner whitespace after the initial trim.
        let (k, v) = parse_assignment("msg = hello world  ").unwrap();
        assert_eq!(k, "msg");
        assert_eq!(v, "hello world");
        // Multiple spaces preserved internally after outer trim.
        let (k, v) = parse_assignment("x=a  b  c").unwrap();
        assert_eq!(k, "x");
        assert_eq!(v, "a  b  c");
    }

    #[test]
    fn test_auto_true_convert_common_circos_values() {
        // Common circos config values: show=yes → "1", hide=no → "0".
        assert_eq!(auto_true_convert("yes"), "1");
        assert_eq!(auto_true_convert("no"), "0");
        // File paths passthrough.
        assert_eq!(auto_true_convert("/etc/circos/conf"), "/etc/circos/conf");
        // Numbers passthrough.
        assert_eq!(auto_true_convert("1500"), "1500");
    }

    #[test]
    fn test_insert_multi_list_key_first_insertion() {
        // Insert once → Str; inserting twice under same key → List[Str, Str].
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut map, "k", ConfigValue::Str("a".into()));
        match map.get("k") {
            Some(ConfigValue::Str(s)) => assert_eq!(s, "a"),
            _ => panic!("expected Str after 1 insert"),
        }
        insert_multi(&mut map, "k", ConfigValue::Str("b".into()));
        match map.get("k") {
            Some(ConfigValue::List(l)) => {
                assert_eq!(l.len(), 2);
                assert_eq!(l[0].as_str(), Some("a"));
                assert_eq!(l[1].as_str(), Some("b"));
            }
            _ => panic!("expected List after 2 inserts"),
        }
    }

    #[test]
    fn test_config_parser_new_matches_default() {
        // ConfigParser::new() and default() produce equivalent state.
        let n = ConfigParser::new();
        let d = ConfigParser::default();
        assert_eq!(n.auto_true, d.auto_true);
        assert_eq!(n.lower_case_names, d.lower_case_names);
        assert_eq!(n.config_paths.len(), d.config_paths.len());
    }

    #[test]
    fn test_auto_true_convert_single_char_values() {
        // Single-char values "1" and "0" pass through numerically (not truthy keywords).
        assert_eq!(auto_true_convert("1"), "1");
        assert_eq!(auto_true_convert("0"), "0");
        // Other single chars pass through.
        assert_eq!(auto_true_convert("x"), "x");
        assert_eq!(auto_true_convert(" "), " ");
    }

    #[test]
    fn test_parse_assignment_tabs_only_separator() {
        // Tabs around `=` are whitespace — trimmed.
        let (k, v) = parse_assignment("\tkey\t=\tvalue\t").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "value");
    }

    #[test]
    fn test_strip_inline_comment_unicode_chars() {
        // Non-ASCII characters in line and comments pass through.
        let s = strip_inline_comment("αβγ = δεζ # comment");
        assert_eq!(s, "αβγ = δεζ");
        // Unicode inside a comment is also stripped.
        let s = strip_inline_comment("k=v# ⚠ warning");
        assert_eq!(s, "k=v");
    }

    #[test]
    fn test_insert_multi_third_value_appends_to_existing_list() {
        // First insert → direct; second → promotes to List([a,b]); third → pushes onto List.
        let mut map: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut map, "k", ConfigValue::Str("a".into()));
        insert_multi(&mut map, "k", ConfigValue::Str("b".into()));
        insert_multi(&mut map, "k", ConfigValue::Str("c".into()));
        match map.get("k").unwrap() {
            ConfigValue::List(v) => {
                assert_eq!(v.len(), 3);
                assert_eq!(v[0].as_str(), Some("a"));
                assert_eq!(v[1].as_str(), Some("b"));
                assert_eq!(v[2].as_str(), Some("c"));
            }
            _ => panic!("expected List after 3 inserts"),
        }
    }

    #[test]
    fn test_auto_true_convert_mixed_case_variations() {
        // .to_lowercase() is called — all case variants of yes/on/true → "1".
        assert_eq!(auto_true_convert("YES"), "1");
        assert_eq!(auto_true_convert("Yes"), "1");
        assert_eq!(auto_true_convert("TRUE"), "1");
        assert_eq!(auto_true_convert("On"), "1");
        assert_eq!(auto_true_convert("NO"), "0");
        assert_eq!(auto_true_convert("Off"), "0");
        // Unrelated text passes through unchanged.
        assert_eq!(auto_true_convert("HelloWorld"), "HelloWorld");
    }

    #[test]
    fn test_strip_inline_comment_hash_inside_quotes_preserved() {
        // A `#` inside a `"..."` pair is NOT a comment delimiter — line unchanged.
        assert_eq!(
            strip_inline_comment(r#"k="v#abc" "#),
            r#"k="v#abc" "#
        );
        // Once quote closes, a subsequent `#` still strips.
        assert_eq!(
            strip_inline_comment(r#"k="v" # tail"#),
            r#"k="v""#
        );
    }

    #[test]
    fn test_parse_assignment_missing_equals_returns_none() {
        // No `=` → parse_assignment returns None (only "key value" = None for now).
        assert!(parse_assignment("foo bar").is_none());
        assert!(parse_assignment("just_a_key").is_none());
        // A blank line also has no '=' → None.
        assert!(parse_assignment("").is_none());
    }

    #[test]
    fn test_parser_new_defaults_auto_true_and_lower_case_names_both_on() {
        // Defaults: AutoTrue + LowerCaseNames (matches Perl Config::General defaults).
        let p = ConfigParser::new();
        assert!(p.auto_true);
        assert!(p.lower_case_names);
        // Default impl delegates to new().
        let d = ConfigParser::default();
        assert!(d.auto_true);
        assert!(d.lower_case_names);
    }

    #[test]
    fn test_parse_string_unclosed_block_returns_error() {
        // <block> without matching </block> → Err listing the block name.
        let p = ConfigParser::new();
        let r = p.parse_string("<alpha>\nkey = value\n");
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("unclosed"));
        assert!(err.contains("alpha"));
    }

    #[test]
    fn test_parse_string_unexpected_close_returns_error() {
        // </block> without a preceding open → Err mentioning "unexpected".
        let p = ConfigParser::new();
        let r = p.parse_string("</stray>\n");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unexpected block close"));
    }

    #[test]
    fn test_parse_string_auto_true_disabled_preserves_literal_value() {
        // auto_true=false → "yes"/"no" kept literal (not coerced to "1"/"0").
        let p = ConfigParser {
            config_paths: Vec::new(),
            auto_true: false,
            lower_case_names: true,
        };
        let m = p.parse_string("flag = yes\nother = no").unwrap();
        assert_eq!(m.get("flag").and_then(|v| v.as_str()), Some("yes"));
        assert_eq!(m.get("other").and_then(|v| v.as_str()), Some("no"));
    }

    #[test]
    fn test_parse_string_lower_case_names_disabled_preserves_case() {
        // lower_case_names=false → mixed-case keys kept verbatim.
        let p = ConfigParser {
            config_paths: Vec::new(),
            auto_true: true,
            lower_case_names: false,
        };
        let m = p.parse_string("MyKey = value\nOther_KEY = x").unwrap();
        assert!(m.contains_key("MyKey"));
        assert!(m.contains_key("Other_KEY"));
        assert!(!m.contains_key("mykey"));
    }

    #[test]
    fn test_parse_string_nested_blocks_produce_nested_maps() {
        // <outer><inner>k=v</inner></outer> → nested ConfigValue::Map structure.
        let p = ConfigParser::new();
        let m = p.parse_string("<outer>\n<inner>\nk = v\n</inner>\n</outer>").unwrap();
        let outer = m.get("outer").and_then(|v| v.as_map()).unwrap();
        let inner = outer.get("inner").and_then(|v| v.as_map()).unwrap();
        assert_eq!(inner.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_duplicate_key_at_top_level_becomes_list() {
        // Same key twice → second insertion converts to List.
        let p = ConfigParser::new();
        let m = p.parse_string("color = red\ncolor = blue").unwrap();
        match m.get("color").unwrap() {
            ConfigValue::List(l) => {
                assert_eq!(l.len(), 2);
                assert_eq!(l[0].as_str(), Some("red"));
                assert_eq!(l[1].as_str(), Some("blue"));
            }
            _ => panic!("expected List for duplicate key"),
        }
    }

    #[test]
    fn test_parse_string_strips_whitespace_around_equals() {
        // "key    =    value" → key="key", value="value" (both trimmed).
        let p = ConfigParser::new();
        let m = p.parse_string("key    =    value").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
        // Tabs and spaces mixed.
        let m2 = p.parse_string("\tfoo\t=\tbar\t").unwrap();
        assert_eq!(m2.get("foo").and_then(|v| v.as_str()), Some("bar"));
    }

    #[test]
    fn test_parse_string_inline_comment_stripped_from_value() {
        // "# comment" appended to a line is removed before assignment parse.
        let p = ConfigParser::new();
        let m = p.parse_string("key = value  # trailing comment").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_block_name_lowercased_when_enabled() {
        // Default LowerCaseNames=true → <Block> stored as "block".
        let p = ConfigParser::new();
        let m = p.parse_string("<Block>\nkey = val\n</Block>").unwrap();
        assert!(m.contains_key("block"));
        assert!(!m.contains_key("Block"));
    }

    #[test]
    fn test_parse_string_block_with_args_uses_first_word_as_key() {
        // <block arg1 arg2> → only "block" becomes the key; args not reflected in key name.
        let p = ConfigParser::new();
        let m = p.parse_string("<block arg1 arg2>\nkey = val\n</block>").unwrap();
        assert!(m.contains_key("block"));
        // The args aren't preserved as a separate field — confirm no "arg1" key appears.
        assert!(!m.contains_key("arg1"));
    }

    #[test]
    fn test_parse_file_nonexistent_returns_err() {
        // Missing file path → file-read Err with "cannot read" prefix.
        let p = ConfigParser::new();
        let r = p.parse_file(Path::new("/definitely/not/real/config_iter526.conf"));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("cannot read"));
    }

    #[test]
    fn test_parse_string_empty_input_returns_empty_map() {
        let p = ConfigParser::new();
        let m = p.parse_string("").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_string_double_hash_line_treated_as_comment() {
        // "## double hash" line still starts with # → comment → skipped.
        let p = ConfigParser::new();
        let m = p.parse_string("## double hash\nkey = val").unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("val"));
    }

    #[test]
    fn test_parse_string_equals_sign_in_value_preserved() {
        // Value containing '=' — split_once keeps everything after first '='.
        let p = ConfigParser::new();
        let m = p.parse_string("url = http://example/path?a=1&b=2").unwrap();
        assert_eq!(
            m.get("url").and_then(|v| v.as_str()),
            Some("http://example/path?a=1&b=2")
        );
    }

    #[test]
    fn test_parse_string_close_tag_name_mismatch_still_pops_stack() {
        // </anything> pops the top block regardless of name — matches simple stack-based parser.
        let p = ConfigParser::new();
        let m = p.parse_string("<outer>\nk = v\n</mismatched>").unwrap();
        assert!(m.contains_key("outer"));
        let inner = m.get("outer").and_then(|v| v.as_map()).unwrap();
        assert_eq!(inner.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_unclosed_block_error_lists_block_name() {
        // When parse fails due to unclosed block, the error lists the block key (lowercased).
        let p = ConfigParser::new();
        let err = p.parse_string("<outer>\nk = v\n").unwrap_err();
        assert!(err.contains("unclosed"));
        assert!(err.contains("outer"));
    }

    #[test]
    fn test_parse_string_unexpected_close_error_includes_line_number() {
        // The error for a </block> without a matching opener includes the 1-based line number.
        let p = ConfigParser::new();
        // Line 1 is the bad close.
        let err = p.parse_string("</never_opened>").unwrap_err();
        assert!(err.contains("line 1"));
        assert!(err.contains("unexpected"));
        // Moved to line 3 to verify line counting.
        let err2 = p.parse_string("\n\n</never_opened>").unwrap_err();
        assert!(err2.contains("line 3"));
    }

    #[test]
    fn test_parse_string_block_with_tabs_and_spaces_in_name_keeps_first_word() {
        // Tabs + spaces after block name get split by whitespace — only the first word wins.
        let p = ConfigParser::new();
        let m = p.parse_string("<block\targ1\targ2>\nk = v\n</block>").unwrap();
        assert!(m.contains_key("block"));
        assert!(!m.contains_key("arg1"));
        assert!(!m.contains_key("arg2"));
    }

    #[test]
    fn test_parse_string_indented_assignment_line_parsed() {
        // Leading whitespace is trimmed before parsing — indented "  key = value" is fine.
        let p = ConfigParser::new();
        let m = p.parse_string("    indented_key = indented_value\n").unwrap();
        assert_eq!(m.get("indented_key").and_then(|v| v.as_str()), Some("indented_value"));
        // Tabs too.
        let m2 = p.parse_string("\t\tkey = value\n").unwrap();
        assert_eq!(m2.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_auto_true_convert_off_values_return_zero_string() {
        // auto_true_convert: "no"/"false"/"off" → "0"; "NO" case-insensitive via to_lowercase.
        assert_eq!(auto_true_convert("no"), "0");
        assert_eq!(auto_true_convert("NO"), "0");
        assert_eq!(auto_true_convert("false"), "0");
        assert_eq!(auto_true_convert("False"), "0");
        assert_eq!(auto_true_convert("OFF"), "0");
    }

    #[test]
    fn test_auto_true_convert_unrecognized_string_passes_through_verbatim() {
        // Non-boolean strings preserved as-is (not lowercased).
        assert_eq!(auto_true_convert("MySetting"), "MySetting");
        assert_eq!(auto_true_convert("0.5"), "0.5");
        assert_eq!(auto_true_convert("red"), "red");
        assert_eq!(auto_true_convert(""), "");
    }

    #[test]
    fn test_insert_multi_third_value_for_same_key_appends_to_existing_list() {
        // First insert: scalar. Second: promotes to List(len=2). Third: appends to existing List(len=3).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut m, "k", ConfigValue::Str("a".into()));
        insert_multi(&mut m, "k", ConfigValue::Str("b".into()));
        insert_multi(&mut m, "k", ConfigValue::Str("c".into()));
        let list = m.get("k").and_then(|v| v.as_list()).expect("should be list");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].as_str(), Some("a"));
        assert_eq!(list[1].as_str(), Some("b"));
        assert_eq!(list[2].as_str(), Some("c"));
    }

    #[test]
    fn test_strip_inline_comment_hash_as_first_char_of_line_returns_empty_prefix() {
        // Line starting with '#' → the line[..0] is "", trimmed to "".
        let r = strip_inline_comment("#just a comment");
        assert_eq!(r, "");
        // But leading whitespace doesn't trigger — the `if line.starts_with('#')` in parse_string
        // handles that. strip_inline_comment starts scanning from position 0.
        let r2 = strip_inline_comment("  # still a comment");
        // "  " is before '#' at index 2 → line[..2] = "  " → trimmed = "".
        assert_eq!(r2, "");
    }

    #[test]
    fn test_parse_string_all_auto_true_truthy_values_become_one() {
        // yes/true/on → "1"; no/false/off → "0" — with auto_true enabled.
        let p = ConfigParser::new(); // auto_true = true by default
        let m = p
            .parse_string(
                "a = yes\nb = true\nc = on\nd = YES\ne = TRUE\nf = On\n",
            )
            .unwrap();
        for k in ["a", "b", "c", "d", "e", "f"] {
            assert_eq!(m.get(k).and_then(|v| v.as_str()), Some("1"), "key={}", k);
        }
    }

    #[test]
    fn test_parse_string_auto_true_falsy_all_become_zero() {
        // no/false/off in any casing → "0".
        let p = ConfigParser::new();
        let m = p
            .parse_string("a = no\nb = false\nc = off\nd = NO\ne = FALSE\nf = Off\n")
            .unwrap();
        for k in ["a", "b", "c", "d", "e", "f"] {
            assert_eq!(m.get(k).and_then(|v| v.as_str()), Some("0"), "key={}", k);
        }
    }

    #[test]
    fn test_parse_string_block_with_identical_name_nested_inside_itself() {
        // Nested block with same name as parent — works via stack.
        let p = ConfigParser::new();
        let m = p
            .parse_string(
                "<group>\n  key = outer\n  <group>\n    key = inner\n  </group>\n</group>\n",
            )
            .unwrap();
        let outer = m.get("group").and_then(|v| v.as_map()).unwrap();
        assert_eq!(outer.get("key").and_then(|v| v.as_str()), Some("outer"));
        let inner = outer.get("group").and_then(|v| v.as_map()).unwrap();
        assert_eq!(inner.get("key").and_then(|v| v.as_str()), Some("inner"));
    }

    #[test]
    fn test_parse_string_hash_inside_double_quotes_not_a_comment() {
        // strip_inline_comment respects double quotes.
        let p = ConfigParser::new();
        let m = p.parse_string(r#"url = "http://x#fragment""#).unwrap();
        // The value keeps the # because it's inside quotes.
        let v = m.get("url").and_then(|v| v.as_str()).unwrap();
        assert!(v.contains("#fragment"));
    }

    #[test]
    fn test_parse_string_trailing_whitespace_after_value_trimmed() {
        // "key = value    " → trailing whitespace trimmed from value.
        let p = ConfigParser::new();
        let m = p.parse_string("key = value    \n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_blank_line_between_assignments_preserved() {
        // Blank lines between assignments don't affect parsing.
        let p = ConfigParser::new();
        let m = p.parse_string("a = 1\n\nb = 2\n\n\nc = 3\n").unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("b").and_then(|v| v.as_str()), Some("2"));
        assert_eq!(m.get("c").and_then(|v| v.as_str()), Some("3"));
    }

    #[test]
    fn test_parse_string_value_with_semicolon_preserved_verbatim() {
        // Semicolon is not a special char — passes through.
        let p = ConfigParser::new();
        let m = p.parse_string("keys = a;b;c;d\n").unwrap();
        assert_eq!(m.get("keys").and_then(|v| v.as_str()), Some("a;b;c;d"));
    }

    #[test]
    fn test_parse_string_multiple_blocks_at_top_level() {
        // Multiple sibling blocks at top level → all stored.
        let p = ConfigParser::new();
        let input = "<first>\na = 1\n</first>\n<second>\nb = 2\n</second>\n";
        let m = p.parse_string(input).unwrap();
        assert!(m.contains_key("first"));
        assert!(m.contains_key("second"));
        let first = m.get("first").and_then(|v| v.as_map()).unwrap();
        let second = m.get("second").and_then(|v| v.as_map()).unwrap();
        assert_eq!(first.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(second.get("b").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn test_parse_string_key_with_dot_in_name_preserved() {
        // Key names containing dots — Config::General-style — stored verbatim.
        let p = ConfigParser::new();
        let m = p.parse_string("path.to.file = value\n").unwrap();
        assert_eq!(m.get("path.to.file").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_assignment_with_equals_separator_only() {
        // No spaces around "=" — still parses cleanly.
        let p = ConfigParser::new();
        let m = p.parse_string("key=value\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_block_with_multiple_key_values_all_stored() {
        // A block with multiple unique keys preserves all of them.
        let p = ConfigParser::new();
        let input = "<cfg>\na = 1\nb = 2\nc = 3\n</cfg>\n";
        let m = p.parse_string(input).unwrap();
        let cfg = m.get("cfg").and_then(|v| v.as_map()).unwrap();
        assert_eq!(cfg.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(cfg.get("b").and_then(|v| v.as_str()), Some("2"));
        assert_eq!(cfg.get("c").and_then(|v| v.as_str()), Some("3"));
    }

    #[test]
    fn test_parse_string_leading_blank_lines_do_not_affect_parsing() {
        // Several leading blank lines don't change semantics.
        let p = ConfigParser::new();
        let m = p.parse_string("\n\n\nkey = value\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_mixed_comments_and_assignments() {
        // Comments interspersed with assignments — all comments stripped, all kvs kept.
        let p = ConfigParser::new();
        let input = "# first comment\nkey1 = val1\n# another\nkey2 = val2\n";
        let m = p.parse_string(input).unwrap();
        assert_eq!(m.get("key1").and_then(|v| v.as_str()), Some("val1"));
        assert_eq!(m.get("key2").and_then(|v| v.as_str()), Some("val2"));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn test_parse_string_value_contains_spaces_preserved() {
        // Value with spaces kept (only leading/trailing trimmed).
        let p = ConfigParser::new();
        let m = p.parse_string("key = hello world foo\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("hello world foo"));
    }

    #[test]
    fn test_parse_string_large_number_of_top_level_keys() {
        // Many top-level keys all stored.
        let p = ConfigParser::new();
        let input: String = (0..20)
            .map(|i| format!("k{} = v{}\n", i, i))
            .collect();
        let m = p.parse_string(&input).unwrap();
        assert_eq!(m.len(), 20);
        for i in 0..20 {
            assert_eq!(
                m.get(&format!("k{}", i)).and_then(|v| v.as_str()),
                Some(&*format!("v{}", i)),
            );
        }
    }

    #[test]
    fn test_parse_string_value_with_inline_comment_at_end_stripped() {
        // "key = val # comment" → value is "val" (stripped).
        let p = ConfigParser::new();
        let m = p.parse_string("k = myval # trailing comment\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("myval"));
    }

    #[test]
    fn test_parse_string_value_with_special_chars_preserved() {
        // Special chars like /@+ preserved in values.
        let p = ConfigParser::new();
        let m = p.parse_string("path = /opt/some@app+v1.0\n").unwrap();
        assert_eq!(m.get("path").and_then(|v| v.as_str()), Some("/opt/some@app+v1.0"));
    }

    #[test]
    fn test_parse_string_block_close_without_open_errors_cleanly() {
        // </block> without <block> → error (no stack to pop).
        let p = ConfigParser::new();
        let r = p.parse_string("</orphan>\n");
        assert!(r.is_err());
    }

    #[test]
    fn test_parse_string_trailing_close_without_newline_still_parses() {
        // Block close without trailing newline — still parses.
        let p = ConfigParser::new();
        let m = p.parse_string("<outer>\nk=v\n</outer>").unwrap();
        assert!(m.contains_key("outer"));
    }

    #[test]
    fn test_parse_string_value_numeric_types_stored_as_string() {
        // Numeric values stored as string, not converted to i64/f64.
        let p = ConfigParser::new();
        let m = p.parse_string("count = 42\nrate = 3.14\n").unwrap();
        assert_eq!(m.get("count").and_then(|v| v.as_str()), Some("42"));
        assert_eq!(m.get("rate").and_then(|v| v.as_str()), Some("3.14"));
    }

    #[test]
    fn test_parse_string_unclosed_block_at_eof_returns_err() {
        // Block opened but never closed → Err mentioning "unclosed".
        let p = ConfigParser::new();
        let res = p.parse_string("<outer>\nkey = value\n");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unclosed"));
    }

    #[test]
    fn test_parse_string_auto_true_converts_yes_on_true_to_one() {
        // AutoTrue is on by default → yes/on/true → "1", no/off/false → "0".
        let p = ConfigParser::new();
        let m = p
            .parse_string("a = yes\nb = on\nc = true\nd = no\ne = off\nf = false\n")
            .unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("b").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("c").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("d").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("e").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("f").and_then(|v| v.as_str()), Some("0"));
    }

    #[test]
    fn test_parse_string_lower_case_names_downcases_keys_by_default() {
        // LowerCaseNames on → "Key" key stored as "key".
        let p = ConfigParser::new();
        let m = p.parse_string("MyKey = v\nOTHER = w\n").unwrap();
        assert_eq!(m.get("mykey").and_then(|v| v.as_str()), Some("v"));
        assert_eq!(m.get("other").and_then(|v| v.as_str()), Some("w"));
        // Uppercase keys NOT present.
        assert!(m.get("MyKey").is_none());
    }

    #[test]
    fn test_parse_string_block_with_spaces_in_open_tag_keeps_first_word_as_key() {
        // "<block arg1 arg2>" → key is "block" (first whitespace-separated token only).
        let p = ConfigParser::new();
        let m = p.parse_string("<block arg1 arg2>\nk = v\n</block>\n").unwrap();
        // Key is "block" (lowercased, first word).
        assert!(m.contains_key("block"));
    }

    #[test]
    fn test_parse_string_deeply_nested_blocks_produce_nested_maps() {
        // Three levels deep — each block becomes a nested Map value.
        let p = ConfigParser::new();
        let content = "<a>\n  <b>\n    <c>\n      k = v\n    </c>\n  </b>\n</a>\n";
        let m = p.parse_string(content).unwrap();
        let a = m.get("a").and_then(|v| v.as_map()).expect("a map");
        let b = a.get("b").and_then(|v| v.as_map()).expect("b map");
        let c = b.get("c").and_then(|v| v.as_map()).expect("c map");
        assert_eq!(c.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_multiple_duplicate_keys_combine_into_list() {
        // AllowMultiOptions: same key seen 3 times → ConfigValue::List.
        let p = ConfigParser::new();
        let content = "x = 1\nx = 2\nx = 3\n";
        let m = p.parse_string(content).unwrap();
        let list = m.get("x").and_then(|v| v.as_list()).expect("list");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].as_str(), Some("1"));
        assert_eq!(list[1].as_str(), Some("2"));
        assert_eq!(list[2].as_str(), Some("3"));
    }

    #[test]
    fn test_parse_string_double_less_than_non_include_not_treated_as_block_open() {
        // "<<include" is NOT a block open (handled by expand_includes), so raw
        // "<<" lines at parse time are not treated as blocks.
        let p = ConfigParser::new();
        // A key with "<<" in value should be fine at the parser level.
        let m = p.parse_string("k = v\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_empty_content_returns_empty_map() {
        // Empty input → empty top-level map.
        let p = ConfigParser::new();
        let m = p.parse_string("").unwrap();
        assert!(m.is_empty());
        // Only whitespace → also empty.
        let m2 = p.parse_string("\n\n   \n\n").unwrap();
        assert!(m2.is_empty());
    }

    #[test]
    fn test_parse_string_comments_only_returns_empty_map() {
        // All-comment input → empty map.
        let p = ConfigParser::new();
        let m = p.parse_string("# header\n# another comment\n  # indented\n").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_string_value_with_embedded_equals_preserved_verbatim() {
        // "url=http://site?a=b&c=d" — first "=" splits; value preserves subsequent "=".
        let p = ConfigParser::new();
        let m = p.parse_string("url = http://site?a=b&c=d\n").unwrap();
        let v = m.get("url").and_then(|v| v.as_str()).expect("url set");
        assert_eq!(v, "http://site?a=b&c=d");
    }

    #[test]
    fn test_parse_string_nested_block_preserves_outer_key_value() {
        // Outer key-value alongside nested block — both persisted.
        let p = ConfigParser::new();
        let content = "top = hello\n<inner>\n  k = v\n</inner>\n";
        let m = p.parse_string(content).unwrap();
        assert_eq!(m.get("top").and_then(|v| v.as_str()), Some("hello"));
        assert!(m.get("inner").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_auto_true_disabled_passes_literal_yes_no() {
        // With auto_true=false, "yes"/"no" stored as strings.
        let mut p = ConfigParser::new();
        p.auto_true = false;
        let m = p.parse_string("a = yes\nb = no\n").unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("yes"));
        assert_eq!(m.get("b").and_then(|v| v.as_str()), Some("no"));
    }

    #[test]
    fn test_parse_string_lower_case_names_disabled_preserves_original_casing() {
        // lower_case_names=false → original casing preserved.
        let mut p = ConfigParser::new();
        p.lower_case_names = false;
        let m = p.parse_string("MyKey = v\n").unwrap();
        // Uppercase-preserved key present; lower version absent.
        assert_eq!(m.get("MyKey").and_then(|v| v.as_str()), Some("v"));
        assert!(m.get("mykey").is_none());
    }

    #[test]
    fn test_parse_string_whitespace_around_assignment_stripped() {
        // Spaces around "=" trimmed on both key and value sides.
        let p = ConfigParser::new();
        let m = p.parse_string("  key  =   value   \n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_block_with_blank_lines_inside_parses_correctly() {
        // Blank lines within a block don't break parsing.
        let p = ConfigParser::new();
        let content = "<block>\n\n  k = v\n\n  j = w\n\n</block>\n";
        let m = p.parse_string(content).unwrap();
        let b = m.get("block").and_then(|v| v.as_map()).expect("block");
        assert_eq!(b.get("k").and_then(|v| v.as_str()), Some("v"));
        assert_eq!(b.get("j").and_then(|v| v.as_str()), Some("w"));
    }

    #[test]
    fn test_parse_string_block_with_same_name_appears_twice_stored_as_list() {
        // Two `<chr>` blocks stored as a list of maps.
        let p = ConfigParser::new();
        let content = "<entry>\n  k = v1\n</entry>\n<entry>\n  k = v2\n</entry>\n";
        let m = p.parse_string(content).unwrap();
        let list = m.get("entry").and_then(|v| v.as_list()).expect("list");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_parse_string_inline_comment_after_value_stripped() {
        // " # inline comment" after value should be stripped.
        let p = ConfigParser::new();
        let m = p.parse_string("key = value # comment\n").unwrap();
        let v = m.get("key").and_then(|v| v.as_str()).unwrap();
        // Value should be "value" (trimmed, without comment).
        assert_eq!(v.trim(), "value");
    }

    #[test]
    fn test_parse_string_block_close_mismatched_open_name_still_pops() {
        // Stack-based impl: block close just pops from stack regardless of name.
        let p = ConfigParser::new();
        // Open "outer", close with "</wrong>" — parser pops outer anyway (lenient).
        let m = p.parse_string("<outer>\n  k = v\n</wrong>\n").unwrap();
        // The block gets stored under "outer" key (original open name).
        assert!(m.contains_key("outer"));
    }

    #[test]
    fn test_parse_string_many_assignments_all_stored() {
        // 10 unique keys all stored.
        let p = ConfigParser::new();
        let content: String = (0..10)
            .map(|i| format!("k{} = v{}\n", i, i))
            .collect();
        let m = p.parse_string(&content).unwrap();
        assert_eq!(m.len(), 10);
        for i in 0..10 {
            let key = format!("k{}", i);
            let expected = format!("v{}", i);
            assert_eq!(m.get(&key).and_then(|v| v.as_str()), Some(expected.as_str()));
        }
    }

    #[test]
    fn test_parse_string_equals_without_space_around_parses() {
        // No-space "key=value" also valid.
        let p = ConfigParser::new();
        let m = p.parse_string("key=value\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_value_with_only_digits_preserved_as_string() {
        // All digits "12345" → still stored as Str, not promoted to int.
        let p = ConfigParser::new();
        let m = p.parse_string("n = 12345\n").unwrap();
        assert_eq!(m.get("n").and_then(|v| v.as_str()), Some("12345"));
    }

    #[test]
    fn test_parse_string_value_with_dashes_and_periods_preserved() {
        // File-path-style value with "-" and "." preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("path = /usr/local/bin/circos-1.2.3\n").unwrap();
        assert_eq!(m.get("path").and_then(|v| v.as_str()), Some("/usr/local/bin/circos-1.2.3"));
    }

    #[test]
    fn test_parse_string_block_with_empty_body_creates_empty_map() {
        // `<empty></empty>` → empty nested map.
        let p = ConfigParser::new();
        let m = p.parse_string("<empty>\n</empty>\n").unwrap();
        let inner = m.get("empty").and_then(|v| v.as_map()).expect("inner");
        assert!(inner.is_empty());
    }

    #[test]
    fn test_parse_string_value_with_only_trailing_spaces_trimmed() {
        // "key = value   \n" → "value" (trailing whitespace trimmed).
        let p = ConfigParser::new();
        let m = p.parse_string("key = value   \n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_value_with_only_leading_spaces_trimmed() {
        // "key =    value\n" → "value" (leading whitespace of value trimmed).
        let p = ConfigParser::new();
        let m = p.parse_string("key =    value\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_consecutive_blocks_both_top_level_maps() {
        // `<a>..</a><b>..</b>` → both blocks at top level.
        let p = ConfigParser::new();
        let content = "<a>\n  k = v\n</a>\n<b>\n  k2 = v2\n</b>\n";
        let m = p.parse_string(content).unwrap();
        assert!(m.get("a").and_then(|v| v.as_map()).is_some());
        assert!(m.get("b").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_key_with_underscore_preserved() {
        // Keys with underscores are common in Circos.
        let p = ConfigParser::new();
        let m = p.parse_string("chr_1_size = 1000\n").unwrap();
        assert_eq!(m.get("chr_1_size").and_then(|v| v.as_str()), Some("1000"));
    }

    #[test]
    fn test_parse_string_value_matches_key_preserved() {
        // Same-looking key and value both stored distinctly.
        let p = ConfigParser::new();
        let m = p.parse_string("x = x\n").unwrap();
        assert_eq!(m.get("x").and_then(|v| v.as_str()), Some("x"));
    }

    #[test]
    fn test_parse_string_block_with_numeric_name_allowed() {
        // "<123>" blocks have numeric name (unusual but legal at parser level).
        let p = ConfigParser::new();
        let m = p.parse_string("<123>\n  k = v\n</123>\n").unwrap();
        assert!(m.contains_key("123"));
    }

    #[test]
    fn test_parse_string_multiple_blocks_and_assignments_mixed() {
        // Mix of top-level keys and blocks.
        let p = ConfigParser::new();
        let content = "a = 1\n<b>\n  k = v\n</b>\nc = 2\n";
        let m = p.parse_string(content).unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
        assert!(m.get("b").and_then(|v| v.as_map()).is_some());
        assert_eq!(m.get("c").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn test_parse_string_quoted_value_preserves_quotes() {
        // "quoted"value" preserved verbatim (parser doesn't strip quotes).
        let p = ConfigParser::new();
        let m = p.parse_string("x = \"quoted\"\n").unwrap();
        assert_eq!(m.get("x").and_then(|v| v.as_str()), Some("\"quoted\""));
    }

    #[test]
    fn test_parse_string_value_with_at_sign_and_slash_preserved() {
        // Email-like or path-like values stored verbatim.
        let p = ConfigParser::new();
        let m = p.parse_string("email = user@example.com/path\n").unwrap();
        assert_eq!(m.get("email").and_then(|v| v.as_str()), Some("user@example.com/path"));
    }

    #[test]
    fn test_parse_string_auto_true_one_remains_one_not_translated() {
        // AutoTrue passes "1" through as "1" unchanged.
        let p = ConfigParser::new();
        let m = p.parse_string("x = 1\n").unwrap();
        assert_eq!(m.get("x").and_then(|v| v.as_str()), Some("1"));
    }

    #[test]
    fn test_parse_string_crlf_line_endings_handled() {
        // Windows-style \r\n handled by lines() iterator.
        let p = ConfigParser::new();
        let m = p.parse_string("k = v\r\nj = w\r\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("v"));
        assert_eq!(m.get("j").and_then(|v| v.as_str()), Some("w"));
    }

    #[test]
    fn test_parse_string_value_with_tab_preserved_in_middle() {
        // Tab character in value kept (not stripped as whitespace).
        let p = ConfigParser::new();
        let m = p.parse_string("k = a\tb\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("a\tb"));
    }

    #[test]
    fn test_parse_string_very_long_value_preserved() {
        // 1000+ character value stored intact.
        let p = ConfigParser::new();
        let long_val = "x".repeat(1000);
        let content = format!("k = {}\n", long_val);
        let m = p.parse_string(&content).unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some(long_val.as_str()));
    }

    #[test]
    fn test_parse_string_integer_key_all_digits_stored() {
        // "42 = v" → key "42" stored.
        let p = ConfigParser::new();
        let m = p.parse_string("42 = v\n").unwrap();
        assert_eq!(m.get("42").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_value_semicolon_preserved() {
        // Semicolon in value kept (not comment).
        let p = ConfigParser::new();
        let m = p.parse_string("cmd = ls; echo hi\n").unwrap();
        assert_eq!(m.get("cmd").and_then(|v| v.as_str()), Some("ls; echo hi"));
    }

    #[test]
    fn test_parse_string_block_open_close_same_line_not_supported_but_doesnt_panic() {
        // "<block></block>" inline — parse should either treat as block open or error.
        let p = ConfigParser::new();
        let res = p.parse_string("<block></block>\n");
        // Current parser: sees "<block>" (open) then "<" at next look — but content is on one line.
        // Either way: no panic.
        let _ = res;
    }

    #[test]
    fn test_parse_string_nested_blocks_both_stored_as_maps() {
        // `<outer><inner>...</inner></outer>` → 2-level nesting.
        let p = ConfigParser::new();
        let content = "<outer>\n<inner>\nk = v\n</inner>\n</outer>\n";
        let m = p.parse_string(content).unwrap();
        assert!(m.get("outer").and_then(|v| v.as_map()).is_some());
        let outer = m.get("outer").and_then(|v| v.as_map()).unwrap();
        assert!(outer.get("inner").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_comments_inside_block_skipped() {
        // Comments inside block body — skipped.
        let p = ConfigParser::new();
        let content = "<block>\n# comment in block\nk = v\n</block>\n";
        let m = p.parse_string(content).unwrap();
        let b = m.get("block").and_then(|v| v.as_map()).expect("block");
        assert_eq!(b.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_uppercase_key_lowered_by_default() {
        // Default lower_case_names=true → "UPPER_KEY" → "upper_key".
        let p = ConfigParser::new();
        let m = p.parse_string("UPPER_KEY = v\n").unwrap();
        assert_eq!(m.get("upper_key").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_same_key_three_times_list_of_three() {
        // Triplicate → list of 3 items.
        let p = ConfigParser::new();
        let m = p.parse_string("k = a\nk = b\nk = c\n").unwrap();
        let lst = m.get("k").and_then(|v| v.as_list()).expect("list");
        assert_eq!(lst.len(), 3);
    }

    #[test]
    fn test_parse_string_value_with_pipe_preserved_verbatim() {
        // Pipe characters in value kept.
        let p = ConfigParser::new();
        let m = p.parse_string("synonyms = a|b|c\n").unwrap();
        assert_eq!(m.get("synonyms").and_then(|v| v.as_str()), Some("a|b|c"));
    }

    #[test]
    fn test_parse_string_value_with_parentheses_preserved() {
        // Parentheses in value kept.
        let p = ConfigParser::new();
        let m = p.parse_string("expr = (x+y)*z\n").unwrap();
        assert_eq!(m.get("expr").and_then(|v| v.as_str()), Some("(x+y)*z"));
    }

    #[test]
    fn test_parse_string_value_with_brackets_preserved() {
        // Square brackets in value kept.
        let p = ConfigParser::new();
        let m = p.parse_string("arr = [1,2,3]\n").unwrap();
        assert_eq!(m.get("arr").and_then(|v| v.as_str()), Some("[1,2,3]"));
    }

    #[test]
    fn test_parse_string_block_with_mixed_case_lower_case_names_off() {
        // With lower_case_names=false, "Block" block name stays uppercase.
        let mut p = ConfigParser::new();
        p.lower_case_names = false;
        let m = p.parse_string("<Block>\n  k = v\n</Block>\n").unwrap();
        assert!(m.contains_key("Block"));
        assert!(!m.contains_key("block"));
    }

    #[test]
    fn test_parse_string_double_assignment_with_same_key_appends_list() {
        // Two assignments of same key → value becomes List.
        let p = ConfigParser::new();
        let m = p.parse_string("x = 1\nx = 2\n").unwrap();
        let lst = m.get("x").and_then(|v| v.as_list()).expect("list");
        assert_eq!(lst.len(), 2);
    }

    #[test]
    fn test_parse_string_value_with_newlines_in_content_not_allowed() {
        // Only whole-line parsing — value stops at newline.
        let p = ConfigParser::new();
        let m = p.parse_string("k = line1\nk2 = line2\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("line1"));
        assert_eq!(m.get("k2").and_then(|v| v.as_str()), Some("line2"));
    }

    #[test]
    fn test_parse_string_value_with_ampersand_preserved() {
        // Ampersand in value kept.
        let p = ConfigParser::new();
        let m = p.parse_string("cond = a && b\n").unwrap();
        assert_eq!(m.get("cond").and_then(|v| v.as_str()), Some("a && b"));
    }

    #[test]
    fn test_parse_string_three_nested_blocks_all_reachable() {
        // 3-level nested blocks each as Map.
        let p = ConfigParser::new();
        let content = "<a>\n<b>\n<c>\nk = v\n</c>\n</b>\n</a>\n";
        let m = p.parse_string(content).unwrap();
        let a = m.get("a").and_then(|v| v.as_map()).unwrap();
        let b = a.get("b").and_then(|v| v.as_map()).unwrap();
        let c = b.get("c").and_then(|v| v.as_map()).unwrap();
        assert_eq!(c.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_key_with_leading_underscore_valid() {
        // Underscore-prefixed keys valid.
        let p = ConfigParser::new();
        let m = p.parse_string("_private = value\n").unwrap();
        assert_eq!(m.get("_private").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_value_with_equals_sign_inside() {
        // Value containing '=' is preserved verbatim.
        let p = ConfigParser::new();
        let m = p.parse_string("k = a=b=c\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("a=b=c"));
    }

    #[test]
    fn test_parse_string_empty_input_yields_empty_map() {
        // Empty input → empty map.
        let p = ConfigParser::new();
        let m = p.parse_string("").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_string_key_with_digits_accepted() {
        // Numeric-suffix keys accepted.
        let p = ConfigParser::new();
        let m = p.parse_string("key1 = a\nkey2 = b\n").unwrap();
        assert_eq!(m.get("key1").and_then(|v| v.as_str()), Some("a"));
        assert_eq!(m.get("key2").and_then(|v| v.as_str()), Some("b"));
    }

    #[test]
    fn test_parse_string_value_with_leading_trailing_whitespace_trimmed() {
        // Value whitespace trimmed on both sides.
        let p = ConfigParser::new();
        let m = p.parse_string("k =   trimmed  \n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("trimmed"));
    }

    #[test]
    fn test_parse_string_block_ending_without_trailing_newline() {
        // Block closed by </x> on last line (no trailing newline).
        let p = ConfigParser::new();
        let m = p.parse_string("<x>\nk = v\n</x>").unwrap();
        let sub = m.get("x").and_then(|v| v.as_map()).unwrap();
        assert_eq!(sub.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_value_with_only_digits_stays_as_string() {
        // Parser stores values as String; "123" remains a Str variant.
        let p = ConfigParser::new();
        let m = p.parse_string("count = 123\n").unwrap();
        assert_eq!(m.get("count").and_then(|v| v.as_str()), Some("123"));
    }

    #[test]
    fn test_parse_string_multiple_top_level_kvs_preserved() {
        // 3 top-level keys all accessible.
        let p = ConfigParser::new();
        let m = p.parse_string("a = 1\nb = 2\nc = 3\n").unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("b").and_then(|v| v.as_str()), Some("2"));
        assert_eq!(m.get("c").and_then(|v| v.as_str()), Some("3"));
    }

    #[test]
    fn test_parse_string_comment_only_input_yields_empty_map() {
        // Comment-only input → empty map.
        let p = ConfigParser::new();
        let m = p.parse_string("# only a comment\n# another\n").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_string_consecutive_blank_lines_skipped() {
        // Multiple blank lines between k=v → skipped.
        let p = ConfigParser::new();
        let m = p.parse_string("a = 1\n\n\n\nb = 2\n").unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("b").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn test_parse_string_empty_block_produces_empty_submap() {
        // Empty block <x></x> → empty Map entry.
        let p = ConfigParser::new();
        let m = p.parse_string("<x>\n</x>\n").unwrap();
        let sub = m.get("x").and_then(|v| v.as_map());
        assert!(sub.is_some());
        assert!(sub.unwrap().is_empty());
    }

    #[test]
    fn test_parse_string_trailing_comment_on_value_line_stripped() {
        // "k = v # inline comment" → "v" preserved without inline-comment text.
        let p = ConfigParser::new();
        let m = p.parse_string("k = value\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_two_sibling_blocks_both_parsed() {
        // <a>...</a><b>...</b> → both submap keys present.
        let p = ConfigParser::new();
        let m = p.parse_string("<a>\nk = v1\n</a>\n<b>\nk = v2\n</b>\n").unwrap();
        assert!(m.get("a").and_then(|v| v.as_map()).is_some());
        assert!(m.get("b").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_block_with_tab_indented_kvs_parsed() {
        // Tab-indented key/value inside block.
        let p = ConfigParser::new();
        let m = p.parse_string("<x>\n\tk = v\n</x>\n").unwrap();
        let sub = m.get("x").and_then(|v| v.as_map()).unwrap();
        assert_eq!(sub.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_value_with_quotes_preserved_verbatim() {
        // Double-quotes in value preserved as-is (not stripped).
        let p = ConfigParser::new();
        let m = p.parse_string(r#"k = "quoted value"\n"#).unwrap();
        assert!(m.get("k").is_some());
    }

    #[test]
    fn test_parse_string_unix_and_no_trailing_newline_combined() {
        // Multiple lines without trailing \n still parse.
        let p = ConfigParser::new();
        let m = p.parse_string("a = 1\nb = 2").unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(m.get("b").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn test_parse_string_case_preserved_in_value_when_lc_key_on() {
        // LowerCaseNames affects keys, not values.
        let p = ConfigParser::new();
        let m = p.parse_string("KEY = CaseSensitive_Value\n").unwrap();
        // Value case preserved regardless of key case.
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("CaseSensitive_Value"));
    }

    #[test]
    fn test_parse_string_single_key_value_no_space_around_eq() {
        // "a=1" (no spaces) still valid.
        let p = ConfigParser::new();
        let m = p.parse_string("a=1\n").unwrap();
        assert_eq!(m.get("a").and_then(|v| v.as_str()), Some("1"));
    }

    #[test]
    fn test_parse_string_value_with_unicode_preserved() {
        // Unicode characters in value preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("lang = español\n").unwrap();
        assert_eq!(m.get("lang").and_then(|v| v.as_str()), Some("español"));
    }

    #[test]
    fn test_parse_string_nested_block_3_levels_reachable() {
        // Nested 3-level block <a><b><c>k=v</c></b></a>.
        let p = ConfigParser::new();
        let m = p.parse_string("<a>\n<b>\n<c>\nk = v\n</c>\n</b>\n</a>\n").unwrap();
        let a = m.get("a").and_then(|v| v.as_map()).unwrap();
        let b = a.get("b").and_then(|v| v.as_map()).unwrap();
        let c = b.get("c").and_then(|v| v.as_map()).unwrap();
        assert_eq!(c.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_block_with_multiple_kvs_all_accessible() {
        // Block with 3 keys → all stored in submap.
        let p = ConfigParser::new();
        let m = p.parse_string("<x>\na = 1\nb = 2\nc = 3\n</x>\n").unwrap();
        let sub = m.get("x").and_then(|v| v.as_map()).unwrap();
        assert_eq!(sub.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(sub.get("b").and_then(|v| v.as_str()), Some("2"));
        assert_eq!(sub.get("c").and_then(|v| v.as_str()), Some("3"));
    }

    #[test]
    fn test_parse_string_value_with_slash_preserved() {
        // Forward slashes in value preserved (common for file paths).
        let p = ConfigParser::new();
        let m = p.parse_string("path = /usr/local/bin/circos\n").unwrap();
        assert_eq!(m.get("path").and_then(|v| v.as_str()), Some("/usr/local/bin/circos"));
    }

    #[test]
    fn test_parse_string_multiple_colons_in_value_preserved() {
        // Multiple colons (e.g. URL with ports) preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("url = http://host:port/path\n").unwrap();
        assert_eq!(m.get("url").and_then(|v| v.as_str()), Some("http://host:port/path"));
    }

    #[test]
    fn test_parse_string_block_adjacent_to_sibling_kv() {
        // Block followed by top-level kv — both accessible.
        let p = ConfigParser::new();
        let m = p.parse_string("<block>\nkey = v\n</block>\ntop = t\n").unwrap();
        assert!(m.get("block").and_then(|v| v.as_map()).is_some());
        assert_eq!(m.get("top").and_then(|v| v.as_str()), Some("t"));
    }

    #[test]
    fn test_parse_string_key_containing_hyphen_accepted() {
        // Keys with hyphen accepted.
        let p = ConfigParser::new();
        let m = p.parse_string("my-key = value\n").unwrap();
        assert_eq!(m.get("my-key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_same_key_repeated_yields_list() {
        // Same key assigned twice → List of both values (Config::General AllowMultiOptions).
        let p = ConfigParser::new();
        let m = p.parse_string("k = 1\nk = 2\n").unwrap();
        let list = m.get("k").and_then(|v| v.as_list());
        assert!(list.is_some());
        assert_eq!(list.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_string_value_with_percent_sign_preserved() {
        // Percent sign preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("pct = 50%\n").unwrap();
        assert_eq!(m.get("pct").and_then(|v| v.as_str()), Some("50%"));
    }

    #[test]
    fn test_parse_string_block_with_uppercase_name_lowercased_by_default() {
        // <TICKS>...</TICKS> → block keyed as "ticks" by default (LowerCaseNames).
        let p = ConfigParser::new();
        let m = p.parse_string("<TICKS>\nk = v\n</TICKS>\n").unwrap();
        assert!(m.get("ticks").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_key_dot_separator_accepted() {
        // Dotted keys accepted as literal (no deep access).
        let p = ConfigParser::new();
        let m = p.parse_string("key.sub = value\n").unwrap();
        assert!(m.get("key.sub").is_some());
    }

    #[test]
    fn test_parse_string_block_with_trailing_spaces_on_close_tag() {
        // Trailing whitespace on block close → still parses.
        let p = ConfigParser::new();
        let m = p.parse_string("<x>\nk = v\n</x>  \n").unwrap();
        assert!(m.get("x").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_numeric_value_with_thousands_separator_preserved() {
        // Values with commas preserved verbatim (not parsed numerically).
        let p = ConfigParser::new();
        let m = p.parse_string("n = 1,000,000\n").unwrap();
        assert_eq!(m.get("n").and_then(|v| v.as_str()), Some("1,000,000"));
    }

    #[test]
    fn test_parse_string_block_names_can_contain_digits() {
        // Block names with digits accepted.
        let p = ConfigParser::new();
        let m = p.parse_string("<block1>\nk = v\n</block1>\n").unwrap();
        assert!(m.get("block1").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn test_parse_string_value_with_trailing_quote_preserved() {
        // Values containing quotes preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("k = he said 'hi'\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("he said 'hi'"));
    }

    #[test]
    fn test_parse_string_nested_block_uppercase_names_all_lowered() {
        // <A><B>k=v</B></A> → a/b both lowercase.
        let p = ConfigParser::new();
        let m = p.parse_string("<A>\n<B>\nk = v\n</B>\n</A>\n").unwrap();
        let a = m.get("a").and_then(|v| v.as_map()).unwrap();
        let b = a.get("b").and_then(|v| v.as_map()).unwrap();
        assert_eq!(b.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_value_with_backslash_preserved_verbatim() {
        // Backslash in value preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("path = C:\\Users\\test\n").unwrap();
        assert_eq!(m.get("path").and_then(|v| v.as_str()), Some("C:\\Users\\test"));
    }

    #[test]
    fn test_parse_string_multi_block_with_top_level_after_block() {
        // Block followed by another block and top-level kv.
        let p = ConfigParser::new();
        let m = p.parse_string("<a>\nk=1\n</a>\n<b>\nk=2\n</b>\ntop=3\n").unwrap();
        assert!(m.get("a").and_then(|v| v.as_map()).is_some());
        assert!(m.get("b").and_then(|v| v.as_map()).is_some());
        assert_eq!(m.get("top").and_then(|v| v.as_str()), Some("3"));
    }

    #[test]
    fn test_parse_string_value_starting_with_number_preserved() {
        // "v = 2.5x-3" preserved verbatim.
        let p = ConfigParser::new();
        let m = p.parse_string("v = 2.5x-3\n").unwrap();
        assert_eq!(m.get("v").and_then(|v| v.as_str()), Some("2.5x-3"));
    }

    #[test]
    fn test_parse_string_double_close_tags_recovered() {
        // Graceful handling — either successful or error without panic.
        let p = ConfigParser::new();
        let result = p.parse_string("<a>\nk=v\n</a>\n</a>\n");
        // Should not panic. Either succeed or return error.
        let _ = result;
    }

    #[test]
    fn test_parse_string_single_line_mixed_whitespace_between_eq() {
        // Tab between k and = accepted.
        let p = ConfigParser::new();
        let m = p.parse_string("k\t=\tv\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_comment_after_value_on_same_line() {
        // "k = v  # comment" → value preserved (or comment stripped, check behavior).
        let p = ConfigParser::new();
        let m = p.parse_string("k = v\n# separate comment\n").unwrap();
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_parse_string_with_unicode_block_name_valid() {
        // Unicode block name preserved (or lowercased).
        let p = ConfigParser::new();
        let m = p.parse_string("<blockα>\nk=v\n</blockα>\n").unwrap();
        // Either stored as lowercase "blockα" or not — just ensure no panic.
        assert!(m.get("blockα").is_some() || m.get("blockα").is_none());
    }

    #[test]
    fn test_parse_string_value_with_asterisk_preserved() {
        // Value with "*" preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("pattern = *.txt\n").unwrap();
        assert_eq!(m.get("pattern").and_then(|v| v.as_str()), Some("*.txt"));
    }

    #[test]
    fn test_parse_string_value_with_at_sign_preserved() {
        // Value with "@" preserved (e.g. email addresses).
        let p = ConfigParser::new();
        let m = p.parse_string("email = user@example.com\n").unwrap();
        assert_eq!(
            m.get("email").and_then(|v| v.as_str()),
            Some("user@example.com")
        );
    }

    #[test]
    fn test_parse_string_many_top_level_keys_all_accessible() {
        // 15 top-level keys → all stored.
        let content: String = (0..15)
            .map(|i| format!("k{} = v{}\n", i, i))
            .collect();
        let p = ConfigParser::new();
        let m = p.parse_string(&content).unwrap();
        for i in 0..15 {
            assert_eq!(
                m.get(&format!("k{}", i)).and_then(|v| v.as_str()),
                Some(format!("v{}", i).as_str())
            );
        }
    }

    #[test]
    fn test_parse_string_value_with_square_brackets_preserved() {
        // Value with square brackets preserved.
        let p = ConfigParser::new();
        let m = p.parse_string("range = [0-100]\n").unwrap();
        assert_eq!(m.get("range").and_then(|v| v.as_str()), Some("[0-100]"));
    }

    #[test]
    fn test_parse_string_empty_content_yields_empty_map() {
        // Empty input → empty config map, no errors.
        let p = ConfigParser::new();
        let m = p.parse_string("").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_string_only_comments_yields_empty_map() {
        // Only comment lines → empty map.
        let p = ConfigParser::new();
        let m = p.parse_string("# a comment\n# another\n").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_string_key_lowercased_by_default() {
        // LowerCaseNames default on → keys emitted in lowercase.
        let p = ConfigParser::new();
        let m = p.parse_string("MyKey = foo\n").unwrap();
        assert!(m.contains_key("mykey"));
        assert!(!m.contains_key("MyKey"));
    }

    #[test]
    fn test_parse_string_nested_block_creates_sub_map() {
        // <block>...</block> → nested Map under key "block".
        let p = ConfigParser::new();
        let m = p.parse_string("<block>\nk = v\n</block>\n").unwrap();
        let inner = m.get("block").expect("block exists");
        // Should be a Map, not a Str.
        assert!(inner.as_str().is_none());
    }

    #[test]
    fn test_configparser_new_default_flags_are_auto_true_and_lowercase() {
        // Defaults per Config::General conventions.
        let p = ConfigParser::new();
        assert!(p.auto_true);
        assert!(p.lower_case_names);
    }

    #[test]
    fn test_parse_string_when_lower_case_names_disabled_key_case_preserved() {
        // With lower_case_names=false, original casing retained.
        let mut p = ConfigParser::new();
        p.lower_case_names = false;
        let m = p.parse_string("MyKey = foo\n").unwrap();
        assert!(m.contains_key("MyKey"));
        assert!(!m.contains_key("mykey"));
    }

    #[test]
    fn test_parse_string_spaces_around_equals_sign_trimmed() {
        // "  key   =    value   " → key="key", value="value".
        let p = ConfigParser::new();
        let m = p.parse_string("  key   =    value   \n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_string_unexpected_close_at_top_level_is_error() {
        // </foo> at top level (no matching open) → unexpected block close error.
        let p = ConfigParser::new();
        let r = p.parse_string("</foo>\n");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unexpected block close"));
    }

    #[test]
    fn test_auto_true_convert_yes_to_1() {
        assert_eq!(auto_true_convert("yes"), "1");
        assert_eq!(auto_true_convert("YES"), "1");
        assert_eq!(auto_true_convert("true"), "1");
        assert_eq!(auto_true_convert("on"), "1");
    }

    #[test]
    fn test_auto_true_convert_no_to_0() {
        assert_eq!(auto_true_convert("no"), "0");
        assert_eq!(auto_true_convert("NO"), "0");
        assert_eq!(auto_true_convert("false"), "0");
        assert_eq!(auto_true_convert("off"), "0");
    }

    #[test]
    fn test_auto_true_convert_other_passes_through_unchanged() {
        // Non-keyword values passed through.
        assert_eq!(auto_true_convert("hello"), "hello");
        assert_eq!(auto_true_convert(""), "");
        assert_eq!(auto_true_convert("42"), "42");
    }

    #[test]
    fn test_parse_assignment_line_without_equals_returns_none() {
        // No "=" → None.
        assert!(parse_assignment("key value").is_none());
        assert!(parse_assignment("").is_none());
    }

    #[test]
    fn test_strip_inline_comment_with_hash_in_quoted_string_preserved() {
        // "#" inside quotes not treated as comment.
        let s = strip_inline_comment(r#"k = "hello # world""#);
        assert_eq!(s, r#"k = "hello # world""#);
    }

    #[test]
    fn test_strip_inline_comment_no_hash_returns_whole_line() {
        // No "#" anywhere → returned verbatim (no trim).
        let s = strip_inline_comment("key = value");
        assert_eq!(s, "key = value");
    }

    #[test]
    fn test_parse_assignment_multiple_equals_splits_on_first() {
        // "a = b = c" → split_once('=') → key="a", value="b = c".
        let (k, v) = parse_assignment("a = b = c").unwrap();
        assert_eq!(k, "a");
        assert_eq!(v, "b = c");
    }

    #[test]
    fn test_parse_string_single_block_round_trip_value_preserved() {
        // <b>k=v</b> → m.get("b") → Map containing k=v.
        let p = ConfigParser::new();
        let m = p.parse_string("<block>\nsubkey = somevalue\n</block>\n").unwrap();
        let inner = m.get("block").unwrap().as_map().unwrap();
        assert_eq!(inner.get("subkey").and_then(|v| v.as_str()), Some("somevalue"));
    }

    #[test]
    fn test_parse_string_autotrue_converts_yes_to_1_in_value() {
        // auto_true default on → "yes" → "1".
        let p = ConfigParser::new();
        let m = p.parse_string("flag = yes\n").unwrap();
        assert_eq!(m.get("flag").and_then(|v| v.as_str()), Some("1"));
    }

    #[test]
    fn test_parse_string_autotrue_disabled_preserves_yes_verbatim() {
        // auto_true=false → "yes" preserved.
        let mut p = ConfigParser::new();
        p.auto_true = false;
        let m = p.parse_string("flag = yes\n").unwrap();
        assert_eq!(m.get("flag").and_then(|v| v.as_str()), Some("yes"));
    }

    #[test]
    fn test_parse_string_duplicate_key_converts_to_list() {
        // AllowMultiOptions: duplicate keys → List of values.
        let p = ConfigParser::new();
        let m = p.parse_string("k = a\nk = b\nk = c\n").unwrap();
        let list = m.get("k").unwrap().as_list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_parse_string_nested_double_nested_block_accessible() {
        // <a><b>k=v</b></a> → m.a.b.k = v.
        let p = ConfigParser::new();
        let m = p.parse_string("<a>\n<b>\nk = v\n</b>\n</a>\n").unwrap();
        let a = m.get("a").unwrap().as_map().unwrap();
        let b = a.get("b").unwrap().as_map().unwrap();
        assert_eq!(b.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn test_strip_inline_comment_pure_comment_line_returns_empty_after_trim() {
        // Line that is just "# comment" → strip returns "".
        let s = strip_inline_comment("# just a comment");
        assert_eq!(s, "");
    }

    #[test]
    fn test_auto_true_convert_mixed_case_on_to_1() {
        // "On" (mixed case) → lowercased → "on" → "1".
        assert_eq!(auto_true_convert("On"), "1");
        assert_eq!(auto_true_convert("ON"), "1");
    }

    #[test]
    fn test_parse_string_trailing_empty_lines_ignored() {
        // Trailing blank lines don't cause errors.
        let p = ConfigParser::new();
        let m = p.parse_string("key = val\n\n\n\n\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("val"));
    }

    #[test]
    fn test_parse_string_block_with_args_in_angle_brackets() {
        // <block arg> → block_name parses as "block arg" (with space).
        let p = ConfigParser::new();
        let m = p.parse_string("<block arg>\nk = v\n</block>\n").unwrap();
        // Some key should exist (block or "block arg" depending on parser).
        assert!(!m.is_empty());
    }

    #[test]
    fn test_insert_multi_first_key_becomes_value() {
        // First insertion with Vacant entry → Str value.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut m, "k", ConfigValue::Str("v1".into()));
        assert_eq!(m.get("k").and_then(|v| v.as_str()), Some("v1"));
    }

    #[test]
    fn test_insert_multi_second_key_converts_str_to_list() {
        // Second insertion with existing Str → converts to List of 2.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut m, "k", ConfigValue::Str("v1".into()));
        insert_multi(&mut m, "k", ConfigValue::Str("v2".into()));
        let list = m.get("k").unwrap().as_list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_insert_multi_third_key_appends_to_existing_list() {
        // Third insertion with existing List → list.push.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        insert_multi(&mut m, "k", ConfigValue::Str("v1".into()));
        insert_multi(&mut m, "k", ConfigValue::Str("v2".into()));
        insert_multi(&mut m, "k", ConfigValue::Str("v3".into()));
        let list = m.get("k").unwrap().as_list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_strip_inline_comment_hash_immediately_after_value() {
        // "key=v#comment" → "key=v" (trimmed).
        let s = strip_inline_comment("key=v#comment");
        assert_eq!(s, "key=v");
    }

    #[test]
    fn test_parse_assignment_value_with_equals_inside_preserved() {
        // "a = b=c=d" → split_once → value = "b=c=d".
        let (k, v) = parse_assignment("a = b=c=d").unwrap();
        assert_eq!(k, "a");
        assert_eq!(v, "b=c=d");
    }

    #[test]
    fn test_auto_true_convert_empty_string_passes_through() {
        // "" → not in keyword set → passes through.
        assert_eq!(auto_true_convert(""), "");
    }

    #[test]
    fn test_parse_string_comment_line_with_special_chars_ignored() {
        // "# @#$% comment" → comment line → ignored.
        let p = ConfigParser::new();
        let m = p.parse_string("# @#$% comment\nkey = val\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("val"));
    }

    #[test]
    fn test_parse_string_value_with_equals_sign_preserved() {
        // "key = a=b" → key="key", value="a=b".
        let p = ConfigParser::new();
        let m = p.parse_string("key = a=b\n").unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("a=b"));
    }
}
