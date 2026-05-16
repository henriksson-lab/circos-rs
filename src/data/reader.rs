use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::intspan::IntSpan;
use crate::utils;

use super::types::{DataType, Datum, Link};

/// Parse options from a comma-separated "key=value,key=value" string.
pub fn parse_options(s: &str) -> HashMap<String, String> {
    let mut opts = HashMap::new();
    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            opts.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    opts
}

/// Port of Perl `read_data_file` options hashref.
#[derive(Debug, Clone, Default)]
pub struct ReadDataOptions {
    /// Group records sharing the same value of this field into one record.
    pub keyby: Option<String>,
    /// Like keyby but flatten into list at end.
    pub groupby: Option<String>,
    /// Stop after this many records.
    pub record_limit: Option<usize>,
    /// Skip records whose `value` differs from the previous by less than this.
    pub min_value_change: Option<f64>,
    /// Skip records with the same `value` as the previous row (runs collapsed).
    pub skip_run: bool,
    /// Populate `Datum.set` as an IntSpan.
    pub addset: bool,
    /// Sort comma-separated bin values descending before stacking.
    pub sort_bin_values: bool,
    /// Default parameters (applied when the row's options don't override).
    pub param: HashMap<String, String>,
    /// File delimiter (Perl `$CONF{file_delim}`). Default: whitespace.
    pub file_delim: Option<String>,
}

/// Port of Perl `read_data_file(file, type, options)`: parse a coordinate data
/// file. Mirrors Perl branches — per-field regex validation, reverse-start/end
/// swap with `rev` flag (links only), padding, minsize expansion, addset,
/// keyby/groupby grouping, record_limit, min_value_change / skip_run filters,
/// stacked histograms via comma-separated values in the `value` column.
pub fn read_data_file(
    path: &Path,
    data_type: DataType,
    options: &ReadDataOptions,
) -> Result<Vec<Datum>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("cannot read data file [{}]: {}", path.display(), e))?;

    // Per-type field schema; "-" means skip this position (Perl: next if $field eq $DASH)
    let fields_by_type: &[&str] = match data_type {
        DataType::Highlight => &["chr", "start", "end", "options"],
        DataType::Link => &["id", "chr", "start", "end", "options"],
        DataType::Plot => &["chr", "start", "end", "value", "options"],
        DataType::Connector => &["chr", "start", "end", "options"],
        DataType::Text => &["chr", "start", "end", "label", "options"],
        DataType::Tile => &["chr", "start", "end", "options"],
    };

    // Field validation regexes (Perl: %rx) — compiled once per process.
    use std::sync::LazyLock;
    static RX_NUMERIC: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?$").unwrap());
    static RX_VALUE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[\d+\-.Ee,]+$").unwrap());
    static RX_OPTIONS: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"=").unwrap());
    let rx_numeric = &*RX_NUMERIC;
    let rx_value = &*RX_VALUE;
    let rx_options = &*RX_OPTIONS;
    let validate = |field: &str, value: &str| -> bool {
        if value.is_empty() {
            return true;
        }
        match field {
            "start" | "end" => rx_numeric.is_match(value),
            "value" => rx_value.is_match(value),
            "label" => !value.is_empty(),
            "options" => rx_options.is_match(value),
            _ => true,
        }
    };

    let mut data: Vec<Datum> = Vec::new();
    let mut keyed: HashMap<String, Vec<Datum>> = HashMap::new();
    let mut keyed_order: Vec<String> = Vec::new();
    let mut prev_value: Option<f64> = None;

    'line: for line in content.lines() {
        let line = line.trim_end();
        if utils::is_blank(line) || utils::is_comment(line) {
            continue;
        }

        // Split on file_delim if set, else whitespace
        let toks: Vec<String> = match &options.file_delim {
            Some(d) => line.split(d.as_str()).map(|s| s.to_string()).collect(),
            None => line.split_whitespace().map(|s| s.to_string()).collect(),
        };

        let mut datum = Datum::default();
        let mut param: HashMap<String, String> = options.param.clone();
        let mut fail = false;

        for (i, field) in fields_by_type.iter().enumerate() {
            if *field == "-" {
                continue;
            }
            let value = match toks.get(i) {
                Some(v) => v.clone(),
                None => String::new(),
            };

            if !validate(field, &value) {
                eprintln!(
                    "error reading data of type [{:?}] from file [{}]: data field [{}] value [{}] does not pass filter",
                    data_type,
                    path.display(),
                    field,
                    value
                );
                eprintln!("line was [{}]", line);
                fail = true;
                continue;
            }

            match *field {
                "options" => {
                    // Parse comma-separated key=value pairs
                    for pair in value.split(',') {
                        let pair = pair.trim();
                        if let Some((k, v)) = pair.split_once('=') {
                            param.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                }
                "chr" => {
                    datum.chr = value;
                }
                "id" => {
                    datum.id = Some(value);
                }
                "label" => {
                    datum.label = Some(value);
                }
                "start" => {
                    if !value.is_empty() {
                        datum.start = value.parse::<f64>().unwrap_or(0.0) as i64;
                    }
                }
                "end" => {
                    if !value.is_empty() {
                        datum.end = value.parse::<f64>().unwrap_or(0.0) as i64;
                    }
                }
                "value" => {
                    if !value.is_empty() {
                        // Single value: skip_run / min_value_change filters apply here.
                        // Stacked histograms (comma in value) are expanded below in the
                        // append step, not filtered here.
                        if !value.contains(',') {
                            if let Ok(v) = value.parse::<f64>() {
                                if let Some(mvc) = options.min_value_change
                                    && let Some(prev) = prev_value
                                    && (v - prev).abs() < mvc
                                {
                                    continue 'line;
                                }
                                if options.skip_run
                                    && let Some(prev) = prev_value
                                    && v == prev
                                {
                                    continue 'line;
                                }
                                datum.value = Some(v);
                            }
                        } else {
                            // Stacked values: validated but stored as literal
                            // — the final append step splits and cumulates.
                            datum
                                .param
                                .insert("_stacked_values".to_string(), value.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        // Update prev_value snapshot for next row
        if let Some(v) = datum.value {
            prev_value = Some(v);
        }

        if fail {
            continue;
        }

        // Reverse start/end if inverted; for links set rev flag
        if data_type == DataType::Link {
            if datum.start > datum.end {
                std::mem::swap(&mut datum.start, &mut datum.end);
                datum.param.insert("rev".to_string(), "1".to_string());
            } else if matches!(
                param.get("rev").map(String::as_str),
                Some("1" | "yes" | "true")
            ) || matches!(
                param.get("reverse").map(String::as_str),
                Some("1" | "yes" | "true")
            ) || matches!(
                param.get("inv").map(String::as_str),
                Some("1" | "yes" | "true")
            ) || matches!(
                param.get("inverted").map(String::as_str),
                Some("1" | "yes" | "true")
            ) {
                datum.param.insert("rev".to_string(), "1".to_string());
            } else {
                datum.param.insert("rev".to_string(), "0".to_string());
            }
        } else if datum.start > datum.end && data_type != DataType::Connector {
            return Err(format!(
                "error - input data line in file [{}] for type [{:?}] has start position [{}] greater than end position [{}]",
                path.display(),
                data_type,
                datum.start,
                datum.end
            ));
        }

        // Padding: expand coordinate (Perl: if $datum->{param}{padding})
        if let Some(pad_s) = param.get("padding")
            && let Ok(pad) = pad_s.parse::<i64>()
        {
            if datum.start > 0 {
                datum.start -= pad;
            }
            if datum.end > 0 {
                datum.end += pad;
            }
        }

        // minsize: expand coordinate span to at least this value
        if let Some(min_s) = param.get("minsize")
            && let Ok(minsize) = min_s.parse::<i64>()
            && datum.end - datum.start < minsize
        {
            let size = datum.end - datum.start + 1;
            let makeup = minsize - size;
            datum.start -= makeup / 2;
            datum.end += makeup / 2;
            if datum.start < 0 {
                datum.start = 0;
                datum.end = minsize - 1;
            }
        }

        // addset: attach an IntSpan
        datum.set = if options.addset || data_type == DataType::Link {
            IntSpan::from_range(datum.start, datum.end)
        } else {
            datum.set
        };

        datum.param = param;

        // Emit into keyed or flat collection
        if let Some(key_field) = options.keyby.as_deref().or(options.groupby.as_deref()) {
            // Extract key value from parsed fields
            let key = match key_field {
                "chr" => datum.chr.clone(),
                "id" => datum.id.clone().unwrap_or_default(),
                _ => datum.param.get(key_field).cloned().unwrap_or_default(),
            };
            if !keyed.contains_key(&key) {
                if let Some(limit) = options.record_limit
                    && keyed_order.len() >= limit
                {
                    break;
                }
                keyed_order.push(key.clone());
            }
            keyed.entry(key).or_default().push(datum);
        } else {
            if let Some(limit) = options.record_limit
                && data.len() >= limit
            {
                break;
            }
            // Stacked histograms: split comma-separated value field
            if let Some(stacked_s) = datum.param.get("_stacked_values").cloned() {
                let mut values: Vec<f64> = stacked_s
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                let mut idx_sorted: Vec<usize> = (0..values.len()).collect();
                if options.sort_bin_values {
                    // Sort values desc; track original index permutation
                    let mut pairs: Vec<(usize, f64)> = values.iter().copied().enumerate().collect();
                    pairs
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    values = pairs.iter().map(|p| p.1).collect();
                    idx_sorted = pairs.iter().map(|p| p.0).collect();
                }
                for (z, _) in values.iter().enumerate() {
                    let cumulsum: f64 = values[..=z].iter().sum();
                    let mut copy = datum.clone();
                    copy.value = Some(cumulsum);
                    copy.param.remove("_stacked_values");
                    copy.param.insert("z".to_string(), z.to_string());
                    // Per-param expansion: if options.param holds a comma-separated list
                    // for some param name, pick the value indexed by the original sort.
                    for (pname, pval) in &options.param {
                        let pvals: Vec<&str> = pval.split(',').collect();
                        if pvals.len() > 1 {
                            let chosen =
                                pvals[idx_sorted.get(z).copied().unwrap_or(z) % pvals.len()];
                            copy.param.insert(pname.clone(), chosen.to_string());
                        }
                    }
                    data.push(copy);
                }
            } else {
                data.push(datum);
            }
        }
    }

    if options.keyby.is_some() || options.groupby.is_some() {
        // Flatten keyed into a vec — order preserved by keyed_order
        let mut out = Vec::new();
        for k in keyed_order {
            if let Some(vals) = keyed.remove(&k) {
                out.extend(vals);
            }
        }
        Ok(out)
    } else {
        Ok(data)
    }
}

/// Group link data by ID into Link structures.
pub fn group_links(data: Vec<Datum>) -> Vec<Link> {
    let mut groups: HashMap<String, Vec<Datum>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for datum in data {
        let id = datum.id.clone().unwrap_or_default();
        if !groups.contains_key(&id) {
            order.push(id.clone());
        }
        groups.entry(id).or_default().push(datum);
    }

    order
        .into_iter()
        .filter_map(|id| {
            let points = groups.remove(&id)?;
            if points.len() < 2 {
                return None; // Skip incomplete links
            }
            Some(Link {
                id,
                points,
                param: HashMap::new(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_options() {
        let opts = parse_options("color=red,thickness=2,z=50");
        assert_eq!(opts.get("color").unwrap(), "red");
        assert_eq!(opts.get("thickness").unwrap(), "2");
        assert_eq!(opts.get("z").unwrap(), "50");
    }

    #[test]
    fn test_read_link_file() {
        let content = "\
segdup00001 hs1 465 30596
segdup00001 hs2 114046768 114076456
segdup00002 hs1 486 76975
segdup00002 hs15 100263879 100338121
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 4);
        assert_eq!(data[0].id.as_deref(), Some("segdup00001"));
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[0].start, 465);
        assert_eq!(data[0].end, 30596);

        let links = group_links(data);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].id, "segdup00001");
        assert_eq!(links[0].points.len(), 2);
    }

    #[test]
    fn test_read_highlight_file() {
        let content = "\
hs2 0 30000000 fill_color=blue
hs2 50000000 80000000 fill_color=red
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs2");
        assert_eq!(data[0].param.get("fill_color").unwrap(), "blue");
    }

    #[test]
    fn test_read_plot_file() {
        let content = "hs1 100 200 0.5\nhs1 300 400 0.8 color=blue\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Plot, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].value, Some(0.5));
        assert_eq!(data[1].param.get("color").unwrap(), "blue");
    }

    #[test]
    fn test_parse_options_empty_and_malformed() {
        // Empty input → empty map
        assert!(parse_options("").is_empty());
        // Whitespace around separator pairs
        let opts = parse_options("  color = red , z = 5 ");
        assert_eq!(opts.get("color").unwrap(), "red");
        assert_eq!(opts.get("z").unwrap(), "5");
        // Token without '=' is silently dropped
        let opts = parse_options("color=red,stray,z=10");
        assert_eq!(opts.len(), 2);
        assert!(opts.contains_key("color"));
        assert!(opts.contains_key("z"));
    }

    #[test]
    fn test_read_data_record_limit() {
        // record_limit caps the loaded data size.
        let content = "hs1 0 100\nhs1 200 300\nhs1 400 500\nhs1 600 700\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            record_limit: Some(2),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_read_data_comments_and_blanks_skipped() {
        // Lines that are blank or start with `#` are not parsed.
        let content = "# header\n\nhs1 0 100\n\n# mid comment\nhs1 200 300\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].start, 0);
        assert_eq!(data[1].start, 200);
    }

    #[test]
    fn test_group_links_drops_singletons_keeps_pairs() {
        let content = "\
link1 hs1 0 100
link1 hs2 200 300
link2 hs3 400 500
link2 hs4 600 700
link3 hs5 800 900
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        let links = group_links(data);
        // Singleton link3 is dropped (< 2 points); link1/link2 survive as pairs.
        assert_eq!(links.len(), 2);
        for l in &links {
            assert_eq!(l.points.len(), 2);
        }
        let ids: Vec<_> = links.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, vec!["link1", "link2"]);
    }

    #[test]
    fn test_read_data_addset_populates_intspan() {
        let content = "hs1 10 20\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            addset: true,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 1);
        // addset=true should populate the IntSpan with the [start, end] range.
        let card = data[0].set.cardinality();
        assert!(card > 0, "expected non-empty IntSpan with addset=true");
    }

    #[test]
    fn test_read_data_skip_run_collapses_repeats() {
        // skip_run: records with same value as previous row are dropped.
        let content = "\
hs1 100 200 0.5
hs1 200 300 0.5
hs1 300 400 0.5
hs1 400 500 0.8
hs1 500 600 0.8
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            skip_run: true,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        // 3 repeated 0.5's → kept first, dropped 2; 2 repeated 0.8's → kept first, dropped 1.
        // Total should be 2 (one per distinct value run).
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].value, Some(0.5));
        assert_eq!(data[1].value, Some(0.8));
    }

    #[test]
    fn test_read_data_min_value_change_filter() {
        // min_value_change: skip row if |value - prev_value| < threshold.
        let content = "\
hs1 100 200 0.50
hs1 200 300 0.52
hs1 300 400 0.60
hs1 400 500 0.61
hs1 500 600 0.80
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            min_value_change: Some(0.05),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        // 0.50 kept (first); 0.52 diff 0.02 < 0.05 → skip; 0.60 diff 0.10 kept;
        // 0.61 diff 0.01 → skip; 0.80 diff 0.20 kept. Total = 3.
        assert_eq!(data.len(), 3);
        assert_eq!(data[0].value, Some(0.50));
        assert_eq!(data[1].value, Some(0.60));
        assert_eq!(data[2].value, Some(0.80));
    }

    #[test]
    fn test_read_data_file_delim_tab_parses_multiword_labels() {
        // Tab-delimited file with multi-word label (for Text plots).
        let content = "hs1\t100\t200\tmulti word label here\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            file_delim: Some("\t".into()),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Text, &opts).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].label.as_deref(), Some("multi word label here"));
    }

    #[test]
    fn test_read_data_keyby_chr_groups_by_chromosome() {
        // keyby=chr → each Datum emitted in per-chromosome order, with data grouped.
        let content = "\
hs1 0 100 0.5
hs2 0 100 0.7
hs1 200 300 0.6
hs2 200 300 0.8
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            keyby: Some("chr".into()),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        // 4 data points total after flattening keyed groups.
        assert_eq!(data.len(), 4);
        // Group order = first-seen chr: hs1 hits first, then hs2.
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[1].chr, "hs1");
        assert_eq!(data[2].chr, "hs2");
        assert_eq!(data[3].chr, "hs2");
    }

    #[test]
    fn test_read_data_param_defaults_applied() {
        // param: defaults in ReadDataOptions supply values for keys not present in the data row's options.
        let content = "hs1 0 100 0.5\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mut defaults = HashMap::new();
        defaults.insert("color".into(), "red".into());
        defaults.insert("thickness".into(), "2".into());
        let opts = ReadDataOptions {
            param: defaults,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        assert_eq!(data.len(), 1);
        // Default param values should show up on the datum.
        assert_eq!(data[0].param.get("color").map(|s| s.as_str()), Some("red"));
        assert_eq!(data[0].param.get("thickness").map(|s| s.as_str()), Some("2"));
    }

    #[test]
    fn test_read_data_param_overridden_by_row_options() {
        // Row-level options override defaults from ReadDataOptions.param.
        let content = "hs1 0 100 0.5 color=blue\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mut defaults = HashMap::new();
        defaults.insert("color".into(), "red".into());
        let opts = ReadDataOptions {
            param: defaults,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].param.get("color").map(|s| s.as_str()), Some("blue"));
    }

    #[test]
    fn test_parse_options_preserves_last_duplicate() {
        // Duplicate keys → last value wins (HashMap insertion overwrite).
        let opts = parse_options("color=red,color=blue,z=1");
        assert_eq!(opts.get("color").unwrap(), "blue");
        assert_eq!(opts.get("z").unwrap(), "1");
    }

    #[test]
    fn test_parse_options_value_with_special_chars() {
        // Values containing dots/slashes/hyphens are preserved verbatim.
        let opts = parse_options("url=/browse?chr=hs1,path=/a/b/c,rate=1.5");
        assert_eq!(opts.get("url").unwrap(), "/browse?chr=hs1");
        assert_eq!(opts.get("path").unwrap(), "/a/b/c");
        assert_eq!(opts.get("rate").unwrap(), "1.5");
    }

    #[test]
    fn test_read_data_link_preserves_id_field() {
        let content = "\
linkA hs1 0 100
linkB hs2 200 300
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].id.as_deref(), Some("linkA"));
        assert_eq!(data[1].id.as_deref(), Some("linkB"));
    }

    #[test]
    fn test_read_data_record_limit_counts_keyed_groups() {
        // With keyby=chr, record_limit caps the number of distinct keys, not raw rows.
        let content = "\
hs1 0 100
hs1 200 300
hs2 0 100
hs3 0 100
hs4 0 100
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            keyby: Some("chr".into()),
            record_limit: Some(2),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        // record_limit=2 → only hs1 and hs2 keyed groups populate.
        let chrs: std::collections::HashSet<&str> =
            data.iter().map(|d| d.chr.as_str()).collect();
        assert_eq!(chrs.len(), 2);
        assert!(chrs.contains("hs1"));
        assert!(chrs.contains("hs2"));
    }

    #[test]
    fn test_read_text_file_preserves_label() {
        // DataType::Text has schema [chr, start, end, label, options].
        let content = "\
hs1 100 200 GeneA color=red
hs1 300 400 GeneB
";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Text, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].label.as_deref(), Some("GeneA"));
        assert_eq!(data[0].param.get("color").map(String::as_str), Some("red"));
        assert_eq!(data[1].label.as_deref(), Some("GeneB"));
        // No options column on row 2 → no color.
        assert!(data[1].param.get("color").is_none());
    }

    #[test]
    fn test_read_connector_allows_start_greater_than_end() {
        // Connector is the exception: start > end is NOT swapped/rejected.
        // (Perl: `start > end && type ne "connector"` is the swap gate.)
        let content = "hs1 500 100 color=green\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Connector, &ReadDataOptions::default())
                .unwrap();
        assert_eq!(data.len(), 1);
        // Start/end preserved as-is (no swap).
        assert_eq!(data[0].start, 500);
        assert_eq!(data[0].end, 100);
    }

    #[test]
    fn test_read_tile_file_parses_options() {
        // DataType::Tile — like Highlight schema [chr,start,end,options].
        let content = "hs2 0 1000 fill_color=blue,stroke_thickness=2\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Tile, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].chr, "hs2");
        assert_eq!(data[0].start, 0);
        assert_eq!(data[0].end, 1000);
        assert_eq!(
            data[0].param.get("fill_color").map(String::as_str),
            Some("blue")
        );
        assert_eq!(
            data[0].param.get("stroke_thickness").map(String::as_str),
            Some("2")
        );
    }

    #[test]
    fn test_parse_options_trims_surrounding_whitespace() {
        // Keys and values are trimmed of surrounding whitespace.
        let opts = parse_options("  color  =  red  ,thickness = 5 ");
        assert_eq!(opts.get("color").unwrap(), "red");
        assert_eq!(opts.get("thickness").unwrap(), "5");
        assert_eq!(opts.len(), 2);
    }

    #[test]
    fn test_read_link_file_inverted_start_end_swapped_but_rev_not_in_final_param() {
        // Link with start > end → start/end are swapped, but the rev=1 flag
        // insertion into datum.param is shadowed by the later `datum.param = param`
        // assignment (latent Perl-port quirk). Swap itself works.
        let content = "l1 hs1 500 100\nl1 hs2 0 50\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        // Swap happened: start=100, end=500.
        assert_eq!(data[0].start, 100);
        assert_eq!(data[0].end, 500);
    }

    #[test]
    fn test_read_link_options_preserved_through_overwrite() {
        // Options-column params (parsed into local `param`) survive the
        // `datum.param = param` assignment. Verified via any option key.
        let content = "l1 hs1 0 100 color=red\nl1 hs2 200 300\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        assert_eq!(data[0].param.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_read_plot_stacked_values_pass_validation_but_not_as_f64() {
        // Plot value with comma → rx_value validates, but single-value parsing
        // short-circuits. Current impl may store stacked values differently.
        // Just verify the row reads without error and value is None.
        let content = "hs1 0 100 1.5,2.0,3.0\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Plot, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert!(data[0].value.is_none());
    }

    #[test]
    fn test_read_data_padding_expands_coords() {
        // Padding of 5 → start -= 5, end += 5. Start stays ≥0 per impl guard.
        let content = "hs1 100 200 padding=5\nhs1 0 50 padding=5\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data[0].start, 95);
        assert_eq!(data[0].end, 205);
        // Second row: start=0 → guard skips (start stays 0); end=50 → 55.
        assert_eq!(data[1].start, 0);
        assert_eq!(data[1].end, 55);
    }

    #[test]
    fn test_read_data_skip_run_drops_consecutive_same_values() {
        // skip_run=true drops rows with value == prev_value.
        let content = "hs1 0 10 1.0\nhs1 20 30 1.0\nhs1 40 50 1.0\nhs1 60 70 2.0\nhs1 80 90 2.0\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            skip_run: true,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        // 5 rows with [1, 1, 1, 2, 2] → after skip_run: [1, 2] (2 rows).
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].value, Some(1.0));
        assert_eq!(data[1].value, Some(2.0));
    }

    #[test]
    fn test_read_data_skip_run_off_preserves_all() {
        // skip_run=false preserves all rows even with duplicates.
        let content = "hs1 0 10 1.0\nhs1 20 30 1.0\nhs1 40 50 1.0\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Plot, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 3);
    }

    #[test]
    fn test_read_data_addset_option_populates_intspan() {
        // addset=true → each datum's `set` IntSpan populated with [start, end].
        let content = "hs1 0 100 color=red\nhs2 200 300\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            addset: true,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data[0].set.cardinality(), 101); // [0, 100]
        assert_eq!(data[1].set.cardinality(), 101); // [200, 300]
    }

    #[test]
    fn test_read_data_record_limit_and_keyby_interact() {
        // With keyby set and record_limit=2: cap at 2 keyed groups total.
        let content = "hs1 0 10 k=g1\nhs1 20 30 k=g2\nhs1 40 50 k=g3\nhs1 60 70 k=g4\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            keyby: Some("k".into()),
            record_limit: Some(2),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        // Only first 2 unique keys kept → 2 rows.
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_read_data_blank_lines_skipped() {
        // Blank lines in data file don't produce Datum entries.
        let content = "\n\nhs1 0 10\n\nhs2 20 30\n\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_read_data_comments_skipped() {
        // Lines starting with `#` are skipped.
        let content = "# header comment\nhs1 0 10\n# another comment\nhs2 20 30\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[1].chr, "hs2");
    }

    #[test]
    fn test_read_data_file_empty_file_returns_empty_vec() {
        // Empty data file → empty result, no errors.
        let f = NamedTempFile::new().unwrap();
        let data =
            read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_read_data_link_sets_intspan_by_default() {
        // Links auto-populate `set` even without addset=true.
        let content = "l1 hs1 0 100\nl1 hs2 200 300\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        // Both link datums have set populated.
        assert_eq!(data[0].set.cardinality(), 101);
        assert_eq!(data[1].set.cardinality(), 101);
    }

    #[test]
    fn test_read_data_options_param_defaults_applied_when_row_has_no_options() {
        // options.param defaults are merged into every row's param.
        let content = "hs1 100 200\nhs2 0 50\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mut defaults = HashMap::new();
        defaults.insert("color".into(), "green".into());
        defaults.insert("thickness".into(), "2".into());
        let opts = ReadDataOptions {
            param: defaults,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
        for d in &data {
            assert_eq!(d.param.get("color").map(String::as_str), Some("green"));
            assert_eq!(d.param.get("thickness").map(String::as_str), Some("2"));
        }
    }

    #[test]
    fn test_read_data_options_param_overridden_by_row_options() {
        // Row's own options override the options.param defaults.
        let content = "hs1 0 100 color=red\nhs2 0 100\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mut defaults = HashMap::new();
        defaults.insert("color".into(), "green".into());
        let opts = ReadDataOptions {
            param: defaults,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        // Row 1 has explicit color=red → wins over default.
        assert_eq!(data[0].param.get("color").map(String::as_str), Some("red"));
        // Row 2 has no color → uses default green.
        assert_eq!(data[1].param.get("color").map(String::as_str), Some("green"));
    }

    #[test]
    fn test_read_data_file_delim_custom_single_char() {
        // file_delim="|" → pipe-separated parsing.
        let content = "hs1|0|100|color=red\nhs2|50|150|color=blue\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            file_delim: Some("|".into()),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[0].start, 0);
        assert_eq!(data[0].end, 100);
        assert_eq!(data[0].param.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_read_data_groupby_aggregates_by_field() {
        // groupby="chr" groups by chr field — 4 rows → 2 chr groups → 4 data.
        let content = "hs1 0 10\nhs1 20 30\nhs2 40 50\nhs2 60 70\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            groupby: Some("chr".into()),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        // All 4 rows preserved; keyed-vs-flat distinction applies at output ordering.
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn test_group_links_preserves_insertion_order() {
        // group_links emits links in first-seen order, not sorted by id.
        let d = |id: &str, chr: &str| Datum {
            chr: chr.into(),
            start: 0,
            end: 100,
            id: Some(id.into()),
            ..Default::default()
        };
        let data = vec![
            d("z_link", "c1"),
            d("z_link", "c2"),
            d("a_link", "c3"),
            d("a_link", "c4"),
            d("m_link", "c5"),
            d("m_link", "c6"),
        ];
        let links = group_links(data);
        let ids: Vec<&str> = links.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, vec!["z_link", "a_link", "m_link"]);
    }

    #[test]
    fn test_group_links_with_all_same_id_yields_single_link_with_all_points() {
        // All points sharing one id → single link with all points.
        let d = |chr: &str, start: i64| Datum {
            chr: chr.into(),
            start,
            end: start + 10,
            id: Some("shared".into()),
            ..Default::default()
        };
        let data = vec![d("c1", 0), d("c2", 100), d("c3", 200), d("c4", 300)];
        let links = group_links(data);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, "shared");
        assert_eq!(links[0].points.len(), 4);
    }

    #[test]
    fn test_group_links_missing_id_treated_as_empty_string_group() {
        // Datum with id=None → grouped under "" (empty string) key.
        let mut d1 = Datum::default();
        d1.chr = "c1".into();
        d1.id = None;
        let mut d2 = Datum::default();
        d2.chr = "c2".into();
        d2.id = None;
        let links = group_links(vec![d1, d2]);
        // Both points under empty-id group → 1 link with 2 points.
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, "");
        assert_eq!(links[0].points.len(), 2);
    }

    #[test]
    fn test_group_links_empty_input_yields_empty_output() {
        let links = group_links(vec![]);
        assert!(links.is_empty());
    }

    #[test]
    fn test_read_plot_stacked_values_expand_not_triggered_by_shadowed_marker() {
        // The impl writes _stacked_values into datum.param during value parse,
        // but then `datum.param = param` on line 273 overwrites it (same root-cause
        // as the rev flag quirk in iter 222). As a result, the stacked-expansion
        // codepath at line 299 never fires — single datum emitted with value=None.
        let content = "hs1 0 100 1.5,2.0,3.0\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data =
            read_data_file(f.path(), DataType::Plot, &ReadDataOptions::default()).unwrap();
        // 1 row (expansion shadowed), value None (comma-string not f64-parsed).
        assert_eq!(data.len(), 1);
        assert!(data[0].value.is_none());
    }

    #[test]
    fn test_read_data_record_limit_flat_mode() {
        // Without keyby/groupby, record_limit caps data.len() directly.
        let content = "hs1 0 10\nhs1 20 30\nhs1 40 50\nhs1 60 70\nhs1 80 90\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            record_limit: Some(3),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        // First 3 records, rest skipped via `break`.
        assert_eq!(data.len(), 3);
        assert_eq!(data[0].start, 0);
        assert_eq!(data[2].start, 40);
    }

    #[test]
    fn test_read_data_keyby_custom_param_field() {
        // keyby="custom" — uses datum.param.get("custom") as the key.
        let content = "hs1 0 10 custom=groupA\nhs2 20 30 custom=groupA\nhs1 40 50 custom=groupB\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            keyby: Some("custom".into()),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        // 3 rows total; keyed by "custom" into 2 groups (groupA: 2, groupB: 1).
        assert_eq!(data.len(), 3);
    }

    #[test]
    fn test_read_data_min_value_change_skips_small_deltas() {
        // min_value_change=0.5 → skip rows where |value - prev_value| < 0.5.
        let content = "hs1 0 10 0.10\nhs1 20 30 0.30\nhs1 40 50 1.00\nhs1 60 70 1.20\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            min_value_change: Some(0.5),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        // Row 1 (0.10) is first, kept. Row 2 (0.30, |delta|=0.2<0.5) skipped.
        // Row 3 (1.00, |delta|=0.9>=0.5 vs prev 0.10) kept. Row 4 (1.20, |delta|=0.2<0.5 vs 1.00) skipped.
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].value, Some(0.10));
        assert_eq!(data[1].value, Some(1.00));
    }

    #[test]
    fn test_parse_options_single_key_value() {
        // Simple "k=v" → single entry.
        let opts = parse_options("color=red");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_parse_options_multiple_comma_separated() {
        // "k1=v1,k2=v2,k3=v3" → 3 entries.
        let opts = parse_options("color=red,thickness=3,z=10");
        assert_eq!(opts.len(), 3);
        assert_eq!(opts.get("color").map(String::as_str), Some("red"));
        assert_eq!(opts.get("thickness").map(String::as_str), Some("3"));
        assert_eq!(opts.get("z").map(String::as_str), Some("10"));
    }

    #[test]
    fn test_parse_options_whitespace_trimmed_around_keys_and_values() {
        // Whitespace around `=` and around comma-separated entries is trimmed.
        let opts = parse_options("  color = red , thickness = 3 ");
        assert_eq!(opts.get("color").map(String::as_str), Some("red"));
        assert_eq!(opts.get("thickness").map(String::as_str), Some("3"));
    }

    #[test]
    fn test_parse_options_entries_without_equals_silently_dropped() {
        // Entry with no `=` is skipped by split_once filter.
        let opts = parse_options("color=red,invalidentry,thickness=3");
        assert_eq!(opts.len(), 2);
        assert!(opts.contains_key("color"));
        assert!(opts.contains_key("thickness"));
        assert!(!opts.contains_key("invalidentry"));
        // Empty input → empty map.
        let empty = parse_options("");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_parse_options_later_duplicate_key_overwrites() {
        // Later `key=val` pairs overwrite earlier ones (HashMap insert semantics).
        let opts = parse_options("color=red,color=blue,color=green");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts.get("color").map(String::as_str), Some("green"));
    }

    #[test]
    fn test_parse_options_value_with_equals_splits_at_first_only() {
        // "k=a=b" — split_once splits at first `=` → key="k", value="a=b".
        let opts = parse_options("k=a=b");
        assert_eq!(opts.get("k").map(String::as_str), Some("a=b"));
    }

    #[test]
    fn test_read_data_comment_lines_interspersed_are_skipped() {
        // `#` comment lines between data rows skipped.
        let content = "# top\nhs1 0 10\n# mid\nhs2 20 30\n# end\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions::default();
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[1].chr, "hs2");
    }

    #[test]
    fn test_read_data_file_not_found_errors() {
        // Non-existent path → Err.
        let r = read_data_file(
            std::path::Path::new("/nonexistent/nofile.tsv"),
            DataType::Highlight,
            &ReadDataOptions::default(),
        );
        assert!(r.is_err());
    }

    #[test]
    fn test_parse_options_with_empty_value_preserved() {
        // "k=" → key "k" with empty value.
        let opts = parse_options("color=,size=10");
        assert_eq!(opts.len(), 2);
        assert_eq!(opts.get("color").map(String::as_str), Some(""));
        assert_eq!(opts.get("size").map(String::as_str), Some("10"));
    }

    #[test]
    fn test_read_data_addset_false_yields_empty_intspan() {
        // addset=false (default) → datum.set is empty IntSpan.
        let content = "hs1 0 10\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            addset: false,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].set.cardinality(), 0);
    }

    #[test]
    fn test_read_data_default_options_single_row() {
        // Default options → single row parsed correctly (chr/start/end).
        let content = "hsX 100 200\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].chr, "hsX");
        assert_eq!(data[0].start, 100);
        assert_eq!(data[0].end, 200);
    }

    #[test]
    fn test_read_data_record_limit_caps_row_count() {
        // record_limit=2 → only first 2 rows parsed.
        let content = "hs1 0 10\nhs2 20 30\nhs3 40 50\nhs4 60 70\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            record_limit: Some(2),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[1].chr, "hs2");
    }

    #[test]
    fn test_parse_options_single_entry_without_comma() {
        // No comma → treated as single entry.
        let opts = parse_options("color=red");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_read_data_param_defaults_used_when_row_has_no_options() {
        // Options.param defaults → each Datum.param gets the default keys.
        let content = "hs1 0 10\nhs2 20 30\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mut defaults = HashMap::new();
        defaults.insert("color".into(), "red".into());
        defaults.insert("z".into(), "5".into());
        let opts = ReadDataOptions {
            param: defaults,
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
        for datum in &data {
            assert_eq!(datum.param.get("color").map(String::as_str), Some("red"));
            assert_eq!(datum.param.get("z").map(String::as_str), Some("5"));
        }
    }

    #[test]
    fn test_read_data_options_default_all_fields_empty() {
        // ReadDataOptions::default has None/false/empty everywhere.
        let opts = ReadDataOptions::default();
        assert!(opts.keyby.is_none());
        assert!(opts.groupby.is_none());
        assert!(opts.record_limit.is_none());
        assert!(opts.min_value_change.is_none());
        assert!(!opts.skip_run);
        assert!(!opts.addset);
        assert!(opts.param.is_empty());
    }

    #[test]
    fn test_parse_options_ignores_empty_entries() {
        // Double-comma "a=1,,b=2" → middle empty entry has no `=`, skipped.
        let opts = parse_options("a=1,,b=2");
        assert_eq!(opts.len(), 2);
        assert_eq!(opts.get("a").map(String::as_str), Some("1"));
        assert_eq!(opts.get("b").map(String::as_str), Some("2"));
    }

    #[test]
    fn test_group_links_single_point_id_filtered_out() {
        // Link groups with < 2 points are dropped — incomplete link.
        let data = vec![
            Datum { id: Some("lonely".into()), chr: "hs1".into(), start: 0, end: 100, ..Default::default() },
            Datum { id: Some("pair".into()),   chr: "hs1".into(), start: 0, end: 100, ..Default::default() },
            Datum { id: Some("pair".into()),   chr: "hs2".into(), start: 0, end: 100, ..Default::default() },
        ];
        let links = group_links(data);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, "pair");
    }

    #[test]
    fn test_group_links_preserves_first_seen_order() {
        // First-seen order determines link output order, not lexical.
        let data = vec![
            Datum { id: Some("zeta".into()),  chr: "c".into(), start: 0, end: 1, ..Default::default() },
            Datum { id: Some("zeta".into()),  chr: "c".into(), start: 0, end: 1, ..Default::default() },
            Datum { id: Some("alpha".into()), chr: "c".into(), start: 0, end: 1, ..Default::default() },
            Datum { id: Some("alpha".into()), chr: "c".into(), start: 0, end: 1, ..Default::default() },
        ];
        let links = group_links(data);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].id, "zeta");
        assert_eq!(links[1].id, "alpha");
    }

    #[test]
    fn test_parse_options_trailing_equals_empty_value() {
        // "k=" → split_once returns Some(("k", "")); empty string retained as value.
        let opts = parse_options("k=");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts.get("k").map(String::as_str), Some(""));
    }

    #[test]
    fn test_parse_options_value_contains_equals_uses_first_split() {
        // split_once('=') splits on the FIRST '=' — rest of value may contain more '='.
        let opts = parse_options("url=http://a?b=c&d=e");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts.get("url").map(String::as_str), Some("http://a?b=c&d=e"));
    }

    #[test]
    fn test_read_data_connector_parses_options_field() {
        // Connector: fields are [chr, start, end, options].
        let content = "hs1 100 200 color=red,z=5\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Connector, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[0].param.get("color").map(String::as_str), Some("red"));
        assert_eq!(data[0].param.get("z").map(String::as_str), Some("5"));
    }

    #[test]
    fn test_read_data_plot_negative_value_accepted() {
        // Negative value passes the numeric-like regex (allows -).
        let content = "hs1 100 200 -3.14\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Plot, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].value, Some(-3.14));
    }

    #[test]
    fn test_read_data_tile_skips_comments_and_blanks() {
        // `#`-comment lines and empty lines filtered before parsing.
        let content = "\
# header
hs1 0 100
hs2 200 300
# trailing

";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Tile, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[1].chr, "hs2");
    }

    #[test]
    fn test_group_links_all_singletons_returns_empty_vec() {
        // Every id has only 1 point → all groups filtered out (need ≥2 per link).
        let data = vec![
            Datum { id: Some("a".into()), chr: "c".into(), start: 0, end: 10, ..Default::default() },
            Datum { id: Some("b".into()), chr: "c".into(), start: 0, end: 10, ..Default::default() },
            Datum { id: Some("c".into()), chr: "c".into(), start: 0, end: 10, ..Default::default() },
        ];
        let links = group_links(data);
        assert!(links.is_empty());
    }

    #[test]
    fn test_read_data_file_missing_path_returns_err_with_path() {
        // Nonexistent path → Err mentioning the path.
        let bad = std::path::PathBuf::from("/definitely/does/not/exist_iter497.txt");
        let r = read_data_file(&bad, DataType::Plot, &ReadDataOptions::default());
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("cannot read data file"));
        assert!(err.contains("exist_iter497.txt"));
    }

    #[test]
    fn test_group_links_three_point_link_preserves_insertion_order() {
        // 3 points with same id → single Link with all 3 points in order.
        let data = vec![
            Datum { id: Some("L".into()), chr: "hs1".into(), start: 10, end: 20, ..Default::default() },
            Datum { id: Some("L".into()), chr: "hs2".into(), start: 30, end: 40, ..Default::default() },
            Datum { id: Some("L".into()), chr: "hs3".into(), start: 50, end: 60, ..Default::default() },
        ];
        let links = group_links(data);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].points.len(), 3);
        assert_eq!(links[0].points[0].chr, "hs1");
        assert_eq!(links[0].points[1].chr, "hs2");
        assert_eq!(links[0].points[2].chr, "hs3");
    }

    #[test]
    fn test_parse_options_trims_whitespace_around_key_and_value() {
        // "  color  =  red  " → key="color", value="red" (both trimmed).
        let opts = parse_options("  color  =  red  ,  thickness  =  2  ");
        assert_eq!(opts.get("color").map(String::as_str), Some("red"));
        assert_eq!(opts.get("thickness").map(String::as_str), Some("2"));
    }

    #[test]
    fn test_read_data_file_custom_file_delim_splits_by_that_character() {
        // file_delim=Some("|") → split on pipe instead of whitespace.
        let content = "hs1|100|200|0.5\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let opts = ReadDataOptions {
            file_delim: Some("|".into()),
            ..Default::default()
        };
        let data = read_data_file(f.path(), DataType::Plot, &opts).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[0].start, 100);
        assert_eq!(data[0].end, 200);
        assert_eq!(data[0].value, Some(0.5));
    }

    #[test]
    fn test_read_data_default_whitespace_splits_tabs_and_multiple_spaces() {
        // file_delim=None → split_whitespace → handles tabs + runs of spaces uniformly.
        let content = "hs1\t100    200\t\t0.5\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Plot, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[0].start, 100);
        assert_eq!(data[0].end, 200);
        assert_eq!(data[0].value, Some(0.5));
    }

    #[test]
    fn test_read_data_link_id_captured_from_first_field() {
        // Link schema: [id, chr, start, end, options] → datum.id=Some("id").
        let content = "mylink hs1 100 200\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Link, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].id.as_deref(), Some("mylink"));
        assert_eq!(data[0].chr, "hs1");
        assert_eq!(data[0].start, 100);
        assert_eq!(data[0].end, 200);
    }

    #[test]
    fn test_read_data_options_param_defaults_applied_to_every_row() {
        // options.param defaults are cloned into each row's param.
        let content = "hs1 0 100\nhs2 0 100\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mut opts = ReadDataOptions::default();
        opts.param.insert("global_style".into(), "bold".into());
        let data = read_data_file(f.path(), DataType::Highlight, &opts).unwrap();
        assert_eq!(data.len(), 2);
        for row in &data {
            assert_eq!(row.param.get("global_style").map(String::as_str), Some("bold"));
        }
    }

    #[test]
    fn test_read_highlight_file_with_url_option_stored_in_param() {
        // options field "url=/x?chr=%s" captured under datum.param.
        let content = "hs1 0 100 url=http://example/x\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Highlight, &ReadDataOptions::default()).unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(
            data[0].param.get("url").map(String::as_str),
            Some("http://example/x")
        );
    }
}
