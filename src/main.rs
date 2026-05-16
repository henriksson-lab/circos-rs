use std::collections::HashMap;
use std::path::Path;

use clap::Parser;

use circos_rs::config::types::ConfigValue;
use circos_rs::render::png;

#[derive(Parser, Debug)]
#[command(
    name = "circos",
    about = "Circular data visualization",
    rename_all = "snake_case",
    rename_all_env = "snake_case"
)]
struct Cli {
    /// Configuration file (required)
    #[arg(short = 'c', long = "configfile", alias = "conf", value_name = "FILE")]
    configfile: String,

    // --- Ideograms ---
    /// Chromosomes to display (semicolon-separated, prefix `-` to exclude, `hs1:10-20` for ranges)
    #[arg(long, value_name = "STRING")]
    chromosomes: Option<String>,

    /// Chromosome display order
    #[arg(long, value_name = "STRING")]
    chromosomes_order: Option<String>,

    /// Chromosome scaling factors (e.g. `hs1=2,hs2=0.5`)
    #[arg(long, value_name = "STRING")]
    chromosomes_scale: Option<String>,

    /// Chromosome radius overrides
    #[arg(long, value_name = "STRING")]
    chromosomes_radius: Option<String>,

    // --- Output format ---
    /// Toggle PNG output
    #[arg(long, default_value_t = false)]
    png: bool,

    /// Request 24-bit PNG (required when using transparency)
    #[arg(long = "24bit", default_value_t = false)]
    bit24: bool,

    /// Toggle SVG output
    #[arg(long, default_value_t = true)]
    svg: bool,

    // --- Output paths ---
    /// Output directory
    #[arg(long, value_name = "DIR", default_value = ".")]
    outputdir: String,

    /// Output filename (without extension)
    #[arg(long, value_name = "FILE", default_value = "circos")]
    outputfile: String,

    // --- Input format ---
    /// Input data file delimiter (default: whitespace; use tab for multi-word labels)
    #[arg(long, value_name = "DELIM")]
    file_delim: Option<String>,

    // --- Custom template fields ---
    /// Custom field, referenced in config as __$CONF{usertext1}__
    #[arg(long, value_name = "STRING")]
    usertext1: Option<String>,
    #[arg(long, value_name = "STRING")]
    usertext2: Option<String>,
    #[arg(long, value_name = "STRING")]
    usertext3: Option<String>,
    #[arg(long, value_name = "STRING")]
    usertext4: Option<String>,

    // --- Ticks ---
    /// Show ticks (use --no-show_ticks to suppress)
    #[arg(long, overrides_with = "_no_show_ticks")]
    show_ticks: bool,
    #[arg(long = "no-show_ticks", hide = true)]
    _no_show_ticks: bool,

    /// Show tick labels (use --no-show_tick_labels to suppress)
    #[arg(long, overrides_with = "_no_show_tick_labels")]
    show_tick_labels: bool,
    #[arg(long = "no-show_tick_labels", hide = true)]
    _no_show_tick_labels: bool,

    // --- Image maps ---
    /// Enable image map (legacy alias)
    #[arg(long, default_value_t = false)]
    imagemap: bool,

    /// Enable image map generation
    #[arg(long, default_value_t = false)]
    image_map_use: bool,

    /// Image map name attribute
    #[arg(long, value_name = "MAPNAME")]
    image_map_name: Option<String>,

    /// Image map output file
    #[arg(long, value_name = "FILE")]
    image_map_file: Option<String>,

    /// Behavior when an image-map parameter is missing ({exit | removeparam | removeurl})
    #[arg(long, value_name = "POLICY")]
    image_map_missing_parameter: Option<String>,

    /// Tag rendered elements with name attribute
    #[arg(long, default_value_t = false)]
    tagname: bool,

    // --- Debugging ---
    /// Silent mode
    #[arg(long, default_value_t = false)]
    silent: bool,

    /// Verbose reporting (repeat for higher verbosity)
    #[arg(short = 'V', long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Debug output (repeat for higher verbosity)
    #[arg(short = 'd', long, action = clap::ArgAction::Count)]
    debug: u8,

    /// Print long-form manual
    #[arg(long, default_value_t = false)]
    man: bool,
}

/// Build the Perl `%OPT` map from parsed CLI args. Each entry overrides the
/// corresponding `%CONF` key via `populateconfiguration` inside `run_with_opt`.
fn opt_from_cli(cli: &Cli) -> HashMap<String, ConfigValue> {
    let mut opt: HashMap<String, ConfigValue> = HashMap::new();

    macro_rules! set_opt_str {
        ($key:expr, $val:expr) => {
            if let Some(v) = &$val {
                opt.insert($key.to_string(), ConfigValue::Str(v.clone()));
            }
        };
    }
    macro_rules! set_opt_flag {
        ($key:expr, $val:expr) => {
            if $val {
                opt.insert($key.to_string(), ConfigValue::Str("1".to_string()));
            }
        };
    }

    set_opt_str!("chromosomes", cli.chromosomes);
    set_opt_str!("chromosomes_order", cli.chromosomes_order);
    set_opt_str!("chromosomes_scale", cli.chromosomes_scale);
    set_opt_str!("chromosomes_radius", cli.chromosomes_radius);
    set_opt_str!("file_delim", cli.file_delim);
    set_opt_str!("usertext1", cli.usertext1);
    set_opt_str!("usertext2", cli.usertext2);
    set_opt_str!("usertext3", cli.usertext3);
    set_opt_str!("usertext4", cli.usertext4);
    set_opt_str!("image_map_name", cli.image_map_name);
    set_opt_str!("image_map_file", cli.image_map_file);
    set_opt_str!("image_map_missing_parameter", cli.image_map_missing_parameter);

    set_opt_flag!("png", cli.png);
    set_opt_flag!("24bit", cli.bit24);
    set_opt_flag!("imagemap", cli.imagemap);
    set_opt_flag!("image_map_use", cli.image_map_use);
    set_opt_flag!("tagname", cli.tagname);
    set_opt_flag!("silent", cli.silent);
    set_opt_flag!("show_ticks", cli.show_ticks);
    set_opt_flag!("show_tick_labels", cli.show_tick_labels);

    opt.insert(
        "outputdir".to_string(),
        ConfigValue::Str(cli.outputdir.clone()),
    );
    opt.insert(
        "outputfile".to_string(),
        ConfigValue::Str(cli.outputfile.clone()),
    );
    if cli.verbose > 0 {
        opt.insert(
            "verbose".to_string(),
            ConfigValue::Str(cli.verbose.to_string()),
        );
    }
    if cli.debug > 0 {
        opt.insert(
            "debug".to_string(),
            ConfigValue::Str(cli.debug.to_string()),
        );
    }

    opt
}

/// CLI entry point: parses arguments, runs the Circos pipeline, and writes
/// the resulting SVG (and optionally PNG) to disk.
fn main() {
    let cli = Cli::parse();

    if !cli.silent {
        eprintln!("circos-rs v0.1.0");
    }

    let conf_path = Path::new(&cli.configfile);
    let opt = opt_from_cli(&cli);

    let out = match circos_rs::run_with_opt(conf_path, opt) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    if cli.svg && out.svg_make {
        let output_path = Path::new(&out.outputfile_svg);
        match std::fs::write(output_path, &out.svg) {
            Ok(()) => {
                if !cli.silent {
                    eprintln!(
                        "wrote SVG to {} ({} bytes)",
                        output_path.display(),
                        out.svg.len()
                    );
                }
            }
            Err(e) => {
                eprintln!("error writing SVG: {}", e);
                std::process::exit(1);
            }
        }
    }

    if cli.png || out.png_make {
        let output_path = Path::new(&out.outputfile_png);
        match png::svg_to_png(&out.svg, output_path) {
            Ok(()) => {
                if !cli.silent {
                    eprintln!("wrote PNG to {}", output_path.display());
                }
            }
            Err(e) => {
                eprintln!("error writing PNG: {}", e);
                std::process::exit(1);
            }
        }
    }
}
