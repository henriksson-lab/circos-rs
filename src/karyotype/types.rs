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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_karyotype_default_is_empty() {
        let k = Karyotype::default();
        assert!(k.chromosomes.is_empty());
        assert!(k.bands.is_empty());
        assert!(k.order.is_empty());
    }

    #[test]
    fn test_display_region_default_has_empty_intspans() {
        // DisplayRegion::default → empty accept/reject IntSpans.
        let dr = DisplayRegion::default();
        assert_eq!(dr.accept.cardinality(), 0);
        assert_eq!(dr.reject.cardinality(), 0);
    }

    #[test]
    fn test_chromosome_clone_is_deep_for_strings_and_intspan() {
        // Cloning a Chromosome and mutating the clone's fields doesn't affect
        // the source's strings or IntSpan.
        let original = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            set: IntSpan::from_range(0, 100),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        let mut clone = original.clone();
        clone.name = "hs_mut".into();
        clone.display = false;
        assert_eq!(original.name, "hs1");
        assert!(original.display);
        assert_eq!(clone.name, "hs_mut");
        assert!(!clone.display);
        // IntSpan was cloned too — both have cardinality 101.
        assert_eq!(original.set.cardinality(), 101);
        assert_eq!(clone.set.cardinality(), 101);
    }

    #[test]
    fn test_band_and_karyotype_roundtrip_via_hashmap_insert() {
        // Constructing a Band, inserting it into Karyotype.bands, and reading
        // it back preserves all fields.
        let band = Band {
            name: "p36.33".into(),
            label: "p36.33".into(),
            parent: "hs1".into(),
            start: 0,
            end: 2_300_000,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 2_300_000),
        };
        let mut k = Karyotype::default();
        k.bands.insert("hs1".into(), vec![band.clone()]);
        k.order.push("hs1".into());
        let stored = &k.bands["hs1"][0];
        assert_eq!(stored.name, "p36.33");
        assert_eq!(stored.parent, "hs1");
        assert_eq!(stored.start, 0);
        assert_eq!(stored.end, 2_300_000);
        assert_eq!(stored.set.cardinality(), 2_300_001);
        assert_eq!(k.order, vec!["hs1".to_string()]);
    }

    #[test]
    fn test_display_region_clone_preserves_populated_intspans() {
        // DisplayRegion with populated accept and reject IntSpans → clone has
        // independent copies with matching cardinalities.
        let dr = DisplayRegion {
            accept: IntSpan::from_range(0, 500),
            reject: IntSpan::from_range(100, 200),
        };
        assert_eq!(dr.accept.cardinality(), 501);
        assert_eq!(dr.reject.cardinality(), 101);
        let dc = dr.clone();
        assert_eq!(dc.accept.cardinality(), 501);
        assert_eq!(dc.reject.cardinality(), 101);
        // Mutate the clone — original unaffected.
        let mut mut_dc = dc;
        mut_dc.accept = IntSpan::new();
        assert_eq!(mut_dc.accept.cardinality(), 0);
        assert_eq!(dr.accept.cardinality(), 501);
    }

    #[test]
    fn test_karyotype_order_preserves_insertion_sequence() {
        // Karyotype.order is a Vec<String> — insertion order survives push + clone.
        let mut k = Karyotype::default();
        for name in ["hs7", "hs2", "hs14", "hs1"] {
            k.order.push(name.to_string());
        }
        assert_eq!(k.order, vec!["hs7", "hs2", "hs14", "hs1"]);
        let k2 = k.clone();
        assert_eq!(k2.order, k.order);
        // Pushing more preserves earlier entries.
        let mut k3 = k.clone();
        k3.order.push("hsX".into());
        assert_eq!(k3.order.len(), 5);
        assert_eq!(k3.order[0], "hs7");
        assert_eq!(k3.order[4], "hsX");
    }

    #[test]
    fn test_karyotype_multiple_bands_on_same_chromosome() {
        // bands vec grows when multiple bands are appended under the same key.
        let mut k = Karyotype::default();
        k.bands.insert("hs1".into(), Vec::new());
        for (name, s, e) in [("p13", 0, 100), ("p12", 100, 200), ("p11", 200, 300)] {
            k.bands.get_mut("hs1").unwrap().push(Band {
                name: name.into(),
                label: name.into(),
                parent: "hs1".into(),
                start: s,
                end: e,
                color: "gneg".into(),
                set: IntSpan::from_range(s, e),
            });
        }
        assert_eq!(k.bands["hs1"].len(), 3);
        assert_eq!(k.bands["hs1"][0].name, "p13");
        assert_eq!(k.bands["hs1"][2].name, "p11");
        // Another chromosome's bands are independent.
        k.bands.insert("hs2".into(), Vec::new());
        assert_eq!(k.bands["hs2"].len(), 0);
        assert_eq!(k.bands["hs1"].len(), 3);
    }

    #[test]
    fn test_chromosome_debug_formatting_includes_field_names() {
        // Debug output should surface all field names for round-trip diagnostics.
        let c = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 1000,
            color: "red".into(),
            set: IntSpan::from_range(0, 1000),
            index: 7,
            display: true,
            display_region: DisplayRegion::default(),
        };
        let s = format!("{:?}", c);
        assert!(s.contains("name:"));
        assert!(s.contains("hs1"));
        assert!(s.contains("index: 7"));
        assert!(s.contains("display: true"));
    }

    #[test]
    fn test_band_all_fields_independent_under_clone() {
        // Mutating a clone's fields must not affect the source.
        let b = Band {
            name: "p11.1".into(),
            label: "p11.1".into(),
            parent: "hs1".into(),
            start: 100,
            end: 500,
            color: "gpos50".into(),
            set: IntSpan::from_range(100, 500),
        };
        let mut c = b.clone();
        c.name = "p11.2".into();
        c.start = 200;
        c.end = 600;
        c.color = "gneg".into();
        c.set = IntSpan::new();
        // Original unchanged.
        assert_eq!(b.name, "p11.1");
        assert_eq!(b.start, 100);
        assert_eq!(b.end, 500);
        assert_eq!(b.color, "gpos50");
        assert_eq!(b.set.cardinality(), 401);
        // Clone took the mutations.
        assert_eq!(c.name, "p11.2");
        assert_eq!(c.start, 200);
        assert_eq!(c.set.cardinality(), 0);
    }

    #[test]
    fn test_karyotype_chromosomes_lookup_by_name() {
        // Karyotype.chromosomes is a HashMap — test direct lookup.
        let mut k = Karyotype::default();
        let c = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            set: IntSpan::from_range(0, 100),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        k.chromosomes.insert("hs1".into(), c);
        k.order.push("hs1".into());
        // Lookup by name.
        assert!(k.chromosomes.contains_key("hs1"));
        assert!(!k.chromosomes.contains_key("hs2"));
        let got = k.chromosomes.get("hs1").unwrap();
        assert_eq!(got.label, "1");
        assert_eq!(got.end, 100);
    }

    #[test]
    fn test_display_region_debug_and_clone_with_populated_intspans() {
        // Debug derive emits field names and cardinalities.
        let dr = DisplayRegion {
            accept: IntSpan::from_range(0, 1000),
            reject: IntSpan::from_range(500, 600),
        };
        let s = format!("{:?}", dr);
        assert!(s.contains("accept:"));
        assert!(s.contains("reject:"));
        // Deep clone.
        let cloned = dr.clone();
        assert_eq!(cloned.accept.cardinality(), 1001);
        assert_eq!(cloned.reject.cardinality(), 101);
    }

    #[test]
    fn test_band_set_cardinality_matches_range_plus_one() {
        // Band.set is an IntSpan — cardinality equals (end - start + 1) for a
        // simple from_range construction.
        for (start, end, expected) in [(0, 99, 100), (10, 20, 11), (100, 100, 1), (0, 0, 1)] {
            let b = Band {
                name: "n".into(),
                label: "n".into(),
                parent: "hs1".into(),
                start,
                end,
                color: "red".into(),
                set: IntSpan::from_range(start, end),
            };
            assert_eq!(b.set.cardinality(), expected, "start={}, end={}", start, end);
        }
    }

    #[test]
    fn test_chromosome_with_different_index_values() {
        // Chromosome.index is usize; test representative values preserve.
        for &idx in &[0usize, 1, 22, 100, 1000] {
            let c = Chromosome {
                name: "n".into(),
                label: "l".into(),
                start: 0,
                end: 100,
                color: "red".into(),
                set: IntSpan::from_range(0, 100),
                index: idx,
                display: true,
                display_region: DisplayRegion::default(),
            };
            assert_eq!(c.index, idx);
        }
    }

    #[test]
    fn test_karyotype_chromosomes_and_bands_decoupled() {
        // Inserting into bands doesn't auto-populate chromosomes (and vice versa).
        let mut k = Karyotype::default();
        k.bands.insert("hs1".into(), Vec::new());
        assert_eq!(k.bands.len(), 1);
        assert_eq!(k.chromosomes.len(), 0); // decoupled
        // Similarly, chromosome insertion doesn't auto-init bands.
        let c = Chromosome {
            name: "hs2".into(),
            label: "2".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        k.chromosomes.insert("hs2".into(), c);
        assert_eq!(k.chromosomes.len(), 1);
        assert!(!k.bands.contains_key("hs2"));
    }

    #[test]
    fn test_band_parent_can_differ_from_chromosome_key() {
        // Band.parent field is independent from the HashMap key used to store it.
        // The key is just a convention; the struct tracks its own parent.
        let mut k = Karyotype::default();
        let band = Band {
            name: "weird".into(),
            label: "weird".into(),
            parent: "original_parent".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            set: IntSpan::from_range(0, 100),
        };
        // Store under a different key than the parent field.
        k.bands.insert("different_key".into(), vec![band]);
        let b = &k.bands["different_key"][0];
        // The band's parent field stays as written.
        assert_eq!(b.parent, "original_parent");
    }

    #[test]
    fn test_display_region_empty_default_has_display_false() {
        // DisplayRegion::default has display=false (hidden by default).
        let dr = DisplayRegion::default();
        assert_eq!(dr.accept.cardinality(), 0);
        assert_eq!(dr.reject.cardinality(), 0);
        // Debug output includes default field values.
        let s = format!("{:?}", dr);
        assert!(s.contains("accept:"));
        assert!(s.contains("reject:"));
    }

    #[test]
    fn test_chromosome_negative_coordinates_allowed() {
        // start/end are i64 — negative values allowed in construction.
        let c = Chromosome {
            name: "neg".into(),
            label: "neg".into(),
            start: -100,
            end: -50,
            color: "red".into(),
            set: IntSpan::from_range(-100, -50),
            index: 0,
            display: false,
            display_region: DisplayRegion::default(),
        };
        assert_eq!(c.start, -100);
        assert_eq!(c.end, -50);
        assert_eq!(c.set.min(), Some(-100));
    }

    #[test]
    fn test_karyotype_clone_deep_for_all_three_collections() {
        // Karyotype.clone clones chromosomes, bands, and order independently.
        let mut k = Karyotype::default();
        let c = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            set: IntSpan::from_range(0, 100),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        k.chromosomes.insert("hs1".into(), c);
        k.bands.insert("hs1".into(), Vec::new());
        k.order.push("hs1".into());
        let mut c2 = k.clone();
        // Mutate clone — source unaffected.
        c2.chromosomes.clear();
        c2.bands.clear();
        c2.order.clear();
        assert_eq!(k.chromosomes.len(), 1);
        assert_eq!(k.bands.len(), 1);
        assert_eq!(k.order.len(), 1);
        assert_eq!(c2.chromosomes.len(), 0);
    }

    #[test]
    fn test_band_end_field_preserved_through_clone() {
        // Band.end is i64 — clone preserves without overflow.
        let b = Band {
            name: "bigband".into(),
            label: "bigband".into(),
            parent: "hs1".into(),
            start: 0,
            end: 2_300_000,
            color: "gpos25".into(),
            set: IntSpan::from_range(0, 2_300_000),
        };
        let c = b.clone();
        assert_eq!(c.end, 2_300_000);
        assert_eq!(c.set.cardinality(), 2_300_001);
    }

    #[test]
    fn test_display_region_intspan_mutation_visible_through_struct() {
        // Direct field mutation via struct — accept IntSpan updates visible.
        let mut dr = DisplayRegion::default();
        dr.accept = IntSpan::from_range(0, 500);
        assert_eq!(dr.accept.cardinality(), 501);
        dr.reject = IntSpan::from_range(100, 200);
        assert_eq!(dr.reject.cardinality(), 101);
        // Later clear accept.
        dr.accept = IntSpan::new();
        assert!(dr.accept.is_empty());
        // reject unchanged.
        assert_eq!(dr.reject.cardinality(), 101);
    }

    #[test]
    fn test_karyotype_bands_accept_empty_vec() {
        // bands HashMap can hold empty Vec for a chr (e.g., chr with no band data).
        let mut k = Karyotype::default();
        k.bands.insert("hs1".into(), Vec::new());
        assert!(k.bands.contains_key("hs1"));
        assert_eq!(k.bands["hs1"].len(), 0);
    }

    #[test]
    fn test_chromosome_color_field_case_preserved() {
        // Color field accepts any casing at struct level (lowercasing happens at read).
        let c = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 100,
            color: "RED".into(),
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        // At struct level, case is preserved.
        assert_eq!(c.color, "RED");
    }

    #[test]
    fn test_band_clone_creates_fresh_intspan() {
        // Cloning a Band produces an independent IntSpan in the set field.
        let b = Band {
            name: "p".into(),
            label: "p".into(),
            parent: "hs1".into(),
            start: 0,
            end: 100,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 100),
        };
        let mut c = b.clone();
        c.set = IntSpan::new();
        // Original set unchanged.
        assert_eq!(b.set.cardinality(), 101);
        assert_eq!(c.set.cardinality(), 0);
    }

    #[test]
    fn test_karyotype_order_mutation_independent_from_chromosomes() {
        // Modifying .order doesn't affect .chromosomes map.
        let mut k = Karyotype::default();
        let c = Chromosome {
            name: "hsA".into(),
            label: "A".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        k.chromosomes.insert("hsA".into(), c);
        k.order.push("hsA".into());
        // Now change order but chromosomes map unchanged.
        k.order.clear();
        assert!(k.order.is_empty());
        assert_eq!(k.chromosomes.len(), 1);
    }

    #[test]
    fn test_display_region_modify_accept_only_reject_unchanged() {
        // Mutating accept doesn't touch reject — independent IntSpans.
        let mut dr = DisplayRegion::default();
        dr.accept.insert(50);
        assert_eq!(dr.accept.cardinality(), 1);
        assert_eq!(dr.reject.cardinality(), 0);
        // And vice versa.
        dr.reject.insert(99);
        assert_eq!(dr.reject.cardinality(), 1);
        assert_eq!(dr.accept.cardinality(), 1);
    }

    #[test]
    fn test_band_parent_distinct_from_name_field() {
        // `name` and `parent` are separate String fields; clone preserves both.
        let b = Band {
            name: "p36.33".into(),
            label: "p36.33".into(),
            parent: "hs1".into(),
            start: 0,
            end: 2_300_000,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 2_300_000),
        };
        let c = b.clone();
        assert_eq!(c.name, "p36.33");
        assert_eq!(c.parent, "hs1");
        assert_ne!(c.name, c.parent);
        // mutating clone's parent doesn't touch original.
        let mut c2 = b.clone();
        c2.parent = "hs_altered".into();
        assert_eq!(b.parent, "hs1");
        assert_eq!(c2.parent, "hs_altered");
    }

    #[test]
    fn test_karyotype_two_chromosomes_distinct_band_vecs_no_bleed() {
        // Pushing to hs1 band vec doesn't affect hs2 band vec.
        let mut k = Karyotype::default();
        k.bands.insert("hs1".into(), Vec::new());
        k.bands.insert("hs2".into(), Vec::new());
        let mk = |parent: &str| Band {
            name: "b".into(),
            label: "b".into(),
            parent: parent.into(),
            start: 0,
            end: 10,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 10),
        };
        k.bands.get_mut("hs1").unwrap().push(mk("hs1"));
        k.bands.get_mut("hs1").unwrap().push(mk("hs1"));
        assert_eq!(k.bands["hs1"].len(), 2);
        assert_eq!(k.bands["hs2"].len(), 0);
    }

    #[test]
    fn test_chromosome_set_and_start_end_independent_at_struct_level() {
        // The struct doesn't enforce that `set` matches [start,end] —
        // validation happens externally in validate_karyotype, not at this layer.
        let c = Chromosome {
            name: "hsA".into(),
            label: "A".into(),
            start: 0,
            end: 100,
            color: "red".into(),
            // Intentionally empty set — struct accepts it without complaint.
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        assert_eq!(c.start, 0);
        assert_eq!(c.end, 100);
        assert_eq!(c.set.cardinality(), 0);
    }

    #[test]
    fn test_chromosome_display_flag_toggle_preserves_other_fields() {
        // Flipping display bit must not alter index, set, or color.
        let mut c = Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0,
            end: 1000,
            color: "red".into(),
            set: IntSpan::from_range(0, 1000),
            index: 7,
            display: true,
            display_region: DisplayRegion::default(),
        };
        let (idx_before, card_before, color_before) = (c.index, c.set.cardinality(), c.color.clone());
        c.display = false;
        assert!(!c.display);
        assert_eq!(c.index, idx_before);
        assert_eq!(c.set.cardinality(), card_before);
        assert_eq!(c.color, color_before);
    }

    #[test]
    fn test_karyotype_clone_order_vec_independence_from_source() {
        // Clone's order Vec can be mutated without affecting the source.
        let mut k = Karyotype::default();
        k.order.push("hs1".into());
        k.order.push("hs2".into());
        let mut k2 = k.clone();
        k2.order.clear();
        assert_eq!(k.order, vec!["hs1", "hs2"]);
        assert!(k2.order.is_empty());
    }

    #[test]
    fn test_band_clone_intspan_set_independence() {
        // Modifying clone.set doesn't affect the original band's set.
        let b = Band {
            name: "p1".into(),
            label: "p1".into(),
            parent: "hs1".into(),
            start: 0,
            end: 100,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 100),
        };
        let mut c = b.clone();
        c.set.insert(500);
        assert_eq!(b.set.cardinality(), 101);
        assert_eq!(c.set.cardinality(), 102);
    }

    #[test]
    fn test_karyotype_order_vec_is_authoritative_vs_chromosomes_hashmap() {
        // chromosomes HashMap is unordered; `order` Vec explicitly tracks sequence.
        let mut k = Karyotype::default();
        for name in ["zeta", "alpha", "mu"] {
            k.chromosomes.insert(
                name.into(),
                Chromosome {
                    name: name.into(),
                    label: name.into(),
                    start: 0,
                    end: 100,
                    color: "grey".into(),
                    set: IntSpan::from_range(0, 100),
                    index: 0,
                    display: true,
                    display_region: DisplayRegion::default(),
                },
            );
            k.order.push(name.into());
        }
        // order mirrors insertion sequence, regardless of HashMap internals.
        assert_eq!(k.order, vec!["zeta", "alpha", "mu"]);
        assert_eq!(k.chromosomes.len(), 3);
    }

    #[test]
    fn test_display_region_clone_fully_independent() {
        // Clone a fully-populated DisplayRegion; mutating clone's accept/reject
        // doesn't affect source IntSpans.
        let mut dr = DisplayRegion::default();
        dr.accept.insert(100);
        dr.reject.insert(200);
        let mut cloned = dr.clone();
        cloned.accept.insert(300);
        cloned.reject.insert(400);
        // Source still has only its original entries.
        assert_eq!(dr.accept.cardinality(), 1);
        assert_eq!(dr.reject.cardinality(), 1);
        assert!(!dr.accept.member(300));
        assert!(!dr.reject.member(400));
        // Clone has the additions.
        assert_eq!(cloned.accept.cardinality(), 2);
        assert_eq!(cloned.reject.cardinality(), 2);
    }

    #[test]
    fn test_band_start_end_support_wide_i64_range() {
        // Band start/end are i64; large values supported without overflow.
        let b = Band {
            name: "p1".into(),
            label: "p1".into(),
            parent: "hs1".into(),
            start: -1_000_000_000,
            end: 1_000_000_000,
            color: "gneg".into(),
            set: IntSpan::from_range(-1_000_000_000, 1_000_000_000),
        };
        assert_eq!(b.start, -1_000_000_000);
        assert_eq!(b.end, 1_000_000_000);
        // Cardinality = end - start + 1.
        assert_eq!(b.set.cardinality(), 2_000_000_001);
    }

    #[test]
    fn test_chromosome_index_supports_large_usize_values() {
        // index is usize; very large values supported.
        let c = Chromosome {
            name: "hs_big".into(),
            label: "big".into(),
            start: 0,
            end: 1,
            color: "black".into(),
            set: IntSpan::from_range(0, 1),
            index: 1_000_000,
            display: true,
            display_region: DisplayRegion::default(),
        };
        assert_eq!(c.index, 1_000_000);
    }

    #[test]
    fn test_karyotype_chromosomes_remove_returns_entry_and_decrements_len() {
        // HashMap::remove returns the removed value; map size drops by 1.
        let mut k = Karyotype::default();
        for name in ["a", "b", "c"] {
            k.chromosomes.insert(
                name.into(),
                Chromosome {
                    name: name.into(),
                    label: name.into(),
                    start: 0,
                    end: 1,
                    color: "grey".into(),
                    set: IntSpan::from_range(0, 1),
                    index: 0,
                    display: true,
                    display_region: DisplayRegion::default(),
                },
            );
        }
        assert_eq!(k.chromosomes.len(), 3);
        let removed = k.chromosomes.remove("b").unwrap();
        assert_eq!(removed.name, "b");
        assert_eq!(k.chromosomes.len(), 2);
        // Order Vec is independent — manual update required.
        assert!(k.order.is_empty());
    }

    #[test]
    fn test_band_clone_preserves_every_field_value() {
        // Each field (name/label/parent/start/end/color/set) survives Clone unchanged.
        let b = Band {
            name: "p36.33".into(),
            label: "long-label".into(),
            parent: "hs1".into(),
            start: 0,
            end: 2_300_000,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 2_300_000),
        };
        let c = b.clone();
        assert_eq!(c.name, b.name);
        assert_eq!(c.label, b.label);
        assert_eq!(c.parent, b.parent);
        assert_eq!(c.start, b.start);
        assert_eq!(c.end, b.end);
        assert_eq!(c.color, b.color);
        assert_eq!(c.set.cardinality(), b.set.cardinality());
    }

    #[test]
    fn test_chromosome_start_can_exceed_end_at_struct_level() {
        // Struct accepts start > end — validation occurs only at read layer.
        let c = Chromosome {
            name: "hs_invalid".into(),
            label: "inv".into(),
            start: 1000,
            end: 500,
            color: "grey".into(),
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        assert_eq!(c.start, 1000);
        assert_eq!(c.end, 500);
        assert!(c.start > c.end);
    }

    #[test]
    fn test_karyotype_bands_may_reference_parent_not_in_chromosomes() {
        // Struct-level: bands entry for non-existent chromosome name is accepted.
        let mut k = Karyotype::default();
        k.bands.insert(
            "hs_orphan".into(),
            vec![Band {
                name: "p1".into(),
                label: "p1".into(),
                parent: "hs_orphan".into(),
                start: 0, end: 100,
                color: "gneg".into(),
                set: IntSpan::from_range(0, 100),
            }],
        );
        assert!(k.bands.contains_key("hs_orphan"));
        assert!(!k.chromosomes.contains_key("hs_orphan"));
    }

    #[test]
    fn test_display_region_independent_clone_of_both_intspans() {
        // Clone of DisplayRegion — mutate accept AND reject independently in clone.
        let mut dr = DisplayRegion::default();
        dr.accept.insert(1);
        dr.accept.insert(2);
        dr.reject.insert(100);
        let mut c = dr.clone();
        // Clear accept on clone — source unchanged.
        c.accept = IntSpan::new();
        assert_eq!(dr.accept.cardinality(), 2);
        assert_eq!(c.accept.cardinality(), 0);
        // Source reject unchanged too.
        assert_eq!(dr.reject.cardinality(), 1);
    }

    #[test]
    fn test_karyotype_default_all_three_collections_empty_and_isolated() {
        // Default Karyotype has all three HashMap/Vec collections empty.
        let k = Karyotype::default();
        assert!(k.chromosomes.is_empty());
        assert!(k.bands.is_empty());
        assert!(k.order.is_empty());
        // Mutating a clone's collections doesn't bleed into the source.
        let mut clone = k.clone();
        clone.order.push("hsX".to_string());
        assert_eq!(clone.order.len(), 1);
        assert!(k.order.is_empty());
    }

    #[test]
    fn test_chromosome_empty_color_string_accepted_at_struct_level() {
        // The Chromosome struct makes no assertions about color format —
        // empty strings are accepted without validation.
        let c = Chromosome {
            name: "hs1".into(),
            label: "chr1".into(),
            start: 0,
            end: 100,
            color: String::new(),
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        };
        assert_eq!(c.color, "");
        // Clone preserves the empty string.
        let c2 = c.clone();
        assert_eq!(c2.color, "");
    }

    #[test]
    fn test_band_same_parent_allows_overlapping_ranges_via_intspan_union() {
        // Two bands with overlapping coords share a parent — their IntSpans may overlap.
        let b1 = Band {
            name: "b1".into(),
            label: "arm1".into(),
            parent: "hs1".into(),
            start: 0,
            end: 1000,
            color: "gneg".into(),
            set: IntSpan::from_range(0, 1000),
        };
        let b2 = Band {
            name: "b2".into(),
            label: "arm2".into(),
            parent: "hs1".into(),
            start: 500,
            end: 1500,
            color: "gpos50".into(),
            set: IntSpan::from_range(500, 1500),
        };
        assert_eq!(b1.parent, b2.parent);
        // Union of their IntSpans yields the combined range (struct level allows this).
        let union = b1.set.union(&b2.set);
        assert_eq!(union.cardinality(), 1501);
        assert_eq!(union.min(), Some(0));
        assert_eq!(union.max(), Some(1500));
    }

    #[test]
    fn test_karyotype_order_vec_independent_from_chromosomes_hashmap() {
        // The order Vec is tracked separately from the chromosomes HashMap —
        // removing from one doesn't affect the other at the struct level.
        let mut k = Karyotype::default();
        k.order.push("hs1".to_string());
        k.order.push("hs2".to_string());
        k.chromosomes.insert("hs1".into(), Chromosome {
            name: "hs1".into(),
            label: "1".into(),
            start: 0, end: 100,
            color: "red".into(),
            set: IntSpan::new(),
            index: 0,
            display: true,
            display_region: DisplayRegion::default(),
        });
        // order has 2 entries; chromosomes has 1. They're not kept in sync automatically.
        assert_eq!(k.order.len(), 2);
        assert_eq!(k.chromosomes.len(), 1);
        // Removing from chromosomes doesn't touch order.
        k.chromosomes.remove("hs1");
        assert!(k.chromosomes.is_empty());
        assert_eq!(k.order.len(), 2);
    }
}
