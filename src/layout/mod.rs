pub mod ideogram;
pub mod spacing;
pub mod units;

use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::intspan::IntSpan;
use crate::karyotype::types::Karyotype;

use ideogram::Ideogram;
use spacing::ideogram_spacing;

/// The computed layout: ideograms positioned around a circle.
#[derive(Debug)]
pub struct Layout {
    pub ideograms: Vec<Ideogram>,
    /// Total circumference in scaled base pairs (ideograms + spacings).
    pub gcircum: f64,
    /// Total unscaled size of all ideograms.
    pub gsize_noscale: f64,
    /// Image radius in pixels.
    pub image_radius: f64,
    /// Angle offset in degrees (default -90 so 0bp is at 12 o'clock).
    pub angle_offset: f64,
    /// Whether angles go counterclockwise.
    pub counterclockwise: bool,
    /// Chromosomes units (e.g. 1000000 for Mb).
    pub chromosomes_units: f64,
    /// Dimensions cache.
    pub dims: Dims,
}

/// Cached dimension values.
#[derive(Debug, Default)]
pub struct Dims {
    pub ideogram_radius: f64,
    pub ideogram_thickness: f64,
    pub ideogram_radius_inner: f64,
    pub ideogram_radius_outer: f64,
}

impl Layout {
    /// Build the layout from configuration and karyotype.
    pub fn build(conf: &HashMap<String, ConfigValue>, karyotype: &Karyotype) -> Result<Self, String> {
        let image_radius = parse_image_radius(conf)?;
        let angle_offset = conf
            .get("image")
            .and_then(|v| v.get("angle_offset"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(-90.0);
        let counterclockwise = conf
            .get("image")
            .and_then(|v| v.get("angle_orientation"))
            .and_then(|v| v.as_str())
            .map(|s| s == "counterclockwise")
            .unwrap_or(false);
        let chromosomes_units = conf
            .get("chromosomes_units")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let units_ok = conf
            .get("units_ok")
            .and_then(|v| v.as_str())
            .unwrap_or("bupr");
        let units_nounit = conf
            .get("units_nounit")
            .and_then(|v| v.as_str())
            .unwrap_or("n");

        // Compute ideogram dimensions
        let ideogram_conf = conf.get("ideogram").and_then(|v| v.as_map());
        let dims = compute_dims(ideogram_conf, image_radius, units_ok, units_nounit)?;

        // Parse which chromosomes to display
        let display_default = conf
            .get("chromosomes_display_default")
            .and_then(|v| v.as_str())
            .map(|s| s == "1")
            .unwrap_or(true);

        let chromosomes_filter = conf
            .get("chromosomes")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Build ideograms from karyotype
        let mut ideograms = create_ideograms(karyotype, display_default, chromosomes_filter)?;

        // Apply scale
        if let Some(scale_str) = conf.get("chromosomes_scale").and_then(|v| v.as_str()) {
            apply_scale(&mut ideograms, scale_str);
        }

        // Apply reverse direction
        if let Some(reverse_str) = conf.get("chromosomes_reverse").and_then(|v| v.as_str()) {
            apply_reverse(&mut ideograms, reverse_str);
        }

        // Apply custom radii
        if let Some(radius_str) = conf.get("chromosomes_radius").and_then(|v| v.as_str()) {
            apply_custom_radius(&mut ideograms, radius_str, image_radius, &dims);
        }

        // Apply ordering
        if let Some(order_str) = conf.get("chromosomes_order").and_then(|v| v.as_str()) {
            apply_ordering(&mut ideograms, order_str);
        } else {
            // Default: use karyotype file order
            ideograms.sort_by_key(|ideo| ideo.display_idx);
        }

        // Set display_idx to match final sorted order
        for (i, ideo) in ideograms.iter_mut().enumerate() {
            ideo.display_idx = i;
        }

        // Link prev/next and set axis breaks
        link_ideograms(&mut ideograms);

        // Compute zoom covers (default: single cover per ideogram at scale 1.0)
        for ideo in &mut ideograms {
            if ideo.covers.is_empty() {
                ideo.covers.push(ideogram::Cover {
                    set: ideo.set.clone(),
                    scale: ideo.scale,
                });
            }
        }

        // Compute lengths
        let mut gsize_noscale: f64 = 0.0;
        let mut gsize_scaled: f64 = 0.0;
        for ideo in &mut ideograms {
            ideo.length_cumulative_scaled = gsize_scaled;
            ideo.length_cumulative_noscale = gsize_noscale;

            let mut scaled_len = 0.0;
            let mut noscale_len = 0.0;
            for cover in &ideo.covers {
                let card = cover.set.cardinality() as f64;
                scaled_len += card * cover.scale;
                noscale_len += card;
            }
            ideo.length_scaled = scaled_len;
            ideo.length_noscale = noscale_len;
            gsize_scaled += scaled_len;
            gsize_noscale += noscale_len;
        }

        // Compute GCIRCUM = total scaled size + all spacings
        let spacing_conf = ideogram_conf.and_then(|m| m.get("spacing")).and_then(|v| v.as_map());
        let default_spacing = compute_default_spacing(
            spacing_conf,
            chromosomes_units,
            gsize_noscale,
            units_ok,
            units_nounit,
        )?;

        let mut gcircum = gsize_scaled;
        let n = ideograms.len();
        for i in 0..n {
            let next_i = (i + 1) % n;
            let sp = ideogram_spacing(
                &ideograms[i],
                &ideograms[next_i],
                spacing_conf,
                default_spacing,
                chromosomes_units,
                gsize_noscale,
                units_ok,
                units_nounit,
            );
            gcircum += sp;
        }

        Ok(Layout {
            ideograms,
            gcircum,
            gsize_noscale,
            image_radius,
            angle_offset,
            counterclockwise,
            chromosomes_units,
            dims,
        })
    }

    /// Get the angle in degrees for a genomic position on a chromosome.
    pub fn get_angle(&self, pos: i64, chr: &str) -> Option<f64> {
        let relpos = self.get_relpos_scaled(pos, chr)?;
        let mut angle = if self.counterclockwise {
            360.0 * (1.0 - relpos / self.gcircum)
        } else {
            360.0 * relpos / self.gcircum
        };
        angle += self.angle_offset;
        if angle > 360.0 {
            angle -= 360.0 * (angle / 360.0).floor();
        }
        if angle < 0.0 {
            angle += 360.0;
        }
        Some(angle)
    }

    /// Convert angle (degrees) and radius (pixels) to (x, y) pixel coordinates.
    pub fn get_xy(&self, angle_deg: f64, radius: f64) -> (f64, f64) {
        let rad = angle_deg * std::f64::consts::PI / 180.0;
        (
            self.image_radius + radius * rad.cos(),
            self.image_radius + radius * rad.sin(),
        )
    }

    /// Get the scaled relative position for a genomic position.
    fn get_relpos_scaled(&self, pos: i64, chr: &str) -> Option<f64> {
        let ideo = self.find_ideogram(pos, chr)?;
        let mut relpos = self.get_relpos_scaled_ideogram_start(ideo.display_idx);

        let direction: f64 = if ideo.reverse { -1.0 } else { 1.0 };

        for cover in &ideo.covers {
            if cover.set.member(pos) {
                let offset = (pos - cover.set.min().unwrap()) as f64;
                relpos += direction * offset * cover.scale;
                return Some(relpos);
            } else {
                relpos += direction * cover.set.cardinality() as f64 * cover.scale;
            }
        }
        None
    }

    /// Get the cumulative scaled position at the start of an ideogram.
    fn get_relpos_scaled_ideogram_start(&self, display_idx: usize) -> f64 {
        let _spacing_conf: Option<&HashMap<String, ConfigValue>> = None;
        let mut pos = 0.0;

        for (i, ideo) in self.ideograms.iter().enumerate() {
            if i == display_idx {
                if ideo.reverse {
                    pos += ideo.length_scaled;
                }
                break;
            }
            pos += ideo.length_scaled;

            // Add spacing to next ideogram
            if i + 1 < self.ideograms.len() {
                pos += self.spacing_between(i, i + 1);
            }
        }
        pos
    }

    /// Get spacing between two adjacent ideograms by display index.
    fn spacing_between(&self, _idx_a: usize, _idx_b: usize) -> f64 {
        // For now use a simple equal spacing model
        // Total spacing = gcircum - sum of all scaled lengths
        let total_scaled: f64 = self.ideograms.iter().map(|i| i.length_scaled).sum();
        let total_spacing = self.gcircum - total_scaled;
        let n = self.ideograms.len() as f64;
        if n > 0.0 {
            total_spacing / n
        } else {
            0.0
        }
    }

    /// Find the ideogram that contains a given genomic position on a chromosome.
    fn find_ideogram(&self, pos: i64, chr: &str) -> Option<&Ideogram> {
        self.ideograms
            .iter()
            .find(|ideo| ideo.chr == chr && ideo.set.member(pos))
    }

    /// Find any ideogram for a given chromosome name.
    pub fn find_ideogram_by_chr(&self, chr: &str) -> Option<&Ideogram> {
        self.ideograms.iter().find(|ideo| ideo.chr == chr)
    }
}

/// Parse image radius from config (handles "1500p" format).
fn parse_image_radius(conf: &HashMap<String, ConfigValue>) -> Result<f64, String> {
    let radius_str = conf
        .get("image")
        .and_then(|v| v.get("radius"))
        .and_then(|v| v.as_str())
        .ok_or("missing image.radius in config")?;

    // Strip 'p' suffix if present
    let num_str = radius_str.trim_end_matches('p');
    num_str
        .parse::<f64>()
        .map_err(|_| format!("cannot parse image radius '{}'", radius_str))
}

/// Compute ideogram dimensions from config.
fn compute_dims(
    ideogram_conf: Option<&HashMap<String, ConfigValue>>,
    image_radius: f64,
    units_ok: &str,
    units_nounit: &str,
) -> Result<Dims, String> {
    let mut dims = Dims::default();

    let (radius_val, radius_unit) = if let Some(r_str) = ideogram_conf
        .and_then(|m| m.get("radius"))
        .and_then(|v| v.as_str())
    {
        units::unit_split(r_str, units_ok, units_nounit)
            .map_err(|e| format!("ideogram radius: {}", e))?
    } else {
        (0.85, "r".to_string())
    };

    dims.ideogram_radius = if radius_unit == "r" {
        radius_val * image_radius
    } else {
        radius_val
    };

    let (thick_val, thick_unit) = if let Some(t_str) = ideogram_conf
        .and_then(|m| m.get("thickness"))
        .and_then(|v| v.as_str())
    {
        units::unit_split(t_str, units_ok, units_nounit)
            .map_err(|e| format!("ideogram thickness: {}", e))?
    } else {
        (100.0, "p".to_string())
    };

    dims.ideogram_thickness = if thick_unit == "r" {
        thick_val * image_radius
    } else {
        thick_val
    };

    dims.ideogram_radius_outer = dims.ideogram_radius;
    dims.ideogram_radius_inner = dims.ideogram_radius - dims.ideogram_thickness;

    Ok(dims)
}

/// Create ideograms from karyotype based on display filters.
fn create_ideograms(
    karyotype: &Karyotype,
    display_default: bool,
    chromosomes_filter: &str,
) -> Result<Vec<Ideogram>, String> {
    let mut ideograms = Vec::new();

    // Parse explicit chromosome filter
    let mut explicit_include: HashMap<String, Option<IntSpan>> = HashMap::new();
    let mut explicit_exclude: Vec<String> = Vec::new();

    if !chromosomes_filter.is_empty() {
        for item in chromosomes_filter.split(';') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (name, exclude) = if item.starts_with('-') {
                (&item[1..], true)
            } else {
                (item, false)
            };

            // Parse optional region: chr:start-end
            let (chr_name, region) = if let Some((n, r)) = name.split_once(':') {
                let parts: Vec<&str> = r.split('-').collect();
                if parts.len() == 2 {
                    let start: i64 = parts[0].parse().unwrap_or(0);
                    let end: i64 = parts[1].parse().unwrap_or(0);
                    (n, Some(IntSpan::from_range(start, end)))
                } else {
                    (name, None)
                }
            } else {
                (name, None)
            };

            if exclude {
                explicit_exclude.push(chr_name.to_string());
            } else {
                explicit_include.insert(chr_name.to_string(), region);
            }
        }
    }

    let mut idx = 0;
    for chr_name in &karyotype.order {
        let chr = &karyotype.chromosomes[chr_name];

        // Determine if this chromosome should be displayed
        let display = if explicit_include.contains_key(chr_name) {
            true
        } else if explicit_exclude.contains(chr_name) {
            false
        } else {
            display_default
        };

        if !display {
            continue;
        }

        // Determine the region to display
        let set = if let Some(Some(region)) = explicit_include.get(chr_name) {
            region.intersect(&chr.set)
        } else {
            chr.set.clone()
        };

        if set.cardinality() < 2 {
            continue;
        }

        let ideo = Ideogram {
            chr: chr.name.clone(),
            label: chr.label.clone(),
            tag: chr.name.clone(),
            chrlength: chr.end - chr.start + 1,
            set,
            scale: 1.0,
            reverse: false,
            idx,
            display_idx: idx,
            covers: Vec::new(),
            length_scaled: 0.0,
            length_noscale: 0.0,
            length_cumulative_scaled: 0.0,
            length_cumulative_noscale: 0.0,
            radius: 0.0,
            radius_inner: 0.0,
            radius_outer: 0.0,
            thickness: 0.0,
            has_break_start: false,
            has_break_end: false,
            color: chr.color.clone(),
        };
        ideograms.push(ideo);
        idx += 1;
    }

    Ok(ideograms)
}

/// Apply scale factors from "tag:scale;tag:scale" format.
fn apply_scale(ideograms: &mut [Ideogram], scale_str: &str) {
    for pair in scale_str.split(';') {
        let pair = pair.trim();
        if let Some((tag, scale_s)) = pair.split_once(':') {
            if let Ok(scale) = scale_s.trim().parse::<f64>() {
                for ideo in ideograms.iter_mut() {
                    if ideo.tag == tag.trim() {
                        ideo.scale = scale;
                    }
                }
            }
        }
    }
}

/// Apply reverse direction from "tag;tag" format.
fn apply_reverse(ideograms: &mut [Ideogram], reverse_str: &str) {
    for tag in reverse_str.split(';') {
        let tag = tag.trim();
        for ideo in ideograms.iter_mut() {
            if ideo.tag == tag {
                ideo.reverse = true;
            }
        }
    }
}

/// Apply custom radii from "tag:radius;tag:radius" format.
fn apply_custom_radius(
    ideograms: &mut [Ideogram],
    radius_str: &str,
    image_radius: f64,
    dims: &Dims,
) {
    for pair in radius_str.split(';') {
        let pair = pair.trim();
        if let Some((tag, r_str)) = pair.split_once(':') {
            let r_str = r_str.trim();
            let radius = if r_str.ends_with('r') {
                r_str.trim_end_matches('r').parse::<f64>().unwrap_or(0.0) * image_radius
            } else if r_str.ends_with('p') {
                r_str.trim_end_matches('p').parse::<f64>().unwrap_or(0.0)
            } else {
                r_str.parse::<f64>().unwrap_or(0.0)
            };
            for ideo in ideograms.iter_mut() {
                if ideo.tag == tag.trim() {
                    ideo.radius = radius;
                    ideo.radius_outer = radius;
                    ideo.radius_inner = radius - dims.ideogram_thickness;
                    ideo.thickness = dims.ideogram_thickness;
                }
            }
        }
    }
}

/// Apply ordering from chromosomes_order string.
fn apply_ordering(ideograms: &mut [Ideogram], order_str: &str) {
    let tags: Vec<&str> = order_str.split(';').map(|s| s.trim()).collect();
    for (i, tag) in tags.iter().enumerate() {
        if tag.is_empty() || *tag == "-" || *tag == "|" {
            continue;
        }
        for ideo in ideograms.iter_mut() {
            if ideo.tag == *tag || ideo.chr == *tag {
                ideo.display_idx = i;
            }
        }
    }
    ideograms.sort_by_key(|ideo| ideo.display_idx);
}

/// Link ideograms: set prev/next relationships and axis breaks.
fn link_ideograms(ideograms: &mut [Ideogram]) {
    let n = ideograms.len();
    if n == 0 {
        return;
    }

    // Set axis breaks: different chromosome neighbors get break markers
    for i in 0..n {
        let next_i = (i + 1) % n;
        let prev_i = if i == 0 { n - 1 } else { i - 1 };

        let this_chr = ideograms[i].chr.clone();
        let next_chr = ideograms[next_i].chr.clone();
        let prev_chr = ideograms[prev_i].chr.clone();

        // Break at end if next ideogram is different chromosome or if this is the last
        // contiguous segment of the same chromosome
        if this_chr != next_chr
            || ideograms[i].set.max() != Some(ideograms[next_i].set.min().unwrap_or(0) - 1)
        {
            ideograms[i].has_break_end = true;
        }
        if this_chr != prev_chr
            || ideograms[i].set.min() != Some(ideograms[prev_i].set.max().unwrap_or(0) + 1)
        {
            ideograms[i].has_break_start = true;
        }
    }
}

/// Compute the default spacing between ideograms.
fn compute_default_spacing(
    spacing_conf: Option<&HashMap<String, ConfigValue>>,
    chromosomes_units: f64,
    gsize_noscale: f64,
    units_ok: &str,
    units_nounit: &str,
) -> Result<f64, String> {
    let spacing_str = spacing_conf
        .and_then(|m| m.get("default"))
        .and_then(|v| v.as_str())
        .unwrap_or("0.005r");

    let (val, unit) = units::unit_split(spacing_str, units_ok, units_nounit)
        .map_err(|e| format!("spacing default: {}", e))?;

    let spacing = match unit.as_str() {
        "u" => val * chromosomes_units,
        "b" => val,
        "r" => val * gsize_noscale,
        "n" => val,
        _ => val,
    };

    Ok(spacing)
}
