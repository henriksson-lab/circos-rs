use std::collections::HashMap;

use crate::intspan::IntSpan;

/// A chromosome definition from the karyotype file.
#[derive(Debug, Clone)]
pub struct Chromosome {
    pub name: String,
    pub label: String,
    pub start: i64,
    pub end: i64,
    pub color: String,
    pub set: IntSpan,
    /// Index in the karyotype file (order of appearance).
    pub index: usize,
    /// Whether this chromosome should be displayed.
    pub display: bool,
    /// Display region (which parts to show after filtering).
    pub display_region: DisplayRegion,
}

/// Display region filters for a chromosome.
#[derive(Debug, Clone, Default)]
pub struct DisplayRegion {
    pub accept: IntSpan,
    pub reject: IntSpan,
}

/// A cytogenetic band on a chromosome.
#[derive(Debug, Clone)]
pub struct Band {
    pub name: String,
    pub label: String,
    pub parent: String,
    pub start: i64,
    pub end: i64,
    pub color: String,
    pub set: IntSpan,
}

/// The full karyotype: chromosomes and their bands.
#[derive(Debug, Clone, Default)]
pub struct Karyotype {
    pub chromosomes: HashMap<String, Chromosome>,
    pub bands: HashMap<String, Vec<Band>>,
    /// Chromosome names in order of appearance in the karyotype file.
    pub order: Vec<String>,
}
