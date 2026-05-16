use std::collections::HashMap;

use crate::intspan::IntSpan;

/// The type of data in a data file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataType {
    Highlight,
    Link,
    Plot,
    Text,
    Tile,
    Connector,
}

/// A single data point (one line in a data file).
#[derive(Debug, Clone, Default)]
pub struct Datum {
    /// Core data fields.
    pub chr: String,
    pub start: i64,
    pub end: i64,
    pub set: IntSpan,
    /// For links: the link ID.
    pub id: Option<String>,
    /// For plots: the numeric value(s).
    pub value: Option<f64>,
    /// For text: the label.
    pub label: Option<String>,
    /// Key-value options from the data line.
    pub param: HashMap<String, String>,
}

/// A link: a pair (or group) of data points with the same ID.
#[derive(Debug, Clone)]
pub struct Link {
    pub id: String,
    pub points: Vec<Datum>,
    pub param: HashMap<String, String>,
}

/// A named data set (e.g., one <link segdup> block or <highlight> block).
#[derive(Debug, Clone)]
pub struct DataSet {
    pub name: String,
    pub data_type: DataType,
    pub data: Vec<Datum>,
    /// For links: grouped by ID.
    pub links: Vec<Link>,
    /// Parameters from the config block.
    pub param: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datum_default_is_empty() {
        // Default Datum has empty chr/id/label/value/param and zeroed coords.
        let d = Datum::default();
        assert_eq!(d.chr, "");
        assert_eq!(d.start, 0);
        assert_eq!(d.end, 0);
        assert!(d.id.is_none());
        assert!(d.value.is_none());
        assert!(d.label.is_none());
        assert!(d.param.is_empty());
        // IntSpan default is empty.
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_datatype_equality_and_copy_semantics() {
        // DataType is Copy + PartialEq — let tests share a value without move.
        let a = DataType::Highlight;
        let b = a;
        assert_eq!(a, b);
        assert_eq!(a, DataType::Highlight);
        assert_ne!(DataType::Highlight, DataType::Link);
        assert_ne!(DataType::Plot, DataType::Tile);
        assert_ne!(DataType::Text, DataType::Connector);
    }

    #[test]
    fn test_datum_clone_is_independent() {
        // Clone produces a fresh map; mutating the clone doesn't affect the source.
        let mut d = Datum {
            chr: "hs1".into(),
            start: 10,
            end: 20,
            ..Default::default()
        };
        d.param.insert("color".into(), "red".into());
        let mut d2 = d.clone();
        d2.chr = "hs2".into();
        d2.param.insert("color".into(), "blue".into());
        assert_eq!(d.chr, "hs1");
        assert_eq!(d.param.get("color").map(String::as_str), Some("red"));
        assert_eq!(d2.chr, "hs2");
        assert_eq!(d2.param.get("color").map(String::as_str), Some("blue"));
    }

    #[test]
    fn test_dataset_construction_and_clone() {
        // DataSet holds name/data_type/data/links/param; Clone preserves all.
        let ds = DataSet {
            name: "mydata".into(),
            data_type: DataType::Highlight,
            data: vec![
                Datum { chr: "c1".into(), start: 0, end: 100, ..Default::default() },
                Datum { chr: "c2".into(), start: 200, end: 300, ..Default::default() },
            ],
            links: Vec::new(),
            param: HashMap::new(),
        };
        let c = ds.clone();
        assert_eq!(c.name, "mydata");
        assert_eq!(c.data_type, DataType::Highlight);
        assert_eq!(c.data.len(), 2);
        assert!(c.links.is_empty());
        assert!(c.param.is_empty());
    }

    #[test]
    fn test_datum_set_field_independent_from_start_end() {
        // The `set` IntSpan is a separate field — you can have start=0/end=100
        // but an empty `set` (or vice versa).
        let d = Datum {
            chr: "c1".into(),
            start: 0,
            end: 100,
            set: IntSpan::new(), // explicitly empty
            ..Default::default()
        };
        assert_eq!(d.start, 0);
        assert_eq!(d.end, 100);
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_link_param_mutation_doesnt_leak_to_clone() {
        // Cloning a Link yields a fresh param map — mutation isolation.
        let mut l = Link {
            id: "x".into(),
            points: vec![],
            param: HashMap::new(),
        };
        l.param.insert("color".into(), "red".into());
        let mut clone = l.clone();
        clone.param.insert("color".into(), "blue".into());
        assert_eq!(l.param.get("color").map(String::as_str), Some("red"));
        assert_eq!(clone.param.get("color").map(String::as_str), Some("blue"));
    }

    #[test]
    fn test_dataset_supports_all_data_types() {
        // DataSet can be constructed for each DataType variant.
        for t in [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ] {
            let ds = DataSet {
                name: format!("{:?}", t),
                data_type: t,
                data: Vec::new(),
                links: Vec::new(),
                param: HashMap::new(),
            };
            assert_eq!(ds.data_type, t);
            assert!(!ds.name.is_empty());
        }
    }

    #[test]
    fn test_link_construction_preserves_fields() {
        // Link holds its id, points slice, and a separate param HashMap.
        let points = vec![
            Datum { chr: "c1".into(), start: 0, end: 100, ..Default::default() },
            Datum { chr: "c2".into(), start: 200, end: 300, ..Default::default() },
        ];
        let link = Link {
            id: "l1".into(),
            points,
            param: HashMap::new(),
        };
        assert_eq!(link.id, "l1");
        assert_eq!(link.points.len(), 2);
        assert_eq!(link.points[0].chr, "c1");
        assert_eq!(link.points[1].end, 300);
        // param starts empty.
        assert!(link.param.is_empty());
    }

    #[test]
    fn test_datum_with_value_some_and_label_some() {
        // Datum can carry both a numeric value and a text label.
        let d = Datum {
            chr: "hs1".into(),
            start: 0,
            end: 100,
            value: Some(3.14),
            label: Some("gene_A".into()),
            id: Some("uid-42".into()),
            ..Default::default()
        };
        assert_eq!(d.value, Some(3.14));
        assert_eq!(d.label.as_deref(), Some("gene_A"));
        assert_eq!(d.id.as_deref(), Some("uid-42"));
    }

    #[test]
    fn test_datatype_debug_names_match_variant_identifiers() {
        // Debug derive yields the bare variant name (e.g. "Highlight").
        for (t, name) in [
            (DataType::Highlight, "Highlight"),
            (DataType::Link, "Link"),
            (DataType::Plot, "Plot"),
            (DataType::Text, "Text"),
            (DataType::Tile, "Tile"),
            (DataType::Connector, "Connector"),
        ] {
            let s = format!("{:?}", t);
            assert_eq!(s, name);
        }
    }

    #[test]
    fn test_datum_param_multiple_keys_roundtrip() {
        // param holds arbitrary key-value strings; inserting several preserves all.
        let mut d = Datum::default();
        d.param.insert("color".into(), "red".into());
        d.param.insert("stroke_thickness".into(), "2".into());
        d.param.insert("fill_color".into(), "transparent".into());
        assert_eq!(d.param.len(), 3);
        assert_eq!(d.param["color"], "red");
        assert_eq!(d.param["stroke_thickness"], "2");
        assert_eq!(d.param["fill_color"], "transparent");
        // Overwriting an existing key replaces its value.
        d.param.insert("color".into(), "blue".into());
        assert_eq!(d.param["color"], "blue");
        assert_eq!(d.param.len(), 3);
    }

    #[test]
    fn test_link_with_three_points_preserves_all() {
        // A Link may carry >2 points; all preserved in insertion order.
        let link = Link {
            id: "trio".into(),
            points: vec![
                Datum { chr: "A".into(), start: 10, end: 20, ..Default::default() },
                Datum { chr: "B".into(), start: 30, end: 40, ..Default::default() },
                Datum { chr: "C".into(), start: 50, end: 60, ..Default::default() },
            ],
            param: HashMap::new(),
        };
        assert_eq!(link.points.len(), 3);
        assert_eq!(link.points[0].chr, "A");
        assert_eq!(link.points[1].start, 30);
        assert_eq!(link.points[2].end, 60);
        // Clone preserves all 3.
        let c = link.clone();
        assert_eq!(c.points.len(), 3);
        assert_eq!(c.points[2].chr, "C");
    }

    #[test]
    fn test_datum_start_end_i64_accepts_genomic_scale() {
        // start/end are i64 — must hold human-genome-scale values without overflow.
        let d = Datum {
            chr: "hs1".into(),
            start: 0,
            end: 247_249_719,
            ..Default::default()
        };
        assert_eq!(d.end, 247_249_719);
        // Negative values accepted (no clamp/validation in types).
        let d2 = Datum {
            chr: "neg".into(),
            start: -100,
            end: -50,
            ..Default::default()
        };
        assert_eq!(d2.start, -100);
        assert_eq!(d2.end, -50);
    }

    #[test]
    fn test_link_points_vec_grows_and_shrinks() {
        // Vec operations on Link.points work as expected.
        let mut link = Link {
            id: "mutable".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        assert!(link.points.is_empty());
        for i in 0..5 {
            link.points.push(Datum {
                chr: format!("chr{}", i),
                start: (i as i64) * 100,
                end: (i as i64) * 100 + 50,
                ..Default::default()
            });
        }
        assert_eq!(link.points.len(), 5);
        // Remove middle.
        link.points.remove(2);
        assert_eq!(link.points.len(), 4);
        assert_eq!(link.points[2].chr, "chr3"); // what was chr3 is now at index 2
        // Clear all.
        link.points.clear();
        assert!(link.points.is_empty());
    }

    #[test]
    fn test_dataset_clone_is_deep_for_all_vecs() {
        // DataSet has 3 owned collections: data, links, param. Clone must be deep.
        let mut ds = DataSet {
            name: "n".into(),
            data_type: DataType::Plot,
            data: vec![Datum { chr: "a".into(), start: 0, end: 10, ..Default::default() }],
            links: Vec::new(),
            param: HashMap::new(),
        };
        ds.param.insert("k".into(), "v".into());
        let mut c = ds.clone();
        // Mutate clone's data + param — source unaffected.
        c.data.push(Datum { chr: "b".into(), start: 20, end: 30, ..Default::default() });
        c.param.insert("k2".into(), "v2".into());
        c.name = "changed".into();
        assert_eq!(ds.data.len(), 1);
        assert_eq!(ds.param.len(), 1);
        assert_eq!(ds.name, "n");
        assert_eq!(c.data.len(), 2);
        assert_eq!(c.param.len(), 2);
        assert_eq!(c.name, "changed");
    }

    #[test]
    fn test_datatype_all_variants_constructable_via_datum_value() {
        // Verify each DataType variant can be used to instantiate a DataSet with
        // a matching Datum — tests the enum's compatibility with the Datum shape.
        for dt in [DataType::Highlight, DataType::Link, DataType::Plot, DataType::Text, DataType::Tile, DataType::Connector] {
            let datum = match dt {
                DataType::Plot => Datum { value: Some(1.0), ..Default::default() },
                DataType::Text => Datum { label: Some("lbl".into()), ..Default::default() },
                DataType::Link => Datum { id: Some("link1".into()), ..Default::default() },
                _ => Datum::default(),
            };
            let ds = DataSet {
                name: format!("{:?}", dt),
                data_type: dt,
                data: vec![datum],
                links: Vec::new(),
                param: HashMap::new(),
            };
            assert_eq!(ds.data.len(), 1);
            assert_eq!(ds.data_type, dt);
        }
    }

    #[test]
    fn test_datum_default_fields_exactly() {
        // Default Datum: all strings empty, numerics 0, options None.
        let d = Datum::default();
        assert_eq!(d.chr, "");
        assert_eq!(d.start, 0);
        assert_eq!(d.end, 0);
        assert!(d.id.is_none());
        assert!(d.value.is_none());
        assert!(d.label.is_none());
        assert!(d.param.is_empty());
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_link_construction_via_macro_style_syntax() {
        // Exhaustive field constructor via struct literal syntax.
        let link = Link {
            id: "myid".into(),
            points: vec![Datum::default()],
            param: {
                let mut p = HashMap::new();
                p.insert("k1".into(), "v1".into());
                p.insert("k2".into(), "v2".into());
                p
            },
        };
        assert_eq!(link.id, "myid");
        assert_eq!(link.points.len(), 1);
        assert_eq!(link.param.len(), 2);
        assert_eq!(link.param["k1"], "v1");
    }

    #[test]
    fn test_dataset_links_field_mutation_independent() {
        // Clone DataSet with populated links — mutating clone's links doesn't affect source.
        let ds = DataSet {
            name: "orig".into(),
            data_type: DataType::Link,
            data: Vec::new(),
            links: vec![Link {
                id: "l1".into(),
                points: Vec::new(),
                param: HashMap::new(),
            }],
            param: HashMap::new(),
        };
        let mut c = ds.clone();
        c.links.push(Link {
            id: "l2".into(),
            points: Vec::new(),
            param: HashMap::new(),
        });
        assert_eq!(ds.links.len(), 1);
        assert_eq!(c.links.len(), 2);
    }

    #[test]
    fn test_datum_label_value_id_are_independent_options() {
        // All 3 Option fields toggle independently.
        let mut d = Datum::default();
        d.value = Some(3.14);
        assert!(d.value.is_some());
        assert!(d.label.is_none());
        assert!(d.id.is_none());
        d.label = Some("text".into());
        assert!(d.value.is_some());
        assert!(d.label.is_some());
        d.id = Some("abc".into());
        assert!(d.value.is_some());
        assert!(d.label.is_some());
        assert!(d.id.is_some());
        // Clearing one doesn't affect others.
        d.value = None;
        assert!(d.value.is_none());
        assert!(d.label.is_some());
        assert!(d.id.is_some());
    }

    #[test]
    fn test_datatype_copy_trait_allows_pass_by_value() {
        // DataType is Copy — can pass by value without move.
        let original = DataType::Highlight;
        let copy1 = original;
        let copy2 = original;
        // Both copies usable, original still accessible.
        assert_eq!(original, DataType::Highlight);
        assert_eq!(copy1, DataType::Highlight);
        assert_eq!(copy2, DataType::Highlight);
    }

    #[test]
    fn test_datum_set_intspan_independent_from_chr() {
        // set field is unrelated to chr String — they are independent fields.
        let mut d = Datum::default();
        d.chr = "hs1".into();
        d.set = IntSpan::from_range(0, 100);
        assert_eq!(d.chr, "hs1");
        assert_eq!(d.set.cardinality(), 101);
        // Change chr — set unchanged.
        d.chr = "hs2".into();
        assert_eq!(d.set.cardinality(), 101);
        // Change set — chr unchanged.
        d.set = IntSpan::new();
        assert_eq!(d.chr, "hs2");
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_link_id_field_separate_from_points_ids() {
        // Link.id is the link-level identifier; points may have their own Option<String> id.
        let link = Link {
            id: "link_id".into(),
            points: vec![
                Datum {
                    chr: "a".into(),
                    id: Some("point_1_id".into()),
                    ..Default::default()
                },
                Datum {
                    chr: "b".into(),
                    id: Some("point_2_id".into()),
                    ..Default::default()
                },
            ],
            param: HashMap::new(),
        };
        assert_eq!(link.id, "link_id");
        assert_eq!(link.points[0].id.as_deref(), Some("point_1_id"));
        assert_eq!(link.points[1].id.as_deref(), Some("point_2_id"));
        // Independent: link.id != points[0].id.
        assert_ne!(link.id, link.points[0].id.as_deref().unwrap());
    }

    #[test]
    fn test_dataset_all_option_fields_empty_by_default() {
        // A freshly constructed DataSet with Vec::new() everywhere has all-empty fields.
        let ds = DataSet {
            name: "test".into(),
            data_type: DataType::Highlight,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(ds.name, "test");
        assert_eq!(ds.data_type, DataType::Highlight);
        assert!(ds.data.is_empty());
        assert!(ds.links.is_empty());
        assert!(ds.param.is_empty());
    }

    #[test]
    fn test_datum_chr_string_independent_of_start_end_i64() {
        // Changing chr doesn't affect start/end.
        let mut d = Datum::default();
        d.chr = "hs1".into();
        d.start = 100;
        d.end = 200;
        assert_eq!(d.start, 100);
        assert_eq!(d.end, 200);
        d.chr = "hs2".into();
        assert_eq!(d.start, 100);
        assert_eq!(d.end, 200);
    }

    #[test]
    fn test_datum_param_with_many_entries() {
        // Datum.param can hold many key-value pairs.
        let mut d = Datum::default();
        for i in 0..50 {
            d.param.insert(format!("k{}", i), format!("v{}", i));
        }
        assert_eq!(d.param.len(), 50);
        assert_eq!(d.param["k25"], "v25");
        assert_eq!(d.param["k0"], "v0");
        assert_eq!(d.param["k49"], "v49");
    }

    #[test]
    fn test_link_points_remove_shifts_indices() {
        // Remove-middle shifts subsequent Vec indices down.
        let mut link = Link {
            id: "t".into(),
            points: vec![
                Datum { chr: "A".into(), ..Default::default() },
                Datum { chr: "B".into(), ..Default::default() },
                Datum { chr: "C".into(), ..Default::default() },
                Datum { chr: "D".into(), ..Default::default() },
            ],
            param: HashMap::new(),
        };
        link.points.remove(1); // remove B
        assert_eq!(link.points.len(), 3);
        assert_eq!(link.points[0].chr, "A");
        assert_eq!(link.points[1].chr, "C"); // C shifted to idx 1
        assert_eq!(link.points[2].chr, "D");
    }

    #[test]
    fn test_dataset_data_and_links_independent_collections() {
        // data and links are separate Vecs in DataSet.
        let mut ds = DataSet {
            name: "mixed".into(),
            data_type: DataType::Link,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        ds.data.push(Datum { chr: "dA".into(), ..Default::default() });
        ds.links.push(Link { id: "lA".into(), points: Vec::new(), param: HashMap::new() });
        assert_eq!(ds.data.len(), 1);
        assert_eq!(ds.links.len(), 1);
        // Clearing data doesn't affect links.
        ds.data.clear();
        assert_eq!(ds.data.len(), 0);
        assert_eq!(ds.links.len(), 1);
    }

    #[test]
    fn test_datum_value_nan_and_infinity_preserved() {
        // Some(NaN) and Some(inf) stored verbatim — compared via is_nan()/is_infinite().
        let d_nan = Datum { value: Some(f64::NAN), ..Default::default() };
        assert!(d_nan.value.unwrap().is_nan());
        let d_inf = Datum { value: Some(f64::INFINITY), ..Default::default() };
        assert!(d_inf.value.unwrap().is_infinite());
        assert!(d_inf.value.unwrap() > 0.0);
        let d_ninf = Datum { value: Some(f64::NEG_INFINITY), ..Default::default() };
        assert!(d_ninf.value.unwrap().is_infinite());
        assert!(d_ninf.value.unwrap() < 0.0);
    }

    #[test]
    fn test_datum_param_overwrites_on_repeat_key_insert() {
        // HashMap::insert replaces existing value; len stays 1 after second insert.
        let mut d = Datum::default();
        d.param.insert("color".into(), "red".into());
        d.param.insert("color".into(), "blue".into());
        assert_eq!(d.param.len(), 1);
        assert_eq!(d.param.get("color").map(String::as_str), Some("blue"));
    }

    #[test]
    fn test_link_empty_points_vec_struct_is_valid() {
        // Link struct allows empty points at type level — grouping/validation
        // happens externally.
        let l = Link {
            id: "empty".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(l.id, "empty");
        assert_eq!(l.points.len(), 0);
        assert!(l.param.is_empty());
        // Clone preserves both.
        let l2 = l.clone();
        assert_eq!(l2.id, "empty");
        assert_eq!(l2.points.len(), 0);
    }

    #[test]
    fn test_dataset_data_type_not_enforced_against_contents() {
        // DataSet{data_type=Link, ...} with highlight-style data in `data` — no
        // runtime check at struct level; all fields accept whatever is stored.
        let ds = DataSet {
            name: "mismatched".into(),
            data_type: DataType::Link,
            data: vec![
                Datum { chr: "hs1".into(), start: 0, end: 100, ..Default::default() },
            ],
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(ds.data_type, DataType::Link);
        assert_eq!(ds.data.len(), 1);
        assert_eq!(ds.links.len(), 0);
        // No panic or validation — mismatch tolerated at this layer.
    }

    #[test]
    fn test_datum_id_and_label_options_toggle_independently() {
        // Option<String> for id and label are stored separately.
        let mut d = Datum::default();
        d.id = Some("link1".into());
        assert_eq!(d.id.as_deref(), Some("link1"));
        assert!(d.label.is_none());
        d.label = Some("gene-A".into());
        assert_eq!(d.label.as_deref(), Some("gene-A"));
        // id still present.
        assert_eq!(d.id.as_deref(), Some("link1"));
        // Clearing id leaves label intact.
        d.id = None;
        assert!(d.id.is_none());
        assert_eq!(d.label.as_deref(), Some("gene-A"));
    }

    #[test]
    fn test_link_param_independent_of_point_params() {
        // Link.param is set-level; individual Datum.param entries are per-point.
        let mut point = Datum::default();
        point.param.insert("color".into(), "red".into());
        let mut link_param: HashMap<String, String> = HashMap::new();
        link_param.insert("thickness".into(), "3".into());
        let l = Link {
            id: "L1".into(),
            points: vec![point],
            param: link_param,
        };
        // Point's color only on point.param; link's thickness only on Link.param.
        assert_eq!(l.points[0].param.get("color").map(String::as_str), Some("red"));
        assert!(l.points[0].param.get("thickness").is_none());
        assert_eq!(l.param.get("thickness").map(String::as_str), Some("3"));
        assert!(l.param.get("color").is_none());
    }

    #[test]
    fn test_dataset_name_independent_of_data_type_and_param() {
        // name String is purely descriptive — mutating doesn't affect other fields.
        let mut ds = DataSet {
            name: "initial".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        ds.param.insert("key".into(), "value".into());
        ds.name = "renamed".into();
        assert_eq!(ds.name, "renamed");
        assert_eq!(ds.data_type, DataType::Plot);
        assert_eq!(ds.param.get("key").map(String::as_str), Some("value"));
    }

    #[test]
    fn test_datatype_all_six_variants_pairwise_not_equal() {
        // All six DataType variants are distinct via PartialEq.
        let variants = [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ];
        for i in 0..variants.len() {
            for j in 0..variants.len() {
                if i != j {
                    assert_ne!(variants[i], variants[j]);
                }
            }
        }
    }

    #[test]
    fn test_datum_default_intspan_has_zero_cardinality() {
        // Default::default for Datum → IntSpan::default() → cardinality 0.
        let d = Datum::default();
        assert_eq!(d.set.cardinality(), 0);
        assert!(!d.set.is_universal());
    }

    #[test]
    fn test_link_clone_produces_independent_points_vec() {
        // Clone.points can be mutated without affecting source.
        let mut l = Link {
            id: "L".into(),
            points: vec![
                Datum { chr: "a".into(), ..Default::default() },
                Datum { chr: "b".into(), ..Default::default() },
            ],
            param: HashMap::new(),
        };
        let mut cloned = l.clone();
        cloned.points.clear();
        assert_eq!(l.points.len(), 2);
        assert_eq!(cloned.points.len(), 0);
        // Mutate source instead — clone's len unchanged.
        l.points.push(Datum { chr: "c".into(), ..Default::default() });
        assert_eq!(l.points.len(), 3);
        assert_eq!(cloned.points.len(), 0);
    }

    #[test]
    fn test_datatype_copy_semantics_allows_use_after_copy() {
        // DataType is Copy — using `a` after let b = a still works.
        let a = DataType::Highlight;
        let _b = a;
        let _c = a;
        assert_eq!(a, DataType::Highlight);
    }

    #[test]
    fn test_datum_start_end_negative_i64_values_stored() {
        // start/end are i64 — negatives stored without overflow.
        let d = Datum {
            chr: "c".into(),
            start: -1_000_000_000,
            end: -100,
            ..Default::default()
        };
        assert_eq!(d.start, -1_000_000_000);
        assert_eq!(d.end, -100);
        // Very large positive also supported.
        let d2 = Datum {
            chr: "c".into(),
            start: i64::MAX - 100,
            end: i64::MAX,
            ..Default::default()
        };
        assert_eq!(d2.start, i64::MAX - 100);
        assert_eq!(d2.end, i64::MAX);
    }

    #[test]
    fn test_datum_value_negative_zero_and_positive_zero_coexist() {
        // f64 has distinct -0.0 and +0.0 bit patterns; both stored verbatim.
        let d_neg = Datum { value: Some(-0.0), ..Default::default() };
        let d_pos = Datum { value: Some(0.0), ..Default::default() };
        // Standard equality: -0.0 == 0.0.
        assert_eq!(d_neg.value, d_pos.value);
        // But sign bit differs when observed via is_sign_negative.
        assert!(d_neg.value.unwrap().is_sign_negative());
        assert!(!d_pos.value.unwrap().is_sign_negative());
    }

    #[test]
    fn test_dataset_clone_produces_independent_data_and_links_vecs() {
        // Clone of DataSet — mutations on either vec in clone don't affect source.
        let ds = DataSet {
            name: "orig".into(),
            data_type: DataType::Plot,
            data: vec![Datum { chr: "a".into(), ..Default::default() }],
            links: vec![Link { id: "L1".into(), points: Vec::new(), param: HashMap::new() }],
            param: HashMap::new(),
        };
        let mut c = ds.clone();
        c.data.clear();
        c.links.clear();
        // Source vecs intact.
        assert_eq!(ds.data.len(), 1);
        assert_eq!(ds.links.len(), 1);
        assert_eq!(c.data.len(), 0);
        assert_eq!(c.links.len(), 0);
    }

    #[test]
    fn test_link_param_mutation_in_clone_isolated() {
        // Link.param is a HashMap — Clone produces a fresh map.
        let mut l = Link {
            id: "L".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        l.param.insert("k".into(), "v".into());
        let mut c = l.clone();
        c.param.insert("new_key".into(), "new_val".into());
        // Source still has 1 entry; clone has 2.
        assert_eq!(l.param.len(), 1);
        assert_eq!(c.param.len(), 2);
    }

    #[test]
    fn test_datum_param_remove_returns_value_and_shrinks_map() {
        // HashMap::remove returns Some(value) + decrements len.
        let mut d = Datum::default();
        d.param.insert("color".into(), "red".into());
        d.param.insert("thickness".into(), "2".into());
        let removed = d.param.remove("color");
        assert_eq!(removed.as_deref(), Some("red"));
        assert_eq!(d.param.len(), 1);
        // Remove missing key → None.
        assert!(d.param.remove("nonexistent").is_none());
    }

    #[test]
    fn test_datum_default_id_value_label_are_all_none() {
        // Default Datum — all three Option<...> fields are None.
        let d = Datum::default();
        assert!(d.id.is_none());
        assert!(d.value.is_none());
        assert!(d.label.is_none());
    }

    #[test]
    fn test_datatype_debug_output_contains_variant_name() {
        // The Debug derive on DataType should emit the variant names.
        assert_eq!(format!("{:?}", DataType::Highlight), "Highlight");
        assert_eq!(format!("{:?}", DataType::Link), "Link");
        assert_eq!(format!("{:?}", DataType::Plot), "Plot");
        assert_eq!(format!("{:?}", DataType::Text), "Text");
        assert_eq!(format!("{:?}", DataType::Tile), "Tile");
        assert_eq!(format!("{:?}", DataType::Connector), "Connector");
    }

    #[test]
    fn test_link_with_empty_id_and_points_vec_still_valid() {
        // Struct doesn't enforce non-empty id or points — all empty is fine.
        let l = Link {
            id: String::new(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        assert!(l.id.is_empty());
        assert!(l.points.is_empty());
        assert!(l.param.is_empty());
        // Clone preserves emptiness.
        let l2 = l.clone();
        assert_eq!(l2.id, "");
        assert_eq!(l2.points.len(), 0);
    }

    #[test]
    fn test_dataset_name_field_stores_original_casing() {
        // No normalization on DataSet name — caller-supplied casing preserved.
        let d = DataSet {
            name: "MyDataSet".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(d.name, "MyDataSet");
        // Clone preserves casing.
        let d2 = d.clone();
        assert_eq!(d2.name, "MyDataSet");
    }

    #[test]
    fn test_datum_param_can_store_multiple_distinct_keys() {
        // Datum.param is a HashMap — arbitrary keys supported.
        let mut d = Datum::default();
        d.param.insert("color".into(), "red".into());
        d.param.insert("thickness".into(), "3".into());
        d.param.insert("url".into(), "http://x".into());
        assert_eq!(d.param.len(), 3);
        assert_eq!(d.param.get("color"), Some(&"red".to_string()));
        assert_eq!(d.param.get("thickness"), Some(&"3".to_string()));
        assert_eq!(d.param.get("url"), Some(&"http://x".to_string()));
    }

    #[test]
    fn test_datatype_partial_eq_same_variant_yields_true() {
        // PartialEq derive: same variant → equal; different variant → not equal.
        assert_eq!(DataType::Plot, DataType::Plot);
        assert_eq!(DataType::Link, DataType::Link);
        assert_ne!(DataType::Plot, DataType::Link);
        assert_ne!(DataType::Highlight, DataType::Text);
    }

    #[test]
    fn test_link_points_push_and_iterate_preserves_insertion_order() {
        // Vec<Datum> — push order preserved on iteration.
        let mut l = Link {
            id: "l1".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        for i in 0..5 {
            l.points.push(Datum {
                chr: format!("hs{}", i),
                start: (i * 100) as i64,
                end: ((i + 1) * 100) as i64,
                ..Default::default()
            });
        }
        let chrs: Vec<_> = l.points.iter().map(|p| p.chr.clone()).collect();
        assert_eq!(chrs, vec!["hs0", "hs1", "hs2", "hs3", "hs4"]);
    }

    #[test]
    fn test_datum_value_special_floats_preserved_through_clone() {
        // Special f64 values stored verbatim — clone doesn't normalize them.
        let d = Datum {
            chr: "c".into(),
            value: Some(f64::NEG_INFINITY),
            ..Default::default()
        };
        let d2 = d.clone();
        assert!(d2.value.unwrap().is_infinite());
        assert!(d2.value.unwrap().is_sign_negative());
    }

    #[test]
    fn test_datum_label_field_some_and_none_independent() {
        // Datum label is Option<String> — Some/None toggle independently.
        let d1 = Datum {
            label: Some("gene_X".into()),
            ..Default::default()
        };
        let d2 = Datum {
            label: None,
            ..Default::default()
        };
        assert_eq!(d1.label.as_deref(), Some("gene_X"));
        assert!(d2.label.is_none());
    }

    #[test]
    fn test_dataset_data_and_links_are_distinct_vecs() {
        // DataSet has both `data` and `links` Vecs — they shouldn't alias.
        let mut ds = DataSet {
            name: "n".into(),
            data_type: DataType::Link,
            data: vec![Datum { chr: "hs1".into(), ..Default::default() }],
            links: vec![Link {
                id: "l1".into(),
                points: vec![],
                param: HashMap::new(),
            }],
            param: HashMap::new(),
        };
        // Mutating one doesn't affect the other.
        ds.data.push(Datum { chr: "hs2".into(), ..Default::default() });
        assert_eq!(ds.data.len(), 2);
        assert_eq!(ds.links.len(), 1);
    }

    #[test]
    fn test_datum_param_preserves_empty_string_values() {
        // HashMap<String, String> allows "" values.
        let mut d = Datum::default();
        d.param.insert("empty_key".into(), String::new());
        assert_eq!(d.param.get("empty_key"), Some(&String::new()));
        assert_eq!(d.param.len(), 1);
    }

    #[test]
    fn test_datatype_can_be_copied_by_value_at_call_site() {
        // DataType implements Copy — can be passed by value without move.
        let dt = DataType::Plot;
        let f = |d: DataType| d;
        let r1 = f(dt);
        let r2 = f(dt);
        // Both uses succeed because Copy avoids move.
        assert_eq!(r1, DataType::Plot);
        assert_eq!(r2, DataType::Plot);
    }

    #[test]
    fn test_datum_start_end_large_i64_values_supported() {
        // Datum.start/end are i64 — accepts large values typical of genomic coords.
        let d = Datum {
            chr: "hs1".into(),
            start: 0,
            end: 3_000_000_000,
            ..Default::default()
        };
        assert_eq!(d.end, 3_000_000_000);
    }

    #[test]
    fn test_link_id_is_string_not_option() {
        // Link.id is a plain String (not Option) — an empty string means "no ID".
        let l = Link {
            id: String::new(),
            points: vec![],
            param: HashMap::new(),
        };
        assert_eq!(l.id.len(), 0);
        // Assignment to a normal string is direct.
        let l2 = Link {
            id: "named_link".into(),
            points: vec![],
            param: HashMap::new(),
        };
        assert_eq!(l2.id, "named_link");
    }

    #[test]
    fn test_datum_clone_is_deep_for_set_field() {
        // Datum.set is an IntSpan — clone copies independently.
        use crate::intspan::IntSpan;
        let d1 = Datum {
            chr: "c".into(),
            set: IntSpan::from_range(0, 100),
            ..Default::default()
        };
        let mut d2 = d1.clone();
        d2.set = IntSpan::from_range(200, 300);
        assert_eq!(d1.set.cardinality(), 101);
        assert_eq!(d2.set.cardinality(), 101);
        assert_eq!(d1.set.min(), Some(0));
        assert_eq!(d2.set.min(), Some(200));
    }

    #[test]
    fn test_dataset_param_distinct_from_datum_param() {
        // DataSet.param is a block-level HashMap; each Datum has its own param.
        // Mutating the block param doesn't affect a datum's param.
        let mut ds = DataSet {
            name: "test".into(),
            data_type: DataType::Plot,
            data: vec![Datum {
                chr: "c".into(),
                param: {
                    let mut m = HashMap::new();
                    m.insert("color".into(), "red".into());
                    m
                },
                ..Default::default()
            }],
            links: vec![],
            param: HashMap::new(),
        };
        ds.param.insert("global_color".into(), "blue".into());
        // Datum param is unaffected.
        assert_eq!(ds.data[0].param.get("color"), Some(&"red".to_string()));
        assert!(ds.data[0].param.get("global_color").is_none());
        // Block param has its value.
        assert_eq!(ds.param.get("global_color"), Some(&"blue".to_string()));
    }

    #[test]
    fn test_datum_default_value_and_id_and_label_none_chr_empty() {
        // Default::default for Datum — all fields at their zero value.
        let d = Datum::default();
        assert!(d.chr.is_empty());
        assert_eq!(d.start, 0);
        assert_eq!(d.end, 0);
        assert!(d.id.is_none());
        assert!(d.value.is_none());
        assert!(d.label.is_none());
        assert!(d.param.is_empty());
    }

    #[test]
    fn test_link_points_push_indexed_datum_by_chr() {
        // Push two points to a Link and index them by their chr field.
        let mut link = Link {
            id: "L1".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        link.points.push(Datum { chr: "hsA".into(), ..Default::default() });
        link.points.push(Datum { chr: "hsB".into(), ..Default::default() });
        assert_eq!(link.points[0].chr, "hsA");
        assert_eq!(link.points[1].chr, "hsB");
    }

    #[test]
    fn test_dataset_data_vec_push_preserves_all_fields() {
        // Pushing multiple Datums into DataSet.data retains all.
        let mut ds = DataSet {
            name: "s".into(),
            data_type: DataType::Highlight,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        for i in 0..3 {
            ds.data.push(Datum {
                chr: format!("hs{}", i),
                start: (i * 100) as i64,
                ..Default::default()
            });
        }
        assert_eq!(ds.data.len(), 3);
        assert_eq!(ds.data[0].chr, "hs0");
        assert_eq!(ds.data[2].start, 200);
    }

    #[test]
    fn test_datatype_all_six_variants_not_equal_pairwise() {
        // 6 DataType variants — all pairs distinct.
        let variants = [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ];
        for i in 0..variants.len() {
            for j in 0..variants.len() {
                if i == j {
                    assert_eq!(variants[i], variants[j]);
                } else {
                    assert_ne!(variants[i], variants[j], "{}:{:?} == {}:{:?}", i, variants[i], j, variants[j]);
                }
            }
        }
    }

    #[test]
    fn test_datum_clone_produces_independent_param_map() {
        // Datum clone creates separate param HashMap.
        let d1 = Datum {
            chr: "c".into(),
            param: {
                let mut m = HashMap::new();
                m.insert("k".into(), "v".into());
                m
            },
            ..Default::default()
        };
        let mut d2 = d1.clone();
        d2.param.insert("k2".into(), "v2".into());
        assert_eq!(d1.param.len(), 1);
        assert_eq!(d2.param.len(), 2);
    }

    #[test]
    fn test_link_clone_points_push_after_clone_diverges() {
        // Link clone — points Vec is independent.
        let l1 = Link {
            id: "l1".into(),
            points: vec![Datum { chr: "a".into(), ..Default::default() }],
            param: HashMap::new(),
        };
        let mut l2 = l1.clone();
        l2.points.push(Datum { chr: "b".into(), ..Default::default() });
        assert_eq!(l1.points.len(), 1);
        assert_eq!(l2.points.len(), 2);
    }

    #[test]
    fn test_datum_default_set_is_empty_and_has_zero_cardinality() {
        // Default Datum.set is empty IntSpan.
        let d = Datum::default();
        assert!(d.set.is_empty());
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_dataset_clone_data_and_links_distinct_vecs() {
        // DataSet clone — both data and links are deep-cloned.
        let ds1 = DataSet {
            name: "n".into(),
            data_type: DataType::Plot,
            data: vec![Datum { chr: "a".into(), ..Default::default() }],
            links: vec![Link { id: "l".into(), points: vec![], param: HashMap::new() }],
            param: HashMap::new(),
        };
        let mut ds2 = ds1.clone();
        ds2.data.clear();
        ds2.links.clear();
        // Source preserved.
        assert_eq!(ds1.data.len(), 1);
        assert_eq!(ds1.links.len(), 1);
        assert_eq!(ds2.data.len(), 0);
        assert_eq!(ds2.links.len(), 0);
    }

    #[test]
    fn test_datum_chr_field_empty_string_distinct_from_unset() {
        // chr="" is a valid value; Default also has chr="".
        let d1 = Datum::default();
        let d2 = Datum { chr: String::new(), ..Default::default() };
        assert_eq!(d1.chr, d2.chr);
        assert!(d1.chr.is_empty());
    }

    #[test]
    fn test_link_construction_with_id_preserves_field() {
        // Link.id field preserves caller-provided value.
        let l = Link {
            id: "my_link".into(),
            points: vec![],
            param: HashMap::new(),
        };
        assert_eq!(l.id, "my_link");
    }

    #[test]
    fn test_dataset_data_type_field_all_six_variants_roundtrip() {
        // Each DataType variant can be stored in DataSet.data_type.
        for dt in [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ] {
            let ds = DataSet {
                name: "s".into(),
                data_type: dt,
                data: vec![],
                links: vec![],
                param: HashMap::new(),
            };
            assert_eq!(ds.data_type, dt);
        }
    }

    #[test]
    fn test_datum_value_f64_precision_preserved() {
        // f64 value with many decimal places preserved.
        let d = Datum {
            value: Some(3.141592653589793),
            ..Default::default()
        };
        assert_eq!(d.value, Some(3.141592653589793));
    }

    #[test]
    fn test_datum_param_map_with_multiple_keys_preserves_all_values() {
        // HashMap param stores arbitrary k/v pairs and retains them across use.
        let mut d = Datum::default();
        d.param.insert("color".into(), "red".into());
        d.param.insert("z".into(), "5".into());
        d.param.insert("label".into(), "my_datum".into());
        assert_eq!(d.param.len(), 3);
        assert_eq!(d.param.get("color"), Some(&"red".to_string()));
        assert_eq!(d.param.get("z"), Some(&"5".to_string()));
        assert_eq!(d.param.get("label"), Some(&"my_datum".to_string()));
    }

    #[test]
    fn test_link_empty_points_vec_allowed() {
        // Explicitly empty Link is a valid state.
        let l = Link {
            id: String::new(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        assert!(l.points.is_empty());
        assert!(l.param.is_empty());
        assert!(l.id.is_empty());
    }

    #[test]
    fn test_dataset_name_field_preserved_through_construction() {
        // name distinguishes DataSets in a multi-plot context.
        let ds = DataSet {
            name: "my_plot_1".to_string(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(ds.name, "my_plot_1");
    }

    #[test]
    fn test_datum_start_end_negative_values_allowed() {
        // start/end are i64 — negative values are valid coordinates.
        let d = Datum {
            start: -1000,
            end: -500,
            ..Default::default()
        };
        assert_eq!(d.start, -1000);
        assert_eq!(d.end, -500);
        // start > end is structurally permitted at Datum level (validation elsewhere).
        assert!(d.start < d.end);
    }

    #[test]
    fn test_datum_label_field_optional_some_and_none_distinct() {
        // label=Some("lbl") distinct from label=None.
        let with_label = Datum {
            label: Some("my_label".to_string()),
            ..Default::default()
        };
        let no_label = Datum::default();
        assert_eq!(with_label.label, Some("my_label".to_string()));
        assert!(no_label.label.is_none());
    }

    #[test]
    fn test_dataset_with_links_and_data_both_populated() {
        // DataSet can hold both per-chr data AND grouped links simultaneously.
        let link = Link { id: "l1".into(), points: Vec::new(), param: HashMap::new() };
        let datum = Datum { chr: "hs1".into(), ..Default::default() };
        let ds = DataSet {
            name: "ds".into(),
            data_type: DataType::Link,
            data: vec![datum],
            links: vec![link],
            param: HashMap::new(),
        };
        assert_eq!(ds.data.len(), 1);
        assert_eq!(ds.links.len(), 1);
    }

    #[test]
    fn test_datum_i64_coords_accept_i64_max_boundary() {
        // i64::MAX coordinates are structurally valid.
        let d = Datum {
            start: 0,
            end: i64::MAX,
            ..Default::default()
        };
        assert_eq!(d.end, i64::MAX);
    }

    #[test]
    fn test_datatype_copy_semantics_allow_reuse_after_match() {
        // DataType is Copy — pattern match doesn't consume the value.
        let dt = DataType::Plot;
        let _s = match dt {
            DataType::Plot => "plot",
            _ => "other",
        };
        // After match, dt still usable (requires Copy).
        assert_eq!(dt, DataType::Plot);
    }

    #[test]
    fn test_link_param_overrides_datum_param_by_key() {
        // Link.param and contained Datum.param can hold same keys independently.
        let mut datum = Datum::default();
        datum.param.insert("color".into(), "datum_red".into());
        let mut link = Link {
            id: "l".into(),
            points: vec![datum],
            param: HashMap::new(),
        };
        link.param.insert("color".into(), "link_red".into());
        // Both keys present in their respective scopes.
        assert_eq!(link.param.get("color"), Some(&"link_red".to_string()));
        assert_eq!(link.points[0].param.get("color"), Some(&"datum_red".to_string()));
    }

    #[test]
    fn test_datum_set_field_reflects_coord_range() {
        // Datum.set IntSpan should cover start..=end.
        let d = Datum {
            chr: "hs1".into(),
            start: 100,
            end: 200,
            set: IntSpan::from_range(100, 200),
            ..Default::default()
        };
        assert_eq!(d.set.cardinality(), 101);
        assert!(d.set.member(100));
        assert!(d.set.member(200));
    }

    #[test]
    fn test_dataset_default_data_type_via_explicit_variant() {
        // DataSet constructed with Highlight variant — field accessible.
        let ds = DataSet {
            name: String::new(),
            data_type: DataType::Highlight,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(ds.data_type, DataType::Highlight);
    }

    #[test]
    fn test_datatype_partial_eq_true_for_same_variant_false_across() {
        // Same variant pairs compare equal; cross-variant compares not-equal.
        assert_eq!(DataType::Tile, DataType::Tile);
        assert_ne!(DataType::Tile, DataType::Connector);
        assert_ne!(DataType::Connector, DataType::Text);
    }

    #[test]
    fn test_datum_clone_preserves_all_optional_field_values() {
        // Cloned Datum preserves Option content for id/value/label.
        let original = Datum {
            chr: "hs1".into(),
            start: 100,
            end: 200,
            set: IntSpan::from_range(100, 200),
            id: Some("my_id".into()),
            value: Some(42.5),
            label: Some("my_label".into()),
            param: HashMap::new(),
        };
        let cloned = original.clone();
        assert_eq!(cloned.id, Some("my_id".to_string()));
        assert_eq!(cloned.value, Some(42.5));
        assert_eq!(cloned.label, Some("my_label".to_string()));
    }

    #[test]
    fn test_link_with_many_points_stores_all_in_vec() {
        // Link.points can hold > 2 datums (multi-point link).
        let points: Vec<Datum> = (0..5)
            .map(|i| Datum {
                chr: format!("hs{}", i),
                ..Default::default()
            })
            .collect();
        let l = Link { id: "l".into(), points, param: HashMap::new() };
        assert_eq!(l.points.len(), 5);
        assert_eq!(l.points[0].chr, "hs0");
        assert_eq!(l.points[4].chr, "hs4");
    }

    #[test]
    fn test_dataset_param_holds_config_style_kv_pairs() {
        // DataSet.param mirrors config block overrides.
        let mut ds = DataSet {
            name: "plot".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        ds.param.insert("r0".into(), "0.5r".into());
        ds.param.insert("r1".into(), "0.8r".into());
        assert_eq!(ds.param.len(), 2);
    }

    #[test]
    fn test_datatype_all_six_variants_distinct() {
        // All 6 variants compare as pairwise not-equal.
        let all = [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }

    #[test]
    fn test_datum_id_preserved_through_direct_assignment() {
        // id Option can be set to Some then None and both reflect immediately.
        let mut d = Datum::default();
        assert!(d.id.is_none());
        d.id = Some("x123".to_string());
        assert_eq!(d.id, Some("x123".to_string()));
        d.id = None;
        assert!(d.id.is_none());
    }

    #[test]
    fn test_link_clone_preserves_id_and_points_vec() {
        // Link clone yields deep copy of id and points.
        let original = Link {
            id: "my_link".into(),
            points: vec![Datum {
                chr: "hs1".into(),
                ..Default::default()
            }],
            param: HashMap::new(),
        };
        let cloned = original.clone();
        assert_eq!(cloned.id, "my_link");
        assert_eq!(cloned.points.len(), 1);
        assert_eq!(cloned.points[0].chr, "hs1");
    }

    #[test]
    fn test_dataset_data_vec_grows_on_push() {
        // DataSet.data can be mutated via push — Vec ops preserved.
        let mut ds = DataSet {
            name: String::new(),
            data_type: DataType::Text,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        for i in 0..3 {
            ds.data.push(Datum {
                chr: format!("c{}", i),
                ..Default::default()
            });
        }
        assert_eq!(ds.data.len(), 3);
    }

    #[test]
    fn test_datatype_debug_format_contains_variant_name() {
        // Debug impl includes the variant name for troubleshooting.
        assert_eq!(format!("{:?}", DataType::Highlight), "Highlight");
        assert_eq!(format!("{:?}", DataType::Plot), "Plot");
        assert_eq!(format!("{:?}", DataType::Link), "Link");
    }

    #[test]
    fn test_datum_value_none_distinct_from_some_zero() {
        // None != Some(0.0) — Option semantics.
        let d1 = Datum { value: None, ..Default::default() };
        let d2 = Datum { value: Some(0.0), ..Default::default() };
        assert!(d1.value.is_none());
        assert_eq!(d2.value, Some(0.0));
    }

    #[test]
    fn test_link_points_mutation_via_push_does_not_affect_id() {
        // Mutating points vec leaves other fields unchanged.
        let mut l = Link { id: "my_link".into(), points: Vec::new(), param: HashMap::new() };
        l.points.push(Datum::default());
        l.points.push(Datum::default());
        assert_eq!(l.id, "my_link");
        assert_eq!(l.points.len(), 2);
    }

    #[test]
    fn test_dataset_clone_creates_independent_data_and_links_vecs() {
        // Cloning DataSet yields independent inner Vecs.
        let orig = DataSet {
            name: "n".into(),
            data_type: DataType::Plot,
            data: vec![Datum::default()],
            links: vec![Link { id: "l".into(), points: Vec::new(), param: HashMap::new() }],
            param: HashMap::new(),
        };
        let mut cloned = orig.clone();
        cloned.data.push(Datum::default());
        cloned.links.push(Link { id: "l2".into(), points: Vec::new(), param: HashMap::new() });
        assert_eq!(orig.data.len(), 1);
        assert_eq!(orig.links.len(), 1);
        assert_eq!(cloned.data.len(), 2);
        assert_eq!(cloned.links.len(), 2);
    }

    #[test]
    fn test_datatype_match_exhaustive_covers_all_six_variants() {
        // Ensure a match on all six variants compiles and returns correct tag.
        fn tag(dt: DataType) -> &'static str {
            match dt {
                DataType::Highlight => "hl",
                DataType::Link => "lk",
                DataType::Plot => "pl",
                DataType::Text => "tx",
                DataType::Tile => "tl",
                DataType::Connector => "cn",
            }
        }
        assert_eq!(tag(DataType::Highlight), "hl");
        assert_eq!(tag(DataType::Link), "lk");
        assert_eq!(tag(DataType::Plot), "pl");
        assert_eq!(tag(DataType::Text), "tx");
        assert_eq!(tag(DataType::Tile), "tl");
        assert_eq!(tag(DataType::Connector), "cn");
    }

    #[test]
    fn test_datum_value_f64_min_max_boundary_values() {
        // f64::MIN and MAX stored intact.
        let min_d = Datum { value: Some(f64::MIN), ..Default::default() };
        let max_d = Datum { value: Some(f64::MAX), ..Default::default() };
        assert_eq!(min_d.value, Some(f64::MIN));
        assert_eq!(max_d.value, Some(f64::MAX));
    }

    #[test]
    fn test_link_param_isolation_from_datum_param_across_clone() {
        // Clone: Link.param and inner Datum.param are independent.
        let mut datum = Datum::default();
        datum.param.insert("ck".into(), "cv".into());
        let mut l = Link {
            id: "x".into(),
            points: vec![datum],
            param: HashMap::new(),
        };
        l.param.insert("lk".into(), "lv".into());
        let cloned = l.clone();
        // Cloned has both.
        assert_eq!(cloned.param.get("lk"), Some(&"lv".to_string()));
        assert_eq!(cloned.points[0].param.get("ck"), Some(&"cv".to_string()));
    }

    #[test]
    fn test_dataset_param_cloned_independently_from_data() {
        // Mutating cloned.param doesn't affect orig.param.
        let mut orig = DataSet {
            name: "x".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        orig.param.insert("k".into(), "v".into());
        let mut cloned = orig.clone();
        cloned.param.insert("k2".into(), "v2".into());
        assert_eq!(orig.param.len(), 1);
        assert_eq!(cloned.param.len(), 2);
    }

    #[test]
    fn test_datum_start_end_zero_zero_roundtrip() {
        // start=end=0 stored intact.
        let d = Datum { start: 0, end: 0, ..Default::default() };
        assert_eq!(d.start, 0);
        assert_eq!(d.end, 0);
    }

    #[test]
    fn test_datum_default_value_field_uninitialized_is_none() {
        // Default Datum has value=None (not Some(0)).
        let d = Datum::default();
        assert!(d.value.is_none());
    }

    #[test]
    fn test_link_construction_sets_id_and_empty_fields() {
        // Basic Link construction with only id set.
        let l = Link {
            id: "link_42".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        assert_eq!(l.id, "link_42");
        assert!(l.points.is_empty());
        assert!(l.param.is_empty());
    }

    #[test]
    fn test_dataset_cloned_data_indep_of_orig_after_mutation() {
        // Mutating orig.data doesn't affect cloned.data.
        let mut orig = DataSet {
            name: "ds".into(),
            data_type: DataType::Plot,
            data: vec![Datum::default()],
            links: Vec::new(),
            param: HashMap::new(),
        };
        let cloned = orig.clone();
        orig.data.push(Datum::default());
        assert_eq!(cloned.data.len(), 1);
        assert_eq!(orig.data.len(), 2);
    }

    #[test]
    fn test_datatype_text_variant_distinct_from_tile() {
        // Text and Tile are distinct variants even though semantically similar.
        assert_ne!(DataType::Text, DataType::Tile);
    }

    #[test]
    fn test_datum_param_independent_per_datum_even_in_list() {
        // Each Datum in a Vec has its own param HashMap.
        let mut data: Vec<Datum> = (0..3).map(|_| Datum::default()).collect();
        data[0].param.insert("k0".into(), "v0".into());
        data[1].param.insert("k1".into(), "v1".into());
        data[2].param.insert("k2".into(), "v2".into());
        assert_eq!(data[0].param.len(), 1);
        assert_eq!(data[1].param.len(), 1);
        assert_eq!(data[2].param.len(), 1);
        assert_eq!(data[0].param.get("k0"), Some(&"v0".to_string()));
    }

    #[test]
    fn test_link_points_preserves_order_of_datum_insertion() {
        // Link.points preserves push order.
        let mut l = Link {
            id: "l".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        for i in 0..5 {
            l.points.push(Datum {
                chr: format!("chr{}", i),
                ..Default::default()
            });
        }
        for i in 0..5 {
            assert_eq!(l.points[i].chr, format!("chr{}", i));
        }
    }

    #[test]
    fn test_dataset_empty_default_all_collections_empty() {
        // Empty DataSet has all empty collections.
        let ds = DataSet {
            name: String::new(),
            data_type: DataType::Highlight,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert!(ds.name.is_empty());
        assert!(ds.data.is_empty());
        assert!(ds.links.is_empty());
        assert!(ds.param.is_empty());
    }

    #[test]
    fn test_datatype_iterate_all_variants_by_explicit_collection() {
        // All 6 variants collected → 6 total.
        let all = [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ];
        assert_eq!(all.len(), 6);
    }

    #[test]
    fn test_datum_all_fields_accessible_after_struct_init() {
        // All 8 fields of Datum set and readable.
        let d = Datum {
            chr: "hs1".into(),
            start: 0,
            end: 100,
            set: IntSpan::from_range(0, 100),
            id: Some("id".into()),
            value: Some(3.14),
            label: Some("lbl".into()),
            param: HashMap::new(),
        };
        assert_eq!(d.chr, "hs1");
        assert_eq!(d.start, 0);
        assert_eq!(d.end, 100);
        assert_eq!(d.id, Some("id".to_string()));
        assert_eq!(d.value, Some(3.14));
        assert_eq!(d.label, Some("lbl".to_string()));
    }

    #[test]
    fn test_link_debug_format_includes_id() {
        // Debug impl — id visible in output.
        let l = Link { id: "my_link".into(), points: Vec::new(), param: HashMap::new() };
        let s = format!("{:?}", l);
        assert!(s.contains("my_link"));
    }

    #[test]
    fn test_dataset_param_inserted_keys_readable() {
        // DataSet.param insert + get round-trip.
        let mut ds = DataSet {
            name: "ds".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        ds.param.insert("color".into(), "red".into());
        assert_eq!(ds.param.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn test_datatype_clone_via_copy_matches_original() {
        // DataType Copy → cloning produces identical value.
        let dt = DataType::Plot;
        let copy = dt;
        assert_eq!(dt, copy);
    }

    #[test]
    fn test_datum_with_negative_value_stored_as_negative() {
        // f64 value can be negative.
        let d = Datum {
            value: Some(-1234.56),
            ..Default::default()
        };
        assert_eq!(d.value, Some(-1234.56));
    }

    #[test]
    fn test_link_points_has_2_datums_standard_pair_link() {
        // Standard: Link with 2 datums (source & target).
        let mut l = Link {
            id: "l".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        l.points.push(Datum { chr: "hs1".into(), ..Default::default() });
        l.points.push(Datum { chr: "hs2".into(), ..Default::default() });
        assert_eq!(l.points.len(), 2);
        assert_eq!(l.points[0].chr, "hs1");
        assert_eq!(l.points[1].chr, "hs2");
    }

    #[test]
    fn test_dataset_data_type_link_distinct_from_plot() {
        // Two DataSets with different types should not equate on data_type.
        let ds_link = DataSet {
            name: "l".into(),
            data_type: DataType::Link,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        let ds_plot = DataSet {
            name: "p".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        assert_ne!(ds_link.data_type, ds_plot.data_type);
    }

    #[test]
    fn test_datum_id_can_hold_empty_string_not_none() {
        // Some("") distinct from None for id.
        let d = Datum {
            id: Some("".into()),
            ..Default::default()
        };
        assert_eq!(d.id, Some("".to_string()));
        assert!(d.id.is_some());
    }

    #[test]
    fn test_datum_label_empty_string_distinct_from_none() {
        // Some("") and None are distinct.
        let a = Datum { label: Some("".into()), ..Default::default() };
        let b = Datum { label: None, ..Default::default() };
        assert!(a.label.is_some());
        assert!(b.label.is_none());
    }

    #[test]
    fn test_link_two_distinct_ids_preserve_different_values() {
        // Two different links have independent ids.
        let l1 = Link { id: "a".into(), points: Vec::new(), param: HashMap::new() };
        let l2 = Link { id: "b".into(), points: Vec::new(), param: HashMap::new() };
        assert_ne!(l1.id, l2.id);
    }

    #[test]
    fn test_dataset_param_can_hold_long_kv_pairs() {
        // Long value strings preserved in param map.
        let long_val = "x".repeat(500);
        let mut ds = DataSet {
            name: "ds".into(),
            data_type: DataType::Plot,
            data: Vec::new(),
            links: Vec::new(),
            param: HashMap::new(),
        };
        ds.param.insert("desc".into(), long_val.clone());
        assert_eq!(ds.param.get("desc"), Some(&long_val));
    }

    #[test]
    fn test_datatype_hashable_via_partial_eq_distinct_indexing() {
        // DataType Copy+Eq → usable in match branches for all 6 variants.
        fn classify(dt: DataType) -> u8 {
            match dt {
                DataType::Highlight => 1,
                DataType::Link => 2,
                DataType::Plot => 3,
                DataType::Text => 4,
                DataType::Tile => 5,
                DataType::Connector => 6,
            }
        }
        assert_eq!(classify(DataType::Highlight), 1);
        assert_eq!(classify(DataType::Connector), 6);
    }

    #[test]
    fn test_datum_start_end_negative_values_preserved() {
        // Datum may hold negative start/end (used for unresolved offsets).
        let d = Datum {
            chr: "x".into(),
            start: -5,
            end: -1,
            ..Default::default()
        };
        assert_eq!(d.start, -5);
        assert_eq!(d.end, -1);
    }

    #[test]
    fn test_link_points_empty_vec_initially_allowed() {
        // Link may be constructed with empty points vec (grow-as-you-go).
        let l = Link { id: "L1".into(), points: vec![], param: HashMap::new() };
        assert_eq!(l.id, "L1");
        assert!(l.points.is_empty());
    }

    #[test]
    fn test_dataset_name_unicode_preserved() {
        // DataSet.name passes through unicode unchanged.
        let ds = DataSet {
            name: "highlights-ñoño".into(),
            data_type: DataType::Highlight,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.name, "highlights-ñoño");
    }

    #[test]
    fn test_datum_clone_preserves_param_hashmap_entries() {
        // Clone deep-copies param HashMap so originals independent.
        let mut d = Datum::default();
        d.param.insert("k".into(), "v".into());
        let c = d.clone();
        assert_eq!(c.param.get("k"), Some(&"v".into()));
        assert_eq!(d.param.len(), 1);
    }

    #[test]
    fn test_datum_value_f64_stored_with_high_precision() {
        // f64 preserves precision like 3.141592653589793.
        let d = Datum {
            value: Some(std::f64::consts::PI),
            ..Default::default()
        };
        assert_eq!(d.value.unwrap(), std::f64::consts::PI);
    }

    #[test]
    fn test_dataset_links_field_growable_after_init() {
        // Push links after creation.
        let mut ds = DataSet {
            name: "n".into(),
            data_type: DataType::Link,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        ds.links.push(Link {
            id: "x".into(),
            points: vec![],
            param: HashMap::new(),
        });
        assert_eq!(ds.links.len(), 1);
    }

    #[test]
    fn test_link_clone_preserves_id_and_points_count() {
        // Link Clone deep-copies points and param.
        let l = Link {
            id: "L".into(),
            points: vec![Datum::default(), Datum::default()],
            param: HashMap::new(),
        };
        let c = l.clone();
        assert_eq!(c.id, "L");
        assert_eq!(c.points.len(), 2);
    }

    #[test]
    fn test_datatype_debug_format_strings_differ_per_variant() {
        // Debug format for each variant produces distinct string.
        let h = format!("{:?}", DataType::Highlight);
        let l = format!("{:?}", DataType::Link);
        let p = format!("{:?}", DataType::Plot);
        assert_ne!(h, l);
        assert_ne!(l, p);
        assert_ne!(h, p);
    }

    #[test]
    fn test_datum_set_field_independently_constructed() {
        // Custom IntSpan stored in set field.
        let d = Datum {
            set: IntSpan::from_runlist("1-10"),
            ..Default::default()
        };
        assert_eq!(d.set.cardinality(), 10);
    }

    #[test]
    fn test_dataset_default_via_constructor_empty_state() {
        // Construct DataSet manually; data and links vecs empty.
        let ds = DataSet {
            name: "test".into(),
            data_type: DataType::Plot,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        assert!(ds.data.is_empty());
        assert!(ds.links.is_empty());
        assert_eq!(ds.name, "test");
    }

    #[test]
    fn test_link_param_kv_pairs_added_after_init() {
        // param HashMap grows after Link created with empty map.
        let mut l = Link {
            id: "l1".into(),
            points: vec![],
            param: HashMap::new(),
        };
        l.param.insert("color".into(), "blue".into());
        l.param.insert("z".into(), "5".into());
        assert_eq!(l.param.len(), 2);
    }

    #[test]
    fn test_datatype_text_and_tile_variants_copy_semantics() {
        // Copy semantics: DataType values reusable without move.
        let a = DataType::Text;
        let b = a;
        assert_eq!(a, b);
        let c = DataType::Tile;
        let d = c;
        assert_eq!(c, d);
        assert_ne!(a, c);
    }

    #[test]
    fn test_datum_with_set_intspan_multi_interval_cardinality() {
        // Multi-interval IntSpan in set field preserves cardinality.
        let d = Datum {
            set: IntSpan::from_runlist("1-5,10-15"),
            ..Default::default()
        };
        assert_eq!(d.set.cardinality(), 11);
    }

    #[test]
    fn test_dataset_clone_deep_copies_data_vec() {
        // DataSet Clone deep-copies the data vec.
        let ds = DataSet {
            name: "n".into(),
            data_type: DataType::Highlight,
            data: vec![Datum::default(), Datum::default()],
            links: vec![],
            param: HashMap::new(),
        };
        let c = ds.clone();
        assert_eq!(c.data.len(), 2);
        assert_eq!(ds.data.len(), 2);
    }

    #[test]
    fn test_link_with_two_points_accessible_by_index() {
        // Two-point link with distinct chrs indexed 0 and 1.
        let l = Link {
            id: "link1".into(),
            points: vec![
                Datum { chr: "chr1".into(), ..Default::default() },
                Datum { chr: "chr2".into(), ..Default::default() },
            ],
            param: HashMap::new(),
        };
        assert_eq!(l.points[0].chr, "chr1");
        assert_eq!(l.points[1].chr, "chr2");
    }

    #[test]
    fn test_datatype_connector_distinct_from_others() {
        // Connector variant != any other DataType.
        let c = DataType::Connector;
        assert_ne!(c, DataType::Highlight);
        assert_ne!(c, DataType::Link);
        assert_ne!(c, DataType::Plot);
        assert_ne!(c, DataType::Text);
        assert_ne!(c, DataType::Tile);
    }

    #[test]
    fn test_datum_default_value_field_is_none() {
        // Default Datum has value=None (type-safe Option).
        let d = Datum::default();
        assert!(d.value.is_none());
    }

    #[test]
    fn test_link_with_three_points_all_accessible() {
        // Multi-point Link with 3 distinct chrs indexable 0..3.
        let l = Link {
            id: "L3".into(),
            points: vec![
                Datum { chr: "x".into(), ..Default::default() },
                Datum { chr: "y".into(), ..Default::default() },
                Datum { chr: "z".into(), ..Default::default() },
            ],
            param: HashMap::new(),
        };
        assert_eq!(l.points.len(), 3);
        assert_eq!(l.points[2].chr, "z");
    }

    #[test]
    fn test_datum_chr_empty_string_valid_default() {
        // Default chr is empty string.
        let d = Datum::default();
        assert_eq!(d.chr, "");
    }

    #[test]
    fn test_dataset_data_type_equality_via_field_access() {
        // DataSet.data_type readable.
        let ds = DataSet {
            name: "n".into(),
            data_type: DataType::Tile,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.data_type, DataType::Tile);
    }

    #[test]
    fn test_datum_all_fields_independently_set() {
        // Each field can be set to a distinct value.
        let d = Datum {
            chr: "chrX".into(),
            start: 100,
            end: 200,
            set: IntSpan::from_range(100, 200),
            id: Some("id1".into()),
            value: Some(3.14),
            label: Some("lbl".into()),
            param: HashMap::new(),
        };
        assert_eq!(d.chr, "chrX");
        assert_eq!(d.start, 100);
        assert_eq!(d.end, 200);
        assert_eq!(d.id, Some("id1".to_string()));
        assert_eq!(d.value, Some(3.14));
        assert_eq!(d.label, Some("lbl".to_string()));
    }

    #[test]
    fn test_link_empty_param_map_initially() {
        // Freshly constructed Link has empty param map.
        let l = Link { id: "L".into(), points: vec![], param: HashMap::new() };
        assert!(l.param.is_empty());
    }

    #[test]
    fn test_datum_set_intspan_can_be_extended_after_construction() {
        // IntSpan in set field is mutable.
        let mut d = Datum::default();
        d.set.insert(5);
        d.set.insert(10);
        assert_eq!(d.set.cardinality(), 2);
    }

    #[test]
    fn test_datatype_all_six_variants_distinct_pairwise() {
        // All 6 DataType variants distinct from each other.
        let variants = [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ];
        for i in 0..variants.len() {
            for j in i + 1..variants.len() {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }

    #[test]
    fn test_dataset_name_with_special_chars_preserved() {
        // Special chars in name (including slashes, equals) preserved.
        let ds = DataSet {
            name: "ds-1/v2=final".into(),
            data_type: DataType::Plot,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.name, "ds-1/v2=final");
    }

    #[test]
    fn test_datum_i64_max_boundary_values_preserved() {
        // Datum with i64::MAX start/end preserved.
        let d = Datum {
            start: i64::MAX,
            end: i64::MAX,
            ..Default::default()
        };
        assert_eq!(d.start, i64::MAX);
        assert_eq!(d.end, i64::MAX);
    }

    #[test]
    fn test_link_id_with_only_digits_stored_as_string() {
        // Numeric-looking id stored as String.
        let l = Link {
            id: "12345".into(),
            points: vec![],
            param: HashMap::new(),
        };
        assert_eq!(l.id, "12345");
    }

    #[test]
    fn test_datum_clone_preserves_value_option_some_or_none() {
        // Clone copies value field state.
        let d_none = Datum { value: None, ..Default::default() };
        let d_some = Datum { value: Some(42.0), ..Default::default() };
        let c_none = d_none.clone();
        let c_some = d_some.clone();
        assert!(c_none.value.is_none());
        assert_eq!(c_some.value, Some(42.0));
    }

    #[test]
    fn test_datum_id_option_some_and_none_distinguished() {
        // Some(val) and None distinguishable in id field.
        let d1 = Datum { id: Some("link1".into()), ..Default::default() };
        let d2 = Datum { id: None, ..Default::default() };
        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn test_link_clone_with_points_and_params_preserves_both() {
        // Full Link Clone deep-copies points AND param.
        let mut param = HashMap::new();
        param.insert("color".into(), "blue".into());
        let l = Link {
            id: "l1".into(),
            points: vec![
                Datum { chr: "chr1".into(), ..Default::default() },
                Datum { chr: "chr2".into(), ..Default::default() },
            ],
            param,
        };
        let c = l.clone();
        assert_eq!(c.points.len(), 2);
        assert_eq!(c.param.get("color"), Some(&"blue".into()));
    }

    #[test]
    fn test_dataset_param_hashmap_grows_after_init() {
        // DataSet.param mutable — entries added post-construction.
        let mut ds = DataSet {
            name: "test".into(),
            data_type: DataType::Plot,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        ds.param.insert("orientation".into(), "in".into());
        ds.param.insert("thickness".into(), "5".into());
        assert_eq!(ds.param.len(), 2);
    }

    #[test]
    fn test_datatype_highlight_plot_not_equal() {
        // Highlight and Plot distinct variants.
        assert_ne!(DataType::Highlight, DataType::Plot);
    }

    #[test]
    fn test_datum_value_f64_positive_infinity_stored() {
        // Infinity stored in value.
        let d = Datum {
            value: Some(f64::INFINITY),
            ..Default::default()
        };
        assert_eq!(d.value, Some(f64::INFINITY));
    }

    #[test]
    fn test_link_id_with_unicode_chars_preserved() {
        // Unicode id preserved.
        let l = Link {
            id: "リンク".into(),
            points: vec![],
            param: HashMap::new(),
        };
        assert_eq!(l.id, "リンク");
    }

    #[test]
    fn test_dataset_with_link_data_type_and_links_populated() {
        // DataSet with DataType::Link + non-empty links vec.
        let ds = DataSet {
            name: "L".into(),
            data_type: DataType::Link,
            data: vec![],
            links: vec![Link {
                id: "link1".into(),
                points: vec![Datum::default(); 2],
                param: HashMap::new(),
            }],
            param: HashMap::new(),
        };
        assert_eq!(ds.data_type, DataType::Link);
        assert_eq!(ds.links.len(), 1);
        assert_eq!(ds.links[0].points.len(), 2);
    }

    #[test]
    fn test_datum_param_empty_string_key_and_value() {
        // Empty-string key and value in param.
        let mut d = Datum::default();
        d.param.insert("".into(), "".into());
        assert_eq!(d.param.get(""), Some(&"".to_string()));
    }

    #[test]
    fn test_datum_value_f64_negative_infinity_stored() {
        // NEG_INFINITY stored in value.
        let d = Datum {
            value: Some(f64::NEG_INFINITY),
            ..Default::default()
        };
        assert_eq!(d.value, Some(f64::NEG_INFINITY));
    }

    #[test]
    fn test_link_param_with_unicode_value_preserved() {
        // Unicode value in param preserved.
        let mut l = Link {
            id: "L".into(),
            points: vec![],
            param: HashMap::new(),
        };
        l.param.insert("label".into(), "日本語".into());
        assert_eq!(l.param.get("label"), Some(&"日本語".to_string()));
    }

    #[test]
    fn test_dataset_param_with_mixed_keys_retrievable() {
        // DataSet.param with unicode + ascii keys.
        let mut ds = DataSet {
            name: "ds".into(),
            data_type: DataType::Text,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        ds.param.insert("color".into(), "red".into());
        ds.param.insert("名前".into(), "x".into());
        assert_eq!(ds.param.get("color"), Some(&"red".to_string()));
        assert_eq!(ds.param.get("名前"), Some(&"x".to_string()));
    }

    #[test]
    fn test_datum_clone_with_label_some_and_none_both_preserved() {
        // Label Some/None preserved in clone.
        let d_lbl = Datum { label: Some("hello".into()), ..Default::default() };
        let d_none = Datum { label: None, ..Default::default() };
        assert_eq!(d_lbl.clone().label, Some("hello".to_string()));
        assert_eq!(d_none.clone().label, None);
    }

    #[test]
    fn test_datum_default_set_is_empty_intspan() {
        // Default Datum has empty IntSpan in set.
        let d = Datum::default();
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_link_clone_preserves_id_string_exactly() {
        // Clone preserves id bytes exactly.
        let l = Link {
            id: "specific-id-with-dashes".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let c = l.clone();
        assert_eq!(c.id, l.id);
    }

    #[test]
    fn test_dataset_clone_preserves_links_count() {
        // DataSet clone preserves links vec length.
        let ds = DataSet {
            name: "test".into(),
            data_type: DataType::Link,
            data: vec![],
            links: vec![
                Link { id: "l1".into(), points: vec![], param: HashMap::new() },
                Link { id: "l2".into(), points: vec![], param: HashMap::new() },
            ],
            param: HashMap::new(),
        };
        let c = ds.clone();
        assert_eq!(c.links.len(), 2);
    }

    #[test]
    fn test_datum_with_all_option_fields_some_populated() {
        // Datum with id, value, label all Some.
        let d = Datum {
            id: Some("id1".into()),
            value: Some(42.0),
            label: Some("lbl1".into()),
            ..Default::default()
        };
        assert!(d.id.is_some());
        assert!(d.value.is_some());
        assert!(d.label.is_some());
    }

    #[test]
    fn test_datatype_exhaustive_variants_distinct() {
        // All 6 DataType variants pairwise distinct.
        let variants = [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ];
        for i in 0..variants.len() {
            for j in (i + 1)..variants.len() {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }

    #[test]
    fn test_link_clone_produces_independent_param_map() {
        // Cloning a Link gives a fresh param map.
        let mut l = Link {
            id: "l1".into(),
            points: vec![],
            param: HashMap::new(),
        };
        l.param.insert("k".into(), "v1".into());
        let mut l2 = l.clone();
        l2.param.insert("k".into(), "v2".into());
        assert_eq!(l.param.get("k").map(String::as_str), Some("v1"));
        assert_eq!(l2.param.get("k").map(String::as_str), Some("v2"));
    }

    #[test]
    fn test_dataset_with_empty_links_and_data_valid() {
        // DataSet with empty data/links/param is valid and preserves name/type.
        let ds = DataSet {
            name: "empty".into(),
            data_type: DataType::Plot,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.name, "empty");
        assert_eq!(ds.data_type, DataType::Plot);
        assert!(ds.data.is_empty());
        assert!(ds.links.is_empty());
    }

    #[test]
    fn test_datum_negative_start_end_preserved() {
        // Negative coordinates preserved (i64 is signed).
        let d = Datum {
            chr: "x".into(),
            start: -1000,
            end: -500,
            ..Default::default()
        };
        assert_eq!(d.start, -1000);
        assert_eq!(d.end, -500);
    }

    #[test]
    fn test_datum_param_insert_after_default_is_mutable() {
        // Default Datum's param is mutable; insert after creation works.
        let mut d = Datum::default();
        d.param.insert("k1".into(), "v1".into());
        d.param.insert("k2".into(), "v2".into());
        assert_eq!(d.param.len(), 2);
        assert_eq!(d.param.get("k1").map(String::as_str), Some("v1"));
    }

    #[test]
    fn test_link_with_empty_points_list_preserves_id() {
        // Link with no points still stores id/param.
        let mut param = HashMap::new();
        param.insert("color".into(), "green".into());
        let l = Link { id: "l42".into(), points: vec![], param };
        assert_eq!(l.id, "l42");
        assert!(l.points.is_empty());
        assert_eq!(l.param.get("color").map(String::as_str), Some("green"));
    }

    #[test]
    fn test_dataset_with_connector_type_preserves_type_through_clone() {
        // DataType::Connector survives clone.
        let ds = DataSet {
            name: "conn".into(),
            data_type: DataType::Connector,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        let ds2 = ds.clone();
        assert_eq!(ds2.data_type, DataType::Connector);
        assert_eq!(ds.name, ds2.name);
    }

    #[test]
    fn test_datum_intspan_set_independent_from_clone() {
        // IntSpan is owned: clone → mutating clone's set doesn't touch original.
        let mut d1 = Datum::default();
        d1.set = IntSpan::from_runlist("1-10");
        let mut d2 = d1.clone();
        d2.set = IntSpan::from_runlist("20-30");
        assert_eq!(d1.set.cardinality(), 10);
        assert_eq!(d2.set.cardinality(), 11);
    }

    #[test]
    fn test_datum_value_some_positive_infinity_preserved() {
        // f64::INFINITY preserved as-is.
        let d = Datum { value: Some(f64::INFINITY), ..Default::default() };
        assert_eq!(d.value, Some(f64::INFINITY));
    }

    #[test]
    fn test_datatype_debug_format_nonempty() {
        // All 6 variants produce non-empty Debug representations.
        for dt in [
            DataType::Highlight,
            DataType::Link,
            DataType::Plot,
            DataType::Text,
            DataType::Tile,
            DataType::Connector,
        ] {
            let s = format!("{:?}", dt);
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn test_link_debug_format_contains_id() {
        // Debug repr includes the id field.
        let l = Link { id: "linkABC".into(), points: vec![], param: HashMap::new() };
        let s = format!("{:?}", l);
        assert!(s.contains("linkABC"));
    }

    #[test]
    fn test_dataset_param_preserves_multi_key_configuration() {
        // DataSet.param is a plain HashMap; multiple keys all retrievable.
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("color".into(), "red".into());
        param.insert("thickness".into(), "2".into());
        param.insert("z_index".into(), "5".into());
        let ds = DataSet {
            name: "x".into(),
            data_type: DataType::Highlight,
            data: vec![],
            links: vec![],
            param,
        };
        assert_eq!(ds.param.len(), 3);
        assert_eq!(ds.param.get("thickness").map(String::as_str), Some("2"));
    }

    #[test]
    fn test_datum_chr_with_unicode_characters_preserved() {
        // Unicode chr name preserved round-trip.
        let d = Datum { chr: "染色体①".into(), ..Default::default() };
        assert_eq!(d.chr, "染色体①");
    }

    #[test]
    fn test_datatype_copy_semantics_no_move() {
        // DataType is Copy — can use same value multiple times after copy.
        let dt = DataType::Link;
        let _a = dt;
        let _b = dt;  // Still available (Copy, not Move).
        assert_eq!(dt, DataType::Link);
    }

    #[test]
    fn test_link_push_points_append_datum() {
        // Link.points supports Vec::push; appends a datum.
        let mut l = Link { id: "l".into(), points: vec![], param: HashMap::new() };
        l.points.push(Datum { chr: "a".into(), ..Default::default() });
        l.points.push(Datum { chr: "b".into(), ..Default::default() });
        assert_eq!(l.points.len(), 2);
        assert_eq!(l.points[0].chr, "a");
        assert_eq!(l.points[1].chr, "b");
    }

    #[test]
    fn test_dataset_data_type_text_with_data_points_preserved() {
        // DataType::Text with some data points preserved.
        let ds = DataSet {
            name: "labels".into(),
            data_type: DataType::Text,
            data: vec![Datum { chr: "hs1".into(), label: Some("L1".into()), ..Default::default() }],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.data_type, DataType::Text);
        assert_eq!(ds.data.len(), 1);
        assert_eq!(ds.data[0].label.as_deref(), Some("L1"));
    }

    #[test]
    fn test_datum_default_set_has_zero_cardinality() {
        // Default IntSpan → 0 elements.
        let d = Datum::default();
        assert_eq!(d.set.cardinality(), 0);
    }

    #[test]
    fn test_datatype_tile_and_link_distinct_in_match_arms() {
        // Tile and Link should map differently under PartialEq.
        let tile = DataType::Tile;
        let link = DataType::Link;
        assert_ne!(tile, link);
        match tile {
            DataType::Tile => {}
            _ => panic!("expected Tile"),
        }
    }

    #[test]
    fn test_link_param_supports_large_keys_and_values() {
        // Large strings stored and retrieved without issue.
        let long_key = "k".repeat(1000);
        let long_val = "v".repeat(1000);
        let mut param = HashMap::new();
        param.insert(long_key.clone(), long_val.clone());
        let l = Link { id: "x".into(), points: vec![], param };
        assert_eq!(l.param.get(&long_key).map(String::as_str), Some(long_val.as_str()));
    }

    #[test]
    fn test_dataset_data_vec_preserves_insertion_order() {
        // Multiple data points — preserved in insertion order.
        let ds = DataSet {
            name: "d".into(),
            data_type: DataType::Plot,
            data: vec![
                Datum { chr: "a".into(), ..Default::default() },
                Datum { chr: "b".into(), ..Default::default() },
                Datum { chr: "c".into(), ..Default::default() },
            ],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.data[0].chr, "a");
        assert_eq!(ds.data[1].chr, "b");
        assert_eq!(ds.data[2].chr, "c");
    }

    #[test]
    fn test_dataset_links_vec_can_hold_multiple_links() {
        // links Vec can hold multiple Link objects.
        let ds = DataSet {
            name: "d".into(),
            data_type: DataType::Link,
            data: vec![],
            links: vec![
                Link { id: "l1".into(), points: vec![], param: HashMap::new() },
                Link { id: "l2".into(), points: vec![], param: HashMap::new() },
            ],
            param: HashMap::new(),
        };
        assert_eq!(ds.links.len(), 2);
        assert_eq!(ds.links[0].id, "l1");
        assert_eq!(ds.links[1].id, "l2");
    }

    #[test]
    fn test_link_points_mutable_after_clone() {
        // Cloned Link's points vec is mutable, independent from original.
        let l1 = Link { id: "l".into(), points: vec![Datum::default()], param: HashMap::new() };
        let mut l2 = l1.clone();
        l2.points.push(Datum::default());
        assert_eq!(l1.points.len(), 1);
        assert_eq!(l2.points.len(), 2);
    }

    #[test]
    fn test_datum_value_comparison_with_nan_partialord() {
        // NaN comparisons yield None — but value: Option<f64> comparisons work for non-NaN.
        let a = Datum { value: Some(1.0), ..Default::default() };
        let b = Datum { value: Some(2.0), ..Default::default() };
        assert!(a.value < b.value);
    }

    #[test]
    fn test_datatype_all_variants_via_format_print() {
        // All DataType variants produce distinct Debug strings.
        let s1 = format!("{:?}", DataType::Highlight);
        let s2 = format!("{:?}", DataType::Tile);
        let s3 = format!("{:?}", DataType::Plot);
        assert_ne!(s1, s2);
        assert_ne!(s2, s3);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_datum_with_i64_max_coordinates_preserved() {
        // i64::MAX coords preserved.
        let d = Datum { chr: "x".into(), start: i64::MAX, end: i64::MAX, ..Default::default() };
        assert_eq!(d.start, i64::MAX);
        assert_eq!(d.end, i64::MAX);
    }

    #[test]
    fn test_datum_id_label_optional_fields_independent() {
        // id and label are independent Option<String>.
        let d1 = Datum { id: Some("id1".into()), label: None, ..Default::default() };
        let d2 = Datum { id: None, label: Some("l1".into()), ..Default::default() };
        assert!(d1.id.is_some() && d1.label.is_none());
        assert!(d2.id.is_none() && d2.label.is_some());
    }

    #[test]
    fn test_link_all_point_chrs_different_preserved() {
        // Link with 3 points on 3 different chrs.
        let l = Link {
            id: "l".into(),
            points: vec![
                Datum { chr: "hs1".into(), ..Default::default() },
                Datum { chr: "hs2".into(), ..Default::default() },
                Datum { chr: "hs3".into(), ..Default::default() },
            ],
            param: HashMap::new(),
        };
        assert_eq!(l.points[0].chr, "hs1");
        assert_eq!(l.points[1].chr, "hs2");
        assert_eq!(l.points[2].chr, "hs3");
    }

    #[test]
    fn test_dataset_name_with_dots_and_underscores_preserved_v2() {
        // name with dots/underscores preserved.
        let ds = DataSet {
            name: "my.ds_name.v2".into(),
            data_type: DataType::Plot,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        assert_eq!(ds.name, "my.ds_name.v2");
    }

    #[test]
    fn test_datum_with_i64_min_coordinates_preserved() {
        // i64::MIN as start/end preserved.
        let d = Datum { chr: "x".into(), start: i64::MIN, end: 0, ..Default::default() };
        assert_eq!(d.start, i64::MIN);
    }

    #[test]
    fn test_datum_value_zero_is_not_none() {
        // Some(0.0) distinct from None.
        let d = Datum { value: Some(0.0), ..Default::default() };
        assert_eq!(d.value, Some(0.0));
        assert!(d.value.is_some());
    }

    #[test]
    fn test_dataset_param_modification_after_clone_independent() {
        // Clone then modify clone — original unchanged.
        let mut ds1 = DataSet {
            name: "d".into(),
            data_type: DataType::Plot,
            data: vec![],
            links: vec![],
            param: HashMap::new(),
        };
        ds1.param.insert("k".into(), "v1".into());
        let mut ds2 = ds1.clone();
        ds2.param.insert("k".into(), "v2".into());
        assert_eq!(ds1.param.get("k").map(String::as_str), Some("v1"));
        assert_eq!(ds2.param.get("k").map(String::as_str), Some("v2"));
    }

    #[test]
    fn test_datum_with_very_long_chr_name_preserved() {
        // Long chr name round-trips.
        let long = "chromosome_".repeat(100);
        let d = Datum { chr: long.clone(), ..Default::default() };
        assert_eq!(d.chr, long);
    }
}
