pub mod api;
pub mod config;
pub mod coord;
pub mod data;
pub mod draw;
pub mod intspan;
pub mod karyotype;
pub mod layout;
pub mod render;
pub mod rules;
pub mod utils;

use std::collections::HashMap;
use std::path::Path;

use config::parser::ConfigParser;
use config::types::ConfigValue;
use layout::Layout;
use render::color::ColorMap;

/// High-level API: generate a Circos visualization from a config file path.
///
/// Returns the SVG output as a string.
///
/// # Example
///
/// ```no_run
/// let svg = circos_rs::render_from_config(
///     std::path::Path::new("circos.conf"),
/// ).unwrap();
/// std::fs::write("output.svg", svg).unwrap();
/// ```
pub fn render_from_config(conf_path: &Path) -> Result<String, String> {
    let base_dir = conf_path.parent().unwrap_or(Path::new("."));

    // Find circos base (where etc/colors.conf lives)
    let circos_base = find_circos_base(conf_path);

    // Build search paths
    let mut search_paths = vec![base_dir.to_path_buf(), base_dir.join("etc")];
    let mut parent = base_dir;
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            search_paths.push(p.to_path_buf());
            if p.join("etc").join("colors.conf").exists() {
                break;
            }
            parent = p;
        } else {
            break;
        }
    }

    let parser = ConfigParser {
        config_paths: search_paths,
        auto_true: true,
        lower_case_names: true,
    };

    let config = parser.parse_file(conf_path)?;
    render_from_parsed_config(&config, &circos_base)
}

/// High-level API: generate SVG from an already-parsed config and base directory.
pub fn render_from_parsed_config(
    config: &HashMap<String, ConfigValue>,
    base_dir: &Path,
) -> Result<String, String> {
    // Read karyotype
    let karyotype_rel = config
        .get("karyotype")
        .and_then(|v| v.as_str())
        .ok_or("no karyotype file specified")?;

    let karyotype_path = resolve_path(karyotype_rel, base_dir);
    let karyotype_data = karyotype::read_karyotype(&karyotype_path, None)?;

    // Build layout
    let layout = Layout::build(config, &karyotype_data)?;

    // Build color map
    let mut colors = ColorMap::new();
    if let Some(color_conf) = config.get("colors").and_then(|v| v.as_map()) {
        colors.load_from_config(color_conf);
    }

    // Generate SVG
    Ok(draw::draw_circos(
        &layout,
        config,
        &karyotype_data,
        &colors,
        base_dir,
    ))
}

fn find_circos_base(conf_path: &Path) -> std::path::PathBuf {
    let mut dir = conf_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    for _ in 0..6 {
        if dir.join("etc").join("colors.conf").exists() {
            return dir;
        }
        dir = match dir.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
    }
    conf_path.parent().unwrap_or(Path::new(".")).to_path_buf()
}

fn resolve_path(file: &str, base_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(file);
    if p.exists() {
        return p.to_path_buf();
    }
    let candidate = base_dir.join(file);
    if candidate.exists() {
        return candidate;
    }
    let mut parent = base_dir;
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            let candidate = p.join(file);
            if candidate.exists() {
                return candidate;
            }
            parent = p;
        } else {
            break;
        }
    }
    base_dir.join(file)
}
