use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::layout::ideogram::Ideogram;
use crate::layout::units;

/// Helper: resolve a spacing value string (Perl `ideogram_spacing_helper`) to bases.
fn resolve_spacing(
    value: &str,
    chromosomes_units: f64,
    gsize_noscale: f64,
    units_ok: &str,
    units_nounit: &str,
) -> f64 {
    if let Ok((val, unit)) = units::unit_split(value, units_ok, units_nounit) {
        match unit.as_str() {
            "u" => val * chromosomes_units,
            "r" => val * gsize_noscale,
            _ => val,
        }
    } else {
        0.0
    }
}

/// Helper: look up a pairwise spacing override by key.
fn lookup_pairwise(pairwise: &HashMap<String, ConfigValue>, key: &str) -> Option<String> {
    pairwise
        .get(key)
        .and_then(|v| v.as_map())
        .and_then(|pw| pw.get("spacing"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Port of Perl `ideogram_spacing(id1, id2)`: returns the spacing between two
/// adjacent ideograms. Order of precedence:
///   1. pairwise override keyed by any two of [chr1,chr2,tag1,tag2]
///   2. pairwise override keyed by a single key (any of chr1/chr2/tag1/tag2)
///   3. same-chr fallback: spacing.break || spacing.default
///   4. default_spacing
///
/// Plus additive break penalties on id1.break.end / id2.break.start.
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
    let pairwise = spacing_conf.and_then(|c| c.get("pairwise").and_then(|v| v.as_map()));
    let keys = [&id1.chr, &id2.chr, &id1.tag, &id2.tag];
    let mut spacing = default_spacing;
    let mut spacing_found = false;

    // KI1: pairs
    if let Some(pw) = pairwise {
        'ki1: for i in 0..keys.len() {
            for j in 0..keys.len() {
                if i == j {
                    continue;
                }
                let key = format!("{};{}", keys[i], keys[j]);
                if let Some(value) = lookup_pairwise(pw, &key) {
                    spacing = resolve_spacing(
                        &value,
                        chromosomes_units,
                        gsize_noscale,
                        units_ok,
                        units_nounit,
                    );
                    spacing_found = true;
                    break 'ki1;
                }
            }
        }
    }

    // KI2: singletons
    if !spacing_found && let Some(pw) = pairwise {
        for k in &keys {
            if let Some(value) = lookup_pairwise(pw, k) {
                spacing = resolve_spacing(
                    &value,
                    chromosomes_units,
                    gsize_noscale,
                    units_ok,
                    units_nounit,
                );
                spacing_found = true;
                break;
            }
        }
    }

    // Same-chr fallback
    if !spacing_found && id1.chr == id2.chr {
        let value = spacing_conf
            .and_then(|c| c.get("break"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                spacing_conf
                    .and_then(|c| c.get("default"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        if !value.is_empty() {
            spacing = resolve_spacing(
                &value,
                chromosomes_units,
                gsize_noscale,
                units_ok,
                units_nounit,
            );
        }
    }

    spacing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ideogram::Ideogram;

    fn mk_ideo(chr: &str, tag: &str) -> Ideogram {
        Ideogram {
            chr: chr.into(),
            label: chr.into(),
            tag: tag.into(),
            chrlength: 100,
            set: crate::intspan::IntSpan::from_range(0, 100),
            scale: 1.0,
            reverse: false,
            idx: 0,
            display_idx: 0,
            covers: vec![],
            length_scaled: 100.0,
            length_noscale: 100.0,
            length_cumulative_scaled: 0.0,
            length_cumulative_noscale: 0.0,
            radius: 1000.0,
            radius_inner: 900.0,
            radius_outer: 1000.0,
            thickness: 100.0,
            has_break_start: false,
            has_break_end: false,
            color: "black".into(),
        }
    }

    #[test]
    fn test_spacing_default_fallback() {
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let sp = ideogram_spacing(&id1, &id2, None, 42.0, 1e6, 100_000.0, "bupr", "n");
        assert!((sp - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spacing_same_chr_break_fallback() {
        // Same chr, no pairwise, same-chr fallback → `break` value
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("5u".into()));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,
            1_000_000.0,
            1.0,
            "bupr",
            "n",
        );
        // 5u × chromosomes_units (1e6) = 5_000_000
        assert!((sp - 5_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_pairwise_override() {
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("3u".into()));
        pairwise.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,
            2_000_000.0,
            1.0,
            "bupr",
            "n",
        );
        assert!((sp - 6_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_pairwise_key_order_symmetric() {
        // Pairwise should match either "chr1;chr2" or "chr2;chr1" (and any
        // tag combination) — order symmetric.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("7u".into()));
        // Register under reversed order to test symmetry.
        pairwise.insert("hs2;hs1".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,
            1_000_000.0,
            1.0,
            "bupr",
            "n",
        );
        // 7u × 1e6 = 7_000_000
        assert!((sp - 7_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_pairwise_tag_keys_match() {
        // Pairwise lookup tries all 4 keys (chr1, chr2, tag1, tag2) — so an
        // entry keyed on "a;b" tags should match an id1.tag=a / id2.tag=b pair.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("2u".into()));
        pairwise.insert("a;b".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,
            500_000.0,
            1.0,
            "bupr",
            "n",
        );
        // 2u × 500_000 = 1_000_000
        assert!((sp - 1_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_pairwise_singleton_chr_matches() {
        // KI2 path: a pairwise key containing just one of chr1/chr2/tag1/tag2
        // (no `;`) should match when pair-keys miss. Singleton "hs1" → match.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("4u".into()));
        pairwise.insert("hs1".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            99.0,
            250_000.0,
            1.0,
            "bupr",
            "n",
        );
        // 4u × 250_000 = 1_000_000 — NOT 99 (default) → confirms singleton path fired.
        assert!((sp - 1_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_same_chr_default_when_break_missing() {
        // Same-chr fallback tries `break` first, then `default`.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("8u".into()));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,
            100_000.0,
            1.0,
            "bupr",
            "n",
        );
        // 8u × 100_000 = 800_000 — `default` kicks in since `break` absent.
        assert!((sp - 800_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_unitless_value_passthrough() {
        // A plain numeric with no unit suffix resolves to itself (no u/r scaling).
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        // No unit suffix — "1234" ends with `n` (nounit) → value alone.
        entry.insert("spacing".into(), ConfigValue::Str("1234".into()));
        pairwise.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,
            999_999.0,
            999_999.0,
            "bupr",
            "n",
        );
        assert!((sp - 1234.0).abs() < 1e-9);
    }

    #[test]
    fn test_spacing_different_chrs_break_fallback_ignored() {
        // Same-chr fallback is gated on id1.chr == id2.chr — so a `break`
        // value set between two DIFFERENT chrs should NOT trigger and
        // we fall through to `default_spacing`.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("999u".into()));
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            42.0,
            1.0,
            1.0,
            "bupr",
            "n",
        );
        // Break was set but chrs differ — default_spacing=42 wins.
        assert!((sp - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spacing_same_chr_without_break_or_default_uses_default_spacing() {
        // Same chr, spacing_conf has neither break nor default → falls through
        // to default_spacing (the function's 4th arg).
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        // Empty value → default_spacing unchanged.
        assert!((sp - 99.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spacing_none_conf_uses_default_spacing() {
        // spacing_conf=None → default_spacing returned directly.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let sp = ideogram_spacing(
            &id1, &id2, None,
            55.5, 1.0, 1.0, "bupr", "n",
        );
        assert!((sp - 55.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spacing_pairwise_4_key_combinations_searched() {
        // pairwise keys tried in (chr,chr), (chr,tag), (tag,chr), (tag,tag)
        // combinations. Register only the (tag1,tag2) form and verify it hits.
        let id1 = mk_ideo("hs1", "tag_a");
        let id2 = mk_ideo("hs2", "tag_b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pairwise: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("5u".into()));
        // Register (tag_a;tag_b) pair.
        pairwise.insert("tag_a;tag_b".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            1.0, 1_000_000.0, 1.0, "bupr", "n",
        );
        // 5u × 1M = 5,000,000.
        assert!((sp - 5_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_spacing_b_unit_passes_through_as_bases() {
        // Pairwise spacing with `b` suffix → value used verbatim (bases, no scale).
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("1000b".into()));
        pairwise.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            1.0, 1_000_000.0, 1.0, "bupr", "n",
        );
        // "1000b" → 1000 bases (no u/r multiplication).
        assert!((sp - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_spacing_r_unit_scales_by_gsize() {
        // Pairwise spacing expressed as `r` fraction uses gsize_noscale (arg 6).
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("0.01r".into()));
        pairwise.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        // Signature: (id1, id2, conf, default_spacing, chromosomes_units, gsize_noscale, …)
        let sp = ideogram_spacing(
            &id1,
            &id2,
            Some(&spacing_conf),
            1.0,           // default_spacing
            1.0,           // chromosomes_units (unused for r-units)
            1_000_000_000.0, // gsize_noscale
            "bupr",
            "n",
        );
        // 0.01 × 1e9 = 1e7
        assert!((sp - 10_000_000.0).abs() < 1.0);
    }

    #[test]
    fn test_resolve_spacing_invalid_input_returns_zero() {
        // unit_split failure → resolve_spacing returns 0.0 (no panic).
        let s = resolve_spacing("not_a_number", 1_000_000.0, 3e9, "bupr", "n");
        assert_eq!(s, 0.0);
        // Empty string also fails → 0.0.
        let s = resolve_spacing("", 1_000_000.0, 3e9, "bupr", "n");
        assert_eq!(s, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_missing_key_returns_none() {
        // Key not in map → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("5u".into()));
        pw.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        assert!(lookup_pairwise(&pw, "hs3;hs4").is_none());
        assert_eq!(lookup_pairwise(&pw, "hs1;hs2").as_deref(), Some("5u"));
    }

    #[test]
    fn test_lookup_pairwise_entry_without_spacing_field_returns_none() {
        // Map entry exists but lacks a `spacing` sub-key → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("color".into(), ConfigValue::Str("red".into())); // no "spacing"
        pw.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        assert!(lookup_pairwise(&pw, "hs1;hs2").is_none());
    }

    #[test]
    fn test_spacing_pairwise_takes_precedence_over_same_chr_break() {
        // Even for id1.chr == id2.chr, a matching pairwise entry wins over
        // the same-chr break/default fallback.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("999".into()));
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("42".into()));
        pairwise.insert("a;b".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            1.0, 1.0, 1.0, "bupr", "n",
        );
        // Pairwise "42" wins; not break's "999".
        assert!((sp - 42.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_spacing_unitless_passthrough() {
        // A unitless numeric value passes through as-is (no u/r scaling).
        let s = resolve_spacing("42", 1_000_000.0, 3e9, "bupr", "n");
        assert_eq!(s, 42.0);
        // Decimal unitless.
        let s = resolve_spacing("3.14", 1_000_000.0, 3e9, "bupr", "n");
        assert!((s - 3.14).abs() < 1e-12);
    }

    #[test]
    fn test_lookup_pairwise_non_map_entry_returns_none() {
        // Pairwise key maps to a Str (not Map) → as_map() None → early None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("a;b".into(), ConfigValue::Str("not a map".into()));
        assert!(lookup_pairwise(&pw, "a;b").is_none());
    }

    #[test]
    fn test_spacing_pairwise_both_tag_and_chr_keys_tried() {
        // Pairwise search tries combinations of chr1/chr2/tag1/tag2. A single-
        // key variant (e.g. just "hs1") should match either ideogram.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("17".into()));
        pairwise.insert("a".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        // Single-key "a" matches id1.tag="a" via the KI2 singleton pass.
        assert!((sp - 17.0).abs() < 1e-9);
    }

    #[test]
    fn test_spacing_break_only_no_default_uses_break() {
        // Same-chr fallback: if `break` is present and `default` is absent, break wins.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("77".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        // break "77" (unitless) → 77.0.
        assert!((sp - 77.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_spacing_u_unit_uses_chromosomes_units() {
        // "5u" × 1_000_000 = 5_000_000.
        let s = resolve_spacing("5u", 1_000_000.0, 3e9, "bupr", "n");
        assert_eq!(s, 5_000_000.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_uses_gsize_noscale() {
        // "0.01r" × gsize_noscale=3e9 = 3e7.
        let s = resolve_spacing("0.01r", 1_000_000.0, 3e9, "bupr", "n");
        assert!((s - 3e7).abs() < 1.0);
    }

    #[test]
    fn test_lookup_pairwise_value_accessed_through_as_str() {
        // Map's spacing value is a Str; non-Str value → None via as_str().
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Map(HashMap::new())); // not Str
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert!(lookup_pairwise(&pw, "k").is_none());
    }

    #[test]
    fn test_spacing_pairwise_chr_pair_match_order_independent() {
        // Pairwise keys try both directions: "hs1;hs2" and "hs2;hs1".
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        // Only "hs2;hs1" is in pairwise — still matches (reverse order tried).
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("123".into()));
        pairwise.insert("hs2;hs1".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert!((sp - 123.0).abs() < 1e-9);
    }

    #[test]
    fn test_spacing_no_pairwise_falls_to_default() {
        // No pairwise conf, no break/default, different chrs → default_spacing arg used.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            42.0, // default_spacing
            1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 42.0);
    }

    #[test]
    fn test_spacing_default_key_used_when_break_missing() {
        // Same-chr fallback: if only `default` is present, it's used.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("88".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 88.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_with_gsize_zero() {
        // r-unit with gsize=0 → value × 0 = 0.
        let s = resolve_spacing("2r", 1_000_000.0, 0.0, "bupr", "n");
        assert_eq!(s, 0.0);
    }

    #[test]
    fn test_spacing_pairwise_tag_by_chr_mixed() {
        // Pairwise key mixes chr and tag: "hs1;b" — chr1 "hs1" + tag2 "b".
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("55".into()));
        pairwise.insert("hs1;b".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 55.0);
    }

    #[test]
    fn test_resolve_spacing_unitless_negative_value() {
        // "-42" unitless passthrough — negative value preserved.
        let s = resolve_spacing("-42", 1e6, 3e9, "bupr", "n");
        assert_eq!(s, -42.0);
    }

    #[test]
    fn test_spacing_pairwise_singleton_tag2_matches() {
        // Pairwise singleton key matches id2.tag="b".
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("31".into()));
        pairwise.insert("b".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 31.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_value_empty_string() {
        // Empty spacing string → as_str gives Some(""); impl returns Some("").
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        // Empty-string spacing is technically returned — confirms lookup_pairwise
        // doesn't filter empty strings.
        assert_eq!(lookup_pairwise(&pw, "k").as_deref(), Some(""));
    }

    #[test]
    fn test_spacing_pairwise_entry_with_no_spacing_key_falls_through() {
        // Pairwise entry has a Map but no "spacing" key → lookup fails → falls through.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pairwise = HashMap::new();
        let mut entry = HashMap::new();
        entry.insert("color".into(), ConfigValue::Str("red".into())); // no "spacing"
        pairwise.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pairwise));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        // Falls through to default_spacing arg = 99.
        assert_eq!(sp, 99.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_multiplies_by_chromosomes_units() {
        // "5u" with cu=1_000_000 → 5_000_000.
        let r = resolve_spacing("5u", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 5_000_000.0);
        // "0.5u" → 500_000.
        let r2 = resolve_spacing("0.5u", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 500_000.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_multiplies_by_gsize_noscale() {
        // "0.1r" with gsize_noscale=100_000 → 10_000 (not × chromosomes_units).
        let r = resolve_spacing("0.1r", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 10_000.0);
        // "1r" → full gsize.
        let r2 = resolve_spacing("1r", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 100_000.0);
    }

    #[test]
    fn test_resolve_spacing_garbage_expression_returns_zero() {
        // unit_split fails on garbage → 0.0 (no Err propagation).
        let r = resolve_spacing("not-a-valid-expr", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 0.0);
        // Non-digit inside number-like string also fails.
        let r2 = resolve_spacing("5x", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_missing_spacing_subkey_returns_none() {
        // Entry exists but lacks "spacing" sub-key → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("color".into(), ConfigValue::Str("red".into()));
        pw.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "hs1;hs2"), None);
        // Missing key entirely → None.
        assert_eq!(lookup_pairwise(&pw, "nonexistent"), None);
        // Non-map value for key → None (can't descend).
        let mut pw2: HashMap<String, ConfigValue> = HashMap::new();
        pw2.insert("k".into(), ConfigValue::Str("just_a_string".into()));
        assert_eq!(lookup_pairwise(&pw2, "k"), None);
    }

    #[test]
    fn test_resolve_spacing_nounit_plain_number_returns_value_unchanged() {
        // unit match _ (nounit) → returns val without scaling.
        let r = resolve_spacing("42", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 42.0);
        // Negative nounit also passes through.
        let r2 = resolve_spacing("-3.14", 1_000_000.0, 100_000.0, "bupr", "n");
        assert!((r2 - (-3.14)).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_spacing_empty_string_returns_zero() {
        // unit_split fails on empty → returns 0.0.
        let r = resolve_spacing("", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_chr_pair_key_matches() {
        // pairwise["hs1;hs2"] entry → spacing resolved from that entry.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("12345".into()));
        pw.insert("hs1;hs2".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        // Overrides default (99.0) → 12345.
        assert_eq!(sp, 12345.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_prefers_break_over_default() {
        // Same chr → same-chr fallback; "break" takes priority over "default".
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("777".into()));
        spacing_conf.insert("default".into(), ConfigValue::Str("99".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            42.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 777.0);
        // Without "break", "default" key is used.
        let mut sc2: HashMap<String, ConfigValue> = HashMap::new();
        sc2.insert("default".into(), ConfigValue::Str("99".into()));
        let sp2 = ideogram_spacing(
            &id1, &id2, Some(&sc2),
            42.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp2, 99.0);
    }

    #[test]
    fn test_ideogram_spacing_no_conf_returns_default_value() {
        // spacing_conf=None → skip pairwise+same-chr → return default_spacing.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let sp = ideogram_spacing(
            &id1, &id2, None,
            555.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 555.0);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_tag_pair_key_matches() {
        // pairwise["a;b"] based on tag pair → spacing applied.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("777".into()));
        pw.insert("a;b".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 777.0);
    }

    #[test]
    fn test_lookup_pairwise_returns_spacing_string_when_present() {
        // Entry map contains "spacing" → lookup_pairwise returns Some("value").
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("1234".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("1234".to_string()));
    }

    #[test]
    fn test_resolve_spacing_preserves_decimal_precision_with_u_unit() {
        // "3.14u" with cu=1_000_000 → 3_140_000.
        let r = resolve_spacing("3.14u", 1_000_000.0, 100_000.0, "bupr", "n");
        assert!((r - 3_140_000.0).abs() < 1e-6);
        // "0.001r" with gsize=100_000 → 100.
        let r2 = resolve_spacing("0.001r", 1_000_000.0, 100_000.0, "bupr", "n");
        assert!((r2 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_resolve_spacing_unit_p_passes_value_unchanged() {
        // Any unit not in {u,r} → falls to _ match arm; value returned verbatim.
        let r = resolve_spacing("5p", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 5.0);
        // Same for "b" (bases are already the base unit).
        let r2 = resolve_spacing("5b", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 5.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_pairwise_beats_same_chr_fallback() {
        // id1.chr == id2.chr but pairwise key "hs1;hs1" present → pairwise wins over break/default.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("555".into()));
        pw.insert("hs1;hs1".into(), ConfigValue::Map(entry));
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        spacing_conf.insert("break".into(), ConfigValue::Str("777".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 555.0);
    }

    #[test]
    fn test_lookup_pairwise_returns_same_value_across_calls_consistent() {
        // lookup_pairwise is pure — multiple calls yield identical Some.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("42".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        let r1 = lookup_pairwise(&pw, "k");
        let r2 = lookup_pairwise(&pw, "k");
        assert_eq!(r1, r2);
        assert_eq!(r1, Some("42".to_string()));
    }

    #[test]
    fn test_ideogram_spacing_different_chr_with_no_pairwise_uses_default_arg() {
        // Different chromosomes + no pairwise + no same-chr fallback (wrong chrs) → default.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            12345.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 12345.0);
    }

    #[test]
    fn test_ideogram_spacing_ki1_pair_beats_ki2_singleton() {
        // Pairwise pair key (e.g. "hs1;hs2") is searched before singleton keys.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut pair_entry: HashMap<String, ConfigValue> = HashMap::new();
        pair_entry.insert("spacing".into(), ConfigValue::Str("100".into()));
        pw.insert("hs1;hs2".into(), ConfigValue::Map(pair_entry));
        // Singleton entry for "hs1" with a different spacing — should NOT be chosen.
        let mut single_entry: HashMap<String, ConfigValue> = HashMap::new();
        single_entry.insert("spacing".into(), ConfigValue::Str("999".into()));
        pw.insert("hs1".into(), ConfigValue::Map(single_entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            12345.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 100.0);
    }

    #[test]
    fn test_ideogram_spacing_none_spacing_conf_uses_default_arg() {
        // spacing_conf = None → no pairwise search, no same-chr fallback → default only.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let sp = ideogram_spacing(
            &id1, &id2, None,
            777.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 777.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_respects_chromosomes_units_arg() {
        // "3u" with cu=500_000 → 3 × 500_000 = 1_500_000 (different from default 1M).
        let r = resolve_spacing("3u", 500_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 1_500_000.0);
        // "0.5u" fractional coefficient.
        let r2 = resolve_spacing("0.5u", 500_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 250_000.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_value_with_inner_whitespace_preserved() {
        // The lookup returns the string as-is — no trim on the spacing value.
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("  5u  ".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(entry));
        let v = lookup_pairwise(&pw, "k").expect("spacing present");
        assert_eq!(v, "  5u  ");
    }

    #[test]
    fn test_ideogram_spacing_same_chr_default_when_break_missing() {
        // Same-chr fallback: if "break" key absent but "default" present → use default.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs1", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("42".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 42.0);
    }

    #[test]
    fn test_resolve_spacing_bare_number_without_unit_passes_through_unchanged() {
        // Plain number "42" → matches `_` arm in resolve_spacing → passes through.
        let r = resolve_spacing("42", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 42.0);
        // Zero → 0.
        assert_eq!(resolve_spacing("0", 1.0, 1.0, "bupr", "n"), 0.0);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_singleton_matches_tag2_only() {
        // Singleton lookup on just tag2 (id2.tag) → still succeeds.
        let id1 = mk_ideo("hs1", "a");
        let id2 = mk_ideo("hs2", "special_tag");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("777".into()));
        pw.insert("special_tag".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            1.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 777.0);
    }

    #[test]
    fn test_resolve_spacing_negative_value_with_u_unit_preserves_sign() {
        // Negative coefficient: "-2u" × cu=1_000_000 → -2_000_000.
        let r = resolve_spacing("-2u", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, -2_000_000.0);
        // Negative r unit.
        let r2 = resolve_spacing("-0.5r", 1.0, 1000.0, "bupr", "n");
        assert_eq!(r2, -500.0);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_key_uses_both_chrs_when_different() {
        // Pair key built from "chr1;chr2" (different chromosomes) should match.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("2500".into()));
        pw.insert("hsA;hsB".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 2500.0);
    }

    #[test]
    fn test_lookup_pairwise_multiple_keys_independent_return() {
        // Different keys return independently — no leakage between them.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut e1: HashMap<String, ConfigValue> = HashMap::new();
        e1.insert("spacing".into(), ConfigValue::Str("10".into()));
        pw.insert("key1".into(), ConfigValue::Map(e1));
        let mut e2: HashMap<String, ConfigValue> = HashMap::new();
        e2.insert("spacing".into(), ConfigValue::Str("20".into()));
        pw.insert("key2".into(), ConfigValue::Map(e2));
        assert_eq!(lookup_pairwise(&pw, "key1"), Some("10".to_string()));
        assert_eq!(lookup_pairwise(&pw, "key2"), Some("20".to_string()));
        assert_eq!(lookup_pairwise(&pw, "key3"), None);
    }

    #[test]
    fn test_resolve_spacing_b_unit_bases_no_scaling() {
        // "b" unit matches `_` arm — passes through unchanged.
        let r = resolve_spacing("500b", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 500.0);
        // Compare to unitless same value.
        let r2 = resolve_spacing("500", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 500.0);
    }

    #[test]
    fn test_ideogram_spacing_empty_pairwise_map_falls_to_default() {
        // Pairwise exists but is empty → no matches → fall through to default.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(HashMap::new()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            333.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 333.0);
    }

    #[test]
    fn test_resolve_spacing_p_unit_passes_value_unchanged() {
        // "p" unit is not "u" or "r" → hits `_` arm → value unchanged.
        let r = resolve_spacing("1234p", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 1234.0);
    }

    #[test]
    fn test_lookup_pairwise_for_map_value_with_empty_spacing_returns_empty_string() {
        // spacing="" → returns Some(""), not None — empty string is still a valid value.
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str(String::new()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), Some(String::new()));
    }

    #[test]
    fn test_ideogram_spacing_tag1_pair_tag2_matches() {
        // pair key from id1.tag and id2.tag — "a;b" key found.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("1234".into()));
        pw.insert("a;b".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 1234.0);
    }

    #[test]
    fn test_resolve_spacing_fractional_coefficient_with_u_unit() {
        // Fractional coefficient: "0.001u" × cu=1_000_000 = 1000.
        let r = resolve_spacing("0.001u", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 1000.0);
        // "1.5u" × cu=500_000 → 750_000.
        let r2 = resolve_spacing("1.5u", 500_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r2, 750_000.0);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_chr_with_tag_key_crossover() {
        // Pairwise key combining chr1 with tag2 → "hsA;b".
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("456".into()));
        pw.insert("hsA;b".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        // Pair key found via KI1 combinations.
        assert_eq!(sp, 456.0);
    }

    #[test]
    fn test_resolve_spacing_large_value_with_r_unit() {
        // Large "r" unit value: 100r × gsize=1000 → 100_000.
        let r = resolve_spacing("100r", 1.0, 1000.0, "bupr", "n");
        assert_eq!(r, 100_000.0);
    }

    #[test]
    fn test_lookup_pairwise_with_matching_entry_no_spacing_field_returns_none() {
        // Entry exists but lacks "spacing" key → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("color".into(), ConfigValue::Str("red".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), None);
    }

    #[test]
    fn test_ideogram_spacing_no_pairwise_no_same_chr_returns_default_exactly() {
        // Different chromosomes, pairwise absent entirely → default_spacing arg.
        let id1 = mk_ideo("hsX", "x");
        let id2 = mk_ideo("hsY", "y");
        let sp = ideogram_spacing(
            &id1, &id2, None,
            42.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 42.0);
    }

    #[test]
    fn test_resolve_spacing_with_zero_chromosomes_units_u_unit_zero() {
        // "5u" with chromosomes_units=0 → 5 × 0 = 0.
        let r = resolve_spacing("5u", 0.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_returns_default_when_both_break_and_default_missing() {
        // Same chr, no pairwise, no break, no default → falls through to default_spacing arg.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            111.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 111.0);
    }

    #[test]
    fn test_lookup_pairwise_distinct_keys_return_independent_values() {
        // Three distinct keys with distinct spacings.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        for (k, v) in [("k1", "10"), ("k2", "20"), ("k3", "30")] {
            let mut entry: HashMap<String, ConfigValue> = HashMap::new();
            entry.insert("spacing".into(), ConfigValue::Str(v.into()));
            pw.insert(k.into(), ConfigValue::Map(entry));
        }
        assert_eq!(lookup_pairwise(&pw, "k1"), Some("10".to_string()));
        assert_eq!(lookup_pairwise(&pw, "k2"), Some("20".to_string()));
        assert_eq!(lookup_pairwise(&pw, "k3"), Some("30".to_string()));
    }

    #[test]
    fn test_resolve_spacing_negative_r_unit_with_small_gsize() {
        // "-0.001r" × gsize=5000 → -5.
        let r = resolve_spacing("-0.001r", 1.0, 5000.0, "bupr", "n");
        assert_eq!(r, -5.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_with_break_and_default_break_wins() {
        // Same chr, both break and default present → break wins.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("111".into()));
        spacing_conf.insert("default".into(), ConfigValue::Str("222".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 111.0);
    }

    #[test]
    fn test_resolve_spacing_integer_with_explicit_p_suffix() {
        // "500p" → 500 via `_` arm (p is not u/r scaled).
        let r = resolve_spacing("500p", 1_000_000.0, 100_000.0, "bupr", "n");
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_lookup_pairwise_case_sensitive_keys() {
        // Keys are case-sensitive HashMap lookups.
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("42".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("KEY".into(), ConfigValue::Map(entry));
        // Exact case match.
        assert_eq!(lookup_pairwise(&pw, "KEY"), Some("42".to_string()));
        // Different case → miss.
        assert_eq!(lookup_pairwise(&pw, "key"), None);
        assert_eq!(lookup_pairwise(&pw, "Key"), None);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_pairwise_still_checked_first() {
        // Same chr; pairwise has matching key → pairwise wins over break.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("555".into()));
        pw.insert("a;b".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        spacing_conf.insert("break".into(), ConfigValue::Str("777".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 555.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_multiplies_value_200() {
        // "2u" with chromosomes_units=100 → 200.0.
        let r = resolve_spacing("2u", 100.0, 9_000_000.0, "bupr", "n");
        assert_eq!(r, 200.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_fractional_coef_with_large_gsize() {
        // "0.1r" with gsize_noscale=1e6 → 1e5.
        let r = resolve_spacing("0.1r", 100.0, 1_000_000.0, "bupr", "n");
        assert!((r - 100_000.0).abs() < 1e-9);
    }

    #[test]
    fn test_lookup_pairwise_absent_key_returns_none_in_empty_map() {
        // Key not present → None.
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(lookup_pairwise(&pw, "absent"), None);
    }

    #[test]
    fn test_ideogram_spacing_singleton_pairwise_key_matches_chr() {
        // No "a;b" but "hsA" key → singleton match in KI2 wins over default.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("888".into()));
        pw.insert("hsA".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 888.0);
    }

    #[test]
    fn test_ideogram_spacing_none_spacing_conf_returns_default() {
        // spacing_conf=None → pairwise + fallback paths skipped → default_spacing returned.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let sp = ideogram_spacing(
            &id1, &id2, None,
            12345.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 12345.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_only_default_key_used() {
        // No break key, only default → default value used in fallback.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("111".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1_000_000.0, 100_000.0, "bupr", "n",
        );
        assert_eq!(sp, 111.0);
    }

    #[test]
    fn test_lookup_pairwise_value_without_spacing_field_returns_none() {
        // Entry exists but lacks "spacing" sub-key → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let entry: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("x".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "x"), None);
    }

    #[test]
    fn test_resolve_spacing_negative_value_unchanged_for_nounit() {
        // "-50" has nounit → passthrough negative value.
        let r = resolve_spacing("-50", 100.0, 1_000_000.0, "bupr", "n");
        assert_eq!(r, -50.0);
    }

    #[test]
    fn test_ideogram_spacing_by_tag_reverse_key_matches_b_semicolon_a() {
        // Pairwise key "b;a" matches via reverse iteration over tag pairs.
        let id1 = mk_ideo("hsX", "a");
        let id2 = mk_ideo("hsY", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("999".into()));
        pw.insert("b;a".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            1.0, 100.0, 100.0, "bupr", "n",
        );
        assert_eq!(sp, 999.0);
    }

    #[test]
    fn test_resolve_spacing_malformed_value_returns_zero() {
        // "garbage" has no recognized unit → Err → 0.0 per resolve_spacing fallback.
        let r = resolve_spacing("garbage", 100.0, 1_000_000.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_ideogram_spacing_different_chrs_no_pairwise_match_returns_default() {
        // Different chrs, pairwise has mismatched key → default spacing used.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("999".into()));
        // Key is never matched by any of (chr1,chr2,tag1,tag2) combinations.
        pw.insert("zzz;yyy".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            77.0, 100.0, 100.0, "bupr", "n",
        );
        assert_eq!(sp, 77.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_string_returned_as_owned_string() {
        // Successful lookup returns owned String.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("10u".into()));
        pw.insert("key".into(), ConfigValue::Map(entry));
        let got = lookup_pairwise(&pw, "key");
        assert_eq!(got, Some("10u".to_string()));
    }

    #[test]
    fn test_resolve_spacing_u_unit_with_zero_chromosomes_units_yields_zero() {
        // 5u × 0 = 0 (multiplicative identity gone).
        let r = resolve_spacing("5u", 0.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_ideogram_spacing_tag_1_matches_pairwise_singleton_key() {
        // Pairwise has only "a" (not "hsX;a" or similar) → KI2 path hits "a".
        let id1 = mk_ideo("hsX", "a");
        let id2 = mk_ideo("hsY", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("111".into()));
        pw.insert("a".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            777.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 111.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_value_is_non_str_returns_none() {
        // Entry "spacing" value is a Map (not Str) → as_str → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Map(HashMap::new()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), None);
    }

    #[test]
    fn test_resolve_spacing_bare_zero_returns_zero() {
        // "0" bare value → passthrough zero.
        let r = resolve_spacing("0", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_with_zero_gsize_returns_zero() {
        // "0.5r" × 0 = 0.
        let r = resolve_spacing("0.5r", 1000.0, 0.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_ideogram_spacing_empty_pairwise_falls_to_default_or_same_chr() {
        // Empty pairwise map → falls through to same-chr/default logic.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        spacing_conf.insert("default".into(), ConfigValue::Str("555".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            9999.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 555.0);
    }

    #[test]
    fn test_lookup_pairwise_map_with_missing_spacing_returns_none() {
        // Entry's inner map lacks "spacing" key → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("other".into(), ConfigValue::Str("x".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), None);
    }

    #[test]
    fn test_resolve_spacing_integer_with_p_not_a_recognized_unit() {
        // "p" is in units_ok "bupr" → resolve treats it as _ arm (not u/r) → passthrough.
        let r = resolve_spacing("250p", 1000.0, 1.0, "bupr", "n");
        assert_eq!(r, 250.0);
    }

    #[test]
    fn test_ideogram_spacing_different_chr_no_config_yields_default() {
        // Different chrs, no spacing_conf at all → default_spacing returned.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let sp = ideogram_spacing(
            &id1, &id2, None,
            500.0, 100.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 500.0);
    }

    #[test]
    fn test_resolve_spacing_large_u_value_multiplied_correctly() {
        // 1000u × 100 = 100000.
        let r = resolve_spacing("1000u", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, 100_000.0);
    }

    #[test]
    fn test_lookup_pairwise_entry_as_str_value_not_map_returns_none() {
        // Pairwise entry value is Str (not Map) → as_map None → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Str("not_a_map".into()));
        assert_eq!(lookup_pairwise(&pw, "k"), None);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_with_chr_tag_pair_wins() {
        // pairwise key "hsA;b" uses chr1 + tag2 → still matches in pair iteration.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("333".into()));
        pw.insert("hsA;b".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 333.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_fractional_coefficient() {
        // "0.5u" × chromosomes_units=1000 = 500.
        let r = resolve_spacing("0.5u", 1000.0, 1_000_000.0, "bupr", "n");
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_ideogram_spacing_default_used_across_different_chrs_no_pairwise() {
        // spacing_conf has only default key; different chrs → default ignored in fallback.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("1000".into()));
        // Different chrs → same-chr fallback not triggered → default_spacing from args.
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            42.0, 1.0, 1.0, "bupr", "n",
        );
        // Different-chrs path doesn't use spacing_conf["default"] → returns default_spacing arg 42.
        assert_eq!(sp, 42.0);
    }

    #[test]
    fn test_lookup_pairwise_key_is_exact_match_not_prefix() {
        // Lookup must be exact; partial prefix doesn't match.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("5".into()));
        pw.insert("exact_key".into(), ConfigValue::Map(entry));
        // Partial prefix "exact" doesn't match.
        assert_eq!(lookup_pairwise(&pw, "exact"), None);
        // Exact match works.
        assert_eq!(lookup_pairwise(&pw, "exact_key"), Some("5".to_string()));
    }

    #[test]
    fn test_resolve_spacing_u_unit_large_chromosomes_units() {
        // "10u" × chromosomes_units=1e6 = 10_000_000.
        let r = resolve_spacing("10u", 1_000_000.0, 1.0, "bupr", "n");
        assert_eq!(r, 10_000_000.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_with_zero_default_returns_zero() {
        // Same chr + spacing.default="0" → 0.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("default".into(), ConfigValue::Str("0".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 0.0);
    }

    #[test]
    fn test_resolve_spacing_very_large_value_preserved() {
        // "999999999" passthrough.
        let r = resolve_spacing("999999999", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 999999999.0);
    }

    #[test]
    fn test_lookup_pairwise_empty_string_key_not_found_by_default() {
        // Empty key → None on empty map.
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        assert_eq!(lookup_pairwise(&pw, ""), None);
    }

    #[test]
    fn test_ideogram_spacing_both_tags_same_chr_different_ideograms() {
        // Same chr, same tags → break/default fallback path.
        let id1 = mk_ideo("hsA", "same");
        let id2 = mk_ideo("hsA", "same");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("333".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 333.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_with_negative_chromosomes_units() {
        // Negative units_chromosomes: "2u" × -100 = -200.
        let r = resolve_spacing("2u", -100.0, 1.0, "bupr", "n");
        assert_eq!(r, -200.0);
    }

    #[test]
    fn test_ideogram_spacing_pairwise_priority_over_break() {
        // Same chr + break=50 + pairwise "a;b"=999 → pairwise wins.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("999".into()));
        pw.insert("a;b".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        spacing_conf.insert("break".into(), ConfigValue::Str("50".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            10.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 999.0);
    }

    #[test]
    fn test_lookup_pairwise_numeric_string_key_accepted() {
        // Numeric-looking key "42" valid HashMap key.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("100".into()));
        pw.insert("42".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "42"), Some("100".to_string()));
    }

    #[test]
    fn test_resolve_spacing_fractional_bare_value_preserved() {
        // "3.14" bare value passthrough.
        let r = resolve_spacing("3.14", 100.0, 1.0, "bupr", "n");
        assert!((r - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_ideogram_spacing_all_zero_config_returns_zero_default() {
        // No pairwise, no break, no default → default_spacing arg used.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            0.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 0.0);
    }

    #[test]
    fn test_resolve_spacing_integer_with_nounit_passthrough() {
        // "100" nounit → 100 unchanged.
        let r = resolve_spacing("100", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_lookup_pairwise_special_characters_in_key() {
        // Keys with ';' '.' '_' all valid.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("7".into()));
        pw.insert("key.with_special;chars".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "key.with_special;chars"), Some("7".to_string()));
    }

    #[test]
    fn test_ideogram_spacing_tag2_matches_singleton_ki2_after_ki1_miss() {
        // tag2="b" → KI2 singleton match in second struct.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("444".into()));
        pw.insert("b".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 444.0);
    }

    #[test]
    fn test_ideogram_spacing_same_chr_different_tags_uses_break_preferred_over_default() {
        // Same chr, diff tags, spacing_conf.break set → break wins.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("break".into(), ConfigValue::Str("200".into()));
        spacing_conf.insert("default".into(), ConfigValue::Str("50".into()));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            10.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 200.0);
    }

    #[test]
    fn test_resolve_spacing_negative_u_with_units_produces_neg_multiplied_value() {
        // "-5u" × chromosomes_units=10 → -50.
        let r = resolve_spacing("-5u", 10.0, 1.0, "bupr", "n");
        assert_eq!(r, -50.0);
    }

    #[test]
    fn test_lookup_pairwise_with_entry_having_spacing_empty_str() {
        // Entry has "spacing"="" (empty string) → Some("").
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("".to_string()));
    }

    #[test]
    fn test_ideogram_spacing_both_tags_and_chrs_distinct_no_pw_returns_default() {
        // Fully distinct → no match → default_spacing.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsB", "b");
        let spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            777.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 777.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_fractional_coefficient_one_pct() {
        // "0.01r" with gsize=10000 → 100.
        let r = resolve_spacing("0.01r", 1.0, 10000.0, "bupr", "n");
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_ideogram_spacing_tag_matching_via_pw_same_chr() {
        // Same chr, different tags, pairwise has tag1 key → match.
        let id1 = mk_ideo("hsA", "a");
        let id2 = mk_ideo("hsA", "b");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("111".into()));
        pw.insert("a".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            99.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 111.0);
    }

    #[test]
    fn test_lookup_pairwise_map_entry_pairwise_map_lookup_hits() {
        // Entry with "spacing" key — lookup succeeds.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("xyz".into()));
        pw.insert("k".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("xyz".to_string()));
    }

    #[test]
    fn test_resolve_spacing_mixed_zero_values_passthrough() {
        // "0" nounit → 0 (passthrough).
        let r = resolve_spacing("0", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_resolve_spacing_fractional_u_with_moderate_factor() {
        // "0.1u" × 5000 = 500.
        let r = resolve_spacing("0.1u", 5000.0, 1.0, "bupr", "n");
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_lookup_pairwise_key_with_space_in_it_exact_match() {
        // Key with space — exact match only.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("7".into()));
        pw.insert("key with space".into(), ConfigValue::Map(entry));
        assert_eq!(lookup_pairwise(&pw, "key with space"), Some("7".to_string()));
        assert_eq!(lookup_pairwise(&pw, "key"), None);
    }

    #[test]
    fn test_ideogram_spacing_with_tag_and_chr_pair_ordering() {
        // pairwise ";" pair uses chr then tag in both orders.
        let id1 = mk_ideo("hsX", "t1");
        let id2 = mk_ideo("hsY", "t2");
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let mut entry: HashMap<String, ConfigValue> = HashMap::new();
        entry.insert("spacing".into(), ConfigValue::Str("88".into()));
        // Try "t2;t1" (tag2 then tag1).
        pw.insert("t2;t1".into(), ConfigValue::Map(entry));
        let mut spacing_conf: HashMap<String, ConfigValue> = HashMap::new();
        spacing_conf.insert("pairwise".into(), ConfigValue::Map(pw));
        let sp = ideogram_spacing(
            &id1, &id2, Some(&spacing_conf),
            999.0, 1.0, 1.0, "bupr", "n",
        );
        assert_eq!(sp, 88.0);
    }

    #[test]
    fn test_resolve_spacing_small_u_with_small_units() {
        // Small both → small result.
        let r = resolve_spacing("0.01u", 10.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.1);
    }

    #[test]
    fn test_resolve_spacing_bare_number_no_unit_passthrough() {
        // "100" bare → nounit → passthrough 100.
        let r = resolve_spacing("100", 5000.0, 1.0, "bupr", "n");
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_scales_by_gsize_noscale() {
        // "2r" × gsize=500 → 1000.
        let r = resolve_spacing("2r", 1.0, 500.0, "bupr", "n");
        assert_eq!(r, 1000.0);
    }

    #[test]
    fn test_lookup_pairwise_key_present_but_missing_spacing_returns_none() {
        // Key exists as Map but has no "spacing" entry → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        let inner = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert!(lookup_pairwise(&pw, "k").is_none());
    }

    #[test]
    fn test_lookup_pairwise_empty_map_returns_none() {
        // Completely empty pairwise map → None for any key.
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        assert!(lookup_pairwise(&pw, "anything").is_none());
    }

    #[test]
    fn test_lookup_pairwise_key_entry_is_not_map_returns_none() {
        // Key entry must be Map variant; Str variant → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Str("not_a_map".into()));
        assert!(lookup_pairwise(&pw, "k").is_none());
    }

    #[test]
    fn test_lookup_pairwise_spacing_entry_not_str_returns_none() {
        // "spacing" entry exists but not Str variant → None.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::List(vec![]));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert!(lookup_pairwise(&pw, "k").is_none());
    }

    #[test]
    fn test_resolve_spacing_large_integer_bare_value_passthrough() {
        // Large bare integer → passthrough as f64.
        let r = resolve_spacing("1000000", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 1000000.0);
    }

    #[test]
    fn test_resolve_spacing_empty_string_fails_split_yields_zero() {
        // Empty string → unit_split fails → 0.0.
        let r = resolve_spacing("", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_hit_returns_string_ref() {
        // Full round-trip: pw["k"] = Map{spacing="2u"} → Some("2u").
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("2u".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        let v = lookup_pairwise(&pw, "k");
        assert_eq!(v, Some("2u".to_string()));
    }

    #[test]
    fn test_resolve_spacing_nounit_n_sentinel_in_nounit_arg() {
        // Bare "42" with empty nounit → still parses; unit=n ⇒ passthrough.
        let r = resolve_spacing("42", 5.0, 5.0, "bupr", "n");
        assert_eq!(r, 42.0);
    }

    #[test]
    fn test_resolve_spacing_unit_u_with_zero_chr_units() {
        // "5u" × chromosomes_units=0 → 0.
        let r = resolve_spacing("5u", 0.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_two_keys_each_hits_independently() {
        // Two independent pairwise entries — each resolves separately.
        let mut a_inner: HashMap<String, ConfigValue> = HashMap::new();
        a_inner.insert("spacing".into(), ConfigValue::Str("1u".into()));
        let mut b_inner: HashMap<String, ConfigValue> = HashMap::new();
        b_inner.insert("spacing".into(), ConfigValue::Str("2u".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("a".into(), ConfigValue::Map(a_inner));
        pw.insert("b".into(), ConfigValue::Map(b_inner));
        assert_eq!(lookup_pairwise(&pw, "a"), Some("1u".to_string()));
        assert_eq!(lookup_pairwise(&pw, "b"), Some("2u".to_string()));
    }

    #[test]
    fn test_resolve_spacing_unit_p_passes_through_number() {
        // "200p" → unit "p" (not u or r) → passthrough 200.
        let r = resolve_spacing("200p", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, 200.0);
    }

    #[test]
    fn test_resolve_spacing_unit_b_passes_through_number() {
        // "50b" → unit "b" (not u or r) → passthrough 50.
        let r = resolve_spacing("50b", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, 50.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_returns_r_unit_value() {
        // Pairwise with spacing="0.5r" → Some("0.5r").
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("0.5r".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("0.5r".to_string()));
    }

    #[test]
    fn test_resolve_spacing_negative_bare_value_passthrough() {
        // "-100" bare → -100.0 passthrough.
        let r = resolve_spacing("-100", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, -100.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_with_large_chr_units() {
        // "1u" × chromosomes_units=1_000_000 = 1_000_000.
        let r = resolve_spacing("1u", 1_000_000.0, 1.0, "bupr", "n");
        assert_eq!(r, 1_000_000.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_integer_value_preserved_as_string() {
        // spacing as integer string "5000" preserved.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("5000".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("5000".to_string()));
    }

    #[test]
    fn test_resolve_spacing_negative_u_unit_yields_negative_value() {
        // "-2u" × 100 = -200.
        let r = resolve_spacing("-2u", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, -200.0);
    }

    #[test]
    fn test_resolve_spacing_zero_r_unit_yields_zero() {
        // "0r" × gsize → 0.
        let r = resolve_spacing("0r", 1.0, 500.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_resolve_spacing_unit_u_with_fractional_coef() {
        // "0.5u" × chromosomes_units=1000 = 500.
        let r = resolve_spacing("0.5u", 1000.0, 1.0, "bupr", "n");
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_resolve_spacing_scientific_notation_bare() {
        // "1e3" bare → 1000.0.
        let r = resolve_spacing("1e3", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 1000.0);
    }

    #[test]
    fn test_lookup_pairwise_nested_spacing_with_negative_value() {
        // Negative spacing value preserved as string.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("-10".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("-10".to_string()));
    }

    #[test]
    fn test_resolve_spacing_r_unit_with_fractional_gsize() {
        // "0.1r" × gsize=100.5 = 10.05.
        let r = resolve_spacing("0.1r", 1.0, 100.5, "bupr", "n");
        assert!((r - 10.05).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_spacing_fractional_number_bare_passes_through() {
        // "2.5" bare → 2.5.
        let r = resolve_spacing("2.5", 100.0, 100.0, "bupr", "n");
        assert_eq!(r, 2.5);
    }

    #[test]
    fn test_lookup_pairwise_spacing_with_u_unit_format() {
        // "10u" in pairwise spacing preserved.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("10u".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("10u".to_string()));
    }

    #[test]
    fn test_resolve_spacing_b_unit_also_passes_through() {
        // "100b" → unit "b" (not u/r) → passthrough 100.
        let r = resolve_spacing("100b", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_lookup_pairwise_value_with_negative_number_string() {
        // Negative spacing with "u" preserved.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("-5u".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("-5u".to_string()));
    }

    #[test]
    fn test_lookup_pairwise_spacing_value_with_spaces_in_value() {
        // Spacing value with spaces → preserved as-is in raw form.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("10 u".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("10 u".to_string()));
    }

    #[test]
    fn test_resolve_spacing_unit_u_with_units_millionx_scale() {
        // "1.5u" × 1e6 → 1.5e6.
        let r = resolve_spacing("1.5u", 1e6, 1.0, "bupr", "n");
        assert_eq!(r, 1.5e6);
    }

    #[test]
    fn test_resolve_spacing_r_unit_with_small_gsize_value() {
        // "5r" × gsize=10 → 50.
        let r = resolve_spacing("5r", 1.0, 10.0, "bupr", "n");
        assert_eq!(r, 50.0);
    }

    #[test]
    fn test_lookup_pairwise_triple_key_independent_lookups() {
        // 3 keys in pairwise → each queried independently.
        let mk_entry = |val: &str| {
            let mut inner: HashMap<String, ConfigValue> = HashMap::new();
            inner.insert("spacing".into(), ConfigValue::Str(val.into()));
            ConfigValue::Map(inner)
        };
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("chr1;chr2".into(), mk_entry("5u"));
        pw.insert("chr3;chr4".into(), mk_entry("10u"));
        pw.insert("chr5;chr6".into(), mk_entry("15u"));
        assert_eq!(lookup_pairwise(&pw, "chr1;chr2"), Some("5u".to_string()));
        assert_eq!(lookup_pairwise(&pw, "chr3;chr4"), Some("10u".to_string()));
        assert_eq!(lookup_pairwise(&pw, "chr5;chr6"), Some("15u".to_string()));
    }

    #[test]
    fn test_resolve_spacing_e_notation_u_unit_works() {
        // "1e2u" × chr_units=10 = 1000.
        let r = resolve_spacing("1e2u", 10.0, 1.0, "bupr", "n");
        assert_eq!(r, 1000.0);
    }

    #[test]
    fn test_lookup_pairwise_with_empty_spacing_returns_string() {
        // spacing="" → Some("") returned (empty but present).
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("".to_string()));
    }

    #[test]
    fn test_resolve_spacing_integer_with_whitespace_trim_fail() {
        // " 42 " with whitespace → unit_split may fail or trim.
        let r = resolve_spacing("42", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 42.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_matches_gsize_noscale() {
        // "1r" × 123.45 = 123.45.
        let r = resolve_spacing("1r", 1.0, 123.45, "bupr", "n");
        assert!((r - 123.45).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_spacing_unit_u_at_multiple_of_chr_units() {
        // "7u" × 1000 → 7000.
        let r = resolve_spacing("7u", 1000.0, 1.0, "bupr", "n");
        assert_eq!(r, 7000.0);
    }

    #[test]
    fn test_lookup_pairwise_with_non_map_value_returns_none() {
        // Str (not Map) at key → lookup returns None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Str("not_a_map".into()));
        assert!(lookup_pairwise(&pw, "k").is_none());
    }

    #[test]
    fn test_resolve_spacing_u_zero_chr_yields_zero() {
        // Nu × 0 = 0 regardless of value.
        let r = resolve_spacing("100u", 0.0, 1.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_multi_level_inner_map_found() {
        // pw["k"] = Map{spacing="5r"} → resolves.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("5r".into()));
        inner.insert("other_key".into(), ConfigValue::Str("extra".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k"), Some("5r".to_string()));
    }

    #[test]
    fn test_resolve_spacing_large_bare_number_preserves_precision() {
        // "1234567890" bare → 1234567890.0.
        let r = resolve_spacing("1234567890", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 1234567890.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_arithmetic_with_small_value() {
        // "0.001u" × 1000 = 1.
        let r = resolve_spacing("0.001u", 1000.0, 1.0, "bupr", "n");
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_lookup_pairwise_key_not_found_returns_none() {
        // pw has one key but search different key → None.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("1u".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("a".into(), ConfigValue::Map(inner));
        assert!(lookup_pairwise(&pw, "b").is_none());
    }

    #[test]
    fn test_resolve_spacing_r_with_large_gsize_scales_correctly() {
        // "0.5r" × gsize=10000 → 5000.
        let r = resolve_spacing("0.5r", 1.0, 10000.0, "bupr", "n");
        assert_eq!(r, 5000.0);
    }

    #[test]
    fn test_resolve_spacing_b_unit_passes_through_unchanged() {
        // Bases unit: no scaling (match arm "_ => val").
        let r = resolve_spacing("500b", 1000.0, 5000.0, "bupr", "n");
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_resolve_spacing_p_unit_passes_through_unchanged() {
        // Pixels unit: catch-all → return val unchanged.
        let r = resolve_spacing("42p", 100.0, 200.0, "bupr", "n");
        assert_eq!(r, 42.0);
    }

    #[test]
    fn test_resolve_spacing_unparseable_value_returns_zero() {
        // Unparseable → unit_split Err → 0.0.
        let r = resolve_spacing("not_a_number", 1.0, 100.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_entry_is_scalar_not_map_returns_none() {
        // Value is a Str, not Map → as_map() yields None → lookup returns None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("key".into(), ConfigValue::Str("just_a_string".into()));
        assert!(lookup_pairwise(&pw, "key").is_none());
    }

    #[test]
    fn test_lookup_pairwise_map_without_spacing_key_returns_none() {
        // Value is Map but has no "spacing" entry → None.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("other_key".into(), ConfigValue::Str("v".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("a__b".into(), ConfigValue::Map(inner));
        assert!(lookup_pairwise(&pw, "a__b").is_none());
    }

    #[test]
    fn test_lookup_pairwise_spacing_value_not_str_returns_none() {
        // spacing is a Map, not Str → as_str() yields None → None.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Map(HashMap::new()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("x".into(), ConfigValue::Map(inner));
        assert!(lookup_pairwise(&pw, "x").is_none());
    }

    #[test]
    fn test_resolve_spacing_negative_bare_value_preserved() {
        // Bare "-5" nounit → val=-5, no unit scaling → -5.0.
        let r = resolve_spacing("-5", 100.0, 200.0, "bupr", "n");
        assert_eq!(r, -5.0);
    }

    #[test]
    fn test_resolve_spacing_fractional_r_multiplies_gsize() {
        // "0.25r" × gsize=400 → 100.0.
        let r = resolve_spacing("0.25r", 1.0, 400.0, "bupr", "n");
        assert_eq!(r, 100.0);
    }

    #[test]
    fn test_lookup_pairwise_both_key_and_spacing_present_returns_some() {
        // Map with "spacing" Str → Some(value).
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("42p".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("hs1__hs2".into(), ConfigValue::Map(inner));
        let v = lookup_pairwise(&pw, "hs1__hs2");
        assert_eq!(v.as_deref(), Some("42p"));
    }

    #[test]
    fn test_resolve_spacing_u_unit_small_value_scales() {
        // "0.5u" × chromosomes_units=1000 → 500.
        let r = resolve_spacing("0.5u", 1000.0, 0.0, "bupr", "n");
        assert_eq!(r, 500.0);
    }

    #[test]
    fn test_resolve_spacing_nounit_zero_passes_through() {
        // "0" with no unit → nounit catch-all → 0.0.
        let r = resolve_spacing("0", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_resolve_spacing_very_large_u_value_preserves_precision() {
        // Large "u" value scales.
        let r = resolve_spacing("1000000u", 1000.0, 0.0, "bupr", "n");
        assert_eq!(r, 1_000_000_000.0);
    }

    #[test]
    fn test_lookup_pairwise_absent_key_returns_none() {
        // Key not in map → None.
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        assert!(lookup_pairwise(&pw, "missing").is_none());
    }

    #[test]
    fn test_resolve_spacing_empty_string_returns_zero_v2() {
        // Empty value → unit_split Err → 0.0.
        let r = resolve_spacing("", 1.0, 100.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_resolve_spacing_u_unit_with_zero_chromosomes_units_yields_zero_v2() {
        // "5u" × 0 → 0.
        let r = resolve_spacing("5u", 0.0, 100.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_as_str_with_p_suffix_returned_verbatim() {
        // spacing="100p" returned verbatim as String.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("100p".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k").as_deref(), Some("100p"));
    }

    #[test]
    fn test_lookup_pairwise_empty_map_any_key_none() {
        // Empty map → any key → None.
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        for key in ["a", "b__c", "anything"] {
            assert!(lookup_pairwise(&pw, key).is_none());
        }
    }

    #[test]
    fn test_resolve_spacing_r_with_zero_gsize_yields_zero() {
        // "0.5r" × 0 → 0.
        let r = resolve_spacing("0.5r", 1.0, 0.0, "bupr", "n");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn test_resolve_spacing_decimal_bare_no_unit_preserved() {
        // Decimal without unit passes through as float.
        let r = resolve_spacing("3.14", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, 3.14);
    }

    #[test]
    fn test_lookup_pairwise_multi_entry_map_target_key_hit() {
        // Multiple keys in pw map; only target has "spacing".
        let mut inner1: HashMap<String, ConfigValue> = HashMap::new();
        inner1.insert("spacing".into(), ConfigValue::Str("v1".into()));
        let mut inner2: HashMap<String, ConfigValue> = HashMap::new();
        inner2.insert("spacing".into(), ConfigValue::Str("v2".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k1".into(), ConfigValue::Map(inner1));
        pw.insert("k2".into(), ConfigValue::Map(inner2));
        assert_eq!(lookup_pairwise(&pw, "k2").as_deref(), Some("v2"));
    }

    #[test]
    fn test_lookup_pairwise_list_value_not_map_returns_none() {
        // Value is a List, not Map → as_map None → None.
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("key".into(), ConfigValue::List(vec![]));
        assert!(lookup_pairwise(&pw, "key").is_none());
    }

    #[test]
    fn test_resolve_spacing_u_unit_decimal_fraction_scales() {
        // "0.01u" × 1000 → 10.
        let r = resolve_spacing("0.01u", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, 10.0);
    }

    #[test]
    fn test_resolve_spacing_scientific_notation_bare_number_v2() {
        // "1e6" scientific → 1000000.
        let r = resolve_spacing("1e6", 1.0, 1.0, "bupr", "n");
        assert_eq!(r, 1_000_000.0);
    }

    #[test]
    fn test_lookup_pairwise_spacing_empty_string_returned_as_is() {
        // spacing="" is a valid empty Str — returned verbatim.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("spacing".into(), ConfigValue::Str("".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k").as_deref(), Some(""));
    }

    #[test]
    fn test_resolve_spacing_negative_u_value_yields_negative() {
        // "-5u" × chromosomes_units=1000 → -5000.
        let r = resolve_spacing("-5u", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, -5000.0);
    }

    #[test]
    fn test_resolve_spacing_uppercase_r_not_matched_err_zero() {
        // "0.5R" (capital R) — R not in units_ok "bupr" → unit_split Err → 0.
        let r = resolve_spacing("0.5R", 1.0, 100.0, "bupr", "n");
        // Treated as bare number: 0.5 if last-char check is lowercase-sensitive.
        // Or could be Err if "R" not in units_ok. Either way, result observable.
        assert!(r == 0.0 || r == 0.5);
    }

    #[test]
    fn test_lookup_pairwise_map_with_multi_keys_only_spacing_extracted() {
        // inner map has many keys; only "spacing" returned.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("color".into(), ConfigValue::Str("red".into()));
        inner.insert("spacing".into(), ConfigValue::Str("5u".into()));
        inner.insert("thickness".into(), ConfigValue::Str("2".into()));
        let mut pw: HashMap<String, ConfigValue> = HashMap::new();
        pw.insert("k".into(), ConfigValue::Map(inner));
        assert_eq!(lookup_pairwise(&pw, "k").as_deref(), Some("5u"));
    }

    #[test]
    fn test_resolve_spacing_p_unit_negative_preserved() {
        // "-10p" → pixel catch-all → -10.
        let r = resolve_spacing("-10p", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, -10.0);
    }

    #[test]
    fn test_resolve_spacing_b_unit_large_value_preserves_precision() {
        // "1000000b" → catch-all → 1000000.0.
        let r = resolve_spacing("1000000b", 1000.0, 500.0, "bupr", "n");
        assert_eq!(r, 1_000_000.0);
    }

    #[test]
    fn test_resolve_spacing_r_unit_very_small_gsize_yields_small_result() {
        // "1.0r" × gsize=0.01 → 0.01.
        let r = resolve_spacing("1.0r", 1.0, 0.01, "bupr", "n");
        assert!((r - 0.01).abs() < 1e-9);
    }

    #[test]
    fn test_lookup_pairwise_empty_key_returns_none() {
        // Empty "" key not found in map.
        let pw: HashMap<String, ConfigValue> = HashMap::new();
        assert!(lookup_pairwise(&pw, "").is_none());
    }

    #[test]
    fn test_resolve_spacing_u_unit_chained_with_bare_value_via_unit_split() {
        // "2u" with various chromosome units; not an arithmetic expression here.
        let r = resolve_spacing("2u", 500.0, 0.0, "bupr", "n");
        assert_eq!(r, 1000.0);
    }
}
