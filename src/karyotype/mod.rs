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
        let start: i64 = fields[4]
            .parse()
            .map_err(|_| format!("line {}: invalid start coordinate '{}'", line_num + 1, fields[4]))?;
        let end: i64 = fields[5]
            .parse()
            .map_err(|_| format!("line {}: invalid end coordinate '{}'", line_num + 1, fields[5]))?;
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
}
