use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::data::types::Datum;
use crate::draw::ideograms::slice_polygon_coords;
use crate::draw::report_image_map;
use crate::intspan::IntSpan;
use crate::layout::Layout;
use crate::render::color::ColorMap;
use crate::render::svg::{SvgDocument, svg_slice};
use crate::utils::format_url;

/// Port of Perl `draw_highlights(datasets, chr, set, ideogram, test)`. Walks
/// each highlight z-group (Perl reads `$datasets->{param}{zlist}`) and, for
/// every point whose chr matches and whose options pass `test`, draws one
/// annular slice per sub-range of `filter_data(dataset, chr)`. Per-point
/// overrides: `r0`/`r1`/`offset`, `ideogram` flag (use ideogram radius),
/// `fill_color`, `stroke_color`, `stroke_thickness`, `url` (image-map, not
/// implemented here — stub).
///
/// Current entry point differs from Perl's (Rust is per-highlight-block with
/// flat datum list); the body below iterates in the same order Perl would.
pub fn draw_highlights(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
) {
    let r0_str = block_conf
        .get("r0")
        .and_then(|v| v.as_str())
        .unwrap_or("0.9r");
    let r1_str = block_conf
        .get("r1")
        .and_then(|v| v.as_str())
        .unwrap_or("0.95r");
    let default_r0 = parse_radius(r0_str, layout);
    let default_r1 = parse_radius(r1_str, layout);

    let default_fill_name = block_conf
        .get("fill_color")
        .and_then(|v| v.as_str())
        .unwrap_or("red");
    let default_stroke_name = block_conf.get("stroke_color").and_then(|v| v.as_str());
    let default_stroke_thickness: f64 = block_conf
        .get("stroke_thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let default_offset: f64 = block_conf
        .get("offset")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(0.0);

    // Perl: `my $url = seek_parameter("url", $data_point, $datum, @param_path)`.
    // We honor the block-level `url` and per-datum `url` param here.
    let block_url_tpl: Option<String> = block_conf
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let missing_policy: String = block_conf
        .get("image_map_missing_parameter")
        .and_then(|v| v.as_str())
        .unwrap_or("removeparam")
        .to_string();

    // Build z-level order: gather all z values from block + per-datum param.
    let mut zs: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    zs.insert(0);
    if let Some(z_str) = block_conf.get("z").and_then(|v| v.as_str())
        && let Ok(z) = z_str.parse::<i64>()
    {
        zs.insert(z);
    }
    for datum in data {
        if let Some(z_str) = datum.param.get("z")
            && let Ok(z) = z_str.parse::<i64>()
        {
            zs.insert(z);
        }
    }

    doc.open_group("highlights");

    for target_z in &zs {
        for datum in data {
            // z-filter: skip datum not at target_z
            let datum_z: i64 = datum
                .param
                .get("z")
                .and_then(|s| s.parse().ok())
                .or_else(|| {
                    block_conf
                        .get("z")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(0);
            if datum_z != *target_z {
                continue;
            }

            // chromosome on-display check (Perl: data->chr eq chr)
            let ideogram = match layout.find_ideogram_by_chr(&datum.chr) {
                Some(i) => i,
                None => continue,
            };

            // Intersect the datum's range with ideogram's display set
            let dataset = IntSpan::from_range(datum.start, datum.end);
            if dataset.intersect(&ideogram.set).cardinality() == 0 {
                continue;
            }
            let filtered = dataset.intersect(&ideogram.set);

            // Per-datum r0/r1/offset/ideogram overrides
            let r0_str_d = datum.param.get("r0").map(|s| s.as_str());
            let r1_str_d = datum.param.get("r1").map(|s| s.as_str());
            let use_ideogram = datum
                .param
                .get("ideogram")
                .map(|s| s == "1" || s == "yes")
                .unwrap_or(false);
            let offset: f64 = datum
                .param
                .get("offset")
                .and_then(|s| s.trim_end_matches('p').parse().ok())
                .unwrap_or(default_offset);

            let (radius_from, radius_to) =
                if use_ideogram && r0_str_d.is_none() && r1_str_d.is_none() {
                    (ideogram.radius_inner, ideogram.radius_outer)
                } else {
                    let r0 = r0_str_d
                        .map(|s| parse_radius(s, layout))
                        .unwrap_or(default_r0);
                    let r1 = r1_str_d
                        .map(|s| parse_radius(s, layout))
                        .unwrap_or(default_r1);
                    (r0 + offset, r1 + offset)
                };

            // Per-datum color/stroke overrides
            let fill_name = datum
                .param
                .get("fill_color")
                .or(datum.param.get("color"))
                .map(|s| s.as_str())
                .unwrap_or(default_fill_name);
            let stroke_name = datum
                .param
                .get("stroke_color")
                .map(|s| s.as_str())
                .or(default_stroke_name);
            let stroke_thickness: f64 = datum
                .param
                .get("stroke_thickness")
                .and_then(|s| s.parse().ok())
                .unwrap_or(default_stroke_thickness);

            let fill_color = colors.resolve(fill_name);
            let edge_color = stroke_name.and_then(|n| colors.resolve(n));

            // One slice per contiguous sub-range of the filtered set (Perl: $set->sets).
            // Our IntSpan doesn't expose sub-sets directly; iterate its intervals.
            for (lo, hi) in filtered.as_intervals() {
                let start_a = match layout.getanglepos(lo, &datum.chr) {
                    Some(a) => a,
                    None => continue,
                };
                let end_a = match layout.getanglepos(hi, &datum.chr) {
                    Some(a) => a,
                    None => continue,
                };
                let svg = svg_slice(
                    layout,
                    start_a,
                    end_a,
                    radius_from.min(radius_to),
                    radius_from.max(radius_to),
                    edge_color.as_ref(),
                    if stroke_thickness > 0.0 {
                        Some(stroke_thickness)
                    } else {
                        None
                    },
                    fill_color.as_ref(),
                    None,
                );
                doc.add(svg);

                // --- Image-map: emit a poly `<area>` for this highlight region ---
                let url_tpl: Option<String> = datum
                    .param
                    .get("url")
                    .cloned()
                    .or_else(|| block_url_tpl.clone());
                if let Some(tpl) = url_tpl {
                    let synthetic = crate::draw::plots::synthetic_datum_map(
                        &datum.chr,
                        lo,
                        hi,
                        None,
                    );
                    let datum_map = crate::draw::plots::datum_param_as_config(&datum.param);
                    if let Ok(Some(url)) =
                        format_url(&tpl, &[&synthetic, &datum_map], &missing_policy)
                    {
                        let coords = slice_polygon_coords(
                            layout.image_radius,
                            start_a,
                            end_a,
                            radius_from.min(radius_to),
                            radius_from.max(radius_to),
                        );
                        report_image_map("poly", &coords, &url);
                    }
                }
            }
        }
    }

    doc.close_group();
}

/// Parse a radius value (e.g., "0.9r", "1200p", "100").
fn parse_radius(s: &str, layout: &Layout) -> f64 {
    let s = s.trim();
    if s.ends_with('r') {
        let val: f64 = s.trim_end_matches('r').parse().unwrap_or(0.0);
        val * layout.dims.ideogram_radius
    } else if s.ends_with('p') {
        s.trim_end_matches('p').parse().unwrap_or(0.0)
    } else {
        s.parse().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_layout(ideo_radius: f64) -> Layout {
        Layout {
            ideograms: Vec::new(),
            gcircum: 3_000_000_000.0,
            gsize_noscale: 3_000_000_000.0,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1_000_000.0,
            dims: crate::layout::Dims {
                ideogram_radius: ideo_radius,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: ideo_radius - 50.0,
                ideogram_radius_outer: ideo_radius + 50.0,
            },
        }
    }

    #[test]
    fn test_parse_radius_suffix_r() {
        let layout = mk_layout(1000.0);
        // "Nr" → N × ideogram_radius.
        assert!((parse_radius("0.9r", &layout) - 900.0).abs() < 1e-9);
        assert!((parse_radius("1.05r", &layout) - 1050.0).abs() < 1e-9);
        assert!((parse_radius("0r", &layout) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_suffix_p() {
        let layout = mk_layout(1500.0);
        // "Np" → raw pixels (layout radius ignored).
        assert!((parse_radius("1200p", &layout) - 1200.0).abs() < 1e-9);
        assert!((parse_radius("42p", &layout) - 42.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_bare_number_treated_as_pixels() {
        let layout = mk_layout(1500.0);
        // Unadorned number → use value as-is.
        assert!((parse_radius("300", &layout) - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_invalid_returns_zero() {
        let layout = mk_layout(1500.0);
        // Garbage → 0.0 (unwrap_or fallback).
        assert_eq!(parse_radius("junk", &layout), 0.0);
        assert_eq!(parse_radius("", &layout), 0.0);
        assert_eq!(parse_radius("abcr", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_negative_p_suffix() {
        // "-100p" → p-suffix path parses to -100.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("-100p", &layout) - (-100.0)).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_large_values() {
        // Very large radius values pass through.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("10000p", &layout) - 10000.0).abs() < 1e-9);
        assert!((parse_radius("99.99r", &layout) - 99990.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_zero_value() {
        // Zero value in any form.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("0", &layout), 0.0);
        assert_eq!(parse_radius("0p", &layout), 0.0);
        assert_eq!(parse_radius("0r", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_r_suffix_zero_with_nonzero_ideogram_radius() {
        // "0r" regardless of ideogram_radius → 0.
        let layout = mk_layout(2500.0);
        assert_eq!(parse_radius("0r", &layout), 0.0);
        // Pure integer fraction: "2r" with ideogram_radius=2500 → 5000.
        assert_eq!(parse_radius("2r", &layout), 5000.0);
    }

    #[test]
    fn test_parse_radius_trims_whitespace() {
        let layout = mk_layout(1000.0);
        // Leading/trailing whitespace trimmed before suffix detection.
        assert!((parse_radius("  0.5r  ", &layout) - 500.0).abs() < 1e-9);
        assert!((parse_radius("  250p\t", &layout) - 250.0).abs() < 1e-9);
    }

    #[test]
    fn test_draw_highlights_empty_data_only_opens_closes_group() {
        // No data → only the `<g id="highlights">` wrapper is added.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // Exactly 2 elements added: open_group("highlights") + close_group.
        assert_eq!(doc.elements.len() - before, 2);
        assert!(doc.elements[before].contains("highlights"));
        assert!(doc.elements[before + 1].contains("</g>"));
    }

    #[test]
    fn test_draw_highlights_unknown_chr_datum_is_skipped() {
        // Datum with chr not in layout.ideograms → skipped, no slice drawn.
        use crate::data::types::Datum;
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let data = vec![Datum {
            chr: "unknown_chr".into(),
            start: 0,
            end: 100,
            ..Default::default()
        }];
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_highlights(&mut doc, &layout, &data, &block_conf, &colors);
        // Only the group wrapper added; no inner `<path>` slices.
        let added = &doc.elements[before..];
        assert!(!added.iter().any(|e| e.starts_with("<path")));
    }

    #[test]
    fn test_draw_highlights_parses_z_values_from_block_and_per_datum() {
        // `z` keys in both block_conf and per-datum.param should populate the
        // z-level BTreeSet used to order layers. With 0 the default, inserting
        // z=2 (block) and z=5 (datum) yields a sorted z set containing 0,2,5.
        use crate::data::types::Datum;
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut p = std::collections::HashMap::new();
        p.insert("z".to_string(), "5".to_string());
        let data = vec![Datum {
            chr: "unknown_chr".into(), // skipped below — but z still parsed for set
            start: 0,
            end: 100,
            param: p,
            ..Default::default()
        }];
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("z".into(), ConfigValue::Str("2".into()));
        // Should not panic; all datum skipped due to unknown chr, but the
        // z-level enumeration traverses each z before the skip.
        draw_highlights(&mut doc, &layout, &data, &block_conf, &colors);
        // Highlights group was opened and closed.
        assert!(doc.elements.iter().any(|e| e.contains("highlights")));
    }

    #[test]
    fn test_draw_highlights_missing_policy_default_is_removeparam() {
        // When `image_map_missing_parameter` is absent, the code falls back to
        // "removeparam" — verify no panic occurs reading that default.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("url".into(), ConfigValue::Str("/click".into()));
        // No `image_map_missing_parameter` → default should kick in.
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // No panic; group emitted.
        assert!(doc.elements.iter().any(|e| e.contains("highlights")));
    }

    #[test]
    fn test_parse_radius_decimal_p_suffix() {
        // "123.4p" → 123.4 via decimal parse of trimmed suffix.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("123.4p", &layout) - 123.4).abs() < 1e-9);
        // Small fractional p-value: "0.5p" → 0.5.
        assert!((parse_radius("0.5p", &layout) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_only_suffix_char_returns_zero() {
        // Input is just the suffix letter → strip leaves "" → parse fails → 0.
        let layout = mk_layout(1500.0);
        assert_eq!(parse_radius("r", &layout), 0.0);
        assert_eq!(parse_radius("p", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_r_suffix_scales_with_ideogram_radius() {
        // Same "1r" expression scales with different ideogram_radius values.
        let layout_small = mk_layout(500.0);
        let layout_big = mk_layout(2000.0);
        assert!((parse_radius("1r", &layout_small) - 500.0).abs() < 1e-9);
        assert!((parse_radius("1r", &layout_big) - 2000.0).abs() < 1e-9);
        // "0.25r" × 500 = 125; × 2000 = 500.
        assert!((parse_radius("0.25r", &layout_small) - 125.0).abs() < 1e-9);
        assert!((parse_radius("0.25r", &layout_big) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_negative_r_fraction() {
        // "-0.5r" × 1000 = -500 (no clamp).
        let layout = mk_layout(1000.0);
        assert!((parse_radius("-0.5r", &layout) - (-500.0)).abs() < 1e-9);
        // "-2r" × 1500 = -3000.
        let layout2 = mk_layout(1500.0);
        assert!((parse_radius("-2r", &layout2) - (-3000.0)).abs() < 1e-9);
    }

    #[test]
    fn test_draw_highlights_default_r0_r1_values() {
        // No r0/r1 in block_conf → defaults "0.9r" and "0.95r" applied.
        // With layout ideo_radius=1000, defaults parse to 900 and 950.
        // Test documents defaults are applied without panic.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // Group opened/closed without panic.
        assert!(doc.elements.iter().any(|e| e.contains("highlights")));
    }

    #[test]
    fn test_draw_highlights_with_stroke_thickness_parses_number() {
        // stroke_thickness="2" → .parse::<f64>() = 2.0. Must not panic.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("stroke_thickness".into(), ConfigValue::Str("2".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
    }

    #[test]
    fn test_draw_highlights_offset_trim_p_suffix() {
        // offset="100p" → trim 'p' → 100.0 parse succeeds.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("offset".into(), ConfigValue::Str("100p".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // Should not panic with p-suffix offset; group emitted.
        assert!(doc.elements.iter().any(|e| e.contains("highlights")));
    }

    #[test]
    fn test_draw_highlights_invalid_stroke_thickness_falls_back_to_zero() {
        // stroke_thickness="notanum" → parse fails → unwrap_or(0.0).
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("stroke_thickness".into(), ConfigValue::Str("notanum".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
    }

    #[test]
    fn test_draw_highlights_custom_fill_color_in_block_conf() {
        // block_conf.fill_color="green" → the default fill_name is "green".
        // With empty data, still verifies no panic and group emitted.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("fill_color".into(), ConfigValue::Str("green".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        assert!(doc.elements.iter().any(|e| e.contains("highlights")));
    }

    #[test]
    fn test_draw_highlights_url_template_with_image_map_params() {
        // block_conf.url is a URL template; image_map_missing_parameter controls
        // handling. Ensure parsing them both doesn't panic even with empty data.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("url".into(), ConfigValue::Str("/h?chr=[chr]".into()));
        block_conf.insert("image_map_missing_parameter".into(), ConfigValue::Str("removeurl".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
    }

    #[test]
    fn test_draw_highlights_stroke_color_without_stroke_thickness() {
        // stroke_color set, no stroke_thickness → defaults 0.0 (no stroke width).
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("stroke_color".into(), ConfigValue::Str("black".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
    }

    #[test]
    fn test_draw_highlights_with_custom_r0_r1_overrides() {
        // Custom r0/r1 (not defaults) → parsed without panic.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("r0".into(), ConfigValue::Str("0.5r".into()));
        block_conf.insert("r1".into(), ConfigValue::Str("0.7r".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
    }

    #[test]
    fn test_parse_radius_p_suffix_only_with_decimal() {
        // Decimal + p-suffix parses correctly.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("42.5p", &layout) - 42.5).abs() < 1e-9);
        assert!((parse_radius("0.001p", &layout) - 0.001).abs() < 1e-12);
    }

    #[test]
    fn test_parse_radius_invalid_r_with_alpha_returns_zero() {
        // "abcr" → trim_end_matches('r') = "abc" → parse fails → 0.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("abcr", &layout), 0.0);
        assert_eq!(parse_radius("xyzp", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_large_negative_r_fraction() {
        // Large negative r-fraction → scales appropriately.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("-10r", &layout) - (-10_000.0)).abs() < 1e-6);
    }

    #[test]
    fn test_draw_highlights_empty_data_emits_highlights_group_only() {
        // Empty data → group opened/closed; no slice paths emitted.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // <g id="highlights"> opens and </g> closes; no other elements.
        let opens = doc.elements.iter().filter(|e| e.contains("highlights")).count();
        let closes = doc.elements.iter().filter(|e| e.contains("</g>")).count();
        assert!(opens >= 1);
        assert!(closes >= 1);
    }

    #[test]
    fn test_parse_radius_r_with_negative_zero() {
        // -0 and -0.0 both → 0.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("-0r", &layout), 0.0);
        assert_eq!(parse_radius("-0.0r", &layout), -0.0);
    }

    #[test]
    fn test_parse_radius_scientific_notation() {
        // Scientific notation in r/p suffixes.
        let layout = mk_layout(1000.0);
        // "1e2p" → 100.
        assert!((parse_radius("1e2p", &layout) - 100.0).abs() < 1e-9);
        // "1.5e-2r" × 1000 = 15.
        assert!((parse_radius("1.5e-2r", &layout) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn test_draw_highlights_z_value_parsing_from_block() {
        // block_conf.z="3" parsed and added to z-levels. With no data, just verify no panic.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("z".into(), ConfigValue::Str("3".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        assert!(doc.elements.iter().any(|e| e.contains("highlights")));
    }

    #[test]
    fn test_parse_radius_many_digit_number() {
        // Very large digit count still parses.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("123456789p", &layout), 123456789.0);
        // With r suffix + large coefficient.
        assert!((parse_radius("1000r", &layout) - 1_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_radius_trims_surrounding_whitespace() {
        // Leading/trailing spaces stripped; inner parse proceeds normally.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("  0.9r  ", &layout) - 900.0).abs() < 1e-9);
        assert!((parse_radius("\t42p\n", &layout) - 42.0).abs() < 1e-9);
        assert!((parse_radius("   500   ", &layout) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_unparseable_number_returns_zero() {
        // parse() fails → unwrap_or(0.0); "xyzr" → trim r, "xyz" fails, → 0 × radius = 0.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("xyzr", &layout), 0.0);
        assert_eq!(parse_radius("abcp", &layout), 0.0);
        assert_eq!(parse_radius("abc", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_bare_number_no_suffix_falls_to_else_branch() {
        // No r/p suffix → else branch uses raw parse.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("500", &layout), 500.0);
        assert!((parse_radius("3.14", &layout) - 3.14).abs() < 1e-9);
        // Negative bare numbers pass through.
        assert_eq!(parse_radius("-123", &layout), -123.0);
    }

    #[test]
    fn test_parse_radius_suffix_r_negative_coefficient() {
        // Negative coefficient × positive radius → negative result.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("-0.5r", &layout) - (-500.0)).abs() < 1e-9);
        assert!((parse_radius("-2r", &layout) - (-2000.0)).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_leading_plus_sign_handled() {
        // "+0.5r" → trim_end_matches('r') → "+0.5" → parse → 0.5 → × 1000 = 500.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("+0.5r", &layout) - 500.0).abs() < 1e-9);
        // "+100p" → 100.
        assert!((parse_radius("+100p", &layout) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_decimal_without_integer_part() {
        // ".5r" → ".5".parse::<f64>() → 0.5 → × 1000 = 500.
        let layout = mk_layout(1000.0);
        assert!((parse_radius(".5r", &layout) - 500.0).abs() < 1e-9);
        // ".25p" → 0.25.
        assert!((parse_radius(".25p", &layout) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_scientific_notation_in_r_suffix() {
        // "1e2r" → parse 1e2 = 100 → × 1000 = 100_000.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("1e2r", &layout) - 100_000.0).abs() < 1e-6);
        // "1.5e-1r" → 0.15 × 1000 = 150.
        assert!((parse_radius("1.5e-1r", &layout) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn test_draw_highlights_empty_data_emits_only_group_wrapper() {
        // With no Datum entries, output contains the highlights group but no paths.
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // No <path ...> elements (no data → no slices).
        assert!(!doc.elements.iter().any(|e| e.contains("<path ")));
    }

    #[test]
    fn test_parse_radius_bare_suffix_only_returns_zero() {
        // "r" alone → trim suffix → "" → unwrap_or(0.0) → 0 * radius = 0.
        let layout = mk_layout(1000.0);
        assert_eq!(parse_radius("r", &layout), 0.0);
        // "p" alone → 0.
        assert_eq!(parse_radius("p", &layout), 0.0);
    }

    #[test]
    fn test_draw_highlights_emits_highlights_group_wrapper_unconditionally() {
        // `doc.open_group("highlights")` is unconditional — even empty data → group opened.
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        assert!(doc
            .elements
            .iter()
            .any(|e| e.contains(r#"<g id="highlights">"#)));
        assert!(doc.elements.iter().any(|e| e == "</g>"));
    }

    #[test]
    fn test_parse_radius_mixed_whitespace_trim_before_parse() {
        // Tab/newline padding all stripped via trim() before suffix check.
        let layout = mk_layout(1000.0);
        assert!((parse_radius("\t\n 0.5r \t\n", &layout) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_fractional_r_coefficient_precision() {
        // Multiple decimal places preserved through f64 arithmetic.
        let layout = mk_layout(1000.0);
        let r = parse_radius("0.123456r", &layout);
        assert!((r - 123.456).abs() < 1e-9);
    }

    #[test]
    fn test_parse_radius_very_small_fractional_r_preserved() {
        // Tiny coefficients survive the multiply: 1e-10 × 1000 = 1e-7.
        let layout = mk_layout(1000.0);
        let r = parse_radius("1e-10r", &layout);
        assert!((r - 1e-7).abs() < 1e-15);
    }

    #[test]
    fn test_parse_radius_unit_r_coefficient_exactly_one_yields_image_radius() {
        // "1r" → 1 × ideogram_radius = exact ideogram_radius (no drift).
        let layout = mk_layout(1234.5);
        let r = parse_radius("1r", &layout);
        assert_eq!(r, 1234.5);
    }

    #[test]
    fn test_parse_radius_very_large_p_value_preserved() {
        // Extreme pixel values pass through without loss.
        let layout = mk_layout(1000.0);
        let r = parse_radius("1e15p", &layout);
        assert_eq!(r, 1e15);
    }

    #[test]
    fn test_draw_highlights_block_with_z_value_set_renders_without_panic() {
        // block_conf with a z override + empty data → no panic, group emitted.
        let layout = mk_layout(1000.0);
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("z".into(), ConfigValue::Str("5".into()));
        draw_highlights(&mut doc, &layout, &[], &block_conf, &colors);
        // No data → no path; but highlights group opened.
        assert!(doc
            .elements
            .iter()
            .any(|e| e.contains(r#"<g id="highlights">"#)));
    }
}
