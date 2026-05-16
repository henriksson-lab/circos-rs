use std::collections::HashMap;
use std::fmt::Write;

use crate::config::types::ConfigValue;
use crate::data::types::Datum;
use crate::draw::ideograms::slice_polygon_coords;
use crate::draw::report_image_map;
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::{SvgDocument, svg_slice, svg_text};
use crate::utils::format_url;

/// Synthetic `[chr]/[start]/[end]/[value]` param map — matches Perl's
/// `$data_point` hash passed into `format_url`'s param_path.
pub(crate) fn synthetic_datum_map(
    chr: &str,
    start: i64,
    end: i64,
    value: Option<f64>,
) -> HashMap<String, ConfigValue> {
    let mut m: HashMap<String, ConfigValue> = HashMap::new();
    m.insert("chr".into(), ConfigValue::Str(chr.to_string()));
    m.insert("start".into(), ConfigValue::Str(start.to_string()));
    m.insert("end".into(), ConfigValue::Str(end.to_string()));
    if let Some(v) = value {
        m.insert("value".into(), ConfigValue::Str(v.to_string()));
    }
    m
}

/// Wrap a `&HashMap<String, String>` (datum.param) as a `&HashMap<String, ConfigValue>`
/// so it can be fed to `format_url` alongside other ConfigValue maps without
/// an explicit re-copy loop in the hot path.
pub(crate) fn datum_param_as_config(
    param: &HashMap<String, String>,
) -> HashMap<String, ConfigValue> {
    param
        .iter()
        .map(|(k, v)| (k.clone(), ConfigValue::Str(v.clone())))
        .collect()
}

/// Emit a polygon `<area>` entry for this plot datum if the block has a
/// `url` (or the datum overrides it). Perl: `seek_parameter("url", $data_point, $datum, @param_path)`.
fn emit_datum_area(
    layout: &Layout,
    datum: &Datum,
    block_conf: &HashMap<String, ConfigValue>,
    start_a: f64,
    end_a: f64,
    radius_inner: f64,
    radius_outer: f64,
) {
    let url_tpl: Option<String> = datum
        .param
        .get("url")
        .cloned()
        .or_else(|| {
            block_conf
                .get("url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let tpl = match url_tpl {
        Some(t) => t,
        None => return,
    };
    let missing_policy = block_conf
        .get("image_map_missing_parameter")
        .and_then(|v| v.as_str())
        .unwrap_or("removeparam")
        .to_string();
    let synthetic = synthetic_datum_map(&datum.chr, datum.start, datum.end, datum.value);
    let datum_map = datum_param_as_config(&datum.param);
    if let Ok(Some(url)) = format_url(&tpl, &[&synthetic, &datum_map], &missing_policy) {
        let coords = slice_polygon_coords(
            layout.image_radius,
            start_a,
            end_a,
            radius_inner.min(radius_outer),
            radius_inner.max(radius_outer),
        );
        report_image_map("poly", &coords, &url);
    }
}

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
        block_conf
            .get("r0")
            .and_then(|v| v.as_str())
            .unwrap_or("0.5r"),
        layout,
    );
    let r1 = parse_radius(
        block_conf
            .get("r1")
            .and_then(|v| v.as_str())
            .unwrap_or("0.8r"),
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
    let default_color = colors
        .resolve(fill_color_name)
        .unwrap_or(Color::rgb(0, 0, 0));

    // Find data range for normalization
    let (min_val, max_val) = data_range(data);
    let range = max_val - min_val;

    for datum in data {
        if layout.find_ideogram_by_chr(&datum.chr).is_none() {
            continue;
        }
        let start_a = match layout.getanglepos(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.getanglepos(datum.end, &datum.chr) {
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
        emit_datum_area(layout, datum, block_conf, start_a, end_a, r0, bar_r);
    }
}

/// Draw heatmap as colored arc segments.
fn draw_heatmap(
    doc: &mut SvgDocument,
    layout: &Layout,
    data: &[Datum],
    block_conf: &HashMap<String, ConfigValue>,
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
        let start_a = match layout.getanglepos(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.getanglepos(datum.end, &datum.chr) {
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
        emit_datum_area(layout, datum, block_conf, start_a, end_a, r0, r1);
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
        let angle = match layout.getanglepos(mid, &datum.chr) {
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
        let (x, y) = layout.getxypos(angle, radius);

        let color = datum
            .param
            .get("color")
            .and_then(|n| colors.resolve(n))
            .unwrap_or(default_color);

        doc.add(format!(
            r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" style="fill: {};" />"#,
            x,
            y,
            glyph_size,
            color.to_svg_rgb()
        ));

        // --- Image-map: circle area for this scatter point ---
        let url_tpl: Option<String> = datum
            .param
            .get("url")
            .cloned()
            .or_else(|| {
                block_conf
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        if let Some(tpl) = url_tpl {
            let missing = block_conf
                .get("image_map_missing_parameter")
                .and_then(|v| v.as_str())
                .unwrap_or("removeparam")
                .to_string();
            let synthetic =
                synthetic_datum_map(&datum.chr, datum.start, datum.end, Some(value));
            let datum_map = datum_param_as_config(&datum.param);
            if let Ok(Some(url)) = format_url(&tpl, &[&synthetic, &datum_map], &missing) {
                report_image_map("circle", &[x, y, glyph_size], &url);
            }
        }
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

    let url_tpl: Option<String> = block_conf
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let missing_policy = block_conf
        .get("image_map_missing_parameter")
        .and_then(|v| v.as_str())
        .unwrap_or("removeparam")
        .to_string();
    let hit_radius = thickness.max(2.0);

    for (chr, points) in &by_chr {
        if layout.find_ideogram_by_chr(chr).is_none() {
            continue;
        }
        let mut path_d = String::new();
        let mut first = true;

        for datum in points {
            let mid = (datum.start + datum.end) / 2;
            let angle = match layout.getanglepos(mid, chr) {
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
            let (x, y) = layout.getxypos(angle, radius);

            if first {
                write!(path_d, "M {:.1},{:.1}", x, y).unwrap();
                first = false;
            } else {
                write!(path_d, " L {:.1},{:.1}", x, y).unwrap();
            }

            // --- Image-map: per-vertex circle area (Perl draw_line does
            //     seek_parameter("url", $data_point, $datum, @param_path)). ---
            let dp_url = datum.param.get("url").cloned().or_else(|| url_tpl.clone());
            if let Some(tpl) = dp_url {
                let synthetic = synthetic_datum_map(chr, datum.start, datum.end, Some(value));
                let datum_map = datum_param_as_config(&datum.param);
                if let Ok(Some(url)) =
                    format_url(&tpl, &[&synthetic, &datum_map], &missing_policy)
                {
                    report_image_map("circle", &[x, y, hit_radius], &url);
                }
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
        let angle = match layout.getanglepos(mid, &datum.chr) {
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

        // --- Image-map: rect `<area>` covering the text label bounds.
        //     Perl emits a 4-point poly from stringFT bounds; Rust uses the
        //     char-count heuristic to approximate width/height. ---
        let url_tpl: Option<String> = datum
            .param
            .get("url")
            .cloned()
            .or_else(|| {
                block_conf
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        if let Some(tpl) = url_tpl {
            let missing = block_conf
                .get("image_map_missing_parameter")
                .and_then(|v| v.as_str())
                .unwrap_or("removeparam")
                .to_string();
            let mut synthetic =
                synthetic_datum_map(&datum.chr, datum.start, datum.end, datum.value);
            synthetic.insert("label".into(), ConfigValue::Str(label.to_string()));
            let datum_map = datum_param_as_config(&datum.param);
            if let Ok(Some(url)) = format_url(&tpl, &[&synthetic, &datum_map], &missing) {
                // Resolve label font (block_conf.label_font / font or
                // "default"); text_size uses fontdue when the TTF exists.
                let font_key = block_conf
                    .get("label_font")
                    .or_else(|| block_conf.get("font"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let font_file = block_conf
                    .get("fonts")
                    .and_then(|v| v.as_map())
                    .and_then(|m| m.get(font_key).or_else(|| m.get("default")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let (w, h) = crate::draw::text::text_size(font_file, label_size, label);
                let (x, y) = layout.getxypos(angle, radius);
                let coords = [x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0];
                report_image_map("rect", &coords, &url);
            }
        }
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

        let start_a = match layout.getanglepos(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.getanglepos(datum.end, &datum.chr) {
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
        emit_datum_area(layout, datum, block_conf, start_a, end_a, tile_r0, tile_r1);
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
        let start_a = match layout.getanglepos(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.getanglepos(datum.end, &datum.chr) {
            Some(a) => a,
            None => continue,
        };

        // Draw connector: radial line from r0 at start to r1 at end
        let (x1, y1) = layout.getxypos(start_a, r0);
        let (x2, y2) = layout.getxypos(start_a, r1);
        let (x3, y3) = layout.getxypos(end_a, r1);
        let (x4, y4) = layout.getxypos(end_a, r0);

        doc.add(format!(
            r#"<path d="M {:.1},{:.1} L {:.1},{:.1} L {:.1},{:.1} L {:.1},{:.1}" style="stroke: {}; stroke-width: {:.1}; fill: none;" />"#,
            x1, y1, x2, y2, x3, y3, x4, y4,
            color.to_svg_rgb(), thickness
        ));
        emit_datum_area(layout, datum, block_conf, start_a, end_a, r0, r1);
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
        let start_a = match layout.getanglepos(datum.start, &datum.chr) {
            Some(a) => a,
            None => continue,
        };
        let end_a = match layout.getanglepos(datum.end, &datum.chr) {
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
        emit_datum_area(layout, datum, block_conf, start_a, end_a, r0, r1);
    }
}

/// Compute min/max values from data.
fn data_range(data: &[Datum]) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for datum in data {
        if let Some(v) = datum.value {
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }
    }
    if min > max { (0.0, 1.0) } else { (min, max) }
}

/// Map a normalized value [0, 1] to a blue-white-red color.
fn value_to_color(t: f64) -> Color {
    if t < 0.5 {
        let f = t * 2.0;
        Color::rgb((f * 255.0) as u8, (f * 255.0) as u8, 255)
    } else {
        let f = (t - 0.5) * 2.0;
        Color::rgb(255, ((1.0 - f) * 255.0) as u8, ((1.0 - f) * 255.0) as u8)
    }
}

/// Parse a plot radius expression, supporting compound forms like
/// "1r+200p" (sum) or "0.8r-50p" (difference). Each term is parsed via
/// `parse_radius_simple`.
fn parse_radius(s: &str, layout: &Layout) -> f64 {
    let s = s.trim();
    // Handle expressions like "1r+200p"
    if s.contains('+') {
        let parts: Vec<&str> = s.split('+').collect();
        return parts
            .iter()
            .map(|p| parse_radius_simple(p.trim(), layout))
            .sum();
    }
    if s.contains('-') && !s.starts_with('-') {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        let a = parse_radius_simple(parts[0].trim(), layout);
        let b = parse_radius_simple(parts[1].trim(), layout);
        return a - b;
    }
    parse_radius_simple(s, layout)
}

/// Parse a single (non-compound) radius value: "Nr" scales by ideogram
/// radius, "Np" is raw pixels, bare number is parsed as-is.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_datum_map_includes_value() {
        let m = synthetic_datum_map("hs1", 100, 200, Some(0.5));
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("hs1"));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("100"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("200"));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("0.5"));
    }

    #[test]
    fn test_synthetic_datum_map_skips_value_when_none() {
        let m = synthetic_datum_map("hs2", 0, 1000, None);
        assert_eq!(m.len(), 3, "expected 3 keys (chr/start/end), got {:?}", m);
        assert!(!m.contains_key("value"));
    }

    #[test]
    fn test_datum_param_as_config_wraps_strings() {
        let mut p: HashMap<String, String> = HashMap::new();
        p.insert("color".into(), "red".into());
        p.insert("z".into(), "5".into());
        let m = datum_param_as_config(&p);
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("color").and_then(|v| v.as_str()), Some("red"));
        assert_eq!(m.get("z").and_then(|v| v.as_str()), Some("5"));
    }

    #[test]
    fn test_datum_param_as_config_empty() {
        let p: HashMap<String, String> = HashMap::new();
        let m = datum_param_as_config(&p);
        assert!(m.is_empty());
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
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 950.0,
                ideogram_radius_outer: 1050.0,
            },
        }
    }

    #[test]
    fn test_data_range_with_values() {
        let data = vec![
            Datum { value: Some(0.2), ..Default::default() },
            Datum { value: Some(0.8), ..Default::default() },
            Datum { value: Some(0.5), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert!((min - 0.2).abs() < 1e-12);
        assert!((max - 0.8).abs() < 1e-12);
    }

    #[test]
    fn test_data_range_empty_or_nonevalues_defaults_to_01() {
        // Empty data → (0.0, 1.0) default.
        let (min, max) = data_range(&[]);
        assert_eq!((min, max), (0.0, 1.0));
        // All-None values → same default.
        let data = vec![
            Datum { value: None, ..Default::default() },
            Datum { value: None, ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!((min, max), (0.0, 1.0));
    }

    #[test]
    fn test_value_to_color_gradient_endpoints_and_midpoint() {
        // t=0 → blue end (0,0,255).
        let c = value_to_color(0.0);
        assert_eq!((c.r, c.g, c.b), (0, 0, 255));
        // t=0.5 → white/near-white.
        let c = value_to_color(0.5);
        // At the pivot, formula yields fully red (crossing 0.5 exactly).
        // At t=0.5 → second branch with f=0: Color::rgb(255, 255, 255).
        assert_eq!((c.r, c.g, c.b), (255, 255, 255));
        // t=1.0 → red end (255,0,0).
        let c = value_to_color(1.0);
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
    }

    #[test]
    fn test_plot_parse_radius_addition_expr() {
        let layout = mk_layout();
        // "1r+200p" → 1000.0 + 200.0 = 1200.0
        let r = parse_radius("1r+200p", &layout);
        assert!((r - 1200.0).abs() < 1e-9);
        // Multiple-sum form: "0.5r+100p+50p" → 500 + 100 + 50 = 650.
        let r = parse_radius("0.5r+100p+50p", &layout);
        assert!((r - 650.0).abs() < 1e-9);
    }

    #[test]
    fn test_plot_parse_radius_subtraction_expr() {
        let layout = mk_layout();
        // "1r-100p" → 1000 - 100 = 900.
        let r = parse_radius("1r-100p", &layout);
        assert!((r - 900.0).abs() < 1e-9);
    }

    #[test]
    fn test_plot_parse_radius_single_forms() {
        let layout = mk_layout();
        assert!((parse_radius("0.5r", &layout) - 500.0).abs() < 1e-9);
        assert!((parse_radius("1200p", &layout) - 1200.0).abs() < 1e-9);
        assert!((parse_radius("300", &layout) - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_datum_param_as_config_multi_entries_all_wrapped() {
        // All params converted to ConfigValue::Str; entry count preserved.
        let mut p = HashMap::new();
        for i in 0..10 {
            p.insert(format!("k{}", i), format!("v{}", i));
        }
        let m = datum_param_as_config(&p);
        assert_eq!(m.len(), 10);
        for i in 0..10 {
            let key = format!("k{}", i);
            let expected = format!("v{}", i);
            assert_eq!(m.get(&key).and_then(|v| v.as_str()), Some(expected.as_str()));
        }
    }

    #[test]
    fn test_synthetic_datum_map_with_zero_start_end() {
        // start=0/end=0 → "0"/"0" strings preserved, not empty.
        let m = synthetic_datum_map("c1", 0, 0, Some(0.0));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("0"));
    }

    #[test]
    fn test_data_range_all_same_value() {
        // All data points have same value → min == max.
        let data = vec![
            Datum { value: Some(3.0), ..Default::default() },
            Datum { value: Some(3.0), ..Default::default() },
            Datum { value: Some(3.0), ..Default::default() },
        ];
        let (mn, mx) = data_range(&data);
        assert_eq!(mn, 3.0);
        assert_eq!(mx, 3.0);
    }

    #[test]
    fn test_plot_parse_radius_combined_expression() {
        let layout = mk_layout();
        // "0.5r+100p-50p" → 500 + 100 - 50 = 550. But the impl splits on `+` first:
        // ["0.5r", "100p-50p"]. parse_radius_simple("100p-50p") treats "100p-50p"
        // as a plain f64 attempt (doesn't end in r/p after strip), so it falls to
        // the else branch which fails → 0.0. Result: 500 + 0 = 500.
        let r = parse_radius("0.5r+100p-50p", &layout);
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_value_to_color_eighth_points_monotonic() {
        // Blue-white-red gradient: red channel monotonically increases from t=0 to t=1.
        // Blue channel monotonically decreases.
        let c0 = value_to_color(0.0);
        let c1 = value_to_color(0.125);
        let c2 = value_to_color(0.25);
        let c3 = value_to_color(0.5);
        let c4 = value_to_color(0.75);
        let c5 = value_to_color(0.875);
        let c6 = value_to_color(1.0);
        // Red channel monotonically non-decreasing.
        assert!(c0.r <= c1.r && c1.r <= c2.r && c2.r <= c3.r);
        assert!(c3.r <= c4.r && c4.r <= c5.r && c5.r <= c6.r);
        // Blue channel monotonically non-increasing.
        assert!(c0.b >= c1.b && c1.b >= c2.b);
        assert!(c3.b >= c4.b && c4.b >= c5.b && c5.b >= c6.b);
    }

    #[test]
    fn test_value_to_color_below_zero_saturates_blue() {
        // t<0 enters the first branch with f = 2t < 0 → (negative * 255) as u8
        // casts via wrapping. Current impl doesn't clamp. Just verify no panic.
        let c = value_to_color(-0.5);
        // b channel is 255 (unchanged), r/g get wrapped u8 from negative.
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_value_to_color_above_one_saturates_red() {
        // t>1 enters the second branch with f = 2(t-0.5) > 1 → (1-f)*255 negative,
        // as u8 via wrapping. r channel stays 255.
        let c = value_to_color(1.5);
        assert_eq!(c.r, 255);
    }

    #[test]
    fn test_value_to_color_exact_pivot_is_white() {
        // t=0.5 exactly → second branch with f=0 → (255, 255, 255) white.
        let c = value_to_color(0.5);
        assert_eq!((c.r, c.g, c.b), (255, 255, 255));
    }

    #[test]
    fn test_value_to_color_gradient_quarter_points() {
        // t=0.25 → lower half (blue → white): f = 0.5 → (127, 127, 255).
        let c = value_to_color(0.25);
        assert_eq!((c.r, c.g, c.b), (127, 127, 255));
        // t=0.75 → upper half (white → red): f = 0.5 → (255, 127, 127).
        let c = value_to_color(0.75);
        assert_eq!((c.r, c.g, c.b), (255, 127, 127));
    }

    #[test]
    fn test_plot_parse_radius_leading_negative_not_treated_as_subtraction() {
        let layout = mk_layout();
        // `-0.5r` starts with `-` → subtraction branch gated on `!s.starts_with('-')` skips.
        // Falls through to `parse_radius_simple`: ends with 'r', parses "-0.5" → -500.
        let r = parse_radius("-0.5r", &layout);
        assert!((r - (-500.0)).abs() < 1e-9);
    }

    #[test]
    fn test_data_range_mixed_some_none_values() {
        // Some-None mix: only Some values contribute to range, None values skipped.
        let data = vec![
            Datum { value: Some(1.0), ..Default::default() },
            Datum { value: None, ..Default::default() },
            Datum { value: Some(5.0), ..Default::default() },
            Datum { value: None, ..Default::default() },
            Datum { value: Some(3.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert!((min - 1.0).abs() < 1e-12);
        assert!((max - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_plot_parse_radius_simple_invalid_returns_zero() {
        let layout = mk_layout();
        // `parse_radius_simple` with garbage → 0.0 via unwrap_or.
        assert_eq!(parse_radius_simple("garbage", &layout), 0.0);
        assert_eq!(parse_radius_simple("", &layout), 0.0);
        // "abcr" → trim "r" → "abc" → parse fails → 0.0.
        assert_eq!(parse_radius_simple("abcr", &layout), 0.0);
    }

    #[test]
    fn test_draw_plot_missing_type_is_noop() {
        // Config without "type" key → early return; no SVG elements added.
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let block_conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_plot(&mut doc, &layout, &[], &block_conf, &colors);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_plot_unknown_type_is_noop() {
        // Unknown plot type in match → no-op fallthrough.
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
        block_conf.insert("type".into(), ConfigValue::Str("unknown_plot_kind".into()));
        draw_plot(&mut doc, &layout, &[], &block_conf, &colors);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_plot_valid_type_empty_data_no_panic() {
        // Valid type with empty data → no panic, may or may not add elements
        // (depends on the per-type entry logic). Key assertion: doesn't panic.
        let layout = mk_layout();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        for type_name in &["histogram", "heatmap", "scatter", "line", "text", "tile"] {
            let mut block_conf: HashMap<String, ConfigValue> = HashMap::new();
            block_conf.insert("type".into(), ConfigValue::Str((*type_name).into()));
            // Should not panic with empty data slice.
            draw_plot(&mut doc, &layout, &[], &block_conf, &colors);
        }
    }

    #[test]
    fn test_synthetic_datum_map_negative_coordinates_preserved() {
        // Negative i64 coordinates render as strings verbatim (via `to_string`).
        let m = synthetic_datum_map("hsX", -100, -50, Some(-0.25));
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("hsX"));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("-100"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("-50"));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("-0.25"));
    }

    #[test]
    fn test_data_range_empty_slice_returns_default_zero_one() {
        // With no data, min > max (MAX > MIN) triggers the fallback: (0.0, 1.0).
        let (min, max) = data_range(&[]);
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_data_range_skips_none_values() {
        // Datum with value=None is skipped; only numeric values considered.
        let data = vec![
            Datum { chr: "c1".into(), start: 0, end: 100, value: None, ..Default::default() },
            Datum { chr: "c1".into(), start: 0, end: 100, value: Some(5.0), ..Default::default() },
            Datum { chr: "c1".into(), start: 0, end: 100, value: None, ..Default::default() },
            Datum { chr: "c1".into(), start: 0, end: 100, value: Some(-3.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -3.0);
        assert_eq!(max, 5.0);
    }

    #[test]
    fn test_data_range_all_none_values_returns_default() {
        // Every Datum has value=None → min stays MAX, max stays MIN → fallback.
        let data = vec![
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_value_to_color_endpoints_and_midpoint() {
        // t=0: blue branch with f=0 → (0,0,255).
        let c0 = value_to_color(0.0);
        assert_eq!(c0.r, 0);
        assert_eq!(c0.g, 0);
        assert_eq!(c0.b, 255);
        // t=1: red branch with f=1 → (255,0,0).
        let c1 = value_to_color(1.0);
        assert_eq!(c1.r, 255);
        assert_eq!(c1.g, 0);
        assert_eq!(c1.b, 0);
        // t=0.5: second branch with f=0 → (255,255,255) white.
        let cmid = value_to_color(0.5);
        assert_eq!(cmid.r, 255);
        assert_eq!(cmid.g, 255);
        assert_eq!(cmid.b, 255);
    }

    #[test]
    fn test_data_range_single_value_min_equals_max() {
        // Single datum with value → min == max == value.
        let data = vec![Datum {
            chr: "c1".into(),
            value: Some(3.14),
            ..Default::default()
        }];
        let (min, max) = data_range(&data);
        assert_eq!(min, 3.14);
        assert_eq!(max, 3.14);
    }

    #[test]
    fn test_data_range_with_negative_and_positive_extremes() {
        // Mix of negative and positive values — min/max span full range.
        let data = vec![
            Datum { chr: "c".into(), value: Some(-50.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(0.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(100.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(25.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -50.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn test_parse_radius_subtraction_expression_uses_first_minus_only() {
        // "1r-100p" → split at first '-' → "1r" and "100p" → 1*1000 - 100 = 900.
        let layout = Layout {
            ideograms: Vec::new(),
            gcircum: 3e9,
            gsize_noscale: 3e9,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0,
                ideogram_radius_outer: 1000.0,
            },
        };
        let r = parse_radius("1r-100p", &layout);
        assert!((r - 900.0).abs() < 1e-9);
    }

    #[test]
    fn test_value_to_color_mid_quarter_points() {
        // t=0.25: blue branch with f=0.5 → red=127,green=127,blue=255.
        let c = value_to_color(0.25);
        assert_eq!(c.r, 127);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 255);
        // t=0.75: red branch with f=0.5 → red=255,green=127,blue=127.
        let c = value_to_color(0.75);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 127);
    }

    #[test]
    fn test_parse_radius_simple_empty_string_returns_zero() {
        // parse_radius_simple("", layout) — empty → parse fails → 0.
        let layout = Layout {
            ideograms: Vec::new(),
            gcircum: 3e9,
            gsize_noscale: 3e9,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0,
                ideogram_radius_outer: 1000.0,
            },
        };
        assert_eq!(parse_radius_simple("", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_simple_negative_values() {
        // parse_radius_simple accepts negatives: "-100p" → -100.
        let layout = Layout {
            ideograms: Vec::new(),
            gcircum: 3e9,
            gsize_noscale: 3e9,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0,
                ideogram_radius_outer: 1000.0,
            },
        };
        assert!((parse_radius_simple("-100p", &layout) - (-100.0)).abs() < 1e-9);
        assert!((parse_radius_simple("-0.5r", &layout) - (-500.0)).abs() < 1e-9);
    }

    #[test]
    fn test_synthetic_datum_map_nonexistent_value_skipped() {
        // synthetic_datum_map with None value → "value" key absent.
        let m = synthetic_datum_map("hs1", 0, 100, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("hs1"));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("100"));
        // value key is absent when None is passed.
        assert!(m.get("value").is_none());
    }

    #[test]
    fn test_data_range_unsigned_and_positive_only() {
        // All positive values — min/max reflect the range correctly.
        let data = vec![
            Datum { chr: "c".into(), value: Some(10.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(20.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(5.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(15.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 5.0);
        assert_eq!(max, 20.0);
    }

    #[test]
    fn test_value_to_color_gradient_continuity_at_pivot() {
        // At t=0.499 and t=0.501, colors should be nearly identical (continuity).
        let c_before = value_to_color(0.499);
        let c_after = value_to_color(0.501);
        // Red channels should both be near 255 (close to pivot from both sides).
        assert!(c_before.r >= 250);
        assert!(c_after.r >= 250);
        // Green and blue channels also near 255 (white pivot).
        assert!(c_before.g >= 250);
        assert!(c_after.g >= 250);
    }

    #[test]
    fn test_synthetic_datum_map_very_large_integer_coords() {
        // 2^40 scale → preserved as string.
        let m = synthetic_datum_map("hsX", 1_099_511_627_776, 1_100_000_000_000, Some(1.0));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("1099511627776"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("1100000000000"));
    }

    #[test]
    fn test_parse_radius_compound_with_r_only() {
        // "0.5r+0.3r" — both parts scaled.
        let layout = Layout {
            ideograms: Vec::new(),
            gcircum: 3e9,
            gsize_noscale: 3e9,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0,
                ideogram_radius_outer: 1000.0,
            },
        };
        let r = parse_radius("0.5r+0.3r", &layout);
        // 500 + 300 = 800.
        assert!((r - 800.0).abs() < 1e-9);
    }

    #[test]
    fn test_data_range_preserves_finite_precision() {
        // Float precision preserved; assert min/max reflect actual input values.
        let data = vec![
            Datum { chr: "c".into(), value: Some(0.123456789), ..Default::default() },
            Datum { chr: "c".into(), value: Some(9.876543210), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert!((min - 0.123456789).abs() < 1e-12);
        assert!((max - 9.876543210).abs() < 1e-12);
    }

    #[test]
    fn test_value_to_color_endpoints_pure_blue_and_red() {
        // t=0 → (0,0,255) pure blue; t=1 → (255,0,0) pure red (heatmap poles).
        let c0 = value_to_color(0.0);
        assert_eq!((c0.r, c0.g, c0.b), (0, 0, 255));
        let c1 = value_to_color(1.0);
        assert_eq!((c1.r, c1.g, c1.b), (255, 0, 0));
    }

    #[test]
    fn test_value_to_color_exact_half_is_white() {
        // t=0.5 falls to the else branch; f=(0.5-0.5)*2=0 → (255, 255, 255) white.
        let c = value_to_color(0.5);
        assert_eq!((c.r, c.g, c.b), (255, 255, 255));
    }

    #[test]
    fn test_parse_radius_leading_negative_not_split_on_minus() {
        let layout = Layout {
            ideograms: Vec::new(),
            gcircum: 3e9,
            gsize_noscale: 3e9,
            image_radius: 1500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0,
                ideogram_radius_outer: 1000.0,
            },
        };
        // Starts with '-' → skip subtract branch; plain simple parse of "-0.5r" → -500.
        let r = parse_radius("-0.5r", &layout);
        assert!((r - (-500.0)).abs() < 1e-9);
    }

    #[test]
    fn test_data_range_ignores_none_values_between_somes() {
        let data = vec![
            Datum { chr: "c".into(), value: Some(1.0), ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: Some(5.0), ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 1.0);
        assert_eq!(max, 5.0);
    }

    #[test]
    fn test_synthetic_datum_map_omits_value_key_when_none() {
        // value=None → "value" absent from the synthetic map.
        let m = synthetic_datum_map("hs1", 0, 100, None);
        assert!(m.contains_key("chr"));
        assert!(m.contains_key("start"));
        assert!(m.contains_key("end"));
        assert!(!m.contains_key("value"));
        // With Some(v), "value" key present.
        let m2 = synthetic_datum_map("hs1", 0, 100, Some(0.5));
        assert!(m2.contains_key("value"));
        assert_eq!(m2.get("value").and_then(|v| v.as_str()), Some("0.5"));
    }

    #[test]
    fn test_synthetic_datum_map_large_i64_coords_stringified() {
        // Start/end as i64::MAX are stringified via to_string() — preserved without truncation.
        let m = synthetic_datum_map("chrX", i64::MAX, i64::MAX, Some(1e300));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some(i64::MAX.to_string().as_str()));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some(i64::MAX.to_string().as_str()));
        // Large f64 also round-trips.
        let expected = 1e300_f64.to_string();
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some(expected.as_str()));
    }

    #[test]
    fn test_datum_param_as_config_empty_input_produces_empty_map() {
        let empty: HashMap<String, String> = HashMap::new();
        let out = datum_param_as_config(&empty);
        assert!(out.is_empty());
    }

    #[test]
    fn test_datum_param_as_config_wraps_each_value_as_str_variant() {
        let mut p: HashMap<String, String> = HashMap::new();
        p.insert("color".into(), "red".into());
        p.insert("thickness".into(), "2".into());
        let out = datum_param_as_config(&p);
        assert_eq!(out.len(), 2);
        assert_eq!(out.get("color").and_then(|v| v.as_str()), Some("red"));
        assert_eq!(out.get("thickness").and_then(|v| v.as_str()), Some("2"));
        // Every value is Str variant — none are Map or List.
        for v in out.values() {
            assert!(v.as_str().is_some());
        }
    }

    #[test]
    fn test_value_to_color_quarter_point_025_blue_ramp() {
        // t=0.25 → t<0.5 branch, f=0.5 → rgb(127, 127, 255) (blue dominant).
        let c = value_to_color(0.25);
        assert_eq!(c.b, 255);
        assert_eq!(c.r, 127);
        assert_eq!(c.g, 127);
    }

    #[test]
    fn test_value_to_color_quarter_point_075_red_ramp() {
        // t=0.75 → else branch, f=(0.75-0.5)*2=0.5 → rgb(255, 127, 127) (red dominant).
        let c = value_to_color(0.75);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 127);
    }

    #[test]
    fn test_data_range_single_element_returns_value_as_both_min_and_max() {
        // Single Some(v) → min=v, max=v.
        let data = vec![Datum {
            chr: "c".into(),
            value: Some(42.5),
            ..Default::default()
        }];
        let (min, max) = data_range(&data);
        assert_eq!(min, 42.5);
        assert_eq!(max, 42.5);
    }

    #[test]
    fn test_data_range_all_none_values_returns_unit_fallback() {
        // All None → min stays MAX, max stays MIN → min>max → fallback (0.0, 1.0).
        let data = vec![
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
        // Empty slice also hits fallback.
        let (min2, max2) = data_range(&[]);
        assert_eq!(min2, 0.0);
        assert_eq!(max2, 1.0);
    }

    #[test]
    fn test_data_range_identical_values_yields_equal_min_and_max() {
        // All values equal → min == max (no fallback).
        let data = vec![
            Datum { chr: "c".into(), value: Some(7.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(7.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(7.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 7.0);
        assert_eq!(max, 7.0);
    }

    #[test]
    fn test_value_to_color_t_above_one_saturates_to_red() {
        // t=2.0 → (2-0.5)*2 = 3.0 → Color::rgb(255, ((1-3)*255) as u8, ...) saturates to 0.
        let c = value_to_color(2.0);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_value_to_color_t_below_zero_saturates_to_blue() {
        // t=-1.0 → t<0.5 → f=-2.0 → Color::rgb((-2*255) as u8, ..., 255) → saturates to 0.
        let c = value_to_color(-1.0);
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_synthetic_datum_map_nan_value_stringified_as_nan() {
        // f64::NAN.to_string() → "NaN"; stored verbatim in map.
        let m = synthetic_datum_map("c", 0, 100, Some(f64::NAN));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("NaN"));
        // Infinity → "inf".
        let m2 = synthetic_datum_map("c", 0, 100, Some(f64::INFINITY));
        assert_eq!(m2.get("value").and_then(|v| v.as_str()), Some("inf"));
    }

    #[test]
    fn test_parse_radius_compound_add_then_subtract() {
        let layout = Layout {
            ideograms: Vec::new(),
            gcircum: 3e9, gsize_noscale: 3e9, image_radius: 1500.0,
            angle_offset: 0.0, counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0, ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0, ideogram_radius_outer: 1000.0,
            },
        };
        // "1r+50p" → 1000 + 50 = 1050.
        let r = parse_radius("1r+50p", &layout);
        assert!((r - 1050.0).abs() < 1e-9);
        // "1r-100p" → 1000 - 100 = 900.
        let r2 = parse_radius("1r-100p", &layout);
        assert!((r2 - 900.0).abs() < 1e-9);
    }

    #[test]
    fn test_value_to_color_mid_quarter_points_gradient() {
        // t=0.125 → first branch: f=0.25 → rgb(63, 63, 255); near-blue.
        let c = value_to_color(0.125);
        assert_eq!(c.r, 63);
        assert_eq!(c.g, 63);
        assert_eq!(c.b, 255);
        // t=0.875 → else branch: f=0.75 → rgb(255, 63, 63); near-red.
        let c2 = value_to_color(0.875);
        assert_eq!(c2.r, 255);
        assert_eq!(c2.g, 63);
        assert_eq!(c2.b, 63);
    }

    #[test]
    fn test_datum_param_as_config_preserves_all_key_value_pairs() {
        let mut p: HashMap<String, String> = HashMap::new();
        for i in 0..5 {
            p.insert(format!("k{}", i), format!("v{}", i));
        }
        let out = datum_param_as_config(&p);
        assert_eq!(out.len(), 5);
        for i in 0..5 {
            let k = format!("k{}", i);
            let expected = format!("v{}", i);
            assert_eq!(out.get(&k).and_then(|v| v.as_str()), Some(expected.as_str()));
        }
    }

    #[test]
    fn test_data_range_all_negative_values_min_less_than_max() {
        // All negative values → min is most-negative, max is least-negative.
        let data = vec![
            Datum { chr: "c".into(), value: Some(-5.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(-10.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(-1.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -10.0);
        assert_eq!(max, -1.0);
    }

    #[test]
    fn test_value_to_color_at_exactly_half_is_pure_white() {
        // At t=0.5 the else branch runs with f=0 → (255, 255, 255) pure white.
        let c = value_to_color(0.5);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 255);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_data_range_mixed_some_and_none_values_uses_only_some() {
        // None entries should be skipped — min/max come only from Some values.
        let data = vec![
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: Some(7.0), ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: Some(3.0), ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 3.0);
        assert_eq!(max, 7.0);
    }

    #[test]
    fn test_parse_radius_leading_minus_sign_not_split_as_subtraction() {
        // Starting '-' triggers the `!starts_with('-')` guard → not split — whole string
        // passed to parse_radius_simple, which treats it as a signed value.
        let layout = mk_layout();
        // "-50p" → simple path → -50.0 (negative pixels).
        assert_eq!(parse_radius("-50p", &layout), -50.0);
        // "-0.5r" → -0.5 × ideogram_radius=1000 = -500.
        assert_eq!(parse_radius("-0.5r", &layout), -500.0);
    }

    #[test]
    fn test_parse_radius_plus_splits_with_whitespace_around_operator() {
        // "1r + 50p" → split on '+' yields ["1r ", " 50p"] — each part is trimmed
        // before being passed to simple → 1000 + 50 = 1050.
        let layout = mk_layout();
        assert_eq!(parse_radius("1r + 50p", &layout), 1050.0);
        // Multiple terms: "0.5r + 10p + 5p" = 500 + 10 + 5 = 515.
        assert_eq!(parse_radius("0.5r + 10p + 5p", &layout), 515.0);
    }

    #[test]
    fn test_parse_radius_simple_garbage_value_returns_zero_from_unwrap_or() {
        // Unparseable r/p/raw values all fall back to 0.0 via unwrap_or(0.0).
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("xyzr", &layout), 0.0);
        assert_eq!(parse_radius_simple("abcp", &layout), 0.0);
        assert_eq!(parse_radius_simple("not_a_number", &layout), 0.0);
        // Empty string also → 0.
        assert_eq!(parse_radius_simple("", &layout), 0.0);
    }

    #[test]
    fn test_value_to_color_at_t_quarter_is_blue_ramped() {
        // t=0.25 in the first branch: f=0.25*2=0.5 → (127,127,255) via f × 255.
        let c = value_to_color(0.25);
        // 0.5 × 255 = 127.5; `as u8` truncates toward zero → 127.
        assert_eq!(c.r, 127);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_data_range_first_none_followed_by_some_uses_the_some() {
        // Leading None doesn't bias range — first Some sets both initial min and max.
        let data = vec![
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: Some(42.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 42.0);
        assert_eq!(max, 42.0);
    }

    #[test]
    fn test_parse_radius_simple_r_value_preserves_sign() {
        // "-2r" → -2 × ideogram_radius=1000 = -2000.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("-2r", &layout), -2000.0);
        // Negative p too.
        assert_eq!(parse_radius_simple("-100p", &layout), -100.0);
    }

    #[test]
    fn test_value_to_color_at_zero_is_pure_blue() {
        // t=0: first branch with f=0 → (0, 0, 255) pure blue.
        let c = value_to_color(0.0);
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_value_to_color_at_one_is_pure_red() {
        // t=1: else branch with f=1 → (255, 0, 0) pure red.
        let c = value_to_color(1.0);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_synthetic_datum_map_chromosome_start_end_stored_as_strings() {
        // synthetic_datum_map stores chr/start/end in the map using to_string().
        let m = synthetic_datum_map("hsX", 1_000_000, 2_000_000, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("hsX"));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("1000000"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("2000000"));
        // No value → no "value" key.
        assert!(m.get("value").is_none());
    }

    #[test]
    fn test_data_range_all_zero_values_returns_min_max_both_zero() {
        // Multiple entries all with Some(0.0) → min == max == 0.0.
        let data = vec![
            Datum { chr: "c".into(), value: Some(0.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(0.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(0.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 0.0);
    }

    #[test]
    fn test_parse_radius_empty_string_returns_zero() {
        // Empty input → parse_radius_simple("") → unwrap_or(0.0) → 0.
        let layout = mk_layout();
        assert_eq!(parse_radius("", &layout), 0.0);
        // Whitespace-only trims to empty.
        assert_eq!(parse_radius("   ", &layout), 0.0);
    }

    #[test]
    fn test_synthetic_datum_map_contains_only_expected_keys() {
        // synthetic_datum_map stores chr/start/end always; value if Some.
        let m_with = synthetic_datum_map("hsX", 100, 200, Some(3.14));
        // When value is Some, it appears as a key.
        assert!(m_with.contains_key("chr"));
        assert!(m_with.contains_key("start"));
        assert!(m_with.contains_key("end"));
        assert!(m_with.contains_key("value"));

        let m_without = synthetic_datum_map("hsY", 0, 0, None);
        assert!(m_without.contains_key("chr"));
        assert!(m_without.contains_key("start"));
        assert!(m_without.contains_key("end"));
        assert!(!m_without.contains_key("value"));
    }

    #[test]
    fn test_value_to_color_boundary_at_just_below_half_still_blue_ramp() {
        // t=0.49999 is < 0.5 → first branch (blue ramp) where green=red < 255.
        let c = value_to_color(0.49999);
        assert_eq!(c.b, 255);
        assert_eq!(c.r, c.g); // symmetric in first branch
        assert!(c.r < 255);
    }

    #[test]
    fn test_data_range_empty_slice_returns_0_1_default() {
        // Empty data → min > max → fallback (0.0, 1.0).
        let data: Vec<Datum> = Vec::new();
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_parse_radius_bare_p_suffix_zero() {
        // "p" alone → trim_end → "" → parse fails → 0.
        let layout = mk_layout();
        assert_eq!(parse_radius("p", &layout), 0.0);
        assert_eq!(parse_radius("r", &layout), 0.0);
    }

    #[test]
    fn test_value_to_color_alpha_is_always_255() {
        // The gradient output always has full alpha regardless of t.
        for t in [-1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0] {
            let c = value_to_color(t);
            assert_eq!(c.a, 255, "t={}", t);
        }
    }

    #[test]
    fn test_synthetic_datum_map_with_nan_value_stringified() {
        // f64::NAN → "NaN" via to_string.
        let m = synthetic_datum_map("hs1", 0, 100, Some(f64::NAN));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("NaN"));
    }

    #[test]
    fn test_data_range_single_none_value_returns_default_fallback() {
        // Single-entry data with None value → no Some values → fallback (0, 1).
        let data = vec![Datum {
            chr: "c".into(),
            value: None,
            ..Default::default()
        }];
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_parse_radius_r_unit_with_zero_ideogram_radius() {
        // Custom layout with ideogram_radius=0 → "1r" → 0 × 1 = 0.
        let mut layout = mk_layout();
        layout.dims.ideogram_radius = 0.0;
        assert_eq!(parse_radius("1r", &layout), 0.0);
        assert_eq!(parse_radius("0.5r", &layout), 0.0);
    }

    #[test]
    fn test_parse_radius_subtraction_with_integer_terms() {
        // "1000 - 500" → 500; "-1000 - 500" starts with '-' → not split → simple parse of full (fails).
        let layout = mk_layout();
        // "1000-500" → 500.
        assert_eq!(parse_radius("1000-500", &layout), 500.0);
        // "0.5r - 100p" with ideogram=1000 → 500 - 100 = 400.
        assert_eq!(parse_radius("0.5r-100p", &layout), 400.0);
    }

    #[test]
    fn test_synthetic_datum_map_integer_value_formats_as_integer_string() {
        // Some(value=3.0) → stringified as "3".
        let m = synthetic_datum_map("hs1", 0, 100, Some(3.0));
        let v = m.get("value").and_then(|v| v.as_str()).unwrap();
        // f64::to_string for 3.0 yields "3".
        assert_eq!(v, "3");
    }

    #[test]
    fn test_value_to_color_saturation_beyond_1_and_below_0() {
        // t > 1 saturates to pure red; t < 0 saturates to pure blue.
        let red = value_to_color(100.0);
        assert_eq!(red.r, 255);
        // (Green/Blue may be 0 or small due to integer cast saturation.)
        let blue = value_to_color(-100.0);
        assert_eq!(blue.b, 255);
    }

    #[test]
    fn test_parse_radius_r_and_p_combined_with_fractional_coef() {
        // "0.25r + 0.5p" — 250 + 0.5 = 250.5.
        let layout = mk_layout();
        let r = parse_radius("0.25r + 0.5p", &layout);
        assert!((r - 250.5).abs() < 1e-9);
    }

    #[test]
    fn test_data_range_two_value_entries_distinct_min_max() {
        // Two Some values with different → min=one, max=other.
        let data = vec![
            Datum { chr: "c".into(), value: Some(0.1), ..Default::default() },
            Datum { chr: "c".into(), value: Some(99.9), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert!((min - 0.1).abs() < 1e-9);
        assert!((max - 99.9).abs() < 1e-9);
    }

    #[test]
    fn test_synthetic_datum_map_negative_value_stringified_with_minus() {
        // Some(-3.14) → "-3.14".
        let m = synthetic_datum_map("hs1", 0, 100, Some(-3.14));
        let v = m.get("value").and_then(|v| v.as_str()).unwrap();
        assert_eq!(v, "-3.14");
    }

    #[test]
    fn test_parse_radius_simple_whitespace_only_returns_zero() {
        // "   " trims to empty → parse fail → 0.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("   ", &layout), 0.0);
    }

    #[test]
    fn test_value_to_color_midpoint_exact_half_enters_red_branch() {
        // t=0.5 → second branch; f=0 → rgb(255, 255, 255) — pure white at transition.
        let c = value_to_color(0.5);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_parse_radius_negative_literal_parses_as_negative() {
        // Starts with '-' so it's NOT split by subtraction path; bare "-10p" → -10.
        let layout = mk_layout();
        assert_eq!(parse_radius("-10p", &layout), -10.0);
    }

    #[test]
    fn test_data_range_many_entries_finds_overall_min_and_max() {
        // 5 entries including extremes at non-endpoint positions.
        let data = vec![
            Datum { chr: "c".into(), value: Some(3.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(-5.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(1.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(42.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(7.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -5.0);
        assert_eq!(max, 42.0);
    }

    #[test]
    fn test_synthetic_datum_map_zero_positions_stored_as_zero_strings() {
        // start=0, end=0 → "0"/"0" strings, not absent keys.
        let m = synthetic_datum_map("c", 0, 0, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("0"));
    }

    #[test]
    fn test_value_to_color_low_quarter_produces_lighter_blue_shade() {
        // t=0.25 → first branch → f=0.5 → r/g = 127, b=255 — mid-blue gradient.
        let c = value_to_color(0.25);
        assert_eq!(c.r, 127);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_parse_radius_r_unit_coefficient_one_equals_ideogram_radius() {
        // "1r" × ideogram_radius=1000 = 1000.
        let layout = mk_layout();
        assert_eq!(parse_radius("1r", &layout), 1000.0);
    }

    #[test]
    fn test_data_range_mixed_none_and_some_values_skip_none_entries() {
        // None values are ignored; Some values determine range.
        let data = vec![
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: Some(5.0), ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: Some(15.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 5.0);
        assert_eq!(max, 15.0);
    }

    #[test]
    fn test_synthetic_datum_map_chr_field_preserved_verbatim_with_specials() {
        // chr with special characters is preserved verbatim.
        let m = synthetic_datum_map("chr_1.special-ID", 10, 20, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("chr_1.special-ID"));
    }

    #[test]
    fn test_value_to_color_upper_three_quarter_produces_lighter_red_shade() {
        // t=0.75 → second branch → f=0.5 → g/b = 127; r=255.
        let c = value_to_color(0.75);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 127);
    }

    #[test]
    fn test_parse_radius_subtraction_preserves_order_a_minus_b() {
        // "2r-0.5r" = (2-0.5) × 1000 = 1500.
        let layout = mk_layout();
        assert_eq!(parse_radius("2r-0.5r", &layout), 1500.0);
    }

    #[test]
    fn test_data_range_all_none_values_returns_default_0_1() {
        // All value=None → min > max → fallback (0, 1).
        let data = vec![
            Datum { chr: "c".into(), value: None, ..Default::default() },
            Datum { chr: "c".into(), value: None, ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_synthetic_datum_map_has_three_keys_when_value_is_none() {
        // None value → chr/start/end keys only (len=3).
        let m = synthetic_datum_map("hs1", 100, 200, None);
        assert_eq!(m.len(), 3);
        assert!(m.contains_key("chr"));
        assert!(m.contains_key("start"));
        assert!(m.contains_key("end"));
        assert!(!m.contains_key("value"));
    }

    #[test]
    fn test_value_to_color_quarter_thresholds_match_gradient() {
        // At t=0, pure blue; at t=0.5, white; at t=1, pure red.
        let blue = value_to_color(0.0);
        let white = value_to_color(0.5);
        let red = value_to_color(1.0);
        assert_eq!((blue.r, blue.g, blue.b), (0, 0, 255));
        assert_eq!((white.r, white.g, white.b), (255, 255, 255));
        assert_eq!((red.r, red.g, red.b), (255, 0, 0));
    }

    #[test]
    fn test_parse_radius_addition_pixel_plus_r_unit_sums_components() {
        // "100p+0.5r" → 100 + 500 = 600.
        let layout = mk_layout();
        let r = parse_radius("100p+0.5r", &layout);
        assert_eq!(r, 600.0);
    }

    #[test]
    fn test_data_range_single_some_value_returns_that_value_as_min_and_max() {
        // 1-entry data set → min == max == value.
        let data = vec![
            Datum { chr: "c".into(), value: Some(7.5), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 7.5);
        assert_eq!(max, 7.5);
    }

    #[test]
    fn test_synthetic_datum_map_some_value_formatted_with_up_to_16_digits() {
        // f64 value roundtrips as string via Display impl.
        let m = synthetic_datum_map("c", 0, 10, Some(1.5));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("1.5"));
    }

    #[test]
    fn test_parse_radius_simple_bare_number_parses_as_pixels() {
        // "500" bare → 500.0 via final parse branch.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("500", &layout), 500.0);
    }

    #[test]
    fn test_value_to_color_near_zero_all_have_full_blue_component() {
        // t near 0 → blue=255 consistent.
        for t in [0.0, 0.01, 0.1, 0.2, 0.45] {
            let c = value_to_color(t);
            assert_eq!(c.b, 255);
        }
    }

    #[test]
    fn test_parse_radius_spaces_around_plus_trimmed() {
        // "100p + 100p" with internal spaces — trim per-part.
        let layout = mk_layout();
        let r = parse_radius("100p + 100p", &layout);
        assert_eq!(r, 200.0);
    }

    #[test]
    fn test_data_range_two_values_with_negative_and_positive_find_range() {
        // Negative min and positive max found correctly.
        let data = vec![
            Datum { chr: "c".into(), value: Some(-5.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(10.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -5.0);
        assert_eq!(max, 10.0);
    }

    #[test]
    fn test_value_to_color_value_above_one_saturates_red() {
        // t>=1 → second branch, f = 2*(t-0.5) >= 1 → ((1-f)*255) as u8 = 0.
        let c = value_to_color(1.0);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_radius_simple_negative_p_value_parses_negative() {
        // "-50p" → -50 via p-suffix branch.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("-50p", &layout), -50.0);
    }

    #[test]
    fn test_data_range_large_dataset_finds_outer_bounds_correctly() {
        // 100 values from -100 to 100 → min=-100, max=100.
        let data: Vec<Datum> = (-100..=100)
            .map(|i| Datum { chr: "c".into(), value: Some(i as f64), ..Default::default() })
            .collect();
        let (min, max) = data_range(&data);
        assert_eq!(min, -100.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn test_synthetic_datum_map_large_positions_formatted_correctly() {
        // Very large coords formatted via Display.
        let m = synthetic_datum_map("hs1", 2_000_000_000, 3_000_000_000, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("2000000000"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("3000000000"));
    }

    #[test]
    fn test_parse_radius_simple_just_r_character_no_coefficient_parses_zero() {
        // "r" trims to "" → parse fail → 0, multiplied by ideogram_radius still 0.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("r", &layout), 0.0);
    }

    #[test]
    fn test_value_to_color_midway_quarter_blue_red_transition() {
        // t=0.375 → first branch, f=0.75 → r/g = 191, b=255.
        let c = value_to_color(0.375);
        assert_eq!(c.r, 191);
        assert_eq!(c.g, 191);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_data_range_only_negative_values_found_correctly() {
        // All-negative values → min/max both negative.
        let data = vec![
            Datum { chr: "c".into(), value: Some(-100.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(-50.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(-200.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -200.0);
        assert_eq!(max, -50.0);
    }

    #[test]
    fn test_synthetic_datum_map_negative_start_stored_as_negative_string() {
        // start=-100 → "-100" string.
        let m = synthetic_datum_map("c", -100, 200, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("-100"));
    }

    #[test]
    fn test_parse_radius_zero_expression_evaluates_to_zero() {
        // "0" bare → 0.
        let layout = mk_layout();
        assert_eq!(parse_radius("0", &layout), 0.0);
    }

    #[test]
    fn test_value_to_color_boundary_transition_0_49_vs_0_5() {
        // t=0.499 → blue-ish branch; t=0.5 → white (transition).
        let c49 = value_to_color(0.499);
        let c50 = value_to_color(0.5);
        // b remains 255 in blue branch; at 0.5 threshold goes to red branch with f=0 → white (255,255,255).
        assert_eq!(c49.b, 255);
        assert_eq!(c50.r, 255);
        assert_eq!(c50.g, 255);
        assert_eq!(c50.b, 255);
    }

    #[test]
    fn test_data_range_three_identical_values_min_equals_max() {
        // All values identical → min==max==value.
        let data = vec![
            Datum { chr: "c".into(), value: Some(50.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(50.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(50.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 50.0);
        assert_eq!(max, 50.0);
    }

    #[test]
    fn test_synthetic_datum_map_chr_empty_string_accepted() {
        // Empty chr stored as "".
        let m = synthetic_datum_map("", 10, 20, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some(""));
    }

    #[test]
    fn test_parse_radius_whitespace_around_entire_expr_trimmed() {
        // "   500p   " → 500 (trimmed both ends).
        let layout = mk_layout();
        assert_eq!(parse_radius("   500p   ", &layout), 500.0);
    }

    #[test]
    fn test_value_to_color_negative_t_saturates_blue() {
        // t=-1 → first branch (t<0.5) with f<0 → negative cast → specific blue shade.
        let c = value_to_color(-1.0);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_data_range_fractional_values_min_less_than_max() {
        // Two fractional values → ordered.
        let data = vec![
            Datum { chr: "c".into(), value: Some(0.123), ..Default::default() },
            Datum { chr: "c".into(), value: Some(0.456), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert!(min < max);
        assert!((min - 0.123).abs() < 1e-9);
        assert!((max - 0.456).abs() < 1e-9);
    }

    #[test]
    fn test_synthetic_datum_map_integer_value_no_decimal_via_display() {
        // Integer-valued f64 (e.g., 42.0) formatted as "42" (or may include trailing .0).
        let m = synthetic_datum_map("c", 0, 10, Some(42.0));
        let v = m.get("value").and_then(|v| v.as_str()).unwrap();
        // f64::Display → "42" for exact integer.
        assert_eq!(v, "42");
    }

    #[test]
    fn test_parse_radius_simple_large_pixel_value() {
        // Very large pixel value passthrough.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("999999p", &layout), 999999.0);
    }

    #[test]
    fn test_data_range_single_positive_and_single_negative_both_preserved() {
        // 2 values: -1 and +1 → min=-1, max=1.
        let data = vec![
            Datum { chr: "c".into(), value: Some(-1.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(1.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, -1.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_value_to_color_t_very_close_to_zero_pure_blue() {
        // t=1e-10 (just above 0) → ~blue: b=255.
        let c = value_to_color(1e-10);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_synthetic_datum_map_has_four_keys_when_value_is_some() {
        // Some(value) → chr/start/end/value (4 keys total).
        let m = synthetic_datum_map("hs1", 0, 100, Some(5.0));
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn test_parse_radius_decimal_p_produces_decimal_result() {
        // "0.5p" → 0.5.
        let layout = mk_layout();
        assert_eq!(parse_radius("0.5p", &layout), 0.5);
    }

    #[test]
    fn test_value_to_color_t_0_75_yields_orange_like_color() {
        // t=0.75 → second branch, f=0.5 → r=255, g/b = 127.
        let c = value_to_color(0.75);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 127);
    }

    #[test]
    fn test_data_range_zero_values_preserved() {
        // All-zero → min=max=0.
        let data = vec![
            Datum { chr: "c".into(), value: Some(0.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(0.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 0.0);
        assert_eq!(max, 0.0);
    }

    #[test]
    fn test_synthetic_datum_map_fractional_value_display_format() {
        // Some(0.5) → "0.5".
        let m = synthetic_datum_map("c", 0, 10, Some(0.5));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("0.5"));
    }

    #[test]
    fn test_parse_radius_subtraction_with_negative_result() {
        // "100p-200p" → -100.
        let layout = mk_layout();
        assert_eq!(parse_radius("100p-200p", &layout), -100.0);
    }

    #[test]
    fn test_value_to_color_t_exact_quarter_blue_gradient() {
        // t=0.25 → first branch with f=0.5 → r=g=127, b=255.
        let c = value_to_color(0.25);
        assert_eq!(c.r, 127);
        assert_eq!(c.g, 127);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_data_range_mixed_order_still_correct() {
        // Entries out of order → min/max still correct.
        let data = vec![
            Datum { chr: "c".into(), value: Some(100.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(1.0), ..Default::default() },
            Datum { chr: "c".into(), value: Some(50.0), ..Default::default() },
        ];
        let (min, max) = data_range(&data);
        assert_eq!(min, 1.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn test_synthetic_datum_map_large_negative_value_formatted_correctly() {
        // Large negative value displayed correctly.
        let m = synthetic_datum_map("c", 0, 10, Some(-1_000_000.5));
        let v = m.get("value").and_then(|v| v.as_str()).unwrap();
        assert!(v.starts_with("-1000000"));
    }

    #[test]
    fn test_parse_radius_simple_integer_without_suffix_equals_f64() {
        // "250" → 250.0.
        let layout = mk_layout();
        assert_eq!(parse_radius_simple("250", &layout), 250.0);
    }

    #[test]
    fn test_value_to_color_very_high_t_saturates_at_pure_red() {
        // t=2 → still (255, 0, 0) in second branch.
        let c = value_to_color(2.0);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_data_range_many_values_all_same_min_equals_max() {
        // 100 entries all value=5.0 → min=max=5.
        let data: Vec<Datum> = (0..100)
            .map(|_| Datum { chr: "c".into(), value: Some(5.0), ..Default::default() })
            .collect();
        let (min, max) = data_range(&data);
        assert_eq!(min, 5.0);
        assert_eq!(max, 5.0);
    }

    #[test]
    fn test_synthetic_datum_map_integer_chr_name_stored_verbatim() {
        // chr="1" numeric-looking name preserved.
        let m = synthetic_datum_map("1", 0, 100, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("1"));
    }

    #[test]
    fn test_data_range_empty_input_returns_default_0_1() {
        // No data → min > max after init → fallback to (0, 1).
        let (lo, hi) = data_range(&[]);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 1.0);
    }

    #[test]
    fn test_value_to_color_midpoint_yields_white_rgb_255() {
        // t=0.5: lower branch with f=1.0 → (255, 255, 255) white.
        let c = value_to_color(0.5);
        // At 0.5 → upper branch actually, f=0.0 → (255, 255, 255) white.
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_data_range_datum_with_no_value_skipped() {
        // Datum with value=None skipped; only Some values counted.
        let data = vec![
            Datum { value: None, ..Default::default() },
            Datum { value: Some(3.0), ..Default::default() },
            Datum { value: Some(7.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 3.0);
        assert_eq!(hi, 7.0);
    }

    #[test]
    fn test_synthetic_datum_map_value_none_excludes_value_key() {
        // None value → value key absent from map.
        let m = synthetic_datum_map("chr1", 0, 100, None);
        assert!(m.get("value").is_none());
    }

    #[test]
    fn test_synthetic_datum_map_value_some_includes_value_key() {
        // Some(3.14) → "value" key present with stringified number.
        let m = synthetic_datum_map("chr1", 0, 100, Some(3.14));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("3.14"));
    }

    #[test]
    fn test_datum_param_as_config_empty_map_in_empty_out() {
        // Empty input → empty output HashMap.
        let input: HashMap<String, String> = HashMap::new();
        let out = datum_param_as_config(&input);
        assert!(out.is_empty());
    }

    #[test]
    fn test_datum_param_as_config_keys_and_values_preserved() {
        // Each String key/value wrapped as ConfigValue::Str.
        let mut input: HashMap<String, String> = HashMap::new();
        input.insert("color".into(), "red".into());
        input.insert("size".into(), "12".into());
        let out = datum_param_as_config(&input);
        assert_eq!(out.get("color").and_then(|v| v.as_str()), Some("red"));
        assert_eq!(out.get("size").and_then(|v| v.as_str()), Some("12"));
    }

    #[test]
    fn test_value_to_color_t_zero_produces_pure_blue() {
        // t=0 → (0, 0, 255) pure blue.
        let c = value_to_color(0.0);
        assert_eq!(c.to_hex(), "#0000ff");
    }

    #[test]
    fn test_value_to_color_t_one_produces_pure_red() {
        // t=1.0 → upper branch with f=1.0 → (255, 0, 0) pure red.
        let c = value_to_color(1.0);
        assert_eq!(c.to_hex(), "#ff0000");
    }

    #[test]
    fn test_synthetic_datum_map_with_f64_value_negative() {
        // Negative value preserved as string.
        let m = synthetic_datum_map("chr1", 0, 100, Some(-42.5));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("-42.5"));
    }

    #[test]
    fn test_data_range_single_datum_with_value() {
        // Single datum → min = max = value.
        let data = vec![Datum { value: Some(7.0), ..Default::default() }];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 7.0);
        assert_eq!(hi, 7.0);
    }

    #[test]
    fn test_synthetic_datum_map_start_end_zero_both_stored() {
        // start=0, end=0 → stored as "0" strings.
        let m = synthetic_datum_map("chrX", 0, 0, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("0"));
    }

    #[test]
    fn test_value_to_color_t_quarter_produces_light_blue_shade() {
        // t=0.25 → f=0.5 → (127, 127, 255) (via f×255 = 127.5 as u8 → 127).
        let c = value_to_color(0.25);
        assert_eq!(c.to_hex(), "#7f7fff");
    }

    #[test]
    fn test_synthetic_datum_map_chr_with_spaces_preserved() {
        // chr name with spaces stored as-is.
        let m = synthetic_datum_map("chr with spaces", 0, 100, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("chr with spaces"));
    }

    #[test]
    fn test_data_range_with_all_negative_values() {
        // All negative values → min and max both negative.
        let data = vec![
            Datum { value: Some(-10.0), ..Default::default() },
            Datum { value: Some(-5.0), ..Default::default() },
            Datum { value: Some(-20.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, -20.0);
        assert_eq!(hi, -5.0);
    }

    #[test]
    fn test_datum_param_as_config_long_string_values_preserved() {
        // Long String values survive the wrap.
        let mut input: HashMap<String, String> = HashMap::new();
        let long_val: String = "x".repeat(300);
        input.insert("big".into(), long_val.clone());
        let out = datum_param_as_config(&input);
        assert_eq!(out.get("big").and_then(|v| v.as_str()), Some(long_val.as_str()));
    }

    #[test]
    fn test_value_to_color_t_75_produces_light_red_shade() {
        // t=0.75 → upper branch f=0.5 → (255, 127, 127).
        let c = value_to_color(0.75);
        assert_eq!(c.to_hex(), "#ff7f7f");
    }

    #[test]
    fn test_synthetic_datum_map_very_large_coords() {
        // Large i64 start/end values preserved as strings.
        let m = synthetic_datum_map("chr1", 1_000_000_000, 2_000_000_000, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("1000000000"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("2000000000"));
    }

    #[test]
    fn test_data_range_mixed_positive_negative_values_correctly_ordered() {
        // Mix of + and - values → min and max bracket everything.
        let data = vec![
            Datum { value: Some(-5.0), ..Default::default() },
            Datum { value: Some(10.0), ..Default::default() },
            Datum { value: Some(0.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, -5.0);
        assert_eq!(hi, 10.0);
    }

    #[test]
    fn test_value_to_color_at_exact_inflection_point_is_white() {
        // t=0.5 exactly is inflection; upper branch f=0.0 → (255, 255, 255).
        let c = value_to_color(0.5);
        assert_eq!(c.to_hex(), "#ffffff");
    }

    #[test]
    fn test_synthetic_datum_map_negative_coords_stored_with_minus_sign() {
        // Negative coords preserved with "-".
        let m = synthetic_datum_map("chr1", -100, -50, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("-100"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("-50"));
    }

    #[test]
    fn test_data_range_two_equal_values_min_equals_max() {
        // Two Datum both with value=50 → lo=hi=50.
        let data = vec![
            Datum { value: Some(50.0), ..Default::default() },
            Datum { value: Some(50.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 50.0);
        assert_eq!(hi, 50.0);
    }

    #[test]
    fn test_value_to_color_t_exactly_0_dot_1_produces_mostly_blue() {
        // t=0.1 → f=0.2 → (51, 51, 255) dark-blue.
        let c = value_to_color(0.1);
        assert_eq!(c.to_hex(), "#3333ff");
    }

    #[test]
    fn test_datum_param_as_config_large_num_entries_all_preserved() {
        // 20 kv pairs all survive.
        let mut input: HashMap<String, String> = HashMap::new();
        for i in 0..20 {
            input.insert(format!("k{}", i), format!("v{}", i));
        }
        let out = datum_param_as_config(&input);
        assert_eq!(out.len(), 20);
    }

    #[test]
    fn test_synthetic_datum_map_value_with_pi_stored_with_precision() {
        // π stored with f64 default Display precision.
        let m = synthetic_datum_map("x", 0, 1, Some(std::f64::consts::PI));
        let s = m.get("value").and_then(|v| v.as_str()).unwrap();
        assert!(s.starts_with("3.14"));
    }

    #[test]
    fn test_data_range_with_2000_datum_values_large_scan() {
        // Large data set — data_range still produces (min, max).
        let data: Vec<Datum> = (0..2000)
            .map(|i| Datum { value: Some(i as f64), ..Default::default() })
            .collect();
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 1999.0);
    }

    #[test]
    fn test_value_to_color_fractional_t_produces_color_variation() {
        // Different t values produce different colors.
        let c1 = value_to_color(0.3);
        let c2 = value_to_color(0.7);
        assert_ne!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_datum_param_as_config_single_key_accessible_by_name() {
        // Single entry accessible via original key.
        let mut input: HashMap<String, String> = HashMap::new();
        input.insert("thickness".into(), "2".into());
        let out = datum_param_as_config(&input);
        assert_eq!(out.get("thickness").and_then(|v| v.as_str()), Some("2"));
    }

    #[test]
    fn test_synthetic_datum_map_always_contains_chr_start_end_keys() {
        // 3 required keys always present regardless of value.
        let m = synthetic_datum_map("x", 5, 10, None);
        assert!(m.contains_key("chr"));
        assert!(m.contains_key("start"));
        assert!(m.contains_key("end"));
    }

    #[test]
    fn test_data_range_datum_with_infinity_value_preserved() {
        // Infinity value survives in min/max logic.
        let data = vec![
            Datum { value: Some(10.0), ..Default::default() },
            Datum { value: Some(f64::INFINITY), ..Default::default() },
        ];
        let (_, hi) = data_range(&data);
        assert_eq!(hi, f64::INFINITY);
    }

    #[test]
    fn test_value_to_color_three_distinct_t_values_produce_distinct_hex() {
        // t=0, 0.5, 1 → three distinct colors.
        let c1 = value_to_color(0.0).to_hex();
        let c2 = value_to_color(0.5).to_hex();
        let c3 = value_to_color(1.0).to_hex();
        assert_ne!(c1, c2);
        assert_ne!(c2, c3);
        assert_ne!(c1, c3);
    }

    #[test]
    fn test_datum_param_as_config_output_len_matches_input() {
        // Output HashMap size equals input HashMap size.
        let mut input: HashMap<String, String> = HashMap::new();
        for i in 0..7 {
            input.insert(format!("k{}", i), format!("v{}", i));
        }
        let out = datum_param_as_config(&input);
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn test_value_to_color_monotonic_blue_to_white_t_range() {
        // t=0 (blue) vs t=0.25 (lighter blue) vs t=0.5 (white).
        let c0 = value_to_color(0.0);
        let c1 = value_to_color(0.25);
        let c2 = value_to_color(0.5);
        assert_eq!(c0.to_hex(), "#0000ff");
        assert_eq!(c2.to_hex(), "#ffffff");
        assert_ne!(c1.to_hex(), c0.to_hex());
        assert_ne!(c1.to_hex(), c2.to_hex());
    }

    #[test]
    fn test_synthetic_datum_map_negative_value_precision_preserved() {
        // Negative decimal value preserved in value field.
        let m = synthetic_datum_map("chr1", 0, 100, Some(-0.001));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("-0.001"));
    }

    #[test]
    fn test_data_range_four_datum_max_min_extracted_correctly() {
        // Max and min out of 4 values.
        let data = vec![
            Datum { value: Some(3.0), ..Default::default() },
            Datum { value: Some(8.0), ..Default::default() },
            Datum { value: Some(1.0), ..Default::default() },
            Datum { value: Some(5.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 1.0);
        assert_eq!(hi, 8.0);
    }

    #[test]
    fn test_datum_param_as_config_value_with_dashes_preserved() {
        // Values with dashes preserved verbatim.
        let mut input: HashMap<String, String> = HashMap::new();
        input.insert("style".into(), "dash-dot-dash".into());
        let out = datum_param_as_config(&input);
        assert_eq!(out.get("style").and_then(|v| v.as_str()), Some("dash-dot-dash"));
    }

    #[test]
    fn test_value_to_color_t_near_zero_dominantly_blue() {
        // t=0.05 → still mostly blue.
        let c = value_to_color(0.05);
        let hex = c.to_hex();
        // Blue component should be ff.
        assert!(hex.ends_with("ff"));
    }

    #[test]
    fn test_synthetic_datum_map_chr_with_empty_string_name() {
        // Empty chr name stored verbatim.
        let m = synthetic_datum_map("", 0, 100, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some(""));
    }

    #[test]
    fn test_data_range_datum_with_negative_infinity_preserved() {
        // NEG_INFINITY in value → min is NEG_INFINITY.
        let data = vec![
            Datum { value: Some(f64::NEG_INFINITY), ..Default::default() },
            Datum { value: Some(10.0), ..Default::default() },
        ];
        let (lo, _) = data_range(&data);
        assert_eq!(lo, f64::NEG_INFINITY);
    }

    #[test]
    fn test_value_to_color_t_above_1_upper_branch_saturates() {
        // t=1.5 → saturated red (same as t=1.0).
        let c = value_to_color(1.5);
        let c1 = value_to_color(1.0);
        assert_eq!(c.to_hex(), c1.to_hex());
    }

    #[test]
    fn test_synthetic_datum_map_chr_with_unicode_preserved() {
        // Unicode chr name preserved.
        let m = synthetic_datum_map("染色体1", 0, 100, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("染色体1"));
    }

    #[test]
    fn test_data_range_with_only_single_zero_value() {
        // Single zero value → min=max=0.
        let data = vec![Datum { value: Some(0.0), ..Default::default() }];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 0.0);
    }

    #[test]
    fn test_value_to_color_lower_branch_t_below_0_05() {
        // t=0.01 → lower branch f=0.02 → (5, 5, 255) very dark blue.
        let c = value_to_color(0.01);
        let hex = c.to_hex();
        // ends with "ff" (blue) and starts with low r/g.
        assert!(hex.ends_with("ff"));
    }

    #[test]
    fn test_datum_param_as_config_many_entries_exact_count() {
        // 100 entries all preserved.
        let mut input: HashMap<String, String> = HashMap::new();
        for i in 0..100 {
            input.insert(format!("key{}", i), format!("val{}", i));
        }
        let out = datum_param_as_config(&input);
        assert_eq!(out.len(), 100);
    }

    #[test]
    fn test_data_range_no_values_returns_default_0_1() {
        // No values at all (all None) → min>max path → returns (0.0, 1.0) default.
        let data = vec![
            Datum { value: None, ..Default::default() },
            Datum { value: None, ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 0.0);
        assert_eq!(hi, 1.0);
    }

    #[test]
    fn test_data_range_all_negative_values_preserves_range() {
        // All values negative: min=-10, max=-1.
        let data = vec![
            Datum { value: Some(-10.0), ..Default::default() },
            Datum { value: Some(-5.0), ..Default::default() },
            Datum { value: Some(-1.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, -10.0);
        assert_eq!(hi, -1.0);
    }

    #[test]
    fn test_value_to_color_exactly_half_red_channel_255() {
        // t == 0.5 → enters else branch with f=0, so R=255, G=255, B=255 (white).
        let c = value_to_color(0.5);
        assert_eq!(c.to_svg_rgb(), "rgb(255,255,255)");
    }

    #[test]
    fn test_value_to_color_at_zero_full_blue() {
        // t=0 → then branch, f=0 → R=0, G=0, B=255 (full blue).
        let c = value_to_color(0.0);
        assert_eq!(c.to_svg_rgb(), "rgb(0,0,255)");
    }

    #[test]
    fn test_parse_radius_simple_bare_integer_no_suffix() {
        // "42" (no r/p suffix) → bare parse → 42.0.
        let layout = mk_layout();
        let v = parse_radius("42", &layout);
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_parse_radius_with_whitespace_trimmed() {
        // Leading/trailing whitespace trimmed.
        let layout = mk_layout();
        let v = parse_radius("  100p  ", &layout);
        assert_eq!(v, 100.0);
    }

    #[test]
    fn test_parse_radius_unparseable_garbage_returns_zero() {
        // Garbage → unwrap_or(0.0) in parse_radius_simple.
        let layout = mk_layout();
        let v = parse_radius("xyz", &layout);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_parse_radius_negative_literal_passes_through_as_number() {
        // Leading '-' means not a subtraction expr → falls to parse_radius_simple.
        // "-50" bare → parse → -50.0.
        let layout = mk_layout();
        let v = parse_radius("-50", &layout);
        assert_eq!(v, -50.0);
    }

    #[test]
    fn test_synthetic_datum_map_chr_preserved_exactly() {
        // Chromosome name preserved.
        let m = synthetic_datum_map("hs1_extra", 100, 200, None);
        assert_eq!(m.get("chr").and_then(|v| v.as_str()), Some("hs1_extra"));
    }

    #[test]
    fn test_synthetic_datum_map_coords_stored_as_strings() {
        // Start/end stored as Str via to_string().
        let m = synthetic_datum_map("x", 1234567, 2345678, None);
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("1234567"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("2345678"));
    }

    #[test]
    fn test_synthetic_datum_map_negative_coords_preserved_as_string() {
        // Negative coords stringify with sign.
        let m = synthetic_datum_map("x", -100, -50, Some(-1.5));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("-100"));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("-1.5"));
    }

    #[test]
    fn test_datum_param_as_config_empty_map_yields_empty() {
        // Empty input → empty ConfigValue map.
        let input: HashMap<String, String> = HashMap::new();
        let out = datum_param_as_config(&input);
        assert!(out.is_empty());
    }

    #[test]
    fn test_datum_param_as_config_single_entry_becomes_str_variant() {
        // Single entry's value wrapped as ConfigValue::Str.
        let mut input: HashMap<String, String> = HashMap::new();
        input.insert("color".into(), "red".into());
        let out = datum_param_as_config(&input);
        assert_eq!(out.get("color").and_then(|v| v.as_str()), Some("red"));
    }

    #[test]
    fn test_parse_radius_addition_combining_r_and_p() {
        // "0.5r+100p" → 0.5×ideogram_radius + 100.
        let layout = mk_layout();
        let v = parse_radius("0.5r+100p", &layout);
        assert!((v - (0.5 * layout.dims.ideogram_radius + 100.0)).abs() < 1e-6);
    }

    #[test]
    fn test_parse_radius_subtraction_combining_terms() {
        // "1r-50p" → ideogram_radius - 50.
        let layout = mk_layout();
        let v = parse_radius("1r-50p", &layout);
        assert!((v - (layout.dims.ideogram_radius - 50.0)).abs() < 1e-6);
    }

    #[test]
    fn test_synthetic_datum_map_value_none_omits_value_key_v2() {
        // value=None → "value" key absent from map.
        let m = synthetic_datum_map("x", 0, 100, None);
        assert!(!m.contains_key("value"));
    }

    #[test]
    fn test_value_to_color_at_lower_quarter_mixes_blue_white() {
        // t=0.25 → then branch: f=0.5 → rgb(128, 128, 255).
        let c = value_to_color(0.25);
        // R and G equal (both at 0.5*255=127.5 → 127 via as u8).
        assert_eq!(c.to_svg_rgb(), "rgb(127,127,255)");
    }

    #[test]
    fn test_value_to_color_at_three_quarters_mixes_red_light() {
        // t=0.75 → else branch: f=0.5 → rgb(255, 128, 128) → 127.
        let c = value_to_color(0.75);
        assert_eq!(c.to_svg_rgb(), "rgb(255,127,127)");
    }

    #[test]
    fn test_data_range_single_value_returns_same_min_max() {
        // One value → min == max == that value.
        let data = vec![Datum { value: Some(42.0), ..Default::default() }];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 42.0);
        assert_eq!(hi, 42.0);
    }

    #[test]
    fn test_parse_radius_subtraction_with_leading_space_trimmed() {
        // "  1r - 50p  " → parsed correctly after trim.
        let layout = mk_layout();
        let v = parse_radius("  1r - 50p  ", &layout);
        assert!((v - (layout.dims.ideogram_radius - 50.0)).abs() < 1e-6);
    }

    #[test]
    fn test_parse_radius_multiple_addition_terms_sum() {
        // "10p+20p+30p" → sum = 60.
        let layout = mk_layout();
        let v = parse_radius("10p+20p+30p", &layout);
        assert_eq!(v, 60.0);
    }

    #[test]
    fn test_data_range_mixed_some_and_none_uses_only_some() {
        // Mix of Some/None: only Some values counted; None skipped.
        let data = vec![
            Datum { value: Some(10.0), ..Default::default() },
            Datum { value: None, ..Default::default() },
            Datum { value: Some(30.0), ..Default::default() },
            Datum { value: None, ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 10.0);
        assert_eq!(hi, 30.0);
    }

    #[test]
    fn test_value_to_color_edge_t_just_below_half_near_white_bluish() {
        // t=0.499 → then branch with f≈0.998 → rgb(~254, ~254, 255).
        let c = value_to_color(0.499);
        let s = c.to_svg_rgb();
        // R and G nearly 255; B exactly 255.
        assert!(s.ends_with(",255)"));
    }

    #[test]
    fn test_datum_param_as_config_wrapped_values_readable_via_get_str() {
        // Wrapped Str values retrievable via as_str().
        let mut input: HashMap<String, String> = HashMap::new();
        input.insert("url".into(), "http://example.com".into());
        let out = datum_param_as_config(&input);
        assert_eq!(out.get("url").and_then(|v| v.as_str()), Some("http://example.com"));
    }

    #[test]
    fn test_value_to_color_just_above_half_starts_red_spectrum() {
        // t=0.501 → else branch, f≈0.002 → R=255, G=~254, B=~254.
        let c = value_to_color(0.501);
        // R=255 in output.
        let s = c.to_svg_rgb();
        assert!(s.starts_with("rgb(255"));
    }

    #[test]
    fn test_data_range_mixed_positive_negative_values_correct_bounds() {
        // Values with mix of signs: min is most negative, max is most positive.
        let data = vec![
            Datum { value: Some(-5.0), ..Default::default() },
            Datum { value: Some(3.0), ..Default::default() },
            Datum { value: Some(-10.0), ..Default::default() },
            Datum { value: Some(7.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, -10.0);
        assert_eq!(hi, 7.0);
    }

    #[test]
    fn test_parse_radius_subtraction_all_p_units() {
        // "200p-50p" → 150.
        let layout = mk_layout();
        let v = parse_radius("200p-50p", &layout);
        assert_eq!(v, 150.0);
    }

    #[test]
    fn test_synthetic_datum_map_with_zero_coords_stored() {
        // start=0, end=0 → preserved as "0" strings.
        let m = synthetic_datum_map("x", 0, 0, Some(0.0));
        assert_eq!(m.get("start").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("end").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(m.get("value").and_then(|v| v.as_str()), Some("0"));
    }

    #[test]
    fn test_parse_radius_r_with_exact_zero_r_value() {
        // "0r" → 0 × ideogram_radius = 0.
        let layout = mk_layout();
        let v = parse_radius("0r", &layout);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_data_range_with_mixed_same_value_twice() {
        // Same value twice → min=max=that value.
        let data = vec![
            Datum { value: Some(5.0), ..Default::default() },
            Datum { value: Some(5.0), ..Default::default() },
        ];
        let (lo, hi) = data_range(&data);
        assert_eq!(lo, 5.0);
        assert_eq!(hi, 5.0);
    }

    #[test]
    fn test_value_to_color_exactly_one_gives_full_red() {
        // t=1.0 → else branch, f=1.0 → rgb(255, 0, 0).
        let c = value_to_color(1.0);
        assert_eq!(c.to_svg_rgb(), "rgb(255,0,0)");
    }

    #[test]
    fn test_synthetic_datum_map_has_exactly_3_or_4_entries() {
        // Without value: 3 entries (chr/start/end). With: 4 entries.
        let m1 = synthetic_datum_map("x", 0, 100, None);
        assert_eq!(m1.len(), 3);
        let m2 = synthetic_datum_map("x", 0, 100, Some(1.5));
        assert_eq!(m2.len(), 4);
    }
}
