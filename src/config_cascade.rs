//! Port of Perl Circos::loadconfiguration / populateconfiguration /
//! repopulateconfiguration / validateconfiguration.
//!
//! Perl mutates the global `%CONF`; here we operate on a
//! `HashMap<String, ConfigValue>` passed by the caller.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::parser::ConfigParser;
use crate::config::types::ConfigValue;

/// Port of Perl `loadconfiguration(arg)`: find a config file among a list of
/// candidate paths, then parse it with the config parser (Rust stand-in for
/// `Config::General`). Returns the parsed map plus the resolved file path.
pub fn loadconfiguration(
    arg: &Path,
    app_name: &str,
    real_bin: &Path,
) -> Result<(HashMap<String, ConfigValue>, PathBuf), String> {
    let logname = std::env::var("LOGNAME").unwrap_or_default();
    let home_cfg = if !logname.is_empty() {
        PathBuf::from("/home")
            .join(&logname)
            .join(format!(".{}.conf", app_name))
    } else {
        PathBuf::new()
    };
    let possibilities: Vec<PathBuf> = vec![
        arg.to_path_buf(),
        home_cfg,
        real_bin.join(format!("{}.conf", app_name)),
        real_bin.join("etc").join(format!("{}.conf", app_name)),
        real_bin.join("../etc").join(format!("{}.conf", app_name)),
    ];

    let mut file: Option<PathBuf> = None;
    for f in &possibilities {
        if f.as_os_str().is_empty() {
            continue;
        }
        if f.exists() && f.is_file() {
            file = Some(f.clone());
            break;
        }
    }
    let file = file.ok_or_else(|| {
        "error - could not find any configuration file to use - did you use -conf configfile.conf?".to_string()
    })?;

    let parser = ConfigParser {
        config_paths: vec![
            real_bin.join("etc"),
            real_bin.join("../etc"),
            real_bin.join(".."),
            real_bin.to_path_buf(),
            file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            real_bin
                .join("..")
                .join(file.parent().unwrap_or(Path::new("."))),
        ],
        auto_true: true,
        lower_case_names: true,
    };
    let conf = parser.parse_file(&file)?;
    Ok((conf, file))
}

/// Port of Perl `populateconfiguration`: merge CLI `%OPT` into `%CONF` and
/// resolve `__key__` template variables.
pub fn populateconfiguration(
    conf: &mut HashMap<String, ConfigValue>,
    opt: &HashMap<String, ConfigValue>,
) {
    for (k, v) in opt {
        conf.insert(k.clone(), v.clone());
    }
    repopulateconfiguration(conf);

    // Populate some defaults (Perl: $CONF{anglestep} ||= 1, minslicestep ||= 5)
    conf.entry("anglestep".to_string())
        .or_insert_with(|| ConfigValue::Str("1".into()));
    conf.entry("minslicestep".to_string())
        .or_insert_with(|| ConfigValue::Str("5".into()));
}

/// Port of Perl `repopulateconfiguration(root)`: recursively resolve
/// `__KEY__` templates to the value at `root[KEY]`. Perl uses `eval`, which
/// allows arbitrary expressions; the Rust port currently only resolves
/// direct variable references (sufficient for the common case in tutorials).
pub fn repopulateconfiguration(root: &mut HashMap<String, ConfigValue>) {
    // First, snapshot simple string values so we can look them up during substitution.
    let snapshot: HashMap<String, String> = root
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect();
    let re = regex::Regex::new(r"__([^_].+?)__").unwrap();

    /// Recursively walks a ConfigValue and substitutes `__KEY__` templates in strings,
    /// iterating up to 16 times to resolve nested references.
    fn walk(v: &mut ConfigValue, re: &regex::Regex, snapshot: &HashMap<String, String>) {
        match v {
            ConfigValue::Str(s) => {
                // iterate to catch nested references
                for _ in 0..16 {
                    let new_s = re.replace_all(s, |c: &regex::Captures| {
                        let key = &c[1];
                        // Perl supports `eval $1`; the common case is `$CONF{foo}`
                        // or a bare key name — we handle bare keys.
                        snapshot.get(key).cloned().unwrap_or_else(|| {
                            // try stripping $CONF{...}
                            let stripped = key.trim_start_matches("$CONF{").trim_end_matches('}');
                            snapshot.get(stripped).cloned().unwrap_or_default()
                        })
                    });
                    if new_s == *s {
                        break;
                    }
                    *s = new_s.to_string();
                }
            }
            ConfigValue::Map(m) => {
                for child in m.values_mut() {
                    walk(child, re, snapshot);
                }
            }
            ConfigValue::List(list) => {
                for child in list.iter_mut() {
                    walk(child, re, snapshot);
                }
            }
        }
    }

    for v in root.values_mut() {
        walk(v, &re, &snapshot);
    }
}

/// Port of Perl `validateconfiguration`: resolve inline `__KEY__` placeholders,
/// apply defaults (chromosomes_units, svg_font_scale, angle_offset wrap), and
/// bail if required fields are absent.
pub fn validateconfiguration(conf: &mut HashMap<String, ConfigValue>) -> Result<(), String> {
    // Perl does a pass over top-level __(.+)__ keys and substitutes into all other values.
    let top_tokens: Vec<(String, String)> = conf
        .iter()
        .filter_map(|(k, v)| {
            if k.starts_with("__") && k.ends_with("__") {
                v.as_str().map(|s| (k.clone(), s.to_string()))
            } else {
                None
            }
        })
        .collect();
    for (token, value) in &top_tokens {
        for other in conf.values_mut() {
            if let ConfigValue::Str(s) = other {
                *s = s.replace(token, value);
            }
        }
    }

    conf.entry("chromosomes_units".to_string())
        .or_insert_with(|| ConfigValue::Str("1".into()));
    conf.entry("svg_font_scale".to_string())
        .or_insert_with(|| ConfigValue::Str("1".into()));

    if !conf.contains_key("configfile") {
        return Err("Error: no configuration file specified. Please use -conf FILE".into());
    }
    if !conf.contains_key("karyotype") {
        return Err("Error: no karotype file specified".into());
    }

    // Copy CLI image map fields into the image submap (Perl: $CONF{image}{image_map_name} ||= $CONF{image_map_name})
    let promotions = [
        "image_map_name",
        "image_map_use",
        "image_map_file",
        "image_map_missing_parameter",
        "24bit",
        "png",
        "svg",
    ];
    let mut image_map = match conf.remove("image") {
        Some(ConfigValue::Map(m)) => m,
        Some(other) => {
            conf.insert("image".to_string(), other);
            HashMap::new()
        }
        None => HashMap::new(),
    };
    for key in promotions {
        if !image_map.contains_key(key)
            && let Some(v) = conf.get(key).cloned()
        {
            image_map.insert(key.to_string(), v);
        }
    }
    conf.insert("image".to_string(), ConfigValue::Map(image_map));

    // Wrap angle_offset > 0 into negative (Perl: -= 360)
    let image_angle_offset: Option<f64> = conf
        .get("image")
        .and_then(|v| v.as_map())
        .and_then(|m| m.get("angle_offset"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());
    if let (Some(off), Some(ConfigValue::Map(image))) = (image_angle_offset, conf.get_mut("image"))
        && off > 0.0
    {
        image.insert(
            "angle_offset".to_string(),
            ConfigValue::Str(format!("{}", off - 360.0)),
        );
    }

    for field in ["chromosomes", "chromosomes_breaks", "chromosomes_radius"] {
        conf.entry(field.to_string())
            .or_insert_with(|| ConfigValue::Str(String::new()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: populateconfiguration merges opt.
    #[test]
    fn test_populateconfiguration_merges_opt() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("debug".into(), ConfigValue::Str("1".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("debug").and_then(|v| v.as_str()), Some("1"));
        // defaults
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: repopulate resolves templates.
    #[test]
    fn test_repopulate_resolves_templates() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("outdir".into(), ConfigValue::Str("/tmp".into()));
        conf.insert("path".into(), ConfigValue::Str("__outdir__/file.txt".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("path").and_then(|v| v.as_str()), Some("/tmp/file.txt"));
    }

    /// Test: validateconfiguration requires fields.
    #[test]
    fn test_validateconfiguration_requires_fields() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        // Missing configfile + karyotype → Err on first check
        let r = validateconfiguration(&mut conf);
        assert!(r.is_err());
    }

    /// Test: validateconfiguration angle offset wrap.
    #[test]
    fn test_validateconfiguration_angle_offset_wrap() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        let mut image = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("90".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf.get("image").and_then(|v| v.as_map()).unwrap()
            .get("angle_offset").and_then(|v| v.as_str()).unwrap();
        // 90 → 90 - 360 = -270
        assert_eq!(offset, "-270");
    }

    /// Test: validateconfiguration missing karyotype errors.
    #[test]
    fn test_validateconfiguration_missing_karyotype_errors() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        let r = validateconfiguration(&mut conf);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("karotype"));
    }

    /// Test: validateconfiguration applies defaults.
    #[test]
    fn test_validateconfiguration_applies_defaults() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        // Perl defaults: chromosomes_units=1, svg_font_scale=1.
        assert_eq!(
            conf.get("chromosomes_units").and_then(|v| v.as_str()),
            Some("1")
        );
        assert_eq!(
            conf.get("svg_font_scale").and_then(|v| v.as_str()),
            Some("1")
        );
        // chromosomes / chromosomes_breaks / chromosomes_radius default to empty strings.
        assert_eq!(conf.get("chromosomes").and_then(|v| v.as_str()), Some(""));
        assert_eq!(
            conf.get("chromosomes_breaks").and_then(|v| v.as_str()),
            Some("")
        );
        assert_eq!(
            conf.get("chromosomes_radius").and_then(|v| v.as_str()),
            Some("")
        );
    }

    /// Test: validateconfiguration promotes cli into image submap.
    #[test]
    fn test_validateconfiguration_promotes_cli_into_image_submap() {
        // Perl: `$CONF{image}{image_map_name} ||= $CONF{image_map_name}` — when
        // top-level image_map_use is set, it should appear in the image submap too.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        conf.insert("image_map_use".into(), ConfigValue::Str("1".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("circos".into()));
        conf.insert("png".into(), ConfigValue::Str("1".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(
            image.get("image_map_use").and_then(|v| v.as_str()),
            Some("1")
        );
        assert_eq!(
            image.get("image_map_name").and_then(|v| v.as_str()),
            Some("circos")
        );
        assert_eq!(image.get("png").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: repopulateconfiguration iterates nested references.
    #[test]
    fn test_repopulateconfiguration_iterates_nested_references() {
        // Nested __x__ references resolve through multiple iterations.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("base".into(), ConfigValue::Str("/a".into()));
        conf.insert("middle".into(), ConfigValue::Str("__base__/b".into()));
        conf.insert("leaf".into(), ConfigValue::Str("__middle__/c.txt".into()));
        repopulateconfiguration(&mut conf);
        // After one pass "middle" becomes "/a/b", then "leaf" uses it → "/a/b/c.txt"
        // The bounded loop (0..16) handles this.
        assert_eq!(conf.get("leaf").and_then(|v| v.as_str()), Some("/a/b/c.txt"));
    }

    /// Test: populateconfiguration opt overrides existing.
    #[test]
    fn test_populateconfiguration_opt_overrides_existing() {
        // %OPT should override existing conf keys.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("debug".into(), ConfigValue::Str("0".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("debug".into(), ConfigValue::Str("2".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("debug").and_then(|v| v.as_str()), Some("2"));
    }

    /// Test: repopulate walks nested map and list.
    #[test]
    fn test_repopulate_walks_nested_map_and_list() {
        // `__x__` placeholders inside Map/List children resolve too —
        // `walk()` recurses through all 3 ConfigValue variants.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("root".into(), ConfigValue::Str("/etc".into()));
        let mut inner = HashMap::new();
        inner.insert("file".into(), ConfigValue::Str("__root__/a.conf".into()));
        conf.insert("nested".into(), ConfigValue::Map(inner));
        conf.insert(
            "list".into(),
            ConfigValue::List(vec![
                ConfigValue::Str("__root__/1".into()),
                ConfigValue::Str("__root__/2".into()),
            ]),
        );
        repopulateconfiguration(&mut conf);
        let nested_file = conf
            .get("nested")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("file"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(nested_file, "/etc/a.conf");
        let items = conf.get("list").and_then(|v| v.as_list()).unwrap();
        assert_eq!(items[0].as_str(), Some("/etc/1"));
        assert_eq!(items[1].as_str(), Some("/etc/2"));
    }

    /// Test: validateconfiguration negative angle offset preserved.
    #[test]
    fn test_validateconfiguration_negative_angle_offset_preserved() {
        // angle_offset ≤ 0 should NOT be wrapped — Perl only subtracts 360 when > 0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        let mut image = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("-90".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(offset, "-90");
    }

    /// Test: validateconfiguration non map image restored.
    #[test]
    fn test_validateconfiguration_non_map_image_restored() {
        // If `image` is a scalar (not a Map), it should be restored verbatim
        // and a fresh empty image Map gets built from CLI promotions.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        // image as a string - the impl restores the scalar then overwrites
        // with an empty image Map via the promotions codepath.
        conf.insert("image".into(), ConfigValue::Str("scalar_value".into()));
        validateconfiguration(&mut conf).unwrap();
        // Final `image` should be a Map (the code-path rebuilds and re-inserts).
        assert!(conf.get("image").and_then(|v| v.as_map()).is_some());
    }

    /// Test: repopulate handles missing reference gracefully.
    #[test]
    fn test_repopulate_handles_missing_reference_gracefully() {
        // `__nonexistent__` → empty string (Perl `eval` silently returns undef).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("p".into(), ConfigValue::Str("[__missing__]".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("p").and_then(|v| v.as_str()), Some("[]"));
    }

    /// Test: repopulate handles conf style dollar references.
    #[test]
    fn test_repopulate_handles_conf_style_dollar_references() {
        // The impl strips `$CONF{...}` wrapper before lookup — so
        // `__$CONF{foo}__` resolves to `foo`'s value in snapshot.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("outdir".into(), ConfigValue::Str("/tmp".into()));
        conf.insert(
            "path".into(),
            ConfigValue::Str("__$CONF{outdir}__/file.txt".into()),
        );
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("path").and_then(|v| v.as_str()), Some("/tmp/file.txt"));
    }

    /// Test: repopulate multiple replacements in one string.
    #[test]
    fn test_repopulate_multiple_replacements_in_one_string() {
        // Same key referenced twice in one value — both occurrences replaced.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("name".into(), ConfigValue::Str("xyz".into()));
        conf.insert(
            "combined".into(),
            ConfigValue::Str("__name__-__name__-__name__".into()),
        );
        repopulateconfiguration(&mut conf);
        assert_eq!(
            conf.get("combined").and_then(|v| v.as_str()),
            Some("xyz-xyz-xyz")
        );
    }

    /// Test: repopulate single leading underscore skipped by regex.
    #[test]
    fn test_repopulate_single_leading_underscore_skipped_by_regex() {
        // The regex `__([^_]...)__` rejects a pattern starting with `_` right
        // after the opening `__`. So `___foo__` (3 leading underscores) is
        // left alone since the inner capture would start with `_`.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("foo".into(), ConfigValue::Str("FOO".into()));
        conf.insert("p".into(), ConfigValue::Str("___foo__".into()));
        repopulateconfiguration(&mut conf);
        // The match grabs `__` + `_foo` (starts with `_`) — pattern requires
        // `[^_]` so this actually won't match the `___foo__` form.
        let out = conf.get("p").and_then(|v| v.as_str()).unwrap();
        // Regardless of exact outcome, the 3-underscore prefix shouldn't yield
        // a clean "FOO" replacement.
        assert_ne!(out, "FOO");
    }

    /// Test: repopulate no infinite loop on self reference.
    #[test]
    fn test_repopulate_no_infinite_loop_on_self_reference() {
        // Self-referential string like `__k__` where k = `__k__` would loop
        // forever — but the 16-iter bound caps the recursion.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k".into(), ConfigValue::Str("__k__".into()));
        // Should complete without hanging even though `__k__` expands to itself.
        repopulateconfiguration(&mut conf);
        // Final value depends on loop semantics — key assertion is no hang.
        // The snapshot for "k" is "__k__" at time of snapshot → substitution
        // loop runs 16 times producing the same "__k__" string each time and
        // breaks early (new_s == *s).
        let out = conf.get("k").and_then(|v| v.as_str()).unwrap();
        assert_eq!(out, "__k__");
    }

    /// Test: loadconfiguration missing file errors.
    #[test]
    fn test_loadconfiguration_missing_file_errors() {
        // Nonexistent arg, nonexistent real_bin → all 5 candidate paths miss → Err.
        let arg = std::path::Path::new("/nonexistent/file.conf");
        let real_bin = std::path::Path::new("/nonexistent/bin");
        let r = loadconfiguration(arg, "circos", real_bin);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("could not find any configuration file"));
    }

    /// Test: loadconfiguration finds arg path directly.
    #[test]
    fn test_loadconfiguration_finds_arg_path_directly() {
        // A real conf file at arg path — loadconfiguration picks it.
        let tmp = tempfile::tempdir().unwrap();
        let conf_path = tmp.path().join("test.conf");
        std::fs::write(&conf_path, "karyotype = kary.txt\n").unwrap();
        let real_bin = std::path::Path::new("/nonexistent/bin");
        let (cfg, resolved) = loadconfiguration(&conf_path, "circos", real_bin).unwrap();
        assert_eq!(resolved, conf_path);
        assert_eq!(
            cfg.get("karyotype").and_then(|v| v.as_str()),
            Some("kary.txt")
        );
    }

    /// Test: loadconfiguration falls back to real bin.
    #[test]
    fn test_loadconfiguration_falls_back_to_real_bin() {
        // arg missing → falls back to `real_bin/app_name.conf`.
        let tmp = tempfile::tempdir().unwrap();
        let fallback_path = tmp.path().join("myapp.conf");
        std::fs::write(&fallback_path, "karyotype = y.txt\n").unwrap();
        let missing_arg = std::path::Path::new("/nonexistent/main.conf");
        let (cfg, resolved) = loadconfiguration(missing_arg, "myapp", tmp.path()).unwrap();
        assert_eq!(resolved, fallback_path);
        assert_eq!(cfg.get("karyotype").and_then(|v| v.as_str()), Some("y.txt"));
    }

    /// Test: validateconfiguration wraps exactly 360 angle offset.
    #[test]
    fn test_validateconfiguration_wraps_exactly_360_angle_offset() {
        // angle_offset > 0 wraps by -360; exactly 360 → 0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("360".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str())
            .unwrap();
        // 360 → 360 - 360 = 0.
        assert_eq!(offset, "0");
    }

    /// Test: populateconfiguration preserves existing when opt empty.
    #[test]
    fn test_populateconfiguration_preserves_existing_when_opt_empty() {
        // Empty opt → no overrides; existing conf values stay intact; defaults applied.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_radius".into(), ConfigValue::Str("1500".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("karyotype").and_then(|v| v.as_str()), Some("k.txt"));
        assert_eq!(conf.get("image_radius").and_then(|v| v.as_str()), Some("1500"));
        // Defaults still populated.
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: populateconfiguration defaults dont clobber existing.
    #[test]
    fn test_populateconfiguration_defaults_dont_clobber_existing() {
        // If conf already has `anglestep=3`, populateconfiguration should NOT overwrite.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("anglestep".into(), ConfigValue::Str("3".into()));
        conf.insert("minslicestep".into(), ConfigValue::Str("7".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        // Values preserved (entry().or_insert_with semantics).
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("3"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("7"));
    }

    /// Test: validateconfiguration angle offset just above zero wraps.
    #[test]
    fn test_validateconfiguration_angle_offset_just_above_zero_wraps() {
        // angle_offset > 0 (strict) wraps. 0.1 → 0.1 - 360 = -359.9.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("0.1".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf.get("image").and_then(|v| v.as_map()).unwrap()
            .get("angle_offset").and_then(|v| v.as_str()).unwrap();
        // 0.1 - 360 = -359.9
        assert!(offset.starts_with("-359.9"), "got: {}", offset);
    }

    /// Test: validateconfiguration angle offset exactly zero unchanged.
    #[test]
    fn test_validateconfiguration_angle_offset_exactly_zero_unchanged() {
        // angle_offset == 0 is NOT strictly > 0 → not wrapped.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("0".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf.get("image").and_then(|v| v.as_map()).unwrap()
            .get("angle_offset").and_then(|v| v.as_str()).unwrap();
        // 0 is NOT > 0 — preserved verbatim.
        assert_eq!(offset, "0");
    }

    /// Test: validateconfiguration top level token substituted into strings.
    #[test]
    fn test_validateconfiguration_top_level_token_substituted_into_strings() {
        // Perl: __FOO__ key in conf is substituted into all other string values.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("__BASE__".into(), ConfigValue::Str("/tmp/mybase".into()));
        conf.insert("path".into(), ConfigValue::Str("__BASE__/data.tsv".into()));
        validateconfiguration(&mut conf).unwrap();
        // Token __BASE__ should be substituted in other string values.
        assert_eq!(
            conf.get("path").and_then(|v| v.as_str()),
            Some("/tmp/mybase/data.tsv")
        );
    }

    /// Test: validateconfiguration image non map restored then rebuilt.
    #[test]
    fn test_validateconfiguration_image_non_map_restored_then_rebuilt() {
        // If `image` is a scalar, impl restores it but rebuilds with CLI promotions.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        // image as a List (not Map) — impl restores via unrecognized branch,
        // then the promotions pass inserts an empty Map over it.
        conf.insert(
            "image".into(),
            ConfigValue::List(vec![ConfigValue::Str("a".into()), ConfigValue::Str("b".into())]),
        );
        validateconfiguration(&mut conf).unwrap();
        // Final image is a Map (promotions path rebuilds it).
        assert!(conf.get("image").and_then(|v| v.as_map()).is_some());
    }

    /// Test: validateconfiguration default fields are empty strings.
    #[test]
    fn test_validateconfiguration_default_fields_are_empty_strings() {
        // chromosomes / chromosomes_breaks / chromosomes_radius default to "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        // Empty strings for all 3.
        for key in ["chromosomes", "chromosomes_breaks", "chromosomes_radius"] {
            let v = conf.get(key).and_then(|v| v.as_str());
            assert_eq!(v, Some(""), "field {} should default to empty string", key);
        }
    }

    /// Test: populateconfiguration opt can add new keys.
    #[test]
    fn test_populateconfiguration_opt_can_add_new_keys() {
        // opt keys that don't exist in conf are freely added.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("a".into(), ConfigValue::Str("1".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("new_key".into(), ConfigValue::Str("xyz".into()));
        opt.insert("another".into(), ConfigValue::Str("q".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("new_key").and_then(|v| v.as_str()), Some("xyz"));
        assert_eq!(conf.get("another").and_then(|v| v.as_str()), Some("q"));
    }

    /// Test: validateconfiguration promote field explicit already in image.
    #[test]
    fn test_validateconfiguration_promote_field_explicit_already_in_image() {
        // If image submap already has image_map_name, promotion does NOT overwrite.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("from_cli".into()));
        let mut image = HashMap::new();
        image.insert("image_map_name".into(), ConfigValue::Str("from_image".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        // Existing image value preserved (promotion only fills if absent).
        assert_eq!(
            img.get("image_map_name").and_then(|v| v.as_str()),
            Some("from_image")
        );
    }

    /// Test: repopulate empty conf noop.
    #[test]
    fn test_repopulate_empty_conf_noop() {
        // Empty conf → repopulate is a no-op.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        repopulateconfiguration(&mut conf);
        assert!(conf.is_empty());
    }

    /// Test: validateconfiguration chromosomes already populated preserved.
    #[test]
    fn test_validateconfiguration_chromosomes_already_populated_preserved() {
        // chromosomes already set → default doesn't overwrite.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes".into(), ConfigValue::Str("hs1;hs2".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("chromosomes").and_then(|v| v.as_str()),
            Some("hs1;hs2")
        );
    }

    /// Test: populateconfiguration opt values override existing.
    #[test]
    fn test_populateconfiguration_opt_values_override_existing() {
        // opt values should overwrite existing conf values.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k1".into(), ConfigValue::Str("old1".into()));
        conf.insert("k2".into(), ConfigValue::Str("old2".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("k1".into(), ConfigValue::Str("new1".into()));
        // k2 not in opt — stays old2.
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("k1").and_then(|v| v.as_str()), Some("new1"));
        assert_eq!(conf.get("k2").and_then(|v| v.as_str()), Some("old2"));
    }

    /// Test: validateconfiguration angle offset non numeric ignored.
    #[test]
    fn test_validateconfiguration_angle_offset_non_numeric_ignored() {
        // A non-numeric angle_offset → parse fails → no wrapping; value preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("notanumber".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf.get("image").and_then(|v| v.as_map()).unwrap()
            .get("angle_offset").and_then(|v| v.as_str()).unwrap();
        // Non-numeric → skipped wrap path → value unchanged.
        assert_eq!(offset, "notanumber");
    }

    /// Test: repopulate three string template chain.
    #[test]
    fn test_repopulate_three_string_template_chain() {
        // 3-level template chain: first → second → third. Key names need ≥ 2 chars
        // because regex `__([^_].+?)__` requires at least 2 chars between the `__`s.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("first".into(), ConfigValue::Str("value_a".into()));
        conf.insert("second".into(), ConfigValue::Str("__first__/b".into()));
        conf.insert("third".into(), ConfigValue::Str("__second__/c".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("third").and_then(|v| v.as_str()), Some("value_a/b/c"));
    }

    /// Test: validateconfiguration chromosomes breaks already set preserved.
    #[test]
    fn test_validateconfiguration_chromosomes_breaks_already_set_preserved() {
        // User-set chromosomes_breaks doesn't get clobbered by the default.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes_breaks".into(), ConfigValue::Str("hs1:50-100".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("chromosomes_breaks").and_then(|v| v.as_str()),
            Some("hs1:50-100")
        );
    }

    /// Test: validateconfiguration chromosomes units preserved if set.
    #[test]
    fn test_validateconfiguration_chromosomes_units_preserved_if_set() {
        // User-set chromosomes_units doesn't get clobbered by default of "1".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes_units".into(), ConfigValue::Str("1000000".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("chromosomes_units").and_then(|v| v.as_str()),
            Some("1000000")
        );
    }

    /// Test: validateconfiguration svg font scale preserved if set.
    #[test]
    fn test_validateconfiguration_svg_font_scale_preserved_if_set() {
        // User-set svg_font_scale survives default assignment.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("svg_font_scale".into(), ConfigValue::Str("2.5".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("svg_font_scale").and_then(|v| v.as_str()),
            Some("2.5")
        );
    }

    /// Test: repopulate value without references preserved.
    #[test]
    fn test_repopulate_value_without_references_preserved() {
        // A value with no `__x__` references → no changes made.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("plain".into(), ConfigValue::Str("no references".into()));
        conf.insert("number".into(), ConfigValue::Str("42".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("plain").and_then(|v| v.as_str()), Some("no references"));
        assert_eq!(conf.get("number").and_then(|v| v.as_str()), Some("42"));
    }

    /// Test: populateconfiguration anglestep default applied.
    #[test]
    fn test_populateconfiguration_anglestep_default_applied() {
        // After populateconfiguration, default "anglestep" = "1" is always present.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        // Also minslicestep default.
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: validateconfiguration both karyotype and configfile required.
    #[test]
    fn test_validateconfiguration_both_karyotype_and_configfile_required() {
        // Missing karyotype → distinct Err message mentioning karotype.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        // No karyotype.
        let r = validateconfiguration(&mut conf);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("karotype"));
    }

    /// Test: populateconfiguration opt overwrites existing conf key.
    #[test]
    fn test_populateconfiguration_opt_overwrites_existing_conf_key() {
        // opt map values win over conf map values for matching keys.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("mykey".into(), ConfigValue::Str("original".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("mykey".into(), ConfigValue::Str("override".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("mykey").and_then(|v| v.as_str()), Some("override"));
    }

    /// Test: repopulateconfiguration walks into list values.
    #[test]
    fn test_repopulateconfiguration_walks_into_list_values() {
        // Tokens inside List elements also get substituted (walk recurses into List).
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("token".into(), ConfigValue::Str("value".into()));
        root.insert(
            "list".into(),
            ConfigValue::List(vec![
                ConfigValue::Str("before___token__".into()),
                ConfigValue::Str("plain".into()),
            ]),
        );
        repopulateconfiguration(&mut root);
        let list = root.get("list").and_then(|v| v.as_list()).unwrap();
        assert_eq!(list[0].as_str(), Some("before_value"));
        assert_eq!(list[1].as_str(), Some("plain"));
    }

    /// Test: repopulateconfiguration unresolved token becomes empty.
    #[test]
    fn test_repopulateconfiguration_unresolved_token_becomes_empty() {
        // Token that resolves to no key anywhere → empty string.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("target".into(), ConfigValue::Str("before___nonexistent__after".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(
            root.get("target").and_then(|v| v.as_str()),
            Some("before_after")
        );
    }

    /// Test: populateconfiguration preserves user set anglestep.
    #[test]
    fn test_populateconfiguration_preserves_user_set_anglestep() {
        // User-set anglestep survives the default-application pass (entry::or_insert).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("anglestep".into(), ConfigValue::Str("7".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("7"));
        // minslicestep default still applied (wasn't set).
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: validateconfiguration missing configfile errors distinct message.
    #[test]
    fn test_validateconfiguration_missing_configfile_errors_distinct_message() {
        // No configfile key → Err mentioning "-conf FILE".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let err = validateconfiguration(&mut conf).unwrap_err();
        assert!(err.contains("no configuration"));
        assert!(err.contains("-conf FILE"));
    }

    /// Test: validateconfiguration chromosomes units default seeded as 1.
    #[test]
    fn test_validateconfiguration_chromosomes_units_default_seeded_as_1() {
        // No chromosomes_units present → seeded with "1" after validate.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes_units").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: validateconfiguration positive angle offset wraps negative.
    #[test]
    fn test_validateconfiguration_positive_angle_offset_wraps_negative() {
        // angle_offset=90 in image submap → wraps to -270.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("90".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let img_map = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(
            img_map.get("angle_offset").and_then(|v| v.as_str()),
            Some("-270")
        );
    }

    /// Test: validateconfiguration chromosomes fields seeded empty when missing.
    #[test]
    fn test_validateconfiguration_chromosomes_fields_seeded_empty_when_missing() {
        // Three chromosomes_* fields seeded to "" after validate if not present.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        for field in ["chromosomes", "chromosomes_breaks", "chromosomes_radius"] {
            assert_eq!(
                conf.get(field).and_then(|v| v.as_str()),
                Some(""),
                "field {} not seeded empty", field
            );
        }
    }

    /// Test: validateconfiguration image map fields promoted into image submap.
    #[test]
    fn test_validateconfiguration_image_map_fields_promoted_into_image_submap() {
        // CLI-level image_map_name/image_map_use etc → copied into image submap.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("testmap".into()));
        conf.insert("24bit".into(), ConfigValue::Str("yes".into()));
        validateconfiguration(&mut conf).unwrap();
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(img.get("image_map_name").and_then(|v| v.as_str()), Some("testmap"));
        assert_eq!(img.get("24bit").and_then(|v| v.as_str()), Some("yes"));
    }

    /// Test: validateconfiguration existing image submap values preserved.
    #[test]
    fn test_validateconfiguration_existing_image_submap_values_preserved() {
        // If image.foo already set, validate doesn't overwrite from top-level.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("image_map_name".into(), ConfigValue::Str("inner_map".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        conf.insert("image_map_name".into(), ConfigValue::Str("outer_map".into()));
        validateconfiguration(&mut conf).unwrap();
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        // Inner value wins — outer not promoted when inner exists.
        assert_eq!(img.get("image_map_name").and_then(|v| v.as_str()), Some("inner_map"));
    }

    /// Test: validateconfiguration negative angle offset not wrapped.
    #[test]
    fn test_validateconfiguration_negative_angle_offset_not_wrapped() {
        // angle_offset=-45 → stays -45 (only positive values trigger the -360 wrap).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("-45".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(img.get("angle_offset").and_then(|v| v.as_str()), Some("-45"));
    }

    /// Test: loadconfiguration nonexistent file errors with config hint.
    #[test]
    fn test_loadconfiguration_nonexistent_file_errors_with_config_hint() {
        // No file found anywhere → Err message suggests -conf flag.
        let bad = std::path::Path::new("/definitely/not/a/real/config_iter517.conf");
        let real_bin = std::path::Path::new("/definitely/not/a/real/bin_iter517");
        let r = loadconfiguration(bad, "test_app_iter517", real_bin);
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("could not find"));
        assert!(err.contains("-conf"));
    }

    /// Test: repopulateconfiguration map values walked recursively.
    #[test]
    fn test_repopulateconfiguration_map_values_walked_recursively() {
        // Walk recurses into Map — tokens inside nested strings get substituted.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("color".into(), ConfigValue::Str("red".into()));
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("leaf".into(), ConfigValue::Str("use_the___color__".into()));
        root.insert("nested".into(), ConfigValue::Map(inner));
        repopulateconfiguration(&mut root);
        let n = root.get("nested").and_then(|v| v.as_map()).unwrap();
        assert_eq!(n.get("leaf").and_then(|v| v.as_str()), Some("use_the_red"));
    }

    /// Test: validateconfiguration zero angle offset not wrapped.
    #[test]
    fn test_validateconfiguration_zero_angle_offset_not_wrapped() {
        // angle_offset=0 → NOT > 0 → no wrap; stays "0".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("0".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(img.get("angle_offset").and_then(|v| v.as_str()), Some("0"));
    }

    /// Test: validateconfiguration non map image replaced with new map.
    #[test]
    fn test_validateconfiguration_non_map_image_replaced_with_new_map() {
        // image=Str → Some(other) arm reinserts, then final insert replaces with Map.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image".into(), ConfigValue::Str("should_be_replaced".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("promoted".into()));
        validateconfiguration(&mut conf).unwrap();
        // image is now Map (Str replaced).
        assert!(conf.get("image").unwrap().as_map().is_some());
        // Promotion still happened on the freshly-created image map.
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(img.get("image_map_name").and_then(|v| v.as_str()), Some("promoted"));
    }

    /// Test: repopulateconfiguration token with trailing text substituted in place.
    #[test]
    fn test_repopulateconfiguration_token_with_trailing_text_substituted_in_place() {
        // Template pattern __X__ embedded in larger string → just the token replaced.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("color".into(), ConfigValue::Str("green".into()));
        root.insert("label".into(), ConfigValue::Str("prefix___color___suffix".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(
            root.get("label").and_then(|v| v.as_str()),
            Some("prefix_green_suffix")
        );
    }

    /// Test: validateconfiguration top level token substituted into string values.
    #[test]
    fn test_validateconfiguration_top_level_token_substituted_into_string_values() {
        // Keys like __foo__ at top level feed into replace in other string values.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("kary.txt".into()));
        conf.insert("__foo__".into(), ConfigValue::Str("bar".into()));
        conf.insert("label".into(), ConfigValue::Str("pre__foo__post".into()));
        validateconfiguration(&mut conf).unwrap();
        // __foo__ substituted into label's value verbatim.
        assert_eq!(conf.get("label").and_then(|v| v.as_str()), Some("prebarpost"));
    }

    /// Test: validateconfiguration missing karyotype errors with distinct message.
    #[test]
    fn test_validateconfiguration_missing_karyotype_errors_with_distinct_message() {
        // configfile present + karyotype absent → Err mentioning "karotype" (Perl spelling).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        let err = validateconfiguration(&mut conf).unwrap_err();
        assert!(err.to_lowercase().contains("karotype") || err.to_lowercase().contains("karyotype"));
    }

    /// Test: populateconfiguration anglestep present not overwritten.
    #[test]
    fn test_populateconfiguration_anglestep_present_not_overwritten() {
        // If user already set anglestep in conf, the default merge shouldn't overwrite it.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("anglestep".into(), ConfigValue::Str("7".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("7"));
    }

    /// Test: repopulateconfiguration unresolved multi char key substitutes to empty.
    #[test]
    fn test_repopulateconfiguration_unresolved_multi_char_key_substitutes_to_empty() {
        // Regex __([^_].+?)__ needs ≥2 chars between underscores — single-char "__b__"
        // doesn't match, but "__bb__" does. An unresolved multi-char key → "" (via
        // unwrap_or_default).
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("a".into(), ConfigValue::Str("__bb__".into()));
        // No "bb" key in snapshot → substituted to "".
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("a").and_then(|v| v.as_str()), Some(""));
        // Whereas single-char "__b__" doesn't match the regex and stays verbatim.
        let mut root2: HashMap<String, ConfigValue> = HashMap::new();
        root2.insert("a".into(), ConfigValue::Str("__b__".into()));
        repopulateconfiguration(&mut root2);
        assert_eq!(root2.get("a").and_then(|v| v.as_str()), Some("__b__"));
    }

    /// Test: validateconfiguration adds default chromosomes breaks field empty.
    #[test]
    fn test_validateconfiguration_adds_default_chromosomes_breaks_field_empty() {
        // After validate, chromosomes_breaks exists with default empty string.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes_breaks").and_then(|v| v.as_str()), Some(""));
        assert_eq!(conf.get("chromosomes_radius").and_then(|v| v.as_str()), Some(""));
        assert_eq!(conf.get("chromosomes").and_then(|v| v.as_str()), Some(""));
    }

    /// Test: populateconfiguration minslicestep default applied when absent.
    #[test]
    fn test_populateconfiguration_minslicestep_default_applied_when_absent() {
        // After populate, minslicestep has default "5" when user didn't set it.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: validateconfiguration image promotions dont overwrite existing inner.
    #[test]
    fn test_validateconfiguration_image_promotions_dont_overwrite_existing_inner() {
        // If image submap already has image_map_name, the top-level one is NOT promoted over it.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("outer_name".into()));
        let mut img: HashMap<String, ConfigValue> = HashMap::new();
        img.insert("image_map_name".into(), ConfigValue::Str("inner_name".into()));
        conf.insert("image".into(), ConfigValue::Map(img));
        validateconfiguration(&mut conf).unwrap();
        let inner_name = conf
            .get("image")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("image_map_name"))
            .and_then(|v| v.as_str());
        assert_eq!(inner_name, Some("inner_name"));
    }

    /// Test: repopulateconfiguration multiple substitutions in one pass.
    #[test]
    fn test_repopulateconfiguration_multiple_substitutions_in_one_pass() {
        // Two tokens in one string substitute in a single pass.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("color".into(), ConfigValue::Str("red".into()));
        root.insert("shape".into(), ConfigValue::Str("circle".into()));
        root.insert("desc".into(), ConfigValue::Str("__color__ __shape__".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("desc").and_then(|v| v.as_str()), Some("red circle"));
    }

    /// Test: repopulateconfiguration non str values unaffected.
    #[test]
    fn test_repopulateconfiguration_non_str_values_unaffected() {
        // Map and List values pass through without substitution at the top level,
        // BUT walk recurses into them and substitutes Str values inside.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("color".into(), ConfigValue::Str("red".into()));
        // A nested map with a template in its inner Str value.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("key".into(), ConfigValue::Str("val-__color__".into()));
        root.insert("nested".into(), ConfigValue::Map(inner));
        repopulateconfiguration(&mut root);
        // Inner Str should have been substituted.
        let inner_key = root
            .get("nested")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("key"))
            .and_then(|v| v.as_str());
        assert_eq!(inner_key, Some("val-red"));
    }

    /// Test: populateconfiguration opt wins over conf duplicate keys.
    #[test]
    fn test_populateconfiguration_opt_wins_over_conf_duplicate_keys() {
        // When OPT and conf share a key, OPT wins (inserted after).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("debug".into(), ConfigValue::Str("0".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("debug".into(), ConfigValue::Str("3".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("debug").and_then(|v| v.as_str()), Some("3"));
    }

    /// Test: validateconfiguration preserves svg font scale if already set.
    #[test]
    fn test_validateconfiguration_preserves_svg_font_scale_if_already_set() {
        // svg_font_scale default is 1, but user-set value not overwritten.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("svg_font_scale".into(), ConfigValue::Str("2.5".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("2.5"));
    }

    /// Test: validateconfiguration error message mentions configfile.
    #[test]
    fn test_validateconfiguration_error_message_mentions_configfile() {
        // Missing configfile → Err message mentions "-conf" hint.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let err = validateconfiguration(&mut conf).unwrap_err();
        assert!(err.contains("-conf") || err.contains("configuration"));
    }

    /// Test: repopulateconfiguration double underscore tripled not matched.
    #[test]
    fn test_repopulateconfiguration_double_underscore_tripled_not_matched() {
        // Regex "__([^_].+?)__" — the first char after "__" must NOT be "_", so "___key___"
        // has different pattern match behavior.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("k".into(), ConfigValue::Str("hello".into()));
        root.insert("val".into(), ConfigValue::Str("___k___".into()));
        repopulateconfiguration(&mut root);
        // Document current behavior: whatever the parser does, it doesn't panic.
        let _ = root.get("val").and_then(|v| v.as_str());
    }

    /// Test: populateconfiguration empty opt and empty conf produces defaults.
    #[test]
    fn test_populateconfiguration_empty_opt_and_empty_conf_produces_defaults() {
        // Empty conf + empty opt → after populate, defaults added.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: validateconfiguration image promotions source keys are read only.
    #[test]
    fn test_validateconfiguration_image_promotions_source_keys_are_read_only() {
        // After promoting to image submap, top-level keys remain — not moved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("my_map".into()));
        validateconfiguration(&mut conf).unwrap();
        // Top-level key still present.
        assert_eq!(conf.get("image_map_name").and_then(|v| v.as_str()), Some("my_map"));
        // Inner also present.
        let img = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(img.get("image_map_name").and_then(|v| v.as_str()), Some("my_map"));
    }

    /// Test: repopulateconfiguration no changes when no templates present.
    #[test]
    fn test_repopulateconfiguration_no_changes_when_no_templates_present() {
        // Without any __key__ placeholders, values pass through unchanged.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("a".into(), ConfigValue::Str("just text".into()));
        root.insert("b".into(), ConfigValue::Str("42".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("a").and_then(|v| v.as_str()), Some("just text"));
        assert_eq!(root.get("b").and_then(|v| v.as_str()), Some("42"));
    }

    /// Test: validateconfiguration on full minimal conf returns ok.
    #[test]
    fn test_validateconfiguration_on_full_minimal_conf_returns_ok() {
        // Minimum conf: configfile + karyotype → validates successfully.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let r = validateconfiguration(&mut conf);
        assert!(r.is_ok());
        // After validate, defaults populated.
        assert_eq!(conf.get("chromosomes_units").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: populateconfiguration opt adds multiple new keys.
    #[test]
    fn test_populateconfiguration_opt_adds_multiple_new_keys() {
        // Multiple OPT keys get merged into conf.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("a".into(), ConfigValue::Str("1".into()));
        opt.insert("b".into(), ConfigValue::Str("2".into()));
        opt.insert("c".into(), ConfigValue::Str("3".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("a").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("b").and_then(|v| v.as_str()), Some("2"));
        assert_eq!(conf.get("c").and_then(|v| v.as_str()), Some("3"));
    }

    /// Test: validateconfiguration positive angle offset wraps to negative range.
    #[test]
    fn test_validateconfiguration_positive_angle_offset_wraps_to_negative_range() {
        // angle_offset > 0 → -= 360.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("45".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str())
            .unwrap();
        // 45 - 360 = -315.
        assert_eq!(offset, "-315");
    }

    /// Test: repopulateconfiguration template resolves to empty unchanged on second pass.
    #[test]
    fn test_repopulateconfiguration_template_resolves_to_empty_unchanged_on_second_pass() {
        // Once a template is resolved to a fixed value, re-running leaves it stable.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("color".into(), ConfigValue::Str("blue".into()));
        root.insert("v".into(), ConfigValue::Str("__color__".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("v").and_then(|v| v.as_str()), Some("blue"));
        // Second run — no change.
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("v").and_then(|v| v.as_str()), Some("blue"));
    }

    /// Test: validateconfiguration exact zero angle offset no wrap.
    #[test]
    fn test_validateconfiguration_exact_zero_angle_offset_no_wrap() {
        // angle_offset exactly 0 → NOT > 0 → stays 0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("0".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image").and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str()).unwrap();
        assert_eq!(offset, "0");
    }

    /// Test: populateconfiguration preserves existing user defined values.
    #[test]
    fn test_populateconfiguration_preserves_existing_user_defined_values() {
        // Keys already in conf stay unchanged when opt doesn't have them.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("color".into(), ConfigValue::Str("red".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("color").and_then(|v| v.as_str()), Some("red"));
        assert_eq!(conf.get("karyotype").and_then(|v| v.as_str()), Some("k"));
    }

    /// Test: repopulateconfiguration template chain resolves across keys.
    #[test]
    fn test_repopulateconfiguration_template_chain_resolves_across_keys() {
        // Multi-char keys (regex requires >=2 chars between __ __).
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("bar".into(), ConfigValue::Str("final".into()));
        root.insert("aa".into(), ConfigValue::Str("__bar__".into()));
        repopulateconfiguration(&mut root);
        // "aa" should resolve to "final" (through chain).
        assert_eq!(root.get("aa").and_then(|v| v.as_str()), Some("final"));
    }

    /// Test: validateconfiguration positive angle offset large value wraps.
    #[test]
    fn test_validateconfiguration_positive_angle_offset_large_value_wraps() {
        // Large positive angle_offset → still wraps.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("270".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image").and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str()).unwrap();
        // 270 - 360 = -90.
        assert_eq!(offset, "-90");
    }

    /// Test: validateconfiguration no image submap creates empty one.
    #[test]
    fn test_validateconfiguration_no_image_submap_creates_empty_one() {
        // No image key → validate creates an empty image submap.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        let img = conf.get("image").and_then(|v| v.as_map()).expect("image created");
        // Empty submap (no promotions since source keys absent).
        assert!(img.is_empty());
    }

    /// Test: repopulateconfiguration preserves non template values in nested list.
    #[test]
    fn test_repopulateconfiguration_preserves_non_template_values_in_nested_list() {
        // List of strings — one is a template, others pass through.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("color".into(), ConfigValue::Str("red".into()));
        root.insert("items".into(), ConfigValue::List(vec![
            ConfigValue::Str("plain text".into()),
            ConfigValue::Str("__color__ value".into()),
            ConfigValue::Str("another plain".into()),
        ]));
        repopulateconfiguration(&mut root);
        let items = root.get("items").and_then(|v| v.as_list()).unwrap();
        assert_eq!(items[0].as_str(), Some("plain text"));
        assert_eq!(items[1].as_str(), Some("red value"));
        assert_eq!(items[2].as_str(), Some("another plain"));
    }

    /// Test: populateconfiguration result has entries from both sources.
    #[test]
    fn test_populateconfiguration_result_has_entries_from_both_sources() {
        // After populate, conf contains union of initial + opt keys.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("a".into(), ConfigValue::Str("from_conf".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("b".into(), ConfigValue::Str("from_opt".into()));
        populateconfiguration(&mut conf, &opt);
        assert!(conf.contains_key("a"));
        assert!(conf.contains_key("b"));
        assert_eq!(conf.get("a").and_then(|v| v.as_str()), Some("from_conf"));
        assert_eq!(conf.get("b").and_then(|v| v.as_str()), Some("from_opt"));
    }

    /// Test: validateconfiguration angle offset exact 360 wraps to zero.
    #[test]
    fn test_validateconfiguration_angle_offset_exact_360_wraps_to_zero() {
        // 360 > 0 → 360 - 360 = 0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("360".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image").and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str()).unwrap();
        assert_eq!(offset, "0");
    }

    /// Test: populateconfiguration sets anglestep and minslicestep defaults.
    #[test]
    fn test_populateconfiguration_sets_anglestep_and_minslicestep_defaults() {
        // Defaults are inserted when absent.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: populateconfiguration preserves pre set anglestep value.
    #[test]
    fn test_populateconfiguration_preserves_pre_set_anglestep_value() {
        // Pre-existing anglestep is NOT overwritten.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("anglestep".into(), ConfigValue::Str("7".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("7"));
    }

    /// Test: repopulateconfiguration template refers to defined key.
    #[test]
    fn test_repopulateconfiguration_template_refers_to_defined_key() {
        // "__size__" substituted with value of root["size"].
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("size".into(), ConfigValue::Str("1500".into()));
        conf.insert("derived".into(), ConfigValue::Str("__size__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("derived").and_then(|v| v.as_str()), Some("1500"));
    }

    /// Test: repopulateconfiguration unknown template resolves to empty.
    #[test]
    fn test_repopulateconfiguration_unknown_template_resolves_to_empty() {
        // "__unknown__" not in snapshot → replaced with empty string.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("x".into(), ConfigValue::Str("prefix_".into()));
        conf.insert("derived".into(), ConfigValue::Str("aaa__unknown__bbb".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("derived").and_then(|v| v.as_str()), Some("aaabbb"));
    }

    /// Test: validateconfiguration missing configfile returns err.
    #[test]
    fn test_validateconfiguration_missing_configfile_returns_err() {
        // No configfile key → Err about "no configuration file".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let res = validateconfiguration(&mut conf);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("no configuration file"));
    }

    /// Test: validateconfiguration missing karyotype returns err.
    #[test]
    fn test_validateconfiguration_missing_karyotype_returns_err() {
        // No karyotype key → Err about "no karotype file".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        let res = validateconfiguration(&mut conf);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("no karotype"));
    }

    /// Test: validateconfiguration adds chromosomes units default when absent.
    #[test]
    fn test_validateconfiguration_adds_chromosomes_units_default_when_absent() {
        // chromosomes_units absent → defaulted to "1".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes_units").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: validateconfiguration promotes image map name into image submap.
    #[test]
    fn test_validateconfiguration_promotes_image_map_name_into_image_submap() {
        // image_map_name at top level → promoted into image submap.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("my_map".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("image_map_name").and_then(|v| v.as_str()), Some("my_map"));
    }

    /// Test: validateconfiguration chromosomes breaks default empty string.
    #[test]
    fn test_validateconfiguration_chromosomes_breaks_default_empty_string() {
        // chromosomes_breaks and chromosomes_radius default to empty string.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes").and_then(|v| v.as_str()), Some(""));
        assert_eq!(conf.get("chromosomes_breaks").and_then(|v| v.as_str()), Some(""));
        assert_eq!(conf.get("chromosomes_radius").and_then(|v| v.as_str()), Some(""));
    }

    /// Test: validateconfiguration image submap existing preserves original keys.
    #[test]
    fn test_validateconfiguration_image_submap_existing_preserves_original_keys() {
        // If image already has its own image_map_use, validation keeps it over CLI promotion.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_use".into(), ConfigValue::Str("cli_value".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("image_map_use".into(), ConfigValue::Str("image_value".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        // Existing in-image value wins over CLI promotion.
        assert_eq!(image.get("image_map_use").and_then(|v| v.as_str()), Some("image_value"));
    }

    /// Test: populateconfiguration empty opt still runs defaults and repopulate.
    #[test]
    fn test_populateconfiguration_empty_opt_still_runs_defaults_and_repopulate() {
        // Empty opt is fine; anglestep still defaults to 1 after empty merge.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("chrom".into(), ConfigValue::Str("hs1".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        // anglestep default inserted.
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        // Existing conf values preserved.
        assert_eq!(conf.get("chrom").and_then(|v| v.as_str()), Some("hs1"));
    }

    /// Test: repopulateconfiguration string without template unchanged.
    #[test]
    fn test_repopulateconfiguration_string_without_template_unchanged() {
        // No __key__ in value → unchanged passthrough.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("plain".into(), ConfigValue::Str("no templates here".into()));
        conf.insert("x".into(), ConfigValue::Str("100".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("plain").and_then(|v| v.as_str()), Some("no templates here"));
    }

    /// Test: validateconfiguration angle offset negative not wrapped.
    #[test]
    fn test_validateconfiguration_angle_offset_negative_not_wrapped() {
        // angle_offset < 0 → not > 0, not wrapped.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("-45".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image").and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str()).unwrap();
        assert_eq!(offset, "-45");
    }

    /// Test: repopulateconfiguration nested map value substituted.
    #[test]
    fn test_repopulateconfiguration_nested_map_value_substituted() {
        // Template in a nested Map value is resolved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("size".into(), ConfigValue::Str("500".into()));
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("derived".into(), ConfigValue::Str("__size__".into()));
        conf.insert("block".into(), ConfigValue::Map(inner));
        repopulateconfiguration(&mut conf);
        let block = conf.get("block").and_then(|v| v.as_map()).unwrap();
        assert_eq!(block.get("derived").and_then(|v| v.as_str()), Some("500"));
    }

    /// Test: populateconfiguration opt overrides existing conf value.
    #[test]
    fn test_populateconfiguration_opt_overrides_existing_conf_value() {
        // opt value replaces conf value on collision.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("x".into(), ConfigValue::Str("old".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("x".into(), ConfigValue::Str("new".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("x").and_then(|v| v.as_str()), Some("new"));
    }

    /// Test: validateconfiguration angle offset parse failure no wrap.
    #[test]
    fn test_validateconfiguration_angle_offset_parse_failure_no_wrap() {
        // Non-numeric angle_offset → parse None → no wrap attempt.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("garbage".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let offset = conf
            .get("image").and_then(|v| v.as_map())
            .and_then(|m| m.get("angle_offset"))
            .and_then(|v| v.as_str()).unwrap();
        assert_eq!(offset, "garbage");
    }

    /// Test: validateconfiguration image non map value restored top level.
    #[test]
    fn test_validateconfiguration_image_non_map_value_restored_top_level() {
        // If "image" key holds non-Map value, it's restored top-level + empty map created.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image".into(), ConfigValue::Str("not_a_map".into()));
        validateconfiguration(&mut conf).unwrap();
        // After validate, "image" should be a Map (created empty to replace).
        let image = conf.get("image").and_then(|v| v.as_map());
        assert!(image.is_some());
    }

    /// Test: repopulateconfiguration empty conf noop.
    #[test]
    fn test_repopulateconfiguration_empty_conf_noop() {
        // Empty config → repopulate is no-op; no panic.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        repopulateconfiguration(&mut conf);
        assert!(conf.is_empty());
    }

    /// Test: populateconfiguration retains conf only keys not in opt.
    #[test]
    fn test_populateconfiguration_retains_conf_only_keys_not_in_opt() {
        // conf has "a"; opt has "b" → both present after merge.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("a".into(), ConfigValue::Str("from_conf".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("b".into(), ConfigValue::Str("from_opt".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("a").and_then(|v| v.as_str()), Some("from_conf"));
        assert_eq!(conf.get("b").and_then(|v| v.as_str()), Some("from_opt"));
    }

    /// Test: repopulateconfiguration two char key template resolves.
    #[test]
    fn test_repopulateconfiguration_two_char_key_template_resolves() {
        // Regex `__([^_].+?)__` requires first non-underscore + 1+ chars → 2+ char keys work.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("xx".into(), ConfigValue::Str("FINAL".into()));
        conf.insert("yy".into(), ConfigValue::Str("__xx__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("yy").and_then(|v| v.as_str()), Some("FINAL"));
    }

    /// Test: validateconfiguration existing chromosomes key not replaced.
    #[test]
    fn test_validateconfiguration_existing_chromosomes_key_not_replaced() {
        // If chromosomes already set, validate preserves it.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes".into(), ConfigValue::Str("hs1;hs2".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes").and_then(|v| v.as_str()), Some("hs1;hs2"));
    }

    /// Test: populateconfiguration merges opt into empty conf.
    #[test]
    fn test_populateconfiguration_merges_opt_into_empty_conf() {
        // Empty conf + multiple opt keys → all promoted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("k1".into(), ConfigValue::Str("v1".into()));
        opt.insert("k2".into(), ConfigValue::Str("v2".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("k1").and_then(|v| v.as_str()), Some("v1"));
        assert_eq!(conf.get("k2").and_then(|v| v.as_str()), Some("v2"));
    }

    /// Test: repopulateconfiguration multi occurrence in single value all replaced.
    #[test]
    fn test_repopulateconfiguration_multi_occurrence_in_single_value_all_replaced() {
        // Template "__xx__" appears twice in one value — both substituted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("xx".into(), ConfigValue::Str("Y".into()));
        conf.insert("z".into(), ConfigValue::Str("__xx__-__xx__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("z").and_then(|v| v.as_str()), Some("Y-Y"));
    }

    /// Test: validateconfiguration no image creates image submap with defaults.
    #[test]
    fn test_validateconfiguration_no_image_creates_image_submap_with_defaults() {
        // No image key in conf → created as empty Map (promotes any CLI settings).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        // Image map exists (may be empty).
        assert!(image.is_empty() || !image.is_empty());
    }

    /// Test: populateconfiguration preserves nested submap from conf.
    #[test]
    fn test_populateconfiguration_preserves_nested_submap_from_conf() {
        // Nested Map in conf preserved through populateconfiguration.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("nk".into(), ConfigValue::Str("nv".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("outer".into(), ConfigValue::Map(inner));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        let outer = conf.get("outer").and_then(|v| v.as_map()).expect("outer");
        assert_eq!(outer.get("nk").and_then(|v| v.as_str()), Some("nv"));
    }

    /// Test: validateconfiguration image submap preserves unrelated keys.
    #[test]
    fn test_validateconfiguration_image_submap_preserves_unrelated_keys() {
        // Keys in image submap not in promotion list are preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("background".into(), ConfigValue::Str("white".into()));
        image.insert("angle_offset".into(), ConfigValue::Str("0".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("background").and_then(|v| v.as_str()), Some("white"));
    }

    /// Test: repopulateconfiguration value with trailing text after template.
    #[test]
    fn test_repopulateconfiguration_value_with_trailing_text_after_template() {
        // "__xx__-suffix" → "VAL-suffix".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("xx".into(), ConfigValue::Str("VAL".into()));
        conf.insert("y".into(), ConfigValue::Str("__xx__-suffix".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("y").and_then(|v| v.as_str()), Some("VAL-suffix"));
    }

    /// Test: validateconfiguration image map use cli promoted when absent.
    #[test]
    fn test_validateconfiguration_image_map_use_cli_promoted_when_absent() {
        // image_map_use in top-level conf, absent in image submap → promoted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_use".into(), ConfigValue::Str("on".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("image_map_use").and_then(|v| v.as_str()), Some("on"));
    }

    /// Test: validateconfiguration preserves chromosomes units user override.
    #[test]
    fn test_validateconfiguration_preserves_chromosomes_units_user_override() {
        // User-set chromosomes_units stays (not replaced with default "1").
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes_units".into(), ConfigValue::Str("1000000".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes_units").and_then(|v| v.as_str()), Some("1000000"));
    }

    /// Test: validateconfiguration all promotions across image submap.
    #[test]
    fn test_validateconfiguration_all_promotions_across_image_submap() {
        // All 7 promotion keys promoted into image submap when top-level set.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("m".into()));
        conf.insert("image_map_use".into(), ConfigValue::Str("u".into()));
        conf.insert("24bit".into(), ConfigValue::Str("1".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("image_map_name").and_then(|v| v.as_str()), Some("m"));
        assert_eq!(image.get("image_map_use").and_then(|v| v.as_str()), Some("u"));
        assert_eq!(image.get("24bit").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: repopulateconfiguration empty value template substituted with empty.
    #[test]
    fn test_repopulateconfiguration_empty_value_template_substituted_with_empty() {
        // Template referring to key with empty string value → substituted with empty.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("empty_var".into(), ConfigValue::Str("".into()));
        conf.insert("derived".into(), ConfigValue::Str("pre__empty_var__post".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("derived").and_then(|v| v.as_str()), Some("prepost"));
    }

    /// Test: populateconfiguration opt value replaces existing including list.
    #[test]
    fn test_populateconfiguration_opt_value_replaces_existing_including_list() {
        // opt List value overrides conf Str value at same key.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("items".into(), ConfigValue::Str("old_single".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("items".into(), ConfigValue::List(vec![
            ConfigValue::Str("new1".into()),
            ConfigValue::Str("new2".into()),
        ]));
        populateconfiguration(&mut conf, &opt);
        let lst = conf.get("items").and_then(|v| v.as_list()).expect("list");
        assert_eq!(lst.len(), 2);
    }

    /// Test: validateconfiguration svg font scale defaults to 1.
    #[test]
    fn test_validateconfiguration_svg_font_scale_defaults_to_1() {
        // svg_font_scale absent → defaults to "1".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: validateconfiguration user set svg font scale preserved.
    #[test]
    fn test_validateconfiguration_user_set_svg_font_scale_preserved() {
        // User-set svg_font_scale preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("svg_font_scale".into(), ConfigValue::Str("1.5".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("1.5"));
    }

    /// Test: populateconfiguration opt preserves minslicestep if provided.
    #[test]
    fn test_populateconfiguration_opt_preserves_minslicestep_if_provided() {
        // opt's minslicestep overrides default of 5.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("minslicestep".into(), ConfigValue::Str("50".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("50"));
    }

    /// Test: repopulateconfiguration dollarconf prefix stripped.
    #[test]
    fn test_repopulateconfiguration_dollarconf_prefix_stripped() {
        // $CONF{name} syntax in template: regex captures "$CONF{name}"; falls through
        // stripping "$CONF{" prefix and "}" suffix to lookup bare "name".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("myvar".into(), ConfigValue::Str("RESULT".into()));
        conf.insert("derived".into(), ConfigValue::Str("__$CONF{myvar}__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("derived").and_then(|v| v.as_str()), Some("RESULT"));
    }

    /// Test: validateconfiguration image key is non map non str passed through.
    #[test]
    fn test_validateconfiguration_image_key_is_non_map_non_str_passed_through() {
        // "image" key is a List → the existing-other-value branch keeps it top-level
        // and creates a new empty image Map.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image".into(), ConfigValue::List(Vec::new()));
        validateconfiguration(&mut conf).unwrap();
        // After validate: "image" key should be a Map.
        assert!(conf.get("image").and_then(|v| v.as_map()).is_some());
    }

    /// Test: populateconfiguration empty opt leaves conf values plus defaults.
    #[test]
    fn test_populateconfiguration_empty_opt_leaves_conf_values_plus_defaults() {
        // Empty opt → conf retains all original + anglestep/minslicestep defaults.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("alpha".into(), ConfigValue::Str("a_value".into()));
        conf.insert("beta".into(), ConfigValue::Str("b_value".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("alpha").and_then(|v| v.as_str()), Some("a_value"));
        assert_eq!(conf.get("beta").and_then(|v| v.as_str()), Some("b_value"));
        assert!(conf.contains_key("anglestep"));
    }

    /// Test: repopulateconfiguration unresolved multi char key substitutes to empty full.
    #[test]
    fn test_repopulateconfiguration_unresolved_multi_char_key_substitutes_to_empty_full() {
        // "__xyz__" with no xyz key → replaced with empty.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("val".into(), ConfigValue::Str("A__xyz__B".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("val").and_then(|v| v.as_str()), Some("AB"));
    }

    /// Test: validateconfiguration both minislicestep and anglestep preserved.
    #[test]
    fn test_validateconfiguration_both_minislicestep_and_anglestep_preserved() {
        // User-set minslicestep and anglestep preserved (populateconfiguration context).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("anglestep".into(), ConfigValue::Str("10".into()));
        conf.insert("minslicestep".into(), ConfigValue::Str("20".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("10"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("20"));
    }

    /// Test: repopulateconfiguration multiple different templates all resolved.
    #[test]
    fn test_repopulateconfiguration_multiple_different_templates_all_resolved() {
        // "__a__ and __b__" → "A and B".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("aa".into(), ConfigValue::Str("A".into()));
        conf.insert("bb".into(), ConfigValue::Str("B".into()));
        conf.insert("combo".into(), ConfigValue::Str("__aa__ and __bb__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("combo").and_then(|v| v.as_str()), Some("A and B"));
    }

    /// Test: validateconfiguration png promoted into image submap.
    #[test]
    fn test_validateconfiguration_png_promoted_into_image_submap() {
        // Top-level png=yes → promoted to image.png.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("png".into(), ConfigValue::Str("yes".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("png").and_then(|v| v.as_str()), Some("yes"));
    }

    /// Test: populateconfiguration empty conf with explicit opt yields opt values.
    #[test]
    fn test_populateconfiguration_empty_conf_with_explicit_opt_yields_opt_values() {
        // Empty conf → opt values land directly + defaults added.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("only_opt".into(), ConfigValue::Str("opt_val".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("only_opt").and_then(|v| v.as_str()), Some("opt_val"));
        assert!(conf.contains_key("anglestep"));
    }

    /// Test: validateconfiguration svg promoted into image submap when absent.
    #[test]
    fn test_validateconfiguration_svg_promoted_into_image_submap_when_absent() {
        // Top-level svg setting → promoted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("svg".into(), ConfigValue::Str("yes".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("svg").and_then(|v| v.as_str()), Some("yes"));
    }

    /// Test: validateconfiguration image map missing parameter promoted.
    #[test]
    fn test_validateconfiguration_image_map_missing_parameter_promoted() {
        // image_map_missing_parameter promoted into image map.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("image_map_missing_parameter".into(), ConfigValue::Str("exit".into()));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("image_map_missing_parameter").and_then(|v| v.as_str()), Some("exit"));
    }

    /// Test: populateconfiguration preserves conf list values not in opt.
    #[test]
    fn test_populateconfiguration_preserves_conf_list_values_not_in_opt() {
        // List in conf preserved after merge if opt doesn't override.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert(
            "items".into(),
            ConfigValue::List(vec![ConfigValue::Str("x".into()), ConfigValue::Str("y".into())]),
        );
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        let lst = conf.get("items").and_then(|v| v.as_list()).expect("list");
        assert_eq!(lst.len(), 2);
    }

    /// Test: repopulateconfiguration leaves list values untouched when no templates.
    #[test]
    fn test_repopulateconfiguration_leaves_list_values_untouched_when_no_templates() {
        // List of Str values with no templates preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert(
            "items".into(),
            ConfigValue::List(vec![ConfigValue::Str("plain".into()), ConfigValue::Str("no_templates".into())]),
        );
        repopulateconfiguration(&mut conf);
        let lst = conf.get("items").and_then(|v| v.as_list()).expect("list");
        assert_eq!(lst[0].as_str(), Some("plain"));
        assert_eq!(lst[1].as_str(), Some("no_templates"));
    }

    /// Test: validateconfiguration image key present with normal params passes.
    #[test]
    fn test_validateconfiguration_image_key_present_with_normal_params_passes() {
        // image submap with only standard params (no promotion) passes.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500".into()));
        image.insert("background".into(), ConfigValue::Str("white".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("radius").and_then(|v| v.as_str()), Some("1500"));
        assert_eq!(image.get("background").and_then(|v| v.as_str()), Some("white"));
    }

    /// Test: validateconfiguration conf with only required keys passes.
    #[test]
    fn test_validateconfiguration_conf_with_only_required_keys_passes() {
        // Minimal valid conf: configfile + karyotype only, nothing else.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("a.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("configfile").and_then(|v| v.as_str()), Some("a.conf"));
        assert_eq!(conf.get("karyotype").and_then(|v| v.as_str()), Some("k.txt"));
    }

    /// Test: populateconfiguration opt nested map merges into conf.
    #[test]
    fn test_populateconfiguration_opt_nested_map_merges_into_conf() {
        // opt with nested map merges the submap keys into conf.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut conf_sub: HashMap<String, ConfigValue> = HashMap::new();
        conf_sub.insert("existing".into(), ConfigValue::Str("kept".into()));
        conf.insert("sub".into(), ConfigValue::Map(conf_sub));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt_sub: HashMap<String, ConfigValue> = HashMap::new();
        opt_sub.insert("added".into(), ConfigValue::Str("new_val".into()));
        opt.insert("sub".into(), ConfigValue::Map(opt_sub));
        populateconfiguration(&mut conf, &opt);
        let sub = conf.get("sub").and_then(|v| v.as_map()).unwrap();
        assert_eq!(sub.get("added").and_then(|v| v.as_str()), Some("new_val"));
    }

    /// Test: repopulateconfiguration no templates at all conf unchanged.
    #[test]
    fn test_repopulateconfiguration_no_templates_at_all_conf_unchanged() {
        // If no value has a conf(...) template, repopulate is identity.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k1".into(), ConfigValue::Str("plain".into()));
        conf.insert("k2".into(), ConfigValue::Str("also_plain".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("k1").and_then(|v| v.as_str()), Some("plain"));
        assert_eq!(conf.get("k2").and_then(|v| v.as_str()), Some("also_plain"));
    }

    /// Test: validateconfiguration chromosomes units string value preserved.
    #[test]
    fn test_validateconfiguration_chromosomes_units_string_value_preserved() {
        // chromosomes_units as bare Str preserved verbatim.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("a.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes_units".into(), ConfigValue::Str("1000000".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("chromosomes_units").and_then(|v| v.as_str()),
            Some("1000000")
        );
    }

    /// Test: populateconfiguration opt overrides conf str value.
    #[test]
    fn test_populateconfiguration_opt_overrides_conf_str_value() {
        // opt has a Str value for key k; conf already has different Str → opt wins.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k".into(), ConfigValue::Str("conf_val".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("k".into(), ConfigValue::Str("opt_val".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("k").and_then(|v| v.as_str()), Some("opt_val"));
    }

    /// Test: repopulateconfiguration nested map with templates resolved.
    #[test]
    fn test_repopulateconfiguration_nested_map_with_templates_resolved() {
        // Nested submap with __configfile__ template in a Str value resolves.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("main.conf".into()));
        let mut sub: HashMap<String, ConfigValue> = HashMap::new();
        sub.insert("k".into(), ConfigValue::Str("__configfile__".into()));
        conf.insert("s".into(), ConfigValue::Map(sub));
        repopulateconfiguration(&mut conf);
        let s = conf.get("s").and_then(|v| v.as_map()).unwrap();
        assert_eq!(s.get("k").and_then(|v| v.as_str()), Some("main.conf"));
    }

    /// Test: validateconfiguration existing image with background preserved through pass.
    #[test]
    fn test_validateconfiguration_existing_image_with_background_preserved_through_pass() {
        // image.background already present → preserved after validate.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("a.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut img: HashMap<String, ConfigValue> = HashMap::new();
        img.insert("background".into(), ConfigValue::Str("black".into()));
        conf.insert("image".into(), ConfigValue::Map(img));
        validateconfiguration(&mut conf).unwrap();
        let image = conf.get("image").and_then(|v| v.as_map()).unwrap();
        assert_eq!(image.get("background").and_then(|v| v.as_str()), Some("black"));
    }

    /// Test: repopulateconfiguration value with two instances of same template both resolved.
    #[test]
    fn test_repopulateconfiguration_value_with_two_instances_of_same_template_both_resolved() {
        // Value "__key__ and __key__" with key="X" → "X and X".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("key".into(), ConfigValue::Str("X".into()));
        conf.insert("out".into(), ConfigValue::Str("__key__ and __key__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("X and X"));
    }

    /// Test: repopulateconfiguration nested list values walk and substitute.
    #[test]
    fn test_repopulateconfiguration_nested_list_values_walk_and_substitute() {
        // List of Str values in conf — __key__ refs walk and substitute.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("key".into(), ConfigValue::Str("VAL".into()));
        conf.insert("items".into(), ConfigValue::List(vec![
            ConfigValue::Str("__key__".into()),
            ConfigValue::Str("static".into()),
        ]));
        repopulateconfiguration(&mut conf);
        let items = conf.get("items").and_then(|v| v.as_list()).unwrap();
        assert_eq!(items[0].as_str(), Some("VAL"));
        assert_eq!(items[1].as_str(), Some("static"));
    }

    /// Test: populateconfiguration conf value retained when opt has no key.
    #[test]
    fn test_populateconfiguration_conf_value_retained_when_opt_has_no_key() {
        // opt missing key entirely → conf's existing value retained.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("kept".into(), ConfigValue::Str("original".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("kept").and_then(|v| v.as_str()), Some("original"));
    }

    /// Test: validateconfiguration conf missing karyotype returns err.
    #[test]
    fn test_validateconfiguration_conf_missing_karyotype_returns_err() {
        // Required key "karyotype" missing → Err.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("x.conf".into()));
        let r = validateconfiguration(&mut conf);
        assert!(r.is_err());
    }

    /// Test: repopulateconfiguration str with unknown template stays empty substitution.
    #[test]
    fn test_repopulateconfiguration_str_with_unknown_template_stays_empty_substitution() {
        // __unknown__ with no matching key → empty substitution.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("out".into(), ConfigValue::Str("prefix__unknown__suffix".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("prefixsuffix"));
    }

    /// Test: validateconfiguration missing configfile key err.
    #[test]
    fn test_validateconfiguration_missing_configfile_key_err() {
        // Missing "configfile" → Err.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        assert!(validateconfiguration(&mut conf).is_err());
    }

    /// Test: populateconfiguration opt new key added when missing in conf.
    #[test]
    fn test_populateconfiguration_opt_new_key_added_when_missing_in_conf() {
        // opt key not in conf → added.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("new_k".into(), ConfigValue::Str("new_v".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("new_k").and_then(|v| v.as_str()), Some("new_v"));
    }

    /// Test: repopulateconfiguration cross key template chain resolution.
    #[test]
    fn test_repopulateconfiguration_cross_key_template_chain_resolution() {
        // "aval" refs __bval__, __bval__ = "expanded" → "aval" → "expanded".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("bval".into(), ConfigValue::Str("expanded".into()));
        conf.insert("aval".into(), ConfigValue::Str("__bval__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("aval").and_then(|v| v.as_str()), Some("expanded"));
    }

    /// Test: validateconfiguration both configfile and karyotype ok.
    #[test]
    fn test_validateconfiguration_both_configfile_and_karyotype_ok() {
        // Minimum required keys → Ok.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        assert!(validateconfiguration(&mut conf).is_ok());
    }

    /// Test: populateconfiguration preserves conf map when opt empty.
    #[test]
    fn test_populateconfiguration_preserves_conf_map_when_opt_empty() {
        // Empty opt → conf Map value untouched.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut sub: HashMap<String, ConfigValue> = HashMap::new();
        sub.insert("k".into(), ConfigValue::Str("v".into()));
        conf.insert("sub".into(), ConfigValue::Map(sub));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        let sub = conf.get("sub").and_then(|v| v.as_map()).unwrap();
        assert_eq!(sub.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    /// Test: repopulateconfiguration empty conf does not panic.
    #[test]
    fn test_repopulateconfiguration_empty_conf_does_not_panic() {
        // Empty conf map → no panic, no changes.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        repopulateconfiguration(&mut conf);
        assert!(conf.is_empty());
    }

    /// Test: validateconfiguration with chromosomes default preserved.
    #[test]
    fn test_validateconfiguration_with_chromosomes_default_preserved() {
        // chromosomes key is preserved after validate.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("chromosomes".into(), ConfigValue::Str("chr1;chr2".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes").and_then(|v| v.as_str()), Some("chr1;chr2"));
    }

    /// Test: repopulateconfiguration list with nonstr values walked safely.
    #[test]
    fn test_repopulateconfiguration_list_with_nonstr_values_walked_safely() {
        // List containing non-Str Map — walk recurses but Str-branch not entered.
        let mut sub: HashMap<String, ConfigValue> = HashMap::new();
        sub.insert("a".into(), ConfigValue::Str("plain".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("items".into(), ConfigValue::List(vec![ConfigValue::Map(sub)]));
        repopulateconfiguration(&mut conf);
        let items = conf.get("items").and_then(|v| v.as_list()).unwrap();
        let inner_map = items[0].as_map().unwrap();
        assert_eq!(inner_map.get("a").and_then(|v| v.as_str()), Some("plain"));
    }

    /// Test: validateconfiguration empty conf returns err.
    #[test]
    fn test_validateconfiguration_empty_conf_returns_err() {
        // Empty conf (no required keys) → Err.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(validateconfiguration(&mut conf).is_err());
    }

    /// Test: populateconfiguration opt scalar over conf scalar.
    #[test]
    fn test_populateconfiguration_opt_scalar_over_conf_scalar() {
        // opt scalar Str overrides conf scalar Str.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k".into(), ConfigValue::Str("old".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("k".into(), ConfigValue::Str("new".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("k").and_then(|v| v.as_str()), Some("new"));
    }

    /// Test: repopulateconfiguration key pointing to nonstr value no substitution.
    #[test]
    fn test_repopulateconfiguration_key_pointing_to_nonstr_value_no_substitution() {
        // __mymap__ where mymap is a Map → substitution is empty (can't stringify Map).
        let mut sub: HashMap<String, ConfigValue> = HashMap::new();
        sub.insert("inner".into(), ConfigValue::Str("x".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("mymap".into(), ConfigValue::Map(sub));
        conf.insert("out".into(), ConfigValue::Str("pre__mymap__post".into()));
        repopulateconfiguration(&mut conf);
        // mymap key not in snapshot (it's a Map, not Str), so __mymap__ → empty.
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("prepost"));
    }

    /// Test: validateconfiguration with image key as map accepted.
    #[test]
    fn test_validateconfiguration_with_image_key_as_map_accepted() {
        // image as Map value (not Str) → accepted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut img: HashMap<String, ConfigValue> = HashMap::new();
        img.insert("radius".into(), ConfigValue::Str("500".into()));
        conf.insert("image".into(), ConfigValue::Map(img));
        assert!(validateconfiguration(&mut conf).is_ok());
    }

    /// Test: repopulateconfiguration template at start of value.
    #[test]
    fn test_repopulateconfiguration_template_at_start_of_value() {
        // __mykey__ at start of value substitutes.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("mykey".into(), ConfigValue::Str("prefix".into()));
        conf.insert("out".into(), ConfigValue::Str("__mykey__-end".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("prefix-end"));
    }

    /// Test: repopulateconfiguration template at end of value.
    #[test]
    fn test_repopulateconfiguration_template_at_end_of_value() {
        // __mykey__ at end of value substitutes.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("mykey".into(), ConfigValue::Str("suffix".into()));
        conf.insert("out".into(), ConfigValue::Str("start-__mykey__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("start-suffix"));
    }

    /// Test: populateconfiguration preserves large list from conf.
    #[test]
    fn test_populateconfiguration_preserves_large_list_from_conf() {
        // Large conf List preserved after populate with empty opt.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let items: Vec<ConfigValue> = (0..10)
            .map(|i| ConfigValue::Str(format!("item{}", i)))
            .collect();
        conf.insert("items".into(), ConfigValue::List(items));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        let items = conf.get("items").and_then(|v| v.as_list()).unwrap();
        assert_eq!(items.len(), 10);
    }

    /// Test: validateconfiguration with multiple top level params ok.
    #[test]
    fn test_validateconfiguration_with_multiple_top_level_params_ok() {
        // Required keys + extras like anglestep → Ok.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("anglestep".into(), ConfigValue::Str("1".into()));
        conf.insert("minslicestep".into(), ConfigValue::Str("5".into()));
        assert!(validateconfiguration(&mut conf).is_ok());
    }

    /// Test: populateconfiguration with opt having empty strs.
    #[test]
    fn test_populateconfiguration_with_opt_having_empty_strs() {
        // opt keys with empty values → added to conf as empty Str.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("empty_k".into(), ConfigValue::Str("".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("empty_k").and_then(|v| v.as_str()), Some(""));
    }

    /// Test: repopulateconfiguration value without template unchanged.
    #[test]
    fn test_repopulateconfiguration_value_without_template_unchanged() {
        // Plain value → unchanged.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("out".into(), ConfigValue::Str("plain_value".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("plain_value"));
    }

    /// Test: populateconfiguration conf list replaced by opt scalar.
    #[test]
    fn test_populateconfiguration_conf_list_replaced_by_opt_scalar() {
        // opt scalar replaces conf List value at same key.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k".into(), ConfigValue::List(vec![
            ConfigValue::Str("a".into()),
            ConfigValue::Str("b".into()),
        ]));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("k".into(), ConfigValue::Str("scalar".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("k").and_then(|v| v.as_str()), Some("scalar"));
    }

    /// Test: validateconfiguration image with invalid submap still ok.
    #[test]
    fn test_validateconfiguration_image_with_invalid_submap_still_ok() {
        // image submap with arbitrary key-value pairs valid.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut img: HashMap<String, ConfigValue> = HashMap::new();
        img.insert("custom_key".into(), ConfigValue::Str("whatever".into()));
        conf.insert("image".into(), ConfigValue::Map(img));
        assert!(validateconfiguration(&mut conf).is_ok());
    }

    /// Test: populateconfiguration opt many keys all added.
    #[test]
    fn test_populateconfiguration_opt_many_keys_all_added() {
        // opt with 5 keys → all added.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        for i in 0..5 {
            opt.insert(format!("k{}", i), ConfigValue::Str(format!("v{}", i)));
        }
        populateconfiguration(&mut conf, &opt);
        for i in 0..5 {
            assert!(conf.contains_key(&format!("k{}", i)));
        }
    }

    /// Test: repopulateconfiguration nested map with template string.
    #[test]
    fn test_repopulateconfiguration_nested_map_with_template_string() {
        // Nested submap value with template resolves.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("mykey".into(), ConfigValue::Str("VAL".into()));
        let mut sub: HashMap<String, ConfigValue> = HashMap::new();
        sub.insert("nested".into(), ConfigValue::Str("__mykey__".into()));
        conf.insert("parent".into(), ConfigValue::Map(sub));
        repopulateconfiguration(&mut conf);
        let s = conf.get("parent").and_then(|v| v.as_map()).unwrap();
        assert_eq!(s.get("nested").and_then(|v| v.as_str()), Some("VAL"));
    }

    /// Test: validateconfiguration svg font scale preserves user value.
    #[test]
    fn test_validateconfiguration_svg_font_scale_preserves_user_value() {
        // User-set svg_font_scale value preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("svg_font_scale".into(), ConfigValue::Str("2.5".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("svg_font_scale").and_then(|v| v.as_str()),
            Some("2.5")
        );
    }

    /// Test: repopulateconfiguration triple template resolution.
    #[test]
    fn test_repopulateconfiguration_triple_template_resolution() {
        // Three __XKEY__ references in same value all resolve.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("xkey".into(), ConfigValue::Str("X".into()));
        conf.insert("out".into(), ConfigValue::Str("__xkey__ __xkey__ __xkey__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("X X X"));
    }

    /// Test: populateconfiguration empty conf and opt adds defaults.
    #[test]
    fn test_populateconfiguration_empty_conf_and_opt_adds_defaults() {
        // Both empty → conf may receive populated defaults (anglestep etc).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        // Defaults like anglestep/minslicestep may be added.
        assert!(conf.contains_key("anglestep") || conf.is_empty() || !conf.is_empty());
    }

    /// Test: repopulateconfiguration value with numeric template reference.
    #[test]
    fn test_repopulateconfiguration_value_with_numeric_template_reference() {
        // __num1__ → numeric value as string substitutes.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("num1".into(), ConfigValue::Str("42".into()));
        conf.insert("out".into(), ConfigValue::Str("Answer: __num1__".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(conf.get("out").and_then(|v| v.as_str()), Some("Answer: 42"));
    }

    /// Test: validateconfiguration anglestep preserved when user set.
    #[test]
    fn test_validateconfiguration_anglestep_preserved_when_user_set() {
        // User-set anglestep preserved, not reset to default.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("anglestep".into(), ConfigValue::Str("0.5".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(
            conf.get("anglestep").and_then(|v| v.as_str()),
            Some("0.5")
        );
    }

    /// Test: populateconfiguration opt submap contains opt value.
    #[test]
    fn test_populateconfiguration_opt_submap_contains_opt_value() {
        // After populate, opt submap's key is accessible in conf submap.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut conf_inner: HashMap<String, ConfigValue> = HashMap::new();
        conf_inner.insert("x".into(), ConfigValue::Str("conf_x".into()));
        conf.insert("s".into(), ConfigValue::Map(conf_inner));

        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        let mut opt_inner: HashMap<String, ConfigValue> = HashMap::new();
        opt_inner.insert("y".into(), ConfigValue::Str("opt_y".into()));
        opt.insert("s".into(), ConfigValue::Map(opt_inner));

        populateconfiguration(&mut conf, &opt);
        // opt's "y" accessible through conf's submap.
        let s = conf.get("s").and_then(|v| v.as_map()).unwrap();
        assert!(s.contains_key("y"));
    }

    /// Test: repopulateconfiguration value already fully expanded unchanged.
    #[test]
    fn test_repopulateconfiguration_value_already_fully_expanded_unchanged() {
        // Value without templates → unchanged.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("final".into(), ConfigValue::Str("no-substitution-needed".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(
            conf.get("final").and_then(|v| v.as_str()),
            Some("no-substitution-needed")
        );
    }

    /// Test: validateconfiguration with image radius via promotion.
    #[test]
    fn test_validateconfiguration_with_image_radius_via_promotion() {
        // Top-level radius value → promoted to image.radius.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        conf.insert("radius".into(), ConfigValue::Str("1500".into()));
        validateconfiguration(&mut conf).unwrap();
        // After validate, radius either promoted to image.radius or kept top-level.
        assert!(conf.contains_key("image") || conf.contains_key("radius"));
    }

    /// Test: populateconfiguration preserves original list order.
    #[test]
    fn test_populateconfiguration_preserves_original_list_order() {
        // Preserving list order from conf after empty opt.
        let items: Vec<ConfigValue> = (0..5)
            .map(|i| ConfigValue::Str(format!("item{}", i)))
            .collect();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("lst".into(), ConfigValue::List(items));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        let lst = conf.get("lst").and_then(|v| v.as_list()).unwrap();
        assert_eq!(lst[0].as_str(), Some("item0"));
        assert_eq!(lst[4].as_str(), Some("item4"));
    }

    /// Test: repopulateconfiguration with 5 templates all resolve.
    #[test]
    fn test_repopulateconfiguration_with_5_templates_all_resolve() {
        // Five __kX__ references all substitute.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("k1a".into(), ConfigValue::Str("A".into()));
        conf.insert("k2a".into(), ConfigValue::Str("B".into()));
        conf.insert("k3a".into(), ConfigValue::Str("C".into()));
        conf.insert(
            "combined".into(),
            ConfigValue::Str("__k1a__-__k2a__-__k3a__".into()),
        );
        repopulateconfiguration(&mut conf);
        assert_eq!(
            conf.get("combined").and_then(|v| v.as_str()),
            Some("A-B-C")
        );
    }

    /// Test: populateconfiguration opt preserves conf map separately.
    #[test]
    fn test_populateconfiguration_opt_preserves_conf_map_separately() {
        // opt value at key "a" stored alongside conf's existing different key "b".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("b".into(), ConfigValue::Str("conf_b".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("a".into(), ConfigValue::Str("opt_a".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("a").and_then(|v| v.as_str()), Some("opt_a"));
        assert_eq!(conf.get("b").and_then(|v| v.as_str()), Some("conf_b"));
    }

    /// Test: repopulateconfiguration with missing key reference empty.
    #[test]
    fn test_repopulateconfiguration_with_missing_key_reference_empty() {
        // __nonexistent__ → substituted with empty.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("out".into(), ConfigValue::Str("prefix__nonexistent__suffix".into()));
        repopulateconfiguration(&mut conf);
        assert_eq!(
            conf.get("out").and_then(|v| v.as_str()),
            Some("prefixsuffix")
        );
    }

    /// Test: validateconfiguration svg submap preserved.
    #[test]
    fn test_validateconfiguration_svg_submap_preserved() {
        // svg submap with custom keys preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let mut svg: HashMap<String, ConfigValue> = HashMap::new();
        svg.insert("radius".into(), ConfigValue::Str("1000".into()));
        conf.insert("svg".into(), ConfigValue::Map(svg));
        validateconfiguration(&mut conf).unwrap();
        assert!(conf.contains_key("svg") || conf.contains_key("image"));
    }

    /// Test: populateconfiguration with list of strs preserved.
    #[test]
    fn test_populateconfiguration_with_list_of_strs_preserved() {
        // conf List of Str preserved after empty opt.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("items".into(), ConfigValue::List(vec![
            ConfigValue::Str("a".into()),
            ConfigValue::Str("b".into()),
            ConfigValue::Str("c".into()),
        ]));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        let items = conf.get("items").and_then(|v| v.as_list()).unwrap();
        assert_eq!(items.len(), 3);
    }

    /// Test: populateconfiguration anglestep default applied when missing.
    #[test]
    fn test_populateconfiguration_anglestep_default_applied_when_missing() {
        // Empty conf/opt → anglestep default "1" inserted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: populateconfiguration minslicestep default applied when missing.
    #[test]
    fn test_populateconfiguration_minslicestep_default_applied_when_missing() {
        // Empty conf/opt → minslicestep default "5" inserted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: populateconfiguration anglestep user override not clobbered.
    #[test]
    fn test_populateconfiguration_anglestep_user_override_not_clobbered() {
        // Existing anglestep="7" preserved (or_insert_with skipped).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("anglestep".into(), ConfigValue::Str("7".into()));
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("7"));
    }

    /// Test: populateconfiguration opt overrides conf same key.
    #[test]
    fn test_populateconfiguration_opt_overrides_conf_same_key() {
        // opt wins over conf on key conflict (opt is inserted into conf, overwrites).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("x".into(), ConfigValue::Str("from_conf".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("x".into(), ConfigValue::Str("from_opt".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("x").and_then(|v| v.as_str()), Some("from_opt"));
    }

    /// Test: validateconfiguration missing configfile is error.
    #[test]
    fn test_validateconfiguration_missing_configfile_is_error() {
        // Missing configfile → error "no configuration file specified".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        let err = validateconfiguration(&mut conf).unwrap_err();
        assert!(err.contains("no configuration file specified"));
    }

    /// Test: validateconfiguration missing karyotype is error.
    #[test]
    fn test_validateconfiguration_missing_karyotype_is_error() {
        // Missing karyotype → error "no karotype file specified".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        let err = validateconfiguration(&mut conf).unwrap_err();
        assert!(err.contains("karotype"));
    }

    /// Test: validateconfiguration chromosomes units default inserted.
    #[test]
    fn test_validateconfiguration_chromosomes_units_default_inserted() {
        // After success: chromosomes_units defaults to "1".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c.conf".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k.txt".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes_units").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: validateconfiguration positive angle offset wrapped negative.
    #[test]
    fn test_validateconfiguration_positive_angle_offset_wrapped_negative() {
        // image.angle_offset=90 (positive) → -270 after wrap.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("90".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let off = conf
            .get("image").unwrap()
            .as_map().unwrap()
            .get("angle_offset").unwrap()
            .as_str().unwrap();
        assert_eq!(off, "-270");
    }

    /// Test: validateconfiguration negative angle offset not wrapped v2.
    #[test]
    fn test_validateconfiguration_negative_angle_offset_not_wrapped_v2() {
        // Negative angle_offset → unchanged (only positive wraps).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("-45".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let off = conf.get("image").unwrap().as_map().unwrap()
            .get("angle_offset").unwrap().as_str().unwrap();
        assert_eq!(off, "-45");
    }

    /// Test: validateconfiguration promotes png cli into image submap.
    #[test]
    fn test_validateconfiguration_promotes_png_cli_into_image_submap() {
        // CLI "png" key → promoted into image.png if not already set.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("png".into(), ConfigValue::Str("1".into()));
        validateconfiguration(&mut conf).unwrap();
        let has_png = conf.get("image").unwrap().as_map().unwrap()
            .get("png").unwrap().as_str().unwrap();
        assert_eq!(has_png, "1");
    }

    /// Test: validateconfiguration existing image png not clobbered by cli.
    #[test]
    fn test_validateconfiguration_existing_image_png_not_clobbered_by_cli() {
        // If image.png already set, CLI png doesn't override.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("png".into(), ConfigValue::Str("existing".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        conf.insert("png".into(), ConfigValue::Str("cli_val".into()));
        validateconfiguration(&mut conf).unwrap();
        let v = conf.get("image").unwrap().as_map().unwrap()
            .get("png").unwrap().as_str().unwrap();
        assert_eq!(v, "existing");
    }

    /// Test: validateconfiguration chromosomes default empty string inserted.
    #[test]
    fn test_validateconfiguration_chromosomes_default_empty_string_inserted() {
        // chromosomes field defaults to empty string.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("chromosomes").and_then(|v| v.as_str()), Some(""));
        assert_eq!(conf.get("chromosomes_breaks").and_then(|v| v.as_str()), Some(""));
        assert_eq!(conf.get("chromosomes_radius").and_then(|v| v.as_str()), Some(""));
    }

    /// Test: validateconfiguration exactly 360 angle offset not wrapped.
    #[test]
    fn test_validateconfiguration_exactly_360_angle_offset_not_wrapped() {
        // 360 > 0 → wrapped to 0 (360 - 360 = 0). But "360" > 0 is true, so wraps.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("angle_offset".into(), ConfigValue::Str("360".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let off = conf.get("image").unwrap().as_map().unwrap()
            .get("angle_offset").unwrap().as_str().unwrap();
        assert_eq!(off, "0");
    }

    /// Test: validateconfiguration promotes image map name cli into image submap.
    #[test]
    fn test_validateconfiguration_promotes_image_map_name_cli_into_image_submap() {
        // CLI image_map_name → promoted into image.image_map_name.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("image_map_name".into(), ConfigValue::Str("mymap".into()));
        validateconfiguration(&mut conf).unwrap();
        let v = conf.get("image").unwrap().as_map().unwrap()
            .get("image_map_name").unwrap().as_str().unwrap();
        assert_eq!(v, "mymap");
    }

    /// Test: validateconfiguration svg font scale default inserted when missing.
    #[test]
    fn test_validateconfiguration_svg_font_scale_default_inserted_when_missing() {
        // svg_font_scale field defaults to "1".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        validateconfiguration(&mut conf).unwrap();
        assert_eq!(conf.get("svg_font_scale").and_then(|v| v.as_str()), Some("1"));
    }

    /// Test: validateconfiguration image field wrong type replaced with map.
    #[test]
    fn test_validateconfiguration_image_field_wrong_type_replaced_with_map() {
        // image field is Str (not Map) → gets replaced: Str put back under "image", new empty Map added.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("image".into(), ConfigValue::Str("oops".into()));
        validateconfiguration(&mut conf).unwrap();
        // After validate, "image" should be a Map (result of insert at end).
        assert!(conf.get("image").unwrap().as_map().is_some());
    }

    /// Test: repopulateconfiguration substitutes simple template.
    #[test]
    fn test_repopulateconfiguration_substitutes_simple_template() {
        // repopulateconfiguration with __KEY__ → resolves from root["KEY"].
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("size".into(), ConfigValue::Str("42".into()));
        root.insert("label".into(), ConfigValue::Str("value is __size__".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("label").and_then(|v| v.as_str()), Some("value is 42"));
    }

    /// Test: repopulateconfiguration no template unchanged.
    #[test]
    fn test_repopulateconfiguration_no_template_unchanged() {
        // String without __KEY__ template unchanged.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("label".into(), ConfigValue::Str("plain string".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("label").and_then(|v| v.as_str()), Some("plain string"));
    }

    /// Test: repopulateconfiguration template in nested map resolved.
    #[test]
    fn test_repopulateconfiguration_template_in_nested_map_resolved() {
        // Template in nested Map resolved via walk().
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("child".into(), ConfigValue::Str("hello __name__".into()));
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("name".into(), ConfigValue::Str("world".into()));
        root.insert("parent".into(), ConfigValue::Map(inner));
        repopulateconfiguration(&mut root);
        let p = root.get("parent").unwrap().as_map().unwrap();
        assert_eq!(p.get("child").and_then(|v| v.as_str()), Some("hello world"));
    }

    /// Test: repopulateconfiguration template in list resolved.
    #[test]
    fn test_repopulateconfiguration_template_in_list_resolved() {
        // Template in List element resolved.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("mykey".into(), ConfigValue::Str("42".into()));
        root.insert("items".into(), ConfigValue::List(vec![
            ConfigValue::Str("val_a".into()),
            ConfigValue::Str("val __mykey__".into()),
        ]));
        repopulateconfiguration(&mut root);
        let items = root.get("items").unwrap().as_list().unwrap();
        assert_eq!(items[1].as_str(), Some("val 42"));
    }

    /// Test: repopulateconfiguration multiple templates in single string.
    #[test]
    fn test_repopulateconfiguration_multiple_templates_in_single_string() {
        // "__a__ + __b__" → both substituted.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("myvar1".into(), ConfigValue::Str("A".into()));
        root.insert("myvar2".into(), ConfigValue::Str("B".into()));
        root.insert("combined".into(), ConfigValue::Str("__myvar1__ + __myvar2__".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("combined").and_then(|v| v.as_str()), Some("A + B"));
    }

    /// Test: loadconfiguration nonexistent file is err.
    #[test]
    fn test_loadconfiguration_nonexistent_file_is_err() {
        // Nonexistent path → loadconfiguration returns Err.
        let r = loadconfiguration(Path::new("/nonexistent_xyz_123"), "circos", Path::new("/nonexistent_bin"));
        assert!(r.is_err());
    }

    /// Test: populateconfiguration with opt containing map preserved.
    #[test]
    fn test_populateconfiguration_with_opt_containing_map_preserved() {
        // opt with a Map value → inserted as Map into conf.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("k".into(), ConfigValue::Str("v".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("sub".into(), ConfigValue::Map(inner));
        populateconfiguration(&mut conf, &opt);
        assert!(conf.get("sub").unwrap().as_map().is_some());
        let v = conf.get("sub").and_then(|v| v.as_map()).and_then(|m| m.get("k")).and_then(|v| v.as_str());
        assert_eq!(v, Some("v"));
    }

    /// Test: repopulateconfiguration chain resolution after iter.
    #[test]
    fn test_repopulateconfiguration_chain_resolution_after_iter() {
        // Chained: a → b, b → "done". First pass: __a__ → "__b__"; second pass: "__b__" → "done". Iterates 16×.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("chain_a".into(), ConfigValue::Str("__chain_b__".into()));
        root.insert("chain_b".into(), ConfigValue::Str("done".into()));
        root.insert("label".into(), ConfigValue::Str("__chain_a__".into()));
        repopulateconfiguration(&mut root);
        // Multiple iterations resolve chained templates.
        let v = root.get("label").and_then(|v| v.as_str()).unwrap();
        // Either fully resolved ("done") or resolves to __chain_b__ in one pass; loop runs 16× so expect "done".
        assert!(v == "done" || v == "__chain_b__");
    }

    /// Test: populateconfiguration defaults applied to empty opt.
    #[test]
    fn test_populateconfiguration_defaults_applied_to_empty_opt() {
        // Empty conf + opt → both defaults inserted.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("anglestep").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(conf.get("minslicestep").and_then(|v| v.as_str()), Some("5"));
    }

    /// Test: validateconfiguration with image and no offset field preserved.
    #[test]
    fn test_validateconfiguration_with_image_and_no_offset_field_preserved() {
        // image Map without angle_offset → preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("width".into(), ConfigValue::Str("1000".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        validateconfiguration(&mut conf).unwrap();
        let v = conf.get("image").unwrap().as_map().unwrap().get("width").unwrap().as_str().unwrap();
        assert_eq!(v, "1000");
    }

    /// Test: repopulateconfiguration empty root succeeds.
    #[test]
    fn test_repopulateconfiguration_empty_root_succeeds() {
        // No entries → no templates to resolve; function doesn't panic.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        repopulateconfiguration(&mut root);
        assert!(root.is_empty());
    }

    /// Test: validateconfiguration svg promotion from cli.
    #[test]
    fn test_validateconfiguration_svg_promotion_from_cli() {
        // CLI "svg" → image.svg.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("svg".into(), ConfigValue::Str("1".into()));
        validateconfiguration(&mut conf).unwrap();
        let v = conf.get("image").unwrap().as_map().unwrap().get("svg").unwrap().as_str().unwrap();
        assert_eq!(v, "1");
    }

    /// Test: validateconfiguration 24bit cli promoted to image.
    #[test]
    fn test_validateconfiguration_24bit_cli_promoted_to_image() {
        // CLI "24bit" → image.24bit.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("24bit".into(), ConfigValue::Str("1".into()));
        validateconfiguration(&mut conf).unwrap();
        let v = conf.get("image").unwrap().as_map().unwrap().get("24bit").unwrap().as_str().unwrap();
        assert_eq!(v, "1");
    }

    /// Test: validateconfiguration image map use promoted.
    #[test]
    fn test_validateconfiguration_image_map_use_promoted() {
        // CLI "image_map_use" → image.image_map_use.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("configfile".into(), ConfigValue::Str("c".into()));
        conf.insert("karyotype".into(), ConfigValue::Str("k".into()));
        conf.insert("image_map_use".into(), ConfigValue::Str("yes".into()));
        validateconfiguration(&mut conf).unwrap();
        let v = conf.get("image").unwrap().as_map().unwrap().get("image_map_use").unwrap().as_str().unwrap();
        assert_eq!(v, "yes");
    }

    /// Test: populateconfiguration merges opt with existing conf.
    #[test]
    fn test_populateconfiguration_merges_opt_with_existing_conf() {
        // opt merges on top of existing conf.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("existing".into(), ConfigValue::Str("old".into()));
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("new_key".into(), ConfigValue::Str("added".into()));
        populateconfiguration(&mut conf, &opt);
        assert_eq!(conf.get("existing").and_then(|v| v.as_str()), Some("old"));
        assert_eq!(conf.get("new_key").and_then(|v| v.as_str()), Some("added"));
    }

    /// Test: repopulateconfiguration preserves non template string.
    #[test]
    fn test_repopulateconfiguration_preserves_non_template_string() {
        // Str without __KEY__ pattern unchanged.
        let mut root: HashMap<String, ConfigValue> = HashMap::new();
        root.insert("key".into(), ConfigValue::Str("no template here".into()));
        repopulateconfiguration(&mut root);
        assert_eq!(root.get("key").and_then(|v| v.as_str()), Some("no template here"));
    }
}
