pub mod bezier;

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
        if (step > 0.0 && angle >= angle_end) || (step < 0.0 && angle <= angle_end) {
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
pub fn svg_arc_path(
    cx: f64,
    cy: f64,
    radius: f64,
    angle_start: f64,
    angle_end: f64,
) -> String {
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
}
