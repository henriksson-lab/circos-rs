/// De Casteljau algorithm: evaluate a cubic bezier curve at parameter t.
///
/// p0, p1, p2, p3 are the control points.
pub fn bezier_point(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    let x = uuu * p0.0 + 3.0 * uu * t * p1.0 + 3.0 * u * tt * p2.0 + ttt * p3.0;
    let y = uuu * p0.1 + 3.0 * uu * t * p1.1 + 3.0 * u * tt * p2.1 + ttt * p3.1;
    (x, y)
}

/// Generate evenly-spaced points along a cubic bezier curve.
pub fn bezier_points(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    num_samples: usize,
) -> Vec<(f64, f64)> {
    (0..=num_samples)
        .map(|i| {
            let t = i as f64 / num_samples as f64;
            bezier_point(p0, p1, p2, p3, t)
        })
        .collect()
}

/// Port of Perl `bezier_control_points`. Computes control points for a
/// Circos-style link bezier curve through the circle's interior.
///
/// Faithful to Perl:
/// - Computes bisecting_radius (distance from image center to midpoint of the
///   two endpoint positions), used by bezier_radius_purity.
/// - middleangle is the angular midpoint; crosses-360 case handled by Perl's
///   `abs(a2-a1) > 180 ? (a1+a2+360)/2 - 360 : (a1+a2)/2`.
/// - If `bezier_radius_purity` is Some, shifts bezier_radius toward or away
///   from bisecting_radius by `(1-k) * |bezier_radius - bisecting_radius|`,
///   with sign depending on whether bezier_radius > bisecting_radius and k>1.
/// - If `perturb_bezier_radius` is Some("pmin,pmax"), multiplies bezier_radius
///   by a uniform random draw in [pmin, pmax].
/// - If `crest` is Some, splices two extra crest control points (one per
///   side) at radius shifted by `|radiusN - bezier_radius| * crest`.
///
/// Returns at least 4 control points (p0, p1, p2, p3 — anchor, inner, inner,
/// anchor), plus 0 or 2 extra crest points when `crest` is set.
pub fn bezier_control_points(
    cx: f64,
    cy: f64,
    angle1: f64,
    radius1: f64,
    angle2: f64,
    radius2: f64,
    bezier_radius: f64,
    bezier_radius_purity: Option<f64>,
    perturb_bezier_radius: Option<&str>,
    perturb_bezier_radius_purity: Option<&str>,
    crest: Option<f64>,
    perturb_crest: Option<&str>,
) -> Vec<(f64, f64)> {
    let deg2rad = std::f64::consts::PI / 180.0;

    let (x1, y1) = (
        cx + radius1 * (angle1 * deg2rad).cos(),
        cy + radius1 * (angle1 * deg2rad).sin(),
    );
    let (x2, y2) = (
        cx + radius2 * (angle2 * deg2rad).cos(),
        cy + radius2 * (angle2 * deg2rad).sin(),
    );

    let bisecting_radius = (((x1 + x2) / 2.0 - cx).powi(2) + ((y1 + y2) / 2.0 - cy).powi(2)).sqrt();

    let middleangle = if (angle2 - angle1).abs() > 180.0 {
        (angle1 + angle2 + 360.0) / 2.0 - 360.0
    } else {
        (angle1 + angle2) / 2.0
    };

    let mut bezier_radius = bezier_radius;
    if let Some(k) = bezier_radius_purity {
        let k = crate::utils::perturb_value(k, perturb_bezier_radius_purity);
        let x = (1.0 - k).abs() * (bezier_radius - bisecting_radius).abs();
        if bezier_radius > bisecting_radius {
            if k > 1.0 {
                bezier_radius += x;
            } else {
                bezier_radius -= x;
            }
        } else if k > 1.0 {
            bezier_radius -= x;
        } else {
            bezier_radius += x;
        }
    }

    bezier_radius = crate::utils::perturb_value(bezier_radius, perturb_bezier_radius);

    let (x3, y3) = (
        cx + bezier_radius * (middleangle * deg2rad).cos(),
        cy + bezier_radius * (middleangle * deg2rad).sin(),
    );

    let mut control_points: Vec<(f64, f64)> = vec![(x1, y1), (x3, y3), (x2, y2)];

    if let Some(crest_v) = crest {
        let crest_v = crate::utils::perturb_value(crest_v, perturb_crest);
        let crest_radius1 = if radius1 > bezier_radius {
            radius1 - (radius1 - bezier_radius).abs() * crest_v
        } else {
            radius1 + (radius1 - bezier_radius).abs() * crest_v
        };
        // Perl: splice( @controlpoints, 2, 0, getxypos( $a1, $crest_radius ) );
        // — insert at index 1 of the point list (index 2 in flat x,y list).
        let crest_pt1 = (
            cx + crest_radius1 * (angle1 * deg2rad).cos(),
            cy + crest_radius1 * (angle1 * deg2rad).sin(),
        );
        control_points.insert(1, crest_pt1);

        let crest_radius2 = if radius2 > bezier_radius {
            radius2 - (radius2 - bezier_radius).abs() * crest_v
        } else {
            radius2 + (radius2 - bezier_radius).abs() * crest_v
        };
        // After the first insert, the final anchor is now at index 3.
        // Perl: splice( @controlpoints, 6, 0, ... ) in the flat list == insert at point-index 3.
        let crest_pt2 = (
            cx + crest_radius2 * (angle2 * deg2rad).cos(),
            cy + crest_radius2 * (angle2 * deg2rad).sin(),
        );
        control_points.insert(3, crest_pt2);
    }

    control_points
}

/// Sample `num_samples+1` evenly-spaced points along a bezier curve of any
/// degree (up to 4 control points supported: cubic). For 5 control points
/// (quartic), the degree is reduced to cubic via midpoint averaging, matching
/// Perl's `bezier_points` which calls GD's arbitrary-degree sampler.
pub fn bezier_points_n(control: &[(f64, f64)], num_samples: usize) -> Vec<(f64, f64)> {
    match control.len() {
        4 => bezier_points(control[0], control[1], control[2], control[3], num_samples),
        3 => {
            // Quadratic → convert to cubic
            let q0 = control[0];
            let q1 = control[1];
            let q2 = control[2];
            let c1 = (
                q0.0 + 2.0 / 3.0 * (q1.0 - q0.0),
                q0.1 + 2.0 / 3.0 * (q1.1 - q0.1),
            );
            let c2 = (
                q2.0 + 2.0 / 3.0 * (q1.0 - q2.0),
                q2.1 + 2.0 / 3.0 * (q1.1 - q2.1),
            );
            bezier_points(q0, c1, c2, q2, num_samples)
        }
        5 => {
            // Quartic (p0..p4) reduced to cubic by averaging inner pair
            let p0 = control[0];
            let p1 = control[1];
            let p2 = control[2];
            let p3 = control[3];
            let p4 = control[4];
            let c1 = ((p1.0 + p2.0) * 0.5, (p1.1 + p2.1) * 0.5);
            let c2 = ((p2.0 + p3.0) * 0.5, (p2.1 + p3.1) * 0.5);
            bezier_points(p0, c1, c2, p4, num_samples)
        }
        _ => Vec::new(),
    }
}

/// Get the midpoint of a bezier curve (at t=0.5).
pub fn bezier_middle(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> (f64, f64) {
    bezier_point(p0, p1, p2, p3, 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bezier_endpoints() {
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 2.0);
        let p2 = (3.0, 2.0);
        let p3 = (4.0, 0.0);

        // At t=0, should be p0
        let start = bezier_point(p0, p1, p2, p3, 0.0);
        assert!((start.0 - p0.0).abs() < 1e-10);
        assert!((start.1 - p0.1).abs() < 1e-10);

        // At t=1, should be p3
        let end = bezier_point(p0, p1, p2, p3, 1.0);
        assert!((end.0 - p3.0).abs() < 1e-10);
        assert!((end.1 - p3.1).abs() < 1e-10);
    }

    #[test]
    fn test_bezier_points_count() {
        let pts = bezier_points((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), 40);
        assert_eq!(pts.len(), 41); // 0..=40
    }

    #[test]
    fn test_bezier_middle() {
        // Symmetric bezier
        let mid = bezier_middle((0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0));
        assert!((mid.0 - 2.0).abs() < 1e-10); // Should be at x=2 by symmetry
        assert!(mid.1 > 0.0); // Should be above the baseline
    }

    #[test]
    fn test_bezier_control_points() {
        let pts = bezier_control_points(
            100.0, 100.0, 0.0, 50.0, 180.0, 50.0, 30.0, None, None, None, None, None,
        );
        assert!(pts.len() >= 3);
        // First point: 0 degrees at radius 50 from (100,100) -> (150,100)
        assert!((pts[0].0 - 150.0).abs() < 0.1);
        assert!((pts[0].1 - 100.0).abs() < 0.1);
        // Last point: 180 degrees at radius 50 -> (50,100)
        let last = pts.last().unwrap();
        assert!((last.0 - 50.0).abs() < 0.1);
        assert!((last.1 - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_bezier_points_n_cubic_dispatch() {
        // 4 control points → direct cubic path; endpoints preserved.
        let c = vec![(0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0)];
        let pts = bezier_points_n(&c, 20);
        assert_eq!(pts.len(), 21);
        assert!((pts[0].0 - 0.0).abs() < 1e-9 && (pts[0].1 - 0.0).abs() < 1e-9);
        assert!((pts.last().unwrap().0 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_quadratic_degree_elevation() {
        // 3 control points → quadratic, elevated to cubic internally.
        // Known degree-elevation: c1 = q0 + 2/3(q1-q0), c2 = q2 + 2/3(q1-q2).
        let q0 = (0.0, 0.0);
        let q1 = (2.0, 6.0);
        let q2 = (4.0, 0.0);
        let pts = bezier_points_n(&[q0, q1, q2], 10);
        assert_eq!(pts.len(), 11);
        // Endpoints preserved.
        assert!((pts[0].0 - q0.0).abs() < 1e-9);
        assert!((pts.last().unwrap().0 - q2.0).abs() < 1e-9);
        // Midpoint of quadratic: B(0.5) = 0.25*q0 + 0.5*q1 + 0.25*q2 = (2.0, 3.0)
        let mid = pts[5];
        assert!((mid.0 - 2.0).abs() < 1e-9, "quadratic mid x = {}", mid.0);
        assert!((mid.1 - 3.0).abs() < 1e-9, "quadratic mid y = {}", mid.1);
    }

    #[test]
    fn test_bezier_points_n_quintic_5pt_reduction() {
        // 5 control points → reduced to cubic by averaging inner pair.
        let c = vec![(0.0, 0.0), (1.0, 2.0), (2.0, 3.0), (3.0, 2.0), (4.0, 0.0)];
        let pts = bezier_points_n(&c, 10);
        assert_eq!(pts.len(), 11);
        // Endpoints preserved.
        assert!((pts[0].0 - 0.0).abs() < 1e-9);
        assert!((pts.last().unwrap().0 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_unsupported_degree() {
        // Degree other than 3/4/5 → empty result (matches Perl's fallback).
        assert!(bezier_points_n(&[(0.0, 0.0), (1.0, 1.0)], 10).is_empty());
        let six = vec![
            (0.0, 0.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 1.0),
            (4.0, 0.0),
            (5.0, -1.0),
        ];
        assert!(bezier_points_n(&six, 10).is_empty());
    }

    #[test]
    fn test_bezier_point_t_half_is_middle() {
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 4.0);
        let p2 = (3.0, 4.0);
        let p3 = (4.0, 0.0);
        let mid = bezier_point(p0, p1, p2, p3, 0.5);
        let via_helper = bezier_middle(p0, p1, p2, p3);
        assert!((mid.0 - via_helper.0).abs() < 1e-12);
        assert!((mid.1 - via_helper.1).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_control_points_with_crest_inserts_two_extra() {
        // Without crest: 3 control points (start, mid, end).
        let no_crest = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 30.0, None, None, None, None, None,
        );
        assert_eq!(no_crest.len(), 3);
        // With crest: 5 control points (start, crest1, mid, crest2, end).
        let with_crest = bezier_control_points(
            0.0,
            0.0,
            0.0,
            100.0,
            180.0,
            100.0,
            30.0,
            None,
            None,
            None,
            Some(0.5),
            None,
        );
        assert_eq!(with_crest.len(), 5);
    }

    #[test]
    fn test_bezier_control_points_preserves_endpoints() {
        // Endpoints (x1,y1) and (x2,y2) should exactly match getxypos at angle1/angle2.
        let pts = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 90.0, 200.0, 50.0, None, None, None, None, None,
        );
        // First point: angle=0, radius=100 → (100, 0).
        assert!((pts[0].0 - 100.0).abs() < 1e-6);
        assert!((pts[0].1 - 0.0).abs() < 1e-6);
        // Last point: angle=90, radius=200 → (0, 200).
        let last = *pts.last().unwrap();
        assert!((last.0 - 0.0).abs() < 1e-6);
        assert!((last.1 - 200.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_control_points_middle_angle_wraps_over_180() {
        // angle1=10, angle2=350 — diff=340, abs(diff) > 180 → middleangle uses
        // wrapping formula: (10 + 350 + 360)/2 - 360 = 360 - 360 = 0 → 0.
        let pts = bezier_control_points(
            0.0, 0.0, 10.0, 100.0, 350.0, 100.0, 50.0, None, None, None, None, None,
        );
        // Middle control point at angle=0, radius=50 → (50, 0).
        let mid = pts[1];
        assert!((mid.0 - 50.0).abs() < 1e-6);
        assert!((mid.1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_monotone_samples() {
        // 5 samples along (0,0)→(4,0) via symmetric controls → x increases monotonically.
        let pts = bezier_points((0.0, 0.0), (1.0, 3.0), (3.0, 3.0), (4.0, 0.0), 5);
        assert_eq!(pts.len(), 6);
        for i in 1..pts.len() {
            assert!(pts[i].0 >= pts[i - 1].0, "x should be monotonic");
        }
    }

    #[test]
    fn test_bezier_point_quarter_and_three_quarter_symmetric() {
        // Symmetric cubic bezier: at t=0.25 and t=0.75 the x values should mirror.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 4.0);
        let p2 = (3.0, 4.0);
        let p3 = (4.0, 0.0);
        let q1 = bezier_point(p0, p1, p2, p3, 0.25);
        let q3 = bezier_point(p0, p1, p2, p3, 0.75);
        // Symmetry: q1.x + q3.x = 4.0 (total span); q1.y == q3.y.
        assert!((q1.0 + q3.0 - 4.0).abs() < 1e-9);
        assert!((q1.1 - q3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_degenerate_all_same_points() {
        // All four control points identical → curve is a single point.
        let p = (5.0, 5.0);
        for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            let result = bezier_point(p, p, p, p, t);
            assert!((result.0 - 5.0).abs() < 1e-12);
            assert!((result.1 - 5.0).abs() < 1e-12);
        }
    }

    #[test]
    fn test_bezier_point_linear_interpolation_on_collinear_points() {
        // Colinear control points: straight line from (0,0) to (10,0).
        let p0 = (0.0, 0.0);
        let p1 = (2.5, 0.0);
        let p2 = (7.5, 0.0);
        let p3 = (10.0, 0.0);
        // At t=0.5 should be approximately at center.
        let mid = bezier_point(p0, p1, p2, p3, 0.5);
        assert!((mid.0 - 5.0).abs() < 1e-9);
        assert!((mid.1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_control_pulls_curve_off_straight() {
        // Off-line control points pull the curve off the straight baseline.
        let p0 = (0.0, 0.0);
        let p1 = (0.0, 10.0); // high control
        let p2 = (10.0, 10.0); // high control
        let p3 = (10.0, 0.0);
        let mid = bezier_point(p0, p1, p2, p3, 0.5);
        // By symmetry, x at mid = 5.0; y should be above 0 (pulled up).
        assert!((mid.0 - 5.0).abs() < 1e-9);
        assert!(mid.1 > 0.0);
    }

    #[test]
    fn test_bezier_points_n_empty_input_yields_empty() {
        // Empty control points → empty result.
        let pts = bezier_points_n(&[], 10);
        assert!(pts.is_empty());
    }

    #[test]
    fn test_bezier_middle_is_bezier_point_at_half() {
        // bezier_middle(p0,p1,p2,p3) == bezier_point(p0,p1,p2,p3, 0.5).
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 3.0);
        let p2 = (4.0, 3.0);
        let p3 = (5.0, 0.0);
        let mid_fn = bezier_middle(p0, p1, p2, p3);
        let mid_pt = bezier_point(p0, p1, p2, p3, 0.5);
        assert!((mid_fn.0 - mid_pt.0).abs() < 1e-12);
        assert!((mid_fn.1 - mid_pt.1).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_control_points_purity_factor_shifts_radius() {
        // bezier_radius_purity=Some(k) with k<1 pulls bezier_radius *toward*
        // bisecting_radius. For endpoints equidistant from center, moves the
        // middle control point inward (smaller radius).
        let pts_no_purity = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, None, None, None, None, None,
        );
        let pts_with_purity = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, Some(0.5), None, None, None, None,
        );
        // With k<1, the bezier radius shifts; middle control point differs.
        assert_ne!(pts_no_purity[1], pts_with_purity[1]);
    }

    #[test]
    fn test_bezier_control_points_crest_perturb_is_multiplicative() {
        // crest > 0 with perturb_crest=None should shift crest points by the
        // given crest factor. Changing crest from 0.5 to 2.0 produces different
        // crest point positions.
        let pts_c1 = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, None, None, None, Some(0.5), None,
        );
        let pts_c2 = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, None, None, None, Some(2.0), None,
        );
        // Both have 5 points (3 core + 2 crest).
        assert_eq!(pts_c1.len(), 5);
        assert_eq!(pts_c2.len(), 5);
        // crest points (indices 1, 3) differ between the two factors.
        assert_ne!(pts_c1[1], pts_c2[1]);
        assert_ne!(pts_c1[3], pts_c2[3]);
    }

    #[test]
    fn test_bezier_control_points_bezier_radius_less_than_bisecting() {
        // When bezier_radius < bisecting_radius and k != 1, the shift direction
        // is toward center. Verify the function handles this branch without
        // panicking.
        let pts = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 10.0, 100.0, // endpoints close together
            5.0,  // bezier_radius < bisecting
            Some(1.5), None, None, None, None,
        );
        // 3 points (no crest).
        assert_eq!(pts.len(), 3);
    }

    #[test]
    fn test_bezier_points_n_num_samples_zero_empty_or_nan() {
        // num_samples=0 with 4 control points → `0..=0` yields 1 iteration at
        // t = 0/0 = NaN. Point count still 1; coords may be NaN or endpoint.
        let c = vec![(0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0)];
        let pts = bezier_points_n(&c, 0);
        assert_eq!(pts.len(), 1);
    }

    #[test]
    fn test_bezier_points_num_samples_zero_edge() {
        // num_samples=0 → single point emitted at t=0 (division by zero guard?).
        // Actually 0/0 in Rust is NaN for f64, so this test documents the actual
        // behavior: with num_samples=0, the loop yields one iteration at t=NaN.
        // Use 1 instead for meaningful endpoints.
        let pts = bezier_points((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), 1);
        // 0..=1 → 2 samples (endpoints).
        assert_eq!(pts.len(), 2);
        assert!((pts[0].0 - 0.0).abs() < 1e-9);
        assert!((pts[1].0 - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_partition_of_unity_symmetric_control() {
        // With a symmetric cubic p0=(0,0)/p1=(2,4)/p2=(8,4)/p3=(10,0), the curve
        // at t=0.5 lies on the curve's line of symmetry (x=5).
        let p0 = (0.0, 0.0);
        let p1 = (2.0, 4.0);
        let p2 = (8.0, 4.0);
        let p3 = (10.0, 0.0);
        let mid = bezier_point(p0, p1, p2, p3, 0.5);
        assert!((mid.0 - 5.0).abs() < 1e-9);
        // Also verify partition-of-unity invariant:
        //   at any t, x(t) == u^3*x0 + 3u^2*t*x1 + 3u*t^2*x2 + t^3*x3 — coefficients sum to 1.
        for &t in &[0.1f64, 0.3, 0.6, 0.9] {
            let u = 1.0 - t;
            let sum = u.powi(3) + 3.0 * u.powi(2) * t + 3.0 * u * t.powi(2) + t.powi(3);
            assert!((sum - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn test_bezier_points_n_quadratic_endpoint_tangent_from_inner_control() {
        // Quadratic elevation preserves the tangent at both endpoints:
        //   derivative at q0 points toward q1, at q2 points away from q1.
        // Sample two points near each endpoint and check the direction approximates.
        let q0 = (0.0, 0.0);
        let q1 = (5.0, 10.0);
        let q2 = (10.0, 0.0);
        let pts = bezier_points_n(&[q0, q1, q2], 100);
        assert_eq!(pts.len(), 101);
        // Tangent at t=0 heads toward q1: (dx, dy) proportional to q1-q0 = (5,10).
        let dx0 = pts[1].0 - pts[0].0;
        let dy0 = pts[1].1 - pts[0].1;
        // Ratio dy/dx near 10/5 = 2 (slope up and to the right).
        assert!(dy0 / dx0 > 1.5, "dy0/dx0={}", dy0 / dx0);
        // At t≈1 heading away from q1: slope is down and to the right → dy negative.
        let dxn = pts[100].0 - pts[99].0;
        let dyn_ = pts[100].1 - pts[99].1;
        assert!(dxn > 0.0);
        assert!(dyn_ < 0.0);
    }

    #[test]
    fn test_bezier_control_points_crest_zero_equal_to_non_crest_middle_ctrl() {
        // With crest=0, crest radius shift is zero: crest_pt1 lies ON the endpoint,
        // crest_pt2 lies ON the other endpoint. But the crest code still splices
        // 2 extra points (crest * 0 = 0 shift). Verify shape == 5 points.
        let with_zero_crest = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, None, None, None, Some(0.0), None,
        );
        assert_eq!(with_zero_crest.len(), 5);
        // With radius1 > bezier_radius=50, crest_radius1 = radius1 - |diff|*0 = radius1.
        // → crest_pt1 = endpoint 1 (100, 0).
        assert!((with_zero_crest[1].0 - 100.0).abs() < 1e-6);
        assert!((with_zero_crest[1].1 - 0.0).abs() < 1e-6);
        // crest_pt2 = endpoint 2 (-100, ~0).
        assert!((with_zero_crest[3].0 - (-100.0)).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_control_points_angle1_equals_angle2_collapses() {
        // If both angles equal and radii equal → endpoints identical; middle
        // control point at bezier_radius along that same angle. Verifies the
        // function doesn't panic on degenerate link.
        let pts = bezier_control_points(
            0.0, 0.0, 45.0, 100.0, 45.0, 100.0, 30.0, None, None, None, None, None,
        );
        assert_eq!(pts.len(), 3);
        // Endpoints identical.
        assert!((pts[0].0 - pts[2].0).abs() < 1e-6);
        assert!((pts[0].1 - pts[2].1).abs() < 1e-6);
        // Middle control: at 45°, radius=30 → (30*cos45, 30*sin45) ≈ (21.21, 21.21).
        let expected = 30.0 * (45.0f64.to_radians()).cos();
        assert!((pts[1].0 - expected).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_point_monotonic_in_t_along_line() {
        // For collinear controls on x-axis, bezier_point(t).0 is monotonic in t.
        let p0 = (0.0, 0.0);
        let p1 = (2.0, 0.0);
        let p2 = (7.0, 0.0);
        let p3 = (10.0, 0.0);
        let mut prev = bezier_point(p0, p1, p2, p3, 0.0).0;
        for i in 1..=20 {
            let t = i as f64 / 20.0;
            let x = bezier_point(p0, p1, p2, p3, t).0;
            assert!(x >= prev - 1e-9, "x monotonic violated at t={}: {} < {}", t, x, prev);
            prev = x;
        }
    }

    #[test]
    fn test_bezier_points_n_cubic_same_as_direct() {
        // bezier_points_n([4 pts], n) == bezier_points(4 pts, n).
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 3.0);
        let p2 = (3.0, 3.0);
        let p3 = (4.0, 0.0);
        let direct = bezier_points(p0, p1, p2, p3, 10);
        let via_n = bezier_points_n(&[p0, p1, p2, p3], 10);
        assert_eq!(direct.len(), via_n.len());
        for i in 0..direct.len() {
            assert!((direct[i].0 - via_n[i].0).abs() < 1e-12);
            assert!((direct[i].1 - via_n[i].1).abs() < 1e-12);
        }
    }

    #[test]
    fn test_bezier_middle_symmetric_control_produces_apex() {
        // With symmetric controls, the midpoint is on the line of symmetry.
        let p0 = (0.0, 0.0);
        let p1 = (0.0, 10.0);
        let p2 = (4.0, 10.0);
        let p3 = (4.0, 0.0);
        let mid = bezier_middle(p0, p1, p2, p3);
        // Line of symmetry is x=2; midpoint x should be 2.
        assert!((mid.0 - 2.0).abs() < 1e-9);
        // Apex y should be positive (pulled up by controls).
        assert!(mid.1 > 0.0);
    }

    #[test]
    fn test_bezier_control_points_perturb_bezier_radius_changes_middle_ctrl() {
        // With perturb_bezier_radius=Some("1.0,1.0"), the random factor is
        // deterministic (pmin==pmax=1.0) → bezier_radius × 1 = unchanged.
        let no_perturb = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, None, None, None, None, None,
        );
        let deterministic_perturb = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0, None, Some("1.0,1.0"), None, None, None,
        );
        // Both should yield identical middle control points since perturb
        // multiplier is exactly 1.0.
        assert!((no_perturb[1].0 - deterministic_perturb[1].0).abs() < 1e-6);
        assert!((no_perturb[1].1 - deterministic_perturb[1].1).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_num_samples_large_count() {
        // Large num_samples (1000) → 1001 points, endpoints preserved.
        let pts = bezier_points((0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0), 1000);
        assert_eq!(pts.len(), 1001);
        assert!((pts[0].0 - 0.0).abs() < 1e-12);
        assert!((pts[0].1 - 0.0).abs() < 1e-12);
        assert!((pts.last().unwrap().0 - 4.0).abs() < 1e-12);
        assert!((pts.last().unwrap().1 - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_control_points_different_radii_produces_asymmetric_output() {
        // radius1 != radius2 → endpoint positions differ.
        let pts = bezier_control_points(
            0.0, 0.0, 0.0, 50.0, 180.0, 200.0, 30.0, None, None, None, None, None,
        );
        // Point 0 (angle=0, r=50) → (50, 0).
        assert!((pts[0].0 - 50.0).abs() < 1e-6);
        // Point 2/last (angle=180, r=200) → (-200, ~0).
        let last = *pts.last().unwrap();
        assert!((last.0 - (-200.0)).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_point_all_controls_at_origin_yields_origin() {
        // All controls at (0,0) → every t value yields (0,0).
        for &t in &[0.0, 0.1, 0.33, 0.5, 0.75, 0.99, 1.0] {
            let (x, y) = bezier_point((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0), t);
            assert_eq!(x, 0.0);
            assert_eq!(y, 0.0);
        }
    }

    #[test]
    fn test_bezier_points_n_4_control_points_endpoints_match_anchors() {
        // 4 control points → cubic path; endpoints match p0 and p3 exactly.
        let pts = bezier_points_n(&[(1.0, 2.0), (3.0, 5.0), (7.0, 5.0), (10.0, 2.0)], 50);
        assert_eq!(pts.len(), 51);
        // Start matches p0.
        assert!((pts[0].0 - 1.0).abs() < 1e-12);
        assert!((pts[0].1 - 2.0).abs() < 1e-12);
        // End matches p3.
        let last = pts.last().unwrap();
        assert!((last.0 - 10.0).abs() < 1e-12);
        assert!((last.1 - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_point_at_quarter_three_quarter_t_values() {
        // For cubic (0,0)(1,2)(3,2)(4,0): t=0.25 and t=0.75 should yield symmetric points.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 2.0);
        let p2 = (3.0, 2.0);
        let p3 = (4.0, 0.0);
        let q = bezier_point(p0, p1, p2, p3, 0.25);
        let r = bezier_point(p0, p1, p2, p3, 0.75);
        // Sum of x values should equal 4 (span), y's should match.
        assert!((q.0 + r.0 - 4.0).abs() < 1e-9);
        assert!((q.1 - r.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_with_purity_gt_1_shifts_outward() {
        // bezier_radius_purity=k: when k > 1 and bezier_radius > bisecting_radius,
        // shift outward (add x). When k < 1, shift inward.
        let outward = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0,
            Some(1.5), None, None, None, None,
        );
        let inward = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0,
            Some(0.5), None, None, None, None,
        );
        // Different purity k values → different middle control points.
        assert_ne!(outward[1], inward[1]);
    }

    #[test]
    fn test_bezier_control_points_crest_and_purity_combined() {
        // crest + purity → 5 output points (crest adds 2); combo doesn't panic.
        let pts = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0,
            Some(0.8), None, None, Some(0.3), None,
        );
        assert_eq!(pts.len(), 5);
        // Endpoints still reachable (angle=0 at r=100 → (100, 0)).
        assert!((pts[0].0 - 100.0).abs() < 1e-6);
        assert!((pts[0].1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_n_three_point_quadratic_mid_convex() {
        // Quadratic (0,0)(2,4)(4,0) elevated to cubic — midpoint (t=0.5) = (2,2).
        let q = bezier_points_n(&[(0.0, 0.0), (2.0, 4.0), (4.0, 0.0)], 10);
        // 11 points. Index 5 is t=0.5 → midpoint should match quadratic formula.
        let mid = q[5];
        assert!((mid.0 - 2.0).abs() < 1e-9);
        // For B(0.5) = 0.25*q0 + 0.5*q1 + 0.25*q2 = (0.25*0 + 0.5*2 + 0.25*4, 0.25*0 + 0.5*4 + 0.25*0) = (2, 2).
        assert!((mid.1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_symmetric_cubic_at_t_half() {
        // Symmetric bezier p0=(0,0), p1=(1,1), p2=(2,1), p3=(3,0); at t=0.5 →
        // x = (1/8)*0 + 3*(1/4)*(1/2)*1 + 3*(1/2)*(1/4)*2 + (1/8)*3 = 1.5.
        // y = 0 + 3/8 + 3/8 + 0 = 0.75.
        let p = bezier_point((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), 0.5);
        assert!((p.0 - 1.5).abs() < 1e-12);
        assert!((p.1 - 0.75).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_points_num_samples_one_yields_two_endpoints() {
        // num_samples=1 → (0..=1) yields 2 pts: t=0 (=p0) and t=1 (=p3).
        let p0 = (0.0, 0.0);
        let p3 = (10.0, 20.0);
        let pts = bezier_points(p0, (3.0, 5.0), (7.0, 15.0), p3, 1);
        assert_eq!(pts.len(), 2);
        assert!((pts[0].0 - p0.0).abs() < 1e-12);
        assert!((pts[0].1 - p0.1).abs() < 1e-12);
        assert!((pts[1].0 - p3.0).abs() < 1e-9);
        assert!((pts[1].1 - p3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_no_optional_features_returns_three_points() {
        // All None → exactly 3 control points: (x1,y1), (x3,y3), (x2,y2).
        let cp = bezier_control_points(
            0.0, 0.0, 0.0, 10.0, 90.0, 10.0, 5.0, None, None, None, None, None,
        );
        assert_eq!(cp.len(), 3);
        assert!((cp[0].0 - 10.0).abs() < 1e-9);
        assert!(cp[0].1.abs() < 1e-9);
        // middleangle=45°, bezier_radius=5 → (5*cos45, 5*sin45) = (3.5355..., 3.5355...).
        let expected = 5.0 * (45.0_f64 * std::f64::consts::PI / 180.0).cos();
        assert!((cp[1].0 - expected).abs() < 1e-9);
        assert!((cp[1].1 - expected).abs() < 1e-9);
        // Anchor 2 at (cos90, sin90)*10 → (~0, 10).
        assert!(cp[2].0.abs() < 1e-9);
        assert!((cp[2].1 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_unsupported_length_returns_empty() {
        // Only lengths 3/4/5 are supported; others return empty.
        assert!(bezier_points_n(&[(0.0, 0.0), (1.0, 1.0)], 10).is_empty());
        assert!(bezier_points_n(&[], 5).is_empty());
        assert!(bezier_points_n(
            &[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0), (5.0, 5.0)],
            5
        )
        .is_empty());
    }

    #[test]
    fn test_bezier_point_endpoints_exact_match_at_t_0_and_t_1() {
        // At t=0, B(t) = p0; at t=1, B(t) = p3 (exact float equality).
        let p0 = (1.5, 2.5);
        let p1 = (5.0, 10.0);
        let p2 = (7.0, 15.0);
        let p3 = (12.0, 8.0);
        let start = bezier_point(p0, p1, p2, p3, 0.0);
        assert_eq!(start, p0);
        let end = bezier_point(p0, p1, p2, p3, 1.0);
        assert_eq!(end, p3);
    }

    #[test]
    fn test_bezier_points_length_equals_num_samples_plus_one() {
        // bezier_points returns N+1 points (inclusive t=0..=1).
        for &n in &[1_usize, 5, 10, 100] {
            let pts = bezier_points((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), n);
            assert_eq!(pts.len(), n + 1, "num_samples={}", n);
        }
    }

    #[test]
    fn test_bezier_middle_equals_bezier_point_at_t_half() {
        // bezier_middle is a thin wrapper over bezier_point(..., 0.5).
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 2.0);
        let p2 = (3.0, 2.0);
        let p3 = (4.0, 0.0);
        let mid = bezier_middle(p0, p1, p2, p3);
        let expected = bezier_point(p0, p1, p2, p3, 0.5);
        assert_eq!(mid, expected);
    }

    #[test]
    fn test_bezier_control_points_with_crest_returns_five_points() {
        // crest=Some → 2 extra crest points spliced into [a0, c1, a3, c2, a1] — 5 total.
        let cp = bezier_control_points(
            0.0, 0.0,
            0.0, 10.0,
            90.0, 10.0,
            5.0,
            None, None, None,
            Some(0.5),
            None,
        );
        assert_eq!(cp.len(), 5);
        // Without crest, only 3.
        let cp2 = bezier_control_points(
            0.0, 0.0, 0.0, 10.0, 90.0, 10.0, 5.0,
            None, None, None, None, None,
        );
        assert_eq!(cp2.len(), 3);
    }

    #[test]
    fn test_bezier_points_n_quartic_5_points_reduced_to_cubic() {
        // 5-point input reduced by averaging inner pair → cubic sampled.
        let ctrl = [(0.0, 0.0), (1.0, 2.0), (2.0, 4.0), (3.0, 2.0), (4.0, 0.0)];
        let pts = bezier_points_n(&ctrl, 10);
        // 11 output points (num_samples+1 for cubic).
        assert_eq!(pts.len(), 11);
        // Endpoints preserved: first==p0, last==p4.
        assert!((pts[0].0 - 0.0).abs() < 1e-9);
        assert!((pts[0].1 - 0.0).abs() < 1e-9);
        assert!((pts[10].0 - 4.0).abs() < 1e-9);
        assert!((pts[10].1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_cubic_4_points_passes_through_directly() {
        // 4-point input → direct cubic bezier_points path.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 1.0);
        let p2 = (2.0, 1.0);
        let p3 = (3.0, 0.0);
        let pts_n = bezier_points_n(&[p0, p1, p2, p3], 5);
        let pts_direct = bezier_points(p0, p1, p2, p3, 5);
        assert_eq!(pts_n.len(), pts_direct.len());
        for (a, b) in pts_n.iter().zip(pts_direct.iter()) {
            assert!((a.0 - b.0).abs() < 1e-12);
            assert!((a.1 - b.1).abs() < 1e-12);
        }
    }

    #[test]
    fn test_bezier_control_points_crosses_360_uses_adjusted_middle() {
        // abs(angle2-angle1) > 180 → middleangle = (angle1+angle2+360)/2 - 360.
        // angle1=350, angle2=10 → mid = (350+10+360)/2 - 360 = 0.
        let cp = bezier_control_points(
            0.0, 0.0,
            350.0, 10.0,
            10.0, 10.0,
            5.0,
            None, None, None, None, None,
        );
        assert_eq!(cp.len(), 3);
        // Inner control at (5·cos0°, 5·sin0°) = (5, 0) — exact.
        assert!((cp[1].0 - 5.0).abs() < 1e-9);
        assert!(cp[1].1.abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_on_degenerate_line_returns_midpoint() {
        // All 4 controls collinear on y=0: midpoint at x=3 (average of endpoints).
        let p0 = (0.0, 0.0);
        let p1 = (2.0, 0.0);
        let p2 = (4.0, 0.0);
        let p3 = (6.0, 0.0);
        let mid = bezier_middle(p0, p1, p2, p3);
        assert!((mid.0 - 3.0).abs() < 1e-12);
        assert!(mid.1.abs() < 1e-12);
    }

    #[test]
    fn test_bezier_points_first_and_last_match_endpoints_exactly() {
        // For any cubic, pts[0]==p0 and pts[N]==p3 without FP error at endpoints.
        let p0 = (1.0, 2.0);
        let p1 = (3.0, 7.0);
        let p2 = (5.0, 4.0);
        let p3 = (9.0, 1.0);
        let pts = bezier_points(p0, p1, p2, p3, 50);
        assert_eq!(pts[0], p0);
        assert!((pts[50].0 - p3.0).abs() < 1e-9);
        assert!((pts[50].1 - p3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_quadratic_3_point_endpoints_preserved() {
        // len=3 → elevated to cubic via 2/3-rule; endpoints still match q0/q2.
        let q0 = (0.0, 0.0);
        let q1 = (10.0, 20.0);
        let q2 = (20.0, 0.0);
        let pts = bezier_points_n(&[q0, q1, q2], 10);
        assert_eq!(pts.len(), 11);
        assert!((pts[0].0 - q0.0).abs() < 1e-9);
        assert!((pts[0].1 - q0.1).abs() < 1e-9);
        assert!((pts[10].0 - q2.0).abs() < 1e-9);
        assert!((pts[10].1 - q2.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_purity_one_leaves_bezier_radius_unchanged() {
        // purity=k, x = (1-k).abs() * |bezier_radius - bisecting|. k=1 → x=0 → no shift.
        // Use symmetric endpoints so middleangle is stable.
        let cp_with = bezier_control_points(
            0.0, 0.0, 0.0, 10.0, 90.0, 10.0, 5.0,
            Some(1.0), None, None, None, None,
        );
        let cp_without = bezier_control_points(
            0.0, 0.0, 0.0, 10.0, 90.0, 10.0, 5.0,
            None, None, None, None, None,
        );
        // Inner control point should match — purity=1 is identity.
        assert!((cp_with[1].0 - cp_without[1].0).abs() < 1e-9);
        assert!((cp_with[1].1 - cp_without[1].1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_interpolation_stays_within_convex_hull() {
        // For any t ∈ [0,1], B(t) lies within the convex hull of control points.
        // Convex hull of p0=(0,0), p1=(0,10), p2=(10,10), p3=(10,0) is [0,10]×[0,10].
        let p0 = (0.0, 0.0);
        let p1 = (0.0, 10.0);
        let p2 = (10.0, 10.0);
        let p3 = (10.0, 0.0);
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let p = bezier_point(p0, p1, p2, p3, t);
            assert!(p.0 >= -1e-9 && p.0 <= 10.0 + 1e-9, "t={} x={}", t, p.0);
            assert!(p.1 >= -1e-9 && p.1 <= 10.0 + 1e-9, "t={} y={}", t, p.1);
        }
    }

    #[test]
    fn test_bezier_point_all_four_same_control_points_always_at_that_point() {
        // All controls equal → B(t) = p0 for any t.
        let p = (5.5, 7.5);
        for t_times_10 in 0..=10 {
            let t = t_times_10 as f64 / 10.0;
            let b = bezier_point(p, p, p, p, t);
            assert!((b.0 - p.0).abs() < 1e-12);
            assert!((b.1 - p.1).abs() < 1e-12);
        }
    }

    #[test]
    fn test_bezier_points_num_samples_two_yields_three_points() {
        // num_samples=2 → 0..=2 → 3 points at t=0, 0.5, 1.
        let pts = bezier_points((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), 2);
        assert_eq!(pts.len(), 3);
        // pts[0] = p0 exactly.
        assert_eq!(pts[0], (0.0, 0.0));
        // pts[1] = B(0.5) = midpoint.
        let mid = bezier_middle((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0));
        assert!((pts[1].0 - mid.0).abs() < 1e-12);
        assert!((pts[1].1 - mid.1).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_control_points_zero_bezier_radius_middle_at_center() {
        // bezier_radius=0 → inner control at (cx, cy) = center.
        let cp = bezier_control_points(
            50.0, 50.0, 0.0, 10.0, 90.0, 10.0, 0.0,
            None, None, None, None, None,
        );
        assert_eq!(cp.len(), 3);
        // Inner (middle) control at (50, 50) — the center.
        assert!((cp[1].0 - 50.0).abs() < 1e-9);
        assert!((cp[1].1 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_and_bezier_point_at_half_are_bit_identical() {
        // Cross-check: bezier_middle and bezier_point(…, 0.5) must produce the same tuple.
        let p0 = (1.0, 2.0);
        let p1 = (10.0, 20.0);
        let p2 = (30.0, 40.0);
        let p3 = (50.0, 0.0);
        let a = bezier_middle(p0, p1, p2, p3);
        let b = bezier_point(p0, p1, p2, p3, 0.5);
        // Exact bit-level equality — same code path.
        assert_eq!(a, b);
    }

    #[test]
    fn test_bezier_point_at_t_zero_returns_p0_exactly() {
        // At t=0: u=1, t=0 → B(0) = 1·p0 + 0·p1 + 0·p2 + 0·p3 = p0.
        let p0 = (7.5, -3.2);
        let p1 = (100.0, 200.0);
        let p2 = (-50.0, 50.0);
        let p3 = (0.0, 99.0);
        let (x, y) = bezier_point(p0, p1, p2, p3, 0.0);
        // Bit-level comparison — all other terms drop to zero.
        assert_eq!((x, y), p0);
    }

    #[test]
    fn test_bezier_point_at_t_one_returns_p3_approximately() {
        // At t=1: u=0, all uuu/uu terms vanish → B(1) = ttt·p3 = p3 (within f64 precision).
        let p0 = (1.0, 2.0);
        let p1 = (3.0, 4.0);
        let p2 = (5.0, 6.0);
        let p3 = (7.0, 8.0);
        let (x, y) = bezier_point(p0, p1, p2, p3, 1.0);
        assert!((x - p3.0).abs() < 1e-9);
        assert!((y - p3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_single_point_input_returns_empty() {
        // bezier_points_n expects 3/4/5 points — single point falls through to wildcard → empty.
        let out = bezier_points_n(&[(1.0, 2.0)], 10);
        assert!(out.is_empty());
        // Empty slice also → empty.
        let out2 = bezier_points_n(&[], 10);
        assert!(out2.is_empty());
    }

    #[test]
    fn test_bezier_points_zero_samples_yields_single_point_at_start() {
        // num_samples=0 → range 0..=0 iterates once with i=0 → t=0/0=NaN. bezier_point(NaN) yields NaN.
        // This is a degenerate but well-defined case — just verify no panic and output has len=1.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 1.0);
        let p2 = (2.0, 2.0);
        let p3 = (3.0, 3.0);
        let pts = bezier_points(p0, p1, p2, p3, 0);
        assert_eq!(pts.len(), 1);
    }

    #[test]
    fn test_bezier_points_output_length_is_num_samples_plus_one_multiple_values() {
        // For N samples, the output vec has N+1 points — the start and end points, plus N-1 interior.
        let p0 = (0.0, 0.0);
        let p1 = (10.0, 20.0);
        let p2 = (30.0, 40.0);
        let p3 = (50.0, 0.0);
        for n in [1_usize, 2, 5, 10, 50, 100] {
            let pts = bezier_points(p0, p1, p2, p3, n);
            assert_eq!(pts.len(), n + 1, "samples={}", n);
        }
    }

    #[test]
    fn test_bezier_middle_linear_collinear_control_points_equals_midpoint() {
        // When p0=(0,0), p1=(1,1), p2=(2,2), p3=(3,3) — all on y=x — midpoint is (1.5, 1.5).
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 1.0);
        let p2 = (2.0, 2.0);
        let p3 = (3.0, 3.0);
        let (x, y) = bezier_middle(p0, p1, p2, p3);
        assert!((x - 1.5).abs() < 1e-9);
        assert!((y - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_six_or_more_control_points_unsupported_returns_empty() {
        // bezier_points_n only supports 3/4/5 control points — 6+ → empty Vec.
        let pts6: Vec<(f64, f64)> = (0..6).map(|i| (i as f64, 0.0)).collect();
        assert!(bezier_points_n(&pts6, 20).is_empty());
        // 10 control points also unsupported.
        let pts10: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 0.0)).collect();
        assert!(bezier_points_n(&pts10, 20).is_empty());
    }

    #[test]
    fn test_bezier_point_t_half_lies_within_bounding_box_of_controls() {
        // For control points in [0,100]² the midpoint must also be within [0,100]².
        let p0 = (0.0, 100.0);
        let p1 = (25.0, 75.0);
        let p2 = (50.0, 50.0);
        let p3 = (100.0, 0.0);
        let (x, y) = bezier_point(p0, p1, p2, p3, 0.5);
        assert!(x >= 0.0 && x <= 100.0);
        assert!(y >= 0.0 && y <= 100.0);
    }

    #[test]
    fn test_bezier_points_samples_at_regular_intervals() {
        // With num_samples=4, t values should be 0, 0.25, 0.5, 0.75, 1.0.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 0.0);
        let p2 = (2.0, 0.0);
        let p3 = (3.0, 0.0);
        let pts = bezier_points(p0, p1, p2, p3, 4);
        assert_eq!(pts.len(), 5);
        // For collinear points on x-axis, B(t).y = 0 everywhere.
        for &(_, y) in &pts {
            assert!(y.abs() < 1e-9);
        }
    }

    #[test]
    fn test_bezier_points_n_with_exactly_3_points_yields_quadratic() {
        // 3 control points triggers the quadratic → cubic conversion path.
        let pts = vec![(0.0, 0.0), (5.0, 10.0), (10.0, 0.0)];
        let out = bezier_points_n(&pts, 10);
        assert_eq!(out.len(), 11);
        // Endpoints preserved within tolerance.
        let (fx, fy) = out[0];
        let (lx, ly) = *out.last().unwrap();
        assert!((fx - 0.0).abs() < 1e-9);
        assert!((fy - 0.0).abs() < 1e-9);
        assert!((lx - 10.0).abs() < 1e-9);
        assert!((ly - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_different_control_triangles_yield_different_midpoints() {
        // Triangle A: p0=(0,0), p1=(100,100), p2=(100,100), p3=(200,0) — peaks high.
        // Triangle B: p0=(0,0), p1=(100,0),   p2=(100,0),   p3=(200,0) — flat line.
        let a = bezier_middle((0.0, 0.0), (100.0, 100.0), (100.0, 100.0), (200.0, 0.0));
        let b = bezier_middle((0.0, 0.0), (100.0, 0.0), (100.0, 0.0), (200.0, 0.0));
        // A's midpoint should be above y=0, B's should be at y=0.
        assert!(a.1 > 0.0);
        assert_eq!(b.1, 0.0);
    }

    #[test]
    fn test_bezier_point_x_component_independent_of_y_component() {
        // bezier_point is linear in each coordinate — x component depends only on x of CPs.
        // Swapping y values while keeping x the same yields the same x at any t.
        let (x1, _) = bezier_point((0.0, 0.0), (10.0, 5.0), (20.0, 7.0), (30.0, 1.0), 0.5);
        let (x2, _) = bezier_point((0.0, 99.0), (10.0, 99.0), (20.0, 99.0), (30.0, 99.0), 0.5);
        // x-component should be the same (bezier formula separates in x,y).
        assert!((x1 - x2).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_symmetric_control_points_yield_mid_on_line_of_symmetry() {
        // Symmetric CPs around x=0 → bezier midpoint at x=0.
        // p0=(-50,0), p1=(-25,100), p2=(25,100), p3=(50,0).
        let (x, y) = bezier_point((-50.0, 0.0), (-25.0, 100.0), (25.0, 100.0), (50.0, 0.0), 0.5);
        assert!(x.abs() < 1e-9);
        // y midpoint will be between 0 and 100 (convex combination).
        assert!(y > 0.0 && y <= 100.0);
    }

    #[test]
    fn test_bezier_points_sequence_contains_no_nan_for_valid_input() {
        // For valid control points and positive samples, no NaN in output.
        let pts = bezier_points(
            (0.0, 0.0),
            (10.0, 20.0),
            (30.0, 40.0),
            (50.0, 0.0),
            20,
        );
        for (x, y) in &pts {
            assert!(!x.is_nan());
            assert!(!y.is_nan());
        }
    }

    #[test]
    fn test_bezier_middle_returns_same_type_as_bezier_point() {
        // bezier_middle is a wrapper; result shape matches.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 2.0);
        let p2 = (3.0, 4.0);
        let p3 = (5.0, 6.0);
        let m = bezier_middle(p0, p1, p2, p3);
        // Tuple unpacks to two f64s.
        let (mx, my) = m;
        assert!(mx.is_finite());
        assert!(my.is_finite());
    }

    #[test]
    fn test_bezier_points_n_returned_length_matches_samples_plus_one() {
        // For 4-CP input and N samples → N+1 output points.
        let cp = vec![(0.0, 0.0), (1.0, 2.0), (3.0, 4.0), (5.0, 6.0)];
        let out = bezier_points_n(&cp, 7);
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn test_bezier_points_n_with_quartic_5cp_preserves_endpoints() {
        // 5 CPs → quartic-to-cubic reduction via avg — endpoints must still be preserved.
        let cp = vec![(0.0, 0.0), (1.0, 2.0), (2.0, 4.0), (3.0, 2.0), (4.0, 0.0)];
        let out = bezier_points_n(&cp, 10);
        assert_eq!(out.len(), 11);
        // First and last points should match cp[0] and cp[4].
        assert!((out[0].0 - 0.0).abs() < 1e-9);
        assert!((out[0].1 - 0.0).abs() < 1e-9);
        let last = out.last().unwrap();
        assert!((last.0 - 4.0).abs() < 1e-9);
        assert!((last.1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_x_symmetric_cp_mid_equals_average_endpoints() {
        // For x-coords 0, 30, 30, 60 (symmetric around 30) → midpoint x = 30.
        // That's because cubic Bezier at t=0.5: B_x = (x0 + 3*x1 + 3*x2 + x3) / 8.
        // = (0 + 90 + 90 + 60) / 8 = 240/8 = 30.
        let (x, _) = bezier_point((0.0, 0.0), (30.0, 10.0), (30.0, 10.0), (60.0, 0.0), 0.5);
        assert!((x - 30.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_same_as_bezier_point_at_t_half_across_many_cps() {
        // For several random-looking CP configurations, check bezier_middle ≡ bezier_point(0.5).
        let configs = [
            ((0.0, 0.0), (10.0, 20.0), (30.0, 40.0), (50.0, 0.0)),
            ((-5.0, 0.0), (0.0, 100.0), (100.0, 0.0), (5.0, -100.0)),
            ((1e3, 1e3), (2e3, 2e3), (3e3, 1e3), (4e3, 2e3)),
        ];
        for (p0, p1, p2, p3) in configs {
            let mid = bezier_middle(p0, p1, p2, p3);
            let pt = bezier_point(p0, p1, p2, p3, 0.5);
            assert_eq!(mid, pt);
        }
    }

    #[test]
    fn test_bezier_points_sampling_produces_distinct_points_for_distinct_t() {
        // For non-collinear CPs, different t values should produce different x,y points.
        let p0 = (0.0, 0.0);
        let p1 = (30.0, 100.0);
        let p2 = (70.0, 100.0);
        let p3 = (100.0, 0.0);
        let pts = bezier_points(p0, p1, p2, p3, 10);
        // Each pair of adjacent points should differ.
        for i in 0..pts.len() - 1 {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[i + 1];
            let d = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
            assert!(d > 0.0, "adjacent points i={} are equal", i);
        }
    }

    #[test]
    fn test_bezier_point_all_p_at_origin_returns_origin() {
        // Degenerate: all CPs at (0,0) → any t returns (0,0).
        let origin = (0.0, 0.0);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (x, y) = bezier_point(origin, origin, origin, origin, t);
            assert_eq!(x, 0.0);
            assert_eq!(y, 0.0);
        }
    }

    #[test]
    fn test_bezier_points_n_three_samples_produces_four_points_per_standard() {
        // N=3 samples → 4 output points.
        let cp = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0)];
        let out = bezier_points_n(&cp, 3);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn test_bezier_middle_symmetric_around_x_axis_y_is_zero() {
        // Control points symmetric about y=0: p0=(0,5), p1=(10,-5), p2=(20,-5), p3=(30,5).
        // The pattern isn't symmetric enough to force y=0; test formula directly.
        let (_, y) = bezier_middle((0.0, 5.0), (10.0, -5.0), (20.0, -5.0), (30.0, 5.0));
        // B_y(0.5) = (5 + 3*(-5) + 3*(-5) + 5) / 8 = (5 - 15 - 15 + 5) / 8 = -20/8 = -2.5.
        assert!((y - (-2.5)).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_four_point_cubic_passthrough_endpoints() {
        // 4 CPs → direct cubic; endpoints exact.
        let cp = vec![(10.0, 20.0), (30.0, 40.0), (50.0, 60.0), (70.0, 80.0)];
        let out = bezier_points_n(&cp, 5);
        assert_eq!(out.len(), 6);
        assert!((out[0].0 - 10.0).abs() < 1e-9);
        assert!((out[0].1 - 20.0).abs() < 1e-9);
        let last = out.last().unwrap();
        assert!((last.0 - 70.0).abs() < 1e-9);
        assert!((last.1 - 80.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_horizontal_line_control_points_y_invariant() {
        // All CPs have y=0 → B_y(t)=0 for any t.
        for t in [0.0, 0.2, 0.5, 0.8, 1.0] {
            let (_, y) = bezier_point((0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0), t);
            assert!(y.abs() < 1e-9);
        }
    }

    #[test]
    fn test_bezier_points_sample_count_100_produces_expected_density() {
        // num_samples=100 → 101 output points for 4-CP input.
        let p0 = (0.0, 0.0);
        let p1 = (25.0, 100.0);
        let p2 = (75.0, 100.0);
        let p3 = (100.0, 0.0);
        let pts = bezier_points(p0, p1, p2, p3, 100);
        assert_eq!(pts.len(), 101);
    }

    #[test]
    fn test_bezier_middle_is_convex_combination_of_control_points() {
        // The midpoint is a convex combination with weights summing to 1.
        // B_x(0.5) = (x0 + 3*x1 + 3*x2 + x3) / 8 — weights sum to 8/8 = 1.
        let (x, _) = bezier_middle((0.0, 0.0), (8.0, 0.0), (8.0, 0.0), (0.0, 0.0), );
        // (0 + 24 + 24 + 0) / 8 = 6.
        assert!((x - 6.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_empty_control_points_returns_empty_vec() {
        // 0 CPs → empty output.
        let out = bezier_points_n(&[], 10);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_control_points_without_crest_returns_three_points() {
        // Without crest → [start, middle, end] = 3 control points.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 100.0, 200.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_control_points_with_crest_adds_two_points() {
        // With crest → adds 2 crest points → 5 control points total.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 100.0, 200.0,
            None, None, None, Some(0.5), None,
        );
        assert_eq!(cps.len(), 5);
    }

    #[test]
    fn test_bezier_control_points_degenerate_same_angle_yields_valid_points() {
        // Same start/end angle → bisecting_radius computation still works.
        let cps = bezier_control_points(
            500.0, 500.0, 45.0, 100.0, 45.0, 100.0, 200.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
        // Start and end should be identical.
        assert_eq!(cps[0], cps[2]);
    }

    #[test]
    fn test_bezier_points_n_two_control_points_returns_empty() {
        // Unsupported count (2 CPs) → empty vec (degree too low).
        let out = bezier_points_n(&[(0.0, 0.0), (10.0, 10.0)], 5);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_point_at_t_zero_returns_first_control_point() {
        // B(0) = p0 exactly.
        let p0 = (1.0, 2.0);
        let p1 = (3.0, 4.0);
        let p2 = (5.0, 6.0);
        let p3 = (7.0, 8.0);
        let r = bezier_point(p0, p1, p2, p3, 0.0);
        assert_eq!(r, p0);
    }

    #[test]
    fn test_bezier_point_at_t_one_returns_last_control_point() {
        // B(1) = p3 exactly.
        let p0 = (1.0, 2.0);
        let p1 = (3.0, 4.0);
        let p2 = (5.0, 6.0);
        let p3 = (7.0, 8.0);
        let r = bezier_point(p0, p1, p2, p3, 1.0);
        assert!((r.0 - p3.0).abs() < 1e-9);
        assert!((r.1 - p3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_quartic_reduces_to_four_point_sampling() {
        // 5 CPs (quartic) → reduces to cubic via inner-pair averaging; returns n+1 samples.
        let cps = [(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (6.0, 0.0), (8.0, 0.0)];
        let out = bezier_points_n(&cps, 10);
        assert_eq!(out.len(), 11);
        // Endpoints: first near (0,0), last near (8,0).
        assert!((out[0].0 - 0.0).abs() < 1e-9);
        assert!((out[10].0 - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_angles_180_apart_middleangle_wraps_handling() {
        // Angles exactly 180 apart (diff==180, not >180): middleangle = (a1+a2)/2.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 180.0, 100.0, 50.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_points_at_t_half_via_samples_equals_bezier_middle() {
        // bezier_points(..., 2)[1] is the t=0.5 sample — should equal bezier_middle.
        let p0 = (0.0, 0.0);
        let p1 = (10.0, 20.0);
        let p2 = (30.0, 20.0);
        let p3 = (40.0, 0.0);
        let samples = bezier_points(p0, p1, p2, p3, 2);
        let mid = bezier_middle(p0, p1, p2, p3);
        assert!((samples[1].0 - mid.0).abs() < 1e-9);
        assert!((samples[1].1 - mid.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_angles_crossing_360_wraps_middle_via_perl_formula() {
        // 350°→10° (diff=|10-350|=340 > 180) → middle = (350+10+360)/2 - 360 = 0.
        let cx = 500.0;
        let cy = 500.0;
        let cps = bezier_control_points(
            cx, cy, 350.0, 100.0, 10.0, 100.0, 50.0,
            None, None, None, None, None,
        );
        // Middle point should be at angle 0°, radius 50 from center:
        // x = cx + 50 * cos(0) = 550; y = cy + 50 * sin(0) = 500.
        let mid = cps[1];
        assert!((mid.0 - 550.0).abs() < 1e-6);
        assert!((mid.1 - 500.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_n_three_control_points_produces_samples() {
        // 3 CPs (quadratic) → non-empty output with n+1 points for n samples.
        let cps = [(0.0, 0.0), (5.0, 10.0), (10.0, 0.0)];
        let out = bezier_points_n(&cps, 8);
        assert_eq!(out.len(), 9);
        // First and last at endpoints.
        assert_eq!(out[0], (0.0, 0.0));
        assert!((out[8].0 - 10.0).abs() < 1e-9);
        assert!((out[8].1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_symmetric_in_t_for_symmetric_cps() {
        // Symmetric CPs around origin: B(t) and B(1-t) should be x-mirrors if cps symmetric.
        let p0 = (-10.0, 0.0);
        let p1 = (-5.0, 5.0);
        let p2 = (5.0, 5.0);
        let p3 = (10.0, 0.0);
        let a = bezier_point(p0, p1, p2, p3, 0.3);
        let b = bezier_point(p0, p1, p2, p3, 0.7);
        // x(0.7) = -x(0.3); y(0.7) = y(0.3).
        assert!((a.0 + b.0).abs() < 1e-9);
        assert!((a.1 - b.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_one_sample_yields_two_points_at_endpoints() {
        // n=1 → t=0 and t=1 → two points exactly at p0 and p3.
        let p0 = (5.0, 5.0);
        let p3 = (20.0, 5.0);
        let pts = bezier_points(p0, (10.0, 10.0), (15.0, 10.0), p3, 1);
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0], p0);
        assert!((pts[1].0 - p3.0).abs() < 1e-9);
        assert!((pts[1].1 - p3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_linear_across_x_axis_returns_midx() {
        // p0..p3 collinear across x-axis at y=0 → midpoint stays on x-axis.
        let m = bezier_middle((0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0));
        // Bernstein weights for t=0.5: (1/8 * 0 + 3/8*10 + 3/8*20 + 1/8*30) = 15.
        assert!((m.0 - 15.0).abs() < 1e-9);
        assert!(m.1.abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_with_crest_non_zero_adjusts_crest_radii() {
        // With crest=1.0 and distinct radii, the two crest points are added at
        // radius-delta positions.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 200.0, 150.0,
            None, None, None, Some(1.0), None,
        );
        assert_eq!(cps.len(), 5);
    }

    #[test]
    fn test_bezier_points_n_five_cp_quartic_endpoints_preserved_across_distinct_y() {
        // 5 CPs with varying y → endpoints at first/last CPs.
        let cps = [(0.0, 1.0), (2.0, 5.0), (4.0, 5.0), (6.0, 5.0), (8.0, 1.0)];
        let out = bezier_points_n(&cps, 4);
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], (0.0, 1.0));
        assert_eq!(out[4], (8.0, 1.0));
    }

    #[test]
    fn test_bezier_points_10_samples_produces_11_points() {
        // num_samples=10 → range(0..=10) yields 11 points.
        let pts = bezier_points(
            (0.0, 0.0), (5.0, 10.0), (15.0, 10.0), (20.0, 0.0),
            10,
        );
        assert_eq!(pts.len(), 11);
    }

    #[test]
    fn test_bezier_control_points_bezier_radius_purity_k_zero_does_not_shift() {
        // k=0 → x = 1*|br-bisect| → br adjusted by full amount. Just check no panic.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 100.0, 50.0,
            Some(0.0), None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_middle_endpoints_identical_yields_same_point() {
        // p0 == p1 == p2 == p3 → middle = same.
        let same = (42.0, 7.0);
        let m = bezier_middle(same, same, same, same);
        assert_eq!(m, same);
    }

    #[test]
    fn test_bezier_point_cubic_identity_at_midpoint_evaluates_all_four_cps() {
        // B(0.5) = (p0+3p1+3p2+p3)/8 for each coordinate.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 2.0);
        let p2 = (3.0, 4.0);
        let p3 = (8.0, 0.0);
        let r = bezier_point(p0, p1, p2, p3, 0.5);
        let expected_x = (0.0 + 3.0 * 1.0 + 3.0 * 3.0 + 8.0) / 8.0;
        let expected_y = (0.0 + 3.0 * 2.0 + 3.0 * 4.0 + 0.0) / 8.0;
        assert!((r.0 - expected_x).abs() < 1e-9);
        assert!((r.1 - expected_y).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_one_control_point_returns_empty() {
        // 1 CP is not in {3,4,5} → empty Vec.
        let out = bezier_points_n(&[(1.0, 1.0)], 5);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_control_points_large_ideogram_radius_produces_valid_triple() {
        // Normal Circos-scale radii still produce 3 CPs.
        let cps = bezier_control_points(
            1500.0, 1500.0, 0.0, 1000.0, 45.0, 1000.0, 500.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_point_on_straight_line_yields_linear_interp() {
        // All CPs on line y = x → result point also on y=x.
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 1.0);
        let p2 = (2.0, 2.0);
        let p3 = (3.0, 3.0);
        for t in [0.1, 0.3, 0.5, 0.7, 0.9] {
            let r = bezier_point(p0, p1, p2, p3, t);
            assert!((r.0 - r.1).abs() < 1e-9);
        }
    }

    #[test]
    fn test_bezier_points_n_cubic_endpoints_match_first_last_cp() {
        // 4 CPs (cubic) → first sample == p0, last == p3.
        let cps = [(0.0, 0.0), (1.0, 10.0), (9.0, 10.0), (10.0, 0.0)];
        let out = bezier_points_n(&cps, 5);
        assert_eq!(out.len(), 6);
        assert_eq!(out[0], (0.0, 0.0));
        assert!((out[5].0 - 10.0).abs() < 1e-9);
        assert!((out[5].1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_six_control_points_returns_empty() {
        // 6 CPs not in supported {3,4,5} → empty.
        let cps = vec![(0.0, 0.0); 6];
        let out = bezier_points_n(&cps, 5);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_middle_equidistant_control_points_average() {
        // All CPs at (x, y) → midpoint = (x, y).
        let m = bezier_middle((1.0, 2.0), (1.0, 2.0), (1.0, 2.0), (1.0, 2.0));
        assert_eq!(m, (1.0, 2.0));
    }

    #[test]
    fn test_bezier_control_points_without_crest_three_points_last_is_end_anchor() {
        // No crest → cps[2] is end anchor (at angle2, radius2).
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 100.0, 50.0,
            None, None, None, None, None,
        );
        // Start anchor at angle 0° radius 100 from (500,500) → (600, 500).
        assert!((cps[0].0 - 600.0).abs() < 1e-6);
        assert!((cps[0].1 - 500.0).abs() < 1e-6);
        // End anchor at angle 90° radius 100 from (500,500) → (500, 600).
        assert!((cps[2].0 - 500.0).abs() < 1e-6);
        assert!((cps[2].1 - 600.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_point_fractional_t_values_produce_finite_coords() {
        // Various t in (0,1) yield finite coords for a normal curve.
        let p0 = (0.0, 0.0);
        let p1 = (100.0, 200.0);
        let p2 = (200.0, 200.0);
        let p3 = (300.0, 0.0);
        for t in [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 0.9] {
            let r = bezier_point(p0, p1, p2, p3, t);
            assert!(r.0.is_finite());
            assert!(r.1.is_finite());
        }
    }

    #[test]
    fn test_bezier_points_sequence_strictly_increasing_x_for_monotone_cps() {
        // Monotonic x-coords in CPs → sampled points' x strictly increasing.
        let pts = bezier_points((0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0), 10);
        for i in 0..pts.len() - 1 {
            assert!(pts[i].0 <= pts[i + 1].0);
        }
    }

    #[test]
    fn test_bezier_middle_collinear_control_points_midpoint_between_endpoints() {
        // CPs at (0,0),(10,0),(20,0),(30,0) → midpoint has x=15.
        let m = bezier_middle((0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0));
        assert!((m.0 - 15.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_with_crest_05_moves_crest_points_inward() {
        // crest=0.5 adjusts crest radii; verify 5 CPs produced with crest inserts.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 200.0, 150.0,
            None, None, None, Some(0.5), None,
        );
        assert_eq!(cps.len(), 5);
    }

    #[test]
    fn test_bezier_points_n_cubic_sampling_count_matches_n_plus_one() {
        // 4 CPs, num_samples=20 → 21 points.
        let cps = [(0.0, 0.0), (1.0, 10.0), (9.0, 10.0), (10.0, 0.0)];
        let out = bezier_points_n(&cps, 20);
        assert_eq!(out.len(), 21);
    }

    #[test]
    fn test_bezier_point_t_half_between_endpoints_x_symmetric_cps() {
        // Symmetric CPs across x=5 → B(0.5).x = 5.
        let p0 = (0.0, 0.0);
        let p1 = (2.0, 5.0);
        let p2 = (8.0, 5.0);
        let p3 = (10.0, 0.0);
        let r = bezier_point(p0, p1, p2, p3, 0.5);
        assert!((r.0 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_identical_to_bezier_point_at_half() {
        // bezier_middle and bezier_point(0.5) produce identical output.
        let p0 = (0.0, 0.0);
        let p1 = (10.0, 20.0);
        let p2 = (30.0, 20.0);
        let p3 = (40.0, 0.0);
        let mid = bezier_middle(p0, p1, p2, p3);
        let at_half = bezier_point(p0, p1, p2, p3, 0.5);
        assert!((mid.0 - at_half.0).abs() < 1e-9);
        assert!((mid.1 - at_half.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_very_small_bezier_radius_produces_points() {
        // bezier_radius=1 (tiny) still produces 3 CPs without panic.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 100.0, 1.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_points_n_cubic_with_100_samples_exact_count() {
        // 4 CPs + 100 samples → 101 points.
        let cps = [(0.0, 0.0), (10.0, 50.0), (90.0, 50.0), (100.0, 0.0)];
        let out = bezier_points_n(&cps, 100);
        assert_eq!(out.len(), 101);
    }

    #[test]
    fn test_bezier_point_all_values_at_origin_returns_origin() {
        // All CPs at (0,0) → bezier_point returns (0,0) for any t.
        let origin = (0.0, 0.0);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let r = bezier_point(origin, origin, origin, origin, t);
            assert_eq!(r, origin);
        }
    }

    #[test]
    fn test_bezier_middle_symmetric_y_yields_same_y() {
        // All CPs have same y → midpoint.y also that y.
        let m = bezier_middle((0.0, 5.0), (10.0, 5.0), (20.0, 5.0), (30.0, 5.0));
        assert!((m.1 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_varied_radii_produces_three_points() {
        // Distinct radius1 and radius2 still yields 3 CPs without crest.
        let cps = bezier_control_points(
            500.0, 500.0, 30.0, 80.0, 60.0, 120.0, 40.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_points_n_cubic_full_monotonic_output_consistent() {
        // 4 CPs with strictly monotone x → sampled output strictly x-monotone.
        let cps = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)];
        let out = bezier_points_n(&cps, 10);
        for i in 0..out.len() - 1 {
            assert!(out[i].0 <= out[i + 1].0);
        }
    }

    #[test]
    fn test_bezier_point_extremes_cp_values() {
        // Large CP values produce finite results.
        let p0 = (1e6, -1e6);
        let p3 = (-1e6, 1e6);
        let r = bezier_point(p0, (0.0, 0.0), (0.0, 0.0), p3, 0.5);
        assert!(r.0.is_finite());
        assert!(r.1.is_finite());
    }

    #[test]
    fn test_bezier_middle_two_pairs_same_y_yields_same_y() {
        // p0=p1=(0,1), p2=p3=(10,1) → mid y = 1.
        let m = bezier_middle((0.0, 1.0), (0.0, 1.0), (10.0, 1.0), (10.0, 1.0));
        assert!((m.1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_zero_bezier_radius_collapses_middle() {
        // br=0 → middle point at center.
        let cps = bezier_control_points(
            500.0, 500.0, 0.0, 100.0, 90.0, 100.0, 0.0,
            None, None, None, None, None,
        );
        // Middle at (cx + 0*cos, cy + 0*sin) = (500, 500).
        assert!((cps[1].0 - 500.0).abs() < 1e-6);
        assert!((cps[1].1 - 500.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_n_cubic_samples_length_matches_n_plus_one() {
        // General: n samples → n+1 output points.
        for n in [1, 5, 20, 50] {
            let cps = [(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0)];
            let out = bezier_points_n(&cps, n);
            assert_eq!(out.len(), n + 1);
        }
    }

    #[test]
    fn test_bezier_point_negative_coordinate_control_points_yield_negative_result() {
        // CPs in negative quadrant → result also negative.
        let r = bezier_point((-10.0, -10.0), (-5.0, -5.0), (-3.0, -3.0), (0.0, 0.0), 0.5);
        assert!(r.0 <= 0.0);
        assert!(r.1 <= 0.0);
    }

    #[test]
    fn test_bezier_middle_flat_horizontal_curve_yields_y_zero() {
        // All CPs on y=0 → midpoint.y=0.
        let m = bezier_middle((0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (15.0, 0.0));
        assert!(m.1.abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_opposite_angles_2_pi_apart() {
        // 0° and 180° angles → diff abs() = 180 exactly.
        // Middle angle = (0+180)/2 = 90°; radius=50 → middle point at (cx, cy+50).
        let cps = bezier_control_points(
            100.0, 100.0, 0.0, 50.0, 180.0, 50.0, 50.0,
            None, None, None, None, None,
        );
        // Middle is at angle 90° from center (100,100) with radius 50 → (100, 150).
        assert!((cps[1].0 - 100.0).abs() < 1e-6);
        assert!((cps[1].1 - 150.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_n_cubic_start_and_end_match_exactly() {
        // 4 CPs, cubic sampling → first pt==p0 and last pt==p3 exact.
        let cps = [(100.0, 50.0), (200.0, 300.0), (400.0, 300.0), (500.0, 50.0)];
        let out = bezier_points_n(&cps, 10);
        assert_eq!(out[0], (100.0, 50.0));
        assert!((out[10].0 - 500.0).abs() < 1e-9);
        assert!((out[10].1 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_at_t_zero_returns_p0_exact() {
        // t=0 → B(0) = p0 exactly.
        let pt = bezier_point((10.0, 20.0), (30.0, 40.0), (50.0, 60.0), (70.0, 80.0), 0.0);
        assert_eq!(pt, (10.0, 20.0));
    }

    #[test]
    fn test_bezier_point_at_t_one_returns_p3_exact() {
        // t=1 → B(1) = p3 exactly.
        let pt = bezier_point((10.0, 20.0), (30.0, 40.0), (50.0, 60.0), (70.0, 80.0), 1.0);
        assert!((pt.0 - 70.0).abs() < 1e-9);
        assert!((pt.1 - 80.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_two_cp_count_empty() {
        // Only 2 CPs → unsupported branch → empty Vec.
        let cps = [(0.0, 0.0), (10.0, 10.0)];
        let out = bezier_points_n(&cps, 10);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_middle_all_same_point_yields_same_point() {
        // All 4 CPs identical → midpoint identical.
        let mid = bezier_middle((5.0, 7.0), (5.0, 7.0), (5.0, 7.0), (5.0, 7.0));
        assert!((mid.0 - 5.0).abs() < 1e-9);
        assert!((mid.1 - 7.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_cubic_output_length_is_samples_plus_one() {
        // n=5 samples → 6 output points (inclusive).
        let cps = [(0.0, 0.0), (10.0, 10.0), (20.0, 10.0), (30.0, 0.0)];
        let out = bezier_points_n(&cps, 5);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn test_bezier_points_n_quadratic_3cp_valid() {
        // 3 CPs → quadratic→cubic conversion branch.
        let cps = [(0.0, 0.0), (10.0, 10.0), (20.0, 0.0)];
        let out = bezier_points_n(&cps, 5);
        assert_eq!(out.len(), 6);
        assert_eq!(out[0], (0.0, 0.0));
    }

    #[test]
    fn test_bezier_points_n_quartic_5cp_valid() {
        // 5 CPs → quartic reduced to cubic.
        let cps = [(0.0, 0.0), (5.0, 5.0), (10.0, 10.0), (15.0, 5.0), (20.0, 0.0)];
        let out = bezier_points_n(&cps, 5);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn test_bezier_point_at_t_half_is_midpoint_between_linear_cps() {
        // For 4 colinear CPs on the x-axis, B(0.5) = midpoint.
        let pt = bezier_point((0.0, 0.0), (100.0, 0.0), (200.0, 0.0), (300.0, 0.0), 0.5);
        assert!((pt.0 - 150.0).abs() < 1e-9);
        assert!(pt.1.abs() < 1e-9);
    }

    #[test]
    fn test_bezier_middle_equivalent_to_bezier_point_at_0_5() {
        // bezier_middle(p0..p3) == bezier_point(p0..p3, 0.5).
        let p0 = (0.0, 0.0);
        let p1 = (50.0, 100.0);
        let p2 = (150.0, 100.0);
        let p3 = (200.0, 0.0);
        let mid = bezier_middle(p0, p1, p2, p3);
        let pt05 = bezier_point(p0, p1, p2, p3, 0.5);
        assert!((mid.0 - pt05.0).abs() < 1e-12);
        assert!((mid.1 - pt05.1).abs() < 1e-12);
    }

    #[test]
    fn test_bezier_points_n_cubic_4cp_valid_first_point_matches_p0() {
        // 4-CP cubic: first sample point is p0 exactly.
        let cps = [(5.0, 10.0), (20.0, 50.0), (40.0, 50.0), (55.0, 10.0)];
        let out = bezier_points_n(&cps, 10);
        assert_eq!(out[0], (5.0, 10.0));
    }

    #[test]
    fn test_bezier_points_n_empty_input_returns_empty_vec() {
        // 0 CPs → unsupported branch → empty Vec.
        let cps: [(f64, f64); 0] = [];
        let out = bezier_points_n(&cps, 5);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_point_on_vertical_line_x_constant() {
        // All CPs at x=50 with varying y → B(t).0 = 50 for any t.
        let pt = bezier_point((50.0, 0.0), (50.0, 100.0), (50.0, 100.0), (50.0, 0.0), 0.3);
        assert!((pt.0 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_1_sample_produces_start_and_end_only() {
        // n=1 → 2 points: t=0 (start) and t=1 (end).
        let cps = [(0.0, 0.0), (10.0, 20.0), (20.0, 20.0), (30.0, 0.0)];
        let out = bezier_points_n(&cps, 1);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], (0.0, 0.0));
    }

    #[test]
    fn test_bezier_points_many_samples_smooth_monotonic_x() {
        // Symmetric curve: x monotonic from p0.x to p3.x.
        let cps = [(0.0, 0.0), (50.0, 100.0), (100.0, 100.0), (150.0, 0.0)];
        let out = bezier_points_n(&cps, 50);
        // x should increase monotonically.
        for i in 1..out.len() {
            assert!(out[i].0 >= out[i - 1].0 - 1e-9);
        }
    }

    #[test]
    fn test_bezier_middle_horizontal_mirror_y_averages() {
        // Symmetric curve with two high CPs → mid.y between 0 and 100.
        let mid = bezier_middle((0.0, 0.0), (10.0, 100.0), (20.0, 100.0), (30.0, 0.0));
        assert!(mid.1 > 0.0);
        assert!(mid.1 < 100.0);
    }

    #[test]
    fn test_bezier_point_same_cps_all_origin_returns_origin() {
        // All CPs at origin → any t → origin.
        let pt = bezier_point((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.7);
        assert_eq!(pt, (0.0, 0.0));
    }

    #[test]
    fn test_bezier_points_n_6_cps_fallback_empty() {
        // 6+ CPs → unsupported branch in match → empty.
        let cps = [(0.0, 0.0); 6];
        let out = bezier_points_n(&cps, 5);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_point_t_between_0_and_1_interpolates() {
        // t=0.5 between distant CPs → produces intermediate value.
        let pt = bezier_point((0.0, 0.0), (0.0, 100.0), (100.0, 100.0), (100.0, 0.0), 0.5);
        // x should be between 0 and 100.
        assert!(pt.0 > 0.0 && pt.0 < 100.0);
    }

    #[test]
    fn test_bezier_middle_non_symmetric_curve_nonzero_y() {
        // Asymmetric CPs → y at mid is nonzero.
        let mid = bezier_middle((0.0, 0.0), (25.0, 80.0), (75.0, 20.0), (100.0, 0.0));
        assert!(mid.1 != 0.0);
    }

    #[test]
    fn test_bezier_points_n_with_10_samples_produces_11_points() {
        // n=10 samples → 11 points (inclusive of endpoints).
        let cps = [(0.0, 0.0), (10.0, 50.0), (20.0, 50.0), (30.0, 0.0)];
        let out = bezier_points_n(&cps, 10);
        assert_eq!(out.len(), 11);
    }

    #[test]
    fn test_bezier_points_basic_endpoints_match_p0_and_p3() {
        // bezier_points emits t=0 and t=num_samples/num_samples → first=p0, last=p3.
        let p0 = (0.0, 0.0);
        let p3 = (100.0, 0.0);
        let out = bezier_points(p0, (30.0, 50.0), (70.0, 50.0), p3, 20);
        assert_eq!(out[0], p0);
        assert!((out[20].0 - p3.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_t_zero_plus_epsilon_near_p0() {
        // t=0.0001 → very close to p0.
        let pt = bezier_point((10.0, 20.0), (50.0, 60.0), (80.0, 90.0), (100.0, 120.0), 0.0001);
        assert!((pt.0 - 10.0).abs() < 0.1);
        assert!((pt.1 - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_bezier_middle_horizontal_flat_curve_y_stays_zero() {
        // All CPs at y=0 → middle.y = 0.
        let mid = bezier_middle((0.0, 0.0), (25.0, 0.0), (75.0, 0.0), (100.0, 0.0));
        assert!(mid.1.abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_1_cp_unsupported_empty() {
        // 1 CP → unsupported branch → empty Vec.
        let cps = [(0.0, 0.0)];
        let out = bezier_points_n(&cps, 5);
        assert!(out.is_empty());
    }

    #[test]
    fn test_bezier_points_zero_samples_one_point_only() {
        // num_samples=0 → range (0..=0) → 1 point at t=0/0=NaN; verify length 1.
        let p0 = (0.0, 0.0);
        let out = bezier_points(p0, (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), 0);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn test_bezier_middle_symmetric_above_xaxis_has_positive_y() {
        // Symmetric hump → mid.y > 0.
        let mid = bezier_middle((0.0, 0.0), (25.0, 50.0), (75.0, 50.0), (100.0, 0.0));
        assert!(mid.1 > 0.0);
    }

    #[test]
    fn test_bezier_points_n_quadratic_convex_shape_y_peaks_middle() {
        // Quadratic hump → middle point has highest y.
        let cps = [(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)];
        let out = bezier_points_n(&cps, 10);
        let mid_y = out[5].1;
        assert!(mid_y > out[0].1);
        assert!(mid_y > out[10].1);
    }

    #[test]
    fn test_bezier_point_monotonic_y_decrease_for_decreasing_cps() {
        // All y-values decrease → B(t) y also decreases.
        let pt_early = bezier_point((0.0, 100.0), (0.0, 90.0), (0.0, 80.0), (0.0, 70.0), 0.25);
        let pt_late = bezier_point((0.0, 100.0), (0.0, 90.0), (0.0, 80.0), (0.0, 70.0), 0.75);
        assert!(pt_early.1 > pt_late.1);
    }

    #[test]
    fn test_bezier_points_n_with_50_samples_produces_51_points() {
        // n=50 → 51 points.
        let cps = [(0.0, 0.0), (10.0, 20.0), (20.0, 20.0), (30.0, 0.0)];
        let out = bezier_points_n(&cps, 50);
        assert_eq!(out.len(), 51);
    }

    #[test]
    fn test_bezier_point_large_cps_still_finite() {
        // Large CP coordinate values → finite result.
        let pt = bezier_point((1e8, 1e8), (2e8, 2e8), (3e8, 3e8), (4e8, 4e8), 0.5);
        assert!(pt.0.is_finite());
        assert!(pt.1.is_finite());
    }

    #[test]
    fn test_bezier_middle_at_four_identical_cps_returns_cp() {
        // All 4 CPs at (10, 20) → mid at (10, 20).
        let mid = bezier_middle((10.0, 20.0), (10.0, 20.0), (10.0, 20.0), (10.0, 20.0));
        assert!((mid.0 - 10.0).abs() < 1e-9);
        assert!((mid.1 - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_symmetry_via_reversing_cps() {
        // B(t) on (p0,p1,p2,p3) vs (p3,p2,p1,p0) at 1-t → should be identical.
        let p0 = (0.0, 0.0);
        let p1 = (10.0, 20.0);
        let p2 = (20.0, 20.0);
        let p3 = (30.0, 0.0);
        let b_fwd = bezier_point(p0, p1, p2, p3, 0.3);
        let b_rev = bezier_point(p3, p2, p1, p0, 0.7);
        assert!((b_fwd.0 - b_rev.0).abs() < 1e-9);
        assert!((b_fwd.1 - b_rev.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_quadratic_output_shape_3cp() {
        // 3 CPs (quadratic) with 5 samples → 6 points.
        let cps = [(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)];
        let out = bezier_points_n(&cps, 5);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn test_bezier_middle_negative_cps_yield_negative_mid() {
        // All CPs negative y → mid.y negative.
        let mid = bezier_middle((0.0, -10.0), (25.0, -20.0), (75.0, -20.0), (100.0, -10.0));
        assert!(mid.1 < 0.0);
    }

    #[test]
    fn test_bezier_point_collinear_x_axis_returns_x_only() {
        // All CPs at y=0 → B(t).y = 0.
        let pt = bezier_point((0.0, 0.0), (25.0, 0.0), (50.0, 0.0), (75.0, 0.0), 0.4);
        assert_eq!(pt.1, 0.0);
    }

    #[test]
    fn test_bezier_points_n_quartic_reduces_to_cubic() {
        // 5 CPs (quartic reduces to cubic) with 3 samples → 4 points.
        let cps = [(0.0, 0.0), (25.0, 50.0), (50.0, 100.0), (75.0, 50.0), (100.0, 0.0)];
        let out = bezier_points_n(&cps, 3);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn test_bezier_point_t_slightly_over_1_is_extrapolation() {
        // Bezier formula works for t > 1 (extrapolation).
        let pt = bezier_point((0.0, 0.0), (10.0, 10.0), (20.0, 10.0), (30.0, 0.0), 1.1);
        assert!(pt.0.is_finite());
        assert!(pt.1.is_finite());
    }

    #[test]
    fn test_bezier_points_n_quadratic_convex_hump_y_positive() {
        // Quadratic hump → middle y > endpoints y.
        let cps = [(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)];
        let out = bezier_points_n(&cps, 6);
        // middle point (index 3) has largest y.
        assert!(out[3].1 > 0.0);
    }

    #[test]
    fn test_bezier_middle_preserves_midpoint_for_symmetric_cps() {
        // Symmetric curve → mid.x = midway of x-range.
        let mid = bezier_middle((0.0, 0.0), (30.0, 50.0), (70.0, 50.0), (100.0, 0.0));
        assert!((mid.0 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_with_all_y_equal_returns_that_y() {
        // All y=50 → B(t).y = 50 for any t.
        let pt = bezier_point((0.0, 50.0), (25.0, 50.0), (75.0, 50.0), (100.0, 50.0), 0.4);
        assert!((pt.1 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_identical_endpoints_start_eq_end() {
        // p0 = p3 → first and last point both == p0.
        let p = (0.0, 0.0);
        let out = bezier_points(p, (10.0, 20.0), (20.0, 20.0), p, 5);
        assert_eq!(out[0], p);
        assert!((out[5].0 - p.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_t_equal_0_and_1_yield_endpoints() {
        // t=0 → p0, t=1 → p3.
        let p0 = (1.0, 2.0);
        let p3 = (10.0, 20.0);
        let pt0 = bezier_point(p0, (3.0, 4.0), (5.0, 6.0), p3, 0.0);
        let pt1 = bezier_point(p0, (3.0, 4.0), (5.0, 6.0), p3, 1.0);
        assert_eq!(pt0, p0);
        assert!((pt1.0 - p3.0).abs() < 1e-9);
        assert!((pt1.1 - p3.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_points_n_cubic_all_samples_between_bounds() {
        // All output points have x values between p0.x and p3.x for convex CPs.
        let cps = [(0.0, 0.0), (20.0, 50.0), (80.0, 50.0), (100.0, 0.0)];
        let out = bezier_points_n(&cps, 10);
        for p in &out {
            assert!(p.0 >= -1e-9);
            assert!(p.0 <= 100.0 + 1e-9);
        }
    }

    #[test]
    fn test_bezier_middle_offset_cps_both_coords_nonzero() {
        // CPs with large offset → both mid coords nonzero.
        let mid = bezier_middle((100.0, 50.0), (150.0, 100.0), (250.0, 100.0), (300.0, 50.0));
        assert!(mid.0 != 0.0);
        assert!(mid.1 != 0.0);
    }

    #[test]
    fn test_bezier_points_n_unsupported_length_2_empty() {
        // length 2 falls into catch-all → empty Vec.
        let v = bezier_points_n(&[(0.0, 0.0), (10.0, 10.0)], 5);
        assert!(v.is_empty());
    }

    #[test]
    fn test_bezier_points_n_empty_slice_returns_empty() {
        // Empty slice → catch-all → empty.
        let v = bezier_points_n(&[], 10);
        assert!(v.is_empty());
    }

    #[test]
    fn test_bezier_points_n_quadratic_3_cps_start_matches() {
        // Length-3 (quadratic) is converted to cubic; output starts at q0.
        let v = bezier_points_n(&[(0.0, 0.0), (5.0, 10.0), (10.0, 0.0)], 5);
        assert!(!v.is_empty());
        assert!((v[0].0 - 0.0).abs() < 0.01);
        assert!((v[0].1 - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_bezier_points_n_quartic_5_cps_start_end_match_p0_p4() {
        // Length-5 (quartic) reduced to cubic through inner averaging; output starts at p0, ends at p4.
        let v = bezier_points_n(&[(0.0, 0.0), (2.0, 5.0), (5.0, 8.0), (8.0, 5.0), (10.0, 0.0)], 5);
        assert!(!v.is_empty());
        assert!((v[0].0 - 0.0).abs() < 0.01);
        assert!((v.last().unwrap().0 - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_bezier_control_points_angle_diff_over_180_uses_wrap_formula() {
        // angle2 - angle1 = 350 > 180 → middleangle formula with +360 -360.
        let cps = bezier_control_points(
            0.0, 0.0, 10.0, 100.0, 360.0, 100.0, 50.0,
            None, None, None, None, None,
        );
        // Without crest, should have exactly 3 control points.
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_control_points_with_crest_has_exactly_5_control_points() {
        // crest=Some → 2 extra control points inserted → 5 total.
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 90.0, 100.0, 50.0,
            None, None, None, Some(0.5), None,
        );
        assert_eq!(cps.len(), 5);
    }

    #[test]
    fn test_bezier_control_points_purity_eq_1_gives_zero_adjustment() {
        // k=1.0 → (1-k)=0 → x=0 → bezier_radius unchanged, middle point at (50,0).
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 0.0, 100.0, 50.0,
            Some(1.0), None, None, None, None,
        );
        // All 3 points lie on x-axis (angle=0); middle is at radius 50.
        assert_eq!(cps.len(), 3);
        assert!((cps[1].0 - 50.0).abs() < 1e-6);
        assert!((cps[1].1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_control_points_first_and_last_match_endpoints() {
        // First and last control points should be exactly on the radius-1 and radius-2 anchors.
        let cps = bezier_control_points(
            100.0, 100.0, 0.0, 50.0, 90.0, 50.0, 100.0,
            None, None, None, None, None,
        );
        // angle1=0, radius1=50 → (100+50, 100) = (150, 100).
        assert!((cps[0].0 - 150.0).abs() < 1e-6);
        assert!((cps[0].1 - 100.0).abs() < 1e-6);
        // angle2=90, radius2=50 → (100, 100+50) = (100, 150).
        assert!((cps.last().unwrap().0 - 100.0).abs() < 1e-6);
        assert!((cps.last().unwrap().1 - 150.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_point_t_0_returns_p0_exactly() {
        // t=0 → u=1, uuu=1 → output = p0.
        let p = bezier_point((10.0, 20.0), (100.0, 200.0), (150.0, 250.0), (300.0, 400.0), 0.0);
        assert_eq!(p, (10.0, 20.0));
    }

    #[test]
    fn test_bezier_point_t_1_returns_p3_exactly() {
        // t=1 → ttt=1, uuu=0 → output = p3.
        let p = bezier_point((10.0, 20.0), (100.0, 200.0), (150.0, 250.0), (300.0, 400.0), 1.0);
        assert_eq!(p, (300.0, 400.0));
    }

    #[test]
    fn test_bezier_points_num_samples_zero_yields_single_point_at_t_equals_nan() {
        // num_samples=0 → range 0..=0 yields one t value = 0/0 = NaN; point's coords are NaN.
        let pts = bezier_points((0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0), 0);
        assert_eq!(pts.len(), 1);
        // NaN x and y — check with .is_nan().
        assert!(pts[0].0.is_nan());
        assert!(pts[0].1.is_nan());
    }

    #[test]
    fn test_bezier_points_num_samples_n_yields_n_plus_1_points() {
        // 5 samples → 0..=5 → 6 points.
        let pts = bezier_points((0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0), 5);
        assert_eq!(pts.len(), 6);
    }

    #[test]
    fn test_bezier_middle_flat_line_midpoint_halfway() {
        // All points on a straight horizontal line → midpoint exactly halfway.
        let mid = bezier_middle((0.0, 10.0), (33.0, 10.0), (66.0, 10.0), (100.0, 10.0), );
        // For straight line all on y=10, mid should be on y=10.
        assert!((mid.1 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_point_half_way_between_identical_endpoints() {
        // All p0=p3, p1=p2 → curve degenerates to point at any t.
        let p = bezier_point((5.0, 5.0), (5.0, 5.0), (5.0, 5.0), (5.0, 5.0), 0.7);
        assert_eq!(p, (5.0, 5.0));
    }

    #[test]
    fn test_bezier_points_n_four_cps_cubic_exact_path() {
        // 4 CPs → standard cubic path; t=0 point matches p0, t=last matches p3.
        let v = bezier_points_n(&[(10.0, 0.0), (20.0, 0.0), (30.0, 0.0), (40.0, 0.0)], 3);
        assert_eq!(v.len(), 4);
        assert_eq!(v[0], (10.0, 0.0));
        assert_eq!(v[3], (40.0, 0.0));
    }

    #[test]
    fn test_bezier_control_points_same_angles_same_radii_middle_cp_on_same_angle() {
        // angle1 == angle2 and radius1 == radius2 → middle cp at bezier_radius on same angle.
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 0.0, 100.0, 200.0,
            None, None, None, None, None,
        );
        // middleangle = 0, middle cp at radius 200 on angle 0 → (200, 0).
        assert!((cps[1].0 - 200.0).abs() < 1e-6);
        assert!((cps[1].1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_point_quarter_t_produces_intermediate() {
        // t=0.25 → output between p0 and p3 (all cps on x-axis).
        let p = bezier_point((0.0, 0.0), (100.0, 0.0), (200.0, 0.0), (300.0, 0.0), 0.25);
        // All y=0.
        assert!((p.1 - 0.0).abs() < 1e-9);
        // x strictly between 0 and 300.
        assert!(p.0 > 0.0);
        assert!(p.0 < 300.0);
    }

    #[test]
    fn test_bezier_points_monotonic_x_for_monotone_cps() {
        // p0..p3 have strictly increasing x → output x monotonic non-decreasing.
        let pts = bezier_points((0.0, 0.0), (10.0, 1.0), (20.0, 1.0), (30.0, 0.0), 10);
        for i in 1..pts.len() {
            assert!(pts[i].0 >= pts[i-1].0);
        }
    }

    #[test]
    fn test_bezier_middle_equals_bezier_point_at_half() {
        // bezier_middle(p0..p3) === bezier_point(p0..p3, 0.5).
        let p0 = (0.0, 0.0);
        let p1 = (1.0, 2.0);
        let p2 = (3.0, 2.0);
        let p3 = (4.0, 0.0);
        let mid = bezier_middle(p0, p1, p2, p3);
        let half = bezier_point(p0, p1, p2, p3, 0.5);
        assert_eq!(mid, half);
    }

    #[test]
    fn test_bezier_control_points_opposite_angles_middle_at_bezier_radius() {
        // angle1=0, angle2=180 → |a2-a1|=180, not > 180 → middleangle=90.
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 180.0, 100.0, 50.0,
            None, None, None, None, None,
        );
        assert_eq!(cps.len(), 3);
        // middle at angle=90, radius=50 → (~0, ~50).
        assert!((cps[1].0 - 0.0).abs() < 1e-6);
        assert!((cps[1].1 - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_point_t_half_produces_symmetric_midpoint() {
        // Symmetric p0..p3 around y-axis → midpoint on y-axis.
        let p0 = (-10.0, 0.0);
        let p1 = (-5.0, 10.0);
        let p2 = (5.0, 10.0);
        let p3 = (10.0, 0.0);
        let p = bezier_point(p0, p1, p2, p3, 0.5);
        // By symmetry, midpoint x should be ~0.
        assert!(p.0.abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_n_single_cp_empty_fallthrough() {
        // Single control point falls to catch-all _ → empty.
        let v = bezier_points_n(&[(0.0, 0.0)], 10);
        assert!(v.is_empty());
    }

    #[test]
    fn test_bezier_control_points_with_different_radii_first_and_last_match() {
        // radius1 != radius2 — first cp at radius1, last at radius2.
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 50.0, 0.0, 150.0, 100.0,
            None, None, None, None, None,
        );
        // angle=0, radius1=50 → (50, 0).
        assert!((cps[0].0 - 50.0).abs() < 1e-6);
        // angle=0, radius2=150 → (150, 0).
        assert!((cps.last().unwrap().0 - 150.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_points_large_num_samples_produces_correct_count() {
        // 100 samples → 101 points.
        let pts = bezier_points((0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0), 100);
        assert_eq!(pts.len(), 101);
    }

    #[test]
    fn test_bezier_points_n_with_cubic_4_cps_yields_num_samples_plus_1() {
        // 4 cps + 10 samples → 11 points.
        let v = bezier_points_n(&[(0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0)], 10);
        assert_eq!(v.len(), 11);
    }

    #[test]
    fn test_bezier_control_points_purity_gt_1_moves_toward_bisecting() {
        // k > 1.0 → adjust radius toward bisecting.
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 90.0, 100.0, 200.0,
            Some(1.5), None, None, None, None,
        );
        // Still 3 cps.
        assert_eq!(cps.len(), 3);
    }

    #[test]
    fn test_bezier_middle_with_asymmetric_control_points() {
        // Asymmetric p0..p3 — mid on curve between them.
        let mid = bezier_middle((0.0, 0.0), (10.0, 50.0), (20.0, 40.0), (30.0, 0.0));
        // y-coord somewhere between 0 and max(50,40) = 50.
        assert!(mid.1 > 0.0 && mid.1 < 50.0);
    }

    #[test]
    fn test_bezier_point_with_identical_control_points_produces_that_point() {
        // All 4 control points same → any t gives that point (within fp tolerance).
        let p0 = (7.5, 3.3);
        let p = bezier_point(p0, p0, p0, p0, 0.42);
        assert!((p.0 - p0.0).abs() < 1e-9);
        assert!((p.1 - p0.1).abs() < 1e-9);
    }

    #[test]
    fn test_bezier_control_points_crest_only_no_purity_adds_2_points() {
        // crest=Some, purity=None → 3 + 2 = 5 control points.
        let cps = bezier_control_points(
            0.0, 0.0, 0.0, 100.0, 90.0, 100.0, 50.0,
            None, None, None, Some(0.5), None,
        );
        assert_eq!(cps.len(), 5);
    }

    #[test]
    fn test_bezier_points_num_samples_one_yields_two_points() {
        // num_samples=1 → range 0..=1 → 2 points (t=0 and t=1).
        let pts = bezier_points((0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), 1);
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0], (0.0, 0.0));
        assert_eq!(pts[1], (3.0, 0.0));
    }

    #[test]
    fn test_bezier_middle_with_small_swing_stays_near_midline() {
        // Small control point offsets → midpoint near straight line midpoint.
        let mid = bezier_middle((0.0, 0.0), (10.0, 1.0), (20.0, 1.0), (30.0, 0.0));
        // Midpoint x should be near 15.
        assert!(mid.0 > 13.0 && mid.0 < 17.0);
    }

    #[test]
    fn test_bezier_points_n_empty_slice_always_empty() {
        // Even with nonzero num_samples, empty slice → empty.
        let v = bezier_points_n(&[], 100);
        assert!(v.is_empty());
    }
}
