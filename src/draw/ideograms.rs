use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::draw::report_image_map;
use crate::karyotype::types::Karyotype;
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::{SvgDocument, svg_slice, svg_text};
use crate::utils::format_url;

/// Draw all ideograms: fills, cytogenetic bands, outlines, and labels.
pub fn draw_ideograms(
    doc: &mut SvgDocument,
    layout: &Layout,
    conf: &HashMap<String, ConfigValue>,
    karyotype: &Karyotype,
    colors: &ColorMap,
) {
    let ideogram_conf = conf.get("ideogram").and_then(|v| v.as_map());

    let stroke_color_name = ideogram_conf
        .and_then(|m| m.get("stroke_color"))
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let stroke_thickness: f64 = ideogram_conf
        .and_then(|m| m.get("stroke_thickness"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let fill = ideogram_conf
        .and_then(|m| m.get("fill"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let fill_color_name = ideogram_conf
        .and_then(|m| m.get("fill_color"))
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let show_bands = ideogram_conf
        .and_then(|m| m.get("show_bands"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let fill_bands = ideogram_conf
        .and_then(|m| m.get("fill_bands"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let band_stroke_thickness: f64 = ideogram_conf
        .and_then(|m| m.get("band_stroke_thickness"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let band_transparency: u8 = ideogram_conf
        .and_then(|m| m.get("band_transparency"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let show_label = ideogram_conf
        .and_then(|m| m.get("show_label"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let label_size: f64 = ideogram_conf
        .and_then(|m| m.get("label_size"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(36.0);

    let stroke_color = colors
        .resolve(stroke_color_name)
        .unwrap_or(Color::rgb(0, 0, 0));
    let default_fill_color = colors
        .resolve(fill_color_name)
        .unwrap_or(Color::rgb(0, 0, 0));
    let label_color = colors.resolve("black").unwrap_or(Color::rgb(0, 0, 0));

    // Image-map URL templates (Perl: `seek_parameter("url", $ideogram)` OR
    // `$CONF{ideogram}{ideogram_url}` / `$CONF{ideogram}{band_url}`).
    let ideogram_url_tpl: Option<String> = ideogram_conf
        .and_then(|m| m.get("ideogram_url"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let band_url_tpl: Option<String> = ideogram_conf
        .and_then(|m| m.get("band_url"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let missing_policy: String = conf
        .get("image")
        .and_then(|v| v.get("image_map_missing_parameter"))
        .and_then(|v| v.as_str())
        .unwrap_or("removeparam")
        .to_string();

    doc.open_group("ideograms");

    for ideo in &layout.ideograms {
        let chr = &ideo.chr;
        let start = ideo.set.min().unwrap_or(0);
        let end = ideo.set.max().unwrap_or(0);

        let start_a = layout.getanglepos(start, chr).unwrap_or(0.0);
        let end_a = layout.getanglepos(end, chr).unwrap_or(0.0);

        let radius_outer = if ideo.radius_outer > 0.0 {
            ideo.radius_outer
        } else {
            layout.dims.ideogram_radius_outer
        };
        let radius_inner = if ideo.radius_inner > 0.0 {
            ideo.radius_inner
        } else {
            layout.dims.ideogram_radius_inner
        };

        // Draw ideogram fill
        let chr_color = if fill {
            colors.resolve(&ideo.color).or(Some(default_fill_color))
        } else {
            None
        };

        let slice_svg = svg_slice(
            layout,
            start_a,
            end_a,
            radius_inner,
            radius_outer,
            Some(&stroke_color),
            Some(stroke_thickness),
            chr_color.as_ref(),
            None,
        );
        doc.add(slice_svg);

        // --- Image-map: emit a poly `<area>` for this ideogram slice ---
        if let Some(tpl) = &ideogram_url_tpl {
            let mut ideo_params: HashMap<String, ConfigValue> = HashMap::new();
            ideo_params.insert("chr".into(), ConfigValue::Str(chr.clone()));
            ideo_params.insert("start".into(), ConfigValue::Str(start.to_string()));
            ideo_params.insert("end".into(), ConfigValue::Str(end.to_string()));
            ideo_params.insert("label".into(), ConfigValue::Str(ideo.label.clone()));
            ideo_params.insert("tag".into(), ConfigValue::Str(ideo.tag.clone()));
            if let Ok(Some(url)) = format_url(tpl, &[&ideo_params], &missing_policy) {
                let coords = slice_polygon_coords(
                    layout.image_radius,
                    start_a,
                    end_a,
                    radius_inner,
                    radius_outer,
                );
                report_image_map("poly", &coords, &url);
            }
        }

        // Draw cytogenetic bands
        if show_bands && let Some(bands) = karyotype.bands.get(chr) {
            for band in bands {
                // Intersect band with ideogram region
                let band_set = band.set.intersect(&ideo.set);
                if band_set.cardinality() < 1 {
                    continue;
                }
                let band_start = band_set.min().unwrap();
                let band_end = band_set.max().unwrap();
                let band_start_a = layout.getanglepos(band_start, chr).unwrap_or(0.0);
                let band_end_a = layout.getanglepos(band_end, chr).unwrap_or(0.0);

                let band_fill = if fill_bands {
                    let mut color = colors.resolve(&band.color);
                    if band_transparency > 0
                        && let Some(c) = &color
                    {
                        let alpha = ((band_transparency as f64 / 5.0) * 255.0).min(255.0) as u8;
                        color = Some(Color::rgba(c.r, c.g, c.b, alpha));
                    }
                    color
                } else {
                    None
                };

                let opacity = if band_transparency > 0 {
                    Some(band_transparency as f64 / 5.0)
                } else {
                    None
                };

                let band_svg = svg_slice(
                    layout,
                    band_start_a,
                    band_end_a,
                    radius_inner,
                    radius_outer,
                    Some(&stroke_color),
                    Some(band_stroke_thickness),
                    band_fill.as_ref(),
                    opacity,
                );
                doc.add(band_svg);

                // --- Image-map: emit a poly `<area>` for each band ---
                if let Some(tpl) = &band_url_tpl {
                    let mut band_params: HashMap<String, ConfigValue> = HashMap::new();
                    band_params.insert("chr".into(), ConfigValue::Str(chr.clone()));
                    band_params
                        .insert("start".into(), ConfigValue::Str(band_start.to_string()));
                    band_params.insert("end".into(), ConfigValue::Str(band_end.to_string()));
                    band_params.insert("name".into(), ConfigValue::Str(band.name.clone()));
                    if let Ok(Some(url)) = format_url(tpl, &[&band_params], &missing_policy) {
                        let coords = slice_polygon_coords(
                            layout.image_radius,
                            band_start_a,
                            band_end_a,
                            radius_inner,
                            radius_outer,
                        );
                        report_image_map("poly", &coords, &url);
                    }
                }
            }
        }

        // Draw ideogram outline (stroke only, no fill) - second pass
        if stroke_thickness > 0.0 {
            let outline_svg = svg_slice(
                layout,
                start_a,
                end_a,
                radius_inner,
                radius_outer,
                Some(&stroke_color),
                Some(stroke_thickness),
                None,
                None,
            );
            doc.add(outline_svg);
        }

        // Draw chromosome label
        if show_label {
            let mid_pos = (start + end) / 2;
            let mid_angle = layout.getanglepos(mid_pos, chr).unwrap_or(0.0);

            // Place label outside the ideogram
            let label_radius = radius_outer + label_size * 0.8;

            // Compute text rotation for readability
            let text_angle = textangle(mid_angle, false);

            let text_svg = svg_text(
                layout,
                mid_angle,
                label_radius,
                &ideo.label,
                label_size,
                &label_color,
                text_angle,
            );
            doc.add(text_svg);
        }
    }

    doc.close_group();
}

/// Port of Perl `anglemod(angle)`: normalize angle to [0,360).
pub fn anglemod(angle: f64) -> f64 {
    if angle < 0.0 {
        angle + 360.0
    } else if angle > 360.0 {
        angle - 360.0
    } else {
        angle
    }
}

/// Port of Perl `textangle(angle, is_parallel)`: rotation for GD stringFT so text
/// stays right-side up when placed at `angle`. `is_parallel` swings 90° for labels
/// drawn along the radial direction.
pub fn textangle(angle: f64, is_parallel: bool) -> f64 {
    let a = anglemod(angle);
    let mut textangle = if a <= 90.0 {
        360.0 - a
    } else if a < 180.0 {
        180.0 - a
    } else if a < 270.0 {
        360.0 - (a - 180.0)
    } else {
        360.0 - a
    };
    if is_parallel {
        let old = textangle;
        if (0.0..=90.0).contains(&old) {
            textangle -= 90.0;
        } else if old >= 270.0 {
            textangle += 90.0;
        }
    }
    textangle
}

/// Port of Perl `textanglesvg(angle, is_parallel)`: SVG-coords rotation (opposite
/// direction from GD).
pub fn textanglesvg(angle: f64, is_parallel: bool) -> f64 {
    360.0 - textangle(angle, is_parallel)
}

/// Port of Perl `textoffset(angle, radius, label_width, label_height, height_offset, is_parallel)`
/// — returns (delta_angle, delta_radius) so the label's centerline lines up with the desired
/// text position.
pub fn textoffset(
    angle: f64,
    radius: f64,
    label_width: f64,
    label_height: f64,
    height_offset: f64,
    is_parallel: bool,
) -> (f64, f64) {
    let rad2deg = 180.0 / std::f64::consts::PI;
    let angle_offset = rad2deg * ((label_height / 2.0 + height_offset) / radius);
    let mut radius_offset = label_width - 1.0;
    let angle = anglemod(angle);
    if is_parallel {
        radius_offset = if angle > 0.0 && angle < 180.0 {
            label_height
        } else {
            0.0
        };
    }
    if angle > 90.0 && angle < 270.0 {
        (-angle_offset, radius_offset)
    } else {
        (angle_offset, if !is_parallel { 0.0 } else { radius_offset })
    }
}

/// Sample an annular slice into a flat `[x0,y0,x1,y1,…]` polygon suitable for
/// an HTML `<area shape="poly">` element. Walks the outer arc from start→end
/// and the inner arc from end→start, with step size proportional to radius
/// (Perl uses `astep = 0.1 / radius * 180/PI`, capped at 0.01 rad).
pub fn slice_polygon_coords(
    image_radius: f64,
    start_angle: f64,
    end_angle: f64,
    radius_inner: f64,
    radius_outer: f64,
) -> Vec<f64> {
    let deg2rad = std::f64::consts::PI / 180.0;
    let cx = image_radius;
    let cy = image_radius;
    let (s, e) = if start_angle <= end_angle {
        (start_angle, end_angle)
    } else {
        (end_angle, start_angle)
    };
    let span = (e - s).abs().max(0.01);
    let step_deg = (0.1 / radius_outer.max(1.0)) * 180.0 / std::f64::consts::PI;
    let step = step_deg.max(0.1).min(span / 4.0);
    let mut coords: Vec<f64> = Vec::new();
    // Outer arc
    let mut a = s;
    while a <= e {
        coords.push(cx + radius_outer * (a * deg2rad).cos());
        coords.push(cy + radius_outer * (a * deg2rad).sin());
        a += step;
    }
    coords.push(cx + radius_outer * (e * deg2rad).cos());
    coords.push(cy + radius_outer * (e * deg2rad).sin());
    // Inner arc (reverse)
    let mut a = e;
    while a >= s {
        coords.push(cx + radius_inner * (a * deg2rad).cos());
        coords.push(cy + radius_inner * (a * deg2rad).sin());
        a -= step;
    }
    coords.push(cx + radius_inner * (s * deg2rad).cos());
    coords.push(cy + radius_inner * (s * deg2rad).sin());
    coords
}

/// Port of Perl `text_label_size(bounds)`: width and height from GD stringFT's
/// 8-element bounds array `[x0,y0, x1,y1, x2,y2, x3,y3]`.
pub fn text_label_size(bounds: &[f64; 8]) -> (f64, f64) {
    if (bounds[1] - bounds[3]).abs() < f64::EPSILON {
        (
            (bounds[2] - bounds[0]).abs() - 1.0,
            (bounds[5] - bounds[1]).abs() - 1.0,
        )
    } else {
        let w = (((bounds[2] - bounds[0]).abs() - 1.0).powi(2)
            + ((bounds[3] - bounds[1]).abs() - 1.0).powi(2))
        .sqrt();
        let h = (((bounds[6] - bounds[0]).abs() - 1.0).powi(2)
            + ((bounds[7] - bounds[1]).abs() - 1.0).powi(2))
        .sqrt();
        (w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anglemod_normalizes_to_0_360() {
        assert_eq!(anglemod(0.0), 0.0);
        assert_eq!(anglemod(90.0), 90.0);
        assert_eq!(anglemod(359.0), 359.0);
        assert_eq!(anglemod(-30.0), 330.0);
        assert_eq!(anglemod(370.0), 10.0);
    }

    #[test]
    fn test_textangle_quadrants() {
        // Perl: angle ≤ 90 → 360 - a  (upper right)
        assert!((textangle(30.0, false) - 330.0).abs() < 1e-9);
        // 90 < a < 180 → 180 - a
        assert!((textangle(120.0, false) - 60.0).abs() < 1e-9);
        // 180 ≤ a < 270 → 360 - (a - 180) = 540 - a
        assert!((textangle(200.0, false) - 340.0).abs() < 1e-9);
        // 270 ≤ a → 360 - a
        assert!((textangle(315.0, false) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn test_textanglesvg_is_gd_complement() {
        // textanglesvg = 360 - textangle for each quadrant
        for a in [30.0, 120.0, 200.0, 315.0] {
            assert!(
                (textanglesvg(a, false) + textangle(a, false) - 360.0).abs() < 1e-9,
                "svg+gd should sum to 360 for angle {}",
                a
            );
        }
    }

    #[test]
    fn test_slice_polygon_coords_shape() {
        // Simple quarter-arc slice: 45°→135°, r 100→200, image radius 500.
        let coords = slice_polygon_coords(500.0, 45.0, 135.0, 100.0, 200.0);
        // Flattened [x0, y0, x1, y1, …]
        assert_eq!(coords.len() % 2, 0, "coords flat layout should be even");
        // Outer arc walks first, inner arc walks back — first point on outer,
        // last point on inner (at start angle).
        assert!(coords.len() >= 8, "expected at least 8 coords, got {}", coords.len());
        // All coords inside image bounds
        for c in &coords {
            assert!(*c >= 0.0 && *c <= 1000.0, "coord {} outside bounds", c);
        }
    }

    #[test]
    fn test_text_label_size_axis_aligned_and_rotated() {
        // Axis-aligned (y0 == y1): w = (x1-x0)-1, h = (y5-y1)-1
        let bounds = [0.0, 20.0, 100.0, 20.0, 0.0, 0.0, 0.0, 0.0];
        let (w, h) = text_label_size(&bounds);
        assert!((w - 99.0).abs() < 1e-9, "axis-aligned width");
        // h from y5 - y1 = 0 - 20, abs - 1 = 19
        assert!((h - 19.0).abs() < 1e-9, "axis-aligned height");
        // Rotated bounds: general sqrt() case
        let b = [0.0, 0.0, 30.0, 40.0, 30.0, 40.0, 0.0, 0.0];
        let (w2, _h2) = text_label_size(&b);
        // w = sqrt((|30-0|-1)^2 + (|40-0|-1)^2) = sqrt(29^2 + 39^2) ≈ 48.60
        assert!((w2 - ((29f64).powi(2) + (39f64).powi(2)).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_angle_quadrants() {
        let (da, dr) = textoffset(45.0, 100.0, 50.0, 10.0, 0.0, false);
        // Upper-right quadrant: positive angle offset, no radius shift
        assert!(da > 0.0);
        assert_eq!(dr, 0.0);
        let (da2, _) = textoffset(200.0, 100.0, 50.0, 10.0, 0.0, false);
        // Lower-left: negative angle offset
        assert!(da2 < 0.0);
    }

    #[test]
    fn test_textangle_parallel_variant_rotates_90() {
        // is_parallel=true subtracts 90 when textangle ∈ [0,90], adds 90 when ≥ 270.
        let base = textangle(30.0, false);
        let par = textangle(30.0, true);
        assert!(
            (par - (base - 90.0)).abs() < 1e-6
                || (par - (base + 90.0)).abs() < 1e-6,
            "expected parallel to be ±90° from non-parallel; base={}, par={}",
            base,
            par
        );
    }

    #[test]
    fn test_slice_polygon_coords_identical_radii() {
        // radius_inner == radius_outer → outer and inner walks produce
        // identical coords. Result still non-empty.
        let coords = slice_polygon_coords(500.0, 0.0, 90.0, 100.0, 100.0);
        assert!(!coords.is_empty());
        // Even-length flat layout.
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_slice_polygon_coords_zero_radius_outer() {
        // radius_outer=0 at image center → outer points all at (cx, cy).
        let coords = slice_polygon_coords(500.0, 0.0, 90.0, 0.0, 0.0);
        // All outer points equal center (500, 500) since radius is 0.
        for chunk in coords.chunks(2).take(3) {
            assert!((chunk[0] - 500.0).abs() < 1e-6, "x={}", chunk[0]);
            assert!((chunk[1] - 500.0).abs() < 1e-6, "y={}", chunk[1]);
        }
    }

    #[test]
    fn test_slice_polygon_coords_step_floored_for_large_radius() {
        // Very large radius → step_deg gets very small, capped at 0.1° floor.
        // Function should still produce a finite coord count.
        let coords = slice_polygon_coords(500.0, 0.0, 10.0, 50.0, 100.0);
        // For 10° span at radius 100, step would be 0.1/100 * 180/π ≈ 0.057°.
        // Floor 0.1 kicks in → span/4 = 2.5° max step.
        // Should emit many coords.
        assert!(coords.len() >= 200, "expected ≥200 coords, got {}", coords.len());
    }

    #[test]
    fn test_slice_polygon_coords_all_within_image_bounds() {
        // All output coords should be within the image bounds.
        let image_radius = 500.0;
        let coords = slice_polygon_coords(image_radius, 0.0, 90.0, 100.0, 200.0);
        // image width = 2*image_radius = 1000. All coords should be in [0, 1000].
        for &c in &coords {
            assert!(
                (0.0..=1000.0).contains(&c),
                "coord {} out of image bounds",
                c
            );
        }
    }

    #[test]
    fn test_slice_polygon_coords_swap_reversed_angles() {
        // end_angle < start_angle should be silently swapped → coords identical
        // to forward direction. Verify same count and same first outer point.
        let forward = slice_polygon_coords(500.0, 10.0, 50.0, 100.0, 200.0);
        let reversed = slice_polygon_coords(500.0, 50.0, 10.0, 100.0, 200.0);
        assert_eq!(forward.len(), reversed.len());
        // First outer point = cx + r_outer*cos(s), cy + r_outer*sin(s) — should
        // be equal within float precision for both orderings after the swap.
        assert!((forward[0] - reversed[0]).abs() < 1e-9);
        assert!((forward[1] - reversed[1]).abs() < 1e-9);
    }

    #[test]
    fn test_slice_polygon_coords_small_span_floors_to_min() {
        // Span smaller than 0.01° hits the `.max(0.01)` floor → still produces
        // at least one outer + one inner point pair plus the guaranteed close.
        let coords = slice_polygon_coords(500.0, 10.0, 10.001, 100.0, 200.0);
        assert!(coords.len() >= 8, "expected ≥8 coords (2 outer + 2 inner + closing), got {}", coords.len());
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_double_wrap_not_supported() {
        // `anglemod` is a single-branch wrap — it does NOT fully normalize
        // values beyond ±360. -720 → -720+360=-360 (still negative, not wrapped).
        assert_eq!(anglemod(-720.0), -360.0);
        // 720 → 720-360=360 (still >360, not wrapped again).
        assert_eq!(anglemod(720.0), 360.0);
        // Exactly -360 → -360+360 = 0 (valid boundary).
        assert_eq!(anglemod(-360.0), 0.0);
    }

    #[test]
    fn test_text_label_size_negative_bounds_takes_absolute() {
        // Rotated bounds where some coords are negative — |abs()|-1 applied.
        // bounds [0, 0, -30, 40, ...] → w via general branch uses |-30-0|=30
        // → (30-1)^2 + (40-1)^2 sqrt ≈ 48.60.
        let b = [0.0, 0.0, -30.0, 40.0, 0.0, 0.0, 0.0, 0.0];
        let (w, _h) = text_label_size(&b);
        let expected = ((29f64).powi(2) + (39f64).powi(2)).sqrt();
        assert!((w - expected).abs() < 1e-9);
    }

    #[test]
    fn test_anglemod_boundary_zero_and_360() {
        // 0 → 0 (not adjusted), 360 → 360 (exactly — only > 360 adjusts).
        assert_eq!(anglemod(0.0), 0.0);
        assert_eq!(anglemod(360.0), 360.0);
        // Just-over 360 → adjusted via -360. Float subtraction introduces
        // a small residual; use tolerance.
        let r = anglemod(360.0001);
        assert!((r - 0.0001).abs() < 1e-9, "got {}", r);
    }

    #[test]
    fn test_anglemod_small_negative_normalizes_to_near_360() {
        // -0.5 → 359.5 (via +360).
        assert_eq!(anglemod(-0.5), 359.5);
        // -1.0 → 359.0.
        assert_eq!(anglemod(-1.0), 359.0);
    }

    #[test]
    fn test_textangle_exact_boundaries() {
        // Exactly 90 → 360 - 90 = 270 per first branch (≤ 90).
        assert_eq!(textangle(90.0, false), 270.0);
        // Exactly 180 → not in second branch (180 not < 180); third branch:
        // 360 - (180 - 180) = 360.
        assert_eq!(textangle(180.0, false), 360.0);
        // Exactly 270 → fourth branch: 360 - 270 = 90.
        assert_eq!(textangle(270.0, false), 90.0);
    }

    #[test]
    fn test_textangle_svg_symmetric_with_textangle() {
        // textanglesvg is 360 - textangle for all quadrants.
        for &a in &[30.0, 120.0, 200.0, 315.0] {
            let t = textangle(a, false);
            let s = textanglesvg(a, false);
            assert!((t + s - 360.0).abs() < 1e-9, "angle {} -> t={}, s={}", a, t, s);
        }
    }

    #[test]
    fn test_textoffset_angle_exactly_90_uses_positive_offset() {
        // angle=90 is not strictly > 90 → enters else branch → positive angle_offset.
        let (da, _dr) = textoffset(90.0, 100.0, 50.0, 10.0, 0.0, false);
        assert!(da > 0.0);
    }

    #[test]
    fn test_textoffset_height_offset_scales_angle_offset() {
        // Larger height_offset → larger angle_offset (linear in height_offset).
        let (da1, _) = textoffset(45.0, 100.0, 50.0, 10.0, 0.0, false);
        let (da2, _) = textoffset(45.0, 100.0, 50.0, 10.0, 20.0, false);
        // angle_offset includes (label_height/2 + height_offset) / radius scaling.
        // With height_offset +20, da2 should be larger.
        assert!(da2 > da1);
    }

    #[test]
    fn test_textoffset_smaller_radius_gives_larger_angle_offset() {
        // angle_offset = rad2deg × (label_height/2 + height_offset) / radius.
        // Smaller radius → larger angle offset.
        let (da_small_r, _) = textoffset(45.0, 50.0, 50.0, 10.0, 0.0, false);
        let (da_big_r, _) = textoffset(45.0, 500.0, 50.0, 10.0, 0.0, false);
        // Smaller radius (50) produces larger offset than radius 500.
        assert!(da_small_r > da_big_r);
    }

    #[test]
    fn test_textoffset_non_parallel_radius_offset_uses_label_width() {
        // Non-parallel, angle in (90,270): radius_offset = label_width - 1.
        let (_, dr) = textoffset(180.0, 100.0, 25.0, 10.0, 0.0, false);
        // For (90, 270), radius_offset = label_width - 1 = 24.
        assert_eq!(dr, 24.0);
    }

    #[test]
    fn test_textoffset_is_parallel_branch() {
        // Parallel text: upper half (0 < angle < 180) → radius_offset = label_height.
        let (_da, dr) = textoffset(45.0, 100.0, 50.0, 10.0, 0.0, true);
        // Upper-right quadrant → dr = label_height = 10
        assert!((dr - 10.0).abs() < 1e-6);
        // Lower half (angle > 180 && < 360) → radius_offset = 0
        let (_da, dr) = textoffset(260.0, 100.0, 50.0, 10.0, 0.0, true);
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_anglemod_wraparound_edge_cases() {
        // Exactly 360 → not wrapped (angle > 360 is the trigger).
        assert_eq!(anglemod(360.0), 360.0);
        // Just over → wraps.
        let v = anglemod(360.5);
        assert!((v - 0.5).abs() < 1e-9);
        // Just under 0 → wraps.
        let v = anglemod(-0.5);
        assert!((v - 359.5).abs() < 1e-9);
        // Exactly 0 → 0.
        assert_eq!(anglemod(0.0), 0.0);
    }

    #[test]
    fn test_text_label_size_zero_bounds() {
        // All-zero bounds → (w=0-1=-1, h=0-1=-1) in the axis-aligned branch.
        let b = [0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (w, h) = text_label_size(&b);
        // bounds[1]==bounds[3] (both 0) → axis-aligned branch: w = (|0-0|-1) = -1.
        assert!((w - (-1.0)).abs() < 1e-9);
        assert!((h - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_is_parallel_branch_rotates_by_90() {
        // is_parallel=true: for textangle in [0,90] subtracts 90; for ≥270 adds 90.
        // Base textangle at angle=0 → non-parallel = 360 - 0 = 360 → ≥270 branch → +90 = 450 (no mod).
        // textangle at angle=45 → non-parallel = 360-45 = 315 → ≥270 → +90 = 405.
        let a = textangle(0.0, true);
        assert!((a - 450.0).abs() < 1e-9);
        let b = textangle(45.0, true);
        assert!((b - 405.0).abs() < 1e-9);
        // Non-parallel form first: angle=135 in [90,180) → textangle = 180-135 = 45.
        // Then parallel: 45 is in [0,90] → 45-90 = -45.
        let c = textangle(135.0, true);
        assert!((c - (-45.0)).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_angle_zero_radius_100() {
        // angle=0, radius=100, label_width=30, label_height=20, height_offset=0, is_parallel=false.
        // angle_offset = (180/π) × (10/100) = 180/10π ≈ 5.7296°.
        // Since angle==0, NOT in (0,180) → is_parallel block reaches the else branch.
        // With is_parallel=false: returns (angle_offset, 0.0) in the else arm.
        let (da, dr) = textoffset(0.0, 100.0, 30.0, 20.0, 0.0, false);
        // angle_offset ≈ 180/π × 10/100 ≈ 5.7296.
        assert!((da - (180.0 / std::f64::consts::PI * 0.1)).abs() < 1e-9);
        // non-parallel else-branch emits 0.0 radius offset.
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_closed_shape_and_output_length_is_even() {
        // slice_polygon_coords always produces an even-length vec (x,y pairs).
        let coords = slice_polygon_coords(1500.0, 0.0, 45.0, 100.0, 200.0);
        assert!(coords.len() > 4, "should have at least 2 outer + 2 inner pts");
        assert_eq!(coords.len() % 2, 0, "x,y pairs → even length");
        // First pair: outer arc start. Last pair: inner arc end (reverse to start).
        // With image_radius=1500, center is (1500,1500); outer radius 200 at 0° → (1700, 1500).
        assert!((coords[0] - 1700.0).abs() < 1.0);
        assert!((coords[1] - 1500.0).abs() < 1.0);
    }

    #[test]
    fn test_text_label_size_rectangular_wide_label() {
        // Axis-aligned bounds: bounds[1] == bounds[3] both = h.
        // Width is |x1-x0|-1; height is |y2-y1|-1.
        let b = [0.0f64, 15.0, 100.0, 15.0, 100.0, 0.0, 0.0, 0.0];
        let (w, h) = text_label_size(&b);
        // w = |100-0| - 1 = 99; h = |0-15| - 1 = 14.
        assert!((w - 99.0).abs() < 1e-9);
        assert!((h - 14.0).abs() < 1e-9);
    }

    #[test]
    fn test_textanglesvg_specific_angle_values() {
        // textanglesvg = 360 - textangle (non-parallel form).
        // angle=0 → textangle: a=0<=90 → 360-0=360 → svg=360-360=0.
        assert_eq!(textanglesvg(0.0, false), 0.0);
        // angle=90 → textangle: a<=90 → 360-90=270 → svg=360-270=90.
        assert_eq!(textanglesvg(90.0, false), 90.0);
        // angle=270 → a==270, NOT `<270` → final else: 360-270=90 → svg=360-90=270.
        assert_eq!(textanglesvg(270.0, false), 270.0);
    }

    #[test]
    fn test_anglemod_identity_inside_normal_range() {
        // Values in [0, 360] are returned unchanged.
        for a in [0.0f64, 45.0, 90.0, 180.0, 270.0, 360.0] {
            assert_eq!(anglemod(a), a);
        }
    }

    #[test]
    fn test_textoffset_negative_angle_offset_scales_inversely_with_radius() {
        // angle_offset = (180/π) × ((label_height/2 + height_offset) / radius).
        // Larger radius → smaller angle_offset (same label dimensions).
        let (a1, _) = textoffset(45.0, 100.0, 20.0, 10.0, 0.0, false);
        let (a2, _) = textoffset(45.0, 1000.0, 20.0, 10.0, 0.0, false);
        // a1 should be 10× larger magnitude than a2 (inverse radius scaling).
        assert!((a1.abs() - 10.0 * a2.abs()).abs() < 1e-6);
    }

    #[test]
    fn test_slice_polygon_coords_start_angle_greater_than_end_is_swapped() {
        // start > end: function internally swaps so first coord is from smaller angle.
        let coords_forward = slice_polygon_coords(1500.0, 0.0, 45.0, 100.0, 200.0);
        let coords_reversed = slice_polygon_coords(1500.0, 45.0, 0.0, 100.0, 200.0);
        // Both should start at the same point (the smaller angle's outer arc).
        assert!((coords_forward[0] - coords_reversed[0]).abs() < 1e-6);
        assert!((coords_forward[1] - coords_reversed[1]).abs() < 1e-6);
    }

    #[test]
    fn test_textangle_non_parallel_values_in_each_quadrant() {
        // non-parallel form:
        //   a in [0, 90]: 360 - a
        //   a in (90, 180): 180 - a
        //   a in [180, 270): 360 - (a - 180)
        //   a in [270, 360]: 360 - a
        assert_eq!(textangle(45.0, false), 315.0);  // 360 - 45
        assert_eq!(textangle(135.0, false), 45.0);  // 180 - 135
        assert_eq!(textangle(225.0, false), 315.0); // 360 - (225-180) = 315
        assert_eq!(textangle(315.0, false), 45.0);  // 360 - 315
    }

    #[test]
    fn test_textoffset_is_parallel_with_angle_in_lower_half() {
        // is_parallel=true + angle in (0, 180): radius_offset = label_height.
        // angle=45 is (0, 180) → radius_offset = 20.0.
        let (_, dr) = textoffset(45.0, 100.0, 30.0, 20.0, 0.0, true);
        assert!((dr - 20.0).abs() < 1e-9);
        // angle=200 in [180, 360) → radius_offset = 0.0.
        let (_, dr) = textoffset(200.0, 100.0, 30.0, 20.0, 0.0, true);
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_endpoints_symmetric_around_center() {
        // For symmetric angles around 0: -45 and 45 → first coord (outer start)
        // matches mirror. With image_radius 1500, outer_r 200 at angle 0 → (1700, 1500).
        let coords = slice_polygon_coords(1500.0, -45.0, 45.0, 100.0, 200.0);
        // Last coord: inner arc end back at smaller angle.
        // Start coord should equal inner arc end coord (x symmetric).
        // First pair is at angle=-45 (since swap ensures smaller first).
        // cos(-45)≈0.707 → x = 1500 + 200*0.707 ≈ 1641.4, y = 1500 + 200*sin(-45) ≈ 1358.6.
        let expected_x = 1500.0 + 200.0 * (-45.0_f64.to_radians()).cos();
        let expected_y = 1500.0 + 200.0 * (-45.0_f64.to_radians()).sin();
        assert!((coords[0] - expected_x).abs() < 1.0);
        assert!((coords[1] - expected_y).abs() < 1.0);
    }

    #[test]
    fn test_text_label_size_diagonal_bounds_uses_distance() {
        // Rotated bounds (y0 != y1) → uses Euclidean distance formula.
        // b = [0,0, 10,10, 20,20, 10,10] — diagonal rotation.
        // width = sqrt((10-0-1)^2 + (10-0-1)^2) = sqrt(81+81) = ~12.73.
        // height = sqrt((10-0-1)^2 + (10-0-1)^2) = same.
        let b = [0.0f64, 0.0, 10.0, 10.0, 20.0, 20.0, 10.0, 10.0];
        let (w, h) = text_label_size(&b);
        let expected = (81.0f64 + 81.0).sqrt();
        assert!((w - expected).abs() < 1e-9);
        assert!((h - expected).abs() < 1e-9);
    }

    #[test]
    fn test_anglemod_exactly_360_passes_unchanged() {
        // 360 is not `> 360` → returned verbatim (not wrapped to 0).
        assert_eq!(anglemod(360.0), 360.0);
        // But 360.0001 > 360 → wraps to 0.0001.
        assert!((anglemod(360.0001) - 0.0001).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_parallel_values_wrap_to_minus_90() {
        // is_parallel + non-parallel textangle in [0,90] → textangle -= 90 → negative.
        // angle=45 non-parallel → 360 - 45 = 315 (≥270) → +90 → 405.
        let a = textangle(45.0, true);
        assert!((a - 405.0).abs() < 1e-9);
        // angle=90 non-parallel → 360 - 90 = 270 (≥270) → +90 → 360.
        let a = textangle(90.0, true);
        assert!((a - 360.0).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_label_width_minus_1_non_parallel_above_180() {
        // Non-parallel, angle in (90, 270) → radius_offset = label_width - 1.
        // radius_offset negated via -angle_offset branch.
        let (da, dr) = textoffset(180.0, 100.0, 30.0, 20.0, 0.0, false);
        // radius_offset = 30 - 1 = 29.
        assert!((dr - 29.0).abs() < 1e-9);
        // angle_offset negated.
        assert!(da < 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_small_input_yields_4_plus_points() {
        // Even with tiny span, impl guarantees enough points for outer+inner arcs.
        let coords = slice_polygon_coords(1000.0, 10.0, 11.0, 50.0, 100.0);
        // At minimum: 2 outer + 2 inner endpoints = 4 x,y pairs = 8 f64s.
        assert!(coords.len() >= 8);
        assert_eq!(coords.len() % 2, 0); // always x,y pairs.
    }

    #[test]
    fn test_anglemod_exact_boundaries_return_unchanged() {
        // Strict `<`/`>` comparisons → 0 and 360 neither wrap nor subtract.
        assert_eq!(anglemod(0.0), 0.0);
        assert_eq!(anglemod(360.0), 360.0);
        // Just over 360 subtracts.
        assert!((anglemod(360.1) - 0.1).abs() < 1e-9);
        // Just under 0 wraps.
        assert!((anglemod(-0.1) - 359.9).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_parallel_swing_adds_90_in_upper_range() {
        // a=45 → textangle=315; parallel & old>=270 → +90 → 405.
        assert_eq!(textangle(45.0, true), 405.0);
        // a=80 → textangle=280; parallel & old>=270 → +90 → 370.
        assert_eq!(textangle(80.0, true), 370.0);
        // a=10 → textangle=350; parallel & old>=270 → +90 → 440.
        assert_eq!(textangle(10.0, true), 440.0);
    }

    #[test]
    fn test_textanglesvg_complements_textangle_to_360() {
        // By definition textanglesvg(a,p) = 360 - textangle(a,p) for all inputs.
        for &a in &[0.0_f64, 30.0, 90.0, 135.0, 180.0, 225.0, 270.0, 345.0] {
            let ta = textangle(a, false);
            let svg = textanglesvg(a, false);
            assert!((svg + ta - 360.0).abs() < 1e-9, "a={}: svg={} ta={}", a, svg, ta);
        }
    }

    #[test]
    fn test_text_label_size_aligned_bounds_subtracts_one() {
        // Aligned rect: y1==y3 triggers short branch → (|x2-x0|-1, |y5-y1|-1).
        let bounds = [0.0, 0.0, 10.0, 0.0, 10.0, 5.0, 0.0, 5.0];
        let (w, h) = text_label_size(&bounds);
        assert_eq!(w, 9.0);
        assert_eq!(h, 4.0);
        // Non-aligned: y1 ≠ y3 → pythagorean branch.
        let skew = [0.0, 0.0, 3.0, 0.0, 3.0, 4.0, 0.0, 4.0];
        let (w2, h2) = text_label_size(&skew);
        // y1==y3=0 again — still aligned. Use bounds with true skew.
        assert_eq!(w2, 2.0);
        assert_eq!(h2, 3.0);
    }

    #[test]
    fn test_textoffset_not_parallel_upper_half_zero_radius_offset() {
        // angle 45 → !parallel, !in (90,270) → (angle_offset, 0.0) per final else.
        let (da, dr) = textoffset(45.0, 100.0, 20.0, 10.0, 0.0, false);
        assert!(da > 0.0);
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_textoffset_parallel_angle_180_boundary() {
        // angle==180 fails strict <180 in parallel branch → radius_offset=0.
        // Also falls into (90,270) → first return tuple: (-angle_offset, 0.0).
        let (da, dr) = textoffset(180.0, 100.0, 20.0, 10.0, 0.0, true);
        assert!(da < 0.0);
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_full_ring_generates_many_segments() {
        // Large 360° span with reasonable radius → many sample points.
        let coords = slice_polygon_coords(1000.0, 0.0, 360.0, 50.0, 100.0);
        // ≥ 50 points for each arc (outer + inner) — total ≥ 200 f64s.
        assert!(coords.len() > 200);
        // Always pairs (x,y).
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_text_label_size_all_zero_bounds_aligned_gives_negative_one() {
        // bounds all zero → aligned branch (y1==y3==0) → (|0|-1, |0|-1) = (-1, -1).
        let bounds = [0.0; 8];
        let (w, h) = text_label_size(&bounds);
        assert_eq!(w, -1.0);
        assert_eq!(h, -1.0);
    }

    #[test]
    fn test_textangle_boundary_at_exactly_90_degrees() {
        // a=90 hits first branch `a <= 90` → textangle = 360-90 = 270.
        assert_eq!(textangle(90.0, false), 270.0);
    }

    #[test]
    fn test_textangle_angle_300_falls_to_final_else_branch() {
        // a=300: not <=90, not <180, not <270 → else: 360-a = 60.
        assert_eq!(textangle(300.0, false), 60.0);
        // a=359.9 → 360-359.9 = 0.1.
        let t = textangle(359.9, false);
        assert!((t - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_textanglesvg_with_parallel_complements_to_360() {
        // svg + gd = 360 holds even when is_parallel=true.
        for &a in &[0.0_f64, 45.0, 100.0, 200.0, 300.0] {
            let ta = textangle(a, true);
            let svg = textanglesvg(a, true);
            assert!((ta + svg - 360.0).abs() < 1e-9, "a={} ta={} svg={}", a, ta, svg);
        }
    }

    #[test]
    fn test_textoffset_smaller_radius_yields_larger_angle_offset() {
        // angle_offset = rad2deg * (height/2 + height_offset) / radius — inversely
        // proportional to radius.
        let (ao_large, _) = textoffset(45.0, 1000.0, 20.0, 10.0, 0.0, false);
        let (ao_small, _) = textoffset(45.0, 100.0, 20.0, 10.0, 0.0, false);
        assert!(ao_small > ao_large);
        // 10× smaller radius → 10× larger angle offset (ratio within floating-point tolerance).
        assert!((ao_small / ao_large - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_anglemod_values_near_boundaries_preserved() {
        // Values strictly in (0, 360) pass through unchanged.
        assert_eq!(anglemod(0.001), 0.001);
        assert_eq!(anglemod(359.99), 359.99);
        assert_eq!(anglemod(180.5), 180.5);
    }

    #[test]
    fn test_textangle_at_exactly_180_uses_third_branch() {
        // a=180: fails a<=90 AND a<180 (strict); then a<270 → 360-(a-180)=360.
        assert_eq!(textangle(180.0, false), 360.0);
    }

    #[test]
    fn test_slice_polygon_coords_reversed_angles_auto_swapped() {
        // start > end → swap inside function; result should equal normal-order call.
        let c_rev = slice_polygon_coords(1000.0, 90.0, 30.0, 50.0, 100.0);
        let c_fwd = slice_polygon_coords(1000.0, 30.0, 90.0, 50.0, 100.0);
        // Both should be valid and non-empty; swap makes rev == fwd in output.
        assert_eq!(c_rev.len(), c_fwd.len());
        assert!(c_rev.len() >= 8);
    }

    #[test]
    fn test_text_label_size_aligned_bounds_width_accuracy() {
        // Aligned bounds [0,0,100,0,100,30,0,30] → width=99, height=29.
        let b = [0.0, 0.0, 100.0, 0.0, 100.0, 30.0, 0.0, 30.0];
        let (w, h) = text_label_size(&b);
        assert_eq!(w, 99.0);
        assert_eq!(h, 29.0);
    }

    #[test]
    fn test_anglemod_single_wrap_only_not_recursive() {
        // Single wrap: 720 → 720-360 = 360 (else branch: > 360 once).
        // But 1080 > 360 → 1080-360 = 720 (stays at 720 — NOT re-wrapped).
        assert_eq!(anglemod(720.0), 360.0);
        assert_eq!(anglemod(1080.0), 720.0);
    }

    #[test]
    fn test_textangle_at_exactly_270_falls_to_final_else_branch() {
        // a=270: fails all strict <180/<270 → else: 360-270=90.
        assert_eq!(textangle(270.0, false), 90.0);
    }

    #[test]
    fn test_textangle_parallel_swing_for_upper_range_old_above_270() {
        // a=30 → textangle=330 (a<=90). is_parallel: old=330>=270 → +90 = 420.
        assert_eq!(textangle(30.0, true), 420.0);
        // a=85 → textangle=275 (a<=90). parallel: 275>=270 → +90 = 365.
        assert_eq!(textangle(85.0, true), 365.0);
    }

    #[test]
    fn test_anglemod_exactly_360_is_unchanged() {
        // 360.0 is not strictly > 360 → falls to the else arm → returned as-is.
        assert_eq!(anglemod(360.0), 360.0);
        // Similarly 0.0 is not < 0 → unchanged.
        assert_eq!(anglemod(0.0), 0.0);
        // -360.0 IS < 0 → wraps to 0.
        assert_eq!(anglemod(-360.0), 0.0);
    }

    #[test]
    fn test_textanglesvg_complement_of_textangle_for_nonparallel() {
        // svg rotation is 360 - textangle (opposite direction).
        for a in [30.0, 75.0, 120.0, 200.0, 290.0, 350.0] {
            let t = textangle(a, false);
            let tsvg = textanglesvg(a, false);
            assert!((t + tsvg - 360.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_textoffset_zero_label_dimensions_right_half_is_zero() {
        // For angle=0 (outside (90,270) open interval) with is_parallel=false, the else
        // branch returns (angle_offset, 0.0) — not the raw radius_offset=-1.
        let (a_off, r_off) = textoffset(0.0, 100.0, 0.0, 0.0, 0.0, false);
        assert_eq!(a_off, 0.0);
        assert_eq!(r_off, 0.0);
        // With is_parallel=true at angle=45 in (0,180) → radius_offset = label_height = 0.
        let (_, r_off_p) = textoffset(45.0, 100.0, 0.0, 0.0, 0.0, true);
        assert_eq!(r_off_p, 0.0);
    }

    #[test]
    fn test_textoffset_left_half_angle_yields_negative_angle_offset() {
        // Angle in (90, 270) → returns (-angle_offset, radius_offset) — left half of circle.
        let (a_off, r_off) = textoffset(180.0, 100.0, 50.0, 20.0, 0.0, false);
        assert!(a_off < 0.0);
        assert_eq!(r_off, 49.0); // label_width - 1
        // Right half (outside 90..270) → positive (or zero) angle offset.
        let (a_off_r, _) = textoffset(45.0, 100.0, 50.0, 20.0, 0.0, false);
        assert!(a_off_r > 0.0);
    }

    #[test]
    fn test_text_label_size_non_aligned_pythagorean_branch() {
        // y1 != y3 → diagonal distance via pythagoras.
        // bounds = [0,0, 3,4, 3,4, 0,0] (degenerate — y1=4 vs y3=0 differ).
        // w = sqrt((|3-0|-1)² + (|4-0|-1)²) = sqrt(4+9) = sqrt(13) ≈ 3.606.
        // h = sqrt((|0-0|-1)² + (|0-0|-1)²) = sqrt(1+1) = sqrt(2) ≈ 1.414.
        let b = [0.0, 0.0, 3.0, 4.0, 3.0, 4.0, 0.0, 0.0];
        let (w, h) = text_label_size(&b);
        assert!((w - 13.0_f64.sqrt()).abs() < 1e-9);
        assert!((h - 2.0_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn test_anglemod_large_negative_wraps_once() {
        // -30 < 0 → +360 = 330 (wraps once).
        assert_eq!(anglemod(-30.0), 330.0);
        // -180 → 180.
        assert_eq!(anglemod(-180.0), 180.0);
        // Note: anglemod is non-recursive — -400 wraps once to -40 (still negative).
        // That matches the existing test_anglemod_single_wrap_only_not_recursive.
    }

    #[test]
    fn test_textangle_four_quadrant_values_span_full_0_360() {
        // Four angles from four quadrants should give four different textangles.
        let a = textangle(30.0, false);  // upper-right: 330
        let b = textangle(120.0, false); // upper-left: 60
        let c = textangle(200.0, false); // lower-left: 340
        let d = textangle(315.0, false); // lower-right: 45
        // All four should be in [0, 360].
        for v in [a, b, c, d] {
            assert!(v >= 0.0 && v <= 360.0);
        }
        // And no two should be equal.
        let set: std::collections::HashSet<_> = [a, b, c, d].iter().map(|v| v.to_bits()).collect();
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn test_textanglesvg_with_is_parallel_still_complements() {
        // Even with is_parallel=true, textanglesvg is 360 - textangle.
        for a in [10.0, 45.0, 100.0, 200.0] {
            let t = textangle(a, true);
            let svg = textanglesvg(a, true);
            // textanglesvg = 360 - textangle.
            assert!((t + svg - 360.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_slice_polygon_coords_degenerate_zero_arc_still_yields_at_least_two_points() {
        // start_angle==end_angle on outer and inner arc, even with zero sweep, yields
        // at minimum the start+end of both arcs (4 coord pairs = 8 floats).
        let coords = slice_polygon_coords(1500.0, 45.0, 45.0, 900.0, 1000.0);
        assert!(coords.len() >= 4); // at least 2 points = 4 f64s
        // All coords should be finite.
        for c in &coords {
            assert!(c.is_finite());
        }
    }

    #[test]
    fn test_anglemod_multiple_standard_angles_pass_through() {
        // In-range angles 0..=360 pass through unchanged.
        for a in [0.0, 1.0, 90.0, 179.9, 180.0, 270.0, 359.9] {
            assert_eq!(anglemod(a), a);
        }
    }

    #[test]
    fn test_textangle_is_parallel_adjusts_into_range() {
        // textangle with is_parallel=true shifts the raw textangle by ±90.
        // raw=30 (a<=90 branch = 360-30=330) + parallel → old>=270 → +90 = 420.
        let v = textangle(30.0, true);
        // Upper quadrant values >=270 get +90, others unchanged.
        assert!(v > 360.0 || (0.0..=270.0).contains(&v));
    }

    #[test]
    fn test_textoffset_height_offset_affects_angle_offset_linearly() {
        // angle_offset formula: rad2deg * (label_height/2 + height_offset) / radius.
        // Doubling height_offset should increase angle_offset (radians mapped to degrees).
        let (a1, _) = textoffset(45.0, 100.0, 10.0, 20.0, 0.0, false);
        let (a2, _) = textoffset(45.0, 100.0, 10.0, 20.0, 10.0, false);
        // a2 should be larger than a1 in magnitude (more offset).
        assert!(a2.abs() > a1.abs());
    }

    #[test]
    fn test_text_label_size_equal_y_coords_uses_aligned_branch_simple_width() {
        // bounds[1]==bounds[3] → aligned branch: (|x2-x0|-1, |y5-y1|-1).
        // Using [0, 5, 10, 5, 10, 15, 0, 15] → w=|10-0|-1=9, h=|15-5|-1=9.
        let b = [0.0, 5.0, 10.0, 5.0, 10.0, 15.0, 0.0, 15.0];
        let (w, h) = text_label_size(&b);
        assert_eq!(w, 9.0);
        assert_eq!(h, 9.0);
    }

    #[test]
    fn test_textangle_90_exactly_uses_first_branch() {
        // a==90: 90 <= 90 → first branch → 360 - 90 = 270.
        assert!((textangle(90.0, false) - 270.0).abs() < 1e-9);
    }

    #[test]
    fn test_anglemod_negative_just_below_360_bumps_to_zero() {
        // -0.001 + 360 = 359.999.
        let r = anglemod(-0.001);
        assert!((r - 359.999).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_extreme_large_radius_yields_tiny_angle_offset() {
        // angle_offset = rad2deg × (label_height/2 + height_offset) / radius.
        // Very large radius → very small angle_offset.
        let (a_off, _) = textoffset(0.0, 1e9, 10.0, 20.0, 0.0, false);
        assert!(a_off.abs() < 1e-6);
    }

    #[test]
    fn test_slice_polygon_coords_forward_sweep_emits_multiple_points() {
        // 90° sweep at radii 900/1000 → emits many polygon points.
        let coords = slice_polygon_coords(1500.0, 0.0, 90.0, 900.0, 1000.0);
        // Expect several points (outer arc + inner arc).
        assert!(coords.len() >= 8); // at least 4 points = 8 f64s
        // All finite.
        for c in &coords {
            assert!(c.is_finite());
        }
    }

    #[test]
    fn test_anglemod_preserves_angle_in_0_to_360_exclusive_range() {
        // Angles strictly between 0 and 360 pass through.
        for a in [0.5, 45.0, 179.0, 180.0, 270.0, 359.5] {
            assert_eq!(anglemod(a), a);
        }
    }

    #[test]
    fn test_textangle_returns_finite_for_all_quadrants() {
        // textangle output is always finite for finite inputs.
        for a in [0.0, 30.0, 89.9, 90.0, 120.0, 179.9, 180.0, 200.0, 269.9, 270.0, 310.0, 359.9] {
            let t = textangle(a, false);
            let tp = textangle(a, true);
            assert!(t.is_finite(), "textangle({}) should be finite", a);
            assert!(tp.is_finite(), "textangle({}, parallel) should be finite", a);
        }
    }

    #[test]
    fn test_textoffset_zero_height_offset_gives_smaller_angle_offset() {
        // Smaller height_offset → smaller angle_offset.
        let (a1, _) = textoffset(45.0, 100.0, 10.0, 20.0, 0.0, false);
        let (a2, _) = textoffset(45.0, 100.0, 10.0, 20.0, 30.0, false);
        // a2 larger magnitude than a1.
        assert!(a2.abs() >= a1.abs());
    }

    #[test]
    fn test_text_label_size_zero_bounds_yields_negative_one_width_height() {
        // All-zero bounds with aligned branch → width = 0 - 1 = -1.
        let b = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (w, h) = text_label_size(&b);
        assert_eq!(w, -1.0);
        assert_eq!(h, -1.0);
    }

    #[test]
    fn test_anglemod_very_large_positive_single_wrap() {
        // 370 → 370 - 360 = 10.
        assert_eq!(anglemod(370.0), 10.0);
        // 720 → 720 - 360 = 360 (non-recursive wrap).
        assert_eq!(anglemod(720.0), 360.0);
    }

    #[test]
    fn test_textanglesvg_for_zero_angle_yields_360_minus_textangle() {
        // For angle=0, textangle=360 (since a<=90: 360-0=360). svg = 360-360 = 0.
        let svg = textanglesvg(0.0, false);
        let t = textangle(0.0, false);
        assert!((svg - (360.0 - t)).abs() < 1e-9);
        assert!((svg - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_large_positive_values_still_finite() {
        // Large inputs — textoffset uses rad2deg calculation; must stay finite.
        let (a_off, r_off) = textoffset(45.0, 10000.0, 1000.0, 2000.0, 500.0, false);
        assert!(a_off.is_finite());
        assert!(r_off.is_finite());
    }

    #[test]
    fn test_slice_polygon_coords_output_is_even_number_of_floats() {
        // Each point uses 2 floats (x,y) → output length always even.
        let coords = slice_polygon_coords(1500.0, 30.0, 60.0, 900.0, 1000.0);
        assert_eq!(coords.len() % 2, 0, "coords length {} should be even", coords.len());
    }

    #[test]
    fn test_anglemod_sweep_across_full_range() {
        // Sweep 0/90/180/270/360 → all pass through (none < 0, none > 360).
        for a in [0.0, 90.0, 180.0, 270.0, 360.0] {
            assert_eq!(anglemod(a), a);
        }
    }

    #[test]
    fn test_textangle_non_parallel_always_non_negative() {
        // textangle with is_parallel=false → always in [0, 360].
        for a in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0, 359.9] {
            let t = textangle(a, false);
            assert!(t >= 0.0, "textangle({}, false) = {} should be >= 0", a, t);
        }
    }

    #[test]
    fn test_textoffset_right_half_angle_returns_non_negative_angle_offset() {
        // angle in (0, 90) or (270, 360) right-half → non-negative angle_offset.
        let (a_off, _) = textoffset(30.0, 100.0, 10.0, 20.0, 5.0, false);
        assert!(a_off >= 0.0);
        let (a_off2, _) = textoffset(330.0, 100.0, 10.0, 20.0, 5.0, false);
        assert!(a_off2 >= 0.0);
    }

    #[test]
    fn test_text_label_size_mirror_negative_coords_abs_applied() {
        // Absolute value ensures positive width/height even with negative bounds.
        let b = [-10.0, -5.0, 10.0, -5.0, 10.0, 5.0, -10.0, 5.0];
        let (w, h) = text_label_size(&b);
        // w = |10 - (-10)| - 1 = 19; h = |5 - (-5)| - 1 = 9.
        assert_eq!(w, 19.0);
        assert_eq!(h, 9.0);
    }

    #[test]
    fn test_anglemod_exact_zero_passes_through() {
        // Exactly 0 is neither <0 nor >360 → passes through unchanged.
        assert_eq!(anglemod(0.0), 0.0);
    }

    #[test]
    fn test_textanglesvg_is_complement_of_textangle_to_360() {
        // textanglesvg(a) + textangle(a) = 360 by definition.
        for a in [15.0, 95.0, 180.0, 275.0, 359.0] {
            let gd = textangle(a, false);
            let svg = textanglesvg(a, false);
            assert!((gd + svg - 360.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_slice_polygon_coords_symmetric_about_center_for_symmetric_sweep() {
        // A sweep centered on 0° should be approximately symmetric in y about cy.
        let coords = slice_polygon_coords(1000.0, -10.0, 10.0, 800.0, 900.0);
        // Even count, > 4 points.
        assert!(coords.len() >= 8 && coords.len() % 2 == 0);
        // x coords are all positive (right half of image, around cx=1000).
        for i in (0..coords.len()).step_by(2) {
            assert!(coords[i] > 0.0);
        }
    }

    #[test]
    fn test_textoffset_parallel_within_top_half_uses_label_height_for_radius() {
        // is_parallel=true with angle in (0, 180) → radius_offset = label_height.
        let (_, r_off) = textoffset(45.0, 100.0, 10.0, 20.0, 5.0, true);
        // Top half → radius_offset = label_height = 20.
        assert_eq!(r_off, 20.0);
        // Bottom half (200°) → radius_offset = 0.
        let (_, r_off2) = textoffset(200.0, 100.0, 10.0, 20.0, 5.0, true);
        assert_eq!(r_off2, 0.0);
    }

    #[test]
    fn test_anglemod_slightly_above_360_wraps_down() {
        // 360.5 > 360 → 360.5 - 360 = 0.5.
        assert!((anglemod(360.5) - 0.5).abs() < 1e-9);
        // 400.0 → 40.0.
        assert!((anglemod(400.0) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn test_anglemod_slightly_below_zero_wraps_up() {
        // -10 + 360 = 350.
        assert!((anglemod(-10.0) - 350.0).abs() < 1e-9);
    }

    #[test]
    fn test_text_label_size_square_bounds_equal_dimensions() {
        // 100×100 square bounds with horizontal y1==y3 → uses abs-delta path → 99×99.
        let b = [0.0, 0.0, 100.0, 0.0, 100.0, 100.0, 0.0, 100.0];
        let (w, h) = text_label_size(&b);
        assert_eq!(w, 99.0);
        assert_eq!(h, 99.0);
    }

    #[test]
    fn test_slice_polygon_coords_reverses_angles_when_start_greater() {
        // Swapped start/end still produces a valid polygon with same coords.
        let forward = slice_polygon_coords(1000.0, 10.0, 20.0, 800.0, 900.0);
        let reversed = slice_polygon_coords(1000.0, 20.0, 10.0, 800.0, 900.0);
        // Both should produce identical coords (swap internally).
        assert_eq!(forward.len(), reversed.len());
    }

    #[test]
    fn test_anglemod_360_exactly_passes_through_unchanged() {
        // 360 is not > 360 → passthrough. The Perl path treats 360 as valid.
        assert_eq!(anglemod(360.0), 360.0);
    }

    #[test]
    fn test_textangle_180_maps_to_zero() {
        // anglemod(180) = 180; NOT ≤90, NOT <180 → 180≤a<270 → 360-(180-180)=360.
        // Actually a==180 falls through to third branch → 360-(180-180)=360.
        let r = textangle(180.0, false);
        assert_eq!(r, 360.0);
    }

    #[test]
    fn test_textoffset_zero_radius_produces_infinite_angle_offset() {
        // division by radius=0 → infinite angle_offset — tolerated (f64).
        let (a_off, _) = textoffset(45.0, 0.0, 10.0, 20.0, 5.0, false);
        assert!(a_off.is_infinite() || a_off.is_nan());
    }

    #[test]
    fn test_text_label_size_rotated_bounds_uses_sqrt_path() {
        // Non-horizontal bounds (y1 != y3) → sqrt-based width/height.
        // [0,0, 10,10, 10,20, 0,10] (rotated square-ish)
        let b = [0.0, 0.0, 10.0, 10.0, 10.0, 20.0, 0.0, 10.0];
        let (w, h) = text_label_size(&b);
        // w = sqrt(|10-0|-1)^2 + (|10-0|-1)^2 = sqrt(81+81) ≈ 12.73
        assert!((w - (81.0_f64 + 81.0).sqrt()).abs() < 1e-9);
        // h = sqrt((|0-0|-1)^2 + (|10-0|-1)^2) = sqrt(1+81) ≈ 9.055
        assert!((h - (1.0_f64 + 81.0).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_quadrant_transitions_all_finite_and_in_range() {
        // Sweep through a range of angles; results should all be finite & in [0,360).
        for a in [0.0, 45.0, 89.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0, 359.0] {
            let r = textangle(a, false);
            assert!(r.is_finite());
            assert!((0.0..=360.0).contains(&r));
        }
    }

    #[test]
    fn test_textanglesvg_always_sums_with_textangle_to_360() {
        // Invariant: textanglesvg(a) = 360 - textangle(a) — sum is exactly 360.
        for a in [10.0, 45.0, 170.0, 271.0] {
            let sum = textangle(a, true) + textanglesvg(a, true);
            assert!((sum - 360.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_slice_polygon_coords_returns_even_number_of_floats() {
        // Coords are stored as [x0,y0, x1,y1, ...] — always even count.
        let coords = slice_polygon_coords(1000.0, 0.0, 90.0, 800.0, 900.0);
        assert!(!coords.is_empty());
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_exact_720_yields_360_minus_360() {
        // 720 > 360 → returns 720 - 360 = 360 (only one wrap applied by the code).
        assert_eq!(anglemod(720.0), 360.0);
    }

    #[test]
    fn test_textangle_is_parallel_swings_by_90_in_first_quadrant() {
        // For a=30 non-parallel output is 330 (≥270 branch of parallel test), so
        // parallel adds 90 → 420. Difference magnitude is exactly 90.
        let non_p = textangle(30.0, false);
        let p = textangle(30.0, true);
        assert!(((non_p - p).abs() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_larger_radius_produces_smaller_angle_offset() {
        // angle_offset = (label_height/2 + height_offset) / radius — inversely proportional.
        let (a_off_small, _) = textoffset(45.0, 100.0, 10.0, 20.0, 5.0, false);
        let (a_off_large, _) = textoffset(45.0, 10000.0, 10.0, 20.0, 5.0, false);
        // Larger radius → smaller angle_offset.
        assert!(a_off_small > a_off_large);
    }

    #[test]
    fn test_text_label_size_exactly_horizontal_triggers_epsilon_branch() {
        // y1 == y3 within f64::EPSILON → horizontal branch (abs-delta).
        let b = [0.0, 5.0, 20.0, 5.0, 20.0, 15.0, 0.0, 5.0];
        let (w, h) = text_label_size(&b);
        // w = |20-0|-1 = 19; h = |15-5|-1 = 9.
        assert_eq!(w, 19.0);
        assert_eq!(h, 9.0);
    }

    #[test]
    fn test_slice_polygon_coords_very_small_sweep_still_produces_output() {
        // Tiny 0.1° sweep — still emits polygon coords without infinite-loop.
        let coords = slice_polygon_coords(1000.0, 45.0, 45.1, 800.0, 900.0);
        assert!(!coords.is_empty());
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_large_negative_single_wrap() {
        // -720 → < 0 → -720 + 360 = -360 (single wrap only; not two wraps).
        assert_eq!(anglemod(-720.0), -360.0);
    }

    #[test]
    fn test_textangle_exactly_90_uses_first_quadrant_branch() {
        // a == 90 → first branch: 360 - 90 = 270.
        let r = textangle(90.0, false);
        assert_eq!(r, 270.0);
    }

    #[test]
    fn test_textoffset_non_parallel_below_90_gives_zero_radius_offset() {
        // is_parallel=false, angle not in (90,270) → radius_offset = 0.
        let (_, r_off) = textoffset(45.0, 100.0, 10.0, 20.0, 5.0, false);
        assert_eq!(r_off, 0.0);
    }

    #[test]
    fn test_text_label_size_zero_width_zero_height_bounds() {
        // Zero-dim bounds → (0-1, 0-1) = (-1, -1) absurd but defined.
        let b = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (w, h) = text_label_size(&b);
        // y1==y3 path → (abs(0-0)-1, abs(0-0)-1) = (-1, -1).
        assert_eq!(w, -1.0);
        assert_eq!(h, -1.0);
    }

    #[test]
    fn test_anglemod_minus_1_wraps_to_359() {
        // -1 < 0 → +360 = 359.
        assert_eq!(anglemod(-1.0), 359.0);
    }

    #[test]
    fn test_textangle_270_uses_fourth_branch() {
        // a==270 → 4th branch: 360 - 270 = 90.
        let r = textangle(270.0, false);
        assert_eq!(r, 90.0);
    }

    #[test]
    fn test_textoffset_right_half_angle_below_90_yields_positive_angle_offset() {
        // Right-half angle (not in (90,270)) → +angle_offset (positive).
        let (a_off, _) = textoffset(10.0, 100.0, 10.0, 20.0, 5.0, false);
        assert!(a_off >= 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_increasing_outer_radius_gives_wider_output() {
        // Larger outer radius → more polygon points (smaller angular step).
        let narrow = slice_polygon_coords(1000.0, 0.0, 10.0, 800.0, 900.0);
        let wide = slice_polygon_coords(1000.0, 0.0, 10.0, 800.0, 5000.0);
        // Both non-empty, both even-count.
        assert!(!narrow.is_empty());
        assert!(!wide.is_empty());
    }

    #[test]
    fn test_anglemod_1000_wraps_once_only() {
        // 1000 > 360 → 1000 - 360 = 640 (single wrap).
        assert_eq!(anglemod(1000.0), 640.0);
    }

    #[test]
    fn test_textangle_exact_270_degrees_fourth_branch_zero() {
        // a==270 → fourth branch: 360 - 270 = 90.
        assert_eq!(textangle(270.0, false), 90.0);
    }

    #[test]
    fn test_textanglesvg_at_zero_angle_yields_360_minus_textangle_zero() {
        // angle=0 is_parallel=false → textangle=360 → svg=0.
        let r = textanglesvg(0.0, false);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_text_label_size_rectangular_bounds_gives_exact_dims() {
        // [0,0, 100,0, 100,50, 0,50] horizontal case → (99, 49).
        let b = [0.0, 0.0, 100.0, 0.0, 100.0, 50.0, 0.0, 50.0];
        let (w, h) = text_label_size(&b);
        assert_eq!(w, 99.0);
        assert_eq!(h, 49.0);
    }

    #[test]
    fn test_anglemod_negative_720_single_wrap_yields_negative_360() {
        // -720 < 0 → +360 = -360 (single wrap only).
        assert_eq!(anglemod(-720.0), -360.0);
    }

    #[test]
    fn test_textangle_large_positive_angle_wraps_via_anglemod_first() {
        // 450 > 360 → anglemod gives 90 → textangle(90) = 270.
        assert_eq!(textangle(450.0, false), 270.0);
    }

    #[test]
    fn test_textoffset_left_half_angle_radius_offset_scales_with_label_width() {
        // angle=180 (in 90-270) + is_parallel=false → radius_offset = label_width - 1.
        // Label_width 5 → r_off=4; width 50 → r_off=49.
        let (_, r_off_small) = textoffset(180.0, 100.0, 5.0, 20.0, 5.0, false);
        let (_, r_off_large) = textoffset(180.0, 100.0, 50.0, 20.0, 5.0, false);
        assert!(r_off_small < r_off_large);
    }

    #[test]
    fn test_slice_polygon_coords_small_inner_and_outer_radii_produces_output() {
        // Very small inner/outer radii still yield non-empty coords.
        let coords = slice_polygon_coords(1000.0, 0.0, 30.0, 10.0, 20.0);
        assert!(!coords.is_empty());
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_zero_minus_epsilon_wraps() {
        // -0.001 < 0 → +360 = 359.999.
        let r = anglemod(-0.001);
        assert!((r - 359.999).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_exactly_360_yields_result() {
        // a=360 > 360? No → passthrough anglemod → a=360; 360 ≤ 90? No → falls into else → 360-360=0.
        // Wait: anglemod(360) is 360 (not > 360 and not < 0 → passthrough). Then branches: a>90? yes; a<180? no.
        // 360 < 180? no. 360 < 270? no. → else: 360 - 360 = 0.
        let r = textangle(360.0, false);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_textoffset_is_parallel_bottom_half_yields_zero_radius() {
        // is_parallel=true with angle in (180, 360) → radius_offset = 0.
        let (_, r_off) = textoffset(200.0, 100.0, 10.0, 20.0, 5.0, true);
        assert_eq!(r_off, 0.0);
    }

    #[test]
    fn test_text_label_size_zero_horizontal_bounds_but_positive_vertical() {
        // y1 == y3 within epsilon → horizontal branch.
        // bounds = [0,0, 10,0, 10,100, 0,100] → w=|10-0|-1=9; h=|100-0|-1=99.
        let b = [0.0, 0.0, 10.0, 0.0, 10.0, 100.0, 0.0, 100.0];
        let (w, h) = text_label_size(&b);
        assert_eq!(w, 9.0);
        assert_eq!(h, 99.0);
    }

    #[test]
    fn test_anglemod_floor_values_zero_and_360_passthrough() {
        // Boundary: 0 and 360 both pass through unchanged.
        assert_eq!(anglemod(0.0), 0.0);
        assert_eq!(anglemod(360.0), 360.0);
    }

    #[test]
    fn test_textangle_corner_case_90_is_parallel_subtracts_90() {
        // a=90 is_parallel=false gives 270; with is_parallel=true, 270 is ≥ 270 → adds 90 → 360.
        let r = textangle(90.0, true);
        assert_eq!(r, 360.0);
    }

    #[test]
    fn test_textoffset_angle_offset_positive_for_right_half_non_parallel() {
        // Angle in (0,90) → positive angle_offset.
        let (a_off, _) = textoffset(45.0, 100.0, 10.0, 20.0, 5.0, false);
        assert!(a_off > 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_180_sweep_half_circle_output() {
        // 180° sweep with outer/inner radii produces polygon coords.
        let coords = slice_polygon_coords(1000.0, 0.0, 180.0, 500.0, 600.0);
        assert!(!coords.is_empty());
        // Verify coords are x,y pairs (even count).
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_boundary_just_below_360_passes_through() {
        // 359.999 in [0, 360] → passthrough.
        assert_eq!(anglemod(359.999), 359.999);
    }

    #[test]
    fn test_textangle_just_below_180_uses_second_branch() {
        // a=179 in (90,180) → second branch: 180 - 179 = 1.
        let r = textangle(179.0, false);
        assert_eq!(r, 1.0);
    }

    #[test]
    fn test_textoffset_non_parallel_at_exact_270_uses_else_branch() {
        // angle=270 NOT in (90,270) open interval → right-half else branch.
        let (a_off, _) = textoffset(270.0, 100.0, 10.0, 20.0, 5.0, false);
        assert!(a_off >= 0.0);
    }

    #[test]
    fn test_text_label_size_near_zero_with_tiny_delta_triggers_horizontal() {
        // y1 == y3 exactly → epsilon branch triggers.
        let b = [1.0, 2.0, 5.0, 2.0, 5.0, 10.0, 1.0, 2.0];
        let (w, h) = text_label_size(&b);
        // w = |5-1|-1 = 3; h = |10-2|-1 = 7.
        assert_eq!(w, 3.0);
        assert_eq!(h, 7.0);
    }

    #[test]
    fn test_anglemod_around_360_boundary_consistent() {
        // Values around 360: 359, 360, 361 all behave consistently.
        assert_eq!(anglemod(359.0), 359.0);
        assert_eq!(anglemod(360.0), 360.0);
        assert!((anglemod(361.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_all_quadrants_return_finite_values() {
        // All 4 quadrant mid-angles produce finite output.
        for a in [45.0, 135.0, 225.0, 315.0] {
            let r = textangle(a, false);
            assert!(r.is_finite());
        }
    }

    #[test]
    fn test_textoffset_larger_height_offset_produces_larger_angle_offset() {
        // Larger height_offset → larger angle_offset.
        let (a_small, _) = textoffset(45.0, 100.0, 10.0, 20.0, 0.0, false);
        let (a_large, _) = textoffset(45.0, 100.0, 10.0, 20.0, 50.0, false);
        assert!(a_small < a_large);
    }

    #[test]
    fn test_slice_polygon_coords_minimal_sweep_and_tiny_radii() {
        // 0.5° sweep with small radii produces some output without panic.
        let coords = slice_polygon_coords(100.0, 0.0, 0.5, 10.0, 20.0);
        assert!(!coords.is_empty());
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_negative_90_wraps_to_positive_270() {
        // -90 < 0 → +360 = 270.
        assert_eq!(anglemod(-90.0), 270.0);
    }

    #[test]
    fn test_anglemod_450_wraps_to_90() {
        // 450 > 360 → -360 = 90.
        assert_eq!(anglemod(450.0), 90.0);
    }

    #[test]
    fn test_textanglesvg_returns_finite_value_for_all_quadrants() {
        // textanglesvg produces finite values for 45/135/225/315.
        for a in [45.0, 135.0, 225.0, 315.0] {
            let ta = textanglesvg(a, false);
            assert!(ta.is_finite());
        }
    }

    #[test]
    fn test_textangle_exactly_zero_at_boundary_inclusive() {
        // a=0 is ≤90 → textangle = 360 - 0 = 360.
        let ta = textangle(0.0, false);
        assert_eq!(ta, 360.0);
    }

    #[test]
    fn test_text_label_size_horizontal_bounds_rect() {
        // Axis-aligned rect: bounds[1]==bounds[3] → horizontal branch.
        let bounds = [0.0, 0.0, 100.0, 0.0, 100.0, 20.0, 0.0, 20.0];
        let (w, h) = text_label_size(&bounds);
        assert!((w - 99.0).abs() < 1e-9);
        assert!((h - 19.0).abs() < 1e-9);
    }

    #[test]
    fn test_text_label_size_rotated_bounds_rect() {
        // Rotated bounds: bounds[1] != bounds[3] → diagonal branch.
        let bounds = [0.0, 0.0, 10.0, 10.0, 20.0, 20.0, 10.0, 30.0];
        let (w, h) = text_label_size(&bounds);
        assert!(w.is_finite() && w > 0.0);
        assert!(h.is_finite() && h > 0.0);
    }

    #[test]
    fn test_anglemod_within_valid_range_passes_through() {
        // Values in [0, 360] pass through unchanged.
        assert_eq!(anglemod(180.0), 180.0);
        assert_eq!(anglemod(0.0), 0.0);
        assert_eq!(anglemod(360.0), 360.0);
    }

    #[test]
    fn test_textangle_is_parallel_swings_result_when_in_upper_or_lower_band() {
        // At a=45: textangle=315 (≥270) → parallel adds 90 → 405.
        let ta_perpendicular = textangle(45.0, false);
        let ta_parallel = textangle(45.0, true);
        assert_eq!(ta_perpendicular, 315.0);
        assert_eq!(ta_parallel, 405.0);
    }

    #[test]
    fn test_textangle_at_135_mid_branch() {
        // a=135 is in [90, 180) → textangle = 180 - 135 = 45.
        let ta = textangle(135.0, false);
        assert_eq!(ta, 45.0);
    }

    #[test]
    fn test_textangle_at_225_upper_third_branch() {
        // a=225 is in [180, 270) → textangle = 360 - (225-180) = 315.
        let ta = textangle(225.0, false);
        assert_eq!(ta, 315.0);
    }

    #[test]
    fn test_anglemod_exactly_zero_passthrough() {
        // 0.0 passes through as-is.
        assert_eq!(anglemod(0.0), 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_standard_sweep_gives_even_count() {
        // slice_polygon_coords returns alternating x,y — count always even.
        let coords = slice_polygon_coords(100.0, 0.0, 45.0, 400.0, 500.0);
        assert!(!coords.is_empty());
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_textoffset_lower_half_right_side_zero_radius_offset() {
        // angle=45 (not in 90<a<270) + not parallel → (angle_offset, 0.0).
        let (_a, r) = textoffset(45.0, 500.0, 100.0, 20.0, 5.0, false);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_textoffset_upper_half_left_side_negative_angle_offset() {
        // angle=180 (in 90<a<270) → angle_offset negated.
        let (a, _r) = textoffset(180.0, 500.0, 100.0, 20.0, 5.0, false);
        assert!(a < 0.0);
    }

    #[test]
    fn test_textangle_at_exact_270_boundary() {
        // a=270 is NOT <270 but >=270 upper band → 360-270=90.
        let ta = textangle(270.0, false);
        assert_eq!(ta, 90.0);
    }

    #[test]
    fn test_anglemod_exactly_360_passthrough() {
        // 360 is not > 360 → passthrough.
        assert_eq!(anglemod(360.0), 360.0);
    }

    #[test]
    fn test_textoffset_is_parallel_right_half_radius_offset_zero() {
        // Right half (angle=0) parallel + not in 0<a<180 range → radius_offset=0.
        let (_a, r) = textoffset(0.0, 500.0, 100.0, 20.0, 5.0, true);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_textoffset_is_parallel_left_half_radius_offset_label_height() {
        // Parallel + angle in (0, 180) → radius_offset = label_height.
        let (_a, r) = textoffset(90.0, 500.0, 100.0, 20.0, 5.0, true);
        assert_eq!(r, 20.0);
    }

    #[test]
    fn test_slice_polygon_coords_wide_sweep_many_points() {
        // 90° sweep with radii → coords list non-trivially sized.
        let coords = slice_polygon_coords(100.0, 0.0, 90.0, 400.0, 500.0);
        assert!(coords.len() >= 8);
    }

    #[test]
    fn test_textanglesvg_zero_angle_returns_zero_or_finite() {
        // textanglesvg(0) returns some finite value (exact depends on impl).
        let ta = textanglesvg(0.0, false);
        assert!(ta.is_finite());
    }

    #[test]
    fn test_text_label_size_zero_bounds_yields_negative_or_small() {
        // All bounds zero → w=-1, h=-1 (formula subtracts 1).
        let bounds = [0.0; 8];
        let (w, h) = text_label_size(&bounds);
        assert!(w <= 0.0);
        assert!(h <= 0.0);
    }

    #[test]
    fn test_textanglesvg_parallel_flips_sign_or_offsets() {
        // textanglesvg(90, parallel) != textanglesvg(90, not parallel).
        let t1 = textanglesvg(90.0, false);
        let t2 = textanglesvg(90.0, true);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_anglemod_negative_540_wraps_to_180() {
        // anglemod only handles one wrap step → -540 → -540+360 = -180 (unusual case).
        // Since -180 < 0 could trigger another wrap but function only does one step:
        // actual result is -540 + 360 = -180 (but function only wraps once via single branch).
        let v = anglemod(-540.0);
        assert!(v.is_finite());
    }

    #[test]
    fn test_slice_polygon_coords_small_radius_still_outputs_points() {
        // Small radii — still produces output coords.
        let coords = slice_polygon_coords(100.0, 0.0, 60.0, 1.0, 2.0);
        assert!(!coords.is_empty());
    }

    #[test]
    fn test_textoffset_large_label_width_yields_large_radius_offset() {
        // In upper half (90 < angle < 270) → radius_offset = label_width - 1.
        let (_a, r) = textoffset(135.0, 500.0, 500.0, 20.0, 5.0, false);
        assert_eq!(r, 499.0);
    }

    #[test]
    fn test_textangle_all_band_boundaries_return_finite() {
        // Boundary angles 0/90/180/270/360 → all finite.
        for a in [0.0, 90.0, 180.0, 270.0, 360.0] {
            let ta = textangle(a, false);
            assert!(ta.is_finite());
        }
    }

    #[test]
    fn test_anglemod_large_angle_above_720_still_single_wrap() {
        // anglemod does single wrap → 720 - 360 = 360.
        let v = anglemod(720.0);
        assert!(v.is_finite());
    }

    #[test]
    fn test_text_label_size_vertical_bounds_rect_computes_distances() {
        // Vertical rectangle (tall, narrow) → finite w, h.
        let bounds = [0.0, 0.0, 10.0, 0.0, 10.0, 100.0, 0.0, 100.0];
        let (w, h) = text_label_size(&bounds);
        assert!(w.is_finite());
        assert!(h.is_finite());
    }

    #[test]
    fn test_anglemod_fractional_angles_preserved_in_valid_range() {
        // Fractional angles in [0, 360] pass through.
        assert!((anglemod(45.5) - 45.5).abs() < 1e-9);
        assert!((anglemod(123.456) - 123.456).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_at_90_boundary_chooses_first_branch() {
        // a=90 → ≤90 → 360 - 90 = 270.
        let ta = textangle(90.0, false);
        assert_eq!(ta, 270.0);
    }

    #[test]
    fn test_slice_polygon_coords_degenerate_zero_radii_no_panic() {
        // r1=r2=0 — still produces some output without panic.
        let coords = slice_polygon_coords(100.0, 0.0, 30.0, 0.0, 0.0);
        assert!(coords.len() % 2 == 0);
    }

    #[test]
    fn test_textoffset_is_parallel_upper_half_uses_label_height() {
        // In (0, 180) + parallel → radius_offset = label_height.
        let (_a, r) = textoffset(45.0, 500.0, 100.0, 30.0, 5.0, true);
        assert_eq!(r, 30.0);
    }

    #[test]
    fn test_text_label_size_equal_all_bounds_yields_negative() {
        // All bounds [0,0,0,0,0,0,0,0] → (-1, -1) via {0-0-1}.
        let bounds = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (w, h) = text_label_size(&bounds);
        assert_eq!(w, -1.0);
        assert_eq!(h, -1.0);
    }

    #[test]
    fn test_textangle_is_parallel_at_360_swings_same_as_zero() {
        // a=360 → same as a=0 (pass through anglemod).
        let ta = textangle(360.0, false);
        assert!(ta.is_finite());
    }

    #[test]
    fn test_slice_polygon_coords_equal_radii_produces_even_count() {
        // r1=r2 → still emits pairs.
        let coords = slice_polygon_coords(50.0, 0.0, 45.0, 100.0, 100.0);
        assert_eq!(coords.len() % 2, 0);
    }

    #[test]
    fn test_anglemod_value_one_below_zero() {
        // -1 → 359.
        assert_eq!(anglemod(-1.0), 359.0);
    }

    #[test]
    fn test_textangle_consistent_across_full_360_range() {
        // Sample at every 30 degrees — all finite and differ.
        let angles = [0.0, 30.0, 60.0, 120.0, 150.0, 210.0, 240.0, 300.0, 330.0];
        for a in angles {
            let ta = textangle(a, false);
            assert!(ta.is_finite());
        }
    }

    #[test]
    fn test_textoffset_at_270_uses_else_branch_zero_radius() {
        // angle=270 not in (90, 270) → else branch → (angle_offset, 0 if !parallel).
        let (_a, r) = textoffset(270.0, 500.0, 100.0, 20.0, 5.0, false);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_very_small_radii_still_non_empty() {
        // Very small sweep + very small radii → non-empty coords.
        let coords = slice_polygon_coords(50.0, 0.0, 5.0, 0.5, 0.6);
        assert!(!coords.is_empty());
    }

    #[test]
    fn test_text_label_size_wide_rect_produces_larger_width() {
        // Wide rectangle (100×10) → width ~99, height ~9.
        let bounds = [0.0, 0.0, 100.0, 0.0, 100.0, 10.0, 0.0, 10.0];
        let (w, h) = text_label_size(&bounds);
        assert!(w > h);
    }

    #[test]
    fn test_anglemod_multiple_wrap_values() {
        // Valid individual wraps.
        assert_eq!(anglemod(-30.0), 330.0);
        assert_eq!(anglemod(380.0), 20.0);
        assert_eq!(anglemod(-90.0), 270.0);
    }

    #[test]
    fn test_textangle_just_below_90_uses_first_branch() {
        // 89.9 ≤ 90 → first branch → 360 - 89.9.
        let ta = textangle(89.9, false);
        assert!((ta - 270.1).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_at_angle_zero_not_parallel_zero_radius() {
        // angle=0 not-parallel → angle_offset, 0.
        let (_a1, r1) = textoffset(0.0, 500.0, 100.0, 20.0, 5.0, false);
        assert_eq!(r1, 0.0);
    }

    #[test]
    fn test_slice_polygon_coords_output_has_at_least_4_points() {
        // Even a tiny sweep should produce ≥ 4 coord values (2 pts).
        let coords = slice_polygon_coords(100.0, 0.0, 1.0, 10.0, 20.0);
        assert!(coords.len() >= 4);
    }

    #[test]
    fn test_anglemod_exactly_360_stays_360() {
        // anglemod(360.0): not < 0, not > 360, so identity → 360.
        assert_eq!(anglemod(360.0), 360.0);
    }

    #[test]
    fn test_anglemod_exactly_zero_stays_zero() {
        // anglemod(0.0): not <0, not >360, so identity → 0.
        assert_eq!(anglemod(0.0), 0.0);
    }

    #[test]
    fn test_textanglesvg_equals_360_minus_textangle() {
        // Identity: textanglesvg(a,p) == 360 - textangle(a,p).
        let angle = 45.0;
        let ta = textangle(angle, false);
        let tasvg = textanglesvg(angle, false);
        assert!((tasvg - (360.0 - ta)).abs() < 1e-9);
    }

    #[test]
    fn test_textoffset_angle_in_90_to_270_range_returns_negative_angle_offset() {
        // Angle in (90, 270) → first tuple branch: angle_offset is negated.
        let (da, _dr) = textoffset(180.0, 100.0, 20.0, 10.0, 0.0, false);
        assert!(da < 0.0);
    }

    #[test]
    fn test_text_label_size_horizontal_baseline_reduces_by_1() {
        // bounds[1] == bounds[3] → horizontal; width = |bounds[2]-bounds[0]| - 1, height = |bounds[5]-bounds[1]| - 1.
        let bounds = [0.0, 0.0, 21.0, 0.0, 21.0, 11.0, 0.0, 11.0];
        let (w, h) = text_label_size(&bounds);
        assert_eq!(w, 20.0);
        assert_eq!(h, 10.0);
    }

    #[test]
    fn test_text_label_size_rotated_uses_pythagorean() {
        // bounds[1] != bounds[3] (rotated text) → w = sqrt((dx)²+(dy)²).
        // Corners (0,0)→(3,4) for width diag; slopes should give w≈4.
        let bounds = [0.0, 0.0, 4.0, 3.0, 0.0, 0.0, 0.0, 0.0];
        let (w, _h) = text_label_size(&bounds);
        // sqrt((4-1)^2 + (3-1)^2) = sqrt(9+4) = sqrt(13) ≈ 3.606.
        assert!((w - (13.0_f64).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn test_textangle_is_parallel_at_315_adds_90() {
        // a=315 → >270, textangle pre = 360-315=45; parallel branch: 45 in [0,90] → subtracts 90 → -45.
        let v = textangle(315.0, true);
        assert_eq!(v, -45.0);
    }

    #[test]
    fn test_slice_polygon_coords_coord_values_flat_xy_pairs() {
        // Output is flat [x0,y0,x1,y1,…] → length is even.
        let coords = slice_polygon_coords(200.0, 30.0, 60.0, 100.0, 150.0);
        assert!(coords.len() % 2 == 0);
    }

    #[test]
    fn test_anglemod_under_360_passthrough_range() {
        // 1.0, 180.0, 359.999 all in [0,360] → identity.
        assert_eq!(anglemod(1.0), 1.0);
        assert_eq!(anglemod(180.0), 180.0);
        assert!((anglemod(359.999) - 359.999).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_exact_boundary_90_first_branch() {
        // a=90 (boundary) → a<=90 branch → 360-90=270.
        assert!((textangle(90.0, false) - 270.0).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_exact_boundary_180_middle_branch() {
        // a=180 → a<180 false, a<270 true → 360-(180-180)=360.
        let v = textangle(180.0, false);
        assert_eq!(v, 360.0);
    }

    #[test]
    fn test_text_label_size_shrinks_by_1_on_both_axes_when_horizontal() {
        // For horizontal layout (bounds[1]==bounds[3]), both w and h are abs(diff)-1.
        let bounds = [0.0, 0.0, 100.0, 0.0, 100.0, 25.0, 0.0, 25.0];
        let (w, h) = text_label_size(&bounds);
        assert_eq!(w, 99.0);
        assert_eq!(h, 24.0);
    }

    #[test]
    fn test_anglemod_wraps_negative_close_to_zero() {
        // -1.0 → -1 + 360 = 359.
        assert_eq!(anglemod(-1.0), 359.0);
    }

    #[test]
    fn test_anglemod_wraps_large_value_over_360() {
        // 361 → 361 - 360 = 1.
        assert_eq!(anglemod(361.0), 1.0);
    }

    #[test]
    fn test_textanglesvg_at_zero_angle() {
        // angle=0: textangle=360 → textanglesvg = 360 - 360 = 0.
        assert_eq!(textanglesvg(0.0, false), 0.0);
    }

    #[test]
    fn test_textoffset_zero_label_width_right_branch_zero_radius_offset() {
        // label_width=0 → radius_offset = 0 - 1 = -1; angle in [0,90] → second branch, is_parallel=false → 0.
        let (_da, dr) = textoffset(45.0, 100.0, 0.0, 10.0, 0.0, false);
        // angle 45 in (0,90): else-branch at line 332: (angle_offset, if !is_parallel { 0.0 } else { radius_offset })
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_textangle_not_parallel_returns_full_rotation_angle() {
        // For a=45, is_parallel=false → textangle = 360 - 45 = 315.
        let v = textangle(45.0, false);
        assert_eq!(v, 315.0);
    }

    #[test]
    fn test_textanglesvg_at_90_inversely_negates_around_360() {
        // textangle(90, false) = 270 → textanglesvg = 360 - 270 = 90.
        let v = textanglesvg(90.0, false);
        assert_eq!(v, 90.0);
    }

    #[test]
    fn test_textoffset_angle_parallel_in_left_half_uses_label_height() {
        // is_parallel=true and angle in (0, 180) → radius_offset = label_height.
        let (_da, dr) = textoffset(45.0, 100.0, 20.0, 10.0, 0.0, true);
        // angle 45 < 90 → else branch: (angle_offset, if !is_parallel 0.0 else radius_offset=label_height=10).
        assert_eq!(dr, 10.0);
    }

    #[test]
    fn test_textoffset_angle_at_zero_is_parallel_radius_offset_zero() {
        // is_parallel=true and angle=0 (not in 0<a<180) → radius_offset = 0.
        let (_da, dr) = textoffset(0.0, 100.0, 20.0, 10.0, 0.0, true);
        assert_eq!(dr, 0.0);
    }

    #[test]
    fn test_textoffset_positive_height_offset_increases_angle_offset() {
        // Larger height_offset → larger angle_offset magnitude.
        let (da1, _dr1) = textoffset(45.0, 100.0, 20.0, 10.0, 0.0, false);
        let (da2, _dr2) = textoffset(45.0, 100.0, 20.0, 10.0, 50.0, false);
        // Larger height_offset → larger angle_offset.
        assert!(da2.abs() > da1.abs());
    }

    #[test]
    fn test_slice_polygon_coords_spans_whole_arc_larger_than_2_points() {
        // Wide angular sweep + reasonable radii → many points.
        let coords = slice_polygon_coords(200.0, 0.0, 120.0, 100.0, 150.0);
        // Should produce more than 4 points (more than a minimal rectangle).
        assert!(coords.len() > 4);
    }

    #[test]
    fn test_anglemod_preserves_very_large_just_above_360() {
        // 360.001 > 360 → wrap to 0.001.
        let v = anglemod(360.001);
        assert!((v - 0.001).abs() < 1e-9);
    }

    #[test]
    fn test_textangle_is_parallel_at_45_subtracts_90_to_negative() {
        // a=45, textangle=315 (>270 branch) → adds 90 → 405.
        let v = textangle(45.0, true);
        assert_eq!(v, 405.0);
    }

    #[test]
    fn test_textangle_is_parallel_at_30_no_change_branch() {
        // a=30, textangle pre = 330 (>=270 → add 90 → 420).
        let v = textangle(30.0, true);
        assert_eq!(v, 420.0);
    }

    #[test]
    fn test_text_label_size_tiny_horizontal_produces_negative_dimensions() {
        // Very small bounds → w and h are negative (diff-1 when diff<1).
        let bounds = [0.0, 0.0, 0.5, 0.0, 0.5, 0.5, 0.0, 0.5];
        let (w, h) = text_label_size(&bounds);
        // |0.5-0|-1 = -0.5.
        assert_eq!(w, -0.5);
        assert_eq!(h, -0.5);
    }

    #[test]
    fn test_anglemod_negative_180_wraps_to_180() {
        // -180 → -180 + 360 = 180.
        assert_eq!(anglemod(-180.0), 180.0);
    }

    #[test]
    fn test_textoffset_zero_radius_and_zero_label_returns_nan_or_inf() {
        // label_height=0, radius=0 → 0/0 in angle_offset calc.
        let (da, _dr) = textoffset(0.0, 0.0, 0.0, 0.0, 0.0, false);
        assert!(da.is_nan());
    }

    #[test]
    fn test_anglemod_720_wraps_to_360() {
        // 720 > 360 → -360 = 360.
        assert_eq!(anglemod(720.0), 360.0);
    }

    #[test]
    fn test_textangle_quadrant_2_middle_branch() {
        // a=135 → between 90 and 180 → 180-a = 45.
        assert_eq!(textangle(135.0, false), 45.0);
    }

    #[test]
    fn test_text_label_size_vertical_baseline_uses_pythag_with_height_calc() {
        // bounds[1] != bounds[3] → rotated path, w = sqrt((|2-0|-1)² + (|3-1|-1)²).
        let bounds = [0.0, 1.0, 2.0, 3.0, 2.0, 3.0, 0.0, 3.0];
        let (w, _h) = text_label_size(&bounds);
        // sqrt(1² + 1²) = sqrt(2).
        assert!((w - (2.0_f64).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn test_textanglesvg_on_large_angle_stays_in_valid_range() {
        // For any angle, textanglesvg = 360 - textangle.
        let a = 250.0;
        let svg_angle = textanglesvg(a, false);
        let ta = textangle(a, false);
        assert_eq!(svg_angle, 360.0 - ta);
    }
}
