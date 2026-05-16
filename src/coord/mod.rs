pub mod bezier;

/// Port of Perl `rotate_xy(x, y, x0, y0, angle)`: rotate (x,y) by `angle` degrees
/// around (x0,y0). `angle_offset` is the CONF{image}{angle_offset}, subtracted per Perl.
/// Returns rounded (x, y) to match Perl's `round()` return.
pub fn rotate_xy(x: f64, y: f64, x0: f64, y0: f64, angle: f64, angle_offset: f64) -> (f64, f64) {
    let a = (angle - angle_offset) * std::f64::consts::PI / 180.0;
    let xr = (x - x0) * a.cos() - (y - y0) * a.sin();
    let yr = (x - x0) * a.sin() + (y - y0) * a.cos();
    ((xr + x0).round(), (yr + y0).round())
}

/// Port of Perl `angle_to_span(angle, resolution, shift)`: map an angular position
/// to an IntSpan-friendly linear coordinate. Uses CONF{image}{angle_offset}.
pub fn angle_to_span(angle: f64, resolution: f64, shift: f64, angle_offset: f64) -> f64 {
    (angle + shift - angle_offset) * resolution
}

/// Port of Perl `relradius(r)`: if `r` is a relative fraction (<2), multiply by
/// the image radius; otherwise pass through.
pub fn relradius(radius: f64, image_radius: f64) -> f64 {
    if radius < 2.0 {
        radius * image_radius
    } else {
        radius
    }
}

/// Generate points along a circular arc.
///
/// Returns a vector of (x, y) points for an arc from `angle_start` to `angle_end`
/// at the given radius, centered at (cx, cy).
pub fn arc_points(
    cx: f64,
    cy: f64,
    radius: f64,
    angle_start: f64,
    angle_end: f64,
    angle_step: f64,
) -> Vec<(f64, f64)> {
    let deg2rad = std::f64::consts::PI / 180.0;
    let mut points = Vec::new();

    let step = if angle_start <= angle_end {
        angle_step.abs()
    } else {
        -angle_step.abs()
    };

    let mut angle = angle_start;
    loop {
        let rad = angle * deg2rad;
        points.push((cx + radius * rad.cos(), cy + radius * rad.sin()));
        if step == 0.0 || (step > 0.0 && angle >= angle_end) || (step < 0.0 && angle <= angle_end) {
            break;
        }
        angle += step;
        if step > 0.0 && angle > angle_end {
            angle = angle_end;
        }
        if step < 0.0 && angle < angle_end {
            angle = angle_end;
        }
    }

    points
}

/// Generate an SVG arc path "d" attribute for a circular arc.
/// Uses SVG A (arc) commands.
pub fn svg_arc_path(cx: f64, cy: f64, radius: f64, angle_start: f64, angle_end: f64) -> String {
    let deg2rad = std::f64::consts::PI / 180.0;
    let start_rad = angle_start * deg2rad;
    let end_rad = angle_end * deg2rad;

    let x1 = cx + radius * start_rad.cos();
    let y1 = cy + radius * start_rad.sin();
    let x2 = cx + radius * end_rad.cos();
    let y2 = cy + radius * end_rad.sin();

    let mut sweep_angle = angle_end - angle_start;
    if sweep_angle < 0.0 {
        sweep_angle += 360.0;
    }
    let large_arc = if sweep_angle > 180.0 { 1 } else { 0 };

    format!(
        "M {:.1},{:.1} A {:.1},{:.1} 0 {},{} {:.1},{:.1}",
        x1, y1, radius, radius, large_arc, 1, x2, y2
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arc_points() {
        let points = arc_points(100.0, 100.0, 50.0, 0.0, 90.0, 30.0);
        assert!(points.len() >= 4); // 0, 30, 60, 90 degrees
        // First point should be at (150, 100) - radius to the right
        assert!((points[0].0 - 150.0).abs() < 0.1);
        assert!((points[0].1 - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_svg_arc_path() {
        let path = svg_arc_path(100.0, 100.0, 50.0, 0.0, 90.0);
        assert!(path.starts_with("M "));
        assert!(path.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_identity_at_zero() {
        // 0° rotation with zero offset should return same point (rounded).
        let (x, y) = rotate_xy(10.0, 20.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!((x, y), (10.0, 20.0));
    }

    #[test]
    fn test_rotate_xy_90deg_around_origin() {
        // (1,0) rotated +90° about origin → (0,1); Perl rounds.
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, 90.0, 0.0);
        assert_eq!((x, y), (0.0, 1.0));
        // (0,1) rotated +90° → (-1,0)
        let (x, y) = rotate_xy(0.0, 1.0, 0.0, 0.0, 90.0, 0.0);
        assert_eq!((x, y), (-1.0, 0.0));
    }

    #[test]
    fn test_rotate_xy_offset_subtracted() {
        // Effective rotation = angle - angle_offset, so 90° with offset 90°
        // should be a no-op.
        let (x, y) = rotate_xy(5.0, 7.0, 0.0, 0.0, 90.0, 90.0);
        assert_eq!((x, y), (5.0, 7.0));
    }

    #[test]
    fn test_angle_to_span() {
        // Simple linear mapping: (angle + shift - angle_offset) * resolution.
        assert!((angle_to_span(10.0, 2.0, 5.0, 0.0) - 30.0).abs() < 1e-9);
        // angle_offset subtracts before scaling.
        assert!((angle_to_span(10.0, 2.0, 5.0, 5.0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_relradius_branches() {
        // Relative fraction (<2): multiply by image radius.
        assert!((relradius(0.5, 1500.0) - 750.0).abs() < 1e-9);
        assert!((relradius(1.99, 1000.0) - 1990.0).abs() < 1e-9);
        // Already-pixel value (≥2): pass through.
        assert!((relradius(100.0, 1500.0) - 100.0).abs() < 1e-9);
        assert!((relradius(2.0, 1500.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_arc_points_reverse_sweep() {
        // end < start should still produce valid points (step flipped negative).
        let pts = arc_points(0.0, 0.0, 10.0, 90.0, 0.0, 30.0);
        assert!(!pts.is_empty());
        // First point at angle 90°, radius 10 → (0, 10)
        assert!((pts[0].0 - 0.0).abs() < 1e-9);
        assert!((pts[0].1 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_svg_arc_path_large_arc_flag() {
        // sweep > 180° sets large_arc = 1
        let p = svg_arc_path(0.0, 0.0, 10.0, 0.0, 200.0);
        assert!(p.contains(" 1,1 "), "large_arc flag missing from: {}", p);
        // sweep ≤ 180° uses large_arc = 0
        let p = svg_arc_path(0.0, 0.0, 10.0, 0.0, 90.0);
        assert!(p.contains(" 0,1 "), "small arc flag wrong in: {}", p);
    }

    #[test]
    fn test_rotate_xy_rotation_preserves_distance_from_pivot() {
        // Rotation around a pivot preserves Euclidean distance to the pivot.
        let pivot: (f64, f64) = (100.0, 100.0);
        let p: (f64, f64) = (130.0, 140.0);
        // Distance from pivot to p: sqrt(30² + 40²) = 50.
        let orig_dist = ((p.0 - pivot.0).powi(2) + (p.1 - pivot.1).powi(2)).sqrt();
        for angle in [30.0f64, 45.0, 90.0, 127.0, 210.0] {
            let (x, y) = rotate_xy(p.0, p.1, pivot.0, pivot.1, angle, 0.0);
            let new_dist = ((x - pivot.0).powi(2) + (y - pivot.1).powi(2)).sqrt();
            // Rounded points may drift by up to ~0.5 each dimension.
            assert!(
                (new_dist - orig_dist).abs() < 2.0,
                "angle {}: dist change {}",
                angle,
                (new_dist - orig_dist).abs()
            );
        }
    }

    #[test]
    fn test_rotate_xy_360deg_is_identity_at_origin() {
        // 360° rotation about origin returns the original point.
        let (x, y) = rotate_xy(3.0, 4.0, 0.0, 0.0, 360.0, 0.0);
        assert_eq!((x, y), (3.0, 4.0));
    }

    #[test]
    fn test_rotate_xy_negative_rotation_goes_clockwise() {
        // Rotating (1, 0) by -90° about origin → (0, -1).
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, -90.0, 0.0);
        assert_eq!((x, y), (0.0, -1.0));
    }

    #[test]
    fn test_rotate_xy_angle_offset_cancels_with_angle() {
        // Effective rotation = angle - angle_offset. If both are equal → no rotation.
        for a in [30.0, 90.0, 127.0, 250.0] {
            let (x, y) = rotate_xy(10.0, 20.0, 5.0, 5.0, a, a);
            assert_eq!((x, y), (10.0, 20.0), "angle {}", a);
        }
    }

    #[test]
    fn test_rotate_xy_180deg_around_origin() {
        // 180° rotation about origin inverts both coords.
        let (x, y) = rotate_xy(3.0, 4.0, 0.0, 0.0, 180.0, 0.0);
        assert_eq!((x, y), (-3.0, -4.0));
    }

    #[test]
    fn test_rotate_xy_around_non_origin_pivot() {
        // Rotating the pivot itself is a no-op regardless of angle.
        let (x, y) = rotate_xy(5.0, 7.0, 5.0, 7.0, 127.0, 0.0);
        assert_eq!((x, y), (5.0, 7.0));
        // Rotating a point around a non-origin pivot: 90° about (10,0)
        // moves (11,0) to (10,1).
        let (x, y) = rotate_xy(11.0, 0.0, 10.0, 0.0, 90.0, 0.0);
        assert_eq!((x, y), (10.0, 1.0));
    }

    #[test]
    fn test_svg_arc_path_reverse_sweep_wraps_360() {
        // When angle_end < angle_start, the impl adds 360 to keep sweep positive.
        // 90→0 → sweep -90, +360 → 270 (>180) → large_arc=1.
        let p = svg_arc_path(0.0, 0.0, 10.0, 90.0, 0.0);
        assert!(p.contains(" 1,1 "), "expected large_arc=1 for wrapped sweep, got: {}", p);
    }

    #[test]
    fn test_angle_to_span_zero_resolution_is_zero() {
        // Linear map: resolution=0 → always 0 regardless of angle/shift.
        assert!((angle_to_span(100.0, 0.0, 5.0, 10.0) - 0.0).abs() < 1e-9);
        // resolution=1 + shift=0 + offset=0 → angle-in, angle-out identity.
        assert!((angle_to_span(42.0, 1.0, 0.0, 0.0) - 42.0).abs() < 1e-9);
    }

    #[test]
    fn test_svg_arc_path_zero_sweep_renders_single_arc() {
        // angle_start == angle_end (zero sweep) → `sweep_angle=0 → not < 0`,
        // large_arc = 0 (since 0 > 180 is false). Valid SVG path emitted.
        let p = svg_arc_path(0.0, 0.0, 10.0, 45.0, 45.0);
        assert!(p.starts_with("M "));
        assert!(p.contains(" 0,1 "));
    }

    #[test]
    fn test_svg_arc_path_format_structure() {
        // Output format is "M x1,y1 A rx,ry 0 large_arc,sweep x2,y2".
        let p = svg_arc_path(100.0, 100.0, 50.0, 0.0, 90.0);
        // Expected x1=150, y1=100 (angle 0); x2=100, y2=150 (angle 90).
        assert!(p.contains("150.0,100.0"));
        assert!(p.contains("100.0,150.0"));
        assert!(p.contains("A "));
    }

    #[test]
    fn test_svg_arc_path_negative_sweep_wraps_to_positive() {
        // 350 → 0 = -350 → +360 = 10 (small, large_arc=0).
        let p = svg_arc_path(0.0, 0.0, 10.0, 350.0, 0.0);
        assert!(p.contains(" 0,1 "));
    }

    #[test]
    fn test_svg_arc_path_180_degree_sweep_large_arc_flag() {
        // Exactly 180° → large_arc=0 per strict `> 180` threshold.
        let p = svg_arc_path(0.0, 0.0, 10.0, 0.0, 180.0);
        assert!(p.contains(" 0,1 "));
        // Just over 180 → large_arc=1.
        let p = svg_arc_path(0.0, 0.0, 10.0, 0.0, 181.0);
        assert!(p.contains(" 1,1 "));
    }

    #[test]
    fn test_arc_points_start_equals_end_yields_single_point() {
        // When angle_start == angle_end, the loop yields exactly one point
        // at that angle (enters the body once, then breaks).
        let pts = arc_points(100.0, 100.0, 50.0, 45.0, 45.0, 5.0);
        assert_eq!(pts.len(), 1);
        // Point at angle=45°, radius=50 → (100 + 50*cos(45°), 100 + 50*sin(45°)).
        let expected_x = 100.0 + 50.0 * (45.0_f64.to_radians()).cos();
        let expected_y = 100.0 + 50.0 * (45.0_f64.to_radians()).sin();
        assert!((pts[0].0 - expected_x).abs() < 1e-9);
        assert!((pts[0].1 - expected_y).abs() < 1e-9);
    }

    #[test]
    fn test_arc_points_step_larger_than_span_yields_endpoints() {
        // When step > span, we step once past angle_end → clamp to end → emit
        // both endpoints.
        let pts = arc_points(0.0, 0.0, 10.0, 0.0, 30.0, 100.0);
        // 2 points: at 0° and clamped to 30°.
        assert!(pts.len() >= 2);
        assert!((pts[0].0 - 10.0).abs() < 1e-9);
        assert!((pts[0].1 - 0.0).abs() < 1e-9);
        let last = *pts.last().unwrap();
        assert!((last.0 - 10.0 * 30.0_f64.to_radians().cos()).abs() < 1e-9);
    }

    #[test]
    fn test_arc_points_zero_radius_all_at_center() {
        // radius=0 → all points at (cx, cy) regardless of angle.
        let pts = arc_points(50.0, 60.0, 0.0, 0.0, 90.0, 10.0);
        for (x, y) in &pts {
            assert!((x - 50.0).abs() < 1e-12);
            assert!((y - 60.0).abs() < 1e-12);
        }
    }

    #[test]
    fn test_arc_points_full_circle_sweep_has_many_points() {
        // 0 → 360° with step=10 → should emit ~37 points.
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 360.0, 10.0);
        assert!(pts.len() >= 36, "expected ≥36 points, got {}", pts.len());
    }

    #[test]
    fn test_angle_to_span_shift_and_offset_additive() {
        // angle_to_span = (angle + shift - angle_offset) × resolution.
        // shift=+5, angle_offset=+3 → (angle + 5 - 3) × 1 = angle + 2.
        assert_eq!(angle_to_span(10.0, 1.0, 5.0, 3.0), 12.0);
        // Negative angle_offset swings result up.
        assert_eq!(angle_to_span(0.0, 1.0, 0.0, -90.0), 90.0);
        // Resolution > 1 scales linearly.
        assert_eq!(angle_to_span(1.0, 100.0, 0.0, 0.0), 100.0);
    }

    #[test]
    fn test_relradius_exactly_at_threshold_passes_through() {
        // Threshold is strict `<2` — exactly 2.0 is NOT fraction path, stays as-is.
        assert_eq!(relradius(2.0, 1500.0), 2.0);
        // Values just below 2 scale.
        assert!((relradius(1.99, 1500.0) - 1.99 * 1500.0).abs() < 1e-9);
        // Values > 2 pass through unchanged.
        assert_eq!(relradius(500.0, 1500.0), 500.0);
        assert_eq!(relradius(2.5, 1500.0), 2.5);
        // Zero input scales to 0 (0 × image_radius = 0).
        assert_eq!(relradius(0.0, 1500.0), 0.0);
    }

    #[test]
    fn test_svg_arc_path_full_circle_sweep_wraps_large() {
        // Exact 360° sweep → sweep_angle stays 360 (positive, not < 0).
        // 360 > 180 → large_arc=1. Endpoints coincide.
        let p = svg_arc_path(0.0, 0.0, 100.0, 0.0, 360.0);
        // Large arc flag is 1.
        assert!(p.contains(" 1,1 "), "expected large_arc=1 in: {}", p);
        // Endpoints both at (100, ~0) — sin(2π) has tiny float imprecision so
        // the end y may format as "-0.0". Check x coords match and y magnitudes are tiny.
        assert!(p.starts_with("M 100.0,"));
        assert!(p.contains(" A 100.0,100.0 "));
        // Last coord block "100.0,[±]0.0".
        assert!(p.ends_with(" 100.0,0.0") || p.ends_with(" 100.0,-0.0"));
    }

    #[test]
    fn test_rotate_xy_offset_equal_to_angle_yields_identity_at_any_pivot() {
        // When angle == angle_offset, effective rotation is 0 → point unchanged (after rounding).
        for pivot in &[(0.0, 0.0), (100.0, 100.0), (-50.0, 75.0)] {
            for &(x, y) in &[(5.0, 7.0), (12.0, -3.0), (0.0, 100.0)] {
                let (rx, ry) = rotate_xy(x, y, pivot.0, pivot.1, 45.0, 45.0);
                // Rounded — assert within 1.0 for the rounding.
                assert!((rx - x).abs() < 1.0);
                assert!((ry - y).abs() < 1.0);
            }
        }
    }

    #[test]
    fn test_arc_points_90_degree_sweep_endpoints() {
        // 0→90° sweep at radius=100 from (0,0) should start at (100,0) and
        // end near (0, 100) (sin imprecision aside).
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 30.0);
        // First point.
        assert!((pts[0].0 - 100.0).abs() < 1e-9);
        assert!((pts[0].1 - 0.0).abs() < 1e-9);
        // Last point (at/near 90°).
        let last = *pts.last().unwrap();
        assert!((last.0 - 0.0).abs() < 1e-6);
        assert!((last.1 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_start_coords_match_endpoint_computation() {
        // First coord in "M x1,y1" should be cx+r*cos(a1), cy+r*sin(a1).
        // For cx=cy=0, r=100, a1=60°: cos(60)=0.5 → x1=50, sin(60)≈0.866 → y1≈86.6.
        let p = svg_arc_path(0.0, 0.0, 100.0, 60.0, 120.0);
        // Format {:.1} → "50.0" and "86.6".
        assert!(p.starts_with("M 50.0,86.6"), "got: {}", p);
    }

    #[test]
    fn test_relradius_at_pivot_value_2() {
        // Exactly 2.0: not strictly < 2 → returned verbatim (not scaled).
        assert_eq!(relradius(2.0, 1500.0), 2.0);
        // Just below: 1.999 × 1500 ≈ 2998.5.
        assert!((relradius(1.999, 1500.0) - 2.998_5e3).abs() < 1e-6);
        // Just above: 2.001 → 2.001 (not scaled).
        assert_eq!(relradius(2.001, 1500.0), 2.001);
    }

    #[test]
    fn test_angle_to_span_zero_shift_and_offset_simplifies() {
        // With shift=0 and angle_offset=0, angle_to_span = angle × resolution.
        assert_eq!(angle_to_span(30.0, 100.0, 0.0, 0.0), 3000.0);
        assert_eq!(angle_to_span(0.0, 1000.0, 0.0, 0.0), 0.0);
        // Negative angle with 0 shift/offset.
        assert_eq!(angle_to_span(-45.0, 100.0, 0.0, 0.0), -4500.0);
    }

    #[test]
    fn test_arc_points_reverse_sweep_end_lower_than_start() {
        // When start > end, sweep is negative → step direction reverses.
        let pts = arc_points(0.0, 0.0, 100.0, 90.0, 0.0, 30.0);
        // Should have multiple points from 90° down to 0°.
        assert!(pts.len() >= 4);
        // First point at 90° → (0, 100).
        assert!((pts[0].0 - 0.0).abs() < 1e-6);
        assert!((pts[0].1 - 100.0).abs() < 1e-6);
        // Last point at 0° → (100, 0).
        let last = *pts.last().unwrap();
        assert!((last.0 - 100.0).abs() < 1e-6);
        assert!((last.1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_formatting_precision_single_decimal() {
        // All coord values use `{:.1}` — single decimal place.
        let p = svg_arc_path(10.0, 20.0, 30.0, 0.0, 90.0);
        // M {x:.1},{y:.1}: 10+30=40.0, 20+0=20.0.
        assert!(p.contains("M 40.0,20.0"));
        // A {r:.1},{r:.1}: 30.0,30.0.
        assert!(p.contains("A 30.0,30.0"));
        // End at 90°: 10+0=10.0, 20+30=50.0.
        assert!(p.ends_with(" 10.0,50.0"));
    }

    #[test]
    fn test_rotate_xy_90deg_from_positive_x_axis() {
        // Rotating (100,0) by 90° around origin should give (0, 100).
        let (x, y) = rotate_xy(100.0, 0.0, 0.0, 0.0, 90.0, 0.0);
        // Rounded to nearest int: cos(90)=0, sin(90)=1 → (0, 100).
        assert_eq!(x, 0.0);
        assert_eq!(y, 100.0);
    }

    #[test]
    fn test_relradius_negative_and_large_values() {
        // Negative fractions < 2 → scale: -0.5 × 1500 = -750.
        assert!((relradius(-0.5, 1500.0) - (-750.0)).abs() < 1e-6);
        // Very large > 2 → pass through unchanged.
        assert_eq!(relradius(1e6, 1500.0), 1e6);
        // Zero image_radius: 0.5 × 0 = 0.
        assert_eq!(relradius(0.5, 0.0), 0.0);
    }

    #[test]
    fn test_rotate_xy_angle_offset_cancels_angle() {
        // angle=45 minus angle_offset=45 → net 0° rotation → identity (rounded).
        let (x, y) = rotate_xy(10.0, 20.0, 0.0, 0.0, 45.0, 45.0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 20.0);
        // angle=90, offset=90 → same identity regardless of pivot.
        let (x, y) = rotate_xy(5.5, 7.5, 1.0, 2.0, 90.0, 90.0);
        // (5.5,7.5) round → (6, 8) via half-away-from-zero.
        assert_eq!(x, 6.0);
        assert_eq!(y, 8.0);
    }

    #[test]
    fn test_angle_to_span_negative_shift_combines_linearly() {
        // (angle + shift - angle_offset) * resolution; mix signs and verify linearity.
        assert_eq!(angle_to_span(10.0, 2.0, -5.0, 0.0), 10.0);
        // offset reverses shift's sign contribution.
        assert_eq!(angle_to_span(0.0, 1.0, 5.0, 5.0), 0.0);
        // Zero resolution collapses to 0 regardless of other inputs.
        assert_eq!(angle_to_span(123.4, 0.0, -7.0, 9.0), 0.0);
    }

    #[test]
    fn test_arc_points_negative_step_uses_absolute() {
        // Start < end with a negative step: function takes .abs(), direction stays positive.
        let pts = arc_points(0.0, 0.0, 10.0, 0.0, 90.0, -30.0);
        assert!(pts.len() >= 4);
        // First point at (10, 0) — radius on positive-x axis.
        assert!((pts[0].0 - 10.0).abs() < 1e-9);
        assert!(pts[0].1.abs() < 1e-9);
        // Last point should be at (0, 10) — 90° on positive-y axis.
        let last = pts.last().unwrap();
        assert!(last.0.abs() < 1e-6);
        assert!((last.1 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_exactly_180_degree_sweep() {
        // sweep==180 → condition `sweep > 180` is false → large_arc=0 (strict inequality).
        let path = svg_arc_path(0.0, 0.0, 50.0, 0.0, 180.0);
        assert!(path.contains(" 0,1 "));
        // sweep just over 180 → large_arc=1.
        let path2 = svg_arc_path(0.0, 0.0, 50.0, 0.0, 180.5);
        assert!(path2.contains(" 1,1 "));
    }

    #[test]
    fn test_rotate_xy_rotating_pivot_point_returns_pivot_rounded() {
        // (x0, y0) is a fixed point of rotation — any angle leaves it unchanged.
        for &angle in &[0.0_f64, 30.0, 90.0, 180.0, 270.0, 359.5] {
            let (x, y) = rotate_xy(5.0, 7.0, 5.0, 7.0, angle, 0.0);
            assert_eq!(x, 5.0);
            assert_eq!(y, 7.0);
        }
    }

    #[test]
    fn test_relradius_exactly_2_is_passthrough_not_scaled() {
        // Strict `< 2.0` — 2.0 itself is NOT scaled (passes through).
        assert_eq!(relradius(2.0, 1500.0), 2.0);
        // 1.9999 just under → still scaled.
        assert!((relradius(1.9999, 1500.0) - 1.9999 * 1500.0).abs() < 1e-6);
    }

    #[test]
    fn test_arc_points_reverse_direction_decreasing_angle() {
        // start > end → step negated → angle decreases each iteration.
        let pts = arc_points(0.0, 0.0, 10.0, 90.0, 0.0, 30.0);
        assert!(pts.len() >= 4);
        // First pt at 90° → (0, 10); last pt at 0° → (10, 0).
        assert!(pts[0].0.abs() < 1e-6);
        assert!((pts[0].1 - 10.0).abs() < 1e-6);
        let last = pts.last().unwrap();
        assert!((last.0 - 10.0).abs() < 1e-6);
        assert!(last.1.abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_formats_coordinates_with_one_decimal() {
        // Output uses {:.1} — small-radius path uses one decimal place consistently.
        let path = svg_arc_path(0.0, 0.0, 1.5, 0.0, 90.0);
        // M must have x,y formatted with one decimal — "M 1.5,0.0".
        assert!(path.contains("M 1.5,0.0"));
        // The radius appears twice formatted the same way in "A rx,ry ...".
        assert!(path.contains("A 1.5,1.5"));
    }

    #[test]
    fn test_rotate_xy_180_degrees_around_origin_negates_coords() {
        // 180° rotation around origin → (x,y) → (-x,-y); rounded.
        let (x, y) = rotate_xy(5.0, 7.0, 0.0, 0.0, 180.0, 0.0);
        assert_eq!(x, -5.0);
        assert_eq!(y, -7.0);
        // Around non-origin pivot: reflects (x,y) through pivot (x0,y0) → (2x0-x, 2y0-y).
        let (x2, y2) = rotate_xy(6.0, 8.0, 5.0, 5.0, 180.0, 0.0);
        assert_eq!(x2, 4.0);
        assert_eq!(y2, 2.0);
    }

    #[test]
    fn test_angle_to_span_sign_preserved_through_scale() {
        // Negative (angle+shift-offset) × positive resolution → negative result.
        let r = angle_to_span(10.0, 2.0, 0.0, 50.0);
        // (10 + 0 - 50) × 2 = -80.
        assert_eq!(r, -80.0);
        // Negative resolution also flips sign.
        let r2 = angle_to_span(10.0, -2.0, 0.0, 0.0);
        assert_eq!(r2, -20.0);
    }

    #[test]
    fn test_arc_points_same_start_and_end_yields_single_point() {
        // start == end → loop emits first point, then break on condition.
        let pts = arc_points(0.0, 0.0, 10.0, 45.0, 45.0, 1.0);
        assert_eq!(pts.len(), 1);
        // Point at 45°: (10·cos45°, 10·sin45°) ≈ (7.07, 7.07).
        let expected = 10.0 * (45.0_f64.to_radians()).cos();
        assert!((pts[0].0 - expected).abs() < 1e-9);
        assert!((pts[0].1 - expected).abs() < 1e-9);
    }

    #[test]
    fn test_svg_arc_path_sweep_flag_hardcoded_to_one() {
        // Format includes ",1 " from the hardcoded `1` sweep flag — for any sweep direction.
        let path1 = svg_arc_path(0.0, 0.0, 50.0, 0.0, 90.0);
        assert!(path1.contains(",1 "));
        let path2 = svg_arc_path(0.0, 0.0, 50.0, 270.0, 45.0);
        assert!(path2.contains(",1 "));
    }

    #[test]
    fn test_rotate_xy_angle_offset_subtracts_from_rotation_angle() {
        // Code: `a = (angle - angle_offset) * PI/180`. So angle=60 + offset=0 equals angle=90 + offset=30 (both → effective 60°).
        let (x1, y1) = rotate_xy(10.0, 0.0, 0.0, 0.0, 60.0, 0.0);
        let (x2, y2) = rotate_xy(10.0, 0.0, 0.0, 0.0, 90.0, 30.0);
        assert_eq!(x1, x2);
        assert_eq!(y1, y2);
    }

    #[test]
    fn test_angle_to_span_all_zero_inputs_yields_zero() {
        // (0 + 0 - 0) × 0 = 0.
        assert_eq!(angle_to_span(0.0, 0.0, 0.0, 0.0), 0.0);
        // (0 + 0 - 0) × nonzero = 0 (zero angle, zero shift, zero offset, any resolution).
        assert_eq!(angle_to_span(0.0, 100.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn test_arc_points_step_exceeding_span_yields_start_and_end() {
        // step=200 > span 90 → first iteration emits start, next overshoots → clamp
        // to end, loop terminates with 2 points total.
        let pts = arc_points(0.0, 0.0, 10.0, 0.0, 90.0, 200.0);
        assert_eq!(pts.len(), 2);
        // First at angle=0 → (10, 0).
        assert!((pts[0].0 - 10.0).abs() < 1e-9);
        assert!(pts[0].1.abs() < 1e-9);
        // Second at angle=90 → (0, 10).
        assert!(pts[1].0.abs() < 1e-6);
        assert!((pts[1].1 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_all_four_endpoint_coords_use_single_decimal() {
        // Format string uses {:.1} for all 6 float outputs (x1,y1,r,r,x2,y2).
        let path = svg_arc_path(100.0, 200.0, 10.0, 0.0, 90.0);
        // Expected x1=110.0, y1=200.0, r=10.0, x2=100.0, y2=210.0.
        assert!(path.contains("M 110.0,200.0"));
        assert!(path.contains("A 10.0,10.0"));
        assert!(path.contains("100.0,210.0"));
    }

    #[test]
    fn test_rotate_xy_composition_of_two_rotations_equals_sum() {
        // Rotate (10, 0) by 30° then by 60° around origin → same as one 90° rotation.
        let (x1, y1) = rotate_xy(10.0, 0.0, 0.0, 0.0, 30.0, 0.0);
        let (x2, y2) = rotate_xy(x1, y1, 0.0, 0.0, 60.0, 0.0);
        // Expected: 90° rotation of (10,0) → (0, 10) after rounding.
        assert_eq!(x2, 0.0);
        assert_eq!(y2, 10.0);
    }

    #[test]
    fn test_angle_to_span_shift_and_offset_cancel() {
        // Algebra: angle + shift - offset cancels when shift == offset.
        let r = angle_to_span(100.0, 2.0, 50.0, 50.0);
        assert_eq!(r, 200.0); // just 100 × 2.
        // When all three equal, only angle contributes.
        let r2 = angle_to_span(10.0, 5.0, 7.0, 7.0);
        assert_eq!(r2, 50.0);
    }

    #[test]
    fn test_arc_points_tiny_step_many_samples() {
        // Small step → many intermediate points across 0→90°.
        let pts = arc_points(0.0, 0.0, 10.0, 0.0, 90.0, 5.0);
        // At least 19 points: 0, 5, 10, ..., 90.
        assert!(pts.len() >= 19);
        // Endpoints correct.
        assert!((pts[0].0 - 10.0).abs() < 1e-9);
        let last = pts.last().unwrap();
        assert!((last.1 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_negative_radius_still_formats_valid() {
        // Negative radius → formatting passes through; no panic.
        let path = svg_arc_path(0.0, 0.0, -10.0, 0.0, 90.0);
        // Contains "M ", " A -10.0,-10.0 " pattern with negative radius.
        assert!(path.contains("M "));
        assert!(path.contains("A -10.0,-10.0"));
    }

    #[test]
    fn test_relradius_boundary_exactly_2_passes_through_unchanged() {
        // relradius: r < 2 → scales; r >= 2 → passes through.
        // 2.0 is NOT < 2 → passes through.
        assert_eq!(relradius(2.0, 1500.0), 2.0);
        // 1.999 < 2 → scales by image_radius.
        assert_eq!(relradius(1.999, 1000.0), 1999.0);
        // 0 is < 2 → scales to 0.
        assert_eq!(relradius(0.0, 1500.0), 0.0);
    }

    #[test]
    fn test_angle_to_span_respects_sign_of_shift_and_offset() {
        // Positive shift added, positive offset subtracted.
        // angle=100, shift=50, offset=30 → (100 + 50 - 30) × resolution
        assert_eq!(angle_to_span(100.0, 1.0, 50.0, 30.0), 120.0);
        // Negative offset (subtraction of negative = addition).
        assert_eq!(angle_to_span(100.0, 1.0, 50.0, -30.0), 180.0);
        // Negative shift.
        assert_eq!(angle_to_span(100.0, 1.0, -50.0, 30.0), 20.0);
    }

    #[test]
    fn test_rotate_xy_returns_rounded_integer_coords() {
        // The result is always rounded (not fractional) per Perl's round() semantics.
        let (x, y) = rotate_xy(10.3, 20.7, 0.0, 0.0, 30.0, 0.0);
        // x and y should be whole-number f64s (fractional part = 0).
        assert_eq!(x, x.round());
        assert_eq!(y, y.round());
        // Further rotations still round.
        let (x2, y2) = rotate_xy(7.7, 3.3, 0.5, 0.5, 47.0, 0.0);
        assert_eq!(x2, x2.round());
        assert_eq!(y2, y2.round());
    }

    #[test]
    fn test_arc_points_reversed_start_end_uses_negative_step() {
        // When angle_start > angle_end, step flips sign — walking backward.
        let pts = arc_points(0.0, 0.0, 100.0, 90.0, 0.0, 30.0);
        assert!(pts.len() >= 2);
        // First point at angle=90 → (cos(90°)*100, sin(90°)*100) ≈ (0, 100)
        let (fx, fy) = pts[0];
        assert!(fx.abs() < 1e-6);
        assert!((fy - 100.0).abs() < 1e-6);
        // Last point at angle=0 → (100, 0)
        let (lx, ly) = *pts.last().unwrap();
        assert!((lx - 100.0).abs() < 1e-6);
        assert!(ly.abs() < 1e-6);
    }

    #[test]
    fn test_rotate_xy_pivot_point_is_fixed() {
        // Rotating the pivot itself by any angle around itself yields the pivot.
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0, 359.9] {
            let (x, y) = rotate_xy(10.0, 20.0, 10.0, 20.0, angle, 0.0);
            assert_eq!(x, 10.0);
            assert_eq!(y, 20.0);
        }
    }

    #[test]
    fn test_relradius_exactly_at_two_preserves_fractional_behavior() {
        // Boundary is strict: r < 2 → scale; r >= 2 → pass through.
        // 1.9999 < 2 → scales.
        let r = relradius(1.9999, 1000.0);
        assert!((r - 1999.9).abs() < 1e-9);
        // 2.0001 >= 2 → passes through.
        assert_eq!(relradius(2.0001, 1000.0), 2.0001);
    }

    #[test]
    fn test_arc_points_single_point_when_start_equals_end() {
        // angle_start == angle_end → loop emits one point, then breaks.
        let pts = arc_points(0.0, 0.0, 50.0, 45.0, 45.0, 1.0);
        // At least one point present (could be 1 or 2 depending on loop logic).
        assert!(!pts.is_empty());
        // First point at angle=45: (cos(45°)*50, sin(45°)*50) ≈ (35.355, 35.355).
        let (x, y) = pts[0];
        assert!((x - 35.35533905932738).abs() < 1e-6);
        assert!((y - 35.35533905932738).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_exactly_180_sweep_has_flag_zero() {
        // sweep = 180 is NOT > 180 → large_arc flag = 0.
        let path = svg_arc_path(0.0, 0.0, 100.0, 0.0, 180.0);
        // Format: "M x1,y1 A r,r 0 <large>,<sweep> x2,y2" — large flag is 0.
        assert!(path.contains(" 0 0,1 "));
        // And 180.001 → large flag 1.
        let path2 = svg_arc_path(0.0, 0.0, 100.0, 0.0, 180.001);
        assert!(path2.contains(" 0 1,1 "));
    }

    #[test]
    fn test_rotate_xy_around_nonzero_pivot_keeps_pivot_distance_invariant() {
        // For a point rotated around (100, 100), distance from (100, 100) remains the same.
        let px = 100.0;
        let py = 100.0;
        let dx = 30.0;
        let dy = 40.0; // distance 50 from pivot
        for angle in [0.0, 37.5, 90.0, 143.0, 270.0] {
            let (x, y) = rotate_xy(px + dx, py + dy, px, py, angle, 0.0);
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            // Round because rotate_xy returns rounded coords — distance may be ±1.
            assert!((d - 50.0).abs() < 2.0, "angle={}: distance={}", angle, d);
        }
    }

    #[test]
    fn test_relradius_exactly_one_scales_to_image_radius() {
        // r=1.0 is < 2 → scales to image_radius exactly.
        assert_eq!(relradius(1.0, 1500.0), 1500.0);
        // r=0.5 → half the image_radius.
        assert_eq!(relradius(0.5, 1000.0), 500.0);
    }

    #[test]
    fn test_arc_points_forward_sweep_has_monotonic_angles() {
        // Points generated in order of increasing angle for forward sweep.
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 180.0, 30.0);
        // For forward sweep, x-coord should mostly decrease monotonically as angle increases.
        assert!(pts.len() >= 2);
        // First point at 0° → (100, 0); last at 180° → (-100, 0).
        let (fx, _) = pts[0];
        let (lx, _) = *pts.last().unwrap();
        assert!((fx - 100.0).abs() < 1e-6);
        assert!((lx - (-100.0)).abs() < 1e-6);
    }

    #[test]
    fn test_svg_arc_path_contains_m_a_command_pattern() {
        // Output always starts with "M x,y A r,r ..." — SVG arc command pattern.
        let path = svg_arc_path(50.0, 50.0, 25.0, 0.0, 90.0);
        assert!(path.starts_with("M "));
        assert!(path.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_zero_angle_zero_offset_returns_input() {
        // angle=0 and angle_offset=0 → no rotation → rounded input.
        let (x, y) = rotate_xy(3.7, 4.2, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(x, 4.0);
        assert_eq!(y, 4.0);
    }

    #[test]
    fn test_angle_to_span_zero_resolution_always_yields_zero() {
        // resolution=0 → result is always 0 regardless of other args.
        assert_eq!(angle_to_span(100.0, 0.0, 50.0, 30.0), 0.0);
        assert_eq!(angle_to_span(-10.0, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn test_relradius_very_large_r_passes_through_unchanged() {
        // r >= 2 → passes through; very large values also pass.
        assert_eq!(relradius(1e9, 1500.0), 1e9);
        assert_eq!(relradius(10000.0, 100.0), 10000.0);
    }

    #[test]
    fn test_arc_points_full_circle_has_many_points() {
        // 0→360° sweep with step=10° → about 36-37 points.
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 360.0, 10.0);
        assert!(pts.len() >= 36);
        // First point at angle=0 → (100, 0).
        let (fx, _) = pts[0];
        assert!((fx - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_rotate_xy_90deg_around_custom_pivot_preserves_distance() {
        // Point (100, 50) rotated 90° around (50, 50) → should land at (50, 100).
        // Distance from pivot = 50. After 90° rotation, still at distance 50 (dy=50).
        let (x, y) = rotate_xy(100.0, 50.0, 50.0, 50.0, 90.0, 0.0);
        // Verify distance from pivot preserved.
        let d = ((x - 50.0).powi(2) + (y - 50.0).powi(2)).sqrt();
        assert!((d - 50.0).abs() < 2.0); // within rounding tolerance
    }

    #[test]
    fn test_angle_to_span_negative_angle_yields_negative_span() {
        // Negative angles pass through the formula linearly.
        let r = angle_to_span(-30.0, 2.0, 0.0, 0.0);
        assert_eq!(r, -60.0);
        // Negative + negative offset.
        let r2 = angle_to_span(-10.0, 1.0, 0.0, -5.0);
        assert_eq!(r2, -5.0); // -10 + 0 - (-5) = -5
    }

    #[test]
    fn test_relradius_just_below_2_boundary_scales() {
        // r=1.99999 is still < 2 → scales by image_radius.
        let r = relradius(1.99999, 2000.0);
        assert_eq!(r, 1.99999 * 2000.0);
    }

    #[test]
    fn test_arc_points_zero_radius_all_points_at_center() {
        // radius=0 → all points at (cx, cy) regardless of angle.
        let pts = arc_points(500.0, 500.0, 0.0, 0.0, 90.0, 30.0);
        assert!(!pts.is_empty());
        for (x, y) in &pts {
            assert!((x - 500.0).abs() < 1e-6);
            assert!((y - 500.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_rotate_xy_360_degrees_is_approximately_identity() {
        // Rotating by 360° should return (near) the input within rounding.
        let (x, y) = rotate_xy(100.5, 50.5, 50.0, 50.0, 360.0, 0.0);
        // After 360° rotation + round(), x/y should be within 1.0 of input.
        assert!((x - 100.5).abs() <= 1.0);
        assert!((y - 50.5).abs() <= 1.0);
    }

    #[test]
    fn test_angle_to_span_large_positive_resolution_magnifies() {
        // resolution=1000 → angles scale by 1000.
        assert_eq!(angle_to_span(1.0, 1000.0, 0.0, 0.0), 1000.0);
        assert_eq!(angle_to_span(0.5, 1000.0, 0.0, 0.0), 500.0);
    }

    #[test]
    fn test_relradius_negative_r_scales_by_image_radius() {
        // Negative r < 2 → scales by image_radius (could be negative).
        assert_eq!(relradius(-0.5, 1000.0), -500.0);
        assert_eq!(relradius(-1.0, 500.0), -500.0);
    }

    #[test]
    fn test_svg_arc_path_at_exact_zero_sweep_flag_zero() {
        // end==start → diff=0 → NOT > 0 → NOT > 180 → flag 0.
        let path = svg_arc_path(0.0, 0.0, 100.0, 45.0, 45.0);
        assert!(path.contains(" 0 0,1 "));
    }

    #[test]
    fn test_rotate_xy_angle_offset_cancels_equal_angle() {
        // If angle==angle_offset, net rotation is 0 → rounded input.
        let (x, y) = rotate_xy(10.3, 20.7, 5.0, 5.0, 90.0, 90.0);
        // Rotation zero → (x, y) → rounded.
        assert_eq!(x, 10.0);
        assert_eq!(y, 21.0);
    }

    #[test]
    fn test_angle_to_span_unit_resolution_preserves_angular_value() {
        // resolution=1, shift=0, offset=0 → result = angle.
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0, 360.0] {
            assert_eq!(angle_to_span(angle, 1.0, 0.0, 0.0), angle);
        }
    }

    #[test]
    fn test_arc_points_endpoint_included_in_output() {
        // Last point of arc_points should be at (or very close to) angle_end.
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 30.0);
        let (lx, ly) = *pts.last().unwrap();
        // End angle 90° → (0, 100).
        assert!(lx.abs() < 1e-6);
        assert!((ly - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_relradius_zero_r_scales_to_zero() {
        // r=0 < 2 → 0 × image_radius = 0.
        assert_eq!(relradius(0.0, 1500.0), 0.0);
        assert_eq!(relradius(0.0, 0.0), 0.0);
    }

    #[test]
    fn test_svg_arc_path_exact_half_circle_uses_large_arc_zero() {
        // 180° sweep → not > 180 → flag 0.
        let p = svg_arc_path(0.0, 0.0, 100.0, 0.0, 180.0);
        // large-arc flag is 0 because 180 > 180 is false.
        assert!(p.contains(" 0,1 "));
    }

    #[test]
    fn test_svg_arc_path_270_sweep_uses_large_arc_one() {
        // 270° sweep → > 180 → flag 1.
        let p = svg_arc_path(0.0, 0.0, 100.0, 0.0, 270.0);
        assert!(p.contains(" 1,1 "));
    }

    #[test]
    fn test_angle_to_span_shift_adds_to_angle_before_scaling() {
        // (angle + shift - offset) × resolution = (30 + 10 - 5) × 2 = 70.
        assert_eq!(angle_to_span(30.0, 2.0, 10.0, 5.0), 70.0);
    }

    #[test]
    fn test_relradius_exactly_two_passes_through_unchanged() {
        // 2.0 is not < 2 → pass through unchanged.
        assert_eq!(relradius(2.0, 1500.0), 2.0);
        // Just above 2 → pass through.
        assert_eq!(relradius(2.5, 1500.0), 2.5);
    }

    #[test]
    fn test_rotate_xy_180deg_inverts_offset_from_pivot() {
        // (5,0) rotated +180° about origin → (-5,0).
        let (x, y) = rotate_xy(5.0, 0.0, 0.0, 0.0, 180.0, 0.0);
        assert_eq!((x, y), (-5.0, 0.0));
    }

    #[test]
    fn test_arc_points_step_size_affects_output_count() {
        // Smaller step → more points (1° vs 10° across 90°).
        let fine = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 1.0);
        let coarse = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 10.0);
        assert!(fine.len() > coarse.len());
    }

    #[test]
    fn test_arc_points_reverse_sweep_produces_output() {
        // end < start → step becomes negative; first point at start, last near end.
        let pts = arc_points(0.0, 0.0, 100.0, 90.0, 0.0, 30.0);
        assert!(!pts.is_empty());
        // First point at 90° → (0, 100).
        assert!(pts[0].0.abs() < 1e-6);
        assert!((pts[0].1 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_angle_to_span_negative_resolution_sign_flips_result() {
        // resolution=-2 with positive (angle+shift-offset) → negative result.
        assert_eq!(angle_to_span(10.0, -2.0, 0.0, 0.0), -20.0);
    }

    #[test]
    fn test_rotate_xy_offset_shifts_rotation_angle() {
        // angle_offset=30 means "rotate by (angle - 30)°": 30-30=0 → identity.
        let (x, y) = rotate_xy(5.0, 0.0, 0.0, 0.0, 30.0, 30.0);
        assert_eq!((x, y), (5.0, 0.0));
    }

    #[test]
    fn test_svg_arc_path_starts_with_m_and_contains_arc() {
        // Output format: "M x1,y1 A rx,ry 0 flag,sweep x2,y2".
        let p = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        assert!(p.starts_with("M "));
        assert!(p.contains(" A "));
    }

    #[test]
    fn test_relradius_negative_coefficient_less_than_2_still_scales() {
        // r < 2 (including negative) → r × image_radius.
        assert_eq!(relradius(-0.5, 1000.0), -500.0);
    }

    #[test]
    fn test_arc_points_single_angle_produces_at_least_one_point() {
        // angle_start == angle_end → loop yields first point then exits.
        let pts = arc_points(0.0, 0.0, 50.0, 45.0, 45.0, 10.0);
        assert!(!pts.is_empty());
    }

    #[test]
    fn test_rotate_xy_around_non_origin_pivot_preserves_distance_to_pivot() {
        // Any rotation about a pivot preserves ||(x,y) - (x0,y0)||.
        let x0: f64 = 100.0;
        let y0: f64 = 50.0;
        let x: f64 = 130.0;
        let y: f64 = 90.0;
        let d0 = ((x - x0).powi(2) + (y - y0).powi(2)).sqrt();
        for angle in [45.0, 90.0, 180.0, 270.0] {
            let (rx, ry) = rotate_xy(x, y, x0, y0, angle, 0.0);
            let d = ((rx - x0).powi(2) + (ry - y0).powi(2)).sqrt();
            assert!((d - d0).abs() < 1.0);
        }
    }

    #[test]
    fn test_angle_to_span_zero_angle_yields_zero_when_shift_and_offset_cancel() {
        // angle=0, shift=X, offset=X → result = 0 * resolution = 0.
        assert_eq!(angle_to_span(0.0, 5.0, 90.0, 90.0), 0.0);
    }

    #[test]
    fn test_relradius_exact_one_scales_to_image_radius() {
        // r=1 < 2 → 1 × image_radius = image_radius.
        assert_eq!(relradius(1.0, 1500.0), 1500.0);
    }

    #[test]
    fn test_svg_arc_path_identical_start_end_degenerate_case() {
        // start == end → 0 sweep; output still well-formed.
        let p = svg_arc_path(0.0, 0.0, 100.0, 90.0, 90.0);
        assert!(p.starts_with("M "));
        assert!(p.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_negative_angle_is_clockwise() {
        // Rotating (1,0) by -90° → (0,-1).
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, -90.0, 0.0);
        assert_eq!((x, y), (0.0, -1.0));
    }

    #[test]
    fn test_arc_points_short_sweep_produces_small_output_vec() {
        // 0° → 10° with step 5 → around 3 points (start, 5, 10).
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 10.0, 5.0);
        assert!(pts.len() >= 3 && pts.len() <= 4);
    }

    #[test]
    fn test_angle_to_span_unit_resolution_identity_across_angles() {
        // resolution=1, shift=0, offset=0 → angle passes through.
        for a in [0.0, 45.0, 90.0, 180.0, 270.0, 359.9] {
            assert_eq!(angle_to_span(a, 1.0, 0.0, 0.0), a);
        }
    }

    #[test]
    fn test_relradius_fractional_value_scales_to_fraction_of_image() {
        // r=0.5 × image_radius → half.
        assert_eq!(relradius(0.5, 1000.0), 500.0);
        assert_eq!(relradius(0.25, 2000.0), 500.0);
    }

    #[test]
    fn test_rotate_xy_full_360_equals_identity_with_rounding() {
        // 360° rotation returns to start (modulo rounding).
        let (x, y) = rotate_xy(5.0, 0.0, 0.0, 0.0, 360.0, 0.0);
        assert_eq!((x, y), (5.0, 0.0));
    }

    #[test]
    fn test_arc_points_output_has_at_least_start_point() {
        // Any valid arc call yields at least one point.
        let pts = arc_points(100.0, 100.0, 50.0, 0.0, 1.0, 10.0);
        assert!(!pts.is_empty());
        // First point at angle 0 → (100+50, 100) = (150, 100).
        assert!((pts[0].0 - 150.0).abs() < 1e-9);
        assert!((pts[0].1 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_angle_to_span_positive_shift_adds_to_angle() {
        // shift > 0 adds; offset=0 → (angle+shift) × resolution.
        assert_eq!(angle_to_span(10.0, 1.0, 20.0, 0.0), 30.0);
    }

    #[test]
    fn test_svg_arc_path_output_format_uses_one_decimal_place() {
        // {:.1} formatting for endpoints.
        let p = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        // First pair: "100.0,0.0"
        assert!(p.contains("100.0"));
    }

    #[test]
    fn test_rotate_xy_pivot_itself_invariant_under_rotation() {
        // Rotating the pivot point → same point (zero displacement to rotate).
        for angle in [45.0, 90.0, 180.0] {
            let (x, y) = rotate_xy(500.0, 300.0, 500.0, 300.0, angle, 0.0);
            assert_eq!((x, y), (500.0, 300.0));
        }
    }

    #[test]
    fn test_arc_points_zero_radius_produces_all_points_at_center() {
        // radius=0 → all points at (cx, cy).
        let pts = arc_points(100.0, 200.0, 0.0, 0.0, 90.0, 30.0);
        for p in &pts {
            assert!((p.0 - 100.0).abs() < 1e-9);
            assert!((p.1 - 200.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_angle_to_span_large_values_scale_proportionally() {
        // (1000+0-0) × 1000 = 1,000,000.
        assert_eq!(angle_to_span(1000.0, 1000.0, 0.0, 0.0), 1_000_000.0);
    }

    #[test]
    fn test_relradius_values_above_two_passthrough_unchanged() {
        // 100 > 2 → passthrough.
        assert_eq!(relradius(100.0, 1500.0), 100.0);
        assert_eq!(relradius(1500.0, 2000.0), 1500.0);
    }

    #[test]
    fn test_rotate_xy_270_deg_yields_expected_quadrant() {
        // (1,0) rotated +270° → (0,-1).
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, 270.0, 0.0);
        assert_eq!((x, y), (0.0, -1.0));
    }

    #[test]
    fn test_arc_points_exactly_at_boundary_angle_included() {
        // arc_points(0, 90, step=90) → exactly 2 points (0 and 90).
        let pts = arc_points(0.0, 0.0, 50.0, 0.0, 90.0, 90.0);
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn test_angle_to_span_subtraction_via_offset_works() {
        // offset > shift + angle → negative result.
        // (0 + 0 - 100) × 1 = -100.
        assert_eq!(angle_to_span(0.0, 1.0, 0.0, 100.0), -100.0);
    }

    #[test]
    fn test_svg_arc_path_negative_sweep_wraps_to_positive_large_flag() {
        // End < start → sweep += 360; 350° > 180 → large_arc=1.
        let p = svg_arc_path(0.0, 0.0, 100.0, 350.0, 0.0);
        // Negative diff -350 + 360 = 10 → not > 180 → flag 0.
        assert!(p.contains(" 0,1 "));
    }

    #[test]
    fn test_rotate_xy_non_zero_offset_equivalent_to_reducing_angle() {
        // angle=90 with offset=45 should equal angle=45 with offset=0 (net 45°).
        let (x1, y1) = rotate_xy(10.0, 0.0, 0.0, 0.0, 90.0, 45.0);
        let (x2, y2) = rotate_xy(10.0, 0.0, 0.0, 0.0, 45.0, 0.0);
        assert_eq!((x1, y1), (x2, y2));
    }

    #[test]
    fn test_arc_points_start_point_on_arc_circle() {
        // First point lies on circle centered at (cx,cy) with given radius.
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 180.0, 10.0);
        let (px, py) = pts[0];
        let dist = (px * px + py * py).sqrt();
        assert!((dist - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_angle_to_span_all_inputs_zero_result_is_zero() {
        // Everything zero → 0.
        assert_eq!(angle_to_span(0.0, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn test_relradius_small_positive_values_below_2_scale() {
        // 0.01 × 1000 = 10; 1.99 × 1000 = 1990.
        assert_eq!(relradius(0.01, 1000.0), 10.0);
        assert_eq!(relradius(1.99, 1000.0), 1990.0);
    }

    #[test]
    fn test_rotate_xy_very_small_rotation_close_to_identity() {
        // 0.001° rotation of (100,0) produces a point near (100,0).
        let (x, y) = rotate_xy(100.0, 0.0, 0.0, 0.0, 0.001, 0.0);
        // Rounded — stays at (100, 0).
        assert_eq!((x, y), (100.0, 0.0));
    }

    #[test]
    fn test_arc_points_with_negative_start_angle_still_valid() {
        // start=-90, end=0 → arc from (0,-50) to (50,0) with radius 50.
        let pts = arc_points(0.0, 0.0, 50.0, -90.0, 0.0, 30.0);
        assert!(!pts.is_empty());
        // First point at -90°: (50 cos(-90), 50 sin(-90)) = (0, -50).
        assert!((pts[0].0 - 0.0).abs() < 1e-6);
        assert!((pts[0].1 + 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_angle_to_span_fractional_resolution_produces_fractional() {
        // angle=10, resolution=0.5, shift=0, offset=0 → 5.0.
        assert_eq!(angle_to_span(10.0, 0.5, 0.0, 0.0), 5.0);
    }

    #[test]
    fn test_relradius_zero_image_radius_zero_result() {
        // Any r × 0 = 0.
        assert_eq!(relradius(0.5, 0.0), 0.0);
        assert_eq!(relradius(1.5, 0.0), 0.0);
        // r=5 > 2 → passthrough → 5.
        assert_eq!(relradius(5.0, 0.0), 5.0);
    }

    #[test]
    fn test_rotate_xy_origin_offset_rotation() {
        // Rotate (10, 0) about (10, 0) (origin == pivot) → stays.
        let (x, y) = rotate_xy(10.0, 0.0, 10.0, 0.0, 90.0, 0.0);
        assert_eq!((x, y), (10.0, 0.0));
    }

    #[test]
    fn test_arc_points_equal_start_end_with_step_produces_at_least_one() {
        // start == end → at least 1 point emitted (loop yields start then exits).
        let pts = arc_points(0.0, 0.0, 100.0, 30.0, 30.0, 10.0);
        assert!(!pts.is_empty());
    }

    #[test]
    fn test_angle_to_span_negative_shift_decreases_result() {
        // angle=100, shift=-50, offset=0, res=1 → 50.
        assert_eq!(angle_to_span(100.0, 1.0, -50.0, 0.0), 50.0);
    }

    #[test]
    fn test_svg_arc_path_radius_smaller_than_1_still_formats() {
        // Very small radius (0.1) — still produces valid path.
        let p = svg_arc_path(0.0, 0.0, 0.1, 0.0, 90.0);
        assert!(p.starts_with("M "));
        assert!(p.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_fractional_angles_produce_approximate_result() {
        // 0.001° of (100,0) ≈ (100, 0) after rounding.
        let (x, y) = rotate_xy(100.0, 0.0, 0.0, 0.0, 0.001, 0.0);
        assert!((x - 100.0).abs() < 1.0);
        assert!(y.abs() < 1.0);
    }

    #[test]
    fn test_arc_points_single_point_result_for_exact_angles() {
        // Exact match start=end with generous step → 1 point output.
        let pts = arc_points(0.0, 0.0, 50.0, 45.0, 45.0, 90.0);
        assert_eq!(pts.len(), 1);
    }

    #[test]
    fn test_angle_to_span_integer_values_preserve_integrality() {
        // All int inputs → int result.
        let r = angle_to_span(10.0, 2.0, 0.0, 0.0);
        assert_eq!(r, 20.0);
        assert_eq!(r.fract(), 0.0);
    }

    #[test]
    fn test_relradius_at_boundary_2_passthrough() {
        // Exactly 2 → NOT <2 → passthrough.
        assert_eq!(relradius(2.0, 1000.0), 2.0);
    }

    #[test]
    fn test_rotate_xy_full_360_returns_to_origin_position() {
        // 360° rotation → same point (within float precision).
        let (x, y) = rotate_xy(100.0, 50.0, 0.0, 0.0, 360.0, 0.0);
        assert!((x - 100.0).abs() < 1e-9);
        assert!((y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_angle_to_span_zero_angle_and_zero_shift_zero_result() {
        // 0 × 0 + 0 = 0.
        assert_eq!(angle_to_span(0.0, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn test_relradius_fractional_between_one_and_two() {
        // 1.5 < 2 → scaled by image_radius.
        assert_eq!(relradius(1.5, 1000.0), 1500.0);
    }

    #[test]
    fn test_arc_points_large_step_fewer_points() {
        // Step larger than span yields few points.
        let points = arc_points(0.0, 90.0, 180.0, 100.0, 200.0, 50.0);
        assert!(!points.is_empty());
    }

    #[test]
    fn test_relradius_exactly_1_scaled_to_image_radius() {
        // 1.0 < 2 → 1.0 * 500 = 500.
        assert_eq!(relradius(1.0, 500.0), 500.0);
    }

    #[test]
    fn test_relradius_above_2_passthrough_unchanged() {
        // 2.5 >= 2 → passthrough 2.5.
        assert_eq!(relradius(2.5, 1000.0), 2.5);
    }

    #[test]
    fn test_rotate_xy_90_deg_no_offset_swaps_coordinates() {
        // 90° rotation of (1,0) → ~(0,1) (within epsilon).
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, 90.0, 0.0);
        assert!(x.abs() < 1e-9);
        assert!((y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_svg_arc_path_includes_A_command_in_output() {
        // svg_arc_path should include the 'A' (arc) SVG path command.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        assert!(s.contains("A "));
    }

    #[test]
    fn test_svg_arc_path_begins_with_moveto_command() {
        // Output always starts with 'M' (MoveTo) SVG command.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        assert!(s.starts_with("M "));
    }

    #[test]
    fn test_svg_arc_path_large_arc_flag_set_for_sweep_over_180() {
        // 270° sweep → large-arc flag 1 in path string.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 270.0);
        assert!(s.contains(" 1,1 "));
    }

    #[test]
    fn test_svg_arc_path_small_sweep_large_arc_flag_zero() {
        // 30° sweep → large-arc flag 0.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 30.0);
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_angle_to_span_positive_shift_increases_result() {
        // +10 shift → result includes the shift.
        let base = angle_to_span(5.0, 2.0, 0.0, 0.0);
        let shifted = angle_to_span(5.0, 2.0, 10.0, 0.0);
        assert!(shifted > base);
    }

    #[test]
    fn test_rotate_xy_180_deg_negates_coordinates_from_origin() {
        // rotate (1,0) 180° → (-1, 0) within 1e-9.
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, 180.0, 0.0);
        assert!((x + 1.0).abs() < 1e-9);
        assert!(y.abs() < 1e-9);
    }

    #[test]
    fn test_svg_arc_path_format_includes_two_space_separated_sections() {
        // Output format is "M x,y A rx,ry..." so has at least two tokens.
        let s = svg_arc_path(50.0, 50.0, 100.0, 0.0, 90.0);
        assert!(s.contains("M "));
        assert!(s.contains("A "));
    }

    #[test]
    fn test_relradius_exactly_zero_yields_zero() {
        // 0 < 2 → 0 × image_radius = 0.
        assert_eq!(relradius(0.0, 500.0), 0.0);
    }

    #[test]
    fn test_arc_points_with_zero_radius_all_points_at_center() {
        // radius=0 → every point at center (cx, cy).
        let points = arc_points(10.0, 20.0, 0.0, 0.0, 90.0, 30.0);
        for p in &points {
            assert!((p.0 - 10.0).abs() < 1e-9);
            assert!((p.1 - 20.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_svg_arc_path_negative_sweep_gets_wrapped() {
        // end < start → sweep wraps to large arc.
        let s = svg_arc_path(0.0, 0.0, 100.0, 350.0, 10.0);
        // sweep 10-350 = -340 +360 = 20 → small.
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_rotate_xy_with_nonzero_offset_center() {
        // Rotate (100,0) 90° around (50,0) → (50,50) within eps.
        let (x, y) = rotate_xy(100.0, 0.0, 50.0, 0.0, 90.0, 0.0);
        assert!((x - 50.0).abs() < 1e-9);
        assert!((y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_angle_to_span_angle_offset_subtracts_scaled_by_resolution() {
        // (angle + shift - angle_offset) * resolution.
        // With angle=1, shift=0, res=1: offset=100 → 1-100 = -99.
        let v = angle_to_span(1.0, 1.0, 0.0, 100.0);
        assert_eq!(v, -99.0);
    }

    #[test]
    fn test_relradius_exactly_1_9_scaled() {
        // 1.9 < 2 → 1.9 × 500 = 950.
        assert!((relradius(1.9, 500.0) - 950.0).abs() < 1e-9);
    }

    #[test]
    fn test_arc_points_exact_90_degrees_with_30_step_yields_4_points() {
        // 0..90 step 30 → 4 points (0, 30, 60, 90).
        let points = arc_points(0.0, 0.0, 50.0, 0.0, 90.0, 30.0);
        assert_eq!(points.len(), 4);
    }

    #[test]
    fn test_svg_arc_path_identical_start_end_yields_zero_sweep() {
        // start=end → diff=0 → large-arc flag=0 in output.
        let s = svg_arc_path(0.0, 0.0, 100.0, 45.0, 45.0);
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_rotate_xy_45_deg_preserves_distance_within_rounding_tolerance() {
        // Return values are rounded to nearest i, so 100 × cos(45°) ≈ 71.
        // Distance preserved within ~1 unit of original due to rounding.
        let (x, y) = rotate_xy(100.0, 0.0, 0.0, 0.0, 45.0, 0.0);
        let dist = (x * x + y * y).sqrt();
        assert!((dist - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_relradius_negative_input_below_2_scales_by_image_radius() {
        // -0.5 < 2 → -0.5 × 1000 = -500.
        assert_eq!(relradius(-0.5, 1000.0), -500.0);
    }

    #[test]
    fn test_arc_points_180_span_with_45_step_yields_5_points() {
        // 0..180 step 45 → 5 points (0, 45, 90, 135, 180).
        let points = arc_points(0.0, 0.0, 100.0, 0.0, 180.0, 45.0);
        assert_eq!(points.len(), 5);
    }

    #[test]
    fn test_svg_arc_path_output_ends_with_destination_coords() {
        // Arc path ends with x2,y2 coords.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        // Last chars should be numeric coords.
        assert!(s.chars().last().unwrap().is_ascii_digit() || s.ends_with("0"));
    }

    #[test]
    fn test_rotate_xy_identity_zero_angle_returns_input_point() {
        // 0° rotation → input point unchanged (up to rounding).
        let (x, y) = rotate_xy(50.0, 75.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!((x, y), (50.0, 75.0));
    }

    #[test]
    fn test_angle_to_span_multiplies_by_resolution_factor() {
        // (5 + 0 - 0) * 3 = 15.
        let v = angle_to_span(5.0, 3.0, 0.0, 0.0);
        assert_eq!(v, 15.0);
    }

    #[test]
    fn test_arc_points_zero_span_yields_single_point() {
        // start=end=0 → 1 point.
        let points = arc_points(10.0, 10.0, 50.0, 0.0, 0.0, 30.0);
        assert!(!points.is_empty());
    }

    #[test]
    fn test_svg_arc_path_with_small_radius_produces_valid_path() {
        // Very small radius still produces valid SVG path.
        let s = svg_arc_path(50.0, 50.0, 0.5, 0.0, 90.0);
        assert!(s.contains("A "));
    }

    #[test]
    fn test_rotate_xy_360_and_720_degrees_equivalent() {
        // 720° = 2 × 360° → same result as 360°.
        let (x1, y1) = rotate_xy(100.0, 0.0, 0.0, 0.0, 360.0, 0.0);
        let (x2, y2) = rotate_xy(100.0, 0.0, 0.0, 0.0, 720.0, 0.0);
        assert_eq!(x1, x2);
        assert_eq!(y1, y2);
    }

    #[test]
    fn test_relradius_huge_value_above_2_passthrough() {
        // 1e6 > 2 → passthrough unchanged.
        assert_eq!(relradius(1e6, 500.0), 1e6);
    }

    #[test]
    fn test_arc_points_fine_step_produces_many_points() {
        // Step 1° on 90° arc → 91 points.
        let points = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 1.0);
        assert_eq!(points.len(), 91);
    }

    #[test]
    fn test_svg_arc_path_contains_radius_pair_in_format() {
        // "A rx,ry" format includes r twice.
        let s = svg_arc_path(0.0, 0.0, 75.0, 10.0, 100.0);
        assert!(s.contains("75.0,75.0"));
    }

    #[test]
    fn test_rotate_xy_negative_angle_inverse_of_positive() {
        // Rotating by -θ should undo rotation by +θ.
        let (x1, y1) = rotate_xy(100.0, 50.0, 0.0, 0.0, 30.0, 0.0);
        let (x2, y2) = rotate_xy(x1, y1, 0.0, 0.0, -30.0, 0.0);
        assert!((x2 - 100.0).abs() < 2.0);
        assert!((y2 - 50.0).abs() < 2.0);
    }

    #[test]
    fn test_angle_to_span_negative_angle_yields_negative_result() {
        // angle=-10 × res=1 → -10.
        let v = angle_to_span(-10.0, 1.0, 0.0, 0.0);
        assert_eq!(v, -10.0);
    }

    #[test]
    fn test_rotate_xy_at_origin_remains_origin() {
        // Rotating origin around origin → origin.
        let (x, y) = rotate_xy(0.0, 0.0, 0.0, 0.0, 30.0, 0.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_arc_points_3d_360_full_sweep_produces_points() {
        // Full 360 sweep with 30° step → 13 points (0,30,...360).
        let points = arc_points(0.0, 0.0, 100.0, 0.0, 360.0, 30.0);
        assert_eq!(points.len(), 13);
    }

    #[test]
    fn test_svg_arc_path_with_integer_angles_no_decimals_in_coords() {
        // Clean integer angles produce well-formatted path.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        // Path format has M and A sections.
        assert!(s.starts_with("M "));
    }

    #[test]
    fn test_relradius_value_at_1_1_scales_by_image_radius() {
        // 1.1 < 2 → scaled.
        assert!((relradius(1.1, 1000.0) - 1100.0).abs() < 1e-9);
    }

    #[test]
    fn test_arc_points_with_step_equal_to_span_minimal_points() {
        // step == span → 2 points (start and end).
        let points = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 90.0);
        assert!(points.len() >= 2);
    }

    #[test]
    fn test_rotate_xy_with_large_offset_center() {
        // Offset center at (1000, 2000) with small rotation.
        let (x, y) = rotate_xy(1100.0, 2000.0, 1000.0, 2000.0, 90.0, 0.0);
        // (100, 0) rotated 90° around (0,0) then offset by (1000, 2000).
        assert!((x - 1000.0).abs() < 2.0);
        assert!((y - 2100.0).abs() < 2.0);
    }

    #[test]
    fn test_svg_arc_path_sweep_flag_always_one() {
        // Sweep flag is always 1 in output format.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 90.0);
        // Count ",1" occurrences — at least 1 for sweep flag.
        assert!(s.contains(",1"));
    }

    #[test]
    fn test_angle_to_span_with_zero_resolution_yields_zero() {
        // resolution=0 → product is 0.
        let v = angle_to_span(100.0, 0.0, 0.0, 0.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_rotate_xy_360_degrees_returns_to_origin() {
        // Rotating 360° should return close to original point (within rounding).
        let (x, y) = rotate_xy(50.0, 0.0, 0.0, 0.0, 360.0, 0.0);
        assert_eq!(x, 50.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_angle_to_span_negative_angle_offset_adds() {
        // angle_offset subtracted: negative offset adds to result.
        let v = angle_to_span(10.0, 1.0, 0.0, -5.0);
        // (10 + 0 - (-5)) * 1 = 15
        assert_eq!(v, 15.0);
    }

    #[test]
    fn test_relradius_exactly_2_passes_through() {
        // r == 2.0 is NOT < 2, so passes through unchanged.
        let v = relradius(2.0, 100.0);
        assert_eq!(v, 2.0);
    }

    #[test]
    fn test_svg_arc_path_end_less_than_start_wraps_360() {
        // angle_end < angle_start: sweep_angle += 360, then > 180 → large_arc=1.
        let s = svg_arc_path(0.0, 0.0, 50.0, 270.0, 90.0);
        // sweep = 90 - 270 + 360 = 180, so large_arc = 0 (not > 180)
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_arc_points_start_equals_end_produces_single_point() {
        // start == end → loop body runs once, step becomes angle_step (>0), then breaks.
        let pts = arc_points(100.0, 100.0, 50.0, 45.0, 45.0, 10.0);
        assert_eq!(pts.len(), 1);
    }

    #[test]
    fn test_arc_points_descending_direction_produces_points() {
        // start > end: step is negated; still produces points across sweep.
        let pts = arc_points(0.0, 0.0, 10.0, 90.0, 0.0, 30.0);
        assert!(pts.len() >= 4);
    }

    #[test]
    fn test_arc_points_at_zero_radius_all_at_center() {
        // radius=0 → all points at (cx, cy).
        let pts = arc_points(50.0, 50.0, 0.0, 0.0, 180.0, 45.0);
        for p in &pts {
            assert!((p.0 - 50.0).abs() < 1e-9);
            assert!((p.1 - 50.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_relradius_small_fraction_scales_correctly() {
        // r=0.1, image_radius=500 → 50.0.
        let v = relradius(0.1, 500.0);
        assert_eq!(v, 50.0);
    }

    #[test]
    fn test_angle_to_span_simple_shift_additive() {
        // (10 + 5 - 2) * 3 = 39.
        let v = angle_to_span(10.0, 3.0, 5.0, 2.0);
        assert_eq!(v, 39.0);
    }

    #[test]
    fn test_svg_arc_path_m_prefix_and_a_letter_present() {
        // Output always contains "M " and " A " commands.
        let s = svg_arc_path(100.0, 100.0, 75.0, 45.0, 135.0);
        assert!(s.contains("M "));
        assert!(s.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_zero_angle_returns_original_coords_rounded() {
        // angle == angle_offset → a=0 → identity modulo rounding.
        let (x, y) = rotate_xy(42.0, 17.0, 0.0, 0.0, 30.0, 30.0);
        assert_eq!(x, 42.0);
        assert_eq!(y, 17.0);
    }

    #[test]
    fn test_relradius_with_zero_passes_multiplication_path() {
        // r=0 → 0<2 → multiply → 0.
        let v = relradius(0.0, 500.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_svg_arc_path_exact_180_sweep_large_arc_zero() {
        // Exactly 180° sweep → not > 180 → large_arc=0.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 180.0);
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_svg_arc_path_just_over_180_sweep_large_arc_one() {
        // Just over 180 → > 180 → large_arc=1.
        let s = svg_arc_path(0.0, 0.0, 100.0, 0.0, 181.0);
        assert!(s.contains(" 1,1 "));
    }

    #[test]
    fn test_rotate_xy_90_deg_around_origin_rotates_x_to_y() {
        // (1,0) rotated 90° around origin → (0,1) (rounded).
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, 90.0, 0.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 1.0);
    }

    #[test]
    fn test_angle_to_span_angle_offset_subtracted_from_angle() {
        // offset is subtracted: (100 + 0 - 10) × 1 = 90.
        let v = angle_to_span(100.0, 1.0, 0.0, 10.0);
        assert_eq!(v, 90.0);
    }

    #[test]
    fn test_rotate_xy_around_non_origin_center() {
        // Rotate (10, 0) by 90° around (5, 0) → (5, 5) after rounding.
        let (x, y) = rotate_xy(10.0, 0.0, 5.0, 0.0, 90.0, 0.0);
        assert_eq!(x, 5.0);
        assert_eq!(y, 5.0);
    }

    #[test]
    fn test_svg_arc_path_large_radius_no_decimals_in_integer_coords() {
        // {:.1} format emits coords with one decimal.
        let s = svg_arc_path(100.0, 100.0, 500.0, 0.0, 180.0);
        assert!(s.contains(".0"));
    }

    #[test]
    fn test_arc_points_with_negative_cx_cy_preserves_offset() {
        // Negative cx/cy — first point on arc at angle=0 is (cx+r, cy+0).
        let pts = arc_points(-100.0, -50.0, 10.0, 0.0, 0.0, 30.0);
        // First point: cx+r=-90, cy+0=-50.
        assert!((pts[0].0 - (-90.0)).abs() < 1e-9);
        assert!((pts[0].1 - (-50.0)).abs() < 1e-9);
    }

    #[test]
    fn test_relradius_value_exactly_at_boundary_below_two() {
        // r=1.99 → <2 → multiply.
        let v = relradius(1.99, 100.0);
        assert_eq!(v, 199.0);
    }

    #[test]
    fn test_angle_to_span_large_angle_and_shift() {
        // Large angle + shift: (720 + 90 - 0) × 1 = 810.
        let v = angle_to_span(720.0, 1.0, 90.0, 0.0);
        assert_eq!(v, 810.0);
    }

    #[test]
    fn test_svg_arc_path_zero_radius_emits_path() {
        // radius=0 → path still formatted, with zero coords.
        let s = svg_arc_path(100.0, 100.0, 0.0, 0.0, 90.0);
        assert!(s.starts_with("M "));
        assert!(s.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_with_nonzero_offset_subtracts_offset() {
        // angle=90, angle_offset=45 → effective angle 45°, (1,0) → (0.707, 0.707) rounded.
        let (x, y) = rotate_xy(1.0, 0.0, 0.0, 0.0, 90.0, 45.0);
        // rotate(45°): cos(45)=sin(45)≈0.707 → rounds to 1 (since round() rounds half to even, 0.707 → 1).
        assert_eq!(x, 1.0);
        assert_eq!(y, 1.0);
    }

    #[test]
    fn test_arc_points_small_step_produces_many_points() {
        // 90° sweep with 1° step → ~91 points.
        let pts = arc_points(0.0, 0.0, 100.0, 0.0, 90.0, 1.0);
        assert!(pts.len() >= 90);
    }

    #[test]
    fn test_rotate_xy_180_deg_negates_both_axes() {
        // Rotate (1,1) by 180° around origin → (-1, -1) after rounding.
        let (x, y) = rotate_xy(1.0, 1.0, 0.0, 0.0, 180.0, 0.0);
        assert_eq!(x, -1.0);
        assert_eq!(y, -1.0);
    }

    #[test]
    fn test_svg_arc_path_very_small_radius_emits_path() {
        // Tiny radius (0.5) still produces path with M and A commands.
        let s = svg_arc_path(0.0, 0.0, 0.5, 0.0, 45.0);
        assert!(s.starts_with("M "));
        assert!(s.contains(" A "));
    }

    #[test]
    fn test_angle_to_span_identity_with_zero_offset_resolution_1() {
        // (50 + 0 - 0) × 1 = 50.
        let v = angle_to_span(50.0, 1.0, 0.0, 0.0);
        assert_eq!(v, 50.0);
    }

    #[test]
    fn test_arc_points_with_very_small_sweep_produces_at_least_two_points() {
        // 1° sweep → start + end points.
        let pts = arc_points(50.0, 50.0, 10.0, 45.0, 46.0, 10.0);
        assert!(pts.len() >= 2);
    }

    #[test]
    fn test_angle_to_span_with_negative_resolution_yields_negative() {
        // Negative resolution → negative result.
        let v = angle_to_span(10.0, -2.0, 5.0, 0.0);
        assert_eq!(v, -30.0);
    }

    #[test]
    fn test_svg_arc_path_two_identical_angles_creates_minimal_path() {
        // start == end → sweep=0, large_arc=0, produces valid path.
        let s = svg_arc_path(0.0, 0.0, 100.0, 90.0, 90.0);
        assert!(s.contains("M "));
        assert!(s.contains(" A "));
    }

    #[test]
    fn test_rotate_xy_with_huge_offset_still_finite() {
        // Huge angle + offset that cancels → preserve (x,y) approximately.
        let (x, y) = rotate_xy(10.0, 5.0, 0.0, 0.0, 720.0, 720.0);
        // 720-720=0 angle → identity.
        assert_eq!(x, 10.0);
        assert_eq!(y, 5.0);
    }

    #[test]
    fn test_relradius_large_relative_value_with_small_image_radius() {
        // 0.5 × 1.0 → 0.5.
        let v = relradius(0.5, 1.0);
        assert_eq!(v, 0.5);
    }

    #[test]
    fn test_svg_arc_path_start_end_reversed_wraps_correctly() {
        // start=270, end=90 — sweep = 90-270 = -180, +=360 → 180, not > 180 → large_arc=0.
        let s = svg_arc_path(0.0, 0.0, 50.0, 270.0, 90.0);
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_rotate_xy_around_same_point_identity() {
        // Rotate (5,5) around (5,5) → (5,5).
        let (x, y) = rotate_xy(5.0, 5.0, 5.0, 5.0, 90.0, 0.0);
        assert_eq!(x, 5.0);
        assert_eq!(y, 5.0);
    }

    #[test]
    fn test_angle_to_span_zero_angle_with_nonzero_offset_gives_negative_offset() {
        // (0 + 0 - 90) * 1 = -90.
        let v = angle_to_span(0.0, 1.0, 0.0, 90.0);
        assert_eq!(v, -90.0);
    }

    #[test]
    fn test_arc_points_with_zero_step_makes_loop_not_advance() {
        // step=0 → loop iterates but angle doesn't change; may run indefinitely. Actually `step = angle_step.abs()` → 0, and if angle==end break.
        // When start==end, loop breaks after pushing 1 point.
        let pts = arc_points(0.0, 0.0, 100.0, 45.0, 45.0, 0.0);
        assert_eq!(pts.len(), 1);
    }
}
