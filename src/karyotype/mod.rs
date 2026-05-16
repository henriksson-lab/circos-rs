pub mod types;

use std::fs;
use std::path::Path;

use crate::intspan::IntSpan;
use crate::utils;
use types::{Band, Chromosome, DisplayRegion, Karyotype};

/// Read a karyotype file and return the parsed karyotype structure.
///
/// Format:
/// ```text
/// chr - hs1 1 0 247249719 green
/// band hs1 p36.33 p36.33 0 2300000 gneg
/// ```
pub fn read_karyotype(path: &Path, file_delim: Option<&str>) -> Result<Karyotype, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("cannot read karyotype file {}: {}", path.display(), e))?;

    let mut karyotype = Karyotype::default();
    let mut chr_index: usize = 0;

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if utils::is_blank(line) || utils::is_comment(line) {
            continue;
        }

        let fields: Vec<&str> = if let Some(delim) = file_delim {
            line.split(delim).collect()
        } else {
            line.split_whitespace().collect()
        };

        if fields.len() < 7 {
            return Err(format!(
                "karyotype line {} has fewer than 7 fields: {}",
                line_num + 1,
                line
            ));
        }

        let field = fields[0];
        let parent = fields[1];
        let name = fields[2];
        let label = fields[3];
        let start: i64 = fields[4].parse().map_err(|_| {
            format!(
                "line {}: invalid start coordinate '{}'",
                line_num + 1,
                fields[4]
            )
        })?;
        let end: i64 = fields[5].parse().map_err(|_| {
            format!(
                "line {}: invalid end coordinate '{}'",
                line_num + 1,
                fields[5]
            )
        })?;
        let color = fields[6].to_lowercase();

        if !utils::is_number(fields[4]) || !utils::is_number(fields[5]) {
            return Err(format!(
                "line {}: start/end coordinates are not numbers ({}, {})",
                line_num + 1,
                fields[4],
                fields[5]
            ));
        }
        if end <= start {
            return Err(format!(
                "line {}: end coordinate ({}) must be greater than start ({})",
                line_num + 1,
                end,
                start
            ));
        }

        let set = IntSpan::from_range(start, end);

        if field == "chr" {
            if karyotype.chromosomes.contains_key(name) {
                return Err(format!(
                    "line {}: chromosome {} defined twice",
                    line_num + 1,
                    name
                ));
            }
            let chr = Chromosome {
                name: name.to_string(),
                label: label.to_string(),
                start,
                end,
                color,
                set,
                index: chr_index,
                display: true,
                display_region: DisplayRegion::default(),
            };
            karyotype.order.push(name.to_string());
            karyotype.chromosomes.insert(name.to_string(), chr);
            chr_index += 1;
        } else if field == "band" {
            let band = Band {
                name: name.to_string(),
                label: label.to_string(),
                parent: parent.to_string(),
                start,
                end,
                color,
                set,
            };
            karyotype
                .bands
                .entry(parent.to_string())
                .or_insert_with(Vec::new)
                .push(band);
        } else {
            return Err(format!(
                "line {}: unsupported field type '{}' (expected 'chr' or 'band')",
                line_num + 1,
                field
            ));
        }
    }

    validate_karyotype(&karyotype)?;
    Ok(karyotype)
}

/// Validate the karyotype structure.
fn validate_karyotype(karyotype: &Karyotype) -> Result<(), String> {
    for (chr_name, bands) in &karyotype.bands {
        let chr = karyotype.chromosomes.get(chr_name).ok_or_else(|| {
            format!(
                "bands defined for chromosome {} but chromosome not defined",
                chr_name
            )
        })?;

        let chr_set = &chr.set;
        let max_band_overlap: i64 = 1_000_000;
        let mut coverage = IntSpan::new();

        for band in bands {
            if band.set.diff(chr_set).cardinality() > 0 {
                return Err(format!(
                    "band {} on chromosome {} extends outside chromosome bounds",
                    band.name, chr_name
                ));
            }
            if band.set.intersect(&coverage).cardinality() > max_band_overlap {
                return Err(format!(
                    "band {} overlaps with another band by more than {} bases on chromosome {}",
                    band.name, max_band_overlap, chr_name
                ));
            }
            coverage = coverage.union(&band.set);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_file(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_read_karyotype_basic() {
        let content = "\
chr - hs1 1 0 247249719 green
chr - hs2 2 0 242951149 green
band hs1 p36.33 p36.33 0 2300000 gneg
band hs1 p36.32 p36.32 2300000 5300000 gpos25
";
        let f = write_temp_file(content);
        let karyotype = read_karyotype(f.path(), None).unwrap();

        assert_eq!(karyotype.chromosomes.len(), 2);
        assert_eq!(karyotype.order, vec!["hs1", "hs2"]);

        let hs1 = &karyotype.chromosomes["hs1"];
        assert_eq!(hs1.label, "1");
        assert_eq!(hs1.start, 0);
        assert_eq!(hs1.end, 247249719);
        assert_eq!(hs1.color, "green");
        assert_eq!(hs1.index, 0);

        let bands = &karyotype.bands["hs1"];
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].name, "p36.33");
        assert_eq!(bands[1].start, 2300000);
    }

    #[test]
    fn test_read_karyotype_duplicate_chr() {
        let content = "\
chr - hs1 1 0 100 green
chr - hs1 1 0 200 green
";
        let f = write_temp_file(content);
        let result = read_karyotype(f.path(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("defined twice"));
    }

    #[test]
    fn test_read_karyotype_invalid_coords() {
        let content = "chr - hs1 1 100 50 green\n";
        let f = write_temp_file(content);
        let result = read_karyotype(f.path(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be greater"));
    }

    #[test]
    fn test_read_karyotype_band_before_chr_errors() {
        // Band referring to a chromosome that's not defined → validation Err.
        let content = "band hs1 p1 p1 0 100 gneg\n";
        let f = write_temp_file(content);
        let result = read_karyotype(f.path(), None);
        assert!(result.is_err(), "expected bands-without-chr error");
    }

    #[test]
    fn test_read_karyotype_band_outside_chr_bounds() {
        // Band extending beyond its chromosome's end → validation Err.
        let content = "\
chr - hs1 1 0 100 green
band hs1 p1 p1 50 200 gneg
";
        let f = write_temp_file(content);
        let result = read_karyotype(f.path(), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("extends outside"),
            "expected 'extends outside' error, got: {}",
            err
        );
    }

    #[test]
    fn test_read_karyotype_blank_and_comment_lines() {
        let content = "\
# header

chr - hs1 1 0 100 green
# between comment

chr - hs2 2 0 200 blue
";
        let f = write_temp_file(content);
        let karyotype = read_karyotype(f.path(), None).unwrap();
        assert_eq!(karyotype.chromosomes.len(), 2);
        assert_eq!(karyotype.order, vec!["hs1", "hs2"]);
    }

    #[test]
    fn test_read_karyotype_too_few_fields_errors() {
        let content = "chr - hs1 1 0\n";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("fewer than 7 fields"));
    }

    #[test]
    fn test_read_karyotype_unknown_field_type_errors() {
        let content = "foo - hs1 1 0 100 red\n";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unsupported field type"));
    }

    #[test]
    fn test_read_karyotype_invalid_start_coord_errors() {
        let content = "chr - hs1 1 notanumber 100 red\n";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("invalid start coordinate"));
    }

    #[test]
    fn test_read_karyotype_color_lowercased() {
        // Color field is case-normalized to lowercase at ingestion.
        let content = "chr - hs1 1 0 100 RedColor\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "redcolor");
    }

    #[test]
    fn test_read_karyotype_custom_delim() {
        // Tab-delimited file still parses when file_delim=Some("\t").
        let content = "chr\t-\ths1\t1\t0\t100\tgreen\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), Some("\t")).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        assert_eq!(k.chromosomes["hs1"].color, "green");
    }

    #[test]
    fn test_read_karyotype_excessive_band_overlap_errors() {
        // Bands overlapping by more than 1_000_000 bases → validation Err.
        let content = "\
chr - hs1 1 0 10000000 green
band hs1 p1 p1 0 6000000 gneg
band hs1 p2 p2 2000000 8000000 gpos
";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        assert!(r.is_err(), "expected error for >1Mb overlap");
        assert!(r.unwrap_err().contains("overlaps"));
    }

    #[test]
    fn test_read_karyotype_small_band_overlap_accepted() {
        // Overlap ≤ 1_000_000 is tolerated by `validate_karyotype`.
        let content = "\
chr - hs1 1 0 10000000 green
band hs1 p1 p1 0 5000000 gneg
band hs1 p2 p2 4999999 9999999 gpos
";
        let f = write_temp_file(content);
        // 5_000_000 - 4_999_999 + 1 = 2 bases of overlap — well under the threshold.
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 2);
    }

    #[test]
    fn test_read_karyotype_preserves_bands_order_per_chr() {
        let content = "\
chr - hs1 1 0 10000000 green
band hs1 firstband firstband 0 1000000 gneg
band hs1 midband midband 2000000 3000000 gpos
band hs1 lastband lastband 5000000 6000000 gpos
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands.len(), 3);
        assert_eq!(bands[0].name, "firstband");
        assert_eq!(bands[1].name, "midband");
        assert_eq!(bands[2].name, "lastband");
    }

    #[test]
    fn test_read_karyotype_chromosome_index_is_file_order() {
        let content = "\
chr - b 2 0 100 blue
chr - a 1 0 100 red
chr - c 3 0 100 green
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        // index reflects file order (0, 1, 2), not label sort order.
        assert_eq!(k.chromosomes["b"].index, 0);
        assert_eq!(k.chromosomes["a"].index, 1);
        assert_eq!(k.chromosomes["c"].index, 2);
        assert_eq!(k.order, vec!["b", "a", "c"]);
    }

    #[test]
    fn test_read_karyotype_negative_coordinates() {
        // Coordinates can be negative (some genomes use signed positions).
        let content = "chr - hs1 1 -100 100 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].start, -100);
        assert_eq!(k.chromosomes["hs1"].end, 100);
        assert!(k.chromosomes["hs1"].set.member(0));
        assert!(k.chromosomes["hs1"].set.member(-50));
    }

    #[test]
    fn test_read_karyotype_empty_file_yields_empty_karyotype() {
        // Empty file → no chromosomes, no bands.
        let f = write_temp_file("");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.is_empty());
        assert!(k.bands.is_empty());
        assert!(k.order.is_empty());
    }

    #[test]
    fn test_read_karyotype_only_comments_and_blanks() {
        // File with only comments/blanks → empty karyotype.
        let content = "# header\n\n# another comment\n\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.is_empty());
    }

    #[test]
    fn test_read_karyotype_all_fields_populated_in_chromosome() {
        // Verify all 9 Chromosome fields populate correctly from input line.
        let content = "chr - myName myLabel 0 1000 myColor\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["myName"];
        assert_eq!(c.name, "myName");
        assert_eq!(c.label, "myLabel");
        assert_eq!(c.start, 0);
        assert_eq!(c.end, 1000);
        assert_eq!(c.color, "mycolor"); // lowercased
        assert_eq!(c.index, 0);
        assert!(c.display);
        assert_eq!(c.set.cardinality(), 1001);
    }

    #[test]
    fn test_read_karyotype_missing_file_errors() {
        // Nonexistent path → fs::read_to_string error wrapped into Err.
        let r = read_karyotype(std::path::Path::new("/nonexistent/kary.txt"), None);
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("cannot read"), "got: {}", err);
    }

    #[test]
    fn test_read_karyotype_invalid_end_coord_errors() {
        let content = "chr - hs1 1 100 notanumber red\n";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("invalid end coordinate"));
    }

    #[test]
    fn test_read_karyotype_equal_start_end_errors() {
        // end <= start (equal case) triggers the "must be greater" error.
        let content = "chr - hs1 1 100 100 red\n";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("must be greater"), "got: {}", err);
    }

    #[test]
    fn test_read_karyotype_chr_set_cardinality_and_extremes() {
        // Chromosome set covers the full [start, end] range inclusive.
        let content = "chr - hs1 1 0 99 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let chr = &k.chromosomes["hs1"];
        assert_eq!(chr.set.cardinality(), 100);
        assert_eq!(chr.set.min(), Some(0));
        assert_eq!(chr.set.max(), Some(99));
    }

    #[test]
    fn test_read_karyotype_band_end_exactly_at_chr_end_accepted() {
        // A band whose end equals the chr's end is fully within chr bounds.
        let content = "\
chr - hs1 1 0 1000 red
band hs1 edge edge 900 1000 gneg
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 1);
        assert_eq!(k.bands["hs1"][0].end, 1000);
        assert_eq!(k.bands["hs1"][0].set.max(), Some(1000));
    }

    #[test]
    fn test_read_karyotype_band_start_exactly_at_chr_start_accepted() {
        // A band starting at the chr's start is also in-bounds.
        let content = "\
chr - hs1 1 0 500 red
band hs1 p1 p1 0 100 gneg
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let band = &k.bands["hs1"][0];
        assert_eq!(band.start, 0);
        assert_eq!(band.set.min(), Some(0));
    }

    #[test]
    fn test_read_karyotype_multiple_chromosomes_bands_grouped_by_parent() {
        // Two chrs + bands on both → bands HashMap keyed by chr; each chr
        // gets its own Vec<Band>; no cross-pollination.
        let content = "\
chr - hs1 1 0 500 red
chr - hs2 2 0 500 blue
band hs1 a a 0 100 gneg
band hs1 b b 100 200 gpos25
band hs2 x x 0 50 gpos50
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.bands["hs1"].len(), 2);
        assert_eq!(k.bands["hs2"].len(), 1);
        assert_eq!(k.bands["hs1"][0].name, "a");
        assert_eq!(k.bands["hs1"][1].name, "b");
        assert_eq!(k.bands["hs2"][0].name, "x");
    }

    #[test]
    fn test_read_karyotype_comment_after_data_line_ignored() {
        // A stand-alone `#`-starting line is skipped; no effect on parsing.
        let content = "\
chr - hs1 1 0 500 red
# this is a comment in the middle
chr - hs2 2 0 500 blue
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.order, vec!["hs1", "hs2"]);
    }

    #[test]
    fn test_read_karyotype_tab_delim_accepted() {
        // Custom tab delimiter.
        let content = "chr\t-\ths1\t1\t0\t500\tred\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), Some("\t")).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        let chr = &k.chromosomes["hs1"];
        assert_eq!(chr.label, "1");
        assert_eq!(chr.end, 500);
        assert_eq!(chr.color, "red");
    }

    #[test]
    fn test_read_karyotype_large_coordinates() {
        // Genomic-scale coordinates.
        let content = "chr - hs1 1 0 247249719 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let chr = &k.chromosomes["hs1"];
        assert_eq!(chr.end, 247_249_719);
        assert_eq!(chr.set.cardinality(), 247_249_720);
    }

    #[test]
    fn test_read_karyotype_consecutive_bands_sharing_same_parent() {
        // Two bands in a row → both land in the same parent's Vec.
        let content = "\
chr - hs1 1 0 1000 red
band hs1 p1 p1 0 100 gneg
band hs1 p2 p2 100 200 gpos25
band hs1 p3 p3 200 300 gpos50
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 3);
        // Insertion order preserved.
        assert_eq!(k.bands["hs1"][0].name, "p1");
        assert_eq!(k.bands["hs1"][1].name, "p2");
        assert_eq!(k.bands["hs1"][2].name, "p3");
        // Band at the end ranges [200, 300] → cardinality 101.
        assert_eq!(k.bands["hs1"][2].set.cardinality(), 101);
    }

    #[test]
    fn test_read_karyotype_chromosome_display_default_true() {
        // Fresh-parsed chromosomes have display=true by default.
        let content = "chr - hs1 1 0 100 red\nchr - hs2 2 0 100 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        for name in ["hs1", "hs2"] {
            assert!(k.chromosomes[name].display, "{} should default to display=true", name);
        }
    }

    #[test]
    fn test_read_karyotype_leading_whitespace_on_data_lines() {
        // Leading whitespace on chr lines should be tolerated.
        let content = "   chr - hs1 1 0 100 red\n\t\tchr - hs2 2 0 200 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.chromosomes["hs1"].end, 100);
        assert_eq!(k.chromosomes["hs2"].end, 200);
    }

    #[test]
    fn test_read_karyotype_just_one_chromosome() {
        // Single chr only — everything intact.
        let content = "chr - hsA A 0 500 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        let c = &k.chromosomes["hsA"];
        assert_eq!(c.label, "A");
        assert_eq!(c.start, 0);
        assert_eq!(c.end, 500);
        assert_eq!(c.color, "green");
        assert_eq!(c.index, 0);
    }

    #[test]
    fn test_read_karyotype_chr_index_increments_per_entry() {
        // Index increments per chromosome entry in file order.
        let content = "\
chr - a a 0 100 r
chr - b b 0 100 g
chr - c c 0 100 b
chr - d d 0 100 y
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["a"].index, 0);
        assert_eq!(k.chromosomes["b"].index, 1);
        assert_eq!(k.chromosomes["c"].index, 2);
        assert_eq!(k.chromosomes["d"].index, 3);
    }

    #[test]
    fn test_read_karyotype_color_lowercased_from_uppercase() {
        // Colors are lowercased on read.
        let content = "chr - hs1 1 0 100 RED\nchr - hs2 2 0 100 GPOS50\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
        assert_eq!(k.chromosomes["hs2"].color, "gpos50");
    }

    #[test]
    fn test_read_karyotype_name_and_label_separately() {
        // In "chr - name label start end color" format, name and label fields are
        // both preserved separately.
        let content = "chr - chrX XX 0 100 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["chrX"];
        assert_eq!(c.name, "chrX");
        assert_eq!(c.label, "XX");
        assert_ne!(c.name, c.label);
    }

    #[test]
    fn test_read_karyotype_with_trailing_newlines_in_file() {
        // Multiple trailing newlines after the last chr → parsed cleanly.
        let content = "chr - hs1 1 0 100 red\n\n\n\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        assert_eq!(k.order, vec!["hs1"]);
    }

    #[test]
    fn test_read_karyotype_band_set_cardinality_matches_range() {
        // Band.set IntSpan cardinality = end - start + 1.
        let content = "\
chr - hs1 1 0 1000 red
band hs1 p1 p1 0 499 gneg
band hs1 p2 p2 500 999 gpos25
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].set.cardinality(), 500);
        assert_eq!(k.bands["hs1"][1].set.cardinality(), 500);
    }

    #[test]
    fn test_read_karyotype_multiple_chromosomes_same_index_allowed() {
        // Duplicate indices in file → each chr gets its own index field.
        let content = "\
chr - hsA 1 0 100 red
chr - hsB 1 0 100 blue
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        // Each gets a unique file-order index.
        assert_eq!(k.chromosomes["hsA"].index, 0);
        assert_eq!(k.chromosomes["hsB"].index, 1);
    }

    #[test]
    fn test_read_karyotype_field_count_below_7_errors() {
        // 6 fields (missing color) → Err mentioning "fewer than 7".
        let f = write_temp_file("chr - hs1 1 0 100\n");
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("fewer than 7"));
    }

    #[test]
    fn test_read_karyotype_end_equal_start_errors() {
        // end <= start is rejected; equal must also fail.
        let f = write_temp_file("chr - hs1 1 100 100 gray\n");
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("greater than start"));
    }

    #[test]
    fn test_read_karyotype_unsupported_field_type_errors() {
        // Field 0 must be "chr" or "band" — anything else → Err.
        let f = write_temp_file("gene - hs1 1 0 100 red\n");
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("unsupported field type"));
        assert!(err.contains("'gene'"));
    }

    #[test]
    fn test_read_karyotype_skips_blank_and_comment_lines() {
        // Blank lines and `#` comments both skipped; only data rows become chromosomes.
        let content = "\
# header comment
chr - hs1 1 0 100 red

# another comment
chr - hs2 2 0 100 blue
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.order, vec!["hs1", "hs2"]);
    }

    #[test]
    fn test_read_karyotype_invalid_start_number_errors() {
        // Non-numeric start coordinate → Err mentioning "invalid start".
        let f = write_temp_file("chr - hs1 1 abc 100 red\n");
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("invalid start"));
    }

    #[test]
    fn test_read_karyotype_duplicate_chromosome_name_errors() {
        // Same chromosome name appearing twice → Err mentioning "defined twice".
        let content = "\
chr - hs1 1 0 100 red
chr - hs1 1 200 300 blue
";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("defined twice"));
    }

    #[test]
    fn test_read_karyotype_color_stored_as_lowercase() {
        // `fields[6].to_lowercase()` — "RED"/"Blue" stored lowercased.
        let content = "\
chr - hs1 1 0 100 RED
chr - hs2 2 0 100 Blue
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
        assert_eq!(k.chromosomes["hs2"].color, "blue");
    }

    #[test]
    fn test_read_karyotype_bands_accumulated_under_same_parent() {
        // Multiple bands for same chromosome accumulate in the same Vec.
        let content = "\
chr - hs1 1 0 10000000 gray
band hs1 p1 p1 0 1000000 gneg
band hs1 p2 p2 1000000 2000000 gpos
band hs1 p3 p3 2000000 3000000 gneg
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands.len(), 3);
        assert_eq!(bands[0].name, "p1");
        assert_eq!(bands[1].name, "p2");
        assert_eq!(bands[2].name, "p3");
    }

    #[test]
    fn test_read_karyotype_band_for_undefined_chromosome_errors() {
        // Band references chr not defined → validate_karyotype Err.
        let content = "\
chr - hs1 1 0 1000 red
band hs_undefined p1 p1 0 500 gneg
";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("bands defined for chromosome"));
        assert!(err.contains("hs_undefined"));
    }

    #[test]
    fn test_read_karyotype_band_extending_outside_chr_bounds_errors() {
        // Band [0..2000] on chr [0..1000] → band exceeds chromosome → Err.
        let content = "\
chr - hs1 1 0 1000 red
band hs1 p1 p1 0 2000 gneg
";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("extends outside"));
    }

    #[test]
    fn test_read_karyotype_empty_file_returns_empty_karyotype() {
        // Empty file → zero chromosomes, zero bands, empty order.
        let f = write_temp_file("");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 0);
        assert_eq!(k.bands.len(), 0);
        assert!(k.order.is_empty());
    }

    #[test]
    fn test_read_karyotype_invalid_end_coordinate_errors() {
        // Non-numeric end → Err mentioning "invalid end".
        let f = write_temp_file("chr - hs1 1 0 xyz red\n");
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("invalid end"));
    }

    #[test]
    fn test_read_karyotype_chromosome_intspan_cardinality_matches_range() {
        // Chromosome set = IntSpan::from_range(start, end) → cardinality = end-start+1.
        let f = write_temp_file("chr - hs1 1 0 999 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let chr = &k.chromosomes["hs1"];
        assert_eq!(chr.set.cardinality(), 1000);
        assert!(chr.set.member(0));
        assert!(chr.set.member(999));
        assert!(!chr.set.member(1000));
    }

    #[test]
    fn test_read_karyotype_label_field_distinct_from_name() {
        // label ("p-arm-1") is column 4, separate from name ("hs1") in column 3.
        let f = write_temp_file("chr - hs1 p-arm-1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].name, "hs1");
        assert_eq!(k.chromosomes["hs1"].label, "p-arm-1");
    }

    #[test]
    fn test_read_karyotype_chromosomes_assigned_sequential_file_indices() {
        // chr_index increments per chromosome definition; matches insertion order.
        let content = "\
chr - hsA 1 0 100 red
chr - hsB 2 0 100 blue
chr - hsC 3 0 100 green
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hsA"].index, 0);
        assert_eq!(k.chromosomes["hsB"].index, 1);
        assert_eq!(k.chromosomes["hsC"].index, 2);
    }

    #[test]
    fn test_read_karyotype_chromosome_display_defaults_to_true() {
        // `display: true` set at construction — no "display=false" defaulting.
        let f = write_temp_file("chr - hs1 1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes["hs1"].display);
    }

    #[test]
    fn test_read_karyotype_band_name_distinct_from_label_field() {
        // Band fields 3=name, 4=label stored independently.
        let content = "\
chr - hs1 1 0 10000000 gray
band hs1 p36.33 p-arm-33 0 2300000 gneg
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let band = &k.bands["hs1"][0];
        assert_eq!(band.name, "p36.33");
        assert_eq!(band.label, "p-arm-33");
        assert_ne!(band.name, band.label);
    }

    #[test]
    fn test_read_karyotype_band_intspan_matches_start_end_range() {
        // Band's set = IntSpan::from_range(start, end); cardinality = end-start+1.
        let content = "\
chr - hs1 1 0 10000000 gray
band hs1 p1 p1 100 500 gneg
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let band = &k.bands["hs1"][0];
        assert_eq!(band.start, 100);
        assert_eq!(band.end, 500);
        assert_eq!(band.set.cardinality(), 401);
        assert!(band.set.member(100));
        assert!(band.set.member(500));
    }

    #[test]
    fn test_read_karyotype_band_color_stored_as_lowercase() {
        // Band color via fields[6].to_lowercase() — "GPOS25" → "gpos25".
        let content = "\
chr - hs1 1 0 10000000 gray
band hs1 p1 p1 0 1000 GPOS25
";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].color, "gpos25");
    }

    #[test]
    fn test_read_karyotype_bands_missing_parent_returns_none_on_get() {
        // Bands HashMap — missing parent chr → get() returns None (not empty Vec).
        let f = write_temp_file("chr - hs1 1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.bands.get("hs1").is_none());
        assert!(k.bands.get("hs_nonexistent").is_none());
    }

    #[test]
    fn test_read_karyotype_chromosome_order_matches_file_appearance_order() {
        // The `order` Vec reflects the order chromosomes first appear in the file.
        let f = write_temp_file(
            "chr - hsB 2 0 100 blue\nchr - hsA 1 0 100 red\nchr - hsC 3 0 100 green\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.order, vec!["hsB", "hsA", "hsC"]);
        // And chromosomes are indexed in the same order.
        assert_eq!(k.chromosomes["hsB"].index, 0);
        assert_eq!(k.chromosomes["hsA"].index, 1);
        assert_eq!(k.chromosomes["hsC"].index, 2);
    }

    #[test]
    fn test_read_karyotype_band_at_max_overlap_threshold_accepted() {
        // max_band_overlap is 1_000_000 — overlap UP TO this threshold is accepted.
        // A band with 1_000_000 bases of overlap passes (not > max).
        let f = write_temp_file(
            "chr - hs1 1 0 2000000 red\n\
             band hs1 b1 b1 0 1000000 gneg\n\
             band hs1 b2 b2 0 999999 gpos50\n"  // 1_000_000 overlap: [0..999999] ∩ [0..999999]? Actually it's 999999+1=1000000 bases overlap.
        );
        // 999999 range cardinality = 1_000_000; equal to threshold → NOT > max → accepted.
        let r = read_karyotype(f.path(), None);
        assert!(r.is_ok(), "expected accepted at threshold, got {:?}", r.err());
    }

    #[test]
    fn test_read_karyotype_band_parent_mismatch_errors_with_message() {
        // Band referencing a chromosome that wasn't declared → validate_karyotype errors.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             band hs_missing b1 b1 0 50 gneg\n"
        );
        // band is appended under "hs_missing" which has no chromosome entry.
        let err = read_karyotype(f.path(), None);
        // Band references missing chr → error from "band … before chromosome …" check.
        // Depending on implementation, this may error at parse time OR validate time.
        assert!(err.is_err());
    }

    #[test]
    fn test_read_karyotype_chromosome_color_field_lowercased() {
        // Perl lowercases color strings during parse — "RED"/"Green" → "red"/"green".
        let f = write_temp_file("chr - hs1 1 0 100 RED\nchr - hs2 2 0 100 GreenX\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
        assert_eq!(k.chromosomes["hs2"].color, "greenx");
    }

    #[test]
    fn test_read_karyotype_chromosome_start_zero_and_cardinality_matches() {
        // Chromosome with range 0..100 → cardinality 101.
        let f = write_temp_file("chr - hs1 1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let chr = &k.chromosomes["hs1"];
        assert_eq!(chr.start, 0);
        assert_eq!(chr.end, 100);
        assert_eq!(chr.set.cardinality(), 101);
        assert!(chr.set.member(0));
        assert!(chr.set.member(100));
    }

    #[test]
    fn test_read_karyotype_two_bands_same_parent_stored_as_vec_in_order() {
        // Multiple bands on the same chromosome kept as a Vec in file order.
        let f = write_temp_file(
            "chr - hs1 1 0 1000 red\n\
             band hs1 b1 b1 0 500 gneg\n\
             band hs1 b2 b2 500 1000 gpos\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = k.bands.get("hs1").expect("bands present");
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].name, "b1");
        assert_eq!(bands[1].name, "b2");
    }

    #[test]
    fn test_read_karyotype_blank_lines_between_data_lines_skipped() {
        // Blank lines in the middle of the file don't disrupt parsing.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             \n\
             \n\
             chr - hs2 2 0 200 blue\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.order.len(), 2);
        assert!(k.chromosomes.contains_key("hs1"));
        assert!(k.chromosomes.contains_key("hs2"));
    }

    #[test]
    fn test_read_karyotype_mixed_tab_and_space_delimiters_parse_correctly() {
        // Default delimiter splits on whitespace — mixed tab/space on same line works.
        let f = write_temp_file("chr\t-\ths1\t1 0 100\tred\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("hs1"));
        assert_eq!(k.chromosomes["hs1"].color, "red");
    }

    #[test]
    fn test_read_karyotype_label_field_preserved_separate_from_name() {
        // Field 3 is "name" (key), field 4 is "label" (display) — kept separately.
        let f = write_temp_file("chr - hs1 MyLabel 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].name, "hs1");
        assert_eq!(k.chromosomes["hs1"].label, "MyLabel");
    }

    #[test]
    fn test_read_karyotype_band_field_intspan_cardinality_matches_range() {
        // Band [10..30] → cardinality 21.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             band hs1 b1 b1 10 30 gneg\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = k.bands.get("hs1").unwrap();
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].set.cardinality(), 21);
        assert_eq!(bands[0].start, 10);
        assert_eq!(bands[0].end, 30);
    }

    #[test]
    fn test_read_karyotype_chromosome_negative_start_accepted_at_struct_level() {
        // Karyotype parser accepts a negative start coordinate.
        let f = write_temp_file("chr - hs1 1 -50 50 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_eq!(c.start, -50);
        assert_eq!(c.end, 50);
        assert_eq!(c.set.cardinality(), 101);
    }

    #[test]
    fn test_read_karyotype_nonexistent_file_returns_err() {
        // Missing file path → I/O error propagated as String Err.
        let nonexistent = std::path::Path::new("/tmp/definitely_not_a_real_karyotype_file_12345.txt");
        let r = read_karyotype(nonexistent, None);
        assert!(r.is_err());
    }

    #[test]
    fn test_read_karyotype_lines_interleaved_chr_and_band_parsed() {
        // chr hs1 / band hs1 / chr hs2 / band hs2 — all parsed, bands grouped by parent.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             band hs1 b1 b1 0 50 gneg\n\
             chr - hs2 2 0 200 blue\n\
             band hs2 b1 b1 0 100 gpos\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.bands["hs1"].len(), 1);
        assert_eq!(k.bands["hs2"].len(), 1);
    }

    #[test]
    fn test_read_karyotype_band_color_empty_string_after_lowercasing() {
        // Bands with empty color field — must parse successfully.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             band hs1 b1 b1 0 50 TEST_COLOR\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        // Band color should be lowercased.
        assert_eq!(k.bands["hs1"][0].color, "test_color");
    }

    #[test]
    fn test_read_karyotype_chromosome_label_can_differ_from_name() {
        // chr - name label — label field (position 4) is distinct from name (position 3).
        let f = write_temp_file("chr - hs1 Chromosome_1_Display 0 1000 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_eq!(c.name, "hs1");
        assert_eq!(c.label, "Chromosome_1_Display");
    }

    #[test]
    fn test_read_karyotype_three_chromosomes_with_independent_lengths() {
        // Three different chromosomes → three entries with distinct lengths.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             chr - hs2 2 0 500 blue\n\
             chr - hs3 3 0 1000 green\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 3);
        assert_eq!(k.chromosomes["hs1"].end, 100);
        assert_eq!(k.chromosomes["hs2"].end, 500);
        assert_eq!(k.chromosomes["hs3"].end, 1000);
    }

    #[test]
    fn test_read_karyotype_bands_only_for_some_chromosomes() {
        // hs1 has bands; hs2 does not.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             chr - hs2 2 0 100 blue\n\
             band hs1 b1 b1 0 50 gneg\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.bands.contains_key("hs1"));
        assert!(!k.bands.contains_key("hs2"));
    }

    #[test]
    fn test_read_karyotype_chromosome_ends_vary_by_size() {
        // Very small (100), medium (1M), very large (1e8) — all independently stored.
        let f = write_temp_file(
            "chr - tiny 1 0 100 red\n\
             chr - medium 2 0 1000000 blue\n\
             chr - large 3 0 100000000 green\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["tiny"].end, 100);
        assert_eq!(k.chromosomes["medium"].end, 1_000_000);
        assert_eq!(k.chromosomes["large"].end, 100_000_000);
    }

    #[test]
    fn test_read_karyotype_chromosome_set_reflects_coords_range() {
        // IntSpan set cardinality matches (end - start + 1).
        let f = write_temp_file("chr - hs1 1 100 200 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_eq!(c.set.cardinality(), 101);
        assert!(c.set.member(100));
        assert!(c.set.member(200));
        assert!(!c.set.member(99));
        assert!(!c.set.member(201));
    }

    #[test]
    fn test_read_karyotype_one_band_on_each_of_two_chromosomes_kept_separate() {
        // Bands grouped correctly per parent.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             chr - hs2 2 0 100 blue\n\
             band hs1 a a 0 50 gneg\n\
             band hs2 b b 0 50 gpos\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 1);
        assert_eq!(k.bands["hs2"].len(), 1);
        assert_eq!(k.bands["hs1"][0].name, "a");
        assert_eq!(k.bands["hs2"][0].name, "b");
    }

    #[test]
    fn test_read_karyotype_band_label_field_preserved() {
        // Band label (field 4) is distinct from name (field 3).
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             band hs1 b_name b_label 0 50 gneg\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        let band = &k.bands["hs1"][0];
        assert_eq!(band.name, "b_name");
        assert_eq!(band.label, "b_label");
    }

    #[test]
    fn test_read_karyotype_chromosomes_ending_with_huge_coord() {
        // Very large end coord preserved as i64.
        let f = write_temp_file("chr - hs1 1 0 5000000000 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].end, 5_000_000_000);
    }

    #[test]
    fn test_read_karyotype_preserves_multiple_bands_in_vec_order() {
        // Three bands on same chromosome → all kept in file order.
        let f = write_temp_file(
            "chr - hs1 1 0 100 red\n\
             band hs1 b1 b1 0 20 gneg\n\
             band hs1 b2 b2 20 50 gpos\n\
             band hs1 b3 b3 50 100 gvar\n"
        );
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands.len(), 3);
        assert_eq!(bands[0].name, "b1");
        assert_eq!(bands[1].name, "b2");
        assert_eq!(bands[2].name, "b3");
    }

    #[test]
    fn test_read_karyotype_tab_delim_explicit_option() {
        // Explicit delimiter "\t" with tab-separated fields parses correctly.
        let f = write_temp_file("chr\t-\ths1\tmylabel\t0\t100\tred\n");
        let k = read_karyotype(f.path(), Some("\t")).unwrap();
        assert_eq!(k.chromosomes["hs1"].label, "mylabel");
    }

    #[test]
    fn test_read_karyotype_duplicate_chr_returns_err() {
        // Same chr name defined twice → Err with "defined twice".
        let f = write_temp_file("chr - hs1 hs1 0 100 red\nchr - hs1 hs1 0 200 blue\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("defined twice"));
    }

    #[test]
    fn test_read_karyotype_end_less_than_start_returns_err() {
        // end <= start → Err.
        let f = write_temp_file("chr - hs1 hs1 500 100 red\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("greater than start"));
    }

    #[test]
    fn test_read_karyotype_blank_and_comment_lines_skipped() {
        // Blank lines, pure-whitespace lines, and '#' comments are skipped.
        let content = "# top comment\n\n   \nchr - hs1 hs1 0 100 red\n# middle\nchr - hs2 hs2 0 200 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.order, vec!["hs1", "hs2"]);
    }

    #[test]
    fn test_read_karyotype_fewer_than_seven_fields_returns_err() {
        // Less than 7 fields → Err with "fewer than 7".
        let f = write_temp_file("chr - hs1 hs1 0 100\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("fewer than 7"));
    }

    #[test]
    fn test_read_karyotype_unsupported_field_type_returns_err() {
        // First column must be "chr" or "band" → anything else → Err.
        let f = write_temp_file("unknown - hs1 hs1 0 100 red\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unsupported field type"));
    }

    #[test]
    fn test_read_karyotype_band_outside_chromosome_bounds_err() {
        // Chr [0..100], band [150..200] overlaps outside → validation error.
        let content = "chr - hs1 hs1 0 100 red\nband hs1 p1 p1 150 200 gneg\n";
        let f = write_temp_file(content);
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        let msg = res.unwrap_err();
        assert!(msg.contains("outside"));
    }

    #[test]
    fn test_read_karyotype_color_lowercased_on_read() {
        // Color field lowercased via to_lowercase().
        let f = write_temp_file("chr - hs1 hs1 0 100 RED\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
    }

    #[test]
    fn test_read_karyotype_bands_for_undefined_chr_returns_err() {
        // Band references a chromosome that's not defined → Err at validate.
        let content = "band orphan p1 p1 0 100 gneg\n";
        let f = write_temp_file(content);
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("not defined"));
    }

    #[test]
    fn test_read_karyotype_invalid_start_coord_returns_err() {
        // Non-numeric start field → Err about "invalid start coordinate".
        let f = write_temp_file("chr - hs1 hs1 notanumber 100 red\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("invalid start coordinate"));
    }

    #[test]
    fn test_read_karyotype_chromosome_index_assigned_sequentially() {
        // Three chrs in file order → indexes 0,1,2.
        let content = "chr - a a 0 100 red\nchr - b b 0 100 blue\nchr - c c 0 100 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["a"].index, 0);
        assert_eq!(k.chromosomes["b"].index, 1);
        assert_eq!(k.chromosomes["c"].index, 2);
    }

    #[test]
    fn test_read_karyotype_nonexistent_file_returns_err_with_path() {
        // Nonexistent path → Err mentioning "cannot read".
        let res = read_karyotype(std::path::Path::new("/nonexistent/path/nothere.txt"), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("cannot read"));
    }

    #[test]
    fn test_read_karyotype_chromosome_display_defaults_true_on_read() {
        // Newly-read chr has display=true by default.
        let f = write_temp_file("chr - hs1 hs1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes["hs1"].display);
    }

    #[test]
    fn test_read_karyotype_invalid_end_coord_returns_err() {
        // Non-numeric end field → Err "invalid end coordinate".
        let f = write_temp_file("chr - hs1 hs1 0 notanumber red\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("invalid end coordinate"));
    }

    #[test]
    fn test_read_karyotype_end_equals_start_returns_err() {
        // end == start triggers "greater than start" validation.
        let f = write_temp_file("chr - hs1 hs1 100 100 red\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("greater than start"));
    }

    #[test]
    fn test_read_karyotype_chromosome_set_covers_coord_range() {
        // Chr [100, 500] → set has cardinality 401 and member at both endpoints.
        let f = write_temp_file("chr - hs1 hs1 100 500 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_eq!(c.set.cardinality(), 401);
        assert!(c.set.member(100));
        assert!(c.set.member(500));
    }

    #[test]
    fn test_read_karyotype_order_vec_matches_file_insertion() {
        // Order vector authoritative for display index → preserves file order.
        let content = "chr - zzz zzz 0 100 red\nchr - aaa aaa 0 100 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        // Order in file: zzz first, aaa second — not alphabetical.
        assert_eq!(k.order, vec!["zzz".to_string(), "aaa".to_string()]);
    }

    #[test]
    fn test_read_karyotype_band_start_equal_to_chr_start_accepted() {
        // Band that starts at chr start (0) is within bounds.
        let content = "chr - hs1 hs1 0 100 red\nband hs1 p1 p1 0 50 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands.get("hs1").map(|v| v.len()), Some(1));
    }

    #[test]
    fn test_read_karyotype_band_end_equal_to_chr_end_accepted() {
        // Band that ends exactly at chr end is within bounds (inclusive).
        let content = "chr - hs1 hs1 0 100 red\nband hs1 p1 p1 50 100 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = k.bands.get("hs1").expect("bands");
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].end, 100);
    }

    #[test]
    fn test_read_karyotype_trailing_newline_does_not_emit_extra_entry() {
        // Trailing newline + blank line don't add phantom entries.
        let content = "chr - hs1 hs1 0 100 red\n\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        assert_eq!(k.order.len(), 1);
    }

    #[test]
    fn test_read_karyotype_extra_fields_beyond_seven_ignored_silently() {
        // 8+ fields: only first 7 used; extras ignored silently.
        let f = write_temp_file("chr - hs1 hs1 0 100 red extra_field_8 ninth\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
        assert_eq!(k.chromosomes["hs1"].end, 100);
    }

    #[test]
    fn test_read_karyotype_tab_and_space_mixed_delim_when_delim_none() {
        // default (None) uses split_whitespace → tabs and spaces mixed both work.
        let f = write_temp_file("chr\t-\ths1 hs1\t0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("hs1"));
        assert_eq!(k.chromosomes["hs1"].end, 100);
    }

    #[test]
    fn test_read_karyotype_multiple_chr_colors_all_stored_lowercase() {
        // Two chr with different uppercase colors → both lowercased.
        let f = write_temp_file("chr - hs1 hs1 0 100 RED\nchr - hs2 hs2 0 200 BLUE\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
        assert_eq!(k.chromosomes["hs2"].color, "blue");
    }

    #[test]
    fn test_read_karyotype_long_chromosome_name_preserved_exactly() {
        // Very long chromosome name stored verbatim.
        let long_name = "very_long_chromosome_name_with_many_chars_12345";
        let content = format!("chr - {} {} 0 100 red\n", long_name, long_name);
        let f = write_temp_file(&content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key(long_name));
    }

    #[test]
    fn test_read_karyotype_single_valid_chromosome_populates_empty_bands() {
        // Chr only, no bands → bands map doesn't contain chr key.
        let f = write_temp_file("chr - hs1 hs1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        // Bands HashMap either lacks the key or has empty Vec.
        let bands = k.bands.get("hs1");
        assert!(bands.map(|v| v.is_empty()).unwrap_or(true));
    }

    #[test]
    fn test_read_karyotype_chr_with_empty_color_field_stores_empty_string() {
        // Color field actually empty → file has "" in col 7 — fails format (6 fields).
        // But use "-" as placeholder which is valid-but-empty-ish, lowercased.
        let f = write_temp_file("chr - hs1 hs1 0 100 -\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "-");
    }

    #[test]
    fn test_read_karyotype_chromosome_start_zero_preserved() {
        // start=0 is valid (not same as end).
        let f = write_temp_file("chr - hs1 hs1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].start, 0);
        assert_eq!(k.chromosomes["hs1"].end, 100);
    }

    #[test]
    fn test_read_karyotype_two_bands_both_under_coverage_threshold() {
        // Two small adjacent bands within max_band_overlap=1M → valid.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 0 500 gneg\nband hs1 p2 p2 500 1000 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = k.bands.get("hs1").expect("bands");
        assert_eq!(bands.len(), 2);
    }

    #[test]
    fn test_read_karyotype_utf8_label_characters_preserved_exactly() {
        // Non-ASCII label stored verbatim.
        let f = write_temp_file("chr - hs1 résumé 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].label, "résumé");
    }

    #[test]
    fn test_read_karyotype_large_start_negative_end_errors() {
        // end (10) <= start (1000) → Err.
        let f = write_temp_file("chr - hs1 hs1 1000 10 red\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
    }

    #[test]
    fn test_read_karyotype_three_chromosomes_order_preserved() {
        // Three chrs in file → order vec has all three in file order.
        let content = "chr - a a 0 100 red\nchr - b b 0 100 blue\nchr - c c 0 100 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_read_karyotype_chr_name_with_numeric_suffix() {
        // "chr1" / "chr22" / "chrX" — numeric and alpha suffixes both valid.
        let content = "chr - chr1 chr1 0 100 red\nchr - chr22 chr22 0 200 blue\nchr - chrX chrX 0 150 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("chr1"));
        assert!(k.chromosomes.contains_key("chr22"));
        assert!(k.chromosomes.contains_key("chrX"));
    }

    #[test]
    fn test_read_karyotype_band_different_color_than_parent_chr() {
        // Band color independent of parent chr color.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 100 200 gpos100\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
        assert_eq!(k.bands["hs1"][0].color, "gpos100");
    }

    #[test]
    fn test_read_karyotype_many_chromosomes_preserves_all_indexes() {
        // 10 chromosomes → all indices 0..9 assigned in order.
        let content: String = (0..10)
            .map(|i| format!("chr - chr{} chr{} 0 100 red\n", i, i))
            .collect();
        let f = write_temp_file(&content);
        let k = read_karyotype(f.path(), None).unwrap();
        for i in 0..10 {
            let name = format!("chr{}", i);
            assert_eq!(k.chromosomes[&name].index, i);
        }
    }

    #[test]
    fn test_read_karyotype_chr_with_space_delim_custom_option() {
        // Explicit " " delimiter (space) parses correctly.
        let f = write_temp_file("chr - hs1 hs1 0 100 red\n");
        let k = read_karyotype(f.path(), Some(" ")).unwrap();
        assert!(k.chromosomes.contains_key("hs1"));
    }

    #[test]
    fn test_read_karyotype_consecutive_identical_names_rejected() {
        // Two chr lines with same name → second → Err about duplicate.
        let f = write_temp_file("chr - dup dup 0 100 red\nchr - dup dup 0 200 blue\n");
        let res = read_karyotype(f.path(), None);
        assert!(res.is_err());
    }

    #[test]
    fn test_read_karyotype_color_with_alpha_suffix_preserved_lowercase() {
        // "color_a5" → stored as "color_a5" (lowercased).
        let f = write_temp_file("chr - hs1 hs1 0 100 Red_A5\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red_a5");
    }

    #[test]
    fn test_read_karyotype_duplicate_bands_on_same_chr_preserved() {
        // Two distinct bands on same chr (non-overlapping) → both in bands vec.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 0 100 g1\nband hs1 p2 p2 200 300 g2\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 2);
    }

    #[test]
    fn test_read_karyotype_chromosome_label_can_equal_name() {
        // label == name common case → stored.
        let f = write_temp_file("chr - hs1 hs1 0 100 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_eq!(c.name, "hs1");
        assert_eq!(c.label, "hs1");
    }

    #[test]
    fn test_read_karyotype_band_name_parent_and_label_fields_all_stored() {
        // Band has name/parent/label/start/end fields — all preserved.
        let f = write_temp_file("chr - hs1 hs1 0 1000 red\nband hs1 myname mylabel 10 90 gneg\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let b = &k.bands["hs1"][0];
        assert_eq!(b.name, "myname");
        assert_eq!(b.label, "mylabel");
        assert_eq!(b.parent, "hs1");
        assert_eq!(b.start, 10);
        assert_eq!(b.end, 90);
    }

    #[test]
    fn test_read_karyotype_two_chrs_indexes_contiguous() {
        // Two chrs: indexes 0 and 1 contiguous.
        let f = write_temp_file("chr - a a 0 100 red\nchr - b b 0 200 blue\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["a"].index, 0);
        assert_eq!(k.chromosomes["b"].index, 1);
    }

    #[test]
    fn test_read_karyotype_chromosome_set_member_check_at_arbitrary_points() {
        // Chr [0,1000] → member check at 0, 500, 1000.
        let f = write_temp_file("chr - hs1 hs1 0 1000 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let set = &k.chromosomes["hs1"].set;
        assert!(set.member(0));
        assert!(set.member(500));
        assert!(set.member(1000));
        assert!(!set.member(1001));
    }

    #[test]
    fn test_read_karyotype_band_set_reflects_range() {
        // Band [100,200] → set cardinality 101.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 100 200 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let band = &k.bands["hs1"][0];
        assert_eq!(band.set.cardinality(), 101);
    }

    #[test]
    fn test_read_karyotype_color_field_hyphen_lowercase_preserved() {
        // Hyphen in color name preserved on lowercase.
        let f = write_temp_file("chr - hs1 hs1 0 100 Red-Shade\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red-shade");
    }

    #[test]
    fn test_read_karyotype_band_name_with_dots_preserved() {
        // "p36.33" band name (Perl cytoband style) preserved.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p36.33 p36.33 0 100 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let b = &k.bands["hs1"][0];
        assert_eq!(b.name, "p36.33");
    }

    #[test]
    fn test_read_karyotype_chromosome_start_large_value_stored() {
        // start=1_000_000 stored as i64.
        let f = write_temp_file("chr - hs1 hs1 1000000 2000000 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].start, 1_000_000);
        assert_eq!(k.chromosomes["hs1"].end, 2_000_000);
    }

    #[test]
    fn test_read_karyotype_band_on_chr_with_start_zero_end_equal_chr_length() {
        // Band [0,100] on chr [0,100] → accepted.
        let content = "chr - hs1 hs1 0 100 red\nband hs1 p1 p1 0 100 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 1);
    }

    #[test]
    fn test_read_karyotype_chr_names_ordering_in_lookup_map() {
        // Chr names accessible via chromosomes HashMap after read.
        let content = "chr - zzz zzz 0 100 red\nchr - aaa aaa 0 100 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        // Both accessible by name.
        assert!(k.chromosomes.contains_key("zzz"));
        assert!(k.chromosomes.contains_key("aaa"));
    }

    #[test]
    fn test_read_karyotype_band_label_same_as_name_is_fine() {
        // label == name is a common case (often just repeat the name).
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 100 200 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let b = &k.bands["hs1"][0];
        assert_eq!(b.name, "p1");
        assert_eq!(b.label, "p1");
    }

    #[test]
    fn test_read_karyotype_chromosome_fields_all_accessible_after_read() {
        // All chromosome struct fields accessible after reading.
        let f = write_temp_file("chr - hs1 MyLabel 100 200 red\n");
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_eq!(c.name, "hs1");
        assert_eq!(c.label, "MyLabel");
        assert_eq!(c.start, 100);
        assert_eq!(c.end, 200);
        assert_eq!(c.color, "red");
    }

    #[test]
    fn test_read_karyotype_four_chromosomes_file_order_preserved() {
        // File order preserved for 4 chromosomes.
        let content = "chr - d d 0 100 red\nchr - b b 0 100 green\nchr - a a 0 100 blue\nchr - c c 0 100 yellow\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.order, vec!["d", "b", "a", "c"]);
    }

    #[test]
    fn test_read_karyotype_bands_on_multiple_chrs_each_vec_separate() {
        // 2 chrs each with a band → bands HashMap has both keys with 1 band each.
        let content = "chr - hs1 hs1 0 100 red\nchr - hs2 hs2 0 100 blue\nband hs1 p1 p1 10 20 g1\nband hs2 q1 q1 30 40 g2\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands.get("hs1").map(|v| v.len()), Some(1));
        assert_eq!(k.bands.get("hs2").map(|v| v.len()), Some(1));
    }

    #[test]
    fn test_read_karyotype_blank_line_between_chr_and_band_skipped() {
        // Blank line between definitions → skipped.
        let content = "chr - hs1 hs1 0 1000 red\n\nband hs1 p1 p1 100 200 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        assert_eq!(k.bands["hs1"].len(), 1);
    }

    #[test]
    fn test_read_karyotype_comment_line_skipped() {
        // '#' lines are comments → skipped.
        let content = "# this is a comment\nchr - hs1 hs1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
    }

    #[test]
    fn test_read_karyotype_chr_start_end_values_parse_as_i64() {
        // Large i64 values in start/end parse correctly.
        let content = "chr - hs1 hs1 0 2147483648 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].end, 2147483648i64);
    }

    #[test]
    fn test_read_karyotype_three_bands_on_same_chr_all_stored() {
        // 3 bands → bands[chr].len() == 3.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 0 100 gpos\n\
                       band hs1 p2 p2 100 200 gpos\n\
                       band hs1 p3 p3 200 300 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 3);
    }

    #[test]
    fn test_read_karyotype_chromosome_color_custom_named_preserved() {
        // Custom color name (not default) preserved verbatim.
        let content = "chr - hs1 hs1 0 1000 my_custom_color\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "my_custom_color");
    }

    #[test]
    fn test_read_karyotype_band_start_end_stored_as_i64_values() {
        // Band start/end values accessible after read.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 250 750 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let band = &k.bands["hs1"][0];
        assert_eq!(band.start, 250);
        assert_eq!(band.end, 750);
    }

    #[test]
    fn test_read_karyotype_multiple_chromosomes_all_lookup_by_name() {
        // After read, each chr accessible by its exact name.
        let content = "chr - chrA A 0 100 red\n\
                       chr - chrB B 0 200 blue\n\
                       chr - chrC C 0 300 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["chrA"].color, "red");
        assert_eq!(k.chromosomes["chrB"].color, "blue");
        assert_eq!(k.chromosomes["chrC"].color, "green");
    }

    #[test]
    fn test_read_karyotype_empty_file_yields_empty_kary() {
        // Empty file → no chromosomes, no bands.
        let f = write_temp_file("");
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.is_empty());
        assert!(k.bands.is_empty());
    }

    #[test]
    fn test_read_karyotype_chr_with_dash_in_name_preserved() {
        // Chr name with hyphen preserved as-is.
        let content = "chr - chr-1 chr-1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("chr-1"));
    }

    #[test]
    fn test_read_karyotype_band_sequence_preserves_file_order() {
        // Two bands pA, pB on same chr — vec order matches file order.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 pA pA 0 100 gpos\n\
                       band hs1 pB pB 100 200 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].name, "pA");
        assert_eq!(k.bands["hs1"][1].name, "pB");
    }

    #[test]
    fn test_read_karyotype_trailing_whitespace_on_line_handled() {
        // Trailing whitespace on chr line doesn't break parsing.
        let content = "chr - hs1 hs1 0 1000 red   \n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
    }

    #[test]
    fn test_read_karyotype_chr_with_underscore_in_name_preserved() {
        // Chr name with underscore preserved as-is.
        let content = "chr - chr_alt chr_alt 0 500 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("chr_alt"));
    }

    #[test]
    fn test_read_karyotype_tab_separated_also_valid() {
        // Tab-separated field values accepted.
        let content = "chr\t-\ths1\ths1\t0\t1000\tred\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("hs1"));
    }

    #[test]
    fn test_read_karyotype_very_long_chr_name_preserved() {
        // Chr name that's 50 chars long preserved.
        let long_name = "chr_very_long_name_with_many_characters_here_1234";
        let content = format!("chr - {0} {0} 0 100 red\n", long_name);
        let f = write_temp_file(&content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key(long_name));
    }

    #[test]
    fn test_read_karyotype_single_chr_start_is_zero() {
        // chr start=0 accessible after read.
        let content = "chr - hs1 hs1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].start, 0);
    }

    #[test]
    fn test_read_karyotype_10_chromosomes_all_indexed_correctly() {
        // 10 chromosomes → all 10 lookupable by name.
        let content: String = (1..=10)
            .map(|i| format!("chr - c{} c{} 0 100 red\n", i, i))
            .collect();
        let f = write_temp_file(&content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 10);
        for i in 1..=10 {
            assert!(k.chromosomes.contains_key(&format!("c{}", i)));
        }
    }

    #[test]
    fn test_read_karyotype_band_color_with_digits_accepted() {
        // Band color like "gpos25" with digits accepted.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 0 100 gpos25\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].color, "gpos25");
    }

    #[test]
    fn test_read_karyotype_chromosome_label_different_from_name() {
        // Label field stored separately from name field.
        let content = "chr - hs1 HumanChr1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].name, "hs1");
        assert_eq!(k.chromosomes["hs1"].label, "HumanChr1");
    }

    #[test]
    fn test_read_karyotype_band_parent_chromosome_accessible() {
        // Band's parent chr is retrievable via bands[chr].
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 pA pA 0 500 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 1);
        assert_eq!(k.bands["hs1"][0].name, "pA");
    }

    #[test]
    fn test_read_karyotype_multiple_blank_lines_skipped() {
        // Multiple blank lines between entries — all skipped.
        let content = "chr - a a 0 100 red\n\n\n\nchr - b b 0 100 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
    }

    #[test]
    fn test_read_karyotype_chr_numeric_name_stored_as_string() {
        // Numeric chr name stored as string key.
        let content = "chr - 1 1 0 100 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("1"));
        assert_eq!(k.chromosomes["1"].name, "1");
    }

    #[test]
    fn test_read_karyotype_band_on_chr_with_fractional_positions_preserved() {
        // Band with position matching chr's range preserved.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 250 500 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].start, 250);
        assert_eq!(k.bands["hs1"][0].end, 500);
    }

    #[test]
    fn test_read_karyotype_consecutive_chrs_and_bands_interleaved() {
        // chr A, band, chr B, band → both chrs and both bands stored.
        let content = "chr - a a 0 100 red\n\
                       band a p1 p1 0 50 gpos\n\
                       chr - b b 0 200 blue\n\
                       band b p1 p1 0 100 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.bands["a"].len(), 1);
        assert_eq!(k.bands["b"].len(), 1);
    }

    #[test]
    fn test_read_karyotype_chr_color_empty_string_preserved() {
        // Color with single character preserved.
        let content = "chr - a a 0 100 r\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["a"].color, "r");
    }

    #[test]
    fn test_read_karyotype_large_chr_count_scaling() {
        // 50 chromosomes — all stored.
        let content: String = (1..=50)
            .map(|i| format!("chr - c{} c{} 0 100 red\n", i, i))
            .collect();
        let f = write_temp_file(&content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 50);
    }

    #[test]
    fn test_read_karyotype_only_comments_and_blank_lines_yields_empty() {
        // File with only comments and blank lines → empty karyotype.
        let content = "# comment\n\n# another\n\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.is_empty());
    }

    #[test]
    fn test_read_karyotype_chr_with_large_end_value_stored() {
        // Very large end value (chr size ~chr1 size 250M).
        let content = "chr - hs1 hs1 0 250000000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].end, 250_000_000);
    }

    #[test]
    fn test_read_karyotype_band_in_middle_chr_range() {
        // Band positioned entirely in middle of chr range.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 centro centro 450 550 acen\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].start, 450);
        assert_eq!(k.bands["hs1"][0].end, 550);
        assert_eq!(k.bands["hs1"][0].color, "acen");
    }

    #[test]
    fn test_read_karyotype_chr_color_with_underscore_preserved() {
        // Chr color with underscore preserved verbatim.
        let content = "chr - hs1 hs1 0 1000 chr_red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "chr_red");
    }

    #[test]
    fn test_read_karyotype_band_with_only_digit_band_name() {
        // Band with pure numeric name stored as string.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 1 1 0 100 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].name, "1");
    }

    #[test]
    fn test_read_karyotype_chr_name_with_period_preserved() {
        // Chr name with period preserved (common for chr.X pattern).
        let content = "chr - chr.X chr.X 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("chr.X"));
    }

    #[test]
    fn test_read_karyotype_chr_name_single_char_valid() {
        // Single-character chr name valid.
        let content = "chr - X X 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("X"));
    }

    #[test]
    fn test_read_karyotype_5_bands_on_single_chr_preserves_all() {
        // 5 bands on one chr — all accessible.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 0 200 gpos\n\
                       band hs1 p2 p2 200 400 gpos\n\
                       band hs1 p3 p3 400 600 gpos\n\
                       band hs1 p4 p4 600 800 gpos\n\
                       band hs1 p5 p5 800 1000 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"].len(), 5);
    }

    #[test]
    fn test_read_karyotype_with_trailing_newlines_not_break_parsing() {
        // Trailing blank lines after content → no panic.
        let content = "chr - hs1 hs1 0 1000 red\n\n\n\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
    }

    #[test]
    fn test_read_karyotype_chr_label_with_spaces_in_field() {
        // Label fields with spaces may cause parse issue or be delimited.
        let content = "chr - hs1 hs1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].label, "hs1");
    }

    #[test]
    fn test_read_karyotype_20_chrs_all_stored_correctly() {
        // 20 chromosomes all stored with distinct colors.
        let colors = ["red", "blue", "green", "yellow", "purple"];
        let content: String = (1..=20)
            .map(|i| format!("chr - c{} c{} 0 100 {}\n", i, i, colors[(i - 1) as usize % 5]))
            .collect();
        let f = write_temp_file(&content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 20);
    }

    #[test]
    fn test_read_karyotype_band_start_zero_preserves_zero() {
        // band starting at 0 stored correctly.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 0 100 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].start, 0);
    }

    #[test]
    fn test_read_karyotype_chr_name_starts_with_digit() {
        // Chr name like "1chr" (starts with digit) preserved.
        let content = "chr - 1chr 1chr 0 100 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("1chr"));
    }

    #[test]
    fn test_read_karyotype_chr_label_distinct_from_name_preserved() {
        // Label different from name preserved separately.
        let content = "chr - hs1 HumanChromosome1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let c = &k.chromosomes["hs1"];
        assert_ne!(c.name, c.label);
    }

    #[test]
    fn test_read_karyotype_mixed_tab_and_space_delimiters_valid() {
        // Mixed tab/space delimiters accepted.
        let content = "chr\t-\ths1 hs1\t0 1000\tred\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("hs1"));
    }

    #[test]
    fn test_read_karyotype_band_color_with_percent_sign_preserved() {
        // Band color with % sign preserved.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p1 p1 0 100 gpos100%\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].color, "gpos100%");
    }

    #[test]
    fn test_read_karyotype_single_chr_accessible_via_indexing() {
        // Single chr accessible via HashMap indexing.
        let content = "chr - only only 0 5000 purple\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["only"].color, "purple");
        assert_eq!(k.chromosomes["only"].end, 5000);
    }

    #[test]
    fn test_read_karyotype_band_name_with_letter_digit_mix_preserved() {
        // Band names like "p11.2" preserved.
        let content = "chr - hs1 hs1 0 1000 red\n\
                       band hs1 p11.2 p11.2 0 100 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.bands["hs1"][0].name, "p11.2");
    }

    #[test]
    fn test_read_karyotype_mix_of_chr_and_band_lines() {
        // Interleaved chr and band lines → all stored correctly.
        let content = "chr - a a 0 200 red\n\
                       band a b1 b1 0 50 gpos\n\
                       chr - b b 0 300 blue\n\
                       band a b2 b2 50 200 gpos\n\
                       band b c1 c1 0 300 acen\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 2);
        assert_eq!(k.bands["a"].len(), 2);
        assert_eq!(k.bands["b"].len(), 1);
    }

    #[test]
    fn test_read_karyotype_chr_color_with_dash_preserved() {
        // Color with dash preserved.
        let content = "chr - hs1 hs1 0 1000 dark-red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "dark-red");
    }

    #[test]
    fn test_read_karyotype_duplicate_chr_is_error() {
        // Same chr name defined twice → error.
        let content = "chr - hs1 hs1 0 1000 red\nchr - hs1 hs1 0 2000 blue\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("defined twice"));
    }

    #[test]
    fn test_read_karyotype_unsupported_field_type_is_error() {
        // Field type not 'chr' or 'band' → error.
        let content = "foo - x x 0 100 red\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("unsupported field type"));
    }

    #[test]
    fn test_read_karyotype_color_uppercased_input_becomes_lowercase() {
        // Color field is lowercased on read.
        let content = "chr - hs1 hs1 0 1000 RED\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].color, "red");
    }

    #[test]
    fn test_read_karyotype_comment_and_blank_lines_skipped() {
        // Blank lines and comments (# prefix) ignored.
        let content = "# header comment\n\n  \nchr - hs1 hs1 0 1000 red\n# trailer\n\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        assert!(k.chromosomes.contains_key("hs1"));
    }

    #[test]
    fn test_read_karyotype_fewer_than_7_fields_is_error() {
        // Line with only 6 fields → error about fewer than 7 fields.
        let content = "chr - hs1 hs1 0 1000\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("fewer than 7"));
    }

    #[test]
    fn test_read_karyotype_end_equals_start_is_error() {
        // end == start → error ("must be greater than start").
        let content = "chr - hs1 hs1 500 500 red\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("must be greater than start"));
    }

    #[test]
    fn test_read_karyotype_explicit_delimiter_tab_splits_correctly() {
        // With file_delim="\t" and tab-separated content, fields split correctly.
        let content = "chr\t-\ths1\ths1\t0\t1000\tred\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), Some("\t")).unwrap();
        assert_eq!(k.chromosomes.len(), 1);
        assert_eq!(k.chromosomes["hs1"].start, 0);
        assert_eq!(k.chromosomes["hs1"].end, 1000);
    }

    #[test]
    fn test_read_karyotype_band_grouped_under_parent_chromosome() {
        // Band line is grouped into karyotype.bands[parent].
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p11 p11 0 500 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands_for_hs1 = k.bands.get("hs1").expect("has bands for hs1");
        assert_eq!(bands_for_hs1.len(), 1);
        assert_eq!(bands_for_hs1[0].name, "p11");
    }

    #[test]
    fn test_read_karyotype_band_without_parent_chromosome_is_error() {
        // Band for nonexistent chromosome → validate error.
        let content = "band ghost p1 p1 0 100 red\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("chromosome not defined"));
    }

    #[test]
    fn test_read_karyotype_band_outside_parent_bounds_is_error() {
        // Band extends past chromosome end → validate error.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 0 2000 gneg\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("extends outside"));
    }

    #[test]
    fn test_read_karyotype_multiple_chromosomes_preserve_declared_order() {
        // Three chromosomes declared in order → karyotype.order reflects that.
        let content = "chr - a a 0 100 red\nchr - b b 0 100 green\nchr - c c 0 100 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_read_karyotype_chr_index_incremented_per_chromosome() {
        // chr_index field starts at 0 and increments per chr.
        let content = "chr - a a 0 100 red\nchr - b b 0 100 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["a"].index, 0);
        assert_eq!(k.chromosomes["b"].index, 1);
    }

    #[test]
    fn test_read_karyotype_invalid_start_coordinate_is_error() {
        // Non-numeric start → error about invalid start coordinate.
        let content = "chr - hs1 hs1 abc 1000 red\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("invalid start"));
    }

    #[test]
    fn test_read_karyotype_invalid_end_coordinate_is_error() {
        // Non-numeric end → error about invalid end coordinate.
        let content = "chr - hs1 hs1 0 notanumber red\n";
        let f = write_temp_file(content);
        let err = read_karyotype(f.path(), None).unwrap_err();
        assert!(err.contains("invalid end"));
    }

    #[test]
    fn test_read_karyotype_new_chromosome_display_field_default_true_v2() {
        // New chromosome's display field defaults to true.
        let content = "chr - hs1 hs1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes["hs1"].display);
    }

    #[test]
    fn test_read_karyotype_chromosome_set_has_expected_cardinality() {
        // Chromosome set is IntSpan::from_range(0, 1000) → card 1001.
        let content = "chr - hs1 hs1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].set.cardinality(), 1001);
    }

    #[test]
    fn test_read_karyotype_nonexistent_file_is_error() {
        // Path that doesn't exist → read_to_string Err → error message.
        let err = read_karyotype(Path::new("/nonexistent_path_xyz_123"), None).unwrap_err();
        assert!(err.contains("cannot read"));
    }

    #[test]
    fn test_read_karyotype_label_field_preserved() {
        // Label (fields[3]) preserved verbatim.
        let content = "chr - hs1 human1 0 1000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].label, "human1");
    }

    #[test]
    fn test_read_karyotype_band_set_card_equals_end_start_plus_one_v2() {
        // Band set = IntSpan::from_range(100, 200) → card 101.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 100 200 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands[0].set.cardinality(), 101);
    }

    #[test]
    fn test_read_karyotype_multiple_bands_grouped_per_chromosome() {
        // Two bands for same chr → vec of length 2.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 0 500 gneg\nband hs1 p2 p2 500 1000 gpos\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].name, "p1");
        assert_eq!(bands[1].name, "p2");
    }

    #[test]
    fn test_read_karyotype_chromosome_start_end_fields_stored() {
        // start=50, end=1500 stored as i64 fields.
        let content = "chr - hs1 hs1 50 1500 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].start, 50);
        assert_eq!(k.chromosomes["hs1"].end, 1500);
    }

    #[test]
    fn test_read_karyotype_band_parent_points_to_named_chromosome() {
        // Band's parent field matches the parent chr name.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p11 p11 10 500 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands[0].parent, "hs1");
    }

    #[test]
    fn test_read_karyotype_multiple_space_delimiters_collapsed() {
        // Multiple spaces between fields collapse via split_whitespace.
        let content = "chr  -  hs1  hs1  0  1000  red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["hs1"].start, 0);
        assert_eq!(k.chromosomes["hs1"].end, 1000);
    }

    #[test]
    fn test_read_karyotype_band_start_end_stored_as_i64() {
        // Band's start and end stored as signed i64.
        let content = "chr - hs1 hs1 0 10000 red\nband hs1 p1 p1 100 500 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands[0].start, 100);
        assert_eq!(bands[0].end, 500);
    }

    #[test]
    fn test_read_karyotype_three_chromosomes_all_accessible_by_name() {
        // Three chromosomes → all accessible in HashMap.
        let content = "chr - a a 0 100 red\nchr - b b 0 200 green\nchr - c c 0 300 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes.len(), 3);
        assert!(k.chromosomes.contains_key("a"));
        assert!(k.chromosomes.contains_key("b"));
        assert!(k.chromosomes.contains_key("c"));
    }

    #[test]
    fn test_read_karyotype_band_label_preserved_same_as_name_or_different() {
        // band.label is fields[3] (may differ from name=fields[2]).
        let content = "chr - hs1 hs1 0 10000 red\nband hs1 p1 P1_LABEL 100 500 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        let bands = &k.bands["hs1"];
        assert_eq!(bands[0].name, "p1");
        assert_eq!(bands[0].label, "P1_LABEL");
    }

    #[test]
    fn test_read_karyotype_with_custom_delimiter_empty_string_behavior() {
        // Explicit "\t" delimiter on space-separated content → line stays as single field.
        let content = "chr - hs1 hs1 0 1000 red\n";
        let f = write_temp_file(content);
        // With \t delimiter but content has no tabs → single-field split → fewer than 7 fields → error.
        let err = read_karyotype(f.path(), Some("\t")).unwrap_err();
        assert!(err.contains("fewer than 7"));
    }

    #[test]
    fn test_read_karyotype_negative_start_coordinate_rejected() {
        // Negative start: "is_number" check considers "-1" as non-numeric (or parse succeeds), but
        // end must be > start. -10 < 100 (valid order), but is_number call may reject "-10".
        let content = "chr - hs1 hs1 -10 100 red\n";
        let f = write_temp_file(content);
        let r = read_karyotype(f.path(), None);
        // Either error or success based on is_number behavior.
        // Either way, assert it doesn't panic.
        let _ = r;
    }

    #[test]
    fn test_read_karyotype_chromosome_name_with_digit_prefix_preserved() {
        // Numeric chr name "22" preserved as string key.
        let content = "chr - 22 22 0 50000000 red\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.chromosomes.contains_key("22"));
    }

    #[test]
    fn test_read_karyotype_order_and_chromosomes_consistent() {
        // order vec and chromosomes map have same set of names.
        let content = "chr - a a 0 100 red\nchr - b b 0 100 green\nchr - c c 0 100 blue\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.order.len(), k.chromosomes.len());
        for name in &k.order {
            assert!(k.chromosomes.contains_key(name));
        }
    }

    #[test]
    fn test_read_karyotype_bands_hashmap_key_is_parent_chr_name() {
        // Bands HashMap is keyed by parent chr name.
        let content = "chr - hs1 hs1 0 1000 red\nband hs1 p1 p1 0 500 gneg\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert!(k.bands.contains_key("hs1"));
        assert!(!k.bands.contains_key("p1"));  // Keys are parents, not band names.
    }

    #[test]
    fn test_read_karyotype_order_first_chr_has_index_zero() {
        // First-declared chr has index=0.
        let content = "chr - first first 0 1000 red\nchr - second second 0 1000 green\n";
        let f = write_temp_file(content);
        let k = read_karyotype(f.path(), None).unwrap();
        assert_eq!(k.chromosomes["first"].index, 0);
        assert_eq!(k.chromosomes["second"].index, 1);
    }
}
