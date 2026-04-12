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

/// Compute control points for a Circos-style link bezier curve.
///
/// Given two endpoints on the circle (start and end angles + radii),
/// compute control points that create a smooth curve through the center.
///
/// `crest` controls how far the bezier "crest" extends toward the center.
/// A crest of 1.0 means the curve passes through the center.
/// A crest of 0.5 means the curve only goes halfway to center.
pub fn link_control_points(
    cx: f64,
    cy: f64,
    angle1: f64,
    radius1: f64,
    angle2: f64,
    radius2: f64,
    crest: f64,
    bezier_radius: f64,
) -> ((f64, f64), (f64, f64), (f64, f64), (f64, f64)) {
    let deg2rad = std::f64::consts::PI / 180.0;

    // Start and end points on the circle
    let p0 = (
        cx + radius1 * (angle1 * deg2rad).cos(),
        cy + radius1 * (angle1 * deg2rad).sin(),
    );
    let p3 = (
        cx + radius2 * (angle2 * deg2rad).cos(),
        cy + radius2 * (angle2 * deg2rad).sin(),
    );

    // Control points: directed toward center, at bezier_radius * crest
    let cr1 = bezier_radius * crest;
    let cr2 = bezier_radius * crest;

    let p1 = (
        cx + cr1 * (angle1 * deg2rad).cos(),
        cy + cr1 * (angle1 * deg2rad).sin(),
    );
    let p2 = (
        cx + cr2 * (angle2 * deg2rad).cos(),
        cy + cr2 * (angle2 * deg2rad).sin(),
    );

    (p0, p1, p2, p3)
}

/// Get the midpoint of a bezier curve (at t=0.5).
pub fn bezier_midpoint(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
) -> (f64, f64) {
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
    fn test_bezier_midpoint() {
        // Symmetric bezier
        let mid = bezier_midpoint((0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0));
        assert!((mid.0 - 2.0).abs() < 1e-10); // Should be at x=2 by symmetry
        assert!(mid.1 > 0.0); // Should be above the baseline
    }

    #[test]
    fn test_link_control_points() {
        let (p0, _p1, _p2, p3) =
            link_control_points(100.0, 100.0, 0.0, 50.0, 180.0, 50.0, 1.0, 30.0);

        // p0 should be at (150, 100) (0 degrees, radius 50)
        assert!((p0.0 - 150.0).abs() < 0.1);
        assert!((p0.1 - 100.0).abs() < 0.1);

        // p3 should be at (50, 100) (180 degrees, radius 50)
        assert!((p3.0 - 50.0).abs() < 0.1);
        assert!((p3.1 - 100.0).abs() < 0.1);
    }
}
