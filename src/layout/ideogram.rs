use crate::intspan::IntSpan;

/// A zoom/cover region within an ideogram.
#[derive(Debug, Clone)]
pub struct Cover {
    pub set: IntSpan,
    pub scale: f64,
}

/// An ideogram: a displayed chromosome region positioned on the circle.
#[derive(Debug, Clone)]
pub struct Ideogram {
    /// Chromosome name (e.g., "hs1").
    pub chr: String,
    /// Display label (e.g., "1").
    pub label: String,
    /// Unique tag (may differ from chr if user-specified).
    pub tag: String,
    /// Full chromosome length in base pairs.
    pub chrlength: i64,
    /// Genomic region displayed.
    pub set: IntSpan,
    /// Scale factor (affects displayed length).
    pub scale: f64,
    /// Whether the ideogram is drawn in reverse.
    pub reverse: bool,
    /// Original creation index.
    pub idx: usize,
    /// Display order index (determines angular position).
    pub display_idx: usize,
    /// Zoom regions (covers).
    pub covers: Vec<Cover>,
    /// Scaled length in base pairs.
    pub length_scaled: f64,
    /// Unscaled length in base pairs.
    pub length_noscale: f64,
    /// Cumulative scaled length of all preceding ideograms.
    pub length_cumulative_scaled: f64,
    /// Cumulative unscaled length.
    pub length_cumulative_noscale: f64,
    /// Outer radius in pixels.
    pub radius: f64,
    /// Inner radius in pixels.
    pub radius_inner: f64,
    /// Outer radius in pixels (same as radius).
    pub radius_outer: f64,
    /// Radial thickness in pixels.
    pub thickness: f64,
    /// Whether there's an axis break at the start.
    pub has_break_start: bool,
    /// Whether there's an axis break at the end.
    pub has_break_end: bool,
    /// Color name.
    pub color: String,
}
