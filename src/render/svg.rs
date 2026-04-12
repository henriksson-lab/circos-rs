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
        let large_arc = if (start_a - end_a_mod).abs() > 180.0 { 1 } else { 0 };
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
        let sweep_large = if (start_a - end_a_mod).abs() > 180.0 { 1 } else { 0 };

        let (ox1, oy1) = polar_to_xy(cx, cy, radius_outer, start_a, deg2rad);
        let (ox2, oy2) = polar_to_xy(cx, cy, radius_outer, end_a_mod, deg2rad);
        let (ix1, iy1) = polar_to_xy(cx, cy, radius_inner, end_a_mod, deg2rad);
        let (ix2, iy2) = polar_to_xy(cx, cy, radius_inner, start_a, deg2rad);

        format!(
            r#"<path d="M {:.3},{:.3} A{:.3},{:.3} {:.3} {},{} {:.3},{:.3} L {:.3},{:.3} A{:.3},{:.3} {:.3} {},{} {:.3},{:.3} Z " style="{} {} {}{}" />"#,
            ox1, oy1,
            radius_outer, radius_outer, 0.0, sweep_large, 1, ox2, oy2,
            ix1, iy1,
            radius_inner, radius_inner, 0.0, sweep_large, 0, ix2, iy2,
            stroke_style, fill_style, opacity_style,
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
    let (x, y) = layout.get_xy(angle, radius);
    if rotation.abs() < 0.01 {
        format!(
            r#"<text x="{:.1}" y="{:.1}" style="font-size: {:.0}px; fill: {};">{}</text>"#,
            x, y, font_size, color.to_svg_rgb(), text
        )
    } else {
        format!(
            r#"<text x="{:.1}" y="{:.1}" transform="rotate({:.2},{:.1},{:.1})" style="font-size: {:.0}px; fill: {};">{}</text>"#,
            x, y, rotation.to_degrees(), x, y, font_size, color.to_svg_rgb(), text
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
    let (x1, y1) = layout.get_xy(angle, radius_from);
    let (x2, y2) = layout.get_xy(angle, radius_to);
    format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" style="stroke: {}; stroke-width: {:.1};" />"#,
        x1, y1, x2, y2, color.to_svg_rgb(), thickness
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
    let large_arc = if (angle_start - end_a).abs() > 180.0 { 1 } else { 0 };

    format!(
        r#"<path d="M {:.1},{:.1} A{:.1},{:.1} 0 {},{} {:.1},{:.1}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
        x1, y1, radius, radius, large_arc, 1, x2, y2,
        color.to_svg_rgb(), thickness
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
}
