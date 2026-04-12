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
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

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
    pub fn new() -> Self {
        ColorMap {
            colors: HashMap::new(),
        }
    }

    /// Parse color definitions from a config map (key = "r,g,b" or "r,g,b,a").
    pub fn load_from_config(&mut self, config: &HashMap<String, super::super::config::types::ConfigValue>) {
        for (name, value) in config {
            if let Some(s) = value.as_str() {
                if let Some(color) = parse_color_string(s) {
                    self.colors.insert(name.clone(), color);
                }
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
        if let Some((base_name, alpha_suffix)) = name.rsplit_once("_a") {
            if let Ok(alpha_idx) = alpha_suffix.parse::<u8>() {
                if let Some(base_color) = self.colors.get(base_name) {
                    let alpha = ((alpha_idx as f64 / 5.0) * 255.0) as u8;
                    return Some(Color::rgba(base_color.r, base_color.g, base_color.b, alpha));
                }
            }
        }

        // Plain named color
        self.colors.get(name).copied()
    }
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
        map.colors.insert("red".to_string(), Color::rgb(247, 42, 66));
        map.colors.insert("blue".to_string(), Color::rgb(54, 116, 217));

        assert_eq!(map.resolve("red"), Some(Color::rgb(247, 42, 66)));
        assert_eq!(map.resolve("255,0,0"), Some(Color::rgb(255, 0, 0)));

        // Alpha variant
        let resolved = map.resolve("red_a3").unwrap();
        assert_eq!(resolved.r, 247);
        assert_eq!(resolved.g, 42);
        assert_eq!(resolved.b, 66);
        assert!(resolved.a < 255); // has alpha applied
    }
}
