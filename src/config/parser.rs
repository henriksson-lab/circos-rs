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

impl ConfigParser {
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
                let block_name = line
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim();
                let block_key = if self.lower_case_names {
                    block_name.split_whitespace().next().unwrap_or(block_name).to_lowercase()
                } else {
                    block_name.split_whitespace().next().unwrap_or(block_name).to_string()
                };
                stack.push((block_key, current));
                current = HashMap::new();
                continue;
            }

            // Block close: </block_name>
            if line.starts_with("</") {
                let (block_key, mut parent) =
                    stack.pop().ok_or_else(|| {
                        format!("line {}: unexpected block close", line_num + 1)
                    })?;
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
                stack.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }

        Ok(current)
    }

    /// Expand `<<include file>>` directives recursively.
    fn expand_includes(
        &self,
        content: &str,
        search_paths: &[PathBuf],
    ) -> Result<String, String> {
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
                    format!("cannot read included file {}: {}", included_path.display(), e)
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

        assert_eq!(
            config.get("key1").unwrap().as_str().unwrap(),
            "value1"
        );
        assert_eq!(
            config.get("key2").unwrap().as_str().unwrap(),
            "42"
        );
        // AutoTrue
        assert_eq!(
            config.get("debug").unwrap().as_str().unwrap(),
            "0"
        );
        assert_eq!(
            config.get("verbose").unwrap().as_str().unwrap(),
            "1"
        );
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
        assert_eq!(
            ideogram.get("thickness").unwrap().as_str().unwrap(),
            "100p"
        );
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
            tick_list[0].as_map().unwrap().get("spacing").unwrap().as_str().unwrap(),
            "5u"
        );
        assert_eq!(
            tick_list[1].as_map().unwrap().get("spacing").unwrap().as_str().unwrap(),
            "10u"
        );
    }

    #[test]
    fn test_lowercase_names() {
        let parser = ConfigParser::new();
        let config = parser.parse_string("MyKey = hello\n").unwrap();
        assert!(config.get("mykey").is_some());
        assert!(config.get("MyKey").is_none());
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
}
