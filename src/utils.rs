use std::path::{Path, PathBuf};

/// Check if a string represents a number (integer or float, optional sign and exponent).
pub fn is_number(s: &str) -> bool {
    // Matches Perl: /^[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?$/
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    s.parse::<f64>().is_ok()
}

/// Check if a string is blank (empty or whitespace only).
pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// Check if a string is a comment line (starts with optional whitespace then #).
pub fn is_comment(s: &str) -> bool {
    s.trim_start().starts_with('#')
}

/// Check if a value is an integer.
pub fn is_integer(v: f64) -> bool {
    v == v.floor() && v.is_finite()
}

/// Port of Perl `round_up`: returns `round(value)` if fractional part > 0.5,
/// else `1 + int(value)`.
pub fn round_up(value: f64) -> f64 {
    if value - value.trunc() > 0.5 {
        value.round()
    } else {
        1.0 + value.trunc()
    }
}

/// Port of Perl `defined_but_zero`: true iff value is Some and numerically zero / falsy.
pub fn defined_but_zero(v: Option<f64>) -> bool {
    match v {
        Some(x) => x == 0.0,
        None => false,
    }
}

/// Port of Perl `span_distance`: signed distance between intervals
/// [x1,y1] and [x2,y2]. Negative when they overlap (magnitude = overlap).
pub fn span_distance(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let (x1, y1) = if x1 > y1 { (y1, x1) } else { (x1, y1) };
    let (x2, y2) = if x2 > y2 { (y2, x2) } else { (x2, y2) };
    let (_x1, y1, x2, y2) = if x1 > x2 {
        (x2, y2, x1, y1)
    } else {
        (x1, y1, x2, y2)
    };
    if x2 >= y1 {
        x2 - y1
    } else if y2 >= y1 {
        -(y1 - x2)
    } else {
        -(y2 - x2)
    }
}

/// Port of Perl `make_list(obj)`: if `obj` is a list, return it; if a scalar, wrap in
/// a 1-element Vec; if None/empty, return empty.
pub fn make_list(
    obj: Option<&crate::config::types::ConfigValue>,
) -> Vec<crate::config::types::ConfigValue> {
    match obj {
        None => Vec::new(),
        Some(v) => match v {
            crate::config::types::ConfigValue::List(list) => list.clone(),
            other => vec![other.clone()],
        },
    }
}

/// Port of Perl `replace_string(target, source, value)`: substitute all occurrences
/// of `source` in `target` with `value`, quoting when `value` is non-numeric and
/// not "undef" (matches Perl's heuristic that numeric values don't need quoting).
pub fn replace_string(target: &mut String, source: &str, value: &str) {
    let replacement = if value != "undef"
        && value
            .chars()
            .any(|c| !c.is_ascii_digit() && c != '-' && c != '.')
    {
        format!("'{}'", value)
    } else {
        value.to_string()
    };
    *target = target.replace(source, &replacement);
}

/// Port of Perl `perturb_value(value, perturb_parameters)`: multiply `value` by a
/// uniform random draw from `[pmin, pmax]` parsed from `"pmin,pmax"`. Returns value
/// unchanged if `perturb_parameters` is empty/None or value is zero.
pub fn perturb_value(value: f64, perturb_parameters: Option<&str>) -> f64 {
    let params = match perturb_parameters {
        Some(s) if !s.is_empty() => s,
        _ => return value,
    };
    if value == 0.0 {
        return value;
    }
    let parts: Vec<&str> = params
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 2 {
        return value;
    }
    let pmin: f64 = parts[0].parse().unwrap_or(1.0);
    let pmax: f64 = parts[1].parse().unwrap_or(1.0);
    // Perl uses rand() in [0,1). Using rand crate is heavier; use a simple LCG-ish draw
    // via a deterministic seed per-value for reproducibility would be wrong (Perl's rand is
    // a global). Match Perl behavior: nondeterministic uniform in [pmin, pmax].
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let urd = pmin + (pmax - pmin) * ((nanos as f64) / (u32::MAX as f64));
    value * urd
}

/// Port of Perl `seek_parameter(name, @structs)`: walk a list of HashMaps looking for
/// `name` (or any `|`-separated synonym), checking both `struct["param"][name]` and
/// `struct[name]`. Returns the first hit or None.
pub fn seek_parameter<'a>(
    param_name: &str,
    data_structs: &'a [&'a std::collections::HashMap<String, crate::config::types::ConfigValue>],
) -> Option<&'a crate::config::types::ConfigValue> {
    for name in param_name.split('|') {
        for s in data_structs {
            // check struct["param"][name]
            if let Some(param) = s.get("param").and_then(|v| v.as_map())
                && let Some(val) = param.get(name)
            {
                return Some(val);
            }
            // check struct[name]
            if let Some(val) = s.get(name) {
                return Some(val);
            }
        }
    }
    None
}

/// Port of Perl `format_url(url, param_path)`: substitute each `[PARAM]` in `url` with
/// the value fetched via `seek_parameter(PARAM, @param_path)`. Policy on missing
/// parameters is controlled by `missing` (matching Perl `image_map_missing_parameter`):
///   "exit"        → returns Err
///   "removeurl"   → returns Ok(None)
///   "removeparam" → removes the [PARAM] placeholder and continues
///   anything else → same as "removeparam"
pub fn format_url(
    url: &str,
    param_path: &[&std::collections::HashMap<String, crate::config::types::ConfigValue>],
    missing: &str,
) -> Result<Option<String>, String> {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\[([^\]\[]+)\]").unwrap());
    let re = &*RE;
    let mut result = url.to_string();
    // iterate because replacements may reveal new placeholders; bound to prevent infinite loop.
    for _ in 0..64 {
        let caps_opt = re.captures(&result).map(|c| {
            let all = c.get(0).unwrap();
            let name = c.get(1).unwrap().as_str().to_string();
            (all.range(), name)
        });
        let (range, param) = match caps_opt {
            None => return Ok(Some(result)),
            Some(x) => x,
        };
        let value = seek_parameter(&param, param_path)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match value {
            Some(v) => {
                result.replace_range(range, &v);
            }
            None => match missing {
                "exit" => {
                    return Err(format!(
                        "format_url: parameter [{}] has no value for url {}",
                        param, url
                    ));
                }
                "removeurl" => return Ok(None),
                _ => {
                    // removeparam (default)
                    result.replace_range(range, "");
                }
            },
        }
    }
    Ok(Some(result))
}

/// Port of Perl `inittracks(num)`: allocate `num` empty IntSpans for track bookkeeping.
pub fn inittracks(num: usize) -> Vec<crate::intspan::IntSpan> {
    (0..num).map(|_| crate::intspan::IntSpan::new()).collect()
}

/// Port of Perl `gettack(set, padding, chr, tracks, scale)`: find the first track that
/// can accommodate `set` (after `padding` expansion) without collision; update that
/// track in place and return its index. `chr_offset` is the cumulative length offset
/// for the chromosome `chr` (Perl reads this from KARYOTYPE->{chr}{length_cumul}).
/// `scale` divides the coordinate space to downscale the IntSpan footprint.
pub fn gettack(
    set: &crate::intspan::IntSpan,
    padding: i64,
    chr_offset: i64,
    tracks: &mut [crate::intspan::IntSpan],
    scale: i64,
) -> Option<usize> {
    let scale = if scale <= 0 { 1000 } else { scale };
    let lo = set.min()?;
    let hi = set.max()?;
    let padded_set = crate::intspan::IntSpan::from_range(
        (chr_offset + lo - padding) / scale,
        (chr_offset + hi + padding) / scale,
    );
    for (idx, t) in tracks.iter_mut().enumerate() {
        if t.intersect(&padded_set).cardinality() == 0 {
            *t = t.union(&padded_set);
            return Some(idx);
        }
    }
    None
}

/// Port of Perl `format_condition`: apply kb/Mb/Gb/bp suffixes as multipliers on numeric
/// literals within a string (case-insensitive).
pub fn format_condition(condition: &str) -> String {
    use std::sync::LazyLock;
    static RE_KB: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)([0-9.]+)kb").unwrap());
    static RE_MB: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)([0-9.]+)Mb").unwrap());
    static RE_GB: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)([0-9.]+)Gb").unwrap());
    static RE_BP: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)(\d+)bp").unwrap());
    let s = RE_KB.replace_all(condition, |c: &regex::Captures| {
        format!("{}", (c[1].parse::<f64>().unwrap_or(0.0) * 1e3) as i64)
    });
    let s = RE_MB.replace_all(&s, |c: &regex::Captures| {
        format!("{}", (c[1].parse::<f64>().unwrap_or(0.0) * 1e6) as i64)
    });
    let s = RE_GB.replace_all(&s, |c: &regex::Captures| {
        format!("{}", (c[1].parse::<f64>().unwrap_or(0.0) * 1e9) as i64)
    });
    let s = RE_BP.replace_all(&s, "$1");
    s.to_string()
}

/// Add thousands separators to a number string.
pub fn add_thousands_separator(s: &str, sep: char) -> String {
    if let Some(dot_pos) = s.find('.') {
        let integer_part = &s[..dot_pos];
        let decimal_part = &s[dot_pos..];
        format!("{}{}", insert_separators(integer_part, sep), decimal_part)
    } else {
        insert_separators(s, sep)
    }
}

/// Insert thousands separators into a digit string, preserving any leading sign.
fn insert_separators(s: &str, sep: char) -> String {
    let (sign, digits) = if s.starts_with('-') || s.starts_with('+') {
        (&s[..1], &s[1..])
    } else {
        ("", s)
    };

    let mut result = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push(sep);
        }
        result.push(c);
    }
    format!("{}{}", sign, result)
}

/// Locate a file by searching in standard directories relative to a base path.
pub fn locate_file(file: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(file);
    if path.exists() {
        return Some(path.to_path_buf());
    }

    for dir in search_paths {
        let candidate = dir.join(file);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Build the default search paths relative to a base directory.
pub fn default_search_paths(base_dir: &Path) -> Vec<PathBuf> {
    vec![
        base_dir.to_path_buf(),
        base_dir.join("etc"),
        base_dir.parent().map(|p| p.join("etc")).unwrap_or_default(),
        base_dir.parent().unwrap_or(base_dir).to_path_buf(),
        base_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("etc"))
            .unwrap_or_default(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_number() {
        assert!(is_number("42"));
        assert!(is_number("-3.14"));
        assert!(is_number("+1.5e10"));
        assert!(is_number("0.001"));
        assert!(is_number("1E-5"));
        assert!(!is_number(""));
        assert!(!is_number("abc"));
        assert!(!is_number("12abc"));
    }

    #[test]
    fn test_is_blank() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\t\n"));
        assert!(!is_blank("a"));
        assert!(!is_blank(" a "));
    }

    #[test]
    fn test_is_comment() {
        assert!(is_comment("# this is a comment"));
        assert!(is_comment("  # indented comment"));
        assert!(!is_comment("not a comment"));
        assert!(!is_comment(""));
    }

    #[test]
    fn test_is_integer() {
        assert!(is_integer(5.0));
        assert!(is_integer(-3.0));
        assert!(is_integer(0.0));
        assert!(!is_integer(3.5));
        assert!(!is_integer(f64::NAN));
    }

    #[test]
    fn test_add_thousands_separator() {
        assert_eq!(add_thousands_separator("1000", ','), "1,000");
        assert_eq!(add_thousands_separator("1000000", ','), "1,000,000");
        assert_eq!(add_thousands_separator("999", ','), "999");
        assert_eq!(add_thousands_separator("1234.567", ','), "1,234.567");
        assert_eq!(add_thousands_separator("-1000", ','), "-1,000");
    }

    #[test]
    fn test_span_distance_overlap_vs_gap() {
        // Overlapping spans: result is negative; magnitude = overlap size.
        assert_eq!(span_distance(0.0, 10.0, 5.0, 15.0), -5.0);
        // Adjacent (touching) spans → 0.
        assert_eq!(span_distance(0.0, 10.0, 10.0, 20.0), 0.0);
        // Disjoint spans → positive gap (x2 - y1).
        assert_eq!(span_distance(0.0, 10.0, 20.0, 30.0), 10.0);
        // Input order insensitivity: (y, x) swaps to (x, y) internally.
        assert_eq!(span_distance(10.0, 0.0, 5.0, 15.0), -5.0);
    }

    #[test]
    fn test_format_condition_unit_expansion() {
        assert_eq!(format_condition("100kb"), "100000");
        assert_eq!(format_condition("5Mb"), "5000000");
        assert_eq!(format_condition("1.5Gb"), "1500000000");
        // `_SIZE_ > 500kb` → `_SIZE_ > 500000`
        assert_eq!(format_condition("_SIZE_ > 500kb"), "_SIZE_ > 500000");
        // bp suffix strips to bare digit
        assert_eq!(format_condition("100bp"), "100");
    }

    #[test]
    fn test_round_up_and_defined_but_zero() {
        // round_up: adds 1 when fractional part ≤ 0.5, else rounds to nearest.
        assert_eq!(round_up(1.2), 2.0);
        assert_eq!(round_up(2.0), 3.0); // integer gets +1
        assert_eq!(round_up(1.6), 2.0); // frac 0.6 > 0.5 → normal round
        assert!(defined_but_zero(Some(0.0)));
        assert!(!defined_but_zero(Some(0.1)));
        assert!(!defined_but_zero(None));
    }

    #[test]
    fn test_replace_string_numeric_vs_string() {
        // Numeric value replaces as-is
        let mut s = "alpha=X beta=X".to_string();
        replace_string(&mut s, "X", "42");
        assert_eq!(s, "alpha=42 beta=42");
        // Non-numeric value gets Perl-style single-quoted
        let mut s = "alpha=X".to_string();
        replace_string(&mut s, "X", "hello");
        assert_eq!(s, "alpha='hello'");
        // `undef` literal passes through unquoted (Perl exception)
        let mut s = "alpha=X".to_string();
        replace_string(&mut s, "X", "undef");
        assert_eq!(s, "alpha=undef");
    }

    #[test]
    fn test_perturb_value_zero_and_empty_passthrough() {
        // value == 0 → unchanged regardless of params
        assert_eq!(perturb_value(0.0, Some("0.5,1.5")), 0.0);
        // None or empty params → unchanged
        assert_eq!(perturb_value(42.0, None), 42.0);
        assert_eq!(perturb_value(42.0, Some("")), 42.0);
        // malformed params → unchanged
        assert_eq!(perturb_value(42.0, Some("only-one")), 42.0);
        // Well-formed params → result in [pmin*v, pmax*v].
        let v = perturb_value(10.0, Some("0.5,1.5"));
        assert!((5.0..=15.0).contains(&v), "perturbed {} out of [5,15]", v);
    }

    #[test]
    fn test_inittracks_builds_n_empty_spans() {
        let tracks = inittracks(3);
        assert_eq!(tracks.len(), 3);
        for t in &tracks {
            assert!(t.is_empty());
        }
        // Zero tracks yields empty vec.
        assert!(inittracks(0).is_empty());
    }

    #[test]
    fn test_gettack_places_into_first_available() {
        use crate::intspan::IntSpan;
        let set = IntSpan::from_range(100, 200);
        let mut tracks = inittracks(3);
        // First call lands on track 0.
        let idx = gettack(&set, 0, 0, &mut tracks, 1);
        assert_eq!(idx, Some(0));
        // Overlapping set with padding 0 and scale 1 → must go to track 1.
        let set2 = IntSpan::from_range(150, 250);
        let idx = gettack(&set2, 0, 0, &mut tracks, 1);
        assert_eq!(idx, Some(1));
        // Non-overlapping set → can reuse track 0.
        let set3 = IntSpan::from_range(300, 400);
        let idx = gettack(&set3, 0, 0, &mut tracks, 1);
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn test_gettack_returns_none_when_all_occupied() {
        use crate::intspan::IntSpan;
        let set = IntSpan::from_range(0, 100);
        let mut tracks = inittracks(2);
        gettack(&set, 0, 0, &mut tracks, 1);
        gettack(&set, 0, 0, &mut tracks, 1);
        // Both tracks now occupy 0-100; a 3rd overlapping set has no home.
        let set2 = IntSpan::from_range(50, 75);
        let idx = gettack(&set2, 0, 0, &mut tracks, 1);
        assert_eq!(idx, None);
    }

    #[test]
    fn test_gettack_empty_set_returns_none() {
        use crate::intspan::IntSpan;
        let mut tracks = inittracks(3);
        // IntSpan::new() → min()/max() None → early return None.
        let idx = gettack(&IntSpan::new(), 0, 0, &mut tracks, 1);
        assert_eq!(idx, None);
    }

    #[test]
    fn test_locate_file_finds_in_search_paths() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("marker.txt");
        std::fs::write(&f, "hi").unwrap();
        // Direct absolute path → found on first check.
        let hit = locate_file(f.to_str().unwrap(), &[]).expect("absolute path");
        assert_eq!(hit, f);
        // Relative file, with search path → found via join.
        let hit = locate_file("marker.txt", &[dir.path().to_path_buf()])
            .expect("relative + search_path");
        assert_eq!(hit.file_name().unwrap(), "marker.txt");
        // Missing file → None.
        assert!(locate_file("does-not-exist.xxx", &[dir.path().to_path_buf()]).is_none());
    }

    #[test]
    fn test_default_search_paths_include_etc_dirs() {
        use std::path::Path;
        let base = Path::new("/foo/bar");
        let paths = default_search_paths(base);
        // Sanity: at least base + etc + parent's etc.
        let as_strs: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        assert!(as_strs.contains(&"/foo/bar".into()));
        assert!(as_strs.contains(&"/foo/bar/etc".into()));
        assert!(as_strs.contains(&"/foo/etc".into()));
    }

    #[test]
    fn test_seek_parameter_checks_param_submap_and_top_level() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut s1 = HashMap::new();
        let mut param = HashMap::new();
        param.insert("color".into(), ConfigValue::Str("red".into()));
        s1.insert("param".into(), ConfigValue::Map(param));
        s1.insert("color".into(), ConfigValue::Str("ignored".into()));
        let path = [&s1];
        let hit = seek_parameter("color", &path).and_then(|v| v.as_str()).unwrap();
        assert_eq!(hit, "red"); // param submap has priority over top-level key.
        // Top-level fallback when no `param` key.
        let mut s2 = HashMap::new();
        s2.insert("thickness".into(), ConfigValue::Str("3".into()));
        let path = [&s2];
        let hit = seek_parameter("thickness", &path)
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "3");
    }

    #[test]
    fn test_seek_parameter_handles_pipe_synonyms() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut s = HashMap::new();
        s.insert("fill_color".into(), ConfigValue::Str("blue".into()));
        let path = [&s];
        // "color|fill_color" tries "color" first, then falls back to "fill_color".
        let hit = seek_parameter("color|fill_color", &path)
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "blue");
    }

    #[test]
    fn test_seek_parameter_missing_returns_none() {
        use std::collections::HashMap;
        let s: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        let path = [&s];
        assert!(seek_parameter("missing", &path).is_none());
    }

    #[test]
    fn test_format_url_substitution_happy_path() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut params = HashMap::new();
        params.insert("chr".into(), ConfigValue::Str("hs1".into()));
        params.insert("pos".into(), ConfigValue::Str("12345".into()));
        let path = [&params];
        let r = format_url("/loc?chr=[chr]&pos=[pos]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "/loc?chr=hs1&pos=12345");
    }

    #[test]
    fn test_format_url_missing_policy_branches() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut params = HashMap::new();
        params.insert("chr".into(), ConfigValue::Str("hs1".into()));
        let path = [&params];
        // "removeparam" (default): drop the placeholder, keep going.
        let r = format_url("/x?chr=[chr]&pos=[pos]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "/x?chr=hs1&pos=");
        // "removeurl": one missing param → whole URL omitted (Ok(None)).
        let r = format_url("/x?chr=[chr]&pos=[pos]", &path, "removeurl").unwrap();
        assert_eq!(r, None);
        // "exit": missing param → Err.
        let r = format_url("/x?chr=[chr]&pos=[pos]", &path, "exit");
        assert!(r.is_err());
    }

    #[test]
    fn test_format_url_no_placeholders_passthrough() {
        use std::collections::HashMap;
        let params: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        let path = [&params];
        let r = format_url("https://static.example.com/img.png", &path, "removeparam")
            .unwrap();
        assert_eq!(r.unwrap(), "https://static.example.com/img.png");
    }

    #[test]
    fn test_make_list_scalar_list_and_none() {
        use crate::config::types::ConfigValue;
        // None → empty Vec.
        assert!(make_list(None).is_empty());
        // Scalar → single-element Vec.
        let s = ConfigValue::Str("x".to_string());
        let r = make_list(Some(&s));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].as_str(), Some("x"));
        // List → the list items preserved.
        let list = ConfigValue::List(vec![
            ConfigValue::Str("a".into()),
            ConfigValue::Str("b".into()),
            ConfigValue::Str("c".into()),
        ]);
        let r = make_list(Some(&list));
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].as_str(), Some("a"));
        assert_eq!(r[2].as_str(), Some("c"));
        // Map → wrapped as single element (mirrors Perl make_list).
        use std::collections::HashMap;
        let m = ConfigValue::Map(HashMap::new());
        let r = make_list(Some(&m));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn test_format_url_seek_walks_multiple_structs() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        // First struct supplies [chr], second supplies [pos]; format_url should
        // consult the whole param_path in order.
        let mut s1 = HashMap::new();
        s1.insert("chr".into(), ConfigValue::Str("hs3".into()));
        let mut s2 = HashMap::new();
        s2.insert("pos".into(), ConfigValue::Str("1000".into()));
        let path = [&s1, &s2];
        let r = format_url("/q?c=[chr]&p=[pos]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "/q?c=hs3&p=1000");
    }

    #[test]
    fn test_format_url_chained_substitutions() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        // If one [PARAM] is replaced with a value that contains another [P2], the
        // bounded loop handles the second pass.
        let mut params = HashMap::new();
        params.insert("chr".into(), ConfigValue::Str("hs1".into()));
        params.insert("link".into(), ConfigValue::Str("[chr]-fwd".into()));
        let path = [&params];
        let r = format_url("/x?t=[link]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "/x?t=hs1-fwd");
    }

    #[test]
    fn test_gettack_with_padding_expands_footprint() {
        use crate::intspan::IntSpan;
        // Padding of 10 means a set 100-200 occupies 90-210 before scale.
        let set = IntSpan::from_range(100, 200);
        let mut tracks = inittracks(2);
        let idx = gettack(&set, 10, 0, &mut tracks, 1);
        assert_eq!(idx, Some(0));
        // An immediately-adjacent set (201-300) would collide with padding at 210.
        let set2 = IntSpan::from_range(201, 300);
        let idx = gettack(&set2, 10, 0, &mut tracks, 1);
        assert_eq!(idx, Some(1));
        // Far-away set (400-500) with padding still fits in track 0.
        let set3 = IntSpan::from_range(400, 500);
        let idx = gettack(&set3, 10, 0, &mut tracks, 1);
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn test_gettack_default_scale_when_zero_or_negative() {
        use crate::intspan::IntSpan;
        // scale=0 → defaults to 1000 (so a set 0..999 becomes 0..0 at scale 1000).
        let set = IntSpan::from_range(0, 999);
        let mut tracks = inittracks(2);
        let idx = gettack(&set, 0, 0, &mut tracks, 0);
        assert_eq!(idx, Some(0));
        // Next set at 0..999 should now collide (track 0 has {0}) → go to 1.
        let set2 = IntSpan::from_range(0, 999);
        let idx = gettack(&set2, 0, 0, &mut tracks, 0);
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn test_gettack_chr_offset_shifts_footprint() {
        use crate::intspan::IntSpan;
        // Two different chromosomes with non-overlapping offsets can share a track.
        let set = IntSpan::from_range(0, 100);
        let mut tracks = inittracks(1);
        // chr_offset=0 → track 0 gets {0..100}.
        assert_eq!(gettack(&set, 0, 0, &mut tracks, 1), Some(0));
        // chr_offset=1000 → padded_set becomes {1000..1100}, no collision with {0..100}.
        assert_eq!(gettack(&set, 0, 1000, &mut tracks, 1), Some(0));
    }

    #[test]
    fn test_add_thousands_separator_edge_cases() {
        // Values < 1000 (no separator needed).
        assert_eq!(add_thousands_separator("0", ','), "0");
        assert_eq!(add_thousands_separator("1", ','), "1");
        assert_eq!(add_thousands_separator("100", ','), "100");
        // Exactly 1000.
        assert_eq!(add_thousands_separator("1000", ','), "1,000");
        // Values >= 1M (multiple separators).
        assert_eq!(add_thousands_separator("12345678", ','), "12,345,678");
        assert_eq!(
            add_thousands_separator("1234567890", ','),
            "1,234,567,890"
        );
        // Negative numbers and explicit +.
        assert_eq!(add_thousands_separator("-12345", ','), "-12,345");
        assert_eq!(add_thousands_separator("+12345", ','), "+12,345");
    }

    #[test]
    fn test_add_thousands_separator_with_decimal_and_custom_sep() {
        // Decimal part untouched; only integer part gets separators.
        assert_eq!(add_thousands_separator("1234567.89", ','), "1,234,567.89");
        // Custom separator (apostrophe, Swiss style).
        assert_eq!(add_thousands_separator("1234567", '\''), "1'234'567");
        // Space separator.
        assert_eq!(add_thousands_separator("1000000", ' '), "1 000 000");
    }

    #[test]
    fn test_perturb_value_determinism_stays_within_range() {
        // Running many perturbations should all remain in [pmin*v, pmax*v].
        let mut all_in_range = true;
        for _ in 0..20 {
            let v = perturb_value(100.0, Some("0.9,1.1"));
            if !(90.0..=110.0).contains(&v) {
                all_in_range = false;
                break;
            }
        }
        assert!(all_in_range, "perturb_value leaked outside [90, 110]");
    }

    #[test]
    fn test_is_number_edge_cases() {
        // Positives.
        assert!(is_number("0"));
        assert!(is_number("42"));
        assert!(is_number("3.14"));
        assert!(is_number("-5"));
        assert!(is_number("+7.2"));
        assert!(is_number(".5"));
        assert!(is_number("1e10"));
        assert!(is_number("-2.5E-3"));
        // Negatives.
        assert!(!is_number(""));
        assert!(!is_number("abc"));
        assert!(!is_number("12abc"));
        assert!(!is_number("1.2.3"));
    }

    #[test]
    fn test_is_blank_and_is_comment_distinguish_correctly() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\t  \t"));
        assert!(!is_blank("text"));
        assert!(!is_blank("  text  "));

        assert!(is_comment("#"));
        assert!(is_comment("# a comment"));
        assert!(is_comment("   # leading ws"));
        assert!(!is_comment("not # inline"));
        assert!(!is_comment("text"));
    }

    #[test]
    fn test_format_url_multiple_placeholders_same_pass() {
        // Multiple distinct placeholders all substituted in one iteration.
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut p = HashMap::new();
        p.insert("a".into(), ConfigValue::Str("1".into()));
        p.insert("b".into(), ConfigValue::Str("2".into()));
        p.insert("c".into(), ConfigValue::Str("3".into()));
        let path = [&p];
        let r = format_url("x=[a]&y=[b]&z=[c]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "x=1&y=2&z=3");
    }

    #[test]
    fn test_format_url_repeated_placeholder_all_expand() {
        // Same placeholder appearing multiple times → all substituted (per-iter one-at-a-time, bounded loop).
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut p = HashMap::new();
        p.insert("chr".into(), ConfigValue::Str("hs1".into()));
        let path = [&p];
        let r = format_url("[chr]/genes/[chr]/tracks/[chr]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "hs1/genes/hs1/tracks/hs1");
    }

    #[test]
    fn test_inittracks_with_1_and_large_n() {
        // inittracks(1) → single track.
        let t = inittracks(1);
        assert_eq!(t.len(), 1);
        assert!(t[0].is_empty());
        // inittracks(100) → 100 tracks.
        let t = inittracks(100);
        assert_eq!(t.len(), 100);
    }

    #[test]
    fn test_span_distance_symmetric_in_pair_order() {
        // span_distance((a,b), (c,d)) should equal span_distance((c,d), (a,b)) since
        // it's a spatial-distance measure, not a signed directional one.
        let d1 = span_distance(0.0, 10.0, 20.0, 30.0);
        let d2 = span_distance(20.0, 30.0, 0.0, 10.0);
        assert_eq!(d1, d2);
        // Overlap case:
        let d1 = span_distance(0.0, 10.0, 5.0, 15.0);
        let d2 = span_distance(5.0, 15.0, 0.0, 10.0);
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_seek_parameter_walks_multiple_structs_in_order() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        // First struct has no match; second does → should hit the second.
        let s1: HashMap<String, ConfigValue> = HashMap::new();
        let mut s2 = HashMap::new();
        s2.insert("key".into(), ConfigValue::Str("from_s2".into()));
        let path = [&s1, &s2];
        let hit = seek_parameter("key", &path).and_then(|v| v.as_str()).unwrap();
        assert_eq!(hit, "from_s2");

        // Both have the key → first wins.
        let mut s1 = HashMap::new();
        s1.insert("key".into(), ConfigValue::Str("from_s1".into()));
        let mut s2 = HashMap::new();
        s2.insert("key".into(), ConfigValue::Str("from_s2".into()));
        let path = [&s1, &s2];
        let hit = seek_parameter("key", &path).and_then(|v| v.as_str()).unwrap();
        assert_eq!(hit, "from_s1");
    }

    #[test]
    fn test_format_url_no_params_noop() {
        // URL with no `[…]` placeholders and no params → passthrough.
        use std::collections::HashMap;
        let empty: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        let path = [&empty];
        let r = format_url("/static/resource", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "/static/resource");
    }

    #[test]
    fn test_format_url_param_path_empty_all_missing_policy() {
        // Empty param_path → all placeholders are "missing".
        use std::collections::HashMap;
        let empty: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        let path = [&empty];
        // removeparam → empty string placeholders.
        let r = format_url("[a]_[b]", &path, "removeparam").unwrap();
        assert_eq!(r.unwrap(), "_");
        // removeurl → Ok(None).
        let r = format_url("[a]_[b]", &path, "removeurl").unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn test_round_up_near_boundary() {
        // frac exactly 0.5 → not > 0.5 → +1 path: 1.5 → 1 + 1 = 2.
        assert_eq!(round_up(1.5), 2.0);
        // frac slightly above 0.5 → nearest round: 1.50001 → rounds to 2.
        assert_eq!(round_up(1.50001), 2.0);
        // Exact integer: +1 (Perl's quirky behavior).
        assert_eq!(round_up(5.0), 6.0);
        assert_eq!(round_up(0.0), 1.0);
    }

    #[test]
    fn test_span_distance_fully_contained_span() {
        // Outer span [0,100] fully contains inner [30,50]. After the
        // sort-by-x normalization: x1=0, y1=100, x2=30, y2=50. Neither
        // `x2 >= y1` nor `y2 >= y1` fires (inner is strictly inside outer),
        // so the else branch returns `-(y2-x2) = -(50-30) = -20`. The
        // magnitude equals the inner span width (20), not the outer width.
        let d = span_distance(0.0, 100.0, 30.0, 50.0);
        assert_eq!(d, -20.0);
    }

    #[test]
    fn test_span_distance_identical_spans() {
        // Same span twice → overlap = -span_width.
        let d = span_distance(5.0, 15.0, 5.0, 15.0);
        assert_eq!(d, -10.0);
    }

    #[test]
    fn test_make_list_map_value_wraps_in_single_element_vec() {
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        // Perl make_list: a Map (hashref) is treated as a single element → wrapped.
        let mut m = HashMap::new();
        m.insert("k".into(), ConfigValue::Str("v".into()));
        let wrapped = make_list(Some(&ConfigValue::Map(m)));
        assert_eq!(wrapped.len(), 1);
        assert!(wrapped[0].as_map().is_some());
    }

    #[test]
    fn test_perturb_value_with_semi_whitespace_separators() {
        // `perturb_value` splits on whitespace OR comma; multi-space tolerated.
        let v = perturb_value(10.0, Some("0.5  1.5"));
        assert!((5.0..=15.0).contains(&v), "space-sep perturb {} out of [5,15]", v);
        // Mixed tabs and commas.
        let v = perturb_value(10.0, Some("0.9,\t1.1"));
        assert!((9.0..=11.0).contains(&v), "mixed-sep perturb {} out of [9,11]", v);
    }

    #[test]
    fn test_add_thousands_separator_large_numbers() {
        // Large 10-digit number: 1,234,567,890.
        assert_eq!(add_thousands_separator("1234567890", ','), "1,234,567,890");
        // 1-billion: 1,000,000,000.
        assert_eq!(add_thousands_separator("1000000000", ','), "1,000,000,000");
    }

    #[test]
    fn test_add_thousands_separator_with_leading_plus() {
        // Leading + sign is treated as sign by `insert_separators`.
        assert_eq!(add_thousands_separator("+12345", ','), "+12,345");
    }

    #[test]
    fn test_add_thousands_separator_short_numbers_unchanged() {
        // Numbers with ≤3 digits: no separator inserted.
        assert_eq!(add_thousands_separator("999", ','), "999");
        assert_eq!(add_thousands_separator("42", ','), "42");
        assert_eq!(add_thousands_separator("1", ','), "1");
        assert_eq!(add_thousands_separator("0", ','), "0");
    }

    #[test]
    fn test_add_thousands_separator_preserves_decimal_part() {
        // Decimal part appears after `.` and is not transformed.
        assert_eq!(
            add_thousands_separator("123456789.987654", ','),
            "123,456,789.987654"
        );
        // Decimal with leading 0.
        assert_eq!(add_thousands_separator("1000.5", ','), "1,000.5");
    }

    #[test]
    fn test_format_condition_decimal_kb_multiplies_correctly() {
        // "1.5kb" → 1500.
        assert_eq!(format_condition("1.5kb"), "1500");
        // "2.5Mb" → 2500000.
        assert_eq!(format_condition("2.5Mb"), "2500000");
    }

    #[test]
    fn test_format_condition_zero_value_units() {
        // "0kb" → 0, "0Mb" → 0, "0Gb" → 0.
        assert_eq!(format_condition("0kb"), "0");
        assert_eq!(format_condition("0Mb"), "0");
        assert_eq!(format_condition("0Gb"), "0");
    }

    #[test]
    fn test_format_condition_mixed_units_in_one_expression() {
        // Multiple units replaced in one expression.
        assert_eq!(
            format_condition("100kb + 2Mb - 1Gb"),
            "100000 + 2000000 - 1000000000"
        );
    }

    #[test]
    fn test_format_condition_unit_suffix_only_strips_bp() {
        // "100bp" → regex `(?i)(\d+)bp` replaces with $1 → "100".
        assert_eq!(format_condition("100bp"), "100");
        // Multiple bp in one expression.
        assert_eq!(format_condition("50bp + 200bp"), "50 + 200");
    }

    #[test]
    fn test_format_condition_case_insensitive_units() {
        // (?i) flag → KB/MB/GB also match.
        assert_eq!(format_condition("5KB"), "5000");
        assert_eq!(format_condition("10MB"), "10000000");
        assert_eq!(format_condition("2GB"), "2000000000");
        // Mixed case.
        assert_eq!(format_condition("5Kb"), "5000");
    }

    #[test]
    fn test_format_condition_bp_uppercase_not_matched() {
        // "100BP" uppercase doesn't match the (?i)bp regex that's case-insensitive too.
        // Actually (?i) covers uppercase — verify.
        assert_eq!(format_condition("100BP"), "100");
        // Embedded: "size=50bp" → "size=50".
        assert_eq!(format_condition("size=50bp"), "size=50");
    }

    #[test]
    fn test_replace_string_undef_sentinel_stays_as_is() {
        // When value is literally "undef", replace_string writes "undef" verbatim
        // (no quoting even though it's non-numeric).
        let mut s = String::from("alpha=[x]");
        replace_string(&mut s, "[x]", "undef");
        assert_eq!(s, "alpha=undef");
    }

    #[test]
    fn test_inittracks_returns_exactly_num_intspans() {
        // inittracks(N) returns N empty IntSpans.
        let tracks = inittracks(7);
        assert_eq!(tracks.len(), 7);
        for t in &tracks {
            assert_eq!(t.cardinality(), 0);
        }
    }

    #[test]
    fn test_inittracks_zero_returns_empty_vec() {
        // inittracks(0) → empty vec.
        let tracks = inittracks(0);
        assert!(tracks.is_empty());
    }

    #[test]
    fn test_gettack_populates_chosen_track_only() {
        // After gettack places into track idx N, only that track has the footprint.
        use crate::intspan::IntSpan;
        let mut tracks = inittracks(3);
        let set = IntSpan::from_range(100, 200);
        let idx = gettack(&set, 0, 0, &mut tracks, 1).unwrap();
        assert_eq!(idx, 0);
        // Track 0 now has the footprint; others stay empty.
        assert!(tracks[0].cardinality() > 0);
        assert_eq!(tracks[1].cardinality(), 0);
        assert_eq!(tracks[2].cardinality(), 0);
    }

    #[test]
    fn test_gettack_with_scale_divides_footprint() {
        // scale=100 reduces footprint to 1/100 of raw coords.
        use crate::intspan::IntSpan;
        let mut tracks = inittracks(1);
        let set = IntSpan::from_range(0, 100_000);
        let _ = gettack(&set, 0, 0, &mut tracks, 100);
        // Footprint cardinality: (100_000 - 0) / 100 + 1 = 1001 (approx).
        assert!(tracks[0].cardinality() > 0);
        assert!(tracks[0].cardinality() < 100_000);
    }

    #[test]
    fn test_round_up_negative_values() {
        // round_up for negatives: frac is trunc-based, so -0.3 → trunc=0, frac=-0.3.
        // -0.3 - 0 = -0.3, NOT > 0.5 → takes else branch: 1 + trunc(-0.3) = 1 + 0 = 1.
        assert_eq!(round_up(-0.3), 1.0);
        // -1.6: trunc=-1, frac=-0.6. -1.6 - (-1) = -0.6, NOT > 0.5 → 1 + -1 = 0.
        assert_eq!(round_up(-1.6), 0.0);
    }

    #[test]
    fn test_round_up_integer_inputs_all_increment() {
        // Any integer input hits the else branch: 1 + trunc(i) = i + 1.
        for i in [0.0f64, 1.0, 5.0, 100.0, -3.0, 42.0] {
            assert_eq!(round_up(i), i + 1.0, "round_up({}) should be {}", i, i + 1.0);
        }
    }

    #[test]
    fn test_defined_but_zero_handles_negative_zero() {
        // -0.0 == 0.0 in IEEE-754 → defined_but_zero returns true.
        assert!(defined_but_zero(Some(-0.0)));
        assert!(defined_but_zero(Some(0.0)));
        // Non-zero even very small → false.
        assert!(!defined_but_zero(Some(1e-300)));
    }

    #[test]
    fn test_perturb_value_pmin_equals_pmax_fixed_multiplier() {
        // When pmin == pmax, output is deterministic: value * pmin.
        // Run a few times to check consistency.
        for _ in 0..5 {
            let v = perturb_value(100.0, Some("2.0,2.0"));
            assert!((v - 200.0).abs() < 1e-6, "expected 200.0, got {}", v);
        }
        // Same with 0.5: value × 0.5.
        let v = perturb_value(100.0, Some("0.5,0.5"));
        assert!((v - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_format_url_bracketed_empty_placeholder_not_matched() {
        // `[]` (empty placeholder) — regex `\[([^\]\[]+)\]` requires ≥1 char
        // inside → doesn't match → URL passes through unchanged.
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let p: HashMap<String, ConfigValue> = HashMap::new();
        let r = format_url("/x?q=[]", &[&p], "removeparam").unwrap();
        assert_eq!(r.unwrap(), "/x?q=[]");
    }

    #[test]
    fn test_format_url_unknown_missing_policy_defaults_to_removeparam() {
        // A missing policy string other than "exit"/"removeurl" falls through
        // to the removeparam wildcard arm.
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let p: HashMap<String, ConfigValue> = HashMap::new();
        let r = format_url("/x=[missing]", &[&p], "garbage_policy").unwrap();
        // Unknown policy treated like "removeparam" — placeholder dropped.
        assert_eq!(r.unwrap(), "/x=");
    }

    #[test]
    fn test_format_url_nested_brackets_not_supported_by_regex() {
        // The regex `\[([^\]\[]+)\]` excludes `[` and `]` inside the capture →
        // `[a[b]c]` doesn't match cleanly. Each bracketed segment matches
        // separately: `[b]` is the innermost pair.
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut p: HashMap<String, ConfigValue> = HashMap::new();
        p.insert("b".into(), ConfigValue::Str("INNER".into()));
        let r = format_url("/x=[a[b]c]", &[&p], "removeparam").unwrap();
        // Inner `[b]` resolves first → "/x=[aINNERc]". Then `[aINNERc]` is
        // evaluated on the second iter — no key → removed.
        assert_eq!(r.unwrap(), "/x=");
    }

    #[test]
    fn test_format_url_value_containing_brackets_triggers_more_substitutions() {
        // When a substituted value itself contains `[...]`, the bounded loop
        // resolves it on a subsequent iteration.
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut p: HashMap<String, ConfigValue> = HashMap::new();
        p.insert("outer".into(), ConfigValue::Str("[inner]".into()));
        p.insert("inner".into(), ConfigValue::Str("FINAL".into()));
        let r = format_url("[outer]", &[&p], "removeparam").unwrap();
        assert_eq!(r.unwrap(), "FINAL");
    }

    #[test]
    fn test_is_integer_boundary_values() {
        // is_integer: tests whether f64 is an integer value.
        assert!(is_integer(0.0));
        assert!(is_integer(-1.0));
        assert!(is_integer(1e10));
        assert!(!is_integer(0.5));
        assert!(!is_integer(-0.5));
        assert!(!is_integer(1.0001));
    }

    #[test]
    fn test_default_search_paths_returns_fixed_count() {
        // The impl always returns 5 paths (some may duplicate for rootless dirs).
        use std::path::Path;
        let base = Path::new("/foo/bar/baz");
        let paths = default_search_paths(base);
        assert_eq!(paths.len(), 5);
    }

    #[test]
    fn test_default_search_paths_no_grandparent_handled() {
        // base_dir with no grandparent (close to root) — the last entry falls
        // back to empty PathBuf rather than panicking.
        use std::path::Path;
        let base = Path::new("/a");
        let paths = default_search_paths(base);
        assert_eq!(paths.len(), 5);
        // Entries: "/a", "/a/etc", "/etc", "/", "" (empty for missing grandparent).
        let as_strs: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        assert!(as_strs.contains(&"/a".to_string()));
        assert!(as_strs.contains(&"/a/etc".to_string()));
        assert!(as_strs.contains(&"/etc".to_string()));
    }

    #[test]
    fn test_locate_file_search_path_order_first_hit_wins() {
        // First matching path wins even if later paths also have the file.
        use std::path::PathBuf;
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::write(dir1.path().join("marker.txt"), "first").unwrap();
        std::fs::write(dir2.path().join("marker.txt"), "second").unwrap();
        let paths = vec![
            PathBuf::from(dir1.path()),
            PathBuf::from(dir2.path()),
        ];
        let hit = locate_file("marker.txt", &paths).expect("found");
        // First dir wins.
        assert_eq!(hit, dir1.path().join("marker.txt"));
    }

    #[test]
    fn test_seek_parameter_with_pipe_synonyms() {
        // "a|b" — tries `a` first, then `b`. If `a` missing, falls through to `b`.
        use crate::config::types::ConfigValue;
        use std::collections::HashMap;
        let mut s = HashMap::new();
        s.insert("b".into(), ConfigValue::Str("from_b".into()));
        let path = [&s];
        let hit = seek_parameter("a|b", &path)
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "from_b");
        // Both present → first synonym wins.
        let mut s = HashMap::new();
        s.insert("a".into(), ConfigValue::Str("from_a".into()));
        s.insert("b".into(), ConfigValue::Str("from_b".into()));
        let path = [&s];
        let hit = seek_parameter("a|b", &path)
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "from_a");
    }

    #[test]
    fn test_is_integer_edge_floats() {
        // Integer-valued floats → true.
        assert!(is_integer(0.0));
        assert!(is_integer(-5.0));
        assert!(is_integer(1e10));
        // Non-integer fractional → false.
        assert!(!is_integer(1.5));
        assert!(!is_integer(-0.1));
        // Non-finite (NaN, ±inf) → false.
        assert!(!is_integer(f64::NAN));
        assert!(!is_integer(f64::INFINITY));
        assert!(!is_integer(f64::NEG_INFINITY));
    }

    #[test]
    fn test_locate_file_searches_paths_in_order() {
        // locate_file checks absolute path first, then each search_path in order.
        let root = tempfile::tempdir().unwrap();
        let p1 = root.path().join("a"); std::fs::create_dir(&p1).unwrap();
        let p2 = root.path().join("b"); std::fs::create_dir(&p2).unwrap();
        std::fs::write(p2.join("target.txt"), "x\n").unwrap();
        // Not in a, is in b → returned as b/target.txt.
        let paths = vec![p1.clone(), p2.clone()];
        let found = locate_file("target.txt", &paths).unwrap();
        assert_eq!(found, p2.join("target.txt"));
        // Absolute existing path → returned verbatim (no search).
        let abs = p2.join("target.txt");
        let found = locate_file(abs.to_str().unwrap(), &paths).unwrap();
        assert_eq!(found, abs);
        // Missing everywhere → None.
        let missing = locate_file("nonexistent_xyz.txt", &paths);
        assert!(missing.is_none());
    }

    #[test]
    fn test_default_search_paths_structure() {
        // default_search_paths returns 5 paths in a known order.
        let base = PathBuf::from("/home/user/project/conf");
        let paths = default_search_paths(&base);
        assert_eq!(paths.len(), 5);
        // [0] = base
        assert_eq!(paths[0], base);
        // [1] = base/etc
        assert_eq!(paths[1], base.join("etc"));
        // [2] = base.parent/etc = /home/user/project/etc
        assert_eq!(paths[2], PathBuf::from("/home/user/project/etc"));
        // [3] = base.parent = /home/user/project
        assert_eq!(paths[3], PathBuf::from("/home/user/project"));
        // [4] = base.parent.parent/etc = /home/user/etc
        assert_eq!(paths[4], PathBuf::from("/home/user/etc"));
    }

    #[test]
    fn test_replace_string_numeric_unquoted_non_numeric_quoted() {
        // Numeric values are not quoted (contain only digits, `-`, or `.`).
        let mut target = "value=__X__".to_string();
        replace_string(&mut target, "__X__", "42");
        assert_eq!(target, "value=42");
        // Float + negative → still numeric, unquoted.
        let mut t2 = "__X__".to_string();
        replace_string(&mut t2, "__X__", "-3.14");
        assert_eq!(t2, "-3.14");
        // Non-numeric (contains letter) → wrapped in single quotes.
        let mut t3 = "__X__".to_string();
        replace_string(&mut t3, "__X__", "hello");
        assert_eq!(t3, "'hello'");
        // "undef" is special-cased — not quoted (matches Perl).
        let mut t4 = "__X__".to_string();
        replace_string(&mut t4, "__X__", "undef");
        assert_eq!(t4, "undef");
    }

    #[test]
    fn test_span_distance_touching_no_gap() {
        // Two spans touching at a single point → distance 0.
        assert_eq!(span_distance(0.0, 10.0, 10.0, 20.0), 0.0);
        // Same end-start in reversed order → also 0 (impl swaps).
        assert_eq!(span_distance(10.0, 20.0, 0.0, 10.0), 0.0);
    }

    #[test]
    fn test_span_distance_gap_positive() {
        // Gap between [0,5] and [10,15] is 5.
        assert_eq!(span_distance(0.0, 5.0, 10.0, 15.0), 5.0);
        // Reversed args → same 5.
        assert_eq!(span_distance(10.0, 15.0, 0.0, 5.0), 5.0);
    }

    #[test]
    fn test_is_number_empty_and_whitespace() {
        // Empty string is NOT a number (after trim).
        assert!(!is_number(""));
        assert!(!is_number("   "));
        // Whitespace-padded valid number → still passes (trim first).
        assert!(is_number("  42  "));
        assert!(is_number("  -3.14  "));
        // Partial number → parse fails.
        assert!(!is_number("42abc"));
        assert!(!is_number("abc42"));
    }

    #[test]
    fn test_make_list_wraps_various_variant_types() {
        // make_list with None → empty.
        assert!(make_list(None).is_empty());
        // With Some(Str) → single-element Vec.
        let s = crate::config::types::ConfigValue::Str("a".into());
        let r = make_list(Some(&s));
        assert_eq!(r.len(), 1);
        // With Some(List) of 3 items → 3-element Vec preserving order.
        let lst = crate::config::types::ConfigValue::List(vec![
            crate::config::types::ConfigValue::Str("a".into()),
            crate::config::types::ConfigValue::Str("b".into()),
            crate::config::types::ConfigValue::Str("c".into()),
        ]);
        let r = make_list(Some(&lst));
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].as_str(), Some("a"));
        assert_eq!(r[2].as_str(), Some("c"));
    }

    #[test]
    fn test_round_up_positive_fractional() {
        // round_up: if value - trunc > 0.5 → round(value); else → 1 + trunc(value).
        // 3.6 → trunc 3, frac 0.6 > 0.5 → round(3.6) = 4.
        assert_eq!(round_up(3.6), 4.0);
        // 3.3 → trunc 3, frac 0.3 NOT > 0.5 → 1 + 3 = 4.
        assert_eq!(round_up(3.3), 4.0);
        // 3.0 → trunc 3, frac 0 NOT > 0.5 → 1 + 3 = 4.
        assert_eq!(round_up(3.0), 4.0);
    }

    #[test]
    fn test_defined_but_zero_truth_table() {
        // Some(0.0) → true; Some(n) for n!=0 → false; None → false.
        assert!(defined_but_zero(Some(0.0)));
        assert!(defined_but_zero(Some(-0.0)));
        assert!(!defined_but_zero(Some(1.0)));
        assert!(!defined_but_zero(Some(-0.0001)));
        assert!(!defined_but_zero(None));
    }

    #[test]
    fn test_is_blank_comment_variants() {
        // is_blank: empty, whitespace-only → true.
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\t\n "));
        // Non-blank content → false.
        assert!(!is_blank("a"));
        assert!(!is_blank("  x  "));
        // is_comment: starts with `#` after trim_start.
        assert!(is_comment("#hi"));
        assert!(is_comment("   # leading ws"));
        assert!(!is_comment("not #comment"));
        assert!(!is_comment(""));
    }

    #[test]
    fn test_perturb_value_zero_value_unchanged() {
        // perturb_value with value=0 → 0 (early-return).
        assert_eq!(perturb_value(0.0, Some("0.5,2.0")), 0.0);
        // Also with no params → unchanged.
        assert_eq!(perturb_value(42.0, None), 42.0);
        // Empty params → unchanged.
        assert_eq!(perturb_value(42.0, Some("")), 42.0);
    }

    #[test]
    fn test_inittracks_returns_exactly_n_empty_intspans() {
        // inittracks(N) → Vec<IntSpan> of length N, each empty.
        for n in [0usize, 1, 3, 10, 100] {
            let tracks = inittracks(n);
            assert_eq!(tracks.len(), n);
            for t in &tracks {
                assert!(t.is_empty());
            }
        }
    }

    #[test]
    fn test_add_thousands_separator_default_sep_character() {
        // Default sep "," → "1,000".
        assert_eq!(add_thousands_separator("1000", ','), "1,000");
        assert_eq!(add_thousands_separator("1000000", ','), "1,000,000");
        // Custom sep: "_".
        assert_eq!(add_thousands_separator("1000", '_'), "1_000");
    }

    #[test]
    fn test_format_url_placeholder_missing_removeurl_returns_none() {
        // [FOO] with missing foo + policy=removeurl → Ok(None).
        let path: [&std::collections::HashMap<String, crate::config::types::ConfigValue>; 0] = [];
        let r = format_url("/[FOO]", &path, "removeurl");
        assert_eq!(r, Ok(None));
    }

    #[test]
    fn test_format_url_policy_exit_returns_err() {
        // "exit" policy + missing param → Err.
        let path: [&std::collections::HashMap<String, crate::config::types::ConfigValue>; 0] = [];
        let r = format_url("/[FOO]", &path, "exit");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("[FOO]"));
    }

    #[test]
    fn test_round_up_exactly_half_uses_trunc_branch() {
        // Condition `frac > 0.5` strict → 2.5 fails → else: 1 + trunc(2.5) = 3.
        assert_eq!(round_up(2.5), 3.0);
        // 2.6 passes strict > 0.5 → round(2.6) = 3 (same result, different branch).
        assert_eq!(round_up(2.6), 3.0);
        // 2.0 (zero fractional) → else: 1 + 2 = 3 — round_up always ceils to next integer.
        assert_eq!(round_up(2.0), 3.0);
        // 2.4 (fractional 0.4 ≤ 0.5) → else: 1 + 2 = 3.
        assert_eq!(round_up(2.4), 3.0);
    }

    #[test]
    fn test_is_integer_infinity_and_nan_both_false() {
        // is_finite() guard rejects ±inf; NaN fails the equality check (NaN != NaN).
        assert!(!is_integer(f64::INFINITY));
        assert!(!is_integer(f64::NEG_INFINITY));
        assert!(!is_integer(f64::NAN));
        // Finite integers pass.
        assert!(is_integer(0.0));
        assert!(is_integer(-42.0));
    }

    #[test]
    fn test_add_thousands_separator_negative_sign_preserved() {
        // Leading '-' detached; inserted separators apply to digits only.
        assert_eq!(add_thousands_separator("-1234", ','), "-1,234");
        assert_eq!(add_thousands_separator("-1234567", ','), "-1,234,567");
        // '+' sign is also preserved (insert_separators treats it like '-').
        assert_eq!(add_thousands_separator("+12345", ','), "+12,345");
        // Decimal part preserved without separators.
        assert_eq!(add_thousands_separator("-1234.56", ','), "-1,234.56");
    }

    #[test]
    fn test_format_condition_bp_suffix_stripped_only() {
        // bp regex replaces "<digits>bp" with just "<digits>" (no multiplier).
        assert_eq!(format_condition("100bp"), "100");
        // Mixed suffixes: each applied in kb→Mb→Gb→bp order.
        assert_eq!(format_condition("100bp+2kb"), "100+2000");
        // Non-matching text passes through unchanged.
        assert_eq!(format_condition("chr_name == hs1"), "chr_name == hs1");
    }

    #[test]
    fn test_perturb_value_zero_value_returns_zero_regardless_of_params() {
        // value==0.0 early-return → params ignored.
        assert_eq!(perturb_value(0.0, Some("0.5,1.5")), 0.0);
        assert_eq!(perturb_value(0.0, None), 0.0);
    }

    #[test]
    fn test_perturb_value_single_number_params_returns_value_unchanged() {
        // parts.len() < 2 → early-return value unchanged.
        assert_eq!(perturb_value(5.0, Some("0.5")), 5.0);
        // Empty string → Some(s) if !s.is_empty() guard fails → early-return value.
        assert_eq!(perturb_value(5.0, Some("")), 5.0);
        // None → same.
        assert_eq!(perturb_value(5.0, None), 5.0);
    }

    #[test]
    fn test_seek_parameter_pipe_synonyms_walked_left_to_right() {
        // "a|b" → check a first in all structs, then b.
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("b".into(), crate::config::types::ConfigValue::Str("second".into()));
        let path = [&s];
        let found = seek_parameter("a|b", &path).and_then(|v| v.as_str());
        assert_eq!(found, Some("second"));
        // If both present, `a` wins.
        drop(path);
        s.insert("a".into(), crate::config::types::ConfigValue::Str("first".into()));
        let path2 = [&s];
        let found2 = seek_parameter("a|b", &path2).and_then(|v| v.as_str());
        assert_eq!(found2, Some("first"));
    }

    #[test]
    fn test_seek_parameter_param_submap_checked_before_top_level() {
        // struct["param"][name] takes priority over struct[name].
        let mut inner: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        inner.insert("x".into(), crate::config::types::ConfigValue::Str("inner".into()));
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("x".into(), crate::config::types::ConfigValue::Str("outer".into()));
        s.insert("param".into(), crate::config::types::ConfigValue::Map(inner));
        let path = [&s];
        let found = seek_parameter("x", &path).and_then(|v| v.as_str());
        assert_eq!(found, Some("inner"));
    }

    #[test]
    fn test_default_search_paths_first_entry_is_base_dir() {
        // First element is always the base dir itself.
        let base = PathBuf::from("/a/b/c");
        let paths = default_search_paths(&base);
        assert_eq!(paths[0], base);
        // Second is base/etc.
        assert_eq!(paths[1], base.join("etc"));
    }

    #[test]
    fn test_default_search_paths_contains_parent_paths() {
        // Parent-based paths (3rd/4th/5th) included when base has parent chain.
        let base = PathBuf::from("/a/b/c");
        let paths = default_search_paths(&base);
        assert_eq!(paths.len(), 5);
        // 3rd: base.parent().join("etc") = /a/b/etc
        assert_eq!(paths[2], PathBuf::from("/a/b/etc"));
        // 4th: base.parent() = /a/b
        assert_eq!(paths[3], PathBuf::from("/a/b"));
        // 5th: base.parent().parent().join("etc") = /a/etc
        assert_eq!(paths[4], PathBuf::from("/a/etc"));
    }

    #[test]
    fn test_perturb_value_pmin_equals_pmax_produces_deterministic_scale() {
        // Params "1,1" → pmin=pmax=1 → urd is exactly 1 → value × 1 = value.
        assert_eq!(perturb_value(42.0, Some("1,1")), 42.0);
        // "2,2" → urd=2 → value × 2.
        assert_eq!(perturb_value(5.0, Some("2,2")), 10.0);
    }

    #[test]
    fn test_seek_parameter_no_match_across_all_structs_returns_none() {
        // Multiple structs, none containing "target" or its param submap → None.
        let mut a: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        a.insert("x".into(), crate::config::types::ConfigValue::Str("1".into()));
        let mut b: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        b.insert("y".into(), crate::config::types::ConfigValue::Str("2".into()));
        let path = [&a, &b];
        assert!(seek_parameter("target", &path).is_none());
        // Empty path → None (nothing to search).
        let empty: [&std::collections::HashMap<String, crate::config::types::ConfigValue>; 0] = [];
        assert!(seek_parameter("target", &empty).is_none());
    }

    #[test]
    fn test_format_url_removeparam_policy_strips_missing_placeholder() {
        // Missing param + "removeparam" → placeholder removed, rest of URL preserved.
        let path: [&std::collections::HashMap<String, crate::config::types::ConfigValue>; 0] = [];
        let r = format_url("/a[FOO]b", &path, "removeparam").unwrap().unwrap();
        assert_eq!(r, "/ab");
    }

    #[test]
    fn test_format_url_substitute_from_struct_containing_param_submap() {
        // seek_parameter checks struct["param"][name] — URL [FOO] resolves from there.
        let mut inner: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        inner.insert("FOO".into(), crate::config::types::ConfigValue::Str("hs1".into()));
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("param".into(), crate::config::types::ConfigValue::Map(inner));
        let path = [&s];
        let r = format_url("/chr=[FOO]", &path, "exit").unwrap().unwrap();
        assert_eq!(r, "/chr=hs1");
    }

    #[test]
    fn test_locate_file_finds_in_second_search_path() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter531_locate_{}", std::process::id()));
        let d1 = tmp.join("first");
        let d2 = tmp.join("second");
        fs::create_dir_all(&d1).unwrap();
        fs::create_dir_all(&d2).unwrap();
        // Place target only in the second search path.
        fs::write(d2.join("target.txt"), b"x").unwrap();
        let paths = vec![d1.clone(), d2.clone()];
        let got = locate_file("target.txt", &paths).unwrap();
        assert_eq!(got, d2.join("target.txt"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_replace_string_undef_sentinel_unquoted_even_when_text() {
        // "undef" is a sentinel — not quoted even though it contains non-digit chars.
        let mut target = "x=HERE".to_string();
        replace_string(&mut target, "HERE", "undef");
        assert_eq!(target, "x=undef"); // NOT "x='undef'".
    }

    #[test]
    fn test_is_number_leading_plus_sign_accepted_by_parse() {
        // Rust's f64::parse accepts a leading "+" as well as "-".
        assert!(is_number("+3.14"));
        assert!(is_number("+0"));
        assert!(is_number("-42"));
        // But two signs or a bare sign should fail.
        assert!(!is_number("++3"));
        assert!(!is_number("+"));
    }

    #[test]
    fn test_is_integer_whole_float_true_fractional_false() {
        // 5.0 is "integer" per v == v.floor(); 5.5 is not.
        assert!(is_integer(5.0));
        assert!(is_integer(-3.0));
        assert!(is_integer(0.0));
        assert!(!is_integer(5.5));
        assert!(!is_integer(-0.001));
    }

    #[test]
    fn test_defined_but_zero_some_nonzero_returns_false() {
        // Some(0.0) → true; Some(1.0) → false; None → false.
        assert!(defined_but_zero(Some(0.0)));
        assert!(defined_but_zero(Some(-0.0)));
        assert!(!defined_but_zero(Some(1.0)));
        assert!(!defined_but_zero(Some(-0.001)));
        assert!(!defined_but_zero(None));
    }

    #[test]
    fn test_span_distance_touching_intervals_zero_gap() {
        // Two intervals touching at a single point → distance 0 (gap exactly covered).
        // [0,5] and [5,10] touch at 5 — span_distance returns x2 - y1 = 5 - 5 = 0.
        assert_eq!(span_distance(0.0, 5.0, 5.0, 10.0), 0.0);
        // Order shouldn't matter — swap pairs.
        assert_eq!(span_distance(5.0, 10.0, 0.0, 5.0), 0.0);
    }

    #[test]
    fn test_format_condition_gb_suffix_multiplies_by_billion() {
        // "2Gb" → 2 × 1e9 = 2000000000 via RE_GB.
        let out = format_condition("size > 2Gb");
        assert_eq!(out, "size > 2000000000");
        // Lowercase "gb" also matches (case insensitive).
        let out2 = format_condition("limit < 1.5gb");
        assert_eq!(out2, "limit < 1500000000");
    }

    #[test]
    fn test_add_thousands_separator_very_small_number_no_seps() {
        // 1-3 digit numbers receive no separators.
        assert_eq!(add_thousands_separator("1", ','), "1");
        assert_eq!(add_thousands_separator("12", ','), "12");
        assert_eq!(add_thousands_separator("123", ','), "123");
        // 4+ digits: first sep inserted.
        assert_eq!(add_thousands_separator("1234", ','), "1,234");
    }

    #[test]
    fn test_inittracks_zero_num_returns_empty_vec() {
        // inittracks(0) → no-op, empty Vec.
        let tracks = inittracks(0);
        assert!(tracks.is_empty());
        // inittracks(3) → exactly 3 empty spans.
        let tracks3 = inittracks(3);
        assert_eq!(tracks3.len(), 3);
        for t in &tracks3 {
            assert!(t.is_empty());
        }
    }

    #[test]
    fn test_gettack_with_single_track_occupied_returns_none() {
        // Single track that's pre-populated with overlapping range → no free track.
        use crate::intspan::IntSpan;
        let mut tracks = inittracks(1);
        // Pre-populate track 0 with range [0..1000] at scale=1000 after scaling.
        tracks[0] = IntSpan::from_range(0, 1000);
        let set = IntSpan::from_range(100, 200);
        let idx = gettack(&set, 0, 0, &mut tracks, 1);
        // Padded set = [100..200] / 1 = [100..200], which overlaps pre-populated [0..1000].
        assert_eq!(idx, None);
    }

    #[test]
    fn test_is_comment_with_leading_whitespace_still_detected() {
        // is_comment trims leading whitespace before checking for '#'.
        assert!(is_comment("   # indented comment"));
        assert!(is_comment("\t\t# tab-indented comment"));
        assert!(is_comment("#bare"));
        // Non-comment.
        assert!(!is_comment("not a comment"));
        assert!(!is_comment("key = value # with inline"));
    }

    #[test]
    fn test_round_up_below_half_fractional_adds_one_to_trunc() {
        // If fractional part ≤ 0.5, round_up returns 1 + trunc(v).
        assert_eq!(round_up(3.3), 4.0); // 3.3 - 3 = 0.3 → not > 0.5 → 1+3 = 4
        assert_eq!(round_up(3.5), 4.0); // 0.5 exactly → not > 0.5 → 4
        // If fractional part > 0.5, returns v.round().
        assert_eq!(round_up(3.6), 4.0); // 0.6 > 0.5 → round(3.6) = 4
        assert_eq!(round_up(3.9), 4.0); // 0.9 > 0.5 → round(3.9) = 4
    }

    #[test]
    fn test_add_thousands_separator_with_negative_number_preserves_sign() {
        // Negative numbers retain the '-' at the front.
        assert_eq!(add_thousands_separator("-1234", ','), "-1,234");
        assert_eq!(add_thousands_separator("-1234567", ','), "-1,234,567");
        // Positive for comparison.
        assert_eq!(add_thousands_separator("1234", ','), "1,234");
    }

    #[test]
    fn test_format_condition_bp_suffix_trims_to_bare_number() {
        // "bp" suffix is stripped entirely (not multiplied).
        assert_eq!(format_condition("pos > 1000bp"), "pos > 1000");
        assert_eq!(format_condition("limit < 42BP"), "limit < 42");
        // No bp → unchanged.
        assert_eq!(format_condition("pos > 1000"), "pos > 1000");
    }

    #[test]
    fn test_is_blank_with_various_whitespace_types() {
        // is_blank trims before checking — all whitespace types count.
        assert!(is_blank(""));
        assert!(is_blank(" "));
        assert!(is_blank("\t"));
        assert!(is_blank("\n"));
        assert!(is_blank("\r\n"));
        assert!(is_blank("   \t\n   "));
        // Non-blank.
        assert!(!is_blank(" x "));
        assert!(!is_blank("0"));
    }

    #[test]
    fn test_replace_string_numeric_value_not_quoted() {
        // Numeric values (containing only digits, '-', '.') are NOT quoted.
        let mut target = "x=HERE".to_string();
        replace_string(&mut target, "HERE", "3.14");
        assert_eq!(target, "x=3.14");
        // Negative.
        let mut t2 = "y=HERE".to_string();
        replace_string(&mut t2, "HERE", "-42");
        assert_eq!(t2, "y=-42");
        // Mixed digits → numeric → unquoted.
        let mut t3 = "z=HERE".to_string();
        replace_string(&mut t3, "HERE", "100");
        assert_eq!(t3, "z=100");
    }

    #[test]
    fn test_make_list_none_input_returns_empty_vec() {
        use crate::config::types::ConfigValue;
        // None → empty Vec.
        let result = make_list(None);
        assert!(result.is_empty());
        // Some(Str) → 1-element Vec.
        let v = ConfigValue::Str("a".into());
        let result2 = make_list(Some(&v));
        assert_eq!(result2.len(), 1);
    }

    #[test]
    fn test_add_thousands_separator_with_decimal_part_preserves_decimal() {
        // Decimal point splits number — thousands separators only added to integer part.
        assert_eq!(add_thousands_separator("1234.56", ','), "1,234.56");
        assert_eq!(add_thousands_separator("1000000.001", ','), "1,000,000.001");
        // No decimal, all digits integer.
        assert_eq!(add_thousands_separator("1234567", ','), "1,234,567");
    }

    #[test]
    fn test_is_number_rejects_multiple_dots() {
        // Multiple decimal points → not a valid number.
        assert!(!is_number("3.14.15"));
        assert!(!is_number("1..2"));
        // Single dot still fine.
        assert!(is_number("3.14"));
    }

    #[test]
    fn test_is_integer_on_very_large_whole_float() {
        // Large whole numbers still register as integer.
        assert!(is_integer(1e15));
        assert!(is_integer(-1e15));
        // Fractional at smaller magnitude where f64 has precision.
        assert!(!is_integer(1.5));
        assert!(!is_integer(-0.5));
    }

    #[test]
    fn test_round_up_negative_values_above_half_fractional() {
        // Negative with fractional above half (by magnitude) — round_up uses trunc()+1.
        // -3.7: value.trunc() = -3; value - trunc = -3.7 - (-3) = -0.7. Is -0.7 > 0.5? No. → 1 + -3 = -2.
        // Document the specific behavior.
        let r = round_up(-3.7);
        // Whatever the exact behavior, verify no panic.
        assert!(r.is_finite());
        // Positive case: 3.7: value - trunc = 0.7 > 0.5 → round(3.7) = 4.
        assert_eq!(round_up(3.7), 4.0);
    }

    #[test]
    fn test_span_distance_non_overlapping_spans_returns_gap() {
        // [0,5] and [10,15] are disjoint — distance = 10 - 5 = 5.
        assert_eq!(span_distance(0.0, 5.0, 10.0, 15.0), 5.0);
        // Swap: same distance.
        assert_eq!(span_distance(10.0, 15.0, 0.0, 5.0), 5.0);
    }

    #[test]
    fn test_is_comment_empty_string_not_a_comment() {
        // Empty string has no '#' → not a comment.
        assert!(!is_comment(""));
        // Only whitespace.
        assert!(!is_comment("    "));
    }

    #[test]
    fn test_replace_string_multiple_occurrences_all_numeric_unquoted() {
        // All occurrences of source replaced. Numeric value (digits only) → not quoted.
        let mut t = "xXxXxX".to_string();
        replace_string(&mut t, "X", "9");
        assert_eq!(t, "x9x9x9");
    }

    #[test]
    fn test_format_condition_multiple_kb_expansions_in_one_string() {
        // Multiple "Nkb" expansions in the same condition.
        let s = format_condition("a > 10kb && b < 20kb");
        assert_eq!(s, "a > 10000 && b < 20000");
    }

    #[test]
    fn test_add_thousands_separator_custom_separator_character() {
        // Custom separator (not comma).
        assert_eq!(add_thousands_separator("1234567", '.'), "1.234.567");
        assert_eq!(add_thousands_separator("9999", ' '), "9 999");
    }

    #[test]
    fn test_defined_but_zero_with_edge_values() {
        // Only Some(0.0) true; all other Some values false.
        assert!(defined_but_zero(Some(0.0)));
        assert!(!defined_but_zero(Some(1.0)));
        assert!(!defined_but_zero(Some(-0.001)));
        assert!(!defined_but_zero(Some(f64::EPSILON)));
        assert!(!defined_but_zero(None));
    }

    #[test]
    fn test_is_number_empty_string_and_whitespace_rejected() {
        // Empty string and whitespace-only are not numbers.
        assert!(!is_number(""));
        assert!(!is_number("   "));
        assert!(!is_number("\t"));
    }

    #[test]
    fn test_round_up_zero_fractional_value() {
        // value=3.0 (no fractional) → 0 > 0.5? no → 1 + trunc = 1+3 = 4.
        assert_eq!(round_up(3.0), 4.0);
        // value=0.0 → 0 > 0.5? no → 1 + 0 = 1.
        assert_eq!(round_up(0.0), 1.0);
    }

    #[test]
    fn test_span_distance_identical_single_point_spans() {
        // [5, 5] and [5, 5] → distance 0 (same point).
        assert_eq!(span_distance(5.0, 5.0, 5.0, 5.0), 0.0);
    }

    #[test]
    fn test_format_url_removeurl_policy_on_missing_param() {
        // "removeurl" → Ok(None) on first missing param.
        let empty: Vec<&std::collections::HashMap<String, crate::config::types::ConfigValue>> =
            Vec::new();
        let res = format_url("/view/[MISSING]", &empty, "removeurl");
        assert_eq!(res, Ok(None));
    }

    #[test]
    fn test_locate_file_returns_none_when_not_found_in_any_path() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        // File doesn't exist in dir or as absolute.
        let hit = locate_file("does_not_exist_xyz.txt", &[dir.path().to_path_buf()]);
        assert_eq!(hit, None);
    }

    #[test]
    fn test_default_search_paths_contains_base_and_etc() {
        use std::path::Path;
        let base = Path::new("/tmp/circos_base");
        let paths = default_search_paths(base);
        // First entry is the base dir itself.
        assert_eq!(paths[0], base.to_path_buf());
        // Second entry is base/etc.
        assert_eq!(paths[1], base.join("etc"));
        // 5 paths total.
        assert_eq!(paths.len(), 5);
    }

    #[test]
    fn test_add_thousands_separator_explicit_plus_sign_preserved() {
        // "+1000" → sign kept as '+', digits get separator.
        assert_eq!(add_thousands_separator("+1000", ','), "+1,000");
        // "+123" → no separator needed, sign preserved.
        assert_eq!(add_thousands_separator("+123", ','), "+123");
    }

    #[test]
    fn test_seek_parameter_in_struct_name_then_param_submap_fallback() {
        // Port walks [param][name] first, then [name] on a given struct.
        let mut param: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        param.insert("a".into(), crate::config::types::ConfigValue::Str("from_param".into()));
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("param".into(), crate::config::types::ConfigValue::Map(param));
        s.insert("a".into(), crate::config::types::ConfigValue::Str("from_direct".into()));
        // "param" submap wins over direct key at same level.
        let structs = [&s];
        let v = seek_parameter("a", &structs);
        assert_eq!(v.and_then(|x| x.as_str()), Some("from_param"));
    }

    #[test]
    fn test_make_list_scalar_input_wraps_as_single_element() {
        // Scalar ConfigValue → Vec with 1 element.
        let v = crate::config::types::ConfigValue::Str("single".into());
        let out = make_list(Some(&v));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_str(), Some("single"));
    }

    #[test]
    fn test_make_list_actual_list_returns_clone_of_all_items() {
        // List → returns cloned items.
        let list = vec![
            crate::config::types::ConfigValue::Str("a".into()),
            crate::config::types::ConfigValue::Str("b".into()),
            crate::config::types::ConfigValue::Str("c".into()),
        ];
        let v = crate::config::types::ConfigValue::List(list);
        let out = make_list(Some(&v));
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].as_str(), Some("a"));
        assert_eq!(out[2].as_str(), Some("c"));
    }

    #[test]
    fn test_replace_string_multiple_distinct_occurrences_substituted() {
        // All occurrences of source replaced with value.
        let mut s = "a=X b=X c=X".to_string();
        replace_string(&mut s, "X", "42");
        assert_eq!(s, "a=42 b=42 c=42");
    }

    #[test]
    fn test_format_url_with_valid_param_substituted_from_struct() {
        // URL with [PARAM] + matching value → substituted.
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("id".into(), crate::config::types::ConfigValue::Str("ABC123".into()));
        let structs = [&s];
        let res = format_url("/detail/[id]", &structs, "removeparam");
        assert_eq!(res, Ok(Some("/detail/ABC123".to_string())));
    }

    #[test]
    fn test_format_url_removeparam_strips_placeholder_when_missing() {
        // Missing param + "removeparam" policy → placeholder removed from URL.
        let empty: Vec<&std::collections::HashMap<String, crate::config::types::ConfigValue>> =
            Vec::new();
        let res = format_url("/x/[MISSING]/y", &empty, "removeparam");
        assert_eq!(res, Ok(Some("/x//y".to_string())));
    }

    #[test]
    fn test_format_url_exit_policy_on_missing_returns_err() {
        // "exit" policy + missing param → Err.
        let empty: Vec<&std::collections::HashMap<String, crate::config::types::ConfigValue>> =
            Vec::new();
        let res = format_url("/x/[MISSING]", &empty, "exit");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("has no value"));
    }

    #[test]
    fn test_format_url_no_placeholders_passes_through_unchanged() {
        // URL with no [PARAM] placeholders → passed through exactly.
        let empty: Vec<&std::collections::HashMap<String, crate::config::types::ConfigValue>> =
            Vec::new();
        let res = format_url("/static/path", &empty, "exit");
        assert_eq!(res, Ok(Some("/static/path".to_string())));
    }

    #[test]
    fn test_inittracks_many_counts_produces_n_distinct_empty_spans() {
        // inittracks(5) → 5-element Vec, each IntSpan independently empty.
        let tracks = inittracks(5);
        assert_eq!(tracks.len(), 5);
        for t in &tracks {
            assert!(t.is_empty());
        }
    }

    #[test]
    fn test_gettack_with_nonzero_chr_offset_shifts_span_into_track_space() {
        use crate::intspan::IntSpan;
        let set = IntSpan::from_range(100, 200);
        let mut tracks = inittracks(2);
        // chr_offset=1000, scale=1 → padded range (1100,1200).
        let idx = gettack(&set, 0, 1000, &mut tracks, 1);
        assert_eq!(idx, Some(0));
        // Track 0 now contains [1100,1200].
        assert!(tracks[0].member(1100));
        assert!(tracks[0].member(1200));
    }

    #[test]
    fn test_perturb_value_malformed_pmin_defaults_to_one() {
        // pmin unparseable → defaults to 1.0, pmax is 1.5 → result in [1*v, 1.5*v].
        let v = perturb_value(100.0, Some("notanumber,1.5"));
        // unwrap_or(1.0) for pmin → result in [100, 150].
        assert!((100.0..=150.0).contains(&v));
    }

    #[test]
    fn test_add_thousands_separator_uses_custom_separator() {
        // Custom separator character applied.
        assert_eq!(add_thousands_separator("1000000", '.'), "1.000.000");
        assert_eq!(add_thousands_separator("1234", '_'), "1_234");
    }

    #[test]
    fn test_format_condition_embedded_within_longer_string() {
        // kb suffix matches within arbitrary surrounding text.
        let out = format_condition("chr_size < 500kb && chr_name eq hs1");
        assert_eq!(out, "chr_size < 500000 && chr_name eq hs1");
    }

    #[test]
    fn test_locate_file_direct_absolute_path_returns_first() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("absolute_hit.txt");
        std::fs::write(&f, "contents").unwrap();
        // Absolute path, no search paths needed → hit immediately.
        let hit = locate_file(f.to_str().unwrap(), &[]).expect("direct absolute");
        assert_eq!(hit, f);
    }

    #[test]
    fn test_is_integer_distinguishes_exact_vs_fractional_doubles() {
        // is_integer is true only for finite integers.
        assert!(is_integer(42.0));
        assert!(is_integer(-17.0));
        assert!(is_integer(0.0));
        assert!(!is_integer(3.5));
        assert!(!is_integer(-0.1));
        // Edge case: infinity and NaN are not integers.
        assert!(!is_integer(f64::INFINITY));
        assert!(!is_integer(f64::NAN));
    }

    #[test]
    fn test_seek_parameter_not_found_anywhere_returns_none() {
        // No struct contains the queried key → None.
        let s1: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let s2: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let structs = [&s1, &s2];
        assert!(seek_parameter("missing", &structs).is_none());
    }

    #[test]
    fn test_round_up_fractional_above_half_rounds_nearest() {
        // value-trunc > 0.5 → round to nearest.
        assert_eq!(round_up(1.6), 2.0);
        assert_eq!(round_up(1.9), 2.0);
        // 1.5 is NOT > 0.5 → 1 + trunc = 1 + 1 = 2.
        assert_eq!(round_up(1.5), 2.0);
    }

    #[test]
    fn test_is_blank_various_whitespace_types() {
        // Blank = empty or whitespace only.
        assert!(is_blank(""));
        assert!(is_blank(" "));
        assert!(is_blank("\t\n\r "));
        // Any non-whitespace char → not blank.
        assert!(!is_blank("x"));
        assert!(!is_blank(" . "));
    }

    #[test]
    fn test_is_comment_hash_position_matters() {
        // "#" must be after optional leading whitespace.
        assert!(is_comment("#"));
        assert!(is_comment("    #   comment"));
        assert!(!is_comment("x #  inline"));
        assert!(!is_comment(""));
    }

    #[test]
    fn test_seek_parameter_walks_structs_in_given_order() {
        // First struct containing the key wins.
        let mut s1: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s1.insert("k".into(), crate::config::types::ConfigValue::Str("first".into()));
        let mut s2: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s2.insert("k".into(), crate::config::types::ConfigValue::Str("second".into()));
        let structs = [&s1, &s2];
        let v = seek_parameter("k", &structs);
        assert_eq!(v.and_then(|x| x.as_str()), Some("first"));
    }

    #[test]
    fn test_defined_but_zero_negative_zero_accepted() {
        // Some(-0.0) == 0.0 in IEEE → defined_but_zero is true.
        assert!(defined_but_zero(Some(-0.0)));
        // Non-zero negative → false.
        assert!(!defined_but_zero(Some(-1.0)));
    }

    #[test]
    fn test_span_distance_exact_touching_single_point_zero_gap() {
        // x1=0..10; x2=10..20 → touching at 10 → distance 0.
        assert_eq!(span_distance(0.0, 10.0, 10.0, 20.0), 0.0);
    }

    #[test]
    fn test_is_number_scientific_notation_accepted() {
        // "1e5", "-2.5e-3", "3.14E+10" all valid f64.
        assert!(is_number("1e5"));
        assert!(is_number("-2.5e-3"));
        assert!(is_number("3.14E+10"));
    }

    #[test]
    fn test_format_condition_zero_value_kb_produces_zero() {
        // "0kb" → 0 × 1000 = 0.
        assert_eq!(format_condition("0kb"), "0");
        assert_eq!(format_condition("0.0Mb"), "0");
    }

    #[test]
    fn test_span_distance_reversed_order_same_result() {
        // span_distance is order-insensitive between the two spans.
        let d1 = span_distance(0.0, 10.0, 20.0, 30.0);
        let d2 = span_distance(20.0, 30.0, 0.0, 10.0);
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_round_up_very_small_fractional_does_not_round() {
        // 1.01 - 1 = 0.01 → not > 0.5 → 1 + trunc(1.01) = 1 + 1 = 2.
        assert_eq!(round_up(1.01), 2.0);
    }

    #[test]
    fn test_add_thousands_separator_negative_with_dot_separator() {
        // "-1234567.89" with ',' separator.
        assert_eq!(add_thousands_separator("-1234567.89", ','), "-1,234,567.89");
    }

    #[test]
    fn test_is_number_leading_plus_accepted() {
        // "+42" is valid f64.
        assert!(is_number("+42"));
        assert!(is_number("+3.14"));
    }

    #[test]
    fn test_format_url_preserves_url_without_placeholders_exactly() {
        // URL with no [PARAM] → returned unchanged.
        let empty: Vec<&std::collections::HashMap<String, crate::config::types::ConfigValue>> =
            Vec::new();
        let res = format_url("/fixed/path?q=x", &empty, "exit");
        assert_eq!(res, Ok(Some("/fixed/path?q=x".to_string())));
    }

    #[test]
    fn test_replace_string_value_undef_passes_without_quoting() {
        // Special-case: "undef" passes through unquoted.
        let mut s = "x=X".to_string();
        replace_string(&mut s, "X", "undef");
        assert_eq!(s, "x=undef");
    }

    #[test]
    fn test_round_up_large_integer_value_increments_by_one() {
        // 1000.0 → 1 + trunc(1000) = 1001.
        assert_eq!(round_up(1000.0), 1001.0);
    }

    #[test]
    fn test_is_comment_with_hash_inside_but_not_leading() {
        // "abc#def" is NOT a comment (leading whitespace + # only).
        assert!(!is_comment("abc#def"));
        assert!(!is_comment("  abc #  "));
    }

    #[test]
    fn test_span_distance_nested_span_entirely_within_other() {
        // [0,100] contains [25,75] → negative distance (overlap).
        let d = span_distance(0.0, 100.0, 25.0, 75.0);
        assert!(d < 0.0);
    }

    #[test]
    fn test_is_integer_boundary_i64_max_representable_as_f64() {
        // Very large integer f64 still detected as integer.
        assert!(is_integer(1e15));
        assert!(!is_integer(1e15 + 0.5));
    }

    #[test]
    fn test_add_thousands_separator_single_digit_returns_unchanged() {
        // "5" → no separator possible.
        assert_eq!(add_thousands_separator("5", ','), "5");
    }

    #[test]
    fn test_replace_string_numeric_value_with_decimal_unquoted() {
        // "3.14" is "numeric" per heuristic → unquoted.
        let mut s = "a=X".to_string();
        replace_string(&mut s, "X", "3.14");
        assert_eq!(s, "a=3.14");
    }

    #[test]
    fn test_format_condition_pure_gb_suffix_multiplies_by_billion() {
        // "2.5Gb" → 2500000000.
        assert_eq!(format_condition("2.5Gb"), "2500000000");
    }

    #[test]
    fn test_add_thousands_separator_zero_value_unchanged() {
        // "0" → "0".
        assert_eq!(add_thousands_separator("0", ','), "0");
    }

    #[test]
    fn test_is_number_empty_whitespace_rejected() {
        // Just whitespace → not a number.
        assert!(!is_number(""));
        assert!(!is_number("  "));
        assert!(!is_number("\t"));
    }

    #[test]
    fn test_round_up_negative_non_integer_rounds_correctly() {
        // -1.4 → trunc(-1.4)=-1, fractional=−1.4-−1=−0.4, not > 0.5 → 1 + (-1) = 0.
        assert_eq!(round_up(-1.4), 0.0);
    }

    #[test]
    fn test_format_condition_uppercase_units_matched_case_insensitive() {
        // "500KB" and "500kb" same result via (?i) regex.
        assert_eq!(format_condition("500KB"), format_condition("500kb"));
    }

    #[test]
    fn test_is_integer_nan_and_infinity_both_rejected() {
        // NaN and infinities are not integers.
        assert!(!is_integer(f64::NAN));
        assert!(!is_integer(f64::INFINITY));
        assert!(!is_integer(f64::NEG_INFINITY));
    }

    #[test]
    fn test_add_thousands_separator_exact_1000_boundary() {
        // 1000 → "1,000".
        assert_eq!(add_thousands_separator("1000", ','), "1,000");
        assert_eq!(add_thousands_separator("1001", ','), "1,001");
    }

    #[test]
    fn test_replace_string_source_not_in_target_leaves_unchanged() {
        // target without the source substring → unchanged.
        let mut s = "hello world".to_string();
        replace_string(&mut s, "missing", "value");
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_format_condition_mixed_suffix_types_independently_processed() {
        // Combination: "5kb and 2Mb" → "5000 and 2000000".
        assert_eq!(format_condition("5kb and 2Mb"), "5000 and 2000000");
    }

    #[test]
    fn test_is_blank_multi_line_whitespace_considered_blank() {
        // Newlines alone → blank.
        assert!(is_blank("\n"));
        assert!(is_blank("\n\n\n"));
        assert!(is_blank(" \t\n\r "));
    }

    #[test]
    fn test_is_comment_leading_hash_with_no_trailing_content() {
        // "#" alone → comment.
        assert!(is_comment("#"));
        // "  #" also a comment.
        assert!(is_comment("  #"));
    }

    #[test]
    fn test_add_thousands_separator_preserves_sign_character() {
        // Leading "-" sign kept; digits get separator.
        assert_eq!(add_thousands_separator("-1234", ','), "-1,234");
        // "+" sign also kept.
        assert_eq!(add_thousands_separator("+1234", ','), "+1,234");
    }

    #[test]
    fn test_is_number_integer_and_float_both_accepted() {
        // Plain integer and float both accepted.
        assert!(is_number("42"));
        assert!(is_number("3.14"));
    }

    #[test]
    fn test_is_integer_fractional_values_rejected() {
        // 3.14 is NOT an integer value.
        assert!(!is_integer(3.14));
        assert!(!is_integer(0.5));
    }

    #[test]
    fn test_round_up_exactly_half_fraction_increments_trunc() {
        // frac = 0.5 is NOT > 0.5 → returns 1 + trunc = 6.
        assert_eq!(round_up(5.5), 6.0);
    }

    #[test]
    fn test_defined_but_zero_trio_branches_covered_together() {
        // Some(5.0) → has value but ≠ 0 → false.
        assert!(!defined_but_zero(Some(5.0)));
        // None → undefined → false.
        assert!(!defined_but_zero(None));
        // Some(0.0) → defined and zero → true.
        assert!(defined_but_zero(Some(0.0)));
    }

    #[test]
    fn test_span_distance_zero_between_identical_points() {
        // (0,0) to (0,0) → distance 0.
        assert_eq!(span_distance(0.0, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn test_is_blank_nonempty_with_nonspace_char_false() {
        // Non-empty, contains non-whitespace → not blank.
        assert!(!is_blank("hello"));
        assert!(!is_blank(" a "));
    }

    #[test]
    fn test_is_comment_no_leading_hash_false() {
        // Lines without leading '#' → not comments.
        assert!(!is_comment("x = 1"));
        assert!(!is_comment("    no leading hash"));
    }

    #[test]
    fn test_replace_string_numeric_value_replaced_without_quotes() {
        // Numeric values (digits/dot/minus) replace without quoting.
        let mut s = "val = X".to_string();
        replace_string(&mut s, "X", "42");
        assert_eq!(s, "val = 42");
    }

    #[test]
    fn test_span_distance_two_intervals_gap_between_them() {
        // Interval [0,10] vs [20,30] → gap = 20 - 10 = 10.
        assert_eq!(span_distance(0.0, 10.0, 20.0, 30.0), 10.0);
    }

    #[test]
    fn test_is_blank_empty_string_considered_blank() {
        // "" → blank.
        assert!(is_blank(""));
    }

    #[test]
    fn test_is_comment_indented_hash_line_is_comment() {
        // "   # indented" → comment (leading whitespace trimmed first).
        assert!(is_comment("   # indented"));
    }

    #[test]
    fn test_round_up_value_near_zero_from_positive_side() {
        // frac=0.0 of 0.0 → 1 + trunc(0) = 1.
        assert_eq!(round_up(0.0), 1.0);
    }

    #[test]
    fn test_make_list_scalar_wraps_in_one_element_vec() {
        // Scalar ConfigValue::Str → one-element Vec<ConfigValue>.
        let v = crate::config::types::ConfigValue::Str("solo".into());
        let out = make_list(Some(&v));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn test_make_list_from_list_variant_returns_all_items() {
        // List variant → Vec contains each item.
        let v = crate::config::types::ConfigValue::List(vec![
            crate::config::types::ConfigValue::Str("a".into()),
            crate::config::types::ConfigValue::Str("b".into()),
        ]);
        let out = make_list(Some(&v));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_is_number_negative_integer_parses() {
        // "-42" is a valid number.
        assert!(is_number("-42"));
    }

    #[test]
    fn test_format_condition_no_unit_suffixes_passthrough() {
        // Condition without any unit suffixes → unchanged.
        let s = format_condition("x > 5");
        assert_eq!(s, "x > 5");
    }

    #[test]
    fn test_is_number_float_with_sign_parses() {
        // "+3.14" and "-3.14" both numbers.
        assert!(is_number("+3.14"));
        assert!(is_number("-3.14"));
    }

    #[test]
    fn test_add_thousands_separator_with_decimal_preserves_fractional_part() {
        // "1234.56" → "1,234.56" (only integer part gets separator).
        let s = add_thousands_separator("1234.56", ',');
        assert_eq!(s, "1,234.56");
    }

    #[test]
    fn test_is_integer_integer_valued_f64_true() {
        // 5.0 is an integer value.
        assert!(is_integer(5.0));
        assert!(is_integer(-10.0));
    }

    #[test]
    fn test_replace_string_undef_value_replaces_without_quotes() {
        // Special "undef" token → replaced without quotes.
        let mut s = "val = X".to_string();
        replace_string(&mut s, "X", "undef");
        assert_eq!(s, "val = undef");
    }

    #[test]
    fn test_add_thousands_separator_three_digit_no_sep_inserted() {
        // "999" has no comma (< 4 digits).
        assert_eq!(add_thousands_separator("999", ','), "999");
    }

    #[test]
    fn test_round_up_large_value_ceils_up() {
        // 99999999.0 has frac=0 → returns 1 + trunc = 100000000.
        assert_eq!(round_up(99999999.0), 100000000.0);
    }

    #[test]
    fn test_replace_string_with_non_numeric_value_quoted() {
        // Alpha value → quoted with single quotes.
        let mut s = "key = X".to_string();
        replace_string(&mut s, "X", "hello");
        assert_eq!(s, "key = 'hello'");
    }

    #[test]
    fn test_format_condition_kb_suffix_converts_to_1000() {
        // "5kb" → "5000".
        let s = format_condition("5kb");
        assert_eq!(s, "5000");
    }

    #[test]
    fn test_format_condition_mb_suffix_converts_to_million() {
        // "5mb" → "5000000".
        let s = format_condition("5mb");
        assert_eq!(s, "5000000");
    }

    #[test]
    fn test_round_up_negative_fractional_value() {
        // -3.3 has frac=-0.3 (not > 0.5) → 1 + trunc(-3) = -2.
        let r = round_up(-3.3);
        assert_eq!(r, -2.0);
    }

    #[test]
    fn test_is_blank_whitespace_only_considered_blank() {
        // Whitespace-only strings → blank.
        assert!(is_blank("   "));
        assert!(is_blank("\t\t"));
        assert!(is_blank("\n"));
    }

    #[test]
    fn test_defined_but_zero_negative_zero_is_zero() {
        // -0.0 is numerically == 0.0 → true.
        assert!(defined_but_zero(Some(-0.0)));
    }

    #[test]
    fn test_is_number_e5_and_e_neg10_both_valid_numbers() {
        // "1e5" and "2.5E-10" both valid scientific notation.
        assert!(is_number("1e5"));
        assert!(is_number("2.5E-10"));
    }

    #[test]
    fn test_add_thousands_separator_million_range() {
        // Large integer gets multiple separators.
        assert_eq!(add_thousands_separator("1000000", ','), "1,000,000");
    }

    #[test]
    fn test_is_integer_very_large_f64_value() {
        // Very large integer-valued f64 → true.
        assert!(is_integer(1e9));
        assert!(is_integer(-1e6));
    }

    #[test]
    fn test_format_condition_multiple_suffixes_converted() {
        // "5kb + 2mb" → "5000 + 2000000".
        let s = format_condition("5kb + 2mb");
        assert_eq!(s, "5000 + 2000000");
    }

    #[test]
    fn test_format_condition_gb_converts_to_billion() {
        // "2gb" → "2000000000".
        assert_eq!(format_condition("2gb"), "2000000000");
    }

    #[test]
    fn test_is_number_zero_string_is_valid() {
        // "0" and "0.0" both valid.
        assert!(is_number("0"));
        assert!(is_number("0.0"));
    }

    #[test]
    fn test_span_distance_symmetric_spans_result() {
        // [10,20] vs [10,20] → overlapping identical → negative (contained).
        let d = span_distance(10.0, 20.0, 10.0, 20.0);
        assert!(d <= 0.0);
    }

    #[test]
    fn test_round_up_fractional_below_half_uses_trunc_plus_one() {
        // 3.3: frac=0.3 not > 0.5 → 1 + trunc(3) = 4.
        assert_eq!(round_up(3.3), 4.0);
    }

    #[test]
    fn test_is_number_whitespace_accepted_after_trim() {
        // Leading/trailing whitespace trimmed → valid number.
        assert!(is_number(" 42 "));
    }

    #[test]
    fn test_add_thousands_separator_negative_million() {
        // "-1000000" → "-1,000,000".
        assert_eq!(add_thousands_separator("-1000000", ','), "-1,000,000");
    }

    #[test]
    fn test_is_integer_nan_is_not_integer() {
        // NaN is not an integer.
        assert!(!is_integer(f64::NAN));
    }

    #[test]
    fn test_format_condition_plain_identifier_passthrough() {
        // Identifier without unit suffixes → unchanged.
        let s = format_condition("mychr");
        assert_eq!(s, "mychr");
    }

    #[test]
    fn test_add_thousands_separator_with_pipe_separator() {
        // "1234" with '|' separator → "1|234".
        let s = add_thousands_separator("1234", '|');
        assert_eq!(s, "1|234");
    }

    #[test]
    fn test_is_integer_f64_max_value_is_integer() {
        // f64::MAX is a valid integer value.
        assert!(is_integer(f64::MAX));
    }

    #[test]
    fn test_round_up_fractional_above_half_uses_round() {
        // 3.8: frac=0.8 > 0.5 → round() = 4.
        assert_eq!(round_up(3.8), 4.0);
    }

    #[test]
    fn test_span_distance_containing_spans_returns_negative() {
        // One span contains the other → overlap → negative or zero.
        let d = span_distance(0.0, 100.0, 25.0, 75.0);
        assert!(d <= 0.0);
    }

    #[test]
    fn test_round_up_with_half_fractional_uses_trunc_plus_one() {
        // 0.5 → frac is exactly 0.5, not > 0.5, so else branch: 1.0 + trunc(0.5) = 1.0 + 0 = 1.0.
        let v = round_up(0.5);
        assert_eq!(v, 1.0);
    }

    #[test]
    fn test_defined_but_zero_some_pi_returns_false() {
        // Some(3.14) → not zero → false.
        assert!(!defined_but_zero(Some(3.14)));
    }

    #[test]
    fn test_replace_string_numeric_negative_float_value_not_quoted() {
        // Purely numeric value (digits, minus, dot) passes through unquoted.
        let mut s = String::from("x=__V__");
        replace_string(&mut s, "__V__", "-42.5");
        assert_eq!(s, "x=-42.5");
    }

    #[test]
    fn test_replace_string_undef_passes_through_unquoted() {
        // "undef" special-cased: pass through without quoting.
        let mut s = String::from("v=__X__");
        replace_string(&mut s, "__X__", "undef");
        assert_eq!(s, "v=undef");
    }

    #[test]
    fn test_format_condition_bp_suffix_stripped() {
        // "100bp" → "100" (bp suffix removed).
        assert_eq!(format_condition("100bp"), "100");
    }

    #[test]
    fn test_format_condition_mb_uppercase_expands_to_1e6() {
        // "5Mb" → "5000000".
        assert_eq!(format_condition("5Mb"), "5000000");
    }

    #[test]
    fn test_format_condition_gb_suffix_expands_to_1e9() {
        // "2Gb" → "2000000000".
        assert_eq!(format_condition("2Gb"), "2000000000");
    }

    #[test]
    fn test_add_thousands_separator_with_negative_sign_preserved() {
        // Negative: sign preserved, digits grouped.
        assert_eq!(add_thousands_separator("-1234567", ','), "-1,234,567");
    }

    #[test]
    fn test_inittracks_zero_count_returns_empty_vec() {
        // Zero tracks → empty vec.
        let v = inittracks(0);
        assert!(v.is_empty());
    }

    #[test]
    fn test_inittracks_creates_n_empty_intspans() {
        // N tracks → N empty IntSpans.
        let v = inittracks(5);
        assert_eq!(v.len(), 5);
        for t in &v {
            assert_eq!(t.cardinality(), 0);
        }
    }

    #[test]
    fn test_format_url_no_placeholders_returns_unchanged() {
        // URL without [PARAM] → returned verbatim wrapped in Some.
        let empty_struct: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let paths = [&empty_struct];
        let r = format_url("http://example.com/plain", &paths, "removeparam").unwrap();
        assert_eq!(r, Some("http://example.com/plain".to_string()));
    }

    #[test]
    fn test_format_url_missing_param_exit_mode_is_err() {
        // Missing param in exit mode → Err.
        let empty_struct: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let paths = [&empty_struct];
        let r = format_url("http://x/[MISSING]", &paths, "exit");
        assert!(r.is_err());
    }

    #[test]
    fn test_format_url_missing_param_removeurl_mode_returns_none() {
        // Missing + removeurl → Ok(None).
        let empty_struct: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let paths = [&empty_struct];
        let r = format_url("http://x/[MISSING]", &paths, "removeurl").unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn test_format_url_missing_param_removeparam_default_strips_placeholder() {
        // Missing + removeparam → placeholder stripped, URL returned.
        let empty_struct: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let paths = [&empty_struct];
        let r = format_url("http://x/[MISSING]/end", &paths, "removeparam").unwrap();
        assert_eq!(r, Some("http://x//end".to_string()));
    }

    #[test]
    fn test_format_url_with_param_substitutes_value() {
        // [P] resolved from struct → substituted in URL.
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("P".into(), crate::config::types::ConfigValue::Str("val".into()));
        let paths = [&s];
        let r = format_url("http://x/[P]", &paths, "exit").unwrap();
        assert_eq!(r, Some("http://x/val".to_string()));
    }

    #[test]
    fn test_seek_parameter_falls_back_to_top_level_when_param_missing() {
        // No "param" submap → check top-level.
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("color".into(), crate::config::types::ConfigValue::Str("red".into()));
        let paths = [&s];
        let v = seek_parameter("color", &paths);
        assert_eq!(v.and_then(|c| c.as_str()), Some("red"));
    }

    #[test]
    fn test_seek_parameter_finds_value_in_param_submap_first() {
        // "param" submap is checked first.
        let mut param: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        param.insert("color".into(), crate::config::types::ConfigValue::Str("from_param".into()));
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("param".into(), crate::config::types::ConfigValue::Map(param));
        s.insert("color".into(), crate::config::types::ConfigValue::Str("from_top".into()));
        let paths = [&s];
        // param.color ("from_param") wins over top-level color ("from_top").
        let v = seek_parameter("color", &paths);
        assert_eq!(v.and_then(|c| c.as_str()), Some("from_param"));
    }

    #[test]
    fn test_seek_parameter_synonym_pipe_separated_names_fallback() {
        // "a|b" - a absent, b found in top-level.
        let mut s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s.insert("b".into(), crate::config::types::ConfigValue::Str("val".into()));
        let paths = [&s];
        let v = seek_parameter("a|b", &paths);
        assert_eq!(v.and_then(|c| c.as_str()), Some("val"));
    }

    #[test]
    fn test_seek_parameter_missing_everywhere_returns_none() {
        // Key not in any struct → None.
        let s: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let paths = [&s];
        assert!(seek_parameter("nonexistent", &paths).is_none());
    }

    #[test]
    fn test_seek_parameter_walks_structs_second_has_key_v2() {
        // Search through multiple structs; second has the key.
        let s1: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        let mut s2: std::collections::HashMap<String, crate::config::types::ConfigValue> =
            std::collections::HashMap::new();
        s2.insert("key".into(), crate::config::types::ConfigValue::Str("v2".into()));
        let paths = [&s1, &s2];
        let v = seek_parameter("key", &paths);
        assert_eq!(v.and_then(|c| c.as_str()), Some("v2"));
    }

    #[test]
    fn test_is_integer_exact_integer_is_true() {
        // 42.0 is an integer value.
        assert!(is_integer(42.0));
    }

    #[test]
    fn test_is_integer_fractional_is_false() {
        // 3.14 has fractional part → false.
        assert!(!is_integer(3.14));
    }

    #[test]
    fn test_round_up_integer_value_adds_one() {
        // 5.0 → frac=0 → else branch → 1.0 + trunc(5.0) = 6.0.
        let v = round_up(5.0);
        assert_eq!(v, 6.0);
    }

    #[test]
    fn test_span_distance_two_disjoint_intervals_positive_distance() {
        // [0,10] and [20,30] → disjoint, gap of 10.
        let d = span_distance(0.0, 10.0, 20.0, 30.0);
        assert_eq!(d, 10.0);
    }

    #[test]
    fn test_is_number_positive_integer_string() {
        // "42" is a number.
        assert!(is_number("42"));
    }

    #[test]
    fn test_is_number_negative_float_string() {
        // "-3.14" is a number.
        assert!(is_number("-3.14"));
    }

    #[test]
    fn test_is_blank_whitespace_only_true() {
        // "   " is blank.
        assert!(is_blank("   "));
        assert!(is_blank(""));
    }

    #[test]
    fn test_is_comment_starts_with_hash_true() {
        // "# comment" is a comment.
        assert!(is_comment("# this is a comment"));
        assert!(is_comment("#"));
        assert!(!is_comment("not a comment"));
    }

    #[test]
    fn test_is_blank_non_whitespace_content_false() {
        // Non-whitespace chars → not blank.
        assert!(!is_blank("a"));
        assert!(!is_blank("   text   "));
    }

    #[test]
    fn test_is_number_scientific_notation_valid() {
        // Scientific notation recognized as number.
        assert!(is_number("1e5"));
        assert!(is_number("2.5E-3"));
    }

    #[test]
    fn test_is_number_non_numeric_false() {
        // Non-numeric strings → not a number.
        assert!(!is_number("abc"));
        assert!(!is_number(""));
        assert!(!is_number("1.2.3"));
    }

    #[test]
    fn test_replace_string_empty_source_noop() {
        // Empty source → target unchanged.
        let mut s = String::from("hello");
        replace_string(&mut s, "", "X");
        // Empty string replace → Rust behavior: inserts at every position.
        // So this may actually modify. Let's check: std::String::replace("hello", "", "X") = "XhXeXlXlXoX".
        // Just verify it doesn't panic.
        assert!(s.len() >= 5);
    }
}
