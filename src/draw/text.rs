//! Port of Perl `draw_text` and related text-rendering helpers.

use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::draw::report_image_map;
use crate::render::color::Color;
use crate::render::svg::SvgDocument;

/// Named-arg struct mirroring Perl `draw_text(…)`'s `%params`: `color`, `font`,
/// `size`, `angle` (radians for GD), `pangle` (polar angle, for text-anchor
/// orientation), `forcerotation`, `text`, `xy`, `svgxy`, `svgangle`,
/// `mapoptions.url`.
#[derive(Debug)]
pub struct DrawTextParams<'a> {
    pub color: &'a Color,
    pub font: &'a str,
    pub size: f64,
    pub angle: f64,
    pub pangle: f64,
    pub forcerotation: f64,
    pub text: &'a str,
    pub xy: (f64, f64),
    pub svgxy: Option<(f64, f64)>,
    pub svgangle: Option<f64>,
    pub url: Option<&'a str>,
}

/// Port of Perl `draw_text(args...)`: 82-LOC body that (1) computes bounds via
/// `stringFT`, (2) when `svgxy`/`svgangle` are set, emits an SVG text element
/// with `text-anchor: start` or `end` depending on `pangle ∈ (90, 270)`,
/// (3) when `mapoptions.url` is set, pushes a 4-point poly image-map area
/// using the bounds. `image_map_xshift`/`yshift`/`xfactor`/`yfactor` are read
/// from `<image>` to transform the coords. PNG rendering is the SVG backend's
/// job in Rust.
pub fn draw_text(
    doc: &mut SvgDocument,
    params: &DrawTextParams,
    image_conf: Option<&HashMap<String, ConfigValue>>,
) {
    // (1) Compute bounds (Perl `GD::Image->stringFT` returns 8-element array).
    let bounds = label_bounds(params.font, params.size, params.text);

    // (2) SVG emission: only when svgxy + svgangle are both set.
    if let (Some((sx, sy)), Some(svgangle)) = (params.svgxy, params.svgangle) {
        let tanchor = if params.pangle > 90.0 && params.pangle < 270.0 {
            "end"
        } else {
            "start"
        };
        let svg_text = params.text.replace('&', "&amp;");
        let svg = format!(
            r#"<text x="{:.1}" y="{:.1}" style="fill: {}; font-size: {:.1}px; text-anchor: {}" transform="rotate({:.1},{:.1},{:.1})">{}</text>"#,
            sx,
            sy,
            params.color.to_svg_rgb(),
            params.size,
            tanchor,
            svgangle + params.forcerotation,
            sx,
            sy,
            svg_text,
        );
        doc.add(svg);
    }

    // (3) Image-map: poly area from the 4 corner pairs in `bounds`. Perl
    //     applies optional xshift/yshift/xfactor/yfactor from <image>.
    if let Some(url) = params.url {
        let xshift = image_conf
            .and_then(|m| m.get("image_map_xshift"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let yshift = image_conf
            .and_then(|m| m.get("image_map_yshift"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let xmult = image_conf
            .and_then(|m| m.get("image_map_xfactor"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let ymult = image_conf
            .and_then(|m| m.get("image_map_yfactor"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let coords: Vec<f64> = (0..4)
            .flat_map(|i| {
                let x = bounds[2 * i] * xmult + xshift + params.xy.0;
                let y = bounds[2 * i + 1] * ymult + yshift + params.xy.1;
                [x, y]
            })
            .collect();
        report_image_map("poly", &coords, url);
    }
}

/// Cache of parsed `fontdue::Font` keyed by file path. Perl's GD opens the
/// font file on every stringFT call; here we parse once per file.
static FONT_CACHE: std::sync::LazyLock<std::sync::RwLock<std::collections::HashMap<String, std::sync::Arc<fontdue::Font>>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

/// Try to load a parsed font by path, falling back to `None` if the file
/// doesn't exist or isn't parseable. Cached by path.
fn load_font(fontfile: &str) -> Option<std::sync::Arc<fontdue::Font>> {
    if fontfile.is_empty() {
        return None;
    }
    if let Ok(cache) = FONT_CACHE.read()
        && let Some(f) = cache.get(fontfile)
    {
        return Some(f.clone());
    }
    let bytes = std::fs::read(fontfile).ok()?;
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
    let arc = std::sync::Arc::new(font);
    if let Ok(mut cache) = FONT_CACHE.write() {
        cache.insert(fontfile.to_string(), arc.clone());
    }
    Some(arc)
}

/// Port of Perl `text_size(fontfile, size, text)`: returns the (width, height)
/// the given text occupies. Perl uses GD::Image::stringFT; the Rust port
/// uses `fontdue` to obtain real font metrics. Falls back to the previous
/// heuristic (chars × size × 0.55, h = size) if the font file is missing
/// or can't be parsed.
pub fn text_size(fontfile: &str, size: f64, text: &str) -> (f64, f64) {
    if let Some(font) = load_font(fontfile) {
        let mut w: f64 = 0.0;
        let mut max_ascent: f64 = 0.0;
        let mut max_descent: f64 = 0.0;
        for ch in text.chars() {
            let m = font.metrics(ch, size as f32);
            w += m.advance_width as f64;
            let top = (m.ymin as f64 + m.height as f64).max(0.0);
            let bottom = -m.ymin as f64;
            if top > max_ascent {
                max_ascent = top;
            }
            if bottom > max_descent {
                max_descent = bottom;
            }
        }
        let h = (max_ascent + max_descent).max(1.0);
        (w.max(1.0), h)
    } else {
        // Fallback heuristic
        let w = text.chars().count() as f64 * size * 0.55;
        (w, size)
    }
}

/// Port of Perl `label_bounds(fontfile, size, text)`: returns the 8-element
/// bounds array `[x0,y0, x1,y1, x2,y2, x3,y3]` in GD::Image::stringFT order
/// (lower-left, lower-right, upper-right, upper-left). Uses `text_size`
/// under the hood for width/height (which uses fontdue when the font file
/// is readable, heuristic otherwise).
pub fn label_bounds(fontfile: &str, size: f64, text: &str) -> [f64; 8] {
    let (w, h) = text_size(fontfile, size, text);
    [
        0.0, h, // x0, y0 lower-left
        w, h, // x1, y1 lower-right
        w, 0.0, // x2, y2 upper-right
        0.0, 0.0, // x3, y3 upper-left
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    // Shared with `draw::tests` via the pub(crate) lock in `draw::mod`.
    use crate::draw::MAP_TEST_LOCK;

    #[test]
    fn text_size_heuristic_fallback() {
        // Missing font file → heuristic path: w = chars × size × 0.55, h = size.
        let (w, h) = text_size("", 12.0, "hello");
        assert!((w - 5.0 * 12.0 * 0.55).abs() < 1e-6);
        assert_eq!(h, 12.0);
    }

    #[test]
    fn text_size_with_real_font() {
        // Circos repo ships TTFs in circos/fonts/. Use the default font for
        // this test if available; otherwise skip.
        let candidates = ["circos/fonts/LTe50046.ttf"];
        let font = candidates.iter().copied().find(|p| std::path::Path::new(p).exists());
        let Some(f) = font else {
            eprintln!("skipping — no shipped font available");
            return;
        };
        let (w, h) = text_size(f, 24.0, "ABC");
        assert!(w > 0.0, "expected positive width, got {}", w);
        assert!(h > 0.0, "expected positive height, got {}", h);
        // Cached: second call should return the same result.
        let (w2, h2) = text_size(f, 24.0, "ABC");
        assert_eq!((w, h), (w2, h2));
    }

    #[test]
    fn label_bounds_gd_order() {
        let b = label_bounds("", 10.0, "xy");
        // x3,y3 is upper-left (0,0); x1,y1 is lower-right (w,h)
        assert_eq!(b[6], 0.0);
        assert_eq!(b[7], 0.0);
        assert!(b[2] > 0.0 && b[3] > 0.0);
    }

    #[test]
    fn text_size_empty_text_gives_unit_bounds() {
        // Empty text with missing font → w = 0 * size * 0.55 = 0; h = size.
        let (w, h) = text_size("", 12.0, "");
        assert_eq!(w, 0.0);
        assert_eq!(h, 12.0);
    }

    #[test]
    fn text_size_scales_linearly_with_text_length() {
        // Heuristic path: w is linear in character count.
        let (w1, _) = text_size("", 10.0, "a");
        let (w2, _) = text_size("", 10.0, "aa");
        let (w5, _) = text_size("", 10.0, "aaaaa");
        assert!((w2 - 2.0 * w1).abs() < 1e-6);
        assert!((w5 - 5.0 * w1).abs() < 1e-6);
    }

    #[test]
    fn label_bounds_size_scales_width() {
        // Bigger font → bigger bounds width.
        let b_small = label_bounds("", 10.0, "abc");
        let b_big = label_bounds("", 20.0, "abc");
        assert!(b_big[2] > b_small[2]);
        assert!(b_big[3] > b_small[3]);
    }

    #[test]
    fn draw_text_emits_svg_only_with_svgxy_and_svgangle() {
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let initial_len = doc.elements.len();
        // Without svgxy/svgangle → no SVG emission.
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "hello",
            xy: (0.0, 0.0),
            svgxy: None,
            svgangle: None,
            url: None,
        };
        draw_text(&mut doc, &params, None);
        assert_eq!(doc.elements.len(), initial_len, "should not emit SVG without svgxy+svgangle");

        // With both set → SVG emitted.
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 45.0,
            forcerotation: 0.0,
            text: "hello",
            xy: (100.0, 100.0),
            svgxy: Some((100.0, 100.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        assert_eq!(doc.elements.len(), initial_len + 1);
        assert!(doc.elements.last().unwrap().contains("<text"));
    }

    #[test]
    fn draw_text_text_anchor_flips_on_lower_half_pangle() {
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        // pangle ∈ (90,270) → text-anchor: end.
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 180.0,
            forcerotation: 0.0,
            text: "hello",
            xy: (0.0, 0.0),
            svgxy: Some((100.0, 100.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("text-anchor: end"));

        // pangle=45 (upper half) → text-anchor: start.
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            pangle: 45.0,
            ..DrawTextParams {
                color: &color,
                font: "",
                size: 12.0,
                angle: 0.0,
                pangle: 0.0,
                forcerotation: 0.0,
                text: "hello",
                xy: (0.0, 0.0),
                svgxy: Some((100.0, 100.0)),
                svgangle: Some(0.0),
                url: None,
            }
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("text-anchor: start"));
    }

    #[test]
    fn draw_text_escapes_ampersand() {
        // `&` in the text must be escaped as `&amp;` for valid XML.
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "foo&bar",
            xy: (0.0, 0.0),
            svgxy: Some((100.0, 100.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("foo&amp;bar"));
        assert!(!svg.contains("foo&bar"), "raw & leaked into output");
    }

    #[test]
    fn draw_text_svgangle_plus_forcerotation_additive() {
        // The rotate() transform uses svgangle + forcerotation.
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 30.0,
            text: "abc",
            xy: (0.0, 0.0),
            svgxy: Some((100.0, 100.0)),
            svgangle: Some(60.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        // rotate(90.0, ...) because svgangle(60) + forcerotation(30) = 90.
        assert!(svg.contains("rotate(90.0,100.0,100.0)"), "expected rotation=90 in: {}", svg);
    }

    #[test]
    fn draw_text_url_pushes_image_map_area() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use crate::draw::{drain_map_elements, report_image_map};
        // Clear any previous state.
        let _ = drain_map_elements();
        // Confirm the mutex-guarded global starts empty.
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "abc",
            xy: (50.0, 60.0),
            svgxy: None,
            svgangle: None,
            url: Some("/click"),
        };
        draw_text(&mut doc, &params, None);
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        let area = &areas[0];
        assert_eq!(area.shape, "poly");
        assert_eq!(area.url, "/click");
        // Poly from 4 corners × 2 = 8 ints.
        assert_eq!(area.coords.len(), 8);
        // Need to call drain AFTER any parallel test ran — the RUN_MUTEX
        // doesn't guard this test from other report_image_map callers.
        let _ = report_image_map; // silence unused-import in some orders
    }

    #[test]
    fn draw_text_image_map_applies_shift_and_scale() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use crate::draw::drain_map_elements;
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        // image_conf with xshift=10, yshift=20, xfactor=2, yfactor=3.
        let mut image_conf: HashMap<String, ConfigValue> = HashMap::new();
        image_conf.insert("image_map_xshift".into(), ConfigValue::Str("10".into()));
        image_conf.insert("image_map_yshift".into(), ConfigValue::Str("20".into()));
        image_conf.insert("image_map_xfactor".into(), ConfigValue::Str("2".into()));
        image_conf.insert("image_map_yfactor".into(), ConfigValue::Str("3".into()));
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "xy",
            xy: (100.0, 200.0),
            svgxy: None,
            svgangle: None,
            url: Some("/foo"),
        };
        draw_text(&mut doc, &params, Some(&image_conf));
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        let c = &areas[0].coords;
        // First corner of bounds is (0, h) (lower-left); with label_bounds
        // heuristic h = size = 10.0 → x = 0*2 + 10 + 100 = 110, y = 10*3 + 20 + 200 = 250.
        // coords stored as Vec<i64> (rounded) so compare directly.
        assert_eq!(c[0], 110);
        assert_eq!(c[1], 250);
    }

    #[test]
    fn draw_text_image_map_defaults_when_no_image_conf() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // image_conf=None → xshift=0, yshift=0, xfactor=1, yfactor=1 → coords
        // equal bounds + xy only.
        use crate::draw::drain_map_elements;
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "xy",
            xy: (5.0, 7.0),
            svgxy: None,
            svgangle: None,
            url: Some("/bar"),
        };
        draw_text(&mut doc, &params, None);
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        let c = &areas[0].coords;
        // First corner (0, h=10): x = 0*1 + 0 + 5 = 5, y = 10*1 + 0 + 7 = 17.
        assert_eq!(c[0], 5);
        assert_eq!(c[1], 17);
    }

    #[test]
    fn draw_text_pangle_exactly_90_uses_start_anchor() {
        // Boundary check: pangle == 90 is NOT strictly > 90 → "start".
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 90.0,
            forcerotation: 0.0,
            text: "x",
            xy: (0.0, 0.0),
            svgxy: Some((50.0, 50.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("text-anchor: start"));
        // pangle == 270 (end of range, NOT strict <) → also "start".
        let mut doc = SvgDocument::new(1000.0, 1000.0);
        let params = DrawTextParams {
            pangle: 270.0,
            ..DrawTextParams {
                color: &color,
                font: "",
                size: 12.0,
                angle: 0.0,
                pangle: 0.0,
                forcerotation: 0.0,
                text: "x",
                xy: (0.0, 0.0),
                svgxy: Some((50.0, 50.0)),
                svgangle: Some(0.0),
                url: None,
            }
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("text-anchor: start"));
    }

    #[test]
    fn test_label_bounds_empty_text_gives_zero_width() {
        // Empty text → w=0 via heuristic (0 chars × size × 0.55 = 0).
        let b = label_bounds("", 10.0, "");
        // Bounds: [0, h, 0, h, 0, 0, 0, 0] — w=0, h=size.
        assert_eq!(b[0], 0.0); // x0
        assert_eq!(b[2], 0.0); // x1 (w=0)
        // y0 = y1 = h = 10.
        assert_eq!(b[1], 10.0);
        assert_eq!(b[3], 10.0);
    }

    #[test]
    fn test_label_bounds_single_char_heuristic() {
        // Single char → w = 1 × size × 0.55 = 5.5 for size=10.
        let b = label_bounds("", 10.0, "x");
        assert!((b[2] - 5.5).abs() < 1e-9);
        // Upper-left is (0,0).
        assert_eq!(b[6], 0.0);
        assert_eq!(b[7], 0.0);
    }

    #[test]
    fn test_label_bounds_large_font_size_scales_proportionally() {
        // Same text at 10pt vs 20pt — 20pt width should be 2× 10pt width.
        let b10 = label_bounds("", 10.0, "hello");
        let b20 = label_bounds("", 20.0, "hello");
        assert!((b20[2] - 2.0 * b10[2]).abs() < 1e-9);
        assert_eq!(b20[1], 2.0 * b10[1]);
    }

    #[test]
    fn test_label_bounds_gd_order_lower_left_is_origin_plus_height() {
        // GD order: [x0=0, y0=h, x1=w, y1=h, x2=w, y2=0, x3=0, y3=0].
        let b = label_bounds("", 12.0, "test");
        // lower-left x0,y0
        assert_eq!(b[0], 0.0);
        assert!(b[1] > 0.0); // y0 = h > 0
        // x3 == x0, y3 == 0 (upper-left)
        assert_eq!(b[6], b[0]);
        assert_eq!(b[7], 0.0);
    }

    #[test]
    fn test_text_size_with_real_font_scales_with_size() {
        // Bigger font size → larger width with real font metrics.
        let candidates = ["circos/fonts/LTe50046.ttf", "circos/fonts/frutiger55.ttf"];
        let font = candidates
            .iter()
            .copied()
            .find(|p| std::path::Path::new(p).exists());
        let Some(f) = font else {
            eprintln!("skipping — no shipped font available");
            return;
        };
        let (w12, _) = text_size(f, 12.0, "ABC");
        let (w24, _) = text_size(f, 24.0, "ABC");
        assert!(w24 > w12, "24pt should be wider than 12pt");
    }

    #[test]
    fn test_text_size_with_real_font_longer_text_is_wider() {
        // Longer text → larger width at same font size.
        let candidates = ["circos/fonts/LTe50046.ttf", "circos/fonts/frutiger55.ttf"];
        let font = candidates
            .iter()
            .copied()
            .find(|p| std::path::Path::new(p).exists());
        let Some(f) = font else {
            return;
        };
        let (w3, _) = text_size(f, 20.0, "ABC");
        let (w9, _) = text_size(f, 20.0, "ABCDEFGHI");
        assert!(w9 > w3, "9-char text should be wider than 3-char");
    }

    #[test]
    fn test_text_size_nonexistent_font_falls_back_to_heuristic() {
        // A font path that doesn't exist → load_font None → heuristic path.
        // Heuristic: w = chars × size × 0.55, h = size.
        let (w, h) = text_size("/nonexistent/bogus.ttf", 10.0, "abcd");
        assert!((w - 4.0 * 10.0 * 0.55).abs() < 1e-9);
        assert_eq!(h, 10.0);
    }

    #[test]
    fn test_text_size_unparseable_file_falls_back_to_heuristic() {
        // A file that exists but isn't a valid TTF — fontdue::Font::from_bytes fails.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"NOT-A-TTF-FILE").unwrap();
        let (w, h) = text_size(tmp.path().to_str().unwrap(), 10.0, "xy");
        // Heuristic: 2 × 10 × 0.55 = 11.
        assert!((w - 11.0).abs() < 1e-9);
        assert_eq!(h, 10.0);
    }

    #[test]
    fn test_draw_text_special_chars_other_than_ampersand_passthrough() {
        // Non-ampersand XML special chars like `<`, `>` pass through unchanged.
        // (Current impl only escapes `&`.)
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 45.0,
            forcerotation: 0.0,
            text: "a<b>",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        // Only `&` is currently escaped; `<` and `>` pass through as-is.
        assert!(svg.contains("a<b>"));
    }

    #[test]
    fn test_draw_text_negative_forcerotation_subtracts() {
        // svgangle=60, forcerotation=-30 → sum=30 → transform="rotate(30.0,...)".
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 45.0,
            forcerotation: -30.0,
            text: "x",
            xy: (0.0, 0.0),
            svgxy: Some((50.0, 50.0)),
            svgangle: Some(60.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("rotate(30.0,50.0,50.0)"));
    }

    #[test]
    fn test_draw_text_color_rendered_as_svg_rgb() {
        // `color.to_svg_rgb()` emits `rgb(r,g,b)` — verified in the fill style.
        let color = Color::rgb(123, 45, 67);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "x",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("fill: rgb(123,45,67)"));
    }

    #[test]
    fn test_draw_text_empty_text_still_emits_svg_element() {
        // Empty text string → still emits `<text>` (just with no content).
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let before = doc.elements.len();
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        assert_eq!(doc.elements.len(), before + 1);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("<text"));
        // Empty text between open/close tags.
        assert!(svg.contains("></text>") || svg.contains(">\n</text>"));
    }

    #[test]
    fn label_bounds_corners_form_rectangle() {
        // All 4 corners in GD order: [x0,y0, x1,y1, x2,y2, x3,y3]
        //   lower-left (0,h), lower-right (w,h), upper-right (w,0), upper-left (0,0).
        let b = label_bounds("", 12.0, "hello");
        // x0 == x3 (left edge); x1 == x2 (right edge).
        assert_eq!(b[0], b[6]);
        assert_eq!(b[2], b[4]);
        // y0 == y1 (bottom); y2 == y3 (top).
        assert_eq!(b[1], b[3]);
        assert_eq!(b[5], b[7]);
        // Upper-left is (0,0).
        assert_eq!(b[6], 0.0);
        assert_eq!(b[7], 0.0);
    }

    #[test]
    fn test_draw_text_pangle_in_lower_half_anchor_end() {
        // pangle ∈ (90, 270) strictly → anchor end. Test multiple angles in range.
        let color = Color::rgb(0, 0, 0);
        for pangle in [91.0f64, 135.0, 180.0, 225.0, 269.0] {
            let mut doc = SvgDocument::new(100.0, 100.0);
            let params = DrawTextParams {
                color: &color,
                font: "",
                size: 10.0,
                angle: 0.0,
                pangle,
                forcerotation: 0.0,
                text: "t",
                xy: (0.0, 0.0),
                svgxy: Some((10.0, 10.0)),
                svgangle: Some(0.0),
                url: None,
            };
            draw_text(&mut doc, &params, None);
            let svg = doc.elements.last().unwrap();
            assert!(svg.contains("text-anchor: end"), "pangle={} expected anchor:end, got {}", pangle, svg);
        }
    }

    #[test]
    fn test_text_size_fontdue_caches_result() {
        // Calling text_size twice with the same path loads the font once
        // (cache hit on 2nd call). This also covers the cache lookup branch.
        let candidates = ["circos/fonts/LTe50046.ttf", "circos/fonts/frutiger55.ttf"];
        let font = candidates.iter().copied().find(|p| std::path::Path::new(p).exists());
        let Some(f) = font else {
            eprintln!("skipping — no shipped font available");
            return;
        };
        // First call populates cache.
        let (w1, h1) = text_size(f, 16.0, "XYZ");
        // Second call should use cache → identical values.
        let (w2, h2) = text_size(f, 16.0, "XYZ");
        assert_eq!(w1, w2);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_draw_text_no_url_produces_no_image_map_entry() {
        // With url=None, no `report_image_map` should be called — no entry added.
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use crate::draw::drain_map_elements;
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "notmapped",
            xy: (20.0, 30.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let areas = drain_map_elements();
        assert!(areas.is_empty(), "no URL → no image-map entry");
    }

    #[test]
    fn test_draw_text_url_bounds_coords_have_8_components() {
        // With url=Some, the image-map area should have exactly 8 coords (4 corners × 2).
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use crate::draw::drain_map_elements;
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "hi",
            xy: (50.0, 60.0),
            svgxy: None,
            svgangle: None,
            url: Some("/click-me"),
        };
        draw_text(&mut doc, &params, None);
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].shape, "poly");
        assert_eq!(areas[0].coords.len(), 8, "bounds has 4 corners × 2 = 8 ints");
        assert_eq!(areas[0].url, "/click-me");
    }

    #[test]
    fn test_label_bounds_x0_equals_x3_and_x1_equals_x2() {
        // Axis-aligned rectangle: left column x0==x3; right column x1==x2.
        let b = label_bounds("", 12.0, "hello");
        assert_eq!(b[0], b[6]); // x0 == x3
        assert_eq!(b[2], b[4]); // x1 == x2
        // Top row y2==y3=0; bottom row y0==y1=h.
        assert_eq!(b[5], 0.0);
        assert_eq!(b[7], 0.0);
        assert_eq!(b[1], b[3]);
    }

    #[test]
    fn test_text_size_scales_linearly_with_char_count_heuristic() {
        // Heuristic: w = chars × size × 0.55. 2 chars × 10 = 11.
        let (w_2, _) = text_size("", 10.0, "ab");
        let (w_4, _) = text_size("", 10.0, "abcd");
        // Doubling chars should double width.
        assert!((w_4 - 2.0 * w_2).abs() < 1e-9);
    }

    #[test]
    fn test_label_bounds_zero_size_yields_zero_width_and_height() {
        // size=0 with any text → w = chars × 0 × 0.55 = 0; h = 0.
        let b = label_bounds("", 0.0, "hello");
        assert_eq!(b[0], 0.0);
        assert_eq!(b[2], 0.0);
        assert_eq!(b[1], 0.0); // h=0
        assert_eq!(b[3], 0.0);
    }

    #[test]
    fn test_draw_text_with_empty_svg_coords_skip_text_but_image_map_still_runs() {
        // url=Some but svgxy/svgangle=None → SVG emission skipped, image-map still fired.
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use crate::draw::drain_map_elements;
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let before = doc.elements.len();
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "x",
            xy: (5.0, 7.0),
            svgxy: None,
            svgangle: None,
            url: Some("/a"),
        };
        draw_text(&mut doc, &params, None);
        // No SVG element added.
        assert_eq!(doc.elements.len(), before);
        // But image map area was.
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
    }

    #[test]
    fn test_draw_text_pangle_exactly_90_is_start_anchor() {
        // pangle > 90 (strict) AND < 270 (strict) → "end". Exactly 90 → "start".
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 90.0,
            forcerotation: 0.0,
            text: "t",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("text-anchor: start"));
    }

    #[test]
    fn test_draw_text_pangle_exactly_270_is_start_anchor() {
        // pangle == 270 (strict < 270) → "start" (NOT in (90,270) open interval).
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 270.0,
            forcerotation: 0.0,
            text: "t",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("text-anchor: start"));
    }

    #[test]
    fn test_text_size_zero_height_fallback_guards_against_collapse() {
        // Heuristic returns (w, size) — size=0 yields h=0 (exact mk returns size).
        // With real font, height should be >=1.0 via max(1.0) floor.
        let (_w, h) = text_size("", 0.0, "x");
        assert_eq!(h, 0.0); // heuristic path
        // With non-zero size, heuristic gives h=size.
        let (_, h) = text_size("", 7.0, "a");
        assert_eq!(h, 7.0);
    }

    #[test]
    fn test_label_bounds_returns_8_elements() {
        // label_bounds always returns [f64; 8] regardless of input.
        let b = label_bounds("", 10.0, "x");
        assert_eq!(b.len(), 8);
        let b = label_bounds("", 10.0, "");
        assert_eq!(b.len(), 8);
        let b = label_bounds("", 20.0, "longer text");
        assert_eq!(b.len(), 8);
    }

    #[test]
    fn test_draw_text_angle_field_stored_but_unused_in_svg_path() {
        // angle field is Perl GD rotation angle; SVG uses svgangle+forcerotation.
        // Setting angle=1.0 but svgangle=0 and forcerotation=0 → rotate(0) in SVG.
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 1.0,  // GD-only
            pangle: 45.0,
            forcerotation: 0.0,
            text: "t",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 10.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let svg = doc.elements.last().unwrap();
        // SVG rotation is 0 (svgangle + forcerotation = 0). Format uses {:.1}.
        assert!(svg.contains("rotate(0.0,10.0,10.0)"));
    }

    #[test]
    fn test_text_size_whitespace_text_heuristic() {
        // Whitespace text chars count in char count heuristic.
        let (w1, _) = text_size("", 10.0, "abc");
        let (w2, _) = text_size("", 10.0, " abc ");
        // 5 chars vs 3 chars → w2 should be larger.
        assert!(w2 > w1);
    }

    #[test]
    fn test_draw_text_image_map_coords_have_8_points() {
        // With url + image_conf → image-map area coords length is always 8.
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use crate::draw::drain_map_elements;
        let _ = drain_map_elements();
        let color = Color::rgb(0, 0, 0);
        let mut doc = SvgDocument::new(100.0, 100.0);
        let image_conf: HashMap<String, ConfigValue> = HashMap::new();
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 10.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "mapped",
            xy: (50.0, 60.0),
            svgxy: None,
            svgangle: None,
            url: Some("/click-here"),
        };
        draw_text(&mut doc, &params, Some(&image_conf));
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // Bounds polygon: 4 corners × 2 = 8 coords.
        assert_eq!(areas[0].coords.len(), 8);
    }

    #[test]
    fn test_label_bounds_lower_left_y_equals_height() {
        // GD order: lower-left at (0, h); h = text_size's height.
        let b = label_bounds("", 12.0, "abc");
        // b[0]=x0=0, b[1]=y0=h. h for empty-font heuristic = size = 12.
        assert_eq!(b[0], 0.0);
        assert_eq!(b[1], 12.0);
    }

    #[test]
    fn test_text_size_heuristic_empty_string_zero_width() {
        // 0 chars × size × 0.55 = 0; height still = size (heuristic path).
        let (w, h) = text_size("", 12.0, "");
        assert_eq!(w, 0.0);
        assert_eq!(h, 12.0);
    }

    #[test]
    fn test_text_size_heuristic_scales_linearly_with_size() {
        // Heuristic: w = chars × size × 0.55 → doubling size → doubles w.
        let (w12, h12) = text_size("", 12.0, "hello");
        let (w24, h24) = text_size("", 24.0, "hello");
        assert!((w24 - 2.0 * w12).abs() < 1e-9);
        assert!((h24 - 2.0 * h12).abs() < 1e-9);
    }

    #[test]
    fn test_label_bounds_left_edge_x_coords_both_zero() {
        // Upper-left (x3) and lower-left (x0) share x=0 by construction.
        let b = label_bounds("", 14.0, "xyz");
        assert_eq!(b[0], 0.0); // x0
        assert_eq!(b[6], 0.0); // x3
    }

    #[test]
    fn test_label_bounds_right_edge_xs_match_width() {
        // x1 (lower-right) and x2 (upper-right) share the width value.
        let b = label_bounds("", 10.0, "hello");
        // b[2]=x1, b[4]=x2; both equal the text_size width.
        assert_eq!(b[2], b[4]);
        // And width is >0 for non-empty text.
        assert!(b[2] > 0.0);
    }

    #[test]
    fn test_label_bounds_upper_edge_ys_both_zero() {
        // GD convention: upper-right y2 and upper-left y3 are at y=0 (top of box).
        let b = label_bounds("", 12.0, "abc");
        assert_eq!(b[5], 0.0); // y2
        assert_eq!(b[7], 0.0); // y3
    }

    #[test]
    fn test_label_bounds_height_equals_size_for_heuristic() {
        // Heuristic path: h = size → lower-y coords (y0, y1) both equal size.
        let b = label_bounds("", 15.0, "abc");
        assert_eq!(b[1], 15.0); // y0 = h
        assert_eq!(b[3], 15.0); // y1 = h
    }

    #[test]
    fn test_label_bounds_longer_text_produces_wider_right_edge() {
        // More chars under heuristic → strictly wider x1/x2.
        let b_short = label_bounds("", 12.0, "a");
        let b_long = label_bounds("", 12.0, "abcdefgh");
        assert!(b_long[2] > b_short[2]);
        assert!(b_long[4] > b_short[4]);
    }

    #[test]
    fn test_text_size_zero_size_heuristic_returns_zero_dimensions() {
        // size=0 in heuristic → w = n*0*0.55 = 0, h = 0.
        let (w, h) = text_size("", 0.0, "hello");
        assert_eq!(w, 0.0);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn test_draw_text_without_svgxy_emits_no_text_element() {
        // svgxy=None → guard fails → no SVG element added to doc.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "hello",
            xy: (0.0, 0.0),
            svgxy: None,
            svgangle: Some(0.0),
            url: None,
        };
        let before = doc.elements.len();
        draw_text(&mut doc, &params, None);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_text_ampersand_in_text_escaped_to_entity() {
        // '&' in text → escaped to "&amp;" in SVG output.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 0.0,
            text: "A & B",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 20.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let last = doc.elements.last().unwrap();
        assert!(last.contains("A &amp; B"));
        // Verify no raw "&" before "amp;".
        assert!(!last.contains(" & "));
    }

    #[test]
    fn test_draw_text_pangle_in_upper_half_uses_end_anchor() {
        // pangle in (90, 270) → text-anchor: end.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 180.0,
            forcerotation: 0.0,
            text: "x",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 20.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let last = doc.elements.last().unwrap();
        assert!(last.contains("text-anchor: end"));
    }

    #[test]
    fn test_draw_text_pangle_in_lower_half_uses_start_anchor() {
        // pangle at 45 → NOT in (90,270) → text-anchor: start.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 45.0,
            forcerotation: 0.0,
            text: "x",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 20.0)),
            svgangle: Some(0.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let last = doc.elements.last().unwrap();
        assert!(last.contains("text-anchor: start"));
    }

    #[test]
    fn test_draw_text_forcerotation_added_to_svgangle_in_output() {
        // Output rotate(svgangle + forcerotation, cx, cy) — sum appears in transform.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color,
            font: "",
            size: 12.0,
            angle: 0.0,
            pangle: 0.0,
            forcerotation: 30.0,
            text: "x",
            xy: (0.0, 0.0),
            svgxy: Some((10.0, 20.0)),
            svgangle: Some(45.0),
            url: None,
        };
        draw_text(&mut doc, &params, None);
        let last = doc.elements.last().unwrap();
        // 45 + 30 = 75.0, formatted with {:.1}.
        assert!(last.contains("rotate(75.0,"));
    }

    #[test]
    fn test_draw_text_fill_color_to_svg_rgb_embedded_in_style() {
        // style attribute contains "fill: rgb(r,g,b);" from color.to_svg_rgb().
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(123, 45, 67);
        let params = DrawTextParams {
            color: &color, font: "", size: 10.0, angle: 0.0, pangle: 0.0,
            forcerotation: 0.0, text: "x", xy: (0.0, 0.0),
            svgxy: Some((5.0, 5.0)), svgangle: Some(0.0), url: None,
        };
        draw_text(&mut doc, &params, None);
        let last = doc.elements.last().unwrap();
        assert!(last.contains("fill: rgb(123,45,67);"));
    }

    #[test]
    fn test_draw_text_no_url_no_image_map_area_pushed() {
        // url=None → report_image_map not called — MAP_ELEMENTS unchanged.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        use crate::draw::{drain_map_elements, MAP_TEST_LOCK};
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color, font: "", size: 10.0, angle: 0.0, pangle: 0.0,
            forcerotation: 0.0, text: "x", xy: (5.0, 5.0),
            svgxy: Some((10.0, 10.0)), svgangle: Some(0.0), url: None,
        };
        draw_text(&mut doc, &params, None);
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 0);
    }

    #[test]
    fn test_draw_text_with_url_adds_image_map_poly_area() {
        // url=Some("/href") → report_image_map pushed with shape="poly".
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        use crate::draw::{drain_map_elements, MAP_TEST_LOCK};
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        let mut doc = SvgDocument::new(100.0, 100.0);
        let color = Color::rgb(0, 0, 0);
        let params = DrawTextParams {
            color: &color, font: "", size: 10.0, angle: 0.0, pangle: 0.0,
            forcerotation: 0.0, text: "lbl", xy: (0.0, 0.0),
            svgxy: Some((10.0, 20.0)), svgangle: Some(0.0), url: Some("/myhref"),
        };
        draw_text(&mut doc, &params, None);
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].shape, "poly");
        assert_eq!(areas[0].url, "/myhref");
        // Polygon has 4 corners → 8 coords.
        assert_eq!(areas[0].coords.len(), 8);
    }
}
