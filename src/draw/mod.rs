pub mod highlights;
pub mod ideograms;
pub mod links;
pub mod plots;
pub mod ticks;

use std::collections::HashMap;
use std::path::Path;

use crate::config::types::ConfigValue;
use crate::data::reader;
use crate::data::types::DataType;
use crate::karyotype::types::Karyotype;
use crate::layout::Layout;
use crate::render::color::ColorMap;
use crate::render::svg::SvgDocument;
use crate::rules;

/// Draw the complete Circos image and return an SVG string.
pub fn draw_circos(
    layout: &Layout,
    conf: &HashMap<String, ConfigValue>,
    karyotype: &Karyotype,
    colors: &ColorMap,
    base_dir: &Path,
) -> String {
    let width = layout.image_radius * 2.0;
    let height = layout.image_radius * 2.0;
    let mut doc = SvgDocument::new(width, height);

    // Draw background
    if let Some(bg_name) = conf
        .get("image")
        .and_then(|v| v.get("background"))
        .and_then(|v| v.as_str())
    {
        if let Some(bg_color) = colors.resolve(bg_name) {
            doc.add(format!(
                r#"<rect x="0" y="0" width="{:.0}" height="{:.0}" style="fill: {};" />"#,
                width, height, bg_color.to_svg_rgb()
            ));
        }
    }

    // Draw ideograms (fills, bands, outlines, labels)
    ideograms::draw_ideograms(&mut doc, layout, conf, karyotype, colors);

    // Draw ticks
    let show_ticks = conf
        .get("show_ticks")
        .and_then(|v| v.as_str())
        .map(|s| s == "1")
        .unwrap_or(false);
    if show_ticks {
        ticks::draw_ticks(&mut doc, layout, conf, karyotype, colors);
    }

    // Draw highlights
    if let Some(highlights_conf) = conf.get("highlights").and_then(|v| v.as_map()) {
        draw_data_sets(&mut doc, layout, conf, colors, base_dir, highlights_conf, "highlight");
    }

    // Draw links
    if let Some(links_conf) = conf.get("links").and_then(|v| v.as_map()) {
        draw_link_sets(&mut doc, layout, conf, colors, base_dir, links_conf);
    }

    // Draw plots
    if let Some(plots_conf) = conf.get("plots").and_then(|v| v.as_map()) {
        draw_plot_sets(&mut doc, layout, colors, base_dir, plots_conf);
    }

    doc.render()
}

/// Draw highlight/plot data sets from config.
fn draw_data_sets(
    doc: &mut SvgDocument,
    layout: &Layout,
    _conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    base_dir: &Path,
    section_conf: &HashMap<String, ConfigValue>,
    set_type: &str,
) {
    let data_type = match set_type {
        "highlight" => DataType::Highlight,
        "plot" => DataType::Plot,
        _ => return,
    };

    // Find all sub-blocks (e.g., <highlight name> blocks)
    for (_key, value) in section_conf {
        if let Some(block) = value.as_map() {
            if let Some(file_path) = block.get("file").and_then(|v| v.as_str()) {
                let full_path = resolve_data_path(file_path, base_dir);
                if let Ok(data) = reader::read_data_file(&full_path, data_type) {
                    if data_type == DataType::Highlight {
                        highlights::draw_highlights(doc, layout, &data, block, colors);
                    }
                }
            }
        }
    }
}

/// Draw plot data sets from config.
fn draw_plot_sets(
    doc: &mut SvgDocument,
    layout: &Layout,
    colors: &ColorMap,
    base_dir: &Path,
    plots_conf: &HashMap<String, ConfigValue>,
) {
    // Find all <plot> blocks
    let plot_values = match plots_conf.get("plot") {
        Some(ConfigValue::List(list)) => list.clone(),
        Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
        _ => return,
    };

    for (i, plot_val) in plot_values.iter().enumerate() {
        if let Some(block) = plot_val.as_map() {
            let plot_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let data_type = match plot_type {
                "text" => DataType::Text,
                "tile" | "connector" | "highlight" => DataType::Highlight,
                _ => DataType::Plot,
            };

            if let Some(file_path) = block.get("file").and_then(|v| v.as_str()) {
                let full_path = resolve_data_path(file_path, base_dir);
                if let Ok(data) = reader::read_data_file(&full_path, data_type) {
                    doc.open_group(&format!("plot-{}", i));
                    plots::draw_plot(doc, layout, &data, block, colors);
                    doc.close_group();
                }
            }
        }
    }
}

/// Draw link data sets from config.
fn draw_link_sets(
    doc: &mut SvgDocument,
    layout: &Layout,
    _conf: &HashMap<String, ConfigValue>,
    colors: &ColorMap,
    base_dir: &Path,
    links_conf: &HashMap<String, ConfigValue>,
) {
    // Get default link parameters
    let default_params = extract_link_defaults(links_conf);

    // Find all <link name> blocks
    for (key, value) in links_conf {
        if let Some(block) = value.as_map() {
            if let Some(file_path) = block.get("file").and_then(|v| v.as_str()) {
                let full_path = resolve_data_path(file_path, base_dir);
                if let Ok(data) = reader::read_data_file(&full_path, DataType::Link) {
                    let link_groups = reader::group_links(data);

                    // Parse rules
                    let rule_list = rules::parse_rules(
                        block.get("rules").and_then(|v| v.as_map()),
                    );

                    doc.open_group(&format!("links-{}", key));
                    links::draw_links(
                        doc,
                        layout,
                        &link_groups,
                        &default_params,
                        block,
                        &rule_list,
                        colors,
                    );
                    doc.close_group();
                }
            }
        }
    }
}

fn extract_link_defaults(links_conf: &HashMap<String, ConfigValue>) -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    for (k, v) in links_conf {
        if let Some(s) = v.as_str() {
            defaults.insert(k.clone(), s.to_string());
        }
    }
    defaults
}

fn resolve_data_path(file_path: &str, base_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(file_path);
    if p.exists() {
        return p.to_path_buf();
    }
    let candidate = base_dir.join(file_path);
    if candidate.exists() {
        return candidate;
    }
    // Try going up directories
    let mut parent = base_dir;
    for _ in 0..5 {
        if let Some(p) = parent.parent() {
            let candidate = p.join(file_path);
            if candidate.exists() {
                return candidate;
            }
            parent = p;
        } else {
            break;
        }
    }
    base_dir.join(file_path)
}
