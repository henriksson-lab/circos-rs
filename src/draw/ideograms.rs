use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::karyotype::types::Karyotype;
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::{svg_slice, svg_text, SvgDocument};

/// Draw all ideograms: fills, cytogenetic bands, outlines, and labels.
pub fn draw_ideograms(
    doc: &mut SvgDocument,
    layout: &Layout,
    conf: &HashMap<String, ConfigValue>,
    karyotype: &Karyotype,
    colors: &ColorMap,
) {
    let ideogram_conf = conf.get("ideogram").and_then(|v| v.as_map());

    let stroke_color_name = ideogram_conf
        .and_then(|m| m.get("stroke_color"))
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let stroke_thickness: f64 = ideogram_conf
        .and_then(|m| m.get("stroke_thickness"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let fill = ideogram_conf
        .and_then(|m| m.get("fill"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let fill_color_name = ideogram_conf
        .and_then(|m| m.get("fill_color"))
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let show_bands = ideogram_conf
        .and_then(|m| m.get("show_bands"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let fill_bands = ideogram_conf
        .and_then(|m| m.get("fill_bands"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let band_stroke_thickness: f64 = ideogram_conf
        .and_then(|m| m.get("band_stroke_thickness"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let band_transparency: u8 = ideogram_conf
        .and_then(|m| m.get("band_transparency"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let show_label = ideogram_conf
        .and_then(|m| m.get("show_label"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let label_size: f64 = ideogram_conf
        .and_then(|m| m.get("label_size"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(36.0);

    let stroke_color = colors.resolve(stroke_color_name).unwrap_or(Color::rgb(0, 0, 0));
    let default_fill_color = colors.resolve(fill_color_name).unwrap_or(Color::rgb(0, 0, 0));
    let label_color = colors.resolve("black").unwrap_or(Color::rgb(0, 0, 0));

    doc.open_group("ideograms");

    for ideo in &layout.ideograms {
        let chr = &ideo.chr;
        let start = ideo.set.min().unwrap_or(0);
        let end = ideo.set.max().unwrap_or(0);

        let start_a = layout.get_angle(start, chr).unwrap_or(0.0);
        let end_a = layout.get_angle(end, chr).unwrap_or(0.0);

        let radius_outer = if ideo.radius_outer > 0.0 {
            ideo.radius_outer
        } else {
            layout.dims.ideogram_radius_outer
        };
        let radius_inner = if ideo.radius_inner > 0.0 {
            ideo.radius_inner
        } else {
            layout.dims.ideogram_radius_inner
        };

        // Draw ideogram fill
        let chr_color = if fill {
            colors
                .resolve(&ideo.color)
                .or_else(|| Some(default_fill_color))
        } else {
            None
        };

        let slice_svg = svg_slice(
            layout,
            start_a,
            end_a,
            radius_inner,
            radius_outer,
            Some(&stroke_color),
            Some(stroke_thickness),
            chr_color.as_ref(),
            None,
        );
        doc.add(slice_svg);

        // Draw cytogenetic bands
        if show_bands {
            if let Some(bands) = karyotype.bands.get(chr) {
                for band in bands {
                    // Intersect band with ideogram region
                    let band_set = band.set.intersect(&ideo.set);
                    if band_set.cardinality() < 1 {
                        continue;
                    }
                    let band_start = band_set.min().unwrap();
                    let band_end = band_set.max().unwrap();
                    let band_start_a = layout.get_angle(band_start, chr).unwrap_or(0.0);
                    let band_end_a = layout.get_angle(band_end, chr).unwrap_or(0.0);

                    let band_fill = if fill_bands {
                        let mut color = colors.resolve(&band.color);
                        if band_transparency > 0 {
                            if let Some(c) = &color {
                                let alpha =
                                    ((band_transparency as f64 / 5.0) * 255.0).min(255.0) as u8;
                                color = Some(Color::rgba(c.r, c.g, c.b, alpha));
                            }
                        }
                        color
                    } else {
                        None
                    };

                    let opacity = if band_transparency > 0 {
                        Some(band_transparency as f64 / 5.0)
                    } else {
                        None
                    };

                    let band_svg = svg_slice(
                        layout,
                        band_start_a,
                        band_end_a,
                        radius_inner,
                        radius_outer,
                        Some(&stroke_color),
                        Some(band_stroke_thickness),
                        band_fill.as_ref(),
                        opacity,
                    );
                    doc.add(band_svg);
                }
            }
        }

        // Draw ideogram outline (stroke only, no fill) - second pass
        if stroke_thickness > 0.0 {
            let outline_svg = svg_slice(
                layout,
                start_a,
                end_a,
                radius_inner,
                radius_outer,
                Some(&stroke_color),
                Some(stroke_thickness),
                None,
                None,
            );
            doc.add(outline_svg);
        }

        // Draw chromosome label
        if show_label {
            let mid_pos = (start + end) / 2;
            let mid_angle = layout.get_angle(mid_pos, chr).unwrap_or(0.0);

            // Place label outside the ideogram
            let label_radius = radius_outer + label_size * 0.8;

            // Compute text rotation for readability
            let text_angle = compute_label_rotation(mid_angle);

            let text_svg = svg_text(
                layout,
                mid_angle,
                label_radius,
                &ideo.label,
                label_size,
                &label_color,
                text_angle,
            );
            doc.add(text_svg);
        }
    }

    doc.close_group();
}

/// Compute text rotation angle for a label at a given position angle.
/// Labels on the right half of the circle are upright, left half are flipped.
fn compute_label_rotation(angle_deg: f64) -> f64 {
    // Normalize to 0-360
    let a = ((angle_deg % 360.0) + 360.0) % 360.0;

    // Convert from Circos angle (0=3 o'clock, clockwise) to SVG rotation
    // For readability: labels at top/right stay upright, bottom/left get flipped
    let rotation = a;
    if rotation > 90.0 && rotation < 270.0 {
        // Left half: rotate 180 degrees so text reads left-to-right
        rotation + 180.0
    } else {
        rotation
    }
}
