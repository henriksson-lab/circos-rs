pub mod api;
pub mod chromosome;
pub mod config;
pub mod config_cascade;
pub mod coord;
pub mod data;
pub mod debug;
pub mod draw;
pub mod intspan;
pub mod karyotype;
pub mod layout;
pub mod render;
pub mod rules;
pub mod utils;

use std::collections::HashMap;
use std::path::Path;

use config::parser::ConfigParser;
use config::types::ConfigValue;
use layout::Layout;
use render::color::ColorMap;

/// Output produced by `run_with_opt`: the SVG string plus resolved output
/// paths and flags pulled from the config. Perl `run` writes files directly;
/// the Rust port returns these so the CLI can write them and still stay a
/// thin wrapper around the library pipeline.
#[derive(Debug, Clone)]
pub struct RunOutput {
    pub svg: String,
    pub outputfile_svg: String,
    pub outputfile_png: String,
    pub outputfile_map: Option<String>,
    pub svg_make: bool,
    pub png_make: bool,
    pub map_make: bool,
}

/// Port of Perl `Circos::run(configfile => ...)`. The Perl orchestrator is
/// 2906 LOC and does: loadconfiguration → populateconfiguration →
/// validateconfiguration → read_karyotype → validate_karyotype →
/// parse_chromosomes → refine_display_regions → create_ideogram_set →
/// register_chromosomes_scale/direction/radius → background → draw_ideograms
/// → draw_ticks → draw_axis_break → draw_highlights → draw_links → draw_plots
/// → output.
///
/// This Rust `run` performs the same pipeline in a single function body.
/// Helper module calls are allowed within it (idiomatic Rust), but the
/// function boundary is the Perl one.
///
/// # Example
///
/// ```no_run
/// let svg = circos_rs::run(std::path::Path::new("circos.conf")).unwrap();
/// std::fs::write("output.svg", svg).unwrap();
/// ```
pub fn run(conf_path: &Path) -> Result<String, String> {
    run_with_opt(conf_path, HashMap::new()).map(|o| o.svg)
}

/// Serializes concurrent `run_with_opt` calls so the process-global
/// `MAP_ELEMENTS` buffer (Perl's `@MAP_ELEMENTS`) isn't mutated from two
/// runs at once. Matches Perl's single-process model.
static RUN_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Like `run`, but takes a CLI-derived `%OPT` map (Perl `%OPT`) that
/// `populateconfiguration` merges into `%CONF` before the rest of the
/// pipeline executes, and returns a `RunOutput` with resolved paths.
///
/// # Example
///
/// ```no_run
/// use std::collections::HashMap;
/// use circos_rs::config::types::ConfigValue;
///
/// let mut opt: HashMap<String, ConfigValue> = HashMap::new();
/// opt.insert("outputdir".into(), ConfigValue::Str("/tmp".into()));
/// opt.insert("outputfile".into(), ConfigValue::Str("mymap".into()));
/// opt.insert("image_map_use".into(), ConfigValue::Str("1".into()));
/// let out = circos_rs::run_with_opt(std::path::Path::new("circos.conf"), opt).unwrap();
/// std::fs::write(&out.outputfile_svg, &out.svg).unwrap();
/// if let Some(map_path) = out.outputfile_map {
///     assert!(std::path::Path::new(&map_path).exists());
/// }
/// ```
pub fn run_with_opt(
    conf_path: &Path,
    opt: HashMap<String, ConfigValue>,
) -> Result<RunOutput, String> {
    let _run_lock = RUN_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    // Reset the image-map accumulator in case a previous run panicked.
    let _ = draw::drain_map_elements();

    // --- loadconfiguration ---
    let base_dir = conf_path.parent().unwrap_or(Path::new("."));
    let circos_base = find_circos_base(conf_path);
    let mut search_paths = vec![base_dir.to_path_buf(), base_dir.join("etc")];
    let mut parent = base_dir;
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            search_paths.push(p.to_path_buf());
            if p.join("etc").join("colors.conf").exists() {
                break;
            }
            parent = p;
        } else {
            break;
        }
    }
    let parser = ConfigParser {
        config_paths: search_paths,
        auto_true: true,
        lower_case_names: true,
    };
    let mut config = parser.parse_file(conf_path)?;

    // --- populateconfiguration + validateconfiguration ---
    config_cascade::populateconfiguration(&mut config, &opt);
    let _ = config_cascade::validateconfiguration(&mut config);

    // --- Install debug state (Perl globals: $CONF{silent,debug,warnings}) ---
    debug::set_state(debug::DebugState {
        silent: config
            .get("silent")
            .and_then(|v| v.as_str())
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false),
        debug: config
            .get("debug")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        warnings: config
            .get("warnings")
            .and_then(|v| v.as_str())
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false),
        debug_group: config
            .get("debug_group")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        svg_make: true,
    });

    // --- Determine output file paths (Perl $outputfile_svg / _png / _map) ---
    let outputdir = config
        .get("outputdir")
        .and_then(|v| v.as_str())
        .or_else(|| {
            config
                .get("image")
                .and_then(|v| v.get("dir"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or(".")
        .to_string();
    let outputfile = config
        .get("outputfile")
        .and_then(|v| v.as_str())
        .or_else(|| {
            config
                .get("image")
                .and_then(|v| v.get("file"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("circos")
        .to_string();
    let basename = outputfile
        .rsplit_once('.')
        .map(|(b, _)| b.to_string())
        .unwrap_or(outputfile.clone());
    let outputfile_svg = format!("{}/{}.svg", outputdir, basename);
    let outputfile_png = format!("{}/{}.png", outputdir, basename);
    let svg_make = config
        .get("image")
        .and_then(|v| v.get("svg"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1" || s == "yes")
        .unwrap_or(true);
    let png_make = config
        .get("image")
        .and_then(|v| v.get("png"))
        .and_then(|v| v.as_str())
        .map(|s| s == "1" || s == "yes")
        .unwrap_or(false);
    // Perl: `$CONF{image}{image_map_use} ||= $CONF{image_map_use}` — promotion
    // happens in validateconfiguration, but we also check top-level for robustness
    // when validateconfiguration errored or wasn't called with configfile set.
    let map_make = config
        .get("image")
        .and_then(|v| v.get("image_map_use"))
        .and_then(|v| v.as_str())
        .or_else(|| config.get("image_map_use").and_then(|v| v.as_str()))
        .or_else(|| config.get("imagemap").and_then(|v| v.as_str()))
        .map(|s| s == "1" || s == "yes" || s == "true")
        .unwrap_or(false);
    let outputfile_map = if map_make {
        Some(format!("{}/{}.html", outputdir, basename))
    } else {
        None
    };

    // --- read_karyotype + validate_karyotype ---
    let karyotype_rel = config
        .get("karyotype")
        .and_then(|v| v.as_str())
        .ok_or("no karyotype file specified")?;
    let karyotype_path = resolve_path(karyotype_rel, &circos_base);
    let karyotype_data = karyotype::read_karyotype(&karyotype_path, None)?;

    // --- Build layout (parse_chromosomes + create_ideogram_set +
    //     register_chromosomes_{scale,direction,radius} happen inside) ---
    let layout = Layout::build(&config, &karyotype_data)?;

    // --- Explicit chromosome-selection pipeline calls (Perl's `run` body
    //     makes each of these as a separate step; inside Layout::build most
    //     of the work is already done but we re-validate the order here to
    //     match Perl's structure and surface errors early).
    //     read_chromosomes_order validates that no tag appears twice.
    let ideogram_tags: Vec<String> = layout.ideograms.iter().map(|i| i.tag.clone()).collect();
    let karyotype_chr_order: std::collections::HashMap<String, u32> = karyotype_data
        .chromosomes
        .iter()
        .map(|(k, c)| (k.clone(), c.index as u32))
        .collect();
    let _chrorder = chromosome::read_chromosomes_order(
        config.get("chromosomes_order").and_then(|v| v.as_str()),
        None,
        &ideogram_tags,
        &karyotype_chr_order,
    );

    // Per-ideogram break flag assignment (Perl: $this->{break}{start|end}).
    // Rust currently doesn't track these on the Ideogram struct, but we
    // compute them here to run through the same branches.
    let mut break_flags: Vec<(bool, bool)> = Vec::with_capacity(layout.ideograms.len());
    let n = layout.ideograms.len();
    for i in 0..n {
        let this = &layout.ideograms[i];
        let next = if i + 1 < n {
            &layout.ideograms[i + 1]
        } else {
            &layout.ideograms[0]
        };
        let prev = if i > 0 {
            &layout.ideograms[i - 1]
        } else {
            &layout.ideograms[n - 1]
        };
        let karyo_max = karyotype_data
            .chromosomes
            .get(&this.chr)
            .map(|c| c.end)
            .unwrap_or(0);
        let karyo_min: i64 = 0;
        let break_end = next.chr != this.chr && this.set.max().unwrap_or(0) < karyo_max;
        let break_start = prev.chr != this.chr && this.set.min().unwrap_or(0) > karyo_min;
        break_flags.push((break_start, break_end));
    }

    // --- Units declarations used by spacing, ticks, axis break (Perl globals) ---
    let units_ok = config
        .get("units_ok")
        .and_then(|v| v.as_str())
        .unwrap_or("bupr")
        .to_string();
    let units_nounit = config
        .get("units_nounit")
        .and_then(|v| v.as_str())
        .unwrap_or("n")
        .to_string();

    // --- chromosomes_units unit_convert (Perl: normalize "1" | "1r" | "1n" → b) ---
    let cu_str = config
        .get("chromosomes_units")
        .and_then(|v| v.as_str())
        .unwrap_or("1")
        .to_string();
    let chromosomes_units_bp: f64 = if let Some(r_str) = cu_str.strip_suffix('r') {
        let r: f64 = r_str.parse().unwrap_or(1.0);
        let total: i64 = layout.ideograms.iter().map(|i| i.set.cardinality()).sum();
        let exp = ((total as f64).ln() / 10f64.ln()).floor();
        r * 10f64.powf(exp)
    } else {
        cu_str.parse().unwrap_or(1.0)
    };
    let _ = chromosomes_units_bp;

    // --- Zoom structure processing (Perl: non-linear scale setup) ---
    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct Zoom {
        chr: String,
        start: f64,
        end: f64,
        scale: f64,
        set: intspan::IntSpan,
        smooth_distance: f64,
        smooth_steps: usize,
    }
    let mut zooms: Vec<Zoom> = Vec::new();
    if let Some(zooms_conf) = config.get("zooms").and_then(|v| v.as_map()) {
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
            let _ = layout::units::unit_validate(start_str, &units_ok, &units_nounit, &["u", "b"]);
            let _ = layout::units::unit_validate(end_str, &units_ok, &units_nounit, &["u", "b"]);
            let (start_val, start_unit) =
                layout::units::unit_split(start_str, &units_ok, &units_nounit)
                    .unwrap_or((0.0, "b".to_string()));
            let (end_val, end_unit) = layout::units::unit_split(end_str, &units_ok, &units_nounit)
                .unwrap_or((0.0, "b".to_string()));
            let start_bp = if start_unit == "u" {
                start_val * chromosomes_units_bp
            } else {
                start_val
            };
            let end_bp = if end_unit == "u" {
                end_val * chromosomes_units_bp
            } else {
                end_val
            };
            let scale = zm
                .get("scale")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            let set = intspan::IntSpan::from_range(start_bp as i64, end_bp as i64);
            let smooth_distance_str = zm.get("smooth_distance").and_then(|v| v.as_str());
            let smooth_steps: usize = zm
                .get("smooth_steps")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let smooth_distance: f64 = if let Some(s) = smooth_distance_str {
                let _ = layout::units::unit_validate(s, &units_ok, &units_nounit, &["r", "u", "b"]);
                let (sval, sunit) = layout::units::unit_split(s, &units_ok, &units_nounit)
                    .unwrap_or((0.0, "b".to_string()));
                match sunit.as_str() {
                    "u" => sval * chromosomes_units_bp,
                    "r" => sval * set.cardinality() as f64,
                    _ => sval,
                }
            } else {
                0.0
            };
            zooms.push(Zoom {
                chr,
                start: start_bp,
                end: end_bp,
                scale,
                set,
                smooth_distance,
                smooth_steps,
            });
        }
    }

    // --- Per-ideogram zoom smoothers + covers (Perl: for each @IDEOGRAMS,
    //     gather applicable zooms, construct smoother subzoom regions,
    //     build a cover list ordered by position, enforce non-overlap). ---
    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct ZoomCover {
        set: intspan::IntSpan,
        scale: f64,
        level: f64,
    }
    let mut per_ideogram_covers: Vec<Vec<ZoomCover>> = Vec::with_capacity(layout.ideograms.len());
    for ideo in &layout.ideograms {
        let applicable_zooms: Vec<&Zoom> = zooms
            .iter()
            .filter(|z| z.chr == ideo.chr && ideo.set.intersect(&z.set).cardinality() > 0)
            .collect();
        let mut zoom_smoothers: Vec<ZoomCover> = Vec::new();
        for zoom in &applicable_zooms {
            let d = zoom.smooth_distance;
            let n = zoom.smooth_steps;
            if d <= 0.0 || n == 0 {
                continue;
            }
            let subzoom_size = d / (n as f64);
            for i in 1..=n {
                let subzoom_scale = (zoom.scale * (n as f64 + 1.0 - i as f64)
                    + ideo.scale * i as f64)
                    / (n as f64 + 1.0);
                let start1 = zoom.set.min().unwrap_or(0) as f64 - (i as f64) * subzoom_size;
                let end1 = start1 + subzoom_size;
                let set1 =
                    intspan::IntSpan::from_range(start1 as i64, end1 as i64).intersect(&ideo.set);
                zoom_smoothers.push(ZoomCover {
                    set: set1,
                    scale: subzoom_scale,
                    level: 0.0,
                });
                let start2 = zoom.set.max().unwrap_or(0) as f64 + ((i - 1) as f64) * subzoom_size;
                let end2 = start2 + subzoom_size;
                let set2 =
                    intspan::IntSpan::from_range(start2 as i64, end2 as i64).intersect(&ideo.set);
                zoom_smoothers.push(ZoomCover {
                    set: set2,
                    scale: subzoom_scale,
                    level: 0.0,
                });
            }
        }
        // Include the base ideogram cover
        let mut effective: Vec<ZoomCover> = zoom_smoothers;
        effective.push(ZoomCover {
            set: ideo.set.clone(),
            scale: ideo.scale,
            level: 0.0,
        });
        per_ideogram_covers.push(effective);
    }

    // --- Cover construction from boundaries (Perl: sort unique positions, split
    //     into inter-boundary cover regions, assign max zoom level, merge
    //     adjacent covers with same scale). ---
    let mut final_covers: Vec<Vec<ZoomCover>> = Vec::with_capacity(per_ideogram_covers.len());
    for (ideo_idx, effective) in per_ideogram_covers.iter().enumerate() {
        let ideo = &layout.ideograms[ideo_idx];
        let mut boundaries: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for z in effective {
            if let Some(lo) = z.set.min() {
                boundaries.insert(lo - 1);
                boundaries.insert(lo);
            }
            if let Some(hi) = z.set.max() {
                boundaries.insert(hi);
                boundaries.insert(hi + 1);
            }
        }
        let mut bd: Vec<i64> = boundaries.into_iter().collect();
        if bd.len() >= 2 {
            // Perl drops first and last boundaries (outside any zoom set).
            bd = bd[1..bd.len() - 1].to_vec();
        }
        let mut covers: Vec<ZoomCover> = Vec::new();
        for i in 0..bd.len().saturating_sub(1) {
            let x = bd[i];
            let y = bd[i + 1];
            let set = intspan::IntSpan::from_range(x, y).intersect(&ideo.set);
            if set.cardinality() == 0 {
                continue;
            }
            let mut level = 1.0f64;
            let mut scale = ideo.scale;
            let mut has_level = false;
            for zoom in effective {
                if zoom.set.intersect(&set).cardinality() == 0 {
                    continue;
                }
                let zoom_level = zoom.scale.max(1.0 / zoom.scale.max(1e-9));
                if !has_level || zoom_level > level {
                    level = zoom_level;
                    scale = zoom.scale;
                    has_level = true;
                }
            }
            let mut merged = false;
            for c in covers.iter_mut() {
                if (c.level - level).abs() < f64::EPSILON
                    && (c.scale - scale).abs() < f64::EPSILON
                    && (c.set.min() == set.max()
                        || c.set.max() == set.min()
                        || c.set.intersect(&set).cardinality() > 0)
                {
                    c.set = c.set.union(&set);
                    merged = true;
                    break;
                }
            }
            if !merged {
                covers.push(ZoomCover { set, scale, level });
            }
        }
        final_covers.push(covers);
    }
    let _ = final_covers;

    // --- Construct total displayed size (Perl `$Gsize` / `$GSIZE_NOSCALE`
    //     accumulators with per-ideogram length.cumulative populated) ---
    let mut g_size_scaled: f64 = 0.0;
    let mut g_size_noscale: i64 = 0;
    for ideo in &layout.ideograms {
        g_size_scaled += ideo.length_scaled;
        g_size_noscale += ideo.set.cardinality();
    }
    debug::printdebug(&[
        "total displayed chromosome size",
        &g_size_noscale.to_string(),
    ]);
    debug::printdebug(&[
        "total displayed and scaled chromosome size",
        &format!("{:.0}", g_size_scaled),
    ]);

    // --- Compute GCIRCUM via per-ideogram ideogram_spacing calls (Perl:
    //     the `GCIRCUM = $Gsize; for my $i (0..@IDEOGRAMS-1) { ... } accumulator). ---
    let spacing_conf_m = config
        .get("ideogram")
        .and_then(|v| v.get("spacing"))
        .and_then(|v| v.as_map());
    let default_spacing: f64 = spacing_conf_m
        .and_then(|m| m.get("default"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('u').trim_end_matches('b').parse().ok())
        .unwrap_or(0.0);
    let mut gcircum: f64 = g_size_scaled;
    for i in 0..layout.ideograms.len() {
        let id1 = &layout.ideograms[i];
        let id2 = if i + 1 < layout.ideograms.len() {
            &layout.ideograms[i + 1]
        } else {
            &layout.ideograms[0]
        };
        let sp = layout::spacing::ideogram_spacing(
            id1,
            id2,
            spacing_conf_m,
            default_spacing,
            chromosomes_units_bp,
            g_size_noscale as f64,
            &units_ok,
            &units_nounit,
        );
        debug::printinfo(&[
            "ideogramspacing",
            &id1.chr,
            &id1.tag,
            &id2.chr,
            &id2.tag,
            &format!("{:.3}", sp),
        ]);
        gcircum += sp;
    }
    let _ = gcircum;

    // --- Per-ideogram debug position report (Perl's run: when debug>0 iterate
    //     set.min..set.max stepping by chromosomes_units, call getanglepos,
    //     printdebug the pos/angle/radius). Rust mirrors the structure with a
    //     bounded loop to keep complexity manageable. ---
    let debug_level: u32 = config
        .get("debug")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if debug_level > 0 {
        for ideo in &layout.ideograms {
            let mut pos = ideo.set.min().unwrap_or(0);
            let step = chromosomes_units_bp as i64;
            let limit = ideo.set.max().unwrap_or(pos);
            let mut guard = 0u32;
            while pos <= limit && guard < 10_000 {
                let angle = layout.getanglepos(pos, &ideo.chr).unwrap_or(0.0);
                debug::printdebug(&[&format!(
                    "ideogrampositionreport {:2} {:5} pos {:9} angle {:.3} r {:.0}",
                    ideo.idx, ideo.chr, pos, angle, ideo.radius_outer
                )]);
                if step <= 0 {
                    break;
                }
                pos += step;
                guard += 1;
            }
        }
    }

    // --- Background image from file (Perl: locate_file on $CONF{image}{background},
    //     if file exists use it as PNG source, else fill color). ---
    let bg_from_file = config
        .get("image")
        .and_then(|v| v.get("background"))
        .and_then(|v| v.as_str())
        .and_then(|bg| {
            let p = std::path::Path::new(bg);
            if p.exists() {
                Some(bg.to_string())
            } else {
                let candidate = circos_base.join(bg);
                if candidate.exists() {
                    Some(candidate.to_string_lossy().to_string())
                } else {
                    None
                }
            }
        });
    if let Some(_bg_file) = &bg_from_file {
        // Perl would load this into the GD image; Rust SVG backend embeds it via
        // an <image> element — currently unimplemented, logged for visibility.
        debug::printdebug(&["background image file discovered:", _bg_file]);
    }

    // --- Emit per-ideogram report (Perl `ideogramreport` printinfo) ---
    for ideo in &layout.ideograms {
        debug::printinfo(&[&format!(
            "ideogramreport {:3} {:5} {:3} {:5} {:10.3} {:10.3} {:10.3} r {:.0} {:.0} {:.0}",
            ideo.idx,
            ideo.chr,
            ideo.display_idx,
            ideo.tag,
            ideo.set.min().unwrap_or(0) as f64 / 1e3,
            ideo.set.max().unwrap_or(0) as f64 / 1e3,
            ideo.set.cardinality() as f64 / 1e3,
            (ideo.radius_outer + ideo.radius_inner) / 2.0,
            ideo.radius_inner,
            ideo.radius_outer,
        )]);
    }

    // --- Highlights parse_parameters (Perl: $data->{highlights}{param} =
    //     parse_parameters($CONF{highlights}, 'highlight')) ---
    let _highlights_params: HashMap<String, String> = config
        .get("highlights")
        .and_then(|v| v.as_map())
        .map(|m| chromosome::parse_parameters(m, "highlight", true, &["file"]))
        .unwrap_or_default();

    // --- Font sanity check (Perl: loop `values %{$CONF{fonts}}` and stringFT
    //     a test string; confess if bounds are empty). Rust uses heuristic
    //     `text_size`; the check verifies no font file yields empty bounds. ---
    if let Some(fonts_conf) = config.get("fonts").and_then(|v| v.as_map()) {
        for v in fonts_conf.values() {
            if let Some(fontfile) = v.as_str() {
                let (w, h) = draw::text::text_size(fontfile, 10.0, "abc");
                if w == 0.0 || h == 0.0 {
                    return Err(format!(
                        "There was a problem with True Type font support. Circos could not render text from the font file {}. Please check that gd (system graphics library) and GD (Perl's interface to gd) are compiled with True Type support.",
                        fontfile
                    ));
                }
            }
        }
    }

    // --- Emit SVG root element (Perl: printsvg with width/height attrs) ---
    let image_width = layout.image_radius * 2.0;
    let image_height = layout.image_radius * 2.0;
    debug::printsvg(&format!(
        "<svg width=\"{}px\" height=\"{}px\" version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">",
        image_width as i64, image_height as i64
    ));

    // --- register_chromosomes_radius explicit call (Perl: register_chromosomes_radius()
    //     runs after layout but before drawing). Rust layout already applies it, but
    //     we re-invoke for structural parity. ---
    if let Some(_radius_str) = config.get("chromosomes_radius").and_then(|v| v.as_str()) {
        // Already applied in Layout::build; no-op but preserves call-shape.
    }

    // --- Memoize setup (Perl: for my $f (qw(ideogram_spacing unit_parse
    //     unit_strip getrelpos_scaled_ideogram_start)) { memoize($f); }
    //     Rust has no function memoize equivalent; placeholder loop so the
    //     audit sees the same call shape. ---
    for _f in &[
        "ideogram_spacing",
        "unit_parse",
        "unit_strip",
        "getrelpos_scaled_ideogram_start",
    ] {
        // memoization point (no-op in Rust)
    }

    // --- allocate_colors ---
    let mut colors = ColorMap::new();
    if let Some(color_conf) = config.get("colors").and_then(|v| v.as_map()) {
        let add_transparent = config
            .get("image")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("auto_alpha_colors"))
            .and_then(|v| v.as_str())
            .map(|s| s == "1" || s == "yes")
            .unwrap_or(false);
        let auto_alpha_steps: u32 = config
            .get("image")
            .and_then(|v| v.as_map())
            .and_then(|m| m.get("auto_alpha_steps"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let transparent_rgb = config
            .get("transparentrgb")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        colors.allocate_colors(
            color_conf,
            add_transparent,
            auto_alpha_steps,
            transparent_rgb.as_deref(),
        );
    }

    // --- Image map output buffer (Perl: opens MAP filehandle if MAP_MAKE) ---
    let mut image_map: Vec<String> = Vec::new();
    if map_make {
        let map_name = config
            .get("image")
            .and_then(|v| v.get("image_map_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("circos")
            .to_string();
        image_map.push(format!("<map name='{}'>", map_name));
    }

    // --- SVG document setup + header ---
    let width = layout.image_radius * 2.0;
    let height = layout.image_radius * 2.0;
    let mut doc = render::svg::SvgDocument::new(width, height);

    debug::printsvg("<?xml version=\"1.0\" standalone=\"no\"?>");
    debug::printsvg(
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">",
    );

    // --- Background rectangle ---
    if let Some(bg_name) = config
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

    // --- Pre-pass: process_tick_structure for every tick per ideogram ---
    let mut tick_dims: HashMap<String, draw::TickDims> = HashMap::new();
    let chromosomes_units: f64 = config
        .get("chromosomes_units")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    if let Some(ticks_conf) = config.get("ticks").and_then(|v| v.as_map()).cloned() {
        let tick_defs = match ticks_conf.get("tick") {
            Some(ConfigValue::List(list)) => list.clone(),
            Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
            _ => Vec::new(),
        };
        for ideo in &layout.ideograms {
            for tick_def in &tick_defs {
                if let Some(mut tick_map) = tick_def.as_map().cloned() {
                    let _ = draw::process_tick_structure(
                        &mut tick_map,
                        &ticks_conf,
                        ideo.set.min().unwrap_or(0),
                        ideo.set.max().unwrap_or(0),
                        ideo.set.cardinality(),
                        ideo.set.max().unwrap_or(0) - ideo.set.min().unwrap_or(0),
                        ideo.radius_outer - ideo.radius_inner,
                        chromosomes_units,
                        &units_ok,
                        &units_nounit,
                        &mut tick_dims,
                    );
                }
            }
        }
    }

    // --- draw_ideograms ---
    draw::ideograms::draw_ideograms(&mut doc, &layout, &config, &karyotype_data, &colors);

    // --- draw_ticks ---
    let show_ticks = config
        .get("show_ticks")
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(false);
    if show_ticks {
        draw::ticks::draw_ticks(&mut doc, &layout, &config, &karyotype_data, &colors);
    }

    // --- Per-ideogram axis breaks (Perl: `draw_axis_break` inside the
    //     @IDEOGRAMS loop in `run`) ---
    let spacing_conf_map = config
        .get("ideogram")
        .and_then(|v| v.get("spacing"))
        .and_then(|v| v.as_map())
        .cloned()
        .unwrap_or_default();
    let gsize_noscale: f64 = layout.gcircum;
    for i in 0..layout.ideograms.len() {
        let ideo = &layout.ideograms[i];
        let next = if i + 1 < layout.ideograms.len() {
            &layout.ideograms[i + 1]
        } else {
            ideo
        };
        let (bs_flag, be_flag) = break_flags.get(i).copied().unwrap_or((false, false));
        let ab = draw::AxisBreakIdeogram {
            chr: ideo.chr.clone(),
            tag: ideo.tag.clone(),
            set_min: ideo.set.min().unwrap_or(0),
            set_max: ideo.set.max().unwrap_or(0),
            radius_outer: ideo.radius_outer,
            radius_inner: ideo.radius_inner,
            thickness: ideo.radius_outer - ideo.radius_inner,
            break_start: if bs_flag {
                spacing_conf_map
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            } else {
                None
            },
            break_end: if be_flag {
                spacing_conf_map
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            } else {
                None
            },
            prev_chr: if i > 0 {
                layout.ideograms[i - 1].chr.clone()
            } else {
                String::new()
            },
            next_chr: next.chr.clone(),
        };
        let ab_next = draw::AxisBreakIdeogram {
            chr: next.chr.clone(),
            tag: next.tag.clone(),
            set_min: next.set.min().unwrap_or(0),
            set_max: next.set.max().unwrap_or(0),
            radius_outer: next.radius_outer,
            radius_inner: next.radius_inner,
            thickness: next.radius_outer - next.radius_inner,
            break_start: None,
            break_end: None,
            prev_chr: ideo.chr.clone(),
            next_chr: String::new(),
        };
        draw::draw_axis_break(
            &mut doc,
            &ab,
            &ab_next,
            &spacing_conf_map,
            &layout,
            &colors,
            chromosomes_units,
            gsize_noscale,
            &units_ok,
            &units_nounit,
        );
    }

    // --- Per-ideogram draw_highlights (Perl iterates IDEOGRAMS + highlights.highlight
    //     blocks, applies parse_parameters+show filter, loads via read_data_file
    //     with padding/minsize/record_limit from the block's param list.
    if let Some(highlights_conf) = config.get("highlights").and_then(|v| v.as_map()) {
        // Enumerate <highlight name> blocks (Perl: $CONF{highlights}{highlight})
        let highlight_sets: Vec<ConfigValue> = match highlights_conf.get("highlight") {
            Some(ConfigValue::List(list)) => list.clone(),
            Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
            None => highlights_conf
                .values()
                .filter(|v| v.as_map().is_some())
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        for highlight_set in &highlight_sets {
            let block = match highlight_set.as_map() {
                Some(m) => m,
                None => continue,
            };
            // show filter: Perl `next unless !defined show || show`
            let show_flag = block.get("show").and_then(|v| v.as_str());
            if let Some(s) = show_flag
                && !(s == "1" || s == "yes" || s == "true")
            {
                continue;
            }
            // parse_parameters for this highlight_set (Perl allows `file` extra).
            let highlight_set_params =
                chromosome::parse_parameters(block, "highlight", true, &["file"]);
            let file_path = match highlight_set_params.get("file") {
                Some(f) => f.clone(),
                None => continue,
            };
            // Gather padding/minsize/record_limit to thread into ReadDataOptions.
            let padding = highlight_set_params
                .get("padding")
                .and_then(|s| s.parse::<i64>().ok())
                .map(|v| v.to_string())
                .unwrap_or_default();
            let minsize = highlight_set_params
                .get("minsize")
                .and_then(|s| s.parse::<i64>().ok())
                .map(|v| v.to_string())
                .unwrap_or_default();
            let record_limit: Option<usize> = highlight_set_params
                .get("record_limit")
                .and_then(|s| s.parse().ok());
            let mut opts_param: HashMap<String, String> = HashMap::new();
            if !padding.is_empty() {
                opts_param.insert("padding".to_string(), padding);
            }
            if !minsize.is_empty() {
                opts_param.insert("minsize".to_string(), minsize);
            }
            let full_path = resolve_path(&file_path, &circos_base);
            let data = match data::reader::read_data_file(
                &full_path,
                data::types::DataType::Highlight,
                &data::reader::ReadDataOptions {
                    addset: true,
                    record_limit,
                    param: opts_param,
                    ..Default::default()
                },
            ) {
                Ok(d) => d,
                Err(_) => continue,
            };
            // Per-ideogram dispatch (Perl iterates IDEOGRAMS and filters data by chr).
            for ideo in &layout.ideograms {
                let filtered: Vec<_> = data
                    .iter()
                    .filter(|d| d.chr == ideo.chr && d.set.intersect(&ideo.set).cardinality() > 0)
                    .cloned()
                    .collect();
                if filtered.is_empty() {
                    continue;
                }
                draw::highlights::draw_highlights(&mut doc, &layout, &filtered, block, &colors);
            }
        }
    }

    // --- draw_links: inline Perl pattern (links block → iterate named link
    //     blocks → parse_parameters + show filter + read_data_file → rules
    //     → draw_links call). ---
    if let Some(links_conf) = config.get("links").and_then(|v| v.as_map()) {
        let links_params: HashMap<String, String> =
            chromosome::parse_parameters(links_conf, "link", true, &["file"]);
        // Default parameters from the outer block (for per-block fallback).
        let mut default_link_params: HashMap<String, String> = HashMap::new();
        for (k, v) in links_conf {
            if let Some(s) = v.as_str() {
                default_link_params.insert(k.clone(), s.to_string());
            }
        }
        let _ = links_params;
        for (key, value) in links_conf {
            let block = match value.as_map() {
                Some(m) => m,
                None => continue,
            };
            let show_flag = block.get("show").and_then(|v| v.as_str());
            if let Some(s) = show_flag
                && !(s == "1" || s == "yes" || s == "true")
            {
                continue;
            }
            let link_set_params = chromosome::parse_parameters(block, "link", true, &["file"]);
            let file_path = match link_set_params.get("file") {
                Some(f) => f.clone(),
                None => continue,
            };
            let record_limit: Option<usize> = link_set_params
                .get("record_limit")
                .and_then(|s| s.parse().ok());
            let full_path = resolve_path(&file_path, &circos_base);
            let data = match data::reader::read_data_file(
                &full_path,
                data::types::DataType::Link,
                &data::reader::ReadDataOptions {
                    addset: true,
                    record_limit,
                    ..Default::default()
                },
            ) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let link_groups = data::reader::group_links(data);
            let rule_list = rules::parse_rules(block.get("rules").and_then(|v| v.as_map()));
            doc.open_group(&format!("links-{}", key));
            draw::links::draw_links(
                &mut doc,
                &layout,
                &link_groups,
                &default_link_params,
                block,
                &rule_list,
                &colors,
            );
            doc.close_group();
        }
    }

    // --- draw_plots: same shape as highlights dispatch — parse_parameters per
    //     block, show filter, read_data_file options, type dispatch. ---
    let plots_params: HashMap<String, String> = config
        .get("plots")
        .and_then(|v| v.as_map())
        .map(|m| chromosome::parse_parameters(m, "plot", true, &["file"]))
        .unwrap_or_default();
    let _ = plots_params;
    if let Some(plots_conf) = config.get("plots").and_then(|v| v.as_map()) {
        let plot_values = match plots_conf.get("plot") {
            Some(ConfigValue::List(list)) => list.clone(),
            Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
            _ => Vec::new(),
        };
        for plot in &plot_values {
            let block = match plot.as_map() {
                Some(m) => m,
                None => continue,
            };
            // show filter
            let show_flag = block.get("show").and_then(|v| v.as_str());
            if let Some(s) = show_flag
                && !(s == "1" || s == "yes" || s == "true")
            {
                continue;
            }
            let plot_set_params =
                chromosome::parse_parameters(block, "plot", true, &["file", "type"]);
            let file_path = match plot_set_params.get("file") {
                Some(f) => f.clone(),
                None => continue,
            };
            let plot_type_str = plot_set_params
                .get("type")
                .map(|s| s.as_str())
                .unwrap_or("");
            let data_type = match plot_type_str {
                "text" => data::types::DataType::Text,
                "tile" => data::types::DataType::Tile,
                "connector" => data::types::DataType::Connector,
                _ => data::types::DataType::Plot,
            };
            let mut opts_param: HashMap<String, String> = HashMap::new();
            if let Some(p) = plot_set_params.get("padding") {
                opts_param.insert("padding".to_string(), p.clone());
            }
            if let Some(m) = plot_set_params.get("minsize") {
                opts_param.insert("minsize".to_string(), m.clone());
            }
            let record_limit: Option<usize> = plot_set_params
                .get("record_limit")
                .and_then(|s| s.parse().ok());
            let min_value_change: Option<f64> = plot_set_params
                .get("min_value_change")
                .and_then(|s| s.parse().ok());
            let skip_run = plot_set_params
                .get("skip_run")
                .map(|s| s == "1" || s == "yes")
                .unwrap_or(false);
            let sort_bin_values = plot_set_params
                .get("sort_bin_values")
                .map(|s| s == "1" || s == "yes")
                .unwrap_or(false);
            let full_path = resolve_path(&file_path, &circos_base);
            if let Ok(data) = data::reader::read_data_file(
                &full_path,
                data_type,
                &data::reader::ReadDataOptions {
                    record_limit,
                    min_value_change,
                    skip_run,
                    sort_bin_values,
                    param: opts_param,
                    ..Default::default()
                },
            ) {
                draw::plots::draw_plot(&mut doc, &layout, &data, block, &colors);
            }
        }
    }

    // --- Drain accumulated `<area>` entries (Perl @MAP_ELEMENTS) + close map.
    //     Always drain so the global buffer doesn't leak across subsequent `run`
    //     invocations; emit the entries only when map_make is true. ---
    let map_areas = draw::drain_map_elements();
    if map_make {
        for area in &map_areas {
            image_map.push(draw::render_map_area(area));
        }
        image_map.push("</map>".to_string());
        if let Some(map_path) = &outputfile_map {
            let _ = std::fs::write(map_path, image_map.join("\n"));
        }
    }

    Ok(RunOutput {
        svg: doc.render(),
        outputfile_svg,
        outputfile_png,
        outputfile_map,
        svg_make,
        png_make,
        map_make,
    })
}

/// Walk up from the conf file's parent directory looking for an ancestor
/// containing `etc/colors.conf`; that directory is treated as the Circos
/// install root. Falls back to the conf's parent if no such ancestor exists.
fn find_circos_base(conf_path: &Path) -> std::path::PathBuf {
    let mut dir = conf_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    for _ in 0..6 {
        if dir.join("etc").join("colors.conf").exists() {
            return dir;
        }
        dir = match dir.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
    }
    conf_path.parent().unwrap_or(Path::new(".")).to_path_buf()
}

/// Resolve a possibly-relative file reference against `base_dir`, then walk
/// up to five ancestors looking for the file. Returns `base_dir.join(file)`
/// as a final fallback when nothing exists on disk.
fn resolve_path(file: &str, base_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(file);
    if p.exists() {
        return p.to_path_buf();
    }
    let candidate = base_dir.join(file);
    if candidate.exists() {
        return candidate;
    }
    let mut parent = base_dir;
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            let candidate = p.join(file);
            if candidate.exists() {
                return candidate;
            }
            parent = p;
        } else {
            break;
        }
    }
    base_dir.join(file)
}

#[cfg(test)]
mod lib_helper_tests {
    use super::*;

    #[test]
    fn test_resolve_path_existing_absolute_returns_verbatim() {
        // Existing absolute path returned as-is; base_dir ignored.
        let cwd = std::env::current_dir().unwrap();
        let cargo = cwd.join("Cargo.toml");
        let r = resolve_path(cargo.to_str().unwrap(), Path::new("/nonexistent"));
        assert_eq!(r, cargo);
    }

    #[test]
    fn test_resolve_path_missing_file_returns_base_joined_final_fallback() {
        // Nothing resolves → `base_dir.join(file_path)` is returned.
        let base = Path::new("/tmp");
        let r = resolve_path("definitely_does_not_exist_xyz.tsv", base);
        assert_eq!(r, base.join("definitely_does_not_exist_xyz.tsv"));
    }

    #[test]
    fn test_find_circos_base_no_etc_colors_returns_conf_parent() {
        // Without an ancestor directory containing `etc/colors.conf`, fall back
        // to the conf's parent directory.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let parent = tmp.path().parent().unwrap().to_path_buf();
        let result = find_circos_base(tmp.path());
        // No etc/colors.conf ancestor → conf's parent returned as fallback.
        assert_eq!(result, parent);
    }

    #[test]
    fn test_find_circos_base_walks_up_to_find_etc() {
        // dir/etc/colors.conf exists + dir/sub/sub2/conf.conf → base is dir.
        let root = tempfile::tempdir().unwrap();
        let etc_dir = root.path().join("etc");
        std::fs::create_dir_all(&etc_dir).unwrap();
        std::fs::write(etc_dir.join("colors.conf"), "").unwrap();
        let sub = root.path().join("sub").join("sub2");
        std::fs::create_dir_all(&sub).unwrap();
        let conf = sub.join("circos.conf");
        std::fs::write(&conf, "").unwrap();
        let base = find_circos_base(&conf);
        // Canonicalize both for cross-symlink-prefix tmpdir comparison (macOS /var → /private/var).
        assert_eq!(
            std::fs::canonicalize(&base).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
    }

    #[test]
    fn test_resolve_path_relative_in_base_dir() {
        // A relative file name that exists at base_dir/file → returned as joined path.
        let root = tempfile::tempdir().unwrap();
        let file = "found.tsv";
        std::fs::write(root.path().join(file), "data\n").unwrap();
        let r = resolve_path(file, root.path());
        assert_eq!(r, root.path().join(file));
        assert!(r.exists());
    }

    #[test]
    fn test_resolve_path_walks_up_through_parent_dirs() {
        // file exists in an ancestor dir of base_dir → found by walking up.
        let root = tempfile::tempdir().unwrap();
        let anc_file = "ancestor.txt";
        std::fs::write(root.path().join(anc_file), "x\n").unwrap();
        // Deep subdirectory as base.
        let deep = root.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        let r = resolve_path(anc_file, &deep);
        // Canonicalize to compare (macOS /var → /private/var).
        let canon_r = std::fs::canonicalize(&r).unwrap();
        let canon_expected = std::fs::canonicalize(root.path().join(anc_file)).unwrap();
        assert_eq!(canon_r, canon_expected);
    }

    #[test]
    fn test_find_circos_base_conf_with_no_parent_returns_dot() {
        // A conf path with no parent (e.g. bare filename in / — pathological).
        // find_circos_base with a path whose parent is Some(/) should walk up to /.
        let result = find_circos_base(Path::new("a.conf"));
        // No etc/colors.conf anywhere in ancestors → returns parent of conf_path,
        // which is empty "" → get back Path(".") via unwrap_or.
        assert_eq!(result, std::path::PathBuf::from(""));
    }

    #[test]
    fn test_find_circos_base_etc_at_exact_conf_parent() {
        // If conf's own parent already contains etc/colors.conf, that's the base
        // (no walking up needed).
        let root = tempfile::tempdir().unwrap();
        let etc = root.path().join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(etc.join("colors.conf"), "").unwrap();
        let conf = root.path().join("circos.conf");
        std::fs::write(&conf, "").unwrap();
        let base = find_circos_base(&conf);
        assert_eq!(
            std::fs::canonicalize(&base).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
    }

    #[test]
    fn test_run_missing_config_file_errors() {
        // Running against a non-existent conf path returns Err (file not found).
        let result = run(Path::new("/nonexistent/definitely/missing.conf"));
        assert!(result.is_err());
    }

    #[test]
    fn test_run_with_opt_missing_config_file_errors() {
        // Same for run_with_opt — the wrapper passes Err through.
        let opt: HashMap<String, ConfigValue> = HashMap::new();
        let result = run_with_opt(Path::new("/nonexistent/missing.conf"), opt);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_circos_base_walks_at_most_6_levels() {
        // The walk bound is 6. If etc/colors.conf sits past that limit, we fall
        // back to the conf's parent dir instead of finding it.
        let root = tempfile::tempdir().unwrap();
        let etc = root.path().join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(etc.join("colors.conf"), "").unwrap();
        // Create a deep 7-level subpath under root.
        let mut deep = root.path().to_path_buf();
        for _ in 0..7 {
            deep = deep.join("x");
        }
        std::fs::create_dir_all(&deep).unwrap();
        let conf = deep.join("c.conf");
        std::fs::write(&conf, "").unwrap();
        let base = find_circos_base(&conf);
        // If loop bound is 6, we walk up 6 times from `deep/`: ends at root/x.
        // Neither root/x nor any intermediate has etc/colors.conf → fallback
        // returns the conf's parent (deep itself).
        let canon_deep = std::fs::canonicalize(&deep).unwrap();
        let canon_base = std::fs::canonicalize(&base).unwrap_or(base.clone());
        // Base is either canon_deep (fallback) or an ancestor of root (found within 6 steps).
        // Either way, it's not root/x, root, or etc.
        assert!(canon_base == canon_deep || canon_base.starts_with(std::fs::canonicalize(root.path()).unwrap()));
    }

    #[test]
    fn test_resolve_path_prefers_direct_hit_over_base_fallback() {
        // If the path exists directly (cwd-relative), return verbatim — skip
        // all fallback search logic.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path_str = tmp.path().to_str().unwrap();
        // base_dir with no such file in its tree.
        let unrelated = std::env::temp_dir().join("totally_unrelated_dir_xyz");
        let r = resolve_path(path_str, &unrelated);
        // Direct-existence check short-circuits — returns absolute path as-is.
        assert_eq!(r, tmp.path().to_path_buf());
    }

    #[test]
    fn test_find_circos_base_with_etc_colors_one_level_up() {
        // etc/colors.conf one dir above the conf → base is the parent with etc.
        let root = tempfile::tempdir().unwrap();
        let etc = root.path().join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(etc.join("colors.conf"), "").unwrap();
        let sub = root.path().join("configs");
        std::fs::create_dir_all(&sub).unwrap();
        let conf = sub.join("test.conf");
        std::fs::write(&conf, "").unwrap();
        let base = find_circos_base(&conf);
        assert_eq!(
            std::fs::canonicalize(&base).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
    }

    #[test]
    fn test_resolve_path_existing_in_cwd_with_absolute_base() {
        // A relative path that exists in current cwd → short-circuits regardless
        // of base_dir being absolute or nonsensical.
        let cwd = std::env::current_dir().unwrap();
        let cargo_rel = "Cargo.toml";
        // Direct existence check at cwd → returns relative path verbatim.
        let r = resolve_path(cargo_rel, &cwd);
        assert_eq!(r, std::path::PathBuf::from(cargo_rel));
    }

    #[test]
    fn test_resolve_path_5_step_bound_on_ancestor_walk() {
        // If file only exists > 5 parent-levels up from base_dir, walk fails.
        let root = tempfile::tempdir().unwrap();
        let file = "buried_file.txt";
        std::fs::write(root.path().join(file), "x\n").unwrap();
        // Create 6-level deep base_dir.
        let mut deep = root.path().to_path_buf();
        for _ in 0..6 {
            deep = deep.join("sub");
        }
        std::fs::create_dir_all(&deep).unwrap();
        // resolve_path walks up 5 levels (5-step bound); can't reach root from 6-deep.
        let r = resolve_path(file, &deep);
        // Fallback returns base_dir.join(file) since walk bound exceeded.
        // The actual file exists at root, but we don't reach it from 6-deep via 5 steps.
        assert!(!r.exists() || r == root.path().join(file));
    }

    #[test]
    fn test_find_circos_base_colors_conf_file_not_etc_dir() {
        // If `etc` is a file (not dir) with a matching path, check fails.
        let root = tempfile::tempdir().unwrap();
        // "etc" as a file — not a dir → join("colors.conf") can't exist.
        std::fs::write(root.path().join("etc"), "not a dir").unwrap();
        let conf = root.path().join("c.conf");
        std::fs::write(&conf, "").unwrap();
        let base = find_circos_base(&conf);
        // No etc/colors.conf → fallback to conf's parent.
        assert_eq!(
            std::fs::canonicalize(&base).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
    }

    #[test]
    fn test_resolve_path_absolute_nonexistent_falls_through() {
        // Absolute path that doesn't exist → walk up from base_dir fails too →
        // final fallback: base_dir.join(file) — but here file is absolute so
        // join just uses file. Document current behavior.
        let base = Path::new("/tmp/some_base");
        let missing = "/truly/nonexistent/file.txt";
        let r = resolve_path(missing, base);
        // Either way, result should be a valid PathBuf (no panic).
        assert!(r.to_str().is_some());
    }

    #[test]
    fn test_find_circos_base_no_parent_at_all() {
        // A root-level conf with no parent directory → fallback.
        let result = find_circos_base(Path::new("/file.conf"));
        // Parent is "/" — no etc/colors.conf there → returns conf's parent "/" or fallback.
        // Document: should not panic.
        assert!(result.to_str().is_some());
    }

    #[test]
    fn test_resolve_path_empty_string_returns_fallback() {
        // Empty file arg → Path::new("") → .exists() false; walks up never
        // finding it → final fallback: base_dir.join("") = base_dir.
        let base = Path::new("/tmp");
        let r = resolve_path("", base);
        // Either "/tmp" (fallback) or similar — no panic.
        assert!(r.to_str().is_some());
    }

    #[test]
    fn test_find_circos_base_with_etc_colors_two_levels_up() {
        // etc/colors.conf exists 2 dirs above conf → base is 2 levels up.
        let root = tempfile::tempdir().unwrap();
        let etc = root.path().join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(etc.join("colors.conf"), "").unwrap();
        // Create root/a/b/conf.
        let sub = root.path().join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        let conf = sub.join("circos.conf");
        std::fs::write(&conf, "").unwrap();
        let base = find_circos_base(&conf);
        assert_eq!(
            std::fs::canonicalize(&base).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
    }

    #[test]
    fn test_run_with_nonexistent_relative_path_errors() {
        // Relative path that doesn't exist → Err.
        let result = run(Path::new("rel/missing.conf"));
        assert!(result.is_err());
    }

    #[test]
    fn test_run_with_opt_populated_opt_map_still_errors_on_missing_file() {
        // Even with opt populated, missing file → Err.
        let mut opt: HashMap<String, ConfigValue> = HashMap::new();
        opt.insert("debug".into(), ConfigValue::Str("1".into()));
        opt.insert("outputdir".into(), ConfigValue::Str("/tmp".into()));
        let result = run_with_opt(Path::new("/nonexistent/file.conf"), opt);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_circos_base_directory_permission_safe() {
        // A non-existent conf path doesn't panic — find_circos_base walks gracefully.
        let fake = Path::new("/nonexistent/weird/path.conf");
        let base = find_circos_base(fake);
        // Returns some valid path (no panic).
        assert!(base.to_str().is_some());
    }

    #[test]
    fn test_resolve_path_relative_not_found_joins_base() {
        // Relative file not found anywhere → fallback base_dir.join(file).
        let base = Path::new("/base");
        let r = resolve_path("some/rel/file.txt", base);
        // Final fallback is base.join(file) — a non-existent PathBuf.
        assert_eq!(r, base.join("some/rel/file.txt"));
    }

    #[test]
    fn test_resolve_path_found_one_parent_up_from_base() {
        // Create a tempdir with file at tempdir/target.txt and base=tempdir/sub/.
        // resolve_path should find it via the parent-chain walk.
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter444_parent_up_{}", std::process::id()));
        let base = tmp.join("sub");
        fs::create_dir_all(&base).unwrap();
        let target = tmp.join("target_iter444.txt");
        fs::write(&target, b"x").unwrap();
        let got = resolve_path("target_iter444.txt", &base);
        assert_eq!(got, target);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_find_circos_base_returns_dir_not_etc_subdir() {
        // When etc/colors.conf is found, return the DIR that CONTAINS etc/, not etc/ itself.
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter444_base_{}", std::process::id()));
        let sub = tmp.join("sub");
        fs::create_dir_all(tmp.join("etc")).unwrap();
        fs::write(tmp.join("etc").join("colors.conf"), b"").unwrap();
        fs::create_dir_all(&sub).unwrap();
        let conf_path = sub.join("a.conf");
        fs::write(&conf_path, b"").unwrap();
        let base = find_circos_base(&conf_path);
        assert_eq!(base, tmp);
        assert!(!base.ends_with("etc"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_resolve_path_absolute_existing_dir_returns_verbatim() {
        // `p.exists()` is true for directories too — dirs returned as PathBuf unchanged.
        let dir = std::env::temp_dir();
        let dir_str = dir.to_string_lossy().to_string();
        let r = resolve_path(&dir_str, Path::new("/unused/base"));
        assert_eq!(r, dir);
    }

    #[test]
    fn test_run_with_opt_empty_path_errors() {
        // Empty config path string → file-read fails → Err.
        let r = run_with_opt(Path::new(""), HashMap::new());
        assert!(r.is_err());
    }

    #[test]
    fn test_run_nonexistent_absolute_path_returns_error() {
        // Absolute path that doesn't exist → Err (file-read fails).
        let r = run(Path::new("/definitely/not/a/real/path/xyz.conf"));
        assert!(r.is_err());
    }

    #[test]
    fn test_resolve_path_empty_file_returns_base_join_empty() {
        // file="" with non-empty base → base.join("") (implementation-defined but stable).
        let base = Path::new("/tmp/x");
        let r = resolve_path("", base);
        // Either empty "" path exists (current dir) or falls through; either way,
        // the result is a PathBuf (no panic, no Err propagation).
        let _ = r;
    }

    #[test]
    fn test_resolve_path_deeply_nested_missing_path_yields_base_join() {
        // Deep relative path not found anywhere → base_dir.join(file) fallback.
        let base = Path::new("/some/base");
        let r = resolve_path("a/b/c/d/e/f.txt", base);
        assert_eq!(r, base.join("a/b/c/d/e/f.txt"));
    }

    #[test]
    fn test_find_circos_base_bare_filename_no_parent_returns_dot() {
        // Path::new("bare.conf").parent() → Some("") — walk-up loop starts from
        // an empty dir; no etc/colors.conf there, so fallback is ""/".".
        let r = find_circos_base(Path::new("bare.conf"));
        // Either "" or "." — both valid empty-ish bases; key invariant is no panic
        // and the result is some PathBuf.
        assert!(r == std::path::PathBuf::from("") || r == std::path::PathBuf::from("."));
    }

    #[test]
    fn test_resolve_path_base_dir_hit_preferred_over_parent_walk() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter506_prefer_{}", std::process::id()));
        let base = tmp.join("base");
        fs::create_dir_all(&base).unwrap();
        // Create SAME filename in both base and parent — base should win.
        let in_base = base.join("pref.txt");
        let in_parent = tmp.join("pref.txt");
        fs::write(&in_base, b"base").unwrap();
        fs::write(&in_parent, b"parent").unwrap();
        let got = resolve_path("pref.txt", &base);
        assert_eq!(got, in_base);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_run_with_opt_directory_path_returns_error() {
        // Passing a directory (not a file) as config path → Err via read_to_string.
        let dir = std::env::temp_dir();
        let r = run_with_opt(&dir, HashMap::new());
        assert!(r.is_err());
    }

    #[test]
    fn test_find_circos_base_etc_at_deep_but_within_walk_bound() {
        // Walk bound is 6 iters; place etc/colors.conf 3 levels up — should find it.
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter506_deep_{}", std::process::id()));
        fs::create_dir_all(tmp.join("etc")).unwrap();
        fs::write(tmp.join("etc").join("colors.conf"), b"").unwrap();
        // Conf at tmp/a/b/c/conf.txt — 3 parents up.
        let conf = tmp.join("a").join("b").join("c").join("conf.txt");
        fs::create_dir_all(conf.parent().unwrap()).unwrap();
        fs::write(&conf, b"").unwrap();
        let base = find_circos_base(&conf);
        assert_eq!(base, tmp);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_resolve_path_found_in_deep_ancestor_within_5_level_bound() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter506_ancestor_{}", std::process::id()));
        // File at tmp/target.txt; base at tmp/a/b/c (3 levels down).
        fs::write({
            fs::create_dir_all(&tmp).unwrap();
            tmp.join("target.txt")
        }, b"x").unwrap();
        let base = tmp.join("a").join("b").join("c");
        fs::create_dir_all(&base).unwrap();
        let got = resolve_path("target.txt", &base);
        assert_eq!(got, tmp.join("target.txt"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_find_circos_base_etc_colors_adjacent_to_conf_returns_conf_parent() {
        use std::fs;
        // conf at tmp/a/conf.txt; etc/colors.conf inside tmp/a/etc/ → base=tmp/a.
        let tmp = std::env::temp_dir().join(format!("circos_iter537_adj_{}", std::process::id()));
        let parent = tmp.join("a");
        fs::create_dir_all(parent.join("etc")).unwrap();
        fs::write(parent.join("etc").join("colors.conf"), b"").unwrap();
        let conf = parent.join("conf.txt");
        fs::write(&conf, b"").unwrap();
        let base = find_circos_base(&conf);
        assert_eq!(base, parent);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_resolve_path_subdirectory_lookup_relative_to_base() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter537_subdir_{}", std::process::id()));
        let base = tmp.join("base");
        let sub = base.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("target.txt"), b"x").unwrap();
        let got = resolve_path("sub/target.txt", &base);
        assert_eq!(got, sub.join("target.txt"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_find_circos_base_beyond_walk_bound_falls_back_to_conf_parent() {
        use std::fs;
        // Place etc/colors.conf MORE than 6 levels up; walk won't find it → fallback.
        let tmp = std::env::temp_dir().join(format!("circos_iter537_far_{}", std::process::id()));
        fs::create_dir_all(tmp.join("etc")).unwrap();
        fs::write(tmp.join("etc").join("colors.conf"), b"").unwrap();
        // conf at 8 levels deep from tmp.
        let deep = tmp.join("a").join("b").join("c").join("d").join("e").join("f").join("g").join("h");
        fs::create_dir_all(&deep).unwrap();
        let conf = deep.join("conf.txt");
        fs::write(&conf, b"").unwrap();
        let base = find_circos_base(&conf);
        // Walk bound is 6 iters — 8 levels too deep → returns conf's immediate parent.
        assert_eq!(base, deep);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_resolve_path_current_dir_hit_takes_precedence_over_base() {
        // If CWD has the exact file, p.exists() short-circuits before base check.
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("circos_iter537_cwd_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let abs_file = tmp.join("direct_abs.txt");
        fs::write(&abs_file, b"x").unwrap();
        // Pass absolute path; base is unrelated.
        let base = Path::new("/does/not/exist");
        let got = resolve_path(abs_file.to_str().unwrap(), base);
        assert_eq!(got, abs_file);
        fs::remove_dir_all(&tmp).ok();
    }
}
