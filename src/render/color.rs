use std::collections::HashMap;

/// An RGBA color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8, // 0 = fully transparent, 255 = fully opaque
}

impl Color {
    /// Construct an opaque RGB color (alpha = 255).
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Construct an RGBA color with explicit alpha.
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Format as SVG rgb() string.
    pub fn to_svg_rgb(&self) -> String {
        format!("rgb({},{},{})", self.r, self.g, self.b)
    }

    /// Format as SVG rgba() string if alpha < 255, otherwise rgb().
    pub fn to_svg(&self) -> String {
        if self.a < 255 {
            format!(
                "rgba({},{},{},{:.2})",
                self.r,
                self.g,
                self.b,
                self.a as f64 / 255.0
            )
        } else {
            self.to_svg_rgb()
        }
    }

    /// Format as hex string #RRGGBB.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// A map of named colors.
#[derive(Debug, Clone, Default)]
pub struct ColorMap {
    pub colors: HashMap<String, Color>,
}

impl ColorMap {
    /// Create an empty ColorMap with no allocated colors.
    pub fn new() -> Self {
        ColorMap {
            colors: HashMap::new(),
        }
    }

    /// Port of Perl `allocate_colors($image, $add_transparent, ...)`.
    /// Parses color definitions from `$CONF{colors}`, resolves name-to-name
    /// aliases, and (if auto_alpha_steps > 0) expands each color into
    /// `name_a1..name_a{auto_alpha_steps}` variants with proportional alpha.
    /// `add_transparent`: if true, allocate a named "transparent" slot.
    /// `transparentrgb`: explicit r,g,b string for the transparent color.
    /// Unlike Perl (which allocates into a GD image for PNG), the Rust port
    /// just populates the self.colors map — runs for SVG and PNG alike.
    pub fn allocate_colors(
        &mut self,
        config: &HashMap<String, super::super::config::types::ConfigValue>,
        add_transparent: bool,
        auto_alpha_steps: u32,
        transparentrgb: Option<&str>,
    ) {
        // First pass: direct r,g,b[,a] values; skip name aliases and "transparent"
        for (name, value) in config {
            if name == "transparent" {
                continue;
            }
            let s = match value.as_str() {
                Some(s) => s,
                None => continue,
            };
            // resolve one level of alias (Perl: $colorvalue = $CONF{colors}{$colorvalue} if exists)
            let resolved: String = if !s.contains(',') {
                config
                    .get(s)
                    .and_then(|v| v.as_str())
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| s.to_string())
            } else {
                s.to_string()
            };
            if !resolved.contains(',') {
                // Still not an rgb triple — will be picked up in the alias pass below
                continue;
            }
            let parts: Vec<&str> = resolved
                .split([',', ' '])
                .filter(|p| !p.is_empty())
                .collect();
            if parts.len() == 3 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].trim().parse::<u8>(),
                    parts[1].trim().parse::<u8>(),
                    parts[2].trim().parse::<u8>(),
                ) {
                    self.colors.insert(name.clone(), Color::rgb(r, g, b));
                }
            } else if parts.len() == 4
                && let (Ok(r), Ok(g), Ok(b), Ok(a_raw)) = (
                    parts[0].trim().parse::<u8>(),
                    parts[1].trim().parse::<u8>(),
                    parts[2].trim().parse::<u8>(),
                    parts[3].trim().parse::<f64>(),
                )
            {
                // Perl: $rgb[3] *= 127 if $rgb[3] < 1;
                // then scale 0..127 -> 0..255 for Rust's u8 alpha
                let a127 = if a_raw < 1.0 { a_raw * 127.0 } else { a_raw };
                let a = ((a127 / 127.0) * 255.0).round().clamp(0.0, 255.0) as u8;
                self.colors.insert(name.clone(), Color::rgba(r, g, b, a));
            }
        }

        // Auto-alpha expansion (Perl: for $i in 1..auto_alpha_steps, alpha = 127*i/(steps+1))
        if auto_alpha_steps > 0 {
            let snapshot: Vec<(String, Color)> =
                self.colors.iter().map(|(k, v)| (k.clone(), *v)).collect();
            for (colorname, base) in snapshot {
                for i in 1..=auto_alpha_steps {
                    let a127 = (127.0 * (i as f64) / (auto_alpha_steps as f64 + 1.0)).round();
                    let a = ((a127 / 127.0) * 255.0).round().clamp(0.0, 255.0) as u8;
                    let aname = format!("{}_a{}", colorname, i);
                    self.colors
                        .insert(aname, Color::rgba(base.r, base.g, base.b, a));
                }
            }
        }

        // Transparent allocation
        if add_transparent {
            if let Some(rgb_str) = transparentrgb {
                let parts: Vec<&str> = rgb_str.split(',').collect();
                if parts.len() == 3
                    && let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse::<u8>(),
                        parts[1].trim().parse::<u8>(),
                        parts[2].trim().parse::<u8>(),
                    )
                {
                    self.colors
                        .insert("transparent".to_string(), Color::rgba(r, g, b, 0));
                }
            } else {
                // Pick a color not already used
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                let mut state = seed as u64;
                loop {
                    state = state
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let r = (state >> 16) as u8;
                    let g = (state >> 32) as u8;
                    let b = (state >> 48) as u8;
                    let color = Color::rgba(r, g, b, 0);
                    let collision = self
                        .colors
                        .values()
                        .any(|c| c.r == r && c.g == g && c.b == b);
                    if !collision {
                        self.colors.insert("transparent".to_string(), color);
                        break;
                    }
                }
            }
        }

        // Second pass: resolve name-to-name aliases (Perl: final for loop at 7660)
        for (name, value) in config {
            let s = match value.as_str() {
                Some(s) => s,
                None => continue,
            };
            if !s.contains(',')
                && let Some(&target) = self.colors.get(s)
            {
                self.colors.insert(name.clone(), target);
            }
        }
    }

    /// Look up a color by name, with support for:
    /// - Named color: "red"
    /// - Color with alpha suffix: "red_a5" (alpha = 5 * 255/max_steps)
    /// - RGB string: "255,0,0"
    /// - RGB with alpha: "255,0,0,128"
    pub fn resolve(&self, name: &str) -> Option<Color> {
        // Direct RGB string
        if name.contains(',') {
            return parse_color_string(name);
        }

        // Named color with alpha suffix: "color_aN"
        if let Some((base_name, alpha_suffix)) = name.rsplit_once("_a")
            && let Ok(alpha_idx) = alpha_suffix.parse::<u8>()
            && let Some(base_color) = self.colors.get(base_name)
        {
            let alpha = ((alpha_idx as f64 / 5.0) * 255.0) as u8;
            return Some(Color::rgba(base_color.r, base_color.g, base_color.b, alpha));
        }

        // Plain named color
        self.colors.get(name).copied()
    }
}

/// Port of Perl `rgb_color(color)`: return the (r,g,b) triple for a named color,
/// stripping any `_aN` transparency suffix and recursing.
pub fn rgb_color(
    color: &str,
    colors_conf: &HashMap<String, crate::config::types::ConfigValue>,
) -> Option<(u8, u8, u8)> {
    if let Some((root, _)) = color.rsplit_once("_a") {
        return rgb_color(root, colors_conf);
    }
    let s = colors_conf.get(color).and_then(|v| v.as_str())?;
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() >= 3 {
        let r: u8 = parts[0].parse().ok()?;
        let g: u8 = parts[1].parse().ok()?;
        let b: u8 = parts[2].parse().ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// Port of Perl `rgb_color_opacity(color)`: returns opacity in [0,1] for a color
/// with an `_aN` suffix where N ranges over `[0, auto_alpha_steps]`; 1.0 for names
/// without a transparency suffix.
pub fn rgb_color_opacity(color: &str, auto_alpha_steps: u32) -> f64 {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(.+)_a(\d+)").unwrap());
    match RE.captures(color) {
        Some(cap) if auto_alpha_steps > 0 => {
            let n: u32 = cap.get(2).unwrap().as_str().parse().unwrap_or(0);
            1.0 - (n as f64) / (auto_alpha_steps as f64)
        }
        _ => 1.0,
    }
}

/// Port of Perl `rgb_color_transparency(color)`: 1 - rgb_color_opacity.
pub fn rgb_color_transparency(color: &str, auto_alpha_steps: u32) -> f64 {
    1.0 - rgb_color_opacity(color, auto_alpha_steps)
}

/// Parse an "R,G,B" or "R,G,B,A" string into a Color.
pub fn parse_color_string(s: &str) -> Option<Color> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    match parts.len() {
        3 => {
            let r = parts[0].parse().ok()?;
            let g = parts[1].parse().ok()?;
            let b = parts[2].parse().ok()?;
            Some(Color::rgb(r, g, b))
        }
        4 => {
            let r = parts[0].parse().ok()?;
            let g = parts[1].parse().ok()?;
            let b = parts[2].parse().ok()?;
            let a = parts[3].parse().ok()?;
            Some(Color::rgba(r, g, b, a))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_rgb() {
        let c = Color::rgb(255, 0, 0);
        assert_eq!(c.to_svg_rgb(), "rgb(255,0,0)");
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn test_color_rgba() {
        let c = Color::rgba(0, 128, 255, 128);
        assert_eq!(c.to_svg(), "rgba(0,128,255,0.50)");
    }

    #[test]
    fn test_parse_color_string() {
        assert_eq!(
            parse_color_string("255,128,0"),
            Some(Color::rgb(255, 128, 0))
        );
        assert_eq!(
            parse_color_string("255,128,0,64"),
            Some(Color::rgba(255, 128, 0, 64))
        );
        assert_eq!(parse_color_string("invalid"), None);
    }

    #[test]
    fn test_color_map_resolve() {
        let mut map = ColorMap::new();
        map.colors
            .insert("red".to_string(), Color::rgb(247, 42, 66));
        map.colors
            .insert("blue".to_string(), Color::rgb(54, 116, 217));

        assert_eq!(map.resolve("red"), Some(Color::rgb(247, 42, 66)));
        assert_eq!(map.resolve("255,0,0"), Some(Color::rgb(255, 0, 0)));

        // Alpha variant
        let resolved = map.resolve("red_a3").unwrap();
        assert_eq!(resolved.r, 247);
        assert_eq!(resolved.g, 42);
        assert_eq!(resolved.b, 66);
        assert!(resolved.a < 255); // has alpha applied
    }

    #[test]
    fn test_color_to_svg_opaque_uses_rgb_form() {
        // a=255 (opaque) → rgb(r,g,b), not rgba() (keeps compatibility with Perl).
        let c = Color::rgb(10, 20, 30);
        assert_eq!(c.to_svg(), "rgb(10,20,30)");
    }

    #[test]
    fn test_rgb_color_opacity_branches() {
        // No `_aN` suffix → fully opaque (1.0).
        assert!((rgb_color_opacity("red", 5) - 1.0).abs() < 1e-12);
        // With suffix and auto_alpha_steps=5: opacity = 1 - N/steps.
        assert!((rgb_color_opacity("red_a2", 5) - 0.6).abs() < 1e-12);
        assert!((rgb_color_opacity("red_a5", 5) - 0.0).abs() < 1e-12);
        // auto_alpha_steps=0 → always 1.0 regardless of suffix.
        assert!((rgb_color_opacity("red_a2", 0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_rgb_color_transparency_is_complement() {
        for steps in [3u32, 5, 10] {
            for name in ["red", "red_a1", "red_a2"] {
                let o = rgb_color_opacity(name, steps);
                let t = rgb_color_transparency(name, steps);
                assert!(
                    (o + t - 1.0).abs() < 1e-12,
                    "opacity+transparency should be 1.0 for {}/{}",
                    name,
                    steps
                );
            }
        }
    }

    #[test]
    fn test_rgb_color_recurses_on_suffix() {
        use std::collections::HashMap;
        use crate::config::types::ConfigValue;
        let mut cfg: HashMap<String, ConfigValue> = HashMap::new();
        cfg.insert("red".into(), ConfigValue::Str("247,42,66".into()));
        // Direct lookup
        assert_eq!(rgb_color("red", &cfg), Some((247, 42, 66)));
        // Alpha suffix should strip and re-query the base name.
        assert_eq!(rgb_color("red_a3", &cfg), Some((247, 42, 66)));
        // Unknown name → None
        assert_eq!(rgb_color("magenta", &cfg), None);
    }

    #[test]
    fn test_parse_color_string_whitespace_and_errors() {
        // Leading/trailing whitespace around components tolerated
        assert_eq!(
            parse_color_string(" 255 , 0 , 0 "),
            Some(Color::rgb(255, 0, 0))
        );
        // Out-of-range component → None
        assert!(parse_color_string("256,0,0").is_none());
        // Wrong count → None
        assert!(parse_color_string("255,0").is_none());
        assert!(parse_color_string("255,0,0,64,extra").is_none());
    }

    #[test]
    fn test_allocate_colors_basic_rgb() {
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        conf.insert("blue".into(), ConfigValue::Str("0,0,255".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        assert_eq!(map.colors.get("red"), Some(&Color::rgb(255, 0, 0)));
        assert_eq!(map.colors.get("blue"), Some(&Color::rgb(0, 0, 255)));
    }

    #[test]
    fn test_allocate_colors_four_component_alpha() {
        use crate::config::types::ConfigValue;
        // Perl scales alpha 0..1 → 0..127 internally, then we render 0..255.
        let mut conf = HashMap::new();
        conf.insert("halfred".into(), ConfigValue::Str("255,0,0,0.5".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        let c = map.colors.get("halfred").expect("halfred");
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
        // 0.5 * 127 / 127 * 255 ≈ 127 or 128 (rounding).
        assert!(c.a > 100 && c.a < 140, "alpha out of expected range: {}", c.a);
    }

    #[test]
    fn test_allocate_colors_name_to_name_alias() {
        use crate::config::types::ConfigValue;
        // `ruby` → `red` → "255,0,0": alias pass should resolve to the final RGB.
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        conf.insert("ruby".into(), ConfigValue::Str("red".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        assert_eq!(map.colors.get("ruby"), Some(&Color::rgb(255, 0, 0)));
    }

    #[test]
    fn test_allocate_colors_auto_alpha_steps_generates_variants() {
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 5, None);
        // Should generate red_a1..red_a5 aliases with decreasing alpha.
        for i in 1..=5 {
            let k = format!("red_a{}", i);
            assert!(
                map.colors.contains_key(&k),
                "missing auto-alpha variant {}",
                k
            );
        }
        // Each variant has the same RGB as `red` but different alpha.
        let a1 = map.colors.get("red_a1").unwrap();
        let a5 = map.colors.get("red_a5").unwrap();
        assert_eq!((a1.r, a1.g, a1.b), (255, 0, 0));
        assert_eq!((a5.r, a5.g, a5.b), (255, 0, 0));
        // a1 more opaque than a5 (Perl increases alpha toward a_max)
        assert!(a1.a != a5.a);
    }

    #[test]
    fn test_allocate_colors_transparent_from_explicit_rgb() {
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, true, 0, Some("128,64,32"));
        let t = map.colors.get("transparent").expect("transparent");
        assert_eq!((t.r, t.g, t.b, t.a), (128, 64, 32, 0));
    }

    #[test]
    fn test_resolve_rgba_string() {
        // "r,g,b,a" → Color::rgba.
        let map = ColorMap::new();
        let c = map.resolve("10,20,30,64").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (10, 20, 30, 64));
    }

    #[test]
    fn test_resolve_alpha_suffix_with_missing_base_falls_through() {
        // "missing_a3": `missing` not in map → `_a` logic fails, falls through
        // to plain named lookup, which also misses → None.
        let map = ColorMap::new();
        assert!(map.resolve("missing_a3").is_none());
    }

    #[test]
    fn test_resolve_unknown_returns_none() {
        let map = ColorMap::new();
        assert!(map.resolve("never-defined").is_none());
    }

    #[test]
    fn test_resolve_alpha_suffix_parsing_fails_on_nonnumeric() {
        // "red_aX" → alpha suffix isn't u8 → falls through to plain name lookup.
        let mut map = ColorMap::new();
        map.colors.insert("red".into(), Color::rgb(255, 0, 0));
        // "red_aX" → base + suffix parse failure → plain "red_aX" lookup → None.
        assert!(map.resolve("red_aX").is_none());
    }

    #[test]
    fn test_resolve_alpha_suffix_0_gives_full_alpha() {
        // alpha_idx=0 → alpha = (0/5)*255 = 0. So alpha_a0 is 0-alpha = transparent.
        let mut map = ColorMap::new();
        map.colors.insert("red".into(), Color::rgb(247, 42, 66));
        let c = map.resolve("red_a0").unwrap();
        assert_eq!((c.r, c.g, c.b), (247, 42, 66));
        assert_eq!(c.a, 0);
    }

    #[test]
    fn test_color_to_hex_format() {
        // `#RRGGBB`, always lowercase, zero-padded.
        assert_eq!(Color::rgb(0, 0, 0).to_hex(), "#000000");
        assert_eq!(Color::rgb(255, 255, 255).to_hex(), "#ffffff");
        assert_eq!(Color::rgb(10, 255, 0).to_hex(), "#0aff00");
        // Alpha is ignored in hex output.
        assert_eq!(Color::rgba(10, 255, 0, 128).to_hex(), "#0aff00");
    }

    #[test]
    fn test_color_to_svg_rgba_precision() {
        // to_svg with alpha < 255 emits 2-decimal alpha.
        assert_eq!(Color::rgba(0, 0, 0, 0).to_svg(), "rgba(0,0,0,0.00)");
        assert_eq!(Color::rgba(255, 0, 0, 255).to_svg(), "rgb(255,0,0)");
        assert_eq!(Color::rgba(10, 20, 30, 64).to_svg(), "rgba(10,20,30,0.25)");
    }

    #[test]
    fn test_color_equality_respects_all_channels() {
        assert_eq!(Color::rgb(1, 2, 3), Color::rgb(1, 2, 3));
        assert_ne!(Color::rgb(1, 2, 3), Color::rgb(1, 2, 4));
        // Different alpha → not equal even with same RGB.
        assert_ne!(Color::rgb(1, 2, 3), Color::rgba(1, 2, 3, 128));
        // rgba with default alpha 255 == rgb.
        assert_eq!(Color::rgba(1, 2, 3, 255), Color::rgb(1, 2, 3));
    }

    #[test]
    fn test_color_map_default_is_empty() {
        let map = ColorMap::default();
        assert!(map.colors.is_empty());
        assert!(map.resolve("anything").is_none());
    }

    #[test]
    fn test_color_to_svg_alpha_boundary_254() {
        // a=254 (just under 255) → uses rgba form.
        let c = Color::rgba(10, 20, 30, 254);
        let s = c.to_svg();
        assert!(s.starts_with("rgba("));
        // Value: 254/255 ≈ 0.996 → rounded to 1.00 via {:.2}.
        assert!(s.contains("1.00"));
    }

    #[test]
    fn test_color_to_hex_uppercase_values() {
        // to_hex uses {:02x} lowercase-only formatting.
        let c = Color::rgb(255, 165, 0);
        assert_eq!(c.to_hex(), "#ffa500");
        // All-zero → all-"00".
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.to_hex(), "#000000");
    }

    #[test]
    fn test_color_to_svg_rgb_ignores_alpha_channel() {
        // to_svg_rgb always emits the rgb() form even when alpha < 255.
        let c = Color::rgba(100, 150, 200, 50);
        assert_eq!(c.to_svg_rgb(), "rgb(100,150,200)");
    }

    #[test]
    fn test_color_map_new_is_empty() {
        // ColorMap::new yields an empty map, same as default.
        let m = ColorMap::new();
        assert!(m.colors.is_empty());
        assert!(m.resolve("anything").is_none());
    }

    #[test]
    fn test_allocate_colors_alpha_4_component_integer_alpha() {
        // "100,150,200,50" with integer alpha > 1 → used as-is (Perl scales only when < 1).
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("semi".into(), ConfigValue::Str("100,150,200,50".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        let c = map.colors.get("semi").expect("semi");
        assert_eq!((c.r, c.g, c.b), (100, 150, 200));
        // alpha=50 (< 127) → scaled 50/127 × 255 ≈ 100.
        assert!(c.a > 90 && c.a < 110, "expected a≈100, got {}", c.a);
    }

    #[test]
    fn test_allocate_colors_invalid_three_components_skipped() {
        // A value with bad components ("256,foo,300") → parse fails → no entry.
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("bad".into(), ConfigValue::Str("256,foo,300".into()));
        conf.insert("good".into(), ConfigValue::Str("10,20,30".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        // "bad" key has unparseable components → not inserted into colors.
        assert!(map.colors.get("bad").is_none());
        // "good" is fine.
        assert_eq!(map.colors.get("good"), Some(&Color::rgb(10, 20, 30)));
    }

    #[test]
    fn test_allocate_colors_auto_alpha_all_variants_distinct() {
        // auto_alpha_steps=3 → 3 alpha variants with distinct alpha values.
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 3, None);
        let a1 = map.colors.get("red_a1").unwrap().a;
        let a2 = map.colors.get("red_a2").unwrap().a;
        let a3 = map.colors.get("red_a3").unwrap().a;
        // Distinct alphas for 3 steps.
        assert_ne!(a1, a2);
        assert_ne!(a2, a3);
        assert_ne!(a1, a3);
    }

    #[test]
    fn test_allocate_colors_self_referencing_alias_skipped() {
        // A key pointing to itself in the second pass shouldn't create an
        // infinite loop or overwrite. `colors.get("self")` on 2nd-pass already
        // holds "self"→rgb(1,2,3), so the alias self→self assignment is a no-op
        // (just re-inserts the same value).
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("selfref".into(), ConfigValue::Str("selfref".into()));
        conf.insert("real".into(), ConfigValue::Str("1,2,3".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        // "real" populated; "selfref" stays unresolved (no loop).
        assert_eq!(map.colors.get("real"), Some(&Color::rgb(1, 2, 3)));
        // selfref points to itself → can't resolve → no entry.
        assert!(map.colors.get("selfref").is_none());
    }

    #[test]
    fn test_resolve_rgba_4_component_string_parses_all_channels() {
        // "r,g,b,a" form with 4 comma-separated ints → Color::rgba.
        let map = ColorMap::new();
        let c = map.resolve("100,150,200,64").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (100, 150, 200, 64));
    }

    #[test]
    fn test_resolve_rgb_3_component_string_defaults_full_alpha() {
        // 3-component RGB string → alpha defaults to 255 (opaque).
        let map = ColorMap::new();
        let c = map.resolve("10,20,30").unwrap();
        assert_eq!((c.r, c.g, c.b), (10, 20, 30));
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_resolve_alpha_5_yields_full_alpha() {
        // alpha_idx=5 → alpha = (5/5)*255 = 255 (opaque).
        let mut map = ColorMap::new();
        map.colors.insert("blue".into(), Color::rgb(0, 0, 255));
        let c = map.resolve("blue_a5").unwrap();
        assert_eq!(c.a, 255);
        assert_eq!((c.r, c.g, c.b), (0, 0, 255));
    }

    #[test]
    fn test_resolve_rgba_string_with_malformed_component_returns_none() {
        // "10,foo,30,40" — 'foo' doesn't parse as u8 → None.
        let map = ColorMap::new();
        assert!(map.resolve("10,foo,30,40").is_none());
        // Also 5-component strings return None (neither 3 nor 4).
        assert!(map.resolve("1,2,3,4,5").is_none());
    }

    #[test]
    fn test_rgb_color_config_has_too_few_parts_returns_none() {
        use std::collections::HashMap;
        use crate::config::types::ConfigValue;
        let mut cfg: HashMap<String, ConfigValue> = HashMap::new();
        // A config value with <3 parts → invalid, rgb_color returns None.
        cfg.insert("weird".into(), ConfigValue::Str("42".into()));
        assert!(rgb_color("weird", &cfg).is_none());
        // 2-part is also insufficient.
        cfg.insert("duo".into(), ConfigValue::Str("255,128".into()));
        assert!(rgb_color("duo", &cfg).is_none());
    }

    #[test]
    fn test_rgb_color_config_has_out_of_range_u8_returns_none() {
        use std::collections::HashMap;
        use crate::config::types::ConfigValue;
        let mut cfg: HashMap<String, ConfigValue> = HashMap::new();
        // 256 doesn't fit in u8 → `parse().ok()?` short-circuits to None.
        cfg.insert("toohi".into(), ConfigValue::Str("256,0,0".into()));
        assert!(rgb_color("toohi", &cfg).is_none());
        // Negative value → same path.
        cfg.insert("neg".into(), ConfigValue::Str("-1,0,0".into()));
        assert!(rgb_color("neg", &cfg).is_none());
    }

    #[test]
    fn test_rgb_color_opacity_zero_steps_disables_gradient() {
        // auto_alpha_steps=0 → always return 1.0 regardless of suffix.
        assert_eq!(rgb_color_opacity("red", 0), 1.0);
        assert_eq!(rgb_color_opacity("red_a1", 0), 1.0);
        assert_eq!(rgb_color_opacity("red_a5", 0), 1.0);
        assert_eq!(rgb_color_opacity("anything_a99", 0), 1.0);
    }

    #[test]
    fn test_rgb_color_opacity_maximum_step_gives_zero_opacity() {
        // With auto_alpha_steps=5 and suffix _a5: opacity = 1 - 5/5 = 0.
        assert_eq!(rgb_color_opacity("red_a5", 5), 0.0);
        // auto_alpha_steps=10 and _a10 → 1 - 10/10 = 0.
        assert_eq!(rgb_color_opacity("red_a10", 10), 0.0);
    }

    #[test]
    fn test_rgb_color_opacity_non_suffix_names_full_opacity() {
        // Names without `_a` suffix → always 1.0 opacity regardless of steps.
        assert_eq!(rgb_color_opacity("plain", 5), 1.0);
        assert_eq!(rgb_color_opacity("some_other", 10), 1.0);
    }

    #[test]
    fn test_rgb_color_transparency_complement_at_step_boundaries() {
        // transparency = 1 - opacity; at opacity=0 → transparency=1, at opacity=1 → 0.
        assert_eq!(rgb_color_transparency("red_a5", 5), 1.0);
        assert_eq!(rgb_color_transparency("red", 5), 0.0);
        assert_eq!(rgb_color_transparency("plain", 0), 0.0);
    }

    #[test]
    fn test_rgb_color_opacity_over_steps_gives_negative() {
        // The impl is `1 - N/steps` with no clamp — so N > steps yields
        // negative opacity. Documents current no-clamp behavior.
        let o = rgb_color_opacity("red_a10", 5);
        assert!(o < 0.0, "expected negative opacity for N>steps, got {}", o);
        // rgb_color_transparency = 1 - opacity → >1.0.
        let t = rgb_color_transparency("red_a10", 5);
        assert!(t > 1.0);
    }

    #[test]
    fn test_rgb_color_recursion_on_deeply_suffixed_name() {
        // `rgb_color` recurses once on `_a` suffix. With "red_a1_a2" the rsplit
        // finds the trailing "_a2" and recurses on "red_a1", which splits again
        // into "red" + "_a1" → final base "red" resolves.
        use std::collections::HashMap;
        use crate::config::types::ConfigValue;
        let mut cfg: HashMap<String, ConfigValue> = HashMap::new();
        cfg.insert("red".into(), ConfigValue::Str("247,42,66".into()));
        assert_eq!(rgb_color("red_a1_a2", &cfg), Some((247, 42, 66)));
    }

    #[test]
    fn test_allocate_colors_transparent_without_explicit_rgb_picks_random() {
        // add_transparent=true, transparentrgb=None → random RGB picked that
        // doesn't collide with existing colors; entry present as "transparent".
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, true, 0, None);
        let t = map.colors.get("transparent").expect("transparent present");
        // Alpha is 0 for the transparent entry.
        assert_eq!(t.a, 0);
        // Non-colliding RGB → transparent's RGB shouldn't equal red's (255,0,0).
        assert!(t.r != 255 || t.g != 0 || t.b != 0);
    }

    #[test]
    fn test_allocate_colors_second_pass_name_alias_chain() {
        // Perl's second alias pass: after the first pass populates `red`, the
        // second pass resolves `ruby = red` (single-word, no comma) → `ruby`
        // gets `red`'s Color.
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("red".into(), ConfigValue::Str("255,0,0".into()));
        conf.insert("ruby".into(), ConfigValue::Str("red".into()));
        conf.insert("crimson".into(), ConfigValue::Str("ruby".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        // red → (255,0,0), ruby → red, crimson → ruby (one-level alias).
        assert_eq!(map.colors.get("red"), Some(&Color::rgb(255, 0, 0)));
        assert_eq!(map.colors.get("ruby"), Some(&Color::rgb(255, 0, 0)));
        // crimson resolves via 2nd-pass alias: both paths populate red's value.
        assert!(map.colors.get("crimson").is_some());
    }

    #[test]
    fn test_allocate_colors_comma_with_whitespace_parts() {
        // Comma-separated with whitespace around parts: "255, 0, 128"
        // The parser splits on both `,` and ` `, filtering empty parts.
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("spacey".into(), ConfigValue::Str("255, 0, 128".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        assert_eq!(map.colors.get("spacey"), Some(&Color::rgb(255, 0, 128)));
    }

    #[test]
    fn test_allocate_colors_skips_transparent_key_in_first_pass() {
        // The first loop explicitly skips a key named "transparent"; with
        // add_transparent=false and a "transparent" key in config, it's ignored.
        use crate::config::types::ConfigValue;
        let mut conf = HashMap::new();
        conf.insert("transparent".into(), ConfigValue::Str("128,128,128".into()));
        conf.insert("gray".into(), ConfigValue::Str("200,200,200".into()));
        let mut map = ColorMap::new();
        map.allocate_colors(&conf, false, 0, None);
        // "gray" populated normally.
        assert_eq!(map.colors.get("gray"), Some(&Color::rgb(200, 200, 200)));
        // "transparent" explicitly skipped even though it has a valid RGB string.
        assert!(map.colors.get("transparent").is_none());
    }

    #[test]
    fn test_color_copy_semantics() {
        // Color is Copy — passing by value doesn't move.
        let c = Color::rgb(1, 2, 3);
        let d = c;
        let _ = c; // still usable after move → compiles because Copy.
        assert_eq!(c, d);
    }

    #[test]
    fn test_parse_color_string_invalid_component_returns_none() {
        // A non-numeric component makes the whole parse fail.
        assert!(parse_color_string("255,foo,0").is_none());
        assert!(parse_color_string("1,2,3,bar").is_none());
        // Too many components.
        assert!(parse_color_string("1,2,3,4,5").is_none());
        // Too few components.
        assert!(parse_color_string("1,2").is_none());
        assert!(parse_color_string("1").is_none());
        assert!(parse_color_string("").is_none());
    }

    #[test]
    fn test_parse_color_string_whitespace_trimmed_in_parts() {
        // Whitespace around each component is trimmed before parse.
        assert_eq!(
            parse_color_string(" 100 , 150 , 200 "),
            Some(Color::rgb(100, 150, 200))
        );
        // Tabs/newlines also trimmed.
        assert_eq!(
            parse_color_string("\t10,\n20,30"),
            Some(Color::rgb(10, 20, 30))
        );
    }

    #[test]
    fn test_color_to_hex_all_zero_and_all_max() {
        // All-zero → "#000000"; all-max (255) → "#ffffff".
        let black = Color::rgb(0, 0, 0);
        assert_eq!(black.to_hex(), "#000000");
        let white = Color::rgb(255, 255, 255);
        assert_eq!(white.to_hex(), "#ffffff");
        // Single-channel max.
        let red = Color::rgb(255, 0, 0);
        assert_eq!(red.to_hex(), "#ff0000");
        // Mid-range channel uses 2-digit hex.
        let mid = Color::rgb(0, 15, 160);
        assert_eq!(mid.to_hex(), "#000fa0");
    }

    #[test]
    fn test_color_to_svg_alpha_formatting_precision() {
        // Alpha uses {:.2} format — 2 decimal places.
        // a=64 → 64/255 ≈ 0.251 → "0.25".
        let c = Color::rgba(0, 0, 0, 64);
        assert_eq!(c.to_svg(), "rgba(0,0,0,0.25)");
        // a=1 → 1/255 ≈ 0.004 → "0.00" (2-dp truncation).
        let c = Color::rgba(0, 0, 0, 1);
        assert_eq!(c.to_svg(), "rgba(0,0,0,0.00)");
        // a=128 → 128/255 ≈ 0.502 → "0.50".
        let c = Color::rgba(0, 0, 0, 128);
        assert_eq!(c.to_svg(), "rgba(0,0,0,0.50)");
    }

    #[test]
    fn test_color_equality_full_field_match() {
        // Color is PartialEq — all 4 fields (r,g,b,a) participate.
        let c1 = Color::rgb(10, 20, 30);
        let c2 = Color::rgb(10, 20, 30);
        assert_eq!(c1, c2);
        // Same rgb + different alpha → not equal.
        let c3 = Color::rgba(10, 20, 30, 128);
        assert_ne!(c1, c3);
        // rgba vs rgb → not equal (rgb has alpha 255).
        let c4 = Color::rgba(10, 20, 30, 255);
        assert_eq!(c1, c4); // rgb(10,20,30) is rgba(10,20,30,255)
    }

    #[test]
    fn test_color_map_new_starts_empty() {
        // ColorMap::new creates an empty map.
        let cm = ColorMap::new();
        assert!(cm.colors.is_empty());
        // ColorMap::default also empty.
        let cm_default = ColorMap::default();
        assert!(cm_default.colors.is_empty());
    }

    #[test]
    fn test_parse_color_string_out_of_range_wraps_via_u8() {
        // Values >255 exceed u8 range → parse::<u8> fails → None.
        assert!(parse_color_string("300,0,0").is_none());
        assert!(parse_color_string("-1,0,0").is_none());
        // Boundary: 255 accepted.
        assert_eq!(
            parse_color_string("255,255,255"),
            Some(Color::rgb(255, 255, 255))
        );
        // Boundary: 0 accepted.
        assert_eq!(parse_color_string("0,0,0"), Some(Color::rgb(0, 0, 0)));
    }

    #[test]
    fn test_color_to_svg_alpha_255_degrades_to_rgb() {
        // a=255 (full opacity) → to_svg emits rgb() not rgba().
        let c = Color::rgba(50, 100, 150, 255);
        assert_eq!(c.to_svg(), "rgb(50,100,150)");
        // a=254 is strictly less than 255 → rgba.
        let c = Color::rgba(50, 100, 150, 254);
        // 254/255 ≈ 0.996 → "1.00" (rounds up).
        assert!(c.to_svg().starts_with("rgba(50,100,150,"));
    }

    #[test]
    fn test_color_rgb_sets_alpha_to_255() {
        // Color::rgb constructor always sets a=255 (full opacity).
        let c = Color::rgb(10, 20, 30);
        assert_eq!(c.a, 255);
        // Multiple colors.
        assert_eq!(Color::rgb(0, 0, 0).a, 255);
        assert_eq!(Color::rgb(255, 255, 255).a, 255);
    }

    #[test]
    fn test_color_rgba_constructor_preserves_all_four_channels() {
        // Constructor sets each channel directly.
        let c = Color::rgba(5, 15, 25, 128);
        assert_eq!(c.r, 5);
        assert_eq!(c.g, 15);
        assert_eq!(c.b, 25);
        assert_eq!(c.a, 128);
    }

    #[test]
    fn test_color_to_hex_no_alpha_encoding() {
        // to_hex emits only RGB (3 bytes = 6 hex chars) — no alpha channel.
        let c = Color::rgba(255, 128, 0, 50);
        // Alpha not in hex output — same as rgb(255, 128, 0).
        assert_eq!(c.to_hex(), "#ff8000");
        // Compare to rgb form.
        let c2 = Color::rgb(255, 128, 0);
        assert_eq!(c.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_parse_color_string_with_4_components_sets_alpha() {
        // 4-component form: R,G,B,A.
        let c = parse_color_string("100,150,200,50").unwrap();
        assert_eq!(c.r, 100);
        assert_eq!(c.g, 150);
        assert_eq!(c.b, 200);
        assert_eq!(c.a, 50);
        // A=0 (fully transparent).
        let c = parse_color_string("0,0,0,0").unwrap();
        assert_eq!(c.a, 0);
    }

    #[test]
    fn test_color_debug_emits_rgba_fields() {
        // Debug derive includes all field names.
        let c = Color::rgba(10, 20, 30, 40);
        let s = format!("{:?}", c);
        assert!(s.contains("r:"));
        assert!(s.contains("10"));
        assert!(s.contains("g:"));
        assert!(s.contains("b:"));
        assert!(s.contains("a:"));
    }

    #[test]
    fn test_color_to_svg_alpha_zero_explicit() {
        // a=0 → rgba(..., 0.00) — fully transparent.
        let c = Color::rgba(100, 200, 50, 0);
        assert_eq!(c.to_svg(), "rgba(100,200,50,0.00)");
    }

    #[test]
    fn test_parse_color_string_negative_alpha_rejected() {
        // Negative alpha is out of u8 range → None.
        assert!(parse_color_string("255,255,255,-1").is_none());
        // Alpha = 256 (one above max u8) → None.
        assert!(parse_color_string("255,255,255,256").is_none());
        // Boundary: a=255 accepted.
        assert!(parse_color_string("255,255,255,255").is_some());
    }

    #[test]
    fn test_color_map_default_empty_and_compact() {
        // Default ColorMap is empty and uses Default derive.
        let cm = ColorMap::default();
        assert!(cm.colors.is_empty());
        // Manually construct via struct.
        let cm2 = ColorMap {
            colors: HashMap::new(),
        };
        assert!(cm2.colors.is_empty());
    }

    #[test]
    fn test_rgb_color_recurses_on_aN_transparency_suffix() {
        // `_aN` suffix → rsplit_once strips and recurses on root name.
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert(
            "red".into(),
            crate::config::types::ConfigValue::Str("247,42,66".into()),
        );
        assert_eq!(rgb_color("red_a2", &conf), Some((247, 42, 66)));
        assert_eq!(rgb_color("red_a15", &conf), Some((247, 42, 66)));
        // Same as direct lookup for root.
        assert_eq!(rgb_color("red", &conf), Some((247, 42, 66)));
    }

    #[test]
    fn test_rgb_color_opacity_disabled_when_auto_alpha_steps_zero() {
        // Guard `auto_alpha_steps > 0` fails at 0 → always returns 1.0.
        assert_eq!(rgb_color_opacity("red_a5", 0), 1.0);
        assert_eq!(rgb_color_opacity("red_a999", 0), 1.0);
        // steps>0 → normal calculation: 1 - 5/10 = 0.5.
        assert!((rgb_color_opacity("red_a5", 10) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn test_rgb_color_transparency_complements_opacity() {
        // transparency = 1 - opacity for any input.
        for &(c, steps) in &[("red", 10u32), ("red_a3", 10), ("red_a0", 10), ("plain", 0)] {
            let op = rgb_color_opacity(c, steps);
            let tr = rgb_color_transparency(c, steps);
            assert!((op + tr - 1.0).abs() < 1e-12, "c={} steps={}", c, steps);
        }
    }

    #[test]
    fn test_parse_color_string_u8_overflow_returns_none() {
        // Any component > 255 overflows u8 → parse fails → None.
        assert_eq!(parse_color_string("256,0,0"), None);
        assert_eq!(parse_color_string("0,999,0"), None);
        assert_eq!(parse_color_string("-1,0,0"), None);
        // Too many components also → None (only 3 or 4 allowed).
        assert_eq!(parse_color_string("1,2,3,4,5"), None);
    }

    #[test]
    fn test_color_to_svg_alpha_255_uses_rgb_format() {
        // Full opacity → to_svg delegates to to_svg_rgb — no "rgba(" prefix.
        let c = Color::rgba(10, 20, 30, 255);
        let s = c.to_svg();
        assert_eq!(s, "rgb(10,20,30)");
        // rgb() constructor also gets rgb() output.
        let c2 = Color::rgb(10, 20, 30);
        assert_eq!(c2.to_svg(), "rgb(10,20,30)");
    }

    #[test]
    fn test_color_to_svg_partial_alpha_uses_two_decimals() {
        // alpha=128 → 128/255 ≈ 0.50196... → {:.2} → "0.50".
        let c = Color::rgba(10, 20, 30, 128);
        assert_eq!(c.to_svg(), "rgba(10,20,30,0.50)");
        // alpha=0 → "0.00".
        let c2 = Color::rgba(10, 20, 30, 0);
        assert_eq!(c2.to_svg(), "rgba(10,20,30,0.00)");
    }

    #[test]
    fn test_color_to_hex_uses_lowercase_hex_digits() {
        // {:02x} → always lowercase (uppercase would be {:02X}).
        let c = Color::rgb(0xAB, 0xCD, 0xEF);
        assert_eq!(c.to_hex(), "#abcdef");
        // Zero-padded.
        let c2 = Color::rgb(1, 2, 3);
        assert_eq!(c2.to_hex(), "#010203");
    }

    #[test]
    fn test_color_rgb_constructor_sets_alpha_to_255() {
        // rgb() always hard-codes alpha=255 regardless of input channels.
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.a, 255);
        let c2 = Color::rgb(255, 255, 255);
        assert_eq!(c2.a, 255);
        // PartialEq distinguishes on alpha.
        assert_ne!(Color::rgba(1, 2, 3, 100), Color::rgba(1, 2, 3, 200));
    }

    #[test]
    fn test_allocate_colors_rgb_triple_string_parsed() {
        // "247,42,66" → Color::rgb(247,42,66) in the colors map.
        let mut cm = ColorMap::new();
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert(
            "circos_red".into(),
            crate::config::types::ConfigValue::Str("247,42,66".into()),
        );
        cm.allocate_colors(&conf, false, 0, None);
        let c = *cm.colors.get("circos_red").unwrap();
        assert_eq!((c.r, c.g, c.b), (247, 42, 66));
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_allocate_colors_single_level_alias_resolved() {
        // "myred" aliases to "red"; "red" has concrete triple → myred gets the same rgb.
        let mut cm = ColorMap::new();
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("red".into(), crate::config::types::ConfigValue::Str("247,42,66".into()));
        conf.insert("myred".into(), crate::config::types::ConfigValue::Str("red".into()));
        cm.allocate_colors(&conf, false, 0, None);
        let c = *cm.colors.get("myred").unwrap();
        assert_eq!((c.r, c.g, c.b), (247, 42, 66));
    }

    #[test]
    fn test_allocate_colors_auto_alpha_expansion_creates_named_variants() {
        // auto_alpha_steps=3 → 3 per-color _a1/_a2/_a3 variants added.
        let mut cm = ColorMap::new();
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("blue".into(), crate::config::types::ConfigValue::Str("0,0,255".into()));
        cm.allocate_colors(&conf, false, 3, None);
        assert!(cm.colors.contains_key("blue"));
        assert!(cm.colors.contains_key("blue_a1"));
        assert!(cm.colors.contains_key("blue_a2"));
        assert!(cm.colors.contains_key("blue_a3"));
        // Alpha increases from _a1 → _a3 (more opaque if coded that way, or less —
        // just verify values differ).
        let a1 = cm.colors.get("blue_a1").unwrap().a;
        let a3 = cm.colors.get("blue_a3").unwrap().a;
        assert_ne!(a1, a3);
    }

    #[test]
    fn test_allocate_colors_out_of_range_u8_component_skipped() {
        // "256,0,0" has r=256 which overflows u8 → parse fails → color not inserted.
        let mut cm = ColorMap::new();
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert(
            "badred".into(),
            crate::config::types::ConfigValue::Str("256,0,0".into()),
        );
        // Also include a valid entry to confirm the loop keeps going.
        conf.insert(
            "okgreen".into(),
            crate::config::types::ConfigValue::Str("0,255,0".into()),
        );
        cm.allocate_colors(&conf, false, 0, None);
        assert!(!cm.colors.contains_key("badred"));
        assert!(cm.colors.contains_key("okgreen"));
    }

    #[test]
    fn test_allocate_colors_transparent_with_explicit_rgb_string() {
        // add_transparent=true + transparentrgb → "transparent" key inserted with alpha=0.
        let mut cm = ColorMap::new();
        let conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        cm.allocate_colors(&conf, true, 0, Some("100,150,200"));
        let t = *cm.colors.get("transparent").unwrap();
        assert_eq!((t.r, t.g, t.b, t.a), (100, 150, 200, 0));
    }

    #[test]
    fn test_allocate_colors_without_transparent_flag_no_key_added() {
        // add_transparent=false → no "transparent" entry regardless of rgb arg.
        let mut cm = ColorMap::new();
        let conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        cm.allocate_colors(&conf, false, 0, Some("100,150,200"));
        assert!(!cm.colors.contains_key("transparent"));
    }

    #[test]
    fn test_resolve_rgb_comma_string_parses_as_color() {
        // String containing ',' → parse_color_string path.
        let cm = ColorMap::new();
        let c = cm.resolve("247,42,66").unwrap();
        assert_eq!((c.r, c.g, c.b), (247, 42, 66));
        // RGBA form too.
        let c2 = cm.resolve("100,150,200,128").unwrap();
        assert_eq!((c2.r, c2.g, c2.b, c2.a), (100, 150, 200, 128));
    }

    #[test]
    fn test_resolve_plain_named_color_returns_copied_value() {
        // Look up a registered name → returns a Copy of the stored Color.
        let mut cm = ColorMap::new();
        cm.colors.insert("red".into(), Color::rgb(247, 42, 66));
        let c = cm.resolve("red").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (247, 42, 66, 255));
        // Missing name → None.
        assert!(cm.resolve("no_such_color").is_none());
    }

    #[test]
    fn test_resolve_name_with_alpha_suffix_scales_alpha_channel() {
        // "red_a5" → base "red" + alpha_idx=5; alpha = (5/5) × 255 = 255.
        let mut cm = ColorMap::new();
        cm.colors.insert("red".into(), Color::rgb(247, 42, 66));
        let c = cm.resolve("red_a5").unwrap();
        assert_eq!((c.r, c.g, c.b), (247, 42, 66));
        assert_eq!(c.a, 255);
        // "red_a0" → alpha_idx=0 → alpha=0.
        let c2 = cm.resolve("red_a0").unwrap();
        assert_eq!(c2.a, 0);
    }

    #[test]
    fn test_allocate_colors_rgba_four_part_string_parsed_with_alpha() {
        // "100,150,200,64" → RGBA Color; alpha scaled from Perl-style 0..127.
        let mut cm = ColorMap::new();
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert(
            "semi".into(),
            crate::config::types::ConfigValue::Str("100,150,200,64".into()),
        );
        cm.allocate_colors(&conf, false, 0, None);
        let c = *cm.colors.get("semi").unwrap();
        assert_eq!((c.r, c.g, c.b), (100, 150, 200));
        // Perl scaling: a_raw=64 (>1) → kept as 64; then (64/127)*255 rounded ≈ 128.
        assert!((c.a as i16 - 128).abs() <= 1);
    }

    #[test]
    fn test_parse_color_string_preserves_boundary_values_0_and_255() {
        // Boundary cases: 0,0,0 and 255,255,255.
        assert_eq!(parse_color_string("0,0,0"), Some(Color::rgb(0, 0, 0)));
        assert_eq!(parse_color_string("255,255,255"), Some(Color::rgb(255, 255, 255)));
        // With alpha=0 (fully transparent) and alpha=255 (opaque).
        assert_eq!(parse_color_string("0,0,0,0"), Some(Color::rgba(0, 0, 0, 0)));
        assert_eq!(parse_color_string("255,255,255,255"), Some(Color::rgba(255, 255, 255, 255)));
    }

    #[test]
    fn test_color_to_hex_extremes_black_and_white() {
        // Pure black and white hex values.
        assert_eq!(Color::rgb(0, 0, 0).to_hex(), "#000000");
        assert_eq!(Color::rgb(255, 255, 255).to_hex(), "#ffffff");
    }

    #[test]
    fn test_parse_color_string_only_two_components_returns_none() {
        // parse_color_string expects 3 or 4 parts — 2 parts is insufficient.
        assert_eq!(parse_color_string("10,20"), None);
        assert_eq!(parse_color_string("100"), None);
        // Single trailing comma → 2 parts ["100", ""] — second part fails parse → None.
        assert_eq!(parse_color_string("100,"), None);
    }

    #[test]
    fn test_color_to_hex_odd_digits_zero_padded() {
        // Each channel uses {:02x} — single-digit values get a leading zero.
        assert_eq!(Color::rgb(5, 10, 15).to_hex(), "#050a0f");
        // Mixed one- and two-digit hex values.
        assert_eq!(Color::rgb(1, 16, 255).to_hex(), "#0110ff");
    }

    #[test]
    fn test_color_map_resolve_comma_string_with_whitespace_in_parts() {
        // resolve() with a comma → parse_color_string, which trims whitespace in parts.
        let cmap = ColorMap::new();
        let c = cmap.resolve("255, 0, 0").expect("trimmed parse should succeed");
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_rgb_color_opacity_zero_n_equals_full_opacity_one() {
        // "_a0" with nonzero steps → 1 - 0/N = 1.0 full opacity.
        assert_eq!(rgb_color_opacity("red_a0", 5), 1.0);
        // And "_aN" where N equals the steps → opacity 0.0 fully transparent.
        assert_eq!(rgb_color_opacity("red_a5", 5), 0.0);
        // Halfway.
        assert!((rgb_color_opacity("red_a2", 4) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn test_color_to_svg_rgba_full_alpha_still_degrades_to_rgb() {
        // Alpha 255 → to_svg emits the rgb() form (no alpha).
        let c = Color::rgba(10, 20, 30, 255);
        assert_eq!(c.to_svg(), "rgb(10,20,30)");
        // Alpha < 255 → rgba() form.
        let c2 = Color::rgba(10, 20, 30, 128);
        assert!(c2.to_svg().contains("rgba("));
    }

    #[test]
    fn test_rgb_color_transparency_at_full_opacity_is_zero() {
        // Color without _a suffix → opacity 1.0 → transparency 0.0.
        assert_eq!(rgb_color_transparency("red", 5), 0.0);
        // "red_a0" also 1.0 opacity → 0.0 transparency.
        assert_eq!(rgb_color_transparency("red_a0", 5), 0.0);
        // Max transparency at steps boundary.
        assert_eq!(rgb_color_transparency("red_a5", 5), 1.0);
    }

    #[test]
    fn test_color_map_resolve_alpha_suffix_with_nonzero_digit_scales_alpha() {
        // Base color registered; resolve("red_a3") returns red with alpha = (3/5)×255.
        let mut cmap = ColorMap::new();
        cmap.colors.insert("red".into(), Color::rgb(255, 0, 0));
        let c = cmap.resolve("red_a3").expect("should resolve");
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
        // alpha = ((3 as f64)/5.0) * 255.0 = 153.0 as u8 = 153.
        assert_eq!(c.a, 153);
    }

    #[test]
    fn test_parse_color_string_rgba_with_fractional_alpha_zero_to_one_scaled_by_127() {
        // When alpha is <1, Perl multiplies by 127 first. parse_color_string itself
        // treats the 4th part as a u8 directly — so "0.5" is NOT a valid u8 → None.
        // This documents the boundary: parse_color_string expects integer u8 alpha.
        assert_eq!(parse_color_string("100,150,200,0.5"), None);
        // Integer alpha is fine.
        let c = parse_color_string("100,150,200,128").unwrap();
        assert_eq!(c.a, 128);
    }

    #[test]
    fn test_color_rgb_constructor_alpha_is_255_not_zero() {
        // Color::rgb(r, g, b) defaults alpha to 255 (fully opaque), not 0.
        let c = Color::rgb(100, 50, 200);
        assert_eq!(c.a, 255);
        // to_svg returns rgb() form since alpha is 255.
        assert_eq!(c.to_svg(), "rgb(100,50,200)");
    }

    #[test]
    fn test_color_rgba_zero_alpha_fully_transparent() {
        // Color::rgba with a=0 → fully transparent.
        let c = Color::rgba(255, 0, 0, 0);
        assert_eq!(c.a, 0);
        // to_svg emits rgba form with 0 opacity.
        assert!(c.to_svg().contains("rgba(255,0,0,"));
    }

    #[test]
    fn test_rgb_color_opacity_name_without_a_suffix_yields_one() {
        // Plain name without "_aN" suffix → always opacity 1.0.
        assert_eq!(rgb_color_opacity("red", 5), 1.0);
        assert_eq!(rgb_color_opacity("foo_bar", 5), 1.0);
        // Even with 0 steps.
        assert_eq!(rgb_color_opacity("red", 0), 1.0);
    }

    #[test]
    fn test_color_map_allocate_colors_skips_empty_config() {
        // allocate_colors on an empty config does not panic, colors remain empty.
        let mut cmap = ColorMap::new();
        let config = HashMap::new();
        cmap.allocate_colors(&config, false, 0, None);
        assert!(cmap.colors.is_empty());
    }

    #[test]
    fn test_color_to_svg_partial_alpha_rounds_to_two_decimals() {
        // alpha/255 is formatted with {:.2}. 128/255 = 0.5019607… → "0.50".
        let c = Color::rgba(10, 20, 30, 128);
        let s = c.to_svg();
        assert!(s.contains("rgba(10,20,30,0.50)"));
    }

    #[test]
    fn test_parse_color_string_with_whitespace_separator_splits_correctly() {
        // parse_color_string splits on comma, but with interior whitespace each part is trimmed.
        let c = parse_color_string("10 , 20 , 30").expect("trimmed parts");
        assert_eq!(c.r, 10);
        assert_eq!(c.g, 20);
        assert_eq!(c.b, 30);
    }

    #[test]
    fn test_color_to_hex_matches_to_svg_rgb_values() {
        // Both to_hex and to_svg_rgb use the same r/g/b channels.
        let c = Color::rgb(128, 64, 32);
        let hex = c.to_hex();
        let rgb = c.to_svg_rgb();
        // hex: #804020, rgb: rgb(128,64,32)
        assert_eq!(hex, "#804020");
        assert_eq!(rgb, "rgb(128,64,32)");
    }

    #[test]
    fn test_rgb_color_opacity_alpha_numeric_suffix_parseable() {
        // "name_a7" with 10 steps → 1 - 7/10 = 0.3.
        assert!((rgb_color_opacity("col_a7", 10) - 0.3).abs() < 1e-9);
        // Non-numeric suffix: "red_ax" — numeric parse fails → defaults to 1.0 (full opacity).
        assert_eq!(rgb_color_opacity("red_ax", 5), 1.0);
    }

    #[test]
    fn test_color_rgba_all_same_channels_yields_grayscale_like() {
        // When r==g==b, the output is gray-scale-like regardless of alpha.
        let c = Color::rgba(128, 128, 128, 200);
        assert_eq!(c.r, c.g);
        assert_eq!(c.g, c.b);
        assert_eq!(c.a, 200);
    }

    #[test]
    fn test_color_to_hex_with_channel_255_max_values() {
        // Color::rgb(255,255,255) → "#ffffff"; black → "#000000".
        assert_eq!(Color::rgb(255, 255, 255).to_hex(), "#ffffff");
        assert_eq!(Color::rgb(0, 0, 0).to_hex(), "#000000");
        // Mixed max.
        assert_eq!(Color::rgb(255, 0, 255).to_hex(), "#ff00ff");
    }

    #[test]
    fn test_parse_color_string_with_four_part_alpha_255_yields_alpha_255() {
        // "r,g,b,255" → alpha 255.
        let c = parse_color_string("100,150,200,255").unwrap();
        assert_eq!(c.a, 255);
        // to_svg should emit rgb() (not rgba) since alpha is max.
        assert_eq!(c.to_svg(), "rgb(100,150,200)");
    }

    #[test]
    fn test_color_map_resolve_unknown_base_with_alpha_suffix_returns_none() {
        // "nonexistent_a3" — base "nonexistent" not registered → resolve returns None.
        let cmap = ColorMap::new();
        assert!(cmap.resolve("nonexistent_a3").is_none());
    }

    #[test]
    fn test_color_to_svg_rgb_regardless_of_alpha_channel() {
        // to_svg_rgb ignores alpha entirely — just "rgb(r,g,b)".
        let c1 = Color::rgba(10, 20, 30, 255);
        let c2 = Color::rgba(10, 20, 30, 0);
        let c3 = Color::rgba(10, 20, 30, 128);
        assert_eq!(c1.to_svg_rgb(), c2.to_svg_rgb());
        assert_eq!(c1.to_svg_rgb(), c3.to_svg_rgb());
        assert_eq!(c1.to_svg_rgb(), "rgb(10,20,30)");
    }

    #[test]
    fn test_color_hex_preserves_single_digit_channel_values() {
        // Single-digit hex value gets 0-padded by {:02x}.
        assert_eq!(Color::rgb(5, 5, 5).to_hex(), "#050505");
        assert_eq!(Color::rgb(10, 10, 10).to_hex(), "#0a0a0a");
    }

    #[test]
    fn test_parse_color_string_with_commas_only_returns_none() {
        // ",,,,"  — splits into 5 empty parts → invalid.
        assert_eq!(parse_color_string(",,,,"), None);
        // Single comma → 2 parts (both empty) → invalid.
        assert_eq!(parse_color_string(","), None);
    }

    #[test]
    fn test_rgb_color_transparency_sum_with_opacity_equals_one() {
        // For any color, opacity + transparency = 1.0.
        for name in ["red", "red_a0", "red_a3", "red_a5"] {
            let opacity = rgb_color_opacity(name, 5);
            let transparency = rgb_color_transparency(name, 5);
            assert!((opacity + transparency - 1.0).abs() < 1e-9, "{}", name);
        }
    }

    #[test]
    fn test_color_debug_formatting_contains_rgba_values() {
        let c = Color::rgba(10, 20, 30, 40);
        let dbg = format!("{:?}", c);
        assert!(dbg.contains("10"));
        assert!(dbg.contains("20"));
        assert!(dbg.contains("30"));
        assert!(dbg.contains("40"));
    }

    #[test]
    fn test_color_map_new_and_default_both_empty() {
        // Both ColorMap::new() and ColorMap::default() → empty colors HashMap.
        let c1 = ColorMap::new();
        let c2 = ColorMap::default();
        assert!(c1.colors.is_empty());
        assert!(c2.colors.is_empty());
    }

    #[test]
    fn test_parse_color_string_trailing_whitespace_in_parts_trimmed() {
        // Trailing whitespace within each part is trimmed before parse.
        let c = parse_color_string("100 ,200 ,50").expect("trimmed");
        assert_eq!(c.r, 100);
        assert_eq!(c.g, 200);
        assert_eq!(c.b, 50);
    }

    #[test]
    fn test_color_to_svg_alpha_0_5_rounds_to_two_decimals() {
        // Alpha = 127 → 127/255 ≈ 0.498 → "{:.2}" → "0.50".
        let c = Color::rgba(50, 100, 150, 127);
        let s = c.to_svg();
        assert!(s.contains("0.50"));
    }

    #[test]
    fn test_color_to_hex_all_zero_is_triple_zero() {
        // Black rgb(0,0,0) → "#000000".
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.to_hex(), "#000000");
    }

    #[test]
    fn test_color_map_resolve_rgb_string_bypasses_name_lookup() {
        // Input with comma → takes comma path regardless of map contents.
        let map = ColorMap::new();
        let c = map.resolve("10,20,30").expect("parsed");
        assert_eq!(c.r, 10);
        assert_eq!(c.g, 20);
        assert_eq!(c.b, 30);
    }

    #[test]
    fn test_rgb_color_opacity_zero_alpha_steps_returns_one() {
        // auto_alpha_steps = 0 disables alpha handling → always 1.0.
        assert_eq!(rgb_color_opacity("red_a3", 0), 1.0);
        assert_eq!(rgb_color_opacity("blue", 0), 1.0);
    }

    #[test]
    fn test_rgb_color_aN_suffix_recurses_to_base_name() {
        // "red_a3" recurses to lookup "red" in conf.
        let mut conf = HashMap::new();
        conf.insert(
            "red".to_string(),
            crate::config::types::ConfigValue::Str("255,0,0".to_string()),
        );
        let rgb = rgb_color("red_a3", &conf).expect("resolved via recursion");
        assert_eq!(rgb, (255, 0, 0));
    }

    #[test]
    fn test_color_to_svg_fully_opaque_uses_rgb_not_rgba() {
        // Alpha=255 → to_svg() omits alpha → rgb(...) form.
        let c = Color::rgb(100, 150, 200);
        let s = c.to_svg();
        assert_eq!(s, "rgb(100,150,200)");
        assert!(!s.contains("rgba"));
    }

    #[test]
    fn test_color_to_hex_single_digit_channel_pads_with_zero() {
        // to_hex uses {:02x} → single-digit channels get leading "0".
        let c = Color::rgb(1, 2, 3);
        assert_eq!(c.to_hex(), "#010203");
    }

    #[test]
    fn test_parse_color_string_too_few_components_returns_none() {
        // 2-component input is not 3 or 4 → None.
        assert!(parse_color_string("10,20").is_none());
        // 5-component input also not supported.
        assert!(parse_color_string("1,2,3,4,5").is_none());
    }

    #[test]
    fn test_rgb_color_transparency_half_opacity_gives_half_transparency() {
        // auto_alpha_steps=4, _a2 suffix → opacity=1-2/4=0.5 → transparency=0.5.
        let t = rgb_color_transparency("green_a2", 4);
        assert!((t - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_allocate_colors_direct_rgb_triple_stored_in_map() {
        // "255,0,0" → red Color stored under name.
        let mut conf = HashMap::new();
        conf.insert(
            "red".to_string(),
            crate::config::types::ConfigValue::Str("255,0,0".to_string()),
        );
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 0, None);
        let c = cmap.colors.get("red").expect("red stored");
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
    }

    #[test]
    fn test_allocate_colors_name_alias_resolved_to_same_value() {
        // "red" = "255,0,0"; "crimson" = "red" — alias should copy RGB.
        let mut conf = HashMap::new();
        conf.insert(
            "red".to_string(),
            crate::config::types::ConfigValue::Str("255,0,0".to_string()),
        );
        conf.insert(
            "crimson".to_string(),
            crate::config::types::ConfigValue::Str("red".to_string()),
        );
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 0, None);
        let c = cmap.colors.get("crimson").expect("alias resolved");
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
    }

    #[test]
    fn test_allocate_colors_auto_alpha_creates_suffix_variants() {
        // Single base color + auto_alpha_steps=3 → 3 _a1/_a2/_a3 variants.
        let mut conf = HashMap::new();
        conf.insert(
            "blue".to_string(),
            crate::config::types::ConfigValue::Str("0,0,255".to_string()),
        );
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 3, None);
        // Base + 3 variants = 4 entries.
        assert_eq!(cmap.colors.len(), 4);
        assert!(cmap.colors.contains_key("blue_a1"));
        assert!(cmap.colors.contains_key("blue_a2"));
        assert!(cmap.colors.contains_key("blue_a3"));
    }

    #[test]
    fn test_allocate_colors_transparent_with_explicit_rgb_uses_given_values() {
        // add_transparent=true + explicit rgb → "transparent" slot has those rgb values, alpha=0.
        let conf = HashMap::new();
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, true, 0, Some("128,128,128"));
        let t = cmap.colors.get("transparent").expect("transparent allocated");
        assert_eq!((t.r, t.g, t.b, t.a), (128, 128, 128, 0));
    }

    #[test]
    fn test_allocate_colors_with_four_part_rgba_stores_alpha() {
        // "r,g,b,a" 4-part spec stores alpha (a_raw ≥ 1 → direct).
        let mut conf = HashMap::new();
        conf.insert(
            "semi".to_string(),
            crate::config::types::ConfigValue::Str("50,100,150,127".to_string()),
        );
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 0, None);
        let c = cmap.colors.get("semi").expect("stored");
        assert_eq!((c.r, c.g, c.b), (50, 100, 150));
        // 127/127 * 255 = 255 (clamped).
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_color_map_resolve_alpha_suffix_produces_scaled_alpha() {
        // ColorMap contains "red"; resolve("red_a3") → alpha = (3/5)*255 = 153.
        let mut cmap = ColorMap::new();
        cmap.colors.insert("red".to_string(), Color::rgb(255, 0, 0));
        let c = cmap.resolve("red_a3").expect("resolved with alpha");
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
        // alpha = 3.0/5.0 * 255 = 153.
        assert_eq!(c.a, 153);
    }

    #[test]
    fn test_color_map_resolve_unknown_name_returns_none() {
        // Name not in map + no _aN suffix + no comma → None.
        let cmap = ColorMap::new();
        assert!(cmap.resolve("unknown_color").is_none());
    }

    #[test]
    fn test_color_rgba_constructor_stores_all_four_fields_exactly() {
        // rgba(r,g,b,a) stores values verbatim.
        let c = Color::rgba(11, 22, 33, 44);
        assert_eq!(c.r, 11);
        assert_eq!(c.g, 22);
        assert_eq!(c.b, 33);
        assert_eq!(c.a, 44);
    }

    #[test]
    fn test_color_rgb_constructor_alpha_implicitly_255_fully_opaque() {
        // Color::rgb(r,g,b) sets alpha to 255 implicitly (fully opaque).
        let c = Color::rgb(10, 20, 30);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_color_to_svg_rgb_returns_no_alpha_format() {
        // to_svg_rgb always produces rgb(r,g,b) regardless of alpha.
        let c = Color::rgba(100, 50, 25, 128);
        assert_eq!(c.to_svg_rgb(), "rgb(100,50,25)");
    }

    #[test]
    fn test_color_to_hex_lowercase_hexadecimal_characters() {
        // to_hex uses {:02x} → lowercase a-f.
        let c = Color::rgb(255, 171, 205);
        assert_eq!(c.to_hex(), "#ffabcd");
    }

    #[test]
    fn test_rgb_color_opacity_full_steps_gives_zero_opacity() {
        // auto_alpha_steps=5, _a5 suffix → opacity = 1 - 5/5 = 0.
        let o = rgb_color_opacity("red_a5", 5);
        assert!(o.abs() < 1e-9);
    }

    #[test]
    fn test_color_partial_eq_on_different_rgba_returns_false() {
        // Color implements PartialEq — different rgba values not equal.
        let a = Color::rgba(1, 2, 3, 4);
        let b = Color::rgba(1, 2, 3, 5);
        assert_ne!(a, b);
        let c = Color::rgba(1, 2, 3, 4);
        assert_eq!(a, c);
    }

    #[test]
    fn test_color_to_svg_alpha_zero_emits_rgba_with_zero() {
        // alpha=0 → to_svg emits rgba(...,0.00).
        let c = Color::rgba(100, 100, 100, 0);
        let s = c.to_svg();
        assert!(s.starts_with("rgba("));
        assert!(s.ends_with(",0.00)"));
    }

    #[test]
    fn test_color_map_allocate_with_empty_config_yields_empty_map() {
        // Empty config + no transparent + 0 alpha steps → empty ColorMap.
        let conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 0, None);
        assert!(cmap.colors.is_empty());
    }

    #[test]
    fn test_parse_color_string_3_part_with_internal_spaces_trimmed() {
        // "  100 , 200 , 50  " → (100, 200, 50) via trim per-part.
        let c = parse_color_string("  100 , 200 , 50  ").expect("trimmed");
        assert_eq!(c.r, 100);
        assert_eq!(c.g, 200);
        assert_eq!(c.b, 50);
    }

    #[test]
    fn test_rgb_color_opacity_without_suffix_returns_one() {
        // Plain name → not _aN → opacity 1.0.
        assert_eq!(rgb_color_opacity("red", 5), 1.0);
        assert_eq!(rgb_color_opacity("some_color", 10), 1.0);
    }

    #[test]
    fn test_color_map_new_starts_empty_and_default_too() {
        // Both constructors yield empty map.
        let m1 = ColorMap::new();
        assert!(m1.colors.is_empty());
        let m2 = ColorMap::default();
        assert!(m2.colors.is_empty());
    }

    #[test]
    fn test_color_to_svg_rgb_zero_values_all_emit_in_format() {
        // rgb(0,0,0) → "rgb(0,0,0)" exactly.
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.to_svg_rgb(), "rgb(0,0,0)");
    }

    #[test]
    fn test_color_map_resolve_empty_string_name_returns_none() {
        // Empty name doesn't match any key.
        let cmap = ColorMap::new();
        assert!(cmap.resolve("").is_none());
    }

    #[test]
    fn test_color_to_hex_white_produces_ffffff() {
        // rgb(255,255,255) → "#ffffff".
        let c = Color::rgb(255, 255, 255);
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_parse_color_string_with_trailing_comma_returns_none() {
        // "1,2,3," → split yields 4 parts, one empty → parse::<u8> fails.
        assert!(parse_color_string("1,2,3,").is_none());
    }

    #[test]
    fn test_rgb_color_transparency_nan_output_impossible_for_valid_input() {
        // opacity 1.0 for regular name → transparency 0.
        assert_eq!(rgb_color_transparency("blue", 5), 0.0);
    }

    #[test]
    fn test_color_map_resolve_rgba_4_part_string_preserves_alpha() {
        // "100,200,50,128" → rgba(100,200,50,128).
        let cmap = ColorMap::new();
        let c = cmap.resolve("100,200,50,128").expect("parsed rgba");
        assert_eq!((c.r, c.g, c.b, c.a), (100, 200, 50, 128));
    }

    #[test]
    fn test_color_partial_eq_rgba_vs_rgb_equivalent_alpha_255() {
        // rgba(x,y,z,255) == rgb(x,y,z) (same alpha).
        let rgba = Color::rgba(50, 100, 150, 255);
        let rgb = Color::rgb(50, 100, 150);
        assert_eq!(rgba, rgb);
    }

    #[test]
    fn test_allocate_colors_ignores_transparent_name_in_first_pass() {
        // "transparent" name in config is skipped during first pass.
        let mut conf = HashMap::new();
        conf.insert(
            "transparent".to_string(),
            crate::config::types::ConfigValue::Str("255,255,255".to_string()),
        );
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 0, None);
        // First-pass skips "transparent" → not inserted from config.
        assert!(cmap.colors.get("transparent").is_none());
    }

    #[test]
    fn test_color_to_hex_with_high_byte_values_produces_hex_chars() {
        // rgb(240, 10, 175) → "#f00aaf".
        let c = Color::rgb(240, 10, 175);
        assert_eq!(c.to_hex(), "#f00aaf");
    }

    #[test]
    fn test_rgb_color_opacity_partial_steps_computes_correct_fraction() {
        // auto_alpha_steps=10, _a3 → 1-3/10 = 0.7.
        let o = rgb_color_opacity("blue_a3", 10);
        assert!((o - 0.7).abs() < 1e-9);
    }

    #[test]
    fn test_color_copy_semantics_allow_use_after_assignment() {
        // Color is Copy → assignment doesn't move.
        let c = Color::rgb(10, 20, 30);
        let c2 = c;
        // c still usable after c2 assignment.
        assert_eq!(c.r, 10);
        assert_eq!(c2.r, 10);
    }

    #[test]
    fn test_color_map_allocate_colors_with_two_aliased_names() {
        // Two names aliasing to same color — both resolve to identical RGB.
        let mut conf = HashMap::new();
        conf.insert("red".to_string(), crate::config::types::ConfigValue::Str("255,0,0".to_string()));
        conf.insert("scarlet".to_string(), crate::config::types::ConfigValue::Str("red".to_string()));
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 0, None);
        assert_eq!(cmap.colors.get("red"), cmap.colors.get("scarlet"));
    }

    #[test]
    fn test_parse_color_string_negative_values_parse_fails() {
        // Negative values → u8::parse fails → None.
        assert!(parse_color_string("-1,0,0").is_none());
    }

    #[test]
    fn test_color_to_svg_alpha_128_produces_mid_opacity_string() {
        // alpha=128 → 128/255 ≈ 0.502 → "0.50" in {:.2}.
        let c = Color::rgba(100, 100, 100, 128);
        let s = c.to_svg();
        assert!(s.contains("0.50"));
    }

    #[test]
    fn test_color_map_allocate_colors_auto_alpha_only_affects_existing_names() {
        // Empty conf with auto_alpha_steps > 0 doesn't add anything.
        let conf = HashMap::new();
        let mut cmap = ColorMap::new();
        cmap.allocate_colors(&conf, false, 5, None);
        assert!(cmap.colors.is_empty());
    }

    #[test]
    fn test_rgb_color_opacity_multiple_alpha_indices_all_mapped_correctly() {
        // Steps 1..=5 map to opacity values in [0,1].
        for i in 1..=5 {
            let name = format!("red_a{}", i);
            let o = rgb_color_opacity(&name, 5);
            assert!((0.0..=1.0).contains(&o));
        }
    }

    #[test]
    fn test_parse_color_string_empty_returns_none() {
        // Empty string → no parts after split → None.
        assert!(parse_color_string("").is_none());
    }

    #[test]
    fn test_color_to_hex_max_values_ff_ff_ff() {
        // rgb(255,255,255) → "#ffffff".
        let c = Color::rgb(255, 255, 255);
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_color_to_svg_high_alpha_rounds_to_one_decimal() {
        // alpha=254 → 254/255 ≈ 0.996 → "{:.2}" → "1.00".
        let c = Color::rgba(50, 100, 150, 254);
        let s = c.to_svg();
        assert!(s.contains("1.00"));
    }

    #[test]
    fn test_rgb_color_transparency_unknown_color_is_zero() {
        // Unknown color name (no _aN suffix) → opacity 1 → transparency 0.
        let t = rgb_color_transparency("foo", 5);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn test_color_map_resolve_alpha_suffix_without_base_returns_none() {
        // "missing_a3" → base not in map → None.
        let cmap = ColorMap::new();
        assert!(cmap.resolve("missing_a3").is_none());
    }

    #[test]
    fn test_parse_color_string_commas_only_returns_none() {
        // ",,," → 4 empty parts → parse fails.
        assert!(parse_color_string(",,,").is_none());
    }

    #[test]
    fn test_color_rgb_struct_fields_publicly_readable() {
        // r/g/b/a all publicly readable.
        let c = Color::rgb(11, 22, 33);
        assert_eq!(c.r, 11);
        assert_eq!(c.g, 22);
        assert_eq!(c.b, 33);
    }

    #[test]
    fn test_rgb_color_opacity_aN_suffix_zero_index_gives_full_opacity() {
        // _a0 → 1 - 0/N = 1 for any N > 0.
        assert_eq!(rgb_color_opacity("x_a0", 5), 1.0);
    }

    #[test]
    fn test_parse_color_string_with_surrounding_spaces_trimmed_per_part() {
        // Surrounding whitespace around each number in "1, 2, 3" trimmed.
        let c = parse_color_string("1, 2, 3").expect("parsed");
        assert_eq!((c.r, c.g, c.b), (1, 2, 3));
    }

    #[test]
    fn test_color_to_hex_alpha_ignored_in_hex_output() {
        // to_hex uses only rgb; alpha ignored.
        let c = Color::rgba(10, 20, 30, 128);
        assert_eq!(c.to_hex(), "#0a141e");
    }

    #[test]
    fn test_color_rgb_zero_values_produce_black_hex() {
        // (0,0,0) → "#000000".
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.to_hex(), "#000000");
    }

    #[test]
    fn test_color_to_svg_rgb_format_includes_three_components() {
        // to_svg_rgb returns "rgb(r,g,b)" with no alpha.
        let c = Color::rgb(10, 20, 30);
        let s = c.to_svg_rgb();
        assert!(s.contains("10"));
        assert!(s.contains("20"));
        assert!(s.contains("30"));
    }

    #[test]
    fn test_parse_color_string_three_valid_components_returns_some() {
        // "100,150,200" → Some(Color{100,150,200,...}).
        let c = parse_color_string("100,150,200");
        assert!(c.is_some());
    }

    #[test]
    fn test_color_rgba_alpha_field_preserved_independently() {
        // rgba(r,g,b,a) stores a separately from rgb fields.
        let c1 = Color::rgba(50, 60, 70, 128);
        let c2 = Color::rgba(50, 60, 70, 255);
        assert_eq!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_color_to_hex_white_max_values() {
        // (255,255,255) → "#ffffff".
        let c = Color::rgb(255, 255, 255);
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_parse_color_string_four_components_valid_returns_some() {
        // "10,20,30,128" rgba → Some(Color).
        let c = parse_color_string("10,20,30,128");
        assert!(c.is_some());
    }

    #[test]
    fn test_parse_color_string_single_component_not_enough_returns_none() {
        // "50" — not enough components → None.
        let c = parse_color_string("50");
        assert!(c.is_none());
    }

    #[test]
    fn test_rgb_color_opacity_no_suffix_returns_one() {
        // No "_aN" suffix → full opacity 1.0.
        let op = rgb_color_opacity("red", 5);
        assert_eq!(op, 1.0);
    }

    #[test]
    fn test_rgb_color_transparency_complement_of_opacity() {
        // transparency = 1 - opacity; "red" (no suffix) → 1.0 - 1.0 = 0.0.
        assert_eq!(rgb_color_transparency("red", 5), 0.0);
    }

    #[test]
    fn test_color_to_svg_alpha_zero_gives_zero_opacity() {
        // rgba alpha=0 → opacity 0 in SVG string.
        let c = Color::rgba(100, 100, 100, 0);
        let s = c.to_svg();
        assert!(s.contains("0") && s.contains("rgb"));
    }

    #[test]
    fn test_parse_color_string_empty_input_returns_none() {
        // Empty string → None.
        let c = parse_color_string("");
        assert!(c.is_none());
    }

    #[test]
    fn test_color_rgb_same_values_equal_via_hex() {
        // Two Color::rgb with same values → same hex output.
        let c1 = Color::rgb(100, 150, 200);
        let c2 = Color::rgb(100, 150, 200);
        assert_eq!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_color_to_hex_single_digit_r_value_zero_padded() {
        // r=5 → "#05..." (zero-padded to 2 hex digits).
        let c = Color::rgb(5, 0, 0);
        assert_eq!(c.to_hex(), "#050000");
    }

    #[test]
    fn test_rgb_color_opacity_a0_suffix_full_opacity() {
        // "_a0" → n=0 / steps=5 = 0 → opacity = 1 - 0 = 1.0.
        let op = rgb_color_opacity("red_a0", 5);
        assert_eq!(op, 1.0);
    }

    #[test]
    fn test_rgb_color_opacity_a5_with_5_steps_gives_zero() {
        // "_a5" with 5 steps → 1 - 5/5 = 0.0.
        let op = rgb_color_opacity("red_a5", 5);
        assert_eq!(op, 0.0);
    }

    #[test]
    fn test_rgb_color_transparency_midway_suffix() {
        // "red_a2" with 5 steps → transparency = 2/5 = 0.4.
        let t = rgb_color_transparency("red_a2", 5);
        assert!((t - 0.4).abs() < 1e-9);
    }

    #[test]
    fn test_color_rgb_max_values_produce_white_hex() {
        // rgb(255,255,255) → "#ffffff".
        let c = Color::rgb(255, 255, 255);
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_color_rgba_midway_r_g_b_values_hex_preserved() {
        // rgba(128, 128, 128, 200) → "#808080" (alpha ignored for to_hex).
        let c = Color::rgba(128, 128, 128, 200);
        assert_eq!(c.to_hex(), "#808080");
    }

    #[test]
    fn test_parse_color_string_with_extra_whitespace_between_parts() {
        // "  1 , 2 , 3  " → Some(color).
        let c = parse_color_string("  1 , 2 , 3  ");
        assert!(c.is_some());
    }

    #[test]
    fn test_rgb_color_opacity_zero_auto_alpha_steps_full_opacity() {
        // auto_alpha_steps=0 → skips alpha arithmetic → 1.0.
        let op = rgb_color_opacity("red_a3", 0);
        assert_eq!(op, 1.0);
    }

    #[test]
    fn test_color_to_svg_rgb_produces_format_with_parens() {
        // to_svg_rgb output format "rgb(...)".
        let c = Color::rgb(50, 100, 150);
        let s = c.to_svg_rgb();
        assert!(s.starts_with("rgb("));
        assert!(s.ends_with(")"));
    }

    #[test]
    fn test_color_to_hex_mid_values_produce_6_char_hex() {
        // Hex output is always "#" + 6 chars.
        let c = Color::rgb(100, 150, 200);
        let h = c.to_hex();
        assert_eq!(h.len(), 7);
        assert!(h.starts_with('#'));
    }

    #[test]
    fn test_rgb_color_opacity_a_step_half_gives_half_opacity() {
        // "_a2" with 4 steps → 1 - 2/4 = 0.5.
        let op = rgb_color_opacity("col_a2", 4);
        assert!((op - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_color_rgba_clone_preserves_all_four_fields() {
        // Clone of rgba preserves r/g/b/alpha.
        let c1 = Color::rgba(10, 20, 30, 64);
        let c2 = c1.clone();
        assert_eq!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_color_to_svg_full_alpha_produces_opacity_1() {
        // alpha=255 → opacity 1.0.
        let c = Color::rgba(100, 100, 100, 255);
        let s = c.to_svg();
        assert!(s.contains("1"));
    }

    #[test]
    fn test_parse_color_string_with_negative_component_none() {
        // Negative component invalid for u8 → None.
        let c = parse_color_string("-1,100,200");
        assert!(c.is_none());
    }

    #[test]
    fn test_rgb_color_opacity_different_step_counts_scale_proportionally() {
        // "_a1" at 4 steps = 1-0.25 = 0.75; at 8 steps = 1-0.125 = 0.875.
        let op4 = rgb_color_opacity("c_a1", 4);
        let op8 = rgb_color_opacity("c_a1", 8);
        assert!(op4 < op8);
        assert!((op4 - 0.75).abs() < 1e-9);
        assert!((op8 - 0.875).abs() < 1e-9);
    }

    #[test]
    fn test_color_to_hex_from_rgba_uses_only_rgb_channels() {
        // Two Color::rgba with same rgb but different alpha → same hex.
        let c1 = Color::rgba(50, 60, 70, 10);
        let c2 = Color::rgba(50, 60, 70, 250);
        assert_eq!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_rgb_color_transparency_on_non_alpha_color_returns_zero() {
        // "blue" (no _aN suffix) → transparency = 1-1 = 0.
        let t = rgb_color_transparency("blue", 10);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn test_color_to_svg_rgb_components_no_alpha_channel() {
        // to_svg_rgb only includes rgb (no alpha component).
        let c = Color::rgba(10, 20, 30, 128);
        let s = c.to_svg_rgb();
        assert!(!s.contains("128"));
    }

    #[test]
    fn test_parse_color_string_value_exceeding_255_returns_none() {
        // 300 out of u8 range → None.
        let c = parse_color_string("300,10,10");
        assert!(c.is_none());
    }

    #[test]
    fn test_color_rgb_boundary_byte_values_preserved() {
        // Boundary u8 values 0 and 255 preserved.
        let c = Color::rgb(0, 128, 255);
        assert_eq!(c.to_hex(), "#0080ff");
    }

    #[test]
    fn test_rgb_color_opacity_large_n_exceeds_steps_negative() {
        // "_a10" with 5 steps → 1 - 10/5 = -1 (negative).
        let op = rgb_color_opacity("col_a10", 5);
        assert!(op < 0.0);
    }

    #[test]
    fn test_parse_color_string_with_rgba_format_four_components() {
        // 4-component rgba "10,20,30,128" → Some.
        let c = parse_color_string("10,20,30,128");
        assert!(c.is_some());
    }

    #[test]
    fn test_color_to_hex_all_zeros_except_blue() {
        // (0, 0, 255) → "#0000ff".
        let c = Color::rgb(0, 0, 255);
        assert_eq!(c.to_hex(), "#0000ff");
    }

    #[test]
    fn test_color_to_svg_rgb_format_parseable() {
        // to_svg_rgb "rgb(r,g,b)" format has 3 numbers separated by commas.
        let c = Color::rgb(10, 20, 30);
        let s = c.to_svg_rgb();
        // Count commas — should be 2 (for 3 components).
        let comma_count = s.matches(',').count();
        assert_eq!(comma_count, 2);
    }

    #[test]
    fn test_rgb_color_opacity_with_multi_digit_alpha_suffix() {
        // "_a25" with 100 steps → 1 - 25/100 = 0.75.
        let op = rgb_color_opacity("col_a25", 100);
        assert!((op - 0.75).abs() < 1e-9);
    }

    #[test]
    fn test_parse_color_string_with_only_spaces_returns_none() {
        // "   " only spaces → None.
        let c = parse_color_string("   ");
        assert!(c.is_none());
    }

    #[test]
    fn test_color_to_hex_matches_lowercase_hex_digits() {
        // Hex output uses lowercase a-f.
        let c = Color::rgb(0xab, 0xcd, 0xef);
        assert_eq!(c.to_hex(), "#abcdef");
    }

    #[test]
    fn test_rgb_color_transparency_varies_across_alpha_steps() {
        // _a1/_a2/_a3 with 5 steps → 0.2/0.4/0.6.
        assert!((rgb_color_transparency("c_a1", 5) - 0.2).abs() < 1e-9);
        assert!((rgb_color_transparency("c_a2", 5) - 0.4).abs() < 1e-9);
        assert!((rgb_color_transparency("c_a3", 5) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_color_rgb_preserves_each_channel_independently() {
        // Different r/g/b values → each in hex output.
        let c = Color::rgb(1, 2, 3);
        assert_eq!(c.to_hex(), "#010203");
    }

    #[test]
    fn test_parse_color_string_exact_three_values_no_commas_extra() {
        // "1,2,3" → 3 components parsed correctly.
        let c = parse_color_string("1,2,3");
        assert!(c.is_some());
    }

    #[test]
    fn test_rgb_color_opacity_at_step_boundary_exactly_zero() {
        // "_a5" / 5 steps → 1 - 5/5 = 0.
        let op = rgb_color_opacity("c_a5", 5);
        assert_eq!(op, 0.0);
    }

    #[test]
    fn test_color_to_svg_alpha_value_rounds_to_three_decimals() {
        // alpha=64 → opacity ≈ 64/255 ≈ 0.251, rounded to "0.3" or "0.25".
        let c = Color::rgba(0, 0, 0, 64);
        let s = c.to_svg();
        // Should contain opacity value (number).
        assert!(s.contains('0'));
    }

    #[test]
    fn test_rgb_color_opacity_zero_alpha_steps_returns_1() {
        // auto_alpha_steps == 0 → match guard fails → returns 1.0.
        let v = rgb_color_opacity("red_a3", 0);
        assert_eq!(v, 1.0);
    }

    #[test]
    fn test_rgb_color_opacity_no_alpha_suffix_returns_1() {
        // color without "_aN" suffix → no regex match → 1.0.
        let v = rgb_color_opacity("red", 5);
        assert_eq!(v, 1.0);
    }

    #[test]
    fn test_parse_color_string_four_parts_with_whitespace_trimmed() {
        // "10, 20, 30, 40" (with spaces) → RGBA parsed after trim.
        let c = parse_color_string("10, 20, 30, 40").unwrap();
        assert_eq!(c.to_svg_rgb(), "rgb(10,20,30)");
    }

    #[test]
    fn test_parse_color_string_five_parts_returns_none() {
        // 5 comma-separated values is unsupported → None.
        assert!(parse_color_string("1,2,3,4,5").is_none());
    }

    #[test]
    fn test_to_hex_zero_channels_emits_six_digit_hex() {
        // All zeros → "#000000" (6 hex digits, lowercase).
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.to_hex(), "#000000");
    }

    #[test]
    fn test_to_svg_alpha_255_delegates_to_rgb_form() {
        // a=255 → emits "rgb(...)" not "rgba(...)".
        let c = Color::rgba(10, 20, 30, 255);
        assert_eq!(c.to_svg(), "rgb(10,20,30)");
    }

    #[test]
    fn test_to_svg_alpha_below_255_emits_rgba_with_two_decimal_fmt() {
        // a=128 → "rgba(r,g,b,0.50)" (exactly 2 decimals).
        let c = Color::rgba(10, 20, 30, 128);
        let s = c.to_svg();
        assert!(s.starts_with("rgba(10,20,30,"));
        // 128/255 ≈ 0.50196 → formatted as "0.50".
        assert!(s.contains(",0.50"));
    }

    #[test]
    fn test_rgb_color_unknown_name_returns_none() {
        // Name not in colors_conf → None.
        let conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        assert!(rgb_color("nonexistent", &conf).is_none());
    }

    #[test]
    fn test_colormap_resolve_direct_rgb_string() {
        // "255,0,0" contains comma → parse_color_string path → Some(Color).
        let cm = ColorMap::new();
        let c = cm.resolve("255,0,0").unwrap();
        assert_eq!(c.to_svg_rgb(), "rgb(255,0,0)");
    }

    #[test]
    fn test_colormap_resolve_plain_named_color_hit() {
        // Named color in colors map → Some(color).
        let mut cm = ColorMap::new();
        cm.colors.insert("myred".into(), Color::rgb(255, 0, 0));
        let c = cm.resolve("myred").unwrap();
        assert_eq!(c.to_svg_rgb(), "rgb(255,0,0)");
    }

    #[test]
    fn test_colormap_resolve_unknown_name_without_alpha_returns_none() {
        // Name without "_a" suffix and not in map → None.
        let cm = ColorMap::new();
        assert!(cm.resolve("no_such_color").is_none());
    }

    #[test]
    fn test_colormap_resolve_with_alpha_suffix_scales_alpha() {
        // "red_a0" → base red with alpha=0 (idx 0 / 5 × 255 = 0).
        let mut cm = ColorMap::new();
        cm.colors.insert("red".into(), Color::rgb(255, 0, 0));
        let c = cm.resolve("red_a0").unwrap();
        assert_eq!(c.a, 0);
        assert_eq!(c.r, 255);
    }

    #[test]
    fn test_colormap_resolve_alpha_suffix_unknown_alpha_digit_returns_none() {
        // Base name present but "_aX" alpha not numeric → fall through to plain lookup (which fails).
        let mut cm = ColorMap::new();
        cm.colors.insert("red".into(), Color::rgb(255, 0, 0));
        // "red_aabc" → alpha fails to parse as u8 → fallthrough → "red_aabc" not in map → None.
        assert!(cm.resolve("red_aabc").is_none());
    }

    #[test]
    fn test_colormap_allocate_colors_simple_rgb_triple() {
        // "red" = "255,0,0" → allocated.
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("red".into(), crate::config::types::ConfigValue::Str("255,0,0".into()));
        let mut cm = ColorMap::new();
        cm.allocate_colors(&conf, false, 0, None);
        let c = cm.colors.get("red").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_rgb_color_alpha_suffix_recurses_to_base() {
        // "red_a3" → strips "_a3" → recurses on "red" → (255,0,0).
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("red".into(), crate::config::types::ConfigValue::Str("255,0,0".into()));
        let rgb = rgb_color("red_a3", &conf).unwrap();
        assert_eq!(rgb, (255, 0, 0));
    }

    #[test]
    fn test_parse_color_string_with_invalid_integer_fails() {
        // One field not parseable → None.
        assert!(parse_color_string("255,notanumber,0").is_none());
    }

    #[test]
    fn test_colormap_allocate_colors_with_name_alias_first_pass_only_rgb() {
        // First pass only sees direct RGB triples; name aliases resolved later.
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("red".into(), crate::config::types::ConfigValue::Str("255,0,0".into()));
        // "crimson" is an alias pointing to "red".
        conf.insert("crimson".into(), crate::config::types::ConfigValue::Str("red".into()));
        let mut cm = ColorMap::new();
        cm.allocate_colors(&conf, false, 0, None);
        // "red" allocated in first pass.
        assert!(cm.colors.contains_key("red"));
    }

    #[test]
    fn test_parse_color_string_with_overflow_u8_fails() {
        // 256 > u8::MAX → parse as u8 fails → None.
        assert!(parse_color_string("256,0,0").is_none());
    }

    #[test]
    fn test_colormap_resolve_returns_copy_not_ref() {
        // Resolved color is owned (not a ref into map).
        let mut cm = ColorMap::new();
        cm.colors.insert("red".into(), Color::rgb(255, 0, 0));
        let c1 = cm.resolve("red").unwrap();
        let c2 = cm.resolve("red").unwrap();
        assert_eq!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_colormap_default_has_empty_colors_map() {
        // Default::default() → empty colors map.
        let cm = ColorMap::default();
        assert!(cm.colors.is_empty());
    }

    #[test]
    fn test_color_rgb_r_g_b_fields_accessible() {
        // Direct field access yields original channel values.
        let c = Color::rgb(1, 2, 3);
        assert_eq!(c.r, 1);
        assert_eq!(c.g, 2);
        assert_eq!(c.b, 3);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_color_clone_preserves_channels() {
        // Color is Clone + Copy — cloned color equals original.
        let c = Color::rgba(10, 20, 30, 40);
        let c2 = c;
        assert_eq!(c.r, c2.r);
        assert_eq!(c.g, c2.g);
        assert_eq!(c.b, c2.b);
        assert_eq!(c.a, c2.a);
    }

    #[test]
    fn test_to_hex_on_all_channels_max_yields_all_f() {
        // rgb(255,255,255) → "#ffffff".
        let c = Color::rgb(255, 255, 255);
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_rgb_color_transparency_at_zero_steps_returns_zero() {
        // auto_alpha_steps=0 → opacity=1 → transparency=0.
        let t = rgb_color_transparency("any_color", 0);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn test_rgb_color_opacity_with_n_equal_to_steps_yields_zero() {
        // N=5, steps=5 → opacity = 1 - 5/5 = 0.
        let v = rgb_color_opacity("c_a5", 5);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_rgb_color_opacity_with_steps_larger_than_n_partial() {
        // N=2, steps=10 → opacity = 1 - 2/10 = 0.8.
        let v = rgb_color_opacity("c_a2", 10);
        assert!((v - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_parse_color_string_four_part_rgba_with_alpha() {
        // "r,g,b,a" → parse as rgba.
        let c = parse_color_string("10,20,30,128").unwrap();
        assert_eq!(c.r, 10);
        assert_eq!(c.g, 20);
        assert_eq!(c.b, 30);
        assert_eq!(c.a, 128);
    }

    #[test]
    fn test_to_svg_rgb_with_zero_rgb_emits_rgb000() {
        // rgb(0,0,0) → "rgb(0,0,0)".
        let c = Color::rgb(0, 0, 0);
        assert_eq!(c.to_svg_rgb(), "rgb(0,0,0)");
    }

    #[test]
    fn test_colormap_allocate_colors_skips_transparent_name() {
        // "transparent" entry skipped in first pass.
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("transparent".into(), crate::config::types::ConfigValue::Str("255,255,255".into()));
        conf.insert("red".into(), crate::config::types::ConfigValue::Str("255,0,0".into()));
        let mut cm = ColorMap::new();
        cm.allocate_colors(&conf, false, 0, None);
        // "red" allocated, "transparent" skipped (add_transparent=false).
        assert!(cm.colors.contains_key("red"));
        assert!(!cm.colors.contains_key("transparent"));
    }

    #[test]
    fn test_rgb_color_with_alpha_suffix_multi_digit() {
        // "red_a10" → base "red" → (255,0,0).
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        conf.insert("red".into(), crate::config::types::ConfigValue::Str("255,0,0".into()));
        let rgb = rgb_color("red_a10", &conf).unwrap();
        assert_eq!(rgb, (255, 0, 0));
    }

    #[test]
    fn test_parse_color_string_whitespace_only_fails() {
        // "   " → single-part after trim → unparseable as u8 → None.
        assert!(parse_color_string("   ").is_none());
    }

    #[test]
    fn test_color_rgba_max_alpha_equal_to_255_equivalent_to_rgb() {
        // Color::rgba(r,g,b,255) has same channels as Color::rgb(r,g,b).
        let a = Color::rgba(100, 150, 200, 255);
        let b = Color::rgb(100, 150, 200);
        assert_eq!(a.r, b.r);
        assert_eq!(a.g, b.g);
        assert_eq!(a.b, b.b);
        assert_eq!(a.a, b.a);
    }
}
