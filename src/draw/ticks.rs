use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::karyotype::types::Karyotype;
use crate::layout::units;
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::{svg_text, svg_tick, SvgDocument};

/// Draw tick marks and labels for all ideograms.
pub fn draw_ticks(
    doc: &mut SvgDocument,
    layout: &Layout,
    conf: &HashMap<String, ConfigValue>,
    _karyotype: &Karyotype,
    colors: &ColorMap,
) {
    let ticks_conf = match conf.get("ticks").and_then(|v| v.as_map()) {
        Some(m) => m,
        None => return,
    };

    let show_tick_labels = conf
        .get("show_tick_labels")
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);

    let units_ok = conf
        .get("units_ok")
        .and_then(|v| v.as_str())
        .unwrap_or("bupr");
    let units_nounit = conf
        .get("units_nounit")
        .and_then(|v| v.as_str())
        .unwrap_or("n");

    // Get default tick properties from the <ticks> block
    let default_color_name = ticks_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let default_color = colors.resolve(default_color_name).unwrap_or(Color::rgb(0, 0, 0));

    let multiplier: f64 = ticks_conf
        .get("multiplier")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);

    let default_label_size: f64 = ticks_conf
        .get("label_size")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(8.0);

    // Get list of tick definitions
    let tick_defs = match ticks_conf.get("tick") {
        Some(ConfigValue::List(list)) => list.clone(),
        Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
        _ => return,
    };

    doc.open_group("ticks");

    for tick_def in &tick_defs {
        let tick_map = match tick_def.as_map() {
            Some(m) => m,
            None => continue,
        };

        // Parse tick properties
        let spacing_str = match tick_map.get("spacing").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        let (spacing_val, spacing_unit) =
            match units::unit_split(spacing_str, units_ok, units_nounit) {
                Ok(v) => v,
                Err(_) => continue,
            };

        let spacing_bp = match spacing_unit.as_str() {
            "u" => spacing_val * layout.chromosomes_units,
            "b" => spacing_val,
            _ => spacing_val,
        };

        let size: f64 = tick_map
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_end_matches('p').parse().ok())
            .unwrap_or(5.0);

        let thickness: f64 = tick_map
            .get("thickness")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_end_matches('p').parse().ok())
            .unwrap_or(2.0);

        let tick_color_name = tick_map
            .get("color")
            .and_then(|v| v.as_str())
            .unwrap_or(default_color_name);
        let tick_color = colors.resolve(tick_color_name).unwrap_or(default_color);

        let tick_show_label = tick_map
            .get("show_label")
            .and_then(|v| v.as_str())
            .map(|s| s == "1")
            .unwrap_or(false);

        let label_size: f64 = tick_map
            .get("label_size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_end_matches('p').parse().ok())
            .unwrap_or(default_label_size);

        let label_offset: f64 = tick_map
            .get("label_offset")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_end_matches('p').parse().ok())
            .unwrap_or(0.0);

        let format_str = tick_map
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("%d");

        let show_grid = tick_map
            .get("grid")
            .and_then(|v| v.as_str())
            .map(|s| s == "1")
            .unwrap_or(false);

        let grid_color_name = tick_map
            .get("grid_color")
            .and_then(|v| v.as_str())
            .unwrap_or("grey");
        let grid_color = colors.resolve(grid_color_name).unwrap_or(Color::rgb(200, 200, 200));

        let grid_thickness: f64 = tick_map
            .get("grid_thickness")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_end_matches('p').parse().ok())
            .unwrap_or(1.0);

        // Draw ticks for each ideogram
        for ideo in &layout.ideograms {
            let chr = &ideo.chr;
            let chr_start = ideo.set.min().unwrap_or(0);
            let chr_end = ideo.set.max().unwrap_or(0);

            let radius_outer = if ideo.radius_outer > 0.0 {
                ideo.radius_outer
            } else {
                layout.dims.ideogram_radius_outer
            };

            // Generate tick positions
            let first_tick = ((chr_start as f64 / spacing_bp).ceil() * spacing_bp) as i64;
            let mut pos = first_tick;
            while pos <= chr_end {
                if let Some(angle) = layout.get_angle(pos, chr) {
                    // Draw tick mark
                    let tick_svg = svg_tick(
                        layout,
                        angle,
                        radius_outer,
                        radius_outer + size,
                        thickness,
                        &tick_color,
                    );
                    doc.add(tick_svg);

                    // Draw tick label
                    if tick_show_label && show_tick_labels {
                        let label_value = pos as f64 * multiplier;
                        let label_text = format_tick_label(format_str, label_value);
                        let label_radius = radius_outer + size + label_offset + label_size * 0.5;
                        let text_svg = svg_text(
                            layout,
                            angle,
                            label_radius,
                            &label_text,
                            label_size,
                            &tick_color,
                            0.0,
                        );
                        doc.add(text_svg);
                    }

                    // Draw grid line
                    if show_grid {
                        // Grid: radial line from inner to specified extent
                        let grid_inner = layout.dims.ideogram_radius_inner;
                        let grid_svg = svg_tick(
                            layout,
                            angle,
                            grid_inner,
                            radius_outer,
                            grid_thickness,
                            &grid_color,
                        );
                        doc.add(grid_svg);
                    }
                }

                pos += spacing_bp as i64;
            }
        }
    }

    doc.close_group();
}

/// Format a tick label value according to a printf-style format string.
fn format_tick_label(format: &str, value: f64) -> String {
    match format {
        "%d" => format!("{}", value as i64),
        "%f" => format!("{}", value),
        "%.1f" => format!("{:.1}", value),
        "%.2f" => format!("{:.2}", value),
        _ => format!("{}", value as i64),
    }
}
