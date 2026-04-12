use std::collections::HashMap;
use std::fmt::Write;

use crate::config::types::ConfigValue;
use crate::data::types::Datum;
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::{svg_slice, svg_text, SvgDocument};

/// Draw all plot types from a <plot> config block.
pub fn draw_plot(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
) {
    let plot_type = match block_conf.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return,
    };

    let r0 = parse_radius(
        block_conf.get("r0").and_then(|v| v.as_str()).unwrap_or("0.5r"),
        layout,
    );
    let r1 = parse_radius(
        block_conf.get("r1").and_then(|v| v.as_str()).unwrap_or("0.8r"),
        layout,
    );

    match plot_type {
        "histogram" => draw_histogram(doc, layout, data, block_conf, colors, r0, r1),
        "heatmap" => draw_heatmap(doc, layout, data, block_conf, colors, r0, r1),
        "scatter" => draw_scatter(doc, layout, data, block_conf, colors, r0, r1),
        "line" => draw_line(doc, layout, data, block_conf, colors, r0, r1),
        "text" => draw_text_track(doc, layout, data, block_conf, colors, r0, r1),
        "tile" => draw_tile(doc, layout, data, block_conf, colors, r0, r1),
        "connector" => draw_connector(doc, layout, data, block_conf, colors, r0, r1),
        "highlight" => draw_highlight_plot(doc, layout, data, block_conf, colors, r0, r1),
        _ => {}
    }
}

/// Draw histogram bars as arc slices.
fn draw_histogram(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let fill_color_name = block_conf
        .get("fill_color")
        .and_then(|v| v.as_str())
        .unwrap_or(color_name);
    let default_color = colors.resolve(fill_color_name).unwrap_or(Color::rgb(0, 0, 0));

    // Find data range for normalization
    let (min_val, max_val) = data_range(data);
    let range = max_val - min_val;

    for datum in data {
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

        let value = datum.value.unwrap_or(0.0);
        let fill_color = datum
            .param
            .get("fill_color")
            .or(datum.param.get("color"))
            .and_then(|n| colors.resolve(n))
            .unwrap_or(default_color);

        // Normalize value to radius range
        let normalized = if range > 0.0 {
            (value - min_val) / range
        } else {
            0.5
        };
        let bar_r = r0 + normalized * (r1 - r0);

        let svg = svg_slice(
            layout,
            start_a,
            end_a,
            r0,
            bar_r,
            None,
            None,
            Some(&fill_color),
            None,
        );
        doc.add(svg);
    }
}

/// Draw heatmap as colored arc segments.
fn draw_heatmap(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    _block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let (min_val, max_val) = data_range(data);
    let range = max_val - min_val;

    for datum in data {
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

        let value = datum.value.unwrap_or(0.0);
        let normalized = if range > 0.0 {
            ((value - min_val) / range).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // Map to color: blue -> white -> red gradient
        let fill_color = datum
            .param
            .get("color")
            .and_then(|n| colors.resolve(n))
            .unwrap_or_else(|| value_to_color(normalized));

        let svg = svg_slice(
            layout,
            start_a,
            end_a,
            r0,
            r1,
            None,
            None,
            Some(&fill_color),
            None,
        );
        doc.add(svg);
    }
}

/// Draw scatter plot as small circles.
fn draw_scatter(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let glyph_size: f64 = block_conf
        .get("glyph_size")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(5.0);
    let color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let default_color = colors.resolve(color_name).unwrap_or(Color::rgb(0, 0, 0));

    let (min_val, max_val) = data_range(data);
    let range = max_val - min_val;

    for datum in data {
        if layout.find_ideogram_by_chr(&datum.chr).is_none() {
            continue;
        }
        let mid = (datum.start + datum.end) / 2;
        let angle = match layout.get_angle(mid, &datum.chr) {
            Some(a) => a,
            None => continue,
        };

        let value = datum.value.unwrap_or(0.0);
        let normalized = if range > 0.0 {
            (value - min_val) / range
        } else {
            0.5
        };
        let radius = r0 + normalized * (r1 - r0);
        let (x, y) = layout.get_xy(angle, radius);

        let color = datum
            .param
            .get("color")
            .and_then(|n| colors.resolve(n))
            .unwrap_or(default_color);

        doc.add(format!(
            r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" style="fill: {};" />"#,
            x, y, glyph_size, color.to_svg_rgb()
        ));
    }
}

/// Draw line plot connecting data points.
fn draw_line(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let thickness: f64 = block_conf
        .get("thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(2.0);
    let color = colors.resolve(color_name).unwrap_or(Color::rgb(0, 0, 0));

    let (min_val, max_val) = data_range(data);
    let range = max_val - min_val;

    // Group by chromosome
    let mut by_chr: HashMap<String, Vec<&Datum>> = HashMap::new();
    for datum in data {
        by_chr.entry(datum.chr.clone()).or_default().push(datum);
    }

    for (chr, points) in &by_chr {
        if layout.find_ideogram_by_chr(chr).is_none() {
            continue;
        }
        let mut path_d = String::new();
        let mut first = true;

        for datum in points {
            let mid = (datum.start + datum.end) / 2;
            let angle = match layout.get_angle(mid, chr) {
                Some(a) => a,
                None => continue,
            };
            let value = datum.value.unwrap_or(0.0);
            let normalized = if range > 0.0 {
                (value - min_val) / range
            } else {
                0.5
            };
            let radius = r0 + normalized * (r1 - r0);
            let (x, y) = layout.get_xy(angle, radius);

            if first {
                write!(path_d, "M {:.1},{:.1}", x, y).unwrap();
                first = false;
            } else {
                write!(path_d, " L {:.1},{:.1}", x, y).unwrap();
            }
        }

        if !path_d.is_empty() {
            doc.add(format!(
                r#"<path d="{}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
                path_d,
                color.to_svg_rgb(),
                thickness
            ));
        }
    }
}

/// Draw text labels at genomic positions.
fn draw_text_track(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let label_size: f64 = block_conf
        .get("label_size")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(12.0);
    let color = colors.resolve(color_name).unwrap_or(Color::rgb(0, 0, 0));

    for datum in data {
        if layout.find_ideogram_by_chr(&datum.chr).is_none() {
            continue;
        }
        let mid = (datum.start + datum.end) / 2;
        let angle = match layout.get_angle(mid, &datum.chr) {
            Some(a) => a,
            None => continue,
        };

        let label = datum
            .label
            .as_deref()
            .or(datum.param.get("label").map(|s| s.as_str()))
            .unwrap_or("");

        let radius = (r0 + r1) / 2.0;
        let text_svg = svg_text(layout, angle, radius, label, label_size, &color, 0.0);
        doc.add(text_svg);
    }
}

/// Draw tile tracks (stacked rectangles).
fn draw_tile(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let default_color = colors.resolve(color_name).unwrap_or(Color::rgb(0, 0, 0));
    let thickness: f64 = block_conf
        .get("thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(10.0);

    // Simple stacking: assign each tile to a track
    let num_tracks = ((r1 - r0) / (thickness + 2.0)).max(1.0) as usize;
    let mut track_ends: Vec<HashMap<String, i64>> = vec![HashMap::new(); num_tracks];

    for datum in data {
        if layout.find_ideogram_by_chr(&datum.chr).is_none() {
            continue;
        }

        // Find first available track
        let track = (0..num_tracks)
            .find(|&t| {
                let end = track_ends[t].get(&datum.chr).copied().unwrap_or(0);
                datum.start > end
            })
            .unwrap_or(0);

        track_ends[track].insert(datum.chr.clone(), datum.end);

        let start_a = match layout.get_angle(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.get_angle(datum.end, &datum.chr) {
            Some(a) => a,
            None => continue,
        };

        let tile_r0 = r0 + track as f64 * (thickness + 2.0);
        let tile_r1 = tile_r0 + thickness;

        let fill_color = datum
            .param
            .get("color")
            .and_then(|n| colors.resolve(n))
            .unwrap_or(default_color);

        let svg = svg_slice(
            layout,
            start_a,
            end_a,
            tile_r0,
            tile_r1,
            None,
            None,
            Some(&fill_color),
            None,
        );
        doc.add(svg);
    }
}

/// Draw connector lines between data points.
fn draw_connector(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let color_name = block_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let thickness: f64 = block_conf
        .get("thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(1.0);
    let color = colors.resolve(color_name).unwrap_or(Color::rgb(0, 0, 0));

    for datum in data {
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

        // Draw connector: radial line from r0 at start to r1 at end
        let (x1, y1) = layout.get_xy(start_a, r0);
        let (x2, y2) = layout.get_xy(start_a, r1);
        let (x3, y3) = layout.get_xy(end_a, r1);
        let (x4, y4) = layout.get_xy(end_a, r0);

        doc.add(format!(
            r#"<path d="M {:.1},{:.1} L {:.1},{:.1} L {:.1},{:.1} L {:.1},{:.1}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
            x1, y1, x2, y2, x3, y3, x4, y4,
            color.to_svg_rgb(), thickness
        ));
    }
}

/// Draw highlight-type plot regions.
fn draw_highlight_plot(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    r0: f64,
    r1: f64,
) {
    let color_name = block_conf
        .get("fill_color")
        .or(block_conf.get("color"))
        .and_then(|v| v.as_str())
        .unwrap_or("red");
    let default_color = colors.resolve(color_name).unwrap_or(Color::rgb(255, 0, 0));

    for datum in data {
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

        let fill_color = datum
            .param
            .get("fill_color")
            .or(datum.param.get("color"))
            .and_then(|n| colors.resolve(n))
            .unwrap_or(default_color);

        let svg = svg_slice(
            layout,
            start_a,
            end_a,
            r0,
            r1,
            None,
            None,
            Some(&fill_color),
            None,
        );
        doc.add(svg);
    }
}

/// Compute min/max values from data.
fn data_range(data: &[Datum]) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for datum in data {
        if let Some(v) = datum.value {
            if v < min { min = v; }
            if v > max { max = v; }
        }
    }
    if min > max {
        (0.0, 1.0)
    } else {
        (min, max)
    }
}

/// Map a normalized value [0, 1] to a blue-white-red color.
fn value_to_color(t: f64) -> Color {
    if t < 0.5 {
        let f = t * 2.0;
        Color::rgb(
            (f * 255.0) as u8,
            (f * 255.0) as u8,
            255,
        )
    } else {
        let f = (t - 0.5) * 2.0;
        Color::rgb(
            255,
            ((1.0 - f) * 255.0) as u8,
            ((1.0 - f) * 255.0) as u8,
        )
    }
}

fn parse_radius(s: &str, layout: &Layout) -> f64 {
    let s = s.trim();
    // Handle expressions like "1r+200p"
    if s.contains('+') {
        let parts: Vec<&str> = s.split('+').collect();
        return parts.iter().map(|p| parse_radius_simple(p.trim(), layout)).sum();
    }
    if s.contains('-') && !s.starts_with('-') {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        let a = parse_radius_simple(parts[0].trim(), layout);
        let b = parse_radius_simple(parts[1].trim(), layout);
        return a - b;
    }
    parse_radius_simple(s, layout)
}

fn parse_radius_simple(s: &str, layout: &Layout) -> f64 {
    if s.ends_with('r') {
        let val: f64 = s.trim_end_matches('r').parse().unwrap_or(0.0);
        val * layout.dims.ideogram_radius
    } else if s.ends_with('p') {
        s.trim_end_matches('p').parse().unwrap_or(0.0)
    } else {
        s.parse().unwrap_or(0.0)
    }
}
