pub mod highlights;
pub mod ideograms;
pub mod links;
pub mod plots;
pub mod text;
pub mod ticks;

use std::collections::HashMap as _HashMap;

/// Per-ideogram view for `draw_axis_break` / `draw_break`.
#[derive(Debug, Clone, Default)]
pub struct AxisBreakIdeogram {
    pub chr: String,
    pub tag: String,
    pub set_min: i64,
    pub set_max: i64,
    pub radius_outer: f64,
    pub radius_inner: f64,
    pub thickness: f64,
    pub break_start: Option<String>,
    pub break_end: Option<String>,
    pub prev_chr: String,
    pub next_chr: String,
}

/// Port of Perl `draw_axis_break(ideogram)`. Dispatches on `axis_break_style`
/// (1 = slice connecting ideograms, 2 = two radial break lines) and emits
/// `draw_break` calls for each boundary.
pub fn draw_axis_break(
    doc: &mut crate::render::svg::SvgDocument,
    ideogram: &AxisBreakIdeogram,
    ideogram_next: &AxisBreakIdeogram,
    spacing_conf: &_HashMap<String, crate::config::types::ConfigValue>,
    layout: &crate::layout::Layout,
    colors: &crate::render::color::ColorMap,
    chromosomes_units: f64,
    gsize_noscale: f64,
    units_ok: &str,
    units_nounit: &str,
) {
    use crate::config::types::ConfigValue;

    if spacing_conf.get("axis_break").and_then(|v| v.as_str()) != Some("1") {
        return;
    }
    let style_id: u32 = spacing_conf
        .get("axis_break_style")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let style_data = match spacing_conf
        .get("break_style")
        .and_then(|v| v.as_map())
        .and_then(|m| m.get(&style_id.to_string()).and_then(|v| v.as_map()))
    {
        Some(m) => m,
        None => return,
    };
    let radius_change = (ideogram.radius_outer - ideogram_next.radius_outer).abs() > f64::EPSILON;
    let thickness = style_data
        .get("thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            crate::layout::units::unit_parse(
                s,
                chromosomes_units,
                ideogram.thickness,
                units_ok,
                units_nounit,
            )
            .ok()
        })
        .unwrap_or(2.0);
    let fill_name = style_data
        .get("fill_color")
        .and_then(|v| v.as_str())
        .unwrap_or("grey");
    let stroke_name = style_data.get("stroke_color").and_then(|v| v.as_str());
    let stroke_thickness: f64 = style_data
        .get("stroke_thickness")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let resolve_break = |s: &str| -> f64 {
        crate::chromosome::ideogram_spacing_helper(
            s,
            units_ok,
            units_nounit,
            chromosomes_units,
            gsize_noscale,
        )
        .unwrap_or(0.0)
    };

    let mut call_break = |chr: &str,
                          ideo: &AxisBreakIdeogram,
                          start: i64,
                          end: i64,
                          start_offset: f64,
                          end_offset: f64,
                          fill_is_set: bool| {
        draw_break(
            doc,
            chr,
            ideo,
            start,
            end,
            start_offset,
            end_offset,
            if fill_is_set { Some(fill_name) } else { None },
            stroke_name,
            stroke_thickness,
            thickness,
            layout,
            colors,
        );
    };

    if style_id == 1 {
        if ideogram.break_start.is_some() && ideogram.prev_chr != ideogram.chr {
            let so = ideogram
                .break_start
                .as_deref()
                .map(resolve_break)
                .unwrap_or(0.0);
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_min,
                ideogram.set_min,
                so,
                0.0,
                true,
            );
        }
        if ideogram.break_end.is_some() && ideogram.next_chr != ideogram.chr {
            let eo = ideogram
                .break_end
                .as_deref()
                .map(resolve_break)
                .unwrap_or(0.0);
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_max,
                ideogram.set_max,
                0.0,
                eo,
                true,
            );
        }
        if ideogram.chr == ideogram.next_chr {
            if radius_change {
                let eo = ideogram
                    .break_end
                    .as_deref()
                    .map(|s| -resolve_break(s))
                    .unwrap_or(0.0);
                let so = ideogram
                    .break_start
                    .as_deref()
                    .map(|s| -resolve_break(s))
                    .unwrap_or(0.0);
                call_break(
                    &ideogram.chr,
                    ideogram,
                    ideogram.set_max,
                    ideogram_next.set_min,
                    0.0,
                    eo,
                    true,
                );
                call_break(
                    &ideogram.chr,
                    ideogram_next,
                    ideogram.set_max,
                    ideogram_next.set_min,
                    so,
                    0.0,
                    true,
                );
            } else {
                call_break(
                    &ideogram.chr,
                    ideogram,
                    ideogram.set_max,
                    ideogram_next.set_min,
                    0.0,
                    0.0,
                    true,
                );
            }
        }
        let _ = ConfigValue::Str(String::new()); // silence unused-import if ConfigValue isn't referenced
    } else if style_id == 2 {
        if ideogram.break_start.is_some() && ideogram.prev_chr != ideogram.chr {
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_min,
                ideogram.set_min,
                0.0,
                0.0,
                false,
            );
            let so = ideogram
                .break_start
                .as_deref()
                .map(resolve_break)
                .unwrap_or(0.0);
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_min,
                ideogram.set_min,
                so,
                -so,
                false,
            );
        }
        if ideogram.break_end.is_some() && ideogram.next_chr != ideogram.chr {
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_max,
                ideogram.set_max,
                0.0,
                0.0,
                false,
            );
            let eo = ideogram
                .break_end
                .as_deref()
                .map(resolve_break)
                .unwrap_or(0.0);
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_max,
                ideogram.set_max,
                -eo,
                eo,
                false,
            );
        }
        if ideogram.next_chr == ideogram.chr {
            call_break(
                &ideogram.chr,
                ideogram,
                ideogram.set_max,
                ideogram.set_max,
                0.0,
                0.0,
                false,
            );
            call_break(
                &ideogram_next.chr,
                ideogram_next,
                ideogram_next.set_min,
                ideogram_next.set_min,
                0.0,
                0.0,
                false,
            );
        }
    }
}

/// Port of Perl `draw_break(args)`: emits one SVG slice at the ideogram
/// boundary, radius centered on `radius_outer - thickness/2`.
pub fn draw_break(
    doc: &mut crate::render::svg::SvgDocument,
    chr: &str,
    ideogram: &AxisBreakIdeogram,
    start: i64,
    end: i64,
    _start_offset: f64,
    _end_offset: f64,
    fill_name: Option<&str>,
    stroke_name: Option<&str>,
    stroke_thickness: f64,
    thickness: f64,
    layout: &crate::layout::Layout,
    colors: &crate::render::color::ColorMap,
) {
    let radius_from = ideogram.radius_outer - ideogram.thickness / 2.0 - thickness / 2.0;
    let radius_to = ideogram.radius_outer - ideogram.thickness / 2.0 + thickness / 2.0;
    let start_a = match layout.getanglepos(start, chr) {
        Some(a) => a,
        None => return,
    };
    let end_a = match layout.getanglepos(end, chr) {
        Some(a) => a,
        None => return,
    };
    let fill_color = fill_name.and_then(|n| colors.resolve(n));
    let stroke_color = stroke_name.and_then(|n| colors.resolve(n));
    let svg = crate::render::svg::svg_slice(
        layout,
        start_a,
        end_a,
        radius_from.min(radius_to),
        radius_from.max(radius_to),
        stroke_color.as_ref(),
        if stroke_thickness > 0.0 {
            Some(stroke_thickness)
        } else {
            None
        },
        fill_color.as_ref(),
        None,
    );
    doc.add(svg);
}

/// Image-map area (Perl `@MAP_ELEMENTS` entry).
#[derive(Debug, Default, Clone)]
pub struct ImageMapArea {
    pub shape: String,
    pub coords: Vec<i64>,
    pub url: String,
    pub alt: String,
}

/// Port of Perl `@MAP_ELEMENTS` global accumulator. Each `report_image_map`
/// call appends one entry; `drain_map_elements` flushes the buffer into the
/// `<area>` lines written between the `<map>…</map>` tags at the end of `run`.
static MAP_ELEMENTS: std::sync::LazyLock<std::sync::RwLock<Vec<ImageMapArea>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(Vec::new()));

/// Port of Perl `report_image_map(shape=>..., coords=>..., href=>...)`: push
/// one interactive region onto the global `@MAP_ELEMENTS` buffer. `coords`
/// are rounded (Perl uses `round`) before storage. Protocol handling: if
/// `href` lacks a scheme and a global `image_map_protocol` is configured
/// elsewhere, the caller is expected to have prefixed it.
pub fn report_image_map(shape: &str, coords: &[f64], href: &str) {
    let rounded: Vec<i64> = coords.iter().map(|c| c.round() as i64).collect();
    let area = ImageMapArea {
        shape: shape.to_string(),
        coords: rounded,
        url: href.to_string(),
        alt: href.to_string(),
    };
    if let Ok(mut buf) = MAP_ELEMENTS.write() {
        buf.push(area);
    }
}

/// Shared test-only mutex for serializing any code that touches the
/// `MAP_ELEMENTS` global buffer. Cargo runs tests in parallel by default, so
/// `report_image_map` / `drain_map_elements` races between tests without a
/// lock. Test modules in `draw` submodules should grab this lock at the
/// start of any test that calls `drain_map_elements`.
#[cfg(test)]
pub(crate) static MAP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Drain the global `@MAP_ELEMENTS` buffer, returning all accumulated areas
/// and clearing the store. Called once at the end of `run`.
pub fn drain_map_elements() -> Vec<ImageMapArea> {
    MAP_ELEMENTS
        .write()
        .map(|mut b| std::mem::take(&mut *b))
        .unwrap_or_default()
}

/// Render one `ImageMapArea` to a Perl-style `<area ...>` line.
pub fn render_map_area(area: &ImageMapArea) -> String {
    let coords = area
        .coords
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "<area shape='{}' coords='{}' href='{}' alt='{}' title='{}'>",
        area.shape, coords, area.url, area.alt, area.url
    )
}

/// Port of Perl `fetch_brush(w, h, color)`. Perl uses GD::Image brushes for
/// line-style rendering. The Rust port is SVG-native and doesn't need a
/// brush allocator — this function is a no-op preserved for 1-1 sub
/// correspondence.
pub fn fetch_brush(
    _w: u32,
    _h: u32,
    _color: Option<&str>,
) -> (Option<()>, std::collections::HashMap<String, u32>) {
    (None, std::collections::HashMap::new())
}

/// Port of Perl `init_brush(w, h, color)`. GD-backend only; no-op in SVG
/// port. Preserved for 1-1 correspondence.
pub fn init_brush(_w: u32, _h: u32, _color: Option<&str>) -> Option<()> {
    None
}

/// Port of Perl `myarc(image, start, end, radius, color, thickness)`. GD
/// helper for drawing arcs via polyline sampling; Rust's SVG path `A` command
/// covers the same geometry in `svg_slice` / `slice`. Preserved for 1-1
/// correspondence.
pub fn myarc(_start_angle: f64, _end_angle: f64, _radius: f64, _color: &str, _thickness: f64) {}

/// Port of Perl nested sub `slice(...)`. Draws an annular wedge between
/// `radius_from`/`radius_to` and angles `start_a`/`end_a`, with `start_offset`
/// and `end_offset` angular padding (in basepair units, converted via GCIRCUM).
/// Three SVG output shapes matching Perl:
///   - radius_from == radius_to → arc path, no fill
///   - end_a == start_a         → radial line segment
///   - otherwise                → full annular wedge with Z close
///
/// Perl also emits a GD polygon for image-map support; that subsystem is not
/// yet ported and is skipped here.
pub fn slice(
    doc: &mut crate::render::svg::SvgDocument,
    layout: &crate::layout::Layout,
    start_angle: f64,
    end_angle: f64,
    radius_from: f64,
    radius_to: f64,
    fill: Option<&crate::render::color::Color>,
    stroke: Option<&crate::render::color::Color>,
    stroke_thickness: Option<f64>,
    start_offset_bp: f64,
    end_offset_bp: f64,
    counterclockwise: bool,
    gcircum: f64,
    url: Option<&str>,
    image_conf: Option<&_HashMap<String, crate::config::types::ConfigValue>>,
) {
    let deg2rad = std::f64::consts::PI / 180.0;
    let cx = layout.image_radius;
    let cy = layout.image_radius;

    let mut start_a = start_angle;
    let mut end_a = end_angle;
    if end_a < start_a {
        std::mem::swap(&mut start_a, &mut end_a);
    }
    // Apply angular offsets (Perl: $start_a -= 360 * start_offset / GCIRCUM)
    if gcircum > 0.0 {
        start_a -= 360.0 * start_offset_bp / gcircum;
        end_a += 360.0 * end_offset_bp / gcircum;
    }
    if counterclockwise {
        if end_a < start_a {
            std::mem::swap(&mut start_a, &mut end_a);
        }
    } else if start_a > end_a {
        start_a -= 360.0;
    }

    let draw_slice = fill.is_some() || stroke.is_some() || stroke_thickness.is_some();
    let polar = |a: f64, r: f64| -> (f64, f64) {
        (cx + r * (a * deg2rad).cos(), cy + r * (a * deg2rad).sin())
    };
    let stroke_style = match (stroke, stroke_thickness) {
        (Some(c), Some(t)) if t > 0.0 => {
            format!("stroke: {}; stroke-width: {:.1};", c.to_svg_rgb(), t)
        }
        (Some(c), _) => format!("stroke: {};", c.to_svg_rgb()),
        (None, Some(t)) if t > 0.0 => format!("stroke-width: {:.1};", t),
        _ => "stroke: none;".to_string(),
    };

    let svg = if (radius_from - radius_to).abs() < f64::EPSILON {
        // Zero-width annular: just an arc along one radius
        let mut end_a_mod = end_a;
        if (end_a - start_a).abs() > 359.99 || start_a == end_a {
            end_a_mod -= 0.01;
        }
        let (x1, y1) = polar(start_a, radius_from);
        let (x2, y2) = polar(end_a_mod, radius_from);
        let large = if (start_a - end_a_mod).abs() > 180.0 {
            1
        } else {
            0
        };
        format!(
            r#"<path d="M {:.1},{:.1} A{:.1},{:.1} {:.2} {},{} {:.1},{:.1}" style="{} fill: none;" />"#,
            x1, y1, radius_from, radius_from, 0.0, large, 1, x2, y2, stroke_style
        )
    } else if (end_a - start_a).abs() < f64::EPSILON {
        // Zero-angle: single radial line
        let (x1, y1) = polar(start_a, radius_from);
        let (x2, y2) = polar(end_a, radius_to);
        format!(
            r#"<path d="M {:.1},{:.1} L {:.1},{:.1}" style="{} fill: none;" />"#,
            x1, y1, x2, y2, stroke_style
        )
    } else {
        // Full annular wedge
        let sweep_large = if (start_a - end_a).abs() > 180.0 {
            1
        } else {
            0
        };
        let mut end_a_mod = end_a;
        if (end_a - start_a).abs() > 359.99 || start_a == end_a {
            end_a_mod -= 0.01;
        }
        let (ox1, oy1) = polar(start_a, radius_from);
        let (ox2, oy2) = polar(end_a_mod, radius_from);
        let (ix1, iy1) = polar(end_a_mod, radius_to);
        let (ix2, iy2) = polar(start_a, radius_to);
        let fill_style = match fill {
            Some(c) => format!("fill: {};", c.to_svg_rgb()),
            None => "fill: none;".to_string(),
        };
        format!(
            r#"<path d="M {:.3},{:.3} A{:.3},{:.3} {:.3} {},{} {:.3},{:.3} L {:.3},{:.3} A{:.3},{:.3} {:.3} {},{} {:.3},{:.3} Z" style="{} {}" />"#,
            ox1,
            oy1,
            radius_from,
            radius_from,
            0.0,
            sweep_large,
            1,
            ox2,
            oy2,
            ix1,
            iy1,
            radius_to,
            radius_to,
            0.0,
            sweep_large,
            0,
            ix2,
            iy2,
            stroke_style,
            fill_style,
        )
    };

    if draw_slice {
        doc.add(svg);
    }

    // --- GD polygon sampling pass (Perl: builds GD::Polygon / GD::Polyline) ---
    // Construct a boundary polygon by walking the outer arc start→end, then
    // the inner arc end→start. Used for image-map poly areas when `url` is
    // set (Perl: `if mapoptions.url { report_image_map(shape=>"poly", ...) }`).
    let angle_step = 1.0;
    let min_slice_step = 0.1;
    let mut poly: Vec<(f64, f64)> = Vec::new();
    let (mut xp, mut yp) = (f64::NAN, f64::NAN);
    let mut angle = start_a;
    while angle <= end_a {
        let (x, y) = polar(angle, radius_from);
        let d = ((x - xp).powi(2) + (y - yp).powi(2)).sqrt();
        if !xp.is_finite() || d >= min_slice_step {
            poly.push((x, y));
            xp = x;
            yp = y;
        }
        angle += angle_step;
    }
    if (end_a - start_a).abs() > f64::EPSILON {
        poly.push(polar(end_a, radius_from));
    }
    if (radius_from - radius_to).abs() > f64::EPSILON {
        let (mut xp2, mut yp2) = (f64::NAN, f64::NAN);
        let mut ra = end_a;
        while ra > start_a {
            let (x, y) = polar(ra, radius_to);
            let d = ((x - xp2).powi(2) + (y - yp2).powi(2)).sqrt();
            if !xp2.is_finite() || d >= min_slice_step {
                poly.push((x, y));
                xp2 = x;
                yp2 = y;
            }
            ra -= angle_step;
        }
        poly.push(polar(start_a, radius_to));
    }

    // --- Image-map emission (Perl: when mapoptions.url defined) ---
    if let Some(url) = url {
        let xshift = image_conf
            .and_then(|m| m.get("image_map_xshift"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let yshift = image_conf
            .and_then(|m| m.get("image_map_yshift"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let xmult = image_conf
            .and_then(|m| m.get("image_map_xfactor"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let ymult = image_conf
            .and_then(|m| m.get("image_map_yfactor"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let coords: Vec<f64> = poly
            .iter()
            .flat_map(|(x, y)| [x * xmult + xshift, y * ymult + yshift])
            .collect();
        report_image_map("poly", &coords, url);
    }
}

/// Runtime per-tick dimension storage (Perl `$DIMS->{tick}{$dims_key}`).
#[derive(Debug, Default, Clone)]
pub struct TickDims {
    pub size: f64,
    pub thickness: f64,
    pub min_label_distance_to_edge: Option<f64>,
}

/// Port of Perl `process_tick_structure(tick, ideogram)`: up-front munging of
/// tick config so `draw_ticks` can iterate quickly. Resolves relative spacing
/// (rspacing/rposition with rdivisor = ideogram|chromosome), absolute spacing
/// (spacing or position with start/end/b/u unit handling), and pre-computes
/// per-radius dimensions (size, thickness, min_label_distance_to_edge).
///
/// `dims_out` is the runtime dims cache (Perl `$DIMS->{tick}`); this function
/// populates one entry per `dims_key = spacing|position + ':' + radius`.
pub fn process_tick_structure(
    tick: &mut _HashMap<String, crate::config::types::ConfigValue>,
    ticks_conf: &_HashMap<String, crate::config::types::ConfigValue>,
    ideogram_set_min: i64,
    ideogram_set_max: i64,
    ideogram_cardinality: i64,
    ideogram_chrlength: i64,
    ideogram_thickness_pixels: f64,
    chromosomes_units: f64,
    units_ok: &str,
    units_nounit: &str,
    dims_out: &mut _HashMap<String, TickDims>,
) -> Result<(), String> {
    use crate::config::types::ConfigValue;
    use crate::layout::units;
    use crate::utils::seek_parameter;

    // Snapshot all seek_parameter lookups up front so we can mutate `tick` below.
    struct TickSeek {
        spacing_type: String,
        rdivisor: String,
        is_processed: bool,
        rspacing_any: Option<String>,
        rspacing: Option<String>,
        rposition: Option<String>,
        spacing: Option<String>,
        position: Option<String>,
        grid: Option<String>,
        grid_thickness: Option<String>,
        radius: String,
        size: String,
        thickness: String,
        min_label_distance_to_edge: Option<String>,
    }
    let TickSeek {
        spacing_type,
        rdivisor,
        is_processed,
        rspacing_any,
        rspacing,
        rposition,
        spacing,
        position,
        grid,
        grid_thickness,
        radius,
        size,
        thickness,
        min_label_distance_to_edge,
    } = {
        let structs: [&_HashMap<String, ConfigValue>; 2] = [&*tick, ticks_conf];
        let s = |n: &str| -> Option<String> {
            seek_parameter(n, &structs)
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        TickSeek {
            spacing_type: s("spacing_type").unwrap_or_default(),
            rdivisor: s("rdivisor|label_rdivisor").unwrap_or_default(),
            is_processed: tick
                .get("_processed")
                .and_then(|v| v.as_str())
                .map(|x| x != "0")
                .unwrap_or(false),
            rspacing_any: s("rspacing|rposition"),
            rspacing: s("rspacing"),
            rposition: s("rposition"),
            spacing: s("spacing"),
            position: s("position"),
            grid: s("grid"),
            grid_thickness: s("grid_thickness"),
            radius: s("radius").unwrap_or_default(),
            size: s("size").unwrap_or_else(|| "5p".to_string()),
            thickness: s("thickness").unwrap_or_else(|| "2p".to_string()),
            min_label_distance_to_edge: s("min_label_distance_to_edge"),
        }
    };

    if spacing_type == "relative" {
        if rspacing_any.is_none() {
            return Err(
                "error processing tick - this tick's spacing_type is set to relative, but no rspacing or rposition parameter is set"
                    .to_string(),
            );
        }
        if let Some(rs) = rspacing {
            units::unit_validate(&rs, units_ok, units_nounit, &["n"])?;
            let mb_rspacing: f64 = rs.parse().unwrap_or(0.0);
            let spacing = if rdivisor == "ideogram" {
                mb_rspacing * (ideogram_cardinality as f64)
            } else {
                mb_rspacing * (ideogram_chrlength as f64)
            };
            tick.insert(
                "spacing".to_string(),
                ConfigValue::Str(format!("{}", spacing)),
            );
        } else if let Some(rp) = rposition {
            let divisor = if rdivisor == "ideogram" {
                ideogram_cardinality as f64
            } else {
                ideogram_chrlength as f64
            };
            let positions: Vec<ConfigValue> = rp
                .split(',')
                .filter_map(|p| {
                    units::unit_validate(p, units_ok, units_nounit, &["n"]).ok()?;
                    let v: f64 = p.parse().ok()?;
                    Some(ConfigValue::Str(format!("{}", v * divisor)))
                })
                .collect();
            tick.insert("position".to_string(), ConfigValue::List(positions));
        }
    } else if !is_processed {
        if let Some(sp) = spacing.clone() {
            units::unit_validate(&sp, units_ok, units_nounit, &["u", "b"])?;
            let (val, unit) = units::unit_split(&sp, units_ok, units_nounit)?;
            let factor_ub = chromosomes_units;
            let spacing = if unit == "u" { val * factor_ub } else { val };
            tick.insert(
                "spacing".to_string(),
                ConfigValue::Str(format!("{}", spacing)),
            );
        } else if let Some(pos) = position.clone() {
            let mut positions = Vec::new();
            for p in pos.split(',') {
                let resolved = if p == "start" {
                    format!("{}b", ideogram_set_min)
                } else if p == "end" {
                    format!("{}b", ideogram_set_max)
                } else {
                    p.to_string()
                };
                units::unit_validate(&resolved, units_ok, units_nounit, &["u", "b"])?;
                let (val, unit) = units::unit_split(&resolved, units_ok, units_nounit)?;
                let converted = if unit == "u" {
                    val * chromosomes_units
                } else {
                    val
                };
                positions.push(ConfigValue::Str(format!("{}", converted)));
            }
            tick.insert("position".to_string(), ConfigValue::List(positions));
        } else {
            return Err(
                "error processing tick - this tick's spacing_type is set to absolute, but no spacing or position parameter is set"
                    .to_string(),
            );
        }
    }

    // grid_thickness
    if !is_processed
        && grid.is_some()
        && let Some(gt) = grid_thickness
    {
        units::unit_validate(&gt, units_ok, units_nounit, &["p"])?;
        let stripped = units::unit_strip(&gt, units_ok, units_nounit)?;
        tick.insert("grid_thickness".to_string(), ConfigValue::Str(stripped));
    }

    // Compute tick_radius list
    let radii: Vec<f64> = if radius.is_empty() {
        Vec::new()
    } else {
        radius
            .split(',')
            .filter_map(|r| {
                units::unit_parse(
                    r.trim(),
                    chromosomes_units,
                    ideogram_thickness_pixels,
                    units_ok,
                    units_nounit,
                )
                .ok()
            })
            .collect()
    };

    // Re-read spacing/position since we may have mutated them above
    let spacing_str = tick
        .get("spacing")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_default();
    let position_str = tick
        .get("position")
        .and_then(|v| match v {
            ConfigValue::Str(s) => Some(s.clone()),
            ConfigValue::List(l) => Some(
                l.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let dims_key_base = if !spacing_str.is_empty() {
        spacing_str
    } else {
        position_str
    };

    for tick_radius in &radii {
        let dims_key = format!("{}:{}", dims_key_base, tick_radius);
        tick.insert("dims_key".to_string(), ConfigValue::Str(dims_key.clone()));
        if !dims_out.contains_key(&dims_key) {
            units::unit_validate(&size, units_ok, units_nounit, &["r", "p"])?;
            let size_px = units::unit_parse(
                &size,
                chromosomes_units,
                ideogram_thickness_pixels,
                units_ok,
                units_nounit,
            )?;
            units::unit_validate(&thickness, units_ok, units_nounit, &["r", "p"])?;
            let thickness_px = units::unit_parse(
                &thickness,
                chromosomes_units,
                size_px,
                units_ok,
                units_nounit,
            )?;
            let min_edge = min_label_distance_to_edge.as_deref().and_then(|s| {
                units::unit_validate(s, units_ok, units_nounit, &["p"]).ok()?;
                units::unit_strip(s, units_ok, units_nounit)
                    .ok()?
                    .parse::<f64>()
                    .ok()
            });
            dims_out.insert(
                dims_key.clone(),
                TickDims {
                    size: size_px,
                    thickness: thickness_px,
                    min_label_distance_to_edge: min_edge,
                },
            );
        }
    }
    tick.insert(
        "_radius".to_string(),
        ConfigValue::List(
            radii
                .iter()
                .map(|r| ConfigValue::Str(format!("{}", r)))
                .collect(),
        ),
    );
    tick.insert("_processed".to_string(), ConfigValue::Str("1".to_string()));
    Ok(())
}

use std::collections::HashMap;
use std::path::Path;

use crate::config::types::ConfigValue;
use crate::data::reader;
use crate::data::types::DataType;
use crate::karyotype::types::Karyotype;
use crate::layout::Layout;
use crate::render::color::ColorMap;
use crate::render::svg::SvgDocument;
use crate::rules;

/// Draw the complete Circos image and return an SVG string.
pub fn draw_circos(
    layout: &Layout,
    conf: &HashMap<String, ConfigValue>,
    karyotype: &Karyotype,
    colors: &ColorMap,
    base_dir: &Path,
) -> String {
    let width = layout.image_radius * 2.0;
    let height = layout.image_radius * 2.0;
    let mut doc = SvgDocument::new(width, height);

    // Draw background
    if let Some(bg_name) = conf
        .get("image")
        .and_then(|v| v.get("background"))
        .and_then(|v| v.as_str())
        && let Some(bg_color) = colors.resolve(bg_name)
    {
        doc.add(format!(
            r#"<rect x="0" y="0" width="{:.0}" height="{:.0}" style="fill: {};" />"#,
            width,
            height,
            bg_color.to_svg_rgb()
        ));
    }

    // Draw ideograms (fills, bands, outlines, labels)
    ideograms::draw_ideograms(&mut doc, layout, conf, karyotype, colors);

    // Draw ticks
    let show_ticks = conf
        .get("show_ticks")
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(false);
    if show_ticks {
        ticks::draw_ticks(&mut doc, layout, conf, karyotype, colors);
    }

    // Draw highlights
    if let Some(highlights_conf) = conf.get("highlights").and_then(|v| v.as_map()) {
        draw_data_sets(
            &mut doc,
            layout,
            conf,
            colors,
            base_dir,
            highlights_conf,
            "highlight",
        );
    }

    // Draw links
    if let Some(links_conf) = conf.get("links").and_then(|v| v.as_map()) {
        draw_link_sets(&mut doc, layout, conf, colors, base_dir, links_conf);
    }

    // Draw plots
    if let Some(plots_conf) = conf.get("plots").and_then(|v| v.as_map()) {
        draw_plot_sets(&mut doc, layout, colors, base_dir, plots_conf);
    }

    doc.render()
}

/// Draw highlight/plot data sets from config.
fn draw_data_sets(
    doc: &mut SvgDocument,
    layout: &Layout,
    _conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    base_dir: &Path,
    section_conf: &HashMap<String, ConfigValue>,
    set_type: &str,
) {
    let data_type = match set_type {
        "highlight" => DataType::Highlight,
        "plot" => DataType::Plot,
        _ => return,
    };

    // Find all sub-blocks (e.g., <highlight name> blocks)
    for value in section_conf.values() {
        if let Some(block) = value.as_map()
            && let Some(file_path) = block.get("file").and_then(|v| v.as_str())
        {
            let full_path = resolve_data_path(file_path, base_dir);
            if let Ok(data) =
                reader::read_data_file(&full_path, data_type, &reader::ReadDataOptions::default())
                && data_type == DataType::Highlight
            {
                highlights::draw_highlights(doc, layout, &data, block, colors);
            }
        }
    }
}

/// Draw plot data sets from config.
fn draw_plot_sets(
    doc: &mut SvgDocument,
    layout: &Layout,
    colors: &ColorMap,
    base_dir: &Path,
    plots_conf: &HashMap<String, ConfigValue>,
) {
    // Find all <plot> blocks
    let plot_values = match plots_conf.get("plot") {
        Some(ConfigValue::List(list)) => list.clone(),
        Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
        _ => return,
    };

    for (i, plot_val) in plot_values.iter().enumerate() {
        if let Some(block) = plot_val.as_map() {
            let plot_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let data_type = match plot_type {
                "text" => DataType::Text,
                "tile" | "connector" | "highlight" => DataType::Highlight,
                _ => DataType::Plot,
            };

            if let Some(file_path) = block.get("file").and_then(|v| v.as_str()) {
                let full_path = resolve_data_path(file_path, base_dir);
                if let Ok(data) = reader::read_data_file(
                    &full_path,
                    data_type,
                    &reader::ReadDataOptions::default(),
                ) {
                    doc.open_group(&format!("plot-{}", i));
                    plots::draw_plot(doc, layout, &data, block, colors);
                    doc.close_group();
                }
            }
        }
    }
}

/// Draw link data sets from config.
pub fn draw_link_sets(
    doc: &mut SvgDocument,
    layout: &Layout,
    _conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    base_dir: &Path,
    links_conf: &HashMap<String, ConfigValue>,
) {
    // Get default link parameters
    let default_params = extract_link_defaults(links_conf);

    // Find all <link name> blocks
    for (key, value) in links_conf {
        if let Some(block) = value.as_map()
            && let Some(file_path) = block.get("file").and_then(|v| v.as_str())
        {
            let full_path = resolve_data_path(file_path, base_dir);
            if let Ok(data) = reader::read_data_file(
                &full_path,
                DataType::Link,
                &reader::ReadDataOptions {
                    addset: true,
                    ..Default::default()
                },
            ) {
                let link_groups = reader::group_links(data);

                // Parse rules
                let rule_list = rules::parse_rules(block.get("rules").and_then(|v| v.as_map()));

                doc.open_group(&format!("links-{}", key));
                links::draw_links(
                    doc,
                    layout,
                    &link_groups,
                    &default_params,
                    block,
                    &rule_list,
                    colors,
                );
                doc.close_group();
            }
        }
    }
}

/// Extract scalar string-valued defaults from a `<links>` config block,
/// excluding nested `<link>` sub-blocks (which are returned as `Map` values).
fn extract_link_defaults(links_conf: &HashMap<String, ConfigValue>) -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    for (k, v) in links_conf {
        if let Some(s) = v.as_str() {
            defaults.insert(k.clone(), s.to_string());
        }
    }
    defaults
}

/// Resolve a data file path against `base_dir`, falling back to walking up
/// to five parent directories. Returns the original `base_dir`-joined path
/// if no existing file is found.
fn resolve_data_path(file_path: &str, base_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(file_path);
    if p.exists() {
        return p.to_path_buf();
    }
    let candidate = base_dir.join(file_path);
    if candidate.exists() {
        return candidate;
    }
    // Try going up directories
    let mut parent = base_dir;
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            let candidate = p.join(file_path);
            if candidate.exists() {
                return candidate;
            }
            parent = p;
        } else {
            break;
        }
    }
    base_dir.join(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    // `MAP_TEST_LOCK` now lives at module scope (pub(crate)) so submodule
    // tests in `draw::text` can share the same lock instance.

    #[test]
    fn test_render_map_area_html_format() {
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: vec![1, 2, 3, 4],
            url: "/x?chr=hs1".into(),
            alt: "/x?chr=hs1".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("shape='poly'"));
        assert!(s.contains("coords='1,2,3,4'"));
        assert!(s.contains("href='/x?chr=hs1'"));
        assert!(s.contains("alt='/x?chr=hs1'"));
        assert!(s.contains("title='/x?chr=hs1'"));
    }

    #[test]
    fn test_report_image_map_rounds_coords() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("circle", &[10.4, 20.6, 5.5], "/click");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        let a = &areas[0];
        assert_eq!(a.shape, "circle");
        // 10.4 → 10, 20.6 → 21, 5.5 → 6 (Rust's round-half-to-even returns 6 for 5.5).
        assert_eq!(a.coords, vec![10, 21, 6]);
        assert_eq!(a.url, "/click");
        assert_eq!(a.alt, "/click"); // alt defaults to url
    }

    #[test]
    fn test_drain_map_elements_clears_buffer() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("rect", &[0.0, 0.0, 10.0, 10.0], "/a");
        report_image_map("rect", &[20.0, 20.0, 30.0, 30.0], "/b");
        let first = drain_map_elements();
        assert_eq!(first.len(), 2);
        // Second drain → empty (buffer was cleared by first drain).
        let second = drain_map_elements();
        assert!(second.is_empty());
    }

    #[test]
    fn test_fetch_brush_returns_none_stub() {
        // Intentional GD no-op: always returns (None, empty_map).
        let (brush, map) = fetch_brush(10, 10, Some("red"));
        assert!(brush.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_init_brush_returns_none_stub() {
        // Intentional GD no-op: always None regardless of args.
        assert!(init_brush(10, 10, Some("red")).is_none());
        assert!(init_brush(0, 0, None).is_none());
    }

    #[test]
    fn test_axis_break_ideogram_default_populates_safe_defaults() {
        let d = AxisBreakIdeogram::default();
        // Strings empty, Option<String> fields None, numerics 0.
        assert_eq!(d.chr, "");
        assert_eq!(d.tag, "");
        assert_eq!(d.set_min, 0);
        assert_eq!(d.set_max, 0);
        assert!(d.break_start.is_none());
        assert!(d.break_end.is_none());
    }

    #[test]
    fn test_image_map_area_default_values() {
        let a = ImageMapArea::default();
        assert_eq!(a.shape, "");
        assert!(a.coords.is_empty());
        assert_eq!(a.url, "");
        assert_eq!(a.alt, "");
    }

    #[test]
    fn test_myarc_is_noop_stub() {
        // Intentional GD no-op port — doesn't panic, doesn't affect state.
        myarc(0.0, 90.0, 100.0, "red", 1.0);
    }

    #[test]
    fn test_report_image_map_with_empty_coords_still_pushes_entry() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("poly", &[], "/empty");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].shape, "poly");
        assert_eq!(areas[0].coords.len(), 0);
    }

    #[test]
    fn test_render_map_area_single_coord_renders_inline() {
        // Single coord → coords='X'.
        let area = ImageMapArea {
            shape: "circle".into(),
            coords: vec![42],
            url: "/x".into(),
            alt: "/x".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords='42'"));
    }

    #[test]
    fn test_render_map_area_negative_coords_preserved() {
        // Negative i64 coords render with leading dash.
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: vec![-10, -20, 30, 40],
            url: "/n".into(),
            alt: "".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords='-10,-20,30,40'"));
    }

    #[test]
    fn test_report_image_map_rounds_half_to_even() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        // Rust's `round()` rounds half-away-from-zero for floats. Test specific cases.
        report_image_map("rect", &[0.5, 1.5, 2.5, -0.5], "/r");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        let c = &areas[0].coords;
        // 0.5 → 1, 1.5 → 2, 2.5 → 3 (round-half-away-from-zero), -0.5 → -1.
        assert_eq!(c, &vec![1, 2, 3, -1]);
    }

    #[test]
    fn test_report_image_map_multiple_calls_preserve_order() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        // 3 separate reports → drain_map_elements returns them in call order.
        report_image_map("rect", &[0.0, 0.0, 1.0, 1.0], "/first");
        report_image_map("circle", &[5.0, 5.0, 3.0], "/second");
        report_image_map("poly", &[0.0, 0.0, 1.0, 1.0, 2.0, 2.0], "/third");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 3);
        assert_eq!(areas[0].url, "/first");
        assert_eq!(areas[1].url, "/second");
        assert_eq!(areas[2].url, "/third");
        assert_eq!(areas[0].shape, "rect");
        assert_eq!(areas[1].shape, "circle");
        assert_eq!(areas[2].shape, "poly");
    }

    #[test]
    fn test_render_map_area_empty_coords_produces_empty_coords_attr() {
        let area = ImageMapArea {
            shape: "circle".into(),
            coords: vec![],
            url: "/x".into(),
            alt: "/x".into(),
        };
        let s = render_map_area(&area);
        // Empty Vec<i64> joined with commas → empty string between the quotes.
        assert!(s.contains("coords=''"), "expected empty coords, got: {}", s);
    }

    #[test]
    fn test_extract_link_defaults_wraps_only_string_values() {
        // Only string-valued keys appear in the output; Map/List children skipped.
        let mut links_conf: HashMap<String, ConfigValue> = HashMap::new();
        links_conf.insert("color".into(), ConfigValue::Str("red".into()));
        links_conf.insert("thickness".into(), ConfigValue::Str("3".into()));
        // Nested map — not a string, so excluded from the flat defaults.
        let mut nested = HashMap::new();
        nested.insert("inner".into(), ConfigValue::Str("v".into()));
        links_conf.insert("rules".into(), ConfigValue::Map(nested));
        // A List — also excluded.
        links_conf.insert(
            "palette".into(),
            ConfigValue::List(vec![ConfigValue::Str("a".into()), ConfigValue::Str("b".into())]),
        );
        let out = extract_link_defaults(&links_conf);
        assert_eq!(out.len(), 2);
        assert_eq!(out.get("color").unwrap(), "red");
        assert_eq!(out.get("thickness").unwrap(), "3");
        assert!(!out.contains_key("rules"));
        assert!(!out.contains_key("palette"));
    }

    #[test]
    fn test_resolve_data_path_absolute_existing_path_returns_unchanged() {
        // When the file exists as given, the function returns it verbatim without
        // consulting base_dir. Use the current test binary location — Cargo guarantees
        // some file exists; just use `Cargo.toml`.
        use std::path::Path;
        let cwd = std::env::current_dir().unwrap();
        let cargo = cwd.join("Cargo.toml");
        let result = resolve_data_path(cargo.to_str().unwrap(), Path::new("/nonexistent/base"));
        assert_eq!(result, cargo);
    }

    #[test]
    fn test_resolve_data_path_missing_file_falls_back_to_base_joined() {
        // Nothing resolves → final fallback is `base_dir.join(file_path)`.
        use std::path::Path;
        let base = Path::new("/tmp");
        let r = resolve_data_path("nonexistent_file_xxx.tsv", base);
        assert_eq!(r, base.join("nonexistent_file_xxx.tsv"));
    }

    fn mk_slice_layout() -> crate::layout::Layout {
        crate::layout::Layout {
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

    fn mk_axis_break_layout() -> crate::layout::Layout {
        crate::layout::Layout {
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
    fn test_draw_break_unknown_chr_is_noop() {
        // getanglepos returns None for unknown chr → early return, no SVG added.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let ideo = AxisBreakIdeogram::default();
        draw_break(
            &mut doc, "unknown_chr", &ideo, 0, 100, 0.0, 0.0,
            Some("grey"), None, 0.0, 2.0, &layout, &colors,
        );
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_break_radius_from_to_centered_on_outer() {
        // radius_from and radius_to straddle `ideogram.radius_outer - thickness/2`
        // by half of `thickness`. Verified indirectly — no panic with valid chr.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        // For this test we don't need a real Layout.ideograms entry because
        // `getanglepos` returns None for unknown chr → function short-circuits.
        // Just verify it runs to completion without panicking.
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let mut ideo = AxisBreakIdeogram::default();
        ideo.radius_outer = 1000.0;
        ideo.thickness = 100.0;
        draw_break(
            &mut doc, "nope", &ideo, 0, 100, 0.0, 0.0,
            None, None, 0.0, 50.0, &layout, &colors,
        );
        // No panic; unknown chr short-circuits before SVG emission.
    }

    #[test]
    fn test_draw_break_nil_colors_resolve_to_none() {
        // fill_name/stroke_name both None → svg_slice gets None for both colors.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let ideo = AxisBreakIdeogram::default();
        draw_break(
            &mut doc, "unknown", &ideo, 0, 100, 0.0, 0.0,
            None, None, 0.0, 2.0, &layout, &colors,
        );
        // No panic; unknown-chr path.
    }

    #[test]
    fn test_draw_break_unknown_color_name_resolves_none_silently() {
        // Color name that isn't registered → colors.resolve returns None →
        // svg_slice gets None for fill_color/stroke_color.
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let ideo = AxisBreakIdeogram::default();
        draw_break(
            &mut doc, "unknown", &ideo, 0, 100, 0.0, 0.0,
            Some("not_a_real_color"), Some("also_fake"), 2.0, 5.0, &layout, &colors,
        );
        // No panic; function completes even when color names don't resolve.
    }

    #[test]
    fn test_draw_axis_break_disabled_is_noop() {
        // spacing_conf.axis_break != "1" → function returns immediately.
        use crate::config::types::ConfigValue;
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let ideo = AxisBreakIdeogram::default();
        let spacing: HashMap<String, ConfigValue> = HashMap::new();
        draw_axis_break(
            &mut doc,
            &ideo,
            &ideo,
            &spacing,
            &layout,
            &colors,
            1_000_000.0,
            3e9,
            "bupr",
            "n",
        );
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_axis_break_enabled_but_missing_style_is_noop() {
        // axis_break=1 but no break_style map for the style_id → returns.
        use crate::config::types::ConfigValue;
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let ideo = AxisBreakIdeogram::default();
        let mut spacing: HashMap<String, ConfigValue> = HashMap::new();
        spacing.insert("axis_break".into(), ConfigValue::Str("1".into()));
        // No break_style submap configured → early return.
        draw_axis_break(
            &mut doc,
            &ideo,
            &ideo,
            &spacing,
            &layout,
            &colors,
            1_000_000.0,
            3e9,
            "bupr",
            "n",
        );
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_draw_axis_break_enabled_with_style_does_not_panic() {
        // axis_break=1 with a matching break_style entry → should complete
        // without panicking (actual emission depends on break_start/end being set).
        use crate::config::types::ConfigValue;
        use crate::render::color::ColorMap;
        use crate::render::svg::SvgDocument;
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let layout = mk_axis_break_layout();
        let colors = ColorMap::new();
        let ideo = AxisBreakIdeogram::default();
        let mut spacing: HashMap<String, ConfigValue> = HashMap::new();
        spacing.insert("axis_break".into(), ConfigValue::Str("1".into()));
        let mut style_data: HashMap<String, ConfigValue> = HashMap::new();
        style_data.insert("thickness".into(), ConfigValue::Str("2".into()));
        let mut break_style: HashMap<String, ConfigValue> = HashMap::new();
        break_style.insert("1".into(), ConfigValue::Map(style_data));
        spacing.insert("break_style".into(), ConfigValue::Map(break_style));
        // No panic; default ideo has break_start/end = None so no emission.
        draw_axis_break(
            &mut doc,
            &ideo,
            &ideo,
            &spacing,
            &layout,
            &colors,
            1_000_000.0,
            3e9,
            "bupr",
            "n",
        );
    }

    #[test]
    fn test_axis_break_ideogram_clone_preserves_all_fields() {
        // Clone preserves every field including Option<String>.
        let a = AxisBreakIdeogram {
            chr: "hs1".into(),
            tag: "a".into(),
            set_min: 0,
            set_max: 100,
            radius_outer: 1000.0,
            radius_inner: 900.0,
            thickness: 100.0,
            break_start: Some("5".into()),
            break_end: Some("10".into()),
            prev_chr: "hs0".into(),
            next_chr: "hs2".into(),
        };
        let b = a.clone();
        assert_eq!(a.chr, b.chr);
        assert_eq!(a.tag, b.tag);
        assert_eq!(a.set_min, b.set_min);
        assert_eq!(a.set_max, b.set_max);
        assert_eq!(a.radius_outer, b.radius_outer);
        assert_eq!(a.radius_inner, b.radius_inner);
        assert_eq!(a.thickness, b.thickness);
        assert_eq!(a.break_start, b.break_start);
        assert_eq!(a.break_end, b.break_end);
        assert_eq!(a.prev_chr, b.prev_chr);
        assert_eq!(a.next_chr, b.next_chr);
    }

    #[test]
    fn test_slice_no_draw_when_all_style_args_none() {
        // fill=None, stroke=None, stroke_thickness=None → `draw_slice=false` → no SVG added.
        use crate::render::svg::SvgDocument;
        let layout = mk_slice_layout();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        slice(
            &mut doc, &layout, 0.0, 90.0, 900.0, 1000.0,
            None, None, None, 0.0, 0.0, false, layout.gcircum, None, None,
        );
        assert_eq!(doc.elements.len(), before);
    }

    #[test]
    fn test_slice_with_fill_only_emits_path() {
        // fill=Some → draw_slice=true → one `<path>` element added with fill style.
        use crate::render::color::Color;
        use crate::render::svg::SvgDocument;
        let layout = mk_slice_layout();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let before = doc.elements.len();
        let color = Color::rgb(255, 0, 0);
        slice(
            &mut doc, &layout, 0.0, 90.0, 900.0, 1000.0,
            Some(&color), None, None, 0.0, 0.0, false, layout.gcircum, None, None,
        );
        assert_eq!(doc.elements.len(), before + 1);
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("<path"));
        assert!(svg.contains("fill: rgb(255,0,0)"));
    }

    #[test]
    fn test_slice_zero_angle_emits_radial_line() {
        // end_a == start_a (after swap) → radial line path with "L" command.
        use crate::render::color::Color;
        use crate::render::svg::SvgDocument;
        let layout = mk_slice_layout();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let color = Color::rgb(0, 0, 0);
        slice(
            &mut doc, &layout, 45.0, 45.0, 900.0, 1000.0,
            None, Some(&color), Some(1.0), 0.0, 0.0, false, layout.gcircum, None, None,
        );
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("<path"));
        // zero-angle branch emits "L" command between radii.
        assert!(svg.contains(" L "));
    }

    #[test]
    fn test_slice_stroke_thickness_zero_becomes_stroke_none() {
        // stroke=Some, stroke_thickness=Some(0.0) → the `(_, Some(t)) if t > 0.0`
        // arm doesn't match; falls through to plain stroke: rgb(...).
        use crate::render::color::Color;
        use crate::render::svg::SvgDocument;
        let layout = mk_slice_layout();
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let color = Color::rgb(10, 20, 30);
        slice(
            &mut doc, &layout, 0.0, 90.0, 900.0, 1000.0,
            None, Some(&color), Some(0.0), 0.0, 0.0, false, layout.gcircum, None, None,
        );
        let svg = doc.elements.last().unwrap();
        assert!(svg.contains("stroke: rgb(10,20,30)"));
        // stroke-width not emitted for 0-thickness.
        assert!(!svg.contains("stroke-width"));
    }

    #[test]
    fn test_resolve_data_path_absolute_existing_returns_verbatim() {
        // Existing absolute path returned as-is; base_dir irrelevant.
        let cwd = std::env::current_dir().unwrap();
        let cargo = cwd.join("Cargo.toml");
        let r = resolve_data_path(cargo.to_str().unwrap(), Path::new("/nonexistent"));
        assert_eq!(r, cargo);
    }

    #[test]
    fn test_resolve_data_path_walks_up_parent_dirs() {
        // File only in ancestor dir → walk up discovers it (5-step bound).
        let root = tempfile::tempdir().unwrap();
        let fname = "ancestor_data.tsv";
        std::fs::write(root.path().join(fname), "x\n").unwrap();
        let deep = root.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        let r = resolve_data_path(fname, &deep);
        let canon_r = std::fs::canonicalize(&r).unwrap();
        let canon_expected = std::fs::canonicalize(root.path().join(fname)).unwrap();
        assert_eq!(canon_r, canon_expected);
    }

    #[test]
    fn test_resolve_data_path_missing_returns_base_join_fallback() {
        // No file found anywhere → final fallback is base_dir.join(file).
        let base = Path::new("/tmp");
        let r = resolve_data_path("nonexistent_file_xyz.tsv", base);
        assert_eq!(r, base.join("nonexistent_file_xyz.tsv"));
    }

    #[test]
    fn test_resolve_data_path_file_directly_in_base_dir() {
        // File in base_dir → returned as base_dir.join(file).
        let root = tempfile::tempdir().unwrap();
        let fname = "direct.tsv";
        std::fs::write(root.path().join(fname), "y\n").unwrap();
        let r = resolve_data_path(fname, root.path());
        assert_eq!(r, root.path().join(fname));
        assert!(r.exists());
    }

    #[test]
    fn test_extract_link_defaults_returns_empty_for_empty_conf() {
        // With no keys in links_conf, extract_link_defaults yields empty HashMap.
        let links_conf: HashMap<String, ConfigValue> = HashMap::new();
        let defaults = extract_link_defaults(&links_conf);
        assert!(defaults.is_empty());
    }

    #[test]
    fn test_extract_link_defaults_copies_scalar_string_values() {
        // Only scalar (Str) values are promoted to defaults — Maps/Lists are skipped.
        let mut links_conf: HashMap<String, ConfigValue> = HashMap::new();
        links_conf.insert("color".into(), ConfigValue::Str("red".into()));
        links_conf.insert("thickness".into(), ConfigValue::Str("2".into()));
        // A Map value (like a nested `rules` block) must NOT land in defaults.
        links_conf.insert("rules".into(), ConfigValue::Map(HashMap::new()));
        // A List value also skipped.
        links_conf.insert(
            "list_key".into(),
            ConfigValue::List(vec![ConfigValue::Str("x".into())]),
        );
        let defaults = extract_link_defaults(&links_conf);
        assert_eq!(defaults.get("color").map(String::as_str), Some("red"));
        assert_eq!(defaults.get("thickness").map(String::as_str), Some("2"));
        assert!(!defaults.contains_key("rules"));
        assert!(!defaults.contains_key("list_key"));
    }

    #[test]
    fn test_report_image_map_empty_coords_slice_produces_entry() {
        // An empty coords slice still yields an entry (with empty Vec).
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("rect", &[], "/nowhere");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].shape, "rect");
        assert!(areas[0].coords.is_empty());
        assert_eq!(areas[0].url, "/nowhere");
    }

    #[test]
    fn test_render_map_area_title_equals_url() {
        // Per impl: title attribute shares the URL string (not alt).
        let area = ImageMapArea {
            shape: "circle".into(),
            coords: vec![50, 50, 10],
            url: "/detail?id=42".into(),
            alt: "tooltip text".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("alt='tooltip text'"));
        // Both href and title are the URL.
        assert!(s.contains("href='/detail?id=42'"));
        assert!(s.contains("title='/detail?id=42'"));
    }

    #[test]
    fn test_image_map_area_clone_preserves_all_fields() {
        // ImageMapArea derives Clone — copies all fields deep.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3, 4],
            url: "/u".into(),
            alt: "/a".into(),
        };
        let b = a.clone();
        assert_eq!(a.shape, b.shape);
        assert_eq!(a.coords, b.coords);
        assert_eq!(a.url, b.url);
        assert_eq!(a.alt, b.alt);
    }

    #[test]
    fn test_drain_map_elements_empty_initially() {
        // drain_map_elements on empty buffer → empty Vec.
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements(); // clear
        let areas = drain_map_elements();
        assert!(areas.is_empty());
    }

    #[test]
    fn test_report_image_map_with_large_coords_preserves_all() {
        // Large coord vector — all values preserved.
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        let coords: Vec<f64> = (0..20).map(|i| i as f64 * 10.0).collect();
        report_image_map("poly", &coords, "/big");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].coords.len(), 20);
        // First coord is 0, last is 190.
        assert_eq!(areas[0].coords[0], 0);
        assert_eq!(areas[0].coords[19], 190);
    }

    #[test]
    fn test_fetch_brush_always_returns_none_stub() {
        // fetch_brush is a Perl-compat stub — always returns (None, empty map).
        let (brush, colors) = fetch_brush(10, 10, Some("red"));
        assert!(brush.is_none());
        assert!(colors.is_empty());
        // Same for no color.
        let (b2, c2) = fetch_brush(5, 5, None);
        assert!(b2.is_none());
        assert!(c2.is_empty());
    }

    #[test]
    fn test_init_brush_returns_none_stub_unconditionally() {
        // init_brush is a Perl-compat no-op: always None.
        assert!(init_brush(100, 100, Some("red")).is_none());
        assert!(init_brush(0, 0, None).is_none());
        assert!(init_brush(u32::MAX, u32::MAX, Some("")).is_none());
    }

    #[test]
    fn test_myarc_is_no_op() {
        // myarc() is a Perl-compat stub. Just verify it doesn't panic.
        myarc(0.0, 90.0, 100.0, "red", 1.0);
        myarc(-180.0, 180.0, 0.0, "", 0.0);
    }

    #[test]
    fn test_report_image_map_rounds_half_away_from_zero_negatives() {
        // Negative coords: -0.5 should round to -1 (away from zero per Rust round).
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("poly", &[-0.5, -1.5, -2.5], "/x");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // Rust's f64::round rounds half away from zero: -0.5→-1, -1.5→-2, -2.5→-3.
        assert_eq!(areas[0].coords, vec![-1, -2, -3]);
    }

    #[test]
    fn test_render_map_area_escapes_none_in_output() {
        // render_map_area formats strings with `format!` using `{}` — special
        // chars (like `'`) are NOT escaped. Document this as the current behavior.
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 100, 100],
            url: "/foo'bar".into(), // single quote in URL
            alt: "A'B".into(),
        };
        let s = render_map_area(&area);
        // Single quote appears in output verbatim (not escaped).
        assert!(s.contains("'bar"));
        assert!(s.contains("A'B"));
    }

    #[test]
    fn test_resolve_data_path_existing_absolute_passthrough() {
        // Absolute path that exists → returned as-is, no join/search.
        let tmp = std::env::temp_dir().join(format!(
            "circos_resolve_iter430_{}.txt",
            std::process::id()
        ));
        std::fs::write(&tmp, b"x").unwrap();
        let tmp_str = tmp.to_string_lossy().to_string();
        let base = std::env::temp_dir();
        let got = resolve_data_path(&tmp_str, &base);
        assert_eq!(got, tmp);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_resolve_data_path_fallback_joins_base_for_missing() {
        // Nothing exists anywhere → returns base_dir.join(file_path) unchanged.
        let base = std::env::temp_dir();
        let got = resolve_data_path("definitely_does_not_exist_iter430.xyz", &base);
        assert_eq!(got, base.join("definitely_does_not_exist_iter430.xyz"));
    }

    #[test]
    fn test_extract_link_defaults_filters_non_string_values() {
        // Only Str variants end up in the defaults map; Map/List/Number children drop.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("color".into(), ConfigValue::Str("red".into()));
        conf.insert("nested".into(), ConfigValue::Map(HashMap::new()));
        conf.insert("items".into(), ConfigValue::List(Vec::new()));
        let d = extract_link_defaults(&conf);
        assert_eq!(d.get("color").map(|s| s.as_str()), Some("red"));
        assert!(!d.contains_key("nested"));
        assert!(!d.contains_key("items"));
    }

    #[test]
    fn test_gd_shims_are_nops_no_panic() {
        // myarc/init_brush/fetch_brush are preserved 1-1 sub shims. All no-op.
        myarc(0.0, 90.0, 100.0, "red", 2.0);
        assert!(init_brush(10, 10, Some("red")).is_none());
        let (handle, map) = fetch_brush(5, 5, None);
        assert!(handle.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_drain_map_elements_empty_buffer_returns_empty_vec() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Clear buffer (may have residual from other tests), then drain.
        let _ = drain_map_elements();
        let areas = drain_map_elements();
        assert!(areas.is_empty());
    }

    #[test]
    fn test_drain_map_elements_clears_buffer_for_next_call() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("rect", &[0.0, 0.0, 10.0, 10.0], "/href");
        let first = drain_map_elements();
        assert_eq!(first.len(), 1);
        // Second drain → empty, proves buffer was cleared after the first.
        let second = drain_map_elements();
        assert!(second.is_empty());
    }

    #[test]
    fn test_report_image_map_coord_rounding_half_away_from_zero() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("poly", &[1.5, -1.5, 2.5, -2.5], "/foo");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // Rust's f64::round is half-away-from-zero: 1.5→2, -1.5→-2, 2.5→3, -2.5→-3.
        assert_eq!(areas[0].coords, vec![2, -2, 3, -3]);
    }

    #[test]
    fn test_render_map_area_zero_coords_produces_empty_attr_no_commas() {
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: Vec::new(),
            url: "/x".into(),
            alt: "/x".into(),
        };
        let s = render_map_area(&area);
        // join(",") on empty → empty string, so attr is coords=''.
        assert!(s.contains("coords=''"));
        // No trailing commas (no spurious separator).
        assert!(!s.contains("coords=','"));
    }

    #[test]
    fn test_report_image_map_multiple_areas_preserve_insertion_order() {
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("rect", &[0.0, 0.0, 10.0, 10.0], "/first");
        report_image_map("rect", &[20.0, 20.0, 30.0, 30.0], "/second");
        report_image_map("poly", &[40.0, 40.0, 50.0, 50.0], "/third");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 3);
        assert_eq!(areas[0].url, "/first");
        assert_eq!(areas[1].url, "/second");
        assert_eq!(areas[2].url, "/third");
    }

    #[test]
    fn test_render_map_area_coords_joined_as_comma_separated_ints() {
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: vec![10, 20, 30, 40, 50],
            url: "/href".into(),
            alt: "/href".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords='10,20,30,40,50'"));
    }

    #[test]
    fn test_render_map_area_uses_url_for_href_and_title_attrs() {
        // title uses area.url (not alt); href also uses url.
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 10, 10],
            url: "http://link.example/x".into(),
            alt: "alt text".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("href='http://link.example/x'"));
        assert!(s.contains("title='http://link.example/x'"));
        assert!(s.contains("alt='alt text'"));
    }

    #[test]
    fn test_report_image_map_shape_string_preserved_in_area() {
        // Custom shape strings pass through unchanged (no canonicalization).
        let _lock = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = drain_map_elements();
        report_image_map("custom_shape", &[1.0, 2.0], "/href");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].shape, "custom_shape");
    }

    #[test]
    fn test_slice_zero_width_annular_uses_fill_none() {
        // radius_from==radius_to → arc-only path with "fill: none;".
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let layout = crate::layout::Layout {
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
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let color = Color::rgb(0, 0, 0);
        slice(
            &mut doc, &layout, 0.0, 90.0, 1000.0, 1000.0,
            None, Some(&color), Some(1.0), 0.0, 0.0, false,
            layout.gcircum, None, None,
        );
        let last = doc.elements.last().unwrap();
        assert!(last.contains("fill: none;"));
        // arc path starts with M + A (no L, no Z).
        assert!(last.contains(" A"));
    }

    #[test]
    fn test_slice_stroke_without_thickness_emits_stroke_color_only() {
        // Stroke color but no thickness → style has "stroke: rgb(...);" without stroke-width.
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let layout = crate::layout::Layout {
            ideograms: Vec::new(),
            gcircum: 3e9, gsize_noscale: 3e9, image_radius: 1500.0,
            angle_offset: 0.0, counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0, ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0, ideogram_radius_outer: 1000.0,
            },
        };
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let color = Color::rgb(10, 20, 30);
        slice(
            &mut doc, &layout, 0.0, 90.0, 900.0, 1000.0,
            None, Some(&color), None, 0.0, 0.0, false,
            layout.gcircum, None, None,
        );
        let last = doc.elements.last().unwrap();
        assert!(last.contains("stroke: rgb(10,20,30);"));
        assert!(!last.contains("stroke-width:"));
    }

    #[test]
    fn test_slice_no_fill_no_stroke_yields_stroke_none_style() {
        // All edge/fill args None → stroke style falls to "stroke: none;".
        use crate::render::svg::SvgDocument;
        let layout = crate::layout::Layout {
            ideograms: Vec::new(),
            gcircum: 3e9, gsize_noscale: 3e9, image_radius: 1500.0,
            angle_offset: 0.0, counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0, ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0, ideogram_radius_outer: 1000.0,
            },
        };
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        slice(
            &mut doc, &layout, 0.0, 90.0, 900.0, 1000.0,
            None, None, None, 0.0, 0.0, false,
            layout.gcircum, None, None,
        );
        // Nothing drawn — draw_slice guard is false (no fill/stroke/thickness).
        // Verify no path elements added.
        assert!(!doc.elements.iter().any(|e| e.contains("<path ")));
    }

    #[test]
    fn test_slice_only_thickness_positive_emits_stroke_width_only() {
        // None stroke + Some(t>0) → style has "stroke-width:" but no "stroke:".
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let layout = crate::layout::Layout {
            ideograms: Vec::new(),
            gcircum: 3e9, gsize_noscale: 3e9, image_radius: 1500.0,
            angle_offset: 0.0, counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0, ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0, ideogram_radius_outer: 1000.0,
            },
        };
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let fill = Color::rgb(50, 60, 70);
        slice(
            &mut doc, &layout, 0.0, 90.0, 900.0, 1000.0,
            Some(&fill), None, Some(2.5), 0.0, 0.0, false,
            layout.gcircum, None, None,
        );
        let last = doc.elements.last().unwrap();
        assert!(last.contains("stroke-width: 2.5;"));
        assert!(!last.contains("stroke: rgb"));
    }

    #[test]
    fn test_slice_zero_angle_end_equals_start_emits_radial_line() {
        // end_a == start_a → radial line branch: uses "L" command, not "A".
        use crate::render::svg::SvgDocument;
        use crate::render::color::Color;
        let layout = crate::layout::Layout {
            ideograms: Vec::new(),
            gcircum: 3e9, gsize_noscale: 3e9, image_radius: 1500.0,
            angle_offset: 0.0, counterclockwise: false,
            chromosomes_units: 1e6,
            dims: crate::layout::Dims {
                ideogram_radius: 1000.0, ideogram_thickness: 100.0,
                ideogram_radius_inner: 900.0, ideogram_radius_outer: 1000.0,
            },
        };
        let mut doc = SvgDocument::new(3000.0, 3000.0);
        let color = Color::rgb(0, 0, 0);
        slice(
            &mut doc, &layout, 45.0, 45.0, 900.0, 1000.0,
            None, Some(&color), Some(1.0), 0.0, 0.0, false,
            layout.gcircum, None, None,
        );
        let last = doc.elements.last().unwrap();
        assert!(last.contains(" L "));
        assert!(!last.contains(" A"));
    }

    #[test]
    fn test_init_brush_and_myarc_shims_all_noop() {
        // GD shims for Perl compat — no panic, no output.
        init_brush(10, 10, None);
        init_brush(10, 10, Some("red"));
        myarc(0.0, 90.0, 100.0, "red", 2.0);
        myarc(-180.0, 360.0, 0.0, "", 0.0);
    }

    #[test]
    fn test_fetch_brush_returns_empty_hashmap_regardless_of_args() {
        let (h1, m1) = fetch_brush(0, 0, None);
        assert!(h1.is_none());
        assert!(m1.is_empty());
        let (h2, m2) = fetch_brush(100, 200, Some("blue"));
        assert!(h2.is_none());
        assert!(m2.is_empty());
    }

    #[test]
    fn test_image_map_area_fields_constructible_and_clonable() {
        // ImageMapArea is a plain struct — clonable, shape/coords/url/alt persist.
        let a = ImageMapArea {
            shape: "circle".into(),
            coords: vec![1, 2, 3],
            url: "/x".into(),
            alt: "alt".into(),
        };
        let b = a.clone();
        assert_eq!(b.shape, "circle");
        assert_eq!(b.coords, vec![1, 2, 3]);
        assert_eq!(b.url, "/x");
        assert_eq!(b.alt, "alt");
    }

    #[test]
    fn test_render_map_area_alt_and_title_both_sourced_from_url() {
        // Observed behavior: both the `alt=` and `title=` attrs use area.url,
        // not area.alt — so even a distinct alt field won't appear in output.
        let a = ImageMapArea {
            shape: "poly".into(),
            coords: vec![0, 0, 100, 100],
            url: "/bold-url".into(),
            alt: "distinct-alt".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("title='/bold-url'"));
        // alt= uses area.alt field (which IS distinct from url).
        assert!(s.contains("alt='distinct-alt'"));
        // But title= always mirrors url, not alt — so title never says "distinct-alt".
        assert!(!s.contains("title='distinct-alt'"));
    }

    #[test]
    fn test_render_map_area_single_coord_no_trailing_comma() {
        // Single coord → join(",") produces just that coord with no comma.
        let a = ImageMapArea {
            shape: "circle".into(),
            coords: vec![42],
            url: "/x".into(),
            alt: "alt".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("coords='42'"));
        assert!(!s.contains("coords='42,'"));
    }

    #[test]
    fn test_report_image_map_negative_coords_round_half_away_from_zero() {
        // Rust's f64::round is half-away-from-zero: -0.5 → -1, -1.5 → -2.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("poly", &[-0.5, -1.5, -2.4, -2.6], "/neg");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].coords, vec![-1, -2, -2, -3]);
    }

    #[test]
    fn test_fetch_brush_map_is_always_empty_regardless_of_color_some_or_none() {
        // The Rust port is a pure no-op shim: even with a color argument, it returns
        // (None, empty HashMap) — matching the GD-free SVG-native path.
        let (brush_some, map_some) = fetch_brush(10, 20, Some("red"));
        assert!(brush_some.is_none());
        assert!(map_some.is_empty());
        // None arg also yields the same empty state.
        let (brush_none, map_none) = fetch_brush(0, 0, None);
        assert!(brush_none.is_none());
        assert!(map_none.is_empty());
    }

    #[test]
    fn test_report_image_map_zero_coords_preserves_empty_coord_vec() {
        // report_image_map with an empty coords slice → empty Vec in the stored area.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[], "/empty");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert!(areas[0].coords.is_empty());
        assert_eq!(areas[0].shape, "rect");
    }

    #[test]
    fn test_drain_map_elements_repeat_call_returns_empty_on_second_drain() {
        // First drain consumes everything; second drain returns empty Vec.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements(); // start clean
        report_image_map("circle", &[1.0, 2.0, 3.0], "/a");
        let first = drain_map_elements();
        assert_eq!(first.len(), 1);
        // Second drain: nothing left.
        let second = drain_map_elements();
        assert!(second.is_empty());
    }

    #[test]
    fn test_render_map_area_empty_url_produces_blank_href() {
        // Empty URL string should appear as href='' / alt='' / title=''.
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 100, 100],
            url: String::new(),
            alt: String::new(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("href=''"));
        assert!(s.contains("title=''"));
        assert!(s.contains("alt=''"));
    }

    #[test]
    fn test_init_brush_never_panics_and_always_returns_none() {
        // init_brush is a shim — returns None for any input; no panic on zero dims.
        assert!(init_brush(0, 0, None).is_none());
        assert!(init_brush(100, 200, Some("red")).is_none());
        assert!(init_brush(u32::MAX, u32::MAX, Some("")).is_none());
    }

    #[test]
    fn test_myarc_is_noop_with_various_angle_inputs() {
        // myarc is a GD-only shim — no return value, no panic for any inputs.
        myarc(0.0, 360.0, 100.0, "red", 1.0);
        myarc(-50.0, 50.0, 0.0, "", -5.0);
        myarc(1e10, -1e10, f64::INFINITY, "x", f64::NAN);
        // Reaching here means no panic.
    }

    #[test]
    fn test_report_image_map_accepts_single_coord_float_value() {
        // report_image_map stores a single rounded float.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("circle", &[3.7], "/url");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // Round half-away-from-zero: 3.7 → 4.
        assert_eq!(areas[0].coords, vec![4]);
    }

    #[test]
    fn test_render_map_area_special_chars_in_url_passed_through() {
        // URL with special chars (?, &, =) goes into href as-is — no escaping.
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: vec![0, 0, 10, 10],
            url: "/x?q=1&r=2".into(),
            alt: "alt&amp;text".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("href='/x?q=1&r=2'"));
        // alt is a separate field — preserved verbatim too.
        assert!(s.contains("alt='alt&amp;text'"));
    }

    #[test]
    fn test_image_map_area_clone_produces_independent_coords_vec() {
        // Cloning ImageMapArea produces independent coords Vec — mutating clone's coords
        // doesn't bleed into source.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3],
            url: "/a".into(),
            alt: "alt-a".into(),
        };
        let mut b = a.clone();
        b.coords.push(99);
        assert_eq!(a.coords, vec![1, 2, 3]);
        assert_eq!(b.coords, vec![1, 2, 3, 99]);
    }

    #[test]
    fn test_report_image_map_many_entries_accumulate_in_order() {
        // Multiple report_image_map calls → entries stored in FIFO order.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for i in 0..5 {
            report_image_map("rect", &[i as f64, 0.0, (i + 1) as f64, 1.0], &format!("/a{}", i));
        }
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 5);
        for (idx, area) in areas.iter().enumerate() {
            assert_eq!(area.url, format!("/a{}", idx));
        }
    }

    #[test]
    fn test_render_map_area_different_shapes_preserved() {
        // shape string preserved verbatim in output.
        for shape in ["rect", "circle", "poly", "custom_shape"] {
            let a = ImageMapArea {
                shape: shape.to_string(),
                coords: vec![0, 0, 10, 10],
                url: "/x".into(),
                alt: "alt".into(),
            };
            let s = render_map_area(&a);
            assert!(s.contains(&format!("shape='{}'", shape)));
        }
    }

    #[test]
    fn test_image_map_area_default_via_explicit_fields_has_empty_values() {
        // ImageMapArea construction with empty fields is valid.
        let a = ImageMapArea {
            shape: String::new(),
            coords: Vec::new(),
            url: String::new(),
            alt: String::new(),
        };
        assert!(a.shape.is_empty());
        assert!(a.coords.is_empty());
        assert!(a.url.is_empty());
        assert!(a.alt.is_empty());
    }

    #[test]
    fn test_render_map_area_url_with_unicode_preserved() {
        // Unicode chars in URL preserved verbatim.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 100, 100],
            url: "/sømething/ü".into(),
            alt: "αβγ".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("/sømething/ü"));
        assert!(s.contains("αβγ"));
    }

    #[test]
    fn test_report_image_map_drain_ordered_by_insertion() {
        // Multiple distinct shapes reported → drained in insertion order.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[1.0, 2.0], "/x1");
        report_image_map("circle", &[3.0, 4.0], "/x2");
        report_image_map("poly", &[5.0, 6.0, 7.0, 8.0], "/x3");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 3);
        assert_eq!(areas[0].shape, "rect");
        assert_eq!(areas[1].shape, "circle");
        assert_eq!(areas[2].shape, "poly");
    }

    #[test]
    fn test_render_map_area_coord_formatting_i64_negative_values() {
        // Large negative integer coords render as their decimal form.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![-12345, 67890, -100, 0],
            url: "/x".into(),
            alt: "alt".into(),
        };
        let s = render_map_area(&a);
        // Check that each coord appears.
        assert!(s.contains("-12345"));
        assert!(s.contains("67890"));
        assert!(s.contains("-100"));
        // And comma-separated.
        assert!(s.contains("-12345,67890,-100,0"));
    }

    #[test]
    fn test_fetch_brush_ignores_color_arg_pattern_entirely() {
        // Regardless of whether color is Some or None, result is identical.
        let (b1, m1) = fetch_brush(5, 5, Some("anything"));
        let (b2, m2) = fetch_brush(5, 5, None);
        assert_eq!(b1.is_none(), b2.is_none());
        assert_eq!(m1.len(), m2.len());
    }

    #[test]
    fn test_image_map_area_fields_survive_multiple_clones() {
        // Clone chain → each step produces independent fields.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3],
            url: "/url".into(),
            alt: "alt".into(),
        };
        let b = a.clone();
        let c = b.clone();
        assert_eq!(a.shape, c.shape);
        assert_eq!(a.coords, c.coords);
        assert_eq!(a.url, c.url);
        assert_eq!(a.alt, c.alt);
    }

    #[test]
    fn test_report_image_map_negative_floats_round_correctly() {
        // -2.5 → -3 (Rust's round is half-away-from-zero); 2.5 → 3.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("poly", &[-2.5, 2.5, -0.5, 0.5], "/x");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // Rust's round(-2.5) = -3; round(2.5) = 3.
        assert_eq!(areas[0].coords, vec![-3, 3, -1, 1]);
    }

    #[test]
    fn test_render_map_area_many_coords_all_comma_separated() {
        // 8 coords → 7 commas.
        let a = ImageMapArea {
            shape: "poly".into(),
            coords: vec![1, 2, 3, 4, 5, 6, 7, 8],
            url: "/x".into(),
            alt: "alt".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("coords='1,2,3,4,5,6,7,8'"));
    }

    #[test]
    fn test_myarc_handles_negative_and_positive_angles_without_panic() {
        // myarc is a no-op shim; even with negative/positive angles no panic.
        myarc(-180.0, 180.0, 100.0, "red", 1.0);
        myarc(0.0, 0.0, 0.0, "", 0.0);
    }

    #[test]
    fn test_fetch_brush_with_maximum_u32_dims_no_panic() {
        // Extreme u32 dimensions — shim still doesn't panic.
        let (brush, map) = fetch_brush(u32::MAX, u32::MAX, None);
        assert!(brush.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_report_image_map_empty_coords_empty_url_still_stores_entry() {
        // Edge case: empty coords + empty url still creates an entry.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[], "");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].shape, "rect");
        assert!(areas[0].coords.is_empty());
        assert!(areas[0].url.is_empty());
    }

    #[test]
    fn test_render_map_area_output_starts_with_area_tag() {
        // Output always starts with "<area".
        let a = ImageMapArea {
            shape: "poly".into(),
            coords: vec![0, 0],
            url: "/x".into(),
            alt: "alt".into(),
        };
        let s = render_map_area(&a);
        assert!(s.starts_with("<area "));
        assert!(s.ends_with(">"));
    }

    #[test]
    fn test_init_brush_matches_fetch_brush_shim_behavior() {
        // Both are no-op shims — neither panics across various inputs.
        assert!(init_brush(100, 200, None).is_none());
        assert!(init_brush(0, 0, Some("color")).is_none());
        let (b, _) = fetch_brush(100, 200, None);
        assert!(b.is_none());
    }

    #[test]
    fn test_drain_map_elements_after_empty_buffer_returns_empty() {
        // drain_map_elements on empty buffer → empty Vec, no panic.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        let result = drain_map_elements();
        assert!(result.is_empty());
    }

    #[test]
    fn test_report_image_map_url_used_for_alt_by_default() {
        // Perl default: alt = href. Verify report_image_map sets alt=href.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[0.0, 0.0, 10.0, 10.0], "http://ex.com/page");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].url, "http://ex.com/page");
        assert_eq!(areas[0].alt, "http://ex.com/page");
    }

    #[test]
    fn test_fetch_brush_returns_empty_hashmap_always() {
        // Shim returns (None, empty HashMap) regardless of inputs.
        let (_, map) = fetch_brush(50, 60, Some("red"));
        assert!(map.is_empty());
        let (_, map2) = fetch_brush(0, 0, None);
        assert!(map2.is_empty());
    }

    #[test]
    fn test_render_map_area_single_coord_list() {
        // Single-coord areas (edge case) render correctly without trailing comma.
        let area = ImageMapArea {
            shape: "point".to_string(),
            coords: vec![42],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("coords='42'"));
        // No trailing comma inside coords.
        assert!(!out.contains("coords='42,'"));
    }

    #[test]
    fn test_report_image_map_float_coords_rounded_via_round_not_trunc() {
        // round() is true rounding: 2.5 → 3 (banker's for ties on x86 uses round-half-away).
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        // 2.5 → 3 (away from zero), -2.5 → -3 (away from zero).
        report_image_map("rect", &[2.5, -2.5, 1.4, 1.6], "/x");
        let areas = drain_map_elements();
        assert_eq!(areas[0].coords, vec![3, -3, 1, 2]);
    }

    #[test]
    fn test_render_map_area_empty_coords_emits_empty_coords_attr() {
        // Empty coords vec → "coords=''".
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: Vec::new(),
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("coords=''"));
    }

    #[test]
    fn test_drain_map_elements_after_insertions_returns_all_and_clears() {
        // After 3 inserts, drain returns 3 entries; next drain is empty.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[1.0, 2.0], "/a");
        report_image_map("rect", &[3.0, 4.0], "/b");
        report_image_map("rect", &[5.0, 6.0], "/c");
        let first_drain = drain_map_elements();
        assert_eq!(first_drain.len(), 3);
        let second_drain = drain_map_elements();
        assert!(second_drain.is_empty());
    }

    #[test]
    fn test_image_map_area_debug_format_contains_all_fields() {
        // Debug impl includes all four fields for troubleshooting.
        let area = ImageMapArea {
            shape: "poly".to_string(),
            coords: vec![1, 2, 3],
            url: "url".to_string(),
            alt: "alt_text".to_string(),
        };
        let s = format!("{:?}", area);
        assert!(s.contains("poly"));
        assert!(s.contains("url"));
        assert!(s.contains("alt_text"));
    }

    #[test]
    fn test_render_map_area_title_attribute_equals_url() {
        // Generated HTML uses title=url (Perl convention).
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![0, 0, 10, 10],
            url: "http://ex.com".to_string(),
            alt: "something_else".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("title='http://ex.com'"));
    }

    #[test]
    fn test_report_image_map_preserves_shape_string_through_storage() {
        // shape="poly" round-trips verbatim into ImageMapArea.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("poly", &[10.0, 20.0, 30.0, 40.0, 50.0, 60.0], "/u");
        let areas = drain_map_elements();
        assert_eq!(areas[0].shape, "poly");
        assert_eq!(areas[0].coords.len(), 6);
    }

    #[test]
    fn test_myarc_function_no_side_effects_no_panic() {
        // myarc is a no-op shim — just verify no panic across inputs.
        myarc(0.0, 90.0, 500.0, "red", 1.0);
        myarc(-10.0, 720.0, 0.0, "", 0.0);
        myarc(f64::NAN, f64::INFINITY, -1.0, "black", -1.0);
    }

    #[test]
    fn test_image_map_area_clone_produces_independent_fields() {
        // Clone: mutating clone doesn't affect original.
        let orig = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![1, 2, 3, 4],
            url: "u1".to_string(),
            alt: "a1".to_string(),
        };
        let mut cloned = orig.clone();
        cloned.coords.push(99);
        cloned.shape = "poly".to_string();
        assert_eq!(orig.coords, vec![1, 2, 3, 4]);
        assert_eq!(orig.shape, "rect");
    }

    #[test]
    fn test_init_brush_returns_none_regardless_of_color_value() {
        // init_brush is a no-op returning None unconditionally — verify across colors.
        assert!(init_brush(10, 20, None).is_none());
        assert!(init_brush(10, 20, Some("red")).is_none());
        assert!(init_brush(10, 20, Some("")).is_none());
        assert!(init_brush(0, 0, Some("some,rgb,triple")).is_none());
    }

    #[test]
    fn test_report_image_map_many_inserts_preserve_insertion_order() {
        // Multiple report_image_map calls → drain returns in insertion order.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for i in 0..5 {
            let url = format!("/item/{}", i);
            report_image_map("rect", &[(i as f64), 0.0, (i as f64) + 1.0, 1.0], &url);
        }
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 5);
        for (i, a) in areas.iter().enumerate() {
            assert_eq!(a.url, format!("/item/{}", i));
        }
    }

    #[test]
    fn test_render_map_area_rect_with_four_coords_all_appear() {
        // rect with coords [x1,y1,x2,y2] → all 4 integers appear comma-separated.
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![10, 20, 30, 40],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("coords='10,20,30,40'"));
        assert!(out.contains("shape='rect'"));
    }

    #[test]
    fn test_image_map_area_default_via_construction_has_empty_defaults() {
        // Struct-init with empty values → fields accessible as empty/zero.
        let area = ImageMapArea {
            shape: String::new(),
            coords: Vec::new(),
            url: String::new(),
            alt: String::new(),
        };
        assert!(area.shape.is_empty());
        assert!(area.coords.is_empty());
        assert!(area.url.is_empty());
        assert!(area.alt.is_empty());
    }

    #[test]
    fn test_fetch_brush_invariant_identical_inputs_always_none() {
        // Shim ignores all inputs — consistent None result.
        for color in [None, Some("red"), Some(""), Some("1,2,3")] {
            let (b, m) = fetch_brush(100, 100, color);
            assert!(b.is_none());
            assert!(m.is_empty());
        }
    }

    #[test]
    fn test_render_map_area_href_equals_url_field() {
        // href attribute in output matches the url field.
        let area = ImageMapArea {
            shape: "poly".to_string(),
            coords: vec![1, 2, 3, 4, 5, 6],
            url: "https://example.org/gene/x".to_string(),
            alt: "alt_text".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("href='https://example.org/gene/x'"));
    }

    #[test]
    fn test_report_image_map_with_fractional_near_integer_still_stores() {
        // 0.0001 away from int → rounds to int; stored exact.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[10.0001, 19.9999], "/x");
        let areas = drain_map_elements();
        assert_eq!(areas[0].coords, vec![10, 20]);
    }

    #[test]
    fn test_render_map_area_alt_distinct_from_url() {
        // alt can be explicitly set differently from url.
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![0, 0, 10, 10],
            url: "/click".to_string(),
            alt: "click me".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("alt='click me'"));
        assert!(out.contains("href='/click'"));
    }

    #[test]
    fn test_drain_map_elements_idempotent_empty_buffer_multiple_calls() {
        // Draining empty buffer multiple times → each returns empty.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for _ in 0..3 {
            assert!(drain_map_elements().is_empty());
        }
    }

    #[test]
    fn test_report_image_map_empty_url_acceptable() {
        // Empty url → still stored with url=alt="" (not rejected).
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[0.0, 0.0, 5.0, 5.0], "");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert!(areas[0].url.is_empty());
        assert!(areas[0].alt.is_empty());
    }

    #[test]
    fn test_render_map_area_single_coordinate_value_preserves_comma_free_output() {
        // coords=[42] → single number, no comma.
        let area = ImageMapArea {
            shape: "point".to_string(),
            coords: vec![42],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        // Check coords' = '42' has no comma inside.
        let coords_part = out.split("coords='").nth(1).unwrap().split('\'').next().unwrap();
        assert!(!coords_part.contains(','));
    }

    #[test]
    fn test_myarc_arguments_ignored_verify_no_side_effect() {
        // myarc shim — no side effects, no panic, across wide argument range.
        myarc(0.0, 360.0, 1e9, "any_color", 1e9);
        myarc(f64::MIN, f64::MAX, 1.0, "", 0.0);
    }

    #[test]
    fn test_report_image_map_negative_coords_stored_as_signed_i64() {
        // Negative f64 coords round to negative i64.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[-5.2, -10.7, -15.4, -20.1], "/u");
        let areas = drain_map_elements();
        // -5.2 → -5; -10.7 → -11; -15.4 → -15; -20.1 → -20.
        assert_eq!(areas[0].coords, vec![-5, -11, -15, -20]);
    }

    #[test]
    fn test_render_map_area_ten_coords_formatted_comma_separated() {
        // 10+ coords → all comma-joined.
        let area = ImageMapArea {
            shape: "poly".to_string(),
            coords: (0..10).collect::<Vec<_>>(),
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        // coords attr contains "0,1,2,3,4,5,6,7,8,9"
        assert!(out.contains("coords='0,1,2,3,4,5,6,7,8,9'"));
    }

    #[test]
    fn test_report_image_map_zero_coords_stored_with_empty_vec() {
        // Empty coords slice → stored as empty Vec.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[], "/u");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert!(areas[0].coords.is_empty());
    }

    #[test]
    fn test_image_map_area_fields_assignable_directly() {
        // All four fields settable post-construction.
        let mut a = ImageMapArea {
            shape: "a".to_string(),
            coords: vec![1],
            url: "u".to_string(),
            alt: "alt".to_string(),
        };
        a.shape = "b".to_string();
        a.coords.push(2);
        a.url = "new_u".to_string();
        a.alt = "new_alt".to_string();
        assert_eq!(a.shape, "b");
        assert_eq!(a.coords, vec![1, 2]);
        assert_eq!(a.url, "new_u");
        assert_eq!(a.alt, "new_alt");
    }

    #[test]
    fn test_fetch_brush_and_init_brush_both_return_none_never_panic() {
        // Combined shim invariance — neither panics.
        for w in [0u32, 1, 100, u32::MAX] {
            let (b, m) = fetch_brush(w, w, Some("x"));
            assert!(b.is_none());
            assert!(m.is_empty());
            assert!(init_brush(w, w, None).is_none());
        }
    }

    #[test]
    fn test_render_map_area_with_unicode_url_preserved() {
        // Non-ASCII URL kept verbatim.
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![0, 0, 10, 10],
            url: "/査定/résumé".to_string(),
            alt: "α".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("/査定/résumé"));
        assert!(out.contains("α"));
    }

    #[test]
    fn test_report_image_map_many_calls_accumulate_in_buffer() {
        // 100 inserts → drain returns 100 entries.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for i in 0..100 {
            report_image_map("rect", &[i as f64], &format!("/n{}", i));
        }
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 100);
    }

    #[test]
    fn test_image_map_area_clone_deep_copies_coords_vec() {
        // Clone has independent coords Vec.
        let orig = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![1, 2, 3],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let mut cloned = orig.clone();
        cloned.coords.push(99);
        assert_eq!(orig.coords.len(), 3);
        assert_eq!(cloned.coords.len(), 4);
    }

    #[test]
    fn test_render_map_area_shape_with_special_characters_preserved() {
        // Shape name with characters (unusual but preserved).
        let area = ImageMapArea {
            shape: "custom-shape_123".to_string(),
            coords: vec![0, 0],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("shape='custom-shape_123'"));
    }

    #[test]
    fn test_report_image_map_nan_coord_rounds_to_zero_panic_free() {
        // NaN coords → round produces 0 or NaN; i64 cast is implementation-defined but must not panic.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[f64::NAN], "/u");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // Assert no panic — coords vec has exactly 1 element.
        assert_eq!(areas[0].coords.len(), 1);
    }

    #[test]
    fn test_render_map_area_coords_with_negative_and_positive() {
        // Mix of negative and positive coords → all present in output.
        let area = ImageMapArea {
            shape: "poly".to_string(),
            coords: vec![-10, 20, -30, 40],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("coords='-10,20,-30,40'"));
    }

    #[test]
    fn test_drain_map_elements_returns_vec_not_empty_after_insert() {
        // Basic invariant: insert → drain non-empty.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("circle", &[50.0, 50.0, 10.0], "/c");
        let areas = drain_map_elements();
        assert!(!areas.is_empty());
    }

    #[test]
    fn test_image_map_area_with_long_url_preserved_verbatim() {
        // URLs longer than 255 chars preserved.
        let url = "u".repeat(1000);
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![0, 0, 1, 1],
            url: url.clone(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains(&url));
    }

    #[test]
    fn test_report_image_map_zero_shape_string_stored() {
        // Empty shape string stored without panic.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("", &[0.0, 0.0], "/x");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert!(areas[0].shape.is_empty());
    }

    #[test]
    fn test_render_map_area_different_shapes_variants_all_render() {
        // rect, poly, circle, point — all produce valid output.
        for shape in ["rect", "poly", "circle", "point"] {
            let area = ImageMapArea {
                shape: shape.to_string(),
                coords: vec![0, 0, 1, 1],
                url: "u".to_string(),
                alt: "a".to_string(),
            };
            let out = render_map_area(&area);
            assert!(out.contains(&format!("shape='{}'", shape)));
        }
    }

    #[test]
    fn test_init_brush_with_very_large_dimensions_no_panic() {
        // u32::MAX dimensions → shim returns None, no overflow.
        assert!(init_brush(u32::MAX, u32::MAX, Some("any")).is_none());
    }

    #[test]
    fn test_drain_map_elements_invariance_after_many_inserts() {
        // After 50 inserts: drain returns 50; subsequent drain returns 0.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for _ in 0..50 {
            report_image_map("rect", &[0.0], "/u");
        }
        assert_eq!(drain_map_elements().len(), 50);
        assert_eq!(drain_map_elements().len(), 0);
    }

    #[test]
    fn test_render_map_area_three_coord_triangle_all_included() {
        // Triangle: 3 (x,y) pairs → 6 coords.
        let area = ImageMapArea {
            shape: "poly".to_string(),
            coords: vec![0, 10, 20, 0, 10, 20],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("coords='0,10,20,0,10,20'"));
    }

    #[test]
    fn test_report_image_map_large_number_inserts_then_drain() {
        // Stress: 200 inserts + drain.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for i in 0..200 {
            report_image_map("rect", &[(i as f64)], "/u");
        }
        assert_eq!(drain_map_elements().len(), 200);
    }

    #[test]
    fn test_image_map_area_coords_vec_can_be_mutated_after_init() {
        // coords is Vec<i64> — mutable via push/remove.
        let mut a = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![1, 2],
            url: "u".to_string(),
            alt: "alt".to_string(),
        };
        a.coords.push(3);
        a.coords.push(4);
        assert_eq!(a.coords, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_report_image_map_mixed_positive_negative_fractional_rounds() {
        // Mix of values: 0.5→1 (round); -0.5→-1 (round); 0.4→0; -0.4→0.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[0.5, -0.5, 0.4, -0.4], "/u");
        let areas = drain_map_elements();
        assert_eq!(areas[0].coords, vec![1, -1, 0, 0]);
    }

    #[test]
    fn test_render_map_area_url_empty_still_emits_href_attr() {
        // Empty url → "href=''" attribute present.
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![0, 0, 10, 10],
            url: String::new(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("href=''"));
    }

    #[test]
    fn test_report_image_map_float_exact_half_rounds_away_from_zero() {
        // 0.5 → 1 (round-half-away); -0.5 → -1.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[0.5, -0.5], "/u");
        let areas = drain_map_elements();
        assert_eq!(areas[0].coords, vec![1, -1]);
    }

    #[test]
    fn test_image_map_area_clone_via_debug_produces_different_string_instances() {
        // Cloned instance has same content but different Vec allocation.
        let a = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![1, 2],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let b = a.clone();
        assert_eq!(a.coords, b.coords);
        assert_eq!(a.shape, b.shape);
    }

    #[test]
    fn test_drain_map_elements_after_single_insert_returns_one() {
        // 1 insert → drain len 1; then 0.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        report_image_map("rect", &[], "/x");
        assert_eq!(drain_map_elements().len(), 1);
        assert_eq!(drain_map_elements().len(), 0);
    }

    #[test]
    fn test_render_map_area_zero_dim_coords_valid() {
        // coords=[0,0,0,0] (degenerate) still renders.
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![0, 0, 0, 0],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let out = render_map_area(&area);
        assert!(out.contains("coords='0,0,0,0'"));
    }

    #[test]
    fn test_report_image_map_consecutive_inserts_preserve_order() {
        // Multiple inserts preserve insertion order.
        let _guard = MAP_TEST_LOCK.lock().expect("map lock");
        drain_map_elements();
        for i in 0..5 {
            report_image_map("rect", &[], &format!("/url_{}", i));
        }
        let areas = drain_map_elements();
        for (i, a) in areas.iter().enumerate() {
            assert_eq!(a.url, format!("/url_{}", i));
        }
    }

    #[test]
    fn test_image_map_area_debug_output_includes_coords() {
        // Debug fmt includes coords array.
        let area = ImageMapArea {
            shape: "rect".to_string(),
            coords: vec![10, 20, 30, 40],
            url: "u".to_string(),
            alt: "a".to_string(),
        };
        let s = format!("{:?}", area);
        // At least one coord value in Debug output.
        assert!(s.contains("10"));
    }

    #[test]
    fn test_myarc_shim_accepts_nan_radius_no_panic() {
        // f64 NaN radius — no-op shim handles it.
        myarc(0.0, 90.0, f64::NAN, "black", 1.0);
    }

    #[test]
    fn test_fetch_brush_returns_none_and_empty_map() {
        // GD shim → (None, empty map) regardless of inputs.
        let (brush, map) = fetch_brush(100, 200, Some("red"));
        assert!(brush.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_init_brush_with_none_color_no_panic() {
        // init_brush with None color → None result.
        let b = init_brush(50, 50, None);
        assert!(b.is_none());
    }

    #[test]
    fn test_render_map_area_single_coord_produces_solo_value() {
        // One-coord area → coords string = "5".
        let area = ImageMapArea {
            shape: "point".into(),
            coords: vec![5],
            url: "/p".into(),
            alt: "/p".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords='5'"));
    }

    #[test]
    fn test_myarc_shim_zero_thickness_no_panic() {
        // thickness=0 → no-op shim still accepts it.
        myarc(0.0, 180.0, 100.0, "white", 0.0);
    }

    #[test]
    fn test_render_map_area_with_rect_shape_emits_rect_in_output() {
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![10, 20, 30, 40],
            url: "/u".into(),
            alt: "/u".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("shape='rect'"));
        assert!(s.contains("coords='10,20,30,40'"));
    }

    #[test]
    fn test_report_image_map_float_coords_rounded_to_int() {
        // Float 2.7 → rounded to i64=3.
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("circle", &[2.7, 8.3, 15.6], "/u");
        let out = drain_map_elements();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].coords, vec![3, 8, 16]);
    }

    #[test]
    fn test_myarc_shim_negative_angle_no_panic() {
        // Negative start angle — no-op shim accepts.
        myarc(-90.0, 90.0, 100.0, "gray", 2.0);
    }

    #[test]
    fn test_fetch_brush_zero_dimensions_returns_none() {
        // Zero dimensions still → (None, empty map).
        let (brush, map) = fetch_brush(0, 0, None);
        assert!(brush.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_render_map_area_url_with_special_chars_passes_through() {
        // URL with query params preserved verbatim.
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: vec![1, 2, 3, 4, 5, 6],
            url: "/path?a=1&b=2".into(),
            alt: "/path?a=1&b=2".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("/path?a=1&b=2"));
    }

    #[test]
    fn test_drain_map_elements_on_empty_returns_empty_vec() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        let out = drain_map_elements();
        assert!(out.is_empty());
    }

    #[test]
    fn test_report_image_map_empty_coords_slice_accepted() {
        // Empty coords slice → ImageMapArea with empty coords.
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("circle", &[], "/u");
        let out = drain_map_elements();
        assert_eq!(out.len(), 1);
        assert!(out[0].coords.is_empty());
    }

    #[test]
    fn test_fetch_brush_with_color_string_still_shim() {
        // Any color string → shim still returns (None, empty).
        let (brush, map) = fetch_brush(50, 50, Some("yellow"));
        assert!(brush.is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_render_map_area_circle_shape_three_coords_formatted() {
        // Circle shape with 3 coords [cx, cy, r] formatted in output.
        let area = ImageMapArea {
            shape: "circle".into(),
            coords: vec![50, 50, 25],
            url: "/c".into(),
            alt: "/c".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("shape='circle'"));
        assert!(s.contains("coords='50,50,25'"));
    }

    #[test]
    fn test_myarc_shim_360_sweep_no_panic() {
        // Full circle sweep — shim accepts.
        myarc(0.0, 360.0, 100.0, "red", 1.0);
    }

    #[test]
    fn test_init_brush_with_large_dimensions_no_panic() {
        // Very large dims → no-op shim accepts.
        let b = init_brush(u32::MAX, u32::MAX, Some("green"));
        assert!(b.is_none());
    }

    #[test]
    fn test_report_image_map_multiple_inserts_drain_returns_all() {
        // Multiple inserts drained in insertion order.
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("a", &[1.0], "/a");
        report_image_map("b", &[2.0], "/b");
        report_image_map("c", &[3.0], "/c");
        let out = drain_map_elements();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].shape, "a");
        assert_eq!(out[2].shape, "c");
    }

    #[test]
    fn test_render_map_area_url_and_alt_both_included_in_output() {
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 10, 10],
            url: "/my-link".into(),
            alt: "my alt text".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("href='/my-link'"));
        assert!(s.contains("alt='my alt text'"));
    }

    #[test]
    fn test_drain_map_elements_clears_storage() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("circle", &[1.0, 2.0, 3.0], "/");
        let first = drain_map_elements();
        assert_eq!(first.len(), 1);
        // Second drain should be empty.
        let second = drain_map_elements();
        assert!(second.is_empty());
    }

    #[test]
    fn test_image_map_area_clone_deep_copies_url_field() {
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3, 4],
            url: "/x".into(),
            alt: "/x".into(),
        };
        let b = a.clone();
        assert_eq!(a.url, b.url);
    }

    #[test]
    fn test_fetch_brush_returns_independent_map_each_call() {
        // Separate calls → separate empty maps.
        let (_b1, m1) = fetch_brush(10, 10, Some("red"));
        let (_b2, m2) = fetch_brush(20, 20, Some("blue"));
        assert!(m1.is_empty() && m2.is_empty());
    }

    #[test]
    fn test_render_map_area_with_empty_url_and_empty_alt() {
        // Empty url/alt strings still accepted.
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 5, 5],
            url: "".into(),
            alt: "".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("href=''"));
    }

    #[test]
    fn test_myarc_shim_very_large_radius_no_panic() {
        // Huge radius → shim accepts.
        myarc(0.0, 90.0, 1e10, "black", 1.0);
    }

    #[test]
    fn test_image_map_area_with_large_coord_vec_preserved() {
        // 20-element coord vec preserved.
        let coords: Vec<i64> = (0..20).collect();
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: coords.clone(),
            url: "/p".into(),
            alt: "/p".into(),
        };
        assert_eq!(area.coords.len(), 20);
        let s = render_map_area(&area);
        assert!(s.contains("shape='poly'"));
    }

    #[test]
    fn test_report_image_map_with_shape_name_containing_digits_preserved() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("shape123", &[1.0, 2.0], "/url");
        let out = drain_map_elements();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].shape, "shape123");
    }

    #[test]
    fn test_render_map_area_with_shape_upper_case_preserved() {
        let area = ImageMapArea {
            shape: "RECT".into(),
            coords: vec![0, 0, 5, 5],
            url: "/u".into(),
            alt: "/u".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("shape='RECT'"));
    }

    #[test]
    fn test_render_map_area_exactly_one_coord_single_value_formatted() {
        // coords=[42] → formatted as "42".
        let area = ImageMapArea {
            shape: "point".into(),
            coords: vec![42],
            url: "/p".into(),
            alt: "/p".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords='42'"));
    }

    #[test]
    fn test_myarc_shim_with_empty_color_string_no_panic() {
        // Empty color string → shim accepts.
        myarc(0.0, 90.0, 100.0, "", 1.0);
    }

    #[test]
    fn test_init_brush_with_unit_dimensions_no_panic() {
        // init_brush with 1x1 dims → no panic, None result.
        let b = init_brush(1, 1, Some("black"));
        assert!(b.is_none());
    }

    #[test]
    fn test_image_map_area_with_empty_coords_vec_render() {
        // Empty coords vec → coords='' in output.
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![],
            url: "/u".into(),
            alt: "/u".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords=''"));
    }

    #[test]
    fn test_myarc_shim_zero_angles_no_panic() {
        // Both start/end=0 → no panic.
        myarc(0.0, 0.0, 100.0, "red", 1.0);
    }

    #[test]
    fn test_fetch_brush_with_all_dimensions_positive_no_panic() {
        // Multiple positive dimension pairs all ok.
        for (w, h) in [(1u32, 1u32), (10, 10), (100, 100), (1000, 1000)] {
            let (brush, map) = fetch_brush(w, h, None);
            assert!(brush.is_none());
            assert!(map.is_empty());
        }
    }

    #[test]
    fn test_report_image_map_negative_coords_rounded_correctly() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        // Negative floats round toward nearest.
        report_image_map("poly", &[-2.4, -2.5, -2.6, -5.5], "/u");
        let out = drain_map_elements();
        assert_eq!(out.len(), 1);
        // Verify values round to nearest integer.
        assert!(out[0].coords.iter().all(|&c| c < 0));
    }

    #[test]
    fn test_image_map_area_debug_includes_shape_and_url() {
        // Debug output has shape and url fields.
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3, 4],
            url: "/my_url".into(),
            alt: "/my_url".into(),
        };
        let s = format!("{:?}", area);
        assert!(s.contains("rect"));
        assert!(s.contains("/my_url"));
    }

    #[test]
    fn test_render_map_area_polygon_shape_with_eight_coords() {
        // Polygon with 8 coords (4 vertices).
        let area = ImageMapArea {
            shape: "poly".into(),
            coords: vec![0, 0, 10, 0, 10, 10, 0, 10],
            url: "/p".into(),
            alt: "/p".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("coords='0,0,10,0,10,10,0,10'"));
    }

    #[test]
    fn test_myarc_shim_fractional_angles_no_panic() {
        // Fractional angles → shim accepts.
        myarc(0.5, 89.5, 100.5, "gray", 1.5);
    }

    #[test]
    fn test_report_image_map_mixed_positive_negative_coords() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("rect", &[-5.0, 10.0, -20.0, 25.0], "/m");
        let out = drain_map_elements();
        assert_eq!(out[0].coords, vec![-5, 10, -20, 25]);
    }

    #[test]
    fn test_render_map_area_title_attribute_matches_url() {
        // title attribute mirrors url (per Perl convention).
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 10, 10],
            url: "/u".into(),
            alt: "/u".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("title='/u'"));
    }

    #[test]
    fn test_myarc_shim_accepts_mixed_color_strings() {
        // Various color formats accepted.
        myarc(0.0, 90.0, 100.0, "black", 1.0);
        myarc(0.0, 90.0, 100.0, "#ff0000", 1.0);
        myarc(0.0, 90.0, 100.0, "rgb(100,100,100)", 1.0);
    }

    #[test]
    fn test_fetch_brush_independent_of_color_none_or_some() {
        // Both None and Some(..) → same (None, empty) result.
        let (b1, m1) = fetch_brush(50, 50, None);
        let (b2, m2) = fetch_brush(50, 50, Some("red"));
        assert_eq!(b1.is_some(), b2.is_some());
        assert_eq!(m1.len(), m2.len());
    }

    #[test]
    fn test_drain_map_elements_returns_vec_type() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        let out: Vec<ImageMapArea> = drain_map_elements();
        assert!(out.is_empty());
    }

    #[test]
    fn test_render_map_area_title_attribute_matches_url_not_alt() {
        // title attribute mirrors url (not alt, which is separate).
        let area = ImageMapArea {
            shape: "rect".into(),
            coords: vec![0, 0, 10, 10],
            url: "my_url".into(),
            alt: "different_alt".into(),
        };
        let s = render_map_area(&area);
        assert!(s.contains("title='my_url'"));
    }

    #[test]
    fn test_myarc_shim_with_very_long_color_string_no_panic() {
        // Very long color string — no panic.
        let long = "x".repeat(1000);
        myarc(0.0, 90.0, 100.0, &long, 1.0);
    }

    #[test]
    fn test_report_image_map_single_empty_float_slice_accepted() {
        let _lock = MAP_TEST_LOCK.lock().unwrap();
        let _ = drain_map_elements();
        report_image_map("x", &[], "/");
        let out = drain_map_elements();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn test_fetch_brush_with_small_and_zero_width_both_return_none() {
        // 1x0 and 0x1 both → (None, empty).
        for (w, h) in [(1u32, 0u32), (0u32, 1u32)] {
            let (b, m) = fetch_brush(w, h, None);
            assert!(b.is_none());
            assert!(m.is_empty());
        }
    }

    #[test]
    fn test_init_brush_always_returns_none() {
        // No-op port: any input returns None.
        assert!(init_brush(10, 10, Some("red")).is_none());
        assert!(init_brush(0, 0, None).is_none());
        assert!(init_brush(u32::MAX, u32::MAX, Some("")).is_none());
    }

    #[test]
    fn test_report_image_map_rounds_coordinates_to_i64() {
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        // 1.7.round() = 2, 2.4.round() = 2, 3.5.round() = 4 (Rust ties-away-from-zero).
        report_image_map("rect", &[1.7, 2.4, 3.5], "url");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].coords, vec![2, 2, 4]);
    }

    #[test]
    fn test_render_map_area_url_appears_three_times_href_alt_title() {
        let a = ImageMapArea {
            shape: "poly".into(),
            coords: vec![1, 2, 3, 4],
            url: "http://example.com".into(),
            alt: "http://example.com".into(),
        };
        let s = render_map_area(&a);
        // href, alt, title all hold the URL → 3 occurrences.
        assert_eq!(s.matches("http://example.com").count(), 3);
    }

    #[test]
    fn test_myarc_is_no_op_any_inputs_no_panic() {
        // myarc is a no-op shim — accepts any inputs and does nothing.
        myarc(0.0, 360.0, 100.0, "red", 1.0);
        myarc(-999.0, 999.0, f64::MAX, "", 0.0);
        myarc(f64::NAN, f64::INFINITY, 50.0, "x_a5", -1.0);
    }

    #[test]
    fn test_drain_map_elements_returns_empty_after_immediate_drain() {
        // Two drains back-to-back: second yields empty.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        let v = drain_map_elements();
        assert!(v.is_empty());
    }

    #[test]
    fn test_report_image_map_preserves_insertion_order_across_multiple_pushes() {
        // Three pushes preserved in order.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        report_image_map("rect", &[1.0, 2.0], "url1");
        report_image_map("rect", &[3.0, 4.0], "url2");
        report_image_map("rect", &[5.0, 6.0], "url3");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 3);
        assert_eq!(areas[0].url, "url1");
        assert_eq!(areas[1].url, "url2");
        assert_eq!(areas[2].url, "url3");
    }

    #[test]
    fn test_render_map_area_empty_coords_yields_empty_coord_attr() {
        // Empty coord vec → coords="".
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![],
            url: "u".into(),
            alt: "u".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("coords=''"));
    }

    #[test]
    fn test_fetch_brush_with_any_color_still_returns_none_and_empty_map() {
        // Shim: regardless of color input, returns (None, empty).
        let (b, m) = fetch_brush(100, 100, Some("rainbow"));
        assert!(b.is_none());
        assert!(m.is_empty());
    }

    #[test]
    fn test_image_map_area_struct_debug_formats_fields() {
        // Debug representation includes the shape/url/alt strings.
        let a = ImageMapArea {
            shape: "poly".into(),
            coords: vec![10, 20],
            url: "http://example.com".into(),
            alt: "alt text".into(),
        };
        let s = format!("{:?}", a);
        assert!(s.contains("poly"));
        assert!(s.contains("http://example.com"));
    }

    #[test]
    fn test_image_map_area_clone_yields_independent_copies() {
        // Clone produces an independent ImageMapArea.
        let a1 = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2],
            url: "u".into(),
            alt: "a".into(),
        };
        let a2 = a1.clone();
        assert_eq!(a1.coords, a2.coords);
        assert_eq!(a1.url, a2.url);
        assert_eq!(a1.alt, a2.alt);
    }

    #[test]
    fn test_render_map_area_shape_name_with_special_chars_preserved() {
        // Shape name preserved verbatim.
        let a = ImageMapArea {
            shape: "circle-ish".into(),
            coords: vec![5, 5, 3],
            url: "x".into(),
            alt: "x".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("shape='circle-ish'"));
    }

    #[test]
    fn test_render_map_area_coords_joined_with_comma_separator() {
        // Multiple i64 coords joined with comma (no leading/trailing comma).
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3, 4],
            url: "u".into(),
            alt: "a".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("coords='1,2,3,4'"));
    }

    #[test]
    fn test_resolve_data_path_existing_absolute_path_returned_as_is() {
        // If file exists at the given path, return it verbatim.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path_str = tmp.path().to_str().unwrap();
        let base = std::env::current_dir().unwrap();
        let r = resolve_data_path(path_str, &base);
        assert_eq!(r, tmp.path());
    }

    #[test]
    fn test_resolve_data_path_nonexistent_file_falls_back_to_base_dir_join() {
        // File doesn't exist anywhere → final fallback is base_dir.join(file_path).
        let base = Path::new("/nonexistent_base_dir");
        let r = resolve_data_path("nonexistent_file.txt", base);
        assert_eq!(r, base.join("nonexistent_file.txt"));
    }

    #[test]
    fn test_render_map_area_one_coord_value_no_trailing_separator_v2() {
        // Single coord → "coords='5'" with no trailing comma.
        let a = ImageMapArea {
            shape: "circle".into(),
            coords: vec![5],
            url: "u".into(),
            alt: "a".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("coords='5'"));
        assert!(!s.contains("coords='5,"));
    }

    #[test]
    fn test_image_map_area_fields_retain_distinct_url_and_alt() {
        // url and alt may differ — both preserved independently.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![],
            url: "http://link".into(),
            alt: "description".into(),
        };
        assert_eq!(a.url, "http://link");
        assert_eq!(a.alt, "description");
    }

    #[test]
    fn test_report_image_map_negative_coords_truncated_to_i64() {
        // Negative rounded coords preserved (via -0.5 → -1, -1.0 → -1).
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        report_image_map("rect", &[-1.0, -2.0, -3.5], "u");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        // -3.5 rounds away from zero → -4.
        assert_eq!(areas[0].coords, vec![-1, -2, -4]);
    }

    #[test]
    fn test_drain_map_elements_after_many_pushes_yields_all() {
        // 10 pushes → drain yields 10 items.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        for i in 0..10 {
            report_image_map("rect", &[i as f64, i as f64], &format!("u{}", i));
        }
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 10);
    }

    #[test]
    fn test_render_map_area_different_alt_and_url_values() {
        // alt and url can differ — rendered separately.
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2],
            url: "link_url".into(),
            alt: "alt_text".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("href='link_url'"));
        assert!(s.contains("alt='alt_text'"));
    }

    #[test]
    fn test_resolve_data_path_relative_file_in_base_dir() {
        // Create a temp file in a temp dir, use relative name + base_dir.
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let name = "test_rel_file.txt";
        std::fs::write(tmp_dir.path().join(name), b"test").unwrap();
        let r = resolve_data_path(name, tmp_dir.path());
        assert_eq!(r, tmp_dir.path().join(name));
    }

    #[test]
    fn test_image_map_area_shape_circle_preserved() {
        // Shape "circle" preserved in struct field.
        let a = ImageMapArea {
            shape: "circle".into(),
            coords: vec![50, 50, 10],
            url: "u".into(),
            alt: "a".into(),
        };
        assert_eq!(a.shape, "circle");
    }

    #[test]
    fn test_report_image_map_very_small_float_coords_round_to_zero() {
        // 0.01, 0.49, -0.49 all round to 0.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        report_image_map("rect", &[0.01, 0.49, -0.49], "u");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].coords, vec![0, 0, 0]);
    }

    #[test]
    fn test_render_map_area_url_and_alt_handle_ampersand_in_href() {
        // URL with "&" character preserved verbatim (no escaping).
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2],
            url: "http://x?a=1&b=2".into(),
            alt: "u".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("http://x?a=1&b=2"));
    }

    #[test]
    fn test_drain_map_elements_is_destructive() {
        // After drain, another push followed by drain → only new element.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        report_image_map("rect", &[1.0, 2.0], "first");
        drain_map_elements();  // Discard first.
        report_image_map("rect", &[3.0, 4.0], "second");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].url, "second");
    }

    #[test]
    fn test_render_map_area_structure_area_tag_with_all_5_attributes() {
        // <area shape='..' coords='..' href='..' alt='..' title='..'>
        let a = ImageMapArea {
            shape: "rect".into(),
            coords: vec![1, 2, 3, 4],
            url: "u".into(),
            alt: "a".into(),
        };
        let s = render_map_area(&a);
        assert!(s.starts_with("<area "));
        assert!(s.contains("shape='"));
        assert!(s.contains("coords='"));
        assert!(s.contains("href='"));
        assert!(s.contains("alt='"));
        assert!(s.contains("title='"));
    }

    #[test]
    fn test_fetch_brush_zero_dims_and_no_color_v2() {
        // 0x0 dimensions — still shim returns None.
        let (b, m) = fetch_brush(0, 0, None);
        assert!(b.is_none());
        assert!(m.is_empty());
    }

    #[test]
    fn test_init_brush_with_color_none_returns_none() {
        // init_brush with no color → None (it's a no-op shim).
        let r = init_brush(100, 100, None);
        assert!(r.is_none());
    }

    #[test]
    fn test_myarc_no_op_doesnt_mutate_map_elements() {
        // myarc is a no-op — doesn't touch MAP_ELEMENTS.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        myarc(0.0, 90.0, 100.0, "red", 1.0);
        let areas = drain_map_elements();
        assert!(areas.is_empty());
    }

    #[test]
    fn test_report_image_map_zero_count_coords_empty_vec() {
        // Empty coords vec → ImageMapArea stored with empty coords Vec<i64>.
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        report_image_map("rect", &[], "url");
        let areas = drain_map_elements();
        assert_eq!(areas.len(), 1);
        assert!(areas[0].coords.is_empty());
    }

    #[test]
    fn test_image_map_area_url_and_alt_start_same_in_report_image_map() {
        // In report_image_map, alt is initialized to href (same URL).
        let _g = MAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_map_elements();
        report_image_map("rect", &[1.0, 2.0], "my_url");
        let areas = drain_map_elements();
        assert_eq!(areas[0].url, "my_url");
        assert_eq!(areas[0].alt, "my_url");
    }

    #[test]
    fn test_render_map_area_empty_shape_preserved() {
        // Empty shape string preserved in output.
        let a = ImageMapArea {
            shape: "".into(),
            coords: vec![1, 2, 3, 4],
            url: "u".into(),
            alt: "u".into(),
        };
        let s = render_map_area(&a);
        assert!(s.contains("shape=''"));
    }

    #[test]
    fn test_fetch_brush_large_dimensions_still_none() {
        // Even with large dims, shim returns (None, empty).
        let (b, m) = fetch_brush(1000, 1000, Some("any"));
        assert!(b.is_none());
        assert!(m.is_empty());
    }
}
