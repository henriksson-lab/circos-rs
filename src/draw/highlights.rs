use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::data::types::Datum;
use crate::layout::Layout;
use crate::render::color::ColorMap;
use crate::render::svg::{svg_slice, SvgDocument};

/// Draw highlight regions.
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

    let r0 = parse_radius(r0_str, layout);
    let r1 = parse_radius(r1_str, layout);

    let default_fill_name = block_conf
        .get("fill_color")
        .and_then(|v| v.as_str())
        .unwrap_or("red");
    let stroke_color_name = block_conf
        .get("stroke_color")
        .and_then(|v| v.as_str());
    let stroke_thickness: f64 = block_conf
        .get("stroke_thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    doc.open_group("highlights");

    for datum in data {
        // Check if this chromosome is displayed
        if layout.find_ideogram_by_chr(&datum.chr).is_none() {
            continue;
        }

        let start_a = match layout.get_angle(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.get_angle(datum.end, &datum.chr) {
            Some(a) => a,
            None => continue,
        };

        // Per-datum overrides
        let fill_name = datum
            .param
            .get("fill_color")
            .or(datum.param.get("color"))
            .map(|s| s.as_str())
            .unwrap_or(default_fill_name);

        let fill_color = colors.resolve(fill_name);
        let edge_color = stroke_color_name.and_then(|n| colors.resolve(n));

        let svg = svg_slice(
            layout,
            start_a,
            end_a,
            r0.min(r1),
            r0.max(r1),
            edge_color.as_ref(),
            if stroke_thickness > 0.0 { Some(stroke_thickness) } else { None },
            fill_color.as_ref(),
            None,
        );
        doc.add(svg);
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
