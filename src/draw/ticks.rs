use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::draw::ideograms::slice_polygon_coords;
use crate::draw::report_image_map;
use crate::karyotype::types::Karyotype;
use crate::layout::Layout;
use crate::layout::units;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::{SvgDocument, svg_text, svg_tick};
use crate::utils::format_url;

/// Helper for reading a parameter with tick→global fallback (Perl
/// `seek_parameter("name", $tick, $CONF{ticks})`).
fn seek_tick<'a>(
    tick: &'a HashMap<String, ConfigValue>,
    ticks_conf: &'a HashMap<String, ConfigValue>,
    name: &str,
) -> Option<&'a ConfigValue> {
    // Handle `|`-separated synonyms like Perl seek_parameter does.
    for n in name.split('|') {
        if let Some(v) = tick.get(n) {
            return Some(v);
        }
        if let Some(v) = ticks_conf.get(n) {
            return Some(v);
        }
    }
    None
}

/// Convenience wrapper over `seek_tick` that returns the underlying `&str`
/// when the resolved `ConfigValue` is a string.
fn seek_tick_str<'a>(
    tick: &'a HashMap<String, ConfigValue>,
    ticks_conf: &'a HashMap<String, ConfigValue>,
    name: &str,
) -> Option<&'a str> {
    seek_tick(tick, ticks_conf, name).and_then(|v| v.as_str())
}

/// Convenience wrapper over `seek_tick_str` that strips an optional trailing
/// `p` suffix and parses the value as `f64`.
fn seek_tick_f64(
    tick: &HashMap<String, ConfigValue>,
    ticks_conf: &HashMap<String, ConfigValue>,
    name: &str,
) -> Option<f64> {
    seek_tick_str(tick, ticks_conf, name).and_then(|s| s.trim_end_matches('p').parse().ok())
}

/// Port of Perl `$CONF{fonts}{$font}` lookup: resolves a font key (e.g.
/// "default", "bold") to a TTF file path. Returns an empty string if the
/// lookup misses — `text_size` then falls back to its char-count heuristic.
pub(crate) fn resolve_font(conf: &HashMap<String, ConfigValue>, font_key: &str) -> String {
    conf.get("fonts")
        .and_then(|v| v.as_map())
        .and_then(|m| m.get(font_key).or_else(|| m.get("default")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Port of Perl `draw_ticks`. The Perl body is 380 LOC: it
///   1. parses the chromosomes filter via parse_ideogram_filter/merge_ideogram_filters
///      for each tick block (so `chromosomes=` in a tick overrides global)
///   2. runs process_tick_structure on every tick (resolves spacing/position/dims)
///   3. walks positions (spacing-based iteration OR explicit position list)
///   4. enforces force_display via pos_ticked tracking
///   5. honors chromosomes_display_default + chromosomes filter per tick
///   6. applies tick_separation (pixel-gap threshold) and min_distance_to_edge
///   7. computes tick_radius with offset (global + per-tick)
///   8. branches orientation=in vs out for r0/r1
///   9. builds label text honoring mod/multiplier/rmultiplier/rdivisor/format/
///      thousands_sep/prefix/suffix/label
///   10. handles label_rotate=no heuristic for horizontal labels
///   11. emits per-tick SVG + optional grid + label
///   12. post-pass: suppress label overlaps within a tick group via
///       label_separation
///
/// This Rust port mirrors the same phases; per-phase options are read through
/// `seek_tick` which mimics Perl's tick→global fallback. Parts that depend
/// on still-to-be-ported modules (process_tick_structure, label_bounds font
/// metrics, image-map output) are handled inline with approximations.
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

    let show_tick_labels_global = conf
        .get("show_tick_labels")
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(true);
    let chromosomes_display_default = conf
        .get("chromosomes_display_default")
        .and_then(|v| v.as_str())
        .map(|s| s == "1" || s == "yes")
        .unwrap_or(true);
    let units_ok = conf
        .get("units_ok")
        .and_then(|v| v.as_str())
        .unwrap_or("bupr");
    let units_nounit = conf
        .get("units_nounit")
        .and_then(|v| v.as_str())
        .unwrap_or("n");
    let chromosomes_units: f64 = conf
        .get("chromosomes_units")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);

    // Default tick appearance from the <ticks> block
    let default_color_name = ticks_conf
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("black");
    let default_color = colors
        .resolve(default_color_name)
        .unwrap_or(Color::rgb(0, 0, 0));
    let default_label_size: f64 = ticks_conf
        .get("label_size")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(8.0);
    let global_tick_offset: f64 = ticks_conf
        .get("offset")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('p').parse().ok())
        .unwrap_or(0.0);


    // Tick definitions: one per <tick> sub-block
    let tick_defs = match ticks_conf.get("tick") {
        Some(ConfigValue::List(list)) => list.clone(),
        Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
        _ => return,
    };

    // Parse global chromosomes filter (Perl: parse_ideogram_filter($CONF{ticks}{chromosomes}))
    let global_chrs_filter = ticks_conf
        .get("chromosomes")
        .and_then(|v| v.as_str())
        .map(|s| crate::chromosome::parse_ideogram_filter(Some(s), Some(chromosomes_units)))
        .unwrap_or_default();

    doc.open_group("ticks");

    for tick_def in &tick_defs {
        let tick_map = match tick_def.as_map() {
            Some(m) => m,
            None => continue,
        };

        // Per-tick chromosomes filter merged with global — Perl keeps these in
        // $tick->{_ideogram}.
        let per_tick_chrs_str = tick_map.get("chromosomes").and_then(|v| v.as_str());
        let per_tick_filter = per_tick_chrs_str
            .map(|s| crate::chromosome::parse_ideogram_filter(Some(s), Some(chromosomes_units)))
            .unwrap_or_default();
        let merged_filter = crate::chromosome::merge_ideogram_filters(&[
            global_chrs_filter.clone(),
            per_tick_filter,
        ]);
        let show_default = seek_tick_str(tick_map, ticks_conf, "chromosomes_display_default")
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(chromosomes_display_default);

        // Absolute (spacing) vs relative (rspacing) vs explicit positions
        let spacing_type = seek_tick_str(tick_map, ticks_conf, "spacing_type").unwrap_or("");
        let is_relative = spacing_type == "relative";
        let rdivisor_is_ideogram =
            seek_tick_str(tick_map, ticks_conf, "rdivisor|label_rdivisor") == Some("ideogram");

        let spacing_str = seek_tick_str(tick_map, ticks_conf, "spacing");
        let position_str = seek_tick_str(tick_map, ticks_conf, "position");
        let rspacing: Option<f64> =
            seek_tick_str(tick_map, ticks_conf, "rspacing").and_then(|s| s.parse().ok());
        let rposition: Option<String> =
            seek_tick_str(tick_map, ticks_conf, "rposition").map(|s| s.to_string());

        // Resolve basic tick geometry
        let spacing_bp_from_spacing: Option<f64> = spacing_str.and_then(|s| {
            units::unit_split(s, units_ok, units_nounit)
                .ok()
                .map(|(v, unit)| match unit.as_str() {
                    "u" => v * chromosomes_units,
                    _ => v,
                })
        });

        let size: f64 = seek_tick_f64(tick_map, ticks_conf, "size").unwrap_or(5.0);
        let thickness: f64 = seek_tick_f64(tick_map, ticks_conf, "thickness").unwrap_or(2.0);
        let tick_offset: f64 =
            seek_tick_f64(tick_map, ticks_conf, "offset").unwrap_or(0.0) + global_tick_offset;
        let orientation = seek_tick_str(tick_map, ticks_conf, "orientation").unwrap_or("out");
        let force_display = seek_tick_str(tick_map, ticks_conf, "force_display")
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false);
        let tick_separation: Option<f64> = seek_tick_str(tick_map, ticks_conf, "tick_separation")
            .and_then(|s| s.trim_end_matches('p').parse::<f64>().ok());
        let min_distance_to_edge: Option<f64> =
            seek_tick_str(tick_map, ticks_conf, "min_distance_to_edge")
                .and_then(|s| s.trim_end_matches('p').parse::<f64>().ok());

        let tick_color_name =
            seek_tick_str(tick_map, ticks_conf, "color").unwrap_or(default_color_name);
        let tick_color = colors.resolve(tick_color_name).unwrap_or(default_color);

        let tick_show_label = seek_tick_str(tick_map, ticks_conf, "show_label")
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false);
        let label_size: f64 =
            seek_tick_f64(tick_map, ticks_conf, "label_size").unwrap_or(default_label_size);
        let label_offset: f64 = seek_tick_f64(tick_map, ticks_conf, "label_offset").unwrap_or(0.0);
        let label_rotate = seek_tick_str(tick_map, ticks_conf, "label_rotate")
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(true);
        let label_relative = seek_tick_str(tick_map, ticks_conf, "label_relative")
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false);
        let format_str = seek_tick_str(tick_map, ticks_conf, "format").unwrap_or("%d");
        let multiplier: f64 = seek_tick_str(tick_map, ticks_conf, "multiplier|label_multiplier")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        let rmultiplier: f64 = seek_tick_str(tick_map, ticks_conf, "rmultiplier|label_rmultiplier")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        let mod_value: Option<f64> =
            seek_tick_str(tick_map, ticks_conf, "mod").and_then(|s| s.parse().ok());
        let thousands_sep =
            seek_tick_str(tick_map, ticks_conf, "thousands_sep|thousands_separator");
        let prefix = seek_tick_str(tick_map, ticks_conf, "prefix").unwrap_or("");
        let suffix = seek_tick_str(tick_map, ticks_conf, "suffix").unwrap_or("");
        let explicit_label = seek_tick_str(tick_map, ticks_conf, "label");

        let show_grid = seek_tick_str(tick_map, ticks_conf, "grid")
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false);
        let grid_color_name = seek_tick_str(tick_map, ticks_conf, "grid_color").unwrap_or("grey");
        let grid_color = colors
            .resolve(grid_color_name)
            .unwrap_or(Color::rgb(200, 200, 200));
        let grid_thickness: f64 =
            seek_tick_f64(tick_map, ticks_conf, "grid_thickness").unwrap_or(1.0);

        // Perl force_display tracking: pos_ticked[tick_radius][pos]++
        let mut pos_ticked: std::collections::HashSet<(i64, i64)> =
            std::collections::HashSet::new();

        for ideo in &layout.ideograms {
            let chr = &ideo.chr;
            let chr_start = ideo.set.min().unwrap_or(0);
            let chr_end = ideo.set.max().unwrap_or(0);
            let chrlength = chr_end - chr_start;
            let tag = if ideo.tag.is_empty() {
                chr.clone()
            } else {
                ideo.tag.clone()
            };

            let radius_outer = if ideo.radius_outer > 0.0 {
                ideo.radius_outer
            } else {
                layout.dims.ideogram_radius_outer
            };
            let this_tick_radius = radius_outer + tick_offset;

            // Compute positions list
            let positions: Vec<i64> = if is_relative {
                if let Some(rs) = rspacing {
                    let count = if rdivisor_is_ideogram {
                        (ideo.set.cardinality() as f64 * rs) as i64
                    } else {
                        (chrlength as f64 * rs) as i64
                    };
                    let mut v = Vec::new();
                    let mut p = chr_start;
                    while p <= chr_end && count > 0 {
                        v.push(p);
                        p += count.max(1);
                    }
                    v
                } else if let Some(rp) = &rposition {
                    let divisor = if rdivisor_is_ideogram {
                        ideo.set.cardinality() as f64
                    } else {
                        chrlength as f64
                    };
                    rp.split(',')
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .map(|v| chr_start + (v * divisor) as i64)
                        .collect()
                } else {
                    Vec::new()
                }
            } else if let Some(spacing_bp) = spacing_bp_from_spacing {
                if spacing_bp <= 0.0 {
                    continue;
                }
                let first = ((chr_start as f64 / spacing_bp).ceil() * spacing_bp) as i64;
                let last = ((chr_end as f64 / spacing_bp).floor() * spacing_bp) as i64;
                let mut v = Vec::new();
                let mut p = first;
                while p <= last {
                    v.push(p);
                    p += spacing_bp as i64;
                }
                v
            } else if let Some(ps) = position_str {
                let mut v: Vec<i64> = ps
                    .split(',')
                    .filter_map(|s| {
                        let s = s.trim();
                        match s {
                            "start" => Some(chr_start),
                            "end" => Some(chr_end),
                            _ => s.parse().ok(),
                        }
                    })
                    .collect();
                v.sort();
                v
            } else {
                continue;
            };

            let mut last_tick_angle: Option<f64> = None;
            let mut last_label_end_a: Option<f64> = None;
            let label_separation: Option<f64> =
                seek_tick_str(tick_map, ticks_conf, "label_separation")
                    .and_then(|s| s.trim_end_matches('p').parse::<f64>().ok());
            let mut drawn_any = false;
            let positions_len = positions.len();
            for (pos_idx, &pos) in positions.iter().enumerate() {
                if !ideo.set.member(pos) {
                    continue;
                }

                // force_display / pos_ticked suppression
                let radius_key = this_tick_radius.round() as i64;
                if !force_display && !pos_ticked.insert((radius_key, pos)) {
                    continue;
                }

                // chromosomes filter
                let is_suppressed = if show_default {
                    merged_filter
                        .get(&tag)
                        .or_else(|| merged_filter.get(chr))
                        .and_then(|cf| cf.hide.as_ref())
                        .map(|h| h.member(pos))
                        .unwrap_or(false)
                } else {
                    !merged_filter
                        .get(&tag)
                        .or_else(|| merged_filter.get(chr))
                        .and_then(|cf| cf.combined.as_ref())
                        .map(|c| c.member(pos))
                        .unwrap_or(false)
                };
                if is_suppressed {
                    continue;
                }

                let tick_angle = match layout.getanglepos(pos, chr) {
                    Some(a) => a,
                    None => continue,
                };

                // tick_separation enforcement
                if let (Some(min_sep), Some(last_a)) = (tick_separation, last_tick_angle) {
                    let pix_sep = this_tick_radius * std::f64::consts::PI / 180.0
                        * (tick_angle - last_a).abs();
                    if pix_sep < min_sep {
                        continue;
                    }
                }

                // min_distance_to_edge
                if let Some(edge_d) = min_distance_to_edge {
                    let start_a = layout.getanglepos(chr_start, chr).unwrap_or(tick_angle);
                    let end_a = layout.getanglepos(chr_end, chr).unwrap_or(tick_angle);
                    let deg2rad = std::f64::consts::PI / 180.0;
                    let d_start = this_tick_radius * deg2rad * (tick_angle - start_a).abs();
                    let d_end = this_tick_radius * deg2rad * (tick_angle - end_a).abs();
                    let d_min = d_start.min(d_end);
                    if d_min < edge_d {
                        continue;
                    }
                }

                // r0/r1 from orientation
                let (r0, r1) = if orientation == "in" {
                    (this_tick_radius - size, this_tick_radius)
                } else {
                    (this_tick_radius, this_tick_radius + size)
                };

                // Emit tick mark
                let tick_svg = svg_tick(layout, tick_angle, r0, r1, thickness, &tick_color);
                doc.add(tick_svg);

                // Emit grid line
                if show_grid {
                    let grid_inner = layout.dims.ideogram_radius_inner;
                    let grid_svg = svg_tick(
                        layout,
                        tick_angle,
                        grid_inner,
                        radius_outer,
                        grid_thickness,
                        &grid_color,
                    );
                    doc.add(grid_svg);
                }

                // Label
                if tick_show_label && show_tick_labels_global {
                    // Compute position-relative value for rmultiplier/label_relative/mod
                    let pos_relative = if rdivisor_is_ideogram && ideo.set.cardinality() > 0 {
                        (pos - ideo.set.min().unwrap_or(0)) as f64 / ideo.set.cardinality() as f64
                    } else if chrlength > 0 {
                        pos as f64 / chrlength as f64
                    } else {
                        0.0
                    };

                    let label_value = if let Some(md) = mod_value {
                        let modded = (pos as f64) % md;
                        if label_relative {
                            (modded / md) * rmultiplier
                        } else {
                            modded * multiplier
                        }
                    } else if label_relative {
                        pos_relative * rmultiplier
                    } else {
                        pos as f64 * multiplier
                    };

                    let mut label_text = format_tick_label(format_str, label_value);
                    if thousands_sep.is_some() {
                        label_text = crate::utils::add_thousands_separator(&label_text, ',');
                    }
                    if !suffix.is_empty() {
                        label_text.push_str(suffix);
                    }
                    if !prefix.is_empty() {
                        label_text.insert_str(0, prefix);
                    }
                    if let Some(el) = explicit_label {
                        label_text = el.to_string();
                    }

                    let label_radius = if orientation == "in" {
                        r0 - label_offset - label_size
                    } else {
                        r1 + label_offset
                    };

                    // first/last skipping
                    let skip_first =
                        seek_tick_str(tick_map, ticks_conf, "skip_first_label") == Some("1");
                    let skip_last =
                        seek_tick_str(tick_map, ticks_conf, "skip_last_label") == Some("1");
                    // label_separation post-pass (Perl's post-loop does this by
                    // collecting tick_with_label; we approximate inline by
                    // checking the previous emitted label's end_a vs this tick).
                    // Real font metrics via fontdue (iter 70) when the font
                    // file is resolvable, heuristic otherwise.
                    let font_key = seek_tick_str(tick_map, ticks_conf, "label_font")
                        .unwrap_or("default");
                    let font_file = resolve_font(conf, font_key);
                    let approx_label_width =
                        crate::draw::text::text_size(&font_file, label_size, &label_text).0;
                    let this_label_start_a = tick_angle
                        - 0.5
                            * (approx_label_width / label_radius.max(1.0))
                            * (180.0 / std::f64::consts::PI);
                    let this_label_end_a = tick_angle
                        + 0.5
                            * (approx_label_width / label_radius.max(1.0))
                            * (180.0 / std::f64::consts::PI);
                    let label_too_close =
                        if let (Some(sep), Some(prev_end)) = (label_separation, last_label_end_a) {
                            let pix_gap = label_radius * std::f64::consts::PI / 180.0
                                * (this_label_start_a - prev_end).abs();
                            pix_gap < sep
                        } else {
                            false
                        };
                    let skip_via_first = skip_first && pos_idx == 0;
                    let skip_via_last = skip_last && pos_idx == positions_len - 1;

                    if skip_via_first || skip_via_last || label_too_close {
                        // skip this label; keep last_label_end_a as-is for next iter
                    } else {
                        let text_angle = if label_rotate {
                            crate::draw::ideograms::textangle(tick_angle, false)
                        } else {
                            0.0
                        };
                        let text_svg = svg_text(
                            layout,
                            tick_angle,
                            label_radius,
                            &label_text,
                            label_size,
                            &tick_color,
                            text_angle,
                        );
                        doc.add(text_svg);
                        last_label_end_a = Some(this_label_end_a);
                    }
                }

                drawn_any = true;
                last_tick_angle = Some(tick_angle);
            }
            let _ = drawn_any;
        }

        // --- URL-ticks image-map pass (Perl: group ticks by r0/spacing, for
        //     each tick with a `url` parameter emit an image-map slice region
        //     spanning from the previous tick to this one; extend the first
        //     region back to ideogram start and the last forward to end). ---
        let url_template = seek_tick_str(tick_map, ticks_conf, "url").map(str::to_string);
        if let Some(url_tpl) = url_template.as_deref() {
            let map_radius_inner = seek_tick_f64(tick_map, ticks_conf, "map_radius_inner");
            let map_radius_outer = seek_tick_f64(tick_map, ticks_conf, "map_radius_outer");
            let map_size = seek_tick_f64(tick_map, ticks_conf, "map_size");
            let missing_policy = conf
                .get("image")
                .and_then(|v| v.get("image_map_missing_parameter"))
                .and_then(|v| v.as_str())
                .unwrap_or("removeparam")
                .to_string();
            for ideo in &layout.ideograms {
                let chr = &ideo.chr;
                let chr_start = ideo.set.min().unwrap_or(0);
                let chr_end = ideo.set.max().unwrap_or(0);
                let radius_outer = if ideo.radius_outer > 0.0 {
                    ideo.radius_outer
                } else {
                    layout.dims.ideogram_radius_outer
                };

                // Recompute positions for this tick block + ideogram — mirror the
                // positions[] generator above but simpler: we only need sorted
                // tick positions within the ideogram.
                let spacing_bp_from_spacing = seek_tick_str(tick_map, ticks_conf, "spacing")
                    .and_then(|s| s.trim_end_matches('u').trim_end_matches('b').parse::<f64>().ok())
                    .map(|v| v * chromosomes_units);
                let mut positions: Vec<i64> = Vec::new();
                if let Some(sp) = spacing_bp_from_spacing
                    && sp > 0.0
                {
                    let first = ((chr_start as f64 / sp).ceil() * sp) as i64;
                    let last = ((chr_end as f64 / sp).floor() * sp) as i64;
                    let mut p = first;
                    while p <= last {
                        positions.push(p);
                        p += sp as i64;
                    }
                }
                if positions.is_empty() {
                    continue;
                }
                // Determine r0/r1 for the image-map slice (Perl: tick->r0 and
                // map_radius_outer OR tick->r1 + map_size OR tick->r1).
                let r0 = map_radius_inner.unwrap_or(radius_outer);
                let r1 = map_radius_outer
                    .or(map_size.map(|s| r0 + s))
                    .unwrap_or(radius_outer);

                // Emit one poly area per adjacent tick pair (Perl's prev_pos → pos
                // slice). First region extends from chr_start to positions[0]; last
                // from positions[-1] to chr_end.
                let mut prev_pos = chr_start;
                for &pos in &positions {
                    let start_a = match layout.getanglepos(prev_pos, chr) {
                        Some(a) => a,
                        None => {
                            prev_pos = pos;
                            continue;
                        }
                    };
                    let end_a = match layout.getanglepos(pos, chr) {
                        Some(a) => a,
                        None => {
                            prev_pos = pos;
                            continue;
                        }
                    };
                    let dp = crate::draw::plots::synthetic_datum_map(chr, prev_pos, pos, None);
                    if let Ok(Some(url)) = format_url(url_tpl, &[&dp], &missing_policy) {
                        let coords = slice_polygon_coords(
                            layout.image_radius,
                            start_a,
                            end_a,
                            r0.min(r1),
                            r0.max(r1),
                        );
                        report_image_map("poly", &coords, &url);
                    }
                    prev_pos = pos;
                }
                // Final region from last tick to chr_end.
                if let (Some(start_a), Some(end_a)) = (
                    layout.getanglepos(prev_pos, chr),
                    layout.getanglepos(chr_end, chr),
                ) {
                    let dp = crate::draw::plots::synthetic_datum_map(
                        chr, prev_pos, chr_end, None,
                    );
                    if let Ok(Some(url)) = format_url(url_tpl, &[&dp], &missing_policy) {
                        let coords = slice_polygon_coords(
                            layout.image_radius,
                            start_a,
                            end_a,
                            r0.min(r1),
                            r0.max(r1),
                        );
                        report_image_map("poly", &coords, &url);
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_font_lookup_then_default_then_empty() {
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/fonts/default.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/fonts/bold.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));

        // Direct hit
        assert_eq!(resolve_font(&conf, "bold"), "/fonts/bold.ttf");
        // Missing key → falls back to "default"
        assert_eq!(resolve_font(&conf, "nonexistent"), "/fonts/default.ttf");
        // No fonts map → empty string
        let empty: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(resolve_font(&empty, "bold"), "");
    }

    #[test]
    fn test_resolve_font_missing_default_also_empty() {
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("bold".into(), ConfigValue::Str("/b.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // `bold` resolves directly
        assert_eq!(resolve_font(&conf, "bold"), "/b.ttf");
        // Missing key with no "default" entry → ""
        assert_eq!(resolve_font(&conf, "italic"), "");
    }

    #[test]
    fn test_format_tick_label_printf_variants() {
        // "%d" → integer format.
        assert_eq!(format_tick_label("%d", 42.7), "42");
        assert_eq!(format_tick_label("%d", -3.2), "-3");
        // "%f" → general float (default Rust fmt).
        let out = format_tick_label("%f", 3.14);
        assert!(out.starts_with("3.14"));
        // "%.1f" and "%.2f" → fixed-precision.
        assert_eq!(format_tick_label("%.1f", 3.14), "3.1");
        assert_eq!(format_tick_label("%.2f", 3.14), "3.14");
        // Unknown format → integer fallback (Perl's sprintf default coerces).
        assert_eq!(format_tick_label("%g", 42.9), "42");
    }

    #[test]
    fn test_seek_tick_prefers_tick_scoped_value() {
        // Tick-scoped value wins over global ticks_conf (Perl seek_parameter semantics).
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("10".into()));
        let mut global: HashMap<String, ConfigValue> = HashMap::new();
        global.insert("size".into(), ConfigValue::Str("20".into()));
        let hit = seek_tick(&tick, &global, "size")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "10");
    }

    #[test]
    fn test_seek_tick_falls_back_to_global() {
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut global: HashMap<String, ConfigValue> = HashMap::new();
        global.insert("color".into(), ConfigValue::Str("red".into()));
        let hit = seek_tick(&tick, &global, "color")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "red");
    }

    #[test]
    fn test_seek_tick_pipe_synonyms_walk_in_order() {
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("label_size".into(), ConfigValue::Str("8".into()));
        let global: HashMap<String, ConfigValue> = HashMap::new();
        // "size|label_size" tries "size" first (miss), then "label_size" (hit).
        let hit = seek_tick(&tick, &global, "size|label_size")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "8");
    }

    #[test]
    fn test_seek_tick_missing_returns_none() {
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let global: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &global, "missing").is_none());
    }

    #[test]
    fn test_seek_tick_f64_strips_p_suffix() {
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("12p".into()));
        let global: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_f64(&tick, &global, "size").unwrap();
        assert!((v - 12.0).abs() < 1e-9);
        // Bare numeric also parses.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("42".into()));
        let v = seek_tick_f64(&tick, &global, "size").unwrap();
        assert!((v - 42.0).abs() < 1e-9);
        // Non-numeric → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("huge".into()));
        assert!(seek_tick_f64(&tick, &global, "size").is_none());
    }

    #[test]
    fn test_format_tick_label_very_large_values() {
        // Million-scale values format correctly via %d.
        assert_eq!(format_tick_label("%d", 1_000_000.0), "1000000");
        assert_eq!(format_tick_label("%d", 1e9), "1000000000");
    }

    #[test]
    fn test_format_tick_label_fractional_variants() {
        // %.1f and %.2f round via Rust's banker's rounding.
        assert_eq!(format_tick_label("%.1f", 1.2345), "1.2");
        // 1.25 → "1.2" (banker's rounds to even; 1.2 is even for the .1f position).
        assert_eq!(format_tick_label("%.1f", 1.25), "1.2");
        assert_eq!(format_tick_label("%.2f", 1.2345), "1.23");
        // 1.35 rounds to nearest even at .1f → "1.4" since 1.4 is even in the tenths.
        assert_eq!(format_tick_label("%.1f", 1.35), "1.4");
    }

    #[test]
    fn test_format_tick_label_unknown_format_uses_integer_fallback() {
        // Any unknown format string → integer fallback (as i64 cast).
        assert_eq!(format_tick_label("%x", 255.0), "255");
        assert_eq!(format_tick_label("custom", 10.7), "10");
        assert_eq!(format_tick_label("", 99.9), "99");
    }

    #[test]
    fn test_format_tick_label_f_variant_general_format() {
        // "%f" uses general float format (Rust default Display).
        let r = format_tick_label("%f", 3.14159265);
        // Rust's default float display includes a decimal portion.
        assert!(r.starts_with("3."));
        // Simple integer value.
        let r = format_tick_label("%f", 42.0);
        assert!(r.starts_with("42"));
    }

    #[test]
    fn test_format_tick_label_negative_and_zero() {
        // "%d" for negatives truncates toward zero (as i64 cast).
        assert_eq!(format_tick_label("%d", -42.7), "-42");
        assert_eq!(format_tick_label("%d", 0.0), "0");
        assert_eq!(format_tick_label("%d", -0.5), "0"); // truncates, not rounds
        // "%.1f" on negative.
        assert_eq!(format_tick_label("%.1f", -3.14), "-3.1");
        // "%.2f" with exact.
        assert_eq!(format_tick_label("%.2f", 0.0), "0.00");
    }

    #[test]
    fn test_seek_tick_f64_decimal_values() {
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("label_size".into(), ConfigValue::Str("12.5p".into()));
        let global: HashMap<String, ConfigValue> = HashMap::new();
        // Decimal with p suffix parses.
        let v = seek_tick_f64(&tick, &global, "label_size").unwrap();
        assert!((v - 12.5).abs() < 1e-9);
        // Decimal without p suffix parses.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("label_size".into(), ConfigValue::Str("0.5".into()));
        let v = seek_tick_f64(&tick, &global, "label_size").unwrap();
        assert!((v - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_seek_tick_only_first_synonym_of_pipe_returns_match() {
        // "a|b" — if `a` exists, `b` is never queried even if present.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("a".into(), ConfigValue::Str("from_a".into()));
        tick.insert("b".into(), ConfigValue::Str("from_b".into()));
        let global: HashMap<String, ConfigValue> = HashMap::new();
        let hit = seek_tick(&tick, &global, "a|b")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(hit, "from_a");
    }

    #[test]
    fn test_resolve_font_empty_fonts_map() {
        // `conf.get("fonts")` returns a Map but it's empty → no "default" entry
        // either → empty string.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let empty_fonts: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(empty_fonts));
        assert_eq!(resolve_font(&conf, "bold"), "");
        assert_eq!(resolve_font(&conf, "default"), "");
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
    fn test_draw_ticks_no_ticks_submap_early_returns() {
        // conf without "ticks" key → early return, no elements added.
        let layout = mk_layout();
        let karyo = Karyotype::default();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        draw_ticks(&mut doc, &layout, &conf, &karyo, &colors);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_ticks_missing_tick_entry_early_returns() {
        // conf has "ticks" but no "tick" sub-entries → early return.
        let layout = mk_layout();
        let karyo = Karyotype::default();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let ticks: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("ticks".into(), ConfigValue::Map(ticks));
        draw_ticks(&mut doc, &layout, &conf, &karyo, &colors);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_ticks_tick_as_scalar_early_returns() {
        // `ticks.tick = "scalar"` — neither Map nor List → wildcard arm returns.
        let layout = mk_layout();
        let karyo = Karyotype::default();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks: HashMap<String, ConfigValue> = HashMap::new();
        ticks.insert("tick".into(), ConfigValue::Str("scalar".into()));
        conf.insert("ticks".into(), ConfigValue::Map(ticks));
        draw_ticks(&mut doc, &layout, &conf, &karyo, &colors);
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_ticks_empty_layout_tick_list_no_panic() {
        // Valid tick list, but layout.ideograms is empty → no per-ideogram
        // iteration happens; function should complete without panicking.
        let layout = mk_layout();
        let karyo = Karyotype::default();
        let colors = ColorMap::new();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks: HashMap<String, ConfigValue> = HashMap::new();
        let mut tick_def: HashMap<String, ConfigValue> = HashMap::new();
        tick_def.insert("spacing".into(), ConfigValue::Str("10u".into()));
        tick_def.insert("size".into(), ConfigValue::Str("5p".into()));
        ticks.insert("tick".into(), ConfigValue::List(vec![ConfigValue::Map(tick_def)]));
        conf.insert("ticks".into(), ConfigValue::Map(ticks));
        // Must not panic with empty ideograms list.
        draw_ticks(&mut doc, &layout, &conf, &karyo, &colors);
    }

    #[test]
    fn test_resolve_font_map_exists_but_key_and_default_both_missing() {
        // `fonts` map exists but neither requested key nor "default" is present.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("foo".into(), ConfigValue::Str("/f.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // Neither "bold" nor "default" in fonts → empty string.
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_resolve_font_non_string_value_returns_empty() {
        // Font key maps to a Map instead of Str → `as_str()` returns None → "".
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("weird".into(), ConfigValue::Map(HashMap::new()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "weird"), "");
    }

    #[test]
    fn test_format_tick_label_integer_value_formats_cleanly() {
        // Integer-valued f64 rounds trivially through every format.
        assert_eq!(format_tick_label("%d", 100.0), "100");
        assert_eq!(format_tick_label("%.1f", 100.0), "100.0");
        assert_eq!(format_tick_label("%.2f", 100.0), "100.00");
        assert_eq!(format_tick_label("%f", 100.0), "100");
    }

    #[test]
    fn test_format_tick_label_truncation_direction_with_negative_values() {
        // Casting negative f64 to i64 truncates toward zero: -3.9 → -3 (not -4).
        assert_eq!(format_tick_label("%d", -3.9), "-3");
        assert_eq!(format_tick_label("%d", -0.5), "0");
        // Unknown format falls through to integer: -5.9 → -5 (truncation).
        assert_eq!(format_tick_label("%x", -5.9), "-5");
    }

    #[test]
    fn test_format_tick_label_precision_1f_rounding() {
        // %.1f uses Rust banker's rounding. 1.45 → "1.5" (round half away from zero),
        // 1.55 → "1.6" per Rust's actual behavior. Document what we see.
        assert_eq!(format_tick_label("%.1f", 0.0), "0.0");
        assert_eq!(format_tick_label("%.1f", 1.0), "1.0");
        assert_eq!(format_tick_label("%.1f", -1.5), "-1.5");
        assert_eq!(format_tick_label("%.1f", 10.95), "10.9");
    }

    #[test]
    fn test_format_tick_label_precision_2f_fixed_decimals() {
        // %.2f always emits exactly 2 decimal places.
        assert_eq!(format_tick_label("%.2f", 0.0), "0.00");
        assert_eq!(format_tick_label("%.2f", 1.0), "1.00");
        assert_eq!(format_tick_label("%.2f", 1.005), "1.00");
        assert_eq!(format_tick_label("%.2f", -3.14159), "-3.14");
    }

    #[test]
    fn test_resolve_font_default_key_fallback_preserves_value() {
        // A key lookup that misses then hits "default" returns the default value.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/fonts/d.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/fonts/b.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // "serif" is missing → falls through to "default".
        assert_eq!(resolve_font(&conf, "serif"), "/fonts/d.ttf");
        // "default" key itself requested — lookup is exact, no recursion.
        assert_eq!(resolve_font(&conf, "default"), "/fonts/d.ttf");
    }

    #[test]
    fn test_resolve_font_conf_missing_fonts_key_empty() {
        // Conf entirely lacks a `fonts` key → resolve_font returns "".
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(resolve_font(&conf, "bold"), "");
        assert_eq!(resolve_font(&conf, "default"), "");
    }

    #[test]
    fn test_format_tick_label_zero_values_across_formats() {
        // 0.0 across all supported formats.
        assert_eq!(format_tick_label("%d", 0.0), "0");
        assert_eq!(format_tick_label("%.1f", 0.0), "0.0");
        assert_eq!(format_tick_label("%.2f", 0.0), "0.00");
        assert_eq!(format_tick_label("%f", 0.0), "0");
    }

    #[test]
    fn test_format_tick_label_large_positive_value_precision_preserved() {
        // Large values at each precision.
        assert_eq!(format_tick_label("%d", 1_000_000.0), "1000000");
        assert_eq!(format_tick_label("%.1f", 1_000_000.5), "1000000.5");
        // %.2f uses Rust banker's rounding: 0.125 → "0.12" (0 is even).
        assert_eq!(format_tick_label("%.2f", 1_000_000.125), "1000000.12");
    }

    #[test]
    fn test_seek_tick_f64_returns_none_for_unparseable_value() {
        // If seek_tick_str returns a non-numeric value, f64 parse fails → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("notanum".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick_f64(&tick, &ticks_conf, "size").is_none());
    }

    #[test]
    fn test_seek_tick_f64_strips_p_then_parses_decimal() {
        // "1.5p" → strip "p" → "1.5" → 1.5.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("spacing".into(), ConfigValue::Str("1.5p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "spacing"), Some(1.5));
        // Bare numeric also parses.
        tick.insert("size".into(), ConfigValue::Str("42".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "size"), Some(42.0));
    }

    #[test]
    fn test_seek_tick_name_not_found_anywhere_returns_none() {
        // Missing from tick + ticks_conf → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "nonexistent").is_none());
        assert!(seek_tick_str(&tick, &ticks_conf, "nonexistent").is_none());
        assert!(seek_tick_f64(&tick, &ticks_conf, "nonexistent").is_none());
    }

    #[test]
    fn test_seek_tick_f64_trims_p_suffix_leaves_integer() {
        // Integer-valued p-suffix: "100p" → 100.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("100p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "size"), Some(100.0));
    }

    #[test]
    fn test_format_tick_label_percent_d_with_fractional_truncates_toward_zero() {
        // %d with positive fractional → truncate toward zero (i64 cast).
        assert_eq!(format_tick_label("%d", 3.9), "3");
        assert_eq!(format_tick_label("%d", 3.1), "3");
        assert_eq!(format_tick_label("%d", -3.9), "-3");
        assert_eq!(format_tick_label("%d", -3.1), "-3");
    }

    #[test]
    fn test_format_tick_label_percent_f_full_precision() {
        // %f uses Rust Display default for f64 — variable precision.
        let out = format_tick_label("%f", 1.0 / 3.0);
        // Output contains "0.3333..." — at least 8 digits.
        assert!(out.starts_with("0.3333"));
        // Integer values display without decimal.
        assert_eq!(format_tick_label("%f", 5.0), "5");
    }

    #[test]
    fn test_seek_tick_f64_non_p_suffix_returns_parse_result() {
        // seek_tick_f64 strips trailing 'p', but bare number also parses.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("x".into(), ConfigValue::Str("5.5".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "x"), Some(5.5));
    }

    #[test]
    fn test_format_tick_label_extreme_float_values() {
        // Very large and very small float values.
        assert_eq!(format_tick_label("%d", 1e15), "1000000000000000");
        assert_eq!(format_tick_label("%.2f", 0.001), "0.00");
        assert_eq!(format_tick_label("%.2f", -1e-5), "-0.00");
    }

    #[test]
    fn test_resolve_font_multi_key_first_match_wins() {
        // Multiple font keys defined — exact-match wins over "default".
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/def.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/bold.ttf".into()));
        fonts.insert("italic".into(), ConfigValue::Str("/italic.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // Direct hits.
        assert_eq!(resolve_font(&conf, "bold"), "/bold.ttf");
        assert_eq!(resolve_font(&conf, "italic"), "/italic.ttf");
        assert_eq!(resolve_font(&conf, "default"), "/def.ttf");
        // Missing → fall through to "default".
        assert_eq!(resolve_font(&conf, "mono"), "/def.ttf");
    }

    #[test]
    fn test_seek_tick_name_missing_from_both_scopes() {
        // A fake name not in tick or ticks_conf → None from seek_tick_str.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick_str(&tick, &ticks_conf, "nonexistent").is_none());
    }

    #[test]
    fn test_format_tick_label_percent_d_with_very_large_value() {
        // `%d` → `value as i64`; 1e15 representable as i64 → "1000000000000000".
        assert_eq!(format_tick_label("%d", 1e15), "1000000000000000");
        // Negative large value.
        assert_eq!(format_tick_label("%d", -1e12), "-1000000000000");
    }

    #[test]
    fn test_format_tick_label_nan_coerces_via_i64_cast() {
        // `NaN as i64` in Rust deterministically yields 0 (saturation → 0).
        assert_eq!(format_tick_label("%d", f64::NAN), "0");
        // +inf as i64 saturates to i64::MAX; -inf to i64::MIN.
        assert_eq!(format_tick_label("%d", f64::INFINITY), i64::MAX.to_string());
        assert_eq!(format_tick_label("%d", f64::NEG_INFINITY), i64::MIN.to_string());
    }

    #[test]
    fn test_seek_tick_f64_value_with_trailing_p_stripped() {
        // Values like "10p" (pixel-suffix) strip the 'p' and parse numerically.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("r".into(), ConfigValue::Str("10p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), Some(10.0));
        // Bare number (no p) also parses.
        tick.insert("r".into(), ConfigValue::Str("25".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), Some(25.0));
    }

    #[test]
    fn test_seek_tick_f64_non_numeric_returns_none() {
        // Non-numeric string → parse fails → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("label".into(), ConfigValue::Str("chr1".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "label"), None);
        // Missing key → None via seek_tick_str.
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "missing"), None);
    }

    #[test]
    fn test_seek_tick_handles_pipe_synonyms_left_to_right() {
        // "a|b" → try a first; if absent, try b. Works for both tick and global.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("from_b".into()));
        let global: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(
            seek_tick_str(&tick, &global, "a|b"),
            Some("from_b")
        );
        // If `a` is present, it wins over `b`.
        tick.insert("a".into(), ConfigValue::Str("from_a".into()));
        assert_eq!(
            seek_tick_str(&tick, &global, "a|b"),
            Some("from_a")
        );
    }

    #[test]
    fn test_format_tick_label_unknown_format_specifier_falls_to_integer() {
        // Unknown format string → final _ arm → i64 cast.
        assert_eq!(format_tick_label("%x", 42.9), "42");
        assert_eq!(format_tick_label("%unknown", -3.7), "-3");
        assert_eq!(format_tick_label("random", 123.456), "123");
    }

    #[test]
    fn test_resolve_font_fonts_value_not_a_map_returns_empty() {
        // fonts is Str (not Map) → as_map() returns None → final unwrap_or("") → "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Str("not_a_map".into()));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_synonyms_prefer_tick_over_global_per_synonym() {
        // For each synonym, tick value checked before global — not all tick-first then all global.
        // Test: "a|b", tick has "b", global has "a" → should return "a" (first synonym wins
        // globally because tick.get("a") is None, then global.get("a") → Some).
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("tick_b".into()));
        let mut global: HashMap<String, ConfigValue> = HashMap::new();
        global.insert("a".into(), ConfigValue::Str("global_a".into()));
        // Synonym order: for n in "a","b": tick.get(a)=None; global.get(a)=Some → returns "global_a".
        assert_eq!(
            seek_tick_str(&tick, &global, "a|b"),
            Some("global_a")
        );
    }

    #[test]
    fn test_format_tick_label_percent_f_default_float_format() {
        // "%f" → Rust default `{}` float format (no trailing zeros).
        assert_eq!(format_tick_label("%f", 3.0), "3");
        let out = format_tick_label("%f", 3.14);
        assert!(out.starts_with("3.14"));
        // Negative preserved.
        let out2 = format_tick_label("%f", -2.5);
        assert!(out2.starts_with("-2.5"));
    }

    #[test]
    fn test_format_tick_label_percent_1f_with_negative_decimal() {
        // "%.1f" with negative: -3.14 → "-3.1".
        assert_eq!(format_tick_label("%.1f", -3.14), "-3.1");
        // %.2f → "-3.14".
        assert_eq!(format_tick_label("%.2f", -3.14), "-3.14");
    }

    #[test]
    fn test_resolve_font_direct_key_hit_skips_default_fallback() {
        // When the requested key is present, the fallback to "default" is not used.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/def.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/bold.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "bold"), "/bold.ttf");
    }

    #[test]
    fn test_seek_tick_f64_trailing_multiple_p_all_trimmed() {
        // trim_end_matches('p') strips ALL trailing 'p' chars → "5ppp" → "5" → 5.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("r".into(), ConfigValue::Str("5ppp".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), Some(5.0));
        // But non-trailing p NOT stripped: "1p0p" → trim trailing → "1p0" → parse fails → None.
        let mut tick2: HashMap<String, ConfigValue> = HashMap::new();
        tick2.insert("r".into(), ConfigValue::Str("1p0p".into()));
        assert_eq!(seek_tick_f64(&tick2, &ticks_conf, "r"), None);
    }

    #[test]
    fn test_format_tick_label_percent_d_zero_value() {
        // 0.0 → i64 cast → "0".
        assert_eq!(format_tick_label("%d", 0.0), "0");
        // Negative zero also → 0.
        assert_eq!(format_tick_label("%d", -0.0), "0");
    }

    #[test]
    fn test_seek_tick_str_local_tick_wins_over_global_ticks_conf_for_same_name() {
        // Even non-synonym query: tick map checked first for name → wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("shared".into(), ConfigValue::Str("local".into()));
        let mut global: HashMap<String, ConfigValue> = HashMap::new();
        global.insert("shared".into(), ConfigValue::Str("GLOBAL".into()));
        assert_eq!(seek_tick_str(&tick, &global, "shared"), Some("local"));
    }

    #[test]
    fn test_resolve_font_missing_fonts_submap_returns_empty_string() {
        // conf has no "fonts" key at all → as_map chain yields None → "".
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_format_tick_label_percent_d_truncates_toward_zero_for_negative_float() {
        // Rust `as i64` truncates toward zero (not floors). -3.9 → -3, not -4.
        assert_eq!(format_tick_label("%d", -3.9), "-3");
        assert_eq!(format_tick_label("%d", -0.5), "0");
        // Positive values also truncate toward zero.
        assert_eq!(format_tick_label("%d", 3.9), "3");
    }

    #[test]
    fn test_format_tick_label_empty_format_string_defaults_to_integer_cast() {
        // Empty format hits the wildcard `_` arm → integer cast.
        assert_eq!(format_tick_label("", 7.3), "7");
        // Unknown garbage format also hits the wildcard arm.
        assert_eq!(format_tick_label("%z", 42.9), "42");
        assert_eq!(format_tick_label("garbage", -5.1), "-5");
    }

    #[test]
    fn test_seek_tick_f64_empty_string_after_p_trim_yields_none() {
        // "p" alone → trimmed to "" → parse() fails → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("val".into(), ConfigValue::Str("p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val"), None);
        // "pp" also trims to empty (trim_end_matches greedy).
        tick.insert("val2".into(), ConfigValue::Str("pp".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val2"), None);
    }

    #[test]
    fn test_seek_tick_f64_whitespace_only_returns_none() {
        // Whitespace-only string — parse fails.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("val".into(), ConfigValue::Str("   ".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val"), None);
        // Tab/newline-only also fails.
        tick.insert("val2".into(), ConfigValue::Str("\t\n".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val2"), None);
    }

    #[test]
    fn test_format_tick_label_percent_2f_on_whole_value_preserves_trailing_zeros() {
        // %.2f format: 3.0 → "3.00"; 10.5 → "10.50".
        assert_eq!(format_tick_label("%.2f", 3.0), "3.00");
        assert_eq!(format_tick_label("%.2f", 10.5), "10.50");
        // Negative.
        assert_eq!(format_tick_label("%.2f", -4.0), "-4.00");
    }

    #[test]
    fn test_seek_tick_synonyms_single_synonym_per_scope_still_walks_across() {
        // Single synonym (no `|`) in name — still walks tick first, then ticks_conf.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("color".into(), ConfigValue::Str("black".into()));
        // tick has no "color" → falls back to ticks_conf.
        let result = seek_tick(&tick, &ticks_conf, "color");
        assert_eq!(result.and_then(|v| v.as_str()), Some("black"));
        // But if tick defines it, tick wins.
        tick.insert("color".into(), ConfigValue::Str("red".into()));
        let result2 = seek_tick(&tick, &ticks_conf, "color");
        assert_eq!(result2.and_then(|v| v.as_str()), Some("red"));
    }

    #[test]
    fn test_seek_tick_f64_whole_number_without_p_parses_ok() {
        // Value without trailing p → parse directly as f64.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("size".into(), ConfigValue::Str("42".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "size"), Some(42.0));
        // Decimals also parse.
        tick.insert("size2".into(), ConfigValue::Str("3.14".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "size2"), Some(3.14));
    }

    #[test]
    fn test_resolve_font_default_fallback_with_empty_string_returns_empty() {
        // fonts.default = "" explicitly → resolve returns "".
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str(String::new()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "italic"), "");
        // Explicit empty still short-circuits properly.
        assert_eq!(resolve_font(&conf, "default"), "");
    }

    #[test]
    fn test_format_tick_label_percent_1f_on_whole_value_emits_one_decimal() {
        // %.1f: 5.0 → "5.0"; 7.999 rounds to "8.0" (half-away-from-zero).
        assert_eq!(format_tick_label("%.1f", 5.0), "5.0");
        assert_eq!(format_tick_label("%.1f", 7.999), "8.0");
        // Negative with trailing.
        assert_eq!(format_tick_label("%.1f", -0.5), "-0.5");
    }

    #[test]
    fn test_seek_tick_pipe_synonyms_walk_and_match_later_synonym() {
        // "color|fill|paint" — none found until third synonym matches.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("paint".into(), ConfigValue::Str("magenta".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let result = seek_tick(&tick, &ticks_conf, "color|fill|paint");
        assert_eq!(result.and_then(|v| v.as_str()), Some("magenta"));
    }

    #[test]
    fn test_seek_tick_f64_negative_value_parses_correctly() {
        // Negative numbers parse as f64; trim_end_matches('p') handles "-5p".
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("offset".into(), ConfigValue::Str("-5p".into()));
        tick.insert("val".into(), ConfigValue::Str("-3.14".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "offset"), Some(-5.0));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val"), Some(-3.14));
    }

    #[test]
    fn test_resolve_font_fonts_key_exists_but_map_empty_returns_empty() {
        // fonts submap is empty → any key lookup → "" (both direct and default fallback miss).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(HashMap::new()));
        assert_eq!(resolve_font(&conf, "bold"), "");
        assert_eq!(resolve_font(&conf, "default"), "");
    }

    #[test]
    fn test_format_tick_label_negative_value_zero_fractional_preserves_sign() {
        // Negative whole number: %.1f and %.2f both preserve the sign.
        assert_eq!(format_tick_label("%.1f", -3.0), "-3.0");
        assert_eq!(format_tick_label("%.2f", -3.0), "-3.00");
        // %d with negative whole → integer cast preserves sign.
        assert_eq!(format_tick_label("%d", -3.0), "-3");
    }

    #[test]
    fn test_seek_tick_both_scopes_different_values_tick_wins() {
        // tick has "color"="red", ticks_conf has "color"="blue". tick wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("color".into(), ConfigValue::Str("red".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("color".into(), ConfigValue::Str("blue".into()));
        let r = seek_tick(&tick, &ticks_conf, "color");
        assert_eq!(r.and_then(|v| v.as_str()), Some("red"));
    }

    #[test]
    fn test_seek_tick_str_returns_none_when_neither_scope_has_key() {
        // Both scopes empty → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "anything"), None);
    }

    #[test]
    fn test_resolve_font_direct_key_with_empty_path_value_returns_empty() {
        // fonts.bold = "" → resolve_font returns empty string.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("bold".into(), ConfigValue::Str(String::new()));
        fonts.insert("default".into(), ConfigValue::Str("/fallback".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // Direct hit "bold" → returns "" (the explicit value).
        assert_eq!(resolve_font(&conf, "bold"), "");
        // Missing key falls back to default.
        assert_eq!(resolve_font(&conf, "italic"), "/fallback");
    }

    #[test]
    fn test_format_tick_label_all_percent_formats_on_zero() {
        // 0.0 across all format variants yields consistent "0" or "0.0..." output.
        assert_eq!(format_tick_label("%d", 0.0), "0");
        assert_eq!(format_tick_label("%f", 0.0), "0");
        assert_eq!(format_tick_label("%.1f", 0.0), "0.0");
        assert_eq!(format_tick_label("%.2f", 0.0), "0.00");
    }

    #[test]
    fn test_seek_tick_synonym_walk_order_is_left_to_right() {
        // "a|b|c" synonyms — leftmost wins. If "c" is in tick but "a" is in ticks_conf,
        // the order of the split means "a" is tried first across both scopes.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("c".into(), ConfigValue::Str("from_c".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("a".into(), ConfigValue::Str("from_a".into()));
        let result = seek_tick(&tick, &ticks_conf, "a|b|c");
        // "a" found in ticks_conf first (within the "a" synonym walk).
        assert_eq!(result.and_then(|v| v.as_str()), Some("from_a"));
    }

    #[test]
    fn test_seek_tick_f64_value_with_extra_whitespace_fails_parse_inside() {
        // trim_end_matches('p') only strips trailing 'p' — interior whitespace fails parse.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("val".into(), ConfigValue::Str("5 0".into())); // space in middle
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        // "5 0" can't parse as f64 → None.
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val"), None);
    }

    #[test]
    fn test_resolve_font_non_map_fonts_value_returns_empty() {
        // fonts key present but not a Map (e.g., Str) → resolve returns "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Str("not_a_map".into()));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_format_tick_label_large_positive_value_formats_without_overflow() {
        // Very large f64 passed to format functions — no panic.
        assert_eq!(format_tick_label("%d", 1e12), "1000000000000");
        // %.2f on large value.
        let s = format_tick_label("%.2f", 1e6);
        assert!(s.starts_with("1000000"));
    }

    #[test]
    fn test_seek_tick_f64_value_exactly_zero_with_p_suffix() {
        // "0p" → trim 'p' → "0" → parse → 0.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("val".into(), ConfigValue::Str("0p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "val"), Some(0.0));
    }

    #[test]
    fn test_seek_tick_ticks_conf_missing_key_returns_none() {
        // Both scopes have no matching key → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "anything").is_none());
    }

    #[test]
    fn test_resolve_font_default_key_found_when_requested_key_missing() {
        // Both "bold" and "default" registered; asking for missing "italic" → default's value.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/default.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/bold.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "italic"), "/default.ttf");
        assert_eq!(resolve_font(&conf, "bold"), "/bold.ttf");
    }

    #[test]
    fn test_format_tick_label_various_formats_on_positive_integer_value() {
        // value=5 across all format variants.
        assert_eq!(format_tick_label("%d", 5.0), "5");
        assert_eq!(format_tick_label("%f", 5.0), "5");
        assert_eq!(format_tick_label("%.1f", 5.0), "5.0");
        assert_eq!(format_tick_label("%.2f", 5.0), "5.00");
    }

    #[test]
    fn test_seek_tick_pipe_synonym_first_match_wins_across_scopes() {
        // "a|b|c" with "a" in ticks_conf and "b" in tick — "a" wins first.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("tick_b".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("a".into(), ConfigValue::Str("conf_a".into()));
        let r = seek_tick(&tick, &ticks_conf, "a|b|c");
        assert_eq!(r.and_then(|v| v.as_str()), Some("conf_a"));
    }

    #[test]
    fn test_seek_tick_f64_with_integer_and_p_suffix() {
        // Integer value with p suffix parses correctly.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("radius".into(), ConfigValue::Str("1500p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "radius"), Some(1500.0));
    }

    #[test]
    fn test_resolve_font_nonstandard_font_key_with_only_default_fallback() {
        // Only "default" font registered; any other key returns default's value.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/D.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "anyname"), "/D.ttf");
        assert_eq!(resolve_font(&conf, "another"), "/D.ttf");
    }

    #[test]
    fn test_format_tick_label_fractional_value_across_formats() {
        // Fractional value formatted across %d/%f/%.1f/%.2f.
        assert_eq!(format_tick_label("%d", 7.8), "7");
        assert_eq!(format_tick_label("%.1f", 7.8), "7.8");
        assert_eq!(format_tick_label("%.2f", 7.8), "7.80");
    }

    #[test]
    fn test_seek_tick_f64_with_p_suffix_trimmed_from_end() {
        // trim_end_matches('p') strips trailing p → "2500p" → 2500.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("radius".into(), ConfigValue::Str("2500p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "radius"), Some(2500.0));
    }

    #[test]
    fn test_seek_tick_str_delegates_to_seek_tick_and_returns_str_slice() {
        // seek_tick_str returns the underlying &str from a Str-value hit.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("label".into(), ConfigValue::Str("foo".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "label"), Some("foo"));
        // Missing key → None through the chain.
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "missing"), None);
    }

    #[test]
    fn test_format_tick_label_negative_fractional_value_across_formats() {
        // Negative fractional values: %d truncates toward zero via "as i64" cast.
        assert_eq!(format_tick_label("%d", -1.8), "-1");
        // %.1f rounds half-up at .5 boundary — standard Rust behavior.
        assert_eq!(format_tick_label("%.1f", -1.25), "-1.2");
        assert_eq!(format_tick_label("%.2f", -1.005), "-1.00");
    }

    #[test]
    fn test_resolve_font_falls_back_to_default_when_font_key_missing() {
        // Both "default" and "bold" present — looking up missing "italic" → default path.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/D.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/B.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // "italic" missing → falls to "default".
        assert_eq!(resolve_font(&conf, "italic"), "/D.ttf");
        // "bold" direct hit.
        assert_eq!(resolve_font(&conf, "bold"), "/B.ttf");
    }

    #[test]
    fn test_seek_tick_pipe_synonym_second_hits_when_first_missing() {
        // "a|b|c" → walks each synonym left-to-right. First match wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("second_hit".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "a|b|c");
        assert_eq!(v.and_then(|x| x.as_str()), Some("second_hit"));
    }

    #[test]
    fn test_seek_tick_f64_bare_number_parses_via_trim_end_matches() {
        // No suffix — trim_end_matches('p') is no-op, then parse.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("w".into(), ConfigValue::Str("42.5".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "w"), Some(42.5));
    }

    #[test]
    fn test_format_tick_label_exact_integer_value_varies_by_format() {
        // 5.0 — all formats handle cleanly.
        assert_eq!(format_tick_label("%d", 5.0), "5");
        assert_eq!(format_tick_label("%.1f", 5.0), "5.0");
        assert_eq!(format_tick_label("%.2f", 5.0), "5.00");
    }

    #[test]
    fn test_seek_tick_returns_none_when_synonym_list_has_no_matches() {
        // "x|y|z" all missing → None through full synonym walk.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "x|y|z").is_none());
    }

    #[test]
    fn test_format_tick_label_unknown_format_defaults_to_integer_cast() {
        // "%g" is unknown → falls to integer cast via as i64.
        assert_eq!(format_tick_label("%g", 42.9), "42");
        // "%5.2f" is unknown → integer cast.
        assert_eq!(format_tick_label("%5.2f", 99.9), "99");
    }

    #[test]
    fn test_format_tick_label_zero_value_renders_as_zero_string() {
        // 0.0 through each format.
        assert_eq!(format_tick_label("%d", 0.0), "0");
        assert_eq!(format_tick_label("%.1f", 0.0), "0.0");
        assert_eq!(format_tick_label("%.2f", 0.0), "0.00");
    }

    #[test]
    fn test_resolve_font_no_fonts_submap_returns_empty_string() {
        // No "fonts" key in conf → fall-through chain → "".
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_same_name_in_both_scopes_returns_tick_first() {
        // tick scope wins over ticks_conf scope — first check order.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("radius".into(), ConfigValue::Str("local".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("radius".into(), ConfigValue::Str("global".into()));
        let v = seek_tick(&tick, &ticks_conf, "radius");
        assert_eq!(v.and_then(|x| x.as_str()), Some("local"));
    }

    #[test]
    fn test_seek_tick_f64_non_numeric_after_trim_returns_none() {
        // Value "abc" has no trailing 'p' → unchanged; parse fails → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("width".into(), ConfigValue::Str("abc".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "width"), None);
    }

    #[test]
    fn test_seek_tick_str_on_non_string_value_returns_none() {
        // Key present but value is Map (not Str) → seek_tick_str returns None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("complex".into(), ConfigValue::Map(HashMap::new()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "complex"), None);
    }

    #[test]
    fn test_resolve_font_bold_key_explicit_non_default_path() {
        // Bold key present; explicit lookup returns the bold font.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/D.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/B.ttf".into()));
        fonts.insert("italic".into(), ConfigValue::Str("/I.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "bold"), "/B.ttf");
        assert_eq!(resolve_font(&conf, "italic"), "/I.ttf");
    }

    #[test]
    fn test_format_tick_label_very_large_value_preserves_precision_in_d() {
        // Integer cast preserves magnitude for large f64.
        assert_eq!(format_tick_label("%d", 1e9), "1000000000");
        assert_eq!(format_tick_label("%d", -1e6), "-1000000");
    }

    #[test]
    fn test_seek_tick_f64_u_suffix_not_trimmed_parse_fails() {
        // Only 'p' is trimmed; "100u" keeps 'u' → parse fail → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("r".into(), ConfigValue::Str("100u".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), None);
    }

    #[test]
    fn test_seek_tick_non_existent_name_none() {
        // No key in either scope → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "radius").is_none());
    }

    #[test]
    fn test_format_tick_label_f_format_uses_default_display() {
        // "%f" format uses Rust's Display → typically "X.Y" variable precision.
        let s = format_tick_label("%f", 1.5);
        assert!(s.starts_with("1.5"));
    }

    #[test]
    fn test_resolve_font_lookup_from_ticks_conf_scope_via_seek_tick() {
        // seek_tick_str fallback to ticks_conf works for any key.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("font".into(), ConfigValue::Str("bold".into()));
        let v = seek_tick_str(&tick, &ticks_conf, "font");
        assert_eq!(v, Some("bold"));
    }

    #[test]
    fn test_seek_tick_pipe_synonym_single_synonym_no_separator_still_walks() {
        // "simple_name" with no | treated as single synonym.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("simple_name".into(), ConfigValue::Str("found".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "simple_name");
        assert_eq!(v.and_then(|x| x.as_str()), Some("found"));
    }

    #[test]
    fn test_seek_tick_f64_integer_with_p_suffix_yields_f64() {
        // "100p" → 100.0 (p stripped, integer-style parse → f64).
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("w".into(), ConfigValue::Str("100p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "w"), Some(100.0));
    }

    #[test]
    fn test_format_tick_label_dot_one_format_preserves_fractional_digit() {
        // %.1f keeps exactly 1 decimal digit.
        assert_eq!(format_tick_label("%.1f", 99.87), "99.9");
        assert_eq!(format_tick_label("%.1f", 0.05), "0.1");
    }

    #[test]
    fn test_resolve_font_empty_fonts_submap_returns_empty_string() {
        // fonts exists but has no keys at all → ""
        let fonts: HashMap<String, ConfigValue> = HashMap::new();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_resolve_font_fonts_is_str_not_map_returns_empty_string() {
        // fonts value is Str (not Map) → as_map None → empty.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Str("not_a_map".into()));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_f64_scientific_notation_no_p_parses() {
        // Scientific notation without suffix → parse OK.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("x".into(), ConfigValue::Str("1.5e3".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "x"), Some(1500.0));
    }

    #[test]
    fn test_format_tick_label_very_small_fractional_value() {
        // 0.001 through various formats.
        assert_eq!(format_tick_label("%d", 0.001), "0");
        assert_eq!(format_tick_label("%.1f", 0.001), "0.0");
        assert_eq!(format_tick_label("%.2f", 0.001), "0.00");
    }

    #[test]
    fn test_seek_tick_synonym_separator_empty_parts_handled() {
        // "a||b" (empty synonym in middle) → walks all parts including "".
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("found_b".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "a||b");
        assert_eq!(v.and_then(|x| x.as_str()), Some("found_b"));
    }

    #[test]
    fn test_seek_tick_f64_negative_value_with_p_suffix() {
        // "-500p" → -500.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("r".into(), ConfigValue::Str("-500p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), Some(-500.0));
    }

    #[test]
    fn test_format_tick_label_2f_handles_negative_small_values() {
        // %.2f on -0.005 → "-0.01" or "-0.00" depending on rounding.
        let s = format_tick_label("%.2f", -0.005);
        assert!(s == "-0.00" || s == "-0.01");
    }

    #[test]
    fn test_resolve_font_with_fonts_has_only_default_any_key_returns_default_path() {
        // Only "default" in fonts → every lookup falls back to it.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/default.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        for key in ["bold", "italic", "semibold", "extraboldextended"] {
            assert_eq!(resolve_font(&conf, key), "/default.ttf");
        }
    }

    #[test]
    fn test_seek_tick_tick_scope_has_str_value_takes_precedence() {
        // tick and ticks_conf both have same name — tick scope wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("key".into(), ConfigValue::Str("tick_value".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("key".into(), ConfigValue::Str("conf_value".into()));
        let v = seek_tick(&tick, &ticks_conf, "key");
        assert_eq!(v.and_then(|x| x.as_str()), Some("tick_value"));
    }

    #[test]
    fn test_seek_tick_non_str_value_still_returned_but_as_str_none() {
        // seek_tick returns value ref but as_str on a Map will return None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Map(HashMap::new()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "k");
        assert!(v.is_some());
        assert!(v.unwrap().as_str().is_none());
    }

    #[test]
    fn test_format_tick_label_large_positive_preserves_digits() {
        // 999999 → all 6 digits preserved.
        assert_eq!(format_tick_label("%d", 999999.0), "999999");
    }

    #[test]
    fn test_resolve_font_bold_and_default_both_present_bold_wins() {
        // Direct lookup "bold" wins over default fallback.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/d.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/b.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "bold"), "/b.ttf");
    }

    #[test]
    fn test_seek_tick_f64_value_with_mixed_case_p_not_stripped() {
        // Upper-case 'P' not in trim_end_matches → not stripped → parse fail.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("r".into(), ConfigValue::Str("500P".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), None);
    }

    #[test]
    fn test_format_tick_label_printf_d_with_zero_precision_loss() {
        // Large float → integer truncation.
        assert_eq!(format_tick_label("%d", 1_234_567.89), "1234567");
    }

    #[test]
    fn test_seek_tick_str_tick_scope_wins_over_ticks_conf() {
        // tick scope has "label" → returned even if ticks_conf also does.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("label".into(), ConfigValue::Str("tick".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("label".into(), ConfigValue::Str("conf".into()));
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "label"), Some("tick"));
    }

    #[test]
    fn test_resolve_font_fonts_submap_with_only_unknown_keys_returns_empty() {
        // Only "weird" key present; asking for "bold" → fallback to "default" (missing) → "".
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("weird".into(), ConfigValue::Str("/w.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_empty_synonym_string_returns_none() {
        // Empty string "" — single empty synonym → no match.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "").is_none());
    }

    #[test]
    fn test_format_tick_label_negative_integer_value() {
        // -42.0 via %d → "-42".
        assert_eq!(format_tick_label("%d", -42.0), "-42");
    }

    #[test]
    fn test_seek_tick_str_f64_chain_via_p_suffix() {
        // seek_tick_str followed by parse — verify chain works.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("w".into(), ConfigValue::Str("42".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "w"), Some("42"));
    }

    #[test]
    fn test_resolve_font_custom_key_unique_fonts_map() {
        // Custom font key "title" present.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("title".into(), ConfigValue::Str("/t.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "title"), "/t.ttf");
    }

    #[test]
    fn test_seek_tick_ticks_conf_fallback_when_tick_missing_key() {
        // Tick has no "w"; ticks_conf has "w"=50 → seek_tick returns ticks_conf value.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("w".into(), ConfigValue::Str("50".into()));
        let v = seek_tick(&tick, &ticks_conf, "w");
        assert_eq!(v.and_then(|x| x.as_str()), Some("50"));
    }

    #[test]
    fn test_format_tick_label_zero_negative_gives_zero_string() {
        // Negative zero behaves as zero for %d.
        assert_eq!(format_tick_label("%d", -0.0), "0");
    }

    #[test]
    fn test_seek_tick_fallback_through_two_synonyms_third_matches() {
        // "a|b|c" — third synonym "c" matches.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("c".into(), ConfigValue::Str("third_hit".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "a|b|c");
        assert_eq!(v.and_then(|x| x.as_str()), Some("third_hit"));
    }

    #[test]
    fn test_resolve_font_case_sensitive_key_lookup() {
        // "Bold" (capital B) NOT equal to "bold" in HashMap.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("bold".into(), ConfigValue::Str("/b.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        // "Bold" misses; falls back to "default" missing → "".
        assert_eq!(resolve_font(&conf, "Bold"), "");
    }

    #[test]
    fn test_seek_tick_str_missing_key_returns_none() {
        // Both scopes empty → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "absent"), None);
    }

    #[test]
    fn test_format_tick_label_large_decimal_value_via_2f() {
        // Large decimal with %.2f → 2 decimal places.
        assert_eq!(format_tick_label("%.2f", 1234.567), "1234.57");
    }

    #[test]
    fn test_seek_tick_f64_ticks_conf_scope_walks_after_tick_miss() {
        // tick scope empty; ticks_conf has value → found via fallback.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("r".into(), ConfigValue::Str("1000p".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "r"), Some(1000.0));
    }

    #[test]
    fn test_format_tick_label_integer_d_with_trailing_digits_past_max_truncates() {
        // Large value well within i64 range.
        assert_eq!(format_tick_label("%d", 99999999.0), "99999999");
    }

    #[test]
    fn test_seek_tick_synonym_pipe_separator_walks_in_order() {
        // First synonym match wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("first".into(), ConfigValue::Str("first_match".into()));
        tick.insert("second".into(), ConfigValue::Str("second_match".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "first|second");
        assert_eq!(v.and_then(|x| x.as_str()), Some("first_match"));
    }

    #[test]
    fn test_format_tick_label_zero_value_d_format_gives_zero_string() {
        // "%d" format with 0 → "0".
        let s = format_tick_label("%d", 0.0);
        assert_eq!(s, "0");
    }

    #[test]
    fn test_seek_tick_second_synonym_matches_when_first_missing() {
        // "a|b" — a not in tick, b is → match on b.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("bvalue".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "a|b");
        assert_eq!(v.and_then(|x| x.as_str()), Some("bvalue"));
    }

    #[test]
    fn test_seek_tick_f64_returns_none_when_not_parseable() {
        // "not_a_number" → None via f64 parse.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("not_a_number".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_f64(&tick, &ticks_conf, "k");
        assert!(v.is_none());
    }

    #[test]
    fn test_format_tick_label_f_format_passes_through_default_repr() {
        // "%f" uses default Display; 2.5 → "2.5".
        let s = format_tick_label("%f", 2.5);
        assert_eq!(s, "2.5");
    }

    #[test]
    fn test_format_tick_label_1f_format_one_decimal_place() {
        // "%.1f" → one decimal.
        let s = format_tick_label("%.1f", 3.14159);
        assert_eq!(s, "3.1");
    }

    #[test]
    fn test_format_tick_label_unknown_format_falls_back_to_integer() {
        // Unknown format "%x" → default branch (i64 cast).
        let s = format_tick_label("%x", 42.7);
        assert_eq!(s, "42");
    }

    #[test]
    fn test_resolve_font_absent_fonts_returns_empty_string_for_any_key() {
        // Conf without "fonts" key → empty string regardless of font_key.
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(resolve_font(&conf, "default"), "");
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_empty_name_string_returns_none() {
        // Empty synonym string → splits to [""] which doesn't match.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "");
        assert!(v.is_none());
    }

    #[test]
    fn test_resolve_font_fonts_present_but_key_missing_uses_default() {
        // fonts has "default" but not "bold" → fall back to default.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/fonts/d.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "missing"), "/fonts/d.ttf");
    }

    #[test]
    fn test_resolve_font_fonts_no_default_fallback_yields_empty() {
        // fonts exists but lacks both requested key and "default" → "".
        let fonts: HashMap<String, ConfigValue> = HashMap::new();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "any"), "");
    }

    #[test]
    fn test_seek_tick_str_wraps_seek_tick_and_returns_str() {
        // seek_tick_str → returns &str on match.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("hello".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_str(&tick, &ticks_conf, "k");
        assert_eq!(v, Some("hello"));
    }

    #[test]
    fn test_seek_tick_f64_with_p_suffix_parses_value() {
        // "100p" → trimmed "100" parses as 100.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("r".into(), ConfigValue::Str("100p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_f64(&tick, &ticks_conf, "r");
        assert_eq!(v, Some(100.0));
    }

    #[test]
    fn test_format_tick_label_d_truncates_toward_zero_positive() {
        // "%d" on 99.9 → truncate via i64 cast → 99.
        let s = format_tick_label("%d", 99.9);
        assert_eq!(s, "99");
    }

    #[test]
    fn test_format_tick_label_d_truncates_toward_zero_negative() {
        // "%d" on -5.9 → truncate toward 0 via i64 cast → -5.
        let s = format_tick_label("%d", -5.9);
        assert_eq!(s, "-5");
    }

    #[test]
    fn test_resolve_font_fonts_not_map_variant_yields_empty() {
        // fonts key exists but as Str (not Map) → "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Str("not_a_map".into()));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_lookup_falls_back_to_ticks_conf_when_tick_missing() {
        // Key not in tick but in ticks_conf → returns from ticks_conf.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("fallback".into(), ConfigValue::Str("via_conf".into()));
        let v = seek_tick(&tick, &ticks_conf, "fallback");
        assert_eq!(v.and_then(|x| x.as_str()), Some("via_conf"));
    }

    #[test]
    fn test_seek_tick_tick_overrides_ticks_conf_when_both_present() {
        // Both tick and ticks_conf have the same key → tick wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("tick_val".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("k".into(), ConfigValue::Str("conf_val".into()));
        let v = seek_tick(&tick, &ticks_conf, "k");
        assert_eq!(v.and_then(|x| x.as_str()), Some("tick_val"));
    }

    #[test]
    fn test_format_tick_label_2f_format_two_decimal_places() {
        // "%.2f" on 3.14159 → "3.14".
        let s = format_tick_label("%.2f", 3.14159);
        assert_eq!(s, "3.14");
    }

    #[test]
    fn test_resolve_font_with_custom_font_key_returns_custom_path() {
        // fonts has "italic" key → resolved to its path.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("italic".into(), ConfigValue::Str("/fonts/italic.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "italic"), "/fonts/italic.ttf");
    }

    #[test]
    fn test_seek_tick_f64_large_numeric_value_parses() {
        // "1000000p" → 1e6.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("1000000p".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_f64(&tick, &ticks_conf, "k");
        assert_eq!(v, Some(1000000.0));
    }

    #[test]
    fn test_format_tick_label_negative_float_with_f_format() {
        // "%f" on -3.14 → "-3.14".
        let s = format_tick_label("%f", -3.14);
        assert_eq!(s, "-3.14");
    }

    #[test]
    fn test_seek_tick_str_returns_none_for_missing_synonym_chain() {
        // "x|y|z" — none in either map → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick_str(&tick, &ticks_conf, "x|y|z").is_none());
    }

    #[test]
    fn test_resolve_font_both_default_and_custom_returns_custom_when_requested() {
        // fonts has both "default" and "bold" — bold explicitly requested.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/d.ttf".into()));
        fonts.insert("bold".into(), ConfigValue::Str("/b.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "bold"), "/b.ttf");
    }

    #[test]
    fn test_seek_tick_f64_bare_integer_no_suffix_parses() {
        // "42" bare (no p suffix) → 42.0.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("42".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "k"), Some(42.0));
    }

    #[test]
    fn test_format_tick_label_f_on_exact_integer_preserves_decimal_point() {
        // "%f" on 5.0 → Display format produces "5" (Rust Display for f64 drops trailing .0).
        let s = format_tick_label("%f", 5.0);
        assert_eq!(s, "5");
    }

    #[test]
    fn test_format_tick_label_1f_on_round_number_pads_decimal() {
        // "%.1f" on 3.0 → "3.0" (one decimal padded).
        let s = format_tick_label("%.1f", 3.0);
        assert_eq!(s, "3.0");
    }

    #[test]
    fn test_seek_tick_three_synonyms_second_matches_when_first_missing() {
        // "a|b|c" — a missing, b in tick → returns b's value.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("b".into(), ConfigValue::Str("bvalue".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick(&tick, &ticks_conf, "a|b|c");
        assert_eq!(v.and_then(|x| x.as_str()), Some("bvalue"));
    }

    #[test]
    fn test_resolve_font_returns_owned_string() {
        // resolve_font returns owned String (not &str) — can outlive conf borrow.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/fonts/d.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        let s: String = resolve_font(&conf, "default");
        assert_eq!(s.as_str(), "/fonts/d.ttf");
    }

    #[test]
    fn test_format_tick_label_scientific_input_without_scientific_format() {
        // Large value with "%d" truncates to i64.
        let s = format_tick_label("%d", 1e6);
        assert_eq!(s, "1000000");
    }

    #[test]
    fn test_seek_tick_f64_with_nonexistent_key_returns_none() {
        // Missing key → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick_f64(&tick, &ticks_conf, "missing").is_none());
    }

    #[test]
    fn test_seek_tick_str_returns_str_slice_not_string() {
        // seek_tick_str returns &str (borrowed from stored Str).
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("val".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let s = seek_tick_str(&tick, &ticks_conf, "k").unwrap();
        assert_eq!(s, "val");
    }

    #[test]
    fn test_format_tick_label_large_d_value_preserves_all_digits() {
        // Large integer → all digits preserved.
        let s = format_tick_label("%d", 987654321.0);
        assert_eq!(s, "987654321");
    }

    #[test]
    fn test_format_tick_label_f_fractional_value() {
        // "%f" on 0.25 → "0.25".
        let s = format_tick_label("%f", 0.25);
        assert_eq!(s, "0.25");
    }

    #[test]
    fn test_seek_tick_f64_with_both_tick_and_conf_tick_wins() {
        // Both tick and ticks_conf have "k" → tick wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("10p".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("k".into(), ConfigValue::Str("20p".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "k"), Some(10.0));
    }

    #[test]
    fn test_resolve_font_map_variant_at_fonts_key_with_empty_map() {
        // fonts map exists but is empty → returns "".
        let fonts: HashMap<String, ConfigValue> = HashMap::new();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "default"), "");
    }

    #[test]
    fn test_seek_tick_str_synonym_chain_falls_through_to_last() {
        // "a|b|last" with only "last" in tick → matches "last".
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("last".into(), ConfigValue::Str("lastval".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_str(&tick, &ticks_conf, "a|b|last");
        assert_eq!(v, Some("lastval"));
    }

    #[test]
    fn test_format_tick_label_d_zero_integer_yields_zero_string() {
        // "%d" on 0.0 → "0".
        let s = format_tick_label("%d", 0.0);
        assert_eq!(s, "0");
    }

    #[test]
    fn test_seek_tick_f64_with_whitespace_only_str_returns_none() {
        // "   " whitespace-only string → parse fails → None.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("   ".into()));
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        let v = seek_tick_f64(&tick, &ticks_conf, "k");
        assert!(v.is_none());
    }

    #[test]
    fn test_resolve_font_prefers_exact_match_over_default() {
        // Both "my_font" and "default" present → exact match wins.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/d.ttf".into()));
        fonts.insert("my_font".into(), ConfigValue::Str("/mf.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "my_font"), "/mf.ttf");
    }

    #[test]
    fn test_seek_tick_str_lookup_in_ticks_conf_only_found() {
        // Only ticks_conf has key — tick is empty → falls back to ticks_conf.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("only_conf".into(), ConfigValue::Str("conf_val".into()));
        assert_eq!(seek_tick_str(&tick, &ticks_conf, "only_conf"), Some("conf_val"));
    }

    #[test]
    fn test_format_tick_label_d_format_very_negative_value() {
        // "%d" on -987654321 → "-987654321".
        let s = format_tick_label("%d", -987654321.0);
        assert_eq!(s, "-987654321");
    }

    #[test]
    fn test_seek_tick_f64_empty_synonym_string_returns_none() {
        // Empty synonym "" in tick → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick_f64(&tick, &ticks_conf, "").is_none());
    }

    #[test]
    fn test_resolve_font_with_fonts_map_containing_multiple_entries() {
        // Multiple font entries → each independently accessible.
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("f1".into(), ConfigValue::Str("/f1.ttf".into()));
        fonts.insert("f2".into(), ConfigValue::Str("/f2.ttf".into()));
        fonts.insert("f3".into(), ConfigValue::Str("/f3.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "f1"), "/f1.ttf");
        assert_eq!(resolve_font(&conf, "f2"), "/f2.ttf");
        assert_eq!(resolve_font(&conf, "f3"), "/f3.ttf");
    }

    #[test]
    fn test_format_tick_label_d_rounds_fractional_toward_zero() {
        // "%d" on 2.9 → 2 (truncation).
        assert_eq!(format_tick_label("%d", 2.9), "2");
    }

    #[test]
    fn test_format_tick_label_f_no_decimals_forced() {
        // "%f" prints Rust default — no forced decimals for integer value.
        let s = format_tick_label("%f", 5.0);
        // Should contain "5" (may be "5" or "5.0" depending on f64 display).
        assert!(s.contains('5'));
    }

    #[test]
    fn test_format_tick_label_unknown_format_defaults_to_d() {
        // Unknown format → falls through to default branch (same as %d).
        assert_eq!(format_tick_label("%x", 7.9), "7");
    }

    #[test]
    fn test_format_tick_label_dot2f_zero_value() {
        // "%.2f" on 0.0 → "0.00".
        assert_eq!(format_tick_label("%.2f", 0.0), "0.00");
    }

    #[test]
    fn test_format_tick_label_d_negative_truncates_toward_zero() {
        // Rust `as i64` truncates toward zero: -2.9 → -2.
        assert_eq!(format_tick_label("%d", -2.9), "-2");
    }

    #[test]
    fn test_seek_tick_f64_trailing_p_suffix_stripped() {
        // "42p" → 42.0 after p suffix stripped.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("width".into(), ConfigValue::Str("42p".into()));
        let v = seek_tick_f64(&tick, &ticks_conf, "width").unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_seek_tick_synonym_second_alt_found_in_ticks_conf() {
        // "a|b" - 'a' absent everywhere, 'b' found in ticks_conf → returns b's value.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("b".into(), ConfigValue::Str("found".into()));
        let v = seek_tick_str(&tick, &ticks_conf, "a|b").unwrap();
        assert_eq!(v, "found");
    }

    #[test]
    fn test_seek_tick_tick_shadows_ticks_conf_same_key() {
        // Key in both: tick hashmap wins (checked first per synonym).
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("width".into(), ConfigValue::Str("10".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("width".into(), ConfigValue::Str("99".into()));
        let v = seek_tick_str(&tick, &ticks_conf, "width").unwrap();
        assert_eq!(v, "10");
    }

    #[test]
    fn test_seek_tick_f64_unparseable_value_returns_none() {
        // Non-numeric string → parse Err → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("width".into(), ConfigValue::Str("abc".into()));
        assert!(seek_tick_f64(&tick, &ticks_conf, "width").is_none());
    }

    #[test]
    fn test_resolve_font_with_empty_fonts_map_returns_empty_string() {
        // Fonts map empty → no lookups succeed → "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(HashMap::new()));
        let r = resolve_font(&conf, "any");
        assert_eq!(r, "");
    }

    #[test]
    fn test_resolve_font_fonts_value_wrong_type_returns_empty() {
        // "fonts" is a Str, not a Map → as_map() None → "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Str("not_a_map".into()));
        let r = resolve_font(&conf, "bold");
        assert_eq!(r, "");
    }

    #[test]
    fn test_format_tick_label_f_on_negative_value() {
        // "%f" on -3.5 → "-3.5".
        let s = format_tick_label("%f", -3.5);
        assert!(s.contains("-3.5"));
    }

    #[test]
    fn test_format_tick_label_dot1f_trailing_zero_preserved() {
        // "%.1f" on 10.0 → "10.0" (trailing zero preserved).
        assert_eq!(format_tick_label("%.1f", 10.0), "10.0");
    }

    #[test]
    fn test_format_tick_label_dot2f_rounds_to_two_decimals() {
        // "%.2f" on 3.14159 → "3.14".
        assert_eq!(format_tick_label("%.2f", 3.14159), "3.14");
    }

    #[test]
    fn test_format_tick_label_dot2f_large_value_formatted() {
        // "%.2f" on 1234.5 → "1234.50".
        assert_eq!(format_tick_label("%.2f", 1234.5), "1234.50");
    }

    #[test]
    fn test_seek_tick_empty_name_no_matches_returns_none_v2() {
        // Empty name split produces one empty synonym; neither map has "" → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "").is_none());
    }

    #[test]
    fn test_seek_tick_str_value_not_str_returns_none() {
        // Found a value but it's a Map (not Str) → as_str None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("x".into(), ConfigValue::Map(HashMap::new()));
        assert!(seek_tick_str(&tick, &ticks_conf, "x").is_none());
    }

    #[test]
    fn test_seek_tick_f64_negative_value_parses() {
        // Negative f64 value parses correctly.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("offset".into(), ConfigValue::Str("-50.5".into()));
        let v = seek_tick_f64(&tick, &ticks_conf, "offset").unwrap();
        assert_eq!(v, -50.5);
    }

    #[test]
    fn test_seek_tick_first_synonym_hit_in_tick_takes_precedence() {
        // "a|b" - a in tick → returns tick.a.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("a".into(), ConfigValue::Str("in_tick".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("b".into(), ConfigValue::Str("in_conf".into()));
        let v = seek_tick_str(&tick, &ticks_conf, "a|b").unwrap();
        assert_eq!(v, "in_tick");
    }

    #[test]
    fn test_resolve_font_returns_empty_when_fonts_submap_absent() {
        // No "fonts" key at all → "".
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(resolve_font(&conf, "any"), "");
    }

    #[test]
    fn test_format_tick_label_f_with_zero_produces_zero_string() {
        // "%f" on 0.0 → contains "0".
        let s = format_tick_label("%f", 0.0);
        assert!(s.contains("0"));
    }

    #[test]
    fn test_seek_tick_with_three_synonym_chain_last_one_hit() {
        // "a|b|c" - only c found → returns c's value.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("c".into(), ConfigValue::Str("third".into()));
        let v = seek_tick_str(&tick, &ticks_conf, "a|b|c").unwrap();
        assert_eq!(v, "third");
    }

    #[test]
    fn test_seek_tick_f64_positive_value_parses() {
        // "3.14" → 3.14.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("pi".into(), ConfigValue::Str("3.14".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "pi"), Some(3.14));
    }

    #[test]
    fn test_resolve_font_direct_match_over_default() {
        // Exact key match wins over "default".
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/def.ttf".into()));
        fonts.insert("italic".into(), ConfigValue::Str("/ital.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "italic"), "/ital.ttf");
    }

    #[test]
    fn test_format_tick_label_with_large_negative_integer() {
        // "%d" on -1000000.0 → "-1000000".
        assert_eq!(format_tick_label("%d", -1000000.0), "-1000000");
    }

    #[test]
    fn test_seek_tick_f64_with_integer_value_parses() {
        // Integer value in Str → parses as f64.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("n".into(), ConfigValue::Str("100".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "n"), Some(100.0));
    }

    #[test]
    fn test_seek_tick_with_nonexistent_name_returns_none() {
        // No match in either map → None.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        assert!(seek_tick(&tick, &ticks_conf, "missing").is_none());
    }

    #[test]
    fn test_resolve_font_fallback_to_default_when_missing_key() {
        // Missing "italic" key → falls back to "default".
        let mut fonts: HashMap<String, ConfigValue> = HashMap::new();
        fonts.insert("default".into(), ConfigValue::Str("/def.ttf".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::Map(fonts));
        assert_eq!(resolve_font(&conf, "italic"), "/def.ttf");
    }

    #[test]
    fn test_format_tick_label_dot2f_on_integer_emits_two_decimals() {
        // "%.2f" on 5.0 → "5.00".
        assert_eq!(format_tick_label("%.2f", 5.0), "5.00");
    }

    #[test]
    fn test_seek_tick_f64_with_float_exponent() {
        // Scientific notation ending in no unit.
        let tick: HashMap<String, ConfigValue> = HashMap::new();
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("big".into(), ConfigValue::Str("1e5".into()));
        assert_eq!(seek_tick_f64(&tick, &ticks_conf, "big"), Some(100000.0));
    }

    #[test]
    fn test_format_tick_label_d_zero_value_emits_0() {
        // %d on 0 → "0".
        assert_eq!(format_tick_label("%d", 0.0), "0");
    }

    #[test]
    fn test_resolve_font_fonts_field_not_map_returns_empty() {
        // "fonts" is a List, not Map → as_map None → "".
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("fonts".into(), ConfigValue::List(vec![]));
        assert_eq!(resolve_font(&conf, "bold"), "");
    }

    #[test]
    fn test_seek_tick_returns_first_match_in_tick_scope() {
        // When both tick and ticks_conf have the key, tick wins.
        let mut tick: HashMap<String, ConfigValue> = HashMap::new();
        tick.insert("k".into(), ConfigValue::Str("tick_val".into()));
        let mut ticks_conf: HashMap<String, ConfigValue> = HashMap::new();
        ticks_conf.insert("k".into(), ConfigValue::Str("conf_val".into()));
        let v = seek_tick(&tick, &ticks_conf, "k");
        assert_eq!(v.and_then(|c| c.as_str()), Some("tick_val"));
    }
}
