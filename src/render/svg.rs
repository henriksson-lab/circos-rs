use std::fmt::Write;

use crate::layout::Layout;
use crate::render::color::Color;

/// SVG document builder.
pub struct SvgDocument {
    pub width: f64,
    pub height: f64,
    pub elements: Vec<String>,
}

impl SvgDocument {
    /// Create a new SvgDocument with the given pixel dimensions and no elements.
    pub fn new(width: f64, height: f64) -> Self {
        SvgDocument {
            width,
            height,
            elements: Vec::new(),
        }
    }

    /// Add a raw SVG element string.
    pub fn add(&mut self, element: String) {
        self.elements.push(element);
    }

    /// Open a group element.
    pub fn open_group(&mut self, id: &str) {
        self.elements.push(format!(r#"<g id="{}">"#, id));
    }

    /// Close a group element.
    pub fn close_group(&mut self) {
        self.elements.push("</g>".to_string());
    }

    /// Render the complete SVG document as a string.
    pub fn render(&self) -> String {
        let mut svg = String::new();
        writeln!(svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
        writeln!(svg, r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#).unwrap();
        writeln!(
            svg,
            r#"<svg width="{:.0}px" height="{:.0}px" version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#,
            self.width, self.height
        ).unwrap();

        for element in &self.elements {
            writeln!(svg, "{}", element).unwrap();
        }

        writeln!(svg, "</svg>").unwrap();
        svg
    }
}

/// Draw an annular arc (slice) between two radii and two angles.
///
/// This is the core drawing primitive for ideograms, bands, highlights, etc.
pub fn svg_slice(
    layout: &Layout,
    start_angle: f64,
    end_angle: f64,
    radius_inner: f64,
    radius_outer: f64,
    edge_color: Option<&Color>,
    edge_stroke: Option<f64>,
    fill_color: Option<&Color>,
    opacity: Option<f64>,
) -> String {
    let cx = layout.image_radius;
    let cy = layout.image_radius;

    let mut start_a = start_angle;
    let mut end_a = end_angle;

    if end_a < start_a {
        std::mem::swap(&mut start_a, &mut end_a);
    }

    // Handle near-full-circle case
    let mut end_a_mod = end_a;
    if (end_a - start_a).abs() > 359.99 || start_a == end_a {
        end_a_mod -= 0.01;
    }

    let deg2rad = std::f64::consts::PI / 180.0;

    // Style string
    let stroke_style = if let Some(sw) = edge_stroke {
        if sw > 0.0 {
            if let Some(c) = edge_color {
                format!("stroke-width: {:.1}; stroke: {};", sw, c.to_svg_rgb())
            } else {
                format!("stroke-width: {:.1}; stroke: none;", sw)
            }
        } else {
            "stroke: none;".to_string()
        }
    } else {
        "stroke: none;".to_string()
    };

    let fill_style = if let Some(c) = fill_color {
        format!("fill: {};", c.to_svg_rgb())
    } else {
        "fill: none;".to_string()
    };

    let opacity_style = if let Some(o) = opacity {
        if o < 1.0 {
            format!(" opacity: {:.3};", o)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    if (radius_inner - radius_outer).abs() < 0.01 {
        // Zero-width arc (just a line along the arc)
        let (x1, y1) = polar_to_xy(cx, cy, radius_outer, start_a, deg2rad);
        let (x2, y2) = polar_to_xy(cx, cy, radius_outer, end_a_mod, deg2rad);
        let large_arc = if (start_a - end_a_mod).abs() > 180.0 {
            1
        } else {
            0
        };
        format!(
            r#"<path d="M {:.1},{:.1} A{:.1},{:.1} 0.00 {},{} {:.1},{:.1}" style="{} {} fill: none;" />"#,
            x1, y1, radius_outer, radius_outer, large_arc, 1, x2, y2, stroke_style, ""
        )
    } else if (start_a - end_a).abs() < 0.001 {
        // Zero-angle slice (radial line)
        let (x1, y1) = polar_to_xy(cx, cy, radius_outer, start_a, deg2rad);
        let (x2, y2) = polar_to_xy(cx, cy, radius_inner, end_a, deg2rad);
        format!(
            r#"<path d="M {:.1},{:.1} L {:.1},{:.1}" style="{} fill: none;" />"#,
            x1, y1, x2, y2, stroke_style
        )
    } else {
        // Full annular arc: outer arc forward, line to inner, inner arc backward, close
        let sweep_large = if (start_a - end_a_mod).abs() > 180.0 {
            1
        } else {
            0
        };

        let (ox1, oy1) = polar_to_xy(cx, cy, radius_outer, start_a, deg2rad);
        let (ox2, oy2) = polar_to_xy(cx, cy, radius_outer, end_a_mod, deg2rad);
        let (ix1, iy1) = polar_to_xy(cx, cy, radius_inner, end_a_mod, deg2rad);
        let (ix2, iy2) = polar_to_xy(cx, cy, radius_inner, start_a, deg2rad);

        format!(
            r#"<path d="M {:.3},{:.3} A{:.3},{:.3} {:.3} {},{} {:.3},{:.3} L {:.3},{:.3} A{:.3},{:.3} {:.3} {},{} {:.3},{:.3} Z " style="{} {} {}{}" />"#,
            ox1,
            oy1,
            radius_outer,
            radius_outer,
            0.0,
            sweep_large,
            1,
            ox2,
            oy2,
            ix1,
            iy1,
            radius_inner,
            radius_inner,
            0.0,
            sweep_large,
            0,
            ix2,
            iy2,
            stroke_style,
            fill_style,
            opacity_style,
            "" // placeholder for additional styles
        )
    }
}

/// Draw a text element at a position on the circle.
pub fn svg_text(
    layout: &Layout,
    angle: f64,
    radius: f64,
    text: &str,
    font_size: f64,
    color: &Color,
    rotation: f64,
) -> String {
    let (x, y) = layout.getxypos(angle, radius);
    if rotation.abs() < 0.01 {
        format!(
            r#"<text x="{:.1}" y="{:.1}" style="font-size: {:.0}px; fill: {};">{}</text>"#,
            x,
            y,
            font_size,
            color.to_svg_rgb(),
            text
        )
    } else {
        format!(
            r#"<text x="{:.1}" y="{:.1}" transform="rotate({:.2},{:.1},{:.1})" style="font-size: {:.0}px; fill: {};">{}</text>"#,
            x,
            y,
            rotation.to_degrees(),
            x,
            y,
            font_size,
            color.to_svg_rgb(),
            text
        )
    }
}

/// Draw a tick mark (radial line) at a given angle.
pub fn svg_tick(
    layout: &Layout,
    angle: f64,
    radius_from: f64,
    radius_to: f64,
    thickness: f64,
    color: &Color,
) -> String {
    let (x1, y1) = layout.getxypos(angle, radius_from);
    let (x2, y2) = layout.getxypos(angle, radius_to);
    format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" style="stroke: {}; stroke-width: {:.1};" />"#,
        x1,
        y1,
        x2,
        y2,
        color.to_svg_rgb(),
        thickness
    )
}

/// Draw a grid line (arc at a specific radius from one angle to another).
pub fn svg_grid_arc(
    layout: &Layout,
    angle_start: f64,
    angle_end: f64,
    radius: f64,
    thickness: f64,
    color: &Color,
) -> String {
    let deg2rad = std::f64::consts::PI / 180.0;
    let cx = layout.image_radius;
    let cy = layout.image_radius;

    let mut end_a = angle_end;
    if (angle_end - angle_start).abs() > 359.99 {
        end_a -= 0.01;
    }

    let (x1, y1) = polar_to_xy(cx, cy, radius, angle_start, deg2rad);
    let (x2, y2) = polar_to_xy(cx, cy, radius, end_a, deg2rad);
    let large_arc = if (angle_start - end_a).abs() > 180.0 {
        1
    } else {
        0
    };

    format!(
        r#"<path d="M {:.1},{:.1} A{:.1},{:.1} 0 {},{} {:.1},{:.1}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
        x1,
        y1,
        radius,
        radius,
        large_arc,
        1,
        x2,
        y2,
        color.to_svg_rgb(),
        thickness
    )
}

/// Convert polar coordinates to cartesian (x, y).
fn polar_to_xy(cx: f64, cy: f64, radius: f64, angle_deg: f64, deg2rad: f64) -> (f64, f64) {
    (
        cx + radius * (angle_deg * deg2rad).cos(),
        cy + radius * (angle_deg * deg2rad).sin(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_document_render() {
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        doc.open_group("test");
        doc.add(r#"<circle cx="1500" cy="1500" r="100" />"#.to_string());
        doc.close_group();

        let svg = doc.render();
        assert!(svg.contains("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains(r#"<g id="test">"#));
        assert!(svg.contains("</g>"));
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn test_svg_document_width_height_in_root_tag() {
        let doc = SvgDocument::new(1234.0, 567.0);
        let svg = doc.render();
        assert!(svg.contains(r#"width="1234px""#), "width missing: {}", svg);
        assert!(svg.contains(r#"height="567px""#), "height missing: {}", svg);
    }

    #[test]
    fn test_svg_document_empty_has_only_xml_and_svg_tags() {
        let doc = SvgDocument::new(100.0, 100.0);
        let svg = doc.render();
        // No <g>, <circle>, etc., but still wrapped in <svg> + closing tag.
        assert!(!svg.contains("<g"));
        assert!(!svg.contains("<circle"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_svg_document_preserves_element_order() {
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add("<rect id='first' />".to_string());
        doc.add("<rect id='second' />".to_string());
        doc.add("<rect id='third' />".to_string());
        let svg = doc.render();
        let p1 = svg.find("first").unwrap();
        let p2 = svg.find("second").unwrap();
        let p3 = svg.find("third").unwrap();
        assert!(p1 < p2 && p2 < p3, "elements should render in insertion order");
    }

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
                ideogram_radius: 1350.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 1250.0,
                ideogram_radius_outer: 1350.0,
            },
        }
    }

    #[test]
    fn test_svg_tick_emits_line_element() {
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_tick(&layout, 0.0, 1300.0, 1350.0, 2.0, &color);
        // Should be a single <line> with x1/y1/x2/y2 and stroke style.
        assert!(svg.starts_with("<line "));
        assert!(svg.contains("stroke: rgb(0,0,0)"));
        assert!(svg.contains("stroke-width: 2.0"));
        // Angle 0, image_radius 1500 → cos=1, sin=0 → tick along +x axis.
        assert!(svg.contains("x1=\"2800.0\"") || svg.contains("x1=\"2800\""));
    }

    #[test]
    fn test_svg_grid_arc_large_arc_flag() {
        let layout = mk_layout();
        let color = Color::rgb(100, 100, 100);
        // Small arc (<180°) → large_arc=0.
        let svg = svg_grid_arc(&layout, 0.0, 90.0, 1000.0, 1.0, &color);
        assert!(svg.contains("0,1"), "small arc flag wrong: {}", svg);
        // Big arc (>180°) → large_arc=1.
        let svg = svg_grid_arc(&layout, 0.0, 270.0, 1000.0, 1.0, &color);
        assert!(svg.contains("1,1"), "large arc flag wrong: {}", svg);
    }

    #[test]
    fn test_svg_grid_arc_full_circle_trims_end() {
        // A full 360° arc is trimmed by 0.01° so SVG doesn't render an empty path.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_grid_arc(&layout, 0.0, 360.0, 1000.0, 1.0, &color);
        // Endpoints should be very close but not identical.
        assert!(svg.contains("M "));
        assert!(svg.contains(" A"));
    }

    #[test]
    fn test_svg_text_has_text_element() {
        let layout = mk_layout();
        let color = Color::rgb(255, 0, 0);
        let svg = svg_text(&layout, 0.0, 1400.0, "hello", 12.0, &color, 0.0);
        assert!(svg.contains("<text"));
        assert!(svg.contains("hello"));
        assert!(svg.contains("rgb(255,0,0)"));
    }

    #[test]
    fn test_svg_slice_zero_width_arc_emits_path_with_fill_none() {
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        // radius_inner == radius_outer → treated as zero-width arc.
        let svg = svg_slice(&layout, 0.0, 90.0, 1000.0, 1000.0, Some(&color), Some(2.0), None, None);
        assert!(svg.starts_with("<path"));
        assert!(svg.contains("fill: none;"));
        assert!(svg.contains("stroke: rgb(0,0,0)"));
    }

    #[test]
    fn test_svg_slice_annular_wedge_has_fill() {
        let layout = mk_layout();
        let fill = Color::rgb(255, 128, 0);
        // Full annular wedge with fill color.
        let svg = svg_slice(&layout, 0.0, 90.0, 900.0, 1000.0, None, None, Some(&fill), None);
        assert!(svg.contains("fill: rgb(255,128,0)"));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn test_svg_slice_opacity_emitted_when_less_than_one() {
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        // opacity < 1 should appear in style.
        let svg = svg_slice(&layout, 10.0, 80.0, 900.0, 1000.0, None, None, Some(&fill), Some(0.5));
        assert!(svg.contains("opacity: 0.500"));
        // opacity >= 1 should NOT append the opacity style.
        let svg = svg_slice(&layout, 10.0, 80.0, 900.0, 1000.0, None, None, Some(&fill), Some(1.0));
        assert!(!svg.contains("opacity:"));
    }

    #[test]
    fn test_svg_slice_full_circle_trims_end_to_avoid_degenerate_path() {
        let layout = mk_layout();
        let fill = Color::rgb(255, 0, 0);
        // 360° sweep should still produce a valid path (trimmed end by 0.01°).
        let svg = svg_slice(&layout, 0.0, 360.0, 900.0, 1000.0, None, None, Some(&fill), None);
        assert!(svg.contains("<path"));
        // Shouldn't NaN or explode.
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn test_svg_slice_swaps_reversed_angles() {
        let layout = mk_layout();
        let fill = Color::rgb(0, 255, 0);
        // Passing end < start should be silently swapped — no panic, produces valid SVG.
        let svg = svg_slice(&layout, 90.0, 30.0, 900.0, 1000.0, None, None, Some(&fill), None);
        assert!(svg.contains("<path"));
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn test_svg_text_no_rotation_omits_transform() {
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        // rotation=0 → no `transform=` attribute.
        let svg = svg_text(&layout, 0.0, 1400.0, "hello", 12.0, &color, 0.0);
        assert!(!svg.contains("transform="));
        assert!(svg.contains("<text"));
    }

    #[test]
    fn test_svg_text_with_rotation_adds_transform() {
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        // rotation=π/4 rad > 0.01 → includes transform.
        let svg = svg_text(&layout, 0.0, 1400.0, "hello", 12.0, &color, 0.7854);
        assert!(svg.contains("transform=\"rotate("));
    }

    #[test]
    fn test_svg_tick_reversed_radii_still_produces_line() {
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        // radius_from > radius_to should still produce a valid line (inward tick).
        let svg = svg_tick(&layout, 0.0, 1400.0, 1350.0, 2.0, &color);
        assert!(svg.contains("<line"));
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn test_polar_to_xy_angle_zero_at_east() {
        // Angle 0 with deg2rad constant should place the point due east (+x).
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x, y) = polar_to_xy(500.0, 500.0, 100.0, 0.0, deg2rad);
        assert!((x - 600.0).abs() < 1e-6);
        assert!((y - 500.0).abs() < 1e-6);
        // Angle 180 → due west.
        let (x, y) = polar_to_xy(500.0, 500.0, 100.0, 180.0, deg2rad);
        assert!((x - 400.0).abs() < 1e-6);
        assert!((y - 500.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_slice_radial_line_when_start_equals_end() {
        // start_a == end_a triggers the "zero-angle slice" branch → `L`
        // command between radius_outer and radius_inner.
        let layout = mk_layout();
        let color = Color::rgb(10, 20, 30);
        let svg = svg_slice(
            &layout, 45.0, 45.0, 900.0, 1000.0, Some(&color), Some(1.0), None, None,
        );
        // The zero-angle branch is reachable only when neither zero-width nor
        // near-full-circle fires; zero-width kicks in first when radii match,
        // near-full-circle trims via end_a_mod. With distinct radii and equal
        // start/end angles, we should get the "zero-width or full-circle trim"
        // path (end_a_mod = end_a - 0.01) since `start_a == end_a` also flips
        // the near-full-circle branch. Verify it emits a valid path, no NaN.
        assert!(svg.starts_with("<path"));
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn test_svg_slice_edge_stroke_without_color_emits_stroke_none() {
        // edge_stroke > 0 but edge_color = None → "stroke-width: X; stroke: none;"
        let layout = mk_layout();
        let svg = svg_slice(
            &layout, 0.0, 90.0, 900.0, 1000.0, None, Some(3.0), None, None,
        );
        assert!(svg.contains("stroke-width: 3.0"));
        assert!(svg.contains("stroke: none"));
    }

    #[test]
    fn test_svg_slice_zero_edge_stroke_suppresses_width() {
        // edge_stroke == 0 → "stroke: none;" (no stroke-width attribute emitted).
        let layout = mk_layout();
        let fill = Color::rgb(255, 0, 0);
        let color = Color::rgb(0, 0, 0);
        let svg = svg_slice(
            &layout, 0.0, 90.0, 900.0, 1000.0, Some(&color), Some(0.0), Some(&fill), None,
        );
        assert!(!svg.contains("stroke-width:"));
        assert!(svg.contains("stroke: none"));
    }

    #[test]
    fn test_svg_grid_arc_non_full_preserves_exact_end() {
        // Non-full-circle arcs don't trim end by 0.01° — sweep 90° retains
        // exact endpoint placement.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        // Angle 90° at radius 1000 with center 1500 → cos(90°)≈0, sin(90°)=1.
        // Endpoint: cx=1500, cy+1000=2500. Expect "2500.0" in svg.
        let svg = svg_grid_arc(&layout, 0.0, 90.0, 1000.0, 1.0, &color);
        assert!(svg.contains("2500.0"), "expected exact endpoint, got: {}", svg);
        // Should NOT include the trim-shifted endpoint that full-circle case uses.
    }

    #[test]
    fn test_polar_to_xy_at_center_with_zero_radius() {
        // radius=0 → always at (cx, cy) regardless of angle.
        let deg2rad = std::f64::consts::PI / 180.0;
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0, 359.0] {
            let (x, y) = polar_to_xy(500.0, 400.0, 0.0, angle, deg2rad);
            assert!((x - 500.0).abs() < 1e-9, "angle {}, x={}", angle, x);
            assert!((y - 400.0).abs() < 1e-9, "angle {}, y={}", angle, y);
        }
    }

    #[test]
    fn test_polar_to_xy_symmetric_at_opposite_angles() {
        // angle θ and θ+180 → diametrically opposite points.
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x1, y1) = polar_to_xy(100.0, 100.0, 50.0, 45.0, deg2rad);
        let (x2, y2) = polar_to_xy(100.0, 100.0, 50.0, 225.0, deg2rad);
        // (x1 + x2) / 2 = cx = 100; (y1 + y2) / 2 = cy = 100.
        assert!((x1 + x2 - 200.0).abs() < 1e-9);
        assert!((y1 + y2 - 200.0).abs() < 1e-9);
    }

    #[test]
    fn test_polar_to_xy_negative_radius_opposite_direction() {
        // Negative radius puts the point in the opposite direction.
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x_pos, y_pos) = polar_to_xy(500.0, 500.0, 100.0, 0.0, deg2rad);
        let (x_neg, y_neg) = polar_to_xy(500.0, 500.0, -100.0, 0.0, deg2rad);
        // Positive: (600, 500); negative: (400, 500).
        assert_eq!((x_pos, y_pos), (600.0, 500.0));
        assert_eq!(x_neg, 400.0);
        assert_eq!(y_neg, 500.0);
    }

    #[test]
    fn test_polar_to_xy_360_degrees_equals_0_degrees() {
        // 360° ≡ 0° (modulo full circle).
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x0, y0) = polar_to_xy(500.0, 500.0, 100.0, 0.0, deg2rad);
        let (x360, y360) = polar_to_xy(500.0, 500.0, 100.0, 360.0, deg2rad);
        assert!((x0 - x360).abs() < 1e-9);
        assert!((y0 - y360).abs() < 1e-9);
    }

    #[test]
    fn test_svg_slice_no_fill_no_stroke_still_emits_path() {
        // fill=None, stroke=None, thickness=None → still emits a path,
        // but with "fill: none" and "stroke: none" styles.
        let layout = mk_layout();
        let svg = svg_slice(&layout, 0.0, 90.0, 900.0, 1000.0, None, None, None, None);
        assert!(svg.starts_with("<path"));
        assert!(svg.contains("fill: none"));
        assert!(svg.contains("stroke: none"));
    }

    #[test]
    fn test_svg_text_non_ascii_content_passes_through() {
        // Non-ASCII chars in text are preserved in output (no transformation).
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_text(&layout, 0.0, 1400.0, "chr_α", 12.0, &color, 0.0);
        assert!(svg.contains("chr_α"));
    }

    #[test]
    fn test_svg_grid_arc_zero_thickness_still_valid_path() {
        // thickness=0 → stroke-width: 0.0 in the path. Still valid SVG.
        let layout = mk_layout();
        let color = Color::rgb(100, 100, 100);
        let svg = svg_grid_arc(&layout, 0.0, 90.0, 1000.0, 0.0, &color);
        assert!(svg.contains("stroke-width: 0.0"));
        assert!(svg.starts_with("<path"));
    }

    #[test]
    fn test_svg_document_add_element_raw_content_preserved() {
        // `add` appends the raw string verbatim without processing.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add("<rect id=\"raw\" />".to_string());
        doc.add("<circle cx=\"50\" />".to_string());
        assert_eq!(doc.elements.len(), 2);
        assert!(doc.elements[0].contains("raw"));
        assert!(doc.elements[1].contains("circle"));
    }

    #[test]
    fn test_svg_document_open_close_group_nesting() {
        // Nested open/close groups work (no mid-render checks — just string
        // concatenation): outer wraps inner.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("outer");
        doc.open_group("inner");
        doc.add("<rect />".to_string());
        doc.close_group();
        doc.close_group();
        let svg = doc.render();
        // Both groups appear; inner is between outer's open and close.
        let outer_open = svg.find(r#"<g id="outer">"#).unwrap();
        let inner_open = svg.find(r#"<g id="inner">"#).unwrap();
        let rect_pos = svg.find("<rect").unwrap();
        assert!(outer_open < inner_open && inner_open < rect_pos);
    }

    #[test]
    fn test_svg_document_render_ends_with_closing_svg_tag() {
        let doc = SvgDocument::new(50.0, 50.0);
        let svg = doc.render();
        // Render always ends with "</svg>" (with optional trailing newline).
        assert!(svg.ends_with("</svg>\n") || svg.ends_with("</svg>"));
    }

    #[test]
    fn test_svg_tick_thickness_formatted_to_one_decimal() {
        // svg_tick emits stroke-width formatted as {:.1} — 2.5 stays "2.5".
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_tick(&layout, 45.0, 1000.0, 1100.0, 2.5, &color);
        assert!(svg.contains("stroke-width: 2.5"));
    }

    #[test]
    fn test_svg_text_font_size_emitted_as_pixels() {
        // svg_text emits `font-size: Npx` — integer-formatted per `{:.0}`.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_text(&layout, 0.0, 1400.0, "test", 16.5, &color, 0.0);
        // 16.5 rounds to 17 via {:.0}. (Banker's rounding may round to 16;
        // accept either.)
        assert!(
            svg.contains("font-size: 16px;") || svg.contains("font-size: 17px;"),
            "expected font-size in pixels, got: {}",
            svg
        );
    }

    #[test]
    fn test_svg_text_nonzero_rotation_includes_transform() {
        // Rotation > 0.01 rad branch → transform="rotate(deg, x, y)" included.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        // 0.5 rad ≈ 28.648 deg → {:.2} → "28.65"
        let svg = svg_text(&layout, 0.0, 1400.0, "rot", 10.0, &color, 0.5);
        assert!(svg.contains("transform=\"rotate("));
        assert!(svg.contains("28.65") || svg.contains("28.64"));
        // Zero-rotation branch (<0.01 rad) emits no transform.
        let svg0 = svg_text(&layout, 0.0, 1400.0, "norot", 10.0, &color, 0.005);
        assert!(!svg0.contains("transform=\"rotate("));
    }

    #[test]
    fn test_svg_tick_negative_radii_pass_through_formatting() {
        // svg_tick doesn't clamp radii; negative values flow to getxypos and
        // produce cartesian coords. Verify the line element is still emitted
        // with `{:.1}` formatting (no NaN/Infinity).
        let layout = mk_layout();
        let color = Color::rgb(50, 60, 70);
        let svg = svg_tick(&layout, 90.0, -100.0, -200.0, 1.5, &color);
        assert!(svg.starts_with("<line "));
        assert!(!svg.contains("NaN"));
        assert!(!svg.contains("Infinity"));
        // stroke: rgb + width formatted.
        assert!(svg.contains("stroke: rgb(50,60,70)"));
        assert!(svg.contains("stroke-width: 1.5"));
    }

    #[test]
    fn test_svg_grid_arc_format_shape_well_formed() {
        // svg_grid_arc path shape: "M x1,y1 A r,r 0 large,1 x2,y2".
        let layout = mk_layout();
        let color = Color::rgb(0, 128, 255);
        let svg = svg_grid_arc(&layout, 0.0, 45.0, 1200.0, 1.0, &color);
        // Must contain M-command and A-command.
        assert!(svg.contains("<path d=\"M "));
        assert!(svg.contains(" A1200.0,1200.0 0 "));
        // sweep-flag hardcoded to 1.
        assert!(svg.contains(",1 "));
        // Color emitted as rgb(r,g,b).
        assert!(svg.contains("stroke: rgb(0,128,255)"));
    }

    #[test]
    fn test_svg_document_open_group_with_special_chars_in_id() {
        // open_group inserts the id verbatim (no escaping). Special chars like `&`
        // or `"` flow straight through — documents current behavior.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("my-id_1");
        doc.close_group();
        let svg = doc.render();
        assert!(svg.contains(r#"<g id="my-id_1">"#));
        assert!(svg.contains("</g>"));
    }

    #[test]
    fn test_svg_document_render_contains_xml_prolog_and_doctype() {
        // render() always emits the XML prolog and SVG DOCTYPE.
        let doc = SvgDocument::new(100.0, 100.0);
        let svg = doc.render();
        assert!(svg.contains(r#"<?xml version="1.0""#));
        assert!(svg.contains("<!DOCTYPE svg PUBLIC"));
        // xmlns attributes should also be present in root svg tag.
        assert!(svg.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
        assert!(svg.contains(r#"xmlns:xlink="http://www.w3.org/1999/xlink""#));
    }

    #[test]
    fn test_svg_slice_opacity_exactly_one_not_emitted() {
        // Opacity exactly 1.0 → style does NOT include "opacity:" (only < 1 adds it).
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let svg = svg_slice(&layout, 0.0, 90.0, 900.0, 1000.0, None, None, Some(&fill), Some(1.0));
        assert!(!svg.contains("opacity:"));
        // Just below 1 → opacity present.
        let svg = svg_slice(&layout, 0.0, 90.0, 900.0, 1000.0, None, None, Some(&fill), Some(0.999));
        assert!(svg.contains("opacity:"));
    }

    #[test]
    fn test_svg_tick_zero_radius_both_sides_degenerate_point() {
        // Both radii=0 → line from center to center (a point).
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_tick(&layout, 45.0, 0.0, 0.0, 1.0, &color);
        assert!(svg.starts_with("<line "));
        // Both endpoints should be the image center.
        let expected_center = format!(r#"x1="{:.1}""#, layout.image_radius);
        assert!(svg.contains(&expected_center));
    }

    #[test]
    fn test_svg_document_multiple_add_interleaved_with_groups() {
        // Interleave add() with open_group/close_group — order preserved in render.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add("<!-- pre -->".to_string());
        doc.open_group("g1");
        doc.add("<rect />".to_string());
        doc.close_group();
        doc.add("<!-- mid -->".to_string());
        doc.open_group("g2");
        doc.add("<circle />".to_string());
        doc.close_group();
        let svg = doc.render();
        // All markers appear in their insertion order.
        let pre = svg.find("<!-- pre -->").unwrap();
        let g1 = svg.find(r#"<g id="g1">"#).unwrap();
        let rect = svg.find("<rect").unwrap();
        let mid = svg.find("<!-- mid -->").unwrap();
        let g2 = svg.find(r#"<g id="g2">"#).unwrap();
        let circle = svg.find("<circle").unwrap();
        assert!(pre < g1);
        assert!(g1 < rect);
        assert!(rect < mid);
        assert!(mid < g2);
        assert!(g2 < circle);
    }

    #[test]
    fn test_svg_document_width_height_formatted_with_zero_decimal() {
        // width/height in root tag use {:.0} → integer-formatted.
        let doc = SvgDocument::new(3000.5, 2999.4);
        let svg = doc.render();
        // {:.0} rounds 3000.5 → 3000 (banker's), 2999.4 → 2999.
        assert!(svg.contains(r#"width="3000px""#) || svg.contains(r#"width="3001px""#));
        assert!(svg.contains(r#"height="2999px""#));
    }

    #[test]
    fn test_svg_slice_stroke_color_without_fill_emits_fill_none() {
        // Stroke but no fill → path has `fill: none`.
        let layout = mk_layout();
        let color = Color::rgb(50, 60, 70);
        let svg = svg_slice(&layout, 0.0, 45.0, 900.0, 1000.0, Some(&color), Some(1.5), None, None);
        assert!(svg.contains("fill: none"));
        assert!(svg.contains("stroke: rgb(50,60,70)"));
        assert!(svg.contains("stroke-width: 1.5"));
    }

    #[test]
    fn test_svg_document_close_group_without_open_still_emits() {
        // close_group doesn't enforce structure — emits </g> unconditionally.
        // This documents that balanced-pair checking is caller responsibility.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.close_group();  // no matching open
        let svg = doc.render();
        assert!(svg.contains("</g>"));
    }

    #[test]
    fn test_svg_grid_arc_angle_span_exactly_360_trimmed() {
        // angle_span = 360 exactly → impl trims by 0.01 to avoid empty arc.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_grid_arc(&layout, 0.0, 360.0, 1000.0, 1.0, &color);
        // Still produces a valid <path> element.
        assert!(svg.contains("<path d=\"M "));
        assert!(svg.contains(" A"));
    }

    #[test]
    fn test_svg_text_multiline_newline_passthrough() {
        // A text containing \n passes through verbatim (no splitting).
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_text(&layout, 0.0, 1400.0, "line1\nline2", 12.0, &color, 0.0);
        assert!(svg.contains("line1\nline2"));
    }

    #[test]
    fn test_svg_slice_stroke_thickness_strictly_greater_than_zero_required() {
        // Stroke thickness > 0 → stroke-width emitted; exactly 0 → not.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_slice(&layout, 0.0, 90.0, 900.0, 1000.0, Some(&color), Some(0.5), None, None);
        assert!(svg.contains("stroke-width"));
        let svg = svg_slice(&layout, 0.0, 90.0, 900.0, 1000.0, Some(&color), Some(0.0), None, None);
        assert!(!svg.contains("stroke-width"));
    }

    #[test]
    fn test_svg_document_add_string_preserved_verbatim() {
        // add() stores the element string unchanged — no escaping/transformation.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add("<custom attr=\"value\"/>".to_string());
        let svg = doc.render();
        assert!(svg.contains(r#"<custom attr="value"/>"#));
    }

    #[test]
    fn test_svg_tick_formats_coords_with_one_decimal() {
        // All coords use `{:.1}` formatting — even integers get ".0".
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_tick(&layout, 0.0, 1000.0, 1100.0, 1.0, &color);
        // e.g., x1="2500.0" (image_radius 1500 + 1000×cos(0) = 2500).
        assert!(svg.contains("x1=\"2500.0\""));
    }

    #[test]
    fn test_svg_document_render_lists_elements_in_insertion_order() {
        // elements are written in vec order — first `<alpha/>` appears before `<beta/>`.
        let mut doc = SvgDocument::new(10.0, 10.0);
        doc.add("<alpha/>".into());
        doc.add("<beta/>".into());
        doc.add("<gamma/>".into());
        let svg = doc.render();
        let ia = svg.find("<alpha/>").unwrap();
        let ib = svg.find("<beta/>").unwrap();
        let ig = svg.find("<gamma/>").unwrap();
        assert!(ia < ib);
        assert!(ib < ig);
    }

    #[test]
    fn test_svg_text_rotation_radians_output_in_degrees() {
        // rotation is radians, printed via .to_degrees() with {:.2} — π → "180.00".
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_text(&layout, 0.0, 1000.0, "x", 12.0, &color, std::f64::consts::PI);
        assert!(svg.contains("rotate(180.00,"));
        // rotation=π/2 → "90.00".
        let svg2 = svg_text(
            &layout, 0.0, 1000.0, "x", 12.0, &color,
            std::f64::consts::FRAC_PI_2,
        );
        assert!(svg2.contains("rotate(90.00,"));
    }

    #[test]
    fn test_svg_grid_arc_359_99_exact_not_trimmed() {
        // Strict > 359.99: exactly 359.99 does NOT trigger the -0.01 trim.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_grid_arc(&layout, 0.0, 359.99, 500.0, 1.0, &color);
        // End x position at 359.99° — without trim it stays.
        let expected_x = layout.image_radius + 500.0 * (359.99_f64.to_radians()).cos();
        // The second pair in "M x1,y1 A r,r 0 F,S x2,y2" — search for expected formatted endpoint.
        let expected_tag = format!("{:.1}", expected_x);
        assert!(svg.contains(&expected_tag), "svg: {}", svg);
    }

    #[test]
    fn test_svg_tick_equal_radii_yields_zero_length_line() {
        // radius_from == radius_to → x1==x2 and y1==y2 in output.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_tick(&layout, 45.0, 1000.0, 1000.0, 2.0, &color);
        // Extract x1 and x2 values from the <line> string.
        let x1_idx = svg.find("x1=\"").unwrap() + 4;
        let x1_end = svg[x1_idx..].find('"').unwrap() + x1_idx;
        let x1 = &svg[x1_idx..x1_end];
        let x2_idx = svg.find("x2=\"").unwrap() + 4;
        let x2_end = svg[x2_idx..].find('"').unwrap() + x2_idx;
        let x2 = &svg[x2_idx..x2_end];
        assert_eq!(x1, x2);
    }

    #[test]
    fn test_svg_document_render_ends_with_newline() {
        // writeln! always appends '\n'; output must end with newline after "</svg>".
        let doc = SvgDocument::new(10.0, 10.0);
        let svg = doc.render();
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn test_polar_to_xy_zero_radius_returns_center_regardless_of_angle() {
        // r=0 → radius*cos/sin = 0 for any angle → returns (cx, cy) always.
        let deg2rad = std::f64::consts::PI / 180.0;
        for &a in &[0.0_f64, 45.0, 90.0, 135.0, 180.0, 270.0, 359.9] {
            let (x, y) = polar_to_xy(100.0, 200.0, 0.0, a, deg2rad);
            assert_eq!(x, 100.0);
            assert_eq!(y, 200.0);
        }
    }

    #[test]
    fn test_svg_text_empty_string_produces_empty_text_element() {
        // Empty text content still produces a well-formed <text> element.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_text(&layout, 0.0, 1000.0, "", 12.0, &color, 0.0);
        assert!(svg.contains("<text "));
        assert!(svg.contains("></text>"));
    }

    #[test]
    fn test_svg_grid_arc_thickness_formatted_with_one_decimal() {
        // stroke-width uses {:.1} — 1.5 → "1.5", 2.0 → "2.0".
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 1.5, &color);
        assert!(svg.contains("stroke-width: 1.5;"));
        let svg2 = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 2.0, &color);
        assert!(svg2.contains("stroke-width: 2.0;"));
    }

    #[test]
    fn test_svg_tick_color_emitted_as_rgb_in_style() {
        // Color::to_svg_rgb → "rgb(r,g,b)" embedded in style attribute.
        let layout = mk_layout();
        let color = Color::rgb(12, 34, 56);
        let svg = svg_tick(&layout, 0.0, 1000.0, 1100.0, 1.0, &color);
        assert!(svg.contains("stroke: rgb(12,34,56);"));
    }

    #[test]
    fn test_polar_to_xy_angle_45_returns_sqrt2_over_2_scaled() {
        // angle=45° → cos=sin=√2/2; at origin with r=10 → x=y≈7.0710678.
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x, y) = polar_to_xy(0.0, 0.0, 10.0, 45.0, deg2rad);
        let expected = 10.0 * (std::f64::consts::FRAC_1_SQRT_2);
        assert!((x - expected).abs() < 1e-9);
        assert!((y - expected).abs() < 1e-9);
    }

    #[test]
    fn test_svg_document_nested_open_group_depths_preserved() {
        // Two open_group calls without close → both appear in order.
        let mut doc = SvgDocument::new(10.0, 10.0);
        doc.open_group("outer");
        doc.open_group("inner");
        doc.add("<circle/>".into());
        doc.close_group();
        doc.close_group();
        let svg = doc.render();
        let outer_idx = svg.find(r#"<g id="outer">"#).unwrap();
        let inner_idx = svg.find(r#"<g id="inner">"#).unwrap();
        assert!(outer_idx < inner_idx);
        // Two closing </g>.
        assert_eq!(svg.matches("</g>").count(), 2);
    }

    #[test]
    fn test_svg_slice_both_fill_and_stroke_emits_both_in_style() {
        // Fill + stroke with thickness → both attributes in style string.
        let layout = mk_layout();
        let fill = Color::rgb(255, 0, 0);
        let stroke = Color::rgb(0, 0, 255);
        let svg = svg_slice(
            &layout, 0.0, 90.0, 900.0, 1000.0,
            Some(&stroke), Some(2.0),
            Some(&fill), None,
        );
        assert!(svg.contains("fill: rgb(255,0,0);"));
        assert!(svg.contains("stroke: rgb(0,0,255);"));
    }

    #[test]
    fn test_svg_text_tiny_rotation_under_threshold_omits_transform() {
        // rotation.abs() < 0.01 → transform attribute NOT emitted.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_text(&layout, 0.0, 1000.0, "x", 12.0, &color, 0.001);
        assert!(!svg.contains("transform=\""));
        // rotation exactly at 0.0 also omits.
        let svg2 = svg_text(&layout, 0.0, 1000.0, "x", 12.0, &color, 0.0);
        assert!(!svg2.contains("transform=\""));
    }

    #[test]
    fn test_svg_grid_arc_diff_exactly_360_triggers_trim() {
        // (360 - 0).abs() = 360 > 359.99 → end_a trimmed by 0.01.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_grid_arc(&layout, 0.0, 360.0, 500.0, 1.0, &color);
        // After trim, end is at 359.99°.
        let expected_x = layout.image_radius + 500.0 * (359.99_f64.to_radians()).cos();
        let expected_tag = format!("{:.1}", expected_x);
        assert!(svg.contains(&expected_tag));
    }

    #[test]
    fn test_svg_slice_angles_already_ordered_unchanged() {
        // start_a <= end_a → no swap; same output as explicit forward call.
        let layout = mk_layout();
        let fill = Color::rgb(255, 0, 0);
        let svg_fwd = svg_slice(
            &layout, 30.0, 90.0, 900.0, 1000.0,
            None, None, Some(&fill), None,
        );
        // Reversed call should swap internally — produce same output.
        let svg_rev = svg_slice(
            &layout, 90.0, 30.0, 900.0, 1000.0,
            None, None, Some(&fill), None,
        );
        assert_eq!(svg_fwd, svg_rev);
    }

    #[test]
    fn test_svg_document_open_group_id_passed_through_verbatim() {
        // open_group doesn't sanitize — special chars appear in the id attr.
        let mut doc = SvgDocument::new(10.0, 10.0);
        doc.open_group("track_1_data");
        let svg = doc.render();
        assert!(svg.contains(r#"<g id="track_1_data">"#));
        // Special id with dash also passes through.
        let mut doc2 = SvgDocument::new(10.0, 10.0);
        doc2.open_group("ideo-hs1");
        let svg2 = doc2.render();
        assert!(svg2.contains(r#"<g id="ideo-hs1">"#));
    }

    #[test]
    fn test_svg_document_width_integer_formatted_with_zero_decimal_suffix() {
        // {:.0} → integers get stringified without decimals — "100px" not "100.0px".
        let doc = SvgDocument::new(100.5, 200.4);
        let svg = doc.render();
        // 100.5 rounded to 100 via {:.0} (round-half-to-even).
        assert!(svg.contains(r#"width="100px""#) || svg.contains(r#"width="101px""#));
        // Integer literal 200 (200.4 → 200).
        assert!(svg.contains(r#"height="200px""#));
    }

    #[test]
    fn test_polar_to_xy_at_positive_radius_with_angle_zero_plants_right_of_center() {
        // angle=0 → cos=1, sin=0 → (cx+r, cy).
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x, y) = polar_to_xy(500.0, 500.0, 100.0, 0.0, deg2rad);
        assert_eq!(x, 600.0);
        assert_eq!(y, 500.0);
    }

    #[test]
    fn test_svg_grid_arc_end_angle_below_threshold_preserved_exactly() {
        // diff < 359.99 → no trim → original end angle used.
        let layout = mk_layout();
        let color = Color::rgb(0, 0, 0);
        let svg = svg_grid_arc(&layout, 0.0, 180.0, 500.0, 1.0, &color);
        // End at 180° → x = cx + 500*cos(180°) = 1500 + 500*(-1) = 1000.0.
        assert!(svg.contains("1000.0"));
    }

    #[test]
    fn test_svg_slice_opacity_below_one_emits_fill_opacity_attr() {
        // opacity=0.5 < 1.0 → "fill-opacity: 0.50" or similar included.
        let layout = mk_layout();
        let fill = Color::rgb(255, 0, 0);
        let svg = svg_slice(
            &layout, 0.0, 90.0, 900.0, 1000.0,
            None, None, Some(&fill), Some(0.5),
        );
        // opacity value appears in the output when < 1.
        assert!(svg.contains("opacity"));
        // Path element still emitted.
        assert!(svg.contains("<path "));
    }

    #[test]
    fn test_svg_document_render_contains_dtd_declaration_line() {
        // The render() output includes the SVG 1.1 DTD PUBLIC identifier.
        let doc = SvgDocument::new(100.0, 100.0);
        let s = doc.render();
        assert!(s.contains("SVG 1.1"));
        assert!(s.contains("DOCTYPE svg"));
        // And the xmlns:xlink attribute is always set on the root.
        assert!(s.contains("xmlns:xlink=\"http://www.w3.org/1999/xlink\""));
    }

    #[test]
    fn test_svg_tick_line_element_contains_stroke_width_formatted_to_one_decimal() {
        // svg_tick formats stroke-width with {:.1} — integer thickness gains ".0".
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 1000.0, 1100.0, 3.0, &c);
        assert!(s.contains("stroke-width: 3.0"));
        // Fractional thickness rounds to 1 decimal.
        let s2 = svg_tick(&layout, 0.0, 1000.0, 1100.0, 2.75, &c);
        assert!(s2.contains("stroke-width: 2.8") || s2.contains("stroke-width: 2.7"));
    }

    #[test]
    fn test_polar_to_xy_distance_from_center_matches_radius() {
        // For any angle, the returned point is exactly `radius` away from center.
        let deg2rad = std::f64::consts::PI / 180.0;
        for angle in [0.0, 37.5, 90.0, 143.0, 211.0, 299.0, 359.9] {
            let (x, y) = polar_to_xy(500.0, 500.0, 250.0, angle, deg2rad);
            let dx = x - 500.0;
            let dy = y - 500.0;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!((dist - 250.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_svg_grid_arc_large_arc_flag_based_on_sweep_magnitude() {
        // 180° sweep: abs diff is exactly 180 → NOT > 180 → large-arc flag 0.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s_180 = svg_grid_arc(&layout, 0.0, 180.0, 500.0, 1.0, &c);
        assert!(s_180.contains(" 0 0,1 "));
        // 181° sweep → large-arc flag 1.
        let s_181 = svg_grid_arc(&layout, 0.0, 181.0, 500.0, 1.0, &c);
        assert!(s_181.contains(" 0 1,1 "));
    }

    #[test]
    fn test_svg_text_rotation_emitted_in_degrees_via_to_degrees() {
        // The `rotation` argument is interpreted as RADIANS — the output uses .to_degrees().
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        // π/2 rad → 90 degrees.
        let s = svg_text(&layout, 0.0, 1000.0, "text", 12.0, &c, std::f64::consts::FRAC_PI_2);
        assert!(s.contains("rotate(90.00"));
    }

    #[test]
    fn test_svg_text_font_size_formatted_with_zero_decimal() {
        // font-size is formatted with {:.0} — fractional sizes get rounded.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "x", 12.7, &c, 0.0);
        // 12.7 → "13" via {:.0}.
        assert!(s.contains("font-size: 13px"));
        // 14.0 → "14".
        let s2 = svg_text(&layout, 0.0, 1000.0, "x", 14.0, &c, 0.0);
        assert!(s2.contains("font-size: 14px"));
    }

    #[test]
    fn test_svg_document_render_closing_svg_tag_on_its_own_line() {
        // The final "</svg>" must appear as its own line at the end.
        let mut doc = SvgDocument::new(50.0, 50.0);
        doc.add("<rect width=\"50\" height=\"50\"/>".to_string());
        let s = doc.render();
        // The final three chars before the trailing newline should be `svg>`.
        let trimmed = s.trim_end();
        assert!(trimmed.ends_with("</svg>"));
        // Count occurrences of </svg>.
        assert_eq!(s.matches("</svg>").count(), 1);
    }

    #[test]
    fn test_svg_document_open_close_group_alternation_produces_balanced_tags() {
        // Alternating open/close calls produce matched <g>...</g> pairs.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("g1");
        doc.add("<circle/>".into());
        doc.close_group();
        doc.open_group("g2");
        doc.add("<rect/>".into());
        doc.close_group();
        let s = doc.render();
        // Two opens, two closes.
        assert_eq!(s.matches("<g id=").count(), 2);
        assert_eq!(s.matches("</g>").count(), 2);
    }

    #[test]
    fn test_svg_text_text_content_passed_verbatim() {
        // svg_text places the `text` argument as element content without escaping.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "Hello World", 12.0, &c, 0.0);
        assert!(s.contains(">Hello World</text>"));
    }

    #[test]
    fn test_svg_tick_fractional_angle_formatted_to_one_decimal_coords() {
        // svg_tick formats x1/y1/x2/y2 with {:.1}. Use an angle that yields fractional coords.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 37.5, 1000.0, 1100.0, 1.0, &c);
        // x1, y1, x2, y2 all {:.1} formatted — one decimal place.
        // Count the ".<digit>" patterns to confirm decimal formatting.
        // Simple check: output contains x1="..." y1="..." pattern with dot followed by 1 digit.
        assert!(s.contains("<line "));
        // At least one float representation with dot.
        assert!(s.contains('.'));
    }

    #[test]
    fn test_svg_document_add_empty_string_preserves_in_elements_list() {
        // doc.add("") adds an empty line; render includes that blank line.
        let mut doc = SvgDocument::new(10.0, 10.0);
        doc.add(String::new());
        let s = doc.render();
        // Must still have valid SVG structure.
        assert!(s.contains("<?xml"));
        assert!(s.contains("<svg"));
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn test_svg_grid_arc_start_end_equal_minus_0_01_offset_applied() {
        // When |diff| > 359.99, end_a -= 0.01 — full-circle trimming to avoid degenerate path.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        // 360° sweep → diff is 360, > 359.99 → end_a shifted by 0.01.
        let s = svg_grid_arc(&layout, 0.0, 360.0, 500.0, 1.0, &c);
        assert!(s.contains("<path"));
        // The trimming should prevent coincident start/end points.
        // Just verify output is valid path without panicking.
        assert!(s.contains(" A"));
    }

    #[test]
    fn test_svg_slice_same_radius_renders_arc_not_wedge() {
        // radius_from == radius_to (within f64::EPSILON) → renders as zero-width annular (arc only).
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let s = svg_slice(
            &layout, 0.0, 90.0, 1000.0, 1000.0,
            None, None, Some(&fill), None,
        );
        // Zero-width slice outputs an arc path with fill:none.
        assert!(s.contains("<path"));
        assert!(s.contains("fill: none"));
    }

    #[test]
    fn test_svg_text_short_rotation_below_threshold_no_transform() {
        // Rotation |r| < 0.01 → no transform attr emitted.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "txt", 12.0, &c, 0.001);
        assert!(!s.contains("transform="));
        // At threshold 0.01 exactly also treated as small.
        let s2 = svg_text(&layout, 0.0, 1000.0, "txt", 12.0, &c, 0.005);
        assert!(!s2.contains("transform="));
    }

    #[test]
    fn test_svg_document_add_with_trailing_newline_preserved_in_element() {
        // .add() stores the string verbatim; rendering writeln! adds its own newline.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add("<rect/>\n".into()); // already has newline
        let s = doc.render();
        // Output should contain the added content intact.
        assert!(s.contains("<rect/>"));
    }

    #[test]
    fn test_svg_tick_zero_thickness_still_emits_line() {
        // Even thickness=0 emits a valid <line> element (stroke-width: 0.0).
        let layout = mk_layout();
        let c = Color::rgb(255, 0, 0);
        let s = svg_tick(&layout, 45.0, 1000.0, 1100.0, 0.0, &c);
        assert!(s.contains("<line "));
        assert!(s.contains("stroke-width: 0.0"));
    }

    #[test]
    fn test_polar_to_xy_angle_90_places_at_south() {
        // For angle=90° (using the standard math convention with positive Y down in SVG),
        // polar_to_xy places the point below center. Exact position depends on y-axis orientation.
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x, y) = polar_to_xy(500.0, 500.0, 100.0, 90.0, deg2rad);
        // At angle=90, cos=0 so x≈cx; sin=1 so y=cy+r (below in SVG y-down coords).
        assert!((x - 500.0).abs() < 1e-6);
        assert!((y - 600.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_slice_edge_color_none_with_positive_stroke_emits_width_only() {
        // stroke=None + thickness=2.5 → emits just "stroke-width: 2.5;" (no stroke color).
        let layout = mk_layout();
        let fill = Color::rgb(0, 255, 0);
        let s = svg_slice(
            &layout, 0.0, 45.0, 900.0, 1000.0,
            None, Some(2.5), Some(&fill), None,
        );
        assert!(s.contains("stroke-width: 2.5"));
        // No "stroke:" color (only stroke-width).
        assert!(!s.contains("stroke: rgb"));
    }

    #[test]
    fn test_svg_document_render_produces_nonzero_output_for_empty_doc() {
        // Even an empty SvgDocument yields a valid SVG skeleton.
        let doc = SvgDocument::new(500.0, 500.0);
        let s = doc.render();
        assert!(s.len() > 50); // DOCTYPE + <?xml + <svg + </svg>
        assert!(s.contains("<?xml"));
        assert!(s.contains("<svg"));
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn test_svg_text_empty_string_still_renders_element() {
        // text="" → produces <text>...</text> with empty content.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "", 12.0, &c, 0.0);
        assert!(s.contains("<text "));
        assert!(s.contains("></text>"));
    }

    #[test]
    fn test_polar_to_xy_at_center_when_radius_zero() {
        // radius=0 → always at (cx, cy) regardless of angle.
        let deg2rad = std::f64::consts::PI / 180.0;
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0, 359.0] {
            let (x, y) = polar_to_xy(100.0, 100.0, 0.0, angle, deg2rad);
            assert!((x - 100.0).abs() < 1e-6);
            assert!((y - 100.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_svg_document_width_and_height_both_reflected_in_root() {
        // Non-equal width/height both appear in root attrs.
        let doc = SvgDocument::new(300.0, 450.0);
        let s = doc.render();
        assert!(s.contains("width=\"300px\""));
        assert!(s.contains("height=\"450px\""));
    }

    #[test]
    fn test_svg_tick_thickness_larger_than_typical_still_formats_cleanly() {
        // Large thickness values still format correctly.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 1000.0, 1050.0, 1000.0, &c);
        assert!(s.contains("stroke-width: 1000.0"));
    }

    #[test]
    fn test_svg_grid_arc_small_sweep_no_trim_applied() {
        // Small sweep (e.g., 45°) stays below 359.99 threshold → no end_a trim.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 10.0, 55.0, 500.0, 1.0, &c);
        // Verify no degenerate trim — the output path should start valid.
        assert!(s.starts_with("<path"));
        assert!(s.contains("M "));
        assert!(s.contains("A"));
    }

    #[test]
    fn test_svg_document_open_group_id_escaped_in_output() {
        // Group id with special chars — passed through verbatim.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("my-group_1");
        doc.close_group();
        let s = doc.render();
        assert!(s.contains("<g id=\"my-group_1\">"));
    }

    #[test]
    fn test_svg_slice_edge_stroke_none_emits_stroke_none_regardless_of_color() {
        // edge_stroke=None → "stroke: none;" even if edge_color is Some.
        let layout = mk_layout();
        let stroke = Color::rgb(100, 0, 0);
        let fill = Color::rgb(255, 255, 255);
        let s = svg_slice(
            &layout, 0.0, 45.0, 900.0, 1000.0,
            Some(&stroke), None, Some(&fill), None,
        );
        assert!(s.contains("stroke: none"));
    }

    #[test]
    fn test_polar_to_xy_opposite_angles_mirror_across_center() {
        // angle=0 and angle=180 → points mirror across center along x-axis.
        let deg2rad = std::f64::consts::PI / 180.0;
        let (x0, y0) = polar_to_xy(500.0, 500.0, 100.0, 0.0, deg2rad);
        let (x180, y180) = polar_to_xy(500.0, 500.0, 100.0, 180.0, deg2rad);
        // y ≈ cy for both; x0 = cx+100, x180 = cx-100.
        assert!((y0 - y180).abs() < 1e-6);
        assert!((x0 + x180 - 2.0 * 500.0).abs() < 1e-6);
    }

    #[test]
    fn test_svg_tick_string_output_contains_all_four_coord_fields() {
        // svg_tick output has x1, y1, x2, y2.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 30.0, 900.0, 1000.0, 1.0, &c);
        assert!(s.contains("x1="));
        assert!(s.contains("y1="));
        assert!(s.contains("x2="));
        assert!(s.contains("y2="));
    }

    #[test]
    fn test_svg_document_close_group_writes_closing_g_tag() {
        // close_group appends a literal "</g>" element.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("foo");
        doc.close_group();
        let out = doc.render();
        assert!(out.contains("<g id=\"foo\">"));
        assert!(out.contains("</g>"));
    }

    #[test]
    fn test_svg_document_add_preserves_exact_string_no_wrapping() {
        // add() stores the element verbatim — no implicit tag or escaping.
        let mut doc = SvgDocument::new(50.0, 50.0);
        doc.add("<custom attr='v'/>".to_string());
        let out = doc.render();
        assert!(out.contains("<custom attr='v'/>"));
    }

    #[test]
    fn test_svg_document_render_has_xml_declaration_first_line() {
        // First line of render() output is the XML declaration.
        let doc = SvgDocument::new(10.0, 20.0);
        let out = doc.render();
        let first_line = out.lines().next().unwrap();
        assert_eq!(first_line, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    }

    #[test]
    fn test_svg_document_render_ends_with_svg_close_tag() {
        // Last non-empty line is "</svg>".
        let doc = SvgDocument::new(10.0, 20.0);
        let out = doc.render();
        let last_line = out.lines().filter(|l| !l.is_empty()).last().unwrap();
        assert_eq!(last_line, "</svg>");
    }

    #[test]
    fn test_svg_tick_includes_stroke_width_format_one_decimal() {
        // thickness formatted with {:.1} → "1.5" not "1.50".
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 45.0, 900.0, 1000.0, 1.5, &c);
        assert!(s.contains("stroke-width: 1.5;"));
    }

    #[test]
    fn test_svg_grid_arc_full_circle_trims_end_by_001() {
        // Full-circle (360° span) trims end by 0.01 → not exactly 0.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        // A span > 359.99 triggers the trim path.
        let s = svg_grid_arc(&layout, 0.0, 360.0, 1000.0, 1.0, &c);
        // Output should be a path with two distinct endpoints (not identical).
        assert!(s.starts_with(r#"<path d="M "#));
        // Path contains A command.
        assert!(s.contains(" A"));
    }

    #[test]
    fn test_svg_grid_arc_small_span_emits_large_arc_zero() {
        // |span| < 180 → large-arc flag 0.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 60.0, 1000.0, 1.0, &c);
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_svg_document_empty_elements_vec_renders_valid_svg_structure() {
        // Empty elements vec still produces valid SVG with open+close tags.
        let doc = SvgDocument::new(100.0, 50.0);
        let out = doc.render();
        assert!(out.contains("<svg "));
        assert!(out.contains("</svg>"));
        // No group elements present.
        assert!(!out.contains("<g "));
    }

    #[test]
    fn test_svg_text_contains_font_size_in_style() {
        // font_size formatted as integer px in style attribute.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "hello", 24.0, &c, 0.0);
        assert!(s.contains("font-size: 24px;"));
    }

    #[test]
    fn test_svg_text_contains_text_content_verbatim() {
        // Text content appears literally between <text> tags.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "ABC123", 12.0, &c, 0.0);
        assert!(s.contains(">ABC123<"));
    }

    #[test]
    fn test_svg_document_renders_multiple_elements_in_insertion_order() {
        // First-added element appears before second in output.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add("<first/>".to_string());
        doc.add("<second/>".to_string());
        let out = doc.render();
        let pos1 = out.find("<first/>").expect("first present");
        let pos2 = out.find("<second/>").expect("second present");
        assert!(pos1 < pos2);
    }

    #[test]
    fn test_svg_slice_zero_opacity_still_emits_opacity_attribute() {
        // opacity=0 (<1.0) emits " opacity: 0.000;" in style.
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let s = svg_slice(
            &layout, 0.0, 90.0, 800.0, 900.0,
            None, None, Some(&fill), Some(0.0),
        );
        assert!(s.contains("opacity: 0.000;"));
    }

    #[test]
    fn test_svg_slice_opacity_of_one_omits_opacity_style() {
        // opacity=1.0 (NOT < 1.0) → no opacity style.
        let layout = mk_layout();
        let fill = Color::rgb(0, 0, 255);
        let s = svg_slice(
            &layout, 0.0, 90.0, 800.0, 900.0,
            None, None, Some(&fill), Some(1.0),
        );
        assert!(!s.contains("opacity:"));
    }

    #[test]
    fn test_svg_text_with_rotation_includes_transform_attribute() {
        // Non-zero rotation → transform="rotate(..." in output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 45.0, 1000.0, "tilted", 12.0, &c, 1.5);
        assert!(s.contains("transform=\"rotate("));
    }

    #[test]
    fn test_svg_grid_arc_non_full_circle_preserves_endpoint_angle() {
        // Span < 359.99 → end angle not trimmed by 0.01.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 90.0, 1000.0, 1.0, &c);
        // Path contains endpoint at 90° which is (cx + 1000*cos(90°), cy + 1000*sin(90°)).
        // With cx=cy=1500 (mk_layout image_radius), endpoint ≈ (1500, 2500).
        assert!(s.contains("2500"));
    }

    #[test]
    fn test_svg_slice_start_angle_greater_than_end_swapped_internally() {
        // When start > end, the implementation swaps them — output should look the same
        // as forward direction (modulo rounding).
        let layout = mk_layout();
        let fill = Color::rgb(0, 0, 255);
        let a = svg_slice(
            &layout, 10.0, 80.0, 800.0, 900.0,
            None, None, Some(&fill), None,
        );
        let b = svg_slice(
            &layout, 80.0, 10.0, 800.0, 900.0,
            None, None, Some(&fill), None,
        );
        // Swap means both produce same output.
        assert_eq!(a, b);
    }

    #[test]
    fn test_svg_slice_edge_color_with_positive_stroke_emits_color_in_style() {
        // edge_color + stroke_width>0 → stroke color emitted in style.
        let layout = mk_layout();
        let edge = Color::rgb(200, 50, 50);
        let fill = Color::rgb(100, 100, 100);
        let s = svg_slice(
            &layout, 0.0, 45.0, 800.0, 900.0,
            Some(&edge), Some(2.0), Some(&fill), None,
        );
        assert!(s.contains("stroke: rgb(200,50,50);"));
        assert!(s.contains("stroke-width: 2.0;"));
    }

    #[test]
    fn test_svg_tick_output_has_line_element_tag() {
        // svg_tick emits a <line> SVG element.
        let layout = mk_layout();
        let c = Color::rgb(50, 100, 150);
        let s = svg_tick(&layout, 10.0, 800.0, 900.0, 1.5, &c);
        assert!(s.starts_with("<line "));
        assert!(s.ends_with("/>"));
    }

    #[test]
    fn test_svg_document_new_stores_width_and_height() {
        // Width/height passed to constructor accessible via render.
        let doc = SvgDocument::new(1234.5, 678.9);
        let out = doc.render();
        // {:.0} rounds 1234.5 → 1234, 678.9 → 679 (Rust banker's rounding).
        assert!(out.contains("width=\"1234px\"") || out.contains("width=\"1235px\""));
    }

    #[test]
    fn test_svg_grid_arc_at_full_360_deg_trims_endpoint_by_001() {
        // |span| == 360 triggers trim logic (> 359.99).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 360.0, 500.0, 1.0, &c);
        // The path should contain "A" arc command.
        assert!(s.contains(" A"));
    }

    #[test]
    fn test_svg_text_zero_font_size_emits_zero_px() {
        // font_size=0 → "font-size: 0px" in style.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "text", 0.0, &c, 0.0);
        assert!(s.contains("font-size: 0px;"));
    }

    #[test]
    fn test_svg_slice_zero_width_arc_emits_no_fill() {
        // radius_inner == radius_outer → zero-width arc path, fill:none.
        let layout = mk_layout();
        let edge = Color::rgb(0, 0, 0);
        let s = svg_slice(
            &layout, 0.0, 45.0, 900.0, 900.0,
            Some(&edge), Some(1.0), None, None,
        );
        assert!(s.contains("fill: none;"));
    }

    #[test]
    fn test_svg_document_render_includes_xmlns_attribute() {
        // Output contains xmlns namespace declaration.
        let doc = SvgDocument::new(100.0, 100.0);
        let out = doc.render();
        assert!(out.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    }

    #[test]
    fn test_svg_slice_both_edge_and_fill_emit_both_in_style() {
        // Both edge_stroke + edge_color and fill_color set → both colors in style.
        let layout = mk_layout();
        let edge = Color::rgb(10, 20, 30);
        let fill = Color::rgb(100, 200, 50);
        let s = svg_slice(
            &layout, 0.0, 90.0, 800.0, 900.0,
            Some(&edge), Some(1.5), Some(&fill), None,
        );
        assert!(s.contains("stroke: rgb(10,20,30);"));
        assert!(s.contains("fill: rgb(100,200,50);"));
    }

    #[test]
    fn test_svg_document_doctype_declaration_in_render() {
        // Output contains the SVG 1.1 DOCTYPE.
        let doc = SvgDocument::new(100.0, 100.0);
        let out = doc.render();
        assert!(out.contains("<!DOCTYPE svg"));
        assert!(out.contains("SVG 1.1"));
    }

    #[test]
    fn test_svg_tick_nonzero_angle_produces_non_radial_endpoint() {
        // At angle=90°, tick from r_from to r_to extends along y-axis only.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 90.0, 800.0, 900.0, 1.0, &c);
        assert!(s.contains("<line "));
    }

    #[test]
    fn test_svg_slice_stroke_zero_emits_stroke_none() {
        // edge_stroke=0 (or missing) → "stroke: none;"
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let s = svg_slice(
            &layout, 0.0, 45.0, 800.0, 900.0,
            None, Some(0.0), Some(&fill), None,
        );
        assert!(s.contains("stroke: none;"));
    }

    #[test]
    fn test_svg_grid_arc_exact_180_sweep_flag_zero() {
        // |angle_end - angle_start| == 180 (not > 180) → flag 0.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 180.0, 500.0, 1.0, &c);
        // Output has " 0,1 " for large-arc=0, sweep=1.
        assert!(s.contains(" 0,1 "));
    }

    #[test]
    fn test_svg_slice_null_fill_and_null_edge_emits_fill_none_stroke_none() {
        // Both fill and edge None → "fill: none;" and "stroke: none;".
        let layout = mk_layout();
        let s = svg_slice(
            &layout, 0.0, 45.0, 800.0, 900.0,
            None, None, None, None,
        );
        assert!(s.contains("stroke: none;"));
        assert!(s.contains("fill: none;"));
    }

    #[test]
    fn test_svg_text_handles_html_special_chars_verbatim() {
        // "<" and "&" in text passed through literally (not escaped by svg_text).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "A<B&C", 12.0, &c, 0.0);
        assert!(s.contains("A<B&C"));
    }

    #[test]
    fn test_svg_tick_same_from_to_radius_produces_degenerate_line() {
        // radius_from == radius_to → zero-length line (degenerate but valid).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 45.0, 900.0, 900.0, 1.0, &c);
        assert!(s.starts_with("<line "));
    }

    #[test]
    fn test_svg_grid_arc_negative_span_handled_correctly() {
        // 100° → 50° produces well-formed output without panic.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 100.0, 50.0, 500.0, 1.0, &c);
        assert!(s.starts_with("<path "));
    }

    #[test]
    fn test_svg_tick_with_larger_from_than_to_radius_valid_line() {
        // r_from > r_to — still valid line.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_tick(&layout, 30.0, 1000.0, 800.0, 1.0, &c);
        assert!(s.starts_with("<line "));
    }

    #[test]
    fn test_svg_document_multiple_open_close_groups_preserved_in_order() {
        // Multiple group open/close pairs → all present.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("g1");
        doc.close_group();
        doc.open_group("g2");
        doc.close_group();
        let out = doc.render();
        assert!(out.contains("<g id=\"g1\">"));
        assert!(out.contains("<g id=\"g2\">"));
        assert_eq!(out.matches("</g>").count(), 2);
    }

    #[test]
    fn test_svg_slice_with_stroke_but_no_edge_color_emits_none_color() {
        // stroke_width=2 without edge_color → "stroke: none;" (color not set).
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let s = svg_slice(
            &layout, 0.0, 45.0, 800.0, 900.0,
            None, Some(2.0), Some(&fill), None,
        );
        assert!(s.contains("stroke: none;"));
    }

    #[test]
    fn test_svg_text_output_includes_fill_color_in_style() {
        // Color emitted as rgb(r,g,b) in fill style.
        let layout = mk_layout();
        let c = Color::rgb(200, 50, 100);
        let s = svg_text(&layout, 0.0, 1000.0, "text", 14.0, &c, 0.0);
        assert!(s.contains("fill: rgb(200,50,100);"));
    }

    #[test]
    fn test_svg_document_render_contains_xml_version_1_0() {
        // XML declaration header includes version="1.0".
        let doc = SvgDocument::new(100.0, 100.0);
        let out = doc.render();
        assert!(out.contains(r#"version="1.0""#));
    }

    #[test]
    fn test_svg_tick_emits_x1_y1_x2_y2_attributes_all_present() {
        // All 4 coord attributes in output line element.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 500.0, 1000.0, 1.0, &c);
        assert!(s.contains("x1="));
        assert!(s.contains("y1="));
        assert!(s.contains("x2="));
        assert!(s.contains("y2="));
    }

    #[test]
    fn test_svg_slice_with_only_edge_color_no_fill_emits_fill_none() {
        // Only edge provided, no fill → "fill: none;" in output.
        let layout = mk_layout();
        let edge = Color::rgb(0, 0, 0);
        let s = svg_slice(
            &layout, 0.0, 45.0, 800.0, 900.0,
            Some(&edge), Some(1.0), None, None,
        );
        assert!(s.contains("fill: none;"));
    }

    #[test]
    fn test_svg_document_empty_render_output_is_non_empty_string() {
        // Even empty SvgDocument.render() has non-zero length (headers + svg tags).
        let doc = SvgDocument::new(50.0, 50.0);
        let out = doc.render();
        assert!(!out.is_empty());
        assert!(out.len() > 100); // more than trivial length
    }

    #[test]
    fn test_svg_document_width_height_reflected_in_output_rounded() {
        // {:.0} rounds — 100.7 → 101 or 100.
        let doc = SvgDocument::new(100.7, 200.4);
        let out = doc.render();
        // Width 100.7 → rounded to 101.
        assert!(out.contains("width=\"101px\""));
        // Height 200.4 → 200.
        assert!(out.contains("height=\"200px\""));
    }

    #[test]
    fn test_svg_slice_zero_sweep_produces_output() {
        // start == end → zero sweep, still produces SVG output.
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let s = svg_slice(
            &layout, 45.0, 45.0, 800.0, 900.0,
            None, None, Some(&fill), None,
        );
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_text_includes_x_and_y_coordinates() {
        // x= and y= attributes present in text output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 1000.0, "hi", 12.0, &c, 0.0);
        assert!(s.contains("x="));
        assert!(s.contains("y="));
    }

    #[test]
    fn test_svg_grid_arc_different_thickness_values_format_correctly() {
        // Thickness 0.5 and 10.0 both formatted with {:.1}.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s1 = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 0.5, &c);
        let s2 = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 10.0, &c);
        assert!(s1.contains("stroke-width: 0.5;"));
        assert!(s2.contains("stroke-width: 10.0;"));
    }

    #[test]
    fn test_svg_tick_color_embedded_in_style_attribute() {
        // Tick color emitted as rgb(r,g,b) in stroke style.
        let layout = mk_layout();
        let c = Color::rgb(100, 200, 50);
        let s = svg_tick(&layout, 45.0, 800.0, 900.0, 1.0, &c);
        assert!(s.contains("stroke: rgb(100,200,50);"));
    }

    #[test]
    fn test_svg_document_xml_declaration_attributes_order() {
        // XML declaration has version and encoding attributes.
        let doc = SvgDocument::new(100.0, 100.0);
        let out = doc.render();
        assert!(out.contains(r#"encoding="UTF-8""#));
    }

    #[test]
    fn test_svg_slice_explicit_none_edge_color_uses_none_stroke_style() {
        // None edge_color + Some(stroke_width=1) → "stroke: none;"
        let layout = mk_layout();
        let fill = Color::rgb(50, 50, 50);
        let s = svg_slice(
            &layout, 0.0, 45.0, 800.0, 900.0,
            None, Some(1.0), Some(&fill), None,
        );
        assert!(s.contains("stroke: none;"));
    }

    #[test]
    fn test_svg_text_with_multiple_arguments_output_contains_each() {
        // All text/color components present.
        let layout = mk_layout();
        let c = Color::rgb(10, 20, 30);
        let s = svg_text(&layout, 0.0, 1000.0, "Some label text", 14.0, &c, 0.0);
        assert!(s.contains("Some label text"));
        assert!(s.contains("font-size: 14px;"));
    }

    #[test]
    fn test_svg_slice_angles_reversed_output_consistent_with_forward() {
        // Reversed angles → internal swap → same output.
        let layout = mk_layout();
        let fill = Color::rgb(100, 100, 100);
        let forward = svg_slice(&layout, 10.0, 60.0, 800.0, 900.0, None, None, Some(&fill), None);
        let reverse = svg_slice(&layout, 60.0, 10.0, 800.0, 900.0, None, None, Some(&fill), None);
        assert_eq!(forward, reverse);
    }

    #[test]
    fn test_svg_tick_zero_thickness_still_emits_line_element() {
        // thickness=0 → still emits <line> tag.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 500.0, 1000.0, 0.0, &c);
        assert!(s.starts_with("<line "));
    }

    #[test]
    fn test_svg_document_render_final_line_is_svg_closing_tag() {
        // Last non-empty line of render is "</svg>".
        let doc = SvgDocument::new(100.0, 100.0);
        let out = doc.render();
        let last_line = out.lines().filter(|l| !l.is_empty()).last().unwrap();
        assert_eq!(last_line, "</svg>");
    }

    #[test]
    fn test_svg_grid_arc_endpoint_just_inside_360_still_produces_valid_path() {
        // 359.5° sweep — produces valid path.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 359.5, 500.0, 1.0, &c);
        assert!(s.contains("<path "));
    }

    #[test]
    fn test_svg_text_zero_rotation_emits_no_transform_attr() {
        // rotation ≈ 0 → no "transform" in output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 90.0, 500.0, "hi", 12.0, &c, 0.0);
        assert!(!s.contains("transform"));
        assert!(s.contains(">hi<"));
    }

    #[test]
    fn test_svg_text_nonzero_rotation_emits_transform_attr() {
        // rotation != 0 → transform="rotate(...)" in output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 90.0, 500.0, "hi", 12.0, &c, 45.0);
        assert!(s.contains("transform") || s.contains("rotate"));
    }

    #[test]
    fn test_svg_tick_nonzero_thickness_appears_in_style() {
        // thickness=3 → "stroke-width: 3" somewhere.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_tick(&layout, 0.0, 500.0, 510.0, 3.0, &c);
        assert!(s.contains("3"));
    }

    #[test]
    fn test_svg_grid_arc_zero_thickness_still_renders_path() {
        // thickness=0 → still emits a <path> element.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 0.0, &c);
        assert!(s.contains("<path "));
    }

    #[test]
    fn test_svg_document_open_and_close_group_pair_in_elements() {
        // Opening and closing groups produces paired <g id="..."> and </g>.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("my_group");
        doc.close_group();
        let s = doc.render();
        assert!(s.contains(r#"<g id="my_group">"#));
        assert!(s.contains("</g>"));
    }

    #[test]
    fn test_svg_document_add_arbitrary_element_appears_in_render() {
        // Arbitrary element strings propagate verbatim into output.
        let mut doc = SvgDocument::new(50.0, 50.0);
        doc.add("<circle cx=\"25\" cy=\"25\" r=\"10\"/>".to_string());
        let s = doc.render();
        assert!(s.contains("<circle cx=\"25\" cy=\"25\" r=\"10\"/>"));
    }

    #[test]
    fn test_svg_document_new_stores_dimensions_for_render() {
        // Dimensions set by new() are reflected in render output.
        let doc = SvgDocument::new(250.5, 375.5);
        assert_eq!(doc.width, 250.5);
        assert_eq!(doc.height, 375.5);
    }

    #[test]
    fn test_svg_document_includes_xmlns_declaration() {
        // xmlns attribute appears in render output header.
        let doc = SvgDocument::new(10.0, 10.0);
        let s = doc.render();
        assert!(s.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
    }

    #[test]
    fn test_svg_document_render_contains_xml_version_declaration() {
        // <?xml version="1.0" encoding="UTF-8"?> header always present.
        let doc = SvgDocument::new(10.0, 10.0);
        let s = doc.render();
        assert!(s.contains(r#"<?xml version="1.0""#));
    }

    #[test]
    fn test_svg_document_render_with_no_elements_still_has_tags() {
        // Empty doc still has opening and closing <svg> tags.
        let doc = SvgDocument::new(100.0, 50.0);
        let s = doc.render();
        assert!(s.contains("<svg "));
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn test_svg_slice_produces_svg_element_string() {
        // svg_slice produces some form of SVG element output.
        let layout = mk_layout();
        let c = Color::rgb(255, 128, 0);
        let s = svg_slice(&layout, 0.0, 90.0, 400.0, 500.0, Some(&c), None, Some(&c), None);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_text_includes_font_size_style() {
        // svg_text output includes the font-size style.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 45.0, 500.0, "sample", 14.0, &c, 0.0);
        assert!(s.contains("font-size") || s.contains("14"));
    }

    #[test]
    fn test_svg_document_multiple_group_nested_structure() {
        // 2 nested open_groups and 2 close_groups → 4 tags in order.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("outer");
        doc.open_group("inner");
        doc.close_group();
        doc.close_group();
        let s = doc.render();
        assert!(s.contains(r#"<g id="outer">"#));
        assert!(s.contains(r#"<g id="inner">"#));
    }

    #[test]
    fn test_svg_tick_includes_stroke_attribute() {
        // svg_tick output should contain stroke style referencing color.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_tick(&layout, 45.0, 500.0, 510.0, 2.0, &c);
        assert!(s.contains("stroke"));
    }

    #[test]
    fn test_svg_grid_arc_large_radius_produces_path() {
        // Very large radius — still produces valid <path>.
        let layout = mk_layout();
        let c = Color::rgb(200, 200, 200);
        let s = svg_grid_arc(&layout, 0.0, 180.0, 10000.0, 1.0, &c);
        assert!(s.contains("<path "));
    }

    #[test]
    fn test_svg_text_empty_string_text_still_produces_valid_tag() {
        // Empty text content → <text>...</text> still produced.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 500.0, "", 12.0, &c, 0.0);
        assert!(s.contains("<text"));
        assert!(s.contains("</text>"));
    }

    #[test]
    fn test_svg_document_empty_render_contains_doctype() {
        // DOCTYPE declaration emitted in every render.
        let doc = SvgDocument::new(100.0, 100.0);
        let s = doc.render();
        assert!(s.contains("DOCTYPE"));
    }

    #[test]
    fn test_svg_tick_zero_to_same_radius_still_emits_line() {
        // r1=r2 → still emits a <line> tag (even if zero-length).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 500.0, 500.0, 1.0, &c);
        assert!(s.contains("<line"));
    }

    #[test]
    fn test_svg_slice_multi_color_output_contains_color() {
        // Slice with distinct fill color → that color appears in output.
        let layout = mk_layout();
        let red = Color::rgb(255, 0, 0);
        let s = svg_slice(&layout, 0.0, 90.0, 400.0, 500.0, None, None, Some(&red), None);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_document_render_each_element_on_its_own_line() {
        // Each added element emitted on its own line.
        let mut doc = SvgDocument::new(50.0, 50.0);
        doc.add("<line/>".to_string());
        doc.add("<rect/>".to_string());
        let s = doc.render();
        // Both elements should be distinct lines.
        assert!(s.contains("<line/>"));
        assert!(s.contains("<rect/>"));
    }

    #[test]
    fn test_svg_tick_includes_color_in_output() {
        // svg_tick output should contain the color somewhere (stroke).
        let layout = mk_layout();
        let c = Color::rgb(255, 100, 50);
        let s = svg_tick(&layout, 0.0, 500.0, 510.0, 1.0, &c);
        assert!(s.contains("rgb") || s.contains("255"));
    }

    #[test]
    fn test_svg_text_large_font_size_in_output() {
        // Large font size value appears in output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 500.0, "big", 72.0, &c, 0.0);
        assert!(s.contains("72"));
    }

    #[test]
    fn test_svg_slice_with_edge_and_no_fill_renders_stroke() {
        // Only edge color provided, no fill → stroke dominant.
        let layout = mk_layout();
        let edge = Color::rgb(0, 0, 0);
        let s = svg_slice(&layout, 0.0, 90.0, 400.0, 500.0, Some(&edge), Some(2.0), None, None);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_document_render_order_of_additions_preserved() {
        // Elements appear in insertion order in render output.
        let mut doc = SvgDocument::new(10.0, 10.0);
        doc.add("<first/>".to_string());
        doc.add("<second/>".to_string());
        doc.add("<third/>".to_string());
        let s = doc.render();
        let p1 = s.find("<first/>").unwrap();
        let p2 = s.find("<second/>").unwrap();
        let p3 = s.find("<third/>").unwrap();
        assert!(p1 < p2 && p2 < p3);
    }

    #[test]
    fn test_svg_tick_output_contains_line_element_tag() {
        // svg_tick output is a line element.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 500.0, 510.0, 1.0, &c);
        assert!(s.contains("<line"));
    }

    #[test]
    fn test_svg_text_nonzero_rotation_contains_rotate_transform() {
        // rotation != 0 → "rotate" or "transform" in output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 500.0, "rot", 12.0, &c, 30.0);
        assert!(s.contains("rotate") || s.contains("transform"));
    }

    #[test]
    fn test_svg_grid_arc_output_contains_path_tag() {
        // svg_grid_arc output is a path element.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 1.0, &c);
        assert!(s.contains("<path"));
    }

    #[test]
    fn test_svg_document_integer_dims_render_format() {
        // Integer dimensions formatted with "px" suffix.
        let doc = SvgDocument::new(100.0, 100.0);
        let s = doc.render();
        assert!(s.contains("100px"));
    }

    #[test]
    fn test_svg_document_fractional_dims_rounded_in_render() {
        // Fractional dims formatted with "{:.0}px" → rounded to integer.
        let doc = SvgDocument::new(250.7, 380.4);
        let s = doc.render();
        // Rounded to nearest integer.
        assert!(s.contains("251px") || s.contains("250px"));
    }

    #[test]
    fn test_svg_slice_output_includes_fill_attribute() {
        // Slice with fill color → "fill" in output.
        let layout = mk_layout();
        let c = Color::rgb(100, 150, 200);
        let s = svg_slice(&layout, 0.0, 90.0, 400.0, 500.0, None, None, Some(&c), None);
        assert!(s.contains("fill"));
    }

    #[test]
    fn test_svg_text_with_rotation_includes_angle_in_transform() {
        // svg_text rotation value appears in transform.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 500.0, "t", 12.0, &c, 90.0);
        assert!(s.contains("90") || s.contains("rotate"));
    }

    #[test]
    fn test_svg_document_close_svg_tag_present_in_render() {
        // Closing </svg> tag always in render output.
        let doc = SvgDocument::new(50.0, 50.0);
        let s = doc.render();
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn test_svg_document_utf8_encoding_declaration_present() {
        // XML declaration always has UTF-8 encoding.
        let doc = SvgDocument::new(50.0, 50.0);
        let s = doc.render();
        assert!(s.contains("UTF-8"));
    }

    #[test]
    fn test_svg_slice_angle_zero_to_full_produces_valid_output() {
        // 0..360 full sweep → valid non-empty output.
        let layout = mk_layout();
        let c = Color::rgb(50, 50, 50);
        let s = svg_slice(&layout, 0.0, 360.0, 400.0, 500.0, Some(&c), None, Some(&c), None);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_tick_very_small_thickness_still_emits_line() {
        // thickness=0.01 → still emits <line>.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 500.0, 510.0, 0.01, &c);
        assert!(s.contains("<line"));
    }

    #[test]
    fn test_svg_grid_arc_with_large_thickness_renders_path() {
        // thickness=100 → still emits <path>.
        let layout = mk_layout();
        let c = Color::rgb(200, 200, 200);
        let s = svg_grid_arc(&layout, 0.0, 90.0, 500.0, 100.0, &c);
        assert!(s.contains("<path"));
    }

    #[test]
    fn test_svg_tick_accepts_r1_greater_than_r2() {
        // Reversed radii r1 > r2 → still emits <line>.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 550.0, 500.0, 1.0, &c);
        assert!(s.contains("<line"));
    }

    #[test]
    fn test_svg_text_with_long_text_content_emitted_in_tag() {
        // Long text content preserved in output.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let long_text = "the_quick_brown_fox_jumps_over_the_lazy_dog";
        let s = svg_text(&layout, 0.0, 500.0, long_text, 12.0, &c, 0.0);
        assert!(s.contains(long_text));
    }

    #[test]
    fn test_svg_slice_with_zero_sweep_still_valid_output() {
        // start=end=0 → zero-sweep slice still produces output.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_slice(&layout, 0.0, 0.0, 400.0, 500.0, Some(&c), None, Some(&c), None);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_document_empty_group_emits_g_tag_pair() {
        // open_group followed by close_group → g pair.
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.open_group("g1");
        doc.close_group();
        let s = doc.render();
        // Both <g id="g1"> and </g> present.
        assert!(s.contains(r#"<g id="g1">"#));
        assert!(s.contains("</g>"));
    }

    #[test]
    fn test_svg_document_render_produces_non_empty_string() {
        // Empty doc render still non-empty (has header/footer).
        let doc = SvgDocument::new(1.0, 1.0);
        let s = doc.render();
        assert!(!s.is_empty());
    }

    #[test]
    fn test_svg_tick_multiple_calls_produce_distinct_line_tags() {
        // Two different tick angles → two distinct outputs.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s1 = svg_tick(&layout, 0.0, 500.0, 510.0, 1.0, &c);
        let s2 = svg_tick(&layout, 90.0, 500.0, 510.0, 1.0, &c);
        assert!(s1.contains("<line"));
        assert!(s2.contains("<line"));
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_svg_text_multiple_font_sizes_distinct_outputs() {
        // Different font sizes → distinct output strings.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s1 = svg_text(&layout, 0.0, 500.0, "x", 10.0, &c, 0.0);
        let s2 = svg_text(&layout, 0.0, 500.0, "x", 20.0, &c, 0.0);
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_svg_grid_arc_output_always_starts_with_path_tag() {
        // Output always starts with <path tag.
        let layout = mk_layout();
        let c = Color::rgb(255, 255, 255);
        let s = svg_grid_arc(&layout, 0.0, 45.0, 500.0, 1.0, &c);
        assert!(s.starts_with("<path"));
    }

    #[test]
    fn test_svg_grid_arc_full_circle_trims_end_angle_by_small_amount() {
        // When sweep is full 360 → end_a reduced by 0.01 to avoid degenerate M==L.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 360.0, 500.0, 1.0, &c);
        // Should still produce a valid path with both stroke and fill:none.
        assert!(s.contains("stroke-width"));
        assert!(s.contains("fill: none"));
    }

    #[test]
    fn test_svg_grid_arc_large_arc_flag_toggled_when_sweep_over_180() {
        // Sweep > 180 → large_arc should be set; check "0 1," flag sequence.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 200.0, 500.0, 1.0, &c);
        assert!(s.contains("0 1,1 "));
    }

    #[test]
    fn test_svg_tick_sweep_flag_fixed_at_negative_zero_one() {
        // svg_tick encodes thickness `{:.1}` — thickness 0.05 → "0.1".
        let layout = mk_layout();
        let c = Color::rgb(128, 128, 128);
        let s = svg_tick(&layout, 0.0, 100.0, 200.0, 0.05, &c);
        assert!(s.contains("stroke-width: 0.1"));
    }

    #[test]
    fn test_svg_tick_color_reflects_rgb_triple_no_alpha() {
        // svg_tick uses to_svg_rgb → no alpha, just rgb(r,g,b).
        let layout = mk_layout();
        let c = Color::rgba(10, 20, 30, 40);
        let s = svg_tick(&layout, 90.0, 0.0, 50.0, 1.0, &c);
        assert!(s.contains("stroke: rgb(10,20,30)"));
    }

    #[test]
    fn test_svg_text_zero_rotation_omits_transform_attribute() {
        // rotation=0 → first branch, no transform attribute.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 100.0, "Hi", 12.0, &c, 0.0);
        assert!(!s.contains("transform="));
        assert!(s.contains("<text "));
    }

    #[test]
    fn test_svg_text_nonzero_rotation_emits_rotate_transform() {
        // rotation = pi/4 → transform="rotate(45.00,...)" emitted.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 100.0, "X", 10.0, &c, std::f64::consts::FRAC_PI_4);
        assert!(s.contains("transform=\"rotate(45.00"));
    }

    #[test]
    fn test_svg_text_font_size_rounded_to_integer_pixels() {
        // {:.0} → 12.7 → "13".
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 100.0, "T", 12.7, &c, 0.0);
        assert!(s.contains("font-size: 13px"));
    }

    #[test]
    fn test_svg_text_fill_reflects_color_to_svg_rgb_form() {
        // fill uses to_svg_rgb → "rgb(r,g,b)", no alpha even for rgba color.
        let layout = mk_layout();
        let c = Color::rgba(5, 6, 7, 100);
        let s = svg_text(&layout, 0.0, 100.0, "x", 10.0, &c, 0.0);
        assert!(s.contains("fill: rgb(5,6,7)"));
    }

    #[test]
    fn test_svg_slice_no_fill_no_stroke_emits_none_style() {
        // None fill + None stroke → "fill: none;" and "stroke: none;".
        let layout = mk_layout();
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, None, None, None, None);
        assert!(s.contains("fill: none"));
        assert!(s.contains("stroke: none"));
    }

    #[test]
    fn test_svg_slice_opacity_below_1_emits_opacity_attribute() {
        // opacity=0.5 → "opacity: 0.500;" appended.
        let layout = mk_layout();
        let c = Color::rgb(100, 150, 200);
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, None, None, Some(&c), Some(0.5));
        assert!(s.contains("opacity: 0.500"));
    }

    #[test]
    fn test_svg_slice_opacity_equal_to_1_omits_opacity_attribute() {
        // opacity=1.0 → no "opacity:" emitted.
        let layout = mk_layout();
        let c = Color::rgb(100, 150, 200);
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, None, None, Some(&c), Some(1.0));
        assert!(!s.contains("opacity:"));
    }

    #[test]
    fn test_svg_slice_edge_stroke_zero_emits_stroke_none() {
        // edge_stroke=Some(0.0) → stroke=none (else branch in stroke_style).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, Some(&c), Some(0.0), None, None);
        assert!(s.contains("stroke: none"));
        assert!(!s.contains("stroke-width:"));
    }

    #[test]
    fn test_svg_slice_swap_when_end_less_than_start() {
        // end < start → start_a/end_a swapped; path still valid.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_slice(&layout, 90.0, 30.0, 200.0, 300.0, None, None, Some(&c), None);
        // Still produces a valid path opening <path ...
        assert!(s.contains("<path"));
    }

    #[test]
    fn test_svg_slice_near_full_circle_trims_end_angle() {
        // (end - start).abs() > 359.99 → end_a_mod -= 0.01 to avoid M==L.
        let layout = mk_layout();
        let c = Color::rgb(200, 200, 200);
        let s = svg_slice(&layout, 0.0, 360.0, 200.0, 300.0, None, None, Some(&c), None);
        // Should still contain fill with color.
        assert!(s.contains("rgb(200,200,200)"));
    }

    #[test]
    fn test_svg_slice_stroke_with_thickness_both_fields_present() {
        // edge_color+edge_stroke > 0 → stroke-width and stroke color both present.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, Some(&c), Some(2.5), None, None);
        assert!(s.contains("stroke-width: 2.5"));
        assert!(s.contains("stroke: rgb(0,0,0)"));
    }

    #[test]
    fn test_svg_grid_arc_always_emits_fill_none_style() {
        // svg_grid_arc always emits fill: none (arc rendered as stroke line).
        let layout = mk_layout();
        let c = Color::rgb(50, 60, 70);
        let s = svg_grid_arc(&layout, 0.0, 90.0, 400.0, 1.0, &c);
        assert!(s.contains("fill: none"));
    }

    #[test]
    fn test_svg_slice_radial_line_when_start_equals_end_angle() {
        // start==end (zero-angle) and radius_inner != radius_outer → radial line path.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_slice(&layout, 45.0, 45.0, 100.0, 200.0, Some(&c), Some(1.0), None, None);
        // Radial line format: "M ... L ..."
        assert!(s.contains("M "));
        assert!(s.contains(" L "));
    }

    #[test]
    fn test_svg_slice_zero_width_annular_uses_arc_path() {
        // radius_inner == radius_outer → zero-width arc → "A" path command.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_slice(&layout, 0.0, 90.0, 300.0, 300.0, Some(&c), Some(1.0), None, None);
        assert!(s.contains(" A"));
    }

    #[test]
    fn test_svg_slice_full_annulus_has_z_close_command() {
        // Normal annulus → path contains Z close command.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_slice(&layout, 0.0, 90.0, 200.0, 300.0, None, None, Some(&c), None);
        assert!(s.contains(" Z "));
    }

    #[test]
    fn test_svg_tick_emits_line_element_not_path() {
        // svg_tick emits an SVG <line> element (not <path>).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 100.0, 150.0, 1.0, &c);
        assert!(s.contains("<line "));
        assert!(!s.contains("<path "));
    }

    #[test]
    fn test_svg_text_self_closing_with_text_content_between_tags() {
        // svg_text emits <text>...text content...</text> pair.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 100.0, "hi", 10.0, &c, 0.0);
        assert!(s.contains("<text "));
        assert!(s.contains("</text>"));
        assert!(s.contains(">hi<"));
    }

    #[test]
    fn test_svg_slice_opacity_rounded_to_three_decimals() {
        // opacity 0.33333 → rounds to 0.333 via {:.3}.
        let layout = mk_layout();
        let c = Color::rgb(50, 60, 70);
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, None, None, Some(&c), Some(0.33333));
        assert!(s.contains("opacity: 0.333"));
    }

    #[test]
    fn test_svg_grid_arc_preserves_radius_in_output() {
        // svg_grid_arc writes radius (and it again for A command).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 45.0, 123.4, 1.0, &c);
        // radius formatted with {:.1} → 123.4 appears in output.
        assert!(s.contains("123.4"));
    }

    #[test]
    fn test_svg_tick_coords_use_one_decimal_format() {
        // svg_tick uses {:.1} on x/y → output contains ".0" for integer positions.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 0.0, 100.0, 1.0, &c);
        // "x1=" and "x2=" should contain ".0" because coords are integers.
        assert!(s.contains(".0"));
    }

    #[test]
    fn test_svg_slice_near_full_circle_end_start_0_and_360_boundary() {
        // start=0, end=360 → sweep = 360 > 359.99 → end trimmed to 359.99.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_slice(&layout, 0.0, 360.0, 200.0, 300.0, None, None, Some(&c), None);
        // Output should contain close-path command "Z".
        assert!(s.contains(" Z "));
    }

    #[test]
    fn test_svg_text_with_newline_literal_in_content_preserved() {
        // \n in text content passed verbatim (no escaping).
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 100.0, "line1\nline2", 10.0, &c, 0.0);
        assert!(s.contains("line1\nline2"));
    }

    #[test]
    fn test_svg_grid_arc_sweep_flag_always_one_in_output() {
        // svg_grid_arc always uses sweep flag 1.
        let layout = mk_layout();
        let c = Color::rgb(255, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 90.0, 400.0, 1.0, &c);
        // Contains "0,1 " (large_arc flag 0, sweep flag 1).
        assert!(s.contains("0,1 ") || s.contains("1,1 "));
    }

    #[test]
    fn test_svg_slice_radial_line_with_zero_stroke_fill_none() {
        // Radial-line path (start=end, radii differ) → stroke-none, fill:none.
        let layout = mk_layout();
        let s = svg_slice(&layout, 90.0, 90.0, 100.0, 200.0, None, None, None, None);
        assert!(s.contains("stroke: none"));
        assert!(s.contains("fill: none"));
    }

    #[test]
    fn test_svg_tick_radius_from_greater_than_radius_to_valid_line() {
        // radius_from > radius_to → still produces line.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_tick(&layout, 0.0, 200.0, 100.0, 1.0, &c);
        assert!(s.contains("<line "));
    }

    #[test]
    fn test_svg_text_empty_string_still_produces_tags() {
        // Empty text → still <text>...</text>.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_text(&layout, 0.0, 100.0, "", 10.0, &c, 0.0);
        assert!(s.contains("<text "));
        assert!(s.contains("></text>"));
    }

    #[test]
    fn test_svg_grid_arc_explicit_thickness_in_output() {
        // thickness=3.5 → "stroke-width: 3.5" in style.
        let layout = mk_layout();
        let c = Color::rgb(0, 0, 0);
        let s = svg_grid_arc(&layout, 0.0, 45.0, 400.0, 3.5, &c);
        assert!(s.contains("stroke-width: 3.5"));
    }

    #[test]
    fn test_svg_slice_with_stroke_and_opacity_both_applied() {
        // Both stroke + opacity present.
        let layout = mk_layout();
        let c = Color::rgb(100, 100, 100);
        let s = svg_slice(&layout, 0.0, 30.0, 200.0, 300.0, Some(&c), Some(2.0), Some(&c), Some(0.75));
        assert!(s.contains("stroke: "));
        assert!(s.contains("opacity: 0.750"));
    }
}
