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
    pub fn color(self, color: &str) -> Self {
        self.plot.link_sets[self.set_idx].links[self.link_idx]
            .param
            .insert("color".into(), color.into());
        self
    }
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
    pub fn color(self, color: &str) -> Self {
        self.plot.highlight_sets[self.set_idx].data[self.entry_idx].color = Some(color.into());
        self
    }
    pub fn r0(self, r0: f64) -> Self {
        self.plot.highlight_sets[self.set_idx].r0_frac = r0;
        self
    }
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
    pub fn r0(self, r0: f64) -> Self {
        self.plot.plot_sets[self.set_idx].r0_frac = r0;
        self
    }
    pub fn r1(self, r1: f64) -> Self {
        self.plot.plot_sets[self.set_idx].r1_frac = r1;
        self
    }
    pub fn color(self, color: &str) -> Self {
        self.plot.plot_sets[self.set_idx].color = color.into();
        self
    }
    pub fn fill_color(self, color: &str) -> Self {
        self.plot.plot_sets[self.set_idx].fill_color = color.into();
        self
    }
    pub fn thickness(self, t: f64) -> Self {
        self.plot.plot_sets[self.set_idx].thickness = t;
        self
    }
    pub fn glyph_size(self, s: f64) -> Self {
        self.plot.plot_sets[self.set_idx].glyph_size = s;
        self
    }
    pub fn label_size(self, s: f64) -> Self {
        self.plot.plot_sets[self.set_idx].label_size = s;
        self
    }
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
    pub fn add_highlight(
        &mut self,
        chr: &str,
        start: i64,
        end: i64,
    ) -> HighlightBuilder<'_> {
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
    pub fn add_ticks(
        &mut self,
        spacing_bp: f64,
        size_px: f64,
        show_label: bool,
    ) -> &mut Self {
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
                width, height, bg.to_svg_rgb()
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
        let tree = usvg::Tree::from_str(&svg, &options)
            .map_err(|e| format!("SVG parse error: {}", e))?;
        let size = tree.size();
        let w = size.width().ceil() as u32;
        let h = size.height().ceil() as u32;
        let mut pixmap =
            tiny_skia::Pixmap::new(w, h).ok_or("failed to create pixmap")?;
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        Ok(pixmap.encode_png().map_err(|e| format!("PNG encode error: {}", e))?)
    }

    // ── Internal ───────────────────────────────────────────────────────

    fn build_layout(&self) -> Layout {
        // Build a minimal config HashMap for Layout::build
        let mut conf: HashMap<String, crate::config::types::ConfigValue> = HashMap::new();
        use crate::config::types::ConfigValue as CV;

        // Image block
        let mut image = HashMap::new();
        image.insert("radius".into(), CV::Str(format!("{}p", self.image_radius)));
        image.insert("angle_offset".into(), CV::Str(self.angle_offset.to_string()));
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
        ideogram.insert("show_label".into(), CV::Str(if self.show_labels { "1" } else { "0" }.into()));
        ideogram.insert("fill".into(), CV::Str("1".into()));
        ideogram.insert("stroke_thickness".into(), CV::Str("2".into()));
        ideogram.insert("stroke_color".into(), CV::Str("black".into()));
        ideogram.insert("show_bands".into(), CV::Str("1".into()));
        ideogram.insert("fill_bands".into(), CV::Str("1".into()));
        ideogram.insert("label_size".into(), CV::Str("36".into()));

        let mut spacing = HashMap::new();
        spacing.insert(
            "default".into(),
            CV::Str(format!("{}r", self.spacing_frac)),
        );
        ideogram.insert("spacing".into(), CV::Map(spacing));
        conf.insert("ideogram".into(), CV::Map(ideogram));

        conf.insert("chromosomes_units".into(), CV::Str(self.chromosomes_units.to_string()));
        conf.insert("chromosomes_display_default".into(), CV::Str("1".into()));
        conf.insert("units_ok".into(), CV::Str("bupr".into()));
        conf.insert("units_nounit".into(), CV::Str("n".into()));

        Layout::build(&conf, &self.karyotype).expect("failed to build layout")
    }

    fn draw_ideograms_api(&self, doc: &mut SvgDocument, layout: &Layout) {
        use crate::render::svg::svg_slice;
        let stroke = self.colors.resolve("black").unwrap_or(Color::rgb(0, 0, 0));
        let label_color = stroke;

        doc.open_group("ideograms");

        for ideo in &layout.ideograms {
            let start = ideo.set.min().unwrap_or(0);
            let end = ideo.set.max().unwrap_or(0);
            let start_a = layout.get_angle(start, &ideo.chr).unwrap_or(0.0);
            let end_a = layout.get_angle(end, &ideo.chr).unwrap_or(0.0);

            let ro = if ideo.radius_outer > 0.0 { ideo.radius_outer } else { layout.dims.ideogram_radius_outer };
            let ri = if ideo.radius_inner > 0.0 { ideo.radius_inner } else { layout.dims.ideogram_radius_inner };

            // Fill
            let fill = self.colors.resolve(&ideo.color);
            doc.add(svg_slice(layout, start_a, end_a, ri, ro, Some(&stroke), Some(2.0), fill.as_ref(), None));

            // Bands
            if let Some(bands) = self.karyotype.bands.get(&ideo.chr) {
                for band in bands {
                    let bs = band.set.intersect(&ideo.set);
                    if bs.cardinality() < 1 { continue; }
                    let ba = layout.get_angle(bs.min().unwrap(), &ideo.chr).unwrap_or(0.0);
                    let be = layout.get_angle(bs.max().unwrap(), &ideo.chr).unwrap_or(0.0);
                    let fc = self.colors.resolve(&band.color);
                    doc.add(svg_slice(layout, ba, be, ri, ro, Some(&stroke), Some(2.0), fc.as_ref(), None));
                }
            }

            // Outline
            doc.add(svg_slice(layout, start_a, end_a, ri, ro, Some(&stroke), Some(2.0), None, None));

            // Label
            if self.show_labels {
                let mid = (start + end) / 2;
                let mid_a = layout.get_angle(mid, &ideo.chr).unwrap_or(0.0);
                let lr = ro + 36.0 * 0.8;
                doc.add(crate::render::svg::svg_text(layout, mid_a, lr, &ideo.label, 36.0, &label_color, 0.0));
            }
        }
        doc.close_group();
    }

    fn draw_ticks_api(&self, doc: &mut SvgDocument, layout: &Layout) {
        use crate::render::svg::svg_tick;
        doc.open_group("ticks");

        for td in &self.tick_defs {
            let color = self.colors.resolve(&td.color).unwrap_or(Color::rgb(0, 0, 0));
            for ideo in &layout.ideograms {
                let s = ideo.set.min().unwrap_or(0);
                let e = ideo.set.max().unwrap_or(0);
                let ro = if ideo.radius_outer > 0.0 { ideo.radius_outer } else { layout.dims.ideogram_radius_outer };

                let first = ((s as f64 / td.spacing_bp).ceil() * td.spacing_bp) as i64;
                let mut pos = first;
                while pos <= e {
                    if let Some(a) = layout.get_angle(pos, &ideo.chr) {
                        doc.add(svg_tick(layout, a, ro, ro + td.size_px, td.thickness_px, &color));
                        if td.show_label {
                            let v = pos as f64 * td.multiplier;
                            let label = match td.format.as_str() {
                                "%d" => format!("{}", v as i64),
                                _ => format!("{}", v as i64),
                            };
                            let lr = ro + td.size_px + td.label_size * 0.5;
                            doc.add(crate::render::svg::svg_text(layout, a, lr, &label, td.label_size, &color, 0.0));
                        }
                    }
                    pos += td.spacing_bp as i64;
                }
            }
        }
        doc.close_group();
    }

    fn draw_highlights_api(&self, doc: &mut SvgDocument, layout: &Layout, hs: &HighlightSet) {
        use crate::render::svg::svg_slice;
        let r0 = hs.r0_frac * layout.dims.ideogram_radius;
        let r1 = hs.r1_frac * layout.dims.ideogram_radius;

        doc.open_group("highlights");
        for entry in &hs.data {
            if layout.find_ideogram_by_chr(&entry.chr).is_none() { continue; }
            let sa = match layout.get_angle(entry.start, &entry.chr) { Some(a) => a, None => continue };
            let ea = match layout.get_angle(entry.end, &entry.chr) { Some(a) => a, None => continue };
            let cn = entry.color.as_deref().unwrap_or(&hs.default_color);
            let fc = self.colors.resolve(cn);
            doc.add(svg_slice(layout, sa, ea, r0.min(r1), r0.max(r1), None, None, fc.as_ref(), None));
        }
        doc.close_group();
    }

    fn draw_links_api(&self, doc: &mut SvgDocument, layout: &Layout, ls: &LinkSet) {
        use crate::draw::links;
        use crate::config::types::ConfigValue as CV;

        let mut defaults: HashMap<String, String> = HashMap::new();
        defaults.insert("radius".into(), format!("{}r", ls.radius_frac));
        defaults.insert("bezier_radius".into(), format!("{}r", ls.bezier_radius_frac));
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

        plot.add_link("link1", "chr1", 10_000_000, 15_000_000, "chr2", 20_000_000, 25_000_000)
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
}
