//! High-level programmatic API for building Circos plots without config files.
//!
//! # Example
//!
//! ```no_run
//! use circos_rs::api::{CircosPlot, PlotType};
//!
//! let mut plot = CircosPlot::new();
//!
//! // Define chromosomes
//! plot.add_chromosome("chr1", "1", 0, 247249719, "chr1");
//! plot.add_chromosome("chr2", "2", 0, 242951149, "chr2");
//!
//! // Add cytogenetic bands
//! plot.add_band("chr1", "p36.33", 0, 2300000, "gneg");
//!
//! // Add links between regions
//! plot.add_link("link1", "chr1", 1000, 5000, "chr2", 80000, 90000)
//!     .color("red")
//!     .thickness(3.0);
//!
//! // Add a histogram track
//! plot.add_plot(PlotType::Histogram)
//!     .r0(0.5)
//!     .r1(0.8)
//!     .add_point("chr1", 0, 5000000, 0.75)
//!     .add_point("chr1", 5000000, 10000000, -0.3);
//!
//! // Add highlights
//! plot.add_highlight("chr1", 50000000, 80000000)
//!     .color("blue")
//!     .r0(0.9)
//!     .r1(0.95);
//!
//! // Render
//! let svg = plot.render_svg();
//! ```

use std::collections::HashMap;

use crate::data::types::{Datum, Link};
use crate::intspan::IntSpan;
use crate::karyotype::types::{Band, Chromosome, DisplayRegion, Karyotype};
use crate::layout::Layout;
use crate::render::color::{Color, ColorMap};
use crate::render::svg::SvgDocument;
use crate::rules::Rule;

/// A Circos plot built entirely in code.
#[derive(Debug, Clone)]
pub struct CircosPlot {
    /// Image radius in pixels.
    pub image_radius: f64,
    /// Angle offset in degrees (default -90, puts 0bp at top).
    pub angle_offset: f64,
    /// Background color name or "r,g,b".
    pub background: String,
    /// Chromosomes units (e.g. 1_000_000 for Mb).
    pub chromosomes_units: f64,
    /// Whether to show ticks.
    pub show_ticks: bool,
    /// Whether to show chromosome labels.
    pub show_labels: bool,
    /// Ideogram thickness in pixels.
    pub ideogram_thickness: f64,
    /// Ideogram radius as fraction of image radius.
    pub ideogram_radius_frac: f64,
    /// Default spacing between ideograms as fraction of total genome size.
    pub spacing_frac: f64,

    karyotype: Karyotype,
    colors: ColorMap,
    link_sets: Vec<LinkSet>,
    highlight_sets: Vec<HighlightSet>,
    plot_sets: Vec<PlotSet>,
    tick_defs: Vec<TickDef>,
}

/// A set of links with shared default styling.
#[derive(Debug, Clone)]
struct LinkSet {
    links: Vec<Link>,
    radius_frac: f64,
    bezier_radius_frac: f64,
    crest: f64,
    color: String,
    thickness: f64,
    ribbon: bool,
    rules: Vec<Rule>,
}

/// A set of highlight regions.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct HighlightSet {
    data: Vec<HighlightEntry>,
    r0_frac: f64,
    r1_frac: f64,
    default_color: String,
    stroke_thickness: f64,
}

#[derive(Debug, Clone)]
struct HighlightEntry {
    chr: String,
    start: i64,
    end: i64,
    color: Option<String>,
}

/// A data plot track.
#[derive(Debug, Clone)]
struct PlotSet {
    plot_type: PlotType,
    data: Vec<Datum>,
    r0_frac: f64,
    r1_frac: f64,
    color: String,
    fill_color: String,
    thickness: f64,
    glyph_size: f64,
    label_size: f64,
}

/// A tick mark definition.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TickDef {
    spacing_bp: f64,
    size_px: f64,
    thickness_px: f64,
    color: String,
    show_label: bool,
    label_size: f64,
    multiplier: f64,
    format: String,
    show_grid: bool,
    grid_color: String,
    grid_thickness: f64,
}

/// Plot type for data tracks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlotType {
    Histogram,
    Heatmap,
    Scatter,
    Line,
    Text,
    Tile,
    Connector,
    Highlight,
}

// ── Builder helpers returned by add_* methods ──────────────────────────────

/// Builder for configuring a link that was just added.
pub struct LinkBuilder<'a> {
    plot: &'a mut CircosPlot,
    set_idx: usize,
    link_idx: usize,
}

impl LinkBuilder<'_> {
    /// Set the color of this specific link, overriding the link set's default.
    pub fn color(self, color: &str) -> Self {
        self.plot.link_sets[self.set_idx].links[self.link_idx]
            .param
            .insert("color".into(), color.into());
        self
    }
    /// Set the stroke thickness of this specific link.
    pub fn thickness(self, t: f64) -> Self {
        self.plot.link_sets[self.set_idx].links[self.link_idx]
            .param
            .insert("thickness".into(), t.to_string());
        self
    }
}

/// Builder for configuring a highlight that was just added.
pub struct HighlightBuilder<'a> {
    plot: &'a mut CircosPlot,
    set_idx: usize,
    entry_idx: usize,
}

impl HighlightBuilder<'_> {
    /// Set the color of this specific highlight entry, overriding the set's default color.
    pub fn color(self, color: &str) -> Self {
        self.plot.highlight_sets[self.set_idx].data[self.entry_idx].color = Some(color.into());
        self
    }
    /// Set the inner radius fraction for the parent highlight set.
    pub fn r0(self, r0: f64) -> Self {
        self.plot.highlight_sets[self.set_idx].r0_frac = r0;
        self
    }
    /// Set the outer radius fraction for the parent highlight set.
    pub fn r1(self, r1: f64) -> Self {
        self.plot.highlight_sets[self.set_idx].r1_frac = r1;
        self
    }
}

/// Builder for configuring a plot track that was just added.
pub struct PlotBuilder<'a> {
    plot: &'a mut CircosPlot,
    set_idx: usize,
}

impl<'a> PlotBuilder<'a> {
    /// Set the inner radius fraction of this plot track.
    pub fn r0(self, r0: f64) -> Self {
        self.plot.plot_sets[self.set_idx].r0_frac = r0;
        self
    }
    /// Set the outer radius fraction of this plot track.
    pub fn r1(self, r1: f64) -> Self {
        self.plot.plot_sets[self.set_idx].r1_frac = r1;
        self
    }
    /// Set the stroke/line color of this plot track.
    pub fn color(self, color: &str) -> Self {
        self.plot.plot_sets[self.set_idx].color = color.into();
        self
    }
    /// Set the fill color of this plot track (used by histograms, heatmaps, tiles, etc.).
    pub fn fill_color(self, color: &str) -> Self {
        self.plot.plot_sets[self.set_idx].fill_color = color.into();
        self
    }
    /// Set the stroke thickness of this plot track in pixels.
    pub fn thickness(self, t: f64) -> Self {
        self.plot.plot_sets[self.set_idx].thickness = t;
        self
    }
    /// Set the glyph size (used by scatter plots) in pixels.
    pub fn glyph_size(self, s: f64) -> Self {
        self.plot.plot_sets[self.set_idx].glyph_size = s;
        self
    }
    /// Set the label font size (used by text plots) in pixels.
    pub fn label_size(self, s: f64) -> Self {
        self.plot.plot_sets[self.set_idx].label_size = s;
        self
    }
    /// Append a numeric data point at the given genomic region to this plot track.
    pub fn add_point(self, chr: &str, start: i64, end: i64, value: f64) -> Self {
        self.plot.plot_sets[self.set_idx].data.push(Datum {
            chr: chr.into(),
            start,
            end,
            set: IntSpan::from_range(start, end),
            id: None,
            value: Some(value),
            label: None,
            param: HashMap::new(),
        });
        self
    }
    /// Append a labeled text entry at the given genomic region to this plot track.
    pub fn add_text(self, chr: &str, start: i64, end: i64, label: &str) -> Self {
        self.plot.plot_sets[self.set_idx].data.push(Datum {
            chr: chr.into(),
            start,
            end,
            set: IntSpan::from_range(start, end),
            id: None,
            value: None,
            label: Some(label.into()),
            param: HashMap::new(),
        });
        self
    }
}

// ── CircosPlot implementation ──────────────────────────────────────────────

impl CircosPlot {
    /// Create a new empty Circos plot with sensible defaults.
    pub fn new() -> Self {
        let mut colors = ColorMap::new();
        // Seed with the essential colors
        for (name, rgb) in [
            ("white", Color::rgb(255, 255, 255)),
            ("black", Color::rgb(0, 0, 0)),
            ("red", Color::rgb(247, 42, 66)),
            ("green", Color::rgb(51, 204, 94)),
            ("blue", Color::rgb(54, 116, 217)),
            ("grey", Color::rgb(200, 200, 200)),
            ("lgrey", Color::rgb(210, 210, 210)),
            ("dgrey", Color::rgb(170, 170, 170)),
            ("vvlgrey", Color::rgb(230, 230, 230)),
            ("vvvlgrey", Color::rgb(240, 240, 240)),
            ("yellow", Color::rgb(255, 255, 0)),
            ("orange", Color::rgb(255, 136, 0)),
            ("purple", Color::rgb(189, 51, 204)),
            ("gneg", Color::rgb(255, 255, 255)),
            ("gpos25", Color::rgb(200, 200, 200)),
            ("gpos50", Color::rgb(200, 200, 200)),
            ("gpos66", Color::rgb(160, 160, 160)),
            ("gpos75", Color::rgb(130, 130, 130)),
            ("gpos100", Color::rgb(0, 0, 0)),
            ("gpos", Color::rgb(0, 0, 0)),
            ("gvar", Color::rgb(220, 220, 220)),
            ("acen", Color::rgb(217, 47, 39)),
            ("stalk", Color::rgb(100, 127, 164)),
            ("chr1", Color::rgb(153, 102, 0)),
            ("chr2", Color::rgb(102, 102, 0)),
            ("chr3", Color::rgb(153, 153, 30)),
            ("chr4", Color::rgb(204, 0, 0)),
            ("chr5", Color::rgb(255, 0, 0)),
            ("chr6", Color::rgb(255, 0, 204)),
            ("chr7", Color::rgb(255, 204, 204)),
            ("chr8", Color::rgb(255, 153, 0)),
            ("chr9", Color::rgb(255, 204, 0)),
            ("chr10", Color::rgb(255, 255, 0)),
            ("chr11", Color::rgb(204, 255, 0)),
            ("chr12", Color::rgb(0, 255, 0)),
            ("chr13", Color::rgb(53, 128, 0)),
            ("chr14", Color::rgb(0, 0, 204)),
            ("chr15", Color::rgb(102, 153, 255)),
            ("chr16", Color::rgb(153, 204, 255)),
            ("chr17", Color::rgb(0, 255, 255)),
            ("chr18", Color::rgb(204, 255, 255)),
            ("chr19", Color::rgb(153, 0, 204)),
            ("chr20", Color::rgb(204, 51, 255)),
            ("chr21", Color::rgb(204, 153, 255)),
            ("chr22", Color::rgb(102, 102, 102)),
            ("chrX", Color::rgb(153, 153, 153)),
            ("chrY", Color::rgb(204, 204, 204)),
        ] {
            colors.colors.insert(name.to_string(), rgb);
        }

        CircosPlot {
            image_radius: 1500.0,
            angle_offset: -90.0,
            background: "white".into(),
            chromosomes_units: 1_000_000.0,
            show_ticks: false,
            show_labels: true,
            ideogram_thickness: 100.0,
            ideogram_radius_frac: 0.85,
            spacing_frac: 0.005,
            karyotype: Karyotype::default(),
            colors,
            link_sets: Vec::new(),
            highlight_sets: Vec::new(),
            plot_sets: Vec::new(),
            tick_defs: Vec::new(),
        }
    }

    /// Register a named color (name -> "r,g,b" or Color).
    pub fn define_color(&mut self, name: &str, r: u8, g: u8, b: u8) -> &mut Self {
        self.colors.colors.insert(name.into(), Color::rgb(r, g, b));
        self
    }

    // ── Karyotype ──────────────────────────────────────────────────────

    /// Add a chromosome.
    pub fn add_chromosome(
        &mut self,
        name: &str,
        label: &str,
        start: i64,
        end: i64,
        color: &str,
    ) -> &mut Self {
        let idx = self.karyotype.chromosomes.len();
        let chr = Chromosome {
            name: name.into(),
            label: label.into(),
            start,
            end,
            color: color.into(),
            set: IntSpan::from_range(start, end),
            index: idx,
            display: true,
            display_region: DisplayRegion::default(),
        };
        self.karyotype.order.push(name.into());
        self.karyotype.chromosomes.insert(name.into(), chr);
        self
    }

    /// Add a cytogenetic band on a chromosome.
    pub fn add_band(
        &mut self,
        chr: &str,
        name: &str,
        start: i64,
        end: i64,
        color: &str,
    ) -> &mut Self {
        let band = Band {
            name: name.into(),
            label: name.into(),
            parent: chr.into(),
            start,
            end,
            color: color.into(),
            set: IntSpan::from_range(start, end),
        };
        self.karyotype
            .bands
            .entry(chr.into())
            .or_default()
            .push(band);
        self
    }

    // ── Links ──────────────────────────────────────────────────────────

    /// Start a new link set with given defaults. Returns &mut self for
    /// further configuration, or use `add_link` for individual links.
    pub fn new_link_set(&mut self) -> &mut Self {
        self.link_sets.push(LinkSet {
            links: Vec::new(),
            radius_frac: 0.9,
            bezier_radius_frac: 0.2,
            crest: 1.0,
            color: "lgrey".into(),
            thickness: 1.0,
            ribbon: false,
            rules: Vec::new(),
        });
        self
    }

    /// Add a link between two genomic regions. Creates a new link set if none exists.
    pub fn add_link(
        &mut self,
        id: &str,
        chr1: &str,
        start1: i64,
        end1: i64,
        chr2: &str,
        start2: i64,
        end2: i64,
    ) -> LinkBuilder<'_> {
        if self.link_sets.is_empty() {
            self.new_link_set();
        }
        let set_idx = self.link_sets.len() - 1;
        let link = Link {
            id: id.into(),
            points: vec![
                Datum {
                    chr: chr1.into(),
                    start: start1,
                    end: end1,
                    set: IntSpan::from_range(start1, end1),
                    id: Some(id.into()),
                    value: None,
                    label: None,
                    param: HashMap::new(),
                },
                Datum {
                    chr: chr2.into(),
                    start: start2,
                    end: end2,
                    set: IntSpan::from_range(start2, end2),
                    id: Some(id.into()),
                    value: None,
                    label: None,
                    param: HashMap::new(),
                },
            ],
            param: HashMap::new(),
        };
        let link_idx = self.link_sets[set_idx].links.len();
        self.link_sets[set_idx].links.push(link);
        LinkBuilder {
            plot: self,
            set_idx,
            link_idx,
        }
    }

    /// Configure the current (last) link set's radius.
    pub fn link_radius(&mut self, frac: f64) -> &mut Self {
        if let Some(ls) = self.link_sets.last_mut() {
            ls.radius_frac = frac;
        }
        self
    }

    /// Configure the current link set's bezier radius.
    pub fn link_bezier_radius(&mut self, frac: f64) -> &mut Self {
        if let Some(ls) = self.link_sets.last_mut() {
            ls.bezier_radius_frac = frac;
        }
        self
    }

    /// Configure the current link set as ribbon mode.
    pub fn link_ribbon(&mut self, ribbon: bool) -> &mut Self {
        if let Some(ls) = self.link_sets.last_mut() {
            ls.ribbon = ribbon;
        }
        self
    }

    /// Set default color for the current link set.
    pub fn link_color(&mut self, color: &str) -> &mut Self {
        if let Some(ls) = self.link_sets.last_mut() {
            ls.color = color.into();
        }
        self
    }

    // ── Highlights ─────────────────────────────────────────────────────

    /// Add a highlight region. Creates a new highlight set if none exists.
    pub fn add_highlight(&mut self, chr: &str, start: i64, end: i64) -> HighlightBuilder<'_> {
        if self.highlight_sets.is_empty() {
            self.highlight_sets.push(HighlightSet {
                data: Vec::new(),
                r0_frac: 0.9,
                r1_frac: 0.95,
                default_color: "red".into(),
                stroke_thickness: 0.0,
            });
        }
        let set_idx = self.highlight_sets.len() - 1;
        let entry = HighlightEntry {
            chr: chr.into(),
            start,
            end,
            color: None,
        };
        let entry_idx = self.highlight_sets[set_idx].data.len();
        self.highlight_sets[set_idx].data.push(entry);
        HighlightBuilder {
            plot: self,
            set_idx,
            entry_idx,
        }
    }

    // ── Plots ──────────────────────────────────────────────────────────

    /// Add a new data plot track. Use the returned builder to configure and add data.
    pub fn add_plot(&mut self, plot_type: PlotType) -> PlotBuilder<'_> {
        self.plot_sets.push(PlotSet {
            plot_type,
            data: Vec::new(),
            r0_frac: 0.5,
            r1_frac: 0.8,
            color: "black".into(),
            fill_color: "black".into(),
            thickness: 2.0,
            glyph_size: 5.0,
            label_size: 12.0,
        });
        let idx = self.plot_sets.len() - 1;
        PlotBuilder {
            plot: self,
            set_idx: idx,
        }
    }

    // ── Ticks ──────────────────────────────────────────────────────────

    /// Add a tick mark definition.
    pub fn add_ticks(&mut self, spacing_bp: f64, size_px: f64, show_label: bool) -> &mut Self {
        self.show_ticks = true;
        self.tick_defs.push(TickDef {
            spacing_bp,
            size_px,
            thickness_px: 2.0,
            color: "black".into(),
            show_label,
            label_size: 12.0,
            multiplier: 1.0,
            format: "%d".into(),
            show_grid: false,
            grid_color: "grey".into(),
            grid_thickness: 1.0,
        });
        self
    }

    // ── Rendering ──────────────────────────────────────────────────────

    /// Render the plot to an SVG string.
    pub fn render_svg(&self) -> String {
        let layout = self.build_layout();
        let width = self.image_radius * 2.0;
        let height = self.image_radius * 2.0;
        let mut doc = SvgDocument::new(width, height);

        // Background
        if let Some(bg) = self.colors.resolve(&self.background) {
            doc.add(format!(
                r#"<rect x="0" y="0" width="{:.0}" height="{:.0}" style="fill: {};" />"#,
                width,
                height,
                bg.to_svg_rgb()
            ));
        }

        // Ideograms
        self.draw_ideograms_api(&mut doc, &layout);

        // Ticks
        if self.show_ticks {
            self.draw_ticks_api(&mut doc, &layout);
        }

        // Highlights
        for hs in &self.highlight_sets {
            self.draw_highlights_api(&mut doc, &layout, hs);
        }

        // Links
        for ls in &self.link_sets {
            self.draw_links_api(&mut doc, &layout, ls);
        }

        // Plots
        for (i, ps) in self.plot_sets.iter().enumerate() {
            doc.open_group(&format!("plot-{}", i));
            self.draw_plot_api(&mut doc, &layout, ps);
            doc.close_group();
        }

        doc.render()
    }

    /// Render to PNG bytes.
    pub fn render_png(&self) -> Result<Vec<u8>, String> {
        let svg = self.render_svg();
        let options = usvg::Options::default();
        let tree =
            usvg::Tree::from_str(&svg, &options).map_err(|e| format!("SVG parse error: {}", e))?;
        let size = tree.size();
        let w = size.width().ceil() as u32;
        let h = size.height().ceil() as u32;
        let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("failed to create pixmap")?;
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        pixmap
            .encode_png()
            .map_err(|e| format!("PNG encode error: {}", e))
    }

    // ── Internal ───────────────────────────────────────────────────────

    /// Construct an internal `Layout` from this plot's configuration and karyotype,
    /// translating the public fields into the config map expected by `Layout::build`.
    fn build_layout(&self) -> Layout {
        // Build a minimal config HashMap for Layout::build
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        use crate::config::types::ConfigValue as CV;

        // Image block
        let mut image = HashMap::new();
        image.insert("radius".into(), CV::Str(format!("{}p", self.image_radius)));
        image.insert(
            "angle_offset".into(),
            CV::Str(self.angle_offset.to_string()),
        );
        image.insert("background".into(), CV::Str(self.background.clone()));
        conf.insert("image".into(), CV::Map(image));

        // Ideogram block
        let mut ideogram = HashMap::new();
        ideogram.insert(
            "radius".into(),
            CV::Str(format!("{}r", self.ideogram_radius_frac)),
        );
        ideogram.insert(
            "thickness".into(),
            CV::Str(format!("{}p", self.ideogram_thickness)),
        );
        ideogram.insert(
            "show_label".into(),
            CV::Str(if self.show_labels { "1" } else { "0" }.into()),
        );
        ideogram.insert("fill".into(), CV::Str("1".into()));
        ideogram.insert("stroke_thickness".into(), CV::Str("2".into()));
        ideogram.insert("stroke_color".into(), CV::Str("black".into()));
        ideogram.insert("show_bands".into(), CV::Str("1".into()));
        ideogram.insert("fill_bands".into(), CV::Str("1".into()));
        ideogram.insert("label_size".into(), CV::Str("36".into()));

        let mut spacing = HashMap::new();
        spacing.insert("default".into(), CV::Str(format!("{}r", self.spacing_frac)));
        ideogram.insert("spacing".into(), CV::Map(spacing));
        conf.insert("ideogram".into(), CV::Map(ideogram));

        conf.insert(
            "chromosomes_units".into(),
            CV::Str(self.chromosomes_units.to_string()),
        );
        conf.insert("chromosomes_display_default".into(), CV::Str("1".into()));
        conf.insert("units_ok".into(), CV::Str("bupr".into()));
        conf.insert("units_nounit".into(), CV::Str("n".into()));

        Layout::build(&conf, &self.karyotype).expect("failed to build layout")
    }

    /// Render ideogram slices (with cytogenetic bands, outlines and labels) into the SVG document.
    fn draw_ideograms_api(&self, doc: &mut SvgDocument, layout: &Layout) {
        use crate::render::svg::svg_slice;
        let stroke = self.colors.resolve("black").unwrap_or(Color::rgb(0, 0, 0));
        let label_color = stroke;

        doc.open_group("ideograms");

        for ideo in &layout.ideograms {
            let start = ideo.set.min().unwrap_or(0);
            let end = ideo.set.max().unwrap_or(0);
            let start_a = layout.getanglepos(start, &ideo.chr).unwrap_or(0.0);
            let end_a = layout.getanglepos(end, &ideo.chr).unwrap_or(0.0);

            let ro = if ideo.radius_outer > 0.0 {
                ideo.radius_outer
            } else {
                layout.dims.ideogram_radius_outer
            };
            let ri = if ideo.radius_inner > 0.0 {
                ideo.radius_inner
            } else {
                layout.dims.ideogram_radius_inner
            };

            // Fill
            let fill = self.colors.resolve(&ideo.color);
            doc.add(svg_slice(
                layout,
                start_a,
                end_a,
                ri,
                ro,
                Some(&stroke),
                Some(2.0),
                fill.as_ref(),
                None,
            ));

            // Bands
            if let Some(bands) = self.karyotype.bands.get(&ideo.chr) {
                for band in bands {
                    let bs = band.set.intersect(&ideo.set);
                    if bs.cardinality() < 1 {
                        continue;
                    }
                    let ba = layout
                        .getanglepos(bs.min().unwrap(), &ideo.chr)
                        .unwrap_or(0.0);
                    let be = layout
                        .getanglepos(bs.max().unwrap(), &ideo.chr)
                        .unwrap_or(0.0);
                    let fc = self.colors.resolve(&band.color);
                    doc.add(svg_slice(
                        layout,
                        ba,
                        be,
                        ri,
                        ro,
                        Some(&stroke),
                        Some(2.0),
                        fc.as_ref(),
                        None,
                    ));
                }
            }

            // Outline
            doc.add(svg_slice(
                layout,
                start_a,
                end_a,
                ri,
                ro,
                Some(&stroke),
                Some(2.0),
                None,
                None,
            ));

            // Label
            if self.show_labels {
                let mid = (start + end) / 2;
                let mid_a = layout.getanglepos(mid, &ideo.chr).unwrap_or(0.0);
                let lr = ro + 36.0 * 0.8;
                doc.add(crate::render::svg::svg_text(
                    layout,
                    mid_a,
                    lr,
                    &ideo.label,
                    36.0,
                    &label_color,
                    0.0,
                ));
            }
        }
        doc.close_group();
    }

    /// Render all configured tick marks (and optional labels) along each ideogram into the SVG document.
    fn draw_ticks_api(&self, doc: &mut SvgDocument, layout: &Layout) {
        use crate::render::svg::svg_tick;
        doc.open_group("ticks");

        for td in &self.tick_defs {
            let color = self
                .colors
                .resolve(&td.color)
                .unwrap_or(Color::rgb(0, 0, 0));
            for ideo in &layout.ideograms {
                let s = ideo.set.min().unwrap_or(0);
                let e = ideo.set.max().unwrap_or(0);
                let ro = if ideo.radius_outer > 0.0 {
                    ideo.radius_outer
                } else {
                    layout.dims.ideogram_radius_outer
                };

                let first = ((s as f64 / td.spacing_bp).ceil() * td.spacing_bp) as i64;
                let mut pos = first;
                while pos <= e {
                    if let Some(a) = layout.getanglepos(pos, &ideo.chr) {
                        doc.add(svg_tick(
                            layout,
                            a,
                            ro,
                            ro + td.size_px,
                            td.thickness_px,
                            &color,
                        ));
                        if td.show_label {
                            let v = pos as f64 * td.multiplier;
                            let label = match td.format.as_str() {
                                "%d" => format!("{}", v as i64),
                                _ => format!("{}", v as i64),
                            };
                            let lr = ro + td.size_px + td.label_size * 0.5;
                            doc.add(crate::render::svg::svg_text(
                                layout,
                                a,
                                lr,
                                &label,
                                td.label_size,
                                &color,
                                0.0,
                            ));
                        }
                    }
                    pos += td.spacing_bp as i64;
                }
            }
        }
        doc.close_group();
    }

    /// Render one highlight set's regions as filled arc slices into the SVG document.
    fn draw_highlights_api(&self, doc: &mut SvgDocument, layout: &Layout, hs: &HighlightSet) {
        use crate::render::svg::svg_slice;
        let r0 = hs.r0_frac * layout.dims.ideogram_radius;
        let r1 = hs.r1_frac * layout.dims.ideogram_radius;

        doc.open_group("highlights");
        for entry in &hs.data {
            if layout.find_ideogram_by_chr(&entry.chr).is_none() {
                continue;
            }
            let sa = match layout.getanglepos(entry.start, &entry.chr) {
                Some(a) => a,
                None => continue,
            };
            let ea = match layout.getanglepos(entry.end, &entry.chr) {
                Some(a) => a,
                None => continue,
            };
            let cn = entry.color.as_deref().unwrap_or(&hs.default_color);
            let fc = self.colors.resolve(cn);
            doc.add(svg_slice(
                layout,
                sa,
                ea,
                r0.min(r1),
                r0.max(r1),
                None,
                None,
                fc.as_ref(),
                None,
            ));
        }
        doc.close_group();
    }

    /// Render one link set by building a minimal block config and delegating to `draw::links::draw_links`.
    fn draw_links_api(&self, doc: &mut SvgDocument, layout: &Layout, ls: &LinkSet) {
        use crate::config::types::ConfigValue as CV;
        use crate::draw::links;

        let mut defaults: HashMap<String, String> = HashMap::new();
        defaults.insert("radius".into(), format!("{}r", ls.radius_frac));
        defaults.insert(
            "bezier_radius".into(),
            format!("{}r", ls.bezier_radius_frac),
        );
        defaults.insert("crest".into(), ls.crest.to_string());
        defaults.insert("color".into(), ls.color.clone());
        defaults.insert("thickness".into(), ls.thickness.to_string());
        if ls.ribbon {
            defaults.insert("ribbon".into(), "1".into());
        }

        // Build a minimal block_conf
        let block_conf: HashMap<String, CV> = defaults
            .iter()
            .map(|(k, v)| (k.clone(), CV::Str(v.clone())))
            .collect();

        doc.open_group("links");
        links::draw_links(
            doc,
            layout,
            &ls.links,
            &defaults,
            &block_conf,
            &ls.rules,
            &self.colors,
        );
        doc.close_group();
    }

    /// Render one plot track by building a minimal block config and delegating to `draw::plots::draw_plot`.
    fn draw_plot_api(&self, doc: &mut SvgDocument, layout: &Layout, ps: &PlotSet) {
        use crate::config::types::ConfigValue as CV;
        use crate::draw::plots;

        let type_str = match ps.plot_type {
            PlotType::Histogram => "histogram",
            PlotType::Heatmap => "heatmap",
            PlotType::Scatter => "scatter",
            PlotType::Line => "line",
            PlotType::Text => "text",
            PlotType::Tile => "tile",
            PlotType::Connector => "connector",
            PlotType::Highlight => "highlight",
        };

        let mut block: HashMap<String, CV> = HashMap::new();
        block.insert("type".into(), CV::Str(type_str.into()));
        block.insert("r0".into(), CV::Str(format!("{}r", ps.r0_frac)));
        block.insert("r1".into(), CV::Str(format!("{}r", ps.r1_frac)));
        block.insert("color".into(), CV::Str(ps.color.clone()));
        block.insert("fill_color".into(), CV::Str(ps.fill_color.clone()));
        block.insert("thickness".into(), CV::Str(format!("{}p", ps.thickness)));
        block.insert("glyph_size".into(), CV::Str(format!("{}p", ps.glyph_size)));
        block.insert("label_size".into(), CV::Str(format!("{}p", ps.label_size)));

        plots::draw_plot(doc, layout, &ps.data, &block, &self.colors);
    }
}

impl Default for CircosPlot {
    /// Returns an empty `CircosPlot` with default settings (equivalent to `CircosPlot::new()`).
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;

        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");
        plot.add_chromosome("chr2", "2", 0, 80_000_000, "chr2");

        plot.add_band("chr1", "p36", 0, 10_000_000, "gneg");
        plot.add_band("chr1", "p35", 10_000_000, 30_000_000, "gpos50");

        let svg = plot.render_svg();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("<path")); // ideogram arcs
        assert!(svg.contains(">1</text>")); // chr1 label
        assert!(svg.contains(">2</text>")); // chr2 label
    }

    #[test]
    fn test_links_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;
        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");
        plot.add_chromosome("chr2", "2", 0, 80_000_000, "chr2");

        plot.add_link(
            "link1", "chr1", 10_000_000, 15_000_000, "chr2", 20_000_000, 25_000_000,
        )
        .color("red")
        .thickness(3.0);

        let svg = plot.render_svg();
        assert!(svg.contains("C ")); // bezier curve command
        assert!(svg.contains("stroke:")); // has styled bezier
    }

    #[test]
    fn test_histogram_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;
        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");

        plot.add_plot(PlotType::Histogram)
            .r0(0.5)
            .r1(0.8)
            .color("blue")
            .fill_color("blue")
            .add_point("chr1", 0, 10_000_000, 0.5)
            .add_point("chr1", 10_000_000, 20_000_000, 0.8)
            .add_point("chr1", 20_000_000, 30_000_000, -0.2);

        let svg = plot.render_svg();
        // Should have plot group and path elements for bars
        assert!(svg.contains(r#"<g id="plot-0">"#));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn test_highlight_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;
        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");

        plot.add_highlight("chr1", 30_000_000, 50_000_000)
            .color("blue")
            .r0(0.85)
            .r1(0.90);

        let svg = plot.render_svg();
        assert!(svg.contains("highlights"));
    }

    #[test]
    fn test_scatter_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;
        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");

        plot.add_plot(PlotType::Scatter)
            .r0(0.4)
            .r1(0.7)
            .add_point("chr1", 5_000_000, 5_000_000, 0.5)
            .add_point("chr1", 25_000_000, 25_000_000, 0.9);

        let svg = plot.render_svg();
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn test_text_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;
        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");

        plot.add_plot(PlotType::Text)
            .r0(0.9)
            .r1(1.1)
            .label_size(18.0)
            .add_text("chr1", 50_000_000, 50_000_000, "GeneA");

        let svg = plot.render_svg();
        assert!(svg.contains("GeneA"));
    }

    #[test]
    fn test_png_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 200.0;
        plot.add_chromosome("chr1", "1", 0, 50_000_000, "chr1");

        let png_bytes = plot.render_png().unwrap();
        // PNG magic bytes
        assert_eq!(&png_bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert!(png_bytes.len() > 1000);
    }

    #[test]
    fn test_ticks_api() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 500.0;
        plot.add_chromosome("chr1", "1", 0, 100_000_000, "chr1");
        plot.add_ticks(10_000_000.0, 8.0, true);

        let svg = plot.render_svg();
        assert!(svg.contains(r#"<g id="ticks">"#));
        assert!(svg.contains("<line")); // tick marks
    }

    #[test]
    fn test_define_color_api() {
        let mut plot = CircosPlot::new();
        plot.define_color("custom", 42, 99, 200);
        // Color name should be resolvable afterwards.
        let c = plot.colors.resolve("custom").expect("custom color missing");
        assert_eq!((c.r, c.g, c.b), (42, 99, 200));
    }

    #[test]
    fn test_define_color_overwrites_existing() {
        let mut plot = CircosPlot::new();
        plot.define_color("mark", 0, 0, 0);
        plot.define_color("mark", 255, 128, 64);
        let c = plot.colors.resolve("mark").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 128, 64));
    }

    #[test]
    fn test_add_chromosome_assigns_sequential_index() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chrA", "A", 0, 100, "red");
        plot.add_chromosome("chrB", "B", 0, 200, "blue");
        plot.add_chromosome("chrC", "C", 0, 300, "green");
        assert_eq!(plot.karyotype.chromosomes["chrA"].index, 0);
        assert_eq!(plot.karyotype.chromosomes["chrB"].index, 1);
        assert_eq!(plot.karyotype.chromosomes["chrC"].index, 2);
        assert_eq!(plot.karyotype.order, vec!["chrA", "chrB", "chrC"]);
    }

    #[test]
    fn test_add_band_populates_parent_chr() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_band("chr1", "p1", 0, 500, "gneg");
        plot.add_band("chr1", "q1", 500, 1000, "gpos");
        let bands = plot.karyotype.bands.get("chr1").expect("bands on chr1");
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].name, "p1");
        assert_eq!(bands[1].name, "q1");
        assert_eq!(bands[0].parent, "chr1");
    }

    #[test]
    fn test_new_default_has_sensible_values() {
        let plot = CircosPlot::new();
        assert!(plot.image_radius > 0.0, "expected positive default radius");
        assert!(plot.karyotype.chromosomes.is_empty());
        assert!(plot.karyotype.bands.is_empty());
        assert!(plot.link_sets.is_empty());
    }

    #[test]
    fn test_new_link_set_appends_with_defaults() {
        let mut plot = CircosPlot::new();
        plot.new_link_set();
        plot.new_link_set();
        assert_eq!(plot.link_sets.len(), 2);
        // Defaults: radius_frac=0.9, bezier_radius_frac=0.2, color="lgrey", thickness=1.0, ribbon=false.
        let ls = &plot.link_sets[0];
        assert!((ls.radius_frac - 0.9).abs() < 1e-12);
        assert!((ls.bezier_radius_frac - 0.2).abs() < 1e-12);
        assert_eq!(ls.color, "lgrey");
        assert!(!ls.ribbon);
    }

    #[test]
    fn test_link_setters_mutate_last_set() {
        let mut plot = CircosPlot::new();
        plot.new_link_set();
        plot.link_radius(0.8).link_bezier_radius(0.3).link_ribbon(true).link_color("red");
        let ls = &plot.link_sets[0];
        assert!((ls.radius_frac - 0.8).abs() < 1e-12);
        assert!((ls.bezier_radius_frac - 0.3).abs() < 1e-12);
        assert!(ls.ribbon);
        assert_eq!(ls.color, "red");
    }

    #[test]
    fn test_link_setters_noop_on_empty() {
        // With no link sets, setters silently return without panicking.
        let mut plot = CircosPlot::new();
        plot.link_radius(0.7);
        plot.link_ribbon(true);
        plot.link_color("purple");
        assert!(plot.link_sets.is_empty());
    }

    #[test]
    fn test_add_link_creates_link_set_if_missing() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 100_000, "red");
        plot.add_chromosome("chr2", "2", 0, 100_000, "blue");
        // No explicit new_link_set — add_link should auto-create one.
        plot.add_link("l1", "chr1", 10, 20, "chr2", 30, 40);
        assert_eq!(plot.link_sets.len(), 1);
        assert_eq!(plot.link_sets[0].links.len(), 1);
        assert_eq!(plot.link_sets[0].links[0].id, "l1");
    }

    #[test]
    fn test_add_highlight_accumulates_entries() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_highlight("chr1", 100, 200);
        plot.add_highlight("chr1", 300, 400);
        plot.add_highlight("chr1", 500, 600);
        assert_eq!(plot.highlight_sets.len(), 1);
        assert_eq!(plot.highlight_sets[0].data.len(), 3);
    }

    #[test]
    fn test_plot_builder_chained_setters() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_plot(PlotType::Histogram)
            .r0(0.4)
            .r1(0.7)
            .color("navy")
            .fill_color("sky")
            .thickness(3.5)
            .glyph_size(6.0)
            .label_size(14.0);
        assert_eq!(plot.plot_sets.len(), 1);
        let ps = &plot.plot_sets[0];
        assert!((ps.r0_frac - 0.4).abs() < 1e-12);
        assert!((ps.r1_frac - 0.7).abs() < 1e-12);
        assert_eq!(ps.color, "navy");
        assert_eq!(ps.fill_color, "sky");
        assert!((ps.thickness - 3.5).abs() < 1e-12);
        assert!((ps.glyph_size - 6.0).abs() < 1e-12);
        assert!((ps.label_size - 14.0).abs() < 1e-12);
    }

    #[test]
    fn test_plot_builder_add_point_and_text() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_plot(PlotType::Scatter)
            .add_point("chr1", 100, 200, 0.5)
            .add_point("chr1", 300, 400, 0.7);
        plot.add_plot(PlotType::Text)
            .add_text("chr1", 500, 600, "label1")
            .add_text("chr1", 700, 800, "label2");
        let scatter = &plot.plot_sets[0];
        assert_eq!(scatter.data.len(), 2);
        assert_eq!(scatter.data[0].value, Some(0.5));
        assert_eq!(scatter.data[1].value, Some(0.7));
        assert!(scatter.data[0].label.is_none());
        let text = &plot.plot_sets[1];
        assert_eq!(text.data.len(), 2);
        assert_eq!(text.data[0].label.as_deref(), Some("label1"));
        assert_eq!(text.data[1].label.as_deref(), Some("label2"));
        assert!(text.data[0].value.is_none());
    }

    #[test]
    fn test_link_builder_chain_setters() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_chromosome("chr2", "2", 0, 1000, "blue");
        plot.add_link("lnk", "chr1", 0, 100, "chr2", 200, 300)
            .color("magenta")
            .thickness(4.2);
        let ls = &plot.link_sets[0];
        assert_eq!(ls.links.len(), 1);
        let link = &ls.links[0];
        assert_eq!(link.param.get("color").map(|s| s.as_str()), Some("magenta"));
        assert_eq!(link.param.get("thickness").map(|s| s.as_str()), Some("4.2"));
    }

    #[test]
    fn test_highlight_builder_chain_setters() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_highlight("chr1", 0, 100)
            .color("yellow")
            .r0(0.1)
            .r1(0.2);
        let set = &plot.highlight_sets[0];
        // Default set-level r0_frac=0.9, r1_frac=0.95 — builder mutates them.
        assert!((set.r0_frac - 0.1).abs() < 1e-12);
        assert!((set.r1_frac - 0.2).abs() < 1e-12);
        // Per-entry color recorded.
        assert_eq!(set.data[0].color.as_deref(), Some("yellow"));
    }

    #[test]
    fn test_new_has_seed_colors() {
        let plot = CircosPlot::new();
        // Key seeded colors should be present from `CircosPlot::new()`.
        for (name, rgb) in [
            ("white", (255u8, 255, 255)),
            ("black", (0, 0, 0)),
            ("red", (247, 42, 66)),
            ("chr1", (153, 102, 0)),
        ] {
            let c = plot
                .colors
                .resolve(name)
                .unwrap_or_else(|| panic!("color {} missing", name));
            assert_eq!((c.r, c.g, c.b), rgb, "wrong rgb for {}", name);
        }
    }

    #[test]
    fn test_render_svg_background_rect_uses_configured_color() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 200.0;
        plot.background = "black".into();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        let svg = plot.render_svg();
        // Background rect covers the full square canvas with fill = rgb(0,0,0).
        assert!(
            svg.contains(r#"<rect x="0" y="0" width="400" height="400" style="fill: rgb(0,0,0);""#),
            "missing expected black bg rect in: {}",
            &svg[..400.min(svg.len())]
        );
    }

    #[test]
    fn test_render_png_produces_magic_bytes() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 200.0;
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        let bytes = plot.render_png().expect("png render");
        assert!(bytes.len() > 8);
        // PNG signature: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&bytes[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_render_svg_wraps_each_plot_in_group() {
        let mut plot = CircosPlot::new();
        plot.image_radius = 200.0;
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_plot(PlotType::Histogram).add_point("chr1", 100, 200, 0.5);
        plot.add_plot(PlotType::Scatter).add_point("chr1", 300, 400, 0.7);
        let svg = plot.render_svg();
        // Each plot set gets its own `<g id="plot-N">` wrapper.
        assert!(svg.contains(r#"<g id="plot-0">"#), "missing plot-0 group");
        assert!(svg.contains(r#"<g id="plot-1">"#), "missing plot-1 group");
    }

    #[test]
    fn test_add_ticks_toggles_show_flag_and_appends() {
        let mut plot = CircosPlot::new();
        assert!(!plot.show_ticks, "show_ticks should default false");
        plot.add_ticks(10_000_000.0, 8.0, true);
        plot.add_ticks(50_000_000.0, 12.0, false);
        // First call sets show_ticks=true; both defs are appended in order.
        assert!(plot.show_ticks);
        assert_eq!(plot.tick_defs.len(), 2);
        assert!((plot.tick_defs[0].spacing_bp - 10_000_000.0).abs() < 1e-6);
        assert!(plot.tick_defs[0].show_label);
        assert!((plot.tick_defs[1].size_px - 12.0).abs() < 1e-12);
        assert!(!plot.tick_defs[1].show_label);
        // Default-initialized fields on tick defs.
        assert_eq!(plot.tick_defs[0].color, "black");
        assert_eq!(plot.tick_defs[0].format, "%d");
    }

    #[test]
    fn test_add_plot_different_types_sequential_sets() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("chr1", "1", 0, 1000, "red");
        plot.add_plot(PlotType::Histogram);
        plot.add_plot(PlotType::Heatmap);
        plot.add_plot(PlotType::Line);
        plot.add_plot(PlotType::Tile);
        assert_eq!(plot.plot_sets.len(), 4);
        assert!(matches!(plot.plot_sets[0].plot_type, PlotType::Histogram));
        assert!(matches!(plot.plot_sets[1].plot_type, PlotType::Heatmap));
        assert!(matches!(plot.plot_sets[2].plot_type, PlotType::Line));
        assert!(matches!(plot.plot_sets[3].plot_type, PlotType::Tile));
    }

    #[test]
    fn test_render_svg_always_includes_xml_declaration() {
        // Even with no karyotype, render_svg should still produce a valid
        // document skeleton (no panic).
        let plot = CircosPlot::new();
        let svg = plot.render_svg();
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.ends_with("</svg>\n") || svg.ends_with("</svg>"));
    }

    #[test]
    fn test_link_config_noops_when_no_link_sets() {
        // `link_radius`/`link_bezier_radius`/`link_ribbon`/`link_color` all use
        // `link_sets.last_mut()` — when empty, they're no-ops (no panic).
        let mut plot = CircosPlot::new();
        plot.link_radius(0.5)
            .link_bezier_radius(0.3)
            .link_ribbon(true)
            .link_color("red");
        assert!(plot.link_sets.is_empty());
    }

    #[test]
    fn test_link_builder_color_and_thickness_per_link_override() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_chromosome("c2", "2", 0, 100, "blue");
        plot.add_link("l1", "c1", 0, 10, "c2", 50, 60)
            .color("green")
            .thickness(3.5);
        let link = &plot.link_sets[0].links[0];
        assert_eq!(link.param.get("color").map(String::as_str), Some("green"));
        assert_eq!(link.param.get("thickness").map(String::as_str), Some("3.5"));
    }

    #[test]
    fn test_plot_builder_thickness_glyph_label_size_setters() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Scatter)
            .thickness(4.0)
            .glyph_size(7.5)
            .label_size(18.0);
        let set = &plot.plot_sets[0];
        assert_eq!(set.thickness, 4.0);
        assert_eq!(set.glyph_size, 7.5);
        assert_eq!(set.label_size, 18.0);
    }

    #[test]
    fn test_highlight_builder_color_applies_to_last_entry() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_highlight("c1", 10, 20).color("blue");
        plot.add_highlight("c1", 30, 40).color("green");
        let data = &plot.highlight_sets[0].data;
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].color.as_deref(), Some("blue"));
        assert_eq!(data[1].color.as_deref(), Some("green"));
    }

    #[test]
    fn test_highlight_builder_r0_r1_apply_to_set_not_entry() {
        // r0 and r1 are per-set (not per-entry) — so even when you call them
        // on the builder returned by the second add_highlight, they mutate
        // the *set's* fraction fields.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_highlight("c1", 10, 20).r0(0.5).r1(0.7);
        plot.add_highlight("c1", 30, 40).r0(0.8).r1(0.9);
        // Last r0/r1 call wins since they target the same set.
        let set = &plot.highlight_sets[0];
        assert!((set.r0_frac - 0.8).abs() < 1e-9);
        assert!((set.r1_frac - 0.9).abs() < 1e-9);
    }

    #[test]
    fn test_plot_builder_add_text_records_label() {
        // PlotBuilder::add_text populates the Datum's `label` field, not `value`.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Text)
            .add_text("c1", 10, 20, "GeneA")
            .add_text("c1", 30, 40, "GeneB");
        let data = &plot.plot_sets[0].data;
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].label.as_deref(), Some("GeneA"));
        assert_eq!(data[1].label.as_deref(), Some("GeneB"));
        // `value` stays None since this is a Text plot using labels.
        assert!(data[0].value.is_none());
        assert!(data[1].value.is_none());
    }

    #[test]
    fn test_plot_builder_add_point_records_value() {
        // PlotBuilder::add_point populates value, leaves label=None.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Histogram)
            .add_point("c1", 0, 50, 0.25)
            .add_point("c1", 50, 100, 0.75);
        let data = &plot.plot_sets[0].data;
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].value, Some(0.25));
        assert_eq!(data[1].value, Some(0.75));
        assert!(data[0].label.is_none());
        assert!(data[1].label.is_none());
        // `set` field populated with IntSpan over [start, end].
        assert_eq!(data[0].set.cardinality(), 51);
    }

    #[test]
    fn test_new_default_seed_colors_contain_core_palette() {
        // CircosPlot::new seeds colors with white, black, red, green, blue.
        let plot = CircosPlot::new();
        let colors = &plot.colors;
        for name in ["white", "black", "red", "green", "blue"] {
            assert!(
                colors.resolve(name).is_some(),
                "expected seed color '{}' in palette",
                name
            );
        }
    }

    #[test]
    fn test_new_default_chromosome_colors_seeded() {
        // chr1..chrY all have colors so the default rendering works for human karyotypes.
        let plot = CircosPlot::new();
        for name in ["chr1", "chrX", "chrY"] {
            assert!(
                plot.colors.resolve(name).is_some(),
                "expected chr color '{}' in palette",
                name
            );
        }
    }

    #[test]
    fn test_new_default_cytoband_stain_colors_seeded() {
        // gneg/gpos25/gpos50/gpos75/gpos100/gvar/acen/stalk all present (karyotype stain types).
        let plot = CircosPlot::new();
        for name in ["gneg", "gpos25", "gpos50", "gpos75", "gpos100", "gvar", "acen", "stalk"] {
            assert!(
                plot.colors.resolve(name).is_some(),
                "expected cytoband stain '{}' in palette",
                name
            );
        }
    }

    #[test]
    fn test_new_default_numeric_fields_and_flags() {
        // Default CircosPlot has image_radius=1500, angle_offset=-90 (top-aligned),
        // chromosomes_units=1e6 (Mb), show_ticks=false, show_labels=true.
        let plot = CircosPlot::new();
        assert_eq!(plot.image_radius, 1500.0);
        assert_eq!(plot.angle_offset, -90.0);
        assert_eq!(plot.chromosomes_units, 1_000_000.0);
        assert!(!plot.show_ticks);
        assert!(plot.show_labels);
        assert_eq!(plot.ideogram_thickness, 100.0);
        assert_eq!(plot.ideogram_radius_frac, 0.85);
        assert_eq!(plot.spacing_frac, 0.005);
    }

    #[test]
    fn test_render_svg_ideograms_group_emitted() {
        // Every render_svg emits the "ideograms" group wrapper.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        let svg = plot.render_svg();
        assert!(svg.contains(r#"<g id="ideograms">"#));
    }

    #[test]
    fn test_render_svg_highlights_group_emitted_when_highlights_present() {
        // Adding a highlight causes the "highlights" group to appear in SVG.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_highlight("c1", 10, 20);
        let svg = plot.render_svg();
        assert!(svg.contains("highlights"));
    }

    #[test]
    fn test_render_svg_no_ticks_group_when_show_ticks_false() {
        // show_ticks=false → no ticks group emitted.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        let svg = plot.render_svg();
        assert!(!svg.contains(r#"<g id="ticks""#));
    }

    #[test]
    fn test_render_svg_show_ticks_emits_ticks_group() {
        // show_ticks=true (via add_ticks) → ticks group emitted.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 1000, "red");
        plot.add_ticks(100.0, 5.0, true);
        let svg = plot.render_svg();
        assert!(svg.contains("ticks"), "expected ticks group in SVG");
    }

    #[test]
    fn test_render_png_from_plot_with_chromosome_has_magic_bytes() {
        // A plot with a chromosome → render_png produces valid PNG magic bytes.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 1000, "red");
        let bytes = plot.render_png().unwrap();
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG magic
    }

    #[test]
    fn test_render_png_empty_plot_still_renders() {
        // Empty plot (no chromosomes) → PNG still generated (background fill).
        let plot = CircosPlot::new();
        let bytes = plot.render_png().unwrap();
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_render_png_scales_with_image_radius() {
        // Larger image_radius → PNG has larger byte count (more pixels).
        let mut p1 = CircosPlot::new();
        p1.image_radius = 500.0;
        let mut p2 = CircosPlot::new();
        p2.image_radius = 1000.0;
        let b1 = p1.render_png().unwrap();
        let b2 = p2.render_png().unwrap();
        assert!(b2.len() > b1.len());
    }

    #[test]
    fn test_render_png_with_plot_track_renders() {
        // Plot with actual data → renders without panic.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100_000, "red");
        plot.add_plot(PlotType::Scatter)
            .r0(0.5)
            .r1(0.8)
            .add_point("c1", 0, 1000, 0.5)
            .add_point("c1", 50_000, 51_000, 0.7);
        let bytes = plot.render_png().unwrap();
        // Valid PNG magic.
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_render_svg_width_height_from_image_radius() {
        // render_svg sets width=height=2*image_radius.
        let mut plot = CircosPlot::new();
        plot.image_radius = 800.0;
        let svg = plot.render_svg();
        assert!(svg.contains("width=\"1600px\""));
        assert!(svg.contains("height=\"1600px\""));
    }

    #[test]
    fn test_render_svg_background_rect_dimensions() {
        // Background rect has x=0, y=0, width=height=2*image_radius.
        let mut plot = CircosPlot::new();
        plot.image_radius = 1000.0;
        let svg = plot.render_svg();
        // Background rect uses width/height matching the SVG dimensions.
        assert!(svg.contains(r#"<rect x="0" y="0" width="2000" height="2000""#));
    }

    #[test]
    fn test_render_svg_plots_wrapped_in_indexed_groups() {
        // Each plot_set emits one `<g id="plot-N">` group in index order.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Histogram);
        plot.add_plot(PlotType::Scatter);
        plot.add_plot(PlotType::Line);
        let svg = plot.render_svg();
        assert!(svg.contains(r#"<g id="plot-0">"#));
        assert!(svg.contains(r#"<g id="plot-1">"#));
        assert!(svg.contains(r#"<g id="plot-2">"#));
    }

    #[test]
    fn test_render_svg_background_uses_resolved_color() {
        // Changing background name to "blue" renders the seed-blue RGB.
        let mut plot = CircosPlot::new();
        plot.background = "blue".into();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        let svg = plot.render_svg();
        // Seed blue = (54, 116, 217).
        assert!(svg.contains("fill: rgb(54,116,217)"));
    }

    #[test]
    fn test_add_link_preserves_chr_start_end_on_both_points() {
        // add_link builds a Link with 2 Datum points, each populated from the
        // respective chr/start/end args.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 10_000, "red");
        plot.add_chromosome("c2", "2", 0, 10_000, "blue");
        plot.add_link("l1", "c1", 100, 200, "c2", 500, 600);
        let link = &plot.link_sets[0].links[0];
        assert_eq!(link.points.len(), 2);
        assert_eq!(link.points[0].chr, "c1");
        assert_eq!(link.points[0].start, 100);
        assert_eq!(link.points[0].end, 200);
        assert_eq!(link.points[0].id.as_deref(), Some("l1"));
        assert_eq!(link.points[1].chr, "c2");
        assert_eq!(link.points[1].start, 500);
        assert_eq!(link.points[1].end, 600);
        // IntSpans on points.
        assert_eq!(link.points[0].set.cardinality(), 101);
        assert_eq!(link.points[1].set.cardinality(), 101);
    }

    #[test]
    fn test_new_link_set_pushes_fresh_set_with_defaults() {
        // new_link_set always appends a new LinkSet with default fields.
        let mut plot = CircosPlot::new();
        plot.new_link_set();
        plot.new_link_set();
        plot.new_link_set();
        assert_eq!(plot.link_sets.len(), 3);
        for ls in &plot.link_sets {
            assert_eq!(ls.radius_frac, 0.9);
            assert_eq!(ls.bezier_radius_frac, 0.2);
            assert_eq!(ls.crest, 1.0);
            assert_eq!(ls.color, "lgrey");
            assert_eq!(ls.thickness, 1.0);
            assert!(!ls.ribbon);
            assert!(ls.links.is_empty());
        }
    }

    #[test]
    fn test_add_link_multiple_links_share_current_set() {
        // Consecutive add_link calls without new_link_set stay in the same set.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 10_000, "red");
        plot.add_chromosome("c2", "2", 0, 10_000, "blue");
        plot.add_link("l1", "c1", 0, 10, "c2", 0, 10);
        plot.add_link("l2", "c1", 20, 30, "c2", 20, 30);
        plot.add_link("l3", "c1", 40, 50, "c2", 40, 50);
        assert_eq!(plot.link_sets.len(), 1);
        assert_eq!(plot.link_sets[0].links.len(), 3);
    }

    #[test]
    fn test_add_ticks_pushes_tick_def_and_enables_show() {
        // add_ticks pushes a TickDef + sets show_ticks=true.
        let mut plot = CircosPlot::new();
        assert!(!plot.show_ticks);
        plot.add_ticks(1_000_000.0, 5.0, true);
        assert!(plot.show_ticks);
        assert_eq!(plot.tick_defs.len(), 1);
        let t = &plot.tick_defs[0];
        assert_eq!(t.spacing_bp, 1_000_000.0);
        assert_eq!(t.size_px, 5.0);
        assert!(t.show_label);
        assert_eq!(t.thickness_px, 2.0);
        assert_eq!(t.format, "%d");
        // Multiple add_ticks calls append.
        plot.add_ticks(5_000_000.0, 10.0, false);
        assert_eq!(plot.tick_defs.len(), 2);
        assert!(plot.show_ticks); // stays on after first call
    }

    #[test]
    fn test_add_plot_defaults_applied_to_new_plot_set() {
        // add_plot creates a PlotSet with default field values.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Histogram);
        let ps = &plot.plot_sets[0];
        // Defaults: r0=0.5, r1=0.8, thickness=2.0, glyph_size=5.0, label_size=12.0, color=black.
        assert_eq!(ps.r0_frac, 0.5);
        assert_eq!(ps.r1_frac, 0.8);
        assert_eq!(ps.thickness, 2.0);
        assert_eq!(ps.glyph_size, 5.0);
        assert_eq!(ps.label_size, 12.0);
        assert_eq!(ps.color, "black");
        assert_eq!(ps.fill_color, "black");
    }

    #[test]
    fn test_plot_builder_fill_color_overrides_default() {
        // .fill_color("orange") overrides default "black" for fill.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Heatmap).fill_color("orange");
        assert_eq!(plot.plot_sets[0].fill_color, "orange");
        // `color` stays default.
        assert_eq!(plot.plot_sets[0].color, "black");
    }

    #[test]
    fn test_plot_builder_mixed_chain_preserves_last_call() {
        // Chained setters — the last call for each field wins.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_plot(PlotType::Scatter)
            .r0(0.1)
            .r0(0.3) // second call wins
            .r1(0.5)
            .glyph_size(3.0)
            .glyph_size(8.0); // second call wins
        let ps = &plot.plot_sets[0];
        assert_eq!(ps.r0_frac, 0.3);
        assert_eq!(ps.r1_frac, 0.5);
        assert_eq!(ps.glyph_size, 8.0);
    }

    #[test]
    fn test_plot_builder_data_accumulates_across_multiple_point_calls() {
        // Calling add_point multiple times in a chain accumulates data.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100_000, "red");
        plot.add_plot(PlotType::Line)
            .add_point("c1", 0, 10_000, 0.1)
            .add_point("c1", 10_000, 20_000, 0.2)
            .add_point("c1", 20_000, 30_000, 0.3)
            .add_point("c1", 30_000, 40_000, 0.4);
        assert_eq!(plot.plot_sets[0].data.len(), 4);
        // Values in order.
        let vs: Vec<f64> = plot.plot_sets[0]
            .data
            .iter()
            .filter_map(|d| d.value)
            .collect();
        assert_eq!(vs, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn test_add_chromosome_records_order_in_karyotype() {
        // add_chromosome appends to `karyotype.order` in call order.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c3", "3", 0, 100, "red");
        plot.add_chromosome("c1", "1", 0, 100, "green");
        plot.add_chromosome("c2", "2", 0, 100, "blue");
        assert_eq!(
            plot.karyotype.order,
            vec!["c3".to_string(), "c1".to_string(), "c2".to_string()]
        );
        // Indexes reflect insertion order: c3=0, c1=1, c2=2.
        assert_eq!(plot.karyotype.chromosomes["c3"].index, 0);
        assert_eq!(plot.karyotype.chromosomes["c1"].index, 1);
        assert_eq!(plot.karyotype.chromosomes["c2"].index, 2);
    }

    #[test]
    fn test_add_chromosome_display_defaults_to_true() {
        // Every added chromosome has display=true by default.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 500, "red");
        assert!(plot.karyotype.chromosomes["c1"].display);
        assert_eq!(plot.karyotype.chromosomes["c1"].start, 0);
        assert_eq!(plot.karyotype.chromosomes["c1"].end, 500);
        // IntSpan set cardinality = 501 for inclusive [0, 500].
        assert_eq!(plot.karyotype.chromosomes["c1"].set.cardinality(), 501);
    }

    #[test]
    fn test_add_band_creates_new_parent_entry_if_missing() {
        // add_band uses `entry(chr).or_default()` — first band creates the vec.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 1000, "red");
        // Adding band for chr with no prior bands.
        plot.add_band("c1", "p1", 0, 500, "gneg");
        assert!(plot.karyotype.bands.contains_key("c1"));
        assert_eq!(plot.karyotype.bands["c1"].len(), 1);
        // Adding another band for same chr → pushed to same vec.
        plot.add_band("c1", "p2", 500, 1000, "gpos");
        assert_eq!(plot.karyotype.bands["c1"].len(), 2);
        // Bands preserve call order.
        assert_eq!(plot.karyotype.bands["c1"][0].name, "p1");
        assert_eq!(plot.karyotype.bands["c1"][1].name, "p2");
    }

    #[test]
    fn test_add_band_populates_all_fields_from_args() {
        // Band fields: name/label (same), parent (chr), start/end/color/set.
        let mut plot = CircosPlot::new();
        plot.add_band("hs1", "myband", 10, 20, "gpos75");
        let b = &plot.karyotype.bands["hs1"][0];
        assert_eq!(b.name, "myband");
        assert_eq!(b.label, "myband");
        assert_eq!(b.parent, "hs1");
        assert_eq!(b.start, 10);
        assert_eq!(b.end, 20);
        assert_eq!(b.color, "gpos75");
        assert_eq!(b.set.cardinality(), 11);
    }

    #[test]
    fn test_define_color_then_render_uses_new_color() {
        // After `define_color("custom", 10, 20, 30)`, an ideogram with color
        // "custom" should have the color applied in the rendered SVG (rgb(10,20,30)).
        let mut plot = CircosPlot::new();
        plot.define_color("custom", 10, 20, 30);
        plot.add_chromosome("c1", "1", 0, 100, "custom");
        let svg = plot.render_svg();
        assert!(
            svg.contains("rgb(10,20,30)"),
            "expected custom color in SVG"
        );
    }

    #[test]
    fn test_circos_plot_new_defaults() {
        // CircosPlot::new() initializes standard defaults.
        let plot = CircosPlot::new();
        // image_radius + angle_offset + chromosomes_units defaults.
        assert!(plot.image_radius > 0.0);
        assert!(plot.chromosomes_units > 0.0);
        assert_eq!(plot.angle_offset, -90.0);
        // ideogram_radius_frac ∈ (0, 1).
        assert!(plot.ideogram_radius_frac > 0.0);
        assert!(plot.ideogram_radius_frac < 1.0);
        // Empty inputs by default.
        assert!(plot.karyotype.chromosomes.is_empty());
        assert!(plot.karyotype.bands.is_empty());
    }

    #[test]
    fn test_circos_plot_chain_multiple_chromosomes_preserves_order() {
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.add_chromosome("c2", "2", 0, 200, "blue");
        plot.add_chromosome("c3", "3", 0, 300, "green");
        // karyotype.order reflects insertion order.
        assert_eq!(plot.karyotype.order, vec!["c1", "c2", "c3"]);
        assert_eq!(plot.karyotype.chromosomes.len(), 3);
        // Each chr has correct label + start/end.
        assert_eq!(plot.karyotype.chromosomes["c2"].label, "2");
        assert_eq!(plot.karyotype.chromosomes["c3"].end, 300);
    }

    #[test]
    fn test_plot_type_all_variants_equality_and_debug() {
        // PlotType derives Debug + Copy + PartialEq. All 5 variants should be
        // distinct by PartialEq.
        for t in [PlotType::Histogram, PlotType::Heatmap, PlotType::Scatter, PlotType::Line, PlotType::Text] {
            // Self-equal via Copy.
            let t2 = t;
            assert_eq!(t, t2);
        }
        // Distinct variants unequal.
        assert_ne!(PlotType::Histogram, PlotType::Heatmap);
        assert_ne!(PlotType::Scatter, PlotType::Line);
        assert_ne!(PlotType::Line, PlotType::Text);
        // Debug emits the variant name.
        let s = format!("{:?}", PlotType::Heatmap);
        assert_eq!(s, "Heatmap");
    }

    #[test]
    fn test_add_highlight_records_entry_with_chr_start_end() {
        // add_highlight appends to an auto-created HighlightSet (only 1 set exists
        // unless the caller explicitly creates more). Both entries land in set[0].
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 1000, "red");
        plot.add_highlight("c1", 100, 200);
        plot.add_highlight("c1", 400, 500).color("blue");
        assert_eq!(plot.highlight_sets.len(), 1);
        assert_eq!(plot.highlight_sets[0].data.len(), 2);
        assert_eq!(plot.highlight_sets[0].data[0].chr, "c1");
        assert_eq!(plot.highlight_sets[0].data[0].start, 100);
        assert_eq!(plot.highlight_sets[0].data[0].end, 200);
        // Second entry got its color applied via the builder.
        assert_eq!(plot.highlight_sets[0].data[1].color.as_deref(), Some("blue"));
    }

    #[test]
    fn test_circos_plot_render_svg_without_chromosomes_still_valid() {
        // An empty plot still renders valid SVG (just empty circle).
        let plot = CircosPlot::new();
        let svg = plot.render_svg();
        assert!(svg.contains("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_circos_plot_render_png_without_chromosomes_valid_bytes() {
        // Empty plot → still a valid PNG (magic bytes present).
        let plot = CircosPlot::new();
        let png = plot.render_png().expect("empty plot PNG should render");
        assert!(png.len() > 8);
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn test_circos_plot_clone_preserves_chromosomes_and_colors() {
        // Clone yields an independent CircosPlot — mutations don't leak.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("c1", "1", 0, 100, "red");
        plot.define_color("custom", 10, 20, 30);
        let cloned = plot.clone();
        // Clone has the same state.
        assert_eq!(cloned.karyotype.chromosomes.len(), 1);
        assert!(cloned.karyotype.chromosomes.contains_key("c1"));
        // Mutate source — clone unaffected.
        plot.add_chromosome("c2", "2", 0, 200, "blue");
        assert_eq!(plot.karyotype.chromosomes.len(), 2);
        assert_eq!(cloned.karyotype.chromosomes.len(), 1);
    }

    #[test]
    fn test_plot_builder_r0_bounded_between_0_and_1() {
        // PlotBuilder.r0 accepts any fraction; typical range is [0, 1].
        let mut plot = CircosPlot::new();
        plot.add_plot(PlotType::Scatter).r0(0.0).r1(1.0);
        let set = plot.plot_sets.last().unwrap();
        assert_eq!(set.r0_frac, 0.0);
        assert_eq!(set.r1_frac, 1.0);
        // Values above 1 or below 0 are stored verbatim (no clamping).
        plot.add_plot(PlotType::Line).r0(-0.5).r1(1.5);
        let set = plot.plot_sets.last().unwrap();
        assert_eq!(set.r0_frac, -0.5);
        assert_eq!(set.r1_frac, 1.5);
    }

    #[test]
    fn test_circos_plot_image_radius_can_be_mutated_directly() {
        // CircosPlot.image_radius is a pub field — direct mutation allowed.
        let mut plot = CircosPlot::new();
        let before = plot.image_radius;
        plot.image_radius = 5000.0;
        assert_eq!(plot.image_radius, 5000.0);
        assert_ne!(before, plot.image_radius);
    }

    #[test]
    fn test_circos_plot_background_field_default_and_mutable() {
        // background is a mutable field.
        let mut plot = CircosPlot::new();
        let before = plot.background.clone();
        plot.background = "black".into();
        assert_eq!(plot.background, "black");
        assert_ne!(before, plot.background);
    }

    #[test]
    fn test_plot_builder_add_point_chain_returns_same_builder() {
        // PlotBuilder.add_point chains: multiple .add_point() calls all land in the same PlotSet.
        let mut plot = CircosPlot::new();
        plot.add_plot(PlotType::Scatter)
            .add_point("c1", 0, 100, 0.5)
            .add_point("c1", 200, 300, 0.8)
            .add_point("c2", 400, 500, -0.3);
        let set = plot.plot_sets.last().unwrap();
        assert_eq!(set.data.len(), 3);
        assert_eq!(set.data[0].chr, "c1");
        assert_eq!(set.data[1].start, 200);
        assert_eq!(set.data[2].value, Some(-0.3));
    }

    #[test]
    fn test_circos_plot_ideogram_thickness_and_spacing_frac_mutable() {
        // Two more public fields — both mutable.
        let mut plot = CircosPlot::new();
        plot.ideogram_thickness = 300.0;
        plot.spacing_frac = 0.02;
        assert_eq!(plot.ideogram_thickness, 300.0);
        assert_eq!(plot.spacing_frac, 0.02);
    }

    #[test]
    fn test_circos_plot_show_flags_mutable() {
        // show_ticks and show_labels are pub bool fields.
        let mut plot = CircosPlot::new();
        plot.show_ticks = !plot.show_ticks;
        plot.show_labels = !plot.show_labels;
        // Any flip succeeds.
        let _ = plot.show_ticks;
        let _ = plot.show_labels;
    }

    #[test]
    fn test_circos_plot_chromosomes_units_mutation_preserved() {
        // chromosomes_units is a pub f64 field.
        let mut plot = CircosPlot::new();
        plot.chromosomes_units = 5.0e6;
        assert_eq!(plot.chromosomes_units, 5.0e6);
    }

    #[test]
    fn test_plot_builder_color_chain_overrides_default() {
        // PlotBuilder.color("orange") stores "orange" as plot.color.
        let mut plot = CircosPlot::new();
        plot.add_plot(PlotType::Scatter).color("orange");
        let set = plot.plot_sets.last().unwrap();
        assert_eq!(set.color, "orange");
    }

    #[test]
    fn test_plot_type_debug_output_matches_variant_name() {
        // Debug derive: each variant formats to its identifier.
        assert_eq!(format!("{:?}", PlotType::Histogram), "Histogram");
        assert_eq!(format!("{:?}", PlotType::Heatmap), "Heatmap");
        assert_eq!(format!("{:?}", PlotType::Scatter), "Scatter");
        assert_eq!(format!("{:?}", PlotType::Line), "Line");
        assert_eq!(format!("{:?}", PlotType::Text), "Text");
    }

    #[test]
    fn test_highlight_builder_color_sets_specific_entry_only() {
        // HighlightBuilder.color() updates the individual entry's color field,
        // not a set-wide default.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("hs1", "1", 0, 1000, "gray");
        plot.add_highlight("hs1", 0, 100).color("red");
        plot.add_highlight("hs1", 200, 300).color("blue");
        // add_highlight auto-groups into a single set; two entries with different colors.
        let set = &plot.highlight_sets[0];
        assert_eq!(set.data[0].color.as_deref(), Some("red"));
        assert_eq!(set.data[1].color.as_deref(), Some("blue"));
    }

    #[test]
    fn test_highlight_builder_r0_r1_apply_to_set_level() {
        // r0 and r1 adjust the SET radius fractions, not per-entry fields.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("hs1", "1", 0, 1000, "gray");
        plot.add_highlight("hs1", 0, 100).r0(0.7).r1(0.9);
        let set = &plot.highlight_sets[0];
        assert!((set.r0_frac - 0.7).abs() < 1e-12);
        assert!((set.r1_frac - 0.9).abs() < 1e-12);
    }

    #[test]
    fn test_plot_builder_add_text_has_label_no_value() {
        // add_text creates a Datum with Some(label) + None value (unlike add_point).
        let mut plot = CircosPlot::new();
        plot.add_plot(PlotType::Text).add_text("hs1", 100, 200, "gene-A");
        let set = plot.plot_sets.last().unwrap();
        assert_eq!(set.data.len(), 1);
        assert_eq!(set.data[0].label.as_deref(), Some("gene-A"));
        assert!(set.data[0].value.is_none());
        assert_eq!(set.data[0].start, 100);
        assert_eq!(set.data[0].end, 200);
    }

    #[test]
    fn test_link_builder_thickness_stored_as_string_param() {
        // t.to_string() — 2.5 → "2.5", integer floats → "2" (no trailing .0).
        let mut plot = CircosPlot::new();
        plot.add_chromosome("hs1", "1", 0, 1000, "gray");
        plot.add_chromosome("hs2", "2", 0, 1000, "gray");
        plot.new_link_set();
        plot.add_link("ln1", "hs1", 0, 100, "hs2", 0, 100).thickness(2.5);
        let link = &plot.link_sets[0].links[0];
        assert_eq!(link.param.get("thickness").map(String::as_str), Some("2.5"));
        // Integer-valued float → no ".0".
        plot.add_link("ln2", "hs1", 0, 100, "hs2", 0, 100).thickness(4.0);
        let link2 = &plot.link_sets[0].links[1];
        assert_eq!(link2.param.get("thickness").map(String::as_str), Some("4"));
    }

    #[test]
    fn test_circos_plot_new_seeds_chr1_through_chr22_plus_xy() {
        // All 22 autosomes + chrX/chrY pre-seeded in colors map.
        let plot = CircosPlot::new();
        for i in 1..=22 {
            let key = format!("chr{}", i);
            assert!(plot.colors.colors.contains_key(&key), "missing {}", key);
        }
        assert!(plot.colors.colors.contains_key("chrX"));
        assert!(plot.colors.colors.contains_key("chrY"));
    }

    #[test]
    fn test_circos_plot_new_numeric_defaults_match_spec() {
        // Core numeric defaults: angle_offset=-90, image_radius=1500, etc.
        let plot = CircosPlot::new();
        assert_eq!(plot.angle_offset, -90.0);
        assert_eq!(plot.image_radius, 1500.0);
        assert_eq!(plot.chromosomes_units, 1_000_000.0);
        assert_eq!(plot.ideogram_thickness, 100.0);
        assert!((plot.ideogram_radius_frac - 0.85).abs() < 1e-12);
        assert!((plot.spacing_frac - 0.005).abs() < 1e-12);
        assert!(!plot.show_ticks);
        assert!(plot.show_labels);
    }

    #[test]
    fn test_define_color_overwrites_existing_color_entry() {
        // Second define_color with the same name replaces the first's RGB.
        let mut plot = CircosPlot::new();
        plot.define_color("red", 100, 100, 100);
        let c1 = *plot.colors.colors.get("red").unwrap();
        assert_eq!((c1.r, c1.g, c1.b), (100, 100, 100));
        plot.define_color("red", 200, 200, 200);
        let c2 = *plot.colors.colors.get("red").unwrap();
        assert_eq!((c2.r, c2.g, c2.b), (200, 200, 200));
    }

    #[test]
    fn test_add_chromosome_assigns_sequential_indices_from_zero() {
        // index field tracks insertion order (0-based).
        let mut plot = CircosPlot::new();
        plot.add_chromosome("hs1", "1", 0, 100, "red");
        plot.add_chromosome("hs2", "2", 0, 100, "green");
        plot.add_chromosome("hs3", "3", 0, 100, "blue");
        assert_eq!(plot.karyotype.chromosomes["hs1"].index, 0);
        assert_eq!(plot.karyotype.chromosomes["hs2"].index, 1);
        assert_eq!(plot.karyotype.chromosomes["hs3"].index, 2);
        assert_eq!(plot.karyotype.order, vec!["hs1", "hs2", "hs3"]);
    }

    #[test]
    fn test_link_radius_without_any_sets_is_noop() {
        // Calling link_radius with zero link_sets → no panic, no change.
        let mut plot = CircosPlot::new();
        assert_eq!(plot.link_sets.len(), 0);
        plot.link_radius(0.7);
        plot.link_ribbon(true);
        plot.link_color("green");
        // Still zero sets; no state change.
        assert_eq!(plot.link_sets.len(), 0);
    }

    #[test]
    fn test_link_setters_chain_modifies_last_set_only() {
        // Chained link_* calls operate on the most recently added set.
        let mut plot = CircosPlot::new();
        plot.new_link_set();
        plot.link_radius(0.8)
            .link_bezier_radius(0.5)
            .link_ribbon(true)
            .link_color("orange");
        let ls = &plot.link_sets[0];
        assert_eq!(ls.radius_frac, 0.8);
        assert_eq!(ls.bezier_radius_frac, 0.5);
        assert!(ls.ribbon);
        assert_eq!(ls.color, "orange");
    }

    #[test]
    fn test_new_link_set_appends_fresh_empty_set() {
        // Each new_link_set() call appends a new LinkSet with 0 links.
        let mut plot = CircosPlot::new();
        plot.new_link_set();
        plot.new_link_set();
        plot.new_link_set();
        assert_eq!(plot.link_sets.len(), 3);
        for ls in &plot.link_sets {
            assert!(ls.links.is_empty());
        }
    }

    #[test]
    fn test_add_highlight_seeds_first_set_with_default_r0_r1_and_color() {
        // First add_highlight creates a set with r0=0.9, r1=0.95, default_color="red".
        let mut plot = CircosPlot::new();
        plot.add_highlight("hs1", 0, 100);
        let set = &plot.highlight_sets[0];
        assert!((set.r0_frac - 0.9).abs() < 1e-12);
        assert!((set.r1_frac - 0.95).abs() < 1e-12);
        assert_eq!(set.default_color, "red");
        assert_eq!(set.stroke_thickness, 0.0);
    }

    #[test]
    fn test_add_band_accumulates_multiple_bands_under_same_parent() {
        // Multiple add_band for same chr → bands vec grows.
        let mut plot = CircosPlot::new();
        plot.add_chromosome("hs1", "1", 0, 10000, "gray");
        plot.add_band("hs1", "p1", 0, 1000, "gneg");
        plot.add_band("hs1", "p2", 1000, 2000, "gpos");
        plot.add_band("hs1", "p3", 2000, 3000, "gneg");
        let bands = &plot.karyotype.bands["hs1"];
        assert_eq!(bands.len(), 3);
        assert_eq!(bands[0].name, "p1");
        assert_eq!(bands[2].name, "p3");
    }

    #[test]
    fn test_add_ticks_toggles_show_ticks_and_appends_def() {
        // add_ticks enables show_ticks and appends to tick_defs.
        let mut plot = CircosPlot::new();
        assert!(!plot.show_ticks);
        assert_eq!(plot.tick_defs.len(), 0);
        plot.add_ticks(10_000_000.0, 15.0, true);
        assert!(plot.show_ticks);
        assert_eq!(plot.tick_defs.len(), 1);
        assert_eq!(plot.tick_defs[0].spacing_bp, 10_000_000.0);
        assert_eq!(plot.tick_defs[0].size_px, 15.0);
        assert!(plot.tick_defs[0].show_label);
        // Second call appends.
        plot.add_ticks(5_000_000.0, 8.0, false);
        assert_eq!(plot.tick_defs.len(), 2);
    }

    #[test]
    fn test_new_link_set_seeds_default_fractions_and_color() {
        // new_link_set defaults: radius_frac=0.9, bezier_radius_frac=0.2, color="lgrey", thickness=1.0, crest=1.0, ribbon=false.
        let mut plot = CircosPlot::new();
        plot.new_link_set();
        let ls = &plot.link_sets[0];
        assert!((ls.radius_frac - 0.9).abs() < 1e-12);
        assert!((ls.bezier_radius_frac - 0.2).abs() < 1e-12);
        assert!((ls.crest - 1.0).abs() < 1e-12);
        assert_eq!(ls.color, "lgrey");
        assert_eq!(ls.thickness, 1.0);
        assert!(!ls.ribbon);
        assert!(ls.rules.is_empty());
    }

    #[test]
    fn test_render_svg_empty_plot_returns_valid_svg_string() {
        // Empty plot → render_svg completes without panic and produces an <svg> root.
        let plot = CircosPlot::new();
        let svg = plot.render_svg();
        assert!(svg.contains("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }
}
