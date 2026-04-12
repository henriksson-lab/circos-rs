use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::layout::ideogram::Ideogram;
use crate::layout::units;

/// Compute spacing between two adjacent ideograms.
pub fn ideogram_spacing(
    id1: &Ideogram,
    id2: &Ideogram,
    spacing_conf: Option<&HashMap<String, ConfigValue>>,
    default_spacing: f64,
    chromosomes_units: f64,
    gsize_noscale: f64,
    units_ok: &str,
    units_nounit: &str,
) -> f64 {
    // Check for pairwise overrides
    if let Some(conf) = spacing_conf {
        if let Some(pairwise) = conf.get("pairwise").and_then(|v| v.as_map()) {
            let keys = [&id1.chr, &id2.chr, &id1.tag, &id2.tag];
            for i in 0..keys.len() {
                for j in 0..keys.len() {
                    if i == j {
                        continue;
                    }
                    let key = format!("{};{}", keys[i], keys[j]);
                    if let Some(pw) = pairwise.get(&key).and_then(|v| v.as_map()) {
                        if let Some(sp_str) = pw.get("spacing").and_then(|v| v.as_str()) {
                            if let Ok((val, unit)) = units::unit_split(sp_str, units_ok, units_nounit) {
                                return match unit.as_str() {
                                    "u" => val * chromosomes_units,
                                    "b" => val,
                                    "r" => val * gsize_noscale,
                                    _ => val,
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    default_spacing
}
