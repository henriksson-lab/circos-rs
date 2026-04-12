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

/// Read a data file and return a vector of Datum entries.
pub fn read_data_file(
    path: &Path,
    data_type: DataType,
) -> Result<Vec<Datum>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("cannot read data file {}: {}", path.display(), e))?;

    let mut data = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if utils::is_blank(line) || utils::is_comment(line) {
            continue;
        }

        let fields: Vec<&str> = line.split_whitespace().collect();

        let datum = match data_type {
            DataType::Link => {
                // id chr start end [options]
                if fields.len() < 4 {
                    return Err(format!(
                        "{}:{}: link line needs at least 4 fields",
                        path.display(),
                        line_num + 1
                    ));
                }
                let id = fields[0].to_string();
                let chr = fields[1].to_string();
                let start: i64 = fields[2].parse().map_err(|_| {
                    format!("{}:{}: invalid start", path.display(), line_num + 1)
                })?;
                let end: i64 = fields[3].parse().map_err(|_| {
                    format!("{}:{}: invalid end", path.display(), line_num + 1)
                })?;
                let (start, end) = if start > end { (end, start) } else { (start, end) };
                let options = if fields.len() > 4 {
                    parse_options(fields[4])
                } else {
                    HashMap::new()
                };
                Datum {
                    chr,
                    start,
                    end,
                    set: IntSpan::from_range(start, end),
                    id: Some(id),
                    value: None,
                    label: None,
                    param: options,
                }
            }
            DataType::Highlight | DataType::Tile | DataType::Connector => {
                // chr start end [options]
                if fields.len() < 3 {
                    return Err(format!(
                        "{}:{}: highlight line needs at least 3 fields",
                        path.display(),
                        line_num + 1
                    ));
                }
                let chr = fields[0].to_string();
                let start: i64 = fields[1].parse().map_err(|_| {
                    format!("{}:{}: invalid start", path.display(), line_num + 1)
                })?;
                let end: i64 = fields[2].parse().map_err(|_| {
                    format!("{}:{}: invalid end", path.display(), line_num + 1)
                })?;
                let (start, end) = if start > end { (end, start) } else { (start, end) };
                let options = if fields.len() > 3 {
                    parse_options(fields[3])
                } else {
                    HashMap::new()
                };
                Datum {
                    chr,
                    start,
                    end,
                    set: IntSpan::from_range(start, end),
                    id: None,
                    value: None,
                    label: None,
                    param: options,
                }
            }
            DataType::Plot => {
                // chr start end value [options]
                if fields.len() < 4 {
                    return Err(format!(
                        "{}:{}: plot line needs at least 4 fields",
                        path.display(),
                        line_num + 1
                    ));
                }
                let chr = fields[0].to_string();
                let start: i64 = fields[1].parse().map_err(|_| {
                    format!("{}:{}: invalid start", path.display(), line_num + 1)
                })?;
                let end: i64 = fields[2].parse().map_err(|_| {
                    format!("{}:{}: invalid end", path.display(), line_num + 1)
                })?;
                let (start, end) = if start > end { (end, start) } else { (start, end) };
                let value: f64 = fields[3].parse().map_err(|_| {
                    format!("{}:{}: invalid value", path.display(), line_num + 1)
                })?;
                let options = if fields.len() > 4 {
                    parse_options(fields[4])
                } else {
                    HashMap::new()
                };
                Datum {
                    chr,
                    start,
                    end,
                    set: IntSpan::from_range(start, end),
                    id: None,
                    value: Some(value),
                    label: None,
                    param: options,
                }
            }
            DataType::Text => {
                // chr start end label [options]
                if fields.len() < 4 {
                    return Err(format!(
                        "{}:{}: text line needs at least 4 fields",
                        path.display(),
                        line_num + 1
                    ));
                }
                let chr = fields[0].to_string();
                let start: i64 = fields[1].parse().map_err(|_| {
                    format!("{}:{}: invalid start", path.display(), line_num + 1)
                })?;
                let end: i64 = fields[2].parse().map_err(|_| {
                    format!("{}:{}: invalid end", path.display(), line_num + 1)
                })?;
                let (start, end) = if start > end { (end, start) } else { (start, end) };
                let label = fields[3].to_string();
                let options = if fields.len() > 4 {
                    parse_options(fields[4])
                } else {
                    HashMap::new()
                };
                Datum {
                    chr,
                    start,
                    end,
                    set: IntSpan::from_range(start, end),
                    id: None,
                    value: None,
                    label: Some(label),
                    param: options,
                }
            }
        };

        data.push(datum);
    }

    Ok(data)
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
        let data = read_data_file(f.path(), DataType::Link).unwrap();
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
        let data = read_data_file(f.path(), DataType::Highlight).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].chr, "hs2");
        assert_eq!(data[0].param.get("fill_color").unwrap(), "blue");
    }

    #[test]
    fn test_read_plot_file() {
        let content = "hs1 100 200 0.5\nhs1 300 400 0.8 color=blue\n";
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let data = read_data_file(f.path(), DataType::Plot).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].value, Some(0.5));
        assert_eq!(data[1].param.get("color").unwrap(), "blue");
    }
}
