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
            let color = colors.resolve(color_name).unwrap_or(Color::rgb(200, 200, 200));

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

                let angle1 = match layout.get_angle(mid1, &p1.chr) {
                    Some(a) => a,
                    None => continue,
                };
                let angle2 = match layout.get_angle(mid2, &p2.chr) {
                    Some(a) => a,
                    None => continue,
                };

                if is_ribbon {
                    let a1_start = layout.get_angle(p1.start, &p1.chr).unwrap_or(angle1);
                    let a1_end = layout.get_angle(p1.end, &p1.chr).unwrap_or(angle1);
                    let a2_start = layout.get_angle(p2.start, &p2.chr).unwrap_or(angle2);
                    let a2_end = layout.get_angle(p2.end, &p2.chr).unwrap_or(angle2);

                    elements.push(draw_ribbon(
                        layout,
                        a1_start, a1_end, a2_start, a2_end,
                        default_radius, default_bezier_radius, default_crest,
                        &color, thickness,
                    ));
                } else {
                    elements.push(draw_bezier_link(
                        layout,
                        angle1, angle2,
                        default_radius, default_bezier_radius, default_crest,
                        &color, thickness,
                    ));
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
fn draw_bezier_link(
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

    let (p0, p1, p2, p3) = bezier::link_control_points(
        cx, cy, angle1, radius, angle2, radius, crest, bezier_radius,
    );

    format!(
        r#"<path d="M {:.1},{:.1} C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
        p0.0, p0.1, p1.0, p1.1, p2.0, p2.1, p3.0, p3.1,
        color.to_svg_rgb(), thickness
    )
}

/// Draw a ribbon (filled bezier path between two arcs).
fn draw_ribbon(
    layout: &Layout,
    a1_start: f64,
    a1_end: f64,
    a2_start: f64,
    a2_end: f64,
    radius: f64,
    bezier_radius: f64,
    crest: f64,
    color: &Color,
    thickness: f64,
) -> String {
    let cx = layout.image_radius;
    let cy = layout.image_radius;
    let deg2rad = std::f64::consts::PI / 180.0;

    let p_a1s = (cx + radius * (a1_start * deg2rad).cos(), cy + radius * (a1_start * deg2rad).sin());
    let p_a1e = (cx + radius * (a1_end * deg2rad).cos(), cy + radius * (a1_end * deg2rad).sin());
    let p_a2s = (cx + radius * (a2_start * deg2rad).cos(), cy + radius * (a2_start * deg2rad).sin());
    let p_a2e = (cx + radius * (a2_end * deg2rad).cos(), cy + radius * (a2_end * deg2rad).sin());

    let (_, c1_1, c1_2, _) = bezier::link_control_points(cx, cy, a1_end, radius, a2_start, radius, crest, bezier_radius);
    let (_, c2_1, c2_2, _) = bezier::link_control_points(cx, cy, a2_end, radius, a1_start, radius, crest, bezier_radius);

    let sweep1 = arc_sweep_flag(a1_start, a1_end);
    let large1 = arc_large_flag(a1_start, a1_end);
    let sweep2 = arc_sweep_flag(a2_start, a2_end);
    let large2 = arc_large_flag(a2_start, a2_end);

    let mut path = String::new();
    write!(path, "M {:.1},{:.1} ", p_a1s.0, p_a1s.1).unwrap();
    write!(path, "A {:.1},{:.1} 0 {},{} {:.1},{:.1} ", radius, radius, large1, sweep1, p_a1e.0, p_a1e.1).unwrap();
    write!(path, "C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} ", c1_1.0, c1_1.1, c1_2.0, c1_2.1, p_a2s.0, p_a2s.1).unwrap();
    write!(path, "A {:.1},{:.1} 0 {},{} {:.1},{:.1} ", radius, radius, large2, sweep2, p_a2e.0, p_a2e.1).unwrap();
    write!(path, "C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} ", c2_1.0, c2_1.1, c2_2.0, c2_2.1, p_a1s.0, p_a1s.1).unwrap();
    write!(path, "Z").unwrap();

    let stroke_style = if thickness > 0.0 {
        format!("stroke: {}; stroke-width: {:.1};", color.to_svg_rgb(), thickness)
    } else {
        "stroke: none;".to_string()
    };

    format!(
        r#"<path d="{}" style="{} fill: {}; opacity: 0.5;" />"#,
        path, stroke_style, color.to_svg_rgb()
    )
}

fn arc_sweep_flag(start_a: f64, end_a: f64) -> i32 {
    let mut diff = end_a - start_a;
    if diff < 0.0 { diff += 360.0; }
    if diff > 0.0 && diff < 360.0 { 1 } else { 0 }
}

fn arc_large_flag(start_a: f64, end_a: f64) -> i32 {
    let mut span = end_a - start_a;
    if span < 0.0 { span += 360.0; }
    if span > 180.0 { 1 } else { 0 }
}

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
