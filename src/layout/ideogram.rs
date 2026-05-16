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

#[cfg(test)]
mod tests {
    use super::*;

    fn mk() -> Ideogram {
        Ideogram {
            chr: "hs1".into(),
            label: "1".into(),
            tag: "a".into(),
            chrlength: 1_000_000,
            set: IntSpan::from_range(0, 1_000_000),
            scale: 1.5,
            reverse: false,
            idx: 3,
            display_idx: 7,
            covers: vec![Cover {
                set: IntSpan::from_range(0, 500),
                scale: 2.0,
            }],
            length_scaled: 1_500_000.0,
            length_noscale: 1_000_000.0,
            length_cumulative_scaled: 3_000_000.0,
            length_cumulative_noscale: 2_000_000.0,
            radius: 1000.0,
            radius_inner: 950.0,
            radius_outer: 1050.0,
            thickness: 100.0,
            has_break_start: true,
            has_break_end: false,
            color: "red".into(),
        }
    }

    #[test]
    fn test_ideogram_clone_preserves_all_fields() {
        let a = mk();
        let b = a.clone();
        assert_eq!(a.chr, b.chr);
        assert_eq!(a.tag, b.tag);
        assert_eq!(a.chrlength, b.chrlength);
        assert_eq!(a.scale, b.scale);
        assert_eq!(a.idx, b.idx);
        assert_eq!(a.display_idx, b.display_idx);
        assert_eq!(a.set.cardinality(), b.set.cardinality());
        assert_eq!(a.covers.len(), b.covers.len());
        assert_eq!(a.has_break_start, b.has_break_start);
        assert_eq!(a.has_break_end, b.has_break_end);
    }

    #[test]
    fn test_ideogram_clone_is_deep_for_covers() {
        // Modifying the clone's covers Vec doesn't affect the source.
        let a = mk();
        let mut b = a.clone();
        b.covers.push(Cover {
            set: IntSpan::from_range(600, 700),
            scale: 3.0,
        });
        b.covers[0].scale = 99.0;
        assert_eq!(a.covers.len(), 1);
        assert_eq!(a.covers[0].scale, 2.0);
        assert_eq!(b.covers.len(), 2);
        assert_eq!(b.covers[0].scale, 99.0);
        assert_eq!(b.covers[1].scale, 3.0);
    }

    #[test]
    fn test_cover_struct_holds_set_and_scale() {
        // Construct a Cover and verify fields round-trip.
        let c = Cover {
            set: IntSpan::from_range(10, 20),
            scale: 0.5,
        };
        assert_eq!(c.set.cardinality(), 11);
        assert_eq!(c.set.min(), Some(10));
        assert_eq!(c.set.max(), Some(20));
        assert_eq!(c.scale, 0.5);
    }

    #[test]
    fn test_ideogram_radii_relationship() {
        // Invariant: radius_inner ≤ radius ≤ radius_outer; thickness = outer-inner.
        let i = mk();
        assert!(i.radius_inner <= i.radius);
        assert!(i.radius <= i.radius_outer);
        assert_eq!(i.radius_outer - i.radius_inner, i.thickness);
    }

    #[test]
    fn test_ideogram_length_fields_scale_relation() {
        // length_scaled should match length_noscale × scale for the mk() fixture.
        let i = mk();
        assert!((i.length_scaled - i.length_noscale * i.scale).abs() < 1e-6);
        // cumulative_scaled > cumulative_noscale when scale > 1.
        assert!(i.scale > 1.0);
        assert!(i.length_cumulative_scaled > i.length_cumulative_noscale);
    }

    #[test]
    fn test_ideogram_reverse_flag_mutation_independent_from_clone() {
        // Cloning then toggling reverse on the clone must not disturb the original.
        let a = mk();
        let mut b = a.clone();
        assert!(!a.reverse);
        assert!(!b.reverse);
        b.reverse = true;
        assert!(!a.reverse);
        assert!(b.reverse);
        // Also verify has_break_end flips independently.
        b.has_break_end = true;
        assert!(!a.has_break_end);
        assert!(b.has_break_end);
    }

    #[test]
    fn test_cover_clone_preserves_intspan_and_scale() {
        // Cover.clone() is deep for its IntSpan: mutating clone.set replacement
        // doesn't affect source.
        let a = Cover {
            set: IntSpan::from_range(0, 99),
            scale: 1.25,
        };
        let mut b = a.clone();
        assert_eq!(a.set.cardinality(), 100);
        assert_eq!(b.set.cardinality(), 100);
        assert_eq!(b.scale, 1.25);
        b.set = IntSpan::new();
        b.scale = 99.0;
        assert_eq!(a.set.cardinality(), 100);
        assert_eq!(a.scale, 1.25);
        assert_eq!(b.set.cardinality(), 0);
        assert_eq!(b.scale, 99.0);
    }

    #[test]
    fn test_ideogram_multi_cover_insertion_preserves_order() {
        // Pushing multiple Cover entries preserves insertion order; set/scale fields
        // of each retained exactly.
        let mut i = mk();
        i.covers.clear();
        for (s, e, sc) in [(0, 100, 1.0), (200, 300, 2.0), (500, 600, 0.5)] {
            i.covers.push(Cover {
                set: IntSpan::from_range(s, e),
                scale: sc,
            });
        }
        assert_eq!(i.covers.len(), 3);
        assert_eq!(i.covers[0].scale, 1.0);
        assert_eq!(i.covers[1].set.cardinality(), 101);
        assert_eq!(i.covers[2].scale, 0.5);
        assert_eq!(i.covers[0].set.min(), Some(0));
        assert_eq!(i.covers[2].set.max(), Some(600));
    }

    #[test]
    fn test_ideogram_debug_includes_all_identifying_fields() {
        // Debug derive on Ideogram emits chr/tag/chrlength/scale/idx/display_idx.
        let i = mk();
        let s = format!("{:?}", i);
        assert!(s.contains("chr:"));
        assert!(s.contains("hs1"));
        assert!(s.contains("tag:"));
        assert!(s.contains("chrlength:"));
        assert!(s.contains("scale:"));
        assert!(s.contains("idx:"));
        assert!(s.contains("display_idx:"));
        assert!(s.contains("reverse:"));
    }

    #[test]
    fn test_cover_debug_format_shows_intspan_and_scale() {
        // Debug on Cover emits its IntSpan and scale value.
        let c = Cover {
            set: IntSpan::from_range(10, 20),
            scale: 0.75,
        };
        let s = format!("{:?}", c);
        assert!(s.contains("set:"));
        assert!(s.contains("scale:"));
        assert!(s.contains("0.75"));
    }

    #[test]
    fn test_ideogram_idx_vs_display_idx_independent_fields() {
        // idx (source index) and display_idx (render order) are independent.
        let mut i = mk();
        assert_eq!(i.idx, 3);
        assert_eq!(i.display_idx, 7);
        // Reorder display without changing source idx.
        i.display_idx = 12;
        assert_eq!(i.idx, 3);
        assert_eq!(i.display_idx, 12);
        // Likewise change idx; display_idx stays.
        i.idx = 99;
        assert_eq!(i.idx, 99);
        assert_eq!(i.display_idx, 12);
    }

    #[test]
    fn test_ideogram_chrlength_is_i64_and_preserves_genomic_scale() {
        // chrlength is i64 — holds full human-genome-scale values without overflow.
        let mut i = mk();
        i.chrlength = 247_249_719; // hs1 length
        assert_eq!(i.chrlength, 247_249_719);
        // Mutation isolation via clone.
        let c = i.clone();
        i.chrlength = 100;
        assert_eq!(c.chrlength, 247_249_719);
        // Negative values accepted (no clamp).
        i.chrlength = -42;
        assert_eq!(i.chrlength, -42);
    }

    #[test]
    fn test_ideogram_cumulative_fields_monotonic_relation() {
        // length_cumulative_scaled should typically be ≥ 0; test a sequence.
        let mut i = mk();
        // Set cumulative values to simulate a build.
        i.length_cumulative_noscale = 0.0;
        i.length_cumulative_scaled = 0.0;
        assert_eq!(i.length_cumulative_noscale, 0.0);
        assert_eq!(i.length_cumulative_scaled, 0.0);
        // Large genomic values.
        i.length_cumulative_noscale = 3e9;
        i.length_cumulative_scaled = 4.5e9;
        assert!(i.length_cumulative_scaled > i.length_cumulative_noscale);
    }

    #[test]
    fn test_ideogram_color_field_mutable() {
        // Color string is independent — mutate without affecting other fields.
        let mut i = mk();
        assert_eq!(i.color, "red");
        i.color = "blue".into();
        assert_eq!(i.color, "blue");
        i.color = "".into();
        assert_eq!(i.color, "");
        // Chromosome field unchanged.
        assert_eq!(i.chr, "hs1");
    }

    #[test]
    fn test_cover_zero_scale_is_valid() {
        // Cover.scale=0 is a valid value (no restriction); Ideogram.covers can hold it.
        let c = Cover {
            set: IntSpan::from_range(0, 100),
            scale: 0.0,
        };
        assert_eq!(c.scale, 0.0);
        assert_eq!(c.set.cardinality(), 101);
        // Negative scale is also legal at the struct level (not validated).
        let c = Cover {
            set: IntSpan::from_range(0, 100),
            scale: -1.5,
        };
        assert_eq!(c.scale, -1.5);
    }

    #[test]
    fn test_ideogram_both_break_flags_independent() {
        // has_break_start and has_break_end are independent bool flags.
        let mut i = mk();
        // Default: start=true (from mk), end=false.
        assert!(i.has_break_start);
        assert!(!i.has_break_end);
        // Flip both.
        i.has_break_start = false;
        i.has_break_end = true;
        assert!(!i.has_break_start);
        assert!(i.has_break_end);
        // Both can be true simultaneously.
        i.has_break_start = true;
        assert!(i.has_break_start && i.has_break_end);
    }

    #[test]
    fn test_ideogram_set_intspan_mutable() {
        // Mutating set field with a fresh IntSpan updates cardinality.
        let mut i = mk();
        let before = i.set.cardinality();
        i.set = IntSpan::from_range(0, 10);
        assert_eq!(i.set.cardinality(), 11);
        assert_ne!(before, i.set.cardinality());
        // Empty the set.
        i.set = IntSpan::new();
        assert_eq!(i.set.cardinality(), 0);
    }

    #[test]
    fn test_ideogram_tag_and_label_independent_strings() {
        // tag and label are separate String fields.
        let mut i = mk();
        i.tag = "custom_tag".into();
        i.label = "1A".into();
        assert_eq!(i.tag, "custom_tag");
        assert_eq!(i.label, "1A");
        assert_ne!(i.tag, i.label);
        // Clearing one doesn't affect the other.
        i.tag = "".into();
        assert_eq!(i.tag, "");
        assert_eq!(i.label, "1A");
    }

    #[test]
    fn test_cover_debug_format_contains_fields() {
        // Cover.Debug shows both `set:` and `scale:` fields.
        let c = Cover {
            set: IntSpan::from_range(100, 200),
            scale: 3.5,
        };
        let s = format!("{:?}", c);
        assert!(s.contains("set:"));
        assert!(s.contains("scale:"));
        assert!(s.contains("3.5"));
    }

    #[test]
    fn test_ideogram_thickness_field_mutable_and_independent() {
        // thickness mutates without affecting radius_inner/radius_outer.
        let mut i = mk();
        let inner = i.radius_inner;
        let outer = i.radius_outer;
        i.thickness = 500.0;
        assert_eq!(i.thickness, 500.0);
        // inner/outer unchanged — they're independent fields.
        assert_eq!(i.radius_inner, inner);
        assert_eq!(i.radius_outer, outer);
    }

    #[test]
    fn test_ideogram_radius_fields_all_independent_f64() {
        // radius, radius_inner, radius_outer are all independent f64 fields.
        let mut i = mk();
        i.radius = 123.4;
        i.radius_inner = 100.0;
        i.radius_outer = 150.0;
        assert_eq!(i.radius, 123.4);
        assert_eq!(i.radius_inner, 100.0);
        assert_eq!(i.radius_outer, 150.0);
    }

    #[test]
    fn test_cover_scale_f64_fractional_values_preserved() {
        // Cover.scale f64 — decimal precision preserved.
        let c = Cover {
            set: IntSpan::from_range(0, 10),
            scale: 0.123456789,
        };
        assert!((c.scale - 0.123456789).abs() < 1e-12);
    }

    #[test]
    fn test_ideogram_display_idx_large_values() {
        // display_idx is usize — holds large values.
        let mut i = mk();
        i.display_idx = 1_000_000;
        assert_eq!(i.display_idx, 1_000_000);
        i.display_idx = 0;
        assert_eq!(i.display_idx, 0);
    }

    #[test]
    fn test_ideogram_clone_covers_vec_independence() {
        // Cloning Ideogram produces an independent covers Vec — push to clone
        // doesn't affect source length.
        let a = mk();
        let before = a.covers.len();
        let mut b = a.clone();
        b.covers.clear();
        b.covers.push(Cover { set: IntSpan::new(), scale: 0.0 });
        // Source has original length, clone has 1.
        assert_eq!(a.covers.len(), before);
        assert_eq!(b.covers.len(), 1);
    }

    #[test]
    fn test_cover_set_and_scale_fields_mutate_independently() {
        // Cover.set and Cover.scale are independent fields — mutating one
        // doesn't affect the other.
        let mut c = Cover { set: IntSpan::from_range(0, 100), scale: 1.0 };
        c.scale = 2.5;
        assert_eq!(c.set.cardinality(), 101);
        assert_eq!(c.scale, 2.5);
        c.set.insert(999);
        assert_eq!(c.set.cardinality(), 102);
        assert_eq!(c.scale, 2.5);
    }

    #[test]
    fn test_ideogram_inner_outer_radius_no_struct_level_enforcement() {
        // Struct allows inner > outer (no invariant check at this layer —
        // enforcement happens in layout building).
        let mut i = mk();
        i.radius_inner = 2000.0;
        i.radius_outer = 500.0;
        assert!(i.radius_inner > i.radius_outer);
    }

    #[test]
    fn test_ideogram_reverse_toggle_preserves_other_fields() {
        // Flipping reverse doesn't alter chr/tag/scale/display_idx.
        let mut i = mk();
        let (chr_before, tag_before) = (i.chr.clone(), i.tag.clone());
        let (scale_before, idx_before) = (i.scale, i.display_idx);
        i.reverse = true;
        assert!(i.reverse);
        assert_eq!(i.chr, chr_before);
        assert_eq!(i.tag, tag_before);
        assert_eq!(i.scale, scale_before);
        assert_eq!(i.display_idx, idx_before);
        i.reverse = false;
        assert!(!i.reverse);
    }

    #[test]
    fn test_ideogram_has_break_start_and_end_independent() {
        // has_break_start and has_break_end are independent bool fields.
        let mut i = mk();
        i.has_break_start = true;
        i.has_break_end = false;
        assert!(i.has_break_start);
        assert!(!i.has_break_end);
        i.has_break_start = false;
        i.has_break_end = true;
        assert!(!i.has_break_start);
        assert!(i.has_break_end);
    }

    #[test]
    fn test_ideogram_length_cumulative_scaled_and_noscale_independent() {
        // length_cumulative_scaled and length_cumulative_noscale are distinct f64 fields.
        let mut i = mk();
        i.length_cumulative_scaled = 1234.5;
        i.length_cumulative_noscale = 5678.9;
        assert_eq!(i.length_cumulative_scaled, 1234.5);
        assert_eq!(i.length_cumulative_noscale, 5678.9);
        // Mutating one doesn't alter the other.
        i.length_cumulative_scaled = 0.0;
        assert_eq!(i.length_cumulative_noscale, 5678.9);
    }

    #[test]
    fn test_ideogram_chr_and_tag_clone_independent_strings() {
        // Clone's chr/tag can be mutated without affecting the source strings.
        let a = mk();
        let mut b = a.clone();
        b.chr = "altered_chr".into();
        b.tag = "altered_tag".into();
        assert_eq!(a.chr, "hs1");
        assert_eq!(a.tag, "a");
        assert_eq!(b.chr, "altered_chr");
        assert_eq!(b.tag, "altered_tag");
    }

    #[test]
    fn test_cover_multiple_with_different_scales_coexist_in_vec() {
        // Ideogram can carry multiple Cover regions each with its own scale.
        let mut i = mk();
        i.covers.clear();
        i.covers.push(Cover { set: IntSpan::from_range(0, 100), scale: 1.0 });
        i.covers.push(Cover { set: IntSpan::from_range(200, 300), scale: 2.5 });
        i.covers.push(Cover { set: IntSpan::from_range(400, 500), scale: 0.5 });
        assert_eq!(i.covers.len(), 3);
        assert_eq!(i.covers[0].scale, 1.0);
        assert_eq!(i.covers[1].scale, 2.5);
        assert_eq!(i.covers[2].scale, 0.5);
    }

    #[test]
    fn test_ideogram_idx_and_display_idx_are_independent_usizes() {
        // idx (creation order) and display_idx (render order) are stored separately.
        let mut i = mk();
        i.idx = 42;
        i.display_idx = 7;
        assert_eq!(i.idx, 42);
        assert_eq!(i.display_idx, 7);
        // Mutating display_idx leaves idx untouched.
        i.display_idx = 100;
        assert_eq!(i.idx, 42);
    }

    #[test]
    fn test_ideogram_chrlength_large_i64_value_preserved() {
        // chrlength is i64; supports very large values without overflow.
        let mut i = mk();
        i.chrlength = 3_000_000_000; // larger than u32::MAX
        assert_eq!(i.chrlength, 3_000_000_000);
    }

    #[test]
    fn test_cover_clone_produces_independent_intspan() {
        // Cover.set is IntSpan; cloning yields a fresh one.
        let c = Cover { set: IntSpan::from_range(0, 100), scale: 1.0 };
        let mut c2 = c.clone();
        c2.set.insert(500);
        // Source cardinality unchanged at 101.
        assert_eq!(c.set.cardinality(), 101);
        assert_eq!(c2.set.cardinality(), 102);
    }

    #[test]
    fn test_ideogram_color_string_accepts_arbitrary_values() {
        // color is a plain String — struct allows any value.
        let mut i = mk();
        i.color = "my_custom_color_xyz".into();
        assert_eq!(i.color, "my_custom_color_xyz");
        // Unicode also works.
        i.color = "αβγ".into();
        assert_eq!(i.color, "αβγ");
        // Empty string allowed.
        i.color = String::new();
        assert!(i.color.is_empty());
    }

    #[test]
    fn test_ideogram_scale_f64_precision_preserved() {
        // scale is f64; tiny fractional differences retained.
        let mut i = mk();
        i.scale = 1.000_000_000_001;
        assert!((i.scale - 1.000_000_000_001).abs() < 1e-14);
        // Negative scale allowed at struct level (no validation here).
        i.scale = -2.5;
        assert_eq!(i.scale, -2.5);
    }

    #[test]
    fn test_ideogram_set_and_covers_sets_mutate_independently() {
        // Top-level `set` and per-cover `set` are separate IntSpans.
        let mut i = mk();
        let set_before = i.set.cardinality();
        let cover_before = i.covers[0].set.cardinality();
        // Mutate only covers[0].set.
        i.covers[0].set.insert(999);
        assert_eq!(i.covers[0].set.cardinality(), cover_before + 1);
        assert_eq!(i.set.cardinality(), set_before); // unchanged
    }

    #[test]
    fn test_ideogram_tag_empty_string_valid_at_struct_level() {
        // Empty tag allowed — struct has no validation.
        let mut i = mk();
        i.tag = String::new();
        assert!(i.tag.is_empty());
        // Clone also preserves empty.
        let c = i.clone();
        assert!(c.tag.is_empty());
    }

    #[test]
    fn test_cover_with_zero_scale_allowed() {
        // Cover.scale = 0.0 is valid (though degenerate).
        let c = Cover { set: IntSpan::from_range(0, 100), scale: 0.0 };
        assert_eq!(c.scale, 0.0);
        assert_eq!(c.set.cardinality(), 101);
    }

    #[test]
    fn test_ideogram_radius_fields_are_three_distinct_f64s() {
        // radius/radius_inner/radius_outer are independent fields — prove by
        // assigning unique values and reading back.
        let mut i = mk();
        i.radius = 1111.0;
        i.radius_inner = 2222.0;
        i.radius_outer = 3333.0;
        assert_eq!(i.radius, 1111.0);
        assert_eq!(i.radius_inner, 2222.0);
        assert_eq!(i.radius_outer, 3333.0);
    }
}
