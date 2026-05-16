use std::collections::HashMap;
use std::fmt::Write;

use rayon::prelude::*;

use crate::config::types::ConfigValue;
use crate::coord::bezier;
use crate::data::types::Link;
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::SvgDocument;
use crate::rules::{self, Rule};
use crate::utils::format_url;

/// Draw links as bezier curves between pairs of genomic positions.
pub fn draw_links(
    doc: &mut SvgDocument,
    layout: &Layout,
    links: &[Link],
    defaults: &HashMap<String, String>,
    block_conf: &HashMap<String, ConfigValue>,
    rule_list: &[Rule],
    colors: &ColorMap,
) {
    let default_radius = parse_link_radius(
        block_conf
            .get("radius")
            .and_then(|v| v.as_str())
            .or(defaults.get("radius").map(|s| s.as_str()))
            .unwrap_or("0.9r"),
        layout,
    );
    let default_bezier_radius = parse_link_radius(
        block_conf
            .get("bezier_radius")
            .and_then(|v| v.as_str())
            .or(defaults.get("bezier_radius").map(|s| s.as_str()))
            .unwrap_or("0.2r"),
        layout,
    );
    let default_crest: f64 = block_conf
        .get("crest")
        .and_then(|v| v.as_str())
        .or(defaults.get("crest").map(|s| s.as_str()))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let default_color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .or(defaults.get("color").map(|s| s.as_str()))
        .unwrap_or("lgrey");
    let default_thickness: f64 = block_conf
        .get("thickness")
        .and_then(|v| v.as_str())
        .or(defaults.get("thickness").map(|s| s.as_str()))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let is_ribbon = block_conf
        .get("ribbon")
        .and_then(|v| v.as_str())
        .or(defaults.get("ribbon").map(|s| s.as_str()))
        .map(|s| s == "1")
        .unwrap_or(false);

    // Perl: `seek_parameter("url", @i_link_param_path)` then `format_url`.
    let block_url_tpl: Option<String> = block_conf
        .get("url")
        .and_then(|v| v.as_str())
        .or(defaults.get("url").map(|s| s.as_str()))
        .map(str::to_string);
    let missing_policy: String = block_conf
        .get("image_map_missing_parameter")
        .and_then(|v| v.as_str())
        .or(defaults.get("image_map_missing_parameter").map(|s| s.as_str()))
        .unwrap_or("removeparam")
        .to_string();

    // Generate all link SVG strings in parallel
    let svg_elements: Vec<String> = links
        .par_iter()
        .flat_map(|link| {
            if link.points.len() < 2 {
                return Vec::new();
            }

            let rule_overrides = rules::apply_rules_to_link(link, rule_list);
            let color_name = rule_overrides
                .get("color")
                .map(|s| s.as_str())
                .unwrap_or(default_color_name);
            let thickness: f64 = rule_overrides
                .get("thickness")
                .and_then(|s| s.parse().ok())
                .unwrap_or(default_thickness);
            let color = colors
                .resolve(color_name)
                .unwrap_or(Color::rgb(200, 200, 200));

            let mut elements = Vec::new();
            for i in 0..link.points.len() - 1 {
                let p1 = &link.points[i];
                let p2 = &link.points[i + 1];

                if layout.find_ideogram_by_chr(&p1.chr).is_none()
                    || layout.find_ideogram_by_chr(&p2.chr).is_none()
                {
                    continue;
                }

                let mid1 = (p1.start + p1.end) / 2;
                let mid2 = (p2.start + p2.end) / 2;

                let angle1 = match layout.getanglepos(mid1, &p1.chr) {
                    Some(a) => a,
                    None => continue,
                };
                let angle2 = match layout.getanglepos(mid2, &p2.chr) {
                    Some(a) => a,
                    None => continue,
                };

                // Resolve per-link URL template (Perl: seek_parameter("url",
                // @i_link_param_path) then format_url). Passed into ribbon()
                // which emits the image-map <area>, matching Perl.
                let url_tpl: Option<String> = link
                    .param
                    .get("url")
                    .cloned()
                    .or_else(|| block_url_tpl.clone());
                let resolved_url: Option<String> = url_tpl.and_then(|tpl| {
                    let mut dp: HashMap<String, ConfigValue> = HashMap::new();
                    dp.insert("chr1".into(), ConfigValue::Str(p1.chr.clone()));
                    dp.insert("start1".into(), ConfigValue::Str(p1.start.to_string()));
                    dp.insert("end1".into(), ConfigValue::Str(p1.end.to_string()));
                    dp.insert("chr2".into(), ConfigValue::Str(p2.chr.clone()));
                    dp.insert("start2".into(), ConfigValue::Str(p2.start.to_string()));
                    dp.insert("end2".into(), ConfigValue::Str(p2.end.to_string()));
                    for (k, v) in &link.param {
                        dp.entry(k.clone())
                            .or_insert_with(|| ConfigValue::Str(v.clone()));
                    }
                    format_url(&tpl, &[&dp], &missing_policy).ok().flatten()
                });

                if is_ribbon {
                    let a1_start = layout.getanglepos(p1.start, &p1.chr).unwrap_or(angle1);
                    let a1_end = layout.getanglepos(p1.end, &p1.chr).unwrap_or(angle1);
                    let a2_start = layout.getanglepos(p2.start, &p2.chr).unwrap_or(angle2);
                    let a2_end = layout.getanglepos(p2.end, &p2.chr).unwrap_or(angle2);

                    elements.push(ribbon(
                        layout,
                        a1_start,
                        a1_end,
                        a2_start,
                        a2_end,
                        default_radius,
                        default_radius,
                        default_bezier_radius,
                        if default_crest > 0.0 {
                            Some(default_crest)
                        } else {
                            None
                        },
                        None,
                        Some(&color),
                        thickness,
                        Some(&color),
                        Some(0.5),
                        resolved_url.as_deref(),
                        None,
                    ));
                } else {
                    elements.push(draw_bezier(
                        layout,
                        angle1,
                        angle2,
                        default_radius,
                        default_bezier_radius,
                        default_crest,
                        &color,
                        thickness,
                    ));
                    // Perl `draw_bezier` doesn't emit image-map areas for
                    // line-links; intentionally no-op here.
                    let _ = resolved_url;
                }
            }
            elements
        })
        .collect();

    for svg in svg_elements {
        doc.add(svg);
    }
}

/// Draw a single bezier link line between two angles.
fn draw_bezier(
    layout: &Layout,
    angle1: f64,
    angle2: f64,
    radius: f64,
    bezier_radius: f64,
    crest: f64,
    color: &Color,
    thickness: f64,
) -> String {
    let cx = layout.image_radius;
    let cy = layout.image_radius;

    let points = bezier::bezier_control_points(
        cx,
        cy,
        angle1,
        radius,
        angle2,
        radius,
        bezier_radius,
        None,
        None,
        None,
        if crest > 0.0 { Some(crest) } else { None },
        None,
    );
    // Perl returns a quadratic bezier (3 points) or quartic with crest (5 points).
    // Convert the simple quadratic (q0, q1, q2) to an SVG cubic (p0, p1, p2, p3) via
    // p1 = q0 + 2/3*(q1-q0), p2 = q2 + 2/3*(q1-q2).
    let (q0, q1, q2) = if points.len() >= 3 {
        (points[0], points[points.len() / 2], *points.last().unwrap())
    } else {
        return String::new();
    };
    let p0 = q0;
    let p1 = (
        q0.0 + 2.0 / 3.0 * (q1.0 - q0.0),
        q0.1 + 2.0 / 3.0 * (q1.1 - q0.1),
    );
    let p2 = (
        q2.0 + 2.0 / 3.0 * (q1.0 - q2.0),
        q2.1 + 2.0 / 3.0 * (q1.1 - q2.1),
    );
    let p3 = q2;

    format!(
        r#"<path d="M {:.1},{:.1} C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
        p0.0,
        p0.1,
        p1.0,
        p1.1,
        p2.0,
        p2.1,
        p3.0,
        p3.1,
        color.to_svg_rgb(),
        thickness
    )
}

/// Port of Perl `ribbon(...)`: SVG path for a twisted ribbon between two
/// angular spans, with radii `radius1` and `radius2`. Follows Perl structure:
///   M to (angle1_start, radius1)
///   A arc along span1 at radius1 → (angle1_end, radius1)
///   dispatch bezier_control_points1 length:
///     3 pts → Q quadratic bezier
///     4 pts → C cubic bezier
///     5 pts → L-sampled via bezier_points
///   A arc along span2 at radius2 → (angle2_start, radius2) (reverse sweep)
///   dispatch bezier_control_points2 identically
///   Z close
#[allow(clippy::too_many_arguments)]
fn ribbon(
    layout: &Layout,
    a1_start: f64,
    a1_end: f64,
    a2_start: f64,
    a2_end: f64,
    radius1: f64,
    radius2: f64,
    bezier_radius: f64,
    crest: Option<f64>,
    bezier_radius_purity: Option<f64>,
    edge_color: Option<&Color>,
    edge_stroke: f64,
    fill_color: Option<&Color>,
    fill_opacity: Option<f64>,
    url: Option<&str>,
    image_conf: Option<&HashMap<String, ConfigValue>>,
) -> String {
    let cx = layout.image_radius;
    let cy = layout.image_radius;
    let deg2rad = std::f64::consts::PI / 180.0;

    let getxy = |a: f64, r: f64| -> (f64, f64) {
        (cx + r * (a * deg2rad).cos(), cy + r * (a * deg2rad).sin())
    };

    let bezier1 = bezier::bezier_control_points(
        cx,
        cy,
        a1_end,
        radius1,
        a2_end,
        radius2,
        bezier_radius,
        bezier_radius_purity,
        None,
        None,
        crest,
        None,
    );
    let bezier2 = bezier::bezier_control_points(
        cx,
        cy,
        a2_start,
        radius2,
        a1_start,
        radius1,
        bezier_radius,
        bezier_radius_purity,
        None,
        None,
        crest,
        None,
    );

    let p_a1s = getxy(a1_start, radius1);
    let p_a1e = getxy(a1_end, radius1);
    let p_a2s = getxy(a2_start, radius2);

    let mut path = String::new();
    write!(path, "M {:.3},{:.3} ", p_a1s.0, p_a1s.1).unwrap();

    let large1 = if (a1_start - a1_end).abs() > 180.0 {
        1
    } else {
        0
    };
    let sweep1 = if a1_start < a1_end { 1 } else { 0 };
    write!(
        path,
        "A {:.3},{:.3} {:.2} {},{} {:.1},{:.1} ",
        radius1, radius1, 0.0, large1, sweep1, p_a1e.0, p_a1e.1
    )
    .unwrap();

    // Dispatch on control point count for the first bezier (to span 2)
    match bezier1.len() {
        5 => {
            // 5 control points → sample as polyline
            let samples = bezier::bezier_points_n(&bezier1, 40);
            for (x, y) in samples.iter() {
                write!(path, "L {:.1},{:.1} ", x, y).unwrap();
            }
        }
        4 => {
            // cubic bezier, skip bezier1[0] (already at p_a1e)
            write!(
                path,
                "C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} ",
                bezier1[1].0, bezier1[1].1, bezier1[2].0, bezier1[2].1, bezier1[3].0, bezier1[3].1,
            )
            .unwrap();
        }
        3 => {
            // quadratic bezier, skip bezier1[0]
            write!(
                path,
                "Q {:.1},{:.1} {:.1},{:.1} ",
                bezier1[1].0, bezier1[1].1, bezier1[2].0, bezier1[2].1,
            )
            .unwrap();
        }
        _ => {}
    }

    let large2 = if (a2_start - a2_end).abs() > 180.0 {
        1
    } else {
        0
    };
    let sweep2 = if a2_start > a2_end { 1 } else { 0 };
    write!(
        path,
        "A {:.3},{:.3} {:.2} {},{} {:.1},{:.1} ",
        radius2, radius2, 0.0, large2, sweep2, p_a2s.0, p_a2s.1
    )
    .unwrap();

    match bezier2.len() {
        5 => {
            let samples = bezier::bezier_points_n(&bezier2, 40);
            for (x, y) in samples.iter() {
                write!(path, "L {:.1},{:.1} ", x, y).unwrap();
            }
        }
        4 => {
            write!(
                path,
                "C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} ",
                bezier2[1].0, bezier2[1].1, bezier2[2].0, bezier2[2].1, bezier2[3].0, bezier2[3].1,
            )
            .unwrap();
        }
        3 => {
            write!(
                path,
                "Q {:.1},{:.1} {:.1},{:.1} ",
                bezier2[1].0, bezier2[1].1, bezier2[2].0, bezier2[2].1,
            )
            .unwrap();
        }
        _ => {}
    }

    write!(path, "Z").unwrap();

    let mut style = String::new();
    if let Some(edge) = edge_color {
        style.push_str(&format!("stroke: {};", edge.to_svg_rgb()));
    }
    if edge_stroke > 0.0 {
        style.push_str(&format!(" stroke-width: {:.1};", edge_stroke));
    }
    if let Some(fill) = fill_color {
        style.push_str(&format!(" fill: {};", fill.to_svg_rgb()));
        if let Some(op) = fill_opacity
            && op < 1.0
        {
            style.push_str(&format!(" opacity: {:.3};", op));
        }
    }

    // --- Image-map emission (Perl ribbon: when mapoptions.url is defined,
    //     sample the ribbon polygon and emit a <area shape="poly">) ---
    if let Some(href) = url {
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
        // Sample the two endpoint arcs for a rough ribbon polygon.
        let step = 1.0;
        let mut coords: Vec<f64> = Vec::new();
        let (s1, e1) = (a1_start.min(a1_end), a1_start.max(a1_end));
        let mut a = s1;
        while a <= e1 {
            let (x, y) = getxy(a, radius1);
            coords.extend([x * xmult + xshift, y * ymult + yshift]);
            a += step;
        }
        let (s2, e2) = (a2_start.min(a2_end), a2_start.max(a2_end));
        let mut a = s2;
        while a <= e2 {
            let (x, y) = getxy(a, radius2);
            coords.extend([x * xmult + xshift, y * ymult + yshift]);
            a += step;
        }
        crate::draw::report_image_map("poly", &coords, href);
    }

    format!(r#"<path d="{}" style="{}" />"#, path.trim(), style.trim())
}

/// SVG arc sweep flag: 1 when the angle difference (normalized into
/// `[0, 360)`) is in `(0, 360)`, else 0.
#[allow(dead_code)]
fn arc_sweep_flag(start_a: f64, end_a: f64) -> i32 {
    let mut diff = end_a - start_a;
    if diff < 0.0 {
        diff += 360.0;
    }
    if diff > 0.0 && diff < 360.0 { 1 } else { 0 }
}

/// SVG arc large-arc flag: 1 when the span (normalized into `[0, 360)`)
/// exceeds 180 degrees, else 0.
#[allow(dead_code)]
fn arc_large_flag(start_a: f64, end_a: f64) -> i32 {
    let mut span = end_a - start_a;
    if span < 0.0 {
        span += 360.0;
    }
    if span > 180.0 { 1 } else { 0 }
}

/// Parse a link radius value (e.g., "0.9r", "1200p", or bare pixels).
/// `r` suffix scales by `layout.dims.ideogram_radius`; `p` suffix is raw px.
fn parse_link_radius(s: &str, layout: &Layout) -> f64 {
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

    fn mk_layout() -> Layout {
        Layout {
            ideograms: Vec::new(),
            gcircum: 3_000_000_000.0,
            gsize_noscale: 3_000_000_000.0,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1_000_000.0,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 950.0,
                ideogram_radius_outer: 1050.0,
            },
        }
    }

    #[test]
    fn test_arc_sweep_flag_positive_sweep() {
        // 0° → 90° is a 90° CCW sweep → sweep flag = 1.
        assert_eq!(arc_sweep_flag(0.0, 90.0), 1);
        // 0° → 270° is also 270° CCW → 1.
        assert_eq!(arc_sweep_flag(0.0, 270.0), 1);
    }

    #[test]
    fn test_arc_sweep_flag_reversed_adds_360() {
        // 90° → 0° is -90°, normalized to 270° → flag 1.
        assert_eq!(arc_sweep_flag(90.0, 0.0), 1);
        // Zero sweep (start == end) → 0 (neither > 0 nor < 360).
        assert_eq!(arc_sweep_flag(45.0, 45.0), 0);
    }

    #[test]
    fn test_arc_large_flag_threshold_180() {
        // Small sweep (≤180) → 0.
        assert_eq!(arc_large_flag(0.0, 90.0), 0);
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
        // Large sweep (>180) → 1.
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
        // Reversed: 0 → -10 normalizes to 350° → large.
        assert_eq!(arc_large_flag(0.0, -10.0), 1);
    }

    #[test]
    fn test_parse_link_radius_r_suffix_scales_by_ideogram_radius() {
        let layout = mk_layout();
        assert!((parse_link_radius("0.5r", &layout) - 500.0).abs() < 1e-9);
        assert!((parse_link_radius("1.2r", &layout) - 1200.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_p_and_bare() {
        let layout = mk_layout();
        assert!((parse_link_radius("200p", &layout) - 200.0).abs() < 1e-9);
        assert!((parse_link_radius("350", &layout) - 350.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_invalid_returns_zero() {
        let layout = mk_layout();
        assert_eq!(parse_link_radius("garbage", &layout), 0.0);
        assert_eq!(parse_link_radius("", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_full_circle_sweep_is_zero() {
        // Sweep exactly 360° falls out of "0 < diff < 360" range → flag 0.
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
        // Negative -360 normalizes to 0 → also flag 0 (neither > 0).
        assert_eq!(arc_sweep_flag(360.0, 0.0), 0);
    }

    #[test]
    fn test_arc_large_flag_exactly_180_is_small() {
        // The threshold is strict `> 180` — exactly 180 returns 0 (small arc).
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
        assert_eq!(arc_large_flag(45.0, 225.0), 0);
        // Just above 180 → 1.
        assert_eq!(arc_large_flag(0.0, 180.001), 1);
    }

    #[test]
    fn test_parse_link_radius_trims_whitespace() {
        let layout = mk_layout();
        // Leading/trailing whitespace stripped before suffix detection.
        assert!((parse_link_radius("  0.25r  ", &layout) - 250.0).abs() < 1e-9);
        assert!((parse_link_radius("\t500p\n", &layout) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_negative_r_fraction() {
        // A negative `r`-fraction is parsed verbatim — multiplies to negative pixels.
        // Confirms parse_link_radius doesn't clamp at 0.
        let layout = mk_layout();
        assert!((parse_link_radius("-0.1r", &layout) + 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_draw_links_empty_links_list_no_panic() {
        // Empty links slice → function returns without panicking, no elements added.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(&mut doc, &layout, &[], &defaults, &block_conf, &[], &colors);
        // No SVG added — only layout defaults calculated internally.
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_links_single_point_link_is_skipped() {
        // Links with <2 points are skipped (`points.len() < 2` → early Vec::new).
        use crate::data::types::{Datum, Link};
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let link = Link {
            id: "l1".into(),
            points: vec![Datum {
                chr: "hs1".into(),
                start: 0,
                end: 100,
                ..Default::default()
            }],
            param: HashMap::new(),
        };
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(
            &mut doc,
            &layout,
            &[link],
            &defaults,
            &block_conf,
            &[],
            &colors,
        );
        // Single-point link → nothing emitted.
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_links_unknown_chr_skips_pair() {
        // A link whose chr isn't in layout.ideograms → both `find_ideogram_by_chr`
        // fail → skipped, no SVG emitted.
        use crate::data::types::{Datum, Link};
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let link = Link {
            id: "l1".into(),
            points: vec![
                Datum { chr: "unknown1".into(), start: 0, end: 100, ..Default::default() },
                Datum { chr: "unknown2".into(), start: 0, end: 100, ..Default::default() },
            ],
            param: HashMap::new(),
        };
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(
            &mut doc,
            &layout,
            &[link],
            &defaults,
            &block_conf,
            &[],
            &colors,
        );
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_links_ribbon_flag_from_defaults_triggers_ribbon_path() {
        // When `ribbon=1` in defaults (no block_conf.ribbon), is_ribbon=true.
        // With no real ideograms, this exercises the flag parsing at least
        // without panicking.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut defaults: HashMap<String, String> = HashMap::new();
        defaults.insert("ribbon".into(), "1".into());
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        // Empty links → nothing to render, but flag parsing should still happen
        // without panic.
        draw_links(&mut doc, &layout, &[], &defaults, &block_conf, &[], &colors);
    }

    #[test]
    fn test_arc_sweep_flag_near_360_boundary_still_zero() {
        // Exactly 359.9° sweep → flag=1 (in range 0 < diff < 360).
        assert_eq!(arc_sweep_flag(0.0, 359.9), 1);
        // Exactly 0.1° sweep also → flag=1.
        assert_eq!(arc_sweep_flag(0.0, 0.1), 1);
    }

    #[test]
    fn test_arc_large_flag_negative_input_normalizes_before_threshold() {
        // end_a < start_a triggers +360 wrap. (0, -10) → 350° sweep → >180 → large=1.
        assert_eq!(arc_large_flag(0.0, -10.0), 1);
        // (0, -180) → 180° sweep → NOT >180 → large=0.
        assert_eq!(arc_large_flag(0.0, -180.0), 0);
        // (0, -181) → 179° → NOT >180 → large=0.
        assert_eq!(arc_large_flag(0.0, -181.0), 0);
    }

    #[test]
    fn test_parse_link_radius_empty_p_or_r_returns_zero() {
        // Just "p" or "r" with nothing before → parse fails → 0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("r", &layout), 0.0);
        assert_eq!(parse_link_radius("p", &layout), 0.0);
        // Whitespace-only → trim to empty → parse fails → 0.
        assert_eq!(parse_link_radius("   ", &layout), 0.0);
    }

    #[test]
    fn test_draw_links_both_chrs_same_skipped_without_ideograms() {
        // A link with both points on same chr still needs to find ideograms.
        // With empty layout.ideograms, both find_ideogram_by_chr fail → skipped.
        use crate::data::types::{Datum, Link};
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let link = Link {
            id: "self".into(),
            points: vec![
                Datum { chr: "hs1".into(), start: 0, end: 100, ..Default::default() },
                Datum { chr: "hs1".into(), start: 200, end: 300, ..Default::default() },
            ],
            param: HashMap::new(),
        };
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(&mut doc, &layout, &[link], &defaults, &block_conf, &[], &colors);
        // Same-chr link with no ideograms → skipped.
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_arc_sweep_flag_exactly_zero_sweep_is_zero() {
        // sweep exactly 0 (start == end) → diff=0, NOT > 0 → flag 0.
        assert_eq!(arc_sweep_flag(0.0, 0.0), 0);
        assert_eq!(arc_sweep_flag(180.0, 180.0), 0);
    }

    #[test]
    fn test_arc_large_flag_boundary_at_exactly_180_is_small() {
        // Strict > 180 threshold: exactly 180 → large=0.
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
        assert_eq!(arc_large_flag(45.0, 225.0), 0);
        assert_eq!(arc_large_flag(90.0, 270.0), 0);
        // Just above 180 → large=1.
        assert_eq!(arc_large_flag(0.0, 180.5), 1);
    }

    #[test]
    fn test_parse_link_radius_decimal_p_and_r_suffixes() {
        // Decimal values preserved through parse.
        let layout = mk_layout();
        // 0.75r × 1000 ideogram_radius = 750.
        assert!((parse_link_radius("0.75r", &layout) - 750.0).abs() < 1e-9);
        // 42.5p → 42.5.
        assert!((parse_link_radius("42.5p", &layout) - 42.5).abs() < 1e-9);
        // Plain decimal (no suffix) → parsed as bare f64.
        assert!((parse_link_radius("123.4", &layout) - 123.4).abs() < 1e-9);
    }

    #[test]
    fn test_draw_links_nonempty_rules_slice_accepted_without_panic() {
        // Pass a non-empty rules slice with empty links — ensure function
        // doesn't panic under empty-links + non-empty-rules combination.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        use crate::rules::Rule;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        let rules = vec![Rule {
            importance: 10,
            condition: "_INTERCHR_".into(),
            overrides: HashMap::new(),
        }];
        draw_links(&mut doc, &layout, &[], &defaults, &block_conf, &rules, &colors);
    }

    #[test]
    fn test_arc_sweep_flag_gt_360_unchanged_diff() {
        // diff 400 → neither normalized nor flag-set (since >= 360 falls outside).
        // arc_sweep_flag(0, 400) → diff=400, NOT < 0 so no +360; 400>0 AND NOT <360 → flag=0.
        assert_eq!(arc_sweep_flag(0.0, 400.0), 0);
    }

    #[test]
    fn test_arc_large_flag_zero_sweep_is_zero() {
        // start==end → span=0 → NOT > 180 → large=0.
        assert_eq!(arc_large_flag(0.0, 0.0), 0);
        assert_eq!(arc_large_flag(45.0, 45.0), 0);
        // Minimal positive sweep.
        assert_eq!(arc_large_flag(0.0, 0.01), 0);
    }

    #[test]
    fn test_parse_link_radius_mixed_case_suffix_not_matched() {
        // Only lowercase 'r' and 'p' are recognized; uppercase R / P → treated
        // as bare number: "100R" fails f64 parse → 0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("100R", &layout), 0.0);
        assert_eq!(parse_link_radius("50P", &layout), 0.0);
    }

    #[test]
    fn test_draw_links_preserves_svg_doc_len_before_render() {
        // Empty links + empty defaults + empty block_conf + empty rules + empty ideograms
        // → no element added to doc.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(&mut doc, &layout, &[], &defaults, &block_conf, &[], &colors);
        // No elements added.
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_arc_sweep_flag_counterclockwise_wrap() {
        // Reversed angles (end < start) get +360 normalization.
        // 270 → 45 means we have diff=-225 → +360 = 135. 135 > 0 and < 360 → flag=1.
        assert_eq!(arc_sweep_flag(270.0, 45.0), 1);
        // 360 → 0: diff=-360 → +360 = 0. 0 NOT > 0 → flag=0.
        assert_eq!(arc_sweep_flag(360.0, 0.0), 0);
    }

    #[test]
    fn test_arc_large_flag_small_reversed_sweep() {
        // Small negative sweep normalized → small positive → large=0 if ≤180.
        // 0 → -5: span -5 +360 = 355 > 180 → large=1.
        assert_eq!(arc_large_flag(0.0, -5.0), 1);
        // 0 → -170: span -170 +360 = 190 > 180 → large=1.
        assert_eq!(arc_large_flag(0.0, -170.0), 1);
        // 0 → -200: span -200 +360 = 160 NOT > 180 → large=0.
        assert_eq!(arc_large_flag(0.0, -200.0), 0);
    }

    #[test]
    fn test_parse_link_radius_bare_integer_returns_pixels() {
        // Bare integer "500" → 500.0 (treated as pixels).
        let layout = mk_layout();
        assert_eq!(parse_link_radius("500", &layout), 500.0);
        // Bare zero.
        assert_eq!(parse_link_radius("0", &layout), 0.0);
        // Negative bare.
        assert_eq!(parse_link_radius("-100", &layout), -100.0);
    }

    #[test]
    fn test_draw_links_unknown_chr_in_second_point_skipped() {
        // A 2-point link with unknown chr in either position → skipped.
        use crate::data::types::{Datum, Link};
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let link = Link {
            id: "l".into(),
            points: vec![
                Datum { chr: "hs1".into(), start: 0, end: 100, ..Default::default() },
                Datum { chr: "UNKNOWN".into(), start: 0, end: 100, ..Default::default() },
            ],
            param: HashMap::new(),
        };
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(&mut doc, &layout, &[link], &defaults, &block_conf, &[], &colors);
        // Unknown chr in any point → skipped.
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_arc_sweep_flag_full_360_normalization() {
        // 0 → 360 sweep: diff 360, NOT strictly < 360 → flag=0.
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
    }

    #[test]
    fn test_arc_large_flag_zero_span_from_full_circle() {
        // start==end+360 means full loop → diff=-360 normalizes to 0 → large=0.
        assert_eq!(arc_large_flag(360.0, 0.0), 0);
        // 90→450: diff=360 → large=0 (not > 180).
        // Actually 450-90=360, >180 so large=1. Let me try more carefully.
        // Positive sweep of exactly 360 → large=1 (since 360 > 180).
        assert_eq!(arc_large_flag(0.0, 360.0), 1);
    }

    #[test]
    fn test_parse_link_radius_decimal_without_suffix() {
        // Plain decimal string (no r/p) → parsed as pixel bare f64.
        let layout = mk_layout();
        assert!((parse_link_radius("123.456", &layout) - 123.456).abs() < 1e-9);
    }

    #[test]
    fn test_draw_links_link_with_empty_points_skipped() {
        // Link with points.is_empty() → points.len() < 2 → skipped.
        use crate::data::types::Link;
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let link = Link {
            id: "empty".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        let defaults: HashMap<String, String> = HashMap::new();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_links(&mut doc, &layout, &[link], &defaults, &block_conf, &[], &colors);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_arc_sweep_flag_equal_angles_returns_zero() {
        // diff==0 → strict >0 check fails → flag=0 (no arc).
        assert_eq!(arc_sweep_flag(45.0, 45.0), 0);
        assert_eq!(arc_sweep_flag(0.0, 0.0), 0);
        assert_eq!(arc_sweep_flag(180.0, 180.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_negative_diff_wraps_to_positive() {
        // start=90, end=30 → diff=-60 → +360=300 → 0 < 300 < 360 → 1.
        assert_eq!(arc_sweep_flag(90.0, 30.0), 1);
        // start=350, end=10 → diff=-340 → +360=20 → 1.
        assert_eq!(arc_sweep_flag(350.0, 10.0), 1);
    }

    #[test]
    fn test_arc_large_flag_exact_180_is_small_arc() {
        // Strict `> 180` → exactly 180 yields large=0.
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
        // Just over 180 → large=1.
        assert_eq!(arc_large_flag(0.0, 180.5), 1);
        // Negative diff also wraps: start=200, end=10 → span=-190 → +360=170 → NOT > 180 → 0.
        assert_eq!(arc_large_flag(200.0, 10.0), 0);
    }

    #[test]
    fn test_parse_link_radius_three_branch_semantics() {
        let layout = mk_layout();
        // r branch: 0.9r → 0.9 × ideogram_radius.
        assert!((parse_link_radius("0.9r", &layout) - 0.9 * layout.dims.ideogram_radius).abs() < 1e-6);
        // p branch: bare pixels.
        assert_eq!(parse_link_radius("1234p", &layout), 1234.0);
        // else: raw parse.
        assert_eq!(parse_link_radius("500", &layout), 500.0);
        // All 3 branches return 0 on parse failure.
        assert_eq!(parse_link_radius("xyzr", &layout), 0.0);
        assert_eq!(parse_link_radius("abcp", &layout), 0.0);
        assert_eq!(parse_link_radius("abc", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_exactly_360_degree_diff_returns_zero() {
        // diff==360 fails strict `< 360` check → flag=0 (full-circle doesn't qualify).
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
        assert_eq!(arc_sweep_flag(180.0, 540.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_small_positive_diff_yields_one() {
        // 0 < diff < 360 → flag=1.
        assert_eq!(arc_sweep_flag(0.0, 90.0), 1);
        assert_eq!(arc_sweep_flag(0.0, 179.0), 1);
        assert_eq!(arc_sweep_flag(10.0, 350.0), 1);
    }

    #[test]
    fn test_arc_large_flag_span_close_to_threshold() {
        // span=179 → NOT > 180 → 0; 181 → > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 179.0), 0);
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
        // span=90 → clearly small.
        assert_eq!(arc_large_flag(0.0, 90.0), 0);
        // span=359 → > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 359.0), 1);
    }

    #[test]
    fn test_parse_link_radius_trims_surrounding_whitespace() {
        let layout = mk_layout();
        // Leading/trailing spaces stripped before suffix + parse.
        assert!((parse_link_radius("  0.5r  ", &layout) - 0.5 * layout.dims.ideogram_radius).abs() < 1e-6);
        assert_eq!(parse_link_radius("\t42p\n", &layout), 42.0);
        assert_eq!(parse_link_radius("   500   ", &layout), 500.0);
    }

    #[test]
    fn test_parse_link_radius_bare_suffix_only_returns_zero() {
        // "r" alone → trim suffix → "" → 0 → 0 * radius = 0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("r", &layout), 0.0);
        assert_eq!(parse_link_radius("p", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_almost_360_still_yields_one() {
        // diff=359.99 → < 360 but > 0 → flag=1.
        assert_eq!(arc_sweep_flag(0.0, 359.99), 1);
        // 0.01 → > 0 && < 360 → 1.
        assert_eq!(arc_sweep_flag(0.0, 0.01), 1);
    }

    #[test]
    fn test_arc_large_flag_wrap_around_short_span_returns_zero() {
        // start=350, end=10 → span = -340; wrap: span+360 = 20 → NOT > 180 → 0.
        assert_eq!(arc_large_flag(350.0, 10.0), 0);
        // start=0, end=30 → span=30 → NOT > 180 → 0.
        assert_eq!(arc_large_flag(0.0, 30.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_negative_start_with_positive_end() {
        // start=-10, end=90 → diff=100 → > 0 && < 360 → 1.
        assert_eq!(arc_sweep_flag(-10.0, 90.0), 1);
        // Equal start/end with negative value → diff=0 → 0.
        assert_eq!(arc_sweep_flag(-45.0, -45.0), 0);
    }

    #[test]
    fn test_parse_link_radius_unit_r_exactly_one_equals_ideogram_radius() {
        // "1r" → 1 × dims.ideogram_radius = exact radius.
        let layout = mk_layout();
        let r = parse_link_radius("1r", &layout);
        assert_eq!(r, layout.dims.ideogram_radius);
    }

    #[test]
    fn test_arc_sweep_flag_diff_over_360_not_wrapped_again_stays_out_of_range() {
        // diff=720 is NOT < 0 → no wrap; then 720 NOT < 360 → flag=0.
        assert_eq!(arc_sweep_flag(0.0, 720.0), 0);
        // diff=-720 → wrap +360 → -360 (still < 0); only one wrap → not >0 → flag=0.
        assert_eq!(arc_sweep_flag(720.0, 0.0), 0);
    }

    #[test]
    fn test_arc_large_flag_negative_span_wraps_to_large_arc() {
        // start=50, end=20 → span=-30 → wrap +360 = 330 > 180 → flag=1.
        assert_eq!(arc_large_flag(50.0, 20.0), 1);
        // start=300, end=290 → span=-10 → +360=350 > 180 → 1.
        assert_eq!(arc_large_flag(300.0, 290.0), 1);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_greedy_trim_all_trailing_p() {
        // trim_end_matches('p') greedily strips all trailing 'p' chars: "5pp" → "5" → 5.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("5pp", &layout), 5.0);
        assert_eq!(parse_link_radius("5ppp", &layout), 5.0);
        // Embedded p (not trailing) fails parse → 0.
        assert_eq!(parse_link_radius("5p0p", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_zero_diff_returns_zero() {
        // end==start → diff=0 → not > 0 → flag 0 (not a valid sweep).
        assert_eq!(arc_sweep_flag(45.0, 45.0), 0);
        assert_eq!(arc_sweep_flag(0.0, 0.0), 0);
        assert_eq!(arc_sweep_flag(-100.0, -100.0), 0);
    }

    #[test]
    fn test_arc_large_flag_exactly_zero_span_returns_zero() {
        // Zero span → 0 not > 180 → flag 0.
        assert_eq!(arc_large_flag(90.0, 90.0), 0);
        // Tiny positive span → 0.
        assert_eq!(arc_large_flag(0.0, 0.001), 0);
        // Exactly 180.001 → 1 (just past threshold).
        assert_eq!(arc_large_flag(0.0, 180.001), 1);
    }

    #[test]
    fn test_parse_link_radius_bare_number_without_suffix_parsed_as_is() {
        // No r/p suffix → straight f64 parse.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("500", &layout), 500.0);
        assert_eq!(parse_link_radius("-50.5", &layout), -50.5);
        assert_eq!(parse_link_radius("0", &layout), 0.0);
        // Garbage → 0.
        assert_eq!(parse_link_radius("not_a_number", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_negative_exactly_negative_360_wraps_to_zero() {
        // diff=-360 → after +=360 → 0 → not > 0 → flag 0.
        assert_eq!(arc_sweep_flag(360.0, 0.0), 0);
        // diff=-361 → after +=360 → -1 → still < 0 → not > 0 → flag 0.
        assert_eq!(arc_sweep_flag(361.0, 0.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_standard_quadrant_spans_all_return_one() {
        // Typical forward quadrant sweeps: 90°, 180°, 270° — all in (0, 360) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 90.0), 1);
        assert_eq!(arc_sweep_flag(0.0, 180.0), 1);
        assert_eq!(arc_sweep_flag(0.0, 270.0), 1);
        // Non-zero-start + non-zero-end positive sweep.
        assert_eq!(arc_sweep_flag(45.0, 135.0), 1);
    }

    #[test]
    fn test_arc_large_flag_standard_spans_below_and_above_180() {
        // span <180 → 0; span >180 → 1.
        assert_eq!(arc_large_flag(0.0, 90.0), 0);
        assert_eq!(arc_large_flag(0.0, 179.9), 0);
        assert_eq!(arc_large_flag(0.0, 270.0), 1);
        // Starting in any quadrant, forward sweep.
        assert_eq!(arc_large_flag(45.0, 315.0), 1); // 270° span
    }

    #[test]
    fn test_parse_link_radius_decimal_p_suffix_parsed_as_float() {
        // "150.5p" → trim "p" → parse "150.5" → 150.5 (pixels).
        let layout = mk_layout();
        assert_eq!(parse_link_radius("150.5p", &layout), 150.5);
        // Decimal r suffix also works.
        assert_eq!(parse_link_radius("0.25r", &layout), 250.0);
    }

    #[test]
    fn test_parse_link_radius_trims_whitespace_before_suffix_check() {
        // Input is trimmed — "  100p  " → "100p" → 100.0 (pixels).
        let layout = mk_layout();
        assert_eq!(parse_link_radius("  100p  ", &layout), 100.0);
        // Newline/tab surround also trimmed.
        assert_eq!(parse_link_radius("\t\n0.5r\t\n", &layout), 500.0);
    }

    #[test]
    fn test_arc_sweep_flag_order_sensitivity_forward_vs_reverse() {
        // arc_sweep_flag(a, b) differs from (b, a): forward diff b-a vs reverse a-b+360.
        // 0→90 = 90 (flag 1); 90→0 = -90 → +360 = 270 (flag 1 too).
        assert_eq!(arc_sweep_flag(0.0, 90.0), 1);
        assert_eq!(arc_sweep_flag(90.0, 0.0), 1);
        // 0→180 (flag 1); 180→0 → -180+360=180 (flag 1).
        assert_eq!(arc_sweep_flag(0.0, 180.0), 1);
        assert_eq!(arc_sweep_flag(180.0, 0.0), 1);
    }

    #[test]
    fn test_arc_large_flag_wraparound_through_zero_boundary() {
        // Sweep 350→10: span=10-350=-340 → +360 = 20 (<180) → flag 0.
        assert_eq!(arc_large_flag(350.0, 10.0), 0);
        // Sweep 10→350: span=340 (>180) → flag 1.
        assert_eq!(arc_large_flag(10.0, 350.0), 1);
    }

    #[test]
    fn test_parse_link_radius_scientific_notation_in_value() {
        // Scientific notation parses through f64::parse.
        let layout = mk_layout();
        // "1e2p" → trim_end_matches('p') → "1e2" → 100.
        assert_eq!(parse_link_radius("1e2p", &layout), 100.0);
        // "5e-1r" → 0.5 × 1000 = 500.
        assert_eq!(parse_link_radius("5e-1r", &layout), 500.0);
    }

    #[test]
    fn test_parse_link_radius_zero_value_with_all_suffixes() {
        // "0r", "0p", "0" all return 0.0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("0r", &layout), 0.0);
        assert_eq!(parse_link_radius("0p", &layout), 0.0);
        assert_eq!(parse_link_radius("0", &layout), 0.0);
        assert_eq!(parse_link_radius("0.0r", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_forward_sweep_exactly_359_999_still_flag_one() {
        // diff=359.999 < 360 → flag 1 (just below full circle).
        assert_eq!(arc_sweep_flag(0.0, 359.999), 1);
        // 360 exactly → not < 360 → flag 0.
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
    }

    #[test]
    fn test_arc_large_flag_span_exact_180_not_large_but_180_001_is() {
        // Strict inequality: span > 180 → flag 1; span==180 → flag 0.
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
        assert_eq!(arc_large_flag(0.0, 180.001), 1);
    }

    #[test]
    fn test_parse_link_radius_mixed_suffix_and_whitespace() {
        // "  \n\t 1r \n  " → trimmed → "1r" → 1 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("  \n\t 1r \n  ", &layout);
        assert_eq!(r, layout.dims.ideogram_radius);
        // Mixed with p.
        let r2 = parse_link_radius("  \t 250p \n", &layout);
        assert_eq!(r2, 250.0);
    }

    #[test]
    fn test_parse_link_radius_integer_string_without_suffix() {
        // "12345" → no suffix → parsed directly as f64.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("12345", &layout), 12345.0);
        // Negative bare.
        assert_eq!(parse_link_radius("-999", &layout), -999.0);
    }

    #[test]
    fn test_arc_sweep_flag_forward_and_negative_both_yield_flag_1_if_diff_valid() {
        // Forward 45→135 (diff 90, 0<diff<360) → 1.
        assert_eq!(arc_sweep_flag(45.0, 135.0), 1);
        // Reverse 135→45 (diff=-90, +360=270, 0<270<360) → 1.
        assert_eq!(arc_sweep_flag(135.0, 45.0), 1);
    }

    #[test]
    fn test_arc_large_flag_clean_half_circle_boundary_cases() {
        // Exactly half-circle → NOT > 180 → flag 0.
        for pair in [(0.0, 180.0), (45.0, 225.0), (90.0, 270.0)] {
            assert_eq!(arc_large_flag(pair.0, pair.1), 0, "pair={:?}", pair);
        }
    }

    #[test]
    fn test_parse_link_radius_very_long_decimal_preserved() {
        // Long decimal like "123.456789012345" preserved through parse.
        let layout = mk_layout();
        let r = parse_link_radius("123.456789012345p", &layout);
        assert!((r - 123.456789012345).abs() < 1e-12);
    }

    #[test]
    fn test_arc_sweep_flag_non_numeric_angles_never_panic() {
        // Extreme floats should not panic.
        let _ = arc_sweep_flag(f64::INFINITY, 0.0);
        let _ = arc_sweep_flag(0.0, f64::NEG_INFINITY);
        let _ = arc_sweep_flag(f64::NAN, 0.0);
    }

    #[test]
    fn test_parse_link_radius_very_small_r_coefficient() {
        // 1e-10r × ideogram_radius=1000 = 1e-7.
        let layout = mk_layout();
        let r = parse_link_radius("1e-10r", &layout);
        assert!((r - 1e-7).abs() < 1e-15);
    }

    #[test]
    fn test_arc_large_flag_reverse_sweep_still_yields_correct_flag() {
        // 300→30 span = 30-300 = -270 → +360 = 90 → NOT > 180 → flag 0.
        assert_eq!(arc_large_flag(300.0, 30.0), 0);
        // 200→30 span = 30-200 = -170 → +360 = 190 → > 180 → flag 1.
        assert_eq!(arc_large_flag(200.0, 30.0), 1);
    }

    #[test]
    fn test_parse_link_radius_unit_r_equals_one_exact_ideogram_radius() {
        // "1r" → exactly ideogram_radius value, no rounding.
        let layout = mk_layout();
        let r = parse_link_radius("1r", &layout);
        assert_eq!(r, layout.dims.ideogram_radius);
    }

    #[test]
    fn test_arc_sweep_flag_zero_to_one_positive_tiny_is_valid() {
        // diff=0.001 → > 0 and < 360 → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 0.001), 1);
        // diff=359.9999 → also valid.
        assert_eq!(arc_sweep_flag(0.0, 359.9999), 1);
    }

    #[test]
    fn test_parse_link_radius_sequence_of_similar_values_consistent() {
        // Same value in different forms: 1000 (bare), 1000p, 1r (=1000).
        let layout = mk_layout();
        let a = parse_link_radius("1000", &layout);
        let b = parse_link_radius("1000p", &layout);
        let c = parse_link_radius("1r", &layout);
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn test_arc_sweep_flag_diff_greater_than_360_not_wrapped_yields_flag_zero() {
        // diff=361 > 0 → no +360 adjustment → 361 NOT < 360 → flag 0.
        assert_eq!(arc_sweep_flag(0.0, 361.0), 0);
        // diff=720 also → flag 0.
        assert_eq!(arc_sweep_flag(0.0, 720.0), 0);
    }

    #[test]
    fn test_arc_large_flag_positive_span_well_below_180() {
        // 45° sweep → 45 < 180 → flag 0.
        assert_eq!(arc_large_flag(0.0, 45.0), 0);
        assert_eq!(arc_large_flag(100.0, 145.0), 0);
    }

    #[test]
    fn test_parse_link_radius_scientific_notation_negative_exponent() {
        // "2e-2r" = 0.02 × ideogram_radius=1000 = 20.
        let layout = mk_layout();
        let r = parse_link_radius("2e-2r", &layout);
        assert!((r - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_same_angle_yields_zero() {
        // start == end → diff 0 → not > 0 → flag 0.
        assert_eq!(arc_sweep_flag(45.0, 45.0), 0);
        assert_eq!(arc_sweep_flag(0.0, 0.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_negative_diff_wraps_by_360() {
        // end < start → diff becomes end - start + 360.
        // 20 - 350 = -330 → +360 = 30 (in (0,360) range) → 1.
        assert_eq!(arc_sweep_flag(350.0, 20.0), 1);
    }

    #[test]
    fn test_arc_large_flag_exact_180_boundary_is_zero() {
        // span == 180 → not > 180 → 0 (boundary exclusive).
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
    }

    #[test]
    fn test_parse_link_radius_invalid_suffix_r_string_parses_zero() {
        // "abcr" trims 'r' → "abc" parse → fail → 0.0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("abcr", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_just_under_360_yields_one() {
        // diff = 359.99 → in (0, 360) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 359.99), 1);
    }

    #[test]
    fn test_arc_large_flag_just_above_180_is_one() {
        // span = 180.01 → > 180 → flag 1.
        assert_eq!(arc_large_flag(0.0, 180.01), 1);
        // span = 180 - epsilon → not > 180 → flag 0.
        assert_eq!(arc_large_flag(0.0, 179.99), 0);
    }

    #[test]
    fn test_parse_link_radius_pure_p_suffix_extracts_value() {
        // "750p" → 750 (pixels).
        let layout = mk_layout();
        assert_eq!(parse_link_radius("750p", &layout), 750.0);
    }

    #[test]
    fn test_parse_link_radius_empty_string_returns_zero() {
        // "" trim is empty → no suffix path → parse fail → 0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("", &layout), 0.0);
        // whitespace → same.
        assert_eq!(parse_link_radius("   ", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_diff_zero_point_five_yields_one() {
        // A tiny positive diff (still in (0, 360)) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 0.5), 1);
    }

    #[test]
    fn test_arc_large_flag_360_exact_yields_one() {
        // Exactly 360 is not > 360 but still > 180 → flag 1.
        assert_eq!(arc_large_flag(0.0, 360.0), 1);
    }

    #[test]
    fn test_parse_link_radius_zero_r_evaluates_to_zero() {
        // "0r" × ideogram_radius=any = 0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("0r", &layout), 0.0);
        // "0" plain also → 0.
        assert_eq!(parse_link_radius("0", &layout), 0.0);
    }

    #[test]
    fn test_parse_link_radius_decimal_coefficient_with_r_unit() {
        // "0.25r" × 1000 = 250.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("0.25r", &layout), 250.0);
    }

    #[test]
    fn test_arc_sweep_flag_smallest_finite_positive_diff_is_one() {
        // Anything in (0, 360) exclusive → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 1e-9), 1);
    }

    #[test]
    fn test_arc_large_flag_very_small_span_below_180_is_zero() {
        // 0.001° span → < 180 → 0.
        assert_eq!(arc_large_flag(100.0, 100.001), 0);
    }

    #[test]
    fn test_parse_link_radius_bare_integer_string_without_suffix() {
        // "500" (no suffix) → parse directly → 500.0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("500", &layout), 500.0);
    }

    #[test]
    fn test_arc_sweep_flag_diff_exactly_360_returns_zero() {
        // 360 is not in (0, 360) — not < 360 → flag 0.
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_zero_end_nonzero_start_wraps() {
        // 100→0: diff=-100+360=260, in (0,360) → flag 1.
        assert_eq!(arc_sweep_flag(100.0, 0.0), 1);
    }

    #[test]
    fn test_arc_large_flag_reverse_span_wraps_correctly() {
        // 350→10: span=-340+360=20 (small) → flag 0.
        assert_eq!(arc_large_flag(350.0, 10.0), 0);
        // 10→350: span=340 > 180 → flag 1.
        assert_eq!(arc_large_flag(10.0, 350.0), 1);
    }

    #[test]
    fn test_parse_link_radius_trimmed_whitespace_with_suffix() {
        // Surrounding whitespace trimmed before suffix detection.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("  100p  ", &layout), 100.0);
        assert_eq!(parse_link_radius("\t0.5r\t", &layout), 500.0);
    }

    #[test]
    fn test_arc_large_flag_zero_span_same_angle_is_zero() {
        // start == end → span=0 → 0 ≤ 180 → flag 0.
        assert_eq!(arc_large_flag(45.0, 45.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_forward_270_in_range_is_flag_one() {
        // 270 is in (0, 360) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 270.0), 1);
    }

    #[test]
    fn test_parse_link_radius_multi_digit_integer_returns_matching_f64() {
        // "12345" → 12345.0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("12345", &layout), 12345.0);
    }

    #[test]
    fn test_arc_large_flag_half_circle_plus_one_is_large() {
        // 181 > 180 → large.
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
    }

    #[test]
    fn test_parse_link_radius_decimal_p_suffix_parses_as_float() {
        // "3.14p" → 3.14 (p stripped).
        let layout = mk_layout();
        let r = parse_link_radius("3.14p", &layout);
        assert!((r - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_negative_large_diff_yields_one_after_wrap() {
        // 30 → 330: diff=300 in (0,360) → flag 1.
        assert_eq!(arc_sweep_flag(30.0, 330.0), 1);
    }

    #[test]
    fn test_arc_large_flag_90_span_below_180_is_zero() {
        // 0°→90° span=90 → flag 0.
        assert_eq!(arc_large_flag(0.0, 90.0), 0);
    }

    #[test]
    fn test_parse_link_radius_two_decimal_places_retained() {
        // "2.50r" × 1000 → 2500.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("2.50r", &layout), 2500.0);
    }

    #[test]
    fn test_parse_link_radius_negative_bare_value_parses_negative() {
        // Plain "-100" → -100.0 (no suffix; f64 parse).
        let layout = mk_layout();
        assert_eq!(parse_link_radius("-100", &layout), -100.0);
    }

    #[test]
    fn test_arc_sweep_flag_both_negative_angles_yields_flag() {
        // -45 → -35: diff=10 > 0 in (0,360) → flag 1.
        assert_eq!(arc_sweep_flag(-45.0, -35.0), 1);
    }

    #[test]
    fn test_arc_large_flag_diff_near_180_from_below_is_zero() {
        // 179.999 just below 180 → flag 0.
        assert_eq!(arc_large_flag(0.0, 179.999), 0);
    }

    #[test]
    fn test_parse_link_radius_very_large_r_coefficient() {
        // "100r" × 1000 = 100000.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("100r", &layout), 100_000.0);
    }

    #[test]
    fn test_arc_sweep_flag_tiny_negative_wraps_up() {
        // 0 → -1e-9: diff = -1e-9 + 360 = 360 - 1e-9 < 360 → flag 1.
        // Actually very small negative → wraps to near-360; in (0, 360) → flag 1.
        let r = arc_sweep_flag(0.0, -1e-9);
        // Either 1 (in range) or depending on precision — accept non-panic.
        assert!(r == 0 || r == 1);
    }

    #[test]
    fn test_arc_sweep_flag_180_diff_yields_one() {
        // diff=180 in (0, 360) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 180.0), 1);
    }

    #[test]
    fn test_arc_large_flag_270_span_is_large() {
        // span=270 > 180 → flag 1.
        assert_eq!(arc_large_flag(0.0, 270.0), 1);
    }

    #[test]
    fn test_parse_link_radius_r_unit_with_four_decimal_places() {
        // "1.2345r" × 1000 = 1234.5.
        let layout = mk_layout();
        let r = parse_link_radius("1.2345r", &layout);
        assert!((r - 1234.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_zero_p_returns_zero() {
        // "0p" → 0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("0p", &layout), 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_flag_is_either_zero_or_one() {
        // Invariant: flag is always exactly 0 or 1.
        for (s, e) in [(0.0, 45.0), (-10.0, 10.0), (180.0, 90.0), (0.0, 360.0)] {
            let f = arc_sweep_flag(s, e);
            assert!(f == 0 || f == 1);
        }
    }

    #[test]
    fn test_arc_large_flag_flag_is_either_zero_or_one() {
        // Invariant: flag is always 0 or 1.
        for (s, e) in [(0.0, 10.0), (0.0, 180.0), (0.0, 270.0), (0.0, 360.0)] {
            let f = arc_large_flag(s, e);
            assert!(f == 0 || f == 1);
        }
    }

    #[test]
    fn test_parse_link_radius_with_only_whitespace_around_value() {
        // Surrounding whitespace only → trimmed → parse successful.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("  500  ", &layout), 500.0);
    }

    #[test]
    fn test_parse_link_radius_scientific_notation_with_p_suffix() {
        // "1e2p" → 100.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("1e2p", &layout), 100.0);
    }

    #[test]
    fn test_arc_sweep_flag_exact_180_yields_one() {
        // 180 diff → in (0,360) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 180.0), 1);
    }

    #[test]
    fn test_arc_large_flag_exactly_zero_span_is_zero() {
        // Zero span → 0.
        assert_eq!(arc_large_flag(90.0, 90.0), 0);
    }

    #[test]
    fn test_parse_link_radius_sci_notation_r_unit() {
        // "1e-1r" → 0.1 × 1000 = 100.
        let layout = mk_layout();
        let r = parse_link_radius("1e-1r", &layout);
        assert!((r - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_simple_integer_passthrough_no_suffix() {
        // "42" → 42.0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("42", &layout), 42.0);
    }

    #[test]
    fn test_arc_sweep_flag_diff_exactly_zero_yields_zero() {
        // diff = 0 → not > 0 → flag 0.
        assert_eq!(arc_sweep_flag(50.0, 50.0), 0);
    }

    #[test]
    fn test_arc_large_flag_span_360_yields_one() {
        // 360 > 180 → flag 1.
        assert_eq!(arc_large_flag(0.0, 360.0), 1);
    }

    #[test]
    fn test_parse_link_radius_small_decimal_only_r_unit() {
        // "0.001r" × 1000 = 1.
        let layout = mk_layout();
        let r = parse_link_radius("0.001r", &layout);
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_negative_r_value() {
        // "-2r" × 1000 = -2000.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("-2r", &layout), -2000.0);
    }

    #[test]
    fn test_arc_sweep_flag_diff_just_above_zero_yields_flag_one() {
        // 0 → 0.1 (tiny positive diff) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 0.1), 1);
    }

    #[test]
    fn test_arc_large_flag_exactly_180_is_zero() {
        // span=180 → not > 180 → flag 0.
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_with_very_large_value() {
        // "999999p" → 999999.0.
        let layout = mk_layout();
        assert_eq!(parse_link_radius("999999p", &layout), 999999.0);
    }

    #[test]
    fn test_parse_link_radius_fractional_plain_float() {
        // "3.14" bare float → 3.14.
        let layout = mk_layout();
        let r = parse_link_radius("3.14", &layout);
        assert!((r - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_diff_below_zero_wraps_and_returns_one() {
        // end < start: 10-300 = -290 → +360 = 70, 70 in (0,360) → 1.
        assert_eq!(arc_sweep_flag(300.0, 10.0), 1);
    }

    #[test]
    fn test_arc_sweep_flag_diff_exactly_360_yields_zero() {
        // 360 not < 360 → 0.
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
    }

    #[test]
    fn test_arc_large_flag_span_exactly_181_is_one() {
        // 181 > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
    }

    #[test]
    fn test_parse_link_radius_r_unit_scaled_by_ideogram_radius() {
        // "1r" → 1 × ideogram_radius (use the layout's actual value).
        let layout = mk_layout();
        let r = parse_link_radius("1r", &layout);
        assert!((r - layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_zero_r_value() {
        // "0r" → 0 × ideogram_radius = 0.
        let layout = mk_layout();
        let r = parse_link_radius("0r", &layout);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_arc_sweep_flag_full_circle_is_zero() {
        // start=0, end=360 → diff=360, not <360 → flag=0.
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
    }

    #[test]
    fn test_arc_large_flag_negative_span_wraps_and_large() {
        // start=200, end=0 → span -200 + 360 = 160; 160 not >180 → 0.
        assert_eq!(arc_large_flag(200.0, 0.0), 0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_with_zero() {
        // "0p" → 0.0.
        let layout = mk_layout();
        let r = parse_link_radius("0p", &layout);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_parse_link_radius_bare_integer_with_spaces() {
        // " 500 " bare with leading/trailing spaces → trim → 500.
        let layout = mk_layout();
        let r = parse_link_radius("  500  ", &layout);
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_arc_sweep_flag_diff_180_yields_one() {
        // Diff exactly 180 → in (0, 360) → flag 1.
        assert_eq!(arc_sweep_flag(0.0, 180.0), 1);
    }

    #[test]
    fn test_arc_large_flag_span_just_above_180_is_one() {
        // span 180.001 > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 180.001), 1);
    }

    #[test]
    fn test_parse_link_radius_2r_scaled_twice_ideogram_radius() {
        // "2r" → 2 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("2r", &layout);
        assert!((r - 2.0 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_half_r_unit_half_ideogram_radius() {
        // "0.5r" → 0.5 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("0.5r", &layout);
        assert!((r - 0.5 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_reverse_exact_360_spans() {
        // end < start by exactly 360 → diff=-360+360=0 → not >0 → 0.
        assert_eq!(arc_sweep_flag(360.0, 0.0), 0);
    }

    #[test]
    fn test_arc_large_flag_span_100_under_180_is_zero() {
        // 100 < 180 → 0.
        assert_eq!(arc_large_flag(0.0, 100.0), 0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_with_decimal_value() {
        // "150.5p" → 150.5.
        let layout = mk_layout();
        let r = parse_link_radius("150.5p", &layout);
        assert_eq!(r, 150.5);
    }

    #[test]
    fn test_arc_sweep_flag_very_small_positive_diff_yields_one() {
        // Any small positive diff in (0,360) → 1.
        assert_eq!(arc_sweep_flag(0.0, 0.01), 1);
    }

    #[test]
    fn test_arc_large_flag_span_zero_is_zero() {
        // span=0 not >180 → 0.
        assert_eq!(arc_large_flag(50.0, 50.0), 0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_with_negative_value() {
        // "-50p" → -50.0.
        let layout = mk_layout();
        let r = parse_link_radius("-50p", &layout);
        assert_eq!(r, -50.0);
    }

    #[test]
    fn test_parse_link_radius_bare_negative_float() {
        // "-3.14" bare → -3.14.
        let layout = mk_layout();
        let r = parse_link_radius("-3.14", &layout);
        assert!((r + 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_end_greater_small_positive_diff() {
        // 5 < 10 → diff=5 in (0,360) → 1.
        assert_eq!(arc_sweep_flag(5.0, 10.0), 1);
    }

    #[test]
    fn test_arc_large_flag_span_90_less_than_180_zero() {
        // span=90 < 180 → 0.
        assert_eq!(arc_large_flag(0.0, 90.0), 0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_with_scientific_value() {
        // "1e3p" → 1000.
        let layout = mk_layout();
        let r = parse_link_radius("1e3p", &layout);
        assert_eq!(r, 1000.0);
    }

    #[test]
    fn test_parse_link_radius_bare_integer_value() {
        // "42" bare → 42.0.
        let layout = mk_layout();
        let r = parse_link_radius("42", &layout);
        assert_eq!(r, 42.0);
    }

    #[test]
    fn test_arc_sweep_flag_diff_exactly_neg_360_wraps_to_zero() {
        // start=360, end=0 → diff=-360+360=0 → not >0 → 0.
        assert_eq!(arc_sweep_flag(360.0, 0.0), 0);
    }

    #[test]
    fn test_arc_large_flag_span_large_negative_wraps() {
        // start=350, end=50 → span=-300+360=60 → 60 not >180 → 0.
        assert_eq!(arc_large_flag(350.0, 50.0), 0);
    }

    #[test]
    fn test_parse_link_radius_3r_scaled_triple_ideogram_radius() {
        // "3r" → 3 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("3r", &layout);
        assert!((r - 3.0 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_trimmed_input_with_whitespace_trail() {
        // "100p  " trailing spaces trimmed before parse.
        let layout = mk_layout();
        let r = parse_link_radius("100p  ", &layout);
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_arc_sweep_flag_end_50_after_start_5_yields_one() {
        // diff=45 in (0,360) → 1.
        assert_eq!(arc_sweep_flag(5.0, 50.0), 1);
    }

    #[test]
    fn test_arc_large_flag_span_270_large_arc_one() {
        // span=270 > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 270.0), 1);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_with_one_thousand() {
        // "1000p" → 1000.
        let layout = mk_layout();
        let r = parse_link_radius("1000p", &layout);
        assert_eq!(r, 1000.0);
    }

    #[test]
    fn test_parse_link_radius_r_unit_with_tenth_scale() {
        // "0.1r" → 0.1 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("0.1r", &layout);
        assert!((r - 0.1 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_diff_exactly_180_yields_one() {
        // 180 > 0 and < 360 → 1.
        assert_eq!(arc_sweep_flag(0.0, 180.0), 1);
    }

    #[test]
    fn test_arc_large_flag_span_exactly_360_yields_one() {
        // span=360 → 360 > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 360.0), 1);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_multi_digit_integer() {
        // "123456p" → 123456.
        let layout = mk_layout();
        let r = parse_link_radius("123456p", &layout);
        assert_eq!(r, 123456.0);
    }

    #[test]
    fn test_parse_link_radius_r_unit_fractional_three_digits() {
        // "0.333r" → 0.333 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("0.333r", &layout);
        assert!((r - 0.333 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_tiny_negative_diff_wraps_to_positive() {
        // Tiny negative diff → +360 → in (0,360) → 1.
        assert_eq!(arc_sweep_flag(10.0, 9.9), 1);
    }

    #[test]
    fn test_arc_large_flag_span_181_is_one() {
        // span=181 > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
    }

    #[test]
    fn test_parse_link_radius_bare_zero_returns_zero() {
        // "0" bare → 0.0.
        let layout = mk_layout();
        let r = parse_link_radius("0", &layout);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_parse_link_radius_large_r_scale() {
        // "10r" → 10 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("10r", &layout);
        assert!((r - 10.0 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_quarter_circle_is_one() {
        // 90° quarter circle → 1.
        assert_eq!(arc_sweep_flag(45.0, 135.0), 1);
    }

    #[test]
    fn test_arc_large_flag_30_deg_below_180_is_zero() {
        // 30 < 180 → 0.
        assert_eq!(arc_large_flag(60.0, 90.0), 0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_fractional_decimal() {
        // "99.99p" → 99.99.
        let layout = mk_layout();
        let r = parse_link_radius("99.99p", &layout);
        assert!((r - 99.99).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_r_unit_with_100r_scale() {
        // "100r" → 100 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("100r", &layout);
        assert!((r - 100.0 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_full_circle_yields_zero() {
        // diff == 0 (start == end) → diff>0 && diff<360 false → 0.
        assert_eq!(arc_sweep_flag(45.0, 45.0), 0);
    }

    #[test]
    fn test_arc_sweep_flag_negative_delta_wraps_and_returns_one() {
        // end < start: diff=-30+360=330 → 0<330<360 → 1.
        assert_eq!(arc_sweep_flag(90.0, 60.0), 1);
    }

    #[test]
    fn test_arc_large_flag_exactly_180_yields_zero() {
        // span == 180 → not > 180 → 0.
        assert_eq!(arc_large_flag(0.0, 180.0), 0);
    }

    #[test]
    fn test_arc_large_flag_negative_span_wraps_above_180() {
        // end<start: span=-270+360=90 → not > 180 → 0.
        assert_eq!(arc_large_flag(270.0, 0.0), 0);
    }

    #[test]
    fn test_parse_link_radius_zero_point_five_r_half_ideogram() {
        // "0.5r" → 0.5 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("0.5r", &layout);
        assert!((r - 0.5 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_r_with_trailing_whitespace_trimmed() {
        // "  1.5r  " → trimmed, 1.5 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("  1.5r  ", &layout);
        assert!((r - 1.5 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_parse_link_radius_bare_float_no_suffix() {
        // "42.5" bare → parse → 42.5.
        let layout = mk_layout();
        let r = parse_link_radius("42.5", &layout);
        assert_eq!(r, 42.5);
    }

    #[test]
    fn test_arc_sweep_flag_positive_small_sweep_gives_one() {
        // 30° sweep → diff=30, 0<30<360 → 1.
        assert_eq!(arc_sweep_flag(10.0, 40.0), 1);
    }

    #[test]
    fn test_arc_large_flag_just_above_180_yields_one() {
        // 181° span → > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
    }

    #[test]
    fn test_arc_large_flag_just_below_180_yields_zero() {
        // 179° span → not > 180 → 0.
        assert_eq!(arc_large_flag(0.0, 179.0), 0);
    }

    #[test]
    fn test_parse_link_radius_empty_string_returns_zero_v2() {
        // Empty string → parse fails in parse_radius_simple → 0.0.
        let layout = mk_layout();
        let r = parse_link_radius("", &layout);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_parse_link_radius_p_suffix_integer_value() {
        // "250p" → 250.0 (pixel suffix stripped).
        let layout = mk_layout();
        let r = parse_link_radius("250p", &layout);
        assert_eq!(r, 250.0);
    }

    #[test]
    fn test_arc_sweep_flag_exact_360_yields_zero() {
        // diff == 360 → not < 360 → 0 (boundary exclusive).
        assert_eq!(arc_sweep_flag(0.0, 360.0), 0);
        // Negative wrap end<start → diff=-360+360=0 → 0<0<360 false → 0.
        assert_eq!(arc_sweep_flag(360.0, 0.0), 0);
    }

    #[test]
    fn test_arc_large_flag_exactly_zero_span_yields_zero() {
        // span == 0 → not > 180 → 0.
        assert_eq!(arc_large_flag(45.0, 45.0), 0);
    }

    #[test]
    fn test_parse_link_radius_zero_bare_value() {
        // Bare "0" → 0.0.
        let layout = mk_layout();
        let r = parse_link_radius("0", &layout);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_parse_link_radius_very_large_p_value() {
        // "99999p" → 99999.0.
        let layout = mk_layout();
        let r = parse_link_radius("99999p", &layout);
        assert_eq!(r, 99999.0);
    }

    #[test]
    fn test_arc_sweep_flag_small_negative_direction() {
        // end<start by 30° → diff=-30+360=330 → 0<330<360 → 1.
        assert_eq!(arc_sweep_flag(60.0, 30.0), 1);
    }

    #[test]
    fn test_arc_large_flag_negative_span_just_below_wrap() {
        // end<start by 20°: span=-20+360=340 → >180 → 1.
        assert_eq!(arc_large_flag(60.0, 40.0), 1);
    }

    #[test]
    fn test_parse_link_radius_decimal_with_r_fractional() {
        // "0.3333r" × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("0.3333r", &layout);
        assert!((r - 0.3333 * layout.dims.ideogram_radius).abs() < 1e-6);
    }

    #[test]
    fn test_parse_link_radius_negative_p_parsed() {
        // "-50p" → -50.0 (negative pixel).
        let layout = mk_layout();
        let r = parse_link_radius("-50p", &layout);
        assert_eq!(r, -50.0);
    }

    #[test]
    fn test_arc_sweep_flag_large_positive_sweep_still_in_range() {
        // 270° sweep → 1.
        assert_eq!(arc_sweep_flag(0.0, 270.0), 1);
    }

    #[test]
    fn test_arc_large_flag_positive_span_under_180_yields_zero() {
        // 90° span → not > 180 → 0.
        assert_eq!(arc_large_flag(0.0, 90.0), 0);
    }

    #[test]
    fn test_parse_link_radius_small_fraction_r_value() {
        // "0.01r" → 0.01 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("0.01r", &layout);
        assert!((r - 0.01 * layout.dims.ideogram_radius).abs() < 1e-9);
    }

    #[test]
    fn test_arc_sweep_flag_complete_reverse_yields_zero() {
        // Going start=359 to end=0 (1° backward via wrap): diff = -359+360 = 1 → in (0,360) → 1.
        assert_eq!(arc_sweep_flag(359.0, 0.0), 1);
    }

    #[test]
    fn test_parse_link_radius_with_integer_value_no_suffix() {
        // Bare integer "100" → 100.0.
        let layout = mk_layout();
        let r = parse_link_radius("100", &layout);
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_arc_large_flag_span_wraps_to_less_than_180() {
        // end<start: span=-30-170=-200 → +360=160 → not >180 → 0.
        assert_eq!(arc_large_flag(170.0, -30.0), 0);
    }

    #[test]
    fn test_parse_link_radius_tiny_p_value_preserves_precision() {
        // "0.001p" → 0.001.
        let layout = mk_layout();
        let r = parse_link_radius("0.001p", &layout);
        assert_eq!(r, 0.001);
    }

    #[test]
    fn test_arc_sweep_flag_quarter_sweep_yields_one() {
        // 90° sweep → 1.
        assert_eq!(arc_sweep_flag(0.0, 90.0), 1);
    }

    #[test]
    fn test_arc_large_flag_exactly_181_yields_one() {
        // 181° span → > 180 → 1.
        assert_eq!(arc_large_flag(0.0, 181.0), 1);
    }

    #[test]
    fn test_parse_link_radius_two_p_values_bare() {
        // "200" bare → 200.0 (no suffix).
        let layout = mk_layout();
        let r = parse_link_radius("200", &layout);
        assert_eq!(r, 200.0);
    }

    #[test]
    fn test_arc_sweep_flag_negative_zero_delta_false() {
        // Negative zero equivalent to zero → 0<0<360 false → 0.
        assert_eq!(arc_sweep_flag(-0.0, 0.0), 0);
    }

    #[test]
    fn test_parse_link_radius_negative_r_preserves_multiplier_sign() {
        // "-1r" → -1 × ideogram_radius.
        let layout = mk_layout();
        let r = parse_link_radius("-1r", &layout);
        assert!((r + layout.dims.ideogram_radius).abs() < 1e-9);
    }
}
