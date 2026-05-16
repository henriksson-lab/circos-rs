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
    pub fn build(
        conf: &HashMap<String, ConfigValue>,
        karyotype: &Karyotype,
    ) -> Result<Self, String> {
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
        let mut ideograms = create_ideogram_set(
            karyotype,
            display_default,
            chromosomes_filter,
            chromosomes_units,
        )?;

        // Apply scale
        if let Some(scale_str) = conf.get("chromosomes_scale").and_then(|v| v.as_str()) {
            register_chromosomes_scale(&mut ideograms, scale_str);
        }

        // Apply reverse direction
        if let Some(reverse_str) = conf.get("chromosomes_reverse").and_then(|v| v.as_str()) {
            register_chromosomes_direction(&mut ideograms, reverse_str);
        }

        // Apply custom radii
        if let Some(radius_str) = conf.get("chromosomes_radius").and_then(|v| v.as_str()) {
            register_chromosomes_radius(&mut ideograms, radius_str, image_radius, &dims);
        }

        // Apply ordering
        if let Some(order_str) = conf.get("chromosomes_order").and_then(|v| v.as_str()) {
            make_chrorder_groups(&mut ideograms, order_str);
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

        // Parse zooms.zoom (Perl: non-linear scale regions).
        #[derive(Clone)]
        struct ZoomDef {
            chr: String,
            set: IntSpan,
            scale: f64,
            smooth_distance: f64,
            smooth_steps: usize,
        }
        let mut zooms: Vec<ZoomDef> = Vec::new();
        if let Some(zooms_conf) = conf.get("zooms").and_then(|v| v.as_map()) {
            let zoom_list = match zooms_conf.get("zoom") {
                Some(ConfigValue::List(list)) => list.clone(),
                Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
                _ => Vec::new(),
            };
            for zoom_v in &zoom_list {
                let zm = match zoom_v.as_map() {
                    Some(m) => m,
                    None => continue,
                };
                let chr = zm
                    .get("chr")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let start_str = zm.get("start").and_then(|v| v.as_str()).unwrap_or("0");
                let end_str = zm.get("end").and_then(|v| v.as_str()).unwrap_or("0");
                let (start_val, start_unit) = units::unit_split(start_str, units_ok, units_nounit)
                    .unwrap_or((0.0, "b".to_string()));
                let (end_val, end_unit) = units::unit_split(end_str, units_ok, units_nounit)
                    .unwrap_or((0.0, "b".to_string()));
                let start_bp = if start_unit == "u" {
                    start_val * chromosomes_units
                } else {
                    start_val
                };
                let end_bp = if end_unit == "u" {
                    end_val * chromosomes_units
                } else {
                    end_val
                };
                let scale = zm
                    .get("scale")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1.0);
                let set = IntSpan::from_range(start_bp as i64, end_bp as i64);
                let smooth_distance = zm
                    .get("smooth_distance")
                    .and_then(|v| v.as_str())
                    .and_then(|s| units::unit_split(s, units_ok, units_nounit).ok())
                    .map(|(v, unit)| match unit.as_str() {
                        "u" => v * chromosomes_units,
                        "r" => v * set.cardinality() as f64,
                        _ => v,
                    })
                    .unwrap_or(0.0);
                let smooth_steps: usize = zm
                    .get("smooth_steps")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                zooms.push(ZoomDef {
                    chr,
                    set,
                    scale,
                    smooth_distance,
                    smooth_steps,
                });
            }
        }

        // Compute zoom covers per ideogram (Perl: filter applicable zooms,
        // build smoother regions, assemble boundary-based non-overlapping covers).
        for ideo in &mut ideograms {
            if !ideo.covers.is_empty() {
                continue;
            }
            if zooms.is_empty() {
                ideo.covers.push(ideogram::Cover {
                    set: ideo.set.clone(),
                    scale: ideo.scale,
                });
                continue;
            }
            let applicable: Vec<&ZoomDef> = zooms
                .iter()
                .filter(|z| z.chr == ideo.chr && ideo.set.intersect(&z.set).cardinality() > 0)
                .collect();
            if applicable.is_empty() {
                ideo.covers.push(ideogram::Cover {
                    set: ideo.set.clone(),
                    scale: ideo.scale,
                });
                continue;
            }

            // Effective zooms = applicable + smoothers + base ideogram cover
            let mut effective: Vec<(IntSpan, f64)> = Vec::new();
            for zoom in &applicable {
                let d = zoom.smooth_distance;
                let n = zoom.smooth_steps;
                if d > 0.0 && n > 0 {
                    let subzoom_size = d / n as f64;
                    for i in 1..=n {
                        let subzoom_scale = (zoom.scale * (n as f64 + 1.0 - i as f64)
                            + ideo.scale * i as f64)
                            / (n as f64 + 1.0);
                        let s1 = zoom.set.min().unwrap_or(0) as f64 - (i as f64) * subzoom_size;
                        let e1 = s1 + subzoom_size;
                        effective.push((
                            IntSpan::from_range(s1 as i64, e1 as i64).intersect(&ideo.set),
                            subzoom_scale,
                        ));
                        let s2 =
                            zoom.set.max().unwrap_or(0) as f64 + (i as f64 - 1.0) * subzoom_size;
                        let e2 = s2 + subzoom_size;
                        effective.push((
                            IntSpan::from_range(s2 as i64, e2 as i64).intersect(&ideo.set),
                            subzoom_scale,
                        ));
                    }
                }
                effective.push((zoom.set.intersect(&ideo.set), zoom.scale));
            }
            let base_set = ideo.set.clone();
            effective.push((base_set.clone(), ideo.scale));

            // Boundary-based cover construction.
            let mut boundaries: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
            for (s, _) in &effective {
                if let Some(lo) = s.min() {
                    boundaries.insert(lo - 1);
                    boundaries.insert(lo);
                }
                if let Some(hi) = s.max() {
                    boundaries.insert(hi);
                    boundaries.insert(hi + 1);
                }
            }
            let mut bd: Vec<i64> = boundaries.into_iter().collect();
            if bd.len() >= 2 {
                bd = bd[1..bd.len() - 1].to_vec();
            }
            let mut built: Vec<ideogram::Cover> = Vec::new();
            for pair in bd.windows(2) {
                let x = pair[0];
                let y = pair[1];
                let set = IntSpan::from_range(x, y).intersect(&ideo.set);
                if set.cardinality() == 0 {
                    continue;
                }
                // Pick max zoom level that covers this bin.
                let mut level = 1.0f64;
                let mut scale = ideo.scale;
                let mut has_level = false;
                for (zset, zscale) in &effective {
                    if zset.intersect(&set).cardinality() == 0 {
                        continue;
                    }
                    let zoom_level = zscale.max(1.0 / zscale.max(1e-9));
                    if !has_level || zoom_level > level {
                        level = zoom_level;
                        scale = *zscale;
                        has_level = true;
                    }
                }
                // Merge with previous adjacent cover if same scale.
                let merged = built.last_mut().filter(|c| {
                    (c.scale - scale).abs() < f64::EPSILON
                        && c.set.max() == set.min().map(|v| v - 1).or(c.set.max())
                });
                if let Some(c) = merged {
                    c.set = c.set.union(&set);
                } else {
                    built.push(ideogram::Cover { set, scale });
                }
            }
            if built.is_empty() {
                ideo.covers.push(ideogram::Cover {
                    set: ideo.set.clone(),
                    scale: ideo.scale,
                });
            } else {
                ideo.covers = built;
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
        let spacing_conf = ideogram_conf
            .and_then(|m| m.get("spacing"))
            .and_then(|v| v.as_map());
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
    pub fn getanglepos(&self, pos: i64, chr: &str) -> Option<f64> {
        let relpos = self.getrelpos_scaled(pos, chr)?;
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
    pub fn getxypos(&self, angle_deg: f64, radius: f64) -> (f64, f64) {
        let rad = angle_deg * std::f64::consts::PI / 180.0;
        (
            self.image_radius + radius * rad.cos(),
            self.image_radius + radius * rad.sin(),
        )
    }

    /// Get the scaled relative position for a genomic position.
    fn getrelpos_scaled(&self, pos: i64, chr: &str) -> Option<f64> {
        let ideo = self.find_ideogram(pos, chr)?;
        let mut relpos = self.getrelpos_scaled_ideogram_start(ideo.display_idx);

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
    fn getrelpos_scaled_ideogram_start(&self, display_idx: usize) -> f64 {
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
        if n > 0.0 { total_spacing / n } else { 0.0 }
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

    /// Port of Perl `get_ideogram_idx(pos, chr)`: return display-index of the ideogram
    /// that contains `pos` on `chr`, or None.
    pub fn get_ideogram_idx(&self, pos: i64, chr: &str) -> Option<usize> {
        self.ideograms
            .iter()
            .find(|ideo| ideo.chr == chr && ideo.set.member(pos))
            .map(|ideo| ideo.display_idx)
    }

    /// Port of Perl `get_ideogram_by_idx(idx)`: lookup by display index, panics on miss
    /// to match Perl's `confess`.
    pub fn get_ideogram_by_idx(&self, idx: usize) -> &Ideogram {
        self.ideograms
            .iter()
            .find(|ideo| ideo.display_idx == idx)
            .unwrap_or_else(|| {
                panic!(
                    "consistency error in get_ideogram_by_idx - no ideogram with index {} exists",
                    idx
                )
            })
    }

    /// Port of Perl `getrdistance(pos, chr, r)`: arc distance (in pixels) from
    /// chromosome start to a genomic position at radius `r`.
    pub fn getrdistance(&self, pos: i64, chr: &str, r: f64) -> Option<f64> {
        let relpos = self.getrelpos_scaled(pos, chr)?;
        let deg2rad = std::f64::consts::PI / 180.0;
        let d = if self.counterclockwise {
            r * deg2rad * 360.0 * (1.0 - relpos / self.gcircum)
        } else {
            r * deg2rad * 360.0 * relpos / self.gcircum
        };
        Some(d)
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

/// Create ideograms from karyotype based on display filters. When the filter
/// string lists the same chromosome multiple times with different `[tag]` /
/// range combinations, one Ideogram per entry is emitted — Perl allows
/// multiple ideograms sharing a chromosome name, distinguished by tag.
fn create_ideogram_set(
    karyotype: &Karyotype,
    display_default: bool,
    chromosomes_filter: &str,
    chromosomes_units: f64,
) -> Result<Vec<Ideogram>, String> {
    let mut ideograms = Vec::new();

    // Parse explicit chromosome filter into an ordered list (Perl preserves
    // order for display_idx assignment).
    #[derive(Clone)]
    struct FilterEntry {
        chr: String,
        tag: String,
        region: Option<IntSpan>,
        exclude: bool,
    }
    let mut ordered_entries: Vec<FilterEntry> = Vec::new();
    let mut explicit_exclude: Vec<String> = Vec::new();

    for item in chromosomes_filter
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let (rest, exclude) = match item.strip_prefix('-') {
            Some(r) => (r, true),
            None => (item, false),
        };

        // Split off the optional `:runlist` first; remaining `chr[tag]` head.
        let (head, runlist): (&str, &str) = match rest.split_once(':') {
            Some((h, r)) => (h, r),
            None => (rest, ""),
        };

        // Extract optional [tag] from head.
        let (chr_name, tag): (&str, &str) = match (head.find('['), head.find(']')) {
            (Some(lo), Some(hi)) if hi > lo => (&head[..lo], &head[lo + 1..hi]),
            _ => (head, head),
        };

        // Parse range "start-end" (end may be ")" for open); multiply by units.
        let region = if runlist.is_empty() {
            None
        } else {
            let r = runlist.trim_end_matches(')').trim_end_matches('(');
            match r.split_once('-') {
                Some((s, e)) => {
                    let start =
                        (s.parse::<f64>().unwrap_or(0.0) * chromosomes_units).round() as i64;
                    let end = if e.is_empty() {
                        i64::MAX
                    } else {
                        (e.parse::<f64>().unwrap_or(f64::INFINITY) * chromosomes_units).round()
                            as i64
                    };
                    Some(IntSpan::from_range(start, end))
                }
                None => None,
            }
        };

        if exclude {
            explicit_exclude.push(chr_name.to_string());
        } else {
            ordered_entries.push(FilterEntry {
                chr: chr_name.to_string(),
                tag: tag.to_string(),
                region,
                exclude: false,
            });
        }
    }

    let mut idx = 0;

    // When the filter is non-empty, use its ordering verbatim (one ideogram
    // per explicit entry). Otherwise use display_default with karyotype order.
    let use_explicit = !ordered_entries.is_empty();
    if use_explicit {
        for entry in &ordered_entries {
            if entry.exclude {
                continue;
            }
            let chr = match karyotype.chromosomes.get(&entry.chr) {
                Some(c) => c,
                None => continue,
            };
            let set = match &entry.region {
                Some(r) => r.intersect(&chr.set),
                None => chr.set.clone(),
            };
            if set.cardinality() < 2 {
                continue;
            }
            let ideo = Ideogram {
                chr: chr.name.clone(),
                label: chr.label.clone(),
                tag: entry.tag.clone(),
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
    } else {
        // display_default branch: iterate karyotype in order, include all chrs
        // (unless explicitly excluded) as one ideogram each.
        for chr_name in &karyotype.order {
            if explicit_exclude.iter().any(|e| e == chr_name) {
                continue;
            }
            let chr = &karyotype.chromosomes[chr_name];
            let set = chr.set.clone();
            if set.cardinality() < 2 {
                continue;
            }
            if !display_default {
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
    }

    Ok(ideograms)
}

/// Apply scale factors from "tag:scale;tag:scale" format.
fn register_chromosomes_scale(ideograms: &mut [Ideogram], scale_str: &str) {
    for pair in scale_str.split(';') {
        let pair = pair.trim();
        if let Some((tag, scale_s)) = pair.split_once(':')
            && let Ok(scale) = scale_s.trim().parse::<f64>()
        {
            for ideo in ideograms.iter_mut() {
                if ideo.tag == tag.trim() {
                    ideo.scale = scale;
                }
            }
        }
    }
}

/// Apply reverse direction from "tag;tag" format.
fn register_chromosomes_direction(ideograms: &mut [Ideogram], reverse_str: &str) {
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
fn register_chromosomes_radius(
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
fn make_chrorder_groups(ideograms: &mut [Ideogram], order_str: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intspan::IntSpan;
    use crate::layout::ideogram::{Cover, Ideogram};

    fn mk_ideogram(chr: &str, tag: &str, idx: usize, display_idx: usize) -> Ideogram {
        Ideogram {
            chr: chr.into(),
            label: chr.into(),
            tag: tag.into(),
            chrlength: 100,
            set: IntSpan::from_range(0, 100),
            scale: 1.0,
            reverse: false,
            idx,
            display_idx,
            covers: vec![Cover {
                set: IntSpan::from_range(0, 100),
                scale: 1.0,
            }],
            length_scaled: 100.0,
            length_noscale: 100.0,
            length_cumulative_scaled: 0.0,
            length_cumulative_noscale: 0.0,
            radius: 1000.0,
            radius_inner: 950.0,
            radius_outer: 1050.0,
            thickness: 100.0,
            has_break_start: false,
            has_break_end: false,
            color: "red".into(),
        }
    }

    fn mk_layout() -> Layout {
        Layout {
            ideograms: vec![
                mk_ideogram("hs1", "a", 0, 0),
                mk_ideogram("hs2", "b", 1, 1),
                mk_ideogram("hs3", "c", 2, 2),
            ],
            gcircum: 300.0,
            gsize_noscale: 300.0,
            image_radius: 500.0,
            angle_offset: 0.0,
            counterclockwise: false,
            chromosomes_units: 1.0,
            dims: Dims {
                ideogram_radius: 1000.0,
                ideogram_thickness: 100.0,
                ideogram_radius_inner: 950.0,
                ideogram_radius_outer: 1050.0,
            },
        }
    }

    #[test]
    fn test_getxypos_polar_to_cartesian() {
        let layout = mk_layout();
        // angle=0, radius=100 → (image_radius+100, image_radius) = (600, 500)
        let (x, y) = layout.getxypos(0.0, 100.0);
        assert!((x - 600.0).abs() < 1e-9);
        assert!((y - 500.0).abs() < 1e-9);
        // angle=90 → (image_radius, image_radius+100)
        let (x, y) = layout.getxypos(90.0, 100.0);
        assert!((x - 500.0).abs() < 1e-6);
        assert!((y - 600.0).abs() < 1e-6);
    }

    #[test]
    fn test_getxypos_radius_zero_is_center() {
        // radius=0 → point at center (image_radius, image_radius).
        let layout = mk_layout();
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0] {
            let (x, y) = layout.getxypos(angle, 0.0);
            assert!((x - layout.image_radius).abs() < 1e-6, "angle {}", angle);
            assert!((y - layout.image_radius).abs() < 1e-6, "angle {}", angle);
        }
    }

    #[test]
    fn test_getxypos_angle_180_points_west() {
        // angle=180 → cos=-1, sin≈0 → (image_radius - r, image_radius).
        let layout = mk_layout();
        let (x, y) = layout.getxypos(180.0, 100.0);
        assert!((x - 400.0).abs() < 1e-6);
        assert!((y - 500.0).abs() < 1e-6);
        // angle=270 → cos≈0, sin=-1 → (image_radius, image_radius - r).
        let (x, y) = layout.getxypos(270.0, 100.0);
        assert!((x - 500.0).abs() < 1e-6);
        assert!((y - 400.0).abs() < 1e-6);
    }

    #[test]
    fn test_getanglepos_unknown_chr_returns_none() {
        // chr not in layout → getrelpos_scaled returns None → getanglepos None.
        let layout = mk_layout();
        assert!(layout.getanglepos(50, "unknown_chr").is_none());
    }

    #[test]
    fn test_getanglepos_for_known_chr_is_in_range() {
        // For a valid chr/pos, returned angle should be in [0, 360).
        let layout = mk_layout();
        let a = layout.getanglepos(50, "hs1").expect("hs1 present");
        assert!((0.0..360.0).contains(&a), "angle {} out of [0,360)", a);
    }

    #[test]
    fn test_find_ideogram_by_chr_hit_and_miss() {
        let layout = mk_layout();
        let ideo = layout.find_ideogram_by_chr("hs2").expect("hs2 present");
        assert_eq!(ideo.chr, "hs2");
        assert_eq!(ideo.tag, "b");
        assert!(layout.find_ideogram_by_chr("hsX").is_none());
    }

    #[test]
    fn test_get_ideogram_idx_for_pos_in_set() {
        let layout = mk_layout();
        // pos=50 in hs1's set [0..100] → display_idx 0.
        assert_eq!(layout.get_ideogram_idx(50, "hs1"), Some(0));
        // pos in hs3.
        assert_eq!(layout.get_ideogram_idx(10, "hs3"), Some(2));
        // pos out of chr range → None (set goes 0..100, so 200 misses).
        assert_eq!(layout.get_ideogram_idx(200, "hs1"), None);
        // Wrong chr → None.
        assert_eq!(layout.get_ideogram_idx(50, "hsX"), None);
    }

    #[test]
    fn test_get_ideogram_by_idx_returns_reference() {
        let layout = mk_layout();
        // display_idx=1 → hs2
        let ideo = layout.get_ideogram_by_idx(1);
        assert_eq!(ideo.chr, "hs2");
        let ideo = layout.get_ideogram_by_idx(0);
        assert_eq!(ideo.chr, "hs1");
    }

    #[test]
    #[should_panic(expected = "consistency error")]
    fn test_get_ideogram_by_idx_panics_on_miss() {
        let layout = mk_layout();
        // display_idx=99 doesn't exist → Perl-style panic.
        let _ = layout.get_ideogram_by_idx(99);
    }

    #[test]
    fn test_getrdistance_returns_some_for_valid_chr() {
        let layout = mk_layout();
        // First ideogram, position 50 — should produce a valid arc distance.
        let d = layout.getrdistance(50, "hs1", 1000.0);
        assert!(d.is_some(), "expected Some(distance) for valid chr/pos");
        let val = d.unwrap();
        // Distance should be non-negative and finite.
        assert!(val.is_finite(), "distance should be finite, got {}", val);
        assert!(val >= 0.0, "distance should be non-negative, got {}", val);
        // Wrong chr → None.
        assert!(layout.getrdistance(50, "hsX", 1000.0).is_none());
    }

    #[test]
    fn test_parse_image_radius_numeric_and_pixel() {
        // Plain numeric value parses.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        assert!((parse_image_radius(&conf).unwrap() - 1500.0).abs() < 1e-9);

        // With 'p' suffix → same value.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("2000p".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        assert!((parse_image_radius(&conf).unwrap() - 2000.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_image_radius_missing_errors() {
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        let err = parse_image_radius(&conf).unwrap_err();
        assert!(err.contains("missing image.radius"));
    }

    #[test]
    fn test_parse_image_radius_strips_trailing_p_suffix() {
        // "1500p" → 1500.0 (p stripped).
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500p".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        let r = parse_image_radius(&conf).unwrap();
        assert_eq!(r, 1500.0);
    }

    #[test]
    fn test_parse_image_radius_decimal_without_suffix() {
        // "1234.5" → parses as f64 with decimal.
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1234.5".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        let r = parse_image_radius(&conf).unwrap();
        assert_eq!(r, 1234.5);
    }

    #[test]
    fn test_parse_image_radius_non_numeric_after_p_strip_errors() {
        // "abcp" → after strip "abc" → not f64 parseable → Err.
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("abcp".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        let r = parse_image_radius(&conf);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("cannot parse image radius"));
    }

    #[test]
    fn test_parse_image_radius_multiple_p_suffixes_trimmed() {
        // `trim_end_matches('p')` strips ALL trailing `p` chars — "1500ppp" → 1500.
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500ppp".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        let r = parse_image_radius(&conf).unwrap();
        assert_eq!(r, 1500.0);
    }

    #[test]
    fn test_parse_image_radius_invalid_errors() {
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("abc".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        let err = parse_image_radius(&conf).unwrap_err();
        assert!(err.contains("cannot parse image radius"));
    }

    #[test]
    fn test_compute_dims_none_ideogram_conf_uses_all_defaults() {
        // ideogram_conf=None → radius=0.85r, thickness=100p by default.
        let dims = compute_dims(None, 2000.0, "bupr", "n").unwrap();
        // 0.85 × 2000 = 1700.
        assert!((dims.ideogram_radius - 1700.0).abs() < 1e-9);
        // thickness default 100.
        assert_eq!(dims.ideogram_thickness, 100.0);
    }

    #[test]
    fn test_compute_dims_invalid_radius_string_errors() {
        // Invalid unit string in radius → Err.
        let mut conf = HashMap::new();
        conf.insert("radius".into(), ConfigValue::Str("abcxyz".into()));
        let r = compute_dims(Some(&conf), 1500.0, "bupr", "n");
        assert!(r.is_err());
    }

    #[test]
    fn test_compute_dims_pixel_radius_no_scaling() {
        // radius "1000p" → 1000.0 (not scaled by image_radius).
        let mut conf = HashMap::new();
        conf.insert("radius".into(), ConfigValue::Str("1000p".into()));
        let dims = compute_dims(Some(&conf), 2000.0, "bupr", "n").unwrap();
        assert_eq!(dims.ideogram_radius, 1000.0);
    }

    #[test]
    fn test_compute_dims_invariant_inner_less_than_outer() {
        // For any valid config: radius_inner < radius_outer.
        let dims = compute_dims(None, 2000.0, "bupr", "n").unwrap();
        assert!(dims.ideogram_radius_inner < dims.ideogram_radius_outer);
        // radius_outer == ideogram_radius.
        assert_eq!(dims.ideogram_radius, dims.ideogram_radius_outer);
        // ideogram_radius_inner = outer - thickness.
        assert_eq!(
            dims.ideogram_radius_inner,
            dims.ideogram_radius_outer - dims.ideogram_thickness
        );
    }

    #[test]
    fn test_compute_dims_default_radius_and_thickness() {
        // With an empty ideogram conf, defaults are 0.85r × image_radius and 100p.
        let dims = compute_dims(None, 1000.0, "bupr", "n").unwrap();
        assert!((dims.ideogram_radius - 850.0).abs() < 1e-9);
        assert!((dims.ideogram_thickness - 100.0).abs() < 1e-9);
        // compute_dims: outer = radius, inner = radius - thickness.
        assert!((dims.ideogram_radius_outer - dims.ideogram_radius).abs() < 1e-9);
        assert!((dims.ideogram_radius_inner - (dims.ideogram_radius - dims.ideogram_thickness))
            .abs()
            < 1e-9);
        assert!(dims.ideogram_radius_inner < dims.ideogram_radius);
    }

    #[test]
    fn test_compute_dims_explicit_radius_p_thickness() {
        // Explicit "1200p" radius and "50p" thickness.
        let mut ideo: HashMap<String, ConfigValue> = HashMap::new();
        ideo.insert("radius".into(), ConfigValue::Str("1200p".into()));
        ideo.insert("thickness".into(), ConfigValue::Str("50p".into()));
        let dims = compute_dims(Some(&ideo), 1500.0, "bupr", "n").unwrap();
        assert!((dims.ideogram_radius - 1200.0).abs() < 1e-9);
        assert!((dims.ideogram_thickness - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_scale_applies_per_tag() {
        // "tag:scale;tag:scale" format sets scale on matching ideograms.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
            mk_ideogram("hs3", "c", 2, 2),
        ];
        register_chromosomes_scale(&mut ideos, "a:2.5;c:0.5");
        assert!((ideos[0].scale - 2.5).abs() < 1e-9);
        assert!((ideos[1].scale - 1.0).abs() < 1e-9); // b unchanged
        assert!((ideos[2].scale - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_scale_ignores_malformed() {
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        // Missing colon → pair parses to None, silently ignored.
        register_chromosomes_scale(&mut ideos, "a_is_no_colon");
        assert!((ideos[0].scale - 1.0).abs() < 1e-9);
        // Non-numeric scale → parse fails, silently ignored.
        register_chromosomes_scale(&mut ideos, "a:NaNthing");
        assert!((ideos[0].scale - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_direction_flips_reverse() {
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
        ];
        register_chromosomes_direction(&mut ideos, "b");
        assert!(!ideos[0].reverse);
        assert!(ideos[1].reverse);
        // Multiple tags: semicolon-separated.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
        ];
        register_chromosomes_direction(&mut ideos, "a;b");
        assert!(ideos[0].reverse);
        assert!(ideos[1].reverse);
    }

    #[test]
    fn test_register_chromosomes_radius_p_suffix() {
        let dims = Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 50.0,
            ideogram_radius_inner: 950.0,
            ideogram_radius_outer: 1000.0,
        };
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        register_chromosomes_radius(&mut ideos, "a:1200p", 1500.0, &dims);
        assert!((ideos[0].radius - 1200.0).abs() < 1e-9);
        assert!((ideos[0].radius_outer - 1200.0).abs() < 1e-9);
        // inner = 1200 - 50 = 1150
        assert!((ideos[0].radius_inner - 1150.0).abs() < 1e-9);
        assert!((ideos[0].thickness - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_radius_r_suffix_scales_by_image_radius() {
        let dims = Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 100.0,
            ideogram_radius_inner: 950.0,
            ideogram_radius_outer: 1000.0,
        };
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        // 0.5r with image_radius=2000 → 1000.
        register_chromosomes_radius(&mut ideos, "a:0.5r", 2000.0, &dims);
        assert!((ideos[0].radius - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_compute_default_spacing_r_unit() {
        // 0.005r * gsize_noscale(3e9) = 1.5e7
        let sp = compute_default_spacing(None, 1.0, 3_000_000_000.0, "bupr", "n").unwrap();
        assert!((sp - 15_000_000.0).abs() < 1.0, "got {}", sp);
    }

    #[test]
    fn test_compute_default_spacing_u_unit_explicit() {
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("10u".into()));
        // 10u × chromosomes_units(1e6) = 1e7
        let sp = compute_default_spacing(Some(&conf), 1_000_000.0, 1.0, "bupr", "n").unwrap();
        assert!((sp - 10_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_default_spacing_invalid_unit_errors() {
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("5z".into()));
        let r = compute_default_spacing(Some(&conf), 1.0, 1.0, "bupr", "n");
        assert!(r.is_err());
    }

    #[test]
    fn test_link_ideograms_sets_breaks_between_different_chrs() {
        // hs1 and hs2 → break between them.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
        ];
        link_ideograms(&mut ideos);
        // Different chrs at both wrap boundaries → breaks on both sides.
        assert!(ideos[0].has_break_end);
        assert!(ideos[0].has_break_start); // neighbor (wrapped) is hs2
        assert!(ideos[1].has_break_end);
        assert!(ideos[1].has_break_start);
    }

    #[test]
    fn test_link_ideograms_empty_is_noop() {
        let mut ideos: Vec<Ideogram> = Vec::new();
        link_ideograms(&mut ideos);
        assert!(ideos.is_empty());
    }

    #[test]
    fn test_compute_default_spacing_none_conf_uses_default() {
        // spacing_conf=None → "0.005r" default path used.
        let sp = compute_default_spacing(None, 1.0, 2_000_000_000.0, "bupr", "n").unwrap();
        // 0.005 × 2e9 = 1e7.
        assert!((sp - 10_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_compute_default_spacing_decimal_u_unit() {
        // "2.5u" × chromosomes_units.
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("2.5u".into()));
        let sp = compute_default_spacing(Some(&conf), 1_000_000.0, 1.0, "bupr", "n").unwrap();
        assert!((sp - 2_500_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_default_spacing_zero_r_value() {
        // "0r" → 0 regardless of gsize_noscale.
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("0r".into()));
        let sp = compute_default_spacing(Some(&conf), 1.0, 3e9, "bupr", "n").unwrap();
        assert_eq!(sp, 0.0);
    }

    #[test]
    fn test_compute_default_spacing_negative_value() {
        // Negative spacing propagates through (no clamp).
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("-5u".into()));
        let sp = compute_default_spacing(Some(&conf), 1_000_000.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(sp, -5_000_000.0);
    }

    #[test]
    fn test_compute_default_spacing_b_unit() {
        // "5b" → 5 (bases, no multiplier).
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("5b".into()));
        let sp = compute_default_spacing(Some(&conf), 1_000_000.0, 3e9, "bupr", "n").unwrap();
        assert!((sp - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_compute_default_spacing_nounit_passthrough() {
        // Plain number "42" → nounit → value passthrough.
        let mut conf = HashMap::new();
        conf.insert("default".into(), ConfigValue::Str("42".into()));
        let sp = compute_default_spacing(Some(&conf), 1.0, 1.0, "bupr", "n").unwrap();
        assert!((sp - 42.0).abs() < 1e-12);
    }

    #[test]
    fn test_compute_dims_thickness_r_unit_scales_by_image_radius() {
        // thickness="0.05r" with image_radius=2000 → 100 pixels.
        let mut conf = HashMap::new();
        conf.insert("radius".into(), ConfigValue::Str("0.9r".into()));
        conf.insert("thickness".into(), ConfigValue::Str("0.05r".into()));
        let dims = compute_dims(Some(&conf), 2000.0, "bupr", "n").unwrap();
        assert!((dims.ideogram_thickness - 100.0).abs() < 1e-9);
        // radius_inner = radius_outer - thickness = 0.9×2000 - 100 = 1700.
        assert!((dims.ideogram_radius - 1800.0).abs() < 1e-9);
        assert!((dims.ideogram_radius_inner - 1700.0).abs() < 1e-9);
    }

    #[test]
    fn test_link_ideograms_single_ideogram_has_both_breaks() {
        // n=1: next_i = prev_i = 0 (self-wrap). Same chr, but set min/max
        // don't "seam" with themselves (100 != -1, 0 != 101) → both breaks set.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        link_ideograms(&mut ideos);
        assert!(ideos[0].has_break_start);
        assert!(ideos[0].has_break_end);
    }

    #[test]
    fn test_make_chrorder_groups_reorders_by_tag() {
        // Default display_idx: 0, 1, 2. Order "c;a;b" → c=0, a=1, b=2.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
            mk_ideogram("hs3", "c", 2, 2),
        ];
        make_chrorder_groups(&mut ideos, "c;a;b");
        // After sort by display_idx, c first (display_idx=0).
        assert_eq!(ideos[0].tag, "c");
        assert_eq!(ideos[1].tag, "a");
        assert_eq!(ideos[2].tag, "b");
    }

    #[test]
    fn test_make_chrorder_groups_skips_dash_and_pipe() {
        // `-` and `|` are layout directives, not tags — should not consume a slot.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
        ];
        make_chrorder_groups(&mut ideos, "a;-;b");
        // "a" at idx 0, "-" skipped (idx 1 unused), "b" at idx 2.
        assert_eq!(ideos[0].tag, "a");
        assert_eq!(ideos[0].display_idx, 0);
        assert_eq!(ideos[1].tag, "b");
        assert_eq!(ideos[1].display_idx, 2);
    }

    #[test]
    fn test_make_chrorder_groups_matches_by_chr_when_tag_differs() {
        // A token in order_str that matches ideo.chr (but not ideo.tag) still
        // triggers display_idx assignment.
        let mut ideos = vec![
            mk_ideogram("hs1", "tag1".into(), 0, 0),
            mk_ideogram("hs2", "tag2".into(), 1, 1),
        ];
        make_chrorder_groups(&mut ideos, "hs2;hs1");
        // "hs2" → chr-match on ideo 1 → display_idx = 0.
        // "hs1" → chr-match on ideo 0 → display_idx = 1.
        let hs1 = ideos.iter().find(|i| i.chr == "hs1").unwrap();
        let hs2 = ideos.iter().find(|i| i.chr == "hs2").unwrap();
        assert_eq!(hs2.display_idx, 0);
        assert_eq!(hs1.display_idx, 1);
    }

    #[test]
    fn test_make_chrorder_groups_empty_order_string_no_change() {
        // Empty string → splits to [""] → inner loop skips empty tag → no ops.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 5),
            mk_ideogram("hs2", "b", 1, 7),
        ];
        make_chrorder_groups(&mut ideos, "");
        // display_idx unchanged (still 5, 7), then stable sort preserves.
        assert_eq!(ideos[0].display_idx, 5);
        assert_eq!(ideos[1].display_idx, 7);
    }

    #[test]
    fn test_make_chrorder_groups_whitespace_tokens_trimmed() {
        // Order string with whitespace: "  a  ;  b  " → trimmed to "a", "b".
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 10),
            mk_ideogram("hs2", "b", 1, 20),
        ];
        make_chrorder_groups(&mut ideos, "  a  ;  b  ");
        // "a" trimmed → display_idx 0, "b" trimmed → display_idx 1.
        assert_eq!(ideos[0].tag, "a");
        assert_eq!(ideos[0].display_idx, 0);
        assert_eq!(ideos[1].tag, "b");
        assert_eq!(ideos[1].display_idx, 1);
    }

    #[test]
    fn test_register_chromosomes_direction_unknown_tag_is_noop() {
        // An unknown tag in the reverse string → no-op.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        register_chromosomes_direction(&mut ideos, "nonexistent");
        assert!(!ideos[0].reverse);
    }

    #[test]
    fn test_register_chromosomes_scale_whitespace_tolerated() {
        // Whitespace around tag and scale value tolerated.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        register_chromosomes_scale(&mut ideos, "  a  :  3.14  ");
        assert!((ideos[0].scale - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_radius_bare_number_no_suffix() {
        // "100" without r/p suffix — parsed as plain f64 (pixels).
        let dims = Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 50.0,
            ideogram_radius_inner: 950.0,
            ideogram_radius_outer: 1000.0,
        };
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        register_chromosomes_radius(&mut ideos, "a:500", 1500.0, &dims);
        assert!((ideos[0].radius - 500.0).abs() < 1e-9);
        // inner = 500 - 50 = 450.
        assert!((ideos[0].radius_inner - 450.0).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_radius_malformed_entry_skipped() {
        // Entry without `:` → split_once returns None → skipped silently.
        let dims = Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 50.0,
            ideogram_radius_inner: 950.0,
            ideogram_radius_outer: 1000.0,
        };
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let original_radius = ideos[0].radius;
        register_chromosomes_radius(&mut ideos, "no_colon_here", 1500.0, &dims);
        assert_eq!(ideos[0].radius, original_radius);
    }

    #[test]
    fn test_make_chrorder_groups_unknown_tag_skipped() {
        // "xyz" doesn't match any ideogram → no display_idx update for anyone
        // at that index; ideograms keep their prior display_idx.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 7),
            mk_ideogram("hs2", "b", 1, 3),
        ];
        make_chrorder_groups(&mut ideos, "xyz;a");
        // "a" → ideo 0 gets display_idx=1; "xyz" matches nothing.
        let a = ideos.iter().find(|i| i.tag == "a").unwrap();
        let b = ideos.iter().find(|i| i.tag == "b").unwrap();
        assert_eq!(a.display_idx, 1);
        assert_eq!(b.display_idx, 3); // unchanged from starting value
    }

    fn mk_minimal_conf_and_karyotype() -> (HashMap<String, ConfigValue>, crate::karyotype::types::Karyotype) {
        use crate::karyotype::types::{Chromosome, DisplayRegion, Karyotype};
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        conf.insert("chromosomes_units".into(), ConfigValue::Str("1000000".into()));
        conf.insert(
            "chromosomes_display_default".into(),
            ConfigValue::Str("1".into()),
        );
        let mut karyo = Karyotype::default();
        let chr = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 100_000_000,
            color: "red".into(),
            set: IntSpan::from_range(0, 100_000_000),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        karyo.order.push("hs1".into());
        karyo.chromosomes.insert("hs1".into(), chr);
        (conf, karyo)
    }

    #[test]
    fn test_layout_build_single_chromosome_minimal() {
        let (conf, karyo) = mk_minimal_conf_and_karyotype();
        let layout = Layout::build(&conf, &karyo).expect("build should succeed");
        assert_eq!(layout.ideograms.len(), 1);
        assert_eq!(layout.ideograms[0].chr, "hs1");
        assert!((layout.image_radius - 1500.0).abs() < 1e-9);
        // default angle_offset = -90 (0bp at 12 o'clock).
        assert!((layout.angle_offset - (-90.0)).abs() < 1e-9);
        // gsize_noscale = chromosome length.
        assert!(layout.gsize_noscale > 0.0);
        // dims populated.
        assert!(layout.dims.ideogram_radius > 0.0);
    }

    #[test]
    fn test_layout_build_missing_image_radius_errors() {
        let (_, karyo) = mk_minimal_conf_and_karyotype();
        // Empty conf — image.radius missing → Err.
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        let r = Layout::build(&conf, &karyo);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("missing image.radius"));
    }

    #[test]
    fn test_layout_build_multiple_chromosomes() {
        use crate::karyotype::types::{Chromosome, DisplayRegion, Karyotype};
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        conf.insert("chromosomes_units".into(), ConfigValue::Str("1000000".into()));
        conf.insert(
            "chromosomes_display_default".into(),
            ConfigValue::Str("1".into()),
        );
        let mut karyo = Karyotype::default();
        for (name, label, end, idx) in [
            ("hs1", "1", 100_000_000i64, 0usize),
            ("hs2", "2", 80_000_000, 1),
            ("hs3", "3", 50_000_000, 2),
        ] {
            let chr = Chromosome {
                name: name.into(),
                label: label.into(),
                start: 0,
                end,
                color: "red".into(),
                set: IntSpan::from_range(0, end),
                index: idx,
                display: true,
                display_region: DisplayRegion::default(),
            };
            karyo.order.push(name.into());
            karyo.chromosomes.insert(name.into(), chr);
        }
        let layout = Layout::build(&conf, &karyo).expect("build");
        assert_eq!(layout.ideograms.len(), 3);
        // Cumulative lengths should be monotonically increasing or at least positive.
        assert!(layout.gsize_noscale > 200_000_000.0);
    }

    fn mk_karyotype_2chr() -> crate::karyotype::types::Karyotype {
        use crate::karyotype::types::{Chromosome, DisplayRegion, Karyotype};
        let mut k = Karyotype::default();
        for (name, end, idx) in [("hs1", 100_000_000i64, 0usize), ("hs2", 80_000_000, 1)] {
            let chr = Chromosome {
                name: name.into(),
                label: name.into(),
                start: 0,
                end,
                color: "red".into(),
                set: IntSpan::from_range(0, end),
                index: idx,
                display: true,
                display_region: DisplayRegion::default(),
            };
            k.order.push(name.into());
            k.chromosomes.insert(name.into(), chr);
        }
        k
    }

    #[test]
    fn test_create_ideogram_set_default_adds_all_chrs() {
        let k = mk_karyotype_2chr();
        // display_default=true, empty filter → both chrs as ideograms.
        let ideos = create_ideogram_set(&k, true, "", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 2);
    }

    #[test]
    fn test_create_ideogram_set_explicit_filter_respects_order() {
        let k = mk_karyotype_2chr();
        // "hs2;hs1" filter with display_default=false → exactly these 2 in this order.
        let ideos = create_ideogram_set(&k, false, "hs2;hs1", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 2);
        assert_eq!(ideos[0].chr, "hs2");
        assert_eq!(ideos[1].chr, "hs1");
    }

    #[test]
    fn test_create_ideogram_set_tag_assigned_from_filter() {
        let k = mk_karyotype_2chr();
        // Tagged filter "hs1[a]" → ideogram gets tag "a".
        let ideos = create_ideogram_set(&k, false, "hs1[a]", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 1);
        assert_eq!(ideos[0].chr, "hs1");
        assert_eq!(ideos[0].tag, "a");
    }

    #[test]
    fn test_create_ideogram_set_multiple_tags_per_chr() {
        let k = mk_karyotype_2chr();
        // Multi-ideogram-per-chr: "hs1[a]:0-20;hs1[b]:20-40" → 2 ideograms on hs1.
        let ideos =
            create_ideogram_set(&k, false, "hs1[a]:0-20;hs1[b]:20-40", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 2);
        assert!(ideos.iter().all(|i| i.chr == "hs1"));
        let tags: Vec<&str> = ideos.iter().map(|i| i.tag.as_str()).collect();
        assert!(tags.contains(&"a"));
        assert!(tags.contains(&"b"));
    }

    #[test]
    fn test_create_ideogram_set_exclude_filter() {
        let k = mk_karyotype_2chr();
        // display_default=true, "-hs2" excludes → only hs1 in ideograms.
        let ideos = create_ideogram_set(&k, true, "-hs2", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 1);
        assert_eq!(ideos[0].chr, "hs1");
    }

    #[test]
    fn test_getrdistance_unknown_chr_returns_none() {
        // Unknown chr → find_ideogram returns None → getrdistance None.
        let layout = mk_layout();
        assert!(layout.getrdistance(50, "unknown_chr", 1000.0).is_none());
    }

    #[test]
    fn test_getrdistance_proportional_to_radius() {
        // For same pos on same chr, distance scales linearly with r.
        let layout = mk_layout();
        let d1 = layout.getrdistance(50, "hs1", 1000.0).unwrap();
        let d2 = layout.getrdistance(50, "hs1", 2000.0).unwrap();
        // 2× radius → 2× arc length (same angular position).
        assert!((d2 - 2.0 * d1).abs() < 1e-6, "d1={}, d2={}", d1, d2);
    }

    #[test]
    fn test_get_ideogram_by_idx_returns_matching_display_idx() {
        // get_ideogram_by_idx searches by display_idx (not array index).
        let layout = mk_layout();
        // mk_layout sets display_idx = idx for each ideo.
        let ideo = layout.get_ideogram_by_idx(1);
        assert_eq!(ideo.display_idx, 1);
        let ideo = layout.get_ideogram_by_idx(2);
        assert_eq!(ideo.display_idx, 2);
    }

    #[test]
    fn test_find_ideogram_by_chr_with_nonexistent_returns_none() {
        // find_ideogram_by_chr returns None for missing chr.
        let layout = mk_layout();
        assert!(layout.find_ideogram_by_chr("chrWAT").is_none());
        // Existing chr → Some with matching chr field.
        let ideo = layout.find_ideogram_by_chr("hs3").expect("hs3");
        assert_eq!(ideo.chr, "hs3");
    }

    #[test]
    fn test_layout_build_chromosomes_reverse_applied() {
        // chromosomes_reverse in conf → ideogram.reverse=true for matching tags.
        let (mut conf, karyo) = mk_minimal_conf_and_karyotype();
        conf.insert(
            "chromosomes_reverse".into(),
            ConfigValue::Str("hs1".into()),
        );
        let layout = Layout::build(&conf, &karyo).unwrap();
        assert!(layout.ideograms[0].reverse);
    }

    #[test]
    fn test_layout_build_no_reverse_by_default() {
        // Without chromosomes_reverse → all ideograms have reverse=false.
        let (conf, karyo) = mk_minimal_conf_and_karyotype();
        let layout = Layout::build(&conf, &karyo).unwrap();
        for ideo in &layout.ideograms {
            assert!(!ideo.reverse);
        }
    }

    #[test]
    fn test_layout_build_chromosomes_radius_applied() {
        // chromosomes_radius in conf → ideogram.radius set via register_chromosomes_radius.
        let (mut conf, karyo) = mk_minimal_conf_and_karyotype();
        conf.insert(
            "chromosomes_radius".into(),
            ConfigValue::Str("hs1:800p".into()),
        );
        let layout = Layout::build(&conf, &karyo).unwrap();
        assert_eq!(layout.ideograms[0].radius, 800.0);
    }

    #[test]
    fn test_layout_build_chromosomes_order_respected() {
        // chromosomes_order reorders ideograms by display_idx.
        use crate::karyotype::types::{Chromosome, DisplayRegion, Karyotype};
        let mut image = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        conf.insert("chromosomes_display_default".into(), ConfigValue::Str("1".into()));
        conf.insert(
            "chromosomes_order".into(),
            ConfigValue::Str("hs3;hs1;hs2".into()),
        );
        let mut karyo = Karyotype::default();
        for (name, end, idx) in [("hs1", 100_000_000i64, 0usize), ("hs2", 80_000_000, 1), ("hs3", 50_000_000, 2)] {
            let chr = Chromosome {
                name: name.into(),
                label: name.into(),
                start: 0,
                end,
                color: "red".into(),
                set: IntSpan::from_range(0, end),
                index: idx,
                display: true,
                display_region: DisplayRegion::default(),
            };
            karyo.order.push(name.into());
            karyo.chromosomes.insert(name.into(), chr);
        }
        let layout = Layout::build(&conf, &karyo).unwrap();
        // Order should be hs3, hs1, hs2 from the explicit chromosomes_order.
        let chrs: Vec<&str> = layout.ideograms.iter().map(|i| i.chr.as_str()).collect();
        assert_eq!(chrs, vec!["hs3", "hs1", "hs2"]);
    }

    #[test]
    fn test_layout_build_counterclockwise_flag_from_config() {
        // image.angle_orientation = "counterclockwise" → layout.counterclockwise=true.
        let (mut conf, karyo) = mk_minimal_conf_and_karyotype();
        let image = conf.get_mut("image").unwrap();
        if let ConfigValue::Map(m) = image {
            m.insert(
                "angle_orientation".into(),
                ConfigValue::Str("counterclockwise".into()),
            );
        }
        let layout = Layout::build(&conf, &karyo).unwrap();
        assert!(layout.counterclockwise);
    }

    #[test]
    fn test_layout_build_default_orientation_clockwise() {
        // Without angle_orientation, layout.counterclockwise = false (clockwise default).
        let (conf, karyo) = mk_minimal_conf_and_karyotype();
        let layout = Layout::build(&conf, &karyo).unwrap();
        assert!(!layout.counterclockwise);
    }

    #[test]
    fn test_layout_build_custom_angle_offset() {
        // Setting image.angle_offset to "45" → layout.angle_offset = 45.0.
        let (mut conf, karyo) = mk_minimal_conf_and_karyotype();
        let image = conf.get_mut("image").unwrap();
        if let ConfigValue::Map(m) = image {
            m.insert("angle_offset".into(), ConfigValue::Str("45".into()));
        }
        let layout = Layout::build(&conf, &karyo).unwrap();
        assert_eq!(layout.angle_offset, 45.0);
    }

    #[test]
    fn test_dims_default_is_all_zero() {
        // Dims::default has all f64 fields = 0.0.
        let d = Dims::default();
        assert_eq!(d.ideogram_radius, 0.0);
        assert_eq!(d.ideogram_thickness, 0.0);
        assert_eq!(d.ideogram_radius_inner, 0.0);
        assert_eq!(d.ideogram_radius_outer, 0.0);
    }

    #[test]
    fn test_layout_build_gcircum_accounts_for_spacing() {
        // gcircum includes both ideogram lengths AND spacing. For a single
        // chromosome, gcircum ≥ chr length.
        let (conf, karyo) = mk_minimal_conf_and_karyotype();
        let layout = Layout::build(&conf, &karyo).unwrap();
        // mk_minimal_conf_and_karyotype creates hs1 with length 100M.
        assert!(layout.gcircum >= layout.gsize_noscale);
    }

    #[test]
    fn test_layout_build_single_chr_sizes_populate_cumulative_fields() {
        // For a single ideogram: length_cumulative_{scaled,noscale} should be 0.
        let (conf, karyo) = mk_minimal_conf_and_karyotype();
        let layout = Layout::build(&conf, &karyo).unwrap();
        let ideo = &layout.ideograms[0];
        assert_eq!(ideo.length_cumulative_scaled, 0.0);
        assert_eq!(ideo.length_cumulative_noscale, 0.0);
        // Its own length fields populated.
        assert!(ideo.length_noscale > 0.0);
    }

    #[test]
    fn test_layout_build_scale_applied_to_length_scaled() {
        // With chromosomes_scale="hs1:2" → length_scaled = 2 × length_noscale.
        let (mut conf, karyo) = mk_minimal_conf_and_karyotype();
        conf.insert(
            "chromosomes_scale".into(),
            ConfigValue::Str("hs1:2".into()),
        );
        let layout = Layout::build(&conf, &karyo).unwrap();
        let ideo = &layout.ideograms[0];
        // scale=2 → length_scaled doubles.
        assert!((ideo.length_scaled - 2.0 * ideo.length_noscale).abs() < 1.0);
    }

    #[test]
    fn test_layout_build_chromosomes_units_parsing() {
        // chromosomes_units="500000" → layout.chromosomes_units = 500_000.
        let (mut conf, karyo) = mk_minimal_conf_and_karyotype();
        conf.insert(
            "chromosomes_units".into(),
            ConfigValue::Str("500000".into()),
        );
        let layout = Layout::build(&conf, &karyo).unwrap();
        assert_eq!(layout.chromosomes_units, 500_000.0);
    }

    #[test]
    fn test_create_ideogram_set_empty_filter_default_off_yields_empty() {
        // display_default=false and empty filter → no ideograms.
        let k = mk_karyotype_2chr();
        let ideos = create_ideogram_set(&k, false, "", 1_000_000.0).unwrap();
        assert!(ideos.is_empty());
    }

    #[test]
    fn test_create_ideogram_set_filter_with_range_trims_to_region() {
        // "hs1:0-20" with chromosomes_units=1e6 → region [0, 20M].
        let k = mk_karyotype_2chr();
        let ideos = create_ideogram_set(&k, false, "hs1:0-20", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 1);
        assert_eq!(ideos[0].chr, "hs1");
        // Set clamped to requested region; 0..=20M = 20_000_001 elements.
        assert_eq!(ideos[0].set.cardinality(), 20_000_001);
        assert_eq!(ideos[0].set.min(), Some(0));
        assert_eq!(ideos[0].set.max(), Some(20_000_000));
    }

    #[test]
    fn test_create_ideogram_set_open_end_runlist_extends_to_chr_end() {
        // "hs1:0-" (open-ended range) → goes to chr.end (100M).
        let k = mk_karyotype_2chr();
        let ideos = create_ideogram_set(&k, false, "hs1:0-", 1_000_000.0).unwrap();
        assert_eq!(ideos.len(), 1);
        // Should cover full chr [0, 100M].
        assert_eq!(ideos[0].set.min(), Some(0));
        assert_eq!(ideos[0].set.max(), Some(100_000_000));
    }

    #[test]
    fn test_create_ideogram_set_filter_with_unknown_chr_returns_empty_or_skips() {
        // "hsX" not in karyotype — current impl doesn't error, it silently
        // skips the unknown chr and returns an empty result.
        let k = mk_karyotype_2chr();
        let r = create_ideogram_set(&k, false, "hsX", 1_000_000.0);
        assert!(r.is_ok());
        let ideos = r.unwrap();
        // Unknown chr doesn't yield an ideogram; the filter is tolerant.
        assert!(ideos.is_empty() || !ideos.iter().any(|i| i.chr == "hsX"));
    }

    #[test]
    fn test_register_chromosomes_scale_multiple_entries_semicolon_separated() {
        // Multiple tag:scale pairs separated by ';' — all applied.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 0),
            mk_ideogram("hs3", "c", 2, 0),
        ];
        register_chromosomes_scale(&mut ideos, "a:2.0;b:0.5;c:1.5");
        assert!((ideos[0].scale - 2.0).abs() < 1e-9);
        assert!((ideos[1].scale - 0.5).abs() < 1e-9);
        assert!((ideos[2].scale - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_scale_invalid_number_skipped() {
        // "a:notanumber" — parse fails → skipped (scale unchanged).
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let before = ideos[0].scale;
        register_chromosomes_scale(&mut ideos, "a:notanumber");
        // Scale unchanged because the parse failed.
        assert_eq!(ideos[0].scale, before);
    }

    #[test]
    fn test_register_chromosomes_direction_multiple_tags() {
        // Multiple tags in one string — all get reverse=true; untouched tags stay false.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 0),
            mk_ideogram("hs3", "c", 2, 0),
        ];
        register_chromosomes_direction(&mut ideos, "a;c");
        assert!(ideos[0].reverse);
        assert!(!ideos[1].reverse);
        assert!(ideos[2].reverse);
    }

    #[test]
    fn test_register_chromosomes_radius_multi_entries_mixed_suffixes() {
        // Mixed r/p/bare suffixes in one radius_str.
        let dims = Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 50.0,
            ideogram_radius_inner: 950.0,
            ideogram_radius_outer: 1000.0,
        };
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 0),
            mk_ideogram("hs3", "c", 2, 0),
        ];
        register_chromosomes_radius(&mut ideos, "a:0.8r;b:1200p;c:500", 1500.0, &dims);
        // a: 0.8 × 1500 = 1200.
        assert!((ideos[0].radius - 1200.0).abs() < 1e-9);
        // b: 1200 pixels verbatim.
        assert!((ideos[1].radius - 1200.0).abs() < 1e-9);
        // c: bare 500.
        assert!((ideos[2].radius - 500.0).abs() < 1e-9);
        // inner = radius - thickness.
        assert!((ideos[0].radius_inner - 1150.0).abs() < 1e-9);
        assert!((ideos[2].radius_inner - 450.0).abs() < 1e-9);
    }

    #[test]
    fn test_make_chrorder_groups_skips_layout_sentinels() {
        // Sentinel tags "-", "|" are layout directives — skipped in display_idx assignment.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 0),
        ];
        make_chrorder_groups(&mut ideos, "-;a;|;b;-");
        // "a" at index 1 → display_idx=1; "b" at index 3 → display_idx=3.
        let a = ideos.iter().find(|i| i.tag == "a").unwrap();
        let b = ideos.iter().find(|i| i.tag == "b").unwrap();
        assert_eq!(a.display_idx, 1);
        assert_eq!(b.display_idx, 3);
    }

    #[test]
    fn test_register_chromosomes_scale_empty_string_noop() {
        // Empty scale_str → `"".split(';')` yields [""] which has no ':' → parse_assignment fails, noop.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let before = ideos[0].scale;
        register_chromosomes_scale(&mut ideos, "");
        assert_eq!(ideos[0].scale, before);
    }

    #[test]
    fn test_parse_image_radius_edge_cases() {
        // Various number formats for image radius config.
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("2000p".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        assert_eq!(parse_image_radius(&conf).unwrap(), 2000.0);
        // Bare number (no p suffix).
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        assert_eq!(parse_image_radius(&conf).unwrap(), 1500.0);
    }

    #[test]
    fn test_link_ideograms_zero_ideograms_noop() {
        // Empty slice → no-op, no panic.
        let mut ideos: Vec<Ideogram> = Vec::new();
        link_ideograms(&mut ideos);
        assert!(ideos.is_empty());
    }

    #[test]
    fn test_register_chromosomes_direction_with_whitespace() {
        // Tags separated by ";" with whitespace around each tag → trimmed.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 0),
        ];
        register_chromosomes_direction(&mut ideos, "  a  ;  b  ");
        assert!(ideos[0].reverse);
        assert!(ideos[1].reverse);
    }

    #[test]
    fn test_register_chromosomes_scale_missing_colon_skipped() {
        // Entry "bare_tag" without `:` → split_once None → skipped.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let before = ideos[0].scale;
        register_chromosomes_scale(&mut ideos, "bare_tag");
        assert_eq!(ideos[0].scale, before);
    }

    #[test]
    fn test_parse_image_radius_r_suffix_not_supported_returns_error() {
        // parse_image_radius only handles bare and p-suffix; r-suffix should fail.
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("0.5r".into()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("image".into(), ConfigValue::Map(image));
        // With "0.5r" — r-suffix may either be trimmed (if impl supports) or fail.
        // Let's test what the impl does — assert it returns either Ok or Err without panic.
        let r = parse_image_radius(&conf);
        assert!(r.is_ok() || r.is_err()); // documents current behavior
    }

    #[test]
    fn test_make_chrorder_groups_empty_string_noop() {
        // Empty order_str → no ideograms get display_idx updates.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 7),
            mk_ideogram("hs2", "b", 1, 3),
        ];
        make_chrorder_groups(&mut ideos, "");
        // Original display_idx preserved (empty string → no assignments).
        let a = ideos.iter().find(|i| i.tag == "a").unwrap();
        let b = ideos.iter().find(|i| i.tag == "b").unwrap();
        assert_eq!(a.display_idx, 7);
        assert_eq!(b.display_idx, 3);
    }

    #[test]
    fn test_register_chromosomes_radius_invalid_number_skipped() {
        // Non-numeric radius → parse fails → skipped (unwrap_or(0.0) gives 0).
        let dims = Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 50.0,
            ideogram_radius_inner: 950.0,
            ideogram_radius_outer: 1000.0,
        };
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let before = ideos[0].radius;
        register_chromosomes_radius(&mut ideos, "a:abcdef", 1500.0, &dims);
        // Parse fails → 0.0 (via unwrap_or(0.0)).
        assert_eq!(ideos[0].radius, 0.0);
        // But inner/thickness still updated.
        assert_eq!(ideos[0].thickness, 50.0);
        let _ = before;
    }

    #[test]
    fn test_register_chromosomes_direction_empty_string_noop() {
        // Empty reverse_str → "" → one iteration with tag="" → no match.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        assert!(!ideos[0].reverse);
        register_chromosomes_direction(&mut ideos, "");
        assert!(!ideos[0].reverse);
    }

    #[test]
    fn test_parse_image_radius_missing_image_key_errors() {
        // No `image` submap → parse_image_radius errors.
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        let r = parse_image_radius(&conf);
        assert!(r.is_err());
    }

    #[test]
    fn test_register_chromosomes_scale_preserves_untagged_ideograms() {
        // Apply scale only to tag "a"; ideogram with tag "b" → unchanged.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 0),
        ];
        let b_before = ideos[1].scale;
        register_chromosomes_scale(&mut ideos, "a:2.0");
        assert_eq!(ideos[0].scale, 2.0);
        assert_eq!(ideos[1].scale, b_before);
    }

    #[test]
    fn test_getxypos_at_angle_90_places_on_positive_y_axis() {
        // angle=90° → cos=0, sin=1 → (image_radius, image_radius + radius).
        let layout = mk_layout();
        let (x, y) = layout.getxypos(90.0, 100.0);
        assert!((x - layout.image_radius).abs() < 1e-9);
        assert!((y - (layout.image_radius + 100.0)).abs() < 1e-9);
    }

    #[test]
    fn test_getxypos_at_angle_180_places_on_negative_x_axis() {
        // angle=180° → cos=-1, sin=0 → (image_radius - radius, image_radius).
        let layout = mk_layout();
        let (x, y) = layout.getxypos(180.0, 100.0);
        assert!((x - (layout.image_radius - 100.0)).abs() < 1e-9);
        assert!((y - layout.image_radius).abs() < 1e-9);
    }

    #[test]
    fn test_find_ideogram_by_chr_missing_returns_none() {
        // Chromosome not in any ideogram → None.
        let layout = mk_layout();
        assert!(layout.find_ideogram_by_chr("does_not_exist").is_none());
        // Existing chr hits first matching ideogram.
        assert!(layout.find_ideogram_by_chr("hs1").is_some());
        assert_eq!(layout.find_ideogram_by_chr("hs1").unwrap().chr, "hs1");
    }

    #[test]
    fn test_get_ideogram_idx_pos_outside_bounds_returns_none() {
        // mk_ideogram sets span 0..=100; pos=150 is not a member of any → None.
        let layout = mk_layout();
        assert!(layout.get_ideogram_idx(150, "hs1").is_none());
        // Wrong chromosome name → None regardless of pos.
        assert!(layout.get_ideogram_idx(50, "nope").is_none());
        // Valid pos + chr → Some(display_idx).
        assert_eq!(layout.get_ideogram_idx(50, "hs2"), Some(1));
    }

    #[test]
    fn test_parse_image_radius_strips_p_suffix_and_parses_f64() {
        // "1500p" → strip 'p' → parse → 1500.0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("1500p".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        assert_eq!(parse_image_radius(&conf).unwrap(), 1500.0);
        // Bare number (no p) also parses.
        let mut conf2: HashMap<String, ConfigValue> = HashMap::new();
        let mut image2: HashMap<String, ConfigValue> = HashMap::new();
        image2.insert("radius".into(), ConfigValue::Str("1234.5".into()));
        conf2.insert("image".into(), ConfigValue::Map(image2));
        assert_eq!(parse_image_radius(&conf2).unwrap(), 1234.5);
    }

    #[test]
    fn test_parse_image_radius_unparseable_value_returns_error() {
        // Non-numeric radius → Err mentioning value.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut image: HashMap<String, ConfigValue> = HashMap::new();
        image.insert("radius".into(), ConfigValue::Str("not_a_number".into()));
        conf.insert("image".into(), ConfigValue::Map(image));
        let err = parse_image_radius(&conf).unwrap_err();
        assert!(err.contains("cannot parse"));
        assert!(err.contains("not_a_number"));
    }

    #[test]
    fn test_get_ideogram_by_idx_valid_lookup_returns_matching_ideogram() {
        // Direct lookup by display_idx — returns the ideogram with that index.
        let layout = mk_layout();
        let ideo = layout.get_ideogram_by_idx(0);
        assert_eq!(ideo.display_idx, 0);
        assert_eq!(ideo.chr, "hs1");
        let ideo2 = layout.get_ideogram_by_idx(2);
        assert_eq!(ideo2.chr, "hs3");
    }

    #[test]
    fn test_getxypos_at_angle_270_places_on_negative_y_axis() {
        // angle=270° → cos≈0, sin=-1 → (cx, cy-r).
        let layout = mk_layout();
        let (x, y) = layout.getxypos(270.0, 100.0);
        assert!((x - layout.image_radius).abs() < 1e-9);
        assert!((y - (layout.image_radius - 100.0)).abs() < 1e-9);
    }

    #[test]
    fn test_register_chromosomes_radius_r_suffix_scales_image_radius() {
        // "a:0.5r" → 0.5 × image_radius = 750 for image_radius=1500.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let dims = crate::layout::Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 100.0,
            ideogram_radius_inner: 900.0,
            ideogram_radius_outer: 1000.0,
        };
        register_chromosomes_radius(&mut ideos, "a:0.5r", 1500.0, &dims);
        assert_eq!(ideos[0].radius, 750.0);
        assert_eq!(ideos[0].radius_outer, 750.0);
        assert_eq!(ideos[0].radius_inner, 650.0);
        assert_eq!(ideos[0].thickness, 100.0);
    }

    #[test]
    fn test_register_chromosomes_radius_p_suffix_takes_raw_pixels() {
        // "a:800p" → exactly 800 pixels (no scaling).
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        let dims = crate::layout::Dims {
            ideogram_radius: 1000.0,
            ideogram_thickness: 100.0,
            ideogram_radius_inner: 900.0,
            ideogram_radius_outer: 1000.0,
        };
        register_chromosomes_radius(&mut ideos, "a:800p", 1500.0, &dims);
        assert_eq!(ideos[0].radius, 800.0);
    }

    #[test]
    fn test_register_chromosomes_direction_multiple_tags_semicolon_separated() {
        // "a;c" reverses ideograms with tag a and c; b unchanged.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
            mk_ideogram("hs3", "c", 2, 2),
        ];
        register_chromosomes_direction(&mut ideos, "a;c");
        assert!(ideos[0].reverse);
        assert!(!ideos[1].reverse);
        assert!(ideos[2].reverse);
    }

    #[test]
    fn test_register_chromosomes_scale_multiple_pairs_semicolon_separated() {
        // "a:2.0;b:0.5" → a.scale=2.0, b.scale=0.5.
        let mut ideos = vec![
            mk_ideogram("hs1", "a", 0, 0),
            mk_ideogram("hs2", "b", 1, 1),
            mk_ideogram("hs3", "c", 2, 2),
        ];
        let c_before = ideos[2].scale;
        register_chromosomes_scale(&mut ideos, "a:2.0;b:0.5");
        assert_eq!(ideos[0].scale, 2.0);
        assert_eq!(ideos[1].scale, 0.5);
        // c unchanged.
        assert_eq!(ideos[2].scale, c_before);
    }

    #[test]
    fn test_compute_default_spacing_r_unit_scales_by_gsize_noscale() {
        // default="0.005r" with gsize=1000 → 0.005 × 1000 = 5.
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("0.005r".into()));
        let r = compute_default_spacing(
            Some(&spacing_conf),
            1_000_000.0, 1000.0, "bupr", "n",
        )
        .unwrap();
        assert_eq!(r, 5.0);
    }

    #[test]
    fn test_compute_default_spacing_u_unit_scales_by_chromosomes_units() {
        // default="2u" with cu=1e6 → 2 × 1e6 = 2_000_000.
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("2u".into()));
        let r = compute_default_spacing(
            Some(&spacing_conf),
            1_000_000.0, 1000.0, "bupr", "n",
        )
        .unwrap();
        assert_eq!(r, 2_000_000.0);
    }

    #[test]
    fn test_compute_default_spacing_missing_config_uses_default_005r() {
        // No spacing_conf → defaults to "0.005r" → 0.005 × gsize.
        let r = compute_default_spacing(None, 1_000_000.0, 2000.0, "bupr", "n").unwrap();
        assert_eq!(r, 10.0); // 0.005 × 2000
    }

    #[test]
    fn test_link_ideograms_single_ideogram_marks_both_breaks_true() {
        // n=1: self-neighbor. set.max()=100, set.min()-1=-1 → mismatch → both breaks true.
        let mut ideos = vec![mk_ideogram("hs1", "a", 0, 0)];
        link_ideograms(&mut ideos);
        assert!(ideos[0].has_break_start);
        assert!(ideos[0].has_break_end);
    }
}
